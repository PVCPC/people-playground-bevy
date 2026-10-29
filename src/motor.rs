//! Healthy static-pose commands and torque-limited hinge velocity constraints.
use avian2d::{
    dynamics::{
        integrator::IntegrationSystems,
        solver::{
            schedule::SubstepSolverSystems,
            solver_body::{SolverBodies, SolverBodyIndex},
            xpbd::XpbdSolverPlugin,
        },
    },
    prelude::*,
};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub struct PortPhysicsPlugin;

impl Plugin for PortPhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<crate::game_assets::SimulationSettings>()
            .add_plugins(
                PhysicsPlugins::default()
                    .build()
                    .disable::<XpbdSolverPlugin>(),
            )
            .add_systems(
                SubstepSchedule,
                solve_velocity_motors
                    .after(SubstepSolverSystems::SolveConstraints)
                    .before(IntegrationSystems::Position),
            )
            .add_systems(
                SubstepSchedule,
                solve_joint_positions
                    .after(SubstepSolverSystems::Relax)
                    .before(SubstepSolverSystems::Damping),
            );
    }
}

fn wrap_angle(angle: f32) -> f32 {
    (angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

#[derive(Component, Default)]
pub struct VelocityMotor {
    pub enabled: bool,
    pub target_velocity: f32,
    pub max_torque: f32,
    pub reference_angle: f32,
    constraint: crate::hinge::VelocityConstraint,
    prepared: Option<(SolverBodyIndex, SolverBodyIndex)>,
    previous_angle: Option<f32>,
    angle: f32,
    previous_inverse_dt: f32,
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
    settings: Res<crate::game_assets::SimulationSettings>,
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
        let delta_a = bodies.get(a).map_or(0.0, |b| b.delta_rotation.as_radians());
        let delta_b = bodies.get(b).map_or(0.0, |b| b.delta_rotation.as_radians());
        let wrapped =
            wrap_angle(rotation_b.as_radians() - rotation_a.as_radians() + delta_b - delta_a);
        motor.angle = if let Some(previous) = motor.previous_angle {
            motor.angle + wrap_angle(wrapped - previous)
        } else {
            wrapped
        };
        motor.previous_angle = Some(wrapped);
        let history = (
            motor.constraint.impulse,
            motor.constraint.motor_impulse,
            motor.constraint.limit_state,
        );
        let velocity = |index| {
            bodies.get(index).map_or([0.0; 3], |body| {
                [
                    body.linear_velocity.x,
                    body.linear_velocity.y,
                    body.angular_velocity,
                ]
            })
        };
        motor.constraint = crate::hinge::VelocityConstraint {
            velocity_a: velocity(a),
            velocity_b: velocity(b),
            impulse: history.0,
            motor_impulse: history.1,
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
            limit_state: history.2,
            target_velocity: motor.target_velocity,
            max_torque: motor.max_torque,
            dt: time.delta_secs(),
            ..default()
        };
        let dt_ratio = time.delta_secs() * motor.previous_inverse_dt;
        let reference = motor.reference_angle;
        let angle = motor.angle - reference;
        motor.constraint.initialize(
            angle,
            joint
                .angle_limit
                .map(|limit| [limit.min - reference, limit.max - reference]),
            dt_ratio,
            true,
        );
        motor.previous_inverse_dt = if time.delta_secs() > 0.0 {
            1.0 / time.delta_secs()
        } else {
            0.0
        };
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
        motor.prepared = Some((a, b));
    }
    // shortcut: Avian contacts and substeps remain; native full-frame trajectories are unverified.
    for _ in 0..settings.velocity_iterations {
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

fn solve_joint_positions(
    mut bodies: ResMut<SolverBodies>,
    frames: Query<(&Position, &Rotation, &ComputedCenterOfMass), Without<RigidBodyDisabled>>,
    joints: Query<(&RevoluteJoint, &VelocityMotor), Without<JointDisabled>>,
    settings: Res<crate::game_assets::SimulationSettings>,
) {
    for _ in 0..settings.position_iterations {
        let mut solved = true;
        for (joint, motor) in &joints {
            let Some((a, b)) = motor.prepared else {
                continue;
            };
            let Ok((pos_a, rot_a, center_a)) = frames.get(joint.body1) else {
                continue;
            };
            let Ok((pos_b, rot_b, center_b)) = frames.get(joint.body2) else {
                continue;
            };
            let position = |index, pos: &Position, rot: &Rotation, center: Vec2| {
                let body = bodies.get(index);
                let com = pos.0 + *rot * center + body.map_or(Vec2::ZERO, |b| b.delta_position);
                [
                    com.x,
                    com.y,
                    rot.as_radians() + body.map_or(0.0, |b| b.delta_rotation.as_radians()),
                ]
            };
            let position_a = position(a, pos_a, rot_a, center_a.0);
            let mut position_b = position(b, pos_b, rot_b, center_b.0);
            // Preserve winding across Avian's normalized rotation representation.
            let wrapped = wrap_angle(position_b[2] - position_a[2]);
            position_b[2] = position_a[2]
                + motor.angle
                + wrap_angle(wrapped - motor.previous_angle.unwrap_or(wrapped));
            let velocity = &motor.constraint;
            let mut constraint = crate::hinge::PositionConstraint {
                position_a,
                position_b,
                inverse_mass_a: velocity.inverse_mass_a,
                inverse_mass_b: velocity.inverse_mass_b,
                inverse_inertia_a: velocity.inverse_inertia_a,
                inverse_inertia_b: velocity.inverse_inertia_b,
                anchor_a: joint.local_anchor1().unwrap_or(Vec2::ZERO).to_array(),
                anchor_b: joint.local_anchor2().unwrap_or(Vec2::ZERO).to_array(),
                local_center_a: center_a.0.to_array(),
                local_center_b: center_b.0.to_array(),
                axis_a: velocity.axis_a,
                axis_b: velocity.axis_b,
                motor_mass: velocity.motor_mass,
                reference_angle: motor.reference_angle,
                limits: joint.angle_limit.map_or([0.0; 2], |limit| {
                    [
                        limit.min - motor.reference_angle,
                        limit.max - motor.reference_angle,
                    ]
                }),
                limit_enabled: velocity.limit_enabled,
                limit_state: velocity.limit_state,
                max_angular_correction: settings.max_angular_correction_degrees.to_radians(),
            };
            solved &= constraint.solve();
            for (index, old, new, rotation) in [
                (a, position_a, constraint.position_a, rot_a),
                (b, position_b, constraint.position_b, rot_b),
            ] {
                if let Some(body) = bodies.get_mut(index) {
                    body.delta_position += Vec2::new(new[0] - old[0], new[1] - old[1]);
                    body.delta_rotation = Rot2::radians(new[2] - rotation.as_radians());
                }
            }
        }
        if solved {
            break;
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
            PortPhysicsPlugin,
        ))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            20,
        )))
        .insert_resource(Time::<Fixed>::from_hz(50.0))
        .insert_resource(SubstepCount(10))
        .insert_resource(Gravity(Vec2::ZERO));
        app
    }

    #[test]
    fn position_corrections_align_offset_centers_without_adding_velocity() {
        let mut app = physics_app();
        app.insert_resource(SubstepCount(1));
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
                AngularInertia(0.1),
                CenterOfMass(Vec2::new(0.2, -0.1)),
                Position(Vec2::new(0.8, 0.3)),
                Rotation::radians(0.7),
            ))
            .id();
        app.world_mut().spawn((
            RevoluteJoint::new(child, parent)
                .with_local_anchor1(Vec2::new(0.0, -0.3))
                .with_local_anchor2(Vec2::ZERO)
                .with_angle_limits(0.0, 0.0),
            VelocityMotor::default(),
        ));
        app.finish();
        app.cleanup();
        for _ in 0..10 {
            app.update();
        }
        let position = app.world().get::<Position>(child).unwrap().0;
        let rotation = *app.world().get::<Rotation>(child).unwrap();
        let anchor = position + rotation * Vec2::new(0.0, -0.3);
        assert!(anchor.length() < 0.005, "anchor={anchor}");
        assert!(rotation.as_radians().abs() < 0.001);
        assert_eq!(
            app.world().get::<LinearVelocity>(child).unwrap().0,
            Vec2::ZERO
        );
        assert_eq!(app.world().get::<AngularVelocity>(child).unwrap().0, 0.0);
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
        for target in [0.0f32, 90.0] {
            for _ in 0..250 {
                let mut case = input();
                let attached = app.world().get::<Rotation>(child).unwrap().as_radians();
                case.current_angle = crate::hinge::joint_angle(attached, 0.0, 0.0).to_degrees();
                case.target_angle = target;
                case.rigidity = 2.0;
                case.previous_speed = previous;
                let output = command(&case);
                previous = output.speed;
                let mut motor = app.world_mut().get_mut::<VelocityMotor>(hinge).unwrap();
                motor.enabled = true;
                motor.target_velocity = output.speed.to_radians();
                motor.max_torque = output.torque;
                app.update();
            }
            let angle = app.world().get::<Rotation>(child).unwrap().as_radians();
            let speed = app.world().get::<AngularVelocity>(child).unwrap().0;
            assert!(
                (angle + target.to_radians()).abs() < 0.01 && speed.abs() < 0.02,
                "angle={angle}, speed={speed}"
            );
        }
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
