//! Translated from the liquid ZombiePoisonLiquid in Assembly-CSharp.dll by tools/iltrans: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, unreachable_patterns, dead_code, unused_imports, unused_assignments)]

use super::rt;
use crate::game::{mathf, Game};

impl Game {
    /// Liquid.OnUpdate
    pub fn zombie_poison_liquid_on_update(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }

    /// ZombiePoisonLiquid.OnEnterContainer
    pub fn zombie_poison_liquid_on_enter_container(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }

    /// ZombiePoisonLiquid.OnEnterLimb
    pub fn zombie_poison_liquid_on_enter_limb(&mut self, this: usize, mut limb: Option<usize>) {
        let mut circulationBehaviour: Option<usize> = None;
        let mut amount: f32 = 0.0;
        if !(self.limbs[rt::nn(limb)].SpeciesIdentity.clone() == "Android") {
            self.limbs[rt::nn(limb)].IsZombie = true;
            self.limbs[rt::nn(limb)].Health = self.limbs[rt::nn(limb)].InitialHealth;
            self.limbs[rt::nn(limb)].Numbness = 0.0;
            circulationBehaviour = limb;
            amount = { let a1 = self.blood_type(rt::nn(limb)); self.get_amount(crate::game::container::Owner::Circulation(rt::nn(circulationBehaviour)), a1) };
            if ((amount as f64) < (0.10000000149011612 as f64)) {
                { let a2 = self.blood_type(rt::nn(limb)); self.add_liquid(crate::game::container::Owner::Circulation(rt::nn(circulationBehaviour)), a2, ((((1.0 as f64) - (amount as f64))) as f32)) };
            }
            self.circulations[rt::nn(circulationBehaviour)].BloodFlow = 1.0;
            self.heal_bleeding(rt::nn(circulationBehaviour));
            self.circulations[rt::nn(circulationBehaviour)].BleedingRate = 0.0;
            self.circulations[rt::nn(circulationBehaviour)].IsPump = self.circulations[rt::nn(circulationBehaviour)].WasInitiallyPumping;
            if (self.limbs[rt::nn(limb)].RoughClassification == 0) {
                self.people[rt::nn(Some(self.limbs[rt::nn(limb)].Person))].Braindead = false;
                self.people[rt::nn(Some(self.limbs[rt::nn(limb)].Person))].BrainDamaged = false;
                self.people[rt::nn(Some(self.limbs[rt::nn(limb)].Person))].Consciousness = 1.0;
                self.people[rt::nn(Some(self.limbs[rt::nn(limb)].Person))].ShockLevel = 0.0;
                self.people[rt::nn(Some(self.limbs[rt::nn(limb)].Person))].PainLevel = 0.0;
                self.people[rt::nn(Some(self.limbs[rt::nn(limb)].Person))].OxygenLevel = 1.0;
                self.people[rt::nn(Some(self.limbs[rt::nn(limb)].Person))].AdrenalineLevel = 1.0;
            }
        }
        return;
    }

    /// ZombiePoisonLiquid.OnExitContainer
    pub fn zombie_poison_liquid_on_exit_container(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }
}
