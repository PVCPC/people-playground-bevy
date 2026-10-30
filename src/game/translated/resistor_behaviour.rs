//! Translated from ResistorBehaviour in Assembly-CSharp.dll by tools/iltrans: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, unreachable_patterns, dead_code, unused_imports, unused_assignments)]

use super::rt;
use crate::assets::db::ObjRef;
use crate::game::explosion::RayHit2D;
use crate::game::{mathf, script, Game};

script! {
    ResistorBehaviour {
        ResistorPower: f32 = 0.0,
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
        HandleJoint: Option<usize> = None,
        PhysicalBehaviour: Option<usize> = None,
        flipped: bool = false,
    }
}

impl Game {
    /// ResistorBehaviour.Start
    pub fn resistor_behaviour_start(&mut self, this: usize) {
        self.tr.resistor_behaviour[this].flipped = (({ let r0 = self.tr_lossy_scale(rt::nn(Some(rt::Xf::this(self.tr.resistor_behaviour[this].this_physical, self.tr.resistor_behaviour[this].this_physics)))); r0[0] } as f64) < (0.0 as f64));
        return;
    }

    /// ResistorBehaviour.FixedUpdate
    pub fn resistor_behaviour_fixed_update(&mut self, this: usize) {
        let mut num: f32 = 0.0;
        let mut num2: f32 = 0.0;
        let mut num3: f32 = 0.0;
        num = ((((self.hinges[self.tr_hinge(self.tr.resistor_behaviour[this].HandleJoint)].joint_angle(&self.world) as f64) - ({ let r0 = self.hinges[self.tr_hinge(self.tr.resistor_behaviour[this].HandleJoint)].limits; r0[0] } as f64))) as f32);
        num2 = (((({ let a3 = { let r1 = self.hinges[self.tr_hinge(self.tr.resistor_behaviour[this].HandleJoint)].limits; r1[0] }; (a3).abs() } as f64) + ({ let r3 = self.hinges[self.tr_hinge(self.tr.resistor_behaviour[this].HandleJoint)].limits; r3[1] } as f64))) as f32);
        num3 = mathf::clamp01(((((num as f64) / (num2 as f64))) as f32));
        if self.tr.resistor_behaviour[this].flipped {
            self.tr.resistor_behaviour[this].ResistorPower = num3;
        } else {
            self.tr.resistor_behaviour[this].ResistorPower = ((((1.0 as f64) - (num3 as f64))) as f32);
        }
        self.physicals[rt::nn(self.tr.resistor_behaviour[this].PhysicalBehaviour)].EnergyWireResistance = self.tr.resistor_behaviour[this].ResistorPower;
        return;
    }

    /// ResistorBehaviour.OnDisable
    pub fn resistor_behaviour_on_disable(&mut self, this: usize) {
        self.physicals[rt::nn(self.tr.resistor_behaviour[this].PhysicalBehaviour)].EnergyWireResistance = 1.0;
        return;
    }
}
