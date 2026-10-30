//! PhysicalBehaviour's stabbing: Penetration, DoStab, the release checks, OnJointBreak2D, OnFlip, the Disintegrate and
//! OnDestroy loops, and the victim-side Stabbed/Unstabbed messages. A SliderJoint2D is a `FixedJoint` arena entry
//! with `slider` set (src/game/simulate.rs); a Penetration lives in `Game.penetration_arena` and the stabber's and
//! victim's lists hold its index.
use super::physical::SharpAxis;
use super::{d, mathf, Game};
use crate::b2::contacts2d::{ContactPoint2D, ScriptCollision};
use crate::unity::transform::bounds_closest_point;

/// UnityEngine.Mathf.Rad2Deg as the native jointSpeed getter multiplies it (0x42652ee1).
const RAD2DEG: f32 = 57.29578;

/// PhysicalBehaviour.Penetration (the SliderJoint2D is an index into `Game.fixed_joints`).
#[derive(Clone, Debug, Default)]
pub struct Penetration {
    pub Stabber: usize,
    pub Victim: Option<usize>,
    pub Joint: Option<usize>,
    pub Duration: f32,
    pub Axis: SharpAxis,
    /// RelevantColliders by instance id.
    pub RelevantColliders: Vec<i32>,
    pub CurrentConnectedAnchor: [f32; 2],
    pub Active: bool,
}

/// Stabbing (Messages.cs)
#[derive(Clone, Copy, Debug)]
pub struct Stabbing {
    pub stabber: usize,
    pub victim: usize,
    pub normal: [f32; 2],
    pub point: [f32; 2],
}

/// JointLimitState2D
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitState {
    Inactive,
    LowerLimit,
    UpperLimit,
    EqualLimits,
}

/// Vector2.LerpUnclamped
fn lerp_unclamped(a: [f32; 2], b: [f32; 2], t: f32) -> [f32; 2] {
    [(d(a[0]) + (d(b[0]) - a[0]) * t).f(), (d(a[1]) + (d(b[1]) - a[1]) * t).f()]
}

/// Vector2.Angle: unsigned degrees.
fn vector2_angle(from: [f32; 2], to: [f32; 2]) -> f32 {
    let sqr = |v: [f32; 2]| (d(v[0]) * v[0] + d(v[1]) * v[1]).f();
    let num = (d(sqr(from)) * sqr(to)).0.sqrt() as f32;
    if num < 1e-15 {
        return 0.0;
    }
    let dot = (d(from[0]) * to[0] + d(from[1]) * to[1]).f();
    let num2 = mathf::clamp((d(dot) / num).f(), -1.0, 1.0);
    (d((num2 as f64).acos() as f32) * 57.29578f32).f()
}

impl Game {
    // ---- IgnoreCollisionStackController -------------------------------------------------------------------------

    /// Collider2D != null: the object behind the instance id is not destroyed.
    fn collider_alive(&self, c: i32) -> bool {
        self.physical_of_collider(c).is_some()
    }

    /// IgnoreCollisionStackController.IgnoreCollisionSubstituteMethod: a counted Physics2D.IgnoreCollision.
    pub fn ignore_collision_substitute(&mut self, a: i32, b: i32, ignore: bool) {
        if !(self.collider_alive(a) && self.collider_alive(b)) {
            return;
        }
        let key = (a.min(b), a.max(b));
        if ignore {
            // RequestIgnoreCollision
            match self.ignore_stack.get_mut(&key) {
                Some(n) => *n += 1,
                None => {
                    self.ignore_stack.insert(key, 1);
                    self.world.ignore_collision(a, b, true);
                }
            }
        } else {
            // RequestDontIgnoreCollision
            let Some(n) = self.ignore_stack.get_mut(&key) else { return };
            *n -= 1;
            if *n <= 0 {
                self.ignore_stack.remove(&key);
                self.world.ignore_collision(a, b, false);
            }
        }
    }

    // ---- Penetration ----------------------------------------------------------------------------------------------

    /// Penetration.GetDirection
    pub fn penetration_direction(&self, pen: usize) -> [f32; 2] {
        let pen = &self.penetration_arena[pen];
        let v = self.transform_vector(pen.Stabber, pen.Axis.Axis);
        [v[0], v[1]]
    }

    /// Penetration.GetPoint: Joint.connectedBody.transform.TransformPoint(Joint.connectedAnchor).
    pub fn penetration_point(&self, pen: usize) -> [f32; 2] {
        let j = &self.fixed_joints[self.penetration_arena[pen].Joint.expect("Penetration.GetPoint: destroyed Joint2D")];
        let body = j.connected.expect("Penetration.GetPoint: connectedBody is null");
        let v = self.transform_point3(body, [j.state.connected_anchor.x, j.state.connected_anchor.y, 0.0]);
        [v[0], v[1]]
    }

