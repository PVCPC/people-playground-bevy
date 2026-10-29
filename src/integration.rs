//! Rigid-body integration in the recovered engine's arithmetic order.
use avian2d::{
    dynamics::{
        integrator::VelocityIntegrationData,
        solver::solver_body::{SolverBodies, SolverBodyIndex},
    },
    prelude::*,
};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub fn integrate_velocities(
    mut bodies: ResMut<SolverBodies>,
    increments: Query<
        (&SolverBodyIndex, &VelocityIntegrationData),
        (Without<RigidBodyDisabled>, Without<Sleeping>),
    >,
) {
    for (index, increment) in &increments {
        let Some(body) = bodies.get_mut(*index) else {
            continue;
        };
        let axes = body.flags.locked_axes();
        let axis = [
            if axes.is_translation_x_locked() {
                0.0
            } else {
                1.0
            },
            if axes.is_translation_y_locked() {
                0.0
            } else {
                1.0
            },
        ];
        let velocity = [
            body.linear_velocity.x,
            body.linear_velocity.y,
            body.angular_velocity,
        ];
        let result = if body.flags.is_dynamic() {
            // Avian retains force accumulation; Unity damps the resulting velocity increment too.
            damped_velocity(
                velocity,
                [
                    increment.linear_increment.x,
                    increment.linear_increment.y,
                    increment.angular_increment,
                ],
                increment.linear_damping_rhs,
                increment.angular_damping_rhs,
                axis,
            )
        } else {
            [velocity[0] * axis[0], velocity[1] * axis[1], velocity[2]]
        };
        body.linear_velocity = Vec2::new(result[0], result[1]);
        body.angular_velocity = result[2];
    }
}

pub fn integrate_positions(
    mut bodies: ResMut<SolverBodies>,
    settings: Res<crate::game_assets::SimulationSettings>,
    time: Res<Time>,
) {
    for body in bodies.iter_mut() {
        let axes = body.flags.locked_axes();
        let axis = [
            if axes.is_translation_x_locked() {
                0.0
            } else {
                1.0
            },
            if axes.is_translation_y_locked() {
                0.0
            } else {
                1.0
            },
        ];
        let mut position = [
            body.delta_position.x,
            body.delta_position.y,
            body.delta_rotation.as_radians(),
        ];
        let mut velocity = [
            body.linear_velocity.x,
            body.linear_velocity.y,
            body.angular_velocity,
        ];
        advance_position(
            &mut position,
            &mut velocity,
            axis,
            time.delta_secs(),
            settings.max_translation,
            settings.max_rotation_degrees.to_radians(),
        );
        body.delta_position = Vec2::new(position[0], position[1]);
        body.delta_rotation = Rot2::radians(position[2]);
        body.linear_velocity = Vec2::new(velocity[0], velocity[1]);
        body.angular_velocity = velocity[2];
    }
}

pub fn damped_velocity(
    velocity: [f32; 3],
    increment: [f32; 3],
    linear_factor: f32,
    angular_factor: f32,
    axis: [f32; 2],
) -> [f32; 3] {
    [
        (velocity[0] + increment[0]) * linear_factor * axis[0],
        (velocity[1] + increment[1]) * linear_factor * axis[1],
        (increment[2] + velocity[2]) * angular_factor,
    ]
}

pub fn advance_position(
    position: &mut [f32; 3],
    velocity: &mut [f32; 3],
    axis: [f32; 2],
    dt: f32,
    max_translation: f32,
    max_rotation: f32,
) {
    let (tx, ty) = (velocity[0] * dt, velocity[1] * dt);
    let length_squared = ty * ty + tx * tx;
    if max_translation * max_translation < length_squared {
        let scale = max_translation / length_squared.sqrt();
        velocity[0] *= scale;
        velocity[1] *= scale;
    }
    let angle = velocity[2] * dt;
    if max_rotation * max_rotation < angle * angle {
        velocity[2] *= max_rotation / angle.abs();
    }
    position[0] += velocity[0] * axis[0] * dt;
    position[1] += velocity[1] * axis[1] * dt;
    position[2] = velocity[2] * dt + position[2];
}

#[derive(Debug, Clone, Serialize)]
struct ReferenceCase {
    integrate_positions: bool,
    integrate_velocities: bool,
    body_count: u32,
    position_a: [f32; 3],
    position_b: [f32; 3],
    velocity_a: [f32; 3],
    velocity_b: [f32; 3],
    axis_a: [f32; 2],
    axis_b: [f32; 2],
    dt: f32,
    max_translation: f32,
    max_rotation: f32,
    body_type: u32,
    inverse_mass_a: f32,
    inverse_inertia_a: f32,
    force: [f32; 2],
    torque: f32,
    gravity: [f32; 2],
    gravity_scale: f32,
    linear_damping: f32,
    angular_damping: f32,
}

