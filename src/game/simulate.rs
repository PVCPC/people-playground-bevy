//! PhysicsManager2D::Simulate (180bf88f0) for the scene's one PhysicsScene2D: the step, the Transform writeback,
//! joint breaking and the contact callbacks.
use super::*;
use crate::b2::contacts2d::{Message, Report, ScriptCollision};
use crate::b2::world::JointKind;
use crate::unity::{acosf, atan2f, cosf, sinf, transform};

/// Quaternionf operator* (1800b7f20), in its single-precision operation order.
fn qmul(l: [f32; 4], r: [f32; 4]) -> [f32; 4] {
    let [lx, ly, lz, lw] = l;
    let [rx, ry, rz, rw] = r;
    [
        ((rx * lw + lx * rw) + rz * ly) - ry * lz,
        ((ry * lw + ly * rw) + rx * lz) - rz * lx,
        ((rz * lw + lz * rw) + ry * lx) - rx * ly,
        ((lw * rw - rx * lx) - ry * ly) - lz * rz,
    ]
}

/// Slerp (1807421f0) on Quaternionf, in its single-precision operation order.
fn slerp(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let [x1, y1, z1, w1] = a;
    let mut dot = ((b[0] * x1 + b[1] * y1) + b[2] * z1) + b[3] * w1;
    let mut b = b;
    if 0.0 > dot {
        dot = -dot;
        b = b.map(|c| -c);
    }
    let [x2, y2, z2, w2] = b;
    if dot < f32::from_bits(0x3f733333) {
        let angle = acosf(dot);
        let inv = 1.0 / sinf(angle);
        let s1 = sinf(angle * t);
        let s0 = sinf((1.0 - t) * angle);
        return [(x1 * s0 + x2 * s1) * inv, (y1 * s0 + y2 * s1) * inv, (z1 * s0 + z2 * s1) * inv, (w1 * s0 + w2 * s1) * inv];
    }
    let r = if 0.0 > ((x1 * x2 + y1 * y2) + z1 * z2) + w1 * w2 {
        [x1 - (x1 + x2) * t, y1 - (y1 + y2) * t, z1 - (z1 + z2) * t, w1 - (w1 + w2) * t]
    } else {
        [(x2 - x1) * t + x1, (y2 - y1) * t + y1, (z2 - z1) * t + z1, (w2 - w1) * t + w1]
    };
    let len = (((r[1] * r[1] + r[0] * r[0]) + r[2] * r[2]) + r[3] * r[3]).sqrt();
    r.map(|c| c / len)
}

impl Game {
    /// PhysicsManager2D::Simulate after the FixedUpdate scripts.
    // ponytail: SyncTransforms and the MovePosition/MoveRotation half of SetupMovementStates do nothing here: no
    // rewritten script moves a Transform or calls MovePosition/MoveRotation. UpdateEffectors waits for the first
    // Effector2D.
    pub fn simulate_physics(&mut self) {
        let dt = self.time.fixedDeltaTime;
        self.last_simulation = (self.time.fixedTimeAsDouble, dt);
        // SetupMovementStatesJobData::Job (180bf2680): each listed body's last pose becomes the one to interpolate
        // from; a non-interpolating one leaves the list.
        for p in &mut self.physicals {
            let m = &mut p.rigidbody.movement;
            if m.listed {
                m.interpolating = p.rigidbody.interpolation != 0;
                if m.interpolating {
                    m.previous = m.body;
                } else {
                    m.listed = false;
                }
            }
        }
        self.world.step(dt, self.velocityIterations, self.positionIterations);
        self.write_back_transforms();
        // After a Rigidbody2D joined or changed, the list is rebuilt from every non-static body.
        if self.rigidbody_hierarchy_changed {
            for p in &mut self.physicals {
                p.rigidbody.movement.listed = p.rigidbody.bodyType != 2;
            }
            self.rigidbody_hierarchy_changed = false;
        }
        self.update_joints(dt);
        let reports = self.world.process_contacts(true);
        self.send_callback_reports(&reports);
        // FixedUpdate.ScriptRunDelayedFixedFrameRate, which also resumes the recorder's FE snapshot.
        // ponytail: the destroys run before that snapshot; order them by time when a recording has gore strings.
        self.gore_string_delayed_calls(false);
    }

    /// Each awake Rigidbody2D's body pose to its Transform: the position's x/y, and the rotation's z turn replaced
    /// by the body angle (keeping any x/y tilt).
    fn write_back_transforms(&mut self) {
        for p in 0..self.physicals.len() {
            let b = self.world.body(self.physicals[p].rigidbody.body);
            // ponytail: Rigidbody2D.simulated is always on (no rewritten script turns it off).
            if b.user.is_none() || b.flags & BODY_AWAKE == 0 {
                continue;
            }
            let (x, y, a) = (b.xf.p.x, b.xf.p.y, b.sweep.a);
            if !x.is_finite() || !y.is_finite() || !a.is_finite() {
                unimplemented!("Rigidbody2D::CorrectBodyState");
            }
            let chain = self.parent_chain(p);
            let t = &mut self.physicals[p].transform;
            let mut cur = t.rotation;
            if cur[3] < 0.0 {
                cur = cur.map(|c| -c);
            }
            let h = atan2f(cur[2], cur[3]);
            let turn = (h + h) * 0.5;
            let undo = [-0.0, -0.0, -sinf(turn), cosf(turn)];
            let half = a * 0.5;
            let body = [0.0, 0.0, sinf(half), cosf(half)];
            // Transform::SetPositionAndRotation stores it local to the parent; Transform.position/rotation read
            // it back.
            let pose = ([x, y, t.position[2]], qmul(qmul(body, undo), cur));
            (t.position, t.rotation) = transform::set_get_world(&chain, pose.0, pose.1);
            let m = &mut self.physicals[p].rigidbody.movement;
            (m.current, m.body) = (pose, pose);
        }
    }

