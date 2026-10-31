//! LiquidOutletBehaviour (a BloodContainer, Pull): while its container holds more than 0.1 it sprays a FancyBloodSplatController
//! (PhysicalParticles) and drains 0.1 a second. Its particles copy 0.1 into the containers they hit (OnCollide). Its Update is
//! BloodContainer's. The particle systems and the awake/destroy subscriptions are presentation or the link.
#![allow(non_snake_case)]

use super::container::{BloodContainer, Owner};
use super::{script, Game};

/// LiquidOutletBehaviour.DrainRate
const DRAIN_RATE: f32 = 0.1;

script! {
    LiquidOutlet {}
    base { base: BloodContainer }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// Behaviour.enabled
        enabled: bool = true,
        /// Phys: the outlet's PhysicalBehaviour (its charge sets the spray's speed).
        physical: usize = 0,
        /// PhysicalParticles: the `Game::splats` index of its FancyBloodSplatController.
        splat: usize = 0,
        /// WaterPuffParticles' Transform id: its position decides whether the puff is under water.
        water_puff: i32 = 0,
    }
}

impl Game {
    /// LiquidOutletBehaviour.FixedUpdate. Out of the water the splat's ejection force and decal colour follow the charge;
    /// with more than 0.1 of liquid it sprays one particle and drains 0.1 (the particle systems are presentation).
    pub fn liquid_outlet_fixed_update(&mut self, l: usize) {
        let o = Owner::LiquidOutlet(l);
        let num = self.physicals[self.liquid_outlets[l].physical].Charge() * 0.4 + 2.0;
        let s = self.liquid_outlets[l].splat;
        self.splats[s].SpawningChance = 0.0;
        let puff = self.liquid_outlets[l].water_puff;
        let submerged = self.water_puff_submerged(puff);
        let flag = self.bc(o).TotalLiquidAmount > 0.1;
        if !submerged {
            let fallback = self.bc(o).cachedComputedColour;
            let colour = self.get_computed_color(o, fallback);
            let splat = &mut self.splats[s];
            splat.DecalColourMultiplier = colour;
            splat.EjectionForce = [0.25f32 * num, 0.3f32 * num];
        }
        if flag {
            self.create_splat_particle(s, 0);
            self.drain(o, DRAIN_RATE);
        }
    }

    /// BloodContainer.Update, which LiquidOutletBehaviour does not override.
    pub fn liquid_outlet_update(&mut self, l: usize) {
        self.blood_container_update(Owner::LiquidOutlet(l));
    }

    /// LiquidOutletBehaviour.OnParticleCollide(hit): CopyTo(0.1, the hit object's BloodContainer), proportional. The listener is
    /// gone once the outlet is destroyed.
    pub fn liquid_outlet_on_particle_collide(&mut self, l: usize, collider: i32) {
        if self.destroyed_components.contains(&self.liquid_outlets[l].id) {
            return;
        }
        if let Some(target) = self.particle_hit_container(collider) {
            self.copy_to(Owner::LiquidOutlet(l), DRAIN_RATE, target, true);
        }
    }
}
