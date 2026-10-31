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

    /// MapLiftBehaviour.PopNextNearestTargetFloorIndex: the queue is sorted with the comparer of the C# lambda (nearest
    /// first), then its first entry is taken off.
    fn map_lift_pop_nearest(&mut self, i: usize, current: f32) -> i32 {
        if self.map_lifts[i].requestIndexList.is_empty() {
            return self.map_lifts[i].TargetFloorIndex;
        }
        let mut list = std::mem::take(&mut self.map_lifts[i].requestIndexList);
        let backing = list.capacity();
        {
            let floors = &self.map_lifts[i].Floors;
            let dist = |a: i32| (current - floors[a as usize]).abs();
            list_sort(&mut list, backing, &|a, b| if dist(a) - dist(b) > 0.0 { 1 } else { -1 });
        }
        let next = list.remove(0);
        self.map_lifts[i].requestIndexList = list;
        next
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

/// List<int>.Sort(Comparison<int>) as the game's mscorlib runs it (ArraySortHelper`1.IntrospectiveSort): insertion sort for
/// up to 16 elements, median-of-three partitioning, heapsort once the depth limit 2 * FloorLog2(backing array length) runs out.
/// `backing` is the list's capacity, which is the array length the C# depth limit reads.
fn list_sort(keys: &mut [i32], backing: usize, cmp: &dyn Fn(i32, i32) -> i32) {
    if keys.len() >= 2 {
        let depth = 2 * (usize::BITS - backing.leading_zeros()) as i32;
        intro_sort(keys, 0, keys.len() as isize - 1, depth, cmp);
    }
}

fn sort_swap_if_greater(keys: &mut [i32], cmp: &dyn Fn(i32, i32) -> i32, a: isize, b: isize) {
    if a != b && cmp(keys[a as usize], keys[b as usize]) > 0 {
        keys.swap(a as usize, b as usize);
    }
}

fn intro_sort(keys: &mut [i32], mut lo: isize, mut hi: isize, mut depth: i32, cmp: &dyn Fn(i32, i32) -> i32) {
    while hi > lo {
        let num = hi - lo + 1;
        if num <= 16 {
            match num {
                1 => {}
                2 => sort_swap_if_greater(keys, cmp, lo, hi),
                3 => {
                    sort_swap_if_greater(keys, cmp, lo, hi - 1);
                    sort_swap_if_greater(keys, cmp, lo, hi);
                    sort_swap_if_greater(keys, cmp, hi - 1, hi);
                }
                _ => insertion_sort(keys, lo, hi, cmp),
            }
            break;
        }
        if depth == 0 {
            heapsort(keys, lo, hi, cmp);
            break;
        }
        depth -= 1;
        let p = pick_pivot_and_partition(keys, lo, hi, cmp);
        intro_sort(keys, p + 1, hi, depth, cmp);
        hi = p - 1;
    }
}

fn pick_pivot_and_partition(keys: &mut [i32], lo: isize, hi: isize, cmp: &dyn Fn(i32, i32) -> i32) -> isize {
    let num = lo + (hi - lo) / 2;
    sort_swap_if_greater(keys, cmp, lo, num);
    sort_swap_if_greater(keys, cmp, lo, hi);
    sort_swap_if_greater(keys, cmp, num, hi);
    let val = keys[num as usize];
    keys.swap(num as usize, (hi - 1) as usize);
    let (mut i, mut j) = (lo, hi - 1);
    while i < j {
        loop {
            i += 1;
            if cmp(keys[i as usize], val) >= 0 {
                break;
            }
        }
        loop {
            j -= 1;
            if cmp(val, keys[j as usize]) >= 0 {
                break;
            }
        }
        if i >= j {
            break;
        }
        keys.swap(i as usize, j as usize);
    }
    keys.swap(i as usize, (hi - 1) as usize);
    i
}

fn heapsort(keys: &mut [i32], lo: isize, hi: isize, cmp: &dyn Fn(i32, i32) -> i32) {
    let n = hi - lo + 1;
    let mut i = n / 2;
    while i >= 1 {
        down_heap(keys, i, n, lo, cmp);
        i -= 1;
    }
    let mut k = n;
    while k > 1 {
        keys.swap(lo as usize, (lo + k - 1) as usize);
        down_heap(keys, 1, k - 1, lo, cmp);
        k -= 1;
    }
}

fn down_heap(keys: &mut [i32], mut i: isize, n: isize, lo: isize, cmp: &dyn Fn(i32, i32) -> i32) {
    let val = keys[(lo + i - 1) as usize];
    while i <= n / 2 {
        let mut c = 2 * i;
        if c < n && cmp(keys[(lo + c - 1) as usize], keys[(lo + c) as usize]) < 0 {
            c += 1;
        }
        if cmp(val, keys[(lo + c - 1) as usize]) >= 0 {
            break;
        }
        keys[(lo + i - 1) as usize] = keys[(lo + c - 1) as usize];
        i = c;
    }
    keys[(lo + i - 1) as usize] = val;
}

fn insertion_sort(keys: &mut [i32], lo: isize, hi: isize, cmp: &dyn Fn(i32, i32) -> i32) {
    for i in lo..hi {
        let mut num = i;
        let val = keys[(i + 1) as usize];
        while num >= lo && cmp(val, keys[num as usize]) < 0 {
            keys[(num + 1) as usize] = keys[num as usize];
            num -= 1;
        }
        keys[(num + 1) as usize] = val;
    }
}

#[cfg(test)]
mod tests {
    use super::list_sort;

    /// Floors at 1, -1, 5 from a current height of 0: distances 1, 1, 5. The two tied entries keep the order the
    /// mscorlib sort gives (a tie is never a swap), so the first queued floor is the one already first.
    #[test]
    fn tied_distances_sort_as_mscorlib_does() {
        let floors = [1.0f32, -1.0, 5.0];
        let dist = |a: i32| (0.0 - floors[a as usize]).abs();
        let cmp = |a: i32, b: i32| if dist(a) - dist(b) > 0.0 { 1 } else { -1 };
        let mut keys = [1, 0, 2];
        list_sort(&mut keys, 4, &cmp);
        assert_eq!(keys, [1, 0, 2]);

        // Five entries (insertion sort): distances 2, 1, 1, 3, 0 for floors 0..4.
        let floors = [2.0f32, -1.0, 1.0, -3.0, 0.0];
        let dist = |a: i32| (0.0 - floors[a as usize]).abs();
        let cmp = |a: i32, b: i32| if dist(a) - dist(b) > 0.0 { 1 } else { -1 };
        let mut keys = [0, 1, 2, 3, 4];
        list_sort(&mut keys, 8, &cmp);
        assert_eq!(keys, [4, 2, 1, 0, 3]);
    }
}
