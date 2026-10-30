//! Translated from MotorisedHingeBehaviour in Assembly-CSharp.dll by tools/iltrans: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, unreachable_patterns, dead_code, unused_imports, unused_assignments)]

use super::rt;
use crate::assets::db::ObjRef;
use crate::game::explosion::RayHit2D;
use crate::game::{mathf, script, Game};

script! {
    MotorisedHingeBehaviour {
        Activated: bool = false,
        Delta: f32 = 90.0,
        Speed: f32 = 1.0,
        Force: f32 = 1.0,
        VolumeMultiplier: f32 = 0.5,
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
        IndicatorArm: Option<rt::Xf> = None,
        AudioSource: bool = false,
        Hinge: Option<usize> = None,
        PhysicalBehaviour: Option<usize> = None,
        ActiveLight: bool = false,
    }
}

impl Game {
    /// MotorisedHingeBehaviour.Start
    pub fn motorised_hinge_behaviour_start(&mut self, this: usize) {
        let mut buttons: () = ();
        let mut array: () = ();
        let mut contextMenuButton: () = ();
        self.motorised_hinge_behaviour_set_delta(this, self.tr.motorised_hinge_behaviour[this].Delta);
        { let _ = self.tr.motorised_hinge_behaviour[this].Activated; rt::pres(self.tr.motorised_hinge_behaviour[this].ActiveLight); };
        buttons = { let _ = rt::nn(self.tr.motorised_hinge_behaviour[this].PhysicalBehaviour); };
        array = ();
        contextMenuButton = ();
        contextMenuButton = ();
        buttons;
        return;
    }

    /// MotorisedHingeBehaviour.ToggleActivation
    pub fn motorised_hinge_behaviour_toggle_activation(&mut self, this: usize) {
        self.tr.motorised_hinge_behaviour[this].Activated = !self.tr.motorised_hinge_behaviour[this].Activated;
        { let _ = self.tr.motorised_hinge_behaviour[this].Activated; rt::pres(self.tr.motorised_hinge_behaviour[this].ActiveLight); };
        return;
    }

    /// MotorisedHingeBehaviour.Use
    pub fn motorised_hinge_behaviour_use(&mut self, this: usize, mut activation: rt::Activation) {
        self.motorised_hinge_behaviour_toggle_activation(this);
        return;
    }

    /// MotorisedHingeBehaviour.FixedUpdate
    pub fn motorised_hinge_behaviour_fixed_update(&mut self, this: usize) {
        let mut motor: [f32; 2] = [0.0; 2];
        let mut num: f32 = 0.0;
        motor = self.hinges[self.tr_hinge(self.tr.motorised_hinge_behaviour[this].Hinge)].motor;
        motor[0] = (((({ let a1 = self.hinges[self.tr_hinge(self.tr.motorised_hinge_behaviour[this].Hinge)].joint_angle(&self.world); let a2 = (((if self.tr.motorised_hinge_behaviour[this].Activated { (self.motorised_hinge_behaviour_get_scaled_delta(this) as f64) } else { (0.0 as f64) })) as f32); mathf::delta_angle(a1, a2) } as f64) * (self.tr.motorised_hinge_behaviour[this].Speed as f64))) as f32);
        motor[1] = ((((self.tr.motorised_hinge_behaviour[this].Force as f64) * ((1.0 as f64) + (self.physicals[rt::nn(self.tr.motorised_hinge_behaviour[this].PhysicalBehaviour)].Charge() as f64)))) as f32);
        { let v = motor; { let h = self.tr_hinge(self.tr.motorised_hinge_behaviour[this].Hinge); let v = v; self.hinges[h].set_motor(&mut self.world, v[0], v[1]); }; };
        num = (((({ let a3 = self.hinges[self.tr_hinge(self.tr.motorised_hinge_behaviour[this].Hinge)].joint_speed(&self.world); (a3).abs() } as f64) * (0.10000000149011612 as f64))) as f32);
        if ((num as f64) > (0.5 as f64)) {
            { rt::pres(self.tr.motorised_hinge_behaviour[this].AudioSource); };
            { let _ = mathf::clamp01((((((num as f64) * (num as f64)) * (self.tr.motorised_hinge_behaviour[this].VolumeMultiplier as f64))) as f32)); rt::pres(self.tr.motorised_hinge_behaviour[this].AudioSource); };
        } else {
            { rt::pres(self.tr.motorised_hinge_behaviour[this].AudioSource); };
        }
        return;
    }

    /// MotorisedHingeBehaviour.GetScaledDelta
    pub fn motorised_hinge_behaviour_get_scaled_delta(&mut self, this: usize) -> f32 {
        return ((((self.tr.motorised_hinge_behaviour[this].Delta as f64) * ((((if ((self.tr_local_scale(rt::nn(Some(rt::Xf::this(self.tr.motorised_hinge_behaviour[this].this_physical, self.tr.motorised_hinge_behaviour[this].this_physics))))[0] as f64) > (0.0 as f64)) { 1 } else { -1 }) as f64) as f32) as f64))) as f32);
    }

    /// MotorisedHingeBehaviour.OnDisable
    pub fn motorised_hinge_behaviour_on_disable(&mut self, this: usize) {
        { let v = false; { let h = self.tr_hinge(self.tr.motorised_hinge_behaviour[this].Hinge); let v = v; self.hinges[h].set_use_motor(&mut self.world, v); }; };
        { rt::pres(self.tr.motorised_hinge_behaviour[this].AudioSource); };
        return;
    }

    /// MotorisedHingeBehaviour.OnEnable
    pub fn motorised_hinge_behaviour_on_enable(&mut self, this: usize) {
        { let v = true; { let h = self.tr_hinge(self.tr.motorised_hinge_behaviour[this].Hinge); let v = v; self.hinges[h].set_use_motor(&mut self.world, v); }; };
        return;
    }

    /// MotorisedHingeBehaviour.SetDelta
    pub fn motorised_hinge_behaviour_set_delta(&mut self, this: usize, mut v_: f32) {
        self.tr.motorised_hinge_behaviour[this].Delta = v_;
        if self.tr.motorised_hinge_behaviour[this].IndicatorArm.is_some() {
            rt::presentation_pose(rt::nn(self.tr.motorised_hinge_behaviour[this].IndicatorArm));
        }
        return;
    }
}
