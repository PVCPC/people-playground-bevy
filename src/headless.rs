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
        motor::PortPhysicsPlugin,
    ))
    .insert_resource(simulation)
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
        simulation.fixed_delta,
    )))
    .insert_resource(Time::<Fixed>::from_seconds(simulation.fixed_delta))
    .insert_resource(SubstepCount(10))
    .insert_resource(Gravity(Vec2::from_array(simulation.gravity)))
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

    let head = app
        .world_mut()
        .query::<(Entity, &Name)>()
        .iter(app.world())
        .find(|(_, name)| name.as_str() == "Head")
        .map(|(entity, _)| entity)
        .ok_or("Imported human has no head")?;
    let initial_y = app.world().get::<Position>(head).unwrap().0.y;
    app.world_mut().resource_mut::<Pointer>().world = Some(Vec2::new(-1.0, 1.0));
    app.world_mut().resource_mut::<Drag>().body = Some(head);
    for _ in 0..100 {
        app.update();
    }
    let lifted_y = app.world().get::<Position>(head).unwrap().0.y;
    if lifted_y < initial_y + 0.4 {
        return Err("Headless drag test failed to lift the human".into());
    }
    app.world_mut().resource_mut::<Drag>().body = None;
    for _ in 0..150 {
        app.update();
    }
    let released_y = app.world().get::<Position>(head).unwrap().0.y;
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

