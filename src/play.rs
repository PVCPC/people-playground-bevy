//! --play <recording>: the rewritten game in a window, starting from the game's own state in a PPG_DUMP recording
//! (its first FixedUpdate snapshot with the b2 world) and running Unity's player loop from there.
//! --play-map <map>: the same, starting from LoadScene("Main") with that map, all from the game's data.

use crate::b2::world::Shape;
use crate::game::{dump, Game};
use crate::game_assets::HumanRig;
use bevy::{prelude::*, sprite::Anchor, text::FontSize};
use crate::assets::db::AssetDb;
use std::path::{Path, PathBuf};

#[derive(Resource)]
struct Play {
    game: Game,
    /// Set when a frame panicked (an unrewritten feature): the world stays frozen as it was.
    stopped: Option<String>,
    paused: bool,
}

/// A limb's sprite, drawn at its PhysicalBehaviour's Transform.
#[derive(Component)]
struct LimbSprite(usize);

#[derive(Component)]
struct Hud;

/// Where the game starts.
pub enum Start {
    Recording(PathBuf),
    /// The installed game's folder, the Map asset's name and the catalog item selected (if any).
    Map(PathBuf, String, Option<String>),
}

fn load(start: &Start, rig: &HumanRig) -> Result<Game, String> {
    let recording = match start {
        Start::Recording(recording) => recording,
        Start::Map(game, map, item) => return new_game(game, map, item.as_deref()),
    };
    let start = dump::snapshots(recording)?
        .find(|s| s.pt == "FS" && s.of("b2World").next().is_some())
        .ok_or("--play needs a recording with the b2 world (dump12 on)")?;
    let mut g = Game::from_snapshot(&start, rig)?;
    g.load_layers(&dump::header(recording)?);
    Ok(g)
}

/// SceneManager.LoadScene("Main") with the main menu's map choice: level1's managers and natives, then the
/// MapLoaderBehaviour's Awake. Its behaviours' Starts run in the first frame.
fn new_game(game: &Path, map: &str, item: Option<&str>) -> Result<Game, String> {
    let mut db = AssetDb::open(game)?;
    let mut g = Game::load_main(&mut db, game)?;
    g.load_scene(&mut db)?;
    g.map_loader_awake(&mut db, map)?;
    g.assets = Some(Box::new(db));
    g.catalog_selected = item.map(str::to_string);
    Ok(g)
}

/// --play-test: 30 s of 60 fps frames without a window; prints where each limb ends up. A --play-map-test with
/// --item spawns the item at the map's center on frame 30 and a flipped one on frame 60.
pub fn test(start: &Start, rig: &HumanRig) -> Result<(), String> {
    let mut g = load(start, rig)?;
    for n in 0..1800 {
        // With a catalog item selected: "spawnRight" released at the map's center on frame 30, "spawnLeft" 3 m
        // to the right on frame 60.
        if g.catalog_selected.is_some() {
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
    for l in &g.limbs {
        let p = &g.physicals[l.PhysicalBehaviour];
        println!("{:14} {:?} active={}", p.name, &p.transform.position[..2], p.active);
    }
    Ok(())
}

pub fn run(start: &Start, rig: HumanRig, cache: PathBuf) -> Result<(), String> {
    let game = load(start, &rig)?;
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window { title: "PP-R/B".into(), resolution: (1280, 800).into(), ..default() }),
                    ..default()
                })
                .set(AssetPlugin { file_path: cache.to_string_lossy().into_owned(), ..default() })
                .set(ImagePlugin::default_nearest()),
        )
        .insert_resource(ClearColor(Color::srgb(0.055, 0.065, 0.08)))
        .insert_resource(rig)
        .insert_resource(Play { game, stopped: None, paused: false })
        .add_systems(Startup, setup)
        .add_systems(Update, (controls, mouse, frame, limb_sprites, object_sprites, draw).chain())
        .run();
    Ok(())
}

