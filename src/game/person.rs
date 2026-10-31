//! PersonBehaviour and RagdollPose.
use super::{d, mathf, script, Game};
use crate::game_assets::{Curve, CurveCache};

/// PoseState
pub mod pose_state {
    pub const REST: i32 = 0;
    pub const PROTECTIVE: i32 = 1;
    pub const FLAILING: i32 = 2;
    pub const STUMBLING: i32 = 3;
    pub const SWIMMING: i32 = 4;
    pub const WRITHING_IN_PAIN: i32 = 5;
    pub const WALKING: i32 = 6;
    pub const SITTING: i32 = 7;
    pub const FLAT: i32 = 8;
    pub const BRAIN_DAMAGE: i32 = 9;
    pub const COUNT: usize = 10;
}

/// RagdollPose.LimbPose
#[derive(Clone, Debug)]
pub struct LimbPose {
    pub Limb: usize,
    pub Animated: bool,
    pub PoseRigidityModifier: f32,
    pub Angle: f32,
    pub StartAngle: f32,
    pub EndAngle: f32,
    pub AnimationDuration: f32,
    pub RandomInfluence: f32,
    pub RandomSpeed: f32,
    pub TimeOffset: f32,
    pub AnimationCurve: Option<Curve>,
    /// The AnimationCurve's native evaluation cache.
    pub cache: CurveCache,
}

impl std::fmt::Debug for Curve {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Curve({} keys)", self.keys.len())
    }
}

impl LimbPose {
    /// LimbPose.EvaluateAngle; `random_offset` is the limb's randomOffset.
    pub fn evaluate_angle(&mut self, time: f32, random_offset: f32, speed_multiplier: f32) -> f32 {
        if !self.Animated {
            return self.Angle;
        }
        let num = speed_multiplier * time;
        let mut num2 = (self.TimeOffset + d(num) / self.AnimationDuration).f();
        if self.RandomInfluence > 0.01 {
            num2 = mathf::lerp(num2, mathf::perlin(random_offset, num), self.RandomInfluence);
        }
        let v = self.AnimationCurve.as_ref().unwrap().evaluate(&mut self.cache, num2);
        mathf::lerp(self.StartAngle, self.EndAngle, v)
    }

    /// LimbPose.EvaluateAngleAt
    pub fn evaluate_angle_at(&mut self, time_override: f32) -> f32 {
        if !self.Animated {
            return self.Angle;
        }
        let time = (self.TimeOffset + d(time_override) / self.AnimationDuration).f();
        let v = self.AnimationCurve.as_ref().unwrap().evaluate(&mut self.cache, time);
        mathf::lerp(self.StartAngle, self.EndAngle, v)
    }
}

/// RagdollPose; `Angles` doubles as AngleDictionary (one entry per limb, dictionary built from it).
#[derive(Clone, Debug)]
pub struct RagdollPose {
    pub Name: String,
    pub Rigidity: f32,
    pub ForceMultiplier: f32,
    pub ShouldStandUpright: bool,
    pub DragInfluence: f32,
    pub UprightForceMultiplier: f32,
    pub ShouldStumble: bool,
    pub State: i32,
    pub AnimationSpeedMultiplier: f32,
    pub Angles: Vec<LimbPose>,
}

impl RagdollPose {
    /// AngleDictionary lookup.
    pub fn angle_index(&self, limb: usize) -> Option<usize> {
        self.Angles.iter().position(|a| a.Limb == limb)
    }
}

