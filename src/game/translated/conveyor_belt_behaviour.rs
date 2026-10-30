//! Translated from ConveyorBeltBehaviour in Assembly-CSharp.dll by tools/iltrans: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, unreachable_patterns, dead_code, unused_imports, unused_assignments)]

use super::rt;
use crate::assets::db::ObjRef;
use crate::game::explosion::RayHit2D;
use crate::game::{mathf, script, Game};

script! {
    ConveyorBeltBehaviour {
        Speed: f32 = 3.0,
        ChargeInfluence: f32 = 1.0,
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
        WheelJoints: Vec<Option<usize>> = Vec::new(),
        PhysicalBehaviour: Option<usize> = None,
    }
}

impl Game {
    /// ConveyorBeltBehaviour.Start
    pub fn conveyor_belt_behaviour_start(&mut self, this: usize) {
        self.conveyor_belt_behaviour_set_activation(this);
        return;
    }

    /// ConveyorBeltBehaviour.Use
    pub fn conveyor_belt_behaviour_use(&mut self, this: usize, mut activation: rt::Activation) {
        if self.tr.conveyor_belt_behaviour[this].enabled {
            self.tr.conveyor_belt_behaviour[this].Activated = !self.tr.conveyor_belt_behaviour[this].Activated;
            self.conveyor_belt_behaviour_set_activation(this);
        }
        return;
    }

    /// ConveyorBeltBehaviour.SetActivation
    pub fn conveyor_belt_behaviour_set_activation(&mut self, this: usize) {
        let mut wheelJoints: Vec<Option<usize>> = Vec::new();
        let mut i: i32 = 0;
        wheelJoints = self.tr.conveyor_belt_behaviour[this].WheelJoints.clone();
        i = 0;
        let mut s1 = 0usize;
        'c1: loop {
            match s1 {
                0 => {
                    if (i < (wheelJoints.len() as i32)) {
                        s1 = 1; continue 'c1;
                    } else {
                        break 'c1;
                    }
                }
                1 => {
                    { let v = self.tr.conveyor_belt_behaviour[this].Activated; { let h = self.tr_hinge(wheelJoints[rt::index(i)]); let v = v; self.hinges[h].set_use_motor(&mut self.world, v); }; };
                    s1 = 2; continue 'c1;
                }
                2 => {
                    i = i.wrapping_add(1);
                    s1 = 0; continue 'c1;
                }
                _ => unreachable!(),
            }
        }
        return;
    }

    /// ConveyorBeltBehaviour.FixedUpdate
    pub fn conveyor_belt_behaviour_fixed_update(&mut self, this: usize) {
        let mut obj: Option<usize> = None;
        let mut motorSpeed: f32 = 0.0;
        let mut wheelJoints: Vec<Option<usize>> = Vec::new();
        let mut i: i32 = 0;
        let mut motor: [f32; 2] = [0.0; 2];
        motorSpeed = (((if self.tr.conveyor_belt_behaviour[this].Activated { (self.conveyor_belt_behaviour_get_final_speed(this) as f64) } else { (0.0 as f64) })) as f32);
        wheelJoints = self.tr.conveyor_belt_behaviour[this].WheelJoints.clone();
        i = 0;
        let mut s1 = 0usize;
        'c1: loop {
            match s1 {
                0 => {
                    if (i < (wheelJoints.len() as i32)) {
                        s1 = 1; continue 'c1;
                    } else {
                        break 'c1;
                    }
                }
                1 => {
                    obj = wheelJoints[rt::index(i)];
                    { let v = self.tr.conveyor_belt_behaviour[this].Activated; { let h = self.tr_hinge(obj); let v = v; self.hinges[h].set_use_motor(&mut self.world, v); }; };
                    motor = self.hinges[self.tr_hinge(obj)].motor;
                    motor[0] = motorSpeed;
                    { let v = motor; { let h = self.tr_hinge(obj); let v = v; self.hinges[h].set_motor(&mut self.world, v[0], v[1]); }; };
                    s1 = 2; continue 'c1;
                }
                2 => {
                    i = i.wrapping_add(1);
                    s1 = 0; continue 'c1;
                }
                _ => unreachable!(),
            }
        }
        return;
    }

    /// ConveyorBeltBehaviour.OnDisable
    pub fn conveyor_belt_behaviour_on_disable(&mut self, this: usize) {
        self.tr.conveyor_belt_behaviour[this].Activated = false;
        self.conveyor_belt_behaviour_set_activation(this);
        return;
    }

    /// ConveyorBeltBehaviour.get_FinalSpeed
    pub fn conveyor_belt_behaviour_get_final_speed(&mut self, this: usize) -> f32 {
        return (((((self.tr.conveyor_belt_behaviour[this].Speed as f64) + ((self.physicals[rt::nn(self.tr.conveyor_belt_behaviour[this].PhysicalBehaviour)].Charge() as f64) * (self.tr.conveyor_belt_behaviour[this].ChargeInfluence as f64))) * ({ let r0 = self.tr_local_scale(rt::nn(Some(rt::Xf::this(self.tr.conveyor_belt_behaviour[this].this_physical, self.tr.conveyor_belt_behaviour[this].this_physics)))); r0[0] } as f64))) as f32);
    }
}
