//! --play <recording>: the rewritten game in a window, starting from the game's own state in a PPG_DUMP recording
//! (its first FixedUpdate snapshot with the b2 world) and running Unity's player loop from there.
//! --play-map <map>: the same, starting from LoadScene("Main") with that map, all from the game's data.

use crate::b2::world::Shape;
use crate::game::{dump, Game, Prefs};
use crate::game_assets::HumanRig;
use bevy::{prelude::*, sprite::Anchor};
use crate::assets::db::AssetDb;
use std::path::{Path, PathBuf};

#[derive(Resource)]
pub(crate) struct Play {
    pub game: Game,
    /// Set when a frame panicked (an unrewritten feature): the world stays frozen as it was.
    pub stopped: Option<String>,
    /// How many limbs, physicals and renderers already have a sprite.
    done: (usize, usize, usize),
}

impl Play {
    pub fn new(game: Game) -> Self {
        Self { game, stopped: None, done: (0, 0, 0) }
    }
}

/// A limb's sprite, drawn at its PhysicalBehaviour's Transform.
#[derive(Component)]
pub(crate) struct LimbSprite(usize);

/// Where the game starts.
pub enum Start {
    Recording(PathBuf),
    /// The installed game's folder, the Map asset's name and the catalog item selected (if any).
    /// The game, the map, the --item and the --target.
    Map(PathBuf, String, Option<String>, Option<String>),
    /// The main menu, with the installed game's folder.
    Menu(PathBuf),
}

/// `prefs` replace config.json's (the windowed game's settings); None for the headless runs.
fn load(start: &Start, rig: &HumanRig, prefs: Option<Prefs>) -> Result<Option<Game>, String> {
    let recording = match start {
        Start::Recording(recording) => recording,
        Start::Map(game, map, item, _) => return new_game(game, map, item.as_deref(), prefs, 0).map(Some),
        Start::Menu(_) => return Ok(None),
    };
    let start = dump::snapshots(recording)?
        .find(|s| s.pt == "FS" && s.of("b2World").next().is_some())
        .ok_or("--play needs a recording with the b2 world (dump12 on)")?;
    let mut g = Game::from_snapshot(&start, rig)?;
    g.load_layers(&dump::header(recording)?);
    Ok(Some(g))
}

/// A panic in `f` (an unrewritten feature) comes back as its message.
pub(crate) fn guarded<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or_else(|panic| Err(panic_message(panic)))
}

fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
    panic.downcast_ref::<String>().cloned().or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default()
}

/// SceneManager.LoadScene("Main") with the main menu's map choice: level1's managers and natives, then the
/// MapLoaderBehaviour's Awake. Its behaviours' Starts run in the first frame.
/// `seed` is UnityEngine.Random's startup seed.
pub(crate) fn new_game(game: &Path, map: &str, item: Option<&str>, prefs: Option<Prefs>, seed: i32) -> Result<Game, String> {
    let mut db = AssetDb::open(game)?;
    let mut g = Game::load_main(&mut db, game)?;
    g.random = crate::unity::Random::new(seed);
    // The heap addresses behind GetHashCode (which reseeds Random in PhysicalBehaviour.Start) differ per session too.
    if seed != 0 {
        g.heap_base = Some(0x0100_0000 + (((seed as u32) as u64 & 0xf_ffff) << 11));
    }
    if let Some(prefs) = prefs {
        g.prefs = prefs;
    }
    g.load_scene(&mut db)?;
    g.map_loader_awake(&mut db, map)?;
    g.assets = Some(Box::new(db));
    g.catalog_selected = item.map(str::to_string);
    Ok(g)
}