script! {
    Person {
        Consciousness: f32 = 1.0,
        ShockLevel: f32 = 0.0,
        PainLevel: f32 = 0.0,
        AdrenalineLevel: f32 = 0.0,
        OxygenLevel: f32 = 1.0,
        AngleOffset: f32 = 0.0,
        GlobalRigidityMultiplier: f32 = 1.0,
        OverridePoseIndex: i32 = -1,
        BalanceOffset: f32 = 0.0,
        Heartbeat: f32 = 0.0,
        ImpactEffectShotDamageThreshold: f32 = 5.0,
        AverageHealth: f32 = 0.0,
        AverageSpeed: f32 = 0.0,
        AverageAngle: f32 = 0.0,
        AverageFireIntensity: f32 = 0.0,
        AverageWetness: f32 = 0.0,
        IsTouchingFloor: bool = false,
        RandomisedSize: bool = true,
        CountDeathAsStat: bool = true,
        dead: bool = false,
        totalAirtime: f32 = 0.0,
        HasAlreadyBeenDeadOnce: bool = false,
        seed: f32 = 0.0,
        Braindead: bool = false,
        BrainDamaged: bool = false,
        BrainDamagedTime: f32 = -1.0,
        SeizureTime: f32 = 0.0,
        DesiredWalkingDirection: f32 = 0.0,
    }
    links {
        name: String = String::new(),
        id: i32 = 0,
        Limbs: Vec<usize> = Vec::new(),
        Poses: Vec<RagdollPose> = Vec::new(),
        /// Index into Poses; None before Start.
        ActivePose: Option<usize> = None,
        /// LinkedPoses[PoseState] -> index into Poses.
        LinkedPoses: [usize; pose_state::COUNT] = [0; pose_state::COUNT],
        Head: Option<usize> = None,
        /// transform.localScale (root object).
        localScale: [f32; 3] = [1.0, 1.0, 1.0],
        BoneBreakClips: usize = 0,
        /// DisintegrationCounterBehaviour.DisintegrationCount (the root object's).
        DisintegrationCount: i32 = 0,
        /// DisintegrationCounterBehaviour.RegisterPseudoChild: the gore shards' PhysicalBehaviours, after the limbs.
        pseudo_children: Vec<usize> = Vec::new(),
        DismembermentClips: usize = 0,
        /// PoolableImpactEffect when set: the prefab, or None from a recording (which only names it).
        PoolableImpactEffect: Option<Option<crate::assets::db::ObjRef>> = None,
        /// DripSounds length of the BleedingParticle prefab (Person.BleedingParticlePrefab); None when it is null.
        BleedingDripSounds: Option<usize> = None,
        /// HingeJointLimitAutofixBehaviour.IsFlipped
        IsFlipped: bool = false,
        /// A pending Object.Destroy(root gameObject): the DelayedCallManager time it runs at.
        destroy_at: Option<f64> = None,
        /// The root GameObject (and its whole hierarchy) was destroyed.
        destroyed: bool = false,
    }
}

impl Game {
    pub fn active_pose(&self, person: usize) -> &RagdollPose {
        let p = &self.people[person];
        &p.Poses[p.ActivePose.unwrap()]
    }

    /// PersonBehaviour.Awake. `Limbs` is GetComponentsInChildren<LimbBehaviour> (the person's limbs in arena order, which is the
    /// hierarchy order); `Poses` and each `Limb.Person` link are the bridge's. Returns the new local scale of the root transform (RandomisedSize), which the bridge applies.
    /// Not modelled: AddToDictionary (AliveBehaviour.AliveByTransform, read by nothing rewritten) and chosenMaterial
    /// (a render material picked from the Goreless/Gore preferences).
    pub fn person_awake(&mut self, person: usize) -> Option<[f32; 3]> {
        self.people[person].seed = self.random.value() * 1000.0;
        // CreateLinkedPoseDictionary: PoseState values in order, PickRandom over the poses of that state.
        for state in 0..pose_state::COUNT {
            let of_state: Vec<usize> = (0..self.people[person].Poses.len()).filter(|&i| self.people[person].Poses[i].State == state as i32).collect();
            if of_state.is_empty() {
                panic!("Missing pose for {state}");
            }
            let pick = self.pick_random(of_state.len()).unwrap();
            self.people[person].LinkedPoses[state] = of_state[pick];
        }
        self.people[person].Consciousness = 1.0;
        self.people[person].OxygenLevel = 1.0;
        self.people[person].Limbs = (0..self.limbs.len()).filter(|&l| self.limbs[l].Person == person).collect();
        for i in 0..self.people[person].Limbs.len() {
            let l = self.people[person].Limbs[i];
            self.limbs[l].Person = person;
            if self.limbs[l].HasBrain {
                self.people[person].Head = Some(l);
            }
        }
        if self.people[person].RandomisedSize {
            let s = 1.0 * self.random.range(1.59, 2.0) / 2.0 / 0.82397;
            self.people[person].localScale = [s, s, 0.0];
            return Some([s, s, 0.0]);
        }
        None
    }

