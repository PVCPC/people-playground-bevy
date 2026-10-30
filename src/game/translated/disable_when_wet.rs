//! Translated from decompiled/Assembly-CSharp/DisableWhenWet.cs by tools/translator/cs2rs.py: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, dead_code, unused_imports)]

use super::rt;
use crate::assets::db::ObjRef;
use crate::game::explosion::RayHit2D;
use crate::game::{mathf, script, Game};

script! {
    DisableWhenWet {
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
        ToDisable: Option<rt::Go> = None,
    }
}

impl Game {
    /// DisableWhenWet.Update
    pub fn disable_when_wet_update(&mut self, this: usize) {
        if (((self.physicals[rt::nn(self.tr.disable_when_wet[this].PhysicalBehaviour)].Wetness as f64) > (0.0 as f64)) && self.tr_active_self(rt::nn(self.tr.disable_when_wet[this].ToDisable))) {
            self.tr_set_active(rt::nn(self.tr.disable_when_wet[this].ToDisable), false);
        } else if !self.tr_active_self(rt::nn(self.tr.disable_when_wet[this].ToDisable)) {
            self.tr_set_active(rt::nn(self.tr.disable_when_wet[this].ToDisable), true);
        }
    }
}