/// --play-test: 30 s of 60 fps frames without a window; prints where each limb ends up. A --play-map-test with
/// --item spawns the item at the map's center on frame 30 and a flipped one on frame 60. With a --target, the target
/// spawns 3 m right of the center, 1.2 m above the ground there, on frame 30 and the item 0.6 m above the ground
/// below the center on frame 60, activated ("activateDirect" with the mouse on its first object's center of mass)
/// on frame 64.
pub fn test(start: &Start, rig: &HumanRig) -> Result<(), String> {
    let mut g = load(start, rig, None)?.ok_or("the menu has no headless run")?;
    let target = match start {
        Start::Map(_, _, _, target) => target.clone(),
        _ => None,
    };
    let item = g.catalog_selected.clone();
    // PP_HOLD=N (with a --target): the item is dragged from frame 62 and "activateDirect" is held for N frames from 64.
    let hold: i32 = std::env::var("PP_HOLD").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut ground = 0.0;
    let mut item_physical = 0;
    for n in 0..1800 {
        if let Some(target) = &target {
            if n == 29 {
                g.catalog_selected = Some(target.clone());
                let c = g.BoundingBox.center;
                ground = g.raycast_first(crate::b2::math::Vec2::new(c[0], c[1]), [0.0, -1.0], 1.0e4).ok_or("--target: no ground below the map's center")?.point.y;
            } else if n == 59 {
                g.catalog_selected = item.clone();
                item_physical = g.physicals.len();
            }
            for (key, at) in [("spawnRight", &[30, 60][..]), ("activateDirect", &[64])] {
                let down = at.contains(&(n + 1));
                let (held, up) = match key {
                    "activateDirect" if hold > 0 => ((63..63 + hold).contains(&n), n == 63 + hold),
                    _ => (down, at.contains(&n)),
                };
                g.input.controls.insert(key.into(), crate::game::tool::Control { down, held, up });
            }
            if hold > 0 {
                g.input.controls.insert("drag".into(), crate::game::tool::Control { down: n == 61, held: n >= 61, up: false });
            }
            g.input.mouse = match n {
                ..59 => [g.BoundingBox.center[0] + 3.0, ground + 1.2, 0.0],
                ..61 => [g.BoundingBox.center[0], ground + 0.6, 0.0],
                _ => {
                    let p = g.physicals.get(item_physical).ok_or("--target: the item has no physical object")?;
                    // Once the item is gone the mouse stays where it was.
                    match p.destroyed {
                        true => g.input.mouse,
                        false => {
                            let c = g.world.body(p.rigidbody.body).sweep.c;
                            [c.x, c.y, 0.0]
                        }
                    }
                }
            };
        } else if g.catalog_selected.is_some() {
            // With a catalog item selected: "spawnRight" released at the map's center on frame 30, "spawnLeft" 3 m
            // to the right on frame 60.
            for (key, at, dx) in [("spawnRight", 30, 0.0), ("spawnLeft", 60, 3.0)] {
                g.input.controls.insert(key.into(), crate::game::tool::Control { down: n == at - 1, held: n == at - 1, up: n == at });
                if n == at {
                    g.input.mouse = [g.BoundingBox.center[0] + dx, g.BoundingBox.center[1], 0.0];
                }
            }
        }
        // PP_ATTACH=<attachment item>[,<point index>]: on frame 900 that item is dropped on the gun's attachment point.
        if let Ok(spec) = std::env::var("PP_ATTACH") {
            let mut it = spec.split(',');
            let name = it.next().unwrap();
            let pt: usize = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            if n == 899 {
                g.catalog_selected = Some(name.to_string());
                g.input.controls.insert("spawnRight".into(), crate::game::tool::Control { down: true, held: true, up: false });
            }
            if n == 900 {
                g.input.controls.insert("spawnRight".into(), crate::game::tool::Control { down: false, held: false, up: true });
                let at = g.attachment_point_position(pt);
                g.input.mouse = at;
                println!("attachment point {pt} at {at:?}");
            }
            // The gun fires with the attachment on it (frames 1000, 1040, 1080), then the attachment is activated: it disconnects.
            let (gun, att) = if n >= 901 { (g.firearms[0].physical.unwrap(), g.firearm_attachments[0].physical.unwrap()) } else { (0, 0) };
            let report = |g: &crate::game::Game, what: &str| {
                let (a, f) = (&g.firearm_attachments[0], &g.firearms[0]);
                let em = f.emitter.as_ref();
                println!(
                    "{what}: connected={} listeners={:?} parent={} driven={} charge={} gun temp={} applyCharge={} inaccuracy={:?} explosive={:?} laser={:?} on fire={} burn sum={} limb health sum={}",
                    a.Connected, f.attachments, g.physicals[att].parent.is_some(), g.physicals[att].driven_by.is_some(), g.physicals[gun].charge, g.physicals[gun].Temperature, f.ApplyCharge, f.InaccuracyMultipliers,
                    em.map(|e| (e.ExplosiveRound, e.ExplodeAtHitPoint)), a.laser.map(|l| g.lasers[l].Activated),
                    g.physicals.iter().filter(|p| p.OnFire).count(), g.physicals.iter().map(|p| p.BurnProgress).sum::<f32>(),
                    g.limbs.iter().map(|l| l.Health).sum::<f32>()
                );
            };
            match n {
                ..901 => {}
                960 | 1120 | 1260 | 1400 => report(&g, &format!("frame {n}")),
                1000 | 1040 | 1080 => g.activate_for_test(gun),
                1200 if std::env::var("PP_ATTACH_DESTROY").is_err() => g.activate_for_test(att),
                // PP_ATTACH_DESTROY=gun|att: that object is destroyed while connected, instead of the attachment being activated.
                1100 if std::env::var("PP_ATTACH_DESTROY").is_ok() => {
                    let victim = if std::env::var("PP_ATTACH_DESTROY").is_ok_and(|v| v == "gun") { gun } else { att };
                    g.destroy_object(g.physicals[victim].id);
                }
                _ => {}
            }
        }
        if let Err(m) = step(&mut g, 1.0 / 60.0) {
            return Err(format!("frame {n}: {m}"));
        }
    }
    for (i, l) in g.limbs.iter().enumerate() {
        let p = &g.physicals[l.PhysicalBehaviour];
        let c = &g.circulations[i];
        println!("{:14} {:?} active={} health={} bleeding={} wounds={}", p.name, &p.transform.position[..2], p.active, l.Health, c.BleedingRate, c.GunshotWoundCount);
    }
    Ok(())
}

