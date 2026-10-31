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

/// Each decoded texture with its width and height.
type Textures = std::collections::HashMap<(usize, i64), (Handle<Image>, [f32; 2])>;

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
    Object(usize, f32, Drawn),
    Renderer(usize, f32, Drawn),
}

/// A sprite from the game's data for each non-limb physical object and each other SpriteRenderer without one;
/// textures decoded once. Every sprite's z is its rank in draw order (sorting layer, sortingOrder, spawn order) over all
/// the sprites, so a new one is ranked among the placed ones and they are re-ranked with it.
pub(crate) fn object_sprites(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
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
        let Some((drawn, scale)) = make_sprite(db, &mut images, &mut meshes, &mut materials, &mut textures, r, color, g.physicals[p].sprite_draw) else { continue };
        let key = SortKey { layer: sorting_layer_index(g.physicals[p].sprite_layer), order, group: 0, index: p };
        new.push((key, NewSprite::Object(p, scale, drawn)));
    }
    for i in done.1..g.renderers.len() {
        let (r, color, order) = g.renderers[i].sprite;
        let Some((drawn, scale)) = make_sprite(db, &mut images, &mut meshes, &mut materials, &mut textures, r, color, g.renderers[i].draw) else { continue };
        let key = SortKey { layer: sorting_layer_index(g.renderers[i].layer), order, group: 1, index: i };
        new.push((key, NewSprite::Renderer(i, scale, drawn)));
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
            NewSprite::Object(p, scale, drawn) => drawn.insert(commands.spawn((ObjectSprite(p, scale), Transform::from_xyz(0.0, 0.0, depth), key))),
            NewSprite::Renderer(i, scale, drawn) => drawn.insert(commands.spawn((RendererSprite(i, scale), Transform::from_xyz(0.0, 0.0, depth), key))),
        }
    }
}

/// How a SpriteRenderer reaches Bevy: a Simple one as a Sprite, a Sliced or Tiled one as the mesh Unity builds for it.
enum Drawn {
    Sprite(Sprite, Anchor),
    Mesh(Handle<Mesh>, Handle<ColorMaterial>),
}

impl Drawn {
    fn insert(self, mut entity: EntityCommands) {
        match self {
            Drawn::Sprite(sprite, anchor) => entity.insert((sprite, anchor)),
            Drawn::Mesh(mesh, material) => entity.insert((Mesh2d(mesh), MeshMaterial2d(material))),
        };
    }
}

/// A sprite's input to Unity's PrepareSpriteTilingData.
struct Tiling {
    size: [f32; 2],
    /// SpriteDrawMode: Sliced 1, Tiled 2.
    draw_mode: i32,
    adaptive: bool,
    threshold: f32,
    /// Sprite.rect's width and height in pixels.
    rect: [f32; 2],
    pivot: [f32; 2],
    /// left, bottom, right, top in pixels.
    border: [f32; 4],
    pixels_to_units: f32,
    /// uvMin.xy, uvMax.xy.
    uv: [f32; 4],
}

/// GetSourceAndDestinationRect on one axis: the (source start, source length, destination start, destination length)
/// of the near border, the middle and the far border, without those that are empty.
fn nine_slice_spans(near: f32, far: f32, rect: f32, length: f32) -> Vec<[f32; 4]> {
    let both = near + far;
    let scale = if length < both { length / both } else { 1.0 };
    let (mut source, mut destination, mut spans) = (0.0, 0.0, Vec::new());
    for [s, d] in [[near, near * scale], [rect - both, length - both], [far, far * scale]] {
        if !(s <= 0.0 || d <= 0.0) {
            spans.push([source, s, destination, d]);
        }
        source += if s < 0.0 { 0.0 } else { s };
        destination += if d < 0.0 { 0.0 } else { d };
    }
    spans
}

/// One axis of a section's whole tiles at Adaptive: below the threshold the last fraction stretches the others.
fn adaptive_tiles(area: f32, tile: f32, threshold: f32) -> f32 {
    let ratio = area / tile;
    let nearest = ratio.round_ties_even();
    let whole = nearest - if ratio < nearest { 1.0 } else { 0.0 };
    let frac = ratio - whole;
    f32::max(1.0, whole + if tile * threshold < frac { 1.0 } else { 0.0 })
}

