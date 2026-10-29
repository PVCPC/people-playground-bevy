//! Healthy static-pose commands and torque-limited hinge velocity constraints.
use avian2d::{
    dynamics::solver::solver_body::{SolverBodies, SolverBodyIndex},
    prelude::*,
};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Component, Default)]
pub struct VelocityMotor {
    pub enabled: bool,
    pub target_velocity: f32,
    pub max_torque: f32,
    impulse: f32,
}

pub fn solve_velocity_motors(
    mut bodies: ResMut<SolverBodies>,
    frames: Query<
        (&Rotation, &ComputedCenterOfMass, Option<&SolverBodyIndex>),
        Without<RigidBodyDisabled>,
    >,
    mut joints: Query<(&RevoluteJoint, &mut VelocityMotor), Without<JointDisabled>>,
    time: Res<Time>,
) {
    for (_, mut motor) in &mut joints {
        motor.impulse = 0.0;
    }
    // Hinge velocity equations: Box2D 2.4.1 b2_revolute_joint.cpp (see THIRD_PARTY_NOTICES.md).
    // shortcut: separate velocity and Avian position/contact passes; Unity solver parity needs trajectories.
    for _ in 0..16 {
        for (joint, mut motor) in &mut joints {
            if !motor.enabled {
                continue;
            }
            let Ok((rotation_a, center_a, index_a)) = frames.get(joint.body1) else {
                continue;
            };
            let Ok((rotation_b, center_b, index_b)) = frames.get(joint.body2) else {
                continue;
            };
            let index = |entity, candidate: Option<&SolverBodyIndex>| {
                candidate
                    .copied()
                    .filter(|&i| bodies.get_entity(i) == Some(entity))
                    .unwrap_or(SolverBodyIndex::INVALID)
            };
            let a = index(joint.body1, index_a);
            let b = index(joint.body2, index_b);
            if a == b {
                continue;
            }
            let inv_a = bodies
                .get_inertia(a)
                .map_or(0.0, |i| i.effective_inv_angular_inertia());
            let inv_b = bodies
                .get_inertia(b)
                .map_or(0.0, |i| i.effective_inv_angular_inertia());
            let inverse_inertia = inv_a + inv_b;
            if inverse_inertia <= 0.0 {
                continue;
            }
            let relative_speed = bodies.get(b).map_or(0.0, |b| b.angular_velocity)
                - bodies.get(a).map_or(0.0, |a| a.angular_velocity);
            let limit = motor.max_torque * time.delta_secs();
            let previous = motor.impulse;
            motor.impulse = (previous + (motor.target_velocity - relative_speed) / inverse_inertia)
                .clamp(-limit, limit);
            let impulse = motor.impulse - previous;
            if let Some(body) = bodies.get_mut(a) {
                body.angular_velocity -= inv_a * impulse;
            }
            if let Some(body) = bodies.get_mut(b) {
                body.angular_velocity += inv_b * impulse;
            }
            // Co-solve anchor velocities so a neighboring motor sees the transmitted motion.
            let offset = |index, rotation: &Rotation, center: Vec2, anchor: Vec2| {
                let base = Vec2::from_angle(rotation.as_radians()).rotate(anchor - center);
                bodies
                    .get(index)
                    .map_or(base, |body| body.delta_rotation * base)
            };
            let r_a = offset(
                a,
                rotation_a,
                center_a.0,
                joint.local_anchor1().unwrap_or(Vec2::ZERO),
            );
            let r_b = offset(
                b,
                rotation_b,
                center_b.0,
                joint.local_anchor2().unwrap_or(Vec2::ZERO),
            );
            let mass_a = bodies
                .get_inertia(a)
                .map_or(Vec2::ZERO, |i| i.effective_inv_mass());
            let mass_b = bodies
                .get_inertia(b)
                .map_or(Vec2::ZERO, |i| i.effective_inv_mass());
            let velocity_error = bodies
                .get(b)
                .map_or(Vec2::ZERO, |body| body.velocity_at_point(r_b))
                - bodies
                    .get(a)
                    .map_or(Vec2::ZERO, |body| body.velocity_at_point(r_a));
            let k_x = mass_a.x + mass_b.x + inv_a * r_a.y * r_a.y + inv_b * r_b.y * r_b.y;
            let k_y = mass_a.y + mass_b.y + inv_a * r_a.x * r_a.x + inv_b * r_b.x * r_b.x;
            let cross = -inv_a * r_a.x * r_a.y - inv_b * r_b.x * r_b.y;
            let determinant = k_x * k_y - cross * cross;
            if determinant <= 0.0 {
                continue;
            }
            let point_impulse = Vec2::new(
                cross * velocity_error.y - k_y * velocity_error.x,
                cross * velocity_error.x - k_x * velocity_error.y,
            ) / determinant;
            if let Some(body) = bodies.get_mut(a) {
                body.linear_velocity -= mass_a * point_impulse;
                body.angular_velocity -= inv_a * r_a.perp_dot(point_impulse);
            }
            if let Some(body) = bodies.get_mut(b) {
                body.linear_velocity += mass_b * point_impulse;
                body.angular_velocity += inv_b * r_b.perp_dot(point_impulse);
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MotorInput {
    pub current_angle: f32,
    pub reference_angle: f32,
    pub target_angle: f32,
    pub previous_speed: f32,
    pub rigidity: f32,
    pub rigidity_modifier: f32,
    pub global_rigidity: f32,
    pub angle_offset: f32,
    pub mass: f32,
    pub initial_mass: f32,
    pub base_strength: f32,
    pub force_multiplier: f32,
    pub active: bool,
    pub strength_multipliers: Vec<f32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MotorOutput {
    pub speed: f32,
    pub torque: f32,
}

pub fn command(input: &MotorInput) -> MotorOutput {
    let mass_ratio = input.mass / input.initial_mass;
    let force = if input.active {
        input.force_multiplier
    } else {
        1.0
    };
    let mut strength = input.base_strength * force * 0.9 * mass_ratio;
    for multiplier in &input.strength_multipliers {
        strength *= multiplier;
        if strength.abs() <= f32::from_bits(1) {
            break;
        }
    }
    let modifier = if input.active {
        (1.0 + input.rigidity_modifier).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let speed = if input.active {
        let target = input.reference_angle + input.target_angle * input.angle_offset;
        let delta = target - input.current_angle;
        let repeated = (delta - (delta / 360.0).floor() * 360.0).clamp(0.0, 360.0);
        // The original chooses positive 180 degrees at the exact half-turn boundary.
        let angle = if repeated > 180.0 {
            repeated - 360.0
        } else {
            repeated
        };
        let desired =
            angle * input.global_rigidity * input.rigidity * (1.0 + input.rigidity_modifier);
        input.previous_speed + (desired - input.previous_speed) * 0.5
    } else {
        input.previous_speed
    };
    MotorOutput {
        speed,
        torque: strength * modifier,
    }
}

pub fn compare_reference(executable: &std::path::Path) -> Result<(), String> {
    use std::process::{Command, Stdio};
    let mut cases = Vec::new();
    let mut state = 0x1234_5678u32;
    let mut random = || {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        state
    };
    for i in 0..2048 {
        cases.push(MotorInput {
            current_angle: (random() % 1441) as f32 - 720.0,
            reference_angle: (random() % 121) as f32 - 60.0,
            target_angle: (random() % 361) as f32 - 180.0,
            previous_speed: (random() % 401) as f32 - 200.0,
            rigidity: (random() % 40 + 1) as f32 / 8.0,
            rigidity_modifier: [-0.9, 0.0, 0.5][i % 3],
            global_rigidity: [0.5, 1.0, 2.0][i % 3],
            angle_offset: if i % 2 == 0 { 1.0 } else { -1.0 },
            mass: (random() % 64 + 1) as f32 / 16.0,
            initial_mass: (random() % 32 + 1) as f32 / 16.0,
            base_strength: (random() % 160 + 1) as f32 / 8.0,
            force_multiplier: [0.0, 0.7, 1.0, 2.0][i % 4],
            active: i % 7 != 0,
            strength_multipliers: match i % 3 {
                0 => vec![],
                1 => vec![0.5, 1.25],
                _ => vec![0.0, 2.0],
            },
        });
    }
    for target in [-540.0, -180.0, 180.0, 540.0] {
        let mut case = cases[0].clone();
        case.current_angle = 0.0;
        case.reference_angle = 0.0;
        case.target_angle = target;
        case.previous_speed = 0.0;
        case.angle_offset = 1.0;
        case.active = true;
        cases.push(case);
    }
    let mut process = Command::new(executable);
    process
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        process.creation_flags(0x08000000);
    }
    let mut child = process
        .spawn()
        .map_err(|e| format!("Cannot start private reference probe: {e}"))?;
    serde_json::to_writer(child.stdin.take().unwrap(), &cases).map_err(|e| e.to_string())?;
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "Reference probe failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let expected: Vec<MotorOutput> = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Invalid reference output: {e}"))?;
    if expected.len() != cases.len() {
        return Err("Reference probe returned an incomplete case set".into());
    }
    let mut mismatches = 0;
    for (index, (input, reference)) in cases.iter().zip(&expected).enumerate() {
        let actual = command(input);
        if actual.speed.to_bits() != reference.speed.to_bits()
            || actual.torque.to_bits() != reference.torque.to_bits()
        {
            mismatches += 1;
            if mismatches <= 5 {
                eprintln!(
                    "Reference mismatch {index}: Rust={actual:?}, C#={reference:?}, input={input:?}"
                );
            }
        }
    }
    if mismatches != 0 {
        return Err(format!(
            "Motor reference comparison failed: {mismatches}/{} cases differ",
            cases.len()
        ));
    }
    println!(
        "MOTOR REFERENCE PASSED: {} static healthy-limb cases match the extracted C# methods bit-for-bit (speed and torque). Native physics, animation, injury and random behavior are outside this check.",
        cases.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use avian2d::dynamics::{
        integrator::IntegrationSystems, solver::schedule::SubstepSolverSystems,
    };
    use bevy::{mesh::MeshPlugin, time::TimeUpdateStrategy};
    use std::time::Duration;
    fn input() -> MotorInput {
        MotorInput {
            current_angle: 0.0,
            reference_angle: 0.0,
            target_angle: 180.0,
            previous_speed: 0.0,
            rigidity: 4.0,
            rigidity_modifier: 0.0,
            global_rigidity: 1.0,
            angle_offset: 1.0,
            mass: 1.0,
            initial_mass: 1.0,
            base_strength: 10.0,
            force_multiplier: 1.0,
            active: true,
            strength_multipliers: vec![],
        }
    }
    #[test]
    fn original_half_turn_rule_selects_positive_rotation() {
        assert_eq!(command(&input()).speed, 360.0);
        let mut negative = input();
        negative.target_angle = -180.0;
        assert_eq!(command(&negative).speed, 360.0);
    }
    #[test]
    fn inactive_pose_keeps_velocity_and_ignores_pose_force_and_softness() {
        let mut inactive = input();
        inactive.active = false;
        inactive.previous_speed = 7.0;
        inactive.force_multiplier = 2.0;
        inactive.rigidity_modifier = -0.9;
        let output = command(&inactive);
        assert_eq!(output.speed, 7.0);
        assert_eq!(output.torque, 9.0);
    }

    fn physics_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            ImagePlugin::default(),
            MeshPlugin,
            TransformPlugin,
            PhysicsPlugins::default(),
        ))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            20,
        )))
        .insert_resource(Time::<Fixed>::from_hz(50.0))
        .insert_resource(SubstepCount(10))
        .insert_resource(Gravity(Vec2::ZERO))
        .add_systems(
            SubstepSchedule,
            solve_velocity_motors
                .after(SubstepSolverSystems::SolveConstraints)
                .before(IntegrationSystems::Position),
        );
        app
    }

    #[test]
    fn velocity_commands_settle_an_isolated_hinge_without_a_spring() {
        let mut app = physics_app();
        let parent = app
            .world_mut()
            .spawn((RigidBody::Static, Position(Vec2::ZERO)))
            .id();
        let child = app
            .world_mut()
            .spawn((
                RigidBody::Dynamic,
                SleepingDisabled,
                Mass(0.5),
                AngularInertia(0.01),
                Position(Vec2::new(-0.1 * 0.7f32.sin(), 0.1 * 0.7f32.cos())),
                Rotation::radians(0.7),
            ))
            .id();
        let hinge = app
            .world_mut()
            .spawn((
                RevoluteJoint::new(child, parent)
                    .with_local_anchor1(Vec2::new(0.0, -0.1))
                    .with_local_anchor2(Vec2::ZERO),
                VelocityMotor::default(),
            ))
            .id();
        app.finish();
        app.cleanup();
        let mut previous = 0.0;
        for _ in 0..250 {
            let mut case = input();
            case.current_angle = app
                .world()
                .get::<Rotation>(child)
                .unwrap()
                .as_radians()
                .to_degrees();
            case.target_angle = 0.0;
            case.rigidity = 2.0;
            case.previous_speed = previous;
            let output = command(&case);
            previous = output.speed;
            let mut motor = app.world_mut().get_mut::<VelocityMotor>(hinge).unwrap();
            *motor = VelocityMotor {
                enabled: true,
                target_velocity: -output.speed.to_radians(),
                max_torque: output.torque,
                ..default()
            };
            app.update();
        }
        let angle = app.world().get::<Rotation>(child).unwrap().as_radians();
        let speed = app.world().get::<AngularVelocity>(child).unwrap().0;
        assert!(
            angle.abs() < 0.01 && speed.abs() < 0.02,
            "angle={angle}, speed={speed}"
        );
    }

    #[test]
    fn motor_respects_total_torque_budget_and_conserves_angular_momentum() {
        let mut app = physics_app();
        let a = app
            .world_mut()
            .spawn((
                RigidBody::Dynamic,
                SleepingDisabled,
                Mass(1.0),
                AngularInertia(2.0),
                Position(Vec2::ZERO),
            ))
            .id();
        let b = app
            .world_mut()
            .spawn((
                RigidBody::Dynamic,
                SleepingDisabled,
                Mass(1.0),
                AngularInertia(3.0),
                Position(Vec2::ZERO),
            ))
            .id();
        let hinge = app
            .world_mut()
            .spawn((
                RevoluteJoint::new(a, b),
                VelocityMotor {
                    enabled: true,
                    target_velocity: 10.0,
                    max_torque: 0.5,
                    ..default()
                },
            ))
            .id();
        app.finish();
        app.cleanup();
        app.update();
        app.update();
        let spin_a = app.world().get::<AngularVelocity>(a).unwrap().0;
        let spin_b = app.world().get::<AngularVelocity>(b).unwrap().0;
        assert!(
            (spin_a + 0.005).abs() < 0.00001,
            "torque budget: spin_a={spin_a}"
        );
        assert!(
            (2.0 * spin_a + 3.0 * spin_b).abs() < 0.00001,
            "angular momentum changed"
        );
        app.world_mut().entity_mut(hinge).insert(JointDisabled);
        app.world_mut().get_mut::<AngularVelocity>(a).unwrap().0 = 0.0;
        app.world_mut().get_mut::<AngularVelocity>(b).unwrap().0 = 0.0;
        app.update();
        assert_eq!(app.world().get::<AngularVelocity>(a).unwrap().0, 0.0);
        assert_eq!(app.world().get::<AngularVelocity>(b).unwrap().0, 0.0);
    }
}
