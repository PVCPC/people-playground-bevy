//! Unity's PhysicsContacts2D and WorldContactListener2D (UnityPlayer 2020.3): the Collision2D record the b2
//! listener keeps per collider pair, and ProcessContacts turning the records into OnCollision*2D / OnTrigger*2D
//! reports after each simulation step.
use super::collide::WorldManifold;
use super::math::Vec2;
use super::solver::VelocityConstraint;
use super::world::{BodyId, ColliderRef, ContactId, FixtureId, RigidbodyRef, World, BODY_AWAKE, CONTACT_ENABLED, CONTACT_KEEP_DISABLED};
use std::collections::HashMap;

/// Effector2D::EffectorContact (0x50 bytes): one b2Contact of an effector's collider, from the effector side: the
/// attached bodies, fixtures and child indices of the effector's collider and of the other.
#[derive(Clone, Copy, Debug)]
pub struct EffectorContact {
    pub body: BodyId,
    pub fixture: FixtureId,
    pub child: usize,
    pub other_body: BodyId,
    pub other_fixture: FixtureId,
    pub other_child: usize,
    pub contact: ContactId,
}

/// The Effector2D fields the b2 listeners read (useColliderMask +0x60, colliderMask +0x64) and its contact array (+0x40).
#[derive(Clone, Debug, Default)]
pub struct Effector2D {
    pub use_collider_mask: bool,
    pub collider_mask: i32,
    pub contacts: Vec<EffectorContact>,
}

/// Collision2D::ContactState.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ContactState {
    #[default]
    Invalid,
    Enter,
    Exit,
    EnterAndExit,
    Stay,
}

/// Collision2D::Manifold (0x70 bytes): one b2Contact of the pair, fixture A being the first collider's.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Manifold2D {
    pub enabled: bool,
    pub contact: Option<ContactId>,
    pub fixture_a: FixtureId,
    pub fixture_b: FixtureId,
    pub index_a: usize,
    pub index_b: usize,
    pub simulation_id: i32,
    pub point_count: usize,
    pub normal: Vec2,
    pub points: [Vec2; 2],
    pub relative_velocity: [Vec2; 2],
    pub separations: [f32; 2],
    pub normal_impulses: [f32; 2],
    pub tangent_impulses: [f32; 2],
}

/// Collision2D (native): collider A has the lower instance id (m_Key is the pair in that order).
#[derive(Clone, Debug, Default)]
pub struct Collision2D {
    pub manifolds: Vec<Manifold2D>,
    pub state: ContactState,
    pub contact_count: i32,
    pub collider_a: ColliderRef,
    pub collider_b: ColliderRef,
    pub rigidbody_a: Option<BodyId>,
    pub rigidbody_b: Option<BodyId>,
    pub enabled: bool,
    pub is_trigger: bool,
    pub flag_for_recreate: bool,
    /// The b2Contact's fixture A belongs to collider B.
    pub swapped: bool,
    /// The Rigidbody2D instance ids of A and B, taken when the report was made: the component outlives its b2 body
    /// (a disintegrated limb) until the callbacks have all run.
    pub rigidbody_ids: [Option<i32>; 2],
}

pub type CollisionId = usize;

