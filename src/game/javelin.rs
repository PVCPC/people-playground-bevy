//! JavelinBehaviour (the Missile Launcher's targeting) and JavelinMissileBehaviour (the missile it launches).
//!
//! The launcher casts a laser from RayStart along its right vector (Utils.GetLaserEndPoint, 15000 long): the last hit's
//! point becomes the missile's StaticTargetPoint, and a PhysicalBehaviour on LockOnLayer it ends on becomes its Target
//! (TransferTarget, subscribed to ProjectileLauncherBehaviour.OnLaunch at Start). The behaviour's own OnEnable and
//! FixedUpdate cast the same laser for the line, its colours and the lock-on sound, and TransferTarget casts again, so
//! only that cast is kept.
//!
//! The missile waits TakeOffDelay, then burns upright (a torque toward 90 degrees) until it has climbed ArcHeight and
//! then steers onto the target with a torque, drag against sideways motion and a forward force. It explodes on a hard
//! hit, a shot of more than 40 damage or its ActOnShot, and an EMP breaks it (gravity returns). The trail, glow, fins,
//! sounds and particles are presentation.
#![allow(non_snake_case)]

use super::coroutine::{Routine, Yield};
use super::explosion::{EffectSize, ExplosionParameters};
use super::{d, mathf, script, Game};
use crate::assets::db::ObjRef;
use crate::b2::contacts2d::ScriptCollision;
use crate::unity::transform::{global_vector, Trs};

script! {
    Javelin {
        LockOnLayer: i32 = 0,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// Behaviour.enabled
        enabled: bool = true,
        /// ProjectileLauncher: a `Game::launchers` index.
        launcher: usize = 0,
        /// PhysicalBehaviour (the launcher's own object).
        physical: usize = 0,
        /// RayStart: the local TRS of its object and of each parent below the launcher's own object.
        RayStart: Vec<Trs> = Vec::new(),
        /// LayerMask bits
        LayerMask: u32 = 0,
        /// Start has subscribed TransferTarget to OnLaunch.
        subscribed: bool = false,
    }
}

script! {
    JavelinMissile {
        LocalDirection: [f32; 2] = [1.0, 0.0],
        StaticTargetPoint: [f32; 3] = [0.0, 0.0, 0.0],
        TakeOffDelay: f32 = 1.0,
        ForceIntensity: f32 = 1.0,
        ArcHeight: f32 = 20.0,
        VerticalLiftoffRotationStrength: f32 = 0.5,
        AimingStrength: f32 = 1.0,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// The missile's PhysicalBehaviour; its Rigidbody is the script's Rigidbody.
        physical: usize = 0,
        /// Target: a `Game::physicals` index.
        Target: Option<usize> = None,
        /// The Transform ids of the Trail (TrailParticleEffect's object) and the Glow.
        trail: i32 = 0,
        glow: i32 = 0,
        ExplosionEffect: Option<ObjRef> = None,
        SparkPrefab: Option<ObjRef> = None,
        /// MissileStage: VerticalTakeOff false, TargetedStrike true.
        targeted: bool = false,
        takenOff: bool = false,
        exploded: bool = false,
        broken: bool = false,
        /// The component's index in its object and whether the PhysicalBehaviour is before it (collision order).
        index: usize = 0,
        after_physical: bool = false,
    }
}

impl Game {
    /// JavelinBehaviour.Start: `ProjectileLauncher.OnLaunch += TransferTarget` (the laser line is presentation).
    pub fn javelin_start(&mut self, j: usize) {
        self.javelins[j].subscribed = true;
    }

