//! Translated from the liquid PinkDormantLiquid in Assembly-CSharp.dll by tools/iltrans: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, unreachable_patterns, dead_code, unused_imports, unused_assignments)]

use super::rt;
use crate::game::{mathf, Game};

impl Game {
    /// TemporaryBodyLiquid.OnUpdate
    pub fn pink_dormant_liquid_on_update(&mut self, this: usize, mut c: crate::game::container::Owner) {
        let mut circulationBehaviour: Option<usize> = None;
        if ({ circulationBehaviour = match c { crate::game::container::Owner::Circulation(l) => Some(l), _ => None }; circulationBehaviour.is_some() }) {
            if ((self.random.value() as f64) < ((self.pink_dormant_liquid_get_removal_chance_per_second(this) as f64) * (self.pink_dormant_liquid_get_removal_chance_per_second(this) as f64))) {
                { let a1 = self.get_amount(c, this); self.remove_liquid(c, this, a1) };
            }
            if self.pink_dormant_liquid_get_should_call_on_enter_every_update(this) {
                self.pink_dormant_liquid_on_enter_limb(this, circulationBehaviour);
            }
        }
        return;
    }

    /// TemporaryBodyLiquid.get_RemovalChancePerSecond
    pub fn pink_dormant_liquid_get_removal_chance_per_second(&mut self, this: usize) -> f32 {
        return self.tr.pink_dormant_liquid__RemovalChancePerSecond__k;
    }

    /// TemporaryBodyLiquid.get_ShouldCallOnEnterEveryUpdate
    pub fn pink_dormant_liquid_get_should_call_on_enter_every_update(&mut self, this: usize) -> bool {
        return true;
    }

    /// PinkDormantLiquid.OnEnterLimb
    pub fn pink_dormant_liquid_on_enter_limb(&mut self, this: usize, mut limb: Option<usize>) {
        return;
    }

    /// PinkDormantLiquid.OnEnterContainer
    pub fn pink_dormant_liquid_on_enter_container(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }

    /// PinkDormantLiquid.OnExitContainer
    pub fn pink_dormant_liquid_on_exit_container(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }
}
