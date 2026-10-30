//! Translated from AerialFaithPlateBehaviour in Assembly-CSharp.dll by tools/iltrans: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, unreachable_patterns, dead_code, unused_imports, unused_assignments)]

use super::rt;
use crate::assets::db::ObjRef;
use crate::game::explosion::RayHit2D;
use crate::game::{mathf, script, Game};

script! {
    AerialFaithPlateBehaviour {
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
        launchJoint: Option<usize> = None,
        phys: Option<usize> = None,
        audio: bool = false,
        isBusy: bool = false,
    }
}

impl Game {
    /// AerialFaithPlateBehaviour.Start
    pub fn aerial_faith_plate_behaviour_start(&mut self, this: usize) {
        self.tr.aerial_faith_plate_behaviour[this].isBusy = false;
        return;
    }

    /// AerialFaithPlateBehaviour.Use
    pub fn aerial_faith_plate_behaviour_use(&mut self, this: usize, mut activation: rt::Activation) {
        if self.tr.aerial_faith_plate_behaviour[this].enabled {
            { let r = self.aerial_faith_plate_behaviour_launch(this); let id = self.tr.aerial_faith_plate_behaviour[this].this_id; self.start_coroutine(id, id, r) };
        }
        return;
    }

    /// AerialFaithPlateBehaviour.Launch (an IEnumerator: nothing runs until the Routine steps)
    pub fn aerial_faith_plate_behaviour_launch(&mut self, this: usize) -> crate::game::coroutine::Routine {
        let mut co = AerialFaithPlateBehaviourLaunchCo { s0: 0 };
        Box::new(move |g: &mut Game| g.aerial_faith_plate_behaviour_launch__step(this, &mut co))
    }

    fn aerial_faith_plate_behaviour_launch__step(&mut self, this: usize, co: &mut AerialFaithPlateBehaviourLaunchCo) -> crate::game::coroutine::Yield {
        'c0: loop {
            match co.s0 {
                0 => {
                    co.s0 = if !self.tr.aerial_faith_plate_behaviour[this].isBusy { 1 } else { 2 }; continue 'c0;
                }
                1 => {
                    self.tr.aerial_faith_plate_behaviour[this].isBusy = true;
                    { let v = true; { let h = self.tr_hinge(self.tr.aerial_faith_plate_behaviour[this].launchJoint); let v = v; self.hinges[h].set_use_motor(&mut self.world, v); }; };
                    { rt::pres(self.tr.aerial_faith_plate_behaviour[this].audio); };
                    { let a3 = (((((-800.0 as f64) - ((self.physicals[rt::nn(self.tr.aerial_faith_plate_behaviour[this].phys)].Charge() as f64) * (20.0 as f64))) * ({ let r1 = { let r0 = Some(self.tr_root(rt::nn(Some(rt::Xf::this(self.tr.aerial_faith_plate_behaviour[this].this_physical, self.tr.aerial_faith_plate_behaviour[this].this_physics))))); self.tr_local_scale(rt::nn(r0)) }; r1[0] } as f64))) as f32); self.aerial_faith_plate_behaviour_set_motor_speed(this, a3) };
                    co.s0 = 3;
                    return crate::game::coroutine::Yield::WaitForSeconds(2.0);
                }
                3 => {
                    { let v = false; { let h = self.tr_hinge(self.tr.aerial_faith_plate_behaviour[this].launchJoint); let v = v; self.hinges[h].set_use_motor(&mut self.world, v); }; };
                    co.s0 = 4;
                    return crate::game::coroutine::Yield::WaitForSeconds(1.0);
                }
                4 => {
                    self.tr.aerial_faith_plate_behaviour[this].isBusy = false;
                    co.s0 = 2; continue 'c0;
                }
                2 => {
                    return crate::game::coroutine::Yield::Done;
                }
                _ => unreachable!(),
            }
        }
    }

    /// AerialFaithPlateBehaviour.SetMotorSpeed
    pub fn aerial_faith_plate_behaviour_set_motor_speed(&mut self, this: usize, mut speed: f32) {
        let mut motor: [f32; 2] = [0.0; 2];
        motor = self.hinges[self.tr_hinge(self.tr.aerial_faith_plate_behaviour[this].launchJoint)].motor;
        motor[0] = speed;
        { let v = motor; { let h = self.tr_hinge(self.tr.aerial_faith_plate_behaviour[this].launchJoint); let v = v; self.hinges[h].set_motor(&mut self.world, v[0], v[1]); }; };
        return;
    }
}

/// AerialFaithPlateBehaviour.Launch's state between its steps.
pub struct AerialFaithPlateBehaviourLaunchCo {
    s0: usize,
}