    /// PersonBehaviour.Start. ConstructDictionary only builds AngleDictionary from Angles (an ArgumentException on a
    /// duplicate limb).
    pub fn person_start(&mut self, person: usize) {
        self.people[person].IsTouchingFloor = false;
        self.people[person].AngleOffset = if !(self.people[person].localScale[0] < 0.0) { 1.0 } else { -1.0 };
        for pose in &self.people[person].Poses {
            for (i, a) in pose.Angles.iter().enumerate() {
                assert!(!pose.Angles[..i].iter().any(|b| b.Limb == a.Limb), "ArgumentException: duplicate key in AngleDictionary");
            }
        }
        // SetCollisionIgnores: every ordered pair of distinct limbs (Limb.Collider is the first collider of the object).
        for i in 0..self.people[person].Limbs.len() {
            for j in 0..self.people[person].Limbs.len() {
                if i != j {
                    let (a, b) = (self.people[person].Limbs[i], self.people[person].Limbs[j]);
                    let (ca, cb) = (self.physicals[self.limbs[a].PhysicalBehaviour].all_colliders[0], self.physicals[self.limbs[b].PhysicalBehaviour].all_colliders[0]);
                    self.ignore_collision_substitute(ca, cb, true);
                }
            }
        }
        self.set_pose(person, 0);
    }

    /// HingeJointLimitAutofixBehaviour.Start (on the root): a mirrored root flips every hinge's limits. The person has no
    /// SliderJoint2D or FixedJoint2D, so FixSlider and FixFixed find nothing.
    pub fn hinge_joint_limit_autofix_start(&mut self, person: usize) {
        self.people[person].IsFlipped = false;
        if !(self.people[person].localScale[0] > 0.0) {
            self.people[person].IsFlipped = true;
            for i in 0..self.people[person].Limbs.len() {
                if let Some(h) = self.limbs[self.people[person].Limbs[i]].Joint {
                    if self.hinges[h].useLimits {
                        let [min, max] = self.hinges[h].limits;
                        self.hinges[h].set_limits(&mut self.world, 0.0 - max, 0.0 - min);
                    }
                }
            }
        }
    }

    /// PersonBehaviour.IsAlive / IsHeadAlive
    pub fn person_is_alive(&mut self, person: usize) -> bool {
        match self.people[person].Head {
            Some(head) => self.limb_is_considered_alive(head),
            None => false,
        }
    }

    /// DisintegrationCounterBehaviour.OnChildDisintegrated: its children are the person's limbs (none destroyed) and
    /// the pseudo children not yet destroyed.
    pub fn person_on_child_disintegrated(&mut self, person: usize) {
        self.people[person].DisintegrationCount += 1;
        let alive = self.people[person].Limbs.len() + self.people[person].pseudo_children.iter().filter(|&&c| !self.physicals[c].destroyed).count();
        if self.people[person].DisintegrationCount >= alive as i32 {
            // Object.Destroy(base.transform.root.gameObject): DestroyObjectDelayed queues it on the
            // DelayedCallManager at min(fixedTimeAsDouble, the frame's time), like Destroy(Joint); a second
            // Destroy finds nothing left to do.
            let at = self.time.fixedTimeAsDouble.min(self.time.timeAsDouble);
            self.people[person].destroy_at.get_or_insert(at);
        }
    }

    /// The people's queued root-GameObject destroys due by now (DelayedCallManager times), as (time, person).
    pub fn person_due_destroys(&self, now: f64) -> Vec<(f64, usize)> {
        (0..self.people.len()).filter_map(|p| self.people[p].destroy_at.filter(|&at| at <= now).map(|at| (at, p))).collect()
    }

