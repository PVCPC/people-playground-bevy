//! Prepared hinge velocity constraints in the installed engine's operation order.
use serde::{Deserialize, Serialize};

pub fn joint_angle(attached: f32, connected: f32, reference: f32) -> f32 {
    (connected - attached) - reference
}

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

#[derive(Clone, Debug, Default, Serialize)]
pub struct PositionConstraint {
    pub position_a: [f32; 3],
    pub position_b: [f32; 3],
    pub inverse_mass_a: f32,
    pub inverse_mass_b: f32,
    pub inverse_inertia_a: f32,
    pub inverse_inertia_b: f32,
    pub anchor_a: [f32; 2],
    pub anchor_b: [f32; 2],
    pub local_center_a: [f32; 2],
    pub local_center_b: [f32; 2],
    pub axis_a: [f32; 2],
    pub axis_b: [f32; 2],
    pub motor_mass: f32,
    pub reference_angle: f32,
    pub limits: [f32; 2],
    pub limit_enabled: bool,
    pub limit_state: u32,
    pub max_angular_correction: f32,
}

fn native_clamp(mut value: f32, minimum: f32, maximum: f32) -> f32 {
    if maximum <= value {
        value = maximum;
    }
    if minimum <= value { value } else { minimum }
}

impl PositionConstraint {
    pub fn solve(&mut self) -> bool {
        let (ma, mb, ia, ib) = (
            self.inverse_mass_a,
            self.inverse_mass_b,
            self.inverse_inertia_a,
            self.inverse_inertia_b,
        );
        let mut a = self.position_a;
        let mut b = self.position_b;
        let mut angular_error = 0.0;
        if self.limit_enabled && self.limit_state != 0 && ia + ib != 0.0 {
            let angle = joint_angle(a[2], b[2], self.reference_angle);
            let max = self.max_angular_correction;
            let slop = f32::from_bits(0x3d0efa36);
            let correction = match self.limit_state {
                3 => {
                    let c = native_clamp(angle - self.limits[0], -max, max);
                    angular_error = c.abs();
                    c
                }
                1 => {
                    let c = angle - self.limits[0];
                    angular_error = -c;
                    native_clamp(c + slop, -max, 0.0)
                }
                2 => {
                    let c = angle - self.limits[1];
                    angular_error = c;
                    native_clamp(c - slop, 0.0, max)
                }
                _ => 0.0,
            };
            let impulse = -(correction * self.motor_mass);
            a[2] -= ia * impulse;
            b[2] += ib * impulse;
        }
        let rotate = |angle: f32, anchor: [f32; 2], center: [f32; 2]| {
            let (s, c) = angle.sin_cos();
            let (x, y) = (anchor[0] - center[0], anchor[1] - center[1]);
            [c * x - s * y, s * x + c * y]
        };
        let ra = rotate(a[2], self.anchor_a, self.local_center_a);
        let rb = rotate(b[2], self.anchor_b, self.local_center_b);
        let cx = ((b[0] + rb[0]) - a[0]) - ra[0];
        let cy = ((b[1] + rb[1]) - a[1]) - ra[1];
        let position_error = (cx * cx + cy * cy).sqrt();
        let mass_sum = mb + ma;
        let xx = ra[1] * ia * ra[1] + mass_sum + rb[1] * ib * rb[1];
        let xy = -(ra[0] * ia * ra[1]) - rb[0] * ib * rb[1];
        let yy = ra[0] * ia * ra[0] + mass_sum + rb[0] * ib * rb[0];
        let mut det = xx * yy - xy * xy;
        if det != 0.0 {
            det = 1.0 / det;
        }
        let px = -((cx * yy - cy * xy) * det);
        let py = -((cy * xx - cx * xy) * det);
        self.position_a = [
            a[0] - px * self.axis_a[0] * ma,
            a[1] - py * self.axis_a[1] * ma,
            a[2] - (ra[0] * py - ra[1] * px) * ia,
        ];
        self.position_b = [
            b[0] + px * self.axis_b[0] * mb,
            b[1] + py * self.axis_b[1] * mb,
            (rb[0] * py - rb[1] * px) * ib + b[2],
        ];
        !(f32::from_bits(0x3ba3d70a) < position_error || f32::from_bits(0x3d0efa36) < angular_error)
    }
}