    /// Joint2D.jointTranslation of a SliderJoint2D (180c04ca0): b2PrismaticJoint::GetJointTranslation, 0 without a
    /// b2 joint.
    pub fn slider_joint_translation(&self, f: usize) -> f32 {
        match self.fixed_joints[f].joint {
            None => 0.0,
            Some(id) => {
                let j = self.world.joints[id].as_ref().unwrap();
                j.prismatic().joint_translation(self.world.body(j.body_a), self.world.body(j.body_b))
            }
        }
    }

    /// Joint2D.jointSpeed of a SliderJoint2D (180c04c70): GetJointSpeed times Rad2Deg, 0 without a b2 joint.
    pub fn slider_joint_speed(&self, f: usize) -> f32 {
        match self.fixed_joints[f].joint {
            None => 0.0,
            Some(id) => {
                let j = self.world.joints[id].as_ref().unwrap();
                j.prismatic().joint_speed(self.world.body(j.body_a), self.world.body(j.body_b)) * RAD2DEG
            }
        }
    }

    /// Penetration.GetCurrentDepth
    pub fn penetration_current_depth(&self, pen: usize) -> f32 {
        let (stabber, victim, axis, joint) = {
            let p = &self.penetration_arena[pen];
            (p.Stabber, p.Victim, p.Axis, p.Joint.expect("Penetration.GetCurrentDepth: destroyed Joint2D"))
        };
        if axis.LooseLowerLimit && axis.LooseUpperLimit {
            return self.slider_joint_translation(joint);
        }
        let t = &self.physicals[stabber].transform;
        let position = t.position;
        // Transform.TransformDirection: rotation only.
        let direction = mathf::rotate(t.rotation, [axis.Axis[0], axis.Axis[1], 0.0]);
        let end = [position[0] + direction[0], position[1] + direction[1]];
        let anchor = self.fixed_joints[joint].state.connected_anchor;
        let victim = victim.expect("Penetration.GetCurrentDepth: Victim is null");
        let target = self.transform_point3(victim, [anchor.x, anchor.y, 0.0]);
        let distance = |a: [f32; 2]| {
            let (dx, dy) = (a[0] - target[0], a[1] - target[1]);
            (d(dx) * dx + d(dy) * dy).0.sqrt() as f32
        };
        if axis.LooseLowerLimit {
            return distance(lerp_unclamped([position[0], position[1]], end, axis.LowerLimit));
        }
        if axis.LooseUpperLimit {
            return distance(lerp_unclamped([position[0], position[1]], end, axis.UpperLimit));
        }
        0.0
    }

    /// PhysicalBehaviour.GetSharpEnd
    pub fn get_sharp_end(&self, p: usize, axis: &SharpAxis) -> [f32; 2] {
        let vector = self.transform_vector(p, axis.Axis);
        let position = self.physicals[p].transform.position;
        let end = [position[0] + vector[0], position[1] + vector[1]];
        let position = [position[0], position[1]];
        if axis.LooseLowerLimit {
            return lerp_unclamped(position, end, axis.LowerLimit);
        }
        if axis.LooseUpperLimit {
            return lerp_unclamped(position, end, axis.UpperLimit);
        }
        unimplemented!("PhysicalBehaviour.GetSharpEnd: Debug.LogError for an object with no sharp ends");
    }

    /// PhysicalBehaviour.GetRelativeStabSpeed: the Joint.jointSpeed magnitude of the penetration into `victim`.
    pub fn get_relative_stab_speed(&self, p: usize, victim: usize) -> f32 {
        for &pen in &self.physicals[p].penetrations {
            let pen = &self.penetration_arena[pen];
            if pen.Victim == Some(victim) {
                return self.slider_joint_speed(pen.Joint.expect("GetRelativeStabSpeed: destroyed Joint2D")).abs();
            }
        }
        0.0
    }

    /// PhysicalBehaviour.GetGlobalStabPoint: victim.transform.TransformPoint(Joint.connectedAnchor).
    pub fn get_global_stab_point(&self, p: usize, victim: usize) -> Option<[f32; 2]> {
        for &pen in &self.physicals[p].penetrations {
            let pen = &self.penetration_arena[pen];
            if pen.Victim == Some(victim) {
                let a = self.fixed_joints[pen.Joint.expect("GetGlobalStabPoint: destroyed Joint2D")].state.connected_anchor;
                let v = self.transform_point3(victim, [a.x, a.y, 0.0]);
                return Some([v[0], v[1]]);
            }
        }
        None
    }

    // ---- Stabbing a victim ----------------------------------------------------------------------------------------

