//! FirearmBehaviour (the guns).
#![allow(non_snake_case)]

use super::ballistics::{Cartridge, Emitter};
use super::misc::FixedIntervalDistributor;
use super::{d, mathf, script, Game};
use crate::assets::db::ObjRef;

script! {
    Firearm {
        barrelPosition: [f32; 2] = [0.0; 2],
        barrelDirection: [f32; 2] = [1.0, 0.0],
        FireDelay: f32 = 0.0,
        InitialInaccuracy: f32 = 0.001,
        Automatic: bool = false,
        BulletsPerShot: i32 = 1,
        AutomaticFireInterval: f32 = 0.05,
        IgnoreUse: bool = false,
        ApplyCharge: bool = false,
        ShotHeatMultiplier: f32 = 1.0,
        MuzzleShockwave: bool = true,
        DistantSound: i32 = 0,
    }
    links {
        /// The component's instance id (its coroutines' owner).
        id: i32 = 0,
        enabled: bool = true,
        /// Phys (GetComponent<PhysicalBehaviour>); None without one.
        physical: Option<usize> = None,
        /// Instance ids of the Prefabs/BulletCasing clone, and of the Prefabs/MuzzleSmoke one (0 with a
        /// MuzzleFlashEffectOverride).
        casing_ids: usize = 0,
        smoke_ids: usize = 0,
        shotIndex: i32 = 0,
        clock: FixedIntervalDistributor = FixedIntervalDistributor::with_rate(60.0),
        /// ShotSounds length.
        ShotSounds: usize = 0,
        InaccuracyMultipliers: Vec<f32> = Vec::new(),
        /// The attachments in BallisticsEmitter.BulletEntryEvent and OnFire (added and removed together), in order.
        attachments: Vec<usize> = Vec::new(),
        MuzzleFlashEffectOverride: Option<ObjRef> = None,
        Cartridge: Cartridge = Cartridge::default(),
        /// LayersToHit (Awake's WaterLayer is Water | LayersToHit).
        LayersToHit: u32 = 0,
        /// BallisticsEmitter (Start).
        emitter: Option<Emitter> = None,
    }
}

impl Game {
    /// FirearmBehaviour.Awake: Instantiate(Prefabs/BulletCasing, transform) (particles only), AddComponent<AudioSource>,
    /// clock.RateHz, and Instantiate(Prefabs/MuzzleSmoke, transform) without an override; the rest sets up audio and
    /// particles.
    pub fn firearm_awake(&mut self, f: usize) {
        for _ in 0..self.firearms[f].casing_ids {
            self.allocate_instance_id();
        }
        // AddComponent<AudioSource>
        self.allocate_instance_id();
        let interval = self.firearms[f].AutomaticFireInterval;
        // RateHz = 1f / AutomaticFireInterval: IntervalSeconds = 1f / (1f / AutomaticFireInterval).
        self.firearms[f].clock.IntervalSeconds = 1.0 / (1.0 / interval);
        for _ in 0..self.firearms[f].smoke_ids {
            self.allocate_instance_id();
        }
    }

    /// FirearmBehaviour.Start: the BallisticsEmitter and SetTraversalSettings.
    pub fn firearm_start(&mut self, f: usize) {
        let g = &self.firearms[f];
        let layers = g.LayersToHit;
        self.firearms[f].emitter = Some(Emitter::new(g.Cartridge, layers, (1 << 4) | layers));
        self.firearm_set_traversal(f);
        self.firearms[f].emitter.as_mut().unwrap().ricochet_callback = true;
    }

    /// SetTraversalSettings
    fn firearm_set_traversal(&mut self, f: usize) {
        let tracers = self.prefs.TracerBullets;
        let speed = self.firearms[f].Cartridge.StartSpeed;
        let e = self.firearms[f].emitter.as_mut().unwrap_or_else(|| unimplemented!("FirearmBehaviour.Shoot before Start (NullReferenceException)"));
        if tracers {
            e.MaxRange = mathf::clamp(speed / 4.0, 4.0, 8.0);
            e.DoTraversal = true;
        } else {
            e.MaxRange = 1000.0;
            e.DoTraversal = false;
        }
    }

    /// FirearmBehaviour.Update
    pub fn firearm_update(&mut self, f: usize) {
        let g = &mut self.firearms[f];
        if g.Automatic && g.AutomaticFireInterval > 0.0 {
            g.clock.IntervalSeconds = g.AutomaticFireInterval;
        }
        let g = &self.firearms[f];
        if g.Automatic && g.physical.is_some_and(|p| self.used_continuously(p)) && !g.IgnoreUse && g.shotIndex != 0 {
            self.firearm_shoot_continuous(f);
        }
    }

    /// FirearmBehaviour.ShootContinuous: the loop condition calls CalculateCycleCount(Time.deltaTime) on every pass.
    pub fn firearm_shoot_continuous(&mut self, f: usize) {
        if self.firearms[f].Automatic {
            let mut i = 0;
            while i < self.firearms[f].clock.calculate_cycle_count(self.time.deltaTime) {
                self.firearm_shoot(f);
                i += 1;
            }
        }
    }

    /// FirearmBehaviour.Use
    pub fn firearm_use(&mut self, f: usize) {
        if self.firearms[f].IgnoreUse {
            return;
        }
        self.firearms[f].shotIndex = 0;
        if self.firearms[f].FireDelay > mathf::EPSILON {
            // DelayedShot: the TriggerSound is audio.
            let (id, delay) = (self.firearms[f].id, self.firearms[f].FireDelay);
            self.start_wait(id, delay, super::destroyable::Step::FirearmShoot(f));
            return;
        }
        self.firearms[f].clock.timeStepAccumulator = 0.0;
        self.firearm_shoot(f);
    }

