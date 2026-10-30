//! LimbBehaviour
use super::physical::{dot2, get_average_impulse_remove_outliers, get_contacts, get_first_valid_contact, get_min_impulse};
use super::{d, mathf, person::pose_state, script, Game};
use crate::b2::contacts2d::ScriptCollision;

/// LimbBehaviour.BodyPart
pub const HEAD: i32 = 0;
pub const TORSO: i32 = 1;
pub const LEGS: i32 = 2;
pub const ARMS: i32 = 3;

/// Instance ids taken by the Instantiate of the LimbStatus prefab (LimbBehaviour.Awake): GameObject, Transform, Optout,
/// LimbStatusBehaviour, SpriteRenderer, ParentConstraint, then the bar's GameObject, Transform and SpriteRenderer.
pub const LIMB_STATUS_OBJECTS: usize = 9;

script! {
    Limb {
        RoughClassification: i32 = 0,
        GForcePassoutThreshold: f32 = 10.0,
        GForceDamageThreshold: f32 = 10.0,
        KillShotParticlesEmitted: bool = false,
        Vitality: f32 = 0.0,
        InitialHealth: f32 = 0.0,
        HasBrain: bool = false,
        FreezingTemperature: f32 = -25.0,
        DiscomfortingHeatTemperature: f32 = 40.0,
        RegenerationSpeed: f32 = 0.0,
        ImpactPainMultiplier: f32 = 0.0,
        ImpactDamageMultiplier: f32 = 1.0,
        BreakingThreshold: f32 = 0.0,
        BaseStrength: f32 = 5.0,
        FakeUprightForce: f32 = 0.0,
        BodyTemperature: f32 = 37.0,
        ShotDamageMultiplier: f32 = 1.0,
        DoStumble: bool = false,
        IsLethalToBreak: bool = false,
        DoBalanceJerk: bool = false,
        ExplodeLiquidAmount: f32 = 5.357143,
        BloodMuscleStrengthRatio: f32 = 1.0,
        BalanceMuscleMovement: f32 = 1.0,
        Health: f32 = 0.0,
        Numbness: f32 = 0.0,
        IsAndroid: bool = false,
        IsZombie: bool = false,
        HasJoint: bool = false,
        Broken: bool = false,
        Frozen: bool = false,
        IsDismembered: bool = false,
        IsOnFloor: bool = false,
        DistanceToBrain: i32 = 0,
        InternalExternalTemperatureTransferRate: f32 = 0.01,
        InternalToExternalTempTransferRate: f32 = 0.002,
        ExternalToInternalTempTransferRate: f32 = 0.005,
        InternalTemperature: f32 = 0.0,
        IsActiveInCurrentPose: bool = false,
        previousVelocity: [f32; 2] = [0.0, 0.0],
        FrictionBurnWoundMinSpeedSqrd: f32 = 62.0,
        BodyHeatProductionFactor: f32 = 1.0,
        stumbleTime: f32 = 0.0,
        JointStress: f32 = 0.0,
        randomOffset: f32 = 0.0,
        BruiseCount: u16 = 0,
        ShatteredObjectChance: f32 = 0.1,
        CurrentlyShattered: u8 = 0,
        ImmuneToDamage: bool = false,
        previousHealth: f32 = 0.0,
        HasLungs: bool = false,
        LungsPunctured: bool = false,
        shotHeat: f32 = 0.0,
        originalJointLimits: [f32; 2] = [0.0, 0.0],
        RegenerateBurnProgressSpeed: f32 = 1.0,
    }
    links {
        name: String = String::new(),
        id: i32 = 0,
        PhysicalBehaviour: usize = 0,
        Person: usize = 0,
        /// Index into Game::hinges.
        Joint: Option<usize> = None,
        GripBehaviour: Option<usize> = None,
        ConnectedLimbs: Vec<usize> = Vec::new(),
        NearestLimbToBrain: Option<usize> = None,
        StrengthMultipliers: Vec<f32> = Vec::new(),
        SpeciesIdentity: String = "Human".into(),
        BloodLiquidType: String = "BLOOD".into(),
        /// The BloodDecal DecalDescriptor's name.
        BloodDecal: String = "Dense_blood".into(),
        enabled: bool = true,
        /// The GoreStringBehaviour component: present, enabled and its `Joint` (a SpringJoint2D).
        goreString: bool = true,
        goreStringEnabled: bool = true,
        goreStringJoint: bool = false,
        /// The GoreStringBehaviour's own values and line state.
        gore: GoreString = GoreString::default(),
    }
}

/// GoreStringBehaviour.TissueString
#[derive(Clone, Copy, Debug)]
pub struct TissueString {
    pub Pos1: f32,
    pub Pos2: f32,
    pub Strength: f32,
    pub Thickness: f32,
    pub ThicknessRatio: f32,
}

/// GoreStringBehaviour (the fields the simulation reads; rendering is skipped).
#[derive(Clone, Debug)]
pub struct GoreString {
    /// The limb whose rigidbody is `Other`.
    pub Other: Option<usize>,
    pub JointDampening: f32,
    pub JointFreq: f32,
    pub JointBreakThreshold: f32,
    pub InvincibilityTime: f32,
    /// LineSegment A.x, A.y, B.x, B.y
    pub OriginLine: [f32; 4],
    pub OtherLine: [f32; 4],
    pub HadJointWhenSerialised: bool,
    /// TissueSnapClips length (0 when null).
    pub TissueSnapClips: usize,
    pub AudioSource: bool,
    /// `lines != null` with its `lineData`.
    pub lineData: Option<Vec<TissueString>>,
    /// The pending SetBreakForce coroutine: Time.timeAsDouble it resumes at.
    pub break_at: Option<f64>,
    /// Joint.breakForce of the SpringJoint2D.
    pub breakForce: f32,
    /// The SpringJoint2D's b2DistanceJoint.
    pub joint: Option<crate::b2::world::JointId>,
    /// A pending Object.Destroy(Joint): the DelayedCallManager time it runs at.
    pub destroy_at: Option<f64>,
}

impl Default for GoreString {
    fn default() -> Self {
        Self {
            Other: None,
            JointDampening: 0.5,
            JointFreq: 4.0,
            JointBreakThreshold: 4.0,
            InvincibilityTime: 0.8,
            OriginLine: [0.0; 4],
            OtherLine: [0.0; 4],
            HadJointWhenSerialised: false,
            TissueSnapClips: 0,
            AudioSource: false,
            lineData: None,
            break_at: None,
            breakForce: f32::INFINITY,
            joint: None,
            destroy_at: None,
        }
    }
}

impl Game {
    fn mass_strength_ratio(&self, l: usize) -> f32 {
        let ph = &self.physicals[self.limbs[l].PhysicalBehaviour];
        ph.rigidbody.mass(&self.world) / ph.TrueInitialMass
    }

    /// The limb's GameObject.activeInHierarchy (a disintegrated limb is deactivated).
    pub fn limb_active(&self, l: usize) -> bool {
        self.physicals[self.limbs[l].PhysicalBehaviour].active
    }

    /// LimbBehaviour.IsParalysed
    pub fn limb_is_paralysed(&self, l: usize) -> bool {
        let limb = &self.limbs[l];
        if !limb.IsAndroid && !limb.IsZombie {
            if let Some(n) = limb.NearestLimbToBrain {
                if self.nodes[n].IsConnectedToRoot {
                    if !self.limbs[n].Broken || self.limbs[n].RoughClassification != TORSO {
                        return self.limb_is_paralysed(n);
                    }
                    return true;
                }
                return false;
            }
        }
        false
    }

    /// LimbBehaviour.IsConsideredAlive
    pub fn limb_is_considered_alive(&self, l: usize) -> bool {
        if self.nodes[l].IsConnectedToRoot && self.circulations[l].BloodFlow > 0.25 {
            return self.limbs[l].Health > 0.01;
        }
        false
    }

    /// LimbBehaviour.IsAnyHeadAlive -> Person.IsHeadAlive
    fn is_any_head_alive(&self, l: usize) -> bool {
        self.people[self.limbs[l].Person].Head.is_some_and(|h| self.limb_is_considered_alive(h))
    }

    /// LimbBehaviour.IsCapable
    pub fn limb_is_capable(&mut self, l: usize) -> bool {
        let limb = &self.limbs[l];
        let ph = &self.physicals[limb.PhysicalBehaviour];
        if limb.Frozen || ph.Temperature <= limb.FreezingTemperature {
            return false;
        }
        let person = &self.people[limb.Person];
        let c = &self.circulations[l];
        if self.nodes[l].IsConnectedToRoot
            && limb.Health > 1.0
            && person.Consciousness > 0.8
            && person.ShockLevel < 0.5
            && person.PainLevel < 0.5
            && limb.Numbness < 0.9
            && c.InternalBleedingIntensity < 0.5
            && self.has_circulation(l)
            && !limb.Broken
            && c.BloodFlow > 0.25
        {
            return self.is_any_head_alive(l);
        }
        false
    }

