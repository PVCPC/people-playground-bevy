//! Translated from FireSoundEmitter in Assembly-CSharp.dll by tools/iltrans: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, unreachable_patterns, dead_code, unused_imports, unused_assignments)]

use super::rt;
use crate::assets::db::ObjRef;
use crate::game::explosion::RayHit2D;
use crate::game::{mathf, script, Game};

script! {
    FireSoundEmitter {
        Intensity: f32 = 1.0,
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
    }
}

impl Game {
    /// FireSoundEmitter.FixedUpdate
    pub fn fire_sound_emitter_fixed_update(&mut self, this: usize) {
        if self.tr_active_in_hierarchy(rt::nn(Some(rt::Go::Local(self.tr.fire_sound_emitter[this].this_id)))) {
            { let _ = self.tr_position(rt::nn(Some(rt::Xf::this(self.tr.fire_sound_emitter[this].this_physical, self.tr.fire_sound_emitter[this].this_physics)))); let _ = mathf::clamp01(self.tr.fire_sound_emitter[this].Intensity); };
        }
        return;
    }
}
