//! SentryTurretBehaviour and SentryTurretGunBehaviour (the Sentry Turret). The root's AutomaticSentryController looks for
//! limbs and the state machine (Idle, Firing, Searching, Panicking) plays its sounds; the gun's joint turns the barrel
//! towards the target (or sweeps) and fires the FirearmBehaviour. Explode (EMP, burning, heat, a hard shot) spawns the
//! explosion and the debris, and destroys the turret. The AudioSource's clips are PlaySound's draws and its isPlaying
//! end times; the LineRenderer, the particles and the one-shot sounds are presentation.
#![allow(non_snake_case)]

use super::automatic_sentry::AutomaticSentry;
use super::destroyable::Step;
use super::{d, mathf, script, Game};

/// SentryTurretAIState
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SentryState {
    #[default]
    Idle,
    Firing,
    Searching,
    Panicking,
}

impl SentryState {
    pub fn from_int(v: i64) -> Option<SentryState> {
        Some(match v {
            0 => SentryState::Idle,
            1 => SentryState::Firing,
            2 => SentryState::Searching,
            3 => SentryState::Panicking,
            _ => return None,
        })
    }
}

/// The clip lists PlaySound picks from (their lengths, for AudioSource.isPlaying).
#[derive(Clone, Debug, Default)]
pub struct SentryClips {
    pub TargetAcquired: Vec<f32>,
    pub TargetLost: Vec<f32>,
    pub Flying: Vec<f32>,
    pub ShotAt: Vec<f32>,
    pub OnFire: Vec<f32>,
    pub Tipped: Vec<f32>,
    pub Retiring: Vec<f32>,
}

/// One of SentryClips' lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SentrySet {
    TargetAcquired,
    TargetLost,
    Flying,
    ShotAt,
    OnFire,
    Tipped,
    Retiring,
}

script! {
    SentryTurret {}
    links {
        /// The component's instance id.
        id: i32 = 0,
        enabled: bool = true,
        /// phys: GetComponent<PhysicalBehaviour>() (Awake).
        phys: usize = 0,
        /// turretGun: the SentryTurretGunBehaviour (`Game::sentry_guns`), resolved after both are linked.
        gun: usize = 0,
        gun_object: usize = 0,
        /// BlockingLayers.m_Bits
        BlockingLayers: u32 = 0,
        /// AudioSource (its component id).
        audio: i32 = 0,
        clips: SentryClips = SentryClips::default(),
        /// Controller (AutomaticSentryController, MaxAngle 100 in Awake).
        controller: AutomaticSentry = AutomaticSentry::default(),
        state: SentryState = SentryState::Idle,
        soundTimer: f32 = 0.0,
        deathTimer: f32 = 0.0,
        timeStartedSearching: f32 = 0.0,
        disabled: bool = false,
        /// rb.velocity of the last FixedUpdate.
        oldVelocity: [f32; 2] = [0.0; 2],
        exploded: bool = false,
    }
}

script! {
    SentryTurretGun {}
    links {
        /// The component's instance id.
        id: i32 = 0,
        enabled: bool = true,
        /// turretJoint: the HingeJoint2D (`Game::hinges` index).
        joint: usize = 0,
        /// firearmBehaviour: the FirearmBehaviour (`Game::firearms` index).
        firearm: usize = 0,
        /// sentryBehaviour: the SentryTurretBehaviour (`Game::sentry_turrets` index), resolved after both are linked.
        sentry: usize = 0,
        sentry_object: usize = 0,
        /// The gun's Transform node.
        node: usize = 0,
        timer: f32 = 0.0,
        canFire: bool = false,
    }
}

/// Vector3.normalized.
fn normalized3(v: [f32; 3]) -> [f32; 3] {
    let num = (d(v[0]) * v[0] + d(v[1]) * v[1] + d(v[2]) * v[2]).0.sqrt() as f32;
    if num > 1e-5 {
        [v[0] / num, v[1] / num, v[2] / num]
    } else {
        [0.0; 3]
    }
}