    /// LimbBehaviour.MotorStrength getter (it writes Health and Numbness).
    pub fn motor_strength(&mut self, l: usize) -> f32 {
        let person = self.limbs[l].Person;
        let (consciousness, adrenaline) = (self.people[person].Consciousness, self.people[person].AdrenalineLevel);
        if consciousness < mathf::max(0.3, 0.8 - mathf::clamp01(adrenaline)) {
            return 0.0;
        }
        let ib = self.circulations[l].InternalBleedingIntensity;
        let limb = &mut self.limbs[l];
        limb.Health = mathf::max(0.0, limb.Health);
        limb.Numbness = mathf::clamp01(limb.Numbness + mathf::min(ib * 0.25, 0.1));
        let charge = self.physicals[self.limbs[l].PhysicalBehaviour].Charge();
        if (self.limbs[l].Broken || self.limb_is_paralysed(l) || !self.limb_is_considered_alive(l) || !self.is_any_head_alive(l))
            && charge < 0.05
        {
            return 0.0;
        }
        let limb = &self.limbs[l];
        let ph = &self.physicals[limb.PhysicalBehaviour];
        let c = &self.circulations[l];
        let ratio = 3.0 * limb.BloodMuscleStrengthRatio;
        let a = ((d(limb.Health) / limb.InitialHealth + mathf::clamp01(adrenaline))
            * mathf::pow(self.amount_of_blood(l), ratio)
            * mathf::pow(c.BloodFlow, 3.0 * limb.BloodMuscleStrengthRatio)
            * mathf::clamp01(consciousness + mathf::clamp01(adrenaline))
            * mathf::clamp01(1.0 - ph.BurnProgress)
            * mathf::clamp01(1.0 - self.skins[l].acidProgress)
            * mathf::clamp01((1.0 - (d(limb.Numbness) - mathf::clamp01(adrenaline))).f()))
        .f();
        let num = match self.people[person].ActivePose {
            Some(_) if limb.IsActiveInCurrentPose => self.active_pose(person).ForceMultiplier,
            _ => 1.0,
        };
        (d(limb.BaseStrength) * num * 0.9 * mathf::clamp01(mathf::max(a, ph.Charge() * 0.5)) * self.mass_strength_ratio(l)).f()
    }

    /// LimbBehaviour.Damage
    pub fn limb_damage(&mut self, l: usize, damage: f32) {
        if self.prefs.StopAnimationOnDamage && !self.limbs[l].IsZombie && damage > 15.5 && self.limb_is_considered_alive(l) {
            self.people[self.limbs[l].Person].OverridePoseIndex = -1;
        }
        self.limbs[l].Health -= damage;
        if self.limbs[l].Health <= 0.0 {
            self.circulations[l].IsPump = false;
        }
    }

    /// LimbBehaviour.Wince
    pub fn limb_wince(&mut self, l: usize, intensity: f32) {
        if self.limbs[l].HasJoint && self.limb_is_considered_alive(l) && !(self.limbs[l].Health < 0.1) {
            let value = (d(60.0f32) * intensity * (d(mathf::perlin(self.time.time * 8.0, self.limbs[l].randomOffset)) * 2.0 - 1.0)).f();
            self.influence_motor_speed(l, mathf::clamp(value, -450.0, 450.0), 0.5);
        }
    }

    /// LimbBehaviour.InfluenceMotorSpeed
    pub fn influence_motor_speed(&mut self, l: usize, mut value: f32, influence: f32) {
        if !self.limbs[l].HasJoint {
            return;
        }
        let h = self.limbs[l].Joint.unwrap();
        let motor = self.hinges[h].motor;
        if self.people[self.limbs[l].Person].BrainDamaged && self.random.value() > 0.95 {
            value *= self.random.range_int(-1, 1) as f32;
        }
        let speed = mathf::lerp(motor[0], value, influence);
        self.hinges[h].set_motor(&mut self.world, speed, motor[1]);
    }

    /// LimbBehaviour.SetMotorStrength
    fn set_motor_strength(&mut self, l: usize, strength: f32) {
        let h = self.limbs[l].Joint.unwrap();
        let motor = self.hinges[h].motor;
        let mut num = 1.0;
        if self.limbs[l].IsActiveInCurrentPose {
            let pose = self.active_pose(self.limbs[l].Person);
            num = mathf::clamp01(1.0 + pose.Angles[pose.angle_index(l).unwrap()].PoseRigidityModifier);
        }
        let torque = (d(strength) * (if self.limbs[l].IsAndroid { 6.0 } else { 1.0 }) * num).f();
        self.hinges[h].set_motor(&mut self.world, motor[0], torque);
    }

    /// LimbBehaviour.SetMotorStrengthToMuscleStrength
    fn set_motor_strength_to_muscle_strength(&mut self, l: usize) {
        let strength = self.motor_strength(l);
        let mut num = mathf::max(strength, self.skins[l].rottenProgress * 0.8);
        for &m in &self.limbs[l].StrengthMultipliers {
            num *= m;
            if num.abs() <= mathf::EPSILON {
                break;
            }
        }
        self.set_motor_strength(l, num);
    }

    /// LimbBehaviour.SetJointFragility
    fn set_joint_fragility(&mut self, l: usize) {
        if !self.limbs[l].HasJoint {
            return;
        }
        let h = self.limbs[l].Joint.unwrap();
        let ph = &self.physicals[self.limbs[l].PhysicalBehaviour];
        if ph.isDisintegrated {
            self.set_hinge_break_force(h, 0.0);
            return;
        }
        let limb = &self.limbs[l];
        // One float expression: Mono evaluates the chain in double.
        let mut num = (d(limb.BreakingThreshold) * 120.0f32 * mathf::clamp(1.0 - self.skins[l].rottenProgress, 0.1, 1.0)
            * self.mass_strength_ratio(l))
        .f();
        num /= self.prefs.FragilityMultiplier;
        if !limb.IsAndroid {
            num *= mathf::clamp(mathf::map_range(-100.0, 0.0, 0.1, 1.0, ph.Temperature), 0.1, 1.0);
        }
        if num.is_infinite() {
            num = f32::MAX;
        }
        let force = num * (if limb.IsAndroid { 80.0 } else { 2.5 });
        self.set_hinge_break_force(h, force);
        self.set_hinge_break_torque(h, num * 5000.0);
    }

    /// LimbBehaviour.CalculateJointStress
    fn calculate_joint_stress(&mut self, l: usize) {
        let Some(h) = self.limbs[l].Joint else { return };
        if self.limbs[l].HasJoint && !self.limbs[l].Broken && self.hinges[h].useLimits {
            let angle = self.hinges[h].joint_angle(&self.world);
            let limits = self.limbs[l].originalJointLimits;
            if angle - 5.0 > limits[1] || angle + 5.0 < limits[0] {
                self.limbs[l].JointStress += if self.limbs[l].IsAndroid { 0.1 } else { 0.25 };
            }
            if self.limbs[l].JointStress > self.limbs[l].BreakingThreshold * self.mass_strength_ratio(l) {
                self.break_bone(l);
            }
            self.limbs[l].JointStress *= 0.982;
        }
    }

    pub fn break_bone(&mut self, l: usize) {
        if !self.limbs[l].Broken {
            self.break_bone_internal(l);
        }
    }

    /// LimbBehaviour.BreakBoneInternal
    fn break_bone_internal(&mut self, l: usize) {
        self.limbs[l].Broken = true;
        if !self.limbs[l].HasJoint {
            return;
        }
        let person = self.limbs[l].Person;
        if self.nodes[l].IsConnectedToRoot {
            self.people[person].ShockLevel = (d(self.people[person].ShockLevel) + d(self.random.value()) * 5.0f32).f();
            let i = self.random.value() * 150.0;
            self.person_wince(person, i);
        }
        if self.limbs[l].IsLethalToBreak {
            let damage = self.limbs[l].Health + 1.0;
            self.limb_damage(l, damage);
        }
        if self.random.value() as f64 > 0.9 {
            self.circulations[l].InternalBleedingIntensity += self.random.value();
        }
        // PlayClipOnce(BoneBreakClips.PickRandom())
        self.pick_random(self.people[person].BoneBreakClips);
        let h = self.limbs[l].Joint.unwrap();
        if self.hinges[h].useLimits {
            let [min, max] = self.hinges[h].limits;
            let max = mathf::lerp(max, 180.0, 0.5);
            let min = mathf::lerp(min, -180.0, 0.5);
            self.hinges[h].set_limits(&mut self.world, min, max);
        }
    }

    /// LimbBehaviour.Awake. The component links (Skin, Physical, Circulation, Node, Grip, GoreString, Collider, Joint,
    /// ShatteredObjectGenerators) are the shared arena index and the fields of this limb. Not modelled: `Color = white`
    /// (sprite colour) and the LimbStatus UI clone's own state (LimbStatusBehaviour holds no simulation data).
    pub fn limb_awake(&mut self, l: usize) {
        // myStatus = Instantiate(LimbStatus, transform): the clone's objects take instance ids.
        for _ in 0..LIMB_STATUS_OBJECTS {
            self.allocate_instance_id();
        }
        self.limbs[l].InitialHealth = self.limbs[l].Health;
        self.limbs[l].HasJoint = self.limbs[l].Joint.is_some();
        self.limbs[l].randomOffset = self.random.range_int(-10000, 10000) as f32;
        if self.limbs[l].HasJoint {
            self.limb_setup_joint(l);
        }
        self.limbs[l].InternalTemperature = self.limbs[l].BodyTemperature;
        let p = self.limbs[l].PhysicalBehaviour;
        self.physicals[p].Temperature = self.limbs[l].BodyTemperature;
    }

