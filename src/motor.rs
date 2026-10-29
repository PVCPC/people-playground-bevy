//! Healthy, non-animated motor-command calculations in the original degree units.
use serde::{Deserialize, Serialize};

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
}
