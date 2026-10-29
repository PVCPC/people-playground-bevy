mod b2;
mod desktop;
mod game;
mod game_assets;
mod headless;
mod hinge;
mod integration;
mod living;
mod motor;
mod physics;
mod replay;
mod unity;

use b2::{
    math::Vec2 as V,
    revolute::RevoluteDef,
    world::{BodyDef, BodyType, Filter, RigidbodyMass},
};
use bevy::{
    app::AppExit,
    prelude::*,
    sprite::Anchor,
    text::FontSize,
    window::{
        CursorOptions, MonitorSelection, PrimaryWindow, RawHandleWrapper, WindowLevel, WindowMode,
    },
};
use game_assets::{BoxShape, HumanRig};
use physics::{Body, Hinge, Physics};
use std::{collections::HashMap, path::PathBuf};

const WORLD_SCALE: f32 = 0.006;
const BACKGROUND: Color = Color::srgb(0.055, 0.065, 0.08);

#[derive(Resource)]
struct Session {
    overlay: bool,
    paused: bool,
    humans: u32,
    cache: PathBuf,
    smoke: bool,
    frames: u32,
}
#[derive(Resource, Default)]
struct Pointer {
    world: Option<Vec2>,
    left: bool,
    left_pressed: bool,
    right_pressed: bool,
    last_right: bool,
    last_toggle: bool,
    toggle: bool,
    local: Option<Vec2>,
}
#[derive(Resource, Default)]
struct Drag {
    body: Option<Entity>,
    local_anchor: Vec2,
}
#[derive(Component)]
struct SandboxObject;
#[derive(Component)]
struct Floor;
#[derive(Component)]
struct Hud;
#[derive(Component)]
struct Grabbable {
    shapes: Vec<BoxShape>,
    mass: f32,
    order: f32,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut explicit_game = None;
    let mut cache = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local-cache");
    let mut smoke = false;
    let mut headless = false;
    let mut reference_probe = None;
    let mut native_probe = None;
    let mut b2_probe = None;
    let mut replay = None;
    let mut script = None;
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--game-dir" => {
                explicit_game = Some(PathBuf::from(
                    arguments.next().ok_or("--game-dir needs a path")?,
                ))
            }
            "--cache" => cache = arguments.next().ok_or("--cache needs a path")?.into(),
            "--smoke-test" => smoke = true,
            "--headless-test" => headless = true,
            "--reference-test" => {
                reference_probe =
                    Some(PathBuf::from(arguments.next().ok_or(
                        "--reference-test needs the private probe executable path",
                    )?))
            }
            "--native-reference-test" => {
                native_probe = Some(PathBuf::from(
                    arguments
                        .next()
                        .ok_or("--native-reference-test needs a probe path")?,
                ));
            }
            "--b2-reference-test" => {
                b2_probe = Some(PathBuf::from(
                    arguments.next().ok_or("--b2-reference-test needs a probe path")?,
                ));
            }
            "--random-test" => {
                let path = PathBuf::from(arguments.next().ok_or("--random-test needs a recording")?);
                return replay::compare_random(&path);
            }
            "--replay-test" => {
                replay = Some(PathBuf::from(arguments.next().ok_or("--replay-test needs a recording")?));
            }
            "--script-test" => {
                script = Some(PathBuf::from(arguments.next().ok_or("--script-test needs a PPG_DUMP recording")?));
            }
            _ => return Err(format!("Unknown argument: {argument}")),
        }
    }
    if let Some(probe) = b2_probe {
        return b2::probe::compare_reference(&probe);
    }
    let game = game_assets::find_game(explicit_game)?;
    let rig = HumanRig::load(&cache, &game)?;
    let cache = cache.canonicalize().map_err(|e| e.to_string())?;
    if let Some(recording) = replay {
        return replay::compare_motors(&rig, &recording);
    }
    if let Some(dump) = script {
        return game::dump::script_test(&dump, &rig);
    }
    if let Some(probe) = reference_probe {
        return motor::compare_reference(&probe);
    }
    if let Some(probe) = native_probe {
        hinge::compare_reference(&probe)?;
        return integration::compare_reference(&probe);
    }
    println!("Installed game verified: {}", game.display());
    println!(
        "Imported human: {} rigid bodies, {} joints",
        rig.parts.len(),
        rig.joints.len()
    );
    if rig.poses.is_empty() {
        println!("This cache has no muscles or poses; prepare a fresh cache to enable them.");
    }
    if rig.parts.iter().any(|p| p.physics_material.is_none()) {
        println!(
            "This cache has prototype contact materials; prepare a fresh cache to import the game's values."
        );
    }
    if headless {
        return headless::test(rig, cache);
    }
    let simulation = rig.simulation;
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "People Playground / Bevy — prototype".into(),
                        resolution: (1280, 800).into(),
                        transparent: true,
                        ..default()
                    }),
                    ..default()
                })
                .set(AssetPlugin {
                    file_path: cache.to_string_lossy().into_owned(),
                    ..default()
                })
                .set(ImagePlugin::default_nearest()),
        )
        .add_plugins(physics::PhysicsPlugin)
        .insert_resource(Physics::new(simulation.gravity))
        .insert_resource(simulation)
        .insert_resource(rig)
        .insert_resource(Session {
            overlay: false,
            paused: false,
            humans: 0,
            cache,
            smoke,
            frames: 0,
        })
        .insert_resource(Time::<Fixed>::from_seconds(simulation.fixed_delta))
        .insert_resource(ClearColor(BACKGROUND))
        .init_resource::<Pointer>()
        .init_resource::<Drag>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                read_pointer,
                controls,
                mouse_actions,
                presentation,
                smoke_test,
            )
                .chain(),
        )
        .add_systems(
            FixedUpdate,
            (living::sense_humans, living::drive_muscles, drag_body).chain(),
        )
        .run();
    Ok(())
}

