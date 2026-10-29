//! Checks against trajectories recorded from the running game (private tools/recorder output).
use crate::{game_assets::HumanRig, hinge, living, motor};
use std::{collections::HashMap, path::Path};

/// One recorded value: the recorder writes `decimal|bits` so the bits are exact.
fn value(token: &str) -> f32 {
    let bits = token.rsplit('|').next().unwrap();
    u32::from_str_radix(bits, 16).map_or_else(|_| token.parse().unwrap(), f32::from_bits)
}

#[derive(Default)]
struct Step {
    /// name -> (rotation degrees)
    rotation: HashMap<String, f32>,
    /// name -> (use, speed degrees/s, max torque)
    motor: HashMap<String, (bool, f32, f32)>,
}

struct Recording {
    mass: HashMap<String, f32>,
    steps: Vec<Step>,
}

fn parse(text: &str) -> Recording {
    let mut mass = HashMap::new();
    let mut steps: Vec<Step> = vec![];
    for line in text.lines() {
        let mut tokens = line.split_whitespace();
        match tokens.next() {
            Some("body") => {
                let name = tokens.next().unwrap().to_string();
                mass.insert(name, value(tokens.next().unwrap().trim_start_matches("mass=")));
            }
            Some("step") => steps.push(Step::default()),
            Some("b") => {
                let t: Vec<_> = tokens.collect();
                steps.last_mut().unwrap().rotation.insert(t[0].into(), value(t[3]));
            }
            Some("j") => {
                let name = tokens.next().unwrap().to_string();
                let m: Vec<_> = tokens.next().unwrap().trim_start_matches("motor=").split(':').collect();
                steps.last_mut().unwrap().motor.insert(name, (m[0] == "True", value(m[1]), value(m[2])));
            }
            _ => {}
        }
    }
    Recording { mass, steps }
}

/// Each step's muscle motor commands, computed by the rewrite from the previous recorded step, vs the game's.
pub fn compare_motors(rig: &HumanRig, path: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let recording = parse(&text);
    let pose = &rig.poses[0];
    let mut worst = (0.0f32, 0.0f32);
    let mut mismatches = 0;
    for (k, pair) in recording.steps.windows(2).enumerate() {
        let (before, after) = (&pair[0], &pair[1]);
        for joint in &rig.joints {
            let part = rig.parts.iter().find(|p| p.name == joint.a).unwrap();
            let Some(spec) = &part.muscle else { continue };
            let current = hinge::joint_angle(
                before.rotation[&joint.a].to_radians(),
                before.rotation[&joint.b].to_radians(),
                0.0,
            )
            .to_degrees();
            let previous = before.motor[&joint.a].1;
            let ours = motor::command(&living::muscle_input(pose, part, spec, current, 0.0, previous, recording.mass[&joint.a]))
                .clamp_native_properties();
            let (on, speed, torque) = after.motor[&joint.a];
            let errors = ((ours.speed - speed).abs(), (ours.torque - torque).abs());
            worst = (worst.0.max(errors.0), worst.1.max(errors.1));
            if errors.0 > 1e-3 || errors.1 > 1e-4 || on != (ours.torque > 0.0) {
                mismatches += 1;
                if mismatches <= 10 {
                    println!(
                        "step {} {}: game {on} {speed} {torque}, rewrite {} {}, angle {current}, previous {previous}",
                        k + 1,
                        joint.a,
                        ours.speed,
                        ours.torque
                    );
                }
            }
        }
    }
    println!(
        "Game motor replay: {} steps x {} muscles, {mismatches} mismatches, worst speed error {} deg/s, worst torque error {}",
        recording.steps.len() - 1,
        rig.joints.len(),
        worst.0,
        worst.1
    );
    if mismatches > 0 {
        return Err("Muscle motor commands differ from the recorded game".into());
    }
    Ok(())
}