#[derive(Debug, Deserialize, PartialEq)]
struct ReferenceBits {
    position_a_bits: [u32; 3],
    position_b_bits: [u32; 3],
    velocity_a_bits: [u32; 3],
    velocity_b_bits: [u32; 3],
}

pub fn compare_reference(executable: &std::path::Path) -> Result<(), String> {
    let mut seed = 0x7841951u32;
    let mut sample = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        (seed % 20001) as f32 / 100.0 - 100.0
    };
    let mut cases = Vec::new();
    for i in 0..4096 {
        cases.push(ReferenceCase {
            integrate_positions: i % 2 == 0,
            integrate_velocities: i % 2 != 0,
            body_count: if i % 4 < 2 { 2 } else { 4 },
            position_a: [sample(), sample(), sample()],
            position_b: [sample(), sample(), sample()],
            velocity_a: [sample(), sample(), sample()],
            velocity_b: [sample(), sample(), sample()],
            axis_a: [if i % 7 == 0 { 0.0 } else { 1.0 }, 1.0],
            axis_b: [1.0, if i % 11 == 0 { 0.0 } else { 1.0 }],
            dt: [0.02, 0.002, 1.0 / 60.0][i % 3],
            max_translation: [100.0, 0.25, 2.0][i % 3],
            max_rotation: [360.0f32.to_radians(), 0.125, 0.5][i % 3],
            body_type: (i / 2 % 3) as u32,
            inverse_mass_a: sample().abs() / 10.0,
            inverse_inertia_a: sample().abs(),
            force: [sample(), sample()],
            torque: sample(),
            gravity: [sample(), sample()],
            gravity_scale: sample() / 10.0,
            linear_damping: sample().abs(),
            angular_damping: sample().abs(),
        });
    }
    let expected: Vec<ReferenceBits> = crate::motor::run_reference(executable, &cases)?;
    let mut failures = 0;
    for (index, (case, reference)) in cases.iter().zip(expected).enumerate() {
        let mut c = case.clone();
        for (position, velocity, axis) in [
            (&mut c.position_a, &mut c.velocity_a, c.axis_a),
            (&mut c.position_b, &mut c.velocity_b, c.axis_b),
        ] {
            if c.integrate_positions {
                advance_position(
                    position,
                    velocity,
                    axis,
                    c.dt,
                    c.max_translation,
                    c.max_rotation,
                );
            } else if c.body_type == 2 {
                *velocity = damped_velocity(
                    *velocity,
                    [
                        (c.gravity_scale * c.gravity[0] + c.inverse_mass_a * c.force[0]) * c.dt,
                        (c.gravity_scale * c.gravity[1] + c.inverse_mass_a * c.force[1]) * c.dt,
                        c.dt * c.inverse_inertia_a * c.torque,
                    ],
                    1.0 / (c.dt * c.linear_damping + 1.0),
                    1.0 / (c.dt * c.angular_damping + 1.0),
                    axis,
                );
            } else {
                velocity[0] *= axis[0];
                velocity[1] *= axis[1];
            }
        }
        let actual = ReferenceBits {
            position_a_bits: c.position_a.map(f32::to_bits),
            position_b_bits: c.position_b.map(f32::to_bits),
            velocity_a_bits: c.velocity_a.map(f32::to_bits),
            velocity_b_bits: c.velocity_b.map(f32::to_bits),
        };
        if actual != reference {
            failures += 1;
            if failures <= 3 {
                eprintln!(
                    "Native integration mismatch {index}: Rust={actual:?}, original={reference:?}; input={case:?}"
                );
            }
        }
    }
    if failures != 0 {
        return Err(format!(
            "Native body integration: {failures}/{} cases differ",
            cases.len()
        ));
    }
    println!(
        "NATIVE BODY INTEGRATION PASSED: {} velocity/position cases match every output bit, including damping, locked axes, displacement caps and scalar/unrolled paths.",
        cases.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acceleration_is_damped_and_displacement_caps_rescale_velocity() {
        assert_eq!(
            damped_velocity([2.0; 3], [2.0; 3], 0.5, 0.25, [1.0, 0.0]),
            [2.0, 0.0, 1.0]
        );
        let mut position = [0.0; 3];
        let mut velocity = [30.0, 40.0, -100.0];
        advance_position(&mut position, &mut velocity, [1.0; 2], 0.1, 1.0, 0.5);
        assert_eq!(position, [0.6, 0.8, -0.5]);
        assert_eq!(velocity, [6.0, 8.0, -5.0]);
    }
}
