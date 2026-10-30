//! Translated from the liquid VestibularPoison in Assembly-CSharp.dll by tools/iltrans: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, unreachable_patterns, dead_code, unused_imports, unused_assignments)]

use super::rt;
use crate::game::{mathf, Game};

impl Game {
    /// TemporaryBodyLiquid.OnUpdate
    pub fn vestibular_poison_on_update(&mut self, this: usize, mut c: crate::game::container::Owner) {
        let mut circulationBehaviour: Option<usize> = None;
        if ({ circulationBehaviour = match c { crate::game::container::Owner::Circulation(l) => Some(l), _ => None }; circulationBehaviour.is_some() }) {
            if ((self.random.value() as f64) < ((self.vestibular_poison_get_removal_chance_per_second(this) as f64) * (self.vestibular_poison_get_removal_chance_per_second(this) as f64))) {
                { let a1 = self.get_amount(c, this); self.remove_liquid(c, this, a1) };
            }
            if self.vestibular_poison_get_should_call_on_enter_every_update(this) {
                self.vestibular_poison_on_enter_limb(this, circulationBehaviour);
            }
        }
        return;
    }

    /// TemporaryBodyLiquid.get_RemovalChancePerSecond
    pub fn vestibular_poison_get_removal_chance_per_second(&mut self, this: usize) -> f32 {
        return self.tr.vestibular_poison__RemovalChancePerSecond__k;
    }

    /// VestibularPoison.get_ShouldCallOnEnterEveryUpdate
    pub fn vestibular_poison_get_should_call_on_enter_every_update(&mut self, this: usize) -> bool {
        return false;
    }

    /// VestibularPoison.OnEnterLimb
    pub fn vestibular_poison_on_enter_limb(&mut self, this: usize, mut limb: Option<usize>) {
        if !(self.limbs[rt::nn(limb)].SpeciesIdentity.clone() == "Android") {
            self.limbs[rt::nn(limb)].BalanceMuscleMovement = ((((self.limbs[rt::nn(limb)].BalanceMuscleMovement as f64) + (((self.random.range_int(-1, 1) as f64) as f32) as f64))) as f32);
            self.limbs[rt::nn(limb)].DoBalanceJerk = (self.limbs[rt::nn(limb)].DoBalanceJerk ^ ((((self.random.range_int(-1, 1) as f64) as f32) as f64) > (0.699999988079071 as f64)));
            self.limbs[rt::nn(limb)].DoStumble = (self.limbs[rt::nn(limb)].DoStumble ^ ((((self.random.range_int(-1, 1) as f64) as f32) as f64) > (0.699999988079071 as f64)));
            if self.limbs[rt::nn(limb)].HasBrain {
                self.circulations[rt::nn(limb)].InternalBleedingIntensity = ((((self.circulations[rt::nn(limb)].InternalBleedingIntensity as f64) + (self.random.value() as f64))) as f32);
            }
        }
        return;
    }

    /// VestibularPoison.OnEnterContainer
    pub fn vestibular_poison_on_enter_container(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }

    /// VestibularPoison.OnExitContainer
    pub fn vestibular_poison_on_exit_container(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }
}