    /// The delayed Object.Destroy of a person's root GameObject (DestroyObjectHighLevel): the whole hierarchy
    /// goes, so every script on it gets OnDisable (while active) and OnDestroy, and its components vanish.
    pub fn person_run_destroy(&mut self, person: usize) {
        self.people[person].destroy_at = None;
        if self.people[person].destroyed {
            return;
        }
        let limbs = self.people[person].Limbs.clone();
        // DisintegrationCount reached the limb count, so normally every limb went through PhysicalBehaviour.Disintegrate and
        // its GameObject is inactive (no OnDisable left to run, the b2 body, joints and colliders already gone). Otherwise
        // the generic hierarchy destroy runs: OnDisable of every active script, then each component's OnDestroy.
        for &l in &limbs {
            let p = self.limbs[l].PhysicalBehaviour;
            if self.limb_active(l) || self.physicals[p].rigidbody.detached.is_none() {
                let id = self.people[person].id;
                self.run_object_destroy(id);
                return;
            }
        }
        // PersonBehaviour.OnDestroy: AliveBehaviour.AliveByTransform.Remove(transform), a lookup nothing rewritten reads.
        // DisintegrationCounterBehaviour.OnDestroy: its OnDisintegration handlers go with the objects.
        // The root's other scripts (IncreaseStatOnStart, HingeJointLimitAutofixBehaviour, Optout) have no OnDestroy.
        // The limbs' OnDestroy calls only touch their own object and other people's flags, so their order among
        // the limbs does not change a result; each limb's scripts run in component order.
        for &l in &limbs {
            let p = self.limbs[l].PhysicalBehaviour;
            // PhysicalBehaviour.OnDestroy: DestroyStabJoint for each penetration (Disintegrate refused any).
            self.stab_destroy(p);
            // PhysicalObjectsInWorld / PhysicalObjectsInWorldByTransform / CameraControlBehaviour.CurrentlyFollowing
            // remove it: the object leaves every manager loop (the camera list has no rewritten state).
            self.physicals[p].enabled = false;
            self.physicals[p].destroyed = true;
            // An empty Collision2D exit to each collider still in the stay buffer (SendMessage skips inactive
            // objects, and a destroyed collider reads null).
            for pair in self.physicals[p].onCollisionStayBuffer.into_iter().flatten() {
                if pair.Active && self.collider_alive(pair.Coll) {
                    self.send_empty_collision_exit(pair.Coll);
                }
            }
            // InvokeOnItemRemoved: the modding event only.
            // LimbBehaviour.OnDestroy: LimbBehaviourManager.Limbs.Remove(this); Destroy(myStatus) is UI; then
            // Destroy(item) for each live DestroyWith.
            self.limbs[l].enabled = false;
            for item in self.limbs[l].destroy_with.clone() {
                self.destroy_object(item);
            }
            // ConnectedNodeBehaviour.OnDestroy: DisconnectFromEverything, then NodeController.Nodes.Remove (the
            // node is isolated and nothing reads it afterwards, so its slot in the update loop changes nothing).
            self.node_disconnect_from_everything(l);
            // SkinMaterialHandler.OnDestroy: Destroy(material), rendering only.
            // GoreStringBehaviour.OnDestroy
            if self.limbs[l].goreString {
                self.gore_string_destroy_joint(l);
            }
            // GripBehaviour.OnDestroy: ModdingApi listener removal only.
        }
        for &l in &limbs {
            self.limb_destroy_joints(l);
        }
        self.people[person].destroyed = true;
    }

    /// The limb's components that go with its GameObject: the SpringJoint2D (and the queued Destroy(Joint) with it), the
    /// HingeJoint2D (Joint2D::Cleanup finds its b2 joint gone) and the bleeding particles under it.
    pub(super) fn limb_destroy_joints(&mut self, l: usize) {
        self.remove_spring_joint(l);
        if let Some(h) = self.limbs[l].Joint {
            self.run_hinge_destroy(h);
        }
        // The bleeding particles are children of the limb (and the extraDecals splat of the particle).
        for i in self.circulations[l].BleedingParticles.clone() {
            self.bleeding[i].destroyed = true;
            if let Some(s) = self.bleeding[i].BloodSplatController {
                self.splats[s].destroyed = true;
            }
        }
    }

    /// The delayed Object.Destroy of a hinge (HingeJoint2D::Cleanup): its b2 joint and break-limit entry go.
    pub(super) fn run_hinge_destroy(&mut self, h: usize) {
        self.hinges[h].destroy_at = None;
        if let Some(j) = self.hinges[h].joint.take() {
            self.world.destroy_joint(j);
        }
        self.hinges[h].destroyed = true;
        self.joint_break_limits.retain(|&k| k != super::BreakJoint::Hinge(h));
    }

    pub fn add_pain(&mut self, person: usize, intensity: f32) {
        self.people[person].PainLevel += intensity;
    }

    /// PersonBehaviour.Wince
    pub fn person_wince(&mut self, person: usize, i: f32) {
        for j in 0..self.people[person].Limbs.len() {
            let limb = self.people[person].Limbs[j];
            self.limb_wince(limb, i);
        }
    }

    /// PersonBehaviour.Shot
    pub fn person_shot(&mut self, person: usize) {
        self.person_wince(person, 165.0);
    }

