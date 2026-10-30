//! Translated from SirenBehaviour in Assembly-CSharp.dll by tools/iltrans: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, unreachable_patterns, dead_code, unused_imports, unused_assignments)]

use super::rt;
use crate::assets::db::ObjRef;
use crate::game::explosion::RayHit2D;
use crate::game::{mathf, script, Game};

script! {
    SirenBehaviour {
        Activated: bool = false,
        RestartDuration: f32 = 1.0,
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
        AudioSource: bool = false,
        SirenSound: bool = false,
        BrokenSirenSound: bool = false,
        DamagableMachineryBehaviour: Option<usize> = None,
        time: f32 = 0.0,
    }
}

impl Game {
    /// SirenBehaviour.Use
    pub fn siren_behaviour_use(&mut self, this: usize) {
        self.tr.siren_behaviour[this].Activated = !self.tr.siren_behaviour[this].Activated;
        return;
    }

    /// SirenBehaviour.Update
    pub fn siren_behaviour_update(&mut self, this: usize) {
        if self.tr.siren_behaviour[this].Activated {
            self.tr.siren_behaviour[this].time = ((((self.tr.siren_behaviour[this].time as f64) + (self.time.deltaTime as f64))) as f32);
            if ((self.tr.siren_behaviour[this].time as f64) > (self.tr.siren_behaviour[this].RestartDuration as f64)) {
                self.tr.siren_behaviour[this].time = 0.0;
                { let _ = (if self.machinery[rt::nn(self.tr.siren_behaviour[this].DamagableMachineryBehaviour)].Destroyed { self.tr.siren_behaviour[this].BrokenSirenSound } else { self.tr.siren_behaviour[this].SirenSound }); rt::pres(self.tr.siren_behaviour[this].AudioSource); };
            }
        }
        return;
    }
}
