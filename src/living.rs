//! PersonBehaviour / LimbBehaviour muscle logic (decompiled Assembly-CSharp), healthy subset.
use super::*;
use b2::world::CONTACT_TOUCHING;
use game_assets::{CurveCache, HumanPose, MuscleSpec, STUMBLING};
use motor::delta_angle;
use std::collections::HashSet;

/// PersonBehaviour values the limbs read.
#[derive(Clone, Copy)]
pub struct PersonState {
    pub consciousness: f32,
    pub adrenaline: f32,
    pub shock: f32,
    pub pain: f32,
    /// BalanceOffset: the head's up vector dotted with world right, times the person's x scale.
    pub balance: f32,
    pub touching_floor: bool,
}

impl Default for PersonState {
    fn default() -> Self {
        Self { consciousness: 1.0, adrenaline: 0.0, shock: 0.0, pain: 0.0, balance: 0.0, touching_floor: false }
    }
}

/// LimbBehaviour / CirculationBehaviour values for one limb.
#[derive(Clone, Copy)]
pub struct LimbState {
    pub health: f32,
    pub initial_health: f32,
    pub numbness: f32,
    pub blood_flow: f32,
    pub blood_amount: f32,
}

impl Default for LimbState {
    // ponytail: constant healthy physiology until CirculationBehaviour and limb health are rewritten.
    fn default() -> Self {
        Self { health: 100.0, initial_health: 100.0, numbness: 0.0, blood_flow: 1.0, blood_amount: 1.0 }
    }
}

#[derive(Component)]
pub struct Human {
    /// Rewrite-only limp toggle: a limp human's limbs count as not alive.
    pub awake: bool,
    pub pose: usize,
    pub root: Entity,
    pub brain: Entity,
    pub state: PersonState,
    pub connected: HashSet<Entity>,
}

#[derive(Component)]
pub struct Limb {
    pub owner: Entity,
    pub definition: usize,
    pub state: LimbState,
}

/// The muscle on a limb's HingeJoint2D: its current JointMotor2D speed (degrees/s) and stumble clock.
#[derive(Component)]
pub struct MuscleJoint {
    pub owner: Entity,
    pub definition: usize,
    pub previous_speed: f32,
    pub stumble_time: f32,
    /// This person's AnimationCurve caches for the limb's pose angles, by pose index.
    pub curves: Vec<CurveCache>,
}

impl MuscleJoint {
    pub fn new(owner: Entity, definition: usize) -> Self {
        Self { owner, definition, previous_speed: 0.0, stumble_time: 0.0, curves: vec![] }
    }

    fn curve(&mut self, pose: usize) -> &mut CurveCache {
        if self.curves.len() <= pose {
            self.curves.resize(pose + 1, CurveCache::default());
        }
        &mut self.curves[pose]
    }
}

/// Everything one LimbBehaviour.ManagedFixedUpdate reads for its muscle.
pub struct LimbInput<'a> {
    pub spec: &'a MuscleSpec,
    pub part: &'a str,
    pub pose: &'a HumanPose,
    pub pose_index: usize,
    pub stumbling: Option<(usize, &'a HumanPose)>,
    pub person: PersonState,
    pub limb: LimbState,
    /// NodeBehaviour.IsConnectedToRoot (and the rewrite's limp toggle).
    pub connected: bool,
    /// IsAnyHeadAlive
    pub head_alive: bool,
    /// rigidbody.mass / TrueInitialMass
    pub mass_ratio: f32,
    /// transform.eulerAngles.z in [0, 360) and transform.up.y
    pub euler: f32,
    pub up_y: f32,
    /// HingeJoint2D.jointAngle and referenceAngle, degrees
    pub joint_angle: f32,
    pub reference: f32,
    /// Person.AngleOffset (-1 when flipped)
    pub angle_offset: f32,
}