    /// The physical's Transform ancestors, direct parent first.
    fn parent_chain(&self, p: usize) -> Vec<transform::Trs> {
        let mut chain = Vec::new();
        let mut n = self.physicals[p].parent;
        while let Some(i) = n {
            chain.push(self.transform_nodes[i].trs);
            n = self.transform_nodes[i].parent;
        }
        chain
    }

    /// PhysicsManager2D::Update (180bf9980) before the Update scripts: each awake interpolating body's Transform
    /// goes to its pose between the last two physics steps, by how far the frame time is past the last one.
    // ponytail: SyncTransforms/ClearTransformChanges do nothing here (no rewritten script moves a Transform).
    pub fn interpolate_bodies(&mut self) {
        let (last_time, last_delta) = self.last_simulation;
        if last_delta < f32::from_bits(0x34000000) {
            return;
        }
        let r = (self.time.timeAsDouble - last_time) / last_delta as f64;
        let t = (if 0.0 <= r { r.min(1.0) } else { 0.0 }) as f32;
        for p in 0..self.physicals.len() {
            let rb = &self.physicals[p].rigidbody;
            // Kinematic bodies driven by a parent body are skipped; no rewritten object has one.
            if !rb.interpolating() || self.world.body(rb.body).flags & BODY_AWAKE == 0 {
                continue;
            }
            // RigidbodyInterpolation2D.Extrapolate is never set by the game (prefabs and Global.CurrentInterpolationMode
            // only use None and Interpolate).
            assert_eq!(rb.interpolation, 1, "Rigidbody2D extrapolation");
            let m = rb.movement;
            let (prev, body) = (m.previous, m.body);
            let s = 1.0 - t;
            let pose = ([t * body.0[0] + s * prev.0[0], t * body.0[1] + s * prev.0[1], prev.0[2]], slerp(prev.1, body.1, t));
            let chain = self.parent_chain(p);
            let ph = &mut self.physicals[p];
            ph.rigidbody.movement.current = pose;
            (ph.transform.position, ph.transform.rotation) = transform::set_get_world(&chain, pose.0, pose.1);
        }
    }

    /// Joint2D::SetBreakForce: the joint moves to the back of the scene's break-limit list, or leaves it once
    /// both limits are infinite (Joint2D::AddBreakLimitsUpdate).
    pub fn set_hinge_break_force(&mut self, h: usize, v: f32) {
        self.hinges[h].set_break_force(v);
        self.update_break_limits(h);
    }

    /// Joint2D::SetBreakTorque.
    pub fn set_hinge_break_torque(&mut self, h: usize, v: f32) {
        self.hinges[h].set_break_torque(v);
        self.update_break_limits(h);
    }

    fn update_break_limits(&mut self, h: usize) {
        let (force, torque) = (self.hinges[h].breakForce, self.hinges[h].breakTorque);
        self.add_break_limits_update(BreakJoint::Hinge(h), force, torque);
    }

    /// Joint2D::AddBreakLimitsUpdate / SetBreakForce: to the back of the list, or out of it when both limits are
    /// infinite.
    pub fn add_break_limits_update(&mut self, j: BreakJoint, force: f32, torque: f32) {
        self.joint_break_limits.retain(|&k| k != j);
        if force != f32::INFINITY || torque != f32::INFINITY {
            self.joint_break_limits.push(j);
        }
    }

