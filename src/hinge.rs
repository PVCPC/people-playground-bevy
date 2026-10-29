//! Prepared hinge velocity constraints in the installed engine's operation order.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize)]
pub struct VelocityConstraint {
    pub velocity_a: [f32; 3],
    pub velocity_b: [f32; 3],
    pub inverse_mass_a: f32,
    pub inverse_mass_b: f32,
    pub inverse_inertia_a: f32,
    pub inverse_inertia_b: f32,
    pub anchor_a: [f32; 2],
    pub anchor_b: [f32; 2],
    pub matrix: [f32; 9],
    pub motor_mass: f32,
    pub impulse: [f32; 3],
    pub motor_impulse: f32,
    pub axis_a: [f32; 2],
    pub axis_b: [f32; 2],
    pub motor_enabled: bool,
    pub limit_enabled: bool,
    pub limit_state: u32,
    pub target_velocity: f32,
    pub max_torque: f32,
    pub dt: f32,
}

fn solve22(m: &[f32; 9], b: [f32; 2]) -> [f32; 2] {
    let mut determinant = m[4] * m[0] - m[1] * m[3];
    if determinant != 0.0 {
        determinant = 1.0 / determinant;
    }
    [
        (b[0] * m[4] - b[1] * m[3]) * determinant,
        (b[1] * m[0] - b[0] * m[1]) * determinant,
    ]
}

fn solve33(m: &[f32; 9], b: [f32; 3]) -> [f32; 3] {
    let mut determinant = m[0] * (m[8] * m[4] - m[7] * m[5])
        + m[1] * (m[6] * m[5] - m[3] * m[8])
        + m[2] * (m[3] * m[7] - m[6] * m[4]);
    if determinant != 0.0 {
        determinant = 1.0 / determinant;
    }
    [
        (b[0] * (m[8] * m[4] - m[7] * m[5])
            + b[1] * (m[6] * m[5] - m[3] * m[8])
            + b[2] * (m[3] * m[7] - m[6] * m[4]))
            * determinant,
        (m[0] * (m[8] * b[1] - m[7] * b[2])
            + m[1] * (m[6] * b[2] - m[8] * b[0])
            + m[2] * (m[7] * b[0] - m[6] * b[1]))
            * determinant,
        (m[0] * (m[4] * b[2] - m[5] * b[1])
            + m[1] * (m[5] * b[0] - m[3] * b[2])
            + m[2] * (m[3] * b[1] - m[4] * b[0]))
            * determinant,
    ]
}

impl VelocityConstraint {
    pub fn prepare_matrix(&mut self) {
        let [ax, ay] = self.anchor_a;
        let [bx, by] = self.anchor_b;
        let (ma, mb, ia, ib) = (
            self.inverse_mass_a,
            self.inverse_mass_b,
            self.inverse_inertia_a,
            self.inverse_inertia_b,
        );
        let xx = ay * ay * ia + mb + ma + by * by * ib;
        let xy = -(ay * ax * ia) - by * bx * ib;
        let xz = -(ay * ia) - by * ib;
        let yy = ax * ax * ia + mb + ma + bx * bx * ib;
        let yz = bx * ib + ax * ia;
        self.matrix = [xx, xy, xz, xy, yy, yz, xz, yz, ia + ib];
        self.motor_mass = if ia + ib > 0.0 { 1.0 / (ia + ib) } else { 0.0 };
    }