/// PhysicsContacts2D.
#[derive(Default)]
pub struct Contacts2D {
    pub records: Vec<Option<Collision2D>>,
    free: Vec<CollisionId>,
    /// m_Collisions, in array order.
    pub collisions: Vec<CollisionId>,
    /// m_CollisionIndex: collider pair -> index into `collisions`.
    pub index: HashMap<(i32, i32), usize>,
    pub simulation_id: i32,
    /// Physics2DSettings.m_CallbacksOnDisable (off in People Playground).
    pub callbacks_on_disable: bool,
    /// Each Collider2D's m_Shapes count.
    pub collider_shapes: HashMap<i32, usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Message {
    CollisionEnter,
    CollisionStay,
    CollisionExit,
    TriggerEnter,
    TriggerStay,
    TriggerExit,
}

/// Collision2D::CollisionReport / TriggerReport: a rigidbody is only kept when it sits on another GameObject.
#[derive(Clone, Debug)]
pub struct Report {
    pub message: Message,
    pub collider_a: ColliderRef,
    pub collider_b: ColliderRef,
    pub rigidbody_a: Option<RigidbodyRef>,
    pub rigidbody_b: Option<RigidbodyRef>,
    /// The record as it was when reported (collision reports only).
    pub collision: Option<Collision2D>,
}

/// UnityEngine.ContactPoint2D: `collider` is the incoming collider, `other_collider` the receiver's own.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ContactPoint2D {
    pub point: Vec2,
    pub normal: Vec2,
    pub relative_velocity: Vec2,
    pub separation: f32,
    pub normal_impulse: f32,
    pub tangent_impulse: f32,
    pub collider: i32,
    pub other_collider: i32,
    pub rigidbody: Option<i32>,
    pub other_rigidbody: Option<i32>,
    pub enabled: bool,
}

/// UnityEngine.Collision2D as a callback receives it: `collider`/`rigidbody` are the incoming side's,
/// `other_collider`/`other_rigidbody` the receiver's own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScriptCollision {
    pub collider: i32,
    pub other_collider: i32,
    pub rigidbody: Option<i32>,
    pub other_rigidbody: Option<i32>,
    pub relative_velocity: Vec2,
    pub enabled: bool,
    pub contacts: Vec<ContactPoint2D>,
}

impl Contacts2D {
    fn record(&mut self, id: CollisionId) -> &mut Collision2D {
        self.records[id].as_mut().unwrap()
    }

    /// PhysicsContacts2D::RemoveContact (180c14960): the last record takes the removed one's place.
    fn remove(&mut self, at: usize) {
        let id = self.collisions[at];
        let r = self.records[id].take().unwrap();
        self.free.push(id);
        self.index.remove(&(r.collider_a.id, r.collider_b.id));
        let last = self.collisions.len() - 1;
        if at < last {
            let moved = self.collisions[last];
            let m = self.records[moved].as_ref().unwrap();
            self.index.insert((m.collider_a.id, m.collider_b.id), at);
            self.collisions[at] = moved;
        }
        self.collisions.pop();
    }

    /// PhysicsContacts2D::DestroyContacts (180c0fd00): every record with the collider goes without a callback,
    /// the last record moving into each freed slot.
    pub fn destroy_contacts(&mut self, collider: i32) {
        let mut i = 0;
        while i < self.collisions.len() {
            let r = self.records[self.collisions[i]].as_ref().unwrap();
            if r.collider_a.id == collider || r.collider_b.id == collider {
                self.remove(i);
            } else {
                i += 1;
            }
        }
    }

    /// Collider2D::Cleanup(kColliderRecreate)'s pass over m_CollisionIndex (180c0d940): each collision with the
    /// collider is kept through the recreate as a Stay.
    pub fn flag_for_recreate(&mut self, collider: i32) {
        for &c in &self.collisions {
            let r = self.records[c].as_mut().unwrap();
            if r.collider_a.id == collider || r.collider_b.id == collider {
                r.flag_for_recreate = true;
                r.state = ContactState::Stay;
            }
        }
    }

    /// A record restored from a game dump, appended in array order.
    pub fn restore(&mut self, r: Collision2D) -> CollisionId {
        let key = (r.collider_a.id, r.collider_b.id);
        let id = self.records.len();
        self.records.push(Some(r));
        self.index.insert(key, self.collisions.len());
        self.collisions.push(id);
        id
    }
}

fn negate(v: Vec2) -> Vec2 {
    Vec2::new(-v.x, -v.y)
}

impl World {
    fn rigidbody_of(&self, f: FixtureId) -> Option<BodyId> {
        let body = self.fixture(f).body;
        self.body(body).user.map(|_| body)
    }

    fn world_manifold(&self, id: ContactId) -> WorldManifold {
        let c = self.contact(id);
        let (a, b) = (self.body(self.fixture(c.fixture_a).body), self.body(self.fixture(c.fixture_b).body));
        WorldManifold::new(&c.manifold, &a.xf, c.radius_a, &b.xf, c.radius_b)
    }

