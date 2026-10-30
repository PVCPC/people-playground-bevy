//! Translated from GyrostabiliserBehaviour in Assembly-CSharp.dll by tools/iltrans: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, unreachable_patterns, dead_code, unused_imports, unused_assignments)]

use super::rt;
use crate::assets::db::ObjRef;
use crate::game::explosion::RayHit2D;
use crate::game::{mathf, script, Game};

script! {
    GyrostabiliserBehaviour {
        Intensity: f32 = 1.0,
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
        physicalBehaviour: Option<usize> = None,
        Light: Option<rt::Go> = None,
    }
}

impl Game {
    /// GyrostabiliserBehaviour.Awake
    pub fn gyrostabiliser_behaviour_awake(&mut self, this: usize) {
        self.tr.gyrostabiliser_behaviour[this].physicalBehaviour = self.tr.gyrostabiliser_behaviour[this].this_physical;
        return;
    }

    /// GyrostabiliserBehaviour.Start
    pub fn gyrostabiliser_behaviour_start(&mut self, this: usize) {
        { let a1 = self.tr.gyrostabiliser_behaviour[this].Activated; self.tr_set_active(rt::nn(self.tr.gyrostabiliser_behaviour[this].Light), a1) };
        return;
    }

    /// GyrostabiliserBehaviour.FixedUpdate
    pub fn gyrostabiliser_behaviour_fixed_update(&mut self, this: usize) {
        let mut num: f32 = 0.0;
        let mut num2: f32 = 0.0;
        let mut num3: f32 = 0.0;
        if (self.physicals[rt::nn(self.tr.gyrostabiliser_behaviour[this].physicalBehaviour)].rigidbody.bodyType == 0) {
            num = (((if self.tr.gyrostabiliser_behaviour[this].Activated { (((0.30000001192092896 as f64) / (self.tr.gyrostabiliser_behaviour[this].Intensity as f64)) / (mathf::max(1.0, self.physicals[rt::nn(self.tr.gyrostabiliser_behaviour[this].physicalBehaviour)].Charge()) as f64)) } else { (1.0 as f64) })) as f32);
            { let v = ((((self.physicals[rt::nn(self.tr.gyrostabiliser_behaviour[this].physicalBehaviour)].rigidbody.angular_velocity(&self.world) as f64) * (num as f64))) as f32); self.physicals[rt::nn(self.tr.gyrostabiliser_behaviour[this].physicalBehaviour)].rigidbody.set_angular_velocity(&mut self.world, v); };
            if self.tr.gyrostabiliser_behaviour[this].Activated {
                num2 = ((((self.physicals[rt::nn(self.tr.gyrostabiliser_behaviour[this].physicalBehaviour)].rigidbody.mass(&self.world) as f64) / (1.5 as f64))) as f32);
                num3 = ((((((self.tr.gyrostabiliser_behaviour[this].Intensity as f64) * (mathf::clamp(self.physicals[rt::nn(self.tr.gyrostabiliser_behaviour[this].physicalBehaviour)].Charge(), 1.0, 5.0) as f64)) * (num2 as f64)) * (num2 as f64))) as f32);
                { let v = ((((self.physicals[rt::nn(self.tr.gyrostabiliser_behaviour[this].physicalBehaviour)].rigidbody.angular_velocity(&self.world) as f64) * (mathf::pow(0.5, num3) as f64))) as f32); self.physicals[rt::nn(self.tr.gyrostabiliser_behaviour[this].physicalBehaviour)].rigidbody.set_angular_velocity(&mut self.world, v); };
                { let a2 = (((({ let a1 = self.tr_euler_angles(rt::nn(Some(rt::Xf::this(self.tr.gyrostabiliser_behaviour[this].this_physical, self.tr.gyrostabiliser_behaviour[this].this_physics))))[2]; mathf::delta_angle(a1, 0.0) } as f64) * (num3 as f64))) as f32); self.physicals[rt::nn(self.tr.gyrostabiliser_behaviour[this].physicalBehaviour)].rigidbody.add_torque(&mut self.world, a2) };
            }
        }
        return;
    }

    /// GyrostabiliserBehaviour.Use
    pub fn gyrostabiliser_behaviour_use(&mut self, this: usize, mut activation: rt::Activation) {
        if self.tr.gyrostabiliser_behaviour[this].enabled {
            self.tr.gyrostabiliser_behaviour[this].Activated = !self.tr.gyrostabiliser_behaviour[this].Activated;
            { let a1 = self.tr.gyrostabiliser_behaviour[this].Activated; self.tr_set_active(rt::nn(self.tr.gyrostabiliser_behaviour[this].Light), a1) };
        }
        return;
    }

    /// GyrostabiliserBehaviour.OnDisable
    pub fn gyrostabiliser_behaviour_on_disable(&mut self, this: usize) {
        self.tr_set_active(rt::nn(self.tr.gyrostabiliser_behaviour[this].Light), false);
        return;
    }
}