    /// The transform.root of a physical: a transform node, or the object itself when it has no parent.
    fn root_of(&self, p: usize) -> (bool, usize) {
        match self.physicals[p].parent {
            None => (false, p),
            Some(mut n) => {
                while let Some(up) = self.transform_nodes[n].parent {
                    n = up;
                }
                (true, n)
            }
        }
    }

    /// transform.root.localScale.x
    fn root_scale_x(&self, p: usize) -> f32 {
        match self.root_of(p) {
            (true, n) => self.transform_nodes[n].trs.s[0],
            (false, p) => self.physicals[p].transform.localScale[0],
        }
    }

    /// PhysicalBehaviour.HandleStabbing from the point the sharp axis matched (the checks are in physical.rs).
    pub fn stab_begin(&mut self, p: usize, victim: usize, axis: SharpAxis, c: &ScriptCollision, contact: &ContactPoint2D) {
        let pen = self.penetration_arena.len();
        self.penetration_arena.push(Penetration { Stabber: p, Axis: axis, Active: true, ..Default::default() });
        let (rb, previous) = (self.physicals[p].rigidbody.clone(), self.physicals[p].previousVelocity);
        rb.set_velocity(&mut self.world, previous);
        if self.physicals[victim].rigidbody.bodyType == 0 {
            let (rb, previous) = (self.physicals[victim].rigidbody.clone(), self.physicals[victim].previousVelocity);
            rb.set_velocity(&mut self.world, previous);
        }
        let closest = self.world.collider_closest_point(c.collider, contact.point);
        let point = [closest.x, closest.y];
        let stabbing = Stabbing { stabber: p, victim, normal: [contact.normal.x, contact.normal.y], point };
        self.send_stabbed(stabbing);
        self.send_lodged(p);
        let local = self.inverse_transform_point(victim, [point[0], point[1], 0.0]);
        self.penetration_arena[pen].CurrentConnectedAnchor = [local[0], local[1]];
        self.do_stab(p, victim, axis, pen);
    }

    /// PhysicalBehaviour.DoStab
    pub fn do_stab(&mut self, p: usize, other: usize, axis: SharpAxis, pen: usize) {
        self.penetration_arena[pen].Active = true;
        self.penetration_arena[pen].Victim = Some(other);
        let v = &mut self.physicals[other];
        v.stabWoundCount = v.stabWoundCount.wrapping_add(1);
        v.beingStabbedBy.push(p);
        v.victimPenetrations.push(pen);
        let f = self.add_slider_joint_component(p);
        self.fixed_joint_set_auto_configure(f, false);
        self.slider_joint_set_auto_configure_angle(f, false);
        self.fixed_joint_set_anchor(f, [0.0, 0.0]);
        self.fixed_joint_set_break_force(f, f32::INFINITY);
        self.fixed_joint_set_break_torque(f, f32::INFINITY);
        self.fixed_joint_set_connected_anchor(f, self.penetration_arena[pen].CurrentConnectedAnchor);
        self.fixed_joint_set_connected_body(f, Some(other));
        self.fixed_joint_set_enable_collision(f, false);
        self.slider_joint_set_use_motor(f, true);
        let props = &self.properties[self.physicals[p].Properties];
        let softness = self.properties[self.physicals[other].Properties].Softness;
        let torque = (d(props.SharpForceThresholdMultiplier) * ((d(1.0) - softness) * 5.0f32 + 5.0f32) * props.LodgeStrengthMultiplier).f();
        let speed = self.fixed_joints[f].slider.as_ref().unwrap().motor_speed;
        self.slider_joint_set_motor(f, speed, torque);
        let angle = vector2_angle([self.root_scale_x(p), 0.0], axis.Axis);
        self.slider_joint_set_angle(f, angle);
        self.slider_joint_set_use_limits(f, true);
        self.slider_joint_set_limits(f, axis.LowerLimit, axis.UpperLimit);
        self.penetration_arena[pen].Joint = Some(f);
        // colliders[0], then other.transform.root.GetComponentsInChildren<Collider2D>(): the active objects under the
        // victim's root, in object order.
        let own = self.physicals[p].colliders[0];
        let root = self.root_of(other);
        let children: Vec<i32> = (0..self.physicals.len())
            .filter(|&q| !self.physicals[q].destroyed && self.physicals[q].active && self.root_of(q) == root)
            .flat_map(|q| self.physicals[q].all_colliders.clone())
            .collect();
        for c in children {
            self.ignore_collision_substitute(c, own, true);
            // ignoredColliders.Add: a list nothing reads.
            self.penetration_arena[pen].RelevantColliders.push(c);
        }
        self.physicals[p].penetrations.push(pen);
    }

    /// PhysicalBehaviour.UndoPenetrationNoCollision
    fn undo_penetration_no_collision(&mut self, pen: usize) {
        let b = self.physicals[self.penetration_arena[pen].Stabber].colliders[0];
        for rc in self.penetration_arena[pen].RelevantColliders.clone() {
            if self.collider_alive(rc) {
                self.ignore_collision_substitute(rc, b, false);
            }
        }
    }