    /// LimbBehaviour.SetupJoint: useMotor = true, then motor = (speed 0, torque MotorStrength). The motor setter forces
    /// useMotor on and enables the b2 motor first (waking both bodies), so it covers the first assignment too.
    fn limb_setup_joint(&mut self, l: usize) {
        let torque = self.motor_strength(l);
        let h = self.limbs[l].Joint.unwrap();
        self.hinges[h].set_motor(&mut self.world, 0.0, torque);
    }

    /// LimbBehaviour.Start. Not modelled: the undo history lookup (UndoControllerBehaviour.FindRelevantAction),
    /// LimbBehaviourManager.Limbs.Add, the OnDisintegration subscription (Disintegrate calls `limb_on_disintegration`
    /// directly) and CreateContextMenuOptions (UI buttons).
    pub fn limb_start(&mut self, l: usize) {
        // gforce = GetOrAddComponent<GForceMeasureBehaviour>(): adds one (and its id) when the prefab has none; the
        // arena keeps the limb's index.
        if l >= self.gforces.len() {
            assert_eq!(self.gforces.len(), l, "GForceMeasureBehaviour arena out of step with the limbs");
            self.allocate_instance_id();
            self.gforces.push(Default::default());
        }
        self.limbs[l].IsOnFloor = false;
        if self.limbs[l].Broken {
            self.break_bone_internal(l);
        }
        if self.limbs[l].IsDismembered {
            unimplemented!("LimbBehaviour.SynchroniseDismemberment from Start: Destroy(Joint) of a dismembered prefab limb");
        }
        if self.limbs[l].HasJoint {
            let h = self.limbs[l].Joint.unwrap();
            self.limbs[l].originalJointLimits = self.hinges[h].limits;
        }
        for i in 0..self.limbs[l].ConnectedLimbs.len() {
            let other = self.limbs[l].ConnectedLimbs[i];
            let closer = match self.limbs[l].NearestLimbToBrain {
                None => true,
                Some(n) => self.limbs[other].DistanceToBrain < self.limbs[n].DistanceToBrain,
            };
            if closer && self.limbs[other].DistanceToBrain < self.limbs[l].DistanceToBrain {
                self.limbs[l].NearestLimbToBrain = Some(other);
            }
        }
        // goreString.TissueColour = GetOriginalBloodType().Color: the line renderer's colour.
    }

    /// GoreStringBehaviour.Awake only reads components and smoothedColour = TissueColour (rendering).
    /// GoreStringBehaviour.Start. `create_joint_on_start` is the prefab's CreateJointOnStart.
    pub fn gore_string_start(&mut self, l: usize, create_joint_on_start: bool) {
        if create_joint_on_start || self.limbs[l].gore.HadJointWhenSerialised {
            self.gore_string_create_joint(l);
        }
    }

    /// Utils.PickRandom over a list of `count` items: the drawn index.
    pub fn pick_random(&mut self, count: usize) -> Option<usize> {
        if count == 0 {
            return None;
        }
        Some(self.random.range_int(0, count as i32) as usize)
    }

    /// LimbBehaviour.FakeStandUpright
    fn fake_stand_upright(&mut self, l: usize) {
        let p = self.limbs[l].PhysicalBehaviour;
        let t = self.physicals[p].transform;
        let up = t.up();
        if self.physicals[p].rigidbody.bodyType != 0 || !self.nodes[l].IsConnectedToRoot || up[0] * 0.0 + up[1] * -1.0 > 0.0 {
            return;
        }
        let euler = super::dump::euler_z(t.rotation);
        let strength = self.motor_strength(l);
        let mut num = (d(mathf::delta_angle(euler, 0.0))
            * self.limbs[l].FakeUprightForce
            * strength
            * mathf::lerp(self.mass_strength_ratio(l), 1.0, 0.6))
        .f();
        if !num.is_nan() {
            if self.limbs[l].IsAndroid {
                num *= 3.2;
            }
            let m = self.active_pose(self.limbs[l].Person).UprightForceMultiplier;
            let rb = self.physicals[p].rigidbody.clone();
            rb.add_torque(&mut self.world, (d(num) * 1.8 * m).f());
        }
    }

    /// LimbBehaviour.MoveIntoPose(Person.ActivePose)
    fn move_into_pose(&mut self, l: usize, pose: usize, mut speed_modifier: f32, rigidity_multiplier: f32) {
        let person = self.limbs[l].Person;
        let ph = &self.physicals[self.limbs[l].PhysicalBehaviour];
        let mut num = if ph.Temperature <= self.limbs[l].FreezingTemperature + 10.0
            || ph.Temperature >= self.limbs[l].DiscomfortingHeatTemperature
        {
            0.5
        } else {
            1.0
        };
        let p = &self.people[person];
        if p.Poses[pose].State == pose_state::WALKING {
            num *= if p.DesiredWalkingDirection >= 0.0 { 1.0 } else { -1.0 };
        }
        speed_modifier /= mathf::lerp(self.mass_strength_ratio(l), 1.0, 0.9);
        let h = self.limbs[l].Joint.unwrap();
        let joint_angle = self.hinges[h].joint_angle(&self.world);
        let reference = self.hinges[h].reference_angle(&self.world);
        let (time, offset) = (self.time.time, self.limbs[l].randomOffset);
        let p = &mut self.people[person];
        let (angle_offset, global) = (p.AngleOffset, p.GlobalRigidityMultiplier);
        let rp = &mut p.Poses[pose];
        let (speed, rigidity) = (rp.AnimationSpeedMultiplier, rp.Rigidity);
        let i = rp.angle_index(l).unwrap();
        let lp = &mut rp.Angles[i];
        let angle = lp.evaluate_angle(time, offset, (d(speed) * speed_modifier * num).f());
        let modifier = lp.PoseRigidityModifier;
        let value = (d(mathf::delta_angle(joint_angle, (reference + d(angle) * angle_offset).f()))
            * rigidity_multiplier
            * global
            * rigidity
            * (1.0 + d(modifier)))
        .f();
        self.influence_motor_speed(l, value, 0.5);
    }

    /// LimbBehaviour.MoveIntoPoseAt
    fn move_into_pose_at(&mut self, l: usize, pose: usize, time_override: f32, rigidity_multiplier: f32) {
        let person = self.limbs[l].Person;
        let h = self.limbs[l].Joint.unwrap();
        let joint_angle = self.hinges[h].joint_angle(&self.world);
        let reference = self.hinges[h].reference_angle(&self.world);
        let p = &mut self.people[person];
        let (angle_offset, global) = (p.AngleOffset, p.GlobalRigidityMultiplier);
        let rp = &mut p.Poses[pose];
        let rigidity = rp.Rigidity;
        let i = rp.angle_index(l).unwrap();
        let lp = &mut rp.Angles[i];
        let angle = lp.evaluate_angle_at(time_override);
        let value = (d(mathf::delta_angle(joint_angle, (reference + d(angle) * angle_offset).f()))
            * rigidity_multiplier
            * global
            * rigidity
            * (1.0 + d(lp.PoseRigidityModifier)))
        .f();
        self.influence_motor_speed(l, value, 0.5);
    }

    /// LimbBehaviour.ApplyPoseOverrides
    fn apply_pose_overrides(&mut self, l: usize) -> bool {
        let person = self.limbs[l].Person;
        if self.limb_is_capable(l) && self.people[person].IsTouchingFloor && self.active_pose(person).ShouldStumble {
            let num = (self.people[person].BalanceOffset * 1.5).abs();
            let strength = self.motor_strength(l);
            let num2 = mathf::clamp(strength, 0.0, num * 2.0);
            if self.limbs[l].DoBalanceJerk {
                let h = self.limbs[l].Joint.unwrap();
                let joint_angle = self.hinges[h].joint_angle(&self.world);
                let euler = super::dump::euler_z(self.physicals[self.limbs[l].PhysicalBehaviour].transform.rotation);
                let v = (d(mathf::delta_angle(joint_angle, 0.0 - mathf::delta_angle(euler, 0.0))) * 1.0 * self.limbs[l].BalanceMuscleMovement).f();
                self.influence_motor_speed(l, v, 0.5 * num2);
            }
            if self.limbs[l].IsActiveInCurrentPose
                && self.people[person].ShockLevel < 0.5
                && self.limbs[l].DoStumble
                && num > 0.3
                && num < 1.0
            {
                self.limbs[l].stumbleTime += mathf::clamp(self.people[person].BalanceOffset * -0.1, -1.7, 1.7);
                let pose = self.people[person].LinkedPoses[pose_state::STUMBLING as usize];
                let t = self.limbs[l].stumbleTime;
                self.move_into_pose_at(l, pose, t, (d(mathf::pow(num, 1.5)) * 1.3 * num2).f());
                return true;
            }
        }
        false
    }

