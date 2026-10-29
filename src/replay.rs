//! Checks against trajectories recorded from the running game (private tools/recorder output).
use crate::{
    game_assets::{HumanRig, STUMBLING},
    living::{self, LimbInput, LimbState, MuscleJoint, PersonState},
};
use bevy::prelude::Entity;
use std::{collections::HashMap, path::Path};

/// One recorded value: the recorder writes `decimal|bits` so the bits are exact.
fn value(token: &str) -> f32 {
    let bits = token.rsplit('|').next().unwrap();
    u32::from_str_radix(bits, 16).map_or_else(|_| token.parse().unwrap(), f32::from_bits)
}

/// LimbBehaviour values logged in FixedUpdate, read by the limb's muscle this step.
struct LimbLine {
    state: LimbState,
    active: bool,
    strength: f32,
    true_mass: f32,
    euler: f32,
    up_y: f32,
}

#[derive(Default)]
struct Step {
    /// name -> (use, speed degrees/s, max torque)
    motor: HashMap<String, (bool, f32, f32)>,
    /// name -> jointAngle degrees
    angle: HashMap<String, f32>,
    /// Inputs logged before this step's physics.
    person: Option<(PersonState, String)>,
    limbs: HashMap<String, LimbLine>,
}

struct Recording {
    mass: HashMap<String, f32>,
    steps: Vec<Step>,
}

fn fields(line: &str) -> HashMap<&str, &str> {
    line.split_whitespace().filter_map(|t| t.split_once('=')).collect()
}

fn parse(text: &str) -> Recording {
    let mut mass = HashMap::new();
    let mut steps: Vec<Step> = vec![];
    let mut next = Step::default();
    for line in text.lines() {
        let mut tokens = line.split_whitespace();
        let f = fields(line);
        match tokens.next() {
            Some("body") => {
                mass.insert(tokens.next().unwrap().to_string(), value(f["mass"]));
            }
            Some("step") => steps.push(std::mem::take(&mut next)),
            Some("j") => {
                let name = tokens.next().unwrap().to_string();
                let m: Vec<_> = f["motor"].split(':').collect();
                let step = steps.last_mut().unwrap();
                step.motor.insert(name.clone(), (m[0] == "True", value(m[1]), value(m[2])));
                step.angle.insert(name, value(f["angle"]));
            }
            Some("p") => {
                let state = PersonState {
                    consciousness: value(f["conscious"]),
                    adrenaline: value(f["adrenaline"]),
                    shock: value(f["shock"]),
                    pain: value(f["pain"]),
                    balance: value(f["balance"]),
                    touching_floor: f["floor"] == "True",
                };
                // Pose names can contain spaces ("In pain"): take everything between pose= and speed=.
                let pose = line.split(" pose=").nth(1).unwrap().split(" speed=").next().unwrap();
                next.person = Some((state, pose.to_string()));
            }
            Some("l") => {
                let (health, initial) = f["health"].split_once('/').unwrap();
                let up = line.split(" up=").nth(1).unwrap().split_whitespace().nth(1).unwrap();
                next.limbs.insert(
                    tokens.next().unwrap().to_string(),
                    LimbLine {
                        state: LimbState {
                            health: value(health),
                            initial_health: value(initial),
                            numbness: value(f["numb"]),
                            blood_flow: value(f["flow"]),
                            blood_amount: value(f["blood"]),
                        },
                        active: f["active"] == "True",
                        strength: value(f["strength"]),
                        true_mass: value(f["trueMass"]),
                        euler: value(f["euler"]),
                        up_y: value(up),
                    },
                );
            }
            _ => {}
        }
    }
    Recording { mass, steps }
}

