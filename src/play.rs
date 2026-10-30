//! --play <recording>: the rewritten game in a window, starting from the game's own state in a PPG_DUMP recording
//! (its first FixedUpdate snapshot with the b2 world) and running Unity's player loop from there.

use crate::b2::world::Shape;
use crate::game::{dump, Game};
use crate::game_assets::HumanRig;
use bevy::{prelude::*, sprite::Anchor, text::FontSize};
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

fn load(recording: &Path, rig: &HumanRig) -> Result<Game, String> {
    let text = std::fs::read_to_string(recording).map_err(|e| e.to_string())?;
    let snaps = dump::parse(&text);
    let start = snaps
        .iter()
        .find(|s| s.pt == "FS" && s.of("b2World").next().is_some())
        .ok_or("--play needs a recording with the b2 world (dump12 on)")?;
    Game::from_snapshot(start, rig)
}

/// --play-test: 30 s of 60 fps frames without a window; prints where each limb ends up.
pub fn test(recording: &Path, rig: &HumanRig) -> Result<(), String> {
    let mut g = load(recording, rig)?;
    for n in 0..1800 {
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

pub fn run(recording: &Path, rig: HumanRig, cache: PathBuf) -> Result<(), String> {
    let game = load(recording, &rig)?;
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
        .add_systems(Update, (controls, frame, draw).chain())
        .run();
    Ok(())
}

fn setup(mut commands: Commands, assets: Res<AssetServer>, rig: Res<HumanRig>, play: Res<Play>) {
    let g = &play.game;
    let center = g.limbs.first().map_or([0.0; 3], |l| g.physicals[l.PhysicalBehaviour].transform.position);
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection { scale: 0.006, ..OrthographicProjection::default_2d() }),
        Transform::from_xyz(center[0], center[1], 0.0),
    ));
    for (l, limb) in g.limbs.iter().enumerate() {
        let name = &g.physicals[limb.PhysicalBehaviour].name;
        let Some(part) = rig.parts.iter().find(|p| &p.name == name) else { continue };
        let [x, y, w, h] = part.rect;
        let mut sprite = Sprite::from_image(assets.load(part.texture.clone()));
        sprite.rect = Some(Rect::new(x, y, x + w, y + h));
        sprite.custom_size = Some(Vec2::from_array(part.size));
        commands.spawn((LimbSprite(l), sprite, Anchor(Vec2::from_array(part.pivot) - Vec2::splat(0.5)), Transform::default()));
    }
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
    }))
    .map_err(|panic| {
        panic.downcast_ref::<String>().cloned().or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default()
    })
}

fn draw(
    play: Res<Play>,
    mut sprites: Query<(&LimbSprite, &mut Transform, &mut Visibility)>,
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
    // Everything else (the map, its props) as collider outlines.
    let limb_bodies: Vec<_> = g.limbs.iter().map(|l| g.physicals[l.PhysicalBehaviour].rigidbody.body).collect();
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
            }
        }
    }
    hud.0 = match &play.stopped {
        Some(m) => format!("PP-R/B   STOPPED: {m}   Esc: exit"),
        None if play.paused => "PP-R/B   PAUSED   Space: run   Esc: exit".into(),
        None => format!("PP-R/B   frame {}   Space: pause   Esc: exit", g.time.frameCount),
    };
}