    /// LimbBehaviour.ManagedFixedUpdate
    pub fn limb_fixed_update(&mut self, l: usize) {
        self.limbs[l].shotHeat = 0.0;
        let p = self.limbs[l].PhysicalBehaviour;
        for i in 0..self.limbs[l].ConnectedLimbs.len() {
            let other = self.limbs[l].ConnectedLimbs[i];
            let op = self.limbs[other].PhysicalBehaviour;
            if !self.physicals[op].isDisintegrated && self.nodes[l].Connections.contains(&Some(other)) {
                self.transfer_energy_fixed_rate(op, p, 0.89);
                self.average_temperature(op, p, 0.01);
            }
        }
        let temperature = self.physicals[p].Temperature;
        let internal = self.limbs[l].InternalTemperature;
        self.physicals[p].Temperature = mathf::lerp(temperature, internal, self.limbs[l].InternalToExternalTempTransferRate);
        let rate = self.limbs[l].ExternalToInternalTempTransferRate;
        self.limbs[l].InternalTemperature =
            mathf::lerp(internal, temperature, if temperature > internal { rate * 0.3 } else { rate });
        let touching = self.is_touching_something(p);
        if touching != self.limbs[l].IsOnFloor {
            self.limbs[l].IsOnFloor = touching;
        }
        let person = self.limbs[l].Person;
        if self.limbs[l].HasBrain {
            if self.circulations[l].BloodFlow < 0.25 && !self.limbs[l].IsAndroid {
                self.people[person].OxygenLevel -= self.time.deltaTime;
            }
            if self.limbs[l].Health < self.limbs[l].InitialHealth * 0.5 {
                for j in 0..self.people[person].Limbs.len() {
                    let other = self.people[person].Limbs[j];
                    if self.limb_is_considered_alive(other) {
                        self.influence_motor_speed(other, 0.0, 0.3);
                    }
                }
            }
            if self.limbs[l].IsZombie {
                self.people[person].BrainDamaged = false;
                self.people[person].Braindead = false;
            } else if self.prefs.BrainDamage
                && !self.limbs[l].IsZombie
                && !self.limbs[l].IsAndroid
                && (self.circulations[l].InternalBleedingIntensity > 0.5 || self.people[person].OxygenLevel <= 0.25)
            {
                let v = self.random.value() > 0.999;
                self.people[person].BrainDamaged |= v;
            }
        }
        if !self.limbs[l].IsZombie && !self.limbs[l].IsAndroid && self.circulations[l].InternalBleedingIntensity > 0.2 {
            let ib = self.circulations[l].InternalBleedingIntensity;
            match self.limbs[l].RoughClassification {
                HEAD => {
                    let damage = (if self.random.value() as f64 > 5.0 / ib as f64 { 20.0 } else { 0.0004 }) * ib;
                    self.limb_damage(l, damage);
                    if self.circulations[l].InternalBleedingIntensity > 0.0 {
                        self.people[person].OxygenLevel = (d(self.people[person].OxygenLevel) - d(0.0001) * ib).f();
                        self.people[person].Consciousness = (d(self.people[person].Consciousness) - d(0.0001) * ib).f();
                    }
                }
                TORSO => self.limb_damage(l, 0.0015 * ib),
                LEGS | ARMS => {
                    self.limbs[l].Numbness = (d(self.limbs[l].Numbness) + d(0.2) * ib).f();
                    self.limb_damage(l, 0.001 * ib);
                }
                _ => {}
            }
            if self.limb_is_considered_alive(l) {
                if self.random.value() > 0.9999 {
                    self.people[person].Consciousness = (d(self.people[person].Consciousness) - d(0.001) * ib).f();
                }
                if self.random.value() as f64 > 0.99 {
                    let i = (d(self.random.value()) * self.circulations[l].InternalBleedingIntensity * 60.0f32).f();
                    self.person_wince(person, i);
                }
            }
        }
        self.calculate_joint_stress(l);
        if (self.limbs[l].InternalTemperature - self.limbs[l].BodyTemperature).abs() >= 3.0 {
            self.limbs[l].Numbness = (d(self.limbs[l].Numbness) + d(0.001) * self.random.value()).f();
            let damage = 0.005 * self.random.value();
            self.limb_damage(l, damage);
        }
        let limb = &self.limbs[l];
        if limb.InternalTemperature >= limb.DiscomfortingHeatTemperature || limb.InternalTemperature <= limb.BodyTemperature - 3.0 {
            if limb.InternalTemperature >= 100.0 {
                self.heal_bleeding(l);
            }
            if self.limbs[l].HasBrain && !self.limb_is_paralysed(l) {
                if self.limbs[l].InternalTemperature > self.limbs[l].BodyTemperature {
                    self.people[person].Consciousness *= 0.9995;
                    self.add_pain(person, 20.0);
                } else {
                    self.people[person].Consciousness *= 0.999;
                }
            }
        }
        if self.physicals[p].Charge() > mathf::EPSILON && !self.limbs[l].IsZombie {
            let damage = self.physicals[p].Charge() * 0.01;
            self.limb_damage(l, damage);
        }
        let threshold = self.properties[self.physicals[p].Properties].BurningTemperatureThreshold;
        if self.physicals[p].Temperature > threshold {
            self.limb_wince(l, 0.1);
            let damage = (d(0.0015) + (d(self.physicals[p].Temperature) - threshold) / 1000.0f32).f();
            self.limb_damage(l, damage);
        } else if self.physicals[p].Temperature < self.limbs[l].FreezingTemperature {
            if self.skins[l].acidProgress < 0.5 + self.limbs[l].randomOffset / 90000.0 {
                { let v = self.skins[l].acidProgress + 0.0005; self.skins[l].set_acid(v); }
            }
            self.limb_damage(l, 0.05);
        }
        if self.physicals[p].Wetness > 0.25 && self.limbs[l].IsAndroid {
            let c = self.physicals[p].Charge() + 0.5;
            self.physicals[p].charge = c;
        }
        if self.has_circulation(l) {
            let limb = &self.limbs[l];
            let mut num = if limb.InternalTemperature <= limb.BodyTemperature { 0.05 } else { 0.025 };
            if self.physicals[p].Temperature < limb.FreezingTemperature {
                num *= 0.03;
            }
            let v = mathf::lerp(limb.InternalTemperature, limb.BodyTemperature, num * limb.BodyHeatProductionFactor);
            self.limbs[l].InternalTemperature = v;
        }
        if self.limb_is_considered_alive(l) {
            if self.physicals[p].OnFire {
                let fdt = self.time.fixedDeltaTime;
                { let v = self.skins[l].acidProgress + fdt * 0.01; self.skins[l].set_acid(v); }
                self.physicals[p].BurnProgress = (d(self.physicals[p].BurnProgress) + d(fdt) * 0.01f32).f();
                let z: f32 = if self.limbs[l].IsZombie { 0.01 } else { 1.0 };
                self.limbs[l].Health = (d(self.limbs[l].Health) - d(self.time.deltaTime) * 0.5f32 * z).f();
                if !self.limbs[l].IsZombie && !self.limb_is_paralysed(l) {
                    if self.prefs.StopAnimationOnDamage {
                        self.people[person].OverridePoseIndex = -1;
                    }
                    let pain = self.physicals[p].burnIntensity * self.time.deltaTime;
                    self.add_pain(person, pain);
                }
            }
            if self.prefs.AutoHealWounds && self.limbs[l].RegenerateBurnProgressSpeed > mathf::EPSILON && self.physicals[p].BurnProgress > 0.0 {
                let ph = &mut self.physicals[p];
                ph.BurnProgress = (d(ph.BurnProgress) - d(self.time.fixedDeltaTime) * self.limbs[l].RegenerateBurnProgressSpeed * 0.001f32).f();
                ph.BurnProgress = mathf::max(0.0, ph.BurnProgress);
            }
        }
        let adrenaline = self.people[person].AdrenalineLevel;
        if self.circulations[l].BloodFlow > mathf::max(0.25, 0.9 - mathf::clamp01(adrenaline))
            && self.people[person].IsTouchingFloor
            && self.active_pose(person).ShouldStandUpright
            && self.limb_is_capable(l)
            && (!self.limbs[l].HasBrain || !self.limbs[l].IsDismembered)
        {
            if self.limbs[l].FakeUprightForce > 0.001 {
                self.fake_stand_upright(l);
            }
            if self.physicals[p].rigidbody.bodyType == 0 {
                let drag = self.active_pose(person).DragInfluence;
                let rb = self.physicals[p].rigidbody.clone();
                let av = rb.angular_velocity(&self.world) * mathf::lerp(1.0, 0.92, drag);
                rb.set_angular_velocity(&mut self.world, av);
                let v = rb.velocity(&self.world);
                let k = mathf::lerp(1.0, 0.94, drag);
                rb.set_velocity(&mut self.world, [v[0] * k, v[1] * k]);
            }
        }
        if self.limbs[l].HasJoint {
            if self.limbs[l].Frozen || self.physicals[p].Temperature <= self.limbs[l].FreezingTemperature {
                self.set_motor_strength(l, 10.0);
                self.influence_motor_speed(l, 0.0, 1.0);
            } else {
                if self.limb_is_considered_alive(l) && self.physicals[p].Temperature <= self.limbs[l].BodyTemperature - 15.0 {
                    let v = self.random.range_int(-45, 45) as f32;
                    self.influence_motor_speed(l, v, 0.5);
                }
                self.set_motor_strength_to_muscle_strength(l);
                if !self.apply_pose_overrides(l) && self.limbs[l].IsActiveInCurrentPose && self.limb_is_considered_alive(l) {
                    let pose = self.people[person].ActivePose.unwrap();
                    self.move_into_pose(l, pose, 1.0, 1.0);
                }
            }
            if self.limb_is_considered_alive(l) && self.physicals[p].Charge() > mathf::EPSILON {
                let v = -50.0 * self.people[person].localScale[0];
                let i = self.physicals[p].Charge() * 0.5;
                self.influence_motor_speed(l, v, i);
            }
            if !self.limbs[l].IsZombie || !self.limb_is_considered_alive(l) {
                let r = self.skins[l].rottenProgress;
                self.influence_motor_speed(l, 0.0, r);
            }
        }
        let limb = &self.limbs[l];
        if limb.ImmuneToDamage
            || self.physicals[p].rigidbody.bodyType != 0
            || limb.SpeciesIdentity != "Human"
            || !(limb.GForcePassoutThreshold > mathf::EPSILON)
        {
            return;
        }
        let sqr = mathf::sqr_magnitude(self.gforces[l].sustainedAcceleration);
        if sqr > limb.GForcePassoutThreshold * limb.GForcePassoutThreshold {
            if limb.HasBrain {
                self.people[person].Consciousness *= 0.5;
            }
            if sqr > self.limbs[l].GForceDamageThreshold * self.limbs[l].GForceDamageThreshold {
                self.limb_damage(l, sqr / 250.0);
            }
        }
    }

