//! Translated from the liquid ExoticLiquid in Assembly-CSharp.dll by tools/iltrans: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, unreachable_patterns, dead_code, unused_imports, unused_assignments)]

use super::rt;
use crate::game::{mathf, Game};

impl Game {
    /// ExoticLiquid.OnEnterContainer
    pub fn exotic_liquid_on_enter_container(&mut self, this: usize, mut container: crate::game::container::Owner) {
        ();
        return;
    }

    /// ExoticLiquid.OnEnterLimb
    pub fn exotic_liquid_on_enter_limb(&mut self, this: usize, mut limb: Option<usize>) {
        if (((self.random.value() as f64) > 0.9) && self.nodes[rt::nn(limb)].IsConnectedToRoot) {
            { let a1 = ((((self.random.value() as f64) * (50.0 as f64))) as f32); self.add_pain(rt::nn(Some(self.limbs[rt::nn(limb)].Person)), a1) };
        }
        if (((self.random.value() as f64) > 0.9) && (self.limbs[rt::nn(limb)].RoughClassification == 0)) {
            self.people[rt::nn(Some(self.limbs[rt::nn(limb)].Person))].Consciousness = ((((self.people[rt::nn(Some(self.limbs[rt::nn(limb)].Person))].Consciousness as f64) * (self.random.value() as f64))) as f32);
        }
        return;
    }

    /// ExoticLiquid.OnExitContainer
    pub fn exotic_liquid_on_exit_container(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }
}