    /// PhysicalBehaviour.GetCustomLimitState
    fn get_custom_limit_state(&self, p: usize, pen: usize) -> LimitState {
        let pen = &self.penetration_arena[pen];
        let Some(joint) = pen.Joint.filter(|&j| pen.Active && !self.fixed_joints[j].destroyed) else { return LimitState::Inactive };
        let modifier = self.physicals[p].StabReleaseSpeedModifier;
        let num = (d(self.slider_joint_speed(joint)) / 1000.0f32 * modifier).f();
        let mut num2 = (d(num) * num * num * num * num * num * num * num * 100.0f32).f();
        if num < 0.0 {
            num2 *= -1.0;
        }
        // Time.fixedUnscaledDeltaTime: the fixed step over the time scale.
        let fixed_unscaled = (self.time.fixedDeltaTime as f64 / self.time.timeScale as f64) as f32;
        let num3 = (d(self.slider_joint_translation(joint)) * modifier * 1.05f32 + d(num2) * fixed_unscaled).f();
        if num3 > pen.Axis.UpperLimit {
            return LimitState::UpperLimit;
        }
        if num3 < pen.Axis.LowerLimit {
            return LimitState::LowerLimit;
        }
        if mathf::approximately(pen.Axis.LowerLimit, pen.Axis.UpperLimit) {
            return LimitState::EqualLimits;
        }
        LimitState::Inactive
    }

    /// (bool)penetration.Victim
    fn penetration_victim_alive(&self, pen: usize) -> bool {
        self.penetration_arena[pen].Victim.is_some_and(|v| !self.physicals[v].destroyed)
    }

    /// PhysicalBehaviour.HandleStabRelease
    fn handle_stab_release(&mut self, p: usize, pen: usize) {
        if !self.penetration_arena[pen].Active {
            return;
        }
        if !self.penetration_victim_alive(pen) {
            self.destroy_stab_joint(pen);
            return;
        }
        let joint = self.penetration_arena[pen].Joint.expect("HandleStabRelease: Joint2D is null");
        match self.get_custom_limit_state(p, pen) {
            LimitState::Inactive => {
                self.fixed_joint_set_break_force(joint, f32::MAX);
                self.fixed_joint_set_break_torque(joint, f32::MAX);
            }
            LimitState::LowerLimit => {
                if self.penetration_arena[pen].Axis.LooseLowerLimit {
                    self.destroy_stab_joint(pen);
                }
            }
            LimitState::UpperLimit => {
                if self.penetration_arena[pen].Axis.LooseUpperLimit {
                    self.destroy_stab_joint(pen);
                }
            }
            // Debug.LogWarning only.
            LimitState::EqualLimits => {}
        }
    }

    /// PhysicalBehaviour.DestroyStabJoint
    pub fn destroy_stab_joint(&mut self, pen: usize) {
        self.send_dislodged(self.penetration_arena[pen].Stabber);
        if self.penetration_victim_alive(pen) {
            let (stabber, victim) = (self.penetration_arena[pen].Stabber, self.penetration_arena[pen].Victim.unwrap());
            let point = self.penetration_point(pen);
            let direction = self.penetration_direction(pen);
            self.send_unstabbed(Stabbing { stabber, victim, normal: [-direction[0], -direction[1]], point });
        }
        self.penetration_arena[pen].Active = false;
        if let Some(f) = self.penetration_arena[pen].Joint.filter(|&f| !self.fixed_joints[f].destroyed) {
            self.fixed_joint_set_break_force(f, 0.0);
            self.fixed_joint_set_break_torque(f, 0.0);
            self.slider_joint_set_use_limits(f, false);
            self.slider_joint_set_use_motor(f, false);
            self.undo_penetration_no_collision(pen);
        }
    }

    /// PhysicalBehaviour.OnJointBreak2D for a broken SliderJoint2D (`joint`, a `fixed_joints` index) on its object.
    pub fn physical_on_joint_break_slider(&mut self, joint: usize) {
        let p = self.fixed_joints[joint].attached;
        for i in 0..self.physicals[p].penetrations.len() {
            let pen = self.physicals[p].penetrations[i];
            if self.penetration_arena[pen].Joint == Some(joint) && !self.fixed_joints[joint].destroyed {
                let victim = self.penetration_arena[pen].Victim.expect("OnJointBreak2D: Victim is null");
                let v = &mut self.physicals[victim];
                v.stabWoundCount = v.stabWoundCount.wrapping_sub(1);
                self.penetration_arena[pen].Active = false;
                let v = &mut self.physicals[victim];
                if let Some(at) = v.beingStabbedBy.iter().position(|&s| s == p) {
                    v.beingStabbedBy.remove(at);
                }
                if let Some(at) = v.victimPenetrations.iter().position(|&s| s == pen) {
                    v.victimPenetrations.remove(at);
                }
                self.undo_penetration_no_collision(pen);
            }
        }
    }

