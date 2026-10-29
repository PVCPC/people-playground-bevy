use super::*;
use bevy::{ecs::system::RunSystemOnce, mesh::MeshPlugin, time::TimeUpdateStrategy};
use std::time::Duration;

pub fn test(rig: HumanRig, cache: PathBuf) -> Result<(), String> {
    let simulation = rig.simulation;
    let expected_bodies = rig.parts.len() + 1;
    let expected_joints = rig.joints.len();
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin {
            file_path: cache.to_string_lossy().into_owned(),
            ..default()
        },
        ImagePlugin::default_nearest(),
        MeshPlugin,
        TransformPlugin,
        physics::PhysicsPlugin,
    ))
    .insert_resource(Physics::new(simulation.gravity))
    .insert_resource(simulation)
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
        simulation.fixed_delta,
    )))
    .insert_resource(Time::<Fixed>::from_seconds(simulation.fixed_delta))
    .insert_resource(rig)
    .insert_resource(Session {
        overlay: false,
        paused: false,
        humans: 0,
        cache,
        smoke: false,
        frames: 0,
    })
    .init_resource::<Pointer>()
    .init_resource::<Drag>()
    .add_systems(Startup, setup)
    .add_systems(
        FixedUpdate,
        (living::sense_humans, living::drive_muscles, drag_body).chain(),
    );
    app.finish();
    app.cleanup();
    app.update();
    // Retain the original passive-ragdoll regression before testing active muscles.
    for mut human in app
        .world_mut()
        .query::<&mut living::Human>()
        .iter_mut(app.world_mut())
    {
        human.awake = false;
    }
    for _ in 0..150 {
        app.update();
    }
    check(&mut app, expected_bodies, expected_joints)?;

    let head = named(&mut app, "Head");
    let initial_y = pose(&app, head).0.y;
    app.world_mut().resource_mut::<Pointer>().world = Some(Vec2::new(-1.0, 1.0));
    app.world_mut().resource_mut::<Drag>().body = Some(head);
    for _ in 0..100 {
        app.update();
    }
    let lifted_y = pose(&app, head).0.y;
    if lifted_y < initial_y + 0.4 {
        return Err("Headless drag test failed to lift the human".into());
    }
    app.world_mut().resource_mut::<Drag>().body = None;
    for _ in 0..150 {
        app.update();
    }
    let released_y = pose(&app, head).0.y;
    if released_y > lifted_y - 0.2 {
        return Err("Released human did not fall under gravity".into());
    }
    check(&mut app, expected_bodies, expected_joints)?;
    println!(
        "HEADLESS TEST PASSED: settling, dragging, release, joint integrity, and render-transform synchronization; no window or renderer created."
    );
    println!("Head Y: settled={initial_y:.4}, dragged={lifted_y:.4}, released={released_y:.4}");
    if !app.world().resource::<HumanRig>().poses.is_empty() {
        active_checks(&mut app)?;
    }
    Ok(())
}

fn named(app: &mut App, name: &str) -> Entity {
    app.world_mut()
        .query::<(Entity, &Name)>()
        .iter(app.world())
        .find(|(_, n)| n.as_str() == name)
        .unwrap()
        .0
}

fn body(app: &App, entity: Entity) -> b2::world::BodyId {
    app.world().get::<Body>(entity).unwrap().0
}

fn pose(app: &App, entity: Entity) -> (Vec2, f32) {
    app.world()
        .resource::<Physics>()
        .pose(app.world().get::<Body>(entity).unwrap())
}

fn spin(app: &App, entity: Entity) -> f32 {
    app.world().resource::<Physics>().world.body(body(app, entity)).angular_velocity
}

/// Hinges as (entity, attached body, connected body, joint id), skipping ones whose bodies were destroyed.
fn hinges(app: &mut App) -> Vec<(Entity, Hinge)> {
    app.world_mut()
        .query::<(Entity, &Hinge)>()
        .iter(app.world())
        .map(|(e, h)| (e, *h))
        .collect()
}

