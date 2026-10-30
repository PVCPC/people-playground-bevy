//! PhysicsManager2D::Simulate (180bf88f0) for the scene's one PhysicsScene2D: the step, the Transform writeback,
//! joint breaking and the contact callbacks.
use super::*;
use crate::b2::contacts2d::{Message, Report, ScriptCollision};
use crate::b2::math::Vec2;
use crate::b2::prismatic::{self, SliderJoint2d};
use crate::b2::weld::{check_bodies, clamp_break, BodyFrame, Connected, FixedJoint2d};
use crate::b2::world::{BodyId, JointId, JointKind};
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

/// PhysicsManager2D::Update's pose between the last two steps.
fn interpolated_pose(m: MovementState, t: f32) -> ([f32; 3], [f32; 4]) {
    let (prev, body) = (m.previous, m.body);
    let s = 1.0 - t;
    ([t * body.0[0] + s * prev.0[0], t * body.0[1] + s * prev.0[1], prev.0[2]], slerp(prev.1, body.1, t))
}

/// The write-back pose of a body: its x/y at the Transform's z, and the rotation's z turn replaced by the body angle.
fn written_pose(b: &crate::b2::world::Body, z: f32, rotation: [f32; 4]) -> ([f32; 3], [f32; 4]) {
    let (x, y, a) = (b.xf.p.x, b.xf.p.y, b.sweep.a);
    if !x.is_finite() || !y.is_finite() || !a.is_finite() {
        unimplemented!("Rigidbody2D::CorrectBodyState");
    }
    let mut cur = rotation;
    if cur[3] < 0.0 {
        cur = cur.map(|c| -c);
    }
    let h = atan2f(cur[2], cur[3]);
    let turn = (h + h) * 0.5;
    let undo = [-0.0, -0.0, -sinf(turn), cosf(turn)];
    let half = a * 0.5;
    let body = [0.0, 0.0, sinf(half), cosf(half)];
    ([x, y, z], qmul(qmul(body, undo), cur))
}

impl Game {
    /// PhysicsManager2D::Simulate after the FixedUpdate scripts.
    // ponytail: the MovePosition/MoveRotation half of SetupMovementStates does nothing here: no rewritten script
    // calls MovePosition/MoveRotation. UpdateEffectors waits for the first Effector2D.
    pub fn simulate_physics(&mut self) {
        self.sync_pending();
        let dt = self.time.fixedDeltaTime;
        self.last_simulation = (self.time.fixedTimeAsDouble, dt);
        // SetupMovementStatesJobData::Job (180bf2680): each listed body's last pose becomes the one to interpolate
        // from; a non-interpolating one leaves the list.
        for rb in self.physicals.iter_mut().map(|p| &mut p.rigidbody).chain(self.node_rigidbodies.iter_mut().map(|n| &mut n.rigidbody)) {
            let m = &mut rb.movement;
            if m.listed {
                m.interpolating = rb.interpolation != 0;
                if m.interpolating {
                    m.previous = m.body;
                } else {
                    m.listed = false;
                }
            }
        }
        self.world.step(dt, self.velocityIterations, self.positionIterations);
        self.write_back_transforms();
        self.tool_write_back();
        // After a Rigidbody2D joined or changed, the list is rebuilt from every non-static body.
        if self.rigidbody_hierarchy_changed {
            for rb in self.physicals.iter_mut().map(|p| &mut p.rigidbody).chain(self.node_rigidbodies.iter_mut().map(|n| &mut n.rigidbody)) {
                rb.movement.listed = rb.bodyType != 2 && rb.detached.is_none();
            }
            self.rigidbody_hierarchy_changed = false;
        }
        self.update_joints(dt);
        let reports = self.world.process_contacts(true);
        self.send_callback_reports(&reports);
        // FixedUpdate.ScriptRunDelayedFixedFrameRate, which also resumes the recorder's FE snapshot.
        // ponytail: the destroys run before that snapshot; order them by time when a recording has gore strings.
        self.gore_string_delayed_calls(false);
        // The delayed Start of behaviours instantiated during this FixedUpdate (registered after those calls).
        self.run_pending_starts();
        // WaitForFixedUpdate (the fixed step's last stage): the coroutines that reached it run on; one that waits for
        // it again here waits for the next step.
        for c in std::mem::take(&mut self.fixed_update_waits) {
            self.resume_coroutine(c);
        }
    }