impl LimbInput<'_> {
    fn active(&self) -> bool {
        self.pose.angles.iter().any(|a| a.part == self.part)
    }

    fn alive(&self) -> bool {
        self.connected && self.limb.blood_flow > 0.25 && self.limb.health > 0.01
    }

    fn capable(&self) -> bool {
        let (p, l) = (&self.person, &self.limb);
        self.connected
            && l.health > 1.0
            && p.consciousness > 0.8
            && p.shock < 0.5
            && p.pain < 0.5
            && l.numbness < 0.9
            && l.blood_flow > 0.25
            && self.head_alive
    }

    /// LimbBehaviour.MotorStrength
    pub fn motor_strength(&self) -> f32 {
        let (p, l) = (&self.person, &self.limb);
        let adrenaline = p.adrenaline.clamp(0.0, 1.0);
        if p.consciousness < 0.3f32.max(0.8 - adrenaline) || !self.alive() || !self.head_alive {
            return 0.0;
        }
        let ratio = 3.0 * self.spec.blood_strength_ratio;
        let numbness = l.numbness.clamp(0.0, 1.0);
        let a = (l.health / l.initial_health + adrenaline)
            * (l.blood_amount as f64).powf(ratio as f64) as f32
            * (l.blood_flow as f64).powf(ratio as f64) as f32
            * (p.consciousness + adrenaline).clamp(0.0, 1.0)
            * (1.0 - (numbness - adrenaline)).clamp(0.0, 1.0);
        let force = if self.active() { self.pose.force_multiplier } else { 1.0 };
        self.spec.strength * force * 0.9 * a.max(0.0).clamp(0.0, 1.0) * self.mass_ratio
    }
}

/// Mathf.Lerp toward `value`, as LimbBehaviour.InfluenceMotorSpeed.
fn influence(speed: &mut f32, value: f32, t: f32) {
    *speed += (value - *speed) * t.clamp(0.0, 1.0);
}

/// A muscle's motor for one fixed step: returns the max motor torque; updates speed and stumble clock.
pub fn limb_motor<'a>(input: &LimbInput<'a>, muscle: &mut MuscleJoint) -> f32 {
    let speed = &mut muscle.previous_speed;
    let active = input.active();
    let part = input.part;
    let target = |pose: &'a HumanPose| pose.angles.iter().find(move |a| a.part == part);
    // SetMotorStrengthToMuscleStrength
    let mut strength = input.motor_strength().max(0.0);
    for m in &input.spec.multipliers {
        strength *= m;
        if strength.abs() <= f32::from_bits(1) {
            break;
        }
    }
    let modifier = target(input.pose).map_or(0.0, |a| a.rigidity_modifier);
    let torque = strength * if active { (1.0 + modifier).clamp(0.0, 1.0) } else { 1.0 };
    // ApplyPoseOverrides
    let pose = input.pose;
    if input.capable() && input.person.touching_floor && pose.should_stumble {
        let num = (input.person.balance * 1.5).abs();
        let num2 = input.motor_strength().clamp(0.0, num * 2.0);
        if input.spec.do_balance_jerk {
            let value = delta_angle(input.joint_angle, 0.0 - delta_angle(input.euler, 0.0)) * 1.0 * input.spec.balance_muscle_movement;
            influence(speed, value, 0.5 * num2);
        }
        if active && input.person.shock < 0.5 && input.spec.do_stumble && num > 0.3 && num < 1.0 {
            muscle.stumble_time += (input.person.balance * -0.1).clamp(-1.7, 1.7);
            // MoveIntoPoseAt(Stumbling); a limb missing from that pose throws in the original and stops here.
            if let Some((index, stumbling)) = input.stumbling
                && let Some(angle) = target(stumbling)
            {
                let rigidity = ((num as f64).powf(1.5) as f32) * 1.3 * num2;
                let time = muscle.stumble_time;
                let value = delta_angle(input.joint_angle, input.reference + angle.degrees_at(muscle.curve(index), time) * input.angle_offset)
                    * rigidity
                    * 1.0
                    * stumbling.rigidity
                    * (1.0 + angle.rigidity_modifier);
                influence(&mut muscle.previous_speed, value, 0.5);
            }
            return torque;
        }
    }
    // MoveIntoPose
    if active && input.alive() {
        let angle = target(pose).unwrap();
        // ponytail: animated poses evaluated at time 0; EvaluateAngle's Time.time and Perlin noise are not rewritten yet.
        let value = delta_angle(input.joint_angle, input.reference + angle.degrees_at(muscle.curve(input.pose_index), 0.0) * input.angle_offset)
            * 1.0
            * 1.0
            * pose.rigidity
            * (1.0 + angle.rigidity_modifier);
        influence(&mut muscle.previous_speed, value, 0.5);
    }
    torque
}