    pub fn solve(&mut self) {
        let mut wa = self.velocity_a[2];
        let mut wb = self.velocity_b[2];
        let fixed_rotation = self.inverse_inertia_a + self.inverse_inertia_b == 0.0;
        if self.motor_enabled && self.limit_state != 3 && !fixed_rotation {
            let old = self.motor_impulse;
            let maximum = self.max_torque * self.dt;
            let mut value = -((wb - wa - self.target_velocity) * self.motor_mass) + old;
            if maximum <= value {
                value = maximum;
            }
            self.motor_impulse = if -maximum <= value { value } else { -maximum };
            let delta = self.motor_impulse - old;
            wb += delta * self.inverse_inertia_b;
            wa -= delta * self.inverse_inertia_a;
        }
        let cdot = [
            ((self.velocity_b[0] + -(wb * self.anchor_b[1])) - self.velocity_a[0])
                - -(wa * self.anchor_a[1]),
            ((wb * self.anchor_b[0] + self.velocity_b[1]) - self.velocity_a[1])
                - wa * self.anchor_a[0],
        ];
        let coupled = self.limit_enabled && self.limit_state != 0 && !fixed_rotation;
        let delta;
        if coupled {
            let full = solve33(&self.matrix, [cdot[0], cdot[1], wb - wa]);
            let mut correction = [-full[0], -full[1], -full[2]];
            let next_z = self.impulse[2] + correction[2];
            if (self.limit_state == 1 && next_z < 0.0) || (self.limit_state == 2 && next_z > 0.0) {
                let reduced = solve22(
                    &self.matrix,
                    [
                        -cdot[0] + self.matrix[6] * self.impulse[2],
                        -cdot[1] + self.matrix[7] * self.impulse[2],
                    ],
                );
                correction = [reduced[0], reduced[1], -self.impulse[2]];
                self.impulse[0] += correction[0];
                self.impulse[1] += correction[1];
                self.impulse[2] = 0.0;
            } else {
                for (accumulated, change) in self.impulse.iter_mut().zip(correction) {
                    *accumulated += change;
                }
            }
            delta = correction;
        } else {
            let point = solve22(&self.matrix, [-cdot[0], -cdot[1]]);
            self.impulse[0] += point[0];
            self.impulse[1] += point[1];
            delta = [point[0], point[1], 0.0];
        }
        let mut angular_a = delta[1] * self.anchor_a[0] - delta[0] * self.anchor_a[1];
        let mut angular_b = delta[1] * self.anchor_b[0] - delta[0] * self.anchor_b[1];
        if coupled {
            angular_a += delta[2];
            angular_b += delta[2];
        }
        self.velocity_a = [
            (self.velocity_a[0] - delta[0] * self.inverse_mass_a) * self.axis_a[0],
            (self.velocity_a[1] - delta[1] * self.inverse_mass_a) * self.axis_a[1],
            wa - angular_a * self.inverse_inertia_a,
        ];
        self.velocity_b = [
            (self.velocity_b[0] + delta[0] * self.inverse_mass_b) * self.axis_b[0],
            (self.velocity_b[1] + delta[1] * self.inverse_mass_b) * self.axis_b[1],
            angular_b * self.inverse_inertia_b + wb,
        ];
    }
}

pub fn limit_state(angle: f32, lower: f32, upper: f32) -> u32 {
    if (upper - lower).abs() < f32::from_bits(0x3d8efa36) {
        3
    } else if angle <= lower {
        1
    } else if upper <= angle {
        2
    } else {
        0
    }
}

#[derive(Debug, Deserialize, PartialEq)]
struct NativeBits {
    velocity_a_bits: [u32; 3],
    velocity_b_bits: [u32; 3],
    impulse_bits: [u32; 3],
    motor_impulse_bits: u32,
}

impl From<&VelocityConstraint> for NativeBits {
    fn from(value: &VelocityConstraint) -> Self {
        Self {
            velocity_a_bits: value.velocity_a.map(f32::to_bits),
            velocity_b_bits: value.velocity_b.map(f32::to_bits),
            impulse_bits: value.impulse.map(f32::to_bits),
            motor_impulse_bits: value.motor_impulse.to_bits(),
        }
    }
}