    /// PhysicsManager2D::Simulate's write-back over the rigidbody update lists: each awake Rigidbody2D's body pose
    /// to its Transform (the position's x/y, and the rotation's z turn replaced by the body angle, keeping any x/y
    /// tilt). A body driven by a parent Rigidbody2D is written asleep too, as is a static one below the root.
    fn write_back_transforms(&mut self) {
        for p in self.rigidbody_order() {
            // A disintegrated object's Rigidbody2D left the scene's list with its body.
            if self.physicals[p].rigidbody.detached.is_some() {
                continue;
            }
            let ph = &self.physicals[p];
            let b = self.world.body(ph.rigidbody.body);
            if b.user.is_none() {
                continue;
            }
            if ph.driven_by.is_none() && b.flags & BODY_AWAKE == 0 && !(ph.rigidbody.bodyType == 2 && self.depth(p) > 0) {
                continue;
            }
            if let Some(parent) = ph.driven_by
                && ph.rigidbody.bodyType == 1
                && self.world.body(parent).flags & BODY_AWAKE != 0
            {
                let (body, pose) = (ph.rigidbody.body, (ph.transform.position, ph.transform.rotation));
                self.follow_transform(body, pose);
                let m = &mut self.physicals[p].rigidbody.movement;
                (m.current, m.body) = (pose, pose);
                continue;
            }
            let chain = self.parent_chain(p);
            let t = &mut self.physicals[p].transform;
            let pose = written_pose(b, t.position[2], t.rotation);
            // Transform::SetPositionAndRotation stores it local to the parent; Transform.position/rotation read
            // it back.
            let (local, world) = transform::set_world(&chain, pose.0, pose.1);
            (t.position, t.rotation, t.local) = (world.0, world.1, Some(local));
            let m = &mut self.physicals[p].rigidbody.movement;
            (m.current, m.body) = (pose, pose);
            self.refresh_nested(p);
        }
        // The bodies without a PhysicalBehaviour parent no other body, so going after every physical keeps each
        // parent ahead of its children as the depth lists do.
        for k in 0..self.node_rigidbodies.len() {
            let nb = &self.node_rigidbodies[k];
            if nb.rigidbody.detached.is_some() {
                continue;
            }
            let b = self.world.body(nb.rigidbody.body);
            if b.user.is_none() {
                continue;
            }
            if nb.driven_by.is_none() && b.flags & BODY_AWAKE == 0 && !(nb.rigidbody.bodyType == 2 && self.transform_nodes[nb.node].parent.is_some()) {
                continue;
            }
            let node = nb.node;
            if let Some(parent) = nb.driven_by
                && nb.rigidbody.bodyType == 1
                && self.world.body(parent).flags & BODY_AWAKE != 0
            {
                let (body, pose) = (nb.rigidbody.body, self.node_world(node));
                self.follow_transform(body, pose);
                let m = &mut self.node_rigidbodies[k].rigidbody.movement;
                (m.current, m.body) = (pose, pose);
                continue;
            }
            let (position, rotation) = self.node_world(node);
            let pose = written_pose(b, position[2], rotation);
            self.set_node_world(node, pose);
            self.node_rigidbodies[k].rigidbody.movement.current = pose;
            self.node_rigidbodies[k].rigidbody.movement.body = pose;
        }
    }

    /// Simulate's write-back for a kinematic body below an awake parent body: the body goes to its Transform (x/y,
    /// and the z turn of the rotation with w made non-negative) and is woken; the Transform keeps its pose.
    fn follow_transform(&mut self, body: BodyId, (position, rotation): ([f32; 3], [f32; 4])) {
        let (mut z, mut w) = (rotation[2], rotation[3]);
        if 0.0 > w {
            (z, w) = (-z, -w);
        }
        let h = atan2f(z, w);
        self.world.set_transform(body, Vec2::new(position[0], position[1]), h + h);
        self.world.body_mut(body).wake();
    }

    /// A node's Transform ancestors, direct parent first.
    fn node_parent_chain(&self, n: usize) -> Vec<transform::Trs> {
        let mut chain = Vec::new();
        let mut up = self.transform_nodes[n].parent;
        while let Some(i) = up {
            chain.push(self.node_trs(i));
            up = self.transform_nodes[i].parent;
        }
        chain
    }

    /// Transform.position/rotation of a node.
    pub(crate) fn node_world(&self, n: usize) -> ([f32; 3], [f32; 4]) {
        let trs = self.node_trs(n);
        let (mut v, mut r) = ([trs.t[0], trs.t[1], trs.t[2], 0.0], trs.q);
        for p in &self.node_parent_chain(n) {
            (v, r) = (transform::global_position(p, v), transform::global_rotation(p, r));
        }
        ([v[0], v[1], v[2]], r)
    }

    /// Transform::SetPositionAndRotation on a node: its local pose.
    fn set_node_world(&mut self, n: usize, pose: ([f32; 3], [f32; 4])) {
        let (local, _) = transform::set_world(&self.node_parent_chain(n), pose.0, pose.1);
        let trs = &mut self.transform_nodes[n].trs;
        (trs.t[0], trs.t[1], trs.t[2], trs.q) = (local.0[0], local.0[1], local.0[2], local.1);
    }

    /// The b2 body of a Rigidbody2D's owner.
    pub(crate) fn owner_body(&self, o: BodyOwner) -> BodyId {
        match o {
            BodyOwner::Physical(p) => self.physicals[p].rigidbody.body,
            BodyOwner::Node(k) => self.node_rigidbodies[k].rigidbody.body,
        }
    }

    /// Transform.lossyScale of a Rigidbody2D's owner.
    fn owner_lossy_scale(&self, o: BodyOwner) -> [f32; 3] {
        match o {
            BodyOwner::Physical(p) => self.lossy_scale(p),
            BodyOwner::Node(k) => {
                let n = self.node_rigidbodies[k].node;
                let mut nodes = vec![self.node_trs(n)];
                nodes.extend(self.node_parent_chain(n));
                transform::lossy_scale(&nodes, self.node_world(n).1)
            }
        }
    }

