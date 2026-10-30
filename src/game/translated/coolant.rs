//! Translated from the liquid Coolant in Assembly-CSharp.dll by tools/iltrans: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, unreachable_patterns, dead_code, unused_imports, unused_assignments)]

use super::rt;
use crate::game::{mathf, Game};

impl Game {
    /// Coolant.OnUpdate
    pub fn coolant_on_update(&mut self, this: usize, mut container: crate::game::container::Owner) {
        let mut circulationBehaviour: Option<usize> = None;
        let mut speciesIdentity: String = String::new();
        if !({ circulationBehaviour = match container { crate::game::container::Owner::Circulation(l) => Some(l), _ => None }; circulationBehaviour.is_some() }) {
            return;
        }
        speciesIdentity = self.limbs[rt::nn(circulationBehaviour)].SpeciesIdentity.clone();
        if ({ let _ = speciesIdentity; true } && (speciesIdentity == "Android")) {
            self.limbs[rt::nn(circulationBehaviour)].InternalTemperature = mathf::lerp(self.limbs[rt::nn(circulationBehaviour)].InternalTemperature, self.limbs[rt::nn(circulationBehaviour)].BodyTemperature, 0.5);
            return;
        }
        if ((self.physicals[rt::nn(Some(self.limbs[rt::nn(circulationBehaviour)].PhysicalBehaviour))].Temperature as f64) < (40.0 as f64)) {
            self.physicals[rt::nn(Some(self.limbs[rt::nn(circulationBehaviour)].PhysicalBehaviour))].Temperature = ((((self.physicals[rt::nn(Some(self.limbs[rt::nn(circulationBehaviour)].PhysicalBehaviour))].Temperature as f64) + (self.random.value() as f64))) as f32);
        }
        if ((self.random.value() as f64) > (0.8999999761581421 as f64)) {
            self.limbs[rt::nn(circulationBehaviour)].InternalTemperature = ((((self.limbs[rt::nn(circulationBehaviour)].InternalTemperature as f64) + (200.0 as f64))) as f32);
        }
        if (self.limbs[rt::nn(circulationBehaviour)].RoughClassification == 0) {
            self.people[rt::nn(Some(self.limbs[rt::nn(circulationBehaviour)].Person))].AngleOffset = ((((self.people[rt::nn(Some(self.limbs[rt::nn(circulationBehaviour)].Person))].AngleOffset as f64) + (self.random.range(-2.0, 2.0) as f64))) as f32);
            if ((self.random.value() as f64) > (0.9800000190734863 as f64)) {
                self.people[rt::nn(Some(self.limbs[rt::nn(circulationBehaviour)].Person))].BrainDamaged = true;
            }
        }
        return;
    }

    /// Coolant.OnEnterContainer
    pub fn coolant_on_enter_container(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }

    /// Coolant.OnEnterLimb
    pub fn coolant_on_enter_limb(&mut self, this: usize, mut limb: Option<usize>) {
        return;
    }

    /// Coolant.OnExitContainer
    pub fn coolant_on_exit_container(&mut self, this: usize, mut container: crate::game::container::Owner) {
        return;
    }
}