impl VelocityConstraint {
    pub fn initialize(
        &mut self,
        angle: f32,
        limits: Option<[f32; 2]>,
        dt_ratio: f32,
        warm_starting: bool,
    ) {
        self.prepare_matrix();
        let fixed = self.inverse_inertia_a + self.inverse_inertia_b == 0.0;
        if !self.motor_enabled || fixed {
            self.motor_impulse = 0.0;
        }
        let old_state = self.limit_state;
        self.limit_enabled = limits.is_some();
        self.limit_state = if let Some([lower, upper]) = limits.filter(|_| !fixed) {
            let state = limit_state(angle, lower, upper);
            if state == 0 || ((state == 1 || state == 2) && state != old_state) {
                self.impulse[2] = 0.0;
            }
            state
        } else {
            0
        };
        if warm_starting {
            for value in &mut self.impulse {
                *value = dt_ratio * *value;
            }
            self.motor_impulse = dt_ratio * self.motor_impulse;
            let [x, y, z] = self.impulse;
            self.velocity_a[0] -= x * self.inverse_mass_a;
            self.velocity_a[1] -= y * self.inverse_mass_a;
            self.velocity_a[2] -=
                ((y * self.anchor_a[0] - x * self.anchor_a[1]) + self.motor_impulse + z)
                    * self.inverse_inertia_a;
            self.velocity_b[0] += x * self.inverse_mass_b;
            self.velocity_b[1] += y * self.inverse_mass_b;
            self.velocity_b[2] +=
                ((y * self.anchor_b[0] - x * self.anchor_b[1]) + self.motor_impulse + z)
                    * self.inverse_inertia_b;
        } else {
            self.impulse = [0.0; 3];
            self.motor_impulse = 0.0;
        }
        self.velocity_a[0] *= self.axis_a[0];
        self.velocity_a[1] *= self.axis_a[1];
        self.velocity_b[0] *= self.axis_b[0];
        self.velocity_b[1] *= self.axis_b[1];
    }

    pub fn prepare_matrix(&mut self) {
        let [ax, ay] = self.anchor_a;
        let [bx, by] = self.anchor_b;
        let (ma, mb, ia, ib) = (
            self.inverse_mass_a,
            self.inverse_mass_b,
            self.inverse_inertia_a,
            self.inverse_inertia_b,
        );
        let mass_sum = mb + ma;
        let xx = ay * ay * ia + mass_sum + by * by * ib;
        let xy = -(ay * ax * ia) - by * bx * ib;
        let xz = -(ay * ia) - by * ib;
        let yy = ax * ax * ia + mass_sum + bx * bx * ib;
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
        "NATIVE HINGE VELOCITY PASSED: {} prepared velocity-constraint iterations match the installed engine bit-for-bit.",
        cases.len()
    );
    compare_initialization(executable, &cases)?;
    compare_positions(executable, &cases)?;
    Ok(())
}

#[derive(Serialize)]
struct PositionCase {
    #[serde(flatten)]
    constraint: PositionConstraint,
    position_solve: bool,
    iterations: u32,
}

#[derive(Debug, Deserialize, PartialEq)]
struct PositionBits {
    position_a_bits: [u32; 3],
    position_b_bits: [u32; 3],
    position_solved: bool,
}

fn compare_positions(
    executable: &std::path::Path,
    source: &[VelocityConstraint],
) -> Result<(), String> {
    let cases: Vec<_> = source
        .iter()
        .enumerate()
        .map(|(i, c)| PositionCase {
            position_solve: true,
            iterations: 1,
            constraint: PositionConstraint {
                position_a: [c.velocity_a[0], c.velocity_a[1], c.velocity_a[2] * 0.25],
                position_b: [c.velocity_b[0], c.velocity_b[1], c.velocity_b[2] * 0.25],
                inverse_mass_a: c.inverse_mass_a,
                inverse_mass_b: c.inverse_mass_b,
                inverse_inertia_a: c.inverse_inertia_a,
                inverse_inertia_b: c.inverse_inertia_b,
                anchor_a: c.anchor_a,
                anchor_b: c.anchor_b,
                local_center_a: [0.125, -0.25],
                local_center_b: [-0.125, 0.0],
                axis_a: c.axis_a,
                axis_b: c.axis_b,
                motor_mass: c.motor_mass,
                reference_angle: [-0.5, 0.0, 0.5][i % 3],
                limits: match i % 4 {
                    0 => [-1.0, 1.0],
                    1 => [0.25, 1.0],
                    2 => [-1.0, -0.25],
                    _ => [0.0, 0.0],
                },
                limit_enabled: c.limit_enabled,
                limit_state: c.limit_state,
                max_angular_correction: [8f32.to_radians(), 0.125, 0.5][i % 3],
            },
        })
        .collect();
    let expected: Vec<PositionBits> = crate::motor::run_reference(executable, &cases)?;
    let mut failures = 0;
    for (i, (c, reference)) in cases.iter().zip(&expected).enumerate() {
        let mut state = c.constraint.clone();
        let solved = state.solve();
        let bits = PositionBits {
            position_a_bits: state.position_a.map(f32::to_bits),
            position_b_bits: state.position_b.map(f32::to_bits),
            position_solved: solved,
        };
        if bits != *reference {
            failures += 1;
            if failures <= 3 {
                eprintln!(
                    "Native position mismatch {i}: Rust={bits:?}, original={reference:?}; input={state:?}"
                );
            }
        }
    }
    if failures != 0 {
        return Err(format!(
            "Native hinge position comparison failed: {failures}/{} differ",
            cases.len()
        ));
    }
    println!(
        "NATIVE HINGE POSITIONS PASSED: {} position/limit correction cases match every output bit and convergence flag.",
        cases.len()
    );
    Ok(())
}

