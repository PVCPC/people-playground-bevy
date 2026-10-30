//! Translated from RocketLauncherBehaviour in Assembly-CSharp.dll by tools/iltrans: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, unreachable_patterns, dead_code, unused_imports, unused_assignments)]

use super::rt;
use crate::assets::db::ObjRef;
use crate::game::explosion::RayHit2D;
use crate::game::{mathf, script, Game};

script! {
    RocketLauncherBehaviour {
        ScreenShakeIntensity: f32 = 0.5,
        barrelPosition: [f32; 2] = [0.0; 2],
        barrelDirection: [f32; 2] = [0.0; 2],
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
        Projectile: Option<rt::Go> = None,
        AudioSource: bool = false,
        LaunchingSounds: usize = 0,
    }
}

impl Game {
    /// RocketLauncherBehaviour.Use
    pub fn rocket_launcher_behaviour_use(&mut self, this: usize, mut activation: rt::Activation) {
        if self.tr.rocket_launcher_behaviour[this].enabled {
            if ((if { let _ = self.tr.rocket_launcher_behaviour[this].LaunchingSounds; true } { (self.tr.rocket_launcher_behaviour[this].LaunchingSounds as i32) } else { 0 }) != 0) {
                { let _ = { let a1 = self.tr.rocket_launcher_behaviour[this].LaunchingSounds; { let _ = self.pick_random(a1); } }; rt::pres(self.tr.rocket_launcher_behaviour[this].AudioSource); };
            } else {
                { let _ = { rt::pres(self.tr.rocket_launcher_behaviour[this].AudioSource); }; rt::pres(self.tr.rocket_launcher_behaviour[this].AudioSource); };
            }
            { let a2 = self.tr.rocket_launcher_behaviour[this].ScreenShakeIntensity; let a3 = self.rocket_launcher_behaviour_get_barrel_position(this); self.camera_shake_at(a2, a3, 1.0) };
            { let r13 = { let r12 = { let a11 = self.tr.rocket_launcher_behaviour[this].Projectile; let a12 = rt::v3(self.rocket_launcher_behaviour_get_barrel_position(this)); self.tr_instantiate(a11, a12, [0.0, 0.0, 0.0, 1.0]) }; Some(self.tr_go_transform(rt::nn(r12))) }; let v = rt::v3(self.rocket_launcher_behaviour_get_barrel_direction(this)); self.tr_set_right(rt::nn(r13), v); };
        }
        return;
    }

    /// RocketLauncherBehaviour.get_BarrelPosition
    pub fn rocket_launcher_behaviour_get_barrel_position(&mut self, this: usize) -> [f32; 2] {
        return rt::v2({ let a1 = rt::v3(self.tr.rocket_launcher_behaviour[this].barrelPosition); self.tr_transform_point(rt::nn(Some(rt::Xf::this(self.tr.rocket_launcher_behaviour[this].this_physical, self.tr.rocket_launcher_behaviour[this].this_physics))), a1) });
    }

    /// RocketLauncherBehaviour.get_BarrelDirection
    pub fn rocket_launcher_behaviour_get_barrel_direction(&mut self, this: usize) -> [f32; 2] {
        return rt::v2(rt::mul({ let a1 = rt::v3(self.tr.rocket_launcher_behaviour[this].barrelDirection); self.tr_transform_direction(rt::nn(Some(rt::Xf::this(self.tr.rocket_launcher_behaviour[this].this_physical, self.tr.rocket_launcher_behaviour[this].this_physics))), a1) }, { let r1 = self.tr_local_scale(rt::nn(Some(rt::Xf::this(self.tr.rocket_launcher_behaviour[this].this_physical, self.tr.rocket_launcher_behaviour[this].this_physics)))); r1[0] }));
    }
}