pub fn run(start: &Start, rig: HumanRig, cache: PathBuf) -> Result<(), String> {
    let dir = match start {
        Start::Map(dir, ..) | Start::Menu(dir) => Some(dir.clone()),
        Start::Recording(_) => None,
    };
    let (settings, controls, files) = crate::ui::load_settings(dir.as_deref());
    let game = load(start, &rig, Some(settings.prefs()))?;
    let (map, item) = match start {
        Start::Map(_, map, item, _) => (Some(map.clone()), item.clone()),
        _ => (None, None),
    };
    let mode = if game.is_some() { crate::ui::Mode::Game } else { crate::ui::Mode::Menu };
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window { title: "People Playground rewrite".into(), resolution: (1280, 800).into(), ..default() }),
                ..default()
            })
            .set(AssetPlugin { file_path: cache.to_string_lossy().into_owned(), ..default() })
            .set(ImagePlugin::default_nearest()),
    )
    .insert_resource(ClearColor(Color::srgb(0.055, 0.065, 0.08)))
    .insert_resource(rig)
    .insert_resource(crate::ui::new_ui(dir, mode, map, item))
    .insert_resource(settings)
    .insert_resource(controls)
    .insert_resource(files);
    if let Some(game) = game {
        app.insert_resource(Play::new(game));
    }
    crate::ui::add(&mut app);
    app.run();
    Ok(())
}