    /// PhysicsScene2D's rigidbody update lists (AddRigidbodyUpdates, 180c1c280): by GetTransformDepth capped at 63,
    /// creation order within a depth.
    fn rigidbody_order(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.physicals.len()).collect();
        if !self.node_bodies.is_empty() {
            order.sort_by_key(|&p| self.depth(p).min(63));
        }
        order
    }

    /// GetTransformDepth (1807f17f0): the physical's ancestor count.
    fn depth(&self, p: usize) -> usize {
        let (mut d, mut n) = (0, self.physicals[p].parent);
        while let Some(i) = n {
            d += 1;
            n = self.transform_nodes[i].parent;
        }
        d
    }

    /// The node that is physical `p`'s own Transform (it parents other physicals).
    pub(crate) fn body_node(&self, p: usize) -> Option<usize> {
        self.node_bodies.iter().find_map(|(&n, &q)| (q == p).then_some(n))
    }

    /// A transform node's TRS; a body's node has its physical's local pose.
    pub(crate) fn node_trs(&self, n: usize) -> transform::Trs {
        let mut trs = self.transform_nodes[n].trs;
        if let Some(&p) = self.node_bodies.get(&n) {
            let (t, q) = self.physicals[p].transform.local.expect("a body node without a local pose");
            (trs.t[0], trs.t[1], trs.t[2], trs.q) = (t[0], t[1], t[2], q);
        }
        trs
    }

    /// Transform.position/rotation of the physicals under body `p`'s Transform once it moved: their local poses
    /// through the new chain.
    pub(super) fn refresh_nested(&mut self, p: usize) {
        let Some(node) = self.body_node(p) else { return };
        for q in 0..self.physicals.len() {
            let mut n = self.physicals[q].parent;
            while n.is_some_and(|i| i != node) {
                n = self.transform_nodes[n.unwrap()].parent;
            }
            if n.is_none() {
                continue;
            }
            let chain = self.parent_chain(q);
            let (t, mut r) = self.physicals[q].transform.local.expect("a nested physical without a local pose");
            let mut v = [t[0], t[1], t[2], 0.0];
            for n in &chain {
                (v, r) = (transform::global_position(n, v), transform::global_rotation(n, r));
            }
            let pose = &mut self.physicals[q].transform;
            (pose.position, pose.rotation) = ([v[0], v[1], v[2]], r);
        }
    }

    /// PhysicsManager2D::SyncTransforms for the instances with Transform changes, in the order they were made
    /// (`Instance::sync_transforms` has the cross-hierarchy caveat).
    pub fn sync_pending(&mut self) {
        for mut inst in std::mem::take(&mut self.unsynced) {
            let prefab = inst.prefab.clone();
            self.sync_transforms(&mut inst, &prefab);
            self.refresh_autofix_joints(&inst);
            // Later SetActive, Destroy, SetParent and bodyType changes need the clone.
            self.retained.push(inst);
        }
    }

    /// The physical's Transform ancestors, direct parent first.
    pub(super) fn parent_chain(&self, p: usize) -> Vec<transform::Trs> {
        let mut chain = Vec::new();
        let mut n = self.physicals[p].parent;
        while let Some(i) = n {
            chain.push(self.node_trs(i));
            n = self.transform_nodes[i].parent;
        }
        chain
    }

    /// PhysicsManager2D::Update (180bf9980) before the Update scripts: each awake interpolating body's Transform
    /// goes to its pose between the last two physics steps, by how far the frame time is past the last one.
    pub fn interpolate_bodies(&mut self) {
        self.sync_pending();
        let (last_time, last_delta) = self.last_simulation;
        if last_delta < f32::from_bits(0x34000000) {
            return;
        }
        let r = (self.time.timeAsDouble - last_time) / last_delta as f64;
        let t = (if 0.0 <= r { r.min(1.0) } else { 0.0 }) as f32;
        for p in self.rigidbody_order() {
            let rb = &self.physicals[p].rigidbody;
            // A kinematic body driven by a parent body follows the parent instead.
            if !rb.interpolating() || self.world.body(rb.body).flags & BODY_AWAKE == 0 || rb.bodyType == 1 && self.physicals[p].driven_by.is_some() {
                continue;
            }
            // RigidbodyInterpolation2D.Extrapolate is never set by the game (prefabs and Global.CurrentInterpolationMode
            // only use None and Interpolate).
            assert_eq!(rb.interpolation, 1, "Rigidbody2D extrapolation");
            let pose = interpolated_pose(rb.movement, t);
            let chain = self.parent_chain(p);
            let ph = &mut self.physicals[p];
            ph.rigidbody.movement.current = pose;
            let (local, world) = transform::set_world(&chain, pose.0, pose.1);
            (ph.transform.position, ph.transform.rotation, ph.transform.local) = (world.0, world.1, Some(local));
            self.refresh_nested(p);
        }
        for k in 0..self.node_rigidbodies.len() {
            let nb = &self.node_rigidbodies[k];
            let rb = &nb.rigidbody;
            if !rb.interpolating() || self.world.body(rb.body).flags & BODY_AWAKE == 0 || rb.bodyType == 1 && nb.driven_by.is_some() {
                continue;
            }
            assert_eq!(rb.interpolation, 1, "Rigidbody2D extrapolation");
            let pose = interpolated_pose(rb.movement, t);
            let node = nb.node;
            self.node_rigidbodies[k].rigidbody.movement.current = pose;
            self.set_node_world(node, pose);
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
    pub(super) fn update_joints(&mut self, dt: f32) {
        let mut next = self.joint_break_limits.first().copied();
        while let Some(j) = next {
            let at = self.joint_break_limits.iter().position(|&k| k == j).unwrap();
            next = self.joint_break_limits.get(at + 1).copied();
            let (force, torque, joint) = match j {
                BreakJoint::Hinge(h) => (self.hinges[h].breakForce, self.hinges[h].breakTorque, self.hinges[h].joint),
                // SpringJoint2D keeps Joint2D::Reset's infinite breakTorque.
                BreakJoint::Spring(l) => (self.limbs[l].gore.breakForce, f32::INFINITY, self.limbs[l].gore.joint),
                BreakJoint::Fixed(f) => (self.fixed_joints[f].state.break_force, self.fixed_joints[f].state.break_torque, self.fixed_joints[f].joint),
                BreakJoint::Anchored(a) => (self.anchored_joints[a].break_force, self.anchored_joints[a].break_torque, self.anchored_joints[a].joint),
            };
            // Joint2D::GetReactionForce/Torque: zero without a b2 joint or under 0.0001s. b2RevoluteJoint gives
            // inv_dt times the point impulse and inv_dt times the limit impulse as torque; b2DistanceJoint gives
            // (inv_dt * impulse) * u and no torque.
            let inv_dt = 1.0 / dt;
            let ([fx, fy], t) = match joint.filter(|_| 0.0001 <= dt).map(|id| &self.world.joints[id].as_ref().unwrap().kind) {
                None => ([0.0, 0.0], 0.0),
                Some(JointKind::Revolute(r)) => ([r.impulse[0] * inv_dt, r.impulse[1] * inv_dt], r.impulse[2] * inv_dt),
                Some(JointKind::Weld(w)) => {
                    let f = w.reaction_force(inv_dt);
                    ([f.x, f.y], w.reaction_torque(inv_dt))
                }
                Some(JointKind::Distance(d)) => {
                    let f = d.reaction_force(inv_dt);
                    ([f.x, f.y], 0.0)
                }
                Some(JointKind::Rope(r)) => {
                    let f = r.reaction_force(inv_dt);
                    ([f.x, f.y], 0.0)
                }
                Some(JointKind::Wheel(w)) => {
                    let f = w.reaction_force(inv_dt);
                    ([f.x, f.y], w.motor_torque(inv_dt))
                }
                Some(JointKind::Prismatic(p)) => {
                    let f = p.reaction_force(inv_dt);
                    ([f.x, f.y], p.reaction_torque(inv_dt))
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
                    BreakJoint::Anchored(a) => {
                        // SendMessage("OnJointBreak2D") finds PhysicalBehaviour, LimbBehaviour and
                        // CirculationBehaviour at most, and each only compares the joint with its own penetration or
                        // limb joint, never a prefab's. DestroyObjectHighLevel then runs Joint2D::Cleanup.
                        if let Some(id) = self.anchored_joints[a].joint.take() {
                            self.world.destroy_joint(id);
                        }
                        // The component is gone: `(bool)joint` reads false.
                        self.destroyed_components.insert(self.anchored_joints[a].component);
                    }
                    BreakJoint::Fixed(f) => {
                        // SendMessage("OnJointBreak2D") to the joint's GameObject: PhysicalBehaviour's only compares
                        // `penetration.Joint` (a SliderJoint2D) with it, and no other rewritten script sits on a piece
                        // (a stabber's other scripts compare against their own joints too).
                        if self.fixed_joints[f].slider.is_some() && matches!(self.fixed_joints[f].attached, BodyOwner::Physical(_)) {
                            self.physical_on_joint_break_slider(f);
                        }
                        self.sticky_grenade_on_joint_break(f);
                        // DestroyObjectHighLevel then removes the component.
                        self.fixed_joint_destroy(f);
                    }
                }
                // The JointBreakSoundBehaviours on the joint's GameObject come after the scripts above in component
                // order, and the destroy draws nothing.
                self.joint_break_sound_on_break(j);
                self.joint_helpers_on_break(j);
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
                // OnTriggerEnter2D/OnTriggerExit2D(other) of the WaterBehaviour on the trigger's GameObject; the other
                // rewritten OnTrigger*2D (FirearmAttachmentPointBehaviour) does nothing without attachments.
                if matches!(r.message, Message::TriggerEnter | Message::TriggerExit) {
                    for (own, other) in [(r.collider_a, r.collider_b), (r.collider_b, r.collider_a)] {
                        if let Some(w) = self.waters.iter().position(|w| w.trigger == own.id) {
                            self.water_on_trigger(w, other.id);
                        }
                    }
                }
                continue;
            };
            for (collider, rigidbody) in [(r.collider_a, r.rigidbody_a), (r.collider_b, r.rigidbody_b)] {
                let c = self.world.script_collision(col, collider.id);
                if let Some(p) = self.physical_of_collider(collider.id) {
                    self.send_collision_message(p, r.message, &c);
                }
                if let Some(rb) = rigidbody {
                    let w = &self.world;
                    if let Some(p) = self.physicals.iter().position(|ph| ph.rigidbody.detached.is_none() && w.body(ph.rigidbody.body).user.is_some_and(|u| u.id == rb.id)) {
                        self.send_collision_message(p, r.message, &c);
                    }
                }
            }
        }
    }

    /// GameObject.SendMessage of a collision message to a physical object's GameObject, in component order
    /// (PhysicalBehaviour, then LimbBehaviour; a DestroyableBehaviour where it sits). Collision messages reach
    /// disabled behaviours too.
    fn send_collision_message(&mut self, p: usize, m: Message, c: &ScriptCollision) {
        let limb = self.limbs.iter().position(|l| l.PhysicalBehaviour == p);
        let destroyable = self.destroyables.iter().position(|x| x.PhysicalBehaviour == p);
        let explosive = self.explosives.iter().position(|x| x.physical == Some(p));
        // The other scripts that take collisions, before and after the PhysicalBehaviour, in component order.
        let act_on_collide = self.act_on_collides.iter().position(|x| x.physical == Some(p));
        let side = |g: &Self, after: bool| -> Vec<(usize, super::acts::CollisionScript)> {
            use super::acts::CollisionScript as S;
            let mut v = Vec::new();
            if let Some(x) = destroyable.filter(|&x| g.destroyables[x].after_physical == after) {
                v.push((g.destroyables[x].index, S::Destroyable(x)));
            }
            if let Some(e) = explosive.filter(|&e| g.explosives[e].after_physical == after) {
                v.push((g.explosives[e].index, S::Explosive(e)));
            }
            if let Some(a) = act_on_collide.filter(|&a| g.act_on_collides[a].after_physical == after) {
                v.push((g.act_on_collides[a].index, S::ActOnCollide(a)));
            }
            if let Some(s) = g.sticky_grenades.iter().position(|s| s.physical == Some(p) && s.after_physical == after) {
                v.push((g.sticky_grenades[s].index, S::Sticky(s)));
            }
            if let Some(b) = g.bulbs.iter().position(|b| b.Phys == p && b.after_physical == after) {
                v.push((g.bulbs[b].index, S::Bulb(b)));
            }
            v.sort_by_key(|&(i, _)| i);
            v
        };
        let (before, after) = (side(self, false), side(self, true));
        match m {
            Message::CollisionEnter => {
                for (_, s) in before {
                    self.collision_enter_script(s, c);
                }
                self.physical_on_collision_enter(p, c);
                if let Some(l) = limb {
                    self.limb_on_collision_enter(l, c);
                }
                for (_, s) in after {
                    self.collision_enter_script(s, c);
                }
            }
            Message::CollisionStay => {
                for (_, s) in before {
                    self.collision_stay_script(s, c);
                }
                self.physical_on_collision_stay(p, c);
                if let Some(l) = limb {
                    self.limb_on_collision_stay(l, c);
                }
                for (_, s) in after {
                    self.collision_stay_script(s, c);
                }
            }
            Message::CollisionExit => self.physical_on_collision_exit(p, c),
            Message::TriggerEnter | Message::TriggerStay | Message::TriggerExit => {}
        }
    }
}

/// UnityEngine.FixedJoint2D over its b2 weld joint, with the Joint2D/AnchoredJoint2D fields native Create reads.
/// A SliderJoint2D shares the arena and every Joint2D/AnchoredJoint2D setter: `slider` holds its own state and the
/// joint is a b2PrismaticJoint (BreakJoint::Fixed covers both).
#[derive(Clone, Debug)]
pub struct FixedJoint {
    pub state: FixedJoint2d,
    pub slider: Option<SliderJoint2d>,
    /// m_Joint (+0x58)
    pub joint: Option<JointId>,
    /// The owner of the Rigidbody2D that is body A, and the connectedBody's (None: the scene's ground body).
    pub attached: BodyOwner,
    pub connected: Option<BodyOwner>,
    /// GameObject.activeInHierarchy and Behaviour.enabled: Create only runs when both hold.
    pub active: bool,
    pub enabled: bool,
    /// The component has been destroyed.
    pub destroyed: bool,
}

/// What FixedJoint2D::Create reads from its two ends.
#[derive(Clone, Copy, Debug)]
struct Ends {
    body_a: BodyId,
    body_b: Option<BodyId>,
    scale_a: Vec2,
    scale_b: Vec2,
}

impl Game {
    fn body_frame(&self, id: BodyId) -> BodyFrame {
        let b = self.world.body(id);
        BodyFrame { position: b.xf.p, q: b.xf.q, angle: b.sweep.a }
    }

    fn fixed_ends(&self, f: usize) -> Ends {
        let j = &self.fixed_joints[f];
        let scale = |o: BodyOwner| {
            let s = self.owner_lossy_scale(o);
            Vec2::new(s[0], s[1])
        };
        Ends {
            body_a: self.owner_body(j.attached),
            body_b: j.connected.map(|c| self.owner_body(c)),
            scale_a: scale(j.attached),
            scale_b: j.connected.map_or(Vec2::new(1.0, 1.0), scale),
        }
    }

    /// AddComponent<FixedJoint2D>(): Reset's defaults, then AddToManager (180c02580) creates the joint (to the
    /// ground body: connectedBody is still unset) when the GameObject is active and the component enabled.
    pub fn add_fixed_joint_component(&mut self, attached: usize) -> usize {
        let f = self.fixed_joints.len();
        self.fixed_joints.push(FixedJoint { state: FixedJoint2d::reset(), slider: None, joint: None, attached: BodyOwner::Physical(attached), connected: None, active: true, enabled: true, destroyed: false });
        self.fixed_joint_create_if_active(f);
        f
    }

    /// AddComponent<SliderJoint2D>(): SliderJoint2D::Reset (180c05740) turns autoConfigureConnectedAnchor off, then
    /// AddToManager creates the joint (to the ground body) when the GameObject is active and the component enabled.
    pub fn add_slider_joint_component(&mut self, attached: usize) -> usize {
        let f = self.fixed_joints.len();
        let mut state = FixedJoint2d::reset();
        state.auto_configure_connected_anchor = false;
        self.fixed_joints.push(FixedJoint { state, slider: Some(SliderJoint2d::reset()), joint: None, attached: BodyOwner::Physical(attached), connected: None, active: true, enabled: true, destroyed: false });
        self.fixed_joint_create_if_active(f);
        f
    }

    fn fixed_joint_create_if_active(&mut self, f: usize) {
        let j = &self.fixed_joints[f];
        if j.active && j.enabled {
            let ends = self.fixed_ends(f);
            self.fixed_joint_create(f, ends);
        }
    }

    /// FixedJoint2D::Create (180c04160) with Joint2D::FinalizeCreateJoint (180c048c0). The refusals (no body, the
    /// same body at both ends, bodies of different worlds) log and leave `joint` unset; the connected anchor
    /// Create auto-configured before them stays stored.
    fn fixed_joint_create(&mut self, f: usize, ends: Ends) {
        if !self.fixed_joints[f].active {
            return;
        }
        let frame_a = self.body_frame(ends.body_a);
        let connected = match ends.body_b {
            None => Connected::World,
            Some(b) => Connected::Body { frame: self.body_frame(b), lossy_scale: ends.scale_b },
        };
        let j = &mut self.fixed_joints[f];
        let def: JointKind = match j.slider.as_mut() {
            Some(slider) => slider.create_def(&mut j.state, ends.scale_a, &frame_a, &connected).into(),
            None => j.state.create_def(ends.scale_a, &frame_a, &connected).into(),
        };
        // FinalizeCreateJoint: body B is the connected body, or the scene's ground body. The one scene has one
        // world, so the different-worlds refusal cannot arise.
        let body_b = ends.body_b.unwrap_or_else(|| self.world.ground_body());
        if check_bodies(Some((ends.body_a, 0u8)), (body_b, 0u8)).is_err() {
            return;
        }
        let collide = self.fixed_joints[f].state.enable_collision;
        let id = self.world.create_joint(def, ends.body_a, body_b, collide);
        self.world.wake_joined(ends.body_a);
        self.world.wake_joined(body_b);
        let j = &mut self.fixed_joints[f];
        j.joint = Some(id);
        let (force, torque) = (j.state.break_force, j.state.break_torque);
        self.add_break_limits_update(BreakJoint::Fixed(f), force, torque);
        self.fixed_joints[f].state.finish_create();
    }

    /// FixedJoint2D::Cleanup (180c03480): a live b2 joint is destroyed (DestroyJoint wakes both ends; the
    /// FlushSimulationChanges after it does nothing while playing).
    pub(super) fn fixed_joint_cleanup(&mut self, f: usize, destroyed: bool) {
        let live = self.fixed_joints[f].joint.map(|id| self.world.joints[id].as_ref().unwrap().reference_angle());
        self.fixed_joints[f].state.cleanup(destroyed, live);
        if let Some(id) = self.fixed_joints[f].joint.take() {
            self.world.destroy_joint(id);
        }
    }

    /// Joint2D::ReCreate (180c05570): Cleanup, then Create when the GameObject is active and the component enabled.
    /// The break-limit entry stays; Create's AddBreakLimitsUpdate moves it to the back.
    pub(super) fn fixed_joint_recreate(&mut self, f: usize) {
        self.fixed_joint_cleanup(f, false);
        self.fixed_joint_create_if_active(f);
    }

    /// AnchoredJoint2D::SetAnchor (180bf85b0): recreated only while a b2 joint exists.
    pub fn fixed_joint_set_anchor(&mut self, f: usize, anchor: [f32; 2]) {
        self.fixed_joints[f].state.anchor = Vec2::new(anchor[0], anchor[1]);
        if self.fixed_joints[f].joint.is_some() {
            self.fixed_joint_recreate(f);
        }
    }

    /// AnchoredJoint2D::SetConnectedAnchor (180bf8630)
    pub fn fixed_joint_set_connected_anchor(&mut self, f: usize, anchor: [f32; 2]) {
        self.fixed_joints[f].state.connected_anchor = Vec2::new(anchor[0], anchor[1]);
        if self.fixed_joints[f].joint.is_some() {
            self.fixed_joint_recreate(f);
        }
    }

    /// AnchoredJoint2D::SetAutoConfigureConnectedAnchor (180bf85d0)
    pub fn fixed_joint_set_auto_configure(&mut self, f: usize, v: bool) {
        self.fixed_joints[f].state.auto_configure_connected_anchor = v;
        if self.fixed_joints[f].joint.is_some() {
            self.fixed_joint_recreate(f);
        }
    }

    /// Joint2D::SetConnectedBody (180c05a20): the new body is stored, then Cleanup(destroyed = true) forgets the
    /// stored reference angle and Create runs again whether or not a b2 joint existed. (A body of another scene
    /// is refused with a log; the rewrite has one scene. PhysicsManager2D::AutoSyncTransforms runs in between.)
    pub fn fixed_joint_set_connected_body(&mut self, f: usize, connected: Option<BodyOwner>) {
        self.fixed_joints[f].connected = connected;
        self.sync_pending();
        self.fixed_joint_cleanup(f, true);
        self.fixed_joint_create_if_active(f);
    }

    /// Joint2D::SetEnableCollision (180c05d80): stored, then Cleanup(false) and Create.
    pub fn fixed_joint_set_enable_collision(&mut self, f: usize, v: bool) {
        self.fixed_joints[f].state.enable_collision = v;
        self.fixed_joint_recreate(f);
    }

    /// FixedJoint2D::SetFrequency / SetDampingRatio: clamped and written into the live b2 joint without recreating it.
    pub fn fixed_joint_set_frequency(&mut self, f: usize, v: f32) {
        assert!(self.fixed_joints[f].slider.is_none(), "frequency of a SliderJoint2D");
        let live = self.fixed_joints[f].joint.map(|id| self.world.joints[id].as_mut().unwrap().weld_mut());
        self.fixed_joints[f].state.set_frequency(v, live);
    }

    pub fn fixed_joint_set_damping_ratio(&mut self, f: usize, v: f32) {
        assert!(self.fixed_joints[f].slider.is_none(), "damping ratio of a SliderJoint2D");
        let live = self.fixed_joints[f].joint.map(|id| self.world.joints[id].as_mut().unwrap().weld_mut());
        self.fixed_joints[f].state.set_damping_ratio(v, live);
    }

    /// Joint2D::SetBreakForce (180c05980) / SetBreakTorque (180c059d0): the clamped value is stored, then
    /// AddBreakLimitsUpdate (180c02540) moves the joint to the back of the list, or off it when both limits are
    /// infinite.
    pub fn fixed_joint_set_break_force(&mut self, f: usize, v: f32) {
        self.fixed_joints[f].state.break_force = clamp_break(v);
        self.fixed_joint_update_break_limits(f);
    }

    pub fn fixed_joint_set_break_torque(&mut self, f: usize, v: f32) {
        self.fixed_joints[f].state.break_torque = clamp_break(v);
        self.fixed_joint_update_break_limits(f);
    }

    fn fixed_joint_update_break_limits(&mut self, f: usize) {
        let (force, torque) = (self.fixed_joints[f].state.break_force, self.fixed_joints[f].state.break_torque);
        self.add_break_limits_update(BreakJoint::Fixed(f), force, torque);
    }

    /// SliderJoint2D::SetAutoConfigureAngle (180c05950): only stored.
    pub fn slider_joint_set_auto_configure_angle(&mut self, f: usize, v: bool) {
        self.fixed_joints[f].slider.as_mut().unwrap().auto_configure_angle = v;
    }

    /// SliderJoint2D::SetAngle (180c05890): the clamped value is stored; a live joint is destroyed (Cleanup(true))
    /// and created again.
    pub fn slider_joint_set_angle(&mut self, f: usize, v: f32) {
        self.fixed_joints[f].slider.as_mut().unwrap().set_angle(v);
        if self.fixed_joints[f].joint.is_some() {
            self.fixed_joint_cleanup(f, true);
            self.fixed_joint_create_if_active(f);
        }
    }

    /// SliderJoint2D::SetUseMotor (180c062f0) / SetUseLimits (180c062d0): stored, then written to the live joint.
    pub fn slider_joint_set_use_motor(&mut self, f: usize, v: bool) {
        self.fixed_joints[f].slider.as_mut().unwrap().use_motor = v;
        if let Some(id) = self.fixed_joints[f].joint {
            prismatic::enable_motor(&mut self.world, id, v);
        }
    }

    pub fn slider_joint_set_use_limits(&mut self, f: usize, v: bool) {
        self.fixed_joints[f].slider.as_mut().unwrap().use_limits = v;
        if let Some(id) = self.fixed_joints[f].joint {
            prismatic::enable_limit(&mut self.world, id, v);
        }
    }

    /// SliderJoint2D::SetMotor (180c06190): clamped and stored with useMotor on; a live joint gets EnableMotor(true),
    /// the speed and the maximum force.
    pub fn slider_joint_set_motor(&mut self, f: usize, speed: f32, max_torque: f32) {
        let s = self.fixed_joints[f].slider.as_mut().unwrap();
        s.set_motor(speed, max_torque);
        let (speed, torque) = (s.motor_speed, s.max_motor_torque);
        if let Some(id) = self.fixed_joints[f].joint {
            prismatic::enable_motor(&mut self.world, id, true);
            prismatic::set_motor_speed(&mut self.world, id, speed);
            prismatic::set_max_motor_force(&mut self.world, id, torque);
        }
    }

    /// SliderJoint2D::SetLimits (180c06000): clamped and stored with useLimits on; a live joint gets EnableLimit(true)
    /// and the limits.
    pub fn slider_joint_set_limits(&mut self, f: usize, lower: f32, upper: f32) {
        let s = self.fixed_joints[f].slider.as_mut().unwrap();
        s.set_limits(lower, upper);
        let (lower, upper) = (s.lower, s.upper);
        if let Some(id) = self.fixed_joints[f].joint {
            prismatic::enable_limit(&mut self.world, id, true);
            prismatic::set_limits(&mut self.world, id, lower, upper);
        }
    }

    /// The component's destruction (Joint2D::Deactivate with a destroy operation): Cleanup(true), and the
    /// break-limit entry leaves the scene's list (Joint2D::ThreadedCleanup).
    pub fn fixed_joint_destroy(&mut self, f: usize) {
        self.fixed_joint_cleanup(f, true);
        self.joint_break_limits.retain(|&k| k != BreakJoint::Fixed(f));
        self.fixed_joints[f].destroyed = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn fixture() -> Game {
        use crate::b2::world::{BodyDef, BodyType};
        let mut g = Game::default();
        for (x, angle) in [(0.0, 0.0), (2.0, 0.5)] {
            let body = g.world.create_body(&BodyDef { kind: BodyType::Dynamic, position: Vec2::new(x, 1.0), angle, ..Default::default() });
            let mut p = Physical::default();
            p.rigidbody.body = body;
            p.transform.localScale = [1.0, 1.0, 1.0];
            p.transform.rotation = [0.0, 0.0, 0.0, 1.0];
            g.physicals.push(p);
        }
        g
    }

    fn weld(g: &Game, f: usize) -> (crate::b2::weld::WeldDef, usize, usize) {
        let j = g.world.joints[g.fixed_joints[f].joint.unwrap()].as_ref().unwrap();
        (j.weld().def, j.body_a, j.body_b)
    }

    #[test]
    fn add_component_welds_to_ground_then_connected_body_recreates() {
        let mut g = fixture();
        let f = g.add_fixed_joint_component(0);
        let (def, a, b) = weld(&g, f);
        assert_eq!((a, b), (g.physicals[0].rigidbody.body, g.world.ground_body()));
        assert_eq!(def.reference_angle, 0.0 - 0.0);
        g.fixed_joint_set_connected_body(f, Some(BodyOwner::Physical(1)));
        let (def, _, b) = weld(&g, f);
        assert_eq!(b, g.physicals[1].rigidbody.body);
        // The reference angle is connected minus attached; the forgotten one is derived again.
        assert_eq!(def.reference_angle, 0.5);
        assert_eq!(g.world.joints.iter().flatten().count(), 1);
        assert!(g.fixed_joints[f].state.anchor_configured);
    }

    #[test]
    fn recreate_keeps_live_reference_angle_and_reconfigures_anchor() {
        let mut g = fixture();
        let f = g.add_fixed_joint_component(0);
        g.fixed_joint_set_connected_body(f, Some(BodyOwner::Physical(1)));
        g.world.body_mut(g.physicals[1].rigidbody.body).sweep.a = 2.0;
        g.fixed_joint_set_anchor(f, [0.5, 0.0]);
        let (def, _, _) = weld(&g, f);
        assert_eq!(def.reference_angle, 0.5, "the live joint's reference angle survives ReCreate");
        assert_eq!(def.local_anchor_a, Vec2::new(0.5, 0.0));
        // Auto-configure ran again: the connected anchor is the world point in body B's frame.
        assert_eq!(g.fixed_joints[f].state.connected_anchor, def.local_anchor_b);
        // Re-selecting the connected body forgets it.
        g.fixed_joint_set_connected_body(f, Some(BodyOwner::Physical(1)));
        assert_eq!(weld(&g, f).0.reference_angle, 2.0 - 0.0);
    }

    #[test]
    fn same_body_is_refused_without_a_joint() {
        let mut g = fixture();
        let f = g.add_fixed_joint_component(0);
        g.fixed_joint_set_connected_body(f, Some(BodyOwner::Physical(0)));
        assert!(g.fixed_joints[f].joint.is_none());
        assert_eq!(g.world.joints.iter().flatten().count(), 0);
        assert!(!g.fixed_joints[f].state.anchor_configured);
        // Anchor setters leave it alone while there is no b2 joint.
        g.fixed_joint_set_anchor(f, [1.0, 0.0]);
        assert!(g.fixed_joints[f].joint.is_none());
        // An inactive object creates nothing either.
        g.fixed_joints[f].active = false;
        g.fixed_joint_set_connected_body(f, Some(BodyOwner::Physical(1)));
        assert!(g.fixed_joints[f].joint.is_none());
    }

    #[test]
    fn break_limit_list_and_break() {
        let mut g = fixture();
        let a = g.add_fixed_joint_component(0);
        let b = g.add_fixed_joint_component(1);
        assert!(g.joint_break_limits.is_empty(), "both limits infinite");
        g.fixed_joint_set_break_force(a, 10.0);
        g.fixed_joint_set_break_force(b, 10.0);
        assert_eq!(g.joint_break_limits, vec![BreakJoint::Fixed(a), BreakJoint::Fixed(b)]);
        // ReCreate moves the entry to the back.
        g.fixed_joint_set_auto_configure(a, true);
        assert_eq!(g.joint_break_limits, vec![BreakJoint::Fixed(b), BreakJoint::Fixed(a)]);
        // Infinite again: off the list.
        g.fixed_joint_set_break_force(b, f32::INFINITY);
        assert_eq!(g.joint_break_limits, vec![BreakJoint::Fixed(a)]);
        // A reaction over the limit breaks it: b2 joint and list entry go.
        let id = g.fixed_joints[a].joint.unwrap();
        g.world.joints[id].as_mut().unwrap().weld_mut().impulse = [1.0, 0.0, 0.0];
        g.update_joints(1.0 / 50.0);
        assert!(g.fixed_joints[a].joint.is_none() && g.fixed_joints[a].destroyed);
        assert!(g.joint_break_limits.is_empty());
        assert!(g.fixed_joints[b].joint.is_some());
        // Under the limit it stays.
        g.fixed_joint_set_break_torque(b, 5.0);
        let id = g.fixed_joints[b].joint.unwrap();
        g.world.joints[id].as_mut().unwrap().weld_mut().impulse = [0.0, 0.0, 0.05];
        g.update_joints(1.0 / 50.0);
        assert!(g.fixed_joints[b].joint.is_some());
    }

    #[test]
    fn frequency_writes_through_and_cleanup_wakes() {
        let mut g = fixture();
        let f = g.add_fixed_joint_component(0);
        g.fixed_joint_set_frequency(f, 8.0);
        g.fixed_joint_set_damping_ratio(f, 3.0);
        let (def, _, _) = weld(&g, f);
        assert_eq!((def.frequency_hz, def.damping_ratio), (8.0, 1.0));
        // The recreated joint is built from the component's values.
        g.fixed_joint_set_enable_collision(f, true);
        assert_eq!(weld(&g, f).0.frequency_hz, 8.0);
        assert!(g.world.joints[g.fixed_joints[f].joint.unwrap()].as_ref().unwrap().collide_connected);
    }
}
