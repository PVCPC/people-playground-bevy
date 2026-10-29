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
    constraint: crate::hinge::VelocityConstraint,
    prepared: Option<(SolverBodyIndex, SolverBodyIndex)>,
    previous_angle: Option<f32>,
    angle: f32,
}

pub fn solve_velocity_motors(
    mut bodies: ResMut<SolverBodies>,
    frames: Query<
        (
            &Rotation,
            &ComputedCenterOfMass,
            &ComputedMass,
            Option<&SolverBodyIndex>,
        ),
        Without<RigidBodyDisabled>,
    >,
    mut joints: Query<(&RevoluteJoint, &mut VelocityMotor), Without<JointDisabled>>,
    time: Res<Time>,
) {
    for (joint, mut motor) in &mut joints {
        motor.prepared = None;
        let Ok((rotation_a, center_a, mass_a, index_a)) = frames.get(joint.body1) else {
            continue;
        };
        let Ok((rotation_b, center_b, mass_b, index_b)) = frames.get(joint.body2) else {
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
        let axis = |index| {
            let locks = bodies
                .get_inertia(index)
                .map_or(LockedAxes::new(), |i| i.flags().locked_axes());
            [
                if locks.is_translation_x_locked() {
                    0.0
                } else {
                    1.0
                },
                if locks.is_translation_y_locked() {
                    0.0
                } else {
                    1.0
                },
            ]
        };
        let wrap = |angle: f32| {
            (angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
        };
        let delta_a = bodies.get(a).map_or(0.0, |b| b.delta_rotation.as_radians());
        let delta_b = bodies.get(b).map_or(0.0, |b| b.delta_rotation.as_radians());
        let wrapped = wrap(rotation_b.as_radians() - rotation_a.as_radians() + delta_b - delta_a);
        motor.angle = if let Some(previous) = motor.previous_angle {
            motor.angle + wrap(wrapped - previous)
        } else {
            wrapped
        };
        motor.previous_angle = Some(wrapped);
        let state = joint.angle_limit.map_or(0, |limit| {
            if inv_a + inv_b == 0.0 {
                0
            } else {
                crate::hinge::limit_state(motor.angle, limit.min, limit.max)
            }
        });
        motor.constraint = crate::hinge::VelocityConstraint {
            inverse_mass_a: if bodies.get(a).is_some_and(|b| b.flags.is_dynamic()) {
                mass_a.inverse()
            } else {
                0.0
            },
            inverse_mass_b: if bodies.get(b).is_some_and(|b| b.flags.is_dynamic()) {
                mass_b.inverse()
            } else {
                0.0
            },
            inverse_inertia_a: inv_a,
            inverse_inertia_b: inv_b,
            anchor_a: r_a.to_array(),
            anchor_b: r_b.to_array(),
            axis_a: axis(a),
            axis_b: axis(b),
            motor_enabled: motor.enabled,
            limit_enabled: joint.angle_limit.is_some(),
            limit_state: state,
            target_velocity: motor.target_velocity,
            max_torque: motor.max_torque,
            dt: time.delta_secs(),
            ..default()
        };
        motor.constraint.prepare_matrix();
        motor.prepared = Some((a, b));
    }
    // shortcut: Avian contacts/positions and substeps remain; native full-frame trajectories are unverified.
    for _ in 0..16 {
        for (_, mut motor) in &mut joints {
            let Some((a, b)) = motor.prepared else {
                continue;
            };
            let velocity = |index| {
                bodies.get(index).map_or([0.0; 3], |body| {
                    [
                        body.linear_velocity.x,
                        body.linear_velocity.y,
                        body.angular_velocity,
                    ]
                })
            };
            motor.constraint.velocity_a = velocity(a);
            motor.constraint.velocity_b = velocity(b);
            motor.constraint.solve();
            if let Some(body) = bodies.get_mut(a) {
                let [x, y, w] = motor.constraint.velocity_a;
                body.linear_velocity = Vec2::new(x, y);
                body.angular_velocity = w;
            }
            if let Some(body) = bodies.get_mut(b) {
                let [x, y, w] = motor.constraint.velocity_b;
                body.linear_velocity = Vec2::new(x, y);
                body.angular_velocity = w;
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

impl MotorOutput {
    pub fn clamp_native_properties(mut self) -> Self {
        // Unity's native HingeJoint2D setter maps NaN to each property's lower bound.
        self.speed = if self.speed.is_nan() {
            -1_000_000.0
        } else {
            self.speed.clamp(-1_000_000.0, 1_000_000.0)
        };
        self.torque = if self.torque.is_nan() {
            0.0
        } else {
            self.torque.clamp(0.0, 1_000_000.0)
        };
        self
    }
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
    let expected: Vec<MotorOutput> = run_reference(executable, &cases)?;
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

pub fn run_reference<I: Serialize, O: serde::de::DeserializeOwned>(
    executable: &std::path::Path,
    cases: &[I],
) -> Result<Vec<O>, String> {
    use std::process::{Command, Stdio};
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
    serde_json::to_writer(child.stdin.take().unwrap(), cases).map_err(|e| e.to_string())?;
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "Reference probe failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let expected: Vec<O> = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Invalid reference output: {e}"))?;
    if expected.len() != cases.len() {
        return Err("Reference probe returned an incomplete case set".into());
    }
    Ok(expected)
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
    fn native_setter_clamps_motor_fields_before_the_next_command() {
        for (speed, torque, expected_speed, expected_torque) in [
            (2_000_000.0, 2_000_000.0, 1_000_000.0, 1_000_000.0),
            (-2_000_000.0, -1.0, -1_000_000.0, 0.0),
            (f32::NAN, f32::NAN, -1_000_000.0, 0.0),
            (f32::INFINITY, f32::INFINITY, 1_000_000.0, 1_000_000.0),
            (7.0, 9.0, 7.0, 9.0),
        ] {
            let result = MotorOutput { speed, torque }.clamp_native_properties();
            assert_eq!(result.speed, expected_speed);
            assert_eq!(result.torque, expected_torque);
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