    /// PhysicalBehaviour.ManagedUpdate's penetration step.
    pub fn stab_update(&mut self, p: usize) {
        if self.physicals[p].penetrations.is_empty() {
            return;
        }
        for i in (0..self.physicals[p].penetrations.len()).rev() {
            if !self.penetration_arena[self.physicals[p].penetrations[i]].Active {
                self.physicals[p].penetrations.remove(i);
            }
        }
        let dt = self.time.deltaTime;
        for i in 0..self.physicals[p].penetrations.len() {
            self.penetration_arena[self.physicals[p].penetrations[i]].Duration += dt;
        }
    }

    /// PhysicalBehaviour.ManagedFixedUpdate's penetration loop.
    pub fn stab_fixed_update(&mut self, p: usize) {
        let mut i = 0;
        while i < self.physicals[p].penetrations.len() {
            let pen = self.physicals[p].penetrations[i];
            i += 1;
            if self.penetration_arena[pen].Duration > self.time.maximumDeltaTime {
                self.handle_stab_release(p, pen);
            }
            let (stabber, victim) = (self.penetration_arena[pen].Stabber, self.penetration_arena[pen].Victim);
            let victim = match victim {
                Some(v) if !self.physicals[stabber].destroyed && !self.physicals[v].destroyed => v,
                _ => continue,
            };
            if self.physicals[stabber].SimulateTemperature && self.physicals[victim].SimulateTemperature {
                if !self.penetration_arena[pen].Active {
                    continue;
                }
                self.average_temperature(victim, stabber, 0.3);
            }
            let (victim_charge, charge) = (self.physicals[victim].Charge(), self.physicals[p].Charge());
            if (d(victim_charge) * 0.95f32).f() > charge {
                self.physicals[p].charge = (d(victim_charge) * 0.95f32).f();
            } else if (d(charge) * 0.95f32).f() > victim_charge {
                self.physicals[victim].charge = (d(charge) * 0.95f32).f();
            }
        }
    }

    /// The penetration part of PhysicalBehaviour.Disintegrate.
    pub fn stab_disintegrate(&mut self, p: usize) {
        for i in 0..self.physicals[p].penetrations.len() {
            self.destroy_stab_joint(self.physicals[p].penetrations[i]);
        }
        for i in 0..self.physicals[p].victimPenetrations.len() {
            let pen = self.physicals[p].victimPenetrations[i];
            self.penetration_arena[pen].Active = false;
            let ph = &mut self.physicals[p];
            ph.stabWoundCount = ph.stabWoundCount.wrapping_sub(1);
            if let Some(at) = ph.beingStabbedBy.iter().position(|&s| s == p) {
                ph.beingStabbedBy.remove(at);
            }
            let stabber = &mut self.physicals[self.penetration_arena[pen].Stabber];
            if let Some(at) = stabber.penetrations.iter().position(|&s| s == pen) {
                stabber.penetrations.remove(at);
            }
            self.undo_penetration_no_collision(pen);
        }
        self.physicals[p].victimPenetrations.clear();
    }

    /// PhysicalBehaviour.OnDestroy's DestroyStabJoint loop.
    pub fn stab_destroy(&mut self, p: usize) {
        for i in 0..self.physicals[p].penetrations.len() {
            self.destroy_stab_joint(self.physicals[p].penetrations[i]);
        }
    }

    /// PhysicalBehaviour.OnFlip: each live stab joint is rebuilt for the flipped object.
    pub fn physical_on_flip(&mut self, p: usize) {
        for i in 0..self.physicals[p].penetrations.len() {
            let pen = self.physicals[p].penetrations[i];
            let Some(old) = self.penetration_arena[pen].Joint.filter(|&j| self.penetration_arena[pen].Active && !self.fixed_joints[j].destroyed) else { continue };
            let (connected, anchor) = (self.fixed_joints[old].connected, self.fixed_joints[old].state.connected_anchor);
            let slider = self.fixed_joints[old].slider.as_ref().unwrap();
            let (speed, torque) = (slider.motor_speed, slider.max_motor_torque);
            self.fixed_joint_destroy(old);
            let f = self.add_slider_joint_component(p);
            self.fixed_joint_set_auto_configure(f, false);
            self.slider_joint_set_auto_configure_angle(f, false);
            self.fixed_joint_set_anchor(f, [0.0, 0.0]);
            self.fixed_joint_set_connected_body(f, connected);
            self.fixed_joint_set_connected_anchor(f, [anchor.x, anchor.y]);
            self.fixed_joint_set_enable_collision(f, false);
            let axis = self.penetration_arena[pen].Axis;
            // Mathf.Atan2 is Math.Atan2 on doubles.
            self.slider_joint_set_angle(f, (d((axis.Axis[1] as f64).atan2(axis.Axis[0] as f64) as f32) * 57.29578f32).f());
            self.slider_joint_set_use_limits(f, true);
            self.slider_joint_set_limits(f, axis.LowerLimit, axis.UpperLimit);
            self.slider_joint_set_use_motor(f, true);
            self.slider_joint_set_motor(f, speed, torque);
            self.fixed_joint_set_break_force(f, f32::INFINITY);
            self.fixed_joint_set_break_torque(f, f32::INFINITY);
            self.penetration_arena[pen].Joint = Some(f);
        }
    }

