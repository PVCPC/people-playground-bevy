//! Translated from the liquid ImmortalitySerum in Assembly-CSharp.dll by tools/iltrans: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, unreachable_patterns, dead_code, unused_imports, unused_assignments)]

use super::rt;
use crate::game::{mathf, Game};

impl Game {
    /// ImmortalitySerum.OnUpdate
    pub fn immortality_serum_on_update(&mut self, this: usize, mut c: crate::game::container::Owner) {
        let mut circulationBehaviour: Option<usize> = None;
        self.immortality_serum_liquid_on_update(this, c);
        if ({ circulationBehaviour = match c { crate::game::container::Owner::Circulation(l) => Some(l), _ => None }; circulationBehaviour.is_some() }) {
            self.immortality_serum_on_enter_limb(this, circulationBehaviour);
        }
        return;
    }

    /// ImmortalitySerum.OnEnterContainer
    pub fn immortality_serum_on_enter_container(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }

    /// ImmortalitySerum.OnEnterLimb
    pub fn immortality_serum_on_enter_limb(&mut self, this: usize, mut limb: Option<usize>) {
        let mut amountOfBlood: f32 = 0.0;
        if (self.limbs[rt::nn(limb)].SpeciesIdentity.clone() == "Android") {
            if ((self.random.value() as f64) > (0.949999988079071 as f64)) {
                { let a1 = self.tr_position(rt::nn(Some(rt::Xf::Physical(self.limbs[rt::nn(limb)].PhysicalBehaviour)))); self.explosion_creator_explode_at(a1, 5.0) };
            }
            return;
        }
        self.limbs[rt::nn(limb)].Health = self.limbs[rt::nn(limb)].InitialHealth;
        self.limb_heal_bone(rt::nn(limb));
        self.limbs[rt::nn(limb)].DiscomfortingHeatTemperature = 3.4028234663852886E+38;
        self.limbs[rt::nn(limb)].IsLethalToBreak = false;
        self.limbs[rt::nn(limb)].Vitality = 0.0;
        self.heal_bleeding(rt::nn(limb));
        self.limbs[rt::nn(limb)].LungsPunctured = false;
        self.limbs[rt::nn(limb)].ShotDamageMultiplier = 0.0;
        self.limbs[rt::nn(limb)].ImpactPainMultiplier = 0.0;
        self.limbs[rt::nn(limb)].InternalTemperature = self.limbs[rt::nn(limb)].BodyTemperature;
        self.limbs[rt::nn(limb)].BreakingThreshold = 3.4028234663852886E+38;
        self.limbs[rt::nn(limb)].Numbness = 0.0;
        { let v = ((((self.skins[rt::nn(limb)].acidProgress as f64) - (0.5 as f64))) as f32); self.skins[rt::nn(limb)].set_acid(v); };
        self.limbs[rt::nn(limb)].RegenerationSpeed = 0.5;
        self.circulations[rt::nn(limb)].BleedingRate = 0.0;
        self.circulations[rt::nn(limb)].IsPump = self.circulations[rt::nn(limb)].WasInitiallyPumping;
        amountOfBlood = self.amount_of_blood(rt::nn(limb));
        if ((amountOfBlood as f64) < (1.0 as f64)) {
            { let a2 = self.blood_type(rt::nn(limb)); self.add_liquid(crate::game::container::Owner::Circulation(rt::nn(limb)), a2, ((((1.0 as f64) - (amountOfBlood as f64))) as f32)) };
        }
        if self.limbs[rt::nn(limb)].HasBrain {
            self.people[rt::nn(Some(self.limbs[rt::nn(limb)].Person))].PainLevel = 0.0;
            self.people[rt::nn(Some(self.limbs[rt::nn(limb)].Person))].ShockLevel = 0.0;
            self.people[rt::nn(Some(self.limbs[rt::nn(limb)].Person))].Consciousness = 1.0;
            self.people[rt::nn(Some(self.limbs[rt::nn(limb)].Person))].OxygenLevel = 1.0;
            self.people[rt::nn(Some(self.limbs[rt::nn(limb)].Person))].Braindead = false;
            self.people[rt::nn(Some(self.limbs[rt::nn(limb)].Person))].BrainDamaged = false;
        }
        return;
    }

    /// ImmortalitySerum.OnExitContainer
    pub fn immortality_serum_on_exit_container(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }

    /// Liquid.OnUpdate
    pub fn immortality_serum_liquid_on_update(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }
}
