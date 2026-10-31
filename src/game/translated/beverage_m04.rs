//! Translated from the liquid BeverageM04 in Assembly-CSharp.dll by tools/iltrans: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, unreachable_patterns, dead_code, unused_imports, unused_assignments)]

use super::rt;
use crate::game::{mathf, Game};

impl Game {
    /// BeverageM04.OnUpdate
    pub fn beverage_m04_on_update(&mut self, this: usize, mut container: crate::game::container::Owner) {
        let mut circulationBehaviour: Option<usize> = None;
        let mut limb: Option<usize> = None;
        self.beverage_m04_liquid_on_update(this, container);
        if !({ circulationBehaviour = match container { crate::game::container::Owner::Circulation(l) => Some(l), _ => None }; circulationBehaviour.is_some() } && { limb = circulationBehaviour; true }) {
            return;
        }
        let mut s1 = 0usize;
        'c1: loop {
            match s1 {
                0 => {
                    match (match self.limbs[rt::nn(limb)].SpeciesIdentity.clone().as_str() { "Human" => 0, "Gorse" => 1, "Android" => 2, _ => -1 }) {
                        i32::MIN..=-1 | 3..=i32::MAX => {
                            break 'c1;
                        }
                        0 => {
                            s1 = 3; continue 'c1;
                        }
                        1 => {
                            s1 = 2; continue 'c1;
                        }
                        2 => {
                            s1 = 1; continue 'c1;
                        }
                    }
                }
                1 => {
                    { let v = ((((self.physicals[rt::nn(Some(self.limbs[rt::nn(limb)].PhysicalBehaviour))].Charge() as f64) + (self.random.value() as f64))) as f32); self.physicals[rt::nn(Some(self.limbs[rt::nn(limb)].PhysicalBehaviour))].charge = v; };
                    break 'c1;
                }
                2 => {
                    { let v = ((((self.skins[rt::nn(limb)].acidProgress as f64) + (0.05000000074505806 as f64))) as f32); self.skins[rt::nn(limb)].set_acid(v); };
                    if self.nodes[rt::nn(limb)].IsConnectedToRoot {
                        self.people[rt::nn(Some(self.limbs[rt::nn(limb)].Person))].Consciousness = ((((self.people[rt::nn(Some(self.limbs[rt::nn(limb)].Person))].Consciousness as f64) * (0.800000011920929 as f64))) as f32);
                    }
                    break 'c1;
                }
                3 => {
                    if self.nodes[rt::nn(limb)].IsConnectedToRoot {
                        self.people[rt::nn(Some(self.limbs[rt::nn(limb)].Person))].AdrenalineLevel = ((((self.people[rt::nn(Some(self.limbs[rt::nn(limb)].Person))].AdrenalineLevel as f64) + (0.10000000149011612 as f64))) as f32);
                    }
                    break 'c1;
                }
                _ => unreachable!(),
            }
        }
        return;
    }

    /// BeverageM04.OnEnterContainer
    pub fn beverage_m04_on_enter_container(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }

    /// BeverageM04.OnEnterLimb
    pub fn beverage_m04_on_enter_limb(&mut self, this: usize, mut limb: Option<usize>) {
        return;
    }

    /// BeverageM04.OnExitContainer
    pub fn beverage_m04_on_exit_container(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }

    /// Liquid.OnUpdate
    pub fn beverage_m04_liquid_on_update(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }
}