    /// BarrelPosition: rb.GetRelativePoint(barrelPosition * lossyScale).
    pub(super) fn barrel_position(&mut self, f: usize) -> [f32; 2] {
        let p = self.firearms[f].physical.expect("a FirearmBehaviour without a Rigidbody2D");
        self.rigidbody_relative_point(p, self.firearms[f].barrelPosition)
    }

    /// Rigidbody2D.GetRelativePoint(local * transform.lossyScale) of physical `p`'s body (shared by the barrels).
    pub(super) fn rigidbody_relative_point(&mut self, p: usize, local: [f32; 2]) -> [f32; 2] {
        let s = self.lossy_scale(p);
        let v = [local[0] * s[0], local[1] * s[1]];
        self.sync_pending();
        let xf = self.world.body(self.physicals[p].rigidbody.body).xf;
        [xf.q.c * v[0] - xf.q.s * v[1] + xf.p.x, xf.q.s * v[0] + xf.q.c * v[1] + xf.p.y]
    }

    /// BarrelDirection: transform.TransformVector(barrelDirection) (each node's scale and rotation, no translation).
    pub(super) fn barrel_direction(&self, f: usize) -> [f32; 2] {
        let p = self.firearms[f].physical.expect("a FirearmBehaviour without a Rigidbody2D");
        self.transform_vector_chain(p, self.firearms[f].barrelDirection)
    }

    /// transform.TransformVector(local) of physical `p`'s object: each node's scale and rotation, no translation.
    pub(super) fn transform_vector_chain(&self, p: usize, local: [f32; 2]) -> [f32; 2] {
        let mut v = [local[0], local[1], 0.0, 0.0];
        for node in self.local_chain(p).iter().rev() {
            let node = crate::unity::transform::Trs { t: [0.0; 4], ..*node };
            v = crate::unity::transform::global_position(&node, v);
        }
        [v[0], v[1]]
    }

    /// FirearmBehaviour.Shoot
    pub fn firearm_shoot(&mut self, f: usize) {
        if !self.firearms[f].enabled {
            return;
        }
        let g = &self.firearms[f];
        let p = g.physical.expect("a FirearmBehaviour without a Rigidbody2D");
        let first = g.shotIndex == 0;
        // audioSource.PlayOneShot(ShotSounds.PickRandom()): audio, the draw stays.
        if g.ShotSounds != 0 {
            self.pick_random(self.firearms[f].ShotSounds);
        }
        let kind = self.firearms[f].DistantSound;
        if self.prefs.DistantSoundEffects && kind != 0 {
            // DistantSoundBehaviour.Play: its clips' PickRandom.
            self.pick_random(self.distant_sounds[kind as usize - 1]);
        }
        if first {
            self.firearm_set_traversal(f);
        }
        let c = self.firearms[f].Cartridge;
        let hc = self.heat_capacity(p);
        let heat = self.firearms[f].ShotHeatMultiplier;
        let t = self.physicals[p].Temperature;
        self.physicals[p].Temperature = (d(t) + d(c.Recoil) / hc * 0.05f32 * heat).f();
        self.firearms[f].shotIndex += 1;
        if self.firearms[f].MuzzleShockwave {
            let at = self.barrel_position(f);
            self.create_pulse_explosion([at[0], at[1], 0.0], c.Recoil / 25.0, c.Recoil / 3.0);
        }
        let n = self.firearms[f].BulletsPerShot;
        match self.firearms[f].MuzzleFlashEffectOverride {
            None => {
                // The muzzle smoke's scale and flash are particles.
                let at = self.physicals[p].transform.position;
                let intensity = (d(c.ImpactForce) * 0.8f32 * n as f32 / 0.9f32).f();
                self.camera_shake(intensity, [at[0], at[1]]);
            }
            Some(effect) => {
                // Instantiate(MuzzleFlashEffectOverride, BarrelPosition, rotation); its pose is presentation (link checks
                // it has no physics).
                let at = self.barrel_position(f);
                self.spawn_prefab(effect, [at[0], at[1], 0.0]);
            }
        }
        // The casing is particles.
        for _ in 0..n {
            self.firearm_fire(f);
        }
        // OnFire: audio, particles and TrivialAnimationBehaviour.Play on colliderless children (link checks).
    }

    /// FirearmBehaviour.Fire
    fn firearm_fire(&mut self, f: usize) {
        let p = self.firearms[f].physical.unwrap();
        let recoil = self.firearms[f].Cartridge.Recoil;
        let s = (d(0.5f32) * (0.0 - recoil) * self.lossy_scale(p)[0]).f();
        let right = mathf::rotate(self.physicals[p].transform.rotation, [1.0, 0.0, 0.0]);
        let at = self.barrel_position(f);
        let rb = self.physicals[p].rigidbody.clone();
        rb.add_impulse_at_position(&mut self.world, [s * right[0], s * right[1]], at);
        let charge = if self.firearms[f].ApplyCharge { self.physicals[p].Charge() } else { 0.0 };
        self.firearms[f].emitter.as_mut().unwrap().Charge = charge;
        let origin = self.barrel_position(f);
        let direction = self.barrel_direction(f);
        let g = &self.firearms[f];
        let mut m = 1.0f32;
        for &x in &g.InaccuracyMultipliers {
            m *= x;
        }
        let spread = mathf::max(0.0025, g.InitialInaccuracy * m);
        let r = self.random.inside_unit_circle();
        self.emitter_emit(super::ballistics::Shooter::Firearm(f), origin, [direction[0] + r[0] * spread, direction[1] + r[1] * spread]);
    }
}