/// The quads of PrepareSpriteTilingData + GenerateSpriteTileMesh, each four (position, uv) in Unity's sprite space.
fn tile_quads(t: &Tiling) -> Vec<[([f32; 2], [f32; 2]); 4]> {
    let units = 1.0 / t.pixels_to_units;
    let rect = t.rect.map(|v| v * units);
    let b = t.border.map(|v| v * units);
    let origin = [0, 1].map(|i| -(t.rect[i] * t.pivot[i]) * t.size[i] / t.rect[i]);
    let across = nine_slice_spans(b[0], b[2], rect[0], t.size[0].abs());
    let up = nine_slice_spans(b[1], b[3], rect[1], t.size[1].abs());
    let at = [0, 1].map(|i| origin[i] + if t.size[i] < 0.0 { t.size[i] } else { 0.0 });
    // Rows from the bottom, each from the left: (uv offset, tile, position, area).
    let sections: Vec<[[f32; 2]; 4]> = up
        .iter()
        .flat_map(|y| across.iter().map(move |x| (x, y)))
        .map(|(x, y)| [[x[0], y[0]], [x[1], y[1]], [x[2] + at[0], y[2] + at[1]], [x[3], y[3]]])
        .collect();
    let (mut verts, mut indices) = (0i32, 0i32);
    for [_, tile, _, area] in &sections {
        if t.draw_mode != 2 {
            verts += 4;
            indices += 6;
            continue;
        }
        let count = [0, 1].map(|i| if t.adaptive { adaptive_tiles(area[i], tile[i], t.threshold) } else { (area[i] / tile[i]).ceil() });
        let product = count[0] * count[1];
        verts = (product * 4.0 + verts as f32) as i32;
        indices = (product * 6.0 + indices as f32) as i32;
    }
    let (uv_min, uv_max) = ([t.uv[0], t.uv[1]], [t.uv[2], t.uv[3]]);
    // Too big for one mesh, or nothing at all: the plain quad.
    if verts <= 0 || verts >= 0xffff || indices >= 0x2fffd {
        let max = [origin[0] + t.size[0], origin[1] + t.size[1]];
        return vec![[(origin, uv_min), ([max[0], origin[1]], [uv_max[0], uv_min[1]]), (max, uv_max), ([origin[0], max[1]], [uv_min[0], uv_max[1]])]];
    }
    let span = [uv_max[0] - uv_min[0], uv_max[1] - uv_min[1]];
    let uv_of = |v: f32, i: usize| v * (1.0 / rect[i]) * span[i] + [uv_min[0], uv_min[1]][i];
    let mut quads = Vec::new();
    for [offset, tile, position, area] in &sections {
        let (count, used) = if t.draw_mode != 2 {
            ([1.0, 1.0], *area)
        } else if t.adaptive {
            let count = [0, 1].map(|i| adaptive_tiles(area[i], tile[i], t.threshold));
            (count, [area[0] / count[0], area[1] / count[1]])
        } else {
            ([area[0] / tile[0], area[1] / tile[1]], *tile)
        };
        let (u0, v0) = (uv_of(offset[0], 0), uv_of(offset[1], 1));
        let (u1, v1) = (uv_of(offset[0] + tile[0], 0), uv_of(offset[1] + tile[1], 1));
        let mut y = 0;
        while count[1] > y as f32 {
            let yf = y as f32;
            let h = if count[1] - yf > 1.0 || t.adaptive { 1.0 } else { count[1] - yf };
            let top = (1.0 - h) * v0 + h * v1;
            let mut x = 0;
            while count[0] > x as f32 {
                let xf = x as f32;
                let w = if count[0] - xf > 1.0 || t.adaptive { 1.0 } else { count[0] - xf };
                let right = (1.0 - w) * u0 + w * u1;
                let (x2, y2) = (w + xf, h + yf);
                let at = |x: f32, y: f32| [x * used[0] + position[0], y * used[1] + position[1]];
                quads.push([(at(xf, yf), [u0, v0]), (at(x2, yf), [right, v0]), (at(x2, y2), [right, top]), (at(xf, y2), [u0, top])]);
                x += 1;
            }
            y += 1;
        }
    }
    quads
}

