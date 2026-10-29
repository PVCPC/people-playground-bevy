use super::*;
use bevy::{mesh::MeshPlugin, time::TimeUpdateStrategy};
use std::time::Duration;

pub fn test(rig: HumanRig, cache: PathBuf) -> Result<(), String> {
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
        PhysicsPlugins::default(),
    ))
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        20,
    )))
    .insert_resource(Time::<Fixed>::from_hz(50.0))
    .insert_resource(SubstepCount(10))
    .insert_resource(Gravity(Vec2::new(0.0, -9.81)))
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
    .add_systems(FixedUpdate, drag_body);
    app.finish();
    app.cleanup();
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