    /// JavelinBehaviour.CastLaser: the last hit of the laser from RayStart. Returns (endPoint, target); the
    /// rigidbody must exist.
    fn javelin_cast_laser(&mut self, j: usize) -> ([f32; 2], Option<usize>) {
        let s = &self.javelins[j];
        let (p, layers, lock_on) = (s.physical, s.LayerMask, s.LockOnLayer);
        if self.physicals[p].destroyed {
            unimplemented!("JavelinBehaviour.CastLaser with a destroyed Rigidbody2D (it returns and keeps the old target)");
        }
        // localPosition = RayStart.localPosition; localPosition.x *= IsFlipped ? -1 : 1
        let local = self.javelins[j].RayStart[0].t;
        let flipped = self.physicals[p].transform.localScale[0] < 0.0;
        let sign = if flipped { -1.0f32 } else { 1.0 };
        let v = [local[0] * sign, local[1]];
        // rigidbody.GetRelativePoint(localPosition)
        self.sync_pending();
        let xf = self.world.body(self.physicals[p].rigidbody.body).xf;
        let origin = [xf.q.c * v[0] - xf.q.s * v[1] + xf.p.x, xf.q.s * v[0] + xf.q.c * v[1] + xf.p.y];
        // RayStart.TransformVector(Vector3.right): every node's rotation and scale, the object first.
        let nodes: Vec<Trs> = self.javelins[j].RayStart.iter().copied().chain(self.local_chain(p).iter().rev().copied()).collect();
        let mut dir = [1.0, 0.0, 0.0, 0.0];
        for n in &nodes {
            dir = global_vector(n, dir);
        }
        let mut hits = Vec::new();
        self.get_laser_end_point(origin, [dir[0], dir[1]], &mut hits, layers, 15000.0, 64);
        let last = hits.last().expect("a laser hit");
        let (point, physical) = (last.point, last.physical);
        let target = physical.filter(|&q| {
            let go = self.physical_game_object(q);
            self.world.layers[&go] == lock_on
        });
        (point, target)
    }

    /// JavelinBehaviour.TransferTarget(source, projectile) for the launcher `l`'s OnLaunch: a projectile with a
    /// JavelinMissileBehaviour (`missile`, its `Game::javelin_missiles` index) gets the laser's end point and target;
    /// anything else only logs a warning.
    pub fn javelin_on_launch(&mut self, l: usize, missile: Option<usize>) {
        for j in 0..self.javelins.len() {
            if self.javelins[j].launcher != l || !self.javelins[j].subscribed || self.destroyed_components.contains(&self.javelins[j].id) {
                continue;
            }
            let Some(m) = missile else { continue };
            let (end, target) = self.javelin_cast_laser(j);
            self.javelin_missiles[m].StaticTargetPoint = [end[0], end[1], 0.0];
            self.javelin_missiles[m].Target = target;
        }
    }

    /// JavelinMissileBehaviour.Start: the trail is hidden and the TakeOff coroutine starts.
    pub fn javelin_missile_start(&mut self, m: usize) {
        self.javelin_missiles[m].targeted = false;
        let trail = self.javelin_missiles[m].trail;
        self.with_component_instance(trail, |g, inst, o| g.set_active_scripts(inst, o, false));
        let id = self.javelin_missiles[m].id;
        self.start_coroutine(id, id, Self::javelin_missile_take_off_routine(m));
    }

    /// JavelinMissileBehaviour.TakeOff: wait TakeOffDelay, light the trail, switch gravity off and climb ArcHeight.
    fn javelin_missile_take_off_routine(m: usize) -> Routine {
        let mut step = 0;
        Box::new(move |g| {
            let p = g.javelin_missiles[m].physical;
            step += 1;
            match step {
                1 => Yield::WaitForSeconds(g.javelin_missiles[m].TakeOffDelay),
                2 => {
                    let start_y = g.physicals[p].transform.position[1];
                    let trail = g.javelin_missiles[m].trail;
                    g.with_component_instance(trail, |g, inst, o| g.set_active_scripts(inst, o, true));
                    g.javelin_missiles[m].takenOff = true;
                    // The take-off sound is audio.
                    let at = g.physicals[p].transform.position;
                    g.camera_shake(6.0, [at[0], at[1]]);
                    g.javelin_missiles[m].targeted = false;
                    let rb = g.physicals[p].rigidbody.clone();
                    rb.set_gravity_scale(&mut g.world, 0.0);
                    Yield::WaitUntil(Box::new(move |g| g.physicals[p].transform.position[1] > start_y + g.javelin_missiles[m].ArcHeight))
                }
                _ => {
                    g.javelin_missiles[m].targeted = true;
                    Yield::Done
                }
            }
        })
    }