    /// LimbBehaviour.ManagedUpdate
    pub fn limb_update(&mut self, l: usize) {
        self.set_joint_fragility(l);
        let dt = self.time.deltaTime;
        let flag = self.limbs[l].Health > mathf::EPSILON;
        let person = self.limbs[l].Person;
        if self.limbs[l].HasBrain {
            if self.people[person].Braindead {
                let p = &mut self.people[person];
                p.Consciousness = 0.0;
                p.ShockLevel = 0.0;
                p.PainLevel = 0.0;
                self.limbs[l].Health = 0.0;
            } else {
                self.people[person].Braindead = !flag;
            }
        }
        let limb = &mut self.limbs[l];
        if flag && limb.Health < limb.InitialHealth {
            limb.Health = (d(limb.Health) + d(limb.RegenerationSpeed) * dt).f();
        }
        self.handle_drowning(l);
        self.limbs[l].Numbness = (d(self.limbs[l].Numbness) - d(dt) / 20.0f32).f();
        if !self.nodes[l].IsConnectedToRoot && self.limbs[l].Health > 0.0 {
            let limb = &mut self.limbs[l];
            limb.Health = (d(limb.Health) - d(dt) / 20.0f32 * limb.InitialHealth).f();
        }
        if let Some(g) = self.limbs[l].GripBehaviour {
            if self.grips[g].isHolding && self.prefs.DropOnDeath && !self.limb_is_considered_alive(l) {
                // ponytail: GripBehaviour.DropObject arrives with holding.
                unimplemented!("GripBehaviour.DropObject");
            }
        }
        let p = self.limbs[l].PhysicalBehaviour;
        if self.physicals[p].isDisintegrated {
            self.limbs[l].RegenerationSpeed = 0.0;
            self.limbs[l].Health = 0.0;
        }
        let limb = &self.limbs[l];
        if limb.HasLungs && limb.LungsPunctured && self.limb_is_considered_alive(l) && !limb.IsZombie && !limb.IsAndroid {
            self.add_pain(person, 2.0);
            self.people[person].OxygenLevel = (d(self.people[person].OxygenLevel) - d(dt) * 0.5f32).f();
        }
        if self.physicals[p].stabWoundCount != 0 {
            let mut i = 0;
            while i < self.physicals[p].beingStabbedBy.len() {
                let stabber = self.physicals[p].beingStabbedBy[i];
                i += 1;
                if !self.physicals[stabber].destroyed && self.physicals[stabber].StabCausesWound && self.get_relative_stab_speed(stabber, p) > 0.05 {
                    let point = self.get_global_stab_point(stabber, p).expect("beingStabbedBy without a penetration");
                    let fallback = self.original_blood_color(l);
                    let colour = self.get_computed_color(l, fallback);
                    // SendMessage skips an inactive stabber.
                    if self.physicals[stabber].active {
                        let ty = self.decal_descriptor(&self.limbs[l].BloodDecal.clone());
                        self.physical_decal(stabber, ty, point, 1.0, colour);
                    }
                }
            }
        }
        let limb = &mut self.limbs[l];
        if limb.Health > mathf::EPSILON && limb.previousHealth <= mathf::EPSILON {
            limb.KillShotParticlesEmitted = false;
        }
        limb.previousHealth = limb.Health;
    }

    /// LimbBehaviour.HandleDrowning
    fn handle_drowning(&mut self, l: usize) {
        if !self.nodes[l].IsConnectedToRoot || self.limbs[l].IsAndroid {
            return;
        }
        let dt = self.time.deltaTime;
        let person = self.limbs[l].Person;
        if self.people[person].OxygenLevel < 0.05 && !self.limbs[l].IsZombie {
            self.limb_damage(l, dt * 4.0);
        }
        if !self.limbs[l].HasBrain || self.physicals[self.limbs[l].PhysicalBehaviour].Wetness < 0.5 {
            return;
        }
        self.people[person].OxygenLevel = (d(self.people[person].OxygenLevel) - d(dt) / 40.0f32).f();
        if self.people[person].OxygenLevel < 0.9 {
            self.people[person].Consciousness = (d(self.people[person].Consciousness) - d(dt) / 40.0f32 * 0.5f32).f();
            if !self.limbs[l].IsZombie {
                self.add_pain(person, dt * 0.9);
            }
        }
    }
}

impl Game {
    /// SkinMaterialHandler.AddDamagePoint
    pub fn skin_add_damage_point(&mut self, l: usize, ty: i32, global_position: [f32; 2], intensity: f32) {
        if self.skins[l].currentDamagePointCount < 128 && !(intensity <= mathf::EPSILON) {
            let intensity = intensity * self.skins[l].intensityMultiplier;
            self.skin_internal_add_damage_point(l, ty, global_position, intensity);
            for a in self.skins[l].adjacentLimbs.clone() {
                self.skin_internal_add_damage_point(a, ty, global_position, intensity);
            }
        }
    }

    /// SkinMaterialHandler.InternalAddDamagePoint (Sync only uploads to the material).
    fn skin_internal_add_damage_point(&mut self, l: usize, ty: i32, global_position: [f32; 2], intensity: f32) {
        use super::misc::damage_type::*;
        if !(self.skins[l].currentDamagePointCount < 128 && !(intensity <= mathf::EPSILON)) {
            return;
        }
        let intensity = mathf::clamp(intensity, 0.0, 100.0);
        let local = self.inverse_transform_point(self.limbs[l].PhysicalBehaviour, [global_position[0], global_position[1], 0.0]);
        let point_intensity = intensity * 0.1;
        let min_distance = match ty {
            BLUNT => 0.1 * point_intensity,
            BULLET => 0.01 * mathf::min(point_intensity, 5.0),
            STAB => 0.02 * point_intensity,
            DISMEMBERMENT => 0.05,
            BURN => 0.015,
            _ => 0.0,
        };
        let skin = &mut self.skins[l];
        // GetNearEntry
        let num = min_distance * min_distance;
        let mut near = None;
        for index in 0..skin.currentDamagePointCount as usize {
            let p = skin.damagePoints[index];
            let dz = local[2] - 0.0;
            let sqr = (d(local[0] - p[0]) * (local[0] - p[0]) + d(local[1] - p[1]) * (local[1] - p[1]) + d(dz) * dz).f();
            if (mathf::floor(p[3]) as i32 == ty || p[2] <= 0.01) && sqr <= num {
                skin.damagePoints[index][3] = ty as f32;
                near = Some(index);
                break;
            }
        }
        match near {
            Some(index) => skin.damagePoints[index][2] = mathf::max(skin.damagePoints[index][2], point_intensity),
            None => {
                let n = skin.currentDamagePointCount as usize;
                skin.damagePointTimeStamps[n] = self.time.time;
                skin.damagePoints[n] = [local[0], local[1], point_intensity, ty as f32];
                skin.currentDamagePointCount += 1;
            }
        }
    }

