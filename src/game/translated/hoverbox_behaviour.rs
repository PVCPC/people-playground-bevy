//! Translated from HoverboxBehaviour in Assembly-CSharp.dll by tools/iltrans: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, unreachable_patterns, dead_code, unused_imports, unused_assignments)]

use super::rt;
use crate::assets::db::ObjRef;
use crate::game::explosion::RayHit2D;
use crate::game::{mathf, script, Game};

script! {
    HoverboxBehaviour {
        Activated: bool = false,
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
        Effects: Option<rt::Go> = None,
        Rigidbody: Option<usize> = None,
        PhysicalBehaviour: Option<usize> = None,
    }
}

impl Game {
    /// HoverboxBehaviour.Start
    pub fn hoverbox_behaviour_start(&mut self, this: usize) {
        self.hoverbox_behaviour_set_activation(this);
        return;
    }

    /// HoverboxBehaviour.Use
    pub fn hoverbox_behaviour_use(&mut self, this: usize, mut activation: rt::Activation) {
        if self.tr.hoverbox_behaviour[this].enabled {
            self.tr.hoverbox_behaviour[this].Activated = !self.tr.hoverbox_behaviour[this].Activated;
            self.hoverbox_behaviour_set_activation(this);
        }
        return;
    }

    /// HoverboxBehaviour.FixedUpdate
    pub fn hoverbox_behaviour_fixed_update(&mut self, this: usize) {
        let mut num: f32 = 0.0;
        if self.tr.hoverbox_behaviour[this].Activated {
            { let a1 = rt::mul([self.world.gravity.x, self.world.gravity.y], (((((-1.0 as f64) * (self.physicals[rt::nn(self.tr.hoverbox_behaviour[this].Rigidbody)].rigidbody.gravity_scale(&self.world) as f64)) * (self.physicals[rt::nn(self.tr.hoverbox_behaviour[this].Rigidbody)].rigidbody.mass(&self.world) as f64))) as f32)); self.physicals[rt::nn(self.tr.hoverbox_behaviour[this].Rigidbody)].rigidbody.add_force(&mut self.world, a1) };
            num = mathf::pow(0.8999999761581421, mathf::max(1.0, self.physicals[rt::nn(self.tr.hoverbox_behaviour[this].PhysicalBehaviour)].Charge()));
            { let v = rt::mul(self.physicals[rt::nn(self.tr.hoverbox_behaviour[this].Rigidbody)].rigidbody.velocity(&self.world), num); self.physicals[rt::nn(self.tr.hoverbox_behaviour[this].Rigidbody)].rigidbody.set_velocity(&mut self.world, v); };
            { let v = ((((self.physicals[rt::nn(self.tr.hoverbox_behaviour[this].Rigidbody)].rigidbody.angular_velocity(&self.world) as f64) * (num as f64))) as f32); self.physicals[rt::nn(self.tr.hoverbox_behaviour[this].Rigidbody)].rigidbody.set_angular_velocity(&mut self.world, v); };
        }
        return;
    }

    /// HoverboxBehaviour.OnEnable
    pub fn hoverbox_behaviour_on_enable(&mut self, this: usize) {
        self.hoverbox_behaviour_set_activation(this);
        return;
    }

    /// HoverboxBehaviour.OnDisable
    pub fn hoverbox_behaviour_on_disable(&mut self, this: usize) {
        self.tr.hoverbox_behaviour[this].Activated = false;
        self.hoverbox_behaviour_set_activation(this);
        return;
    }

    /// HoverboxBehaviour.SetActivation
    pub fn hoverbox_behaviour_set_activation(&mut self, this: usize) {
        { let a1 = self.tr.hoverbox_behaviour[this].Activated; self.tr_set_active(rt::nn(self.tr.hoverbox_behaviour[this].Effects), a1) };
        return;
    }
}