fn setup(mut commands: Commands, play: Res<Play>) {
    let g = &play.game;
    let center = g.limbs.first().map_or([g.BoundingBox.center[0], g.BoundingBox.center[1], 0.0], |l| g.physicals[l.PhysicalBehaviour].transform.position);
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection { scale: 0.006, ..OrthographicProjection::default_2d() }),
        Transform::from_xyz(center[0], center[1], 0.0),
    ));
    commands.spawn((
        Hud,
        Text::new(""),
        TextFont { font_size: FontSize::Px(16.0), ..default() },
        TextColor(Color::srgb(0.86, 0.9, 0.94)),
        Node { position_type: PositionType::Absolute, left: px(12), top: px(12), ..default() },
    ));
}

fn controls(keys: Res<ButtonInput<KeyCode>>, mut play: ResMut<Play>, mut exit: MessageWriter<AppExit>) {
    if keys.just_pressed(KeyCode::Space) {
        play.paused = !play.paused;
    }
    if keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
}

/// The cursor and left button as InputSystem's "drag" control and Input.mousePosition, through this window's camera.
fn mouse(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    window: Single<&Window>,
    camera: Single<(&Camera, &GlobalTransform)>,
    mut play: ResMut<Play>,
) {
    let (camera, at) = *camera;
    let size = window.size();
    let (Ok(origin), Ok(one)) = (camera.viewport_to_world_2d(at, Vec2::new(0.0, size.y)), camera.viewport_to_world_2d(at, Vec2::new(1.0, size.y))) else { return };
    let input = &mut play.game.input;
    if let (Ok(min), Ok(max)) = (camera.viewport_to_world_2d(at, Vec2::new(0.0, size.y)), camera.viewport_to_world_2d(at, Vec2::new(size.x, 0.0))) {
        input.view = Some((min.to_array(), max.to_array()));
    }
    if let Some(c) = window.cursor_position() {
        // Unity's screen y runs up from the bottom.
        let pixel = [c.x, size.y - c.y];
        let screen = crate::game::tool::Screen { mouse: pixel, origin: origin.to_array(), scale: one.x - origin.x };
        let w = screen.to_world(pixel);
        input.mouse = [w[0], w[1], 0.0];
        input.screen = Some(screen);
    }
    let control = crate::game::tool::Control {
        down: buttons.just_pressed(MouseButton::Left),
        held: buttons.pressed(MouseButton::Left),
        up: buttons.just_released(MouseButton::Left),
    };
    input.controls.insert("drag".into(), control);
    // InputSystem's default "spawnRight" (E) and "spawnLeft" (Q).
    for (name, key) in [("spawnRight", KeyCode::KeyE), ("spawnLeft", KeyCode::KeyQ)] {
        let control = crate::game::tool::Control { down: keys.just_pressed(key), held: keys.pressed(key), up: keys.just_released(key) };
        input.controls.insert(name.into(), control);
    }
}

/// One Unity player-loop frame: FixedUpdate scripts and the physics pass until fixed time catches up with the
/// frame's time, then the PreUpdate interpolation and the Update/LateUpdate scripts.
fn frame(time: Res<bevy::time::Time<Real>>, mut play: ResMut<Play>) {
    if play.paused || play.stopped.is_some() {
        return;
    }
    if let Err(m) = step(&mut play.game, time.delta_secs()) {
        play.stopped = Some(m);
    }
}

/// A frame panic (an unrewritten feature) comes back as its message.
fn step(g: &mut Game, real: f32) -> Result<(), String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let t = &mut g.time;
        // Time.deltaTime: the unscaled frame time clamped to maximumDeltaTime, then scaled.
        let dt = real.min(t.maximumDeltaTime) * t.timeScale;
        let frame_time = t.timeAsDouble + dt as f64;
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
        t.frameCount += 1;
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
    }))
    .map_err(|panic| {
        panic.downcast_ref::<String>().cloned().or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default()
    })
}

/// A physical object's own sprite (the limbs use the rig's).
#[derive(Component)]
struct ObjectSprite(usize);