    /// LimbBehaviour.OnCollisionEnter2D
    pub fn limb_on_collision_enter(&mut self, l: usize, c: &ScriptCollision) {
        use super::misc::damage_type::*;
        let mut buffer = self.limb_contact_buffer;
        let contacts = get_contacts(c, &mut buffer);
        self.limb_contact_buffer = buffer;
        let mut num = (d(get_average_impulse_remove_outliers(&buffer, contacts)) / self.mass_strength_ratio(l) * self.prefs.FragilityMultiplier * 0.8f32).f();
        let normal = [buffer[0].normal.x, buffer[0].normal.y];
        let point = [buffer[0].point.x, buffer[0].point.y];
        if self.limbs[l].IsAndroid {
            num *= 0.1;
        }
        if let Some(v) = self.collision_physical(c) {
            let value = &self.physicals[v];
            if value.SimulateTemperature && value.Temperature >= 70.0 && !self.limbs[l].IsAndroid {
                let temperature = value.Temperature;
                self.limb_damage(l, temperature / 140.0);
                self.skin_add_damage_point(l, BURN, point, temperature * 0.01);
                if self.limb_is_considered_alive(l) && !self.limb_is_paralysed(l) {
                    self.add_pain(self.limbs[l].Person, 1.0);
                }
                self.limb_wince(l, 150.0);
                if temperature >= 100.0 {
                    self.heal_bleeding(l);
                }
            }
        }
        if self.limbs[l].HasBrain && num > 0.6 && self.random.value() as f64 > 0.8 {
            self.circulations[l].InternalBleedingIntensity += num;
        }
        if num < 2.0 {
            return;
        }
        self.limbs[l].BruiseCount = self.limbs[l].BruiseCount.wrapping_add(1);
        self.propagate_impact_damage(l, num, normal, point, 0, l);
        if !(num < 1.0) && !self.limbs[l].IsAndroid && !(self.amount_of_blood(l) < 0.2) {
            if self.random.value() > 0.8 {
                self.cut(l, point, normal);
            }
            if !(num < 3.0) || !(self.limbs[l].Health > self.limbs[l].InitialHealth * 0.2) {
                let p = self.limbs[l].PhysicalBehaviour;
                self.create_impact_effect(p, point, normal, mathf::clamp(num / 4.0, 1.0, 2.0));
                let fallback = self.original_blood_color(l);
                let colour = self.get_computed_color(l, fallback);
                if let Some(other) = self.collision_physical(c) {
                    let ty = self.decal_descriptor(&self.limbs[l].BloodDecal.clone());
                    self.physical_decal(other, ty, point, 1.0, colour);
                }
            }
        }
    }

    /// LimbBehaviour.PropagateImpactDamage
    fn propagate_impact_damage(&mut self, l: usize, impulse: f32, direction: [f32; 2], pos: [f32; 2], iteration: i32, origin: usize) {
        self.act_on_impact(l, impulse, [pos[0], pos[1], 0.0]);
        if iteration >= 8 {
            return;
        }
        for i in 0..self.limbs[l].ConnectedLimbs.len() {
            let other = self.limbs[l].ConnectedLimbs[i];
            if other == origin {
                continue;
            }
            let (a, b) = (self.physicals[self.limbs[other].PhysicalBehaviour].transform.position, self.physicals[self.limbs[l].PhysicalBehaviour].transform.position);
            let num = dot2([a[0] - b[0], a[1] - b[1]], direction);
            if num > 0.0 {
                self.propagate_impact_damage(other, (d(num) * impulse * 0.9f32).f(), direction, pos, iteration + 1, l);
            }
        }
    }

    /// LimbBehaviour.OnCollisionStay2D
    pub fn limb_on_collision_stay(&mut self, l: usize, c: &ScriptCollision) {
        use super::misc::damage_type::*;
        if self.limbs[l].IsAndroid {
            return;
        }
        let mut buffer = self.limb_contact_buffer;
        let contacts = get_contacts(c, &mut buffer);
        self.limb_contact_buffer = buffer;
        let first_valid_contact = get_first_valid_contact(&buffer, contacts);
        let mass_strength_ratio = self.mass_strength_ratio(l);
        let sqr = mathf::sqr_magnitude([c.relative_velocity.x, c.relative_velocity.y]);
        if d(sqr) * self.prefs.FragilityMultiplier / mass_strength_ratio > d(self.limbs[l].FrictionBurnWoundMinSpeedSqrd) {
            self.limb_damage(l, 1.0);
            self.skin_add_damage_point(l, BURN, [first_valid_contact.point.x, first_valid_contact.point.y], 2.0);
        }
        let p = self.limbs[l].PhysicalBehaviour;
        if self.prefs.LimbCrushing && self.physicals[p].rigidbody.bodyType != 2 {
            let threshold = d(mathf::max(10.0, self.limbs[l].BreakingThreshold))
                * mathf::lerp(self.positionIterations as f32 / 16.0, 1.0, 0.2);
            let force = d(get_min_impulse(&buffer, contacts)) * self.prefs.CrushForceMultiplier / mass_strength_ratio * self.prefs.FragilityMultiplier;
            if !(force < threshold) {
                let fallback = self.original_blood_color(l);
                let colour = self.get_computed_color(l, fallback);
                if let Some(other) = self.collision_physical(c) {
                    let ty = self.decal_descriptor(&self.limbs[l].BloodDecal.clone());
                    let at = self.physicals[p].transform.position;
                    self.physical_decal(other, ty, [at[0], at[1]], 1.0, colour);
                }
                self.limb_crush(l);
            }
        }
    }

    /// LimbBehaviour.ActOnImpact
    pub fn act_on_impact(&mut self, l: usize, impulse: f32, global_position: [f32; 3]) {
        use super::misc::damage_type::*;
        if self.limbs[l].ImmuneToDamage {
            return;
        }
        let limb = &self.limbs[l];
        let num = (d(limb.BreakingThreshold) * self.mass_strength_ratio(l) / limb.ImpactDamageMultiplier).f();
        if impulse > num && self.random.value() > 0.2 {
            self.break_bone(l);
        }
        let num2 = mathf::max(1.0, self.limbs[l].Vitality);
        if d(impulse) > d(num) * 0.5f32 / num2 && d(self.random.value()) > d(0.8) / num2 {
            self.circulations[l].InternalBleedingIntensity += mathf::clamp(impulse * num2, 0.0, 1.0);
        }
        let person = self.limbs[l].Person;
        if !self.limbs[l].IsAndroid {
            if self.prefs.BrainDamage && !self.limbs[l].IsZombie && self.limbs[l].HasBrain && self.random.value() > 0.991 && impulse > 4.0 {
                self.people[person].BrainDamaged = true;
            } else {
                let damage = (d(impulse) * impulse * impulse * 2.8f32 * self.limbs[l].ImpactDamageMultiplier).f();
                self.limb_damage(l, damage);
            }
        }
        self.skin_add_damage_point(l, BLUNT, [global_position[0], global_position[1]], impulse * 4.0);
        let num3 = (d(impulse) * (d(self.limbs[l].Vitality) + 1.0f32) * 0.25f32).f();
        if !self.people[person].BrainDamaged && self.limbs[l].HasBrain && num3 > 1.0 && !self.limbs[l].IsAndroid && self.random.value() > 0.5 {
            let r = self.random.value();
            self.people[person].Consciousness = (d(self.people[person].Consciousness) * (d(r) * 0.8f32)).f();
        }
        if self.limb_is_considered_alive(l) && num3 > 0.2 && !self.limbs[l].IsZombie {
            self.people[person].ShockLevel = (d(self.people[person].ShockLevel) + d(num3) * 0.04f32).f();
        }
    }

    /// LimbBehaviour.ShatterProcedurally(ShatterFlags.All): Random.value is drawn first.
    fn limb_shatter_procedurally(&mut self, l: usize) {
        let v = self.random.value();
        if !(self.limbs[l].ShatteredObjectChance < v) && self.prefs.ProceduralFragments {
            unimplemented!("LimbBehaviour.ShatterProcedurally: ShatteredObjectGenerator.Generate (procedural fragments)");
        }
    }