fn setup(
    mut commands: Commands,
    assets: Res<AssetServer>,
    rig: Res<HumanRig>,
    mut session: ResMut<Session>,
    mut physics: ResMut<Physics>,
) {
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scale: WORLD_SCALE,
            ..OrthographicProjection::default_2d()
        }),
    ));
    commands.spawn((
        Floor,
        floor_body(&mut physics, -2.2, 12.0),
        Transform::from_xyz(0.0, -2.2, -1.0),
        Sprite::from_color(Color::srgb(0.19, 0.22, 0.26), Vec2::new(12.0, 0.2)),
    ));
    commands.spawn((
        Hud,
        Text::new("Loading local game assets…"),
        TextFont {
            font_size: FontSize::Px(16.0),
            ..default()
        },
        TextColor(Color::srgb(0.86, 0.9, 0.94)),
        BackgroundColor(Color::srgba(0.035, 0.045, 0.06, 0.92)),
        Node {
            position_type: PositionType::Absolute,
            left: px(12),
            top: px(12),
            padding: UiRect::all(px(12)),
            ..default()
        },
    ));
    spawn_human(
        &mut commands,
        &assets,
        &rig,
        &mut session,
        &mut physics,
        Vec2::new(0.0, 0.6),
    );
    spawn_box(&mut commands, &mut physics, Vec2::new(1.3, -1.7));
}

fn floor_body(physics: &mut Physics, y: f32, width: f32) -> Body {
    let id = physics.world.create_body(&BodyDef {
        position: V::new(0.0, y),
        ..BodyDef::default()
    });
    let shape = BoxShape {
        offset: [0.0; 2],
        size: [width, 0.2],
    };
    physics.world.create_fixture(
        id,
        &physics::box_fixture(&shape, 0.6, 0.05, Filter::default()),
        true,
    );
    Body(id)
}