/// A frame panic (an unrewritten feature) comes back as its message.
pub(crate) fn step(g: &mut Game, real: f32) -> Result<(), String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let t = &mut g.time;
        // Time.deltaTime: the unscaled frame time clamped to maximumDeltaTime, then scaled.
        let dt = real.min(t.maximumDeltaTime) * t.timeScale;
        // TimeManager::Update's smoothDeltaTime: the counter c = c * 0.8 + 0.2 weights the frame by 0.2 / c.
        t.smoothCount = t.smoothCount * 0.8 + 0.2;
        let w = 0.2 / t.smoothCount;
        t.smoothDeltaTime = (1.0 - w) * t.smoothDeltaTime + w * dt;
        let frame_time = t.timeAsDouble + dt as f64;
        // TimeManager::Update (the first system of the loop): the frame count, the dynamic time and the clock.
        t.frameCount += 1;
        t.dynamicTimeAsDouble = frame_time;
        t.realtimeSinceStartup += real as f64;
        while g.time.fixedTimeAsDouble + g.time.fixedDeltaTime as f64 <= frame_time {
            let t = &mut g.time;
            t.fixedTimeAsDouble += t.fixedDeltaTime as f64;
            t.fixedTime = t.fixedTimeAsDouble as f32;
            (t.timeAsDouble, t.time, t.deltaTime) = (t.fixedTimeAsDouble, t.fixedTime, t.fixedDeltaTime);
            g.fixed_update_scripts();
            g.simulate_physics();
        }
        let t = &mut g.time;
        (t.timeAsDouble, t.time, t.deltaTime) = (frame_time, frame_time as f32, dt);
        g.interpolate_bodies();
        g.update_scripts();
        // OnWillRenderObject for the limbs the camera draws: renderer.bounds against the orthographic view (the
        // sprites sit between its near and far planes, so the frustum test is the rectangle overlap).
        for l in 0..g.limbs.len() {
            if !g.limb_active(l) {
                continue;
            }
            let drawn = g.input.view.is_none_or(|(min, max)| {
                let (c, e) = g.sprite_world_bounds(g.limbs[l].PhysicalBehaviour);
                (0..2).all(|i| c[i] + e[i] >= min[i] && c[i] - e[i] <= max[i])
            });
            if drawn {
                g.skin_will_render(l);
            }
        }
        g.foliage_will_render(g.input.view);
        g.end_of_frame_calls();
    }))
    .map_err(panic_message)
}

/// A physical object's own sprite (the limbs use the rig's).
#[derive(Component)]
/// The physical, and the scale from the sprite's units to the world (sliced and tiled sprites are laid out in
/// pixels so their borders keep their texel size).
pub(crate) struct ObjectSprite(usize, f32);

/// A SpriteRenderer on an object without a PhysicalBehaviour (`Game.renderers`), and the same scale.
#[derive(Component)]
pub(crate) struct RendererSprite(usize, f32);

type Textures = std::collections::HashMap<(usize, i64), Handle<Image>>;

/// TagManager.asset's m_SortingLayers uniqueIDs, in draw order (Bottom first). A SpriteRenderer's sortingLayerID is one of them.
const SORTING_LAYERS: [u32; 8] = [334215217, 1370879715, 0, 182652991, 1377110059, 2185794113, 2257819283, 2607693843];

/// A sprite's draw key: its sorting layer's index, its sortingOrder, then its group (objects before renderers) and index
/// (the spawn order). Unity's tie rule for sprites with the same layer and order is not in the export, so ties keep the
/// spawn order.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SortKey {
    layer: usize,
    order: i16,
    group: u8,
    index: usize,
}

/// The index of a sortingLayerID in TagManager's order.
fn sorting_layer_index(id: i32) -> usize {
    SORTING_LAYERS.iter().position(|&u| u == id as u32).unwrap_or_else(|| unimplemented!("SpriteRenderer sortingLayerID {id} is not in TagManager's sortingLayers"))
}

/// Each key's z: its rank in draw order over all the keys, spread over -0.5 (drawn first) to 0.5 (drawn last).
fn sprite_depths(keys: &[SortKey]) -> Vec<f32> {
    let mut order: Vec<usize> = (0..keys.len()).collect();
    order.sort_by_key(|&i| keys[i]);
    let n = keys.len() as f32;
    let mut z = vec![0.0; keys.len()];
    for (rank, &i) in order.iter().enumerate() {
        z[i] = -0.5 + (rank as f32 + 0.5) / n;
    }
    z
}