    /// PersonBehaviour.SetPose
    pub fn set_pose(&mut self, person: usize, pose: usize) {
        if Some(pose) != self.people[person].ActivePose {
            self.people[person].ActivePose = Some(pose);
            for j in 0..self.people[person].Limbs.len() {
                let limb = self.people[person].Limbs[j];
                let active = self.people[person].Poses[pose].angle_index(limb).is_some();
                self.limbs[limb].IsActiveInCurrentPose = active;
            }
        }
    }

    /// PersonBehaviour.FixedUpdate
    pub fn person_fixed_update(&mut self, person: usize) {
        if self.people[person].destroyed {
            return;
        }
        if self.people[person].BrainDamaged && self.person_is_alive(person) && self.random.value() > 0.99 {
            self.person_wince(person, 120.0);
            if self.people[person].SeizureTime <= mathf::EPSILON && self.random.value() > 0.95 {
                self.people[person].SeizureTime = self.random.range_int(2, 30) as f32;
            }
        }
        if self.people[person].SeizureTime > 0.0 {
            self.people[person].SeizureTime -= self.time.deltaTime;
            let i = 16.0 * self.random.value();
            self.person_wince(person, i);
        }
    }

    /// PersonBehaviour.Update
    pub fn person_update(&mut self, person: usize) {
        if self.people[person].destroyed {
            return;
        }
        let dt = self.time.deltaTime;
        if self.people[person].BrainDamaged {
            if !self.prefs.BrainDamage {
                self.people[person].BrainDamaged = false;
            }
            self.people[person].BrainDamagedTime += dt;
        } else {
            self.people[person].BrainDamagedTime = 0.0;
        }
        self.all_limb_loop_checks(person);
        self.attached_limb_loop_checks(person);
        let head = self.people[person].Head.unwrap();
        let up = self.transform_up(self.limbs[head].PhysicalBehaviour);
        let dir = if self.circulations[head].InternalBleedingIntensity > 0.5 {
            let n = mathf::perlin2_mapped(self.time.time, self.people[person].seed);
            normalized([up[0] + n[0] * 0.7, up[1] + n[1] * 0.7, up[2] + 0.0 * 0.7])
        } else {
            up
        };
        // Vector2.Dot(Vector3 -> Vector2, Vector2.right)
        self.people[person].BalanceOffset = (dir[0] * 1.0 + dir[1] * 0.0) * self.people[person].localScale[0];
        if self.person_is_alive(person) {
            self.people[person].Consciousness = (d(self.people[person].Consciousness) + d(dt) / 120.0f32).f();
            if self.physicals[self.limbs[head].PhysicalBehaviour].Wetness < 0.5 {
                self.people[person].OxygenLevel +=
                    mathf::pow(mathf::sin(self.time.time * 3.0), 16.0) * 0.5 / 40.0 * (dt * 60.0);
            }
        } else {
            self.people[person].Consciousness = (d(self.people[person].Consciousness) - d(dt) / 60.0f32).f();
            self.people[person].OxygenLevel = (d(self.people[person].OxygenLevel) - d(dt) / 40.0f32).f();
        }
        let p = &mut self.people[person];
        p.OxygenLevel = mathf::clamp01(p.OxygenLevel);
        p.Consciousness = mathf::clamp01(p.Consciousness);
        p.ShockLevel = (d(p.ShockLevel) - d(dt) / 25.0f32).f();
        p.ShockLevel = mathf::clamp01(p.ShockLevel);
        p.PainLevel = (d(p.PainLevel) - d(dt) / 5.0f32).f();
        p.PainLevel = mathf::clamp01(p.PainLevel);
        p.AdrenalineLevel -= dt;
        p.AdrenalineLevel = mathf::max(0.0, mathf::min(20.0, p.AdrenalineLevel));
        p.totalAirtime += dt;
        // StatManager RAGDOLL_AIR_TIME: Steam stat only.
        self.handle_death_counter(person);
        let p = &mut self.people[person];
        if p.DesiredWalkingDirection > 0.0 {
            p.DesiredWalkingDirection -= dt;
        } else {
            p.DesiredWalkingDirection += dt;
        }
        p.DesiredWalkingDirection = mathf::clamp(p.DesiredWalkingDirection, -4.0, 4.0);
    }