/// The Bevy mesh of [`tile_quads`], mirrored by the renderer's flips about the pivot as the sprite shader's _Flip does.
fn tile_mesh(t: &Tiling, flip: [bool; 2]) -> Mesh {
    let sign = flip.map(|f| if f { -1.0 } else { 1.0 });
    let (mut positions, mut uvs, mut indices) = (Vec::new(), Vec::new(), Vec::new());
    for quad in tile_quads(t) {
        let first = positions.len() as u32;
        let p: Vec<[f32; 3]> = quad.iter().map(|(p, _)| [p[0] * sign[0], p[1] * sign[1], 0.0]).collect();
        // Mesh2d culls back faces; Unity's sprites draw both.
        let front = (p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[1][1] - p[0][1]) * (p[2][0] - p[0][0]) >= 0.0;
        indices.extend(if front { [0, 1, 2, 0, 2, 3] } else { [0, 2, 1, 0, 3, 2] }.map(|i| first + i));
        positions.extend(p);
        // Unity's v runs up; an Image's down.
        uvs.extend(quad.iter().map(|(_, uv)| [uv[0], 1.0 - uv[1]]));
    }
    let mut mesh = Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::RENDER_WORLD);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(bevy::mesh::Indices::U32(indices));
    mesh
}

/// What Bevy draws for a Unity Sprite: its packed region, colour, flips and draw mode, the anchor at its pivot, and
/// the scale from its units to the world.
fn make_sprite(db: &crate::assets::db::AssetDb, images: &mut Assets<Image>, meshes: &mut Assets<Mesh>, materials: &mut Assets<ColorMaterial>, textures: &mut Textures, r: crate::assets::db::ObjRef, color: [f32; 4], draw: crate::game::SpriteDraw) -> Option<(Drawn, f32)> {
    {
        let s = match crate::assets::components::Sprite::read(db.data(r)) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("sprite {r:?}: {e}");
                return None;
            }
        };
        let t = db.resolve(r.file, s.texture)?;
        let (handle, texture) = match textures.get(&(t.file, t.path_id)) {
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
                let entry = (images.add(image), [tex.width as f32, tex.height as f32]);
                textures.insert((t.file, t.path_id), entry.clone());
                entry
            }
        };
        // The packed region (texture_rect) sits at texture_rect_offset inside the sprite's rect.
        let [x, y, w, h] = s.texture_rect;
        let linear = Color::srgba(color[0], color[1], color[2], color[3]);
        let ([flip_x, flip_y], draw_mode, size, threshold, adaptive) = draw;
        if draw_mode != 0 {
            // The rect's corner in the texture: the packed region less its offset.
            let [u, v] = [(x - s.texture_rect_offset[0]) / texture[0], (y - s.texture_rect_offset[1]) / texture[1]];
            let uv = [u, v, u + s.rect[2] / texture[0], v + s.rect[3] / texture[1]];
            let tiling = Tiling { size, draw_mode, adaptive, threshold, rect: [s.rect[2], s.rect[3]], pivot: s.pivot, border: s.border, pixels_to_units: s.pixels_to_units, uv };
            let mesh = meshes.add(tile_mesh(&tiling, [flip_x, flip_y]));
            let material = materials.add(ColorMaterial { color: linear, texture: Some(handle), ..default() });
            return Some((Drawn::Mesh(mesh, material), 1.0));
        }
        let mut sprite = Sprite::from_image(handle);
        sprite.rect = Some(Rect::new(x, texture[1] - y - h, x + w, texture[1] - y));
        sprite.custom_size = Some(Vec2::new(w, h) / s.pixels_to_units);
        sprite.color = linear;
        (sprite.flip_x, sprite.flip_y) = (flip_x, flip_y);
        let pivot = Vec2::from_array(s.pivot) * Vec2::new(s.rect[2], s.rect[3]) - Vec2::from_array(s.texture_rect_offset);
        // _Flip mirrors about the pivot; Bevy's flips only swap the uvs, so the anchor mirrors with them.
        let mut anchor = pivot / Vec2::new(w, h) - Vec2::splat(0.5);
        anchor *= Vec2::new(if flip_x { -1.0 } else { 1.0 }, if flip_y { -1.0 } else { 1.0 });
        Some((Drawn::Sprite(sprite, Anchor(anchor)), 1.0))
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

