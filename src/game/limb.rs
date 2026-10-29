//! LimbBehaviour
use super::{d, mathf, person::pose_state, script, Game};

/// LimbBehaviour.BodyPart
pub const HEAD: i32 = 0;
pub const TORSO: i32 = 1;
pub const LEGS: i32 = 2;
pub const ARMS: i32 = 3;

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
        enabled: bool = true,
    }
}

impl Game {
    fn mass_strength_ratio(&self, l: usize) -> f32 {
        let ph = &self.physicals[self.limbs[l].PhysicalBehaviour];
        ph.rigidbody.mass(&self.world) / ph.TrueInitialMass
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
            let value = 60.0 * intensity * (mathf::perlin(self.time.time * 8.0, self.limbs[l].randomOffset) * 2.0 - 1.0);
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
            self.hinges[h].set_break_force(0.0);
            return;
        }
        let limb = &self.limbs[l];
        let mut num = limb.BreakingThreshold
            * 120.0
            * mathf::clamp(1.0 - self.skins[l].rottenProgress, 0.1, 1.0)
            * self.mass_strength_ratio(l);
        num /= self.prefs.FragilityMultiplier;
        if !limb.IsAndroid {
            num *= mathf::clamp(mathf::map_range(-100.0, 0.0, 0.1, 1.0, ph.Temperature), 0.1, 1.0);
        }
        if num.is_infinite() {
            num = f32::MAX;
        }
        let force = num * (if limb.IsAndroid { 80.0 } else { 2.5 });
        self.hinges[h].set_break_force(force);
        self.hinges[h].set_break_torque(num * 5000.0);
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
            self.people[person].ShockLevel += self.random.value() * 5.0;
            let i = self.random.value() * 150.0;
            self.person_wince(person, i);
        }
        if self.limbs[l].IsLethalToBreak {
            let d = self.limbs[l].Health + 1.0;
            self.limb_damage(l, d);
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
                    let d = (if self.random.value() > 5.0 / ib { 20.0 } else { 0.0004 }) * ib;
                    self.limb_damage(l, d);
                    if self.circulations[l].InternalBleedingIntensity > 0.0 {
                        self.people[person].OxygenLevel -= 0.0001 * ib;
                        self.people[person].Consciousness -= 0.0001 * ib;
                    }
                }
                TORSO => self.limb_damage(l, 0.0015 * ib),
                LEGS | ARMS => {
                    self.limbs[l].Numbness += 0.2 * ib;
                    self.limb_damage(l, 0.001 * ib);
                }
                _ => {}
            }
            if self.limb_is_considered_alive(l) {
                if self.random.value() > 0.9999 {
                    self.people[person].Consciousness -= 0.001 * ib;
                }
                if self.random.value() as f64 > 0.99 {
                    let i = self.random.value() * self.circulations[l].InternalBleedingIntensity * 60.0;
                    self.person_wince(person, i);
                }
            }
        }
        self.calculate_joint_stress(l);
        if (self.limbs[l].InternalTemperature - self.limbs[l].BodyTemperature).abs() >= 3.0 {
            self.limbs[l].Numbness += 0.001 * self.random.value();
            let d = 0.005 * self.random.value();
            self.limb_damage(l, d);
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
            let d = self.physicals[p].Charge() * 0.01;
            self.limb_damage(l, d);
        }
        let threshold = self.properties[self.physicals[p].Properties].BurningTemperatureThreshold;
        if self.physicals[p].Temperature > threshold {
            self.limb_wince(l, 0.1);
            let d = 0.0015 + (self.physicals[p].Temperature - threshold) / 1000.0;
            self.limb_damage(l, d);
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
                self.physicals[p].BurnProgress += fdt * 0.01;
                self.limbs[l].Health -= self.time.deltaTime * 0.5 * if self.limbs[l].IsZombie { 0.01 } else { 1.0 };
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
                ph.BurnProgress -= self.time.fixedDeltaTime * self.limbs[l].RegenerateBurnProgressSpeed * 0.001;
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
            limb.Health += limb.RegenerationSpeed * dt;
        }
        self.handle_drowning(l);
        self.limbs[l].Numbness -= dt / 20.0;
        if !self.nodes[l].IsConnectedToRoot && self.limbs[l].Health > 0.0 {
            let limb = &mut self.limbs[l];
            limb.Health -= dt / 20.0 * limb.InitialHealth;
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
            self.people[person].OxygenLevel -= dt * 0.5;
        }
        // ponytail: stab-wound blood decals arrive with stabbing.
        assert!(self.physicals[p].stabWoundCount == 0, "stabbing is not rewritten yet");
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
        self.people[person].OxygenLevel -= dt / 40.0;
        if self.people[person].OxygenLevel < 0.9 {
            self.people[person].Consciousness -= dt / 40.0 * 0.5;
            if !self.limbs[l].IsZombie {
                self.add_pain(person, dt * 0.9);
            }
        }
    }
}
