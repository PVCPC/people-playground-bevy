mod desktop;
mod game_assets;
mod headless;
mod hinge;
mod living;
mod motor;

use avian2d::dynamics::{integrator::IntegrationSystems, solver::schedule::SubstepSolverSystems};
use avian2d::prelude::*;
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
            _ => return Err(format!("Unknown argument: {argument}")),
        }
    }
    let game = game_assets::find_game(explicit_game)?;
    let rig = HumanRig::load(&cache, &game)?;
    let cache = cache.canonicalize().map_err(|e| e.to_string())?;
    if let Some(probe) = reference_probe {
        return motor::compare_reference(&probe);
    }
    if let Some(probe) = native_probe {
        return hinge::compare_reference(&probe);
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
        .add_plugins(PhysicsPlugins::default())
        .add_systems(
            SubstepSchedule,
            motor::solve_velocity_motors
                .after(SubstepSolverSystems::SolveConstraints)
                .before(IntegrationSystems::Position),
        )
        .insert_resource(rig)
        .insert_resource(Session {
            overlay: false,
            paused: false,
            humans: 0,
            cache,
            smoke,
            frames: 0,
        })
        .insert_resource(Time::<Fixed>::from_hz(50.0))
        .insert_resource(SubstepCount(10))
        .insert_resource(Gravity(Vec2::new(0.0, -9.81)))
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
        RigidBody::Static,
        Collider::rectangle(12.0, 0.2),
        Position(Vec2::new(0.0, -2.2)),
        Transform::from_xyz(0.0, -2.2, -1.0),
        Sprite::from_color(Color::srgb(0.19, 0.22, 0.26), Vec2::new(12.0, 0.2)),
        Friction::new(0.6),
        Restitution::new(0.05),
        CollisionLayers::ALL,
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
        Vec2::new(0.0, 0.6),
    );
    spawn_box(&mut commands, Vec2::new(1.3, -1.7));
}