fn active_checks(app: &mut App) -> Result<(), String> {
    app.world_mut()
        .run_system_once(
            |mut commands: Commands,
             assets: Res<AssetServer>,
             rig: Res<HumanRig>,
             mut session: ResMut<Session>,
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
                    Vec2::new(0.0, 0.6),
                );
                spawn_box(&mut commands, Vec2::new(1.3, -1.7));
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
    let height = app.world().get::<Position>(head).unwrap().0.y
        - app.world().get::<Position>(foot).unwrap().0.y;
    let tilt = app.world().get::<Rotation>(torso).unwrap().as_radians();
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
    let sustained_height = app.world().get::<Position>(head).unwrap().0.y
        - app.world().get::<Position>(foot).unwrap().0.y;
    let sustained_tilt = app.world().get::<Rotation>(torso).unwrap().as_radians();
    println!(
        "Standing after 10 seconds: height={sustained_height:.4} m, tilt={:.3} degrees",
        sustained_tilt.to_degrees()
    );
    if sustained_height < 1.4 || sustained_tilt.abs() > 0.25 {
        return Err("Active human did not sustain the upright pose".into());
    }
    let owner = app.world().get::<living::Limb>(head).unwrap().owner;
    if !app.world().get::<living::Human>(owner).unwrap().grounded {
        return Err("Standing human did not detect floor contact".into());
    }
    app.world_mut()
        .get_mut::<living::Human>(owner)
        .unwrap()
        .awake = false;
    app.world_mut().get_mut::<LinearVelocity>(head).unwrap().0.x = 1.0;
    for _ in 0..200 {
        app.update();
    }
    let limp_height = app.world().get::<Position>(head).unwrap().0.y
        - app.world().get::<Position>(foot).unwrap().0.y;
    println!("Limp human: head-to-foot height={limp_height:.4} m");
    if limp_height > height - 0.5 {
        return Err("Disabled muscles did not release the pose".into());
    }
    if app
        .world_mut()
        .query::<&motor::VelocityMotor>()
        .iter(app.world())
        .any(|m| m.enabled)
    {
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
    app.world_mut().resource_mut::<Gravity>().0 = Vec2::ZERO;
    for (mut position, mut velocity, mut spin) in app.world_mut()
        .query_filtered::<(&mut Position, &mut LinearVelocity, &mut AngularVelocity), With<Grabbable>>()
        .iter_mut(app.world_mut()) {
        position.0.y += 3.0;
        velocity.0 = Vec2::ZERO;
        spin.0 = 0.0;
    }
    for _ in 0..250 {
        app.update();
    }
    check(app, 15, 13)?;
    let thigh = named(app, "UpperLeg");
    let hip = named(app, "LowerBody");
    let relative = app.world().get::<Rotation>(hip).unwrap().as_radians()
        - app.world().get::<Rotation>(thigh).unwrap().as_radians();
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
    for (joint, muscle, motor) in app
        .world_mut()
        .query::<(&RevoluteJoint, &living::MuscleJoint, &motor::VelocityMotor)>()
        .iter(app.world())
    {
        let name = &app.world().resource::<HumanRig>().parts[muscle.definition].name;
        let a = app
            .world()
            .get::<Rotation>(joint.body1)
            .unwrap()
            .as_radians();
        let b = app
            .world()
            .get::<Rotation>(joint.body2)
            .unwrap()
            .as_radians();
        let target = app.world().resource::<HumanRig>().poses[sitting]
            .angles
            .iter()
            .find(|a| a.part == *name)
            .map_or(0.0, |a| 2.0 * motor.reference_angle + a.radians());
        let pose_error = ((b - a - target + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI)
            .abs();
        let speed = (app.world().get::<AngularVelocity>(joint.body2).unwrap().0
            - app.world().get::<AngularVelocity>(joint.body1).unwrap().0)
            .abs();
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
             mut session: ResMut<Session>| {
                spawn_human(
                    &mut commands,
                    &assets,
                    &rig,
                    &mut session,
                    Vec2::new(-3.0, 0.6),
                );
            },
        )
        .map_err(|e| e.to_string())?;
    app.update();
    let arm = named(app, "UpperArm");
    let arm_joint = app
        .world_mut()
        .query::<(Entity, &RevoluteJoint)>()
        .iter(app.world())
        .find(|(_, j)| j.body1 == arm)
        .unwrap()
        .0;
    app.world_mut().despawn(arm_joint);
    app.update();
    let arm_enabled = app
        .world_mut()
        .query::<(&RevoluteJoint, &motor::VelocityMotor)>()
        .iter(app.world())
        .any(|(j, m)| j.body2 == arm && m.enabled);
    if arm_enabled {
        return Err("Detached arm muscles remained active".into());
    }
    let head_joint = app
        .world_mut()
        .query::<(Entity, &RevoluteJoint)>()
        .iter(app.world())
        .find(|(_, j)| j.body1 == head)
        .unwrap()
        .0;
    app.world_mut().despawn(head_joint);
    app.update();
    let mut motors = app
        .world_mut()
        .query::<(&motor::VelocityMotor, &living::MuscleJoint)>();
    if motors
        .iter(app.world())
        .any(|(j, m)| m.owner == owner && j.enabled)
    {
        return Err("Human muscles remained active after brain disconnection".into());
    }
    let unaffected = motors
        .iter(app.world())
        .filter(|(j, m)| m.owner != owner && j.enabled)
        .count();
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
) {
    commands.spawn((
        Floor,
        RigidBody::Static,
        Collider::rectangle(20.0, 0.2),
        Position(Vec2::new(0.0, -2.3)),
        Transform::from_xyz(0.0, -2.3, -1.0),
        Friction::new(0.6),
        Restitution::new(0.05),
        CollisionLayers::ALL,
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

fn check(app: &mut App, expected_bodies: usize, expected_joints: usize) -> Result<(), String> {
    let mut bodies = app.world_mut().query_filtered::<(
        &Name,
        &Position,
        &Rotation,
        &Transform,
        &GlobalTransform,
        &LinearVelocity,
    ), With<Grabbable>>();
    let mut states = HashMap::new();
    let mut max_transform_error = 0.0f32;
    for (name, position, _, transform, global, velocity) in bodies.iter(app.world()) {
        if !position.0.is_finite() || !velocity.0.is_finite() || position.0.y < -2.4 {
            return Err(format!("Invalid simulated body: {name}"));
        }
        let error = position.0.distance(transform.translation.truncate());
        max_transform_error = max_transform_error.max(error);
        if error > 0.05 || transform.translation.distance(global.translation()) > 0.001 {
            return Err(format!(
                "Physics/render transform mismatch on {name}: {error}"
            ));
        }
    }
    let mut poses = app
        .world_mut()
        .query_filtered::<(Entity, &Position, &Rotation), With<Grabbable>>();
    for (entity, position, rotation) in poses.iter(app.world()) {
        states.insert(entity, (position.0, *rotation));
    }
    if states.len() != expected_bodies {
        return Err("Unexpected rigid body count".into());
    }
    let mut joints = app.world_mut().query::<&RevoluteJoint>();
    let mut count = 0;
    let mut max_gap = 0.0f32;
    for joint in joints.iter(app.world()) {
        count += 1;
        let (a, ar) = states[&joint.body1];
        let (b, br) = states[&joint.body2];
        let gap = (a + ar * joint.local_anchor1().unwrap())
            .distance(b + br * joint.local_anchor2().unwrap());
        max_gap = max_gap.max(gap);
        if gap > 0.05 {
            return Err(format!("Joint separation is too large: {gap}"));
        }
    }
    if count != expected_joints {
        return Err("Unexpected joint count".into());
    }
    println!(
        "Headless physics: bodies={}, joints={count}, maximum anchor gap={max_gap:.6} m, maximum render-transform error={max_transform_error:.6} m",
        states.len()
    );
    Ok(())
}
