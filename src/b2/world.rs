//! b2World as UnityPlayer.dll compiles it with m_UseMultithreading off: Unity's contact manager (TOI/non-TOI
//! contact arrays), island solve and sleep, using the game's Physics2DSettings. Native linked lists are Vecs
//! whose head is the LAST element, so `.iter().rev()` walks them in native order.

use super::broadphase::{Aabb, BroadPhase};
use super::collide::*;
use super::contacts2d::{CollisionId, Contacts2D};
use super::distance::{distance, DistanceProxy, SimplexCache};
use super::math::{Rot, Transform, Vec2};
use super::revolute::{JointBody, RevoluteDef, RevoluteJoint};
use super::shape::{Circle, MassData, Polygon, POLYGON_RADIUS};
use super::solver::*;
use super::toi::{time_of_impact, Sweep, ToiState};

const DEG_TO_RAD: f32 = f32::from_bits(0x3c8e_fa35);
/// Physics2DSettings (m_MaxTranslationSpeed etc.) as PhysicsManager2D copies them into the b2_ globals.
const MAX_TRANSLATION: f32 = 100.0;
const MAX_TRANSLATION_SQUARED: f32 = MAX_TRANSLATION * MAX_TRANSLATION;
const MAX_ROTATION: f32 = 360.0 * DEG_TO_RAD;
const MAX_ROTATION_SQUARED: f32 = MAX_ROTATION * MAX_ROTATION;
const TIME_TO_SLEEP: f32 = 0.5;
const LINEAR_SLEEP_TOLERANCE: f32 = 0.01;
const ANGULAR_SLEEP_TOLERANCE: f32 = 2.0 * DEG_TO_RAD;
/// b2_maxSubSteps; SolveTOI also stops when the earliest impact is past 1 - 10 FLT_EPSILON.
const MAX_SUB_STEPS: i32 = 8;
const ONE_MINUS_TEN_EPSILON: f32 = f32::from_bits(0x3f7f_ffec);
const TOI_BODY_CAPACITY: usize = 64;
const TOI_CONTACT_CAPACITY: usize = 32;

pub const BODY_ISLAND: u16 = 0x1;
pub const BODY_AWAKE: u16 = 0x2;
pub const BODY_AUTO_SLEEP: u16 = 0x4;
pub const BODY_BULLET: u16 = 0x8;
pub const BODY_FIXED_ROTATION: u16 = 0x10;
pub const BODY_ACTIVE: u16 = 0x20;
/// Unity: created asleep this step, so contact creation does not wake it. Cleared by every Solve.
pub const BODY_STARTED_ASLEEP: u16 = 0x100;

pub const CONTACT_ISLAND: u32 = 0x1;
pub const CONTACT_TOUCHING: u32 = 0x2;
pub const CONTACT_ENABLED: u32 = 0x4;
pub const CONTACT_FILTER: u32 = 0x8;
/// The contact's toi is current for this sub-step.
pub const CONTACT_TOI: u32 = 0x20;
/// Unity: the user disabled the contact, so Update does not re-enable it.
pub const CONTACT_KEEP_DISABLED: u32 = 0x40;
/// Unity: a non-sensor contact with a bullet body, kept in the TOI contact array.
pub const CONTACT_CONTINUOUS: u32 = 0x80;

const WORLD_NEW_FIXTURE: u32 = 0x1;
const WORLD_LOCKED: u32 = 0x2;
const WORLD_CLEAR_FORCES: u32 = 0x4;

pub type BodyId = usize;
pub type FixtureId = usize;
pub type ContactId = usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum BodyType {
    #[default]
    Static,
    Kinematic,
    Dynamic,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    Circle(Circle),
    Polygon(Polygon),
}

impl Shape {
    /// b2Shape::Type: circle 0, edge 1, polygon 2, chain 3, capsule 4.
    fn type_index(&self) -> usize {
        match self {
            Shape::Circle(_) => 0,
            Shape::Polygon(_) => 2,
        }
    }

    pub fn radius(&self) -> f32 {
        match self {
            Shape::Circle(c) => c.radius,
            Shape::Polygon(p) => p.radius,
        }
    }

    /// b2PolygonShape::ComputeAABB (180bb2bc0) and b2CircleShape::ComputeAABB (180bb2ac0).
    fn compute_aabb(&self, xf: &Transform) -> Aabb {
        match self {
            Shape::Circle(c) => {
                let p = xf.apply(c.p);
                Aabb { lower: Vec2::new(p.x - c.radius, p.y - c.radius), upper: Vec2::new(c.radius + p.x, c.radius + p.y) }
            }
            Shape::Polygon(polygon) => {
                let first = xf.apply(polygon.vertices[0]);
                let (mut lower, mut upper) = (first, first);
                for &v in &polygon.vertices[1..polygon.count] {
                    let v = xf.apply(v);
                    if v.x <= lower.x { lower.x = v.x }
                    if upper.x <= v.x { upper.x = v.x }
                    if v.y <= lower.y { lower.y = v.y }
                    if upper.y <= v.y { upper.y = v.y }
                }
                let r = polygon.radius;
                Aabb { lower: Vec2::new(lower.x - r, lower.y - r), upper: Vec2::new(upper.x + r, upper.y + r) }
            }
        }
    }

    fn compute_mass(&self, density: f32) -> MassData {
        match self {
            // ponytail: Box2D 2.3's b2CircleShape::ComputeMass, not yet compared against the DLL.
            Shape::Circle(c) => {
                let area = std::f32::consts::PI * c.radius * c.radius;
                let mass = density * area;
                MassData { mass, center: c.p, inertia: mass * (0.5 * c.radius * c.radius + (c.p.x * c.p.x + c.p.y * c.p.y)), area }
            }
            Shape::Polygon(p) => p.compute_mass(density),
        }
    }

    fn proxy(&self) -> DistanceProxy<'_> {
        match self {
            Shape::Circle(c) => DistanceProxy { vertices: std::slice::from_ref(&c.p), radius: c.radius },
            Shape::Polygon(p) => DistanceProxy { vertices: &p.vertices[..p.count], radius: p.radius },
        }
    }
}

/// The Rigidbody2D fields b2Body::ResetMassData (180bb9e80) reads through m_userData.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RigidbodyMass {
    pub use_auto_mass: bool,
    pub mass: f32,
    pub inertia: Option<f32>,
    pub center_of_mass: Option<Vec2>,
}

impl Default for RigidbodyMass {
    fn default() -> Self {
        Self { use_auto_mass: false, mass: 1.0, inertia: None, center_of_mass: None }
    }
}

/// Unity's b2Fixture user data: the Collider2D (instance id) and its GameObject.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ColliderRef {
    pub id: i32,
    pub game_object: i32,
}

/// Unity's b2Body user data: the Rigidbody2D (instance id) and its GameObject.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RigidbodyRef {
    pub id: i32,
    pub game_object: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct BodyDef {
    pub kind: BodyType,
    pub position: Vec2,
    pub angle: f32,
    pub linear_velocity: Vec2,
    pub angular_velocity: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub allow_sleep: bool,
    pub awake: bool,
    pub fixed_rotation: bool,
    pub bullet: bool,
    pub active: bool,
    pub gravity_scale: f32,
    pub rigidbody: RigidbodyMass,
    /// None for a body without a Rigidbody2D (the static ground body).
    pub user: Option<RigidbodyRef>,
}