    /// Effector2D::ProcessCheckEffectorColliderMask (180bfd510): whether `c`'s effector lets `other`'s layer touch it.
    pub(super) fn effector_mask_allows(&self, c: ColliderRef, other: ColliderRef) -> bool {
        match self.effector_colliders.get(&c.id) {
            None => true,
            // usedByEffector without an Effector2D on the GameObject.
            Some(None) => false,
            // IgnoreContacts() is false for a PointEffector2D (the only Effector2D the rewrite reads).
            Some(Some(e)) => {
                let e = &self.effectors[e];
                !e.use_collider_mask || e.collider_mask >> (self.layers[&other.game_object] & 31) & 1 != 0
            }
        }
    }

    /// Effector2D::BeginContact (180bfbd00): ProcessBeginContact (180bfd3c0) for fixture A's collider, then B's. A
    /// collider used by an effector adds the contact to the effector's array (UseContactUpdate is true for a
    /// PointEffector2D; its BeginContactCallback is empty).
    fn effector_begin_contact(&mut self, id: ContactId) {
        let c = self.contact(id);
        let (fa, fb, ia, ib) = (c.fixture_a, c.fixture_b, c.child_a, c.child_b);
        for (f, i, o, oi) in [(fa, ia, fb, ib), (fb, ib, fa, ia)] {
            let Some(&Some(e)) = self.effector_colliders.get(&self.fixture(f).collider.id) else { continue };
            let contact = EffectorContact { body: self.fixture(f).body, fixture: f, child: i, other_body: self.fixture(o).body, other_fixture: o, other_child: oi, contact: id };
            self.effectors.get_mut(&e).unwrap().contacts.push(contact);
        }
    }

    /// Effector2D::EndContact (180bfd190): ProcessEndContact (180bfd580) for fixture A's collider, then B's: the
    /// contact's entry leaves the effector's array, the last entry taking its place.
    fn effector_end_contact(&mut self, id: ContactId) {
        let c = self.contact(id);
        for f in [c.fixture_a, c.fixture_b] {
            let Some(&Some(e)) = self.effector_colliders.get(&self.fixture(f).collider.id) else { continue };
            let contacts = &mut self.effectors.get_mut(&e).unwrap().contacts;
            if let Some(at) = contacts.iter().position(|x| x.contact == id) {
                contacts.swap_remove(at);
            }
        }
    }

    /// WorldContactListener2D::BeginContact (180c1cdb0).
    pub fn begin_contact(&mut self, id: ContactId) {
        self.effector_begin_contact(id);
        let c = self.contact_mut(id);
        if c.flags & CONTACT_KEEP_DISABLED != 0 {
            c.flags &= !CONTACT_ENABLED;
        }
        let enabled = c.flags & CONTACT_ENABLED != 0;
        let (fa, fb, ia, ib) = (c.fixture_a, c.fixture_b, c.child_a, c.child_b);
        let (ca, cb) = (self.fixture(fa).collider, self.fixture(fb).collider);
        let swapped = cb.id < ca.id;
        let (f1, f2, c1, c2, i1, i2) = if swapped { (fb, fa, cb, ca, ib, ia) } else { (fa, fb, ca, cb, ia, ib) };
        let trigger = self.fixture(f1).is_sensor || self.fixture(f2).is_sensor;
        let manifold = Manifold2D { enabled, contact: Some(id), fixture_a: f1, fixture_b: f2, index_a: i1, index_b: i2, ..Default::default() };
        let existing = self.contacts2d.index.get(&(c1.id, c2.id)).map(|&at| self.contacts2d.collisions[at]);
        let Some(rid) = existing else {
            let mut r = Collision2D {
                manifolds: Vec::new(),
                state: ContactState::Enter,
                contact_count: 1,
                collider_a: c1,
                collider_b: c2,
                rigidbody_a: self.rigidbody_of(f1),
                rigidbody_b: self.rigidbody_of(f2),
                enabled,
                is_trigger: trigger,
                flag_for_recreate: false,
                swapped,
                rigidbody_ids: [None; 2],
            };
            if !trigger {
                r.manifolds.push(manifold);
                self.contact_mut(id).user_index = 0;
            }
            let c2d = &mut self.contacts2d;
            let rid = c2d.free.pop().unwrap_or(c2d.records.len());
            if rid == c2d.records.len() {
                c2d.records.push(None);
            }
            c2d.records[rid] = Some(r);
            c2d.index.insert((c1.id, c2.id), c2d.collisions.len());
            c2d.collisions.push(rid);
            self.contact_mut(id).user_data = Some(rid);
            return;
        };
        let r = self.contacts2d.record(rid);
        r.contact_count += 1;
        r.is_trigger = trigger;
        let mut user_index = None;
        if !trigger {
            r.manifolds.push(manifold);
            user_index = Some(r.manifolds.len() as i32 - 1);
        }
        let (recreate, record_enabled) = (r.flag_for_recreate, r.enabled);
        if !recreate {
            r.enabled = enabled;
            r.state = match r.state {
                ContactState::Exit => ContactState::Stay,
                ContactState::EnterAndExit => ContactState::Enter,
                s => s,
            };
        }
        let c = self.contact_mut(id);
        if let Some(i) = user_index {
            c.user_index = i;
        }
        c.user_data = Some(rid);
        if recreate {
            c.flags = if record_enabled { (c.flags & !CONTACT_KEEP_DISABLED) | CONTACT_ENABLED } else { (c.flags | CONTACT_KEEP_DISABLED) & !CONTACT_ENABLED };
        }
    }