/// Mathf.Sign: 1 for zero.
fn sign(x: f32) -> f32 {
    if x >= 0.0 { 1.0 } else { -1.0 }
}

impl Game {
    /// `sentry.clips` for one of PlaySound's sets.
    fn sentry_clips(&self, i: usize, set: SentrySet) -> &Vec<f32> {
        let c = &self.sentry_turrets[i].clips;
        match set {
            SentrySet::TargetAcquired => &c.TargetAcquired,
            SentrySet::TargetLost => &c.TargetLost,
            SentrySet::Flying => &c.Flying,
            SentrySet::ShotAt => &c.ShotAt,
            SentrySet::OnFire => &c.OnFire,
            SentrySet::Tipped => &c.Tipped,
            SentrySet::Retiring => &c.Retiring,
        }
    }

    /// SentryTurretBehaviour.PlaySound(set): AudioSource.clip = set.PickRandom(); AudioSource.Play(), which runs the clip.
    fn sentry_play_sound(&mut self, i: usize, set: SentrySet) {
        let len = self.sentry_clips(i, set).len();
        let k = self.pick_random(len).unwrap_or_else(|| unimplemented!("SentryTurretBehaviour: PickRandom of an empty array (IndexOutOfRangeException)"));
        let length = self.sentry_clips(i, set)[k];
        let until = self.time.fixedTimeAsDouble + length as f64;
        let source = self.sentry_turrets[i].audio;
        let end = self.audio_playing_until.entry(source).or_insert(until);
        *end = end.max(until);
    }

    /// SentryTurretBehaviour.Panic: the flying sound unless disabled.
    fn sentry_panic(&mut self, i: usize) {
        if !self.sentry_turrets[i].disabled {
            self.sentry_play_sound(i, SentrySet::Flying);
        }
    }

    /// SentryTurretBehaviour.Searching
    fn sentry_searching(&mut self, i: usize) {
        self.sentry_turrets[i].state = SentryState::Searching;
        if (self.random.value() as f64) > 0.2 {
            self.sentry_play_sound(i, SentrySet::TargetLost);
        }
    }

    /// SentryTurretBehaviour.Activate
    fn sentry_activate(&mut self, i: usize) {
        self.sentry_turrets[i].state = SentryState::Firing;
        if (self.random.value() as f64) > 0.2 {
            self.sentry_play_sound(i, SentrySet::TargetAcquired);
        }
    }

    /// The gun's transform node pose: its world position, rotation and lossy scale.
    fn sentry_gun_pose(&self, g: usize) -> ([f32; 3], [f32; 4], [f32; 3]) {
        self.node_pose(self.sentry_guns[g].node)
    }

    /// The gun's barrel direction vector (Vector3): transform.right * Mathf.Sign(lossyScale.x).
    fn sentry_gun_vector(&self, g: usize) -> [f32; 2] {
        let (_, rot, lossy) = self.sentry_gun_pose(g);
        let right = mathf::rotate(rot, [1.0, 0.0, 0.0]);
        let s = sign(lossy[0]);
        [right[0] * s, right[1] * s]
    }

    /// SentryTurretBehaviour.Start: a disabled turret is disabled.
    pub fn sentry_turret_start(&mut self, i: usize) {
        if self.sentry_turrets[i].disabled {
            self.sentry_turret_disable(i);
        }
    }

    /// SentryTurretBehaviour.Update
    pub fn sentry_turret_update(&mut self, i: usize) {
        let delta = self.time.deltaTime;
        let s = &mut self.sentry_turrets[i];
        s.controller.LayerMask = s.BlockingLayers;
        s.soundTimer += delta;
        if s.state == SentryState::Panicking {
            s.deathTimer += delta;
        } else {
            s.deathTimer = 0.0;
        }
    }

    /// SentryTurretBehaviour.OnEMPHit: Explode.
    pub fn sentry_turret_on_emp_hit(&mut self, i: usize) {
        self.sentry_turret_explode(i);
    }