impl Default for BodyDef {
    fn default() -> Self {
        Self {
            kind: BodyType::Static,
            position: Vec2::ZERO,
            angle: 0.0,
            linear_velocity: Vec2::ZERO,
            angular_velocity: 0.0,
            linear_damping: 0.0,
            angular_damping: 0.0,
            allow_sleep: true,
            awake: true,
            fixed_rotation: false,
            bullet: false,
            active: true,
            gravity_scale: 1.0,
            rigidbody: RigidbodyMass::default(),
            user: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Body {
    pub kind: BodyType,
    pub flags: u16,
    pub(crate) island_index: i32,
    pub xf: Transform,
    pub sweep: Sweep,
    pub linear_velocity: Vec2,
    pub angular_velocity: f32,
    pub force: Vec2,
    pub torque: f32,
    /// Fixture list, head last.
    pub fixtures: Vec<FixtureId>,
    /// Contact edges (other body, contact), head last.
    pub(crate) contacts: Vec<(BodyId, ContactId)>,
    /// Joint edges (other body, joint), head last.
    pub(crate) joints: Vec<(BodyId, JointId)>,
    pub mass: f32,
    pub inv_mass: f32,
    pub axis: Vec2,
    pub i: f32,
    pub inv_i: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub gravity_scale: f32,
    pub sleep_time: f32,
    pub rigidbody: RigidbodyMass,
    pub user: Option<RigidbodyRef>,
}

impl Body {
    /// b2Body::SynchronizeTransform: xf from the sweep's current centre and angle.
    fn synchronize_transform(&mut self) {
        let q = Rot::new(self.sweep.a);
        self.xf = Transform { p: self.sweep.c - q.apply(self.sweep.local_center), q };
    }

    /// b2Body::Advance (180bb87b0): move the body to fraction `alpha` of the step and stop there.
    fn advance(&mut self, alpha: f32) {
        self.sweep.advance(alpha);
        self.sweep.c = self.sweep.c0;
        self.sweep.a = self.sweep.a0;
        self.synchronize_transform();
    }

    /// b2Body::ApplyForce (Rigidbody2D.AddForceAtPosition): dynamic bodies only, wakes the body.
    pub fn apply_force(&mut self, force: Vec2, point: Vec2) {
        if self.kind != BodyType::Dynamic {
            return;
        }
        self.wake_up();
        self.force = self.force + force;
        let r = point - self.sweep.c;
        self.torque += r.x * force.y - r.y * force.x;
    }

    /// b2Body::ApplyTorque (Rigidbody2D.AddTorque).
    pub fn apply_torque(&mut self, torque: f32) {
        if self.kind != BodyType::Dynamic {
            return;
        }
        self.wake_up();
        self.torque += torque;
    }

    /// b2Body::GetLinearVelocityFromWorldPoint.
    pub fn velocity_at(&self, point: Vec2) -> Vec2 {
        let r = point - self.sweep.c;
        self.linear_velocity + Vec2::new(-self.angular_velocity * r.y, self.angular_velocity * r.x)
    }

    /// b2Body::SetAwake(true).
    fn wake_up(&mut self) {
        if self.flags & BODY_AWAKE == 0 {
            self.flags |= BODY_AWAKE;
            self.sleep_time = 0.0;
        }
    }

    fn active_and_moving(&self) -> bool {
        self.flags & BODY_AWAKE != 0 && self.kind != BodyType::Static
    }

    /// The wake Contact::Update and ContactManager::Destroy inline: static bodies are put to sleep instead.
    fn wake(&mut self) {
        self.sleep_time = 0.0;
        if self.kind == BodyType::Static {
            self.flags &= !BODY_AWAKE;
            self.linear_velocity = Vec2::ZERO;
            self.angular_velocity = 0.0;
            self.force = Vec2::ZERO;
            self.torque = 0.0;
        } else {
            self.flags |= BODY_AWAKE;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Filter {
    pub category_bits: u16,
    pub mask_bits: u16,
    pub group_index: i16,
}

impl Default for Filter {
    fn default() -> Self {
        Self { category_bits: 1, mask_bits: 0xffff, group_index: 0 }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FixtureDef {
    pub shape: Shape,
    pub friction: f32,
    pub restitution: f32,
    pub density: f32,
    pub is_sensor: bool,
    pub filter: Filter,
    pub collider: ColliderRef,
}

#[derive(Clone, Debug)]
pub struct Fixture {
    pub body: BodyId,
    pub shape: Shape,
    pub density: f32,
    pub friction: f32,
    pub restitution: f32,
    pub is_sensor: bool,
    pub filter: Filter,
    /// (aabb, proxy id) per shape child; circles and polygons have one.
    proxies: Vec<(Aabb, i32)>,
    pub mass_data: MassData,
    pub collider: ColliderRef,
}

#[derive(Clone, Debug)]
pub struct Contact {
    pub flags: u32,
    pub fixture_a: FixtureId,
    pub fixture_b: FixtureId,
    pub child_a: usize,
    pub child_b: usize,
    pub(crate) island_index_a: usize,
    pub(crate) island_index_b: usize,
    pub manifold: Manifold,
    pub radius_a: f32,
    pub radius_b: f32,
    pub toi_count: i32,
    pub toi: f32,
    pub friction: f32,
    pub restitution: f32,
    pub tangent_speed: f32,
    pub(crate) manager_index: usize,
    /// Unity: the contact's manifold index in its Collision2D record (-1 for none) and the record.
    pub user_index: i32,
    pub user_data: Option<CollisionId>,
}

pub type JointId = usize;

/// b2Joint: Unity's HingeJoint2D is the only joint the game's bodies use.
#[derive(Clone, Debug)]
pub struct Joint {
    pub revolute: RevoluteJoint,
    pub body_a: BodyId,
    pub body_b: BodyId,
    pub collide_connected: bool,
    island_flag: bool,
}

pub struct World {
    pub bodies: Vec<Option<Body>>,
    pub fixtures: Vec<Option<Fixture>>,
    pub contacts: Vec<Option<Contact>>,
    free_contacts: Vec<ContactId>,
    /// Body list, head last.
    pub(crate) body_list: Vec<BodyId>,
    non_static_bodies: Vec<BodyId>,
    static_bodies: Vec<BodyId>,
    /// Contact list, head last.
    pub(crate) contact_list: Vec<ContactId>,
    pub joints: Vec<Option<Joint>>,
    /// Joint list, head last.
    joint_list: Vec<JointId>,
    contacts_non_toi: Vec<ContactId>,
    contacts_toi: Vec<ContactId>,
    pub broad_phase: BroadPhase,
    flags: u32,
    pub gravity: Vec2,
    pub allow_sleep: bool,
    pub warm_starting: bool,
    pub continuous_physics: bool,
    pub sub_stepping: bool,
    step_complete: bool,
    inv_dt0: f32,
    /// Physics2D.IgnoreCollision pairs (lower fixture id first), checked where Unity's contact filter runs.
    pub ignored: std::collections::HashSet<(FixtureId, FixtureId)>,
    /// The contact listener's PhysicsContacts2D.
    pub contacts2d: Contacts2D,
}

impl Default for World {
    fn default() -> Self {
        Self::new(Vec2::new(0.0, -9.81))
    }
}

impl World {
    pub fn new(gravity: Vec2) -> Self {
        Self {
            bodies: Vec::new(),
            fixtures: Vec::new(),
            contacts: Vec::new(),
            free_contacts: Vec::new(),
            body_list: Vec::new(),
            non_static_bodies: Vec::new(),
            static_bodies: Vec::new(),
            contact_list: Vec::new(),
            joints: Vec::new(),
            joint_list: Vec::new(),
            contacts_non_toi: Vec::new(),
            contacts_toi: Vec::new(),
            broad_phase: BroadPhase::default(),
            flags: WORLD_CLEAR_FORCES,
            gravity,
            allow_sleep: true,
            warm_starting: true,
            continuous_physics: true,
            sub_stepping: false,
            step_complete: true,
            inv_dt0: 0.0,
            ignored: Default::default(),
            contacts2d: Contacts2D::default(),
        }
    }

    pub fn body(&self, id: BodyId) -> &Body {
        self.bodies[id].as_ref().unwrap()
    }

    pub fn body_mut(&mut self, id: BodyId) -> &mut Body {
        self.bodies[id].as_mut().unwrap()
    }

    pub(crate) fn fixture(&self, id: FixtureId) -> &Fixture {
        self.fixtures[id].as_ref().unwrap()
    }

    pub(crate) fn contact(&self, id: ContactId) -> &Contact {
        self.contacts[id].as_ref().unwrap()
    }

    pub(crate) fn contact_mut(&mut self, id: ContactId) -> &mut Contact {
        self.contacts[id].as_mut().unwrap()
    }

    /// b2World::CreateBody (180bb8ac0).
    pub fn create_body(&mut self, def: &BodyDef) -> BodyId {
        let mut flags = if def.active { BODY_ACTIVE } else { 0 };
        if def.bullet { flags |= BODY_BULLET }
        if def.fixed_rotation { flags |= BODY_FIXED_ROTATION }
        if def.allow_sleep { flags |= BODY_AUTO_SLEEP }
        flags |= if def.awake { BODY_AWAKE } else { BODY_STARTED_ASLEEP };
        let mass = if def.kind == BodyType::Dynamic { 1.0 } else { 0.0 };
        let body = Body {
            kind: def.kind,
            flags,
            island_index: -1,
            xf: Transform { p: def.position, q: Rot::new(def.angle) },
            sweep: Sweep { local_center: Vec2::ZERO, c0: def.position, c: def.position, a0: def.angle, a: def.angle, alpha0: 0.0 },
            linear_velocity: def.linear_velocity,
            angular_velocity: def.angular_velocity,
            force: Vec2::ZERO,
            torque: 0.0,
            fixtures: Vec::new(),
            contacts: Vec::new(),
            joints: Vec::new(),
            mass,
            inv_mass: mass,
            axis: Vec2::new(1.0, 1.0),
            i: 0.0,
            inv_i: 0.0,
            linear_damping: def.linear_damping,
            angular_damping: def.angular_damping,
            gravity_scale: def.gravity_scale,
            sleep_time: 0.0,
            rigidbody: def.rigidbody,
            user: def.user,
        };
        let id = self.bodies.len();
        self.bodies.push(Some(body));
        self.body_list.push(id);
        if def.kind == BodyType::Static { self.static_bodies.push(id) } else { self.non_static_bodies.push(id) }
        id
    }

    /// b2World::CreateJoint (180bb8f10).
    pub fn create_joint(&mut self, def: RevoluteDef, body_a: BodyId, body_b: BodyId, collide_connected: bool) -> JointId {
        let id = self.joints.len();
        self.joints.push(Some(Joint { revolute: RevoluteJoint::new(def), body_a, body_b, collide_connected, island_flag: false }));
        self.joint_list.push(id);
        self.body_mut(body_a).joints.push((body_b, id));
        self.body_mut(body_b).joints.push((body_a, id));
        if !collide_connected {
            self.flag_contacts_between(body_b, body_a);
        }
        id
    }

    /// b2World::DestroyBody (180bb9360): joints, contacts, then fixture proxies; Unity's body arrays swap-remove.
    pub fn destroy_body(&mut self, id: BodyId) {
        for (_, j) in self.body(id).joints.clone().into_iter().rev() {
            self.destroy_joint(j);
        }
        for (_, c) in self.body(id).contacts.clone().into_iter().rev() {
            self.destroy_contact(c);
        }
        let body = self.bodies[id].take().unwrap();
        for &f in body.fixtures.iter().rev() {
            for &(_, proxy) in &self.fixtures[f].as_ref().unwrap().proxies {
                self.broad_phase.destroy_proxy(proxy);
            }
            let collider = self.fixtures[f].take().unwrap().collider;
            *self.contacts2d.collider_shapes.get_mut(&collider.id).unwrap() -= 1;
        }
        self.body_list.retain(|&b| b != id);
        let list = if body.kind == BodyType::Static { &mut self.static_bodies } else { &mut self.non_static_bodies };
        let at = list.iter().position(|&b| b == id).unwrap();
        list.swap_remove(at);
    }

    /// b2Body::SetTransform: teleport, keeping velocities. ponytail: Unity's Rigidbody2D.position setter is not compared.
    pub fn set_transform(&mut self, id: BodyId, position: Vec2, angle: f32) {
        let b = self.body_mut(id);
        b.xf = Transform { p: position, q: Rot::new(angle) };
        b.sweep.c = b.xf.apply(b.sweep.local_center);
        b.sweep.c0 = b.sweep.c;
        b.sweep.a = angle;
        b.sweep.a0 = angle;
        self.synchronize_fixtures(id);
    }

    /// b2World::DestroyJoint (180bb9680).
    pub fn destroy_joint(&mut self, id: JointId) {
        let j = self.joints[id].take().unwrap();
        self.joint_list.retain(|&k| k != id);
        for b in [j.body_a, j.body_b] {
            let body = self.body_mut(b);
            body.wake();
            body.joints.retain(|&(_, k)| k != id);
        }
        if !j.collide_connected {
            self.flag_contacts_between(j.body_b, j.body_a);
        }
    }

    /// Re-filter every contact on `b` with `other`, so the next Collide re-checks ShouldCollide.
    fn flag_contacts_between(&mut self, b: BodyId, other: BodyId) {
        let edges: Vec<(BodyId, ContactId)> = self.body(b).contacts.clone();
        for (o, c) in edges {
            if o == other {
                self.contact_mut(c).flags |= CONTACT_FILTER;
            }
        }
    }

    /// b2Body::ShouldCollide (180bba730), called on body B with body A.
    fn bodies_should_collide(&self, b: BodyId, a: BodyId) -> bool {
        let (body, other) = (self.body(b), self.body(a));
        if body.kind == BodyType::Static && other.kind == BodyType::Static {
            return false;
        }
        !body.joints.iter().rev().any(|&(o, j)| o == a && !self.joints[j].as_ref().unwrap().collide_connected)
    }

    /// b2Body::CreateFixture (180bb8d60) with Fixture::CreateProxies (180bb9040).
    pub fn create_fixture(&mut self, body_id: BodyId, def: &FixtureDef, reset_mass: bool) -> FixtureId {
        let id = self.fixtures.len();
        let body = self.body(body_id);
        let proxies = if body.flags & BODY_ACTIVE != 0 {
            let aabb = def.shape.compute_aabb(&body.xf);
            vec![(aabb, self.broad_phase.create_proxy(&aabb, id))]
        } else {
            Vec::new()
        };
        self.fixtures.push(Some(Fixture {
            body: body_id,
            shape: def.shape,
            density: def.density,
            friction: def.friction,
            restitution: def.restitution,
            is_sensor: def.is_sensor,
            filter: def.filter,
            proxies,
            mass_data: MassData::default(),
            collider: def.collider,
        }));
        *self.contacts2d.collider_shapes.entry(def.collider.id).or_default() += 1;
        self.body_mut(body_id).fixtures.push(id);
        if reset_mass && 0.0 < def.density {
            self.reset_mass_data(body_id);
        }
        self.flags |= WORLD_NEW_FIXTURE;
        id
    }

    /// b2Body::ResetMassData (180bb9e80): Unity takes the mass, inertia and centre from the Rigidbody2D.
    pub fn reset_mass_data(&mut self, body_id: BodyId) {
        let masses: Vec<(FixtureId, MassData)> = self
            .body(body_id)
            .fixtures
            .iter()
            .rev()
            .map(|&f| (f, self.fixture(f)))
            .filter(|(_, f)| f.density != 0.0 && !f.is_sensor)
            .map(|(id, f)| (id, f.shape.compute_mass(f.density)))
            .collect();
        for &(f, md) in &masses {
            self.fixtures[f].as_mut().unwrap().mass_data = md;
        }
        let b = self.body_mut(body_id);
        b.mass = 0.0;
        b.inv_mass = 0.0;
        b.i = 0.0;
        b.inv_i = 0.0;
        b.sweep.local_center = Vec2::ZERO;
        if b.kind < BodyType::Dynamic {
            b.sweep.c0 = b.xf.p;
            b.sweep.c = b.xf.p;
            b.sweep.a0 = b.sweep.a;
            return;
        }
        let mut center = Vec2::ZERO;
        for (_, md) in &masses {
            b.mass = md.mass + b.mass;
            center.x = center.x + md.center.x * md.mass;
            center.y = center.y + md.center.y * md.mass;
            b.i = md.inertia + b.i;
        }
        if b.mass <= 0.0 {
            b.mass = 1.0;
        } else {
            let inv = 1.0 / b.mass;
            center = Vec2::new(center.x * inv, center.y * inv);
        }
        if b.rigidbody.use_auto_mass {
            let lo = f32::from_bits(0x38d1_b717);
            let hi = f32::from_bits(0x4974_2400);
            b.mass = if lo <= b.mass && b.mass <= hi { b.mass } else if lo <= b.mass { hi } else { lo };
            b.rigidbody.mass = b.mass;
        }
        if b.flags & BODY_FIXED_ROTATION == 0 {
            if b.i <= 0.0 {
                b.i = b.rigidbody.inertia.unwrap_or(1.0);
            } else {
                let computed = b.mass;
                b.i = b.i - (center.x * center.x + center.y * center.y) * computed;
                if b.i < f32::EPSILON {
                    b.i = f32::EPSILON;
                }
                b.i = match b.rigidbody.inertia {
                    Some(i) => i,
                    None => (b.rigidbody.mass / computed) * b.i,
                };
            }
            b.mass = b.rigidbody.mass;
            b.inv_i = 1.0 / b.i;
        } else {
            b.mass = b.rigidbody.mass;
            b.i = 0.0;
            b.inv_i = 0.0;
        }
        b.inv_mass = 1.0 / b.mass;
        if let Some(c) = b.rigidbody.center_of_mass {
            center = c;
        }
        let old = b.sweep.c;
        b.sweep.local_center = center;
        let c = b.xf.apply(center);
        b.sweep.c0 = c;
        b.sweep.c = c;
        let w = b.angular_velocity;
        b.linear_velocity.y = (c.x - old.x) * w + b.linear_velocity.y;
        b.linear_velocity.x = -((c.y - old.y) * w) + b.linear_velocity.x;
    }

    /// b2World::Step (180bbc560) on the single-threaded path, as PhysicsScene2D::Simulate (180c24660) inlines it
    /// after counting the simulation step.
    pub fn step(&mut self, dt: f32, velocity_iterations: i32, position_iterations: i32) {
        self.contacts2d.simulation_id += 1;
        if self.flags & WORLD_NEW_FIXTURE != 0 {
            self.find_new_contacts();
            self.flags &= !WORLD_NEW_FIXTURE;
        }
        self.flags |= WORLD_LOCKED;
        let inv_dt = if dt <= 0.0 { 0.0 } else { 1.0 / dt };
        let step = TimeStep { dt, inv_dt, dt_ratio: dt * self.inv_dt0, velocity_iterations, position_iterations, warm_starting: self.warm_starting };
        self.collide(false);
        self.collide(true);
        if self.step_complete && 0.0 < dt {
            self.solve(&step);
        }
        if self.continuous_physics && 0.0 < dt {
            self.solve_toi(&step);
        }
        if 0.0 < dt {
            self.inv_dt0 = inv_dt;
        }
        if self.flags & WORLD_CLEAR_FORCES != 0 {
            for &id in self.body_list.iter().rev() {
                let b = self.bodies[id].as_mut().unwrap();
                b.force = Vec2::ZERO;
                b.torque = 0.0;
            }
        }
        // Unity keeps sensor contacts of moving bodies current.
        let sensors: Vec<ContactId> = self
            .contact_list
            .iter()
            .rev()
            .copied()
            .filter(|&c| {
                let c = self.contact(c);
                let (fa, fb) = (self.fixture(c.fixture_a), self.fixture(c.fixture_b));
                c.flags & CONTACT_ENABLED != 0
                    && (self.body(fa.body).active_and_moving() || self.body(fb.body).active_and_moving())
                    && (fa.is_sensor || fb.is_sensor)
            })
            .collect();
        for c in sensors {
            self.update_contact(c);
        }
        self.flags &= !WORLD_LOCKED;
    }

    /// UpdatePairs (180bb7af0) with b2ContactManager::AddPair inlined.
    fn find_new_contacts(&mut self) {
        let mut pairs = Vec::new();
        self.broad_phase.update_pairs(|a, b| pairs.push((a, b)));
        for (fa, fb) in pairs {
            self.add_pair(fa, fb);
        }
    }

    fn add_pair(&mut self, fa: FixtureId, fb: FixtureId) {
        let (body_a, body_b) = (self.fixture(fa).body, self.fixture(fb).body);
        if body_a == body_b {
            return;
        }
        for &(other, c) in self.body(body_b).contacts.iter().rev() {
            if other == body_a {
                let c = self.contact(c);
                if (c.fixture_a == fa && c.fixture_b == fb) || (c.fixture_a == fb && c.fixture_b == fa) {
                    return;
                }
            }
        }
        if !self.bodies_should_collide(body_b, body_a) {
            return;
        }
        if !self.fixtures_should_collide(fa, fb) {
            return;
        }
        let c = self.create_contact(fa, fb);
        self.on_contact_create(c);
    }

    /// b2Contact::Create (180bbcce0) and the b2Contact constructor (180bbc9f0); polygon-circle puts the polygon first.
    fn create_contact(&mut self, fa: FixtureId, fb: FixtureId) -> ContactId {
        let (fa, fb) = if self.fixture(fa).shape.type_index() < self.fixture(fb).shape.type_index() { (fb, fa) } else { (fa, fb) };
        let (a, b) = (self.fixture(fa), self.fixture(fb));
        let contact = Contact {
            flags: CONTACT_ENABLED,
            fixture_a: fa,
            fixture_b: fb,
            child_a: 0,
            child_b: 0,
            island_index_a: usize::MAX,
            island_index_b: usize::MAX,
            manifold: Manifold::default(),
            radius_a: a.shape.radius(),
            radius_b: b.shape.radius(),
            toi_count: 0,
            toi: 0.0,
            friction: (a.friction * b.friction).sqrt(),
            restitution: if a.restitution <= b.restitution { b.restitution } else { a.restitution },
            tangent_speed: 0.0,
            manager_index: 0,
            user_index: -1,
            user_data: None,
        };
        match self.free_contacts.pop() {
            Some(id) => {
                self.contacts[id] = Some(contact);
                id
            }
            None => {
                self.contacts.push(Some(contact));
                self.contacts.len() - 1
            }
        }
    }

    /// A contact restored from a game dump: created like a new pair, then the caller sets its state.
    /// Restoring the native list tail first rebuilds the list and every body's edge list in native order.
    pub(crate) fn restore_contact(&mut self, fa: FixtureId, fb: FixtureId) -> ContactId {
        let id = self.create_contact(fa, fb);
        self.on_contact_create(id);
        id
    }

    /// After a restore (contact flags and manager indices set): Unity's contact arrays by the CONTINUOUS flag in
    /// manager-index order, and the dumped move buffer, e_newFixture and m_inv_dt0.
    pub(crate) fn finish_restore(&mut self, inv_dt0: f32, new_fixture: bool, moves: Vec<i32>) -> Result<(), String> {
        let mut all: Vec<ContactId> = self.contact_list.clone();
        all.sort_by_key(|&c| self.contact(c).manager_index);
        let (toi, non_toi): (Vec<ContactId>, Vec<ContactId>) = all.into_iter().partition(|&c| self.contact(c).flags & CONTACT_CONTINUOUS != 0);
        for list in [&toi, &non_toi] {
            if list.iter().enumerate().any(|(i, &c)| self.contact(c).manager_index != i) {
                return Err("contact manager indices have gaps".into());
            }
        }
        (self.contacts_toi, self.contacts_non_toi) = (toi, non_toi);
        self.broad_phase.move_buffer = moves;
        self.flags &= !WORLD_NEW_FIXTURE;
        if new_fixture {
            self.flags |= WORLD_NEW_FIXTURE;
        }
        self.inv_dt0 = inv_dt0;
        Ok(())
    }

    /// A fresh broadphase holding the dumped proxies (fixture, AABB, fat AABB), given in the game's proxy-id order:
    /// the fresh ids then sort the same, which is all UpdatePairs sees of them.
    pub(crate) fn restore_proxies(&mut self, proxies: &[(FixtureId, Aabb, Aabb)]) -> Vec<i32> {
        self.broad_phase = BroadPhase::default();
        for f in self.fixtures.iter_mut().flatten() {
            f.proxies.clear();
        }
        let mut ids = Vec::new();
        for &(f, aabb, fat) in proxies {
            let id = self.broad_phase.restore_proxy(&fat, f);
            self.fixtures[f].as_mut().unwrap().proxies.push((aabb, id));
            ids.push(id);
        }
        ids
    }

    /// Unity's b2ContactFilter: the fixture filters, then Physics2D.IgnoreCollision.
    // ponytail: Unity's layer collision matrix is not modelled yet (every shipped object is on a colliding layer).
    fn fixtures_should_collide(&self, fa: FixtureId, fb: FixtureId) -> bool {
        should_collide(&self.fixture(fa).filter, &self.fixture(fb).filter) && !self.ignored.contains(&(fa.min(fb), fa.max(fb)))
    }

    /// b2ContactManager::OnContactCreate (180bb98f0).
    fn on_contact_create(&mut self, id: ContactId) {
        let c = self.contact(id);
        let (fa, fb) = (self.fixture(c.fixture_a), self.fixture(c.fixture_b));
        let (sensor_a, sensor_b, body_a, body_b) = (fa.is_sensor, fb.is_sensor, fa.body, fb.body);
        let bullet = (self.body(body_a).flags | self.body(body_b).flags) & BODY_BULLET != 0;
        let continuous = !sensor_a && !sensor_b && bullet;
        let list = if continuous { &mut self.contacts_toi } else { &mut self.contacts_non_toi };
        let index = list.len();
        list.push(id);
        let c = self.contact_mut(id);
        if continuous {
            c.flags |= CONTACT_CONTINUOUS;
        }
        c.manager_index = index;
        self.contact_list.push(id);
        self.body_mut(body_a).contacts.push((body_b, id));
        self.body_mut(body_b).contacts.push((body_a, id));
        let static_a = self.body(body_a).kind == BodyType::Static;
        let static_b = self.body(body_b).kind == BodyType::Static;
        let wake = (!sensor_a && !sensor_b) || (sensor_a && static_a) || (sensor_b && static_b);
        if wake {
            for b in [body_a, body_b] {
                let b = self.body_mut(b);
                if b.flags & BODY_STARTED_ASLEEP == 0 {
                    b.wake();
                }
            }
        }
    }

    /// b2ContactManager::Destroy (180bb90e0).
    fn destroy_contact(&mut self, id: ContactId) {
        if self.contact(id).flags & CONTACT_TOUCHING != 0 {
            self.end_contact(id);
        }
        let c = self.contacts[id].take().unwrap();
        self.free_contacts.push(id);
        let pos = self.contact_list.iter().position(|&x| x == id).unwrap();
        self.contact_list.remove(pos);
        let list = if c.flags & CONTACT_CONTINUOUS != 0 { &mut self.contacts_toi } else { &mut self.contacts_non_toi };
        let last = *list.last().unwrap();
        list.swap_remove(c.manager_index);
        if last != id {
            self.contacts[last].as_mut().unwrap().manager_index = c.manager_index;
        }
        let (fa, fb) = (self.fixture(c.fixture_a), self.fixture(c.fixture_b));
        let (body_a, body_b, sensors) = (fa.body, fb.body, fa.is_sensor || fb.is_sensor);
        for b in [body_a, body_b] {
            let edges = &mut self.body_mut(b).contacts;
            let pos = edges.iter().position(|&(_, x)| x == id).unwrap();
            edges.remove(pos);
        }
        if 0 < c.manifold.point_count && !sensors {
            self.body_mut(body_a).wake();
            self.body_mut(body_b).wake();
        }
    }

    /// b2ContactManager::Collide (180bb88c0) over one of Unity's contact arrays.
    fn collide(&mut self, toi: bool) {
        let mut i = 0;
        while i < if toi { self.contacts_toi.len() } else { self.contacts_non_toi.len() } {
            let id = if toi { self.contacts_toi[i] } else { self.contacts_non_toi[i] };
            let c = self.contact(id);
            let (fa, fb) = (self.fixture(c.fixture_a), self.fixture(c.fixture_b));
            let (body_a, body_b) = (fa.body, fb.body);
            if c.flags & CONTACT_FILTER != 0 {
                if !self.bodies_should_collide(body_b, body_a) || !self.fixtures_should_collide(c.fixture_a, c.fixture_b) {
                    self.destroy_contact(id);
                    continue;
                }
                self.contact_mut(id).flags &= !CONTACT_FILTER;
            }
            if !self.body(body_a).active_and_moving() && !self.body(body_b).active_and_moving() {
                i += 1;
                continue;
            }
            let c = self.contact(id);
            let a = &self.broad_phase.tree.nodes[self.fixture(c.fixture_a).proxies[c.child_a].1 as usize].aabb;
            let b = &self.broad_phase.tree.nodes[self.fixture(c.fixture_b).proxies[c.child_b].1 as usize].aabb;
            let apart = 0.0 < b.lower.x - a.upper.x || 0.0 < b.lower.y - a.upper.y || 0.0 < a.lower.x - b.upper.x || 0.0 < a.lower.y - b.upper.y;
            if apart {
                self.destroy_contact(id);
                continue;
            }
            self.update_contact(id);
            i += 1;
        }
    }

    /// b2Contact::Update (180bbd6e0) with Unity's WorldContactListener2D.
    fn update_contact(&mut self, id: ContactId) {
        let c = self.contact(id);
        let old = c.manifold;
        let was_touching = c.flags & CONTACT_TOUCHING != 0;
        let (fa, fb) = (self.fixture(c.fixture_a), self.fixture(c.fixture_b));
        let (body_a, body_b) = (fa.body, fb.body);
        let (xf_a, xf_b) = (self.body(body_a).xf, self.body(body_b).xf);
        let sensor = fa.is_sensor || fb.is_sensor;
        let (shape_a, shape_b) = (fa.shape, fb.shape);
        let c = self.contact_mut(id);
        if c.flags & CONTACT_KEEP_DISABLED == 0 {
            c.flags |= CONTACT_ENABLED;
        }
        let touching = if sensor {
            c.manifold.point_count = 0;
            test_overlap(&shape_a, &xf_a, &shape_b, &xf_b)
        } else {
            c.radius_a = shape_a.radius();
            c.radius_b = shape_b.radius();
            evaluate(&mut c.manifold, &shape_a, &xf_a, &shape_b, &xf_b);
            for point in &mut c.manifold.points[..c.manifold.point_count] {
                point.normal_impulse = 0.0;
                point.tangent_impulse = 0.0;
                if let Some(o) = old.points[..old.point_count].iter().find(|o| o.id.key() == point.id.key()) {
                    point.normal_impulse = o.normal_impulse;
                    point.tangent_impulse = o.tangent_impulse;
                }
            }
            let touching = 0 < c.manifold.point_count;
            if touching != was_touching {
                self.body_mut(body_a).wake();
                self.body_mut(body_b).wake();
            }
            touching
        };
        let c = self.contact_mut(id);
        if touching { c.flags |= CONTACT_TOUCHING } else { c.flags &= !CONTACT_TOUCHING }
        if !was_touching && touching {
            self.begin_contact(id);
        }
        if was_touching && !touching {
            self.end_contact(id);
        }
        if !sensor && touching {
            self.pre_solve(id);
        }
    }

    /// b2World::Solve (180bbae90): Unity seeds islands from m_nonStaticBodies and skips kinematic-only contacts.
    fn solve(&mut self, step: &TimeStep) {
        for &id in &self.body_list {
            self.bodies[id].as_mut().unwrap().flags &= 0xfefe;
        }
        for &id in &self.contact_list {
            self.contacts[id].as_mut().unwrap().flags &= !CONTACT_ISLAND;
        }
        for &id in &self.joint_list {
            self.joints[id].as_mut().unwrap().island_flag = false;
        }
        for seed_index in 0..self.non_static_bodies.len() {
            let seed = self.non_static_bodies[seed_index];
            let flags = self.body(seed).flags;
            if flags & BODY_ISLAND != 0 || flags & (BODY_AWAKE | BODY_ACTIVE) != BODY_AWAKE | BODY_ACTIVE {
                continue;
            }
            let mut island_bodies: Vec<BodyId> = Vec::new();
            let mut island_contacts: Vec<ContactId> = Vec::new();
            let mut island_joints: Vec<JointId> = Vec::new();
            let mut stack = vec![seed];
            self.body_mut(seed).flags |= BODY_ISLAND;
            while let Some(b) = stack.pop() {
                self.body_mut(b).island_index = island_bodies.len() as i32;
                island_bodies.push(b);
                if self.body(b).kind == BodyType::Static {
                    continue;
                }
                self.body_mut(b).flags |= BODY_AWAKE;
                let edges: Vec<(BodyId, ContactId)> = self.body(b).contacts.iter().rev().copied().collect();
                for (other, cid) in edges {
                    let c = self.contact(cid);
                    let (fa, fb) = (self.fixture(c.fixture_a), self.fixture(c.fixture_b));
                    let joins = c.flags & CONTACT_ISLAND == 0
                        && c.flags & (CONTACT_TOUCHING | CONTACT_ENABLED) == CONTACT_TOUCHING | CONTACT_ENABLED
                        && !fa.is_sensor
                        && !fb.is_sensor
                        && (self.body(fa.body).kind == BodyType::Dynamic || self.body(fb.body).kind == BodyType::Dynamic);
                    if !joins {
                        continue;
                    }
                    island_contacts.push(cid);
                    self.contact_mut(cid).flags |= CONTACT_ISLAND;
                    if self.body(other).flags & BODY_ISLAND == 0 {
                        stack.push(other);
                        self.body_mut(other).flags |= BODY_ISLAND;
                    }
                }
                let edges: Vec<(BodyId, JointId)> = self.body(b).joints.iter().rev().copied().collect();
                for (other, jid) in edges {
                    if self.joints[jid].as_ref().unwrap().island_flag || self.body(other).flags & BODY_ACTIVE == 0 {
                        continue;
                    }
                    island_joints.push(jid);
                    self.joints[jid].as_mut().unwrap().island_flag = true;
                    if self.body(other).flags & BODY_ISLAND == 0 {
                        stack.push(other);
                        self.body_mut(other).flags |= BODY_ISLAND;
                    }
                }
            }
            for &cid in &island_contacts {
                let c = self.contact(cid);
                let a = self.body(self.fixture(c.fixture_a).body).island_index as usize;
                let b = self.body(self.fixture(c.fixture_b).body).island_index as usize;
                let c = self.contact_mut(cid);
                c.island_index_a = a;
                c.island_index_b = b;
            }
            for &jid in &island_joints {
                let j = self.joints[jid].as_ref().unwrap();
                let (a, b) = (self.body(j.body_a), self.body(j.body_b));
                let ends = [(a.island_index as usize, a.axis), (b.island_index as usize, b.axis)];
                let r = &mut self.joints[jid].as_mut().unwrap().revolute;
                (r.a.index, r.a.axis) = ends[0];
                (r.b.index, r.b.axis) = ends[1];
            }
            for &b in &island_bodies {
                self.body_mut(b).island_index = -1;
            }
            self.solve_island(&island_bodies, &island_contacts, &island_joints, step);
            for &b in &island_bodies {
                let b = self.body_mut(b);
                if b.kind == BodyType::Static {
                    b.flags &= !BODY_ISLAND;
                }
            }
        }
        for i in 0..self.non_static_bodies.len() {
            let id = self.non_static_bodies[i];
            if self.body(id).flags & BODY_ISLAND != 0 {
                self.synchronize_fixtures(id);
            }
        }
        self.find_new_contacts();
    }

    /// b2Island::Solve (180bba770).
    pub(crate) fn solve_island(&mut self, island_bodies: &[BodyId], island_contacts: &[ContactId], island_joints: &[JointId], step: &TimeStep) {
        let h = step.dt;
        let gravity = self.gravity;
        let mut positions = Vec::with_capacity(island_bodies.len());
        let mut velocities = Vec::with_capacity(island_bodies.len());
        for &id in island_bodies {
            let b = self.body_mut(id);
            b.sweep.c0 = b.sweep.c;
            b.sweep.a0 = b.sweep.a;
            let (mut v, mut w) = (b.linear_velocity, b.angular_velocity);
            if b.kind == BodyType::Dynamic {
                let damping = 1.0 / (h * b.linear_damping + 1.0);
                v.x = (v.x + (b.gravity_scale * gravity.x + b.inv_mass * b.force.x) * h) * damping;
                v.y = (v.y + (b.gravity_scale * gravity.y + b.inv_mass * b.force.y) * h) * damping;
                w = (h * b.inv_i * b.torque + w) * (1.0 / (h * b.angular_damping + 1.0));
            }
            positions.push(Position { c: b.sweep.c, a: b.sweep.a });
            velocities.push(Velocity { v: Vec2::new(v.x * b.axis.x, v.y * b.axis.y), w });
        }
        let solver_contacts = self.solver_contacts(island_contacts);
        let mut solver = ContactSolver::new(*step, &solver_contacts);
        solver.initialize_velocity_constraints(&positions, &velocities);
        if step.warm_starting {
            solver.warm_start(&mut velocities);
        }
        for &jid in island_joints {
            let j = self.joints[jid].as_ref().unwrap();
            let end = |id: BodyId, e: JointBody| {
                let b = self.body(id);
                JointBody { local_center: b.sweep.local_center, inv_mass: b.inv_mass, inv_i: b.inv_i, ..e }
            };
            let (a, b) = (end(j.body_a, j.revolute.a), end(j.body_b, j.revolute.b));
            self.joints[jid].as_mut().unwrap().revolute.init_velocity_constraints(a, b, step, &positions, &mut velocities);
        }
        for _ in 0..step.velocity_iterations {
            for &jid in island_joints {
                self.joints[jid].as_mut().unwrap().revolute.solve_velocity_constraints(&mut velocities);
            }
            solver.solve_velocity_constraints(&mut velocities);
        }
        let mut manifolds: Vec<Manifold> = solver_contacts.iter().map(|c| c.manifold).collect();
        solver.store_impulses(&mut manifolds);
        for (&cid, m) in island_contacts.iter().zip(&manifolds) {
            self.contact_mut(cid).manifold = *m;
        }
        for (i, &id) in island_bodies.iter().enumerate() {
            let Velocity { mut v, mut w } = velocities[i];
            let (tx, ty) = (v.x * h, v.y * h);
            let t2 = ty * ty + tx * tx;
            if MAX_TRANSLATION_SQUARED < t2 {
                let ratio = MAX_TRANSLATION / t2.sqrt();
                v = Vec2::new(v.x * ratio, v.y * ratio);
            }
            let rotation = w * h;
            if MAX_ROTATION_SQUARED < rotation * rotation {
                w = w * (MAX_ROTATION / rotation.abs());
            }
            let axis = self.body(id).axis;
            let p = &mut positions[i];
            p.c = Vec2::new(p.c.x + v.x * axis.x * h, p.c.y + v.y * axis.y * h);
            p.a = w * h + p.a;
            velocities[i] = Velocity { v, w };
        }
        let mut position_solved = false;
        for _ in 0..step.position_iterations {
            let contacts_solved = solver.solve_position_constraints(&mut positions);
            let mut joints_solved = true;
            for &jid in island_joints {
                joints_solved &= self.joints[jid].as_ref().unwrap().revolute.solve_position_constraints(&mut positions);
            }
            if contacts_solved && joints_solved {
                position_solved = true;
                break;
            }
        }
        for (i, &id) in island_bodies.iter().enumerate() {
            let b = self.body_mut(id);
            if b.kind == BodyType::Static {
                continue;
            }
            b.sweep.c = positions[i].c;
            b.sweep.a = positions[i].a;
            b.linear_velocity = velocities[i].v;
            b.angular_velocity = velocities[i].w;
            b.synchronize_transform();
        }
        self.post_solve(island_contacts, &solver.velocity_constraints);
        if !self.allow_sleep {
            return;
        }
        let mut min_sleep_time = f32::MAX;
        let linear = LINEAR_SLEEP_TOLERANCE * LINEAR_SLEEP_TOLERANCE;
        let angular = ANGULAR_SLEEP_TOLERANCE * ANGULAR_SLEEP_TOLERANCE;
        for &id in island_bodies {
            let b = self.body_mut(id);
            if b.kind == BodyType::Static {
                continue;
            }
            let v = b.linear_velocity;
            if b.flags & BODY_AUTO_SLEEP == 0 || angular < b.angular_velocity * b.angular_velocity || linear < v.y * v.y + v.x * v.x {
                b.sleep_time = 0.0;
                min_sleep_time = 0.0;
            } else {
                b.sleep_time = h + b.sleep_time;
                if b.sleep_time <= min_sleep_time {
                    min_sleep_time = b.sleep_time;
                }
            }
        }
        if TIME_TO_SLEEP <= min_sleep_time && position_solved {
            for &id in island_bodies {
                let b = self.body_mut(id);
                b.flags &= !BODY_AWAKE;
                b.sleep_time = 0.0;
                b.linear_velocity = Vec2::ZERO;
                b.angular_velocity = 0.0;
                b.force = Vec2::ZERO;
                b.torque = 0.0;
            }
        }
    }

    /// What the b2ContactSolver constructor reads from each contact and its bodies.
    fn solver_contacts(&self, contacts: &[ContactId]) -> Vec<SolverContact> {
        let solver_body = |f: FixtureId| {
            let b = self.body(self.fixture(f).body);
            SolverBody { inv_mass: b.inv_mass, axis: b.axis, inv_i: b.inv_i, local_center: b.sweep.local_center }
        };
        contacts
            .iter()
            .map(|&cid| {
                let c = self.contact(cid);
                SolverContact {
                    manifold: c.manifold,
                    radius_a: c.radius_a,
                    radius_b: c.radius_b,
                    friction: c.friction,
                    restitution: c.restitution,
                    tangent_speed: c.tangent_speed,
                    index_a: c.island_index_a,
                    index_b: c.island_index_b,
                    body_a: solver_body(c.fixture_a),
                    body_b: solver_body(c.fixture_b),
                }
            })
            .collect()
    }

    /// b2World::SolveTOI (180bbb7a0): Unity only sweeps contacts in the TOI array, and only when an awake
    /// body in the pair is a bullet.
    fn solve_toi(&mut self, step: &TimeStep) {
        if self.step_complete {
            for &id in &self.body_list {
                let b = self.bodies[id].as_mut().unwrap();
                b.flags &= !BODY_ISLAND;
                b.sweep.alpha0 = 0.0;
                b.island_index = -1;
            }
            for &id in &self.contacts_toi {
                let c = self.contacts[id].as_mut().unwrap();
                c.flags &= !(CONTACT_ISLAND | CONTACT_TOI);
                c.toi_count = 0;
                c.toi = 1.0;
            }
        }
        loop {
            let mut min_contact = None;
            let mut min_alpha = 1.0f32;
            for i in 0..self.contacts_toi.len() {
                let id = self.contacts_toi[i];
                let c = self.contact(id);
                if c.flags & CONTACT_ENABLED == 0 || MAX_SUB_STEPS < c.toi_count {
                    continue;
                }
                let alpha = if c.flags & CONTACT_TOI != 0 {
                    c.toi
                } else {
                    let (fa, fb) = (self.fixture(c.fixture_a), self.fixture(c.fixture_b));
                    if fa.is_sensor || fb.is_sensor {
                        continue;
                    }
                    let (ia, ib) = (fa.body, fb.body);
                    let (a, b) = (self.body(ia), self.body(ib));
                    let moving = a.active_and_moving() || b.active_and_moving();
                    if !moving || (a.flags | b.flags) & BODY_BULLET == 0 {
                        continue;
                    }
                    let mut alpha0 = a.sweep.alpha0;
                    if b.sweep.alpha0 <= alpha0 {
                        if b.sweep.alpha0 < alpha0 {
                            self.body_mut(ib).sweep.advance(alpha0);
                        }
                    } else {
                        alpha0 = b.sweep.alpha0;
                        self.body_mut(ia).sweep.advance(alpha0);
                    }
                    let c = self.contact(id);
                    let (fa, fb) = (self.fixture(c.fixture_a), self.fixture(c.fixture_b));
                    let proxy_a = DistanceProxy { radius: c.radius_a, ..fa.shape.proxy() };
                    let proxy_b = DistanceProxy { radius: c.radius_b, ..fb.shape.proxy() };
                    let output = time_of_impact(&proxy_a, &self.body(ia).sweep, &proxy_b, &self.body(ib).sweep, 1.0);
                    let mut alpha = 1.0;
                    if output.state == ToiState::Touching {
                        alpha = (1.0 - alpha0) * output.t + alpha0;
                        if !(alpha < 1.0) {
                            alpha = 1.0;
                        }
                    }
                    let c = self.contact_mut(id);
                    c.flags |= CONTACT_TOI;
                    c.toi = alpha;
                    alpha
                };
                if alpha < min_alpha {
                    min_contact = Some(id);
                    min_alpha = alpha;
                }
            }
            let Some(min) = min_contact.filter(|_| min_alpha <= ONE_MINUS_TEN_EPSILON) else {
                self.step_complete = true;
                break;
            };
            let c = self.contact(min);
            let (ia, ib) = (self.fixture(c.fixture_a).body, self.fixture(c.fixture_b).body);
            let (backup_a, backup_b) = (self.body(ia).sweep, self.body(ib).sweep);
            self.body_mut(ia).advance(min_alpha);
            self.body_mut(ib).advance(min_alpha);
            self.update_contact(min);
            let c = self.contact_mut(min);
            c.flags &= !CONTACT_TOI;
            c.toi_count += 1;
            if c.flags & (CONTACT_ENABLED | CONTACT_TOUCHING) != CONTACT_ENABLED | CONTACT_TOUCHING {
                c.flags &= !CONTACT_ENABLED;
                for (id, backup) in [(ia, backup_a), (ib, backup_b)] {
                    let b = self.body_mut(id);
                    b.sweep = backup;
                    b.synchronize_transform();
                }
                continue;
            }
            self.body_mut(ia).wake();
            self.body_mut(ib).wake();
            let mut island_bodies = vec![ia, ib];
            let mut island_contacts = vec![min];
            self.body_mut(ia).island_index = 0;
            self.body_mut(ib).island_index = 1;
            self.body_mut(ia).flags |= BODY_ISLAND;
            self.body_mut(ib).flags |= BODY_ISLAND;
            self.contact_mut(min).flags |= CONTACT_ISLAND;
            for body in [ia, ib] {
                if self.body(body).kind != BodyType::Dynamic {
                    continue;
                }
                let edges: Vec<(BodyId, ContactId)> = self.body(body).contacts.iter().rev().copied().collect();
                for (other, cid) in edges {
                    if island_bodies.len() == TOI_BODY_CAPACITY || island_contacts.len() == TOI_CONTACT_CAPACITY {
                        break;
                    }
                    let c = self.contact(cid);
                    if c.flags & CONTACT_ISLAND != 0 {
                        continue;
                    }
                    let o = self.body(other);
                    if o.kind == BodyType::Dynamic && (self.body(body).flags | o.flags) & BODY_BULLET == 0 {
                        continue;
                    }
                    if self.fixture(c.fixture_a).is_sensor || self.fixture(c.fixture_b).is_sensor {
                        continue;
                    }
                    let backup = o.sweep;
                    if o.flags & BODY_ISLAND == 0 {
                        self.body_mut(other).advance(min_alpha);
                    }
                    self.update_contact(cid);
                    let c = self.contact_mut(cid);
                    if c.flags & (CONTACT_ENABLED | CONTACT_TOUCHING) != CONTACT_ENABLED | CONTACT_TOUCHING {
                        let o = self.body_mut(other);
                        o.sweep = backup;
                        o.synchronize_transform();
                        continue;
                    }
                    c.flags |= CONTACT_ISLAND;
                    island_contacts.push(cid);
                    let o = self.body_mut(other);
                    if o.flags & BODY_ISLAND == 0 {
                        o.flags |= BODY_ISLAND;
                        if o.kind != BodyType::Static {
                            o.sleep_time = 0.0;
                            o.flags |= BODY_AWAKE;
                        }
                        o.island_index = island_bodies.len() as i32;
                        island_bodies.push(other);
                    }
                }
            }
            let dt = (1.0 - min_alpha) * step.dt;
            let sub_step = TimeStep { dt, inv_dt: 1.0 / dt, dt_ratio: 1.0, position_iterations: 20, velocity_iterations: step.velocity_iterations, warm_starting: false };
            for &cid in &island_contacts {
                let c = self.contact(cid);
                let a = self.body(self.fixture(c.fixture_a).body).island_index as usize;
                let b = self.body(self.fixture(c.fixture_b).body).island_index as usize;
                let c = self.contact_mut(cid);
                c.island_index_a = a;
                c.island_index_b = b;
            }
            for &b in &island_bodies {
                self.body_mut(b).island_index = -1;
            }
            self.solve_toi_island(&island_bodies, &island_contacts, &sub_step, 0, 1);
            for &id in &island_bodies {
                self.body_mut(id).flags &= !BODY_ISLAND;
                if self.body(id).kind != BodyType::Dynamic {
                    continue;
                }
                self.synchronize_fixtures(id);
                for (_, cid) in self.body(id).contacts.clone() {
                    self.contact_mut(cid).flags &= !(CONTACT_ISLAND | CONTACT_TOI);
                }
            }
            self.find_new_contacts();
            if self.sub_stepping {
                self.step_complete = false;
                break;
            }
        }
    }

    /// b2Island::SolveTOI (180bbb360): Unity integrates without the body axis constraints here.
    pub(crate) fn solve_toi_island(&mut self, island_bodies: &[BodyId], island_contacts: &[ContactId], step: &TimeStep, toi_index_a: usize, toi_index_b: usize) {
        let mut positions: Vec<Position> = island_bodies.iter().map(|&id| Position { c: self.body(id).sweep.c, a: self.body(id).sweep.a }).collect();
        let mut velocities: Vec<Velocity> = island_bodies.iter().map(|&id| Velocity { v: self.body(id).linear_velocity, w: self.body(id).angular_velocity }).collect();
        let mut solver = ContactSolver::new(*step, &self.solver_contacts(island_contacts));
        for _ in 0..step.position_iterations {
            if solver.solve_toi_position_constraints(&mut positions, toi_index_a, toi_index_b) {
                break;
            }
        }
        for i in [toi_index_a, toi_index_b] {
            let b = self.body_mut(island_bodies[i]);
            b.sweep.c0 = positions[i].c;
            b.sweep.a0 = positions[i].a;
        }
        solver.initialize_velocity_constraints(&positions, &velocities);
        for _ in 0..step.velocity_iterations {
            solver.solve_velocity_constraints(&mut velocities);
        }
        let h = step.dt;
        for (i, &id) in island_bodies.iter().enumerate() {
            let Velocity { mut v, mut w } = velocities[i];
            let (tx, ty) = (v.x * h, v.y * h);
            let t2 = ty * ty + tx * tx;
            if MAX_TRANSLATION_SQUARED < t2 {
                let length = t2.sqrt();
                v = Vec2::new(v.x * (MAX_TRANSLATION / length), v.y * (MAX_TRANSLATION / length));
            }
            let rotation = w * h;
            if MAX_ROTATION_SQUARED < rotation * rotation {
                w = w * (MAX_ROTATION / rotation.abs());
            }
            let b = self.body_mut(id);
            b.sweep.c = Vec2::new(positions[i].c.x + v.x * h, positions[i].c.y + v.y * h);
            b.sweep.a = positions[i].a + w * h;
            b.linear_velocity = v;
            b.angular_velocity = w;
            b.synchronize_transform();
        }
        self.post_solve(island_contacts, &solver.velocity_constraints);
    }

    /// b2Body::SynchronizeFixtures (180bbc900) with Fixture::Synchronize (180bbc7d0).
    fn synchronize_fixtures(&mut self, id: BodyId) {
        let b = self.body(id);
        let q = Rot::new(b.sweep.a0);
        let lc = b.sweep.local_center;
        let xf1 = Transform { p: Vec2::new(b.sweep.c0.x - (lc.x * q.c - lc.y * q.s), b.sweep.c0.y - (lc.y * q.c + lc.x * q.s)), q };
        let xf2 = b.xf;
        let displacement = xf2.p - xf1.p;
        for f in b.fixtures.iter().rev().copied().collect::<Vec<_>>() {
            let fixture = self.fixtures[f].as_mut().unwrap();
            for (aabb, proxy) in &mut fixture.proxies {
                *aabb = Aabb::combine(&fixture.shape.compute_aabb(&xf1), &fixture.shape.compute_aabb(&xf2));
                self.broad_phase.move_proxy(*proxy, aabb, displacement);
            }
        }
    }
}

/// b2ContactFilter::ShouldCollide.
fn should_collide(a: &Filter, b: &Filter) -> bool {
    if a.group_index == b.group_index && a.group_index != 0 {
        return 0 < a.group_index;
    }
    a.mask_bits & b.category_bits != 0 && a.category_bits & b.mask_bits != 0
}

/// The b2*Contact::Evaluate natives: polygons with a rounded radius take the radial path.
fn evaluate(manifold: &mut Manifold, a: &Shape, xf_a: &Transform, b: &Shape, xf_b: &Transform) {
    match (a, b) {
        (Shape::Polygon(a), Shape::Polygon(b)) if a.radius <= POLYGON_RADIUS && b.radius <= POLYGON_RADIUS => collide_polygons(manifold, a, xf_a, b, xf_b),
        (Shape::Polygon(a), Shape::Polygon(b)) => collide_radial_polygons(manifold, a, xf_a, b, xf_b),
        (Shape::Polygon(a), Shape::Circle(b)) => collide_polygon_and_circle(manifold, a, xf_a, b, xf_b),
        (Shape::Circle(a), Shape::Circle(b)) => collide_circles(manifold, a, xf_a, b, xf_b),
        (Shape::Circle(_), Shape::Polygon(_)) => unreachable!("create_contact puts the polygon first"),
    }
}

/// ponytail: Box2D 2.3's b2TestOverlap; Unity's 180bb1970 not yet compared.
fn test_overlap(a: &Shape, xf_a: &Transform, b: &Shape, xf_b: &Transform) -> bool {
    let mut cache = SimplexCache::default();
    distance(&mut cache, &a.proxy(), xf_a, &b.proxy(), xf_b, true).distance < 10.0 * f32::EPSILON
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::b2::shape::{box_corners, Circle, Polygon};

    fn box_fixture(size: Vec2) -> FixtureDef {
        let shape = Shape::Polygon(Polygon::from_points(&box_corners(Vec2::ZERO, size), POLYGON_RADIUS));
        FixtureDef { shape, friction: 0.4, restitution: 0.0, density: 1.0, is_sensor: false, filter: Filter::default(), collider: ColliderRef::default() }
    }

    #[test]
    fn box_lands_on_ground_and_sleeps() {
        let mut world = World::default();
        let ground = world.create_body(&BodyDef::default());
        world.create_fixture(ground, &box_fixture(Vec2::new(20.0, 1.0)), true);
        let def = BodyDef { kind: BodyType::Dynamic, position: Vec2::new(0.0, 3.0), ..BodyDef::default() };
        let crate_ = world.create_body(&def);
        world.create_fixture(crate_, &box_fixture(Vec2::new(1.0, 1.0)), true);
        for _ in 0..300 {
            world.step(1.0 / 60.0, 16, 16);
        }
        let b = world.body(crate_);
        assert!((b.xf.p.y - 1.0).abs() < 0.05, "rest height {}", b.xf.p.y);
        assert!(b.xf.p.x.abs() < 1e-3 && b.sweep.a.abs() < 1e-3);
        assert_eq!(b.flags & BODY_AWAKE, 0, "should sleep");
    }

    #[test]
    fn bullet_stops_at_thin_wall() {
        let run = |bullet: bool| {
            let mut world = World::default();
            let wall = world.create_body(&BodyDef { position: Vec2::new(5.0, 0.0), ..BodyDef::default() });
            world.create_fixture(wall, &box_fixture(Vec2::new(0.1, 4.0)), true);
            let def = BodyDef { kind: BodyType::Dynamic, linear_velocity: Vec2::new(400.0, 0.0), gravity_scale: 0.0, bullet, ..BodyDef::default() };
            let ball = world.create_body(&def);
            let shape = Shape::Circle(Circle { p: Vec2::ZERO, radius: 0.1 });
            world.create_fixture(ball, &FixtureDef { shape, ..box_fixture(Vec2::new(1.0, 1.0)) }, true);
            for _ in 0..10 {
                world.step(1.0 / 60.0, 16, 16);
            }
            world.body(ball).xf.p.x
        };
        assert!(run(false) > 5.0, "a discrete body tunnels");
        assert!(run(true) < 5.0, "a bullet stops at the wall");
    }

    #[test]
    fn hinged_box_swings_without_touching_its_pivot() {
        let mut world = World::default();
        let pivot = world.create_body(&BodyDef::default());
        world.create_fixture(pivot, &box_fixture(Vec2::new(0.5, 0.5)), true);
        let arm = world.create_body(&BodyDef { kind: BodyType::Dynamic, position: Vec2::new(1.0, 0.0), ..BodyDef::default() });
        world.create_fixture(arm, &box_fixture(Vec2::new(1.5, 0.2)), true);
        let def = RevoluteDef { local_anchor_b: Vec2::new(-1.0, 0.0), ..RevoluteDef::default() };
        world.create_joint(def, pivot, arm, false);
        let mut lowest = 0.0f32;
        for _ in 0..120 {
            world.step(1.0 / 60.0, 16, 16);
            let b = world.body(arm);
            let anchor = b.xf.p + b.xf.q.apply(Vec2::new(-1.0, 0.0));
            assert!(anchor.x.abs() < 0.02 && anchor.y.abs() < 0.02, "anchor drifted to {anchor:?}");
            lowest = lowest.min(b.xf.p.y);
        }
        assert!(lowest < -0.9, "the arm should swing down, lowest {lowest}");
        assert!(world.body(arm).contacts.is_empty(), "jointed bodies must not collide");
    }
}
