//! MapLiftBehaviour: the Substructure lift, a SliderJoint2D driven toward the requested floor. FixedUpdate, the floor
//! queue and EmergencyStop are rewritten; the audio is presentation. The motor is Perlin-driven (Mathf.PerlinNoise on
//! Time.time). A lift whose machinery is destroyed drops its motor.
#![allow(non_snake_case)]

use super::{mathf, script, Game};

script! {
    MapLift {
        TargetFloorIndex: i32 = 0,
        MotorSpeedMultiplier: f32 = 2.0,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// Behaviour.enabled
        enabled: bool = true,
        /// Floors: the floor heights, in serialized order.
        Floors: Vec<f32> = Vec::new(),
        /// requestIndexList
        requestIndexList: Vec<i32> = Vec::new(),
        /// waitAtFloorTimer
        waitAtFloorTimer: f32 = 0.0,
        /// isEmergencyStopped
        isEmergencyStopped: bool = false,
        /// joint: the SliderJoint2D's `Game::fixed_joints` entry, adopted at the lift's pending start.
        joint: Option<usize> = None,
        /// damagableMachinery (`Game::machinery`)
        Machinery: usize = 0,
    }
}

/// MapLiftBehaviour.FloorWaitDuration
const FLOOR_WAIT_DURATION: f32 = 4.0;

impl Game {
    /// The lift with component id `id` (a UseEventTrigger call's target).
    fn map_lift_index(&self, id: i32) -> usize {
        self.map_lifts.iter().position(|m| m.id == id).expect("MapLiftBehaviour call on an unlinked lift")
    }

    /// Floors[k], with C#'s IndexOutOfRangeException as unimplemented.
    fn map_lift_floor(&self, i: usize, k: i32) -> f32 {
        match self.map_lifts[i].Floors.get(k as usize) {
            Some(&h) if k >= 0 => h,
            _ => unimplemented!("MapLiftBehaviour: Floors[{k}] out of range (IndexOutOfRangeException)"),
        }
    }

    /// MapLiftBehaviour.PopNextNearestTargetFloorIndex. The C# sort's comparer never returns 0, so two queued floors at
    /// the same distance leave the order to the sort algorithm, which is not ported (unimplemented). Otherwise the nearest
    /// queued floor is the sort's first element.
    fn map_lift_pop_nearest(&mut self, i: usize, current: f32) -> i32 {
        if self.map_lifts[i].requestIndexList.is_empty() {
            return self.map_lifts[i].TargetFloorIndex;
        }
        let dist: Vec<f32> = self.map_lifts[i].requestIndexList.iter().map(|&a| (current - self.map_lift_floor(i, a)).abs()).collect();
        let mut best = 0;
        for k in 1..dist.len() {
            if dist[k] == dist[best] {
                unimplemented!("MapLiftBehaviour: two queued floors equally near the lift (List.Sort order not ported)");
            }
            if dist[k] < dist[best] {
                best = k;
            }
        }
        self.map_lifts[i].requestIndexList.remove(best)
    }

    /// MapLiftBehaviour.FixedUpdate
    pub fn map_lift_fixed_update(&mut self, i: usize) {
        let Some(f) = self.map_lifts[i].joint else { return };
        if self.map_lifts[i].Floors.is_empty() {
            return;
        }
        if self.machinery[self.map_lifts[i].Machinery].Destroyed {
            self.slider_joint_set_use_motor(f, false);
        } else if !self.map_lifts[i].isEmergencyStopped {
            self.map_lift_drive(i, f);
        }
    }