    /// SentryTurretBehaviour.Shot(shot), with the shot's damage: PlaySound(ShotAt) unless disabled; a shot above
    /// 100 + Random.value * 150 panics and explodes after a Random.value delay (DelayCoroutine).
    pub fn sentry_turret_shot(&mut self, i: usize, damage: f32) {
        if !self.sentry_turrets[i].disabled {
            self.sentry_play_sound(i, SentrySet::ShotAt);
        }
        if damage > 100.0 + self.random.value() * 150.0 {
            self.sentry_panic(i);
            let delay = self.random.value();
            let owner = self.sentry_turrets[i].id;
            self.start_wait(owner, delay, Step::SentryTurretExplode(i));
        }
    }

    /// SentryTurretBehaviour.Disable: the turret stops; the gun's LineRenderer is presentation.
    pub fn sentry_turret_disable(&mut self, i: usize) {
        self.sentry_turrets[i].disabled = true;
    }

    /// SentryTurretBehaviour.Explode: once, the camera shake, the Explosion clone, the SentryTurretDebris clone (its scale
    /// the turret's, its velocities from the turret's body, its undo entry replaced), and the turret's Destroy.
    pub fn sentry_turret_explode(&mut self, i: usize) {
        if self.sentry_turrets[i].exploded {
            return;
        }
        self.sentry_turrets[i].exploded = true;
        let p = self.sentry_turrets[i].phys;
        let pos = self.physicals[p].transform.position;
        self.camera_shake(10.0, [pos[0], pos[1]]);
        self.spawn_resource("Prefabs/Explosion", pos);
        let rot = self.physicals[p].transform.rotation;
        let scale = self.physicals[p].transform.localScale;
        let mut db = self.assets.take().expect("SentryTurretBehaviour.Explode without the game's data");
        let source = db.resource("Prefabs/SentryTurretDebris", 1).ok().flatten().unwrap_or_else(|| panic!("Resources.Load(Prefabs/SentryTurretDebris) found no GameObject"));
        let spawned = db.prefab(source).and_then(|prefab| self.spawn(&mut db, &prefab, pos, rot, None));
        self.assets = Some(db);
        let (mut inst, linked) = spawned.unwrap_or_else(|e| panic!("SentryTurretBehaviour: Instantiate(debris): {e}"));
        // createdDebris.transform.localScale = base.transform.localScale
        inst.set_local_scale(0, scale);
        self.refresh_transforms(&inst, &linked);
        let debris_root = inst.objects[0].components[0];
        let children: Vec<i32> = (1..inst.objects.len()).filter(|&k| inst.objects[k].parent == Some(0)).map(|k| inst.objects[k].components[0]).collect();
        self.unsynced.push(inst);
        // DestroyableBehaviour.ApplyAncestorProperties(rb, phys, default, debris): the turret's body is the velocity ancestor.
        self.destroyable_apply_ancestor_properties(p, p, [0.0, 0.0], &linked);
        // DestroyableBehaviour.ReplaceUndoEntryWith(base.transform, debris): the creation action of the turret's root.
        let root = self.with_component_instance(self.sentry_turrets[i].id, |_, inst, _| inst.objects[0].components[0]);
        if let Some(a) = self.undo.history.iter().position(|a| a.RelevantObjects.contains(&root)) {
            let action = &mut self.undo.history[a];
            action.RelevantObjects.clear();
            action.RelevantObjects.push(debris_root);
            action.RelevantObjects.extend(children);
        }
        // Object.Destroy(base.gameObject)
        let id = self.sentry_turrets[i].id;
        self.destroy_object(id);
    }

