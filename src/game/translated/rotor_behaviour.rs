//! Translated from RotorBehaviour in Assembly-CSharp.dll by tools/iltrans: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, unreachable_patterns, dead_code, unused_imports, unused_assignments)]

use super::rt;
use crate::assets::db::ObjRef;
use crate::game::explosion::RayHit2D;
use crate::game::{mathf, script, Game};

script! {
    RotorBehaviour {
        Activated: bool = false,
        Speed: f32 = 1.0,
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
        Hinge: Option<usize> = None,
        AudioSource: bool = false,
        PhysicalBehaviour: Option<usize> = None,
        ActiveLight: bool = false,
    }
}

impl Game {
    /// RotorBehaviour.Start
    pub fn rotor_behaviour_start(&mut self, this: usize) {
        let mut buttons: () = ();
        let mut array: () = ();
        let mut contextMenuButton: () = ();
        self.rotor_behaviour_set_speed(this, self.tr.rotor_behaviour[this].Speed);
        { let _ = self.tr.rotor_behaviour[this].Activated; rt::pres(self.tr.rotor_behaviour[this].ActiveLight); };
        buttons = { let _ = rt::nn(self.tr.rotor_behaviour[this].PhysicalBehaviour); };
        array = ();
        contextMenuButton = ();
        contextMenuButton = ();
        buttons;
        return;
    }

    /// RotorBehaviour.SetSpeed
    pub fn rotor_behaviour_set_speed(&mut self, this: usize, mut v_: f32) {
        self.tr.rotor_behaviour[this].Speed = mathf::clamp(v_, -8000.0, 8000.0);
        return;
    }

    /// RotorBehaviour.OnDisable
    pub fn rotor_behaviour_on_disable(&mut self, this: usize) {
        { let v = false; { let h = self.tr_hinge(self.tr.rotor_behaviour[this].Hinge); let v = v; self.hinges[h].set_use_motor(&mut self.world, v); }; };
        return;
    }

    /// RotorBehaviour.OnEnable
    pub fn rotor_behaviour_on_enable(&mut self, this: usize) {
        { let v = true; { let h = self.tr_hinge(self.tr.rotor_behaviour[this].Hinge); let v = v; self.hinges[h].set_use_motor(&mut self.world, v); }; };
        return;
    }

    /// RotorBehaviour.Use
    pub fn rotor_behaviour_use(&mut self, this: usize, mut activation: rt::Activation) {
        if (activation.Channel == 1) {
            self.rotor_behaviour_set_speed(this, ((((0.0 as f64) - (self.tr.rotor_behaviour[this].Speed as f64))) as f32));
            return;
        }
        self.tr.rotor_behaviour[this].Activated = !self.tr.rotor_behaviour[this].Activated;
        { let _ = self.tr.rotor_behaviour[this].Activated; rt::pres(self.tr.rotor_behaviour[this].ActiveLight); };
        return;
    }

    /// RotorBehaviour.GetScaledSpeed
    pub fn rotor_behaviour_get_scaled_speed(&mut self, this: usize) -> f32 {
        return ((((self.tr.rotor_behaviour[this].Speed as f64) * ((((if !((self.tr_local_scale(rt::nn(Some(rt::Xf::this(self.tr.rotor_behaviour[this].this_physical, self.tr.rotor_behaviour[this].this_physics))))[0] as f64) < (0.0 as f64)) { 1 } else { -1 }) as f64) as f32) as f64))) as f32);
    }

    /// RotorBehaviour.FixedUpdate
    pub fn rotor_behaviour_fixed_update(&mut self, this: usize) {
        let mut motor: [f32; 2] = [0.0; 2];
        motor = self.hinges[self.tr_hinge(self.tr.rotor_behaviour[this].Hinge)].motor;
        motor[1] = ((((100.0 as f64) + ((self.physicals[rt::nn(self.tr.rotor_behaviour[this].PhysicalBehaviour)].Charge() as f64) * (4.0 as f64)))) as f32);
        motor[0] = (((if self.tr.rotor_behaviour[this].Activated { ((0.0 as f64) - (self.rotor_behaviour_get_scaled_speed(this) as f64)) } else { (0.0 as f64) })) as f32);
        { let v = motor; { let h = self.tr_hinge(self.tr.rotor_behaviour[this].Hinge); let v = v; self.hinges[h].set_motor(&mut self.world, v[0], v[1]); }; };
        return;
    }
}