fn active_checks(app: &mut App) -> Result<(), String> {
    app.world_mut()
        .run_system_once(
            |mut commands: Commands,
             assets: Res<AssetServer>,
             rig: Res<HumanRig>,
             mut session: ResMut<Session>,
             mut physics: ResMut<Physics>,
             objects: Query<Entity, With<SandboxObject>>| {
                for entity in &objects {
                    commands.entity(entity).despawn();
                }
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
            },
        )
        .map_err(|e| e.to_string())?;
    for _ in 0..250 {
        app.update();
    }
    check(app, 15, 13)?;
    let head = named(app, "Head");
    let torso = named(app, "UpperBody");
    let foot = named(app, "Foot");
    let height = pose(app, head).0.y - pose(app, foot).0.y;
    let tilt = pose(app, torso).1;
    println!(
        "Active human: head-to-foot height={height:.4} m, torso tilt={:.3} degrees",
        tilt.to_degrees()
    );
    if height < 1.4 || tilt.abs() > 0.25 {
        return Err("Active human failed to hold an upright rest pose".into());
    }
    for _ in 0..250 {
        app.update();
    }
    let sustained_height = pose(app, head).0.y - pose(app, foot).0.y;
    let sustained_tilt = pose(app, torso).1;
    println!(
        "Standing after 10 seconds: height={sustained_height:.4} m, tilt={:.3} degrees",
        sustained_tilt.to_degrees()
    );
    if sustained_height < 1.4 || sustained_tilt.abs() > 0.25 {
        return Err("Active human did not sustain the upright pose".into());
    }
    let owner = app.world().get::<living::Limb>(head).unwrap().owner;
    if !app.world().get::<living::Human>(owner).unwrap().state.touching_floor {
        return Err("Standing human did not detect floor contact".into());
    }
    app.world_mut()
        .get_mut::<living::Human>(owner)
        .unwrap()
        .awake = false;
    let head_body = body(app, head);
    app.world_mut()
        .resource_mut::<Physics>()
        .world
        .body_mut(head_body)
        .linear_velocity
        .x = 1.0;
    for _ in 0..200 {
        app.update();
    }
    let limp_height = pose(app, head).0.y - pose(app, foot).0.y;
    println!("Limp human: head-to-foot height={limp_height:.4} m");
    if limp_height > height - 0.5 {
        return Err("Disabled muscles did not release the pose".into());
    }
    let motor_enabled = |app: &App, joint: b2::world::JointId| {
        app.world().resource::<Physics>().world.joints[joint]
            .as_ref()
            .is_some_and(|j| j.revolute.def.enable_motor)
    };
    if hinges(app).iter().any(|(_, h)| motor_enabled(app, h.joint)) {
        return Err("A muscle motor remained enabled on a limp human".into());
    }
    app.world_mut()
        .get_mut::<living::Human>(owner)
        .unwrap()
        .awake = true;
    let sitting = app
        .world()
        .resource::<HumanRig>()
        .poses
        .iter()
        .position(|p| p.name == "Sitting")
        .ok_or("Missing sitting pose")?;
    app.world_mut()
        .get_mut::<living::Human>(owner)
        .unwrap()
        .pose = sitting;
    // Isolate pose tracking from floor obstruction after the preceding collapse test.
    let ids: Vec<_> = app
        .world_mut()
        .query_filtered::<&Body, With<Grabbable>>()
        .iter(app.world())
        .map(|b| b.0)
        .collect();
    let mut physics = app.world_mut().resource_mut::<Physics>();
    physics.world.gravity = b2::math::Vec2::ZERO;
    for id in ids {
        let b = physics.world.body(id);
        let (p, a) = (b.xf.p, b.sweep.a);
        physics.world.set_transform(id, p + b2::math::Vec2::new(0.0, 3.0), a);
        let b = physics.world.body_mut(id);
        b.linear_velocity = b2::math::Vec2::ZERO;
        b.angular_velocity = 0.0;
    }
    for _ in 0..250 {
        app.update();
    }
    check(app, 15, 13)?;
    let thigh = named(app, "UpperLeg");
    let hip = named(app, "LowerBody");
    let relative = pose(app, hip).1 - pose(app, thigh).1;
    let target = app.world().resource::<HumanRig>().poses[sitting]
        .angles
        .iter()
        .find(|a| a.part == "UpperLeg")
        .unwrap()
        .radians();
    let error = (relative - target + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    println!(
        "Sitting pose: thigh error={:.3} degrees",
        error.to_degrees()
    );
    let mut max_pose_error = 0.0f32;
    let mut max_relative_speed = 0.0f32;
    let muscles: Vec<(Hinge, usize)> = app
        .world_mut()
        .query::<(&Hinge, &living::MuscleJoint)>()
        .iter(app.world())
        .map(|(h, m)| (*h, m.definition))
        .collect();
    for (hinge, definition) in muscles {
        let name = &app.world().resource::<HumanRig>().parts[definition].name;
        let reference = app.world().resource::<Physics>().world.joints[hinge.joint]
            .as_ref()
            .unwrap()
            .revolute
            .def
            .reference_angle;
        let (a, b) = (pose(app, hinge.body_a).1, pose(app, hinge.body_b).1);
        let target = app.world().resource::<HumanRig>().poses[sitting]
            .angles
            .iter()
            .find(|a| a.part == *name)
            .map_or(0.0, |a| 2.0 * reference + a.radians());
        let pose_error = ((b - a - target + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI)
            .abs();
        let speed = (spin(app, hinge.body_b) - spin(app, hinge.body_a)).abs();
        max_pose_error = max_pose_error.max(pose_error);
        max_relative_speed = max_relative_speed.max(speed);
        if pose_error > 0.1 || speed > 1.0 {
            return Err(format!(
                "Unsettled pose motor {name}: error={pose_error} rad, speed={speed} rad/s"
            ));
        }
    }
    println!(
        "All pose motors: maximum angle error={:.3} degrees, maximum relative speed={max_relative_speed:.3} rad/s",
        max_pose_error.to_degrees()
    );
    if error.abs() > 0.2 {
        return Err("Sitting motor failed to reach the imported thigh target".into());
    }
    app.world_mut()
        .run_system_once(
            |mut commands: Commands,
             assets: Res<AssetServer>,
             rig: Res<HumanRig>,
             mut session: ResMut<Session>,
             mut physics: ResMut<Physics>| {
                spawn_human(
                    &mut commands,
                    &assets,
                    &rig,
                    &mut session,
                    &mut physics,
                    Vec2::new(-3.0, 0.6),
                );
            },
        )
        .map_err(|e| e.to_string())?;
    app.update();
    let arm = named(app, "UpperArm");
    let arm_joint = hinges(app).into_iter().find(|(_, h)| h.body_a == arm).unwrap().0;
    app.world_mut().despawn(arm_joint);
    app.update();
    if hinges(app)
        .iter()
        .any(|(_, h)| h.body_b == arm && motor_enabled(app, h.joint))
    {
        return Err("Detached arm muscles remained active".into());
    }
    let head_joint = hinges(app).into_iter().find(|(_, h)| h.body_a == head).unwrap().0;
    app.world_mut().despawn(head_joint);
    app.update();
    let motors: Vec<(bool, Entity)> = app
        .world_mut()
        .query::<(&Hinge, &living::MuscleJoint)>()
        .iter(app.world())
        .map(|(h, m)| (h.joint, m.owner))
        .collect::<Vec<_>>()
        .into_iter()
        .map(|(j, owner)| (motor_enabled(app, j), owner))
        .collect();
    if motors.iter().any(|&(on, o)| o == owner && on) {
        return Err("Human muscles remained active after brain disconnection".into());
    }
    let unaffected = motors.iter().filter(|&&(on, o)| o != owner && on).count();
    if unaffected != 13 {
        return Err("One person's injury affected another person's muscles".into());
    }
    println!(
        "ACTIVE MUSCLE TEST PASSED: upright rest, limp release, all sitting targets, detached limb and brain-disconnection gating, and isolation between two humans."
    );
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
        Floor,
        floor_body(&mut physics, -2.3, 20.0),
        Transform::from_xyz(0.0, -2.3, -1.0),
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

fn check(app: &mut App, expected_bodies: usize, expected_joints: usize) -> Result<(), String> {
    let mut bodies = app
        .world_mut()
        .query_filtered::<(&Name, &Body, &Transform, &GlobalTransform), With<Grabbable>>();
    let physics = app.world().resource::<Physics>();
    let mut count = 0;
    let mut max_transform_error = 0.0f32;
    for (name, body, transform, global) in bodies.iter(app.world()) {
        count += 1;
        let b = physics.world.body(body.0);
        let position = physics::bevy(b.xf.p);
        if !position.is_finite() || !physics::bevy(b.linear_velocity).is_finite() || position.y < -2.4 {
            return Err(format!("Invalid simulated body: {name}"));
        }
        let error = position.distance(transform.translation.truncate());
        max_transform_error = max_transform_error.max(error);
        if error > 0.05 || transform.translation.distance(global.translation()) > 0.001 {
            return Err(format!(
                "Physics/render transform mismatch on {name}: {error}"
            ));
        }
    }
    if count != expected_bodies {
        return Err("Unexpected rigid body count".into());
    }
    let mut joints = app.world_mut().query::<&Hinge>();
    let physics = app.world().resource::<Physics>();
    let mut count = 0;
    let mut max_gap = 0.0f32;
    for joint in joints.iter(app.world()) {
        count += 1;
        let (a, b) = physics.anchors(joint).ok_or("A hinge lost its joint")?;
        let gap = a.distance(b);
        max_gap = max_gap.max(gap);
        if gap > 0.05 {
            return Err(format!("Joint separation is too large: {gap}"));
        }
    }
    if count != expected_joints {
        return Err("Unexpected joint count".into());
    }
    println!(
        "Headless physics: bodies={expected_bodies}, joints={count}, maximum anchor gap={max_gap:.6} m, maximum render-transform error={max_transform_error:.6} m"
    );
    Ok(())
}
