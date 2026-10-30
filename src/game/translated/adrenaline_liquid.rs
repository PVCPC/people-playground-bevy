//! Translated from the liquid AdrenalineLiquid in Assembly-CSharp.dll by tools/iltrans: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, unreachable_patterns, dead_code, unused_imports, unused_assignments)]

use super::rt;
use crate::game::{mathf, Game};

impl Game {
    /// AdrenalineLiquid.OnUpdate
    pub fn adrenaline_liquid_on_update(&mut self, this: usize, mut c: crate::game::container::Owner) {
        let mut circulationBehaviour: Option<usize> = None;
        self.adrenaline_liquid_temporary_body_liquid_on_update(this, c);
        if ({ circulationBehaviour = match c { crate::game::container::Owner::Circulation(l) => Some(l), _ => None }; circulationBehaviour.is_some() }) {
            self.adrenaline_liquid_push_adrenaline(this, circulationBehaviour);
        }
        return;
    }

    /// TemporaryBodyLiquid.get_RemovalChancePerSecond
    pub fn adrenaline_liquid_get_removal_chance_per_second(&mut self, this: usize) -> f32 {
        return self.tr.adrenaline_liquid__RemovalChancePerSecond__k;
    }

    /// TemporaryBodyLiquid.get_ShouldCallOnEnterEveryUpdate
    pub fn adrenaline_liquid_get_should_call_on_enter_every_update(&mut self, this: usize) -> bool {
        return true;
    }

    /// AdrenalineLiquid.OnEnterContainer
    pub fn adrenaline_liquid_on_enter_container(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }

    /// AdrenalineLiquid.OnEnterLimb
    pub fn adrenaline_liquid_on_enter_limb(&mut self, this: usize, mut limb: Option<usize>) {
        self.adrenaline_liquid_push_adrenaline(this, limb);
        return;
    }

    /// AdrenalineLiquid.OnExitContainer
    pub fn adrenaline_liquid_on_exit_container(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }

    /// AdrenalineLiquid.PushAdrenaline
    pub fn adrenaline_liquid_push_adrenaline(&mut self, this: usize, mut limb: Option<usize>) {
        let mut num: f32 = 0.0;
        if !(self.limbs[rt::nn(limb)].SpeciesIdentity.clone() == "Android") {
            num = { let a1 = ((((self.get_amount(crate::game::container::Owner::Circulation(rt::nn(limb)), this) as f64) * (450.0 as f64))) as f32); mathf::clamp01(a1) };
            self.limbs[rt::nn(limb)].Numbness = 0.0;
            if ((num as f64) > (1.399999976158142 as f64)) {
                { let a2 = ((((10.0 as f64) * (self.random.value() as f64))) as f32); self.limb_damage(rt::nn(limb), a2) };
            }
            if (self.limbs[rt::nn(limb)].RoughClassification == 0) {
                self.people[rt::nn(Some(self.limbs[rt::nn(limb)].Person))].AdrenalineLevel = ((((self.people[rt::nn(Some(self.limbs[rt::nn(limb)].Person))].AdrenalineLevel as f64) + ((1.0 as f64) * (num as f64)))) as f32);
                self.people[rt::nn(Some(self.limbs[rt::nn(limb)].Person))].Consciousness = mathf::max(num, self.people[rt::nn(Some(self.limbs[rt::nn(limb)].Person))].Consciousness);
                self.people[rt::nn(Some(self.limbs[rt::nn(limb)].Person))].PainLevel = mathf::max(0.0, ((((self.people[rt::nn(Some(self.limbs[rt::nn(limb)].Person))].PainLevel as f64) - (num as f64))) as f32));
                self.people[rt::nn(Some(self.limbs[rt::nn(limb)].Person))].ShockLevel = mathf::max(0.0, ((((self.people[rt::nn(Some(self.limbs[rt::nn(limb)].Person))].ShockLevel as f64) - (num as f64))) as f32));
            }
        }
        return;
    }

    /// TemporaryBodyLiquid.OnUpdate
    pub fn adrenaline_liquid_temporary_body_liquid_on_update(&mut self, this: usize, mut c: crate::game::container::Owner) {
        let mut circulationBehaviour: Option<usize> = None;
        if ({ circulationBehaviour = match c { crate::game::container::Owner::Circulation(l) => Some(l), _ => None }; circulationBehaviour.is_some() }) {
            if ((self.random.value() as f64) < ((self.adrenaline_liquid_get_removal_chance_per_second(this) as f64) * (self.adrenaline_liquid_get_removal_chance_per_second(this) as f64))) {
                { let a1 = self.get_amount(c, this); self.remove_liquid(c, this, a1) };
            }
            if self.adrenaline_liquid_get_should_call_on_enter_every_update(this) {
                self.adrenaline_liquid_on_enter_limb(this, circulationBehaviour);
            }
        }
        return;
    }
}