fn spawn_human(
    commands: &mut Commands,
    assets: &AssetServer,
    rig: &HumanRig,
    session: &mut Session,
    physics: &mut Physics,
    offset: Vec2,
) {
    if session.humans >= 28 {
        return;
    }
    // One negative group per person: its own limbs never collide, other people's do.
    let filter = Filter {
        group_index: -(session.humans as i16 + 1),
        ..Filter::default()
    };
    session.humans += 1;
    let owner = commands.spawn(SandboxObject).id();
    let mut entities = HashMap::new();
    for (definition, part) in rig.parts.iter().enumerate() {
        let [x, y, w, h] = part.rect;
        let order = part.layer as f32 * 0.01
            + if part.name.ends_with("Front") {
                0.005
            } else {
                0.0
            };
        let mut sprite = Sprite::from_image(assets.load(part.texture.clone()));
        sprite.rect = Some(Rect::new(x, y, x + w, y + h));
        sprite.custom_size = Some(Vec2::from_array(part.size));
        let (friction, restitution) = part
            .physics_material
            .as_ref()
            .map_or((0.5, 0.0), |m| (m.friction, m.restitution));
        let position = Vec2::from_array(part.position) + offset;
        // ponytail: limbs never sleep and are not bullets; PPG's sleep mode and CollisionQuality are not imported yet.
        let id = physics.world.create_body(&BodyDef {
            kind: BodyType::Dynamic,
            position: physics::v(position),
            angle: part.angle,
            linear_damping: part.linear_drag,
            angular_damping: part.angular_drag,
            allow_sleep: false,
            rigidbody: RigidbodyMass {
                mass: part.mass,
                ..RigidbodyMass::default()
            },
            ..BodyDef::default()
        });
        for shape in &part.colliders {
            let fixture = physics::box_fixture(shape, friction, restitution, filter);
            physics.world.create_fixture(id, &fixture, true);
        }
        let entity = commands
            .spawn((
                SandboxObject,
                Name::new(part.name.clone()),
                Body(id),
                Transform::from_translation(position.extend(order))
                    .with_rotation(Quat::from_rotation_z(part.angle)),
                sprite,
                Anchor(Vec2::from_array(part.pivot) - Vec2::splat(0.5)),
                Grabbable {
                    shapes: part.colliders.clone(),
                    mass: part.mass,
                    order,
                },
            ))
            .id();
        commands
            .entity(entity)
            .insert(living::Limb {
                owner,
                definition,
                state: Default::default(),
            });
        entities.insert(part.name.as_str(), (entity, id));
    }
    let root = rig
        .parts
        .iter()
        .find(|part| !rig.joints.iter().any(|joint| joint.a == part.name))
        .unwrap();
    let brain = rig
        .parts
        .iter()
        .find(|part| part.muscle.as_ref().is_some_and(|m| m.has_brain))
        .unwrap_or(root);
    commands.entity(owner).insert(living::Human {
        awake: !rig.poses.is_empty(),
        pose: 0,
        root: entities[root.name.as_str()].0,
        brain: entities[brain.name.as_str()].0,
        state: Default::default(),
        connected: Default::default(),
    });
    for joint in &rig.joints {
        let definition = rig
            .parts
            .iter()
            .position(|part| part.name == joint.a)
            .unwrap();
        let parent = rig.parts.iter().find(|part| part.name == joint.b).unwrap();
        let reference = parent.angle - rig.parts[definition].angle;
        let (a, b) = (entities[joint.a.as_str()], entities[joint.b.as_str()]);
        // Native FetchBodyA is attached; FetchBodyB is connected. Both solvers measure B minus A.
        let [lower_angle, upper_angle] = joint.radians_limits().unwrap_or([0.0; 2]);
        let def = RevoluteDef {
            local_anchor_a: physics::v(Vec2::from_array(joint.anchor_a)),
            local_anchor_b: physics::v(Vec2::from_array(joint.anchor_b)),
            reference_angle: reference,
            enable_limit: joint.radians_limits().is_some(),
            lower_angle,
            upper_angle,
            ..RevoluteDef::default()
        };
        commands.spawn((
            SandboxObject,
            Hinge {
                joint: physics.world.create_joint(def, a.1, b.1, false),
                body_a: a.0,
                body_b: b.0,
            },
            living::MuscleJoint::new(owner, definition),
        ));
    }
}

