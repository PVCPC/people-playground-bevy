//! Translated from the liquid Oil in Assembly-CSharp.dll by tools/iltrans: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, unreachable_patterns, dead_code, unused_imports, unused_assignments)]

use super::rt;
use crate::game::{mathf, Game};

impl Game {
    /// Oil.OnUpdate
    pub fn oil_on_update(&mut self, this: usize, mut container: crate::game::container::Owner) {
        let mut circulationBehaviour: Option<usize> = None;
        if ({ circulationBehaviour = match container { crate::game::container::Owner::Circulation(l) => Some(l), _ => None }; circulationBehaviour.is_some() }) {
            self.oil_on_enter_limb(this, circulationBehaviour);
        }
        return;
    }

    /// Oil.OnEnterContainer
    pub fn oil_on_enter_container(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }

    /// Oil.OnEnterLimb
    pub fn oil_on_enter_limb(&mut self, this: usize, mut limb: Option<usize>) {
        if (self.limbs[rt::nn(limb)].SpeciesIdentity.as_str() != "Android") {
            self.limbs[rt::nn(limb)].Numbness = ((((self.limbs[rt::nn(limb)].Numbness as f64) + (0.5 as f64))) as f32);
            self.circulations[rt::nn(limb)].IsPump = false;
        }
        return;
    }

    /// Oil.OnExitContainer
    pub fn oil_on_exit_container(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }
}