    /// LimbBehaviour.Crush
    pub fn limb_crush(&mut self, l: usize) {
        let p = self.limbs[l].PhysicalBehaviour;
        if self.physicals[p].isDisintegrated || self.limbs[l].ImmuneToDamage {
            return;
        }
        let person = self.limbs[l].Person;
        if self.nodes[l].IsConnectedToRoot {
            if self.prefs.StopAnimationOnDamage {
                self.people[person].OverridePoseIndex = -1;
            }
            self.add_pain(person, 25.0);
        }
        if !self.prefs.GorelessMode {
            // Instantiate(BloodExplosionPrefab, transform.position, identity). Awake in hierarchy order: the
            // root's ExplosionSoundBehviour (audio source registration), then the ActivateIfFancyEffects of
            // Debris 2, extraDecals and chunks each draw `Chance(1) > Random.value` before reading FancyEffects.
            // The particle systems draw from their own generators, not UnityEngine.Random.
            let position = self.physicals[p].transform.position;
            let _debris2 = 1.0 > self.random.value() && self.prefs.FancyEffects;
            let extra_decals = 1.0 > self.random.value() && self.prefs.FancyEffects;
            let _chunks = 1.0 > self.random.value() && self.prefs.FancyEffects;
            self.pending_starts.push(super::misc::PendingStart::BloodExplosionSound);
            // extraDecals: a circle of 25..32 drops from the explosion (its parent at Start).
            let splat = extra_decals.then(|| {
                self.splats.push(super::misc::FancyBloodSplat {
                    UnparentAtStart: true,
                    DirectionMode: 2,
                    Continuous: false,
                    Direction: [0.0, 1.0],
                    SpawningChance: 0.1,
                    AmountRange: [25.0, 32.0],
                    AngleRandomisation: 11.25,
                    LifetimeInSeconds: [1.0, 2.0],
                    EjectionForce: [0.0, 5.0],
                    DecalColourMultiplier: [f32::from_bits(0x3f169697), 0.0, 0.0, 1.0],
                    Size: 0.3,
                    DestroyWhenNoParticles: true,
                    TriggerOnCollide: false,
                    Decal: "Sparse_blood".into(),
                    particles: Default::default(),
                    parent_position: position,
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    started: false,
                    destroyed: false,
                    CollisionLayers: 10752,
                    attached: None,
                    parent_person: None,
                });
                let s = self.splats.len() - 1;
                self.pending_starts.push(super::misc::PendingStart::Splat(s));
                s
            });
            // DeleteAfterTime (5 s) only removes the explosion's own objects.
            if !self.limbs[l].IsAndroid {
                // BloodExplosionBehaviour.SetColor: the particles' start colour and the splat's decal colour.
                let fallback = self.original_blood_color(l);
                let color = self.get_computed_color(l, fallback);
                if let Some(s) = splat {
                    self.splats[s].DecalColourMultiplier = color;
                }
            }
            self.limb_shatter_procedurally(l);
        }
        if !self.physicals[p].isDisintegrated {
            self.physical_disintegrate(p);
        }
    }
}

impl Game {
    /// SendMessage("OnJointBreak2D", joint) for hinge `h`, to the limb GameObject that owns it.
    /// Receivers in component order (Person.prefab, every limb): PhysicalBehaviour, LimbBehaviour,
    /// GoreStringBehaviour; disabled behaviours receive it too. The caller destroys the joint
    /// afterwards (Hinge.destroyed), so everything here still sees it intact.
    pub fn joint_break_messages(&mut self, h: usize) {
        let Some(l) = self.limbs.iter().position(|limb| limb.Joint == Some(h)) else { return };
        // PhysicalBehaviour.OnJointBreak2D: only matches `penetration.Joint` (a SliderJoint2D), never a hinge.
        // LimbBehaviour.OnJointBreak2D
        self.limb_on_joint_break(l, h);
        // GoreStringBehaviour.OnJointBreak2D: acts only when `joint == Joint` (its own SpringJoint2D), never a hinge.
    }

    /// ConnectedNodeBehaviour.SetToNull
    fn node_set_to_null(&mut self, node: usize, other: usize) {
        if let Some(c) = self.nodes[node].Connections.iter_mut().find(|c| **c == Some(other)) {
            *c = None;
        }
    }

    /// LimbBehaviour.OnJointBreak2D (joint == Joint)
    fn limb_on_joint_break(&mut self, l: usize, h: usize) {
        let component = self.hinges[h].connected;
        self.circulation_on_joint_break(l);
        if self.limbs[l].IsLethalToBreak {
            self.limbs[l].Health = 0.0;
        }
        if self.nodes[l].IsConnectedToRoot && !self.limb_is_paralysed(l) {
            let person = self.limbs[l].Person;
            self.add_pain(person, 15.0);
        }
        // The connected body always has a ConnectedNodeBehaviour (nodes are indexed by limb).
        self.node_set_to_null(component, l);
        self.node_set_to_null(l, component);
        if let Some(i) = self.limbs[l].ConnectedLimbs.iter().position(|&x| x == component) {
            self.limbs[l].ConnectedLimbs.remove(i);
        }
        let p = self.limbs[l].PhysicalBehaviour;
        if !self.prefs.GorelessMode
            && self.prefs.DismembermentLooseTissue
            && self.limbs[l].goreString
            && self.random.value() > 0.5
            && self.limbs[l].goreStringEnabled
            && !self.limbs[l].goreStringJoint
            && self.skins[l].acidProgress < 0.7
            && self.physicals[p].BurnProgress < 0.8
        {
            self.gore_string_create_joint(l);
        }
        self.limbs[l].IsDismembered = true;
        let vector = self.transform_point(p, self.hinges[h].anchor);
        // CreateImpactEffect: effect prefab only.
        self.skin_add_damage_point(l, super::misc::damage_type::DISMEMBERMENT, vector, 25.0);
        let clips = self.people[self.limbs[l].Person].DismembermentClips;
        // PlayClipOnce(PickRandom()): audio, the draw stays.
        self.pick_random(clips);
        self.synchronise_dismemberment(l, component);
    }

    /// LimbBehaviour.SynchroniseDismemberment(connectedLimb) (IsDismembered is true here)
    fn synchronise_dismemberment(&mut self, l: usize, connected: usize) {
        // Destroy(Joint): the joint is already being destroyed by the break.
        self.limbs[l].HasJoint = false;
        let remove_first = |v: &mut Vec<usize>, x: usize| {
            if let Some(i) = v.iter().position(|&y| y == x) {
                v.remove(i);
            }
        };
        let mut i = 0;
        while i < self.limbs[l].ConnectedLimbs.len() {
            let o = self.limbs[l].ConnectedLimbs[i];
            let attached = self.limbs[o].HasJoint && {
                let oh = self.limbs[o].Joint.expect("MissingReferenceException: HasJoint without a Joint");
                assert!(!self.hinges[oh].destroyed, "MissingReferenceException: connected limb's Joint was destroyed");
                self.hinges[oh].connected == l
            };
            if !attached {
                remove_first(&mut self.limbs[o].ConnectedLimbs, l);
                // `Source == this` compares a CirculationBehaviour with a LimbBehaviour: never equal.
            }
            i += 1;
        }
        remove_first(&mut self.limbs[connected].ConnectedLimbs, l);
        if let Some(src) = self.circulations[l].Source {
            remove_first(&mut self.limbs[l].ConnectedLimbs, src);
            remove_first(&mut self.limbs[src].ConnectedLimbs, l);
        }
    }
}

impl Game {
    /// AddComponent<SpringJoint2D> and the setters GoreStringBehaviour.CreateJoint runs, ending with
    /// connectedBody = other. Each ReCreate (anchor, connectedAnchor, connectedBody) destroys the b2 joint and
    /// builds it again; the ones before connectedBody hang from the scene's fixtureless ground body, so all they
    /// leave behind is limb `l` woken by b2World::DestroyJoint. The last one is SpringJoint2D::Create with
    /// autoconfiguration already complete: anchors scaled by each body's lossy scale, the clamped distance,
    /// frequency and damping ratio, breakForce infinite (so out of the break-limit list).
    fn create_spring_joint(&mut self, l: usize, other: usize, anchor: [f32; 2], connected_anchor: [f32; 2], distance: f32, frequency: f32, damping_ratio: f32) {
        let (pa, pb) = (self.limbs[l].PhysicalBehaviour, self.limbs[other].PhysicalBehaviour);
        let (sa, sb) = (self.lossy_scale(pa), self.lossy_scale(pb));
        let (a, b) = (self.physicals[pa].rigidbody.body, self.physicals[pb].rigidbody.body);
        // SpringJoint2D::SetDistance / SetFrequency / SetDampingRatio.
        let clamp = |v: f32, lo: f32, hi: f32| if lo <= v && v <= hi { v } else if lo <= v { hi } else { lo };
        let def = crate::b2::distance_joint::DistanceDef {
            local_anchor_a: crate::b2::math::Vec2::new(sa[0] * anchor[0], sa[1] * anchor[1]),
            local_anchor_b: crate::b2::math::Vec2::new(sb[0] * connected_anchor[0], sb[1] * connected_anchor[1]),
            length: clamp(distance, f32::from_bits(0x3ba3_d70a), 1_000_000.0),
            frequency_hz: clamp(frequency, 0.0, 1_000_000.0),
            damping_ratio: clamp(damping_ratio, 0.0, 1.0),
        };
        self.world.wake_body(a);
        let id = self.world.create_joint(def, a, b, false);
        self.world.wake_joined(a);
        self.world.wake_joined(b);
        self.limbs[l].gore.joint = Some(id);
    }

    /// Object.Destroy(Joint): DestroyObjectDelayed queues it on the DelayedCallManager at
    /// min(fixedTimeAsDouble, the frame's time); a second Destroy finds nothing left to do.
    fn destroy_spring_joint(&mut self, l: usize) {
        let at = self.time.fixedTimeAsDouble.min(self.time.timeAsDouble);
        self.limbs[l].gore.destroy_at.get_or_insert(at);
    }