/// A sprite from the game's data for each non-limb physical object without one; textures decoded once.
fn object_sprites(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    play: Res<Play>,
    mut done: Local<usize>,
    mut textures: Local<std::collections::HashMap<(usize, i64), Handle<Image>>>,
) {
    let g = &play.game;
    let Some(db) = g.assets.as_deref() else { return };
    let limbs: Vec<usize> = g.limbs.iter().map(|l| l.PhysicalBehaviour).collect();
    for p in *done..g.physicals.len() {
        let Some((r, color, order)) = g.physicals[p].sprite else { continue };
        if limbs.contains(&p) {
            continue;
        }
        let s = match crate::assets::components::Sprite::read(db.data(r)) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{}: {e}", g.physicals[p].name);
                continue;
            }
        };
        let Some(t) = db.resolve(r.file, s.texture) else { continue };
        let handle = match textures.get(&(t.file, t.path_id)) {
            Some(h) => h.clone(),
            None => {
                let tex = crate::assets::components::Texture2D::read(db.data(t));
                let Ok((tex, rgba)) = tex.and_then(|tex| tex.rgba(&db.data_dir).map(|px| (tex, px))) else { continue };
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
        let pivot = Vec2::from_array(s.pivot) * Vec2::new(s.rect[2], s.rect[3]) - Vec2::from_array(s.texture_rect_offset);
        let anchor = Anchor(pivot / Vec2::new(w, h) - Vec2::splat(0.5));
        // ponytail: sorting order only; sorting layers (TagManager order) when two layers overlap wrongly.
        commands.spawn((ObjectSprite(p), sprite, anchor, Transform::from_xyz(0.0, 0.0, order as f32 * 0.01 - 0.5)));
    }
    *done = g.physicals.len();
}

/// A sprite for each limb without one: the starting people's, then the ones spawned since.
fn limb_sprites(mut commands: Commands, assets: Res<AssetServer>, rig: Res<HumanRig>, play: Res<Play>, mut done: Local<usize>) {
    let g = &play.game;
    for l in *done..g.limbs.len() {
        let name = &g.physicals[g.limbs[l].PhysicalBehaviour].name;
        let Some(part) = rig.parts.iter().find(|p| &p.name == name) else { continue };
        let [x, y, w, h] = part.rect;
        let mut sprite = Sprite::from_image(assets.load(part.texture.clone()));
        sprite.rect = Some(Rect::new(x, y, x + w, y + h));
        sprite.custom_size = Some(Vec2::from_array(part.size));
        commands.spawn((LimbSprite(l), sprite, Anchor(Vec2::from_array(part.pivot) - Vec2::splat(0.5)), Transform::default()));
    }
    *done = g.limbs.len();
}

fn draw(
    play: Res<Play>,
    mut sprites: Query<(&LimbSprite, &mut Transform, &mut Visibility)>,
    mut objects: Query<(&ObjectSprite, &mut Transform, &mut Visibility), Without<LimbSprite>>,
    mut hud: Single<&mut Text, With<Hud>>,
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
    for (&ObjectSprite(p), mut transform, mut visibility) in &mut objects {
        let p = &g.physicals[p];
        let [x, y, _] = p.transform.position;
        let [qx, qy, qz, qw] = p.transform.rotation;
        let s = p.transform.localScale;
        let z = transform.translation.z;
        *transform = Transform::from_xyz(x, y, z).with_rotation(Quat::from_xyzw(qx, qy, qz, qw)).with_scale(Vec3::new(s[0], s[1], 1.0));
        *visibility = if p.active && !p.isDisintegrated { Visibility::Inherited } else { Visibility::Hidden };
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
            }
        }
    }
    // SelectionBoxBehaviour, shown past √200 px.
    if let (Some(t), Some(s)) = (g.tool.as_ref(), g.input.screen) {
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
    hud.0 = match &play.stopped {
        Some(m) => format!("PP-R/B   STOPPED: {m}   Esc: exit"),
        None if play.paused => "PP-R/B   PAUSED   Space: run   Esc: exit".into(),
        None => format!("PP-R/B   frame {}   Space: pause   Esc: exit", g.time.frameCount),
    };
}