fn spawn_box(commands: &mut Commands, physics: &mut Physics, point: Vec2) {
    let shape = BoxShape {
        offset: [0.0; 2],
        size: [0.5; 2],
    };
    let id = physics.world.create_body(&BodyDef {
        kind: BodyType::Dynamic,
        position: physics::v(point),
        ..BodyDef::default()
    });
    physics.world.create_fixture(
        id,
        &physics::box_fixture(&shape, 0.6, 0.0, Filter::default()),
        true,
    );
    commands.spawn((
        SandboxObject,
        Name::new("Test crate"),
        Body(id),
        Transform::from_translation(point.extend(0.2)),
        Sprite::from_color(Color::srgb(0.63, 0.4, 0.22), Vec2::splat(0.5)),
        Grabbable {
            shapes: vec![shape],
            mass: 1.0,
            order: 0.2,
        },
    ));
}

fn read_pointer(
    mut pointer: ResMut<Pointer>,
    session: Res<Session>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    window: Single<(&Window, &RawHandleWrapper), With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform)>,
) {
    let (window, raw) = *window;
    let toggle = desktop::key_down(0x79) || keys.pressed(KeyCode::F10);
    pointer.toggle = toggle && !pointer.last_toggle;
    pointer.last_toggle = toggle;
    let local = if session.overlay {
        desktop::cursor(raw, window.scale_factor())
    } else {
        window.cursor_position()
    };
    let (camera, transform) = *camera;
    pointer.world = local.and_then(|p| camera.viewport_to_world_2d(transform, p).ok());
    pointer.local = local;
    let left = if session.overlay {
        desktop::key_down(0x01)
    } else {
        mouse.pressed(MouseButton::Left)
    };
    let right = if session.overlay {
        desktop::key_down(0x02)
    } else {
        mouse.pressed(MouseButton::Right)
    };
    pointer.left_pressed = left && !pointer.left;
    pointer.right_pressed = right && !pointer.last_right;
    pointer.left = left;
    pointer.last_right = right;
}

fn controls(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    pointer: Res<Pointer>,
    mut session: ResMut<Session>,
    mut drag: ResMut<Drag>,
    rig: Res<HumanRig>,
    assets: Res<AssetServer>,
    objects: Query<Entity, With<SandboxObject>>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    mut clear: ResMut<ClearColor>,
    mut physics: ResMut<Physics>,
    mut exit: MessageWriter<AppExit>,
) {
    if keys.just_pressed(KeyCode::Escape) || (session.overlay && desktop::key_down(0x1b)) {
        exit.write(AppExit::Success);
    }
    if pointer.toggle {
        session.overlay = !session.overlay;
        window.decorations = !session.overlay;
        window.window_level = if session.overlay {
            WindowLevel::AlwaysOnTop
        } else {
            WindowLevel::Normal
        };
        window.mode = if session.overlay {
            WindowMode::BorderlessFullscreen(MonitorSelection::Primary)
        } else {
            WindowMode::Windowed
        };
        if !session.overlay {
            window.resolution.set(1280.0, 800.0);
        }
        clear.0 = if session.overlay {
            Color::NONE
        } else {
            BACKGROUND
        };
    }
    if keys.just_pressed(KeyCode::Space) {
        session.paused = !session.paused;
        physics.paused = session.paused;
    }
    if keys.just_pressed(KeyCode::KeyR) {
        for entity in &objects {
            commands.entity(entity).despawn();
        }
        drag.body = None;
        session.humans = 0;
        spawn_human(
            &mut commands,
            &assets,
            &rig,
            &mut session,
            &mut physics,
            Vec2::new(0.0, 0.6),
        );
        spawn_box(&mut commands, &mut physics, Vec2::new(1.3, -1.7));
    } else if let Some(point) = pointer.world {
        if keys.just_pressed(KeyCode::KeyN) {
            spawn_human(
                &mut commands,
                &assets,
                &rig,
                &mut session,
                &mut physics,
                point,
            );
        }
        if keys.just_pressed(KeyCode::KeyB) {
            spawn_box(&mut commands, &mut physics, point);
        }
    }
}

