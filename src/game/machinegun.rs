//! MachineGunBehaviour (the Machine Gun and the detached cannons).
#![allow(non_snake_case)]

use super::ballistics::{Cartridge, Emitter, Shooter};
use super::explosion::ExplosionParameters;
use super::{mathf, script, Game};
use crate::assets::db::ObjRef;

script! {
    MachineGun {
        barrelPosition: [f32; 2] = [0.0; 2],
        barrelDirection: [f32; 2] = [0.0; 2],
        ExplosiveRounds: bool = false,
        FireRateMultiplier: f32 = 1.0,
        ShockwaveIntensity: f32 = 0.25,
        Automatic: bool = true,
        DistantSound: i32 = 0,
    }
    links {
        /// The component's instance id (its coroutines' owner).
        id: i32 = 0,
        /// phys and rb (GetComponent in Awake).
        physical: usize = 0,
        /// Sounds length.
        Sounds: usize = 0,
        Cartridge: Cartridge = Cartridge::default(),
        ExplosionSettings: ExplosionParameters = ExplosionParameters::default(),
        /// ImpactEffect and RicochetEffect; their poses are presentation (link checks they have no physics).
        ImpactEffect: Option<ObjRef> = None,
        RicochetEffect: Option<ObjRef> = None,
        t: f32 = 0.0,
        /// BallisticsEmitter (Start).
        emitter: Option<Emitter> = None,
    }
}

/// LayerMask.GetMask("Bounds", "Objects", "CollidingDebris") and LayerMask.GetMask("Water"): BallisticsEmitter's
/// constructor.
const LAYERS_TO_HIT: u32 = (1 << 11) | (1 << 9) | (1 << 13);
const WATER_LAYER: u32 = 1 << 4;

impl Game {
    /// MachineGunBehaviour.Start: Effect's pose for a flipped root is particles.
    pub fn machine_gun_start(&mut self, m: usize) {
        let mut e = Emitter::new(self.machine_guns[m].Cartridge, LAYERS_TO_HIT, WATER_LAYER);
        e.MaxRange = 5.0;
        e.DoTraversal = true;
        self.machine_guns[m].emitter = Some(e);
    }

    /// MachineGunBehaviour.Update: phys.IsBeingUsedContinuously() is false, as nothing in the rewrite calls
    /// UseContinuous, so nothing runs.
    pub fn machine_gun_update(&mut self, _m: usize) {}

    /// MachineGunBehaviour.Use
    pub fn machine_gun_use(&mut self, m: usize) {
        if self.machine_guns[m].Automatic {
            self.machine_guns[m].t = 0.0;
        }
        self.machine_gun_shoot(m);
    }

    /// BarrelPosition: transform.TransformPoint(barrelPosition), as a Vector2.
    fn machine_gun_barrel_position(&self, m: usize) -> [f32; 2] {
        let b = self.machine_guns[m].barrelPosition;
        let p = self.transform_point3(self.machine_guns[m].physical, [b[0], b[1], 0.0]);
        [p[0], p[1]]
    }

    /// BarrelDirection: transform.TransformDirection(barrelDirection) * transform.localScale.x.
    fn machine_gun_barrel_direction(&self, m: usize) -> [f32; 2] {
        let p = self.machine_guns[m].physical;
        let b = self.machine_guns[m].barrelDirection;
        let v = self.transform_direction(p, [b[0], b[1], 0.0]);
        let s = self.physicals[p].transform.localScale[0];
        [v[0] * s, v[1] * s]
    }

    /// MachineGunBehaviour.Shoot; Effect.Play is particles.
    fn machine_gun_shoot(&mut self, m: usize) {
        let g = &self.machine_guns[m];
        let (explosive, settings) = (g.ExplosiveRounds, g.ExplosionSettings);
        let e = self.machine_guns[m].emitter.as_mut().unwrap_or_else(|| unimplemented!("MachineGunBehaviour.Use before Start (NullReferenceException)"));
        e.ExplosiveRound = explosive;
        if explosive {
            e.ExplosiveRoundParams = settings;
        }
        // AudioSource.PlayOneShot(Sounds.PickRandom()): audio, the draw stays.
        self.pick_random(self.machine_guns[m].Sounds);
        let p = self.machine_guns[m].physical;
        let direction = self.machine_gun_barrel_direction(m);
        let at = self.machine_gun_barrel_position(m);
        let rb = self.physicals[p].rigidbody.clone();
        rb.add_impulse_at_position(&mut self.world, [direction[0] * -15.0, direction[1] * -15.0], at);
        let k = self.machine_guns[m].ShockwaveIntensity;
        if k > mathf::EPSILON {
            let at = self.machine_gun_barrel_position(m);
            self.create_pulse_explosion([at[0], at[1], 0.0], 3.0 * k, 4.0 * k);
        }
        let at = self.machine_gun_barrel_position(m);
        let direction = mathf::normalized2(self.machine_gun_barrel_direction(m));
        self.emitter_emit(Shooter::MachineGun(m), at, direction);
        let kind = self.machine_guns[m].DistantSound;
        if self.prefs.DistantSoundEffects && kind != 0 {
            // DistantSoundBehaviour.Play: its clips' PickRandom.
            self.pick_random(self.distant_sounds[kind as usize - 1]);
        }
    }

    /// OnEntry: Instantiate(ImpactEffect, info.Position, identity); its transform.right is presentation.
    pub(super) fn machine_gun_on_entry(&mut self, m: usize, position: [f32; 2], _direction: [f32; 2]) {
        let effect = self.machine_guns[m].ImpactEffect.unwrap_or_else(|| unimplemented!("MachineGunBehaviour.OnEntry: Instantiate(null) (ArgumentException)"));
        self.spawn_prefab(effect, [position[0], position[1], 0.0]);
    }

    /// OnExit: Instantiate(ImpactEffect, info.Position, identity); its localScale and transform.right are presentation.
    pub(super) fn machine_gun_on_exit(&mut self, m: usize, position: [f32; 2], direction: [f32; 2]) {
        self.machine_gun_on_entry(m, position, direction);
    }

    /// OnRicochet: Instantiate(RicochetEffect, info.Position, identity).
    pub(super) fn machine_gun_on_ricochet(&mut self, m: usize, position: [f32; 2]) {
        let effect = self.machine_guns[m].RicochetEffect.unwrap_or_else(|| unimplemented!("MachineGunBehaviour.OnRicochet: Instantiate(null) (ArgumentException)"));
        self.spawn_prefab(effect, [position[0], position[1], 0.0]);
    }
}