    /// WorldContactListener2D::EndContact (180c1f010).
    pub(crate) fn end_contact(&mut self, id: ContactId) {
        self.effector_end_contact(id);
        let c = self.contact_mut(id);
        c.flags &= !CONTACT_KEEP_DISABLED;
        let (rid, ui) = (c.user_data.unwrap(), c.user_index);
        c.user_index = -1;
        c.user_data = None;
        let r = self.contacts2d.record(rid);
        r.contact_count -= 1;
        let mut moved = None;
        if !r.is_trigger && 0 <= ui {
            let ui = ui as usize;
            if matches!(r.state, ContactState::Enter | ContactState::EnterAndExit) {
                r.manifolds[ui].contact = None;
            } else {
                r.manifolds.swap_remove(ui);
                if ui < r.manifolds.len() {
                    moved = r.manifolds[ui].contact.map(|c| (c, ui));
                }
            }
        }
        if r.contact_count < 1 && !r.flag_for_recreate {
            r.state = match r.state {
                ContactState::Enter => ContactState::EnterAndExit,
                ContactState::Stay => ContactState::Exit,
                s => s,
            };
        }
        if let Some((c, ui)) = moved {
            self.contact_mut(c).user_index = ui as i32;
        }
    }

    /// WorldContactListener2D::PreSolve (180c20d50): the first PreSolve of a simulation step also takes each
    /// point's relative velocity (second body minus first) and clears the summed impulses.
    pub(crate) fn pre_solve(&mut self, id: ContactId) {
        let c = self.contact(id);
        let (rid, ui, enabled, point_count) = (c.user_data.unwrap(), c.user_index, c.flags & CONTACT_ENABLED != 0, c.manifold.point_count);
        let sim = self.contacts2d.simulation_id;
        self.contacts2d.record(rid).enabled = enabled;
        if ui < 0 {
            return;
        }
        let wm = self.world_manifold(id);
        let r = self.contacts2d.records[rid].as_ref().unwrap();
        let m = r.manifolds[ui as usize];
        let (a, b) = (self.body(self.fixture(m.fixture_a).body), self.body(self.fixture(m.fixture_b).body));
        let (ca, va, wa, cb, vb, wb) = (a.sweep.c, a.linear_velocity, a.angular_velocity, b.sweep.c, b.linear_velocity, b.angular_velocity);
        let swapped = r.swapped;
        let m = &mut self.contacts2d.record(rid).manifolds[ui as usize];
        m.normal = if swapped { wm.normal } else { negate(wm.normal) };
        m.point_count = point_count;
        for i in 0..point_count {
            let p = wm.points[i];
            m.points[i] = p;
            m.separations[i] = wm.separations[i];
            if m.simulation_id != sim {
                m.relative_velocity[i] = Vec2::new(
                    (-(wb * (p.y - cb.y)) + vb.x) - (-((p.y - ca.y) * wa) + va.x),
                    (wb * (p.x - cb.x) + vb.y) - ((p.x - ca.x) * wa + va.y),
                );
                m.normal_impulses[i] = 0.0;
                m.tangent_impulses[i] = 0.0;
            }
        }
        m.simulation_id = sim;
    }