    /// JavelinMissileBehaviour.TargetPosition: the Target's position while it exists, else StaticTargetPoint.
    fn javelin_missile_target_position(&self, m: usize) -> [f32; 2] {
        let s = &self.javelin_missiles[m];
        match s.Target.filter(|&t| !self.physicals[t].destroyed) {
            Some(t) => [self.physicals[t].transform.position[0], self.physicals[t].transform.position[1]],
            None => [s.StaticTargetPoint[0], s.StaticTargetPoint[1]],
        }
    }

    /// JavelinMissileBehaviour.FixedUpdate
    pub fn javelin_missile_fixed_update(&mut self, m: usize) {
        let s = &self.javelin_missiles[m];
        if s.broken || !s.takenOff {
            return;
        }
        let (p, strength, aiming, force, direction, targeted) = (s.physical, s.VerticalLiftoffRotationStrength, s.AimingStrength, s.ForceIntensity, s.LocalDirection, s.targeted);
        let rb = self.physicals[p].rigidbody.clone();
        let pose = self.physicals[p].transform;
        let euler = super::dump::euler_z(pose.rotation);
        if !targeted {
            rb.add_torque(&mut self.world, strength * mathf::delta_angle(euler, 90.0));
        } else {
            let target = self.javelin_missile_target_position(m);
            let at = [pose.position[0], pose.position[1]];
            let magnitude = mathf::magnitude2([target[0] - at[0], target[1] - at[1]]);
            let velocity = rb.velocity(&self.world);
            let vector = match self.javelin_missiles[m].Target.filter(|&t| !self.physicals[t].destroyed) {
                Some(t) => {
                    let tv = self.physicals[t].rigidbody.velocity(&self.world);
                    let own = mathf::magnitude2(velocity);
                    [tv[0] * magnitude / own, tv[1] * magnitude / own]
                }
                None => [0.0, 0.0],
            };
            let normalized = mathf::normalized2([target[0] + vector[0] - at[0], target[1] + vector[1] - at[1]]);
            // Debug.DrawLine and Debug.DrawRay draw nothing.
            let angle = 57.29578f32 * (normalized[1] as f64).atan2(normalized[0] as f64) as f32;
            rb.add_torque(&mut self.world, aiming * mathf::delta_angle(euler, angle));
        }
        let av = rb.angular_velocity(&self.world);
        rb.set_angular_velocity(&mut self.world, av * 0.92f32);
        rb.add_relative_force(&mut self.world, [direction[0] * force, direction[1] * force]);
        self.javelin_missile_aerodynamics(m);
    }

    /// JavelinMissileBehaviour.HandleAerodynamics: a force against the sideways part of the velocity.
    fn javelin_missile_aerodynamics(&mut self, m: usize) {
        let p = self.javelin_missiles[m].physical;
        let pose = self.physicals[p].transform;
        let up = pose.up();
        let vector = [up[0], up[1]];
        let x = pose.localScale[0];
        let rb = self.physicals[p].rigidbody.clone();
        let velocity = rb.velocity(&self.world);
        let magnitude = mathf::magnitude2(velocity);
        let rhs = if magnitude == 0.0 { [1.0, 0.0] } else { [velocity[0] / magnitude, velocity[1] / magnitude] };
        // Vector2.Dot, then * -3f
        let num = (d(vector[0]) * rhs[0] + d(vector[1]) * rhs[1]).f() * -3.0f32;
        let k = (d(num) * x.abs() * magnitude).f();
        rb.add_force(&mut self.world, [k * vector[0], k * vector[1]]);
    }

