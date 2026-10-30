//! Object.Instantiate of a prefab from the game's own data: InstantiateObject (180634340) clones the hierarchy,
//! then AwakeAndActivateClonedObjects (1806321d0) wakes the clones through an AwakeFromLoadQueue in
//! kInstantiateOrCreateFromCodeAwakeFromLoad mode. This module is the native part of that: the clones' instance
//! ids and Transforms, and the Rigidbody2D, Collider2D and HingeJoint2D AwakeFromLoads that build the b2 world
//! state.

use super::*;
use crate::assets::components::{ColliderShape, Joint2D as Joint2DData, JointKind as JointData, Rigidbody2D as RigidbodyData};
use crate::assets::db::{Component, ObjRef, Prefab};
use crate::b2::revolute::RevoluteDef;
use crate::b2::shape::{box_collider_polygon, ColliderMatrix};
use crate::b2::weld::check_bodies;
use crate::b2::world::{BodyDef, BodyId, BodyType, ColliderRef, FixtureDef, FixtureId, JointId, RigidbodyMass, RigidbodyRef, Shape};
use crate::unity::transform::{global_position, global_rotation, local_to_world, lossy_scale, quaternion_to_matrix, Trs};
use std::collections::HashMap;

/// One cloned GameObject: its instance id, its components' ids (prefab order) and its local Transform.
#[derive(Clone, Debug)]
pub struct Clone {
    pub game_object: i32,
    pub components: Vec<i32>,
    pub trs: Trs,
    pub parent: Option<usize>,
    /// GameObject.activeInHierarchy.
    pub active: bool,
}

/// An instantiated prefab: the clones in prefab order, and the b2 objects their components made.
#[derive(Clone, Debug, Default)]
pub struct Instance {
    pub objects: Vec<Clone>,
    /// Rigidbody2D bodies by object.
    pub bodies: HashMap<usize, BodyId>,
    /// Collider2D fixtures by (object, component).
    pub fixtures: HashMap<(usize, usize), Vec<FixtureId>>,
    /// HingeJoint2D joints by (object, component).
    pub joints: HashMap<(usize, usize), JointId>,
    /// The HingeJoint2Ds' native state by (object, component).
    pub hinges: HashMap<(usize, usize), HingeState>,
    /// The Collider2Ds' native state by (object, component).
    pub colliders: HashMap<(usize, usize), ColliderState>,
    /// Each object's TransformChangeDispatch interest in the Physics2D systems (the CHANGE_* bits).
    pub interest: Vec<u8>,
    /// Each object's changed bits for those systems, cleared by SyncTransforms.
    pub changed: Vec<u8>,
}

/// The Physics2D TransformChangeDispatch systems InitializePhysicsManager2D (180bf14a0) registers: the
/// Rigidbody2D T (global T), R (global R), S (global S) and Anim (physics animation) systems and the Collider2D
/// TRS system (global T, R and S).
pub const CHANGE_RB_T: u8 = 1;
pub const CHANGE_RB_R: u8 = 2;
pub const CHANGE_RB_S: u8 = 4;
pub const CHANGE_RB_ANIM: u8 = 8;
pub const CHANGE_COLLIDER: u8 = 16;
/// The systems interested in each kind of change (TransformInternal::g_ChangeMaskCache).
const GLOBAL_T: u8 = CHANGE_RB_T | CHANGE_COLLIDER;
const GLOBAL_R: u8 = CHANGE_RB_R | CHANGE_COLLIDER;
const GLOBAL_S: u8 = CHANGE_RB_S | CHANGE_COLLIDER;

/// HingeJoint2D's runtime fields.
#[derive(Clone, Copy, Debug, Default)]
pub struct HingeState {
    /// AnchoredJoint2D.m_ConnectedAnchor: serialized, then rewritten by auto-configuration.
    pub connected_anchor: [f32; 2],
    /// Joint2D.m_AutoConfigurationComplete: set by FinalizeCreateJoint while the world plays.
    pub auto_configuration_complete: bool,
    pub has_previous_state: bool,
    pub reference_angle: f32,
    pub body_angle_a: f32,
    pub body_angle_b: f32,
}

/// Collider2D's runtime transform fields, set by CreateFixtures.
#[derive(Clone, Copy, Debug)]
pub struct ColliderState {
    pub relative_transform: [f32; 16],
    pub rigidbody_scale: [f32; 3],
}

/// AwakeFromLoadQueue::DetermineQueueIndex (180911850) for the component kinds a prefab here holds: the custom
/// queues of InitializeClass (180914380), then MonoBehaviours 0x14, Collider2D 0xd and other components 0x10.
fn queue_index(c: Option<&Component>) -> usize {
    match c {
        None => 1,
        Some(Component::Rigidbody2D(_)) => 0xb,
        Some(Component::Collider2D(_)) => 0xd,
        Some(Component::Script { .. }) => 0x14,
        // ParticleSystem (198) has its own queue; ParticleSystemRenderer (199) and the rest are components.
        Some(Component::Other(198)) => 0xf,
        Some(_) => 0x10,
    }
}

impl Instance {
    /// The object's own local TRS, then each parent's up to the root.
    pub fn chain(&self, o: usize) -> Vec<Trs> {
        let mut out = vec![self.objects[o].trs];
        let mut p = self.objects[o].parent;
        while let Some(i) = p {
            out.push(self.objects[i].trs);
            p = self.objects[i].parent;
        }
        out
    }

    /// Transform::GetPosition (CalculateGlobalPosition).
    pub fn position(&self, o: usize) -> [f32; 3] {
        let chain = self.chain(o);
        let mut p = chain[0].t;
        for n in &chain[1..] {
            p = global_position(n, p);
        }
        [p[0], p[1], p[2]]
    }

    /// Transform::GetRotation (CalculateGlobalRotation).
    pub fn rotation(&self, o: usize) -> [f32; 4] {
        let chain = self.chain(o);
        let mut q = chain[0].q;
        for n in &chain[1..] {
            q = global_rotation(n, q);
        }
        q
    }

    /// Transform::GetWorldScaleLossy.
    pub fn lossy_scale(&self, o: usize) -> [f32; 3] {
        lossy_scale(&self.chain(o), self.rotation(o))
    }

