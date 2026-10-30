//! Translated from decompiled/Assembly-CSharp/CosmeticRotationBehaviour.cs by tools/translator/cs2rs.py: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, dead_code, unused_imports)]

use super::rt;
use crate::assets::db::ObjRef;
use crate::game::explosion::RayHit2D;
use crate::game::{mathf, script, Game};

script! {
    CosmeticRotationBehaviour {
        Axis: i32 = 2,
        RotationSpeedTarget: f32 = 10.0,
        Dampening: f32 = 0.8999999761581421,
        Acceleration: f32 = 0.10000000149011612,
        ShouldRotate: bool = true,
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
        rot: f32 = 0.0,
        vel: f32 = 0.0,
    }
}

impl Game {
    /// CosmeticRotationBehaviour.CurrentSpeed
    pub fn cosmetic_rotation_behaviour_current_speed(&mut self, this: usize) -> f32 {
        self.tr.cosmetic_rotation_behaviour[this].vel
    }

    /// CosmeticRotationBehaviour.Update
    pub fn cosmetic_rotation_behaviour_update(&mut self, this: usize) {
        if self.tr.cosmetic_rotation_behaviour[this].ShouldRotate {
            self.tr.cosmetic_rotation_behaviour[this].vel = mathf::lerp(self.tr.cosmetic_rotation_behaviour[this].vel, self.tr.cosmetic_rotation_behaviour[this].RotationSpeedTarget, mathf::lerp_factor_dt(self.tr.cosmetic_rotation_behaviour[this].Acceleration, self.time.deltaTime));
        }
        self.tr.cosmetic_rotation_behaviour[this].vel = mathf::lerp(self.tr.cosmetic_rotation_behaviour[this].vel, 0.0, mathf::lerp_factor_dt(((((1.0 as f64) - (self.tr.cosmetic_rotation_behaviour[this].Dampening as f64))) as f32), self.time.deltaTime));
        self.tr.cosmetic_rotation_behaviour[this].rot = ((((self.tr.cosmetic_rotation_behaviour[this].rot as f64) + ((self.tr.cosmetic_rotation_behaviour[this].vel as f64) * (self.time.deltaTime as f64)))) as f32);
        match self.tr.cosmetic_rotation_behaviour[this].Axis {
            0 => {
                rt::presentation_rotation(rt::Xf::this(self.tr.cosmetic_rotation_behaviour[this].this_physical, self.tr.cosmetic_rotation_behaviour[this].this_physics));
            }
            1 => {
                rt::presentation_rotation(rt::Xf::this(self.tr.cosmetic_rotation_behaviour[this].this_physical, self.tr.cosmetic_rotation_behaviour[this].this_physics));
            }
            _ => {
                rt::presentation_rotation(rt::Xf::this(self.tr.cosmetic_rotation_behaviour[this].this_physical, self.tr.cosmetic_rotation_behaviour[this].this_physics));
            }
        }
    }
}
