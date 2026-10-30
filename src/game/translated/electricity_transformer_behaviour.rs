//! Translated from ElectricityTransformerBehaviour in Assembly-CSharp.dll by tools/iltrans: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, unreachable_patterns, dead_code, unused_imports, unused_assignments)]

use super::rt;
use crate::assets::db::ObjRef;
use crate::game::explosion::RayHit2D;
use crate::game::{mathf, script, Game};

script! {
    ElectricityTransformerBehaviour {
    }
    links {
        /// The component's instance id.
        this_id: i32 = 0,
        /// Behaviour.enabled.
        enabled: bool = false,
        /// The PhysicalBehaviour on its GameObject (its Transform's pose).
        this_physical: Option<usize> = None,
        /// Its GameObject or a descendant has a Collider2D or Rigidbody2D.
        this_physics: bool = false,
        PhysicalBehaviour: Option<usize> = None,
    }
}

impl Game {
    /// ElectricityTransformerBehaviour.UseContinuous
    pub fn electricity_transformer_behaviour_use_continuous(&mut self, this: usize, mut activation: rt::Activation) {
        if self.tr.electricity_transformer_behaviour[this].enabled {
            self.physicals[rt::nn(self.tr.electricity_transformer_behaviour[this].PhysicalBehaviour)].charge = ((((self.physicals[rt::nn(self.tr.electricity_transformer_behaviour[this].PhysicalBehaviour)].charge as f64) + (5.0 as f64))) as f32);
        }
        return;
    }

    /// ElectricityTransformerBehaviour.Use
    pub fn electricity_transformer_behaviour_use(&mut self, this: usize, mut activation: rt::Activation) {
        if self.tr.electricity_transformer_behaviour[this].enabled {
            self.physicals[rt::nn(self.tr.electricity_transformer_behaviour[this].PhysicalBehaviour)].charge = ((((self.physicals[rt::nn(self.tr.electricity_transformer_behaviour[this].PhysicalBehaviour)].charge as f64) + (50.0 as f64))) as f32);
        }
        return;
    }
}