    /// PhysicsScene2D::UpdateJoints (180c24cc0): a joint over its force (reaction magnitude) or torque (signed)
    /// gets OnJointBreak2D and is destroyed. The next joint is taken before each one is handled, as the native
    /// list walk does.
    fn update_joints(&mut self, dt: f32) {
        let mut next = self.joint_break_limits.first().copied();
        while let Some(j) = next {
            let at = self.joint_break_limits.iter().position(|&k| k == j).unwrap();
            next = self.joint_break_limits.get(at + 1).copied();
            let (force, torque, joint) = match j {
                BreakJoint::Hinge(h) => (self.hinges[h].breakForce, self.hinges[h].breakTorque, self.hinges[h].joint),
                // SpringJoint2D keeps Joint2D::Reset's infinite breakTorque.
                BreakJoint::Spring(l) => (self.limbs[l].gore.breakForce, f32::INFINITY, self.limbs[l].gore.joint),
            };
            // Joint2D::GetReactionForce/Torque: zero without a b2 joint or under 0.0001s. b2RevoluteJoint gives
            // inv_dt times the point impulse and inv_dt times the limit impulse as torque; b2DistanceJoint gives
            // (inv_dt * impulse) * u and no torque.
            let inv_dt = 1.0 / dt;
            let ([fx, fy], t) = match joint.filter(|_| 0.0001 <= dt).map(|id| &self.world.joints[id].as_ref().unwrap().kind) {
                None => ([0.0, 0.0], 0.0),
                Some(JointKind::Revolute(r)) => ([r.impulse[0] * inv_dt, r.impulse[1] * inv_dt], r.impulse[2] * inv_dt),
                Some(JointKind::Distance(d)) => {
                    let f = d.reaction_force(inv_dt);
                    ([f.x, f.y], 0.0)
                }
            };
            let broken = (force != f32::INFINITY && (fx * fx + fy * fy).sqrt() > force) || (torque != f32::INFINITY && t > torque);
            if broken {
                match j {
                    BreakJoint::Hinge(h) => {
                        self.on_joint_break(h);
                        // DestroyObjectHighLevel: HingeJoint2D::Cleanup destroys the b2 joint.
                        if let Some(j) = self.hinges[h].joint.take() {
                            self.world.destroy_joint(j);
                        }
                        self.hinges[h].destroyed = true;
                    }
                    BreakJoint::Spring(l) => {
                        // SendMessage("OnJointBreak2D") to the limb: only its GoreStringBehaviour owns the joint.
                        self.gore_string_on_joint_break(l);
                        // DestroyObjectHighLevel, ahead of the script's own delayed Destroy.
                        self.remove_spring_joint(l);
                    }
                }
                self.joint_break_limits.retain(|&k| k != j);
            }
            if next.is_some_and(|n| !self.joint_break_limits.contains(&n)) {
                next = None;
            }
        }
    }

    /// SendMessage("OnJointBreak2D") to the hinge's GameObject (its limb).
    fn on_joint_break(&mut self, h: usize) {
        self.joint_break_messages(h);
    }

    /// PhysicsContacts2D::SendCallbackReports (after ProcessContacts): each report to collider A's GameObject and
    /// its rigidbody's (when on another object), then to B's.
    fn send_callback_reports(&mut self, reports: &[Report]) {
        for r in reports {
            let Some(col) = &r.collision else {
                // No rewritten script has OnTrigger*2D.
                continue;
            };
            for (collider, rigidbody) in [(r.collider_a, r.rigidbody_a), (r.collider_b, r.rigidbody_b)] {
                let c = self.world.script_collision(col, collider.id);
                if let Some(p) = self.physical_of_collider(collider.id) {
                    self.send_collision_message(p, r.message, &c);
                }
                if let Some(rb) = rigidbody {
                    let w = &self.world;
                    if let Some(p) = self.physicals.iter().position(|ph| w.body(ph.rigidbody.body).user.is_some_and(|u| u.id == rb.id)) {
                        self.send_collision_message(p, r.message, &c);
                    }
                }
            }
        }
    }

    /// GameObject.SendMessage of a collision message to a physical object's GameObject, in component order
    /// (PhysicalBehaviour, then LimbBehaviour). Collision messages reach disabled behaviours too.
    fn send_collision_message(&mut self, p: usize, m: Message, c: &ScriptCollision) {
        let limb = self.limbs.iter().position(|l| l.PhysicalBehaviour == p);
        match m {
            Message::CollisionEnter => {
                self.physical_on_collision_enter(p, c);
                if let Some(l) = limb {
                    self.limb_on_collision_enter(l, c);
                }
            }
            Message::CollisionStay => {
                self.physical_on_collision_stay(p, c);
                if let Some(l) = limb {
                    self.limb_on_collision_stay(l, c);
                }
            }
            Message::CollisionExit => self.physical_on_collision_exit(p, c),
            Message::TriggerEnter | Message::TriggerStay | Message::TriggerExit => {}
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn slerp_ends_and_middle() {
        let (a, b) = ([0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.5f32.sin(), 0.5f32.cos()]);
        // Narrow angle: the normalized-lerp branch, exact at the ends.
        let n = [0.0, 0.0, 0.1f32.sin(), 0.1f32.cos()];
        assert_eq!(super::slerp(a, n, 0.0), a);
        let mid = super::slerp(a, b, 0.5);
        assert!((mid[2] - 0.25f32.sin()).abs() < 1e-6 && (mid[3] - 0.25f32.cos()).abs() < 1e-6);
        // Wide angle: the acos branch.
        let c = [0.0, 0.0, 1.0f32.sin(), 1.0f32.cos()];
        let mid = super::slerp(a, c, 0.5);
        assert!((mid[2] - 0.5f32.sin()).abs() < 1e-6 && (mid[3] - 0.5f32.cos()).abs() < 1e-6);
    }

    #[test]
    fn qmul_identity() {
        let q = [0.1, -0.2, 0.3, 0.9];
        assert_eq!(super::qmul([0.0, 0.0, 0.0, 1.0], q), q);
    }
}
