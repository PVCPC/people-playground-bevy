//! Translated from the liquid KnockoutPoisonLiquid in Assembly-CSharp.dll by tools/iltrans: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, unreachable_patterns, dead_code, unused_imports, unused_assignments)]

use super::rt;
use crate::game::{mathf, Game};

impl Game {
    /// KnockoutPoisonLiquid.OnUpdate
    pub fn knockout_poison_liquid_on_update(&mut self, this: usize, mut c: crate::game::container::Owner) {
        let mut circulationBehaviour: Option<usize> = None;
        let mut limb: Option<usize> = None;
        let mut num: f32 = 0.0;
        self.knockout_poison_liquid_temporary_body_liquid_on_update(this, c);
        if (({ circulationBehaviour = match c { crate::game::container::Owner::Circulation(l) => Some(l), _ => None }; circulationBehaviour.is_some() } && { limb = circulationBehaviour; true }) && !(self.limbs[rt::nn(limb)].SpeciesIdentity.clone() == "Android")) {
            num = { let a1 = ((((self.get_amount(crate::game::container::Owner::Circulation(rt::nn(limb)), this) as f64) * (350.0 as f64))) as f32); mathf::clamp01(a1) };
            self.limbs[rt::nn(limb)].Numbness = mathf::max(num, self.limbs[rt::nn(limb)].Numbness);
            if (self.limbs[rt::nn(limb)].RoughClassification == 0) {
                self.people[rt::nn(Some(self.limbs[rt::nn(limb)].Person))].Consciousness = mathf::min(((((1.0 as f64) - (num as f64))) as f32), self.people[rt::nn(Some(self.limbs[rt::nn(limb)].Person))].Consciousness);
            }
        }
        return;
    }

    /// TemporaryBodyLiquid.get_RemovalChancePerSecond
    pub fn knockout_poison_liquid_get_removal_chance_per_second(&mut self, this: usize) -> f32 {
        return self.tr.knockout_poison_liquid__RemovalChancePerSecond__k;
    }

    /// TemporaryBodyLiquid.get_ShouldCallOnEnterEveryUpdate
    pub fn knockout_poison_liquid_get_should_call_on_enter_every_update(&mut self, this: usize) -> bool {
        return true;
    }

    /// KnockoutPoisonLiquid.OnEnterContainer
    pub fn knockout_poison_liquid_on_enter_container(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }

    /// KnockoutPoisonLiquid.OnEnterLimb
    pub fn knockout_poison_liquid_on_enter_limb(&mut self, this: usize, mut limb: Option<usize>) {
        return;
    }

    /// KnockoutPoisonLiquid.OnExitContainer
    pub fn knockout_poison_liquid_on_exit_container(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }

    /// TemporaryBodyLiquid.OnUpdate
    pub fn knockout_poison_liquid_temporary_body_liquid_on_update(&mut self, this: usize, mut c: crate::game::container::Owner) {
        let mut circulationBehaviour: Option<usize> = None;
        if ({ circulationBehaviour = match c { crate::game::container::Owner::Circulation(l) => Some(l), _ => None }; circulationBehaviour.is_some() }) {
            if ((self.random.value() as f64) < ((self.knockout_poison_liquid_get_removal_chance_per_second(this) as f64) * (self.knockout_poison_liquid_get_removal_chance_per_second(this) as f64))) {
                { let a1 = self.get_amount(c, this); self.remove_liquid(c, this, a1) };
            }
            if self.knockout_poison_liquid_get_should_call_on_enter_every_update(this) {
                self.knockout_poison_liquid_on_enter_limb(this, circulationBehaviour);
            }
        }
        return;
    }
}
