//! Translated from the liquid Nitroglycerine in Assembly-CSharp.dll by tools/iltrans: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, unreachable_patterns, dead_code, unused_imports, unused_assignments)]

use super::rt;
use crate::game::{mathf, Game};

impl Game {
    /// Nitroglycerine.OnUpdate
    pub fn nitroglycerine_on_update(&mut self, this: usize, mut container: crate::game::container::Owner) {
        let mut circulationBehaviour: Option<usize> = None;
        if (({ circulationBehaviour = match container { crate::game::container::Owner::Circulation(l) => Some(l), _ => None }; circulationBehaviour.is_some() }) && ((self.limbs[rt::nn(circulationBehaviour)].SpeciesIdentity.clone() != "Android") && ((self.get_amount(container, this) as f64) > (0.2800000011920929 as f64)))) {
            { let a1 = ((((10.0 as f64) * (self.random.value() as f64))) as f32); self.limb_damage(rt::nn(circulationBehaviour), a1) };
        }
        return;
    }

    /// Nitroglycerine.OnEnterLimb
    pub fn nitroglycerine_on_enter_limb(&mut self, this: usize, mut limb: Option<usize>) {
        self.person_wince(rt::nn(Some(self.limbs[rt::nn(limb)].Person)), 12.0);
        return;
    }
}