/// A sprite made this frame, with the key that ranks it.
enum NewSprite {
    Object(usize, f32, Sprite, Anchor),
    Renderer(usize, f32, Sprite, Anchor),
}

/// A sprite from the game's data for each non-limb physical object and each other SpriteRenderer without one;
/// textures decoded once. Every sprite's z is its rank in draw order (sorting layer, sortingOrder, spawn order) over all
/// the sprites, so a new one is ranked among the placed ones and they are re-ranked with it.
pub(crate) fn object_sprites(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut play: ResMut<Play>,
    mut textures: Local<Textures>,
    mut placed: Query<(Entity, &SortKey, &mut Transform)>,
    limbs_added: Query<(), (Added<SortKey>, With<LimbSprite>)>,
) {
    let mut done = (play.done.1, play.done.2);
    let g = &play.game;
    let Some(db) = g.assets.as_deref() else { return };
    let limbs: Vec<usize> = g.limbs.iter().map(|l| l.PhysicalBehaviour).collect();
    let mut new: Vec<(SortKey, NewSprite)> = Vec::new();
    for p in done.0..g.physicals.len() {
        let Some((r, color, order)) = g.physicals[p].sprite else { continue };
        if limbs.contains(&p) {
            continue;
        }
        let Some((sprite, anchor, scale)) = make_sprite(db, &mut images, &mut textures, r, color, g.physicals[p].sprite_draw) else { continue };
        let key = SortKey { layer: sorting_layer_index(g.physicals[p].sprite_layer), order, group: 0, index: p };
        new.push((key, NewSprite::Object(p, scale, sprite, anchor)));
    }
    for i in done.1..g.renderers.len() {
        let (r, color, order) = g.renderers[i].sprite;
        let Some((sprite, anchor, scale)) = make_sprite(db, &mut images, &mut textures, r, color, g.renderers[i].draw) else { continue };
        let key = SortKey { layer: sorting_layer_index(g.renderers[i].layer), order, group: 1, index: i };
        new.push((key, NewSprite::Renderer(i, scale, sprite, anchor)));
    }
    done = (g.physicals.len(), g.renderers.len());
    (play.done.1, play.done.2) = done;
    // limb_sprites ran just before: its new limbs are ranked here too.
    if new.is_empty() && limbs_added.is_empty() {
        return;
    }

    let placed_now: Vec<(Entity, SortKey)> = placed.iter().map(|(e, k, _)| (e, *k)).collect();
    let mut keys: Vec<SortKey> = placed_now.iter().map(|&(_, k)| k).collect();
    keys.extend(new.iter().map(|(k, _)| *k));
    let depths = sprite_depths(&keys);
    let (old, fresh) = depths.split_at(placed_now.len());
    for ((entity, _), &depth) in placed_now.iter().zip(old) {
        if let Ok((_, _, mut transform)) = placed.get_mut(*entity) {
            transform.translation.z = depth;
        }
    }
    for ((key, sprite), &depth) in new.into_iter().zip(fresh) {
        match sprite {
            NewSprite::Object(p, scale, sprite, anchor) => {
                commands.spawn((ObjectSprite(p, scale), sprite, anchor, Transform::from_xyz(0.0, 0.0, depth), key));
            }
            NewSprite::Renderer(i, scale, sprite, anchor) => {
                commands.spawn((RendererSprite(i, scale), sprite, anchor, Transform::from_xyz(0.0, 0.0, depth), key));
            }
        }
    }
}