    /// SpringJoint2D destruction (Joint2D::Cleanup): the b2 joint and its break-limit entry go; `Joint` reads null.
    pub fn remove_spring_joint(&mut self, l: usize) {
        if let Some(id) = self.limbs[l].gore.joint.take() {
            self.world.destroy_joint(id);
        }
        self.joint_break_limits.retain(|&k| k != super::BreakJoint::Spring(l));
        self.limbs[l].goreStringJoint = false;
        self.limbs[l].gore.destroy_at = None;
    }

    /// Joint.breakForce = v on limb `l`'s SpringJoint2D (Joint2D::SetBreakForce).
    fn set_spring_break_force(&mut self, l: usize, v: f32) {
        let v = if 0.0 <= v && v <= f32::INFINITY { v } else if 0.0 <= v { f32::INFINITY } else { 0.0 };
        self.limbs[l].gore.breakForce = v;
        self.add_break_limits_update(super::BreakJoint::Spring(l), v, f32::INFINITY);
    }

    /// GoreStringBehaviour.CreateJoint
    pub fn gore_string_create_joint(&mut self, l: usize) {
        if self.limbs[l].goreStringJoint {
            // Debug.LogError("Tried to create gore string joint again ...")
            return;
        }
        let Some(other) = self.limbs[l].gore.Other else {
            panic!("Tried to create gore string joint without a rigidbody on either one or both ends");
        };
        self.limbs[l].gore.HadJointWhenSerialised = true;
        let g = &self.limbs[l].gore;
        // LineSegment.GetMidpoint: (A + B) * 0.5f
        let anchor = [(g.OriginLine[0] + g.OriginLine[2]) * 0.5, (g.OriginLine[1] + g.OriginLine[3]) * 0.5];
        let connected_anchor = [(g.OtherLine[0] + g.OtherLine[2]) * 0.5, (g.OtherLine[1] + g.OtherLine[3]) * 0.5];
        let (freq, damping, invincibility) = (g.JointFreq, g.JointDampening, g.InvincibilityTime);
        self.create_spring_joint(l, other, anchor, connected_anchor, 0.05, freq, damping);
        self.limbs[l].goreStringJoint = true;
        self.limbs[l].gore.breakForce = f32::INFINITY;
        // StartCoroutine(DelayCoroutine(InvincibilityTime, SetBreakForce)): WaitForSeconds.
        self.limbs[l].gore.break_at = Some(self.time.timeAsDouble + invincibility as f64);
        let num = self.random.range_int(5, 14);
        let mut data = Vec::with_capacity(num as usize);
        data.push(TissueString { Pos1: 0.5, Pos2: 0.5, Strength: f32::INFINITY, Thickness: 2.0, ThicknessRatio: 1.0 });
        for _ in 1..num {
            // Constructor arguments evaluate in order: pos1, pos2, thickness, strength, thicknessRatio.
            let pos1 = self.random.value();
            let pos2 = self.random.value();
            let thickness = self.random.range(0.5, 2.0);
            let strength = self.random.range(0.1, 1.0);
            let ratio = self.random.range(0.5, 2.0);
            data.push(TissueString { Pos1: pos1, Pos2: pos2, Strength: strength, Thickness: thickness, ThicknessRatio: ratio });
        }
        // The Lines renderer setup is presentation.
        self.limbs[l].gore.lineData = Some(data);
    }

    /// LimbBehaviour.PhysicalBehaviour_OnDisintegration
    pub fn limb_on_disintegration(&mut self, l: usize) {
        if self.limb_is_considered_alive(l) && !self.limb_is_paralysed(l) {
            self.add_pain(self.limbs[l].Person, 25.0);
        }
        let position = self.physicals[self.limbs[l].PhysicalBehaviour].transform.position;
        self.skin_add_damage_point(l, super::misc::damage_type::DISMEMBERMENT, [position[0], position[1]], 25.0);
        self.circulation_disintegrate(l);
        for i in 0..self.limbs[l].ConnectedLimbs.len() {
            let other = self.limbs[l].ConnectedLimbs[i];
            if self.node_is_connected_to(l, other) && self.limbs[other].HasJoint {
                let h = self.limbs[other].Joint.unwrap();
                if self.hinges[h].connected == l {
                    self.set_hinge_break_force(h, 0.0);
                }
            }
        }
        if self.limbs[l].goreString {
            self.gore_string_destroy_joint(l);
        }
        self.node_disconnect_from_everything(l);
        if self.limbs[l].HasJoint {
            let h = self.limbs[l].Joint.unwrap();
            self.set_hinge_break_force(h, 0.0);
            self.set_hinge_break_torque(h, 0.0);
        }
    }

    /// GoreStringBehaviour.DestroyJoint
    pub fn gore_string_destroy_joint(&mut self, l: usize) {
        self.limbs[l].gore.HadJointWhenSerialised = false;
        self.limbs[l].gore.lineData = None;
        if self.limbs[l].goreStringJoint {
            self.destroy_spring_joint(l);
        }
    }

    /// GoreStringBehaviour.TryPlaySnapSound: the clip pick draws whether AudioSource or the PhysicalBehaviour plays it.
    fn gore_string_snap_sound(&mut self, l: usize) {
        let n = self.limbs[l].gore.TissueSnapClips;
        if n != 0 {
            self.pick_random(n);
        }
    }

    /// GoreStringBehaviour.OnJointBreak2D for the string's own SpringJoint2D breaking.
    pub fn gore_string_on_joint_break(&mut self, l: usize) {
        self.gore_string_snap_sound(l);
        self.gore_string_destroy_joint(l);
    }

    /// GoreStringBehaviour.Update (the colour smoothing and the line drawing are presentation).
    pub fn gore_string_update(&mut self, l: usize) {
        if !self.limbs[l].goreString || !self.limbs[l].goreStringEnabled || self.limbs[l].gore.lineData.is_none() {
            return;
        }
        let Some(other) = self.limbs[l].gore.Other else { return };
        if !self.limb_active(other) {
            self.gore_string_destroy_joint(l);
            return;
        }
        let g = &self.limbs[l].gore;
        let break_force = g.breakForce;
        let flag = !break_force.is_infinite();
        let anchor = [(g.OriginLine[0] + g.OriginLine[2]) * 0.5, (g.OriginLine[1] + g.OriginLine[3]) * 0.5];
        let connected_anchor = [(g.OtherLine[0] + g.OtherLine[2]) * 0.5, (g.OtherLine[1] + g.OtherLine[3]) * 0.5];
        let a = self.transform_point(self.limbs[l].PhysicalBehaviour, anchor);
        let b = self.transform_point(self.limbs[other].PhysicalBehaviour, connected_anchor);
        let num = mathf::magnitude2([a[0] - b[0], a[1] - b[1]]);
        for i in 0..self.limbs[l].gore.lineData.as_ref().unwrap().len() {
            let t = self.limbs[l].gore.lineData.as_ref().unwrap()[i];
            if flag && t.Thickness > f32::from_bits(1) && ((num / (t.Strength * t.Strength)) as f64) > (break_force as f64) * 0.05 {
                self.gore_string_snap_sound(l);
                self.limbs[l].gore.lineData.as_mut().unwrap()[i].Thickness = 0.0;
            }
        }
    }

    /// DelayedCallManager::Update (180634dd0): the gore strings' due callbacks in time order. The fixed-rate
    /// pass (after the physics step) runs only the delayed Destroys; the dynamic one (after Update) also the
    /// SetBreakForce coroutines.
    // ponytail: callbacks queued during this pass wait for the next (timeStamp); nothing here queues any.
    pub fn gore_string_delayed_calls(&mut self, dynamic: bool) {
        let now = self.time.timeAsDouble;
        // (time, kind: 0 break force, 1 gore string destroy, 2 drag joint destroy, 3 person destroy, limb/person)
        let mut due: Vec<(f64, u8, usize)> = Vec::new();
        for l in 0..self.limbs.len() {
            let g = &self.limbs[l].gore;
            // A deactivated limb's coroutines stopped with it.
            if let Some(t) = g.break_at.filter(|&t| dynamic && t <= now && self.limb_active(l)) {
                due.push((t, 0, l));
            }
            if let Some(t) = g.destroy_at.filter(|&t| t <= now) {
                due.push((t, 1, l));
            }
        }
        due.extend(self.tool_due_destroys(now).into_iter().map(|t| (t, 2, 0)));
        due.extend(self.person_due_destroys(now).into_iter().map(|(t, p)| (t, 3, p)));
        due.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        for (t, kind, l) in due {
            if kind == 3 {
                self.person_run_destroy(l);
            } else if kind == 2 {
                self.tool_run_destroy(t);
            } else if kind == 1 {
                if self.limbs[l].gore.destroy_at.is_some() {
                    self.remove_spring_joint(l);
                }
            } else {
                self.limbs[l].gore.break_at = None;
                if self.limbs[l].goreStringJoint {
                    let v = self.limbs[l].gore.JointBreakThreshold * 1.5;
                    self.set_spring_break_force(l, v);
                }
            }
        }
    }
}