    /// WorldContactListener2D::PostSolve (180c20d40), from b2Island::Report (180bb9d70) after an island's solve.
    pub(crate) fn post_solve(&mut self, contacts: &[ContactId], constraints: &[VelocityConstraint]) {
        for (&id, vc) in contacts.iter().zip(constraints) {
            let c = self.contact(id);
            let (rid, ui, point_count) = (c.user_data, c.user_index, c.manifold.point_count);
            if ui < 0 {
                continue;
            }
            let wm = self.world_manifold(id);
            let r = self.contacts2d.record(rid.unwrap());
            let swapped = r.swapped;
            let m = &mut r.manifolds[ui as usize];
            m.normal = if swapped { wm.normal } else { negate(wm.normal) };
            m.point_count = point_count;
            for i in 0..point_count {
                m.normal_impulses[i] = vc.points[i].normal_impulse + m.normal_impulses[i];
                m.tangent_impulses[i] = vc.points[i].tangent_impulse + m.tangent_impulses[i];
            }
        }
    }

    /// AddCollisionReport (180c0b990) / AddTriggerReport (180c0bd00).
    fn report(&self, r: &Collision2D, trigger: bool) -> Report {
        let message = match (r.state, trigger) {
            (ContactState::Enter | ContactState::EnterAndExit, false) => Message::CollisionEnter,
            (ContactState::Exit, false) => Message::CollisionExit,
            (_, false) => Message::CollisionStay,
            (ContactState::Enter | ContactState::EnterAndExit, true) => Message::TriggerEnter,
            (ContactState::Exit, true) => Message::TriggerExit,
            (_, true) => Message::TriggerStay,
        };
        let other_object = |rb: Option<BodyId>, collider: ColliderRef| rb.and_then(|b| self.body(b).user).filter(|u| u.game_object != collider.game_object);
        Report {
            message,
            collider_a: r.collider_a,
            collider_b: r.collider_b,
            rigidbody_a: other_object(r.rigidbody_a, r.collider_a),
            rigidbody_b: other_object(r.rigidbody_b, r.collider_b),
            collision: if trigger {
                None
            } else {
                let mut c = r.clone();
                c.rigidbody_ids = [r.rigidbody_a, r.rigidbody_b].map(|b| b.and_then(|b| self.body(b).user).map(|u| u.id));
                Some(c)
            },
        }
    }