    /// The non-destroyed, not stopped branch of FixedUpdate: the floor arrival and queue, then the motor.
    fn map_lift_drive(&mut self, i: usize, f: usize) {
        let count = self.map_lifts[i].Floors.len() as i32;
        self.map_lifts[i].TargetFloorIndex = self.map_lifts[i].TargetFloorIndex.clamp(0, count);
        let jt = self.slider_joint_translation(f);
        let num = self.map_lift_floor(i, self.map_lifts[i].TargetFloorIndex) - jt;
        if num.abs() < 0.1 && self.slider_joint_speed(f) < 0.5 {
            // The arrival sound (liftAudioSource.PlayOneShot) is presentation.
            self.map_lifts[i].waitAtFloorTimer -= self.time.deltaTime;
            if !self.map_lifts[i].requestIndexList.is_empty() && self.map_lifts[i].waitAtFloorTimer <= f32::from_bits(1) {
                let before = self.map_lifts[i].TargetFloorIndex;
                while before == self.map_lifts[i].TargetFloorIndex && !self.map_lifts[i].requestIndexList.is_empty() {
                    self.map_lifts[i].TargetFloorIndex = self.map_lift_pop_nearest(i, jt);
                }
                self.map_lifts[i].waitAtFloorTimer = FLOOR_WAIT_DURATION;
            }
        }
        let (speed, max_torque) = {
            let s = self.fixed_joints[f].slider.as_ref().unwrap();
            (s.motor_speed, s.max_motor_torque)
        };
        let blend = 0.1 * mathf::map_range(0.0, 1.0, 0.25, 1.0, mathf::perlin(self.time.time, 3294.1233));
        let target = (num * 0.8).clamp(-1.0, 1.0) * self.map_lifts[i].MotorSpeedMultiplier;
        let new_speed = mathf::lerp(speed, target, blend);
        self.slider_joint_set_motor(f, new_speed, max_torque);
        self.slider_joint_set_use_motor(f, true);
    }

    /// MapLiftBehaviour.SetFloor(i): the queue of floors, as the UseEventTrigger calls it. Its arrival sounds are presentation.
    pub fn map_lift_set_floor(&mut self, id: i32, i: i32) {
        let k = self.map_lift_index(id);
        let count = self.map_lifts[k].Floors.len() as i32;
        if i < 0 || i >= count {
            return;
        }
        self.map_lifts[k].isEmergencyStopped = false;
        let Some(f) = self.map_lifts[k].joint else { return };
        if self.map_lifts[k].requestIndexList.contains(&i) {
            return;
        }
        let jt = self.slider_joint_translation(f);
        let target = self.map_lifts[k].TargetFloorIndex;
        let flag = target >= 0 && target < count && mathf::approximately(self.map_lifts[k].waitAtFloorTimer, FLOOR_WAIT_DURATION);
        let flag2 = (jt - self.map_lift_floor(k, target.clamp(0, count))).abs() > (jt - self.map_lift_floor(k, i)).abs();
        let num = flag && flag2 && ((jt - self.map_lift_floor(k, i) > 0.0) == (jt - self.map_lift_floor(k, target) > 0.0));
        if self.map_lifts[k].requestIndexList.is_empty() && !flag {
            self.map_lifts[k].TargetFloorIndex = i;
        }
        if num {
            let old = self.map_lifts[k].TargetFloorIndex;
            self.map_lifts[k].requestIndexList.push(old);
            self.map_lifts[k].TargetFloorIndex = i;
        } else {
            self.map_lifts[k].requestIndexList.push(i);
        }
    }

    /// MapLiftBehaviour.EmergencyStop: only while enabled, not destroyed, with a joint that is moving (jointSpeed at least 0.1).
    pub fn map_lift_emergency_stop(&mut self, id: i32) {
        let k = self.map_lift_index(id);
        let Some(f) = self.map_lifts[k].joint else { return };
        if !self.map_lifts[k].enabled || self.machinery[self.map_lifts[k].Machinery].Destroyed || self.slider_joint_speed(f).abs() < 0.1 {
            return;
        }
        self.map_lifts[k].isEmergencyStopped = true;
        let max_torque = self.fixed_joints[f].slider.as_ref().unwrap().max_motor_torque;
        self.slider_joint_set_motor(f, 0.0, max_torque);
        self.slider_joint_set_use_motor(f, true);
        self.map_lifts[k].waitAtFloorTimer = 0.0;
        self.map_lifts[k].requestIndexList.clear();
    }
}
