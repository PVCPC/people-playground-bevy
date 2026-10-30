//! Translated from the liquid NumbingPoison in Assembly-CSharp.dll by tools/iltrans: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, unreachable_patterns, dead_code, unused_imports, unused_assignments)]

use super::rt;
use crate::game::{mathf, Game};

impl Game {
    /// TemporaryBodyLiquid.OnUpdate
    pub fn numbing_poison_on_update(&mut self, this: usize, mut c: crate::game::container::Owner) {
        let mut circulationBehaviour: Option<usize> = None;
        if ({ circulationBehaviour = match c { crate::game::container::Owner::Circulation(l) => Some(l), _ => None }; circulationBehaviour.is_some() }) {
            if ((self.random.value() as f64) < ((self.numbing_poison_get_removal_chance_per_second(this) as f64) * (self.numbing_poison_get_removal_chance_per_second(this) as f64))) {
                { let a1 = self.get_amount(c, this); self.remove_liquid(c, this, a1) };
            }
            if self.numbing_poison_get_should_call_on_enter_every_update(this) {
                self.numbing_poison_on_enter_limb(this, circulationBehaviour);
            }
        }
        return;
    }

    /// TemporaryBodyLiquid.get_RemovalChancePerSecond
    pub fn numbing_poison_get_removal_chance_per_second(&mut self, this: usize) -> f32 {
        return self.tr.numbing_poison__RemovalChancePerSecond__k;
    }

    /// NumbingPoison.get_ShouldCallOnEnterEveryUpdate
    pub fn numbing_poison_get_should_call_on_enter_every_update(&mut self, this: usize) -> bool {
        return false;
    }

    /// NumbingPoison.OnEnterLimb
    pub fn numbing_poison_on_enter_limb(&mut self, this: usize, mut limb: Option<usize>) {
        if !(self.limbs[rt::nn(limb)].SpeciesIdentity.clone() == "Android") {
            self.limbs[rt::nn(limb)].Numbness = ((((self.limbs[rt::nn(limb)].Numbness as f64) + (((self.random.range_int(-1, 1) as f64) as f32) as f64))) as f32);
        }
        return;
    }

    /// NumbingPoison.OnEnterContainer
    pub fn numbing_poison_on_enter_container(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }

    /// NumbingPoison.OnExitContainer
    pub fn numbing_poison_on_exit_container(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }
}