/// FakeStandUpright torque and the drag factors (angular, linear) the upright block applies, if it runs.
pub fn upright(input: &LimbInput) -> Option<(f32, f32, f32)> {
    let adrenaline = input.person.adrenaline.clamp(0.0, 1.0);
    let pose = input.pose;
    if !(input.limb.blood_flow > 0.25f32.max(0.9 - adrenaline)
        && input.person.touching_floor
        && pose.stand_upright
        && input.capable())
    {
        return None;
    }
    let mut torque = 0.0;
    if input.spec.upright_force > 0.001 && input.connected && input.up_y >= 0.0 {
        let lerp = input.mass_ratio + (1.0 - input.mass_ratio) * 0.6;
        let num = delta_angle(input.euler, 0.0) * input.spec.upright_force * input.motor_strength() * lerp;
        if !num.is_nan() {
            torque = num * 1.8 * pose.upright_multiplier;
        }
    }
    let drag = pose.drag_influence;
    Some((torque, 1.0 + (0.92 - 1.0) * drag.clamp(0.0, 1.0), 1.0 + (0.94 - 1.0) * drag.clamp(0.0, 1.0)))
}

pub fn sense_humans(
    mut humans: Query<&mut Human>,
    limbs: Query<(Entity, &Body), With<Limb>>,
    joints: Query<&Hinge, With<MuscleJoint>>,
    physics: Res<Physics>,
) {
    let world = &physics.world;
    for mut human in &mut humans {
        human.connected.clear();
        let root = human.root;
        human.connected.insert(root);
        loop {
            let old_len = human.connected.len();
            for joint in joints.iter().filter(|j| world.joints[j.joint].is_some()) {
                if human.connected.contains(&joint.body_a) {
                    human.connected.insert(joint.body_b);
                }
                if human.connected.contains(&joint.body_b) {
                    human.connected.insert(joint.body_a);
                }
            }
            if human.connected.len() == old_len {
                break;
            }
        }
        // AttachedLimbLoopChecks: IsOnFloor is PhysicalBehaviour.IsTouchingSomething (any current contact).
        // ponytail: PersonBehaviour.Update runs per rendered frame; here once per fixed step.
        human.state.touching_floor = limbs.iter().any(|(entity, body)| {
            human.connected.contains(&entity)
                && world.body(body.0).contacts.iter().any(|&(_, c)| world.contact(c).flags & CONTACT_TOUCHING != 0)
        });
        if let Ok((_, head)) = limbs.get(human.brain) {
            human.state.balance = -world.body(head.0).sweep.a.sin();
        }
    }
}

pub fn drive_muscles(
    rig: Res<HumanRig>,
    session: Res<Session>,
    humans: Query<&Human>,
    mut joints: Query<(&Hinge, &mut MuscleJoint)>,
    limbs: Query<(Entity, &Limb, &Body)>,
    mut physics: ResMut<Physics>,
) {
    if session.paused {
        return;
    }
    let stumbling = rig.poses.iter().enumerate().find(|(_, p)| p.state == STUMBLING);
    let world = &mut physics.world;
    let mut hinges: Vec<_> = joints.iter_mut().collect();
    for (entity, limb, body) in &limbs {
        let human = humans.get(limb.owner).unwrap();
        let (Some(pose), Some(spec)) = (rig.poses.get(human.pose), &rig.parts[limb.definition].muscle) else {
            continue;
        };
        let part = &rig.parts[limb.definition];
        let b = world.body(body.0);
        let angle = b.sweep.a;
        let mut input = LimbInput {
            spec,
            part: &part.name,
            pose,
            pose_index: human.pose,
            stumbling,
            person: human.state,
            limb: limb.state,
            connected: human.awake && human.connected.contains(&entity),
            head_alive: human.connected.contains(&human.brain),
            mass_ratio: b.mass / part.mass,
            euler: angle.to_degrees().rem_euclid(360.0),
            up_y: angle.cos(),
            joint_angle: 0.0,
            reference: 0.0,
            angle_offset: 1.0,
        };
        if let Some((torque, angular, linear)) = upright(&input) {
            let b = world.body_mut(body.0);
            b.apply_torque(torque);
            b.angular_velocity *= angular;
            b.linear_velocity = b.linear_velocity * linear;
        }
        let Some((hinge, muscle)) = hinges.iter_mut().find(|(h, _)| h.body_a == entity) else {
            continue;
        };
        let Some(joint) = world.joints[hinge.joint].as_ref() else {
            continue;
        };
        let reference = joint.revolute().def.reference_angle;
        input.joint_angle = hinge::joint_angle(angle, world.body(joint.body_b).sweep.a, reference).to_degrees();
        input.reference = reference.to_degrees();
        let torque = limb_motor(&input, muscle);
        let out = motor::MotorOutput { speed: muscle.previous_speed, torque }.clamp_native_properties();
        muscle.previous_speed = out.speed;
        let def = &mut world.joints[hinge.joint].as_mut().unwrap().revolute_mut().def;
        def.enable_motor = out.torque > 0.0;
        def.motor_speed = out.speed * 0.017453292;
        def.max_motor_torque = out.torque;
    }
}