fn spawn_human(
    commands: &mut Commands,
    assets: &AssetServer,
    rig: &HumanRig,
    session: &mut Session,
    offset: Vec2,
) {
    if session.humans >= 28 {
        return;
    }
    let group = 1u32 << (session.humans + 2);
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
        let collider = Collider::compound(
            part.colliders
                .iter()
                .map(|shape| {
                    (
                        Vec2::from_array(shape.offset),
                        0.0,
                        Collider::rectangle(shape.size[0], shape.size[1]),
                    )
                })
                .collect(),
        );
        let (friction, restitution) = part.physics_material.as_ref().map_or(
            (Friction::new(0.5), Restitution::new(0.0)),
            |material| {
                (
                    Friction::new(material.friction)
                        .with_combine_rule(CoefficientCombine::GeometricMean),
                    Restitution::new(material.restitution)
                        .with_combine_rule(CoefficientCombine::Max),
                )
            },
        );
        let entity = commands
            .spawn((
                (
                    SandboxObject,
                    Name::new(part.name.clone()),
                    SleepingDisabled,
                ),
                RigidBody::Dynamic,
                collider,
                Mass(part.mass),
                LinearDamping(part.linear_drag),
                AngularDamping(part.angular_drag),
                friction,
                restitution,
                CollisionLayers::from_bits(group, !group),
                Position(Vec2::from_array(part.position) + offset),
                Rotation::radians(part.angle),
                Transform::from_translation(
                    (Vec2::from_array(part.position) + offset).extend(order),
                )
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
        commands.entity(entity).insert((
            living::Limb { owner, definition },
            CollidingEntities::default(),
        ));
        entities.insert(part.name.as_str(), entity);
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
        root: entities[root.name.as_str()],
        brain: entities[brain.name.as_str()],
        grounded: false,
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
        let mut hinge = RevoluteJoint::new(entities[joint.a.as_str()], entities[joint.b.as_str()])
            .with_local_anchor1(Vec2::from_array(joint.anchor_a))
            .with_local_anchor2(Vec2::from_array(joint.anchor_b));
        // Native FetchBodyA is attached; FetchBodyB is connected. Both solvers measure B minus A.
        if let Some([min, max]) = joint.limits {
            hinge = hinge.with_angle_limits(min + reference, max + reference);
        }
        let mut velocity_motor = motor::VelocityMotor::default();
        velocity_motor.reference_angle = reference;
        commands.spawn((
            SandboxObject,
            hinge,
            JointCollisionDisabled,
            velocity_motor,
            living::MuscleJoint {
                owner,
                definition,
                previous_speed: 0.0,
            },
        ));
    }
}

fn spawn_box(commands: &mut Commands, point: Vec2) {
    commands.spawn((
        SandboxObject,
        Name::new("Test crate"),
        RigidBody::Dynamic,
        Collider::rectangle(0.5, 0.5),
        Mass(1.0),
        Friction::new(0.6),
        CollisionLayers::from_bits(1, u32::MAX),
        Position(point),
        Transform::from_translation(point.extend(0.2)),
        Sprite::from_color(Color::srgb(0.63, 0.4, 0.22), Vec2::splat(0.5)),
        Grabbable {
            shapes: vec![BoxShape {
                offset: [0.0; 2],
                size: [0.5; 2],
            }],
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
    mut physics: ResMut<Time<Physics>>,
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
        if session.paused {
            physics.pause();
        } else {
            physics.unpause();
        }
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
            Vec2::new(0.0, 0.6),
        );
        spawn_box(&mut commands, Vec2::new(1.3, -1.7));
    } else if let Some(point) = pointer.world {
        if keys.just_pressed(KeyCode::KeyN) {
            spawn_human(&mut commands, &assets, &rig, &mut session, point);
        }
        if keys.just_pressed(KeyCode::KeyB) {
            spawn_box(&mut commands, point);
        }
    }
}

fn contains(point: Vec2, position: Vec2, rotation: Rotation, shapes: &[BoxShape]) -> bool {
    let local = rotation.inverse() * (point - position);
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
    bodies: Query<(Entity, &Position, &Rotation, &Grabbable)>,
    joints: Query<(Entity, &RevoluteJoint)>,
    mut window: Single<&mut CursorOptions, With<PrimaryWindow>>,
    keys: Res<ButtonInput<KeyCode>>,
    rig: Res<HumanRig>,
    limbs: Query<&living::Limb>,
    mut humans: Query<&mut living::Human>,
) {
    let hovered = pointer.world.and_then(|point| {
        bodies
            .iter()
            .filter(|(_, p, r, g)| contains(point, p.0, **r, &g.shapes))
            .max_by(|a, b| a.3.order.total_cmp(&b.3.order))
    });
    if pointer.left_pressed
        && let Some((entity, position, rotation, _)) = hovered
    {
        drag.body = Some(entity);
        drag.local_anchor = rotation.inverse() * (pointer.world.unwrap() - position.0);
    }
    if !pointer.left {
        drag.body = None;
    }
    if pointer.right_pressed
        && let Some((body, _, _, _)) = hovered
    {
        if let Some((entity, _)) = joints.iter().find(|(_, j)| j.body1 == body) {
            commands.entity(entity).despawn();
        }
    }
    if let Some((body, _, _, _)) = hovered
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
    mut bodies: Query<(&Position, &Rotation, &Grabbable, Forces)>,
) {
    if session.paused {
        return;
    }
    let (Some(entity), Some(target)) = (drag.body, pointer.world) else {
        return;
    };
    let Ok((position, rotation, grabbable, mut forces)) = bodies.get_mut(entity) else {
        return;
    };
    let point = position.0 + *rotation * drag.local_anchor;
    let acceleration =
        ((target - point) * 180.0 - forces.velocity_at_point(point) * 20.0).clamp_length_max(350.0);
    forces.apply_force_at_point(acceleration * grabbable.mass, point);
}

fn presentation(
    session: Res<Session>,
    drag: Res<Drag>,
    pointer: Res<Pointer>,
    window: Single<(&Window, &RawHandleWrapper), With<PrimaryWindow>>,
    mut floor: Single<(&mut Position, &mut Sprite, &mut Collider), With<Floor>>,
    mut hud: Single<&mut Text, With<Hud>>,
    bodies: Query<&Position, (With<Grabbable>, Without<Floor>)>,
    mut gizmos: Gizmos,
) {
    let (window, raw) = *window;
    desktop::non_activating(raw, session.overlay);
    let extent = Vec2::new(window.width(), window.height()) * WORLD_SCALE * 0.5;
    let floor_y = -extent.y + 0.1;
    let (position, sprite, collider) = &mut *floor;
    if position.0.y != floor_y || sprite.custom_size != Some(Vec2::new(extent.x * 2.0, 0.2)) {
        position.0 = Vec2::new(0.0, floor_y);
        sprite.custom_size = Some(Vec2::new(extent.x * 2.0, 0.2));
        **collider = Collider::rectangle(extent.x * 2.0, 0.2);
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
        && let Ok(position) = bodies.get(entity)
    {
        gizmos.line_2d(position.0, target, Color::srgb(0.4, 0.85, 1.0));
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
    bodies: Query<(&Position, &Rotation, &LinearVelocity), With<Grabbable>>,
    floor: Single<&Position, With<Floor>>,
    joints: Query<&RevoluteJoint>,
    physics: Res<Time<Physics>>,
    images: Res<Assets<Image>>,
    rig: Res<HumanRig>,
    assets: Res<AssetServer>,
    mut exit: MessageWriter<AppExit>,
) {
    if !session.smoke {
        return;
    }
    session.frames += 1;
    if session.frames < 240 || physics.elapsed_secs() < 4.0 {
        return;
    }
    assert_eq!(bodies.iter().count(), rig.parts.len() + 1);
    for (position, _, velocity) in &bodies {
        assert!(
            position.0.is_finite() && velocity.0.is_finite(),
            "Physics produced non-finite values"
        );
        assert!(
            position.0.y > floor.0.y - 0.1,
            "Body fell through the floor"
        );
    }
    assert!(
        bodies.iter().any(|(p, _, _)| p.0.y < floor.0.y + 0.5),
        "No body reached the floor"
    );
    for joint in &joints {
        let (a, ar, _) = bodies.get(joint.body1).unwrap();
        let (b, br, _) = bodies.get(joint.body2).unwrap();
        let anchor_a = a.0 + *ar * joint.local_anchor1().unwrap();
        let anchor_b = b.0 + *br * joint.local_anchor2().unwrap();
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
        let rotation = Rotation::radians(std::f32::consts::FRAC_PI_2);
        assert!(contains(Vec2::new(0.4, 0.0), Vec2::ZERO, rotation, &shape));
        assert!(!contains(Vec2::new(0.0, 0.4), Vec2::ZERO, rotation, &shape));
    }
}