fn contains(point: Vec2, position: Vec2, angle: f32, shapes: &[BoxShape]) -> bool {
    let local = Vec2::from_angle(-angle).rotate(point - position);
    shapes.iter().any(|s| {
        ((local - Vec2::from_array(s.offset)).abs() - Vec2::from_array(s.size) * 0.5).max_element()
            <= 0.025
    })
}

fn mouse_actions(
    mut commands: Commands,
    pointer: Res<Pointer>,
    mut drag: ResMut<Drag>,
    session: Res<Session>,
    bodies: Query<(Entity, &Body, &Grabbable)>,
    joints: Query<(Entity, &Hinge)>,
    physics: Res<Physics>,
    mut window: Single<&mut CursorOptions, With<PrimaryWindow>>,
    keys: Res<ButtonInput<KeyCode>>,
    rig: Res<HumanRig>,
    limbs: Query<&living::Limb>,
    mut humans: Query<&mut living::Human>,
) {
    let hovered = pointer.world.and_then(|point| {
        bodies
            .iter()
            .map(|(entity, body, g)| (entity, physics.pose(body), g))
            .filter(|(_, (p, a), g)| contains(point, *p, *a, &g.shapes))
            .max_by(|a, b| a.2.order.total_cmp(&b.2.order))
    });
    if pointer.left_pressed
        && let Some((entity, (position, angle), _)) = hovered
    {
        drag.body = Some(entity);
        drag.local_anchor = Vec2::from_angle(-angle).rotate(pointer.world.unwrap() - position);
    }
    if !pointer.left {
        drag.body = None;
    }
    if pointer.right_pressed
        && let Some((body, _, _)) = hovered
    {
        if let Some((entity, _)) = joints.iter().find(|(_, j)| j.body_a == body) {
            commands.entity(entity).despawn();
        }
    }
    if let Some((body, _, _)) = hovered
        && let Ok(limb) = limbs.get(body)
        && let Ok(mut human) = humans.get_mut(limb.owner)
    {
        if keys.just_pressed(KeyCode::KeyM) {
            human.awake = !human.awake;
        }
        if keys.just_pressed(KeyCode::KeyP) && !rig.poses.is_empty() {
            human.pose = (human.pose + 1) % rig.poses.len();
        }
    }
    let toolbar = pointer
        .local
        .is_some_and(|p| p.x >= 12.0 && p.x < 800.0 && p.y >= 12.0 && p.y < 92.0);
    window.hit_test = !session.overlay || drag.body.is_some() || hovered.is_some() || toolbar;
}

fn drag_body(
    pointer: Res<Pointer>,
    drag: Res<Drag>,
    session: Res<Session>,
    mut physics: ResMut<Physics>,
    bodies: Query<(&Body, &Grabbable)>,
) {
    if session.paused {
        return;
    }
    let (Some(entity), Some(target)) = (drag.body, pointer.world) else {
        return;
    };
    let Ok((body, grabbable)) = bodies.get(entity) else {
        return;
    };
    let body = physics.world.body_mut(body.0);
    let point = body.xf.apply(physics::v(drag.local_anchor));
    let velocity = physics::bevy(body.velocity_at(point));
    let acceleration = ((target - physics::bevy(point)) * 180.0 - velocity * 20.0)
        .clamp_length_max(350.0);
    body.apply_force(physics::v(acceleration * grabbable.mass), point);
}

