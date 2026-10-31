//! Translated from the liquid OsteomorphosisAgent in Assembly-CSharp.dll by tools/iltrans: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, unreachable_patterns, dead_code, unused_imports, unused_assignments)]

use super::rt;
use crate::game::{mathf, Game};

impl Game {
    /// TemporaryBodyLiquid.OnUpdate
    pub fn osteomorphosis_agent_on_update(&mut self, this: usize, mut c: crate::game::container::Owner) {
        let mut circulationBehaviour: Option<usize> = None;
        if ({ circulationBehaviour = match c { crate::game::container::Owner::Circulation(l) => Some(l), _ => None }; circulationBehaviour.is_some() }) {
            if ((self.random.value() as f64) < ((self.osteomorphosis_agent_get_removal_chance_per_second(this) as f64) * (self.osteomorphosis_agent_get_removal_chance_per_second(this) as f64))) {
                { let a1 = self.get_amount(c, this); self.remove_liquid(c, this, a1) };
            }
            if self.osteomorphosis_agent_get_should_call_on_enter_every_update(this) {
                self.osteomorphosis_agent_on_enter_limb(this, circulationBehaviour);
            }
        }
        return;
    }

    /// OsteomorphosisAgent.get_RemovalChancePerSecond
    pub fn osteomorphosis_agent_get_removal_chance_per_second(&mut self, this: usize) -> f32 {
        return ((((self.osteomorphosis_agent_temporary_body_liquid_get_removal_chance_per_second(this) as f64) * (0.10000000149011612 as f64))) as f32);
    }

    /// OsteomorphosisAgent.get_ShouldCallOnEnterEveryUpdate
    pub fn osteomorphosis_agent_get_should_call_on_enter_every_update(&mut self, this: usize) -> bool {
        return true;
    }

    /// OsteomorphosisAgent.OnEnterContainer
    pub fn osteomorphosis_agent_on_enter_container(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }

    /// OsteomorphosisAgent.OnEnterLimb
    pub fn osteomorphosis_agent_on_enter_limb(&mut self, this: usize, mut limb: Option<usize>) {
        if !(self.limbs[rt::nn(limb)].SpeciesIdentity.clone() == "Android") {
            { let v = ((((self.skins[rt::nn(limb)].acidProgress as f64) + ((0.10000000149011612 as f64) * (self.random.value() as f64)))) as f32); self.skins[rt::nn(limb)].set_acid(v); };
            if self.nodes[rt::nn(limb)].IsConnectedToRoot {
                self.add_pain(rt::nn(Some(self.limbs[rt::nn(limb)].Person)), 5.0);
            }
        }
        return;
    }

    /// OsteomorphosisAgent.OnExitContainer
    pub fn osteomorphosis_agent_on_exit_container(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }

    /// TemporaryBodyLiquid.get_RemovalChancePerSecond
    pub fn osteomorphosis_agent_temporary_body_liquid_get_removal_chance_per_second(&mut self, this: usize) -> f32 {
        return self.tr.osteomorphosis_agent__RemovalChancePerSecond__k;
    }
}
