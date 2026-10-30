//! Translated from decompiled/Assembly-CSharp/EmitParticlesOnShakeBehaviour.cs by tools/translator/cs2rs.py: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, dead_code, unused_imports)]

use super::rt;
use crate::assets::db::ObjRef;
use crate::game::explosion::RayHit2D;
use crate::game::{mathf, script, Game};

script! {
    EmitParticlesOnShakeBehaviour {
        ShakeThreshold: f32 = 3.0,
        EmissionCount: f32 = 1.0,
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
        rb: Option<usize> = None,
        ps: bool = false,
        previousVelocity: [f32; 2] = [0.0; 2],
    }
}

impl Game {
    /// EmitParticlesOnShakeBehaviour.Awake
    pub fn emit_particles_on_shake_behaviour_awake(&mut self, this: usize) {
        self.tr.emit_particles_on_shake_behaviour[this].ps = self.tr_has_component(self.tr.emit_particles_on_shake_behaviour[this].this_id, 198);
        self.tr.emit_particles_on_shake_behaviour[this].rb = self.tr.emit_particles_on_shake_behaviour[this].this_physical;
    }

    /// EmitParticlesOnShakeBehaviour.FixedUpdate
    pub fn emit_particles_on_shake_behaviour_fixed_update(&mut self, this: usize) {
        let mut velocity: [f32; 2] = self.physicals[rt::nn(self.tr.emit_particles_on_shake_behaviour[this].rb)].rigidbody.velocity(&self.world);
        let mut sqrMagnitude: f32 = mathf::sqr_magnitude((rt::sub(self.tr.emit_particles_on_shake_behaviour[this].previousVelocity, velocity)));
        self.tr.emit_particles_on_shake_behaviour[this].previousVelocity = velocity;
        let mut num: f32 = ((((self.tr.emit_particles_on_shake_behaviour[this].ShakeThreshold as f64) * (self.tr.emit_particles_on_shake_behaviour[this].ShakeThreshold as f64))) as f32);
        if ((sqrMagnitude as f64) > (num as f64)) {
            { let _ = { let (a, b) = (rt::to_int((((((self.tr.emit_particles_on_shake_behaviour[this].EmissionCount as f64) * (((sqrMagnitude as f64) / (num as f64))))) as f32) as f64).ceil()), 4); if a < b { a } else { b } }; rt::pres(self.tr.emit_particles_on_shake_behaviour[this].ps); };
        }
    }
}
