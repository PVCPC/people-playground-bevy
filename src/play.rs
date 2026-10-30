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
        Start::Map(game, map, item, _) => return new_game(game, map, item.as_deref(), prefs).map(Some),
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
pub(crate) fn new_game(game: &Path, map: &str, item: Option<&str>, prefs: Option<Prefs>) -> Result<Game, String> {
    let mut db = AssetDb::open(game)?;
    let mut g = Game::load_main(&mut db, game)?;
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

/// A sprite from the game's data for each non-limb physical object and each other SpriteRenderer without one;
/// textures decoded once.
pub(crate) fn object_sprites(mut commands: Commands, mut images: ResMut<Assets<Image>>, mut play: ResMut<Play>, mut textures: Local<Textures>) {
    let mut done = (play.done.1, play.done.2);
    let g = &play.game;
    let Some(db) = g.assets.as_deref() else { return };
    let limbs: Vec<usize> = g.limbs.iter().map(|l| l.PhysicalBehaviour).collect();
    for p in done.0..g.physicals.len() {
        let Some((r, color, order)) = g.physicals[p].sprite else { continue };
        if limbs.contains(&p) {
            continue;
        }
        let Some((sprite, anchor, scale)) = make_sprite(db, &mut images, &mut textures, r, color, g.physicals[p].sprite_draw) else { continue };
        // ponytail: sorting order only; sorting layers (TagManager order) when two layers overlap wrongly.
        commands.spawn((ObjectSprite(p, scale), sprite, anchor, Transform::from_xyz(0.0, 0.0, order as f32 * 0.01 - 0.5)));
    }
    for i in done.1..g.renderers.len() {
        let (r, color, order) = g.renderers[i].sprite;
        let Some((sprite, anchor, scale)) = make_sprite(db, &mut images, &mut textures, r, color, g.renderers[i].draw) else { continue };
        commands.spawn((RendererSprite(i, scale), sprite, anchor, Transform::from_xyz(0.0, 0.0, order as f32 * 0.01 - 0.5)));
    }
    done = (g.physicals.len(), g.renderers.len());
    (play.done.1, play.done.2) = done;
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
        commands.spawn((LimbSprite(l), sprite, Anchor(Vec2::from_array(part.pivot) - Vec2::splat(0.5)), Transform::default()));
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
        *transform = Transform::from_xyz(x, y, l as f32 * 0.001)
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