    fn handle_death_counter(&mut self, person: usize) {
        if !self.people[person].CountDeathAsStat {
            return;
        }
        let alive = self.person_is_alive(person);
        if !alive && !self.people[person].dead {
            self.people[person].dead = true;
            // OnDeath event, BODY_COUNT stats and Global.LastKillTime: no simulation effect.
            let head = self.people[person].Head;
            if head.is_some_and(|h| self.limbs[h].IsZombie)
                && !self.people[person].HasAlreadyBeenDeadOnce
                && self.random.value() < 0.02
            {
                // Object.Instantiate(Global.main.HolleManPrefab, Head.transform.position, identity).
                let h = head.unwrap();
                let source = self.holle_man_prefab.unwrap_or_else(|| unimplemented!("Object.Instantiate(null HolleManPrefab) (ArgumentException)"));
                let position = self.physicals[self.limbs[h].PhysicalBehaviour].transform.position;
                self.spawn_prefab(source, position);
            }
            self.people[person].HasAlreadyBeenDeadOnce = true;
        } else if alive && self.people[person].dead {
            self.people[person].dead = false;
        }
    }

    /// PersonBehaviour.AllLimbLoopChecks: the original never counts limbs, so this is a sum.
    fn all_limb_loop_checks(&mut self, person: usize) {
        let mut num = 0.0f32;
        for &l in &self.people[person].Limbs {
            num = (d(num) + d(self.limbs[l].Health) / self.limbs[l].InitialHealth).f();
        }
        num /= 1.0;
        self.people[person].AverageHealth = num;
    }

    fn attached_limb_loop_checks(&mut self, person: usize) {
        let (mut num, mut num2, mut num3, mut num4) = (0.0f32, 0.0f32, 0.0f32, 0i32);
        self.people[person].IsTouchingFloor = false;
        self.people[person].Heartbeat = 0.0;
        let head = self.people[person].Head;
        for j in 0..self.people[person].Limbs.len() {
            let l = self.people[person].Limbs[j];
            if self.nodes[l].IsConnectedToRoot {
                let rate = self.heart_rate(l);
                self.people[person].Heartbeat = mathf::max(self.people[person].Heartbeat, rate);
                let ph = &self.physicals[self.limbs[l].PhysicalBehaviour];
                num += ph.burnIntensity;
                num2 += mathf::sqr_magnitude(ph.rigidbody.velocity(&self.world));
                num3 += ph.Wetness;
                if self.limbs[l].IsOnFloor {
                    self.people[person].IsTouchingFloor = true;
                }
                num4 += 1;
                if let Some(h) = head {
                    let ib = self.circulations[h].InternalBleedingIntensity;
                    if ib > mathf::EPSILON {
                        self.limbs[l].Numbness = (d(self.limbs[l].Numbness) + d(ib) * self.time.deltaTime).f();
                    }
                }
            }
        }
        let p = &mut self.people[person];
        if p.IsTouchingFloor {
            p.totalAirtime = 0.0;
        }
        let n = num4.max(1) as f32;
        p.AverageFireIntensity = num / n;
        p.AverageSpeed = num2 / n;
        p.AverageWetness = num3 / n;
    }

    /// PersonBehaviour.LateUpdate -> DetermineActivePose
    pub fn person_late_update(&mut self, person: usize) {
        if self.people[person].destroyed {
            return;
        }
        use pose_state::*;
        let p = &self.people[person];
        if p.dead {
            self.set_pose(person, p.LinkedPoses[REST as usize]);
            return;
        }
        let flag = p.SeizureTime > mathf::EPSILON
            || (p.BrainDamaged && p.BrainDamagedTime < mathf::map_range(0.0, 1000.0, 0.1, 30.0, p.seed));
        let linked = p.LinkedPoses;
        if p.OverridePoseIndex != -1 && !flag {
            let n = p.Poses.len() as i32;
            let index = (p.OverridePoseIndex % n) as usize;
            self.people[person].DesiredWalkingDirection = 1.0;
            self.set_pose(person, index);
        } else if flag {
            self.people[person].Consciousness = mathf::max(p.Consciousness, 0.8);
            self.set_pose(person, linked[BRAIN_DAMAGE as usize]);
        } else if p.DesiredWalkingDirection.abs() > 0.5 && p.IsTouchingFloor && p.PainLevel < 0.5 {
            self.set_pose(person, linked[WALKING as usize]);
        } else if p.PainLevel > 0.8 && !self.limbs[p.Head.unwrap()].IsZombie {
            self.set_pose(person, linked[WRITHING_IN_PAIN as usize]);
        } else if p.IsTouchingFloor {
            if p.AverageSpeed > 25.0 || p.ShockLevel > 0.5 {
                self.set_pose(person, linked[PROTECTIVE as usize]);
            } else if p.AverageSpeed < 15.0 || p.ShockLevel < 0.2 {
                self.set_pose(person, linked[REST as usize]);
            }
        } else if p.AverageSpeed < 10.0 && p.ShockLevel <= 0.5 {
            let state = if !(p.AverageWetness < 0.2) { SWIMMING } else { REST };
            self.set_pose(person, linked[state as usize]);
        } else if p.AverageSpeed < 80.0 || p.ShockLevel > 0.5 {
            self.set_pose(person, linked[PROTECTIVE as usize]);
        } else if p.AverageWetness < 0.2 {
            self.set_pose(person, linked[FLAILING as usize]);
        }
    }
}