    /// Transform::SetLocalScale (1807f4b10): SetLocalS (1800ca2d0) marks the object local and global S and every
    /// descendant global T, R and S, for the systems interested in each.
    pub fn set_local_scale(&mut self, o: usize, s: [f32; 3]) {
        let old = std::mem::replace(&mut self.objects[o].trs.s, [s[0], s[1], s[2], 0.0]);
        if (0..3).all(|i| old[i] == s[i]) {
            return;
        }
        self.changed[o] |= self.interest[o] & GLOBAL_S;
        for d in self.descendants(o) {
            self.changed[d] |= self.interest[d] & (GLOBAL_T | GLOBAL_R | GLOBAL_S);
        }
    }

    /// The object's descendants in hierarchy (pre-) order.
    fn descendants(&self, o: usize) -> Vec<usize> {
        (o + 1..self.objects.len()).take_while(|&d| self.is_descendant(d, o)).collect()
    }

    fn is_descendant(&self, d: usize, o: usize) -> bool {
        let mut p = self.objects[d].parent;
        while let Some(i) = p {
            if i == o {
                return true;
            }
            p = self.objects[i].parent;
        }
        false
    }
}

const IDENTITY: [f32; 16] = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

fn neg(x: f32) -> f32 {
    f32::from_bits(x.to_bits() ^ 0x8000_0000)
}

/// The (z, w) PhysicsUtility2D::ZRotFromFullRot (180bfabf0) and CalculateColliderTransformation's inline copy take
/// the angle from: the quaternion's sign flipped when w < 0, then again if w is still negative.
fn z_rot_angle(q: [f32; 4]) -> f32 {
    let (mut z, mut w) = if 0.0 <= q[3] { (q[2], q[3]) } else { (neg(q[2]), neg(q[3])) };
    if w < 0.0 {
        (z, w) = (neg(z), neg(w));
    }
    crate::unity::atan2f(z, w)
}

/// The body angle Rigidbody2D::Create and SyncTransforms take from a rotation: the quaternion's sign flipped
/// when w < 0, then twice the half angle.
fn body_angle(mut q: [f32; 4]) -> f32 {
    if q[3] < 0.0 {
        (q[2], q[3]) = (neg(q[2]), neg(q[3]));
    }
    let a = crate::unity::atan2f(q[2], q[3]);
    a + a
}

/// result = R · M in Collider2D::CalculateColliderTransformation's add order.
fn collider_product(m: &[f32; 16], r: &[f32; 16]) -> [f32; 16] {
    let mut out = [0.0f32; 16];
    for c in 0..4 {
        for row in 0..4 {
            out[c * 4 + row] = m[c * 4 + 1] * r[4 + row] + m[c * 4] * r[row] + m[c * 4 + 2] * r[8 + row] + m[c * 4 + 3] * r[12 + row];
        }
    }
    out
}