/// Seeded UnityEngine.Random sequences logged by the recorder's PPG_RANDOM_TEST mode.
pub fn compare_random(path: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut random = crate::unity::Random::new(0);
    let (mut compared, mut mismatches) = (0, 0);
    let bits = |t: &str| u32::from_str_radix(t, 16).unwrap();
    for line in text.lines() {
        let t: Vec<&str> = line.split_whitespace().collect();
        let (game, ours): (Vec<u32>, Vec<u32>) = match t.first().copied() {
            Some("rinit") => {
                random = crate::unity::Random::new(t[1].parse().unwrap());
                (t[2..6].iter().map(|s| bits(s)).collect(), random.state.to_vec())
            }
            Some("rv") => (vec![bits(t[1])], vec![random.value().to_bits()]),
            Some("rf") => {
                let (min, max) = (f32::from_bits(bits(t[1])), f32::from_bits(bits(t[2])));
                (vec![bits(t[3])], vec![random.range(min, max).to_bits()])
            }
            Some("ri") => (vec![t[3].parse::<i32>().unwrap() as u32], vec![random.range_int(t[1].parse().unwrap(), t[2].parse().unwrap()) as u32]),
            Some("rc") => (t[1..3].iter().map(|s| bits(s)).collect(), random.inside_unit_circle().map(f32::to_bits).to_vec()),
            Some("rs") => (t[1..4].iter().map(|s| bits(s)).collect(), random.inside_unit_sphere().map(f32::to_bits).to_vec()),
            Some("ro") => (t[1..4].iter().map(|s| bits(s)).collect(), random.on_unit_sphere().map(f32::to_bits).to_vec()),
            Some("rstate") => (t[1..5].iter().map(|s| bits(s)).collect(), random.state.to_vec()),
            _ => continue,
        };
        compared += 1;
        if game != ours {
            mismatches += 1;
            if mismatches <= 12 {
                println!("{line}: rewrite {ours:08x?}");
            }
        }
    }
    println!("Game Random replay: {compared} values, {mismatches} mismatches");
    if compared == 0 || mismatches > 0 {
        return Err("UnityEngine.Random differs from the recorded game".into());
    }
    Ok(())
}

/// Each step's muscle motor commands, computed by the rewrite from the recorded inputs, vs the game's.
pub fn compare_motors(rig: &HumanRig, path: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let recording = parse(&text);
    let stumbling = rig.poses.iter().enumerate().find(|(_, p)| p.state == STUMBLING);
    let mut muscles: HashMap<&str, MuscleJoint> = HashMap::new();
    let (mut worst, mut mismatches, mut compared) = ([0.0f32; 3], 0, 0);
    let mut report = |what: String| {
        mismatches += 1;
        if mismatches <= 12 {
            println!("{what}");
        }
    };
    for (k, pair) in recording.steps.windows(2).enumerate() {
        let (before, after) = (&pair[0], &pair[1]);
        let Some((person, pose_name)) = &after.person else { continue };
        let Some((pose_index, pose)) = rig.poses.iter().enumerate().find(|(_, p)| &p.name == pose_name) else {
            return Err(format!("step {}: pose {pose_name} is not in the rig", k + 1));
        };
        for joint in &rig.joints {
            let part = rig.parts.iter().find(|p| p.name == joint.a).unwrap();
            let (Some(spec), Some(limb)) = (&part.muscle, after.limbs.get(&joint.a)) else { continue };
            let muscle = muscles.entry(&joint.a).or_insert(MuscleJoint::new(Entity::PLACEHOLDER, 0));
            muscle.previous_speed = before.motor[&joint.a].1;
            let input = LimbInput {
                spec,
                part: &part.name,
                pose,
                pose_index,
                stumbling,
                person: *person,
                limb: limb.state,
                connected: true,
                head_alive: true,
                mass_ratio: recording.mass[&joint.a] / limb.true_mass,
                euler: limb.euler,
                up_y: limb.up_y,
                joint_angle: before.angle[&joint.a],
                reference: 0.0,
                angle_offset: 1.0,
            };
            compared += 1;
            let strength = input.motor_strength();
            let torque = living::limb_motor(&input, muscle);
            let (on, speed, game_torque) = after.motor[&joint.a];
            let errors = [(strength - limb.strength).abs(), (muscle.previous_speed - speed).abs(), (torque - game_torque).abs()];
            for (w, e) in worst.iter_mut().zip(errors) {
                *w = w.max(e);
            }
            let active = pose.angles.iter().any(|a| a.part == joint.a);
            if errors[0] > 1e-4 || errors[1] > 1e-3 || errors[2] > 1e-4 || on != (torque > 0.0) || active != limb.active {
                report(format!(
                    "step {} {} [{pose_name} balance {} floor {}]: game strength {} speed {speed} torque {game_torque} active {}; rewrite {strength} {} {torque} {active}",
                    k + 1,
                    joint.a,
                    person.balance,
                    person.touching_floor,
                    limb.strength,
                    limb.active,
                    muscle.previous_speed,
                ));
            }
        }
    }
    println!(
        "Game motor replay: {compared} muscle steps, {mismatches} mismatches, worst error strength {} speed {} deg/s torque {}",
        worst[0], worst[1], worst[2]
    );
    if compared == 0 || mismatches > 0 {
        return Err("Muscle motor commands differ from the recorded game".into());
    }
    Ok(())
}