    /// PhysicsContacts2D::ProcessContacts (180c13920) for the whole world on the single-threaded path, after
    /// PhysicsScene2D::Simulate. Returns the reports SendCallbackReports sends: triggers first, then collisions.
    pub fn process_contacts(&mut self, send: bool) -> Vec<Report> {
        let (mut triggers, mut collisions) = (Vec::new(), Vec::new());
        let mut i = 0;
        while i < self.contacts2d.collisions.len() {
            let rid = self.contacts2d.collisions[i];
            let r = self.contacts2d.record(rid);
            if r.flag_for_recreate {
                r.flag_for_recreate = false;
                if r.contact_count == 0 {
                    r.state = ContactState::Exit;
                }
            }
            let r = self.contacts2d.records[rid].as_ref().unwrap();
            let trigger = r.is_trigger;
            let shapes = |c: &ColliderRef| self.contacts2d.collider_shapes.get(&c.id).copied().unwrap_or(0);
            let skip = r.state == ContactState::Exit && !self.contacts2d.callbacks_on_disable && (shapes(&r.collider_a) == 0 || shapes(&r.collider_b) == 0);
            if send && !skip {
                let sleeping = |rb: Option<BodyId>| rb.map_or(true, |b| self.body(b).flags & BODY_AWAKE == 0);
                if !(r.state == ContactState::Stay && sleeping(r.rigidbody_a) && sleeping(r.rigidbody_b)) {
                    let report = self.report(r, trigger);
                    if trigger { triggers.push(report) } else { collisions.push(report) }
                }
            }
            let r = self.contacts2d.record(rid);
            let mut moved = Vec::new();
            if r.contact_count != r.manifolds.len() as i32 {
                let mut k = 0;
                while k < r.manifolds.len() {
                    if r.manifolds[k].contact.is_none() {
                        r.manifolds.swap_remove(k);
                        if k < r.manifolds.len() {
                            if let Some(c) = r.manifolds[k].contact {
                                moved.push((c, k));
                            }
                        }
                    } else {
                        k += 1;
                    }
                }
            }
            let state = r.state;
            for (c, k) in moved {
                self.contact_mut(c).user_index = k as i32;
            }
            match state {
                ContactState::Enter => {
                    self.contacts2d.record(rid).state = ContactState::Stay;
                    i += 1;
                }
                ContactState::Exit => self.contacts2d.remove(i),
                ContactState::EnterAndExit => {
                    if send {
                        if trigger {
                            let last = triggers.last_mut().unwrap();
                            last.message = Message::TriggerEnter;
                            let mut exit = last.clone();
                            exit.message = Message::TriggerExit;
                            triggers.push(exit);
                        } else {
                            let last = collisions.last_mut().unwrap();
                            last.message = Message::CollisionEnter;
                            let col = last.collision.as_mut().unwrap();
                            col.state = ContactState::Enter;
                            let mut exit = col.clone();
                            exit.state = ContactState::Exit;
                            exit.manifolds.clear();
                            exit.contact_count = 0;
                            let report = self.report(&exit, false);
                            collisions.push(report);
                        }
                    }
                    self.contacts2d.remove(i);
                }
                _ => i += 1,
            }
        }
        triggers.extend(collisions);
        triggers
    }

    /// PhysicsManager2D::ConvertCollision2DToScripting (180beb220) with PopulateContacts (180bf53c0): the
    /// Collision2D seen from `receiving`; collider B's view negates the normal and relative velocities.
    pub fn script_collision(&self, r: &Collision2D, receiving: i32) -> ScriptCollision {
        let rb = |i: usize, b: Option<BodyId>| r.rigidbody_ids[i].or_else(|| b.and_then(|b| self.body(b).user).map(|u| u.id));
        let flip = receiving == r.collider_b.id && receiving != r.collider_a.id;
        let (collider, other_collider, rigidbody, other_rigidbody) =
            if flip { (r.collider_a.id, r.collider_b.id, rb(0, r.rigidbody_a), rb(1, r.rigidbody_b)) } else { (r.collider_b.id, r.collider_a.id, rb(1, r.rigidbody_b), rb(0, r.rigidbody_a)) };
        let side = |v: Vec2| if flip { negate(v) } else { v };
        let mut contacts = Vec::new();
        for m in &r.manifolds {
            for i in 0..m.point_count {
                contacts.push(ContactPoint2D {
                    point: m.points[i],
                    normal: side(m.normal),
                    relative_velocity: side(m.relative_velocity[i]),
                    separation: m.separations[i],
                    normal_impulse: m.normal_impulses[i],
                    tangent_impulse: m.tangent_impulses[i],
                    collider,
                    other_collider,
                    rigidbody,
                    other_rigidbody,
                    enabled: r.enabled,
                });
            }
        }
        ScriptCollision {
            collider,
            other_collider,
            rigidbody,
            other_rigidbody,
            relative_velocity: side(r.manifolds.first().map_or(Vec2::ZERO, |m| m.relative_velocity[0])),
            enabled: r.enabled,
            contacts,
        }
    }
}
