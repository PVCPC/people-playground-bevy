//! PhysicsManager2D::Simulate (180bf88f0) for the scene's one PhysicsScene2D: the step, the Transform writeback,
//! joint breaking and the contact callbacks.
use super::*;
use crate::b2::contacts2d::{Message, Report, ScriptCollision};
use crate::unity::{atan2f, cosf, sinf, transform};

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

impl Game {
    /// PhysicsManager2D::Simulate after the FixedUpdate scripts.
    // ponytail: SyncTransforms and SetupMovementStates do nothing here: no rewritten script moves a Transform or
    // calls MovePosition/MoveRotation. UpdateEffectors waits for the first Effector2D.
    pub fn simulate_physics(&mut self) {
        let dt = self.time.fixedDeltaTime;
        self.world.step(dt, self.velocityIterations, self.positionIterations);
        self.write_back_transforms();
        self.update_joints(dt);
        let reports = self.world.process_contacts(true);
        self.send_callback_reports(&reports);
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
            let mut chain = Vec::new();
            let mut n = self.physicals[p].parent;
            while let Some(i) = n {
                chain.push(self.transform_nodes[i].trs);
                n = self.transform_nodes[i].parent;
            }
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
            (t.position, t.rotation) = transform::set_get_world(&chain, [x, y, t.position[2]], qmul(qmul(body, undo), cur));
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
        self.joint_break_limits.retain(|&k| k != h);
        if self.hinges[h].breakForce != f32::INFINITY || self.hinges[h].breakTorque != f32::INFINITY {
            self.joint_break_limits.push(h);
        }
    }

    /// PhysicsScene2D::UpdateJoints (180c24cc0): a joint over its force (reaction magnitude) or torque (signed)
    /// gets OnJointBreak2D and is destroyed. The next joint is taken before each one is handled, as the native
    /// list walk does.
    fn update_joints(&mut self, dt: f32) {
        let mut next = self.joint_break_limits.first().copied();
        while let Some(h) = next {
            let at = self.joint_break_limits.iter().position(|&k| k == h).unwrap();
            next = self.joint_break_limits.get(at + 1).copied();
            let (force, torque) = (self.hinges[h].breakForce, self.hinges[h].breakTorque);
            // Joint2D::GetReactionForce/Torque: zero without a b2 joint or under 0.0001s; b2RevoluteJoint gives
            // inv_dt times the point impulse, and inv_dt times the limit impulse as torque.
            let impulse = self.hinges[h].joint.filter(|_| 0.0001 <= dt).map(|j| self.world.joints[j].as_ref().unwrap().revolute.impulse);
            let inv_dt = 1.0 / dt;
            let over_force = force != f32::INFINITY && {
                let [fx, fy] = impulse.map_or([0.0, 0.0], |i| [i[0] * inv_dt, i[1] * inv_dt]);
                (fx * fx + fy * fy).sqrt() > force
            };
            let broken = over_force || (torque != f32::INFINITY && impulse.map_or(0.0, |i| i[2] * inv_dt) > torque);
            if broken {
                self.on_joint_break(h);
                // DestroyObjectHighLevel: HingeJoint2D::Cleanup destroys the b2 joint.
                if let Some(j) = self.hinges[h].joint.take() {
                    self.world.destroy_joint(j);
                }
                self.hinges[h].destroyed = true;
                self.joint_break_limits.retain(|&k| k != h);
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
    fn qmul_identity() {
        let q = [0.1, -0.2, 0.3, 0.9];
        assert_eq!(super::qmul([0.0, 0.0, 0.0, 1.0], q), q);
    }
}
