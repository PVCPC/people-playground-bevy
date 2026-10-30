//! GlowtubeBehaviour and LEDBulbBehaviour. The sprite colours, the sprite and light toggles and the context menu
//! buttons are rendering and UI; the activation, charge and gas state stay.
#![allow(non_snake_case)]

use super::explosion::{EffectSize, ExplosionParameters};
use super::{d, mathf, script, Game};
use crate::assets::db::ObjRef;

script! {
    Glowtube {
        Broken: bool = false,
        Activated: bool = false,
        GasContents: f32 = 1.0,
        TargetTemperature: f32 = 35.0,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// PhysicalBehaviour
        Phys: usize = 0,
        /// Gas (Break instantiates it)
        Gas: Option<ObjRef> = None,
    }
}

script! {
    LedBulb {
        MinBrightness: f32 = 0.2,
        MaxCharge: f32 = 5.0,
        Activated: bool = false,
        lastIntensity: f32 = f32::MAX,
        colourCached: bool = false,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// PhysicalBehaviour
        Phys: usize = 0,
        /// Behaviour.enabled
        enabled: bool = true,
    }
}

impl Game {
    // ---- GlowtubeBehaviour ----

    /// GlowtubeBehaviour.FixedUpdate
    pub fn glowtube_fixed_update(&mut self, g: usize) {
        let s = &self.glowtubes[g];
        let p = &mut self.physicals[s.Phys];
        if s.Activated && !s.Broken && p.SimulateTemperature {
            p.Temperature = mathf::lerp(p.Temperature, s.TargetTemperature, 0.01);
        }
    }

    /// GlowtubeBehaviour.Update (the sprite colours are rendering)
    pub fn glowtube_update(&mut self, g: usize) {
        let dt = self.time.deltaTime;
        let s = &self.glowtubes[g];
        let p = s.Phys;
        if self.physicals[p].Charge() > 800.0 && !s.Broken {
            self.glowtubes[g].GasContents += dt;
            let position = self.physicals[p].transform.position;
            let under_water = self.is_point_under_water([position[0], position[1]]);
            let ex = ExplosionParameters {
                FragmentationRayCount: 16,
                Position: position,
                Range: 4.0,
                FragmentForce: 5.0,
                CreateParticlesAndSound: true,
                EffectSize: EffectSize::Medium,
                DismemberChance: 0.0,
                BallisticShrapnelCount: 0,
            };
            self.create_explosion_with_water(under_water, ex);
            self.glowtube_break(g);
        }
        if self.glowtubes[g].Broken {
            let c = self.glowtubes[g].GasContents;
            self.glowtubes[g].GasContents = (d(c) - d(dt) * 0.25f32).f();
        }
    }

    /// GlowtubeBehaviour.Use: Activated toggles (UpdateActivation only sets the sprites).
    pub fn glowtube_use(&mut self, g: usize) {
        let s = &mut self.glowtubes[g];
        s.Activated = !s.Activated;
    }

    /// GlowtubeBehaviour.Shot
    pub fn glowtube_shot(&mut self, g: usize, damage: f32) {
        if damage > 50.0 {
            self.glowtube_break(g);
        }
    }

    /// GlowtubeBehaviour.Break: the gas cloud (its colour and Play are particles).
    pub fn glowtube_break(&mut self, g: usize) {
        if self.glowtubes[g].Broken {
            return;
        }
        self.glowtubes[g].Broken = true;
        let gas = self.glowtubes[g].Gas.unwrap_or_else(|| unimplemented!("GlowtubeBehaviour.Break: Object.Instantiate(null) (ArgumentException)"));
        let position = self.physicals[self.glowtubes[g].Phys].transform.position;
        self.spawn_prefab(gas, position);
    }

    // ---- LEDBulbBehaviour ----

    /// LEDBulbBehaviour.Use
    pub fn led_use(&mut self, i: usize) {
        let s = &mut self.leds[i];
        if s.enabled {
            s.Activated = !s.Activated;
            // UpdateActivation
            s.colourCached = false;
        }
    }

    /// LEDBulbBehaviour.OnDisable
    pub fn led_on_disable(&mut self, i: usize) {
        let s = &mut self.leds[i];
        s.Activated = false;
        s.colourCached = false;
    }

    /// LEDBulbBehaviour.Update (the colours are rendering; the intensity cache stays)
    pub fn led_update(&mut self, i: usize) {
        let s = &self.leds[i];
        let num = mathf::map_range(0.0, s.MaxCharge, s.MinBrightness, 1.0, self.physicals[s.Phys].Charge());
        let s = &mut self.leds[i];
        if s.colourCached {
            if !((s.lastIntensity - num).abs() > 0.01) {
                return;
            }
            s.lastIntensity = num;
            s.colourCached = false;
        }
        s.colourCached = true;
    }
}