/// Loads a rewritten script's fields from its serialized MonoBehaviour. Returns the fields the data lacks: the
/// C# runtime state, which keeps its field initializer.
pub fn load_values(fields: Vec<(&'static str, &mut dyn Field)>, data: &crate::assets::mono::Value) -> Result<Vec<&'static str>, String> {
    let mut runtime = Vec::new();
    for (name, field) in fields {
        match data.get(name) {
            Some(v) => field.set(v).map_err(|e| format!("{name}: {e}"))?,
            None => runtime.push(name),
        }
    }
    Ok(runtime)
}

impl Game {
    /// Object.Instantiate(prefab, position, rotation): the clone ids go down from `next_instance_id` by 2 in
    /// prefab pre-order, each GameObject before its components. `original_ids` are the prefab objects' own
    /// instance ids (GameObject or component ObjRef -> id), whose order is the remap's and so the queue's.
    pub fn instantiate(&mut self, prefab: &Prefab, position: [f32; 3], rotation: [f32; 4], original_ids: &HashMap<ObjRef, i32>) -> Instance {
        let (inst, behaviours) = self.instantiate_native(prefab, position, rotation, original_ids);
        if !behaviours.is_empty() {
            unimplemented!("AwakeFromLoadQueue 0x14: MonoBehaviour Awake/OnEnable in SortBehaviourItemByExecutionOrderAndReverseInstanceID order");
        }
        inst
    }

    /// Instantiate up to the last AwakeFromLoadQueue: the clones and queues 0..0x13 (every native component).
    /// Returns the MonoBehaviours of queue 0x14, (object, component), in the order AwakeFromLoadQueue::AwakeFromLoad
    /// (1809112c0) wakes them: sorted by SortBehaviourItemByExecutionOrderAndReverseInstanceID (180915860),
    /// script execution order first, then the higher clone id.
    pub fn instantiate_native(&mut self, prefab: &Prefab, position: [f32; 3], rotation: [f32; 4], original_ids: &HashMap<ObjRef, i32>) -> (Instance, Vec<(usize, usize)>) {
        let mut inst = Instance::default();
        for (i, o) in prefab.objects.iter().enumerate() {
            let game_object = self.allocate_instance_id();
            let components = o.components.iter().map(|_| self.allocate_instance_id()).collect();
            let Some((_, Component::Transform(t))) = o.components.first() else { panic!("{}: first component not a Transform", o.name) };
            let v3 = |v: [f32; 3]| [v[0], v[1], v[2], 0.0];
            let mut trs = Trs { t: v3(t.local_position), q: t.local_rotation, s: v3(t.local_scale) };
            // The root takes the given pose (Instantiate's SetPositionAndRotation on a parentless clone).
            if i == 0 {
                trs.t = v3(position);
                trs.q = rotation;
            }
            let active = o.active && o.parent.map_or(true, |p| inst.objects[p].active);
            inst.objects.push(Clone { game_object, components, trs, parent: o.parent, active });
        }
        // A clone copies its original's interests (CopyTransformSubhierarchy); a prefab asset has no Physics2D
        // interest, so the clones start with none and nothing changed.
        inst.interest = vec![0; inst.objects.len()];
        inst.changed = vec![0; inst.objects.len()];
        // The remap: every original (GameObject and component) with its clone, sorted by original id.
        let mut items: Vec<(i32, usize, Option<usize>)> = Vec::new();
        for (i, o) in prefab.objects.iter().enumerate() {
            items.push((original_ids[&o.game_object], i, None));
            for (c, (r, _)) in o.components.iter().enumerate() {
                items.push((original_ids[r], i, Some(c)));
            }
        }
        items.sort_by_key(|x| x.0);
        let mut queues: Vec<Vec<(usize, Option<usize>)>> = vec![Vec::new(); 0x15];
        for &(_, o, c) in &items {
            queues[queue_index(c.map(|c| &prefab.objects[o].components[c].1))].push((o, c));
        }
        for items in &queues[..0x14] {
            for &(o, c) in items {
                let Some(c) = c else { continue };
                match &prefab.objects[o].components[c].1 {
                    Component::Rigidbody2D(rb) => self.rigidbody2d_awake(&mut inst, prefab, o, c, rb),
                    Component::Collider2D(_) => self.collider2d_awake(&mut inst, prefab, o, c),
                    Component::Joint2D(j) => self.joint2d_awake(&mut inst, prefab, o, c, j),
                    // Transform, SpriteRenderer and the unread classes make no physics state.
                    _ => {}
                }
            }
        }
        let mut behaviours: Vec<(usize, usize)> = queues[0x14].iter().map(|&(o, c)| (o, c.unwrap())).collect();
        behaviours.sort_by_key(|&(o, c)| {
            let Component::Script { execution_order, .. } = &prefab.objects[o].components[c].1 else { unreachable!() };
            (*execution_order, std::cmp::Reverse(inst.objects[o].components[c]))
        });
        (inst, behaviours)
    }

    /// Instantiate's id allocation: each new object takes the next id below the last.
    pub fn allocate_instance_id(&mut self) -> i32 {
        let id = self.next_instance_id;
        self.next_instance_id -= 2;
        id
    }

    /// Rigidbody2D::AwakeFromLoad (180c1c8e0) in instantiate mode: Create on an active object without a body.
    fn rigidbody2d_awake(&mut self, inst: &mut Instance, prefab: &Prefab, o: usize, c: usize, rb: &RigidbodyData) {
        if inst.objects[o].active && !inst.bodies.contains_key(&o) {
            self.rigidbody2d_create(inst, prefab, o, c, rb);
        }
    }

    /// Rigidbody2D::Create (180c1e6b0).
    fn rigidbody2d_create(&mut self, inst: &mut Instance, _prefab: &Prefab, o: usize, c: usize, rb: &RigidbodyData) {
        if rb.constraints & 3 != 0 {
            unimplemented!("Rigidbody2D FreezePositionX/Y (b2BodyDef.fixedPositionX/Y)");
        }
        let kind = match rb.body_type {
            0 => BodyType::Dynamic,
            1 => BodyType::Kinematic,
            _ => BodyType::Static,
        };
        let (awake, allow_sleep) = if kind == BodyType::Static { (false, true) } else { (rb.sleeping_mode != 2, rb.sleeping_mode != 0) };
        let mut def = BodyDef {
            kind,
            bullet: rb.collision_detection == 1,
            linear_damping: rb.linear_drag,
            angular_damping: rb.angular_drag,
            fixed_rotation: rb.constraints & 4 != 0,
            gravity_scale: rb.gravity_scale,
            active: rb.simulated,
            awake,
            allow_sleep,
            rigidbody: RigidbodyMass { use_auto_mass: rb.use_auto_mass, mass: rb.mass, inertia: None, center_of_mass: None },
            user: Some(RigidbodyRef { id: inst.objects[o].components[c], game_object: inst.objects[o].game_object }),
            ..Default::default()
        };
        if inst.objects[o].active {
            let p = inst.position(o);
            def.position = V::new(p[0], p[1]);
            def.angle = body_angle(inst.rotation(o));
        }
        let body = self.world.create_body(&def);
        self.world.reset_mass_data(body);
        inst.bodies.insert(o, body);
        // PhysicsManager2D::RegisterTransformChangeInterestForRigidbody (180bf71b0).
        inst.interest[o] |= CHANGE_RB_T | CHANGE_RB_R | CHANGE_RB_S | CHANGE_RB_ANIM;
        if rb.interpolate != 0 && rb.interpolate != 1 {
            unimplemented!("Rigidbody2D extrapolation");
        }
    }

    /// PhysicsManager2D::SyncTransforms (180bf91d0), the start of every Simulate, for the instance's hierarchy.
    /// The Rigidbody2D systems' changes first, objects in hierarchy order: a moved or turned body is set to its
    /// Transform's pose and its contacts recalculated, then its joints recreated. Then the collider system's: an
    /// enabled collider whose body scale or body-relative matrix moved past Vector3f::epsilon is recreated.
    /// Unity gathers the changes as one job per range of hierarchies, so the order across several changed
    /// hierarchies is the jobs' finishing order; within one it is the hierarchy's.
    /// The instance's bodies were never interpolated nor given MovePosition/MoveRotation, so the
    /// RigidbodyMovementState2D updates leave the b2 state alone.
    pub fn sync_transforms(&mut self, inst: &mut Instance, prefab: &Prefab) {
        const RB: u8 = CHANGE_RB_T | CHANGE_RB_R | CHANGE_RB_S | CHANGE_RB_ANIM;
        for o in 0..inst.objects.len() {
            let bits = inst.changed[o] & RB;
            inst.changed[o] &= !RB;
            if bits == 0 {
                continue;
            }
            let Some(&body) = inst.bodies.get(&o) else { continue };
            let anim = bits & CHANGE_RB_ANIM != 0 && self.world.body(body).kind != BodyType::Static;
            let (t, r) = (bits & CHANGE_RB_T != 0, bits & CHANGE_RB_R != 0);
            if t || r {
                let (pos, rot) = (inst.position(o), inst.rotation(o));
                let b = self.world.body(body);
                let p = if t { V::new(pos[0], pos[1]) } else { b.xf.p };
                let angle = if r { body_angle(rot) } else { b.sweep.a };
                self.world.set_transform(body, p, angle);
                self.rigidbody2d_recalculate_contacts(inst, prefab, body);
                if anim {
                    continue;
                }
            } else if anim || bits & CHANGE_RB_S == 0 {
                continue;
            }
            self.rigidbody2d_recreate_joints(inst, prefab, o);
        }
        for o in 0..inst.objects.len() {
            if inst.changed[o] & CHANGE_COLLIDER == 0 {
                continue;
            }
            inst.changed[o] &= !CHANGE_COLLIDER;
            for (c, (_, comp)) in prefab.objects[o].components.iter().enumerate() {
                let Component::Collider2D(col) = comp else { continue };
                if !col.enabled {
                    continue;
                }
                let (body_object, m) = self.collider_transformation(inst, prefab, o);
                let scale = match body_object {
                    Some(b) => {
                        let s = inst.objects[b].trs.s;
                        [s[0], s[1], s[2]]
                    }
                    None => [1.0; 3],
                };
                let st = inst.colliders[&(o, c)];
                let d = [scale[0] - st.rigidbody_scale[0], scale[1] - st.rigidbody_scale[1], scale[2] - st.rigidbody_scale[2]];
                const EPSILON: f32 = f32::from_bits(0x3727_c5ac);
                // CompareApproximately(Matrix4x4f) (18073cbb0): every element within epsilon.
                let same = (0..16).all(|i| EPSILON >= (st.relative_transform[i] - m[i]).abs());
                if EPSILON * EPSILON < d[0] * d[0] + d[1] * d[1] + d[2] * d[2] || !same {
                    self.collider2d_recreate(inst, prefab, o, c);
                }
            }
        }
    }

    /// Rigidbody2D::RecalculateContacts (180c22660): Collider2D::RecalculateContacts on each enabled attached
    /// collider with shapes (not used by an effector), then the body is woken.
    fn rigidbody2d_recalculate_contacts(&mut self, inst: &Instance, prefab: &Prefab, body: BodyId) {
        if self.world.body(body).fixtures.is_empty() {
            return;
        }
        let attached: Vec<(usize, usize)> = inst.fixtures.iter().filter(|(_, f)| f.iter().any(|&f| self.world.fixture(f).body == body)).map(|(&k, _)| k).collect();
        if 1 < attached.len() {
            unimplemented!("Rigidbody2D::GetAttachedColliders: several colliders on one body come in Collider2D pointer order");
        }
        for &(o, c) in &attached {
            let Component::Collider2D(col) = &prefab.objects[o].components[c].1 else { unreachable!() };
            if col.enabled && !col.used_by_effector {
                self.world.refilter_collider(inst.objects[o].components[c]);
                if self.world.body(body).kind == BodyType::Static {
                    unimplemented!("Collider2D::WakeAllBodyContacts on a static Rigidbody2D");
                }
            }
        }
        self.world.wake_body(body);
    }

    /// Collider2D::RecreateCollider(null) (180c14750) outside a composite: the body's contacts wake their bodies,
    /// Cleanup(kColliderRecreate) (180c0d940) keeps the collider's collisions and destroys its fixtures (last
    /// first) and resets the body mass, then CreateFixtures, and the attached Rigidbody2D is woken.
    fn collider2d_recreate(&mut self, inst: &mut Instance, prefab: &Prefab, o: usize, c: usize) {
        let id = inst.objects[o].components[c];
        let fixtures = inst.fixtures.remove(&(o, c)).unwrap_or_default();
        if let Some(&first) = fixtures.first() {
            let body = self.world.fixture(first).body;
            self.world.wake_contact_bodies(body);
            self.world.contacts2d.flag_for_recreate(id);
            for &f in fixtures.iter().rev() {
                self.world.destroy_fixture(body, f);
            }
            self.world.reset_mass_data(body);
        }
        if let Some(st) = inst.colliders.get_mut(&(o, c)) {
            st.relative_transform = IDENTITY;
        }
        self.collider2d_create_fixtures(inst, prefab, o, c);
        let created = inst.fixtures.get(&(o, c)).filter(|f| !f.is_empty()).map(|f| f[0]);
        match created {
            None => self.world.contacts2d.destroy_contacts(id),
            Some(first) => {
                let body = self.world.fixture(first).body;
                if inst.bodies.values().any(|&b| b == body) {
                    self.world.wake_body(body);
                }
            }
        }
    }

    /// Collider2D::AwakeFromLoad (180c0c370) in instantiate mode: CreateFixtures on an active object, and
    /// Collider2D::AddToManager (180c0bcc0) on an enabled one, which registers the GameObject's interest in the
    /// collider TRS system.
    fn collider2d_awake(&mut self, inst: &mut Instance, prefab: &Prefab, o: usize, c: usize) {
        if inst.objects[o].active {
            self.collider2d_create_fixtures(inst, prefab, o, c);
            let Component::Collider2D(col) = &prefab.objects[o].components[c].1 else { unreachable!() };
            if col.enabled {
                inst.interest[o] |= CHANGE_COLLIDER;
            }
        }
    }

    /// The Rigidbody2D the collider's shapes go on and the collider-to-body matrix:
    /// Collider2D::CalculateColliderTransformation (180c0c8b0). None is the ground body.
    fn collider_transformation(&self, inst: &Instance, prefab: &Prefab, o: usize) -> (Option<usize>, [f32; 16]) {
        let has_body = |i: usize| inst.objects[i].active && inst.bodies.contains_key(&i) && prefab.objects[i].components.iter().any(|(_, c)| matches!(c, Component::Rigidbody2D(_)));
        if has_body(o) {
            // The collider on its own body: the world matrix without translation, rotated back by the body's
            // z rotation.
            let mut m = local_to_world(&inst.chain(o));
            (m[12], m[13], m[14]) = (0.0, 0.0, 0.0);
            let a = z_rot_angle(inst.rotation(o));
            let (s, c) = (crate::unity::sinf(a), crate::unity::cosf(a));
            let r = quaternion_to_matrix([neg(0.0), neg(0.0), neg(s), c]);
            return (Some(o), collider_product(&m, &r));
        }
        let mut p = inst.objects[o].parent;
        while let Some(i) = p {
            if has_body(i) {
                let m = local_to_world(&inst.chain(o));
                let a = z_rot_angle(inst.rotation(i));
                let q = [neg(0.0), neg(0.0), neg(crate::unity::sinf(a)), crate::unity::cosf(a)];
                let pos = inst.position(i);
                let (px, py, pz) = (neg(pos[0]), neg(pos[1]), neg(pos[2]));
                let [x, y, z, w] = q;
                let (f10, f11, f12, f13) = (y * 2.0, z * 2.0, w * x * 2.0, x * x * 2.0);
                let t = [
                    (1.0 - (z * f11 + y * f10)) * px + (x * f10 - w * f11) * py + (w * f10 + x * f11) * pz,
                    (1.0 - (z * f11 + f13)) * py + (w * f11 + x * f10) * px + (y * f11 - f12) * pz,
                    (x * f11 - w * f10) * px + (f12 + y * f11) * py + (1.0 - (y * f10 + f13)) * pz,
                ];
                let mut r = quaternion_to_matrix(q);
                (r[12], r[13], r[14]) = (t[0], t[1], t[2]);
                return (Some(i), collider_product(&m, &r));
            }
            p = inst.objects[i].parent;
        }
        (None, local_to_world(&inst.chain(o)))
    }

    /// Collider2D::CreateFixtures (180c0dba0) with no explicit body, for a collider outside any composite.
    fn collider2d_create_fixtures(&mut self, inst: &mut Instance, prefab: &Prefab, o: usize, c: usize) {
        let Component::Collider2D(col) = &prefab.objects[o].components[c].1 else { unreachable!() };
        if !col.enabled {
            return;
        }
        if col.used_by_composite {
            unimplemented!("CompositeCollider2D");
        }
        let (body_object, m) = self.collider_transformation(inst, prefab, o);
        let rigidbody_scale = match body_object {
            Some(b) => {
                let s = inst.objects[b].trs.s;
                [s[0], s[1], s[2]]
            }
            None => [1.0; 3],
        };
        inst.colliders.insert((o, c), ColliderState { relative_transform: m, rigidbody_scale });
        let matrix = ColliderMatrix([m[0], m[1], m[4], m[5], m[8], m[9], m[12], m[13]]);
        let shapes: Vec<Shape> = match &col.shape {
            ColliderShape::Box { tiling, auto_tiling, size, edge_radius } => {
                if *auto_tiling || tiling.draw_mode != 0 {
                    unimplemented!("BoxCollider2D auto-tiling");
                }
                box_collider_polygon(V::new(col.offset[0], col.offset[1]), V::new(size[0], size[1]), *edge_radius, &matrix).map(Shape::Polygon).into_iter().collect()
            }
            other => unimplemented!("Collider2D::PrepareShapes for {other:?}"),
        };
        if shapes.is_empty() {
            return;
        }
        let (friction, restitution) = self.collider_material_state(prefab, o, col, body_object);
        let body = match body_object {
            Some(b) => inst.bodies[&b],
            None => self.world.ground_body(),
        };
        let rb = body_object.and_then(|b| prefab.objects[b].components.iter().find_map(|(_, c)| match c {
            Component::Rigidbody2D(r) => Some(r.clone()),
            _ => None,
        }));
        let density = match rb {
            Some(r) if r.use_auto_mass && r.body_type == 0 => col.density,
            _ => 1.0,
        };
        let collider = ColliderRef { id: inst.objects[o].components[c], game_object: inst.objects[o].game_object };
        let mut ids = Vec::new();
        for shape in shapes {
            let def = FixtureDef { shape, friction, restitution, density, is_sensor: col.is_trigger, filter: Default::default(), collider };
            ids.push(self.world.create_fixture(body, &def, false));
        }
        self.world.reset_mass_data(body);
        inst.fixtures.insert((o, c), ids);
    }

    /// Collider2D::GetColliderMaterialState (180c11360): the collider's PhysicsMaterial2D, else its body's, else
    /// Physics2DSettings' default material, else friction 0.4 and bounciness 0.
    fn collider_material_state(&self, prefab: &Prefab, o: usize, col: &crate::assets::components::Collider2D, body_object: Option<usize>) -> (f32, f32) {
        if !col.material.is_null() {
            unimplemented!("{}: a Collider2D PhysicsMaterial2D", prefab.objects[o].name);
        }
        if let Some(b) = body_object {
            if prefab.objects[b].components.iter().any(|(_, c)| matches!(c, Component::Rigidbody2D(r) if !r.material.is_null())) {
                unimplemented!("{}: a Rigidbody2D PhysicsMaterial2D", prefab.objects[b].name);
            }
        }
        match self.default_material2d {
            Some(m) => m,
            None => (0.4, 0.0),
        }
    }

    /// Joint2D::AwakeFromLoad (180c026e0) in instantiate mode: Cleanup (nothing to clean on a clone), then Create
    /// on an active object with the joint enabled; HingeJoint2D::AwakeFromLoad's clamps run only for animation.
    fn joint2d_awake(&mut self, inst: &mut Instance, prefab: &Prefab, o: usize, c: usize, j: &Joint2DData) {
        if inst.objects[o].active && j.enabled {
            match &j.kind {
                JointData::Hinge { .. } => self.hinge_joint2d_create(inst, prefab, o, c, j),
                other => unimplemented!("{}: Joint2D Create for {other:?}", prefab.objects[o].name),
            }
        }
    }

    /// HingeJoint2D::Create (180bfcdc0), AnchoredJoint2D::ConfigureAnchors (180beaf30) and
    /// Joint2D::FinalizeCreateJoint (180c048c0).
    fn hinge_joint2d_create(&mut self, inst: &mut Instance, prefab: &Prefab, o: usize, c: usize, j: &Joint2DData) {
        let JointData::Hinge { use_motor, motor_speed, max_motor_torque, use_limits, lower, upper } = j.kind else { unreachable!() };
        let connected = (!j.connected_body.is_null()).then(|| {
            let r = prefab.objects[o].components[c].0;
            let target = crate::assets::db::ObjRef { file: r.file, path_id: j.connected_body.path_id };
            assert_eq!(j.connected_body.file, 0, "a joint connected outside its prefab");
            prefab.index[&target].0
        });
        if let Some(b) = connected {
            if !inst.objects[b].active {
                return;
            }
        }
        const DEG2RAD: f32 = f32::from_bits(0x3c8e_fa35);
        let (mut lo, mut hi) = (lower * DEG2RAD, upper * DEG2RAD);
        if hi < lo {
            (lo, hi) = (hi, lo);
        }
        // FetchBodyA / FetchBodyB: the attached and connected Rigidbody2D (Create if needed) or the ground body.
        let body_a = inst.bodies.get(&o).copied();
        let body_b = match connected {
            Some(b) => inst.bodies.get(&b).copied(),
            None => Some(self.world.ground_body()),
        };
        let (Some(a), Some(b)) = (body_a, body_b) else { unimplemented!("Joint2D on an object without a created Rigidbody2D") };
        let mut st = *inst.hinges.entry((o, c)).or_insert(HingeState { connected_anchor: j.connected_anchor, ..Default::default() });
        let (angle_a, angle_b) = (self.world.body(a).sweep.a, self.world.body(b).sweep.a);
        let reference_angle = if !st.has_previous_state {
            angle_b - angle_a
        } else {
            // A body angle that wrapped by a turn since Cleanup moves the reference angle with it (within 2°).
            // The second test of each pair reads body angle A, as the native does.
            const PI: f32 = f32::from_bits(0x4049_0fdb);
            const TAU: f32 = f32::from_bits(0x40c9_0fdb);
            const TOLERANCE: f32 = f32::from_bits(0x3d0e_fa36);
            let near = |x: f32, y: f32| TOLERANCE >= (x - y).abs();
            let (prev_a, prev_b) = (st.body_angle_a, st.body_angle_b);
            if prev_a > PI && near(angle_a, prev_a - TAU) {
                st.reference_angle = st.reference_angle + TAU;
            } else if -PI > prev_a && near(angle_a, prev_a + TAU) {
                st.reference_angle = st.reference_angle - TAU;
            }
            if prev_b > PI && near(angle_b, prev_b - TAU) {
                st.reference_angle = st.reference_angle - TAU;
            } else if -PI > prev_a && near(angle_b, prev_b + TAU) {
                st.reference_angle = st.reference_angle + TAU;
            }
            st.reference_angle
        };
        let s = inst.lossy_scale(o);
        let anchor_a = V::new(s[0] * j.anchor[0], s[1] * j.anchor[1]);
        let sb = match connected {
            Some(b) => inst.lossy_scale(b),
            None => [1.0, 1.0, 1.0],
        };
        let anchor_b = if !j.auto_configure_connected_anchor || st.auto_configuration_complete {
            V::new(sb[0] * st.connected_anchor[0], sb[1] * st.connected_anchor[1])
        } else {
            let xa = self.world.body(a).xf;
            let wx = (anchor_a.x * xa.q.c - anchor_a.y * xa.q.s) + xa.p.x;
            let wy = xa.q.s * anchor_a.x + anchor_a.y * xa.q.c + xa.p.y;
            match connected {
                None => {
                    st.connected_anchor = [wx, wy];
                    V::new(wx, wy)
                }
                Some(_) => {
                    let xb = self.world.body(b).xf;
                    let (dx, dy) = (wx - xb.p.x, wy - xb.p.y);
                    let local = V::new(xb.q.c * dx + xb.q.s * dy, xb.q.c * dy - xb.q.s * dx);
                    st.connected_anchor = [local.x / sb[0], local.y / sb[1]];
                    local
                }
            }
        };
        inst.hinges.insert((o, c), st);
        let def = RevoluteDef {
            local_anchor_a: anchor_a,
            local_anchor_b: anchor_b,
            reference_angle,
            enable_limit: use_limits,
            lower_angle: lo,
            upper_angle: hi,
            enable_motor: use_motor,
            motor_speed: motor_speed * DEG2RAD,
            max_motor_torque,
        };
        if check_bodies(Some((a, 0u8)), (b, 0u8)).is_err() {
            return;
        }
        let id = self.world.create_joint(def, a, b, j.enable_collision);
        self.world.wake_joined(a);
        self.world.wake_joined(b);
        inst.joints.insert((o, c), id);
        if j.break_force.is_finite() || j.break_torque.is_finite() {
            unimplemented!("HingeJoint2D break limits on a spawned joint");
        }
        // The world is playing.
        inst.hinges.get_mut(&(o, c)).unwrap().auto_configuration_complete = true;
    }

    /// HingeJoint2D::Cleanup(false) (180bfc5e0) while the world plays: the joint's reference angle and both body
    /// angles are kept for the next Create, then Joint2D::Cleanup destroys the b2 joint.
    fn hinge_joint2d_cleanup(&mut self, inst: &mut Instance, o: usize, c: usize) {
        let Some(id) = inst.joints.remove(&(o, c)) else { return };
        let joint = self.world.joints[id].as_ref().unwrap();
        let (a, b, reference_angle) = (joint.body_a, joint.body_b, joint.revolute().def.reference_angle);
        let st = inst.hinges.get_mut(&(o, c)).unwrap();
        st.reference_angle = reference_angle;
        st.body_angle_a = self.world.body(a).sweep.a;
        st.body_angle_b = self.world.body(b).sweep.a;
        st.has_previous_state = true;
        // Joint2D::Cleanup (180c03500): the auto-configuration is redone by the next Create.
        st.auto_configuration_complete = false;
        self.world.destroy_joint(id);
    }

    /// Rigidbody2D::RecreateJoints (180c22780): Joint2D::ReCreate (180c05570) on each joint of the body, its joint
    /// list from the head: Cleanup, then Create on an active GameObject with the joint enabled.
    fn rigidbody2d_recreate_joints(&mut self, inst: &mut Instance, prefab: &Prefab, o: usize) {
        let body = inst.bodies[&o];
        for (_, id) in self.world.body(body).joints.clone().into_iter().rev() {
            let Some(&(jo, jc)) = inst.joints.iter().find(|(_, j)| **j == id).map(|(k, _)| k) else {
                unimplemented!("RecreateJoints: a joint outside the instance");
            };
            let Component::Joint2D(j) = &prefab.objects[jo].components[jc].1 else { unreachable!() };
            if !matches!(j.kind, JointData::Hinge { .. }) {
                unimplemented!("{}: Joint2D ReCreate for {:?}", prefab.objects[jo].name, j.kind);
            }
            self.hinge_joint2d_cleanup(inst, jo, jc);
            if inst.objects[jo].active && j.enabled {
                self.hinge_joint2d_create(inst, prefab, jo, jc, j);
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::assets::db::AssetDb;
    use crate::assets::mono::Value;

    /// Every rewritten script on the Person prefab loads its serialized fields; prints the runtime-only ones.
    #[test]
    fn person_scripts_load_from_prefab() {
        let Ok(game) = crate::game_assets::find_game(None) else { return };
        let mut db = AssetDb::open(&game).unwrap();
        let catalog = db.find_scripts("CatalogData").unwrap()[0];
        let (_, _, cat) = db.mono(catalog).unwrap().unwrap();
        let mut person = None;
        for item in cat.get("Items").and_then(Value::array).unwrap().to_vec() {
            let asset = db.resolve(catalog.file, item.pptr().unwrap()).unwrap();
            let (_, _, sp) = db.mono(asset).unwrap().unwrap();
            if let Some(p) = sp.get("Prefab").and_then(Value::pptr).and_then(|p| db.resolve(asset.file, p)) {
                if db.game_object(p).unwrap().name == "Person" {
                    person = Some(p);
                }
            }
        }
        let prefab = db.prefab(person.unwrap()).unwrap();
        let mut runtime: std::collections::BTreeMap<String, Vec<&str>> = Default::default();
        for o in &prefab.objects {
            for (_, c) in &o.components {
                let Component::Script { class, fields, .. } = c else { continue };
                let r = match class.as_str() {
                    "PhysicalBehaviour" => load_values(Physical::default().fields(), fields),
                    "LimbBehaviour" => load_values(Limb::default().fields(), fields),
                    "CirculationBehaviour" => load_values(Circulation::default().fields(), fields),
                    "ConnectedNodeBehaviour" => load_values(Node::default().fields(), fields),
                    "SkinMaterialHandler" => load_values(Skin::default().fields(), fields),
                    "PersonBehaviour" => load_values(Person::default().fields(), fields),
                    "GripBehaviour" => load_values(Grip::default().fields(), fields),
                    _ => continue,
                };
                let names = r.unwrap_or_else(|e| panic!("{} {class}: {e}", o.name));
                runtime.insert(class.clone(), names);
            }
        }
        for (class, names) in &runtime {
            eprintln!("{class} runtime: {}", names.join(" "));
        }
        assert_eq!(runtime.len(), 7);
    }

    /// The Person prefab spawned as a recording spawned it: the recorder's spawn point and original ids, the
    /// clone ids from its Human, PersonBehaviour.Awake's recorded root scale, then the recorder's
    /// Physics2D.SyncTransforms. Also returns the asset database and the first snapshot.
    pub(crate) fn spawn_recorded(name: &str) -> Option<(Game, Instance, Prefab, AssetDb, dump::Snapshot)> {
        let game = crate::game_assets::find_game(None).ok()?;
        let dump = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("reference-probe/game/{name}.txt"));
        let mut snaps = dump::snapshots(&dump).ok()?;
        let first = snaps.next().unwrap();
        let mut db = AssetDb::open(&game).unwrap();
        let catalog = db.find_scripts("CatalogData").unwrap()[0];
        let (_, _, cat) = db.mono(catalog).unwrap().unwrap();
        let mut person = None;
        for item in cat.get("Items").and_then(Value::array).unwrap().to_vec() {
            let asset = db.resolve(catalog.file, item.pptr().unwrap()).unwrap();
            let (_, _, sp) = db.mono(asset).unwrap().unwrap();
            if let Some(p) = sp.get("Prefab").and_then(Value::pptr).and_then(|p| db.resolve(asset.file, p)) {
                if db.game_object(p).unwrap().name == "Person" {
                    person = Some(p);
                }
            }
        }
        let prefab = db.prefab(person.unwrap()).unwrap();
        let root = first.of("TransformChain").find(|r| r.name == "Human").unwrap();
        let hex3 = |k: &str| -> [f32; 3] {
            let w: Vec<f32> = root.get(k).unwrap().split(',').map(|w| f32::from_bits(u32::from_str_radix(w, 16).unwrap())).collect();
            [w[0], w[1], w[2]]
        };
        let spawn = hex3("lpos");
        // The recorder's original ids, matched to the prefab in pre-order (component names repeat per object).
        let mut original_ids = HashMap::new();
        let header = &snaps.header;
        let (mut gos, mut comps): (Vec<(&str, i32)>, Vec<(&str, &str, i32)>) = (Vec::new(), Vec::new());
        for l in header.iter().filter_map(|l| l.strip_prefix("orig ")) {
            let (rest, id) = l.rsplit_once(' ').unwrap();
            let id: i32 = id.parse().unwrap();
            if let Some(n) = rest.strip_prefix("GO ") {
                gos.push((n, id));
            } else {
                let (go, ty) = rest.strip_prefix("C ").unwrap().rsplit_once(' ').unwrap();
                comps.push((go, ty, id));
            }
        }
        assert_eq!(gos.len() + comps.len(), 240, "orig lines");
        let mut ci = 0;
        for (i, o) in prefab.objects.iter().enumerate() {
            assert_eq!(gos[i].0, o.name);
            original_ids.insert(o.game_object, gos[i].1);
            for (r, _) in &o.components {
                assert_eq!(comps[ci].0, o.name);
                original_ids.insert(*r, comps[ci].2);
                ci += 1;
            }
        }
        let human_go = root.id + 2;
        let mut g = Game::default();
        g.world = World::new(V::new(-0.0, -9.81));
        g.next_instance_id = human_go;
        let (mut inst, _) = g.instantiate_native(&prefab, spawn, [0.0, 0.0, 0.0, 1.0], &original_ids);
        inst.set_local_scale(0, hex3("lscale"));
        g.sync_transforms(&mut inst, &prefab);
        Some((g, inst, prefab, db, first))
    }

    /// The Person prefab instantiated natively against dump26stabhead2's first snapshot: every limb body's words,
    /// its fixture proxies and its hinge's b2 definition, after PersonBehaviour.Awake's rescale and the recorder's
    /// Physics2D.SyncTransforms.
    #[test]
    fn person_spawn_matches_game() {
        let Some((g, inst, prefab, _, first)) = spawn_recorded("dump26stabhead2") else { return };
        let mut out = Vec::new();
        // Body creation order: the game's list (head newest) restricted to the person.
        let name_of = |b: BodyId| {
            let o = *inst.bodies.iter().find(|(_, x)| **x == b).unwrap().0;
            let c = prefab.objects[o].components.iter().position(|(_, c)| matches!(c, Component::Rigidbody2D(_))).unwrap();
            format!("{}#{}", prefab.objects[o].name, inst.objects[o].components[c])
        };
        let ours: Vec<String> = g.world.body_list.iter().rev().filter(|b| inst.bodies.values().any(|x| x == *b)).map(|&b| name_of(b)).collect();
        let world = first.of("b2World").next().unwrap();
        let theirs: Vec<&str> = world.get("bodies").unwrap().split(';').filter(|n| ours.iter().any(|o| o == n)).collect();
        if ours.len() != 14 || theirs != ours {
            out.push(format!("bodies: game {theirs:?} ours {ours:?}"));
        }
        for (&o, &b) in &inst.bodies {
            let name = name_of(b);
            let Some(e) = first.of("b2Edges").find(|r| r.name == name) else {
                out.push(format!("{name}: no b2Edges"));
                continue;
            };
            let mut game: Vec<u32> = e.get("body").unwrap().split(',').map(|w| u32::from_str_radix(w, 16).unwrap()).collect();
            game[1] &= 0xffff;
            let mut ours = dump::physics::body_words(g.world.body(b));
            // PhysicalBehaviour.Start sets collisionDetectionMode (the bullet flag) from the user preference.
            (game[1], ours[1]) = (game[1] & !8, ours[1] & !8);
            for (k, (gw, ow)) in game.iter().zip(&ours).enumerate() {
                if gw != ow {
                    out.push(format!("{name} body.w{k}: game {gw:08x} ours {ow:08x}"));
                }
            }
            // The body's joint list from the head: RecreateJoints reorders it.
            let joint_names: Vec<String> = g.world.body(b).joints.iter().rev().map(|&(_, j)| {
                let (&(jo, jc), _) = inst.joints.iter().find(|(_, x)| **x == j).unwrap();
                format!("{}#{}", prefab.objects[jo].name, inst.objects[jo].components[jc])
            }).collect();
            let theirs: Vec<&str> = e.get("joints").unwrap().split(';').filter(|n| !n.is_empty()).collect();
            if theirs != joint_names {
                out.push(format!("{name} joints: game {theirs:?} ours {joint_names:?}"));
            }
            // Fixture proxies: the AABB and fat AABB of each fixture's single proxy.
            for (&(fo, fc), fixtures) in &inst.fixtures {
                if fo != o {
                    continue;
                }
                let fname = format!("{}#{}:0", prefab.objects[fo].name, inst.objects[fo].components[fc]);
                let Some(rec) = first.of("b2Fixture").find(|r| r.name == fname) else {
                    out.push(format!("{fname}: no b2Fixture"));
                    continue;
                };
                let theirs: Vec<u32> = rec.get("proxies").unwrap().split_once(',').unwrap().1.split(',').map(|w| u32::from_str_radix(w, 16).unwrap()).collect();
                let f = g.world.fixtures[fixtures[0]].as_ref().unwrap();
                let (aabb, proxy) = f.proxies[0];
                let fat = g.world.broad_phase.tree.fat_aabb(proxy);
                let ours: Vec<u32> = [aabb.lower.x, aabb.lower.y, aabb.upper.x, aabb.upper.y, fat.lower.x, fat.lower.y, fat.upper.x, fat.upper.y].map(f32::to_bits).to_vec();
                if theirs != ours {
                    out.push(format!("{fname} proxies: game {theirs:08x?} ours {ours:08x?}"));
                }
            }
        }
        let game_joints = first.of("b2RevoluteJoint").filter(|r| inst.objects.iter().any(|o| o.components.contains(&r.id))).count();
        if inst.joints.len() != 13 || game_joints != 13 {
            out.push(format!("hinges: game {game_joints} ours {}", inst.joints.len()));
        }
        // Hinges: anchors (w0-3), motor (w8 low byte, w9, w10), limits (w11 low byte, w13, w14), reference angle (w12).
        for (&(o, c), &id) in &inst.joints {
            let name = format!("{}", prefab.objects[o].name);
            let Some(rec) = first.of("b2RevoluteJoint").find(|r| r.name == name && r.id == inst.objects[o].components[c]) else {
                out.push(format!("{name}: no b2RevoluteJoint"));
                continue;
            };
            let w: Vec<u32> = rec.get("joint").unwrap().split(',').map(|w| u32::from_str_radix(w, 16).unwrap()).collect();
            let r = g.world.joints[id].as_ref().unwrap().revolute();
            let pairs = [
                ("anchorA.x", w[0], r.def.local_anchor_a.x.to_bits()),
                ("anchorA.y", w[1], r.def.local_anchor_a.y.to_bits()),
                ("anchorB.x", w[2], r.def.local_anchor_b.x.to_bits()),
                ("anchorB.y", w[3], r.def.local_anchor_b.y.to_bits()),
                ("enableMotor", w[8] & 0xff, r.def.enable_motor as u32),
                // maxMotorTorque: LimbBehaviour sets Joint.motor from MotorStrength (queue 0x14).
                ("motorSpeed", w[10], r.def.motor_speed.to_bits()),
                ("enableLimit", w[11] & 0xff, r.def.enable_limit as u32),
                ("referenceAngle", w[12], r.def.reference_angle.to_bits()),
                ("lower", w[13], r.def.lower_angle.to_bits()),
                ("upper", w[14], r.def.upper_angle.to_bits()),
            ];
            for (k, gw, ow) in pairs {
                if gw != ow {
                    out.push(format!("{name} joint.{k}: game {gw:08x} ours {ow:08x}"));
                }
            }
        }
        assert!(out.is_empty(), "{} mismatches:
{}", out.len(), out.join("
"));
    }
}
