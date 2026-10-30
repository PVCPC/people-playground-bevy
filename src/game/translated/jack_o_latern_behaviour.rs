//! Translated from decompiled/Assembly-CSharp/JackOLaternBehaviour.cs by tools/translator/cs2rs.py: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, dead_code, unused_imports)]

use super::rt;
use crate::assets::db::ObjRef;
use crate::game::explosion::RayHit2D;
use crate::game::{mathf, script, Game};

script! {
    JackOLaternBehaviour {
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
        GlowingSprites: bool = false,
        ObjectSprite: bool = false,
        LightSprite: bool = false,
    }
}

impl Game {
    /// JackOLaternBehaviour.Start
    pub fn jack_o_latern_behaviour_start(&mut self, this: usize) {
        self.jack_o_latern_behaviour_refresh(this);
    }

    /// JackOLaternBehaviour.Refresh
    pub fn jack_o_latern_behaviour_refresh(&mut self, this: usize) {
        let _ = { rt::pres(self.tr.jack_o_latern_behaviour[this].GlowingSprites);  };
        rt::pres(self.tr.jack_o_latern_behaviour[this].LightSprite);
    }
}