/// The Bevy sprite for a Unity Sprite: its packed region, colour, flips and draw mode, the anchor at its pivot, and
/// the scale from its units to the world.
fn make_sprite(db: &crate::assets::db::AssetDb, images: &mut Assets<Image>, textures: &mut Textures, r: crate::assets::db::ObjRef, color: [f32; 4], draw: ([bool; 2], i32, [f32; 2])) -> Option<(Sprite, Anchor, f32)> {
    {
        let s = match crate::assets::components::Sprite::read(db.data(r)) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("sprite {r:?}: {e}");
                return None;
            }
        };
        let t = db.resolve(r.file, s.texture)?;
        let handle = match textures.get(&(t.file, t.path_id)) {
            Some(h) => h.clone(),
            None => {
                let tex = crate::assets::components::Texture2D::read(db.data(t));
                let Ok((tex, rgba)) = tex.and_then(|tex| tex.rgba(&db.data_dir).map(|px| (tex, px))) else { return None };
                // Unity's rows run bottom first; an Image's top first.
                let row = tex.width as usize * 4;
                let flipped: Vec<u8> = rgba.chunks(row).rev().flatten().copied().collect();
                let image = Image::new(
                    bevy::render::render_resource::Extent3d { width: tex.width as u32, height: tex.height as u32, depth_or_array_layers: 1 },
                    bevy::render::render_resource::TextureDimension::D2,
                    flipped,
                    bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                    bevy::asset::RenderAssetUsages::RENDER_WORLD,
                );
                let h = images.add(image);
                textures.insert((t.file, t.path_id), h.clone());
                h
            }
        };
        let height = images.get(&handle).map_or(0.0, |i| i.height() as f32);
        // The packed region (texture_rect) sits at texture_rect_offset inside the sprite's rect.
        let [x, y, w, h] = s.texture_rect;
        let mut sprite = Sprite::from_image(handle);
        sprite.rect = Some(Rect::new(x, height - y - h, x + w, height - y));
        sprite.custom_size = Some(Vec2::new(w, h) / s.pixels_to_units);
        sprite.color = Color::srgba(color[0], color[1], color[2], color[3]);
        let ([flip_x, flip_y], draw_mode, size) = draw;
        (sprite.flip_x, sprite.flip_y) = (flip_x, flip_y);
        let pivot = Vec2::from_array(s.pivot) * Vec2::new(s.rect[2], s.rect[3]) - Vec2::from_array(s.texture_rect_offset);
        let mut anchor = Anchor(pivot / Vec2::new(w, h) - Vec2::splat(0.5));
        let mut scale = 1.0;
        if draw_mode != 0 {
            // Unity's border is left, bottom, right, top in pixels; Bevy's insets run from the top left.
            let [l, b, r, t] = s.border;
            let border = bevy::sprite::BorderRect { min_inset: Vec2::new(l, t), max_inset: Vec2::new(r, b) };
            // ponytail: Tiled draws as Continuous tiling; Adaptive (SpriteTileMode) would stretch below its threshold.
            let mode = if draw_mode == 1 { bevy::sprite::SliceScaleMode::Stretch } else { bevy::sprite::SliceScaleMode::Tile { stretch_value: 1.0 } };
            sprite.image_mode = bevy::sprite::SpriteImageMode::Sliced(bevy::sprite::TextureSlicer { border, center_scale_mode: mode, sides_scale_mode: mode, max_corner_scale: 1.0 });
            sprite.custom_size = Some(Vec2::from_array(size) * s.pixels_to_units);
            anchor = Anchor(Vec2::from_array(s.pivot) - Vec2::splat(0.5));
            scale = 1.0 / s.pixels_to_units;
        }
        Some((sprite, anchor, scale))
    }
}

