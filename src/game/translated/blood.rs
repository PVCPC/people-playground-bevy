//! Translated from the liquid Blood in Assembly-CSharp.dll by tools/iltrans: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, unreachable_patterns, dead_code, unused_imports, unused_assignments)]

use super::rt;
use crate::game::{mathf, Game};

impl Game {
    /// Blood.OnUpdate
    pub fn blood_on_update(&mut self, this: usize, mut container: crate::game::container::Owner) {
        let mut circulationBehaviour: Option<usize> = None;
        if (({ circulationBehaviour = match container { crate::game::container::Owner::Circulation(l) => Some(l), _ => None }; circulationBehaviour.is_some() }) && ((self.limbs[rt::nn(circulationBehaviour)].SpeciesIdentity.clone() == "Android") && ((self.random.value() as f64) > (0.9950000047683716 as f64)))) {
            { let v = ((((self.physicals[rt::nn(Some(self.limbs[rt::nn(circulationBehaviour)].PhysicalBehaviour))].Charge() as f64) + (5.0 as f64))) as f32); self.physicals[rt::nn(Some(self.limbs[rt::nn(circulationBehaviour)].PhysicalBehaviour))].charge = v; };
        }
        return;
    }

    /// Blood.OnEnterContainer
    pub fn blood_on_enter_container(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }

    /// Blood.OnEnterLimb
    pub fn blood_on_enter_limb(&mut self, this: usize, mut limb: Option<usize>) {
        return;
    }

    /// Blood.OnExitContainer
    pub fn blood_on_exit_container(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }
}