    /// JavelinMissileBehaviour.OnCollisionEnter2D: a hard hit explodes it.
    pub fn javelin_missile_on_collision_enter(&mut self, m: usize, c: &ScriptCollision) {
        if mathf::sqr_magnitude([c.relative_velocity.x, c.relative_velocity.y]) >= 600.0 {
            self.javelin_missile_explode(m);
        }
    }

    /// JavelinMissileBehaviour.Shot: more than 40 damage explodes it.
    pub fn javelin_missile_shot(&mut self, m: usize, damage: f32) {
        if damage > 40.0 {
            self.javelin_missile_explode(m);
        }
    }

    /// JavelinMissileBehaviour.Explode
    pub fn javelin_missile_explode(&mut self, m: usize) {
        let s = &self.javelin_missiles[m];
        if s.exploded || !s.takenOff {
            return;
        }
        let (p, id, glow, trail, effect) = (s.physical, s.id, s.glow, s.trail, s.ExplosionEffect);
        self.javelin_missiles[m].exploded = true;
        self.with_component_instance(glow, |g, inst, o| g.set_active_scripts(inst, o, false));
        let position = self.physicals[p].transform.position;
        let under_water = self.is_point_under_water([position[0], position[1]]);
        self.create_explosion_with_water(
            under_water,
            ExplosionParameters {
                FragmentationRayCount: 64,
                Position: position,
                Range: 40.0,
                FragmentForce: 90.0,
                CreateParticlesAndSound: false,
                EffectSize: EffectSize::Medium,
                DismemberChance: 0.0,
                BallisticShrapnelCount: 0,
            },
        );
        // CreatePulseExplosion(position, 2f, 40f, soundAndEffects: false): breakObjects defaults to true.
        self.create_pulse_explosion_with(position, 2.0, 40.0, false, true);
        if !self.is_point_under_water([position[0], position[1]]) {
            self.spawn_prefab(effect.expect("JavelinMissileBehaviour.ExplosionEffect: Instantiate(null)"), position);
        }
        // TrailParticleEffect.transform.SetParent(null); Stop is presentation; AddComponent<DeleteAfterTime>().Life = 5f.
        self.set_parent_null_of(trail, |_, _, _| {});
        let (active, key) = self.with_component_instance(trail, |_, inst, o| (inst.objects[o].self_active, inst.objects[o].components[0]));
        if active {
            self.add_delete_after(key, 5.0);
        } else {
            // On an inactive object the component is added but never runs.
            self.allocate_instance_id();
        }
        self.destroy_object(id);
    }

    /// JavelinMissileBehaviour.OnEMPHit: a spark, then the engine dies and gravity returns.
    pub fn javelin_missile_on_emp_hit(&mut self, m: usize) {
        let (p, id, trail, spark) = (self.javelin_missiles[m].physical, self.javelin_missiles[m].id, self.javelin_missiles[m].trail, self.javelin_missiles[m].SparkPrefab);
        if !self.javelin_missiles[m].broken {
            let position = self.physicals[p].transform.position;
            self.spawn_prefab(spark.expect("JavelinMissileBehaviour.SparkPrefab: Instantiate(null)"), position);
        }
        self.javelin_missiles[m].broken = true;
        self.stop_all_coroutines(id);
        // AudioSource.Stop is audio.
        self.with_component_instance(trail, |g, inst, o| g.set_active_scripts(inst, o, false));
        let rb = self.physicals[p].rigidbody.clone();
        rb.set_gravity_scale(&mut self.world, 1.0);
    }

    /// OnEMPHit of the JavelinMissileBehaviour on the GameObject of physical `p`, if it has one.
    pub(super) fn javelin_missile_emp_hit_of(&mut self, p: usize) {
        if let Some(m) = self.javelin_missiles.iter().position(|x| x.physical == p) {
            self.javelin_missile_on_emp_hit(m);
        }
    }
}