#[cfg(test)]
mod tiling_tests {
    use super::*;

    // A 4 by 4 unit sprite at 1 pixel per unit, centre pivot, no border, whole texture.
    fn tiled(size: [f32; 2], adaptive: bool, threshold: f32) -> Tiling {
        Tiling { size, draw_mode: 2, adaptive, threshold, rect: [4.0, 4.0], pivot: [0.5, 0.5], border: [0.0; 4], pixels_to_units: 1.0, uv: [0.0, 0.0, 1.0, 1.0] }
    }

    #[test]
    fn adaptive_stretches_below_the_threshold_and_adds_a_tile_above_it() {
        // ratio 1.25: the fraction .25 under threshold*tile 2.0 stretches one tile.
        assert_eq!(adaptive_tiles(5.0, 4.0, 0.5), 1.0);
        // The same fraction over threshold*tile 0.04 adds a second.
        assert_eq!(adaptive_tiles(5.0, 4.0, 0.01), 2.0);
        let stretched = tile_quads(&tiled([5.0, 4.0], true, 0.5));
        assert_eq!(stretched.len(), 1);
        assert_eq!(stretched[0][1], ([2.5, -2.0], [1.0, 0.0]));
    }

    #[test]
    fn continuous_clips_the_last_tile() {
        let q = tile_quads(&tiled([6.0, 4.0], false, 0.0));
        assert_eq!(q.len(), 2);
        assert_eq!(q[1][0].0, [1.0, -2.0]);
        assert_eq!(q[1][1], ([3.0, -2.0], [0.5, 0.0]));
    }

    #[test]
    fn nine_slice_keeps_borders_and_scales_them_when_too_small() {
        let mut t = tiled([10.0, 4.0], false, 0.0);
        t.draw_mode = 1;
        t.border = [1.0, 0.0, 2.0, 0.0];
        let q = tile_quads(&t);
        assert_eq!(q.len(), 3);
        assert_eq!((q[0][0].0[0], q[0][1].0[0]), (-5.0, -4.0));
        assert_eq!((q[1][0].0[0], q[1][1].0[0]), (-4.0, 3.0));
        assert_eq!((q[2][0].0[0], q[2][1].0[0]), (3.0, 5.0));
        assert_eq!((q[1][0].1[0], q[1][1].1[0]), (0.25, 0.5));
        // Drawn 1.5 wide: borders 1 and 2 scale by 1.5 / 3, the middle goes.
        assert_eq!(nine_slice_spans(1.0, 2.0, 4.0, 1.5), vec![[0.0, 1.0, 0.0, 0.5], [2.0, 2.0, 0.5, 1.0]]);
    }

    #[test]
    fn too_many_tiles_fall_back_to_one_quad() {
        let t = Tiling { rect: [1.0, 1.0], ..tiled([1000.0, 1000.0], false, 0.0) };
        assert_eq!(tile_quads(&t).len(), 1);
    }

    #[test]
    fn flips_mirror_about_the_pivot_and_stay_front_facing() {
        let mesh = tile_mesh(&tiled([4.0, 4.0], false, 0.0), [true, false]);
        let bevy::mesh::VertexAttributeValues::Float32x3(p) = mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() else { panic!() };
        assert_eq!(p[0], [2.0, -2.0, 0.0]);
        let Some(bevy::mesh::Indices::U32(i)) = mesh.indices() else { panic!() };
        assert_eq!(i[..3], [0, 2, 1]);
    }
}