fn presentation(
    session: Res<Session>,
    drag: Res<Drag>,
    pointer: Res<Pointer>,
    window: Single<(&Window, &RawHandleWrapper), With<PrimaryWindow>>,
    mut floor: Single<(&mut Body, &mut Sprite), With<Floor>>,
    mut physics: ResMut<Physics>,
    mut hud: Single<&mut Text, With<Hud>>,
    bodies: Query<&Body, (With<Grabbable>, Without<Floor>)>,
    mut gizmos: Gizmos,
) {
    let (window, raw) = *window;
    desktop::non_activating(raw, session.overlay);
    let extent = Vec2::new(window.width(), window.height()) * WORLD_SCALE * 0.5;
    let floor_y = -extent.y + 0.1;
    let (body, sprite) = &mut *floor;
    if physics.pose(body).0.y != floor_y
        || sprite.custom_size != Some(Vec2::new(extent.x * 2.0, 0.2))
    {
        // A static body is rebuilt, not resized, so its hook-less id swap destroys the old one here.
        physics.world.destroy_body(body.0);
        **body = floor_body(&mut physics, floor_y, extent.x * 2.0);
        sprite.custom_size = Some(Vec2::new(extent.x * 2.0, 0.2));
    }
    if !session.overlay {
        let color = Color::srgb(0.095, 0.11, 0.13);
        for x in -(extent.x as i32 + 1)..=(extent.x as i32 + 1) {
            gizmos.line_2d(
                Vec2::new(x as f32, -extent.y),
                Vec2::new(x as f32, extent.y),
                color,
            );
        }
        for y in -(extent.y as i32 + 1)..=(extent.y as i32 + 1) {
            gizmos.line_2d(
                Vec2::new(-extent.x, y as f32),
                Vec2::new(extent.x, y as f32),
                color,
            );
        }
    }
    if let (Some(entity), Some(target)) = (drag.body, pointer.world)
        && let Ok(body) = bodies.get(entity)
    {
        gizmos.line_2d(physics.pose(body).0, target, Color::srgb(0.4, 0.85, 1.0));
    }
    hud.0 = format!(
        "PEOPLE PLAYGROUND / BEVY     {}     {}     {} bodies\nLMB: drag  RMB: detach  N: human  B: crate  M: limp  P: pose  Space: pause  R: reset  Esc: exit",
        if session.overlay {
            "DESKTOP"
        } else {
            "SANDBOX"
        },
        if session.paused { "PAUSED" } else { "RUNNING" },
        bodies.iter().count()
    );
}

fn smoke_test(
    mut session: ResMut<Session>,
    bodies: Query<&Body, With<Grabbable>>,
    floor: Single<&Body, With<Floor>>,
    joints: Query<&Hinge>,
    physics: Res<Physics>,
    images: Res<Assets<Image>>,
    rig: Res<HumanRig>,
    assets: Res<AssetServer>,
    mut exit: MessageWriter<AppExit>,
) {
    if !session.smoke {
        return;
    }
    session.frames += 1;
    if session.frames < 240 || physics.elapsed < 4.0 {
        return;
    }
    assert_eq!(bodies.iter().count(), rig.parts.len() + 1);
    let floor_y = physics.pose(&floor).0.y;
    for body in &bodies {
        let b = physics.world.body(body.0);
        assert!(
            physics::bevy(b.xf.p).is_finite() && physics::bevy(b.linear_velocity).is_finite(),
            "Physics produced non-finite values"
        );
        assert!(b.xf.p.y > floor_y - 0.1, "Body fell through the floor");
    }
    assert!(
        bodies.iter().any(|b| physics.pose(b).0.y < floor_y + 0.5),
        "No body reached the floor"
    );
    for joint in &joints {
        let (anchor_a, anchor_b) = physics.anchors(joint).unwrap();
        assert!(
            anchor_a.distance(anchor_b) < 0.05,
            "Ragdoll joint separated"
        );
    }
    for part in &rig.parts {
        assert!(
            images
                .get(&assets.load::<Image>(part.texture.clone()))
                .is_some(),
            "Texture did not load"
        );
    }
    println!(
        "SMOKE TEST PASSED: finite bodies, floor collision, and original textures loaded from {}",
        session.cache.display()
    );
    exit.write(AppExit::Success);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn picking_rotates_the_collider_into_local_space() {
        let shape = [BoxShape {
            offset: [0.0; 2],
            size: [0.2, 1.0],
        }];
        let angle = std::f32::consts::FRAC_PI_2;
        assert!(contains(Vec2::new(0.4, 0.0), Vec2::ZERO, angle, &shape));
        assert!(!contains(Vec2::new(0.0, 0.4), Vec2::ZERO, angle, &shape));
    }
}
