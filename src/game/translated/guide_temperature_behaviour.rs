//! Translated from GuideTemperatureBehaviour in Assembly-CSharp.dll by tools/iltrans: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, unreachable_patterns, dead_code, unused_imports, unused_assignments)]

use super::rt;
use crate::assets::db::ObjRef;
use crate::game::explosion::RayHit2D;
use crate::game::{mathf, script, Game};

script! {
    GuideTemperatureBehaviour {
        TargetTemperature: f32 = 0.0,
        GuideSpeed: f32 = 0.0010000000474974513,
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
        phys: Option<usize> = None,
    }
}

impl Game {
    /// GuideTemperatureBehaviour.Awake
    pub fn guide_temperature_behaviour_awake(&mut self, this: usize) {
        self.tr.guide_temperature_behaviour[this].phys = self.tr.guide_temperature_behaviour[this].this_physical;
        return;
    }

    /// GuideTemperatureBehaviour.FixedUpdate
    pub fn guide_temperature_behaviour_fixed_update(&mut self, this: usize) {
        self.physicals[rt::nn(self.tr.guide_temperature_behaviour[this].phys)].Temperature = mathf::lerp(self.physicals[rt::nn(self.tr.guide_temperature_behaviour[this].phys)].Temperature, self.tr.guide_temperature_behaviour[this].TargetTemperature, self.tr.guide_temperature_behaviour[this].GuideSpeed);
        return;
    }
}