    // ---- The victim's Stabbed / Unstabbed receivers --------------------------------------------------------------

    /// gameObject.SendMessage("Lodged", stabbing): ExcaliburBehaviour, OldSyringeBehaviour and SyringeBehaviour are the
    /// only receivers; an item without one (Knife, Sword) does nothing.
    fn send_lodged(&self, p: usize) {
        if self.physicals[p].active && !self.physicals[p].lodge_receiver.is_empty() {
            unimplemented!("{}.Lodged", self.physicals[p].lodge_receiver);
        }
    }

    /// gameObject.SendMessage("Dislodged", penetration): SyringeBehaviour is the only receiver.
    fn send_dislodged(&self, p: usize) {
        if self.physicals[p].active && { let r = &self.physicals[p].lodge_receiver; r.contains("Syringe") && !r.starts_with("Old") } {
            unimplemented!("{}.Dislodged", self.physicals[p].lodge_receiver);
        }
    }

    /// victim.SendMessage("Stabbed", stabbing): the components of a Person limb, in component order. (SendMessage
    /// skips an inactive object.)
    pub fn send_stabbed(&mut self, s: Stabbing) {
        if !self.physicals[s.victim].active || self.physicals[s.victim].destroyed {
            return;
        }
        // PhysicalBehaviour.Stabbed: the stab sound's random pick; the playback is audio.
        if let Some(n) = self.properties[self.physicals[s.victim].Properties].StabSound.filter(|&n| n != 0) {
            self.pick_random(n);
        }
        if let Some(l) = self.limbs.iter().position(|l| l.PhysicalBehaviour == s.victim) {
            self.limb_stabbed(l, s);
            if self.physicals[s.stabber].StabCausesWound {
                let size = (d(8.0) * self.physicals[s.stabber].StabWoundSizeMultiplier).f();
                self.skin_add_damage_point(l, super::misc::damage_type::STAB, s.point, size);
            }
            self.circulation_stabbed(l, s);
        }
    }

    /// victim.SendMessage("Unstabbed", stabbing): CirculationBehaviour (LiquidContainerLeaking and TyreDeflation are
    /// not on a Person).
    pub fn send_unstabbed(&mut self, s: Stabbing) {
        if !self.physicals[s.victim].active || self.physicals[s.victim].destroyed {
            return;
        }
        if let Some(l) = self.limbs.iter().position(|l| l.PhysicalBehaviour == s.victim) {
            self.circulation_unstabbed(l, s);
        }
    }

    /// LimbBehaviour.IsWorldPointInVitalPart (mindistance = 4f / 35f)
    pub fn is_world_point_in_vital_part(&self, l: usize, world_point: [f32; 2]) -> bool {
        let Some(bounds) = self.vital_parts.get(&l) else {
            unimplemented!("LimbBehaviour.VitalParts: not loaded for this limb (Game.vital_parts)")
        };
        let mindistance = 4.0f32 / 35.0;
        let num = mindistance * mindistance;
        let vector = self.inverse_transform_point(self.limbs[l].PhysicalBehaviour, [world_point[0], world_point[1], 0.0]);
        for r in bounds {
            let (closest, _) = bounds_closest_point(vector, [r[0], r[1], r[2]], [r[3], r[4], r[5]]);
            let dv = [closest[0] - vector[0], closest[1] - vector[1], closest[2] - vector[2]];
            if (d(dv[0]) * dv[0] + d(dv[1]) * dv[1] + d(dv[2]) * dv[2]).f() <= num {
                return true;
            }
        }
        false
    }