#[derive(Serialize)]
struct InitializationCase {
    #[serde(flatten)]
    constraint: VelocityConstraint,
    initialize: bool,
    iterations: u32,
    warm_starting: bool,
    dt_ratio: f32,
    reference_angle: f32,
    position_a: [f32; 3],
    position_b: [f32; 3],
    local_center_a: [f32; 2],
    local_center_b: [f32; 2],
    limits: [f32; 2],
}

#[derive(Debug, Deserialize, PartialEq)]
struct InitializationBits {
    #[serde(flatten)]
    velocity: NativeBits,
    matrix_bits: [u32; 9],
    motor_mass_bits: u32,
    limit_state: u32,
    anchor_a_bits: [u32; 2],
    anchor_b_bits: [u32; 2],
    joint_angle_bits: u32,
}

fn compare_initialization(
    executable: &std::path::Path,
    source: &[VelocityConstraint],
) -> Result<(), String> {
    let cases: Vec<_> = source
        .iter()
        .take(1024)
        .enumerate()
        .map(|(i, c)| InitializationCase {
            constraint: c.clone(),
            initialize: true,
            iterations: 0,
            warm_starting: i % 3 != 0,
            dt_ratio: [0.5, 1.0, 2.0][i % 3],
            reference_angle: [-0.5, 0.0, 0.5][i % 3],
            position_a: [0.0, 0.0, c.velocity_a[0] * 0.25],
            position_b: [0.0, 0.0, c.velocity_b[0] * 0.25],
            local_center_a: [0.125, -0.25],
            local_center_b: [-0.125, 0.0],
            limits: match i % 4 {
                0 => [-1.0, 1.0],
                1 => [0.25, 1.0],
                2 => [-1.0, -0.25],
                _ => [0.0, 0.0],
            },
        })
        .collect();
    let expected: Vec<InitializationBits> = crate::motor::run_reference(executable, &cases)?;
    let mut failures = 0;
    for (i, (c, reference)) in cases.iter().zip(&expected).enumerate() {
        let mut state = c.constraint.clone();
        let rotate = |angle: f32, anchor: [f32; 2], center: [f32; 2]| {
            let (sine, cosine) = angle.sin_cos();
            let (x, y) = (anchor[0] - center[0], anchor[1] - center[1]);
            [x * cosine - y * sine, y * cosine + x * sine]
        };
        state.anchor_a = rotate(c.position_a[2], state.anchor_a, c.local_center_a);
        state.anchor_b = rotate(c.position_b[2], state.anchor_b, c.local_center_b);
        state.initialize(
            (c.position_b[2] - c.position_a[2]) - c.reference_angle,
            if state.limit_enabled {
                Some(c.limits)
            } else {
                None
            },
            c.dt_ratio,
            c.warm_starting,
        );
        let bits = InitializationBits {
            velocity: NativeBits::from(&state),
            matrix_bits: state.matrix.map(f32::to_bits),
            motor_mass_bits: state.motor_mass.to_bits(),
            limit_state: state.limit_state,
            anchor_a_bits: state.anchor_a.map(f32::to_bits),
            anchor_b_bits: state.anchor_b.map(f32::to_bits),
            joint_angle_bits: joint_angle(c.position_a[2], c.position_b[2], c.reference_angle)
                .to_degrees()
                .to_bits(),
        };
        if bits != *reference {
            failures += 1;
            if failures <= 3 {
                eprintln!(
                    "Native initialization mismatch {i}: Rust={bits:?}, original={reference:?}"
                );
            }
        }
    }
    if failures != 0 {
        return Err(format!(
            "Native hinge initialization failed: {failures}/{} cases differ",
            cases.len()
        ));
    }
    println!(
        "NATIVE HINGE INITIALIZATION PASSED: {} matrix, limit-state and warm-start cases match the installed engine bit-for-bit.",
        cases.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn joint_angle_uses_connected_minus_attached_minus_reference() {
        let angle = joint_angle(90f32.to_radians(), 0.0, 15f32.to_radians()).to_degrees();
        assert!((angle + 105.0).abs() < 0.00001);
    }
    #[test]
    fn warm_start_scales_history_and_cold_start_discards_it() {
        let mut state = VelocityConstraint {
            inverse_mass_a: 1.0,
            inverse_mass_b: 1.0,
            inverse_inertia_a: 1.0,
            inverse_inertia_b: 1.0,
            axis_a: [1.0; 2],
            axis_b: [1.0; 2],
            motor_enabled: true,
            impulse: [2.0, -4.0, 0.0],
            motor_impulse: 2.0,
            ..Default::default()
        };
        state.initialize(0.0, None, 0.5, true);
        assert_eq!(state.impulse, [1.0, -2.0, 0.0]);
        assert_eq!(state.motor_impulse, 1.0);
        assert_eq!(state.velocity_a, [-1.0, 2.0, -1.0]);
        assert_eq!(state.velocity_b, [1.0, -2.0, 1.0]);
        state.initialize(0.0, None, 1.0, false);
        assert_eq!(state.impulse, [0.0; 3]);
        assert_eq!(state.motor_impulse, 0.0);
    }
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