/// Vector3.normalized (Vector3.Magnitude: one double chain under Math.Sqrt, rounded at its return).
pub fn normalized(v: [f32; 3]) -> [f32; 3] {
    let num = (d(v[0]) * v[0] + d(v[1]) * v[1] + d(v[2]) * v[2]).0.sqrt() as f32;
    if num > 1e-5 { [v[0] / num, v[1] / num, v[2] / num] } else { [0.0, 0.0, 0.0] }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PersonBehaviour.Awake's Random draws from the state recorded before the Instantiate: seed = value * 1000,
    /// one Range(0, n) per PoseState (CreateLinkedPoseDictionary's PickRandom over that state's poses), then
    /// RandomisedSize's Range(1.59, 2) -> the root's local scale (z 0: a Vector2 assigned to localScale).
    #[test]
    fn awake_random_matches_game() {
        use super::super::dump;
        use crate::game_assets::HumanRig;
        use std::path::Path;
        let Ok(game) = crate::game_assets::find_game(None) else { return };
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let Ok(rig) = HumanRig::load(&root.join("local-cache"), &game) else { return };
        let path = root.join("reference-probe/game/dump31spawnrand.txt");
        let Ok(header) = dump::header(&path) else { return };
        let Some(state) = dump::spawn_random_state(&header) else { return };
        let mut snaps = dump::snapshots(&path).unwrap();
        let first = snaps.next().unwrap();
        let fs = snaps.find(|s| s.pt == "FS" && s.of("b2World").next().is_some()).unwrap();
        let g = Game::from_snapshot(&fs, &rig).unwrap();
        let p = &g.people[0];
        let mut random = crate::unity::Random { state };
        assert_eq!(random.value() * 1000.0, p.seed, "seed");
        for s in 0..pose_state::COUNT {
            let of_state: Vec<usize> = (0..p.Poses.len()).filter(|&i| p.Poses[i].State == s as i32).collect();
            assert_eq!(of_state[random.range_int(0, of_state.len() as i32) as usize], p.LinkedPoses[s], "LinkedPoses[{s}] of {}", of_state.len());
        }
        let scale = 1.0 * random.range(1.59, 2.0) / 2.0 / 0.82397;
        let human = first.of("TransformChain").find(|r| r.name == "Human").unwrap();
        let want: Vec<f32> = human.get("lscale").unwrap().split(',').map(|w| f32::from_bits(u32::from_str_radix(w, 16).unwrap())).collect();
        assert_eq!([scale.to_bits(), scale.to_bits(), 0], [want[0].to_bits(), want[1].to_bits(), want[2].to_bits()], "lscale {want:?} ours {scale}");
    }

    /// The Awake and Start passes of a spawned Person take the instance ids the recording lists, in order: the type at
    /// the first id of each phase and the final next id (dump33human: the Person's clone ends at -62472; the recorded
    /// ids-after33 lists every UnityEngine.Object by id).
    #[test]
    fn awake_start_ids_match_game() {
        use super::super::dump;
        use crate::game_assets::HumanRig;
        use std::path::Path;
        let Ok(game) = crate::game_assets::find_game(None) else { return };
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let Ok(rig) = HumanRig::load(&root.join("local-cache"), &game) else { return };
        let Ok(mut snaps) = dump::snapshots(&root.join("reference-probe/game/dump33human.txt")) else { return };
        let Ok(ids) = std::fs::read_to_string(root.join("reference-probe/game/ids-after33.txt")) else { return };
        let kind_at = |id: i32| ids.lines().find_map(|l| l.strip_prefix(&format!("{id} "))).map(|r| r.split(' ').next().unwrap().to_string());
        let _first = snaps.next().unwrap();
        let fs = snaps.find(|s| s.pt == "FS" && s.of("b2World").next().is_some()).unwrap();
        let mut g = Game::from_snapshot(&fs, &rig).unwrap();
        let limbs = g.people[0].Limbs.clone();
        assert_eq!(limbs.len(), 14);
        g.next_instance_id = -62474;
        let mut phase = |g: &mut Game, name: &str, want_first: &str, f: &dyn Fn(&mut Game)| {
            let first = g.next_instance_id;
            assert_eq!(kind_at(first).as_deref(), Some(want_first), "{name}: type at {first}");
            f(g);
        };
        phase(&mut g, "person awake", "Material", &|g| {
            g.person_awake(0);
        });
        // Awake order: SkinMaterialHandler (0), PhysicalBehaviour (80), LimbBehaviour (200).
        for &l in &limbs {
            g.skin_awake(l);
        }
        assert_eq!(g.next_instance_id, -62502);
        for &l in &limbs {
            let p = g.limbs[l].PhysicalBehaviour;
            g.physicals[p].ids.ActivationNode = 0;
            g.physicals[p].ids.ContextMenuOptions = 0;
        }
        phase(&mut g, "physical awake", "NodeBehaviour", &|g| {
            for &l in &limbs {
                g.physical_awake(g.limbs[l].PhysicalBehaviour);
            }
        });
        assert_eq!(g.next_instance_id, -62558);
        phase(&mut g, "limb awake", "GameObject", &|g| {
            for &l in &limbs {
                g.limb_awake(l);
            }
        });
        assert_eq!(g.next_instance_id, -62810);
        for &l in &limbs {
            g.circulation_awake(l);
        }
        // Start order: the order-0 scripts, PhysicalBehaviour (80), HingeJointLimitAutofixBehaviour (100), LimbBehaviour (200).
        g.person_start(0);
        phase(&mut g, "circulation start", "GameObject", &|g| {
            for &l in &limbs {
                g.circulation_start(l);
            }
        });
        assert_eq!(g.next_instance_id, -63408);
        // PhysicalBehaviour.Start (80), with the snapshot's Start-made state undone; the first also adds the root's
        // AudioSourceTimeScaleBehaviour.
        for &l in &limbs {
            let p = g.limbs[l].PhysicalBehaviour;
            let ph = &mut g.physicals[p];
            (ph.audioSource, ph.audio_time_scale) = (false, false);
            (ph.ids.audioSource, ph.ids.slidingAudioSource, ph.ids.selectionOutlineObject, ph.ids.outlineSpriteRenderer) = (0, 0, 0, 0);
        }
        phase(&mut g, "physical start", "AudioSourceTimeScaleBehaviour", &|g| {
            for &l in &limbs {
                g.physical_start(g.limbs[l].PhysicalBehaviour);
            }
        });
        assert_eq!(g.next_instance_id, -63746);
        g.gforces.clear();
        g.hinge_joint_limit_autofix_start(0);
        phase(&mut g, "limb start", "GForceMeasureBehaviour", &|g| {
            for &l in &limbs {
                g.limb_start(l);
            }
        });
        assert_eq!(g.next_instance_id, -63774);
    }

    /// The last disintegration queues the root GameObject's Destroy once, at min(fixed time, frame time).
    #[test]
    fn last_disintegration_queues_destroy() {
        let mut g = Game::default();
        g.time.fixedTimeAsDouble = 2.0;
        g.time.timeAsDouble = 3.0;
        g.people.push(Person { Limbs: vec![0, 1], ..Default::default() });
        g.person_on_child_disintegrated(0);
        assert_eq!(g.people[0].destroy_at, None);
        g.person_on_child_disintegrated(0);
        g.time.fixedTimeAsDouble = 4.0;
        g.person_on_child_disintegrated(0);
        assert_eq!(g.people[0].destroy_at, Some(2.0));
        assert_eq!(g.person_due_destroys(1.9), vec![]);
        assert_eq!(g.person_due_destroys(2.0), vec![(2.0, 0)]);
    }
}