/// A sprite for each limb without one: the starting people's, then the ones spawned since.
pub(crate) fn limb_sprites(mut commands: Commands, assets: Res<AssetServer>, rig: Res<HumanRig>, mut play: ResMut<Play>) {
    let g = &play.game;
    let done = play.done.0;
    for l in done..g.limbs.len() {
        let name = &g.physicals[g.limbs[l].PhysicalBehaviour].name;
        let Some(part) = rig.parts.iter().find(|p| &p.name == name) else { continue };
        let [x, y, w, h] = part.rect;
        let mut sprite = Sprite::from_image(assets.load(part.texture.clone()));
        sprite.rect = Some(Rect::new(x, y, x + w, y + h));
        sprite.custom_size = Some(Vec2::from_array(part.size));
        let p = g.limbs[l].PhysicalBehaviour;
        let Some((_, _, order)) = g.physicals[p].sprite else { unimplemented!("limb {name} without a SpriteRenderer") };
        let key = SortKey { layer: sorting_layer_index(g.physicals[p].sprite_layer), order, group: 0, index: p };
        commands.spawn((LimbSprite(l), sprite, Anchor(Vec2::from_array(part.pivot) - Vec2::splat(0.5)), Transform::default(), key));
    }
    play.done.0 = play.game.limbs.len();
}

