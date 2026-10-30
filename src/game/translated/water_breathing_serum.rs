//! Translated from the liquid WaterBreathingSerum in Assembly-CSharp.dll by tools/iltrans: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, unreachable_patterns, dead_code, unused_imports, unused_assignments)]

use super::rt;
use crate::game::{mathf, Game};

impl Game {
    /// WaterBreathingSerum.OnUpdate
    pub fn water_breathing_serum_on_update(&mut self, this: usize, mut c: crate::game::container::Owner) {
        let mut circulationBehaviour: Option<usize> = None;
        let mut limb: Option<usize> = None;
        self.water_breathing_serum_liquid_on_update(this, c);
        if (({ circulationBehaviour = match c { crate::game::container::Owner::Circulation(l) => Some(l), _ => None }; circulationBehaviour.is_some() } && { limb = circulationBehaviour; true }) && (!(self.limbs[rt::nn(limb)].SpeciesIdentity.as_str() == "Android") && (self.limbs[rt::nn(limb)].RoughClassification == 0))) {
            self.people[rt::nn(Some(self.limbs[rt::nn(limb)].Person))].OxygenLevel = 1.0;
        }
        return;
    }

    /// WaterBreathingSerum.OnEnterContainer
    pub fn water_breathing_serum_on_enter_container(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }

    /// WaterBreathingSerum.OnEnterLimb
    pub fn water_breathing_serum_on_enter_limb(&mut self, this: usize, mut limb: Option<usize>) {
        return;
    }

    /// WaterBreathingSerum.OnExitContainer
    pub fn water_breathing_serum_on_exit_container(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }

    /// Liquid.OnUpdate
    pub fn water_breathing_serum_liquid_on_update(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }
}
