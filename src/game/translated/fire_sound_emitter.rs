//! Translated from decompiled/Assembly-CSharp/FireSoundEmitter.cs by tools/translator/cs2rs.py: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, dead_code, unused_imports)]

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
        }
    }
}