    /// LimbBehaviour.Stabbed
    fn limb_stabbed(&mut self, l: usize, s: Stabbing) {
        if self.limbs[l].ImmuneToDamage || !self.physicals[s.stabber].StabCausesWound {
            return;
        }
        if self.amount_of_blood(l) > 0.2 {
            let fallback = self.original_blood_color(l);
            let colour = self.get_computed_color(l, fallback);
            // stabber.SendMessage("Decal", ...)
            if self.physicals[s.stabber].active {
                let ty = super::misc::decal_descriptor(&self.limbs[l].BloodDecal);
                self.physical_decal(s.stabber, ty, s.point, 1.0, colour);
            }
        }
        if self.limbs[l].HasLungs && !self.limbs[l].IsAndroid && self.random.value() > 0.9 {
            self.limbs[l].LungsPunctured = true;
        }
        let flag = self.is_world_point_in_vital_part(l, s.point) && self.random.value() > 0.05;
        let limb = &self.limbs[l];
        let zombie = if limb.IsZombie { 0.1f32 } else { 1.0 };
        let damage = (d(limb.Health) * 0.5f32 * zombie * (if flag { 2.0f32 } else { 1.0 }) * self.prefs.FragilityMultiplier).f();
        self.limb_damage(l, damage);
        if flag && !self.limbs[l].IsZombie {
            let c = &mut self.circulations[l];
            c.InternalBleedingIntensity = (d(c.InternalBleedingIntensity) + d(5.0) * self.random.value()).f();
        }
        self.limb_wince(l, 165.0);
        let person = self.limbs[l].Person;
        if !self.limbs[l].IsZombie && self.limb_is_considered_alive(l) {
            self.people[person].AdrenalineLevel += 1.0;
            self.people[person].ShockLevel = (d(self.people[person].ShockLevel) + d(self.random.value())).f();
        }
        self.limbs[l].Numbness = 1.0;
        if self.limbs[l].HasBrain && flag && (!self.limbs[l].IsZombie || !(self.random.value() > 0.5)) {
            self.add_pain(person, 90.0);
            let c = &mut self.circulations[l];
            c.InternalBleedingIntensity = (d(c.InternalBleedingIntensity) + d(5.0) * self.random.value()).f();
            self.limbs[l].Health = 0.0;
            if self.random.value() > 0.25 {
                self.people[person].Consciousness = 0.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::b2::math::Vec2;
    use crate::b2::world::{BodyDef, BodyType};

    /// A sharp stabber (collider 10) and a victim root of two objects (colliders 11, 12) on a shared transform node.
    fn fixture() -> (Game, SharpAxis) {
        let mut g = Game::default();
        let axis = SharpAxis { Axis: [1.0, 0.0], LowerLimit: -0.4, UpperLimit: 0.1, LooseUpperLimit: false, LooseLowerLimit: true };
        g.properties.push(super::super::physical::PhysicalProperties {
            Softness: 0.5,
            Sharp: true,
            SharpForceThresholdMultiplier: 2.0,
            LodgeStrengthMultiplier: 3.0,
            SharpAxes: vec![axis],
            ..Default::default()
        });
        g.transform_nodes.push(crate::unity::transform::Node { id: 0, trs: crate::unity::transform::Trs { t: [0.0; 4], q: [0.0, 0.0, 0.0, 1.0], s: [1.0, 1.0, 1.0, 0.0] }, parent: None });
        for (i, (x, parent)) in [(0.0f32, None), (0.6, Some(0)), (0.6, Some(0))].into_iter().enumerate() {
            let body = g.world.create_body(&BodyDef { kind: BodyType::Dynamic, position: Vec2::new(x, 1.0), ..Default::default() });
            let mut p = super::super::Physical::default();
            p.rigidbody.body = body;
            p.colliders = vec![10 + i as i32];
            p.all_colliders = p.colliders.clone();
            p.parent = parent;
            p.transform.localScale = [1.0, 1.0, 1.0];
            p.transform.rotation = [0.0, 0.0, 0.0, 1.0];
            p.transform.position = [x, 1.0, 0.0];
            g.physicals.push(p);
        }
        (g, axis)
    }

    fn start(g: &mut Game, axis: SharpAxis) -> usize {
        g.penetration_arena.push(Penetration { Stabber: 0, Axis: axis, CurrentConnectedAnchor: [0.1, 0.0], ..Default::default() });
        g.do_stab(0, 1, axis, 0);
        0
    }

    #[test]
    fn do_stab_builds_the_slider_and_ignores_the_victim_root() {
        let (mut g, axis) = fixture();
        let pen = start(&mut g, axis);
        let p = &g.penetration_arena[pen];
        assert!(p.Active && p.Victim == Some(1) && p.RelevantColliders == vec![11, 12]);
        assert_eq!((g.physicals[1].stabWoundCount, g.physicals[1].beingStabbedBy.clone(), g.physicals[1].victimPenetrations.clone()), (1, vec![0], vec![0]));
        assert_eq!(g.physicals[0].penetrations, vec![0]);
        let f = p.Joint.unwrap();
        let s = g.fixed_joints[f].slider.as_ref().unwrap();
        // 2 * ((1 - 0.5) * 5 + 5) * 3
        assert_eq!((s.max_motor_torque, s.lower, s.upper, s.use_limits, s.use_motor, s.auto_configure_angle), (45.0, -0.4, 0.1, true, true, false));
        assert_eq!(s.angle, 0.0, "Vector2.Angle((1, 0), (1, 0))");
        let j = g.world.joints[g.fixed_joints[f].joint.unwrap()].as_ref().unwrap();
        assert_eq!((j.body_a, j.body_b, j.collide_connected), (g.physicals[0].rigidbody.body, g.physicals[1].rigidbody.body, false));
        assert!(matches!(j.kind, crate::b2::world::JointKind::Prismatic(_)));
        assert!(g.joint_break_limits.is_empty(), "infinite break limits");
        assert_eq!(g.ignore_stack.len(), 2);
        assert!(g.world.ignored.contains(&(10, 11)) && g.world.ignored.contains(&(10, 12)));
    }

    #[test]
    fn ignore_stack_counts_requests() {
        let (mut g, _) = fixture();
        g.ignore_collision_substitute(10, 11, true);
        g.ignore_collision_substitute(11, 10, true);
        g.ignore_collision_substitute(10, 11, false);
        assert!(g.world.ignored.contains(&(10, 11)));
        g.ignore_collision_substitute(10, 11, false);
        assert!(g.world.ignored.is_empty() && g.ignore_stack.is_empty());
        // A pair that was never requested, or with a dead end, does nothing.
        g.ignore_collision_substitute(10, 12, false);
        g.physicals[2].destroyed = true;
        g.ignore_collision_substitute(10, 12, true);
        assert!(g.world.ignored.is_empty() && g.ignore_stack.is_empty());
    }

    #[test]
    fn release_then_break_undoes_the_stab() {
        let (mut g, axis) = fixture();
        let pen = start(&mut g, axis);
        let f = g.penetration_arena[pen].Joint.unwrap();
        g.destroy_stab_joint(pen);
        assert!(!g.penetration_arena[pen].Active);
        let s = g.fixed_joints[f].slider.as_ref().unwrap();
        assert!(!s.use_limits && !s.use_motor);
        assert!(g.ignore_stack.is_empty() && g.world.ignored.is_empty(), "UndoPenetrationNoCollision");
        // The stab counter drops only when the zero-force joint breaks (a reaction above 0).
        assert_eq!(g.physicals[1].stabWoundCount, 1);
        let id = g.fixed_joints[f].joint.unwrap();
        let joint = g.world.joints[id].as_mut().unwrap().prismatic_mut();
        joint.impulse = [1.0, 0.0, 0.0];
        joint.perp = Vec2::new(0.0, 1.0);
        g.update_joints(1.0 / 50.0);
        assert!(g.fixed_joints[f].destroyed && g.fixed_joints[f].joint.is_none());
        assert_eq!((g.physicals[1].stabWoundCount, g.physicals[1].beingStabbedBy.len(), g.physicals[1].victimPenetrations.len()), (0, 0, 0));
        // ManagedUpdate then drops the inactive penetration.
        g.stab_update(0);
        assert!(g.physicals[0].penetrations.is_empty());
    }

    #[test]
    fn disintegrating_the_victim_clears_its_penetrations() {
        let (mut g, axis) = fixture();
        let pen = start(&mut g, axis);
        g.stab_disintegrate(1);
        assert!(!g.penetration_arena[pen].Active && g.physicals[1].victimPenetrations.is_empty() && g.physicals[0].penetrations.is_empty());
        assert_eq!(g.physicals[1].stabWoundCount, 0);
        assert!(g.ignore_stack.is_empty());
    }

    #[test]
    fn flip_rebuilds_the_joint_with_the_same_motor() {
        let (mut g, axis) = fixture();
        let pen = start(&mut g, axis);
        let old = g.penetration_arena[pen].Joint.unwrap();
        g.physical_on_flip(0);
        let new = g.penetration_arena[pen].Joint.unwrap();
        assert!(new != old && g.fixed_joints[old].destroyed);
        let s = g.fixed_joints[new].slider.as_ref().unwrap();
        assert_eq!((s.max_motor_torque, s.angle, s.lower, s.upper), (45.0, 0.0, -0.4, 0.1));
        assert_eq!(g.fixed_joints[new].connected, Some(1));
        assert_eq!(g.fixed_joints[new].state.connected_anchor, Vec2::new(0.1, 0.0));
        assert_eq!(g.world.joints.iter().flatten().count(), 1);
    }

    #[test]
    fn angle_is_unsigned() {
        assert_eq!(vector2_angle([1.0, 0.0], [0.0, -1.0]), 90.0);
        assert_eq!(vector2_angle([0.0, 0.0], [1.0, 0.0]), 0.0);
    }
}