pub(crate) fn draw(
    play: Res<Play>,
    settings: Res<crate::ui::Settings>,
    mut sprites: Query<(&LimbSprite, &mut Transform, &mut Visibility)>,
    mut objects: Query<(&ObjectSprite, &mut Transform, &mut Visibility), Without<LimbSprite>>,
    mut renderers: Query<(&RendererSprite, &mut Transform, &mut Visibility), (Without<LimbSprite>, Without<ObjectSprite>)>,
    mut gizmos: Gizmos,
) {
    let g = &play.game;
    for (&LimbSprite(l), mut transform, mut visibility) in &mut sprites {
        let limb = &g.limbs[l];
        let p = &g.physicals[limb.PhysicalBehaviour];
        let [sx, sy, _] = g.people[limb.Person].localScale;
        let [x, y, _] = p.transform.position;
        let [qx, qy, qz, qw] = p.transform.rotation;
        let z = transform.translation.z;
        *transform = Transform::from_xyz(x, y, z)
            .with_rotation(Quat::from_xyzw(qx, qy, qz, qw))
            .with_scale(Vec3::new(sx * p.transform.localScale[0], sy * p.transform.localScale[1], 1.0));
        *visibility = if p.active { Visibility::Inherited } else { Visibility::Hidden };
    }
    for (&ObjectSprite(p, k), mut transform, mut visibility) in &mut objects {
        let p = &g.physicals[p];
        let [x, y, _] = p.transform.position;
        let [qx, qy, qz, qw] = p.transform.rotation;
        let s = p.transform.localScale;
        let z = transform.translation.z;
        *transform = Transform::from_xyz(x, y, z).with_rotation(Quat::from_xyzw(qx, qy, qz, qw)).with_scale(Vec3::new(s[0] * k, s[1] * k, 1.0));
        *visibility = if p.active && !p.isDisintegrated { Visibility::Inherited } else { Visibility::Hidden };
    }
    for (&RendererSprite(i, k), mut transform, mut visibility) in &mut renderers {
        let r = &g.renderers[i];
        let ([x, y, _], [qx, qy, qz, qw], s) = g.anchored_world(r.anchor, &r.chain);
        let z = transform.translation.z;
        *transform = Transform::from_xyz(x, y, z).with_rotation(Quat::from_xyzw(qx, qy, qz, qw)).with_scale(Vec3::new(s[0] * k, s[1] * k, 1.0));
        let shown = match r.anchor {
            crate::game::BareAnchor::Physical(p) => g.physicals[p].active && !g.physicals[p].isDisintegrated,
            _ => true,
        };
        *visibility = if r.visible && shown { Visibility::Inherited } else { Visibility::Hidden };
    }
    // Everything else (sprite-less map parts and props) as collider outlines.
    let mut limb_bodies: Vec<_> = g.limbs.iter().map(|l| g.physicals[l.PhysicalBehaviour].rigidbody.body).collect();
    if g.assets.is_some() {
        limb_bodies.extend(g.physicals.iter().filter(|p| p.sprite.is_some()).map(|p| p.rigidbody.body));
    }
    for (id, body) in g.world.bodies.iter().enumerate() {
        let Some(body) = body else { continue };
        if limb_bodies.contains(&id) {
            continue;
        }
        let (p, q) = (body.xf.p, body.xf.q);
        let world = |v: crate::b2::math::Vec2| Vec2::new(q.c * v.x - q.s * v.y + p.x, q.s * v.x + q.c * v.y + p.y);
        for &f in &body.fixtures {
            let color = Color::srgb(0.45, 0.5, 0.56);
            match &g.world.fixture(f).shape {
                Shape::Polygon(poly) => {
                    let points = poly.vertices[..poly.count].iter().chain(&poly.vertices[..1]).map(|&v| world(v));
                    gizmos.linestrip_2d(points, color);
                }
                Shape::Circle(c) => {
                    gizmos.circle_2d(world(c.p), c.radius, color);
                }
                Shape::Chain(c) => {
                    gizmos.linestrip_2d(c.vertices.iter().map(|&v| world(v)), color);
                }
                Shape::Capsule(c) => {
                    let [a, b] = c.vertices.map(world);
                    gizmos.circle_2d(a, c.radius, color);
                    gizmos.circle_2d(b, c.radius, color);
                    let n = (b - a).normalize_or_zero().perp() * c.radius;
                    gizmos.line_2d(a + n, b + n, color);
                    gizmos.line_2d(a - n, b - n, color);
                }
            }
        }
    }
    // SelectionBoxBehaviour, shown past √200 px.
    if let (Some(t), Some(s), true) = (g.tool.as_ref(), g.input.screen, settings.flag("ShowOutlines")) {
        let d = [s.mouse[0] - t.selectionDragStart[0], s.mouse[1] - t.selectionDragStart[1]];
        if t.isSelecting && d[0] * d[0] + d[1] * d[1] > 200.0 {
            let (a, b) = (Vec2::from_array(s.to_world(t.selectionDragStart)), Vec2::from_array(s.to_world(s.mouse)));
            gizmos.rect_2d((a + b) * 0.5, (a - b).abs(), Color::srgb(0.6, 0.8, 1.0));
        }
        for &p in &t.rawSelection {
            let [x, y, _] = g.physicals[p].transform.position;
            gizmos.circle_2d(Vec2::new(x, y), 0.05, Color::srgb(0.4, 0.9, 0.5));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sorting layers are drawn in TagManager's order before sortingOrder: a Default-layer sprite with a higher order is
    /// still behind a Foreground one, and within a layer the lower order is behind.
    #[test]
    fn sprites_are_ranked_by_layer_then_order() {
        let key = |layer: i32, order: i16, index: usize| SortKey { layer: sorting_layer_index(layer), order, group: 0, index };
        let foreground = 1377110059u32 as i32;
        let default_layer = 0;
        let keys = [key(foreground, 0, 0), key(default_layer, 100, 1), key(default_layer, 3, 2), key(default_layer, 5, 3)];
        let z = sprite_depths(&keys);
        assert!(z[1] < z[0], "the Default layer is behind Foreground");
        assert!(z[2] < z[3] && z[3] < z[1], "within a layer the lower sortingOrder is behind");
        assert!(z.iter().all(|&d| (-0.5..0.5).contains(&d)), "depths stay in the camera range: {z:?}");
        assert_eq!(sorting_layer_index(2607693843u32 as i32), 7, "Decals is the last layer");
    }

    /// A frame at Time.timeScale 0 still runs (the frame count moves) with deltaTime 0 and the game clock standing.
    #[test]
    fn a_zero_time_scale_frame_still_runs() {
        let mut g = Game::default();
        g.time.timeScale = 0.0;
        g.time.fixedDeltaTime = 0.02;
        let before = (g.time.timeAsDouble, g.time.fixedTimeAsDouble, g.time.frameCount);
        step(&mut g, 1.0 / 60.0).unwrap();
        assert_eq!((g.time.timeAsDouble, g.time.fixedTimeAsDouble), (before.0, before.1));
        assert_eq!((g.time.deltaTime, g.time.frameCount), (0.0, before.2 + 1));
        assert!(g.time.realtimeSinceStartup > 0.0);
    }
}