    /// SentryTurretBehaviour.FixedUpdate
    pub fn sentry_turret_fixed_update(&mut self, i: usize) {
        let p = self.sentry_turrets[i].phys;
        let source = self.sentry_turrets[i].audio;
        if self.physicals[p].OnFire && self.random.value() > 0.9 && !self.audio_is_playing(source) {
            self.sentry_play_sound(i, SentrySet::OnFire);
        }
        if self.physicals[p].BurnProgress > 0.9 || self.physicals[p].Temperature > 2500.0 || self.physicals[p].Charge() > 500.0 {
            self.sentry_turret_explode(i);
        }
        if self.sentry_turrets[i].disabled {
            self.sentry_turrets[i].state = SentryState::Idle;
            return;
        }
        let g = self.sentry_turrets[i].gun;
        let vector = self.sentry_gun_vector(g);
        let barrel = self.barrel_position(self.sentry_guns[g].firearm);
        let mut ctrl = std::mem::take(&mut self.sentry_turrets[i].controller);
        let delta = self.time.deltaTime;
        match self.sentry_turrets[i].state {
            SentryState::Idle => {
                self.sentry_get_target_motor_speed_of(&mut ctrl, p, barrel, vector, delta);
                if ctrl.currentTarget.is_some() {
                    self.sentry_activate(i);
                }
            }
            SentryState::Firing => {
                if ctrl.currentTarget.is_none() {
                    self.sentry_turrets[i].timeStartedSearching = self.time.time;
                    self.sentry_searching(i);
                } else {
                    let target = ctrl.currentTarget.unwrap();
                    self.sentry_get_target_motor_speed_of(&mut ctrl, p, barrel, vector, delta);
                    if ctrl.currentTarget.is_none() && !self.limb_is_considered_alive(target) {
                        self.sentry_turrets[i].state = SentryState::Idle;
                        self.sentry_play_sound(i, SentrySet::Retiring);
                    }
                }
                if self.sentry_turrets[i].soundTimer > 0.135 {
                    self.sentry_turrets[i].soundTimer = 0.0;
                }
            }
            SentryState::Searching => {
                if self.sentry_turrets[i].soundTimer > 1.1 {
                    self.sentry_turrets[i].soundTimer = 0.0;
                }
                if self.time.time - self.sentry_turrets[i].timeStartedSearching > 4.0 {
                    self.sentry_turrets[i].state = SentryState::Idle;
                    self.sentry_play_sound(i, SentrySet::Retiring);
                }
                self.sentry_get_target_motor_speed_of(&mut ctrl, p, barrel, vector, delta);
                if ctrl.currentTarget.is_some() {
                    self.sentry_activate(i);
                }
            }
            SentryState::Panicking => {
                if self.sentry_turrets[i].soundTimer > 0.135 {
                    self.sentry_turrets[i].soundTimer = 0.0;
                }
            }
        }
        self.sentry_turrets[i].controller = ctrl;
        // rb.velocity: a sudden speed-up panics the turret.
        let rb = self.physicals[p].rigidbody.clone();
        let velocity = rb.velocity(&self.world);
        let old = self.sentry_turrets[i].oldVelocity;
        let (dvx, dvy) = (velocity[0] - old[0], velocity[1] - old[1]);
        if dvx * dvx + dvy * dvy > 110.0 && old[0] * old[0] + old[1] * old[1] < velocity[0] * velocity[0] + velocity[1] * velocity[1] {
            self.sentry_panic(i);
        }
        let up = self.transform_up(p);
        let gravity = self.world.gravity;
        let gn = {
            let len = (gravity.x * gravity.x + gravity.y * gravity.y).sqrt();
            [gravity.x / len, gravity.y / len]
        };
        if up[0] * gn[0] + up[1] * gn[1] > -0.4 {
            self.sentry_turrets[i].state = SentryState::Panicking;
            if self.sentry_turrets[i].deathTimer > 1.0 {
                self.sentry_play_sound(i, SentrySet::Tipped);
                self.sentry_turret_disable(i);
            }
        } else if self.sentry_turrets[i].deathTimer > 1.0 {
            self.sentry_turrets[i].state = SentryState::Idle;
        }
        self.sentry_turrets[i].oldVelocity = velocity;
    }

