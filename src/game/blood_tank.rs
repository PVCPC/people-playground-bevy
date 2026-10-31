//! BloodTankBehaviour (a BloodContainer): a Use steps Mode (Push, Pull, Idle, Drain; channel 1 steps back). Its container
//! follows the mode's pressure. In Drain it drains at DrainRate and feeds its FancyBloodSplatController (PhysicalParticles),
//! whose particles copy blood into the containers they hit (OnParticleCollide). The splat's spawning, ejection and decal
//! colour are simulated; the particle systems, the liquid bar, the mode display and the context menu are presentation.
#![allow(non_snake_case)]

use super::container::{BloodContainer, Owner};
use super::flask::BLOOD_CONTAINERS;
use super::{script, Game};

/// TankMode values: Push 0, Pull 1, Idle 2, Drain 3.
const DRAIN: i32 = 3;

script! {
    BloodTank {
        DrainRate: f32 = 1.0,
        /// Mode (TankMode): Push 0, Pull 1, Idle 2, Drain 3.
        Mode: i32 = 2,
    }
    base { base: BloodContainer }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// Behaviour.enabled
        enabled: bool = true,
        /// GetComponent<PhysicalBehaviour>(): the physical that TryGetComponent<BloodContainer> finds this container by.
        physical: usize = 0,
        /// PhysicalParticles: the `Game::splats` index of its FancyBloodSplatController.
        splat: usize = 0,
        /// WaterPuffParticles' Transform id: its position decides whether the particles are under water.
        water_puff: i32 = 0,
    }
}

impl Game {
    /// BloodTankBehaviour.Start: UpdateActivation.
    pub fn blood_tank_start(&mut self, t: usize) {
        self.blood_tank_update_activation(t);
    }

    /// BloodTankBehaviour.UpdateActivation: outside Drain the splat's spawning chance is 0 (the particles are presentation).
    fn blood_tank_update_activation(&mut self, t: usize) {
        if self.blood_tanks[t].Mode != DRAIN {
            let s = self.blood_tanks[t].splat;
            self.splats[s].SpawningChance = 0.0;
        }
    }

    /// BloodTankBehaviour.Use: channel 1 steps the mode back, any other channel forward (wrapping through the four modes).
    pub fn blood_tank_use(&mut self, t: usize, channel: i32) {
        let mode = self.blood_tanks[t].Mode;
        self.blood_tanks[t].Mode = if channel != 1 { (mode + 1) % 4 } else { (mode + 3) % 4 };
        self.blood_tank_update_activation(t);
    }

    /// BloodTankBehaviour.Update: the container's update, then the mode's splat settings.
    pub fn blood_tank_update(&mut self, t: usize) {
        let o = Owner::BloodTank(t);
        self.blood_container_update(o);
        let s = self.blood_tanks[t].splat;
        match self.blood_tanks[t].Mode {
            // Push and Pull: no spawning.
            0 | 1 => self.splats[s].SpawningChance = 0.0,
            DRAIN => {
                let amount = self.time.deltaTime * self.blood_tanks[t].DrainRate;
                self.drain(o, amount);
                let submerged = self.blood_tank_puff_submerged(t);
                if submerged {
                    self.splats[s].SpawningChance = 0.0;
                } else {
                    // The drain's particles start at ScaledLiquidAmount * 5 and eject with it: the splat's ejection
                    // force is (0.25, 0.35) times that speed; its decals take the computed colour.
                    let speed = self.scaled_liquid_amount(o) * 5.0;
                    let fallback = self.bc(o).cachedComputedColour;
                    let colour = self.get_computed_color(o, fallback);
                    let total = self.bc(o).TotalLiquidAmount;
                    let splat = &mut self.splats[s];
                    splat.DecalColourMultiplier = colour;
                    splat.EjectionForce = [0.25f32 * speed, 0.35f32 * speed];
                    splat.SpawningChance = total;
                }
            }
            // Idle: nothing.
            _ => {}
        }
    }

    /// IsParticleSubmerged: WaterBehaviour.IsPointUnderWater(WaterPuffParticles.transform.position).
    fn blood_tank_puff_submerged(&mut self, t: usize) -> bool {
        let id = self.blood_tanks[t].water_puff;
        let position = self.with_component_instance(id, |_, inst, o| inst.position(o));
        self.is_point_under_water([position[0], position[1]])
    }

    /// BloodTankBehaviour.OnParticleCollide(hit): the container the hit collider's object holds takes DrainRate * dt
    /// of this tank's blood, proportionally. The object's container is found by TryGetComponent<BloodContainer>.
    pub fn blood_tank_on_particle_collide(&mut self, t: usize, collider: i32) {
        // OnDestroy removed the listener.
        if self.destroyed_components.contains(&self.blood_tanks[t].id) {
            return;
        }
        let target = match self.physical_of_collider(collider) {
            Some(p) => self.physical_container(p),
            None => {
                if self.bare_colliders[&collider].scripts.iter().any(|s| BLOOD_CONTAINERS.contains(&s.as_str())) {
                    unimplemented!("BloodTankBehaviour.OnParticleCollide: a BloodContainer on a collider without a PhysicalBehaviour");
                }
                None
            }
        };
        if let Some(target) = target {
            let amount = self.blood_tanks[t].DrainRate * self.time.deltaTime;
            self.copy_to(Owner::BloodTank(t), amount, target, true);
        }
    }
}
