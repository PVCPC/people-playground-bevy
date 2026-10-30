//! Translated from the liquid CombustionAgent in Assembly-CSharp.dll by tools/iltrans: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, unreachable_patterns, dead_code, unused_imports, unused_assignments)]

use super::rt;
use crate::game::{mathf, Game};

impl Game {
    /// CombustionAgent.OnUpdate
    pub fn combustion_agent_on_update(&mut self, this: usize, mut c: crate::game::container::Owner) {
        let mut circulationBehaviour: Option<usize> = None;
        if (({ circulationBehaviour = match c { crate::game::container::Owner::Circulation(l) => Some(l), _ => None }; circulationBehaviour.is_some() }) && (self.limbs[rt::nn(circulationBehaviour)].SpeciesIdentity.clone() != "Android")) {
            self.physicals[rt::nn(Some(self.limbs[rt::nn(circulationBehaviour)].PhysicalBehaviour))].Temperature = ((((self.physicals[rt::nn(Some(self.limbs[rt::nn(circulationBehaviour)].PhysicalBehaviour))].Temperature as f64) + (10.0 as f64))) as f32);
        }
        self.combustion_agent_liquid_on_update(this, c);
        return;
    }

    /// CombustionAgent.OnEnterLimb
    pub fn combustion_agent_on_enter_limb(&mut self, this: usize, mut limb: Option<usize>) {
        if (!(self.limbs[rt::nn(limb)].SpeciesIdentity.clone() == "Android") && ((self.random.value() as f64) > (0.949999988079071 as f64))) {
            self.ignite(rt::nn(Some(self.limbs[rt::nn(limb)].PhysicalBehaviour)), true);
        }
        return;
    }

    /// CombustionAgent.OnEnterContainer
    pub fn combustion_agent_on_enter_container(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }

    /// CombustionAgent.OnExitContainer
    pub fn combustion_agent_on_exit_container(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }

    /// Liquid.OnUpdate
    pub fn combustion_agent_liquid_on_update(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }
}
