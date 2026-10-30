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
        DismembermentClips: usize = 0,
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

    /// PersonBehaviour.IsAlive / IsHeadAlive
    pub fn person_is_alive(&mut self, person: usize) -> bool {
        match self.people[person].Head {
            Some(head) => self.limb_is_considered_alive(head),
            None => false,
        }
    }

    /// DisintegrationCounterBehaviour.OnChildDisintegrated: its children are the person's limbs, none destroyed.
    pub fn person_on_child_disintegrated(&mut self, person: usize) {
        self.people[person].DisintegrationCount += 1;
        if self.people[person].DisintegrationCount >= self.people[person].Limbs.len() as i32 {
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
        // DisintegrationCount reached the limb count, so every limb went through PhysicalBehaviour.Disintegrate and
        // its GameObject is inactive (no OnDisable left to run, the b2 body, joints and colliders already gone).
        for &l in &limbs {
            let p = self.limbs[l].PhysicalBehaviour;
            if self.limb_active(l) || self.physicals[p].rigidbody.detached.is_none() {
                unimplemented!("Object.Destroy of a person with an active limb: OnDisable order of the hierarchy");
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
            if !self.physicals[p].penetrations.is_empty() {
                unimplemented!("PhysicalBehaviour.DestroyStabJoint");
            }
            // PhysicalObjectsInWorld / PhysicalObjectsInWorldByTransform / CameraControlBehaviour.CurrentlyFollowing
            // remove it: the object leaves every manager loop (the camera list has no rewritten state).
            self.physicals[p].enabled = false;
            self.physicals[p].destroyed = true;
            // An empty Collision2D exit to each collider still in the stay buffer (SendMessage skips inactive
            // objects, and a destroyed collider reads null).
            for pair in self.physicals[p].onCollisionStayBuffer.into_iter().flatten() {
                if pair.Active && !self.physicals[pair.Coll].destroyed && self.physicals[pair.Coll].active {
                    self.physical_on_collision_exit(pair.Coll, &crate::b2::contacts2d::ScriptCollision::default());
                }
            }
            // InvokeOnItemRemoved: the modding event only.
            // LimbBehaviour.OnDestroy: LimbBehaviourManager.Limbs.Remove(this); Destroy(myStatus) is UI and
            // DestroyWith is only filled by attachments, which are not rewritten.
            self.limbs[l].enabled = false;
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
            // The limb's joints are destroyed with it: the SpringJoint2D (and the queued Destroy(Joint) with it) ...
            self.remove_spring_joint(l);
            // ... and the HingeJoint2D (Joint2D::Cleanup finds its b2 joint gone for a disintegrated limb).
            if let Some(h) = self.limbs[l].Joint {
                if let Some(j) = self.hinges[h].joint.take() {
                    self.world.destroy_joint(j);
                }
                self.hinges[h].destroyed = true;
                self.joint_break_limits.retain(|&k| k != super::BreakJoint::Hinge(h));
            }
            // The bleeding particles are children of the limb (and the extraDecals splat of the particle).
            for i in self.circulations[l].BleedingParticles.clone() {
                self.bleeding[i].destroyed = true;
                if let Some(s) = self.bleeding[i].BloodSplatController {
                    self.splats[s].destroyed = true;
                }
            }
        }
        self.people[person].destroyed = true;
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
        let up = self.physicals[self.limbs[head].PhysicalBehaviour].transform.up();
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
                // ponytail: HolleManPrefab spawn (zombie easter egg) is not rewritten yet.
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