pub fn compare_reference(executable: &std::path::Path) -> Result<(), String> {
    let mut seed = 0x12345678u32;
    let mut random = || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        seed
    };
    let mut cases = Vec::new();
    for i in 0..4096 {
        let ma = (random() % 16 + 1) as f32 / 8.0;
        let mb = (random() % 16 + 1) as f32 / 8.0;
        let ia = if i % 7 == 0 {
            0.0
        } else {
            (random() % 32 + 1) as f32 / 8.0
        };
        let ib = if i % 11 == 0 {
            0.0
        } else {
            (random() % 32 + 1) as f32 / 8.0
        };
        let ra = [
            (random() % 17) as f32 / 8.0 - 1.0,
            (random() % 17) as f32 / 8.0 - 1.0,
        ];
        let rb = [
            (random() % 17) as f32 / 8.0 - 1.0,
            (random() % 17) as f32 / 8.0 - 1.0,
        ];
        let xx = ra[1] * ra[1] * ia + mb + ma + rb[1] * rb[1] * ib;
        let xy = -(ra[1] * ra[0] * ia) - rb[1] * rb[0] * ib;
        let xz = -(ra[1] * ia) - rb[1] * ib;
        let yy = ra[0] * ra[0] * ia + mb + ma + rb[0] * rb[0] * ib;
        let yz = rb[0] * ib + ra[0] * ia;
        cases.push(VelocityConstraint {
            velocity_a: [
                (random() % 129) as f32 / 8.0 - 8.0,
                (random() % 129) as f32 / 8.0 - 8.0,
                (random() % 129) as f32 / 8.0 - 8.0,
            ],
            velocity_b: [
                (random() % 129) as f32 / 8.0 - 8.0,
                (random() % 129) as f32 / 8.0 - 8.0,
                (random() % 129) as f32 / 8.0 - 8.0,
            ],
            inverse_mass_a: ma,
            inverse_mass_b: mb,
            inverse_inertia_a: ia,
            inverse_inertia_b: ib,
            anchor_a: ra,
            anchor_b: rb,
            matrix: [xx, xy, xz, xy, yy, yz, xz, yz, ia + ib],
            motor_mass: if ia + ib > 0.0 { 1.0 / (ia + ib) } else { 0.0 },
            impulse: [
                0.125,
                -0.25,
                match i % 4 {
                    1 => 0.5,
                    2 => -0.5,
                    _ => 0.0,
                },
            ],
            motor_impulse: 0.125,
            axis_a: [1.0, if i % 13 == 0 { 0.0 } else { 1.0 }],
            axis_b: [1.0, 1.0],
            motor_enabled: i % 3 != 0,
            limit_enabled: i % 5 != 0,
            limit_state: i % 4,
            target_velocity: (random() % 65) as f32 / 8.0 - 4.0,
            max_torque: (i % 32) as f32,
            dt: [0.02, 0.005, 1.0 / 120.0][(i % 3) as usize],
        });
    }
    let expected: Vec<NativeBits> = crate::motor::run_reference(executable, &cases)?;
    let mut mismatches = 0;
    for (index, (initial, reference)) in cases.iter().zip(&expected).enumerate() {
        let mut actual = initial.clone();
        actual.solve();
        let bits = NativeBits::from(&actual);
        if bits != *reference {
            mismatches += 1;
            if mismatches <= 3 {
                eprintln!(
                    "Native hinge mismatch {index}: Rust={bits:?}, original={reference:?}; input={initial:?}"
                );
            }
        }
    }
    if mismatches != 0 {
        return Err(format!(
            "Native hinge comparison failed: {mismatches}/{} cases differ",
            cases.len()
        ));
    }
    println!(
        "NATIVE HINGE REFERENCE PASSED: {} prepared velocity-constraint iterations match the installed engine bit-for-bit. Integration, contacts, preparation and position solving are outside this check.",
        cases.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn limit_states_preserve_the_native_slop_boundary() {
        assert_eq!(limit_state(0.0, -1.0, 1.0), 0);
        assert_eq!(limit_state(-1.0, -1.0, 1.0), 1);
        assert_eq!(limit_state(1.0, -1.0, 1.0), 2);
        assert_eq!(limit_state(0.0, 0.0, f32::from_bits(0x3d8efa35)), 3);
        assert_eq!(limit_state(0.0, 0.0, f32::from_bits(0x3d8efa36)), 1);
    }
    #[test]
    fn matrix_solvers_handle_known_and_singular_systems() {
        let matrix = [4.0, 1.0, 0.0, 1.0, 3.0, 1.0, 0.0, 1.0, 2.0];
        for (actual, expected) in solve33(&matrix, [6.0, 10.0, 8.0])
            .into_iter()
            .zip([1.0, 2.0, 3.0])
        {
            assert!((actual - expected).abs() < 0.00001);
        }
        assert_eq!(solve22(&[0.0; 9], [1.0, 2.0]), [0.0, 0.0]);
    }
    #[test]
    fn equal_limits_disable_the_motor() {
        let mut state = VelocityConstraint {
            motor_enabled: true,
            limit_enabled: true,
            limit_state: 3,
            inverse_inertia_a: 1.0,
            inverse_inertia_b: 1.0,
            motor_mass: 0.5,
            max_torque: 100.0,
            target_velocity: 10.0,
            dt: 0.02,
            axis_a: [1.0; 2],
            axis_b: [1.0; 2],
            matrix: [2.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 2.0],
            ..Default::default()
        };
        state.solve();
        assert_eq!(state.motor_impulse, 0.0);
        assert_eq!(state.velocity_a, [0.0; 3]);
        assert_eq!(state.velocity_b, [0.0; 3]);
    }
}