    /// AutomaticSentryController.GetTargetMotorSpeed on `ctrl`, which is the turret's own (taken out of it for the call).
    /// Its OnSight and OnShoot have no listeners on a sentry turret, so the events change nothing.
    fn sentry_get_target_motor_speed_of(&mut self, ctrl: &mut AutomaticSentry, p: usize, barrel: [f32; 2], vector: [f32; 2], delta: f32) {
        let _ = self.sentry_get_target_motor_speed(ctrl, p, barrel, vector, delta);
    }

    /// SentryTurretGunBehaviour.Update
    pub fn sentry_gun_update(&mut self, g: usize) {
        let s = self.sentry_guns[g].sentry;
        let state = self.sentry_turrets[s].state;
        if state == SentryState::Firing || state == SentryState::Panicking {
            self.sentry_guns[g].timer += self.time.deltaTime;
        }
    }

    /// SentryTurretGunBehaviour.FixedUpdate: the joint's motor follows the target (or the sweep) and fires.
    pub fn sentry_gun_fixed_update(&mut self, g: usize) {
        let (h, f, s, node) = (self.sentry_guns[g].joint, self.sentry_guns[g].firearm, self.sentry_guns[g].sentry, self.sentry_guns[g].node);
        if self.hinges[h].destroyed {
            unimplemented!("SentryTurretGunBehaviour.turretJoint destroyed (MissingReferenceException in FixedUpdate)");
        }
        let state = self.sentry_turrets[s].state;
        let (mut speed, torque) = (self.hinges[h].motor[0], self.hinges[h].motor[1]);
        let angle = self.hinges[h].joint_angle(&self.world);
        match state {
            SentryState::Idle => {
                self.sentry_guns[g].canFire = false;
                speed = mathf::clamp(mathf::delta_angle(angle, 0.0) * 3.0, -25.0, 25.0);
            }
            SentryState::Firing => {
                if let Some(target) = self.sentry_turrets[s].controller.currentTarget {
                    let tp = self.physicals[self.limbs[target].PhysicalBehaviour].transform.position;
                    let (gp, _, _) = self.sentry_gun_pose(g);
                    let n = normalized3([tp[0] - gp[0], tp[1] - gp[1], tp[2] - gp[2]]);
                    let mut num = n[1].atan2(n[0]) * 57.29578;
                    num -= self.node_euler_z(node);
                    let root_scale = self.physicals[self.sentry_turret_root(s)].transform.localScale[0];
                    speed = mathf::clamp(root_scale * mathf::delta_angle(angle, 0.0 - num) * 3.0, -35.0, 35.0);
                    if self.sentry_guns[g].timer > 1.0 {
                        self.sentry_guns[g].canFire = true;
                    }
                    if self.sentry_guns[g].canFire && self.sentry_guns[g].timer > 0.15 {
                        self.sentry_guns[g].timer = 0.0;
                        self.firearm_shoot(f);
                    }
                }
            }
            SentryState::Searching => {
                self.sentry_guns[g].canFire = false;
                speed = mathf::clamp(mathf::delta_angle(angle, mathf::sin(self.time.time * 3.0) * 45.0) * 3.0, -25.0, 25.0);
            }
            SentryState::Panicking => {
                speed = mathf::delta_angle(angle, mathf::sin(self.time.time * 6.0) * 45.0) * 3.0;
                if self.sentry_guns[g].timer > 0.15 {
                    self.sentry_guns[g].timer = 0.0;
                    self.firearm_shoot(f);
                }
            }
        }
        self.hinges[h].set_motor(&mut self.world, speed, torque);
    }

    /// The physical object at the top of a turret's hierarchy (its root): the turret's own, since its root is itself.
    fn sentry_turret_root(&self, s: usize) -> usize {
        self.sentry_turrets[s].phys
    }

    /// transform.parent.eulerAngles.z of a node's parent.
    fn node_euler_z(&self, node: usize) -> f32 {
        let parent = self.transform_nodes[node].parent.unwrap_or_else(|| unimplemented!("SentryTurretGunBehaviour: transform.parent of a root (NullReferenceException)"));
        let (_, rot, _) = self.node_pose(parent);
        super::dump::euler_z(rot)
    }
}
