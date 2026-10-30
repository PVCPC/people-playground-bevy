//! Object.Instantiate of a prefab from the game's own data: InstantiateObject (180634340) clones the hierarchy,
//! then AwakeAndActivateClonedObjects (1806321d0) wakes the clones through an AwakeFromLoadQueue in
//! kInstantiateOrCreateFromCodeAwakeFromLoad mode. This module is the native part of that: the clones' instance
//! ids and Transforms, and the Rigidbody2D, Collider2D and HingeJoint2D AwakeFromLoads that build the b2 world
//! state.

use super::*;
use crate::assets::components::{ColliderShape, Joint2D as Joint2DData, JointKind as JointData, Rigidbody2D as RigidbodyData};
use crate::assets::db::{AssetDb, Component, ObjRef, Prefab, PrefabObject};
use crate::b2::revolute::RevoluteDef;
use crate::b2::shape::{box_collider_polygon, capsule_collider, edge_collider_chain, polygon_collider_polygons, Circle, ColliderMatrix};
use crate::b2::prismatic::SliderJoint2d;
use crate::b2::wheel_joint::WheelJoint2d;
use crate::b2::weld::{check_bodies, BodyFrame, Connected, FixedJoint2d};
use crate::b2::world::{BodyDef, BodyId, BodyType, ColliderRef, FixtureDef, FixtureId, JointId, JointKind, RigidbodyMass, RigidbodyRef, Shape};
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
    /// GameObject.activeSelf.
    pub self_active: bool,
    /// The object has been active in the hierarchy before: its scripts have been awoken.
    pub activated: bool,
}

/// An instantiated prefab: the clones in prefab order, and the b2 objects their components made.
#[derive(Clone, Debug, Default)]
pub struct Instance {
    /// The prefab it was cloned from.
    pub prefab: std::sync::Arc<Prefab>,
    pub objects: Vec<Clone>,
    /// Rigidbody2D bodies by object.
    pub bodies: HashMap<usize, BodyId>,
    /// Collider2D fixtures by (object, component).
    pub fixtures: HashMap<(usize, usize), Vec<FixtureId>>,
    /// HingeJoint2D joints by (object, component).
    pub joints: HashMap<(usize, usize), JointId>,
    /// The HingeJoint2Ds' native state by (object, component).
    pub hinges: HashMap<(usize, usize), HingeState>,
    /// The FixedJoint2Ds', SliderJoint2Ds' and SpringJoint2Ds' native state by (object, component).
    pub anchored: HashMap<(usize, usize), AnchoredState>,
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

/// A FixedJoint2D, SliderJoint2D, SpringJoint2D, DistanceJoint2D or WheelJoint2D component's native state: the AnchoredJoint2D/Joint2D fields
/// every kind shares, the kind's own, and the slot in `Game.anchored_joints` the break-limit walk reads.
#[derive(Clone, Debug)]
pub struct AnchoredState {
    pub base: FixedJoint2d,
    pub slider: Option<SliderJoint2d>,
    pub spring: Option<SpringState>,
    pub distance: Option<DistanceState>,
    pub wheel: Option<WheelJoint2d>,
    pub slot: Option<usize>,
}

/// DistanceJoint2D +0x98..+0x9d: distance, maxDistanceOnly and autoConfigureDistance.
#[derive(Clone, Copy, Debug)]
pub struct DistanceState {
    pub auto_configure_distance: bool,
    pub distance: f32,
    pub max_distance_only: bool,
}

/// SpringJoint2D +0x98..+0xa4: distance, damping ratio, frequency and autoConfigureDistance.
#[derive(Clone, Copy, Debug)]
pub struct SpringState {
    pub auto_configure_distance: bool,
    pub distance: f32,
    pub damping_ratio: f32,
    pub frequency: f32,
}

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
    /// The slot in `Game.anchored_joints` the break-limit walk reads (a prefab hinge no script links).
    pub slot: Option<usize>,
}

/// AnchoredJoint2D / Joint2D serialized state of a FixedJoint2D or SliderJoint2D component.
fn anchored_base(j: &Joint2DData, damping_ratio: f32, frequency: f32) -> FixedJoint2d {
    FixedJoint2d {
        anchor: V::new(j.anchor[0], j.anchor[1]),
        connected_anchor: V::new(j.connected_anchor[0], j.connected_anchor[1]),
        auto_configure_connected_anchor: j.auto_configure_connected_anchor,
        anchor_configured: false,
        damping_ratio,
        frequency,
        stored_reference_angle: None,
        enable_collision: j.enable_collision,
        break_force: j.break_force,
        break_torque: j.break_torque,
    }
}

/// The native state a FixedJoint2D, SliderJoint2D, SpringJoint2D, DistanceJoint2D or WheelJoint2D component has before its
/// first Create: the serialized fields.
pub(super) fn anchored_initial_state(j: &Joint2DData) -> AnchoredState {
    let (base, slider, spring, distance, wheel) = match j.kind {
        JointData::Fixed { damping_ratio, frequency } => (anchored_base(j, damping_ratio, frequency), None, None, None, None),
        JointData::Wheel { damping_ratio, frequency, angle, use_motor, motor_speed, max_motor_torque } => (
            anchored_base(j, 0.0, 0.0),
            None,
            None,
            None,
            Some(WheelJoint2d { damping_ratio, frequency, angle, use_motor, motor_speed, max_motor_torque }),
        ),
        JointData::Distance { auto_configure_distance, distance, max_distance_only } => {
            (anchored_base(j, 0.0, 0.0), None, None, Some(DistanceState { auto_configure_distance, distance, max_distance_only }), None)
        }
        JointData::Slider { auto_configure_angle, angle, use_motor, motor_speed, max_motor_force, use_limits, lower, upper } => (
            anchored_base(j, 0.0, 0.0),
            Some(SliderJoint2d { angle, motor_speed, max_motor_torque: max_motor_force, lower, upper, use_motor, use_limits, auto_configure_angle }),
            None,
            None,
            None,
        ),
        JointData::Spring { auto_configure_distance, distance, damping_ratio, frequency } => {
            (anchored_base(j, 0.0, 0.0), None, Some(SpringState { auto_configure_distance, distance, damping_ratio, frequency }), None, None)
        }
        _ => unreachable!(),
    };
    AnchoredState { base, slider, spring, distance, wheel, slot: None }
}

/// The prefab object a Joint2D's connectedBody points at.
pub(super) fn joint_connected_object(prefab: &Prefab, o: usize, c: usize, j: &Joint2DData) -> Option<usize> {
    (!j.connected_body.is_null()).then(|| {
        let r = prefab.objects[o].components[c].0;
        let target = ObjRef { file: r.file, path_id: j.connected_body.path_id };
        assert_eq!(j.connected_body.file, 0, "a joint connected outside its prefab");
        prefab.index[&target].0
    })
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

/// level1's first instance id (its object 0): a scene object's id is this plus twice its index in the file.
const LEVEL1_FIRST_ID: i32 = 66952;

/// The Main scene's Physics2D natives: the ToolController's body and every scene hierarchy that holds
/// Physics2D components (root first, loaded ids), for `set_active` and the scripts' references.
pub struct SceneNatives {
    pub tool_body: BodyId,
    pub roots: Vec<Instance>,
}

/// A scene GameObject's Transform.
fn scene_transform(db: &AssetDb, go: ObjRef) -> Result<crate::assets::classes::Transform, String> {
    let g = db.game_object(go)?;
    let t = g.components.iter().filter_map(|&c| db.resolve(go.file, c)).find(|&c| matches!(db.class(c), 4 | 224)).ok_or_else(|| format!("{}: no Transform", g.name))?;
    crate::assets::classes::Transform::read(db.data(t))
}

/// Whether a hierarchy holds a Rigidbody2D, Collider2D or Joint2D on an object active in it.
fn has_physics(db: &AssetDb, go: ObjRef) -> Result<bool, String> {
    let g = db.game_object(go)?;
    if !g.active {
        return Ok(false);
    }
    for c in g.components.iter().filter_map(|&c| db.resolve(go.file, c)) {
        let class = db.class(c);
        if class == 50 || crate::assets::components::Collider2D::CLASSES.contains(&class) || crate::assets::components::Joint2D::CLASSES.contains(&class) {
            return Ok(true);
        }
    }
    for child in scene_transform(db, go)?.children {
        let t = db.resolve(go.file, child).ok_or("unresolved child transform")?;
        let child_go = db.resolve(go.file, crate::assets::classes::Transform::read(db.data(t))?.game_object).ok_or("child without GameObject")?;
        if has_physics(db, child_go)? {
            return Ok(true);
        }
    }
    Ok(false)
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
    /// Instantiate up to the last AwakeFromLoadQueue: the clones and queues 0..0x13 (every native component).
    /// The clone ids go down from `next_instance_id` by 2 in prefab pre-order, each GameObject before its
    /// components. `original_ids` are the prefab objects' own instance ids (GameObject or component ObjRef -> id,
    /// `AssetDb::resource_load_order`), whose order is the remap's and so the queues'.
    /// Returns the MonoBehaviours of queue 0x14, (object, component), in the order AwakeFromLoadQueue::AwakeFromLoad
    /// (1809112c0) wakes them: sorted by SortBehaviourItemByExecutionOrderAndReverseInstanceID (180915860),
    /// script execution order first, then the higher clone id.
    pub fn instantiate_native(&mut self, prefab: &Prefab, position: [f32; 3], rotation: [f32; 4], original_ids: &HashMap<ObjRef, i32>) -> (Instance, Vec<(usize, usize)>) {
        let inst = self.clone_prefab(prefab, position, rotation, |g, o| (g.allocate_instance_id(), o.components.iter().map(|_| g.allocate_instance_id()).collect()));
        let mut insts = [inst];
        let behaviours = self.awake_instances(&mut insts, original_ids).pop().unwrap();
        let [inst] = insts;
        (inst, behaviours)
    }

    /// The clones of `prefab`, with the ids `ids` gives each object (GameObject id, component ids).
    fn clone_prefab(&mut self, prefab: &Prefab, position: [f32; 3], rotation: [f32; 4], mut ids: impl FnMut(&mut Game, &PrefabObject) -> (i32, Vec<i32>)) -> Instance {
        let mut inst = Instance { prefab: std::sync::Arc::new(prefab.clone()), ..Default::default() };
        for (i, o) in prefab.objects.iter().enumerate() {
            let (game_object, components) = ids(self, o);
            // GameObject.layer, which the overlap queries filter by.
            self.world.layers.insert(game_object, o.layer as i32);
            let Some((_, Component::Transform(t))) = o.components.first() else { panic!("{}: first component not a Transform", o.name) };
            let v3 = |v: [f32; 3]| [v[0], v[1], v[2], 0.0];
            let mut trs = Trs { t: v3(t.local_position), q: t.local_rotation, s: v3(t.local_scale) };
            // The root takes the given pose (Instantiate's SetPositionAndRotation on a parentless clone).
            if i == 0 {
                trs.t = v3(position);
                trs.q = rotation;
            }
            let active = o.active && o.parent.map_or(true, |p| inst.objects[p].active);
            inst.objects.push(Clone { game_object, components, trs, parent: o.parent, active, self_active: o.active, activated: active });
        }
        // A clone copies its original's interests (CopyTransformSubhierarchy); a prefab asset has no Physics2D
        // interest, so the clones start with none and nothing changed.
        inst.interest = vec![0; inst.objects.len()];
        inst.changed = vec![0; inst.objects.len()];
        inst
    }

    /// The AwakeFromLoadQueue of `insts` together: queues 0..0x13 (every native component) in one pass, each
    /// queue in the order of the originals' ids. Returns each instance's queue 0x14 MonoBehaviours.
    fn awake_instances(&mut self, insts: &mut [Instance], original_ids: &HashMap<ObjRef, i32>) -> Vec<Vec<(usize, usize)>> {
        // The remap: every original (GameObject and component) with its clone, sorted by original id.
        let mut items: Vec<(i32, usize, usize, Option<usize>)> = Vec::new();
        for (n, inst) in insts.iter().enumerate() {
            for (i, o) in inst.prefab.objects.iter().enumerate() {
                items.push((original_ids[&o.game_object], n, i, None));
                for (c, (r, _)) in o.components.iter().enumerate() {
                    items.push((original_ids[r], n, i, Some(c)));
                }
            }
        }
        items.sort_by_key(|x| x.0);
        let mut queues: Vec<Vec<(usize, usize, Option<usize>)>> = vec![Vec::new(); 0x15];
        for &(_, n, o, c) in &items {
            queues[queue_index(c.map(|c| &insts[n].prefab.objects[o].components[c].1))].push((n, o, c));
        }
        for items in &queues[..0x14] {
            for &(n, o, c) in items {
                let Some(c) = c else { continue };
                let prefab = insts[n].prefab.clone();
                match &prefab.objects[o].components[c].1 {
                    Component::Rigidbody2D(rb) => self.rigidbody2d_awake(&mut insts[n], &prefab, o, c, rb),
                    Component::Collider2D(_) => self.collider2d_awake(&mut insts[n], &prefab, o, c),
                    Component::Joint2D(j) => self.joint2d_awake(&mut insts[n], &prefab, o, c, j),
                    // Transform, SpriteRenderer and the unread classes make no physics state.
                    _ => {}
                }
            }
        }
        insts
            .iter()
            .enumerate()
            .map(|(n, inst)| {
                let mut behaviours: Vec<(usize, usize)> = queues[0x14].iter().filter(|q| q.0 == n).map(|&(_, o, c)| (o, c.unwrap())).collect();
                behaviours.sort_by_key(|&(o, c)| {
                    let Component::Script { execution_order, .. } = &inst.prefab.objects[o].components[c].1 else { unreachable!() };
                    (*execution_order, std::cmp::Reverse(inst.objects[o].components[c]))
                });
                behaviours
            })
            .collect()
    }

    /// SceneManager.LoadScene("Main") up to its native AwakeFromLoad: the Rigidbody2Ds and Collider2Ds of the
    /// scene's hierarchies, all in one AwakeFromLoadQueue in the order of their loaded ids (the scene's first id
    /// plus twice the object's index in the file). Everything here exists before the first MonoBehaviour's Awake
    /// (MapLoaderBehaviour.Awake instantiates the map after it).
    pub fn load_scene_natives(&mut self, db: &mut AssetDb) -> Result<SceneNatives, String> {
        let file = db.file_index("level1").ok_or("no level1")?;
        // The scene's PhysicsScene2D, made first, owns the static ground body the body-less colliders go on.
        self.world.ground_body();
        let index: HashMap<i64, i32> = db.files[file].objects.iter().enumerate().map(|(i, o)| (o.path_id, i as i32)).collect();
        let id = |r: ObjRef| LEVEL1_FIRST_ID + 2 * index[&r.path_id];
        let gos: Vec<i64> = db.files[file].objects.iter().filter(|o| o.class_id == 1).map(|o| o.path_id).collect();
        let mut roots = Vec::new();
        let mut original_ids = HashMap::new();
        for path_id in gos {
            let go = ObjRef { file, path_id };
            if !scene_transform(db, go)?.father.is_null() || !has_physics(db, go)? {
                continue;
            }
            let prefab = db.prefab(go)?;
            for o in &prefab.objects {
                original_ids.insert(o.game_object, id(o.game_object));
                original_ids.extend(o.components.iter().map(|&(r, _)| (r, id(r))));
            }
            let t = scene_transform(db, go)?;
            roots.push(self.clone_prefab(&prefab, t.local_position, t.local_rotation, |_, o| (id(o.game_object), o.components.iter().map(|&(r, _)| id(r)).collect())));
        }
        self.awake_instances(&mut roots, &original_ids);
        let tool = roots.iter().find(|r| r.prefab.objects[0].name == "ToolController").ok_or("level1 has no ToolController with Physics2D components")?;
        let tool_body = *tool.bodies.get(&0).ok_or("the ToolController has no Rigidbody2D body")?;
        Ok(SceneNatives { tool_body, roots })
    }

    /// Instantiate's id allocation: each new object takes the next id below the last.
    pub fn allocate_instance_id(&mut self) -> i32 {
        let id = self.next_instance_id;
        self.next_instance_id -= 2;
        id
    }

    /// GameObject.SetActive on clone `o` (GameObject::SetSelfActive, 180589740). Deactivating an active object is
    /// GameObject::Deactivate (180587a20) through ActivateAwakeRecursivelyInternal (1805865f0), which goes through
    /// the children first, in Transform order, then deactivates the object's own components in component order;
    /// activating queues the same walk's components on an AwakeFromLoadQueue. An object already inactive in the
    /// hierarchy only takes the flag. Every Collider2D and Rigidbody2D of the subtree is (de)activated here;
    /// the MonoBehaviours it passes (their OnDisable, or OnEnable) are returned, (object, component), in Unity's
    /// order.
    pub fn set_active(&mut self, inst: &mut Instance, o: usize, active: bool) -> Vec<(usize, usize)> {
        if active {
            return self.activate(inst, o);
        }
        inst.objects[o].self_active = false;
        let prefab = inst.prefab.clone();
        let mut scripts = Vec::new();
        self.deactivate_recursively(inst, &prefab, o, &mut scripts);
        scripts
    }

    /// GameObject.SetActive(true): the flag, then (with the parents active) ActivateAwakeRecursivelyInternal queues
    /// the components of the subtree, children first, and AwakeFromLoadQueue::AwakeFromLoad wakes the queues in
    /// order, each in the order its items were queued; the MonoBehaviours (queue 0x14) by execution order and,
    /// within it, the higher instance id first. Returns those, (object, component).
    fn activate(&mut self, inst: &mut Instance, o: usize) -> Vec<(usize, usize)> {
        inst.objects[o].self_active = true;
        if inst.objects[o].active || inst.objects[o].parent.is_some_and(|p| !inst.objects[p].active) {
            return Vec::new();
        }
        let prefab = inst.prefab.clone();
        let mut queued = Vec::new();
        self.activate_recursively(inst, &prefab, o, &mut queued);
        let mut scripts = Vec::new();
        for queue in 0..=0x14 {
            for &(d, c) in &queued {
                let comp = &prefab.objects[d].components[c].1;
                if queue_index(Some(comp)) != queue {
                    continue;
                }
                match comp {
                    Component::Rigidbody2D(rb) => self.rigidbody2d_activate(inst, &prefab, d, c, rb),
                    Component::Collider2D(_) => self.collider2d_awake(inst, &prefab, d, c),
                    Component::Joint2D(_) => unimplemented!("Joint2D::AwakeFromLoad on GameObject.SetActive(true)"),
                    Component::Script { .. } => scripts.push((d, c)),
                    // Transform, SpriteRenderer and the unread classes hold no Physics2D state.
                    _ => {}
                }
            }
        }
        scripts.sort_by_key(|&(d, c)| {
            let Component::Script { execution_order, .. } = &prefab.objects[d].components[c].1 else { unreachable!() };
            (*execution_order, std::cmp::Reverse(inst.objects[d].components[c]))
        });
        for &(d, c) in &scripts {
            let Component::Script { class, .. } = &prefab.objects[d].components[c].1 else { unreachable!() };
            super::link::check_rewritten(class, false);
            if !inst.objects[d].activated {
                unimplemented!("{}: GameObject.SetActive(true) on an object whose scripts never ran Awake and Start", prefab.objects[d].name);
            }
        }
        scripts
    }

    fn activate_recursively(&mut self, inst: &mut Instance, prefab: &Prefab, o: usize, queued: &mut Vec<(usize, usize)>) {
        inst.objects[o].active = true;
        for &child in &prefab.objects[o].children {
            if inst.objects[child].self_active {
                self.activate_recursively(inst, prefab, child, queued);
            }
        }
        queued.extend((0..prefab.objects[o].components.len()).map(|c| (o, c)));
    }

    fn deactivate_recursively(&mut self, inst: &mut Instance, prefab: &Prefab, o: usize, scripts: &mut Vec<(usize, usize)>) {
        if !inst.objects[o].active {
            return;
        }
        inst.objects[o].active = false;
        for &child in &prefab.objects[o].children {
            self.deactivate_recursively(inst, prefab, child, scripts);
        }
        for c in 0..prefab.objects[o].components.len() {
            match &prefab.objects[o].components[c].1 {
                Component::Collider2D(_) => self.collider2d_deactivate(inst, o, c),
                Component::Rigidbody2D(_) => self.rigidbody2d_deactivate(inst, prefab, o),
                Component::Script { .. } => scripts.push((o, c)),
                Component::Joint2D(_) => unimplemented!("Joint2D::Deactivate"),
                // Transform, SpriteRenderer and the unread classes hold no Physics2D state.
                _ => {}
            }
        }
    }

    /// Collider2D::Deactivate (180c0fc00) -> Cleanup (180c0d940) with kColliderDestroy and the callbacks off: the
    /// collider's Physics2D.IgnoreCollision pairs go, its fixtures are destroyed last first, the body's mass is
    /// reset and its contact records are dropped without callbacks.
    fn collider2d_deactivate(&mut self, inst: &mut Instance, o: usize, c: usize) {
        let id = inst.objects[o].components[c];
        self.world.ignored.retain(|&(a, b)| a != id && b != id);
        if let Some(fixtures) = inst.fixtures.remove(&(o, c)).filter(|f| !f.is_empty()) {
            let body = self.world.fixture(fixtures[0]).body;
            for &f in fixtures.iter().rev() {
                self.world.destroy_fixture(body, f);
            }
            self.world.reset_mass_data(body);
        }
        self.world.contacts2d.destroy_contacts(id);
        inst.colliders.remove(&(o, c));
        // Collider2D::RemoveFromManager.
        inst.interest[o] &= !CHANGE_COLLIDER;
    }

    /// Rigidbody2D::Deactivate (180c1ec40) -> Cleanup (180c1dd20) with DeactivateOperation 0: the body goes
    /// (its colliders were cleaned before it), the Rigidbody2D leaves the scene's lists and its Transform
    /// interest.
    fn rigidbody2d_deactivate(&mut self, inst: &mut Instance, prefab: &Prefab, o: usize) {
        let Some(&body) = inst.bodies.get(&o) else { return };
        if inst.descendants(o).iter().any(|d| inst.bodies.contains_key(d)) {
            unimplemented!("Rigidbody2D::Cleanup: CheckForDrivenByParentRigidbody on a child Rigidbody2D");
        }
        if !self.world.body(body).fixtures.is_empty() {
            unimplemented!("{}: Rigidbody2D::Cleanup with colliders still attached", prefab.objects[o].name);
        }
        if !self.world.body(body).joints.is_empty() {
            unimplemented!("{}: Rigidbody2D::Cleanup with joints", prefab.objects[o].name);
        }
        // m_Body becomes null: a PhysicalBehaviour's Rigidbody keeps the component's own mass fields.
        let b = self.world.body(body);
        let (mass, props) = (b.rigidbody, [b.linear_damping, b.angular_damping, b.gravity_scale]);
        for rb in self.physicals.iter_mut().map(|ph| &mut ph.rigidbody).chain(self.node_rigidbodies.iter_mut().map(|n| &mut n.rigidbody)) {
            if rb.body == body && rb.detached.is_none() {
                rb.detached = Some(mass);
                rb.detached_props = props;
                rb.movement.listed = false;
            }
        }
        self.world.destroy_body(body);
        inst.bodies.remove(&o);
        inst.interest[o] &= !(CHANGE_RB_T | CHANGE_RB_R | CHANGE_RB_S | CHANGE_RB_ANIM);
        self.rigidbody_hierarchy_changed = true;
    }

    /// Rigidbody2D::AwakeFromLoad (180c1c8e0) in instantiate mode: Create on an active object without a body.
    fn rigidbody2d_awake(&mut self, inst: &mut Instance, prefab: &Prefab, o: usize, c: usize, rb: &RigidbodyData) {
        if inst.objects[o].active && !inst.bodies.contains_key(&o) {
            self.rigidbody2d_create(inst, prefab, o, c, rb, None);
        }
    }

    /// Rigidbody2D::AwakeFromLoad (180c1c8e0) in kActivateAwakeFromLoad mode: Create on an active object without a
    /// body, from the component's own fields (its owner keeps them while the body is gone) and the owner's Transform;
    /// the scene's lists get the Rigidbody2D and the nearest ancestor Rigidbody2D drives it.
    fn rigidbody2d_activate(&mut self, inst: &mut Instance, prefab: &Prefab, o: usize, c: usize, rb: &RigidbodyData) {
        if inst.bodies.contains_key(&o) {
            return;
        }
        let Some(owner) = self.rigidbody_owner(inst.objects[o].components[c]) else {
            unimplemented!("{}: Rigidbody2D re-created without a PhysicalBehaviour or Transform node owner", prefab.objects[o].name);
        };
        let mut fields = rb.clone();
        let r = self.owner_rigidbody(owner);
        let mass = r.detached.expect("a Rigidbody2D without a body is detached");
        (fields.body_type, fields.collision_detection, fields.interpolate) = (r.bodyType, r.collisionDetectionMode, r.interpolation);
        [fields.linear_drag, fields.angular_drag, fields.gravity_scale] = r.detached_props;
        let pose = self.owner_pose(owner);
        self.rigidbody2d_create(inst, prefab, o, c, &fields, Some((pose, mass)));
        let body = inst.bodies[&o];
        let driven_by = self.rigidbody2d_driven_by(inst, prefab, o, fields.body_type);
        let r = self.owner_rigidbody_mut(owner);
        r.body = body;
        r.detached = None;
        r.movement.listed = false;
        self.set_driven_by(owner, driven_by);
        // AddRigidbodyUpdates: m_RigidbodyHierarchyChanged.
        self.rigidbody_hierarchy_changed = true;
    }

    /// The owner of the Rigidbody2D component `id`: its PhysicalBehaviour, or its Transform node's body.
    pub(super) fn rigidbody_owner(&self, id: i32) -> Option<BodyOwner> {
        self.physicals
            .iter()
            .position(|p| p.rigidbody.component == id)
            .map(BodyOwner::Physical)
            .or_else(|| self.node_rigidbodies.iter().position(|n| n.rigidbody.component == id).map(BodyOwner::Node))
    }

    pub(super) fn owner_rigidbody(&self, o: BodyOwner) -> &Rigidbody {
        match o {
            BodyOwner::Physical(p) => &self.physicals[p].rigidbody,
            BodyOwner::Node(k) => &self.node_rigidbodies[k].rigidbody,
        }
    }

    pub(super) fn owner_rigidbody_mut(&mut self, o: BodyOwner) -> &mut Rigidbody {
        match o {
            BodyOwner::Physical(p) => &mut self.physicals[p].rigidbody,
            BodyOwner::Node(k) => &mut self.node_rigidbodies[k].rigidbody,
        }
    }

    fn set_driven_by(&mut self, o: BodyOwner, driven_by: Option<BodyId>) {
        match o {
            BodyOwner::Physical(p) => self.physicals[p].driven_by = driven_by,
            BodyOwner::Node(k) => self.node_rigidbodies[k].driven_by = driven_by,
        }
    }

    /// Transform.position/rotation of a Rigidbody2D's owner.
    fn owner_pose(&self, o: BodyOwner) -> ([f32; 3], [f32; 4]) {
        match o {
            BodyOwner::Physical(p) => (self.physicals[p].transform.position, self.physicals[p].transform.rotation),
            BodyOwner::Node(k) => self.node_world(self.node_rigidbodies[k].node),
        }
    }

    /// Rigidbody2D::CheckForDrivenByParentRigidbody(null) (180c1dc80): unless static, the nearest ancestor with a
    /// Rigidbody2D drives it.
    fn rigidbody2d_driven_by(&self, inst: &Instance, prefab: &Prefab, o: usize, body_type: i32) -> Option<BodyId> {
        if body_type == 2 {
            return None;
        }
        let mut up = inst.objects[o].parent;
        while let Some(t) = up {
            if prefab.objects[t].components.iter().any(|(_, x)| matches!(x, Component::Rigidbody2D(_))) {
                return Some(*inst.bodies.get(&t).unwrap_or_else(|| unimplemented!("{}: driven by a Rigidbody2D without a body", prefab.objects[o].name)));
            }
            up = inst.objects[t].parent;
        }
        None
    }

    /// Rigidbody2D::SetBodyType (180c23120) through the `bodyType` setter (force off) of the Rigidbody2D `owner`
    /// has on object `o`. The type is stored; a changed type moves the Rigidbody2D in the scene's update lists
    /// (RemoveRigidbodyUpdates, AddRigidbodyUpdates: the movement list is rebuilt after the next step); a Rigidbody2D
    /// with a body then has its colliders' contacts flagged and the colliders re-created (RecreateCollider),
    /// b2Body::SetType, the movement state reset to the body pose, the contacts recalculated and the driving
    /// ancestor looked up again.
    pub fn rigidbody2d_set_body_type(&mut self, inst: &mut Instance, owner: BodyOwner, o: usize, body_type: i32) {
        let prefab = inst.prefab.clone();
        let rb = self.owner_rigidbody_mut(owner);
        let old = rb.bodyType;
        rb.bodyType = body_type;
        if body_type == old || rb.detached.is_some() {
            return;
        }
        rb.movement.listed = false;
        let body = rb.body;
        self.rigidbody_hierarchy_changed = true;
        // GetAttachedColliders sorts the Collider2D pointers; with one collider the order cannot differ.
        let mut colliders: Vec<(usize, usize)> = inst.fixtures.iter().filter(|(_, f)| f.iter().any(|&f| self.world.fixture(f).body == body)).map(|(&k, _)| k).collect();
        colliders.sort_unstable();
        if colliders.len() > 1 {
            unimplemented!("{}: Rigidbody2D.bodyType with several attached colliders (GetAttachedColliders pointer order)", prefab.objects[o].name);
        }
        // FlagAllColliderContactsForRecreate.
        for &(co, cc) in &colliders {
            self.world.contacts2d.flag_for_recreate(inst.objects[co].components[cc]);
        }
        for &(co, cc) in &colliders {
            self.collider2d_recreate(inst, &prefab, co, cc);
        }
        self.world.set_body_type(body, match body_type {
            0 => BodyType::Dynamic,
            1 => BodyType::Kinematic,
            _ => BodyType::Static,
        });
        // RigidbodyMovementState2D: previous and current become the body pose; no interpolation. The linear and
        // angular move states (MovePosition, MoveRotation) are never set.
        let m = &mut self.owner_rigidbody_mut(owner).movement;
        (m.previous, m.current, m.interpolating) = (m.body, m.body, false);
        self.rigidbody2d_recalculate_contacts(inst, &prefab, body);
        let driven_by = self.rigidbody2d_driven_by(inst, &prefab, o, body_type);
        self.set_driven_by(owner, driven_by);
    }

    /// Rigidbody2D::Create (180c1e6b0).
    /// `restore` is a re-creation after SetActive(true): the owner's pose, and the component's mass fields.
    fn rigidbody2d_create(&mut self, inst: &mut Instance, _prefab: &Prefab, o: usize, c: usize, rb: &RigidbodyData, restore: Option<(([f32; 3], [f32; 4]), RigidbodyMass)>) {
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
            rigidbody: restore.map_or(RigidbodyMass { use_auto_mass: rb.use_auto_mass, mass: rb.mass, inertia: None, center_of_mass: None }, |r| r.1),
            user: Some(RigidbodyRef { id: inst.objects[o].components[c], game_object: inst.objects[o].game_object }),
            ..Default::default()
        };
        if inst.objects[o].active {
            let (p, q) = restore.map_or_else(|| (inst.position(o), inst.rotation(o)), |r| r.0);
            def.position = V::new(p[0], p[1]);
            def.angle = body_angle(q);
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
        // GetAttachedColliders (180c1f350) sorts the Collider2D pointers. The order cannot show: each refilter only
        // flags contacts, touches proxies (UpdatePairs sorts and dedups the pairs), wakes the body and remixes
        // materials, so any order gives the same world.
        let mut attached: Vec<(usize, usize)> = inst.fixtures.iter().filter(|(_, f)| f.iter().any(|&f| self.world.fixture(f).body == body)).map(|(&k, _)| k).collect();
        attached.sort_unstable();
        for &(o, c) in &attached {
            let Component::Collider2D(col) = &prefab.objects[o].components[c].1 else { unreachable!() };
            if col.enabled && !col.used_by_effector {
                self.world.refilter_collider(inst.objects[o].components[c]);
                if self.world.body(body).kind == BodyType::Static {
                    // Collider2D::WakeAllBodyContacts (180c17960).
                    self.world.wake_contact_bodies(body);
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
                // ColliderTilingExtension::IsTilingOn (180bfd370): auto-tiling over a sprite drawn sliced or tiled.
                if *auto_tiling && tiling.draw_mode != 0 {
                    unimplemented!("BoxCollider2D auto-tiling (ScheduleGenerateBoxTilingShape)");
                }
                box_collider_polygon(V::new(col.offset[0], col.offset[1]), V::new(size[0], size[1]), *edge_radius, &matrix).map(Shape::Polygon).into_iter().collect()
            }
            // CircleCollider2D::PrepareShapes (180c13210): the offset through the matrix, and the radius times the
            // larger absolute lossy scale axis, clamped to 0.0001..1e6.
            ColliderShape::Circle { radius } => {
                let s = inst.lossy_scale(o);
                let scale = if s[1].abs() > s[0].abs() { s[1].abs() } else { s[0].abs() };
                let r = scale * radius;
                let r = if r <= 0.0001 { 0.0001 } else { f32::min(1e6, r) };
                vec![Shape::Circle(Circle { radius: r, p: matrix.apply(V::new(col.offset[0], col.offset[1])) })]
            }
            ColliderShape::Polygon { tiling, auto_tiling, paths } => {
                // ColliderTilingExtension::IsTilingOn (180bfd370)
                if *auto_tiling && tiling.draw_mode != 0 {
                    unimplemented!("PolygonCollider2D auto-tiling");
                }
                polygon_collider_polygons(paths, &m, V::new(col.offset[0], col.offset[1])).into_iter().map(Shape::Polygon).collect()
            }
            ColliderShape::Edge { edge_radius, points, adjacent_start, adjacent_end } => {
                edge_collider_chain(points, &m, V::new(col.offset[0], col.offset[1]), *edge_radius, *adjacent_start, *adjacent_end).map(Shape::Chain).into_iter().collect()
            }
            ColliderShape::Capsule { size, direction } => capsule_collider(*size, *direction, V::new(col.offset[0], col.offset[1]), &m).map(Shape::Capsule).into_iter().collect(),
        };
        if shapes.is_empty() {
            return;
        }
        let (friction, restitution) = self.collider_material_state(prefab, o, c, body_object);
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
    fn collider_material_state(&self, prefab: &Prefab, o: usize, c: usize, body_object: Option<usize>) -> (f32, f32) {
        let body_material = body_object.and_then(|b| (0..prefab.objects[b].components.len()).find_map(|k| prefab.materials.get(&(b, k)).filter(|_| matches!(prefab.objects[b].components[k].1, Component::Rigidbody2D(_)))));
        match prefab.materials.get(&(o, c)).or(body_material) {
            Some(&m) => m,
            None => self.default_material2d.unwrap_or((0.4, 0.0)),
        }
    }

    /// Joint2D::AwakeFromLoad (180c026e0) in instantiate mode: Cleanup (nothing to clean on a clone), then Create
    /// on an active object with the joint enabled; HingeJoint2D::AwakeFromLoad's clamps run only for animation.
    fn joint2d_awake(&mut self, inst: &mut Instance, prefab: &Prefab, o: usize, c: usize, j: &Joint2DData) {
        if inst.objects[o].active && j.enabled {
            match &j.kind {
                JointData::Hinge { .. } => self.hinge_joint2d_create(inst, prefab, o, c, j),
                JointData::Fixed { .. } | JointData::Slider { .. } | JointData::Spring { .. } | JointData::Distance { .. } | JointData::Wheel { .. } => self.anchored_joint2d_create(inst, prefab, o, c, j),
                other => unimplemented!("{}: Joint2D Create for {other:?}", prefab.objects[o].name),
            }
        }
    }

    /// HingeJoint2D::Create (180bfcdc0), AnchoredJoint2D::ConfigureAnchors (180beaf30) and
    /// Joint2D::FinalizeCreateJoint (180c048c0).
    fn hinge_joint2d_create(&mut self, inst: &mut Instance, prefab: &Prefab, o: usize, c: usize, j: &Joint2DData) {
        let JointData::Hinge { use_motor, motor_speed, max_motor_torque, use_limits, lower, upper } = j.kind else { unreachable!() };
        let connected = joint_connected_object(prefab, o, c, j);
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
        // FinalizeCreateJoint: AddBreakLimitsUpdate (to the back of the list, or off it when both are infinite).
        let st = inst.hinges.get_mut(&(o, c)).unwrap();
        let slot = *st.slot.get_or_insert_with(|| {
            self.anchored_joints.push(AnchoredJoint { break_force: j.break_force, break_torque: j.break_torque, joint: None, hinge: None });
            self.anchored_joints.len() - 1
        });
        // The world is playing.
        st.auto_configuration_complete = true;
        // A hinge a script links (`Game::hinges`) keeps its place in the list as BreakJoint::Hinge.
        let key = match self.anchored_joints[slot].hinge {
            Some(h) => BreakJoint::Hinge(h),
            None => {
                self.anchored_joints[slot].joint = Some(id);
                BreakJoint::Anchored(slot)
            }
        };
        self.add_break_limits_update(key, j.break_force, j.break_torque);
    }

    /// FixedJoint2D::Create (180c04160), SliderJoint2D::Create (180c04400), SpringJoint2D::Create (180bfd060) and
    /// DistanceJoint2D::Create (180bebd50) and WheelJoint2D::Create (180bec010)
    /// with Joint2D::FinalizeCreateJoint (180c048c0): the definition the native side builds from the component,
    /// bodies A (attached) and B (connected, or the ground body).
    fn anchored_joint2d_create(&mut self, inst: &mut Instance, prefab: &Prefab, o: usize, c: usize, j: &Joint2DData) {
        let connected = joint_connected_object(prefab, o, c, j);
        // WheelJoint2D::Create returns without a connected Rigidbody2D.
        if connected.is_none() && matches!(j.kind, JointData::Wheel { .. }) {
            return;
        }
        if let Some(b) = connected {
            if !inst.objects[b].active {
                return;
            }
        }
        let body_a = inst.bodies.get(&o).copied();
        let body_b = match connected {
            Some(b) => inst.bodies.get(&b).copied(),
            None => Some(self.world.ground_body()),
        };
        let (Some(a), Some(b)) = (body_a, body_b) else { unimplemented!("Joint2D on an object without a created Rigidbody2D") };
        let AnchoredState { mut base, mut slider, spring, mut distance, wheel, mut slot } = inst.anchored.remove(&(o, c)).unwrap_or_else(|| anchored_initial_state(j));
        let mut spring = spring;
        let frame = |g: &Self, id: BodyId| {
            let bd = g.world.body(id);
            BodyFrame { position: bd.xf.p, q: bd.xf.q, angle: bd.sweep.a }
        };
        let s = inst.lossy_scale(o);
        let scale_a = V::new(s[0], s[1]);
        let frame_a = frame(self, a);
        let conn = match connected {
            None => Connected::World,
            Some(cb) => {
                let sb = inst.lossy_scale(cb);
                Connected::Body { frame: frame(self, b), lossy_scale: V::new(sb[0], sb[1]) }
            }
        };
        let def: JointKind = match (slider.as_mut(), spring.as_mut(), distance.as_mut()) {
            _ if wheel.is_some() => {
                // WheelJoint2D::Create: no auto-configuration, the anchors scaled by each transform's lossy scale.
                let scale_b = match &conn {
                    Connected::World => V::new(1.0, 1.0),
                    Connected::Body { lossy_scale, .. } => *lossy_scale,
                };
                let (anchor, connected_anchor) = (V::new(base.anchor.x, base.anchor.y), V::new(base.connected_anchor.x, base.connected_anchor.y));
                wheel.unwrap().create_def(anchor, connected_anchor, scale_a, scale_b).into()
            }
            (Some(sl), _, _) => sl.create_def(&mut base, scale_a, &frame_a, &conn).into(),
            (None, Some(sp), _) => {
                // SpringJoint2D::Create: the anchors, then the distance between them when it autoconfigures.
                let (anchor_a, anchor_b) = base.configure_anchors(scale_a, &frame_a, &conn);
                if sp.auto_configure_distance && !base.anchor_configured {
                    let world = |f: &BodyFrame, p: V| V::new(p.x * f.q.c - p.y * f.q.s + f.position.x, f.q.s * p.x + p.y * f.q.c + f.position.y);
                    let frame_b = match conn {
                        Connected::World => BodyFrame::default(),
                        Connected::Body { frame, .. } => frame,
                    };
                    let (wa, wb) = (world(&frame_a, anchor_a), world(&frame_b, anchor_b));
                    let (dx, dy) = (wb.x - wa.x, wb.y - wa.y);
                    let d = (dx * dx + dy * dy).sqrt();
                    sp.distance = if 0.005 > d { 0.005 } else { f32::min(1_000_000.0, d) };
                }
                crate::b2::distance_joint::DistanceDef { local_anchor_a: anchor_a, local_anchor_b: anchor_b, length: sp.distance, frequency_hz: sp.frequency, damping_ratio: sp.damping_ratio }.into()
            }
            (None, None, Some(dj)) => {
                // DistanceJoint2D::Create: the same anchors and autoconfigured distance; maxDistanceOnly makes it a
                // b2RopeJoint of that maximum length, otherwise a rigid b2DistanceJoint (frequency 0, damping ratio 1).
                let (anchor_a, anchor_b) = base.configure_anchors(scale_a, &frame_a, &conn);
                if dj.auto_configure_distance && !base.anchor_configured {
                    let world = |f: &BodyFrame, p: V| V::new(p.x * f.q.c - p.y * f.q.s + f.position.x, f.q.s * p.x + p.y * f.q.c + f.position.y);
                    let frame_b = match conn {
                        Connected::World => BodyFrame::default(),
                        Connected::Body { frame, .. } => frame,
                    };
                    let (wa, wb) = (world(&frame_a, anchor_a), world(&frame_b, anchor_b));
                    let (dx, dy) = (wb.x - wa.x, wb.y - wa.y);
                    let d = (dx * dx + dy * dy).sqrt();
                    dj.distance = if 0.005 > d { 0.005 } else { f32::min(1_000_000.0, d) };
                }
                if dj.max_distance_only {
                    crate::b2::rope_joint::RopeDef { local_anchor_a: anchor_a, local_anchor_b: anchor_b, max_length: dj.distance }.into()
                } else {
                    crate::b2::distance_joint::DistanceDef { local_anchor_a: anchor_a, local_anchor_b: anchor_b, length: dj.distance, frequency_hz: 0.0, damping_ratio: 1.0 }.into()
                }
            }
            (None, None, None) => base.create_def(scale_a, &frame_a, &conn).into(),
        };
        if check_bodies(Some((a, 0u8)), (b, 0u8)).is_ok() {
            let id = self.world.create_joint(def, a, b, base.enable_collision);
            self.world.wake_joined(a);
            self.world.wake_joined(b);
            inst.joints.insert((o, c), id);
            // FinalizeCreateJoint: AddBreakLimitsUpdate (to the back of the list, or off it when both are infinite).
            let slot = *slot.get_or_insert_with(|| {
                self.anchored_joints.push(AnchoredJoint { break_force: base.break_force, break_torque: base.break_torque, joint: None, hinge: None });
                self.anchored_joints.len() - 1
            });
            self.anchored_joints[slot].joint = Some(id);
            self.add_break_limits_update(BreakJoint::Anchored(slot), base.break_force, base.break_torque);
            base.finish_create();
        }
        inst.anchored.insert((o, c), AnchoredState { base, slider, spring, distance, wheel, slot });
    }

    /// FixedJoint2D::Cleanup(false) (180c03480) / SliderJoint2D::Cleanup (180c03550): the live joint's reference
    /// angle is kept for the next Create, then Joint2D::Cleanup destroys the b2 joint.
    fn anchored_joint2d_cleanup(&mut self, inst: &mut Instance, o: usize, c: usize) {
        let Some(id) = inst.joints.remove(&(o, c)) else { return };
        let st = inst.anchored.get_mut(&(o, c)).unwrap();
        if st.spring.is_some() || st.distance.is_some() || st.wheel.is_some() {
            // Joint2D::Cleanup alone: the auto-configuration is redone by the next Create.
            st.base.anchor_configured = false;
        } else {
            st.base.cleanup(false, Some(self.world.joints[id].as_ref().unwrap().reference_angle()));
        }
        if let Some(slot) = st.slot {
            self.anchored_joints[slot].joint = None;
        }
        self.world.destroy_joint(id);
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
        if let Some(slot) = st.slot {
            self.anchored_joints[slot].joint = None;
        }
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
                // A Fixed/SliderJoint2D HingeJointLimitAutofixBehaviour took over from the instance.
                let f = self.fixed_joints.iter().position(|f| f.joint == Some(id)).unwrap_or_else(|| unimplemented!("RecreateJoints: a joint outside the instance"));
                self.fixed_joint_recreate(f);
                continue;
            };
            let Component::Joint2D(j) = &prefab.objects[jo].components[jc].1 else { unreachable!() };
            if matches!(j.kind, JointData::Fixed { .. } | JointData::Slider { .. } | JointData::Spring { .. } | JointData::Distance { .. } | JointData::Wheel { .. }) {
                self.anchored_joint2d_cleanup(inst, jo, jc);
                if inst.objects[jo].active && j.enabled {
                    self.anchored_joint2d_create(inst, prefab, jo, jc, j);
                }
                continue;
            }
            if !matches!(j.kind, JointData::Hinge { .. }) {
                unimplemented!("{}: Joint2D ReCreate for {:?}", prefab.objects[jo].name, j.kind);
            }
            // ReCreate takes the component's current properties: a linked hinge's, which scripts may have set.
            let mut j = j.clone();
            let linked = self.hinges.iter().position(|h| h.joint == Some(id));
            if let Some(h) = linked {
                let h = &self.hinges[h];
                j.kind = JointData::Hinge { use_motor: h.useMotor, motor_speed: h.motor[0], max_motor_torque: h.motor[1], use_limits: h.useLimits, lower: h.limits[0], upper: h.limits[1] };
                (j.enabled, j.anchor, j.break_force, j.break_torque) = (h.enabled, h.anchor, h.breakForce, h.breakTorque);
            }
            self.hinge_joint2d_cleanup(inst, jo, jc);
            if inst.objects[jo].active && j.enabled {
                self.hinge_joint2d_create(inst, prefab, jo, jc, &j);
            }
            if let Some(h) = linked {
                self.hinges[h].joint = inst.joints.get(&(jo, jc)).copied();
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
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

    /// The Person's objects in the order of their original instance ids: the recorder's `orig` lines and
    /// `resource_load_order` agree.
    #[test]
    fn resource_load_order_matches_recording() {
        let Some((_, _, prefab, db, _)) = spawn_recorded("dump31spawnrand") else { return };
        let dump = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("reference-probe/game/dump31spawnrand.txt");
        let mut snaps = dump::snapshots(&dump).unwrap();
        snaps.next().unwrap();
        let recorded = recorded_original_ids(&snaps.header, &prefab);
        let order = db.resource_load_order().unwrap();
        let mut objects: Vec<ObjRef> = recorded.keys().copied().collect();
        objects.sort_by_key(|o| recorded[o]);
        let mut by_order = objects.clone();
        by_order.sort_by_key(|o| order[o]);
        assert_eq!(objects, by_order);
    }

    /// The recorder's `orig` lines (original instance ids), matched to the prefab in pre-order (component names
    /// repeat per object).
    fn recorded_original_ids(header: &[String], prefab: &Prefab) -> HashMap<ObjRef, i32> {
        let mut original_ids = HashMap::new();
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
        original_ids
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
        let original_ids = db.resource_load_order().unwrap();
        let human_go = root.id + 2;
        let mut g = Game::default();
        g.apply_physics2d_settings(&Game::load_physics2d_settings(&db).unwrap());
        g.next_instance_id = human_go;
        let (mut inst, _) = g.instantiate_native(&prefab, spawn, [0.0, 0.0, 0.0, 1.0], &original_ids);
        inst.set_local_scale(0, hex3("lscale"));
        g.sync_transforms(&mut inst, &prefab);
        Some((g, inst, prefab, db, first))
    }

    /// The recorder's name for an instance body: the object's name (spaces as underscores) and its Rigidbody2D id.
    fn body_name(inst: &Instance, prefab: &Prefab, b: BodyId) -> String {
        let o = *inst.bodies.iter().find(|(_, x)| **x == b).unwrap().0;
        let c = prefab.objects[o].components.iter().position(|(_, c)| matches!(c, Component::Rigidbody2D(_))).unwrap();
        format!("{}#{}", prefab.objects[o].name.replace(' ', "_"), inst.objects[o].components[c])
    }

    /// Every instance body against the first snapshot: its words, joint list and fixture proxies.
    ///
    /// `full` adds the body mass words, the fixtures' words and shapes and the proxy ids: for a person the scripts
    /// (PhysicalBehaviour.Start's mass and material) and the other objects' proxies change those.
    fn compare_bodies(g: &Game, inst: &Instance, prefab: &Prefab, first: &dump::Snapshot, full: bool, out: &mut Vec<String>) {
        for (&o, &b) in &inst.bodies {
            let name = body_name(inst, prefab, b);
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
            // Mass, inverse mass, axis, inertia, inverse inertia, damping and gravity scale (not the sleep time).
            let bw = g.world.body(b);
            let mass_words: Vec<u32> = [bw.mass, bw.inv_mass, bw.axis.x, bw.axis.y, bw.i, bw.inv_i, bw.linear_damping, bw.angular_damping, bw.gravity_scale].map(f32::to_bits).to_vec();
            let game_mass: Vec<u32> = e.get("mass").unwrap().split(',').take(9).map(|w| u32::from_str_radix(w, 16).unwrap()).collect();
            if full && game_mass != mass_words {
                out.push(format!("{name} mass: game {game_mass:08x?} ours {mass_words:08x?}"));
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
                let fname = format!("{}#{}:0", prefab.objects[fo].name.replace(' ', "_"), inst.objects[fo].components[fc]);
                let Some(rec) = first.of("b2Fixture").find(|r| r.name == fname) else {
                    out.push(format!("{fname}: no b2Fixture"));
                    continue;
                };
                let word = |k: &str| -> Vec<u32> { rec.get(k).unwrap().split(',').map(|w| u32::from_str_radix(w, 16).unwrap()).collect() };
                let fx = g.world.fixtures[fixtures[0]].as_ref().unwrap();
                let (gf, gs) = (word("fixture"), word("shape"));
                let md = fx.mass_data;
                // The filter word's top byte is padding. Friction and restitution are the scripts' materials: a
                // person's are not applied here.
                let mut ours_f = [md.mass, md.center.x, md.center.y, md.inertia, md.area, fx.density, fx.friction, fx.restitution].map(f32::to_bits).to_vec();
                ours_f.extend([fx.filter.category_bits as u32 | (fx.filter.mask_bits as u32) << 16, fx.filter.group_index as u16 as u32 | (fx.is_sensor as u32) << 16]);
                let game_f: Vec<u32> = gf[..8].iter().copied().chain([gf[9], gf[10] & 0xff_ffff]).collect();
                if full && game_f != ours_f {
                    out.push(format!("{fname} fixture: game {game_f:08x?} ours {ours_f:08x?}"));
                }
                let Shape::Polygon(poly) = &fx.shape else { unimplemented!("map fixture shape") };
                let mut ours_s = vec![2, poly.radius.to_bits(), poly.centroid.x.to_bits(), poly.centroid.y.to_bits()];
                ours_s.extend(poly.vertices[..poly.count].iter().flat_map(|v| [v.x.to_bits(), v.y.to_bits()]));
                let mut game_s = gs[..4 + 2 * poly.count].to_vec();
                game_s.extend(&gs[20..20 + 2 * poly.count]);
                ours_s.extend(poly.normals[..poly.count].iter().flat_map(|v| [v.x.to_bits(), v.y.to_bits()]));
                ours_s.push(poly.count as u32);
                game_s.push(gs[36]);
                if full && game_s != ours_s {
                    out.push(format!("{fname} shape: game {game_s:08x?} ours {ours_s:08x?}"));
                }
                let theirs: Vec<u32> = rec.get("proxies").unwrap().split_once(',').unwrap().1.split(',').map(|w| u32::from_str_radix(w, 16).unwrap()).collect();
                let f = g.world.fixtures[fixtures[0]].as_ref().unwrap();
                let (aabb, proxy) = f.proxies[0];
                let fat = g.world.broad_phase.tree.fat_aabb(proxy);
                let ours: Vec<u32> = [aabb.lower.x, aabb.lower.y, aabb.upper.x, aabb.upper.y, fat.lower.x, fat.lower.y, fat.upper.x, fat.upper.y].map(f32::to_bits).to_vec();
                if theirs != ours {
                    out.push(format!("{fname} proxies: game {theirs:08x?} ours {ours:08x?}"));
                }
                let game_proxy = rec.get("proxies").unwrap().split_once(',').unwrap().0;
                if full && game_proxy != proxy.to_string() {
                    out.push(format!("{fname} proxy id: game {game_proxy} ours {proxy}"));
                }
            }
        }
    }

    /// The Person prefab instantiated natively against dump26stabhead2's first snapshot: every limb body's words,
    /// its fixture proxies and its hinge's b2 definition, after PersonBehaviour.Awake's rescale and the recorder's
    /// Physics2D.SyncTransforms.
    #[test]
    fn person_spawn_matches_game() {
        let Some((g, inst, prefab, _, first)) = spawn_recorded("dump26stabhead2") else { return };
        let mut out = Vec::new();
        // Body creation order: the game's list (head newest) restricted to the person.
        let name_of = |b: BodyId| body_name(&inst, &prefab, b);
        let ours: Vec<String> = g.world.body_list.iter().rev().filter(|b| inst.bodies.values().any(|x| x == *b)).map(|&b| name_of(b)).collect();
        let world = first.of("b2World").next().unwrap();
        let theirs: Vec<&str> = world.get("bodies").unwrap().split(';').filter(|n| ours.iter().any(|o| o == n)).collect();
        if ours.len() != 14 || theirs != ours {
            out.push(format!("bodies: game {theirs:?} ours {ours:?}"));
        }
        compare_bodies(&g, &inst, &prefab, &first, false, &mut out);
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

    /// The Default map prefab instantiated natively (WORLD's child at the origin) against dump26stabhead2's first
    /// snapshot: the map bodies' creation order, words and fixture proxies.
    #[test]
    fn map_spawn_matches_game() {
        let Ok(game) = crate::game_assets::find_game(None) else { return };
        let dump = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("reference-probe/game/dump26stabhead2.txt");
        let Ok(mut snaps) = dump::snapshots(&dump) else { return };
        let first = snaps.next().unwrap();
        let mut db = AssetDb::open(&game).unwrap();
        let mut map = None;
        for m in db.find_scripts("Map").unwrap() {
            let (_, _, v) = db.mono(m).unwrap().unwrap();
            let p = v.get("Prefab").and_then(Value::pptr).and_then(|p| db.resolve(m.file, p)).unwrap();
            if db.game_object(p).unwrap().name == "Default map" {
                map = Some(p);
            }
        }
        let prefab = db.prefab(map.unwrap()).unwrap();
        let mut g = Game::default();
        g.apply_physics2d_settings(&Game::load_physics2d_settings(&db).unwrap());
        // LoadScene's natives come first: the ToolController's body and the two resize handles' fixtures.
        let mut scene = g.load_scene_natives(&mut db).unwrap();
        // The recorded clone of the map root: its GameObject is the first id.
        g.next_instance_id = -12588;
        let (mut inst, _) = g.instantiate_native(&prefab, [0.0; 3], [0.0, 0.0, 0.0, 1.0], &db.resource_load_order().unwrap());
        g.sync_transforms(&mut inst, &prefab);
        // ActivateIfTutorial.Awake: SetActive(UserPreferenceManager.Current.ShowTutorial), off in the recording.
        let tutorial = prefab.objects.iter().position(|o| o.name == "tutorial").unwrap();
        g.set_active(&mut inst, tutorial, false);
        // PhysicalBehaviour.Start -> SetProperties: rigidbody.sharedMaterial = Properties.PhysicMaterial (the prefab
        // Rigidbodies have none), whose friction and bounciness Rigidbody2D::SetMaterial gives the body's fixtures.
        for (o, ob) in prefab.objects.iter().enumerate() {
            for (_, c) in &ob.components {
                let Component::Script { class, fields, .. } = c else { continue };
                let (true, Some(&b)) = (class == "PhysicalBehaviour", inst.bodies.get(&o)) else { continue };
                let props = db.resolve(ob.game_object.file, fields.get("Properties").and_then(Value::pptr).unwrap()).unwrap();
                let (_, _, props) = db.mono(props).unwrap().unwrap();
                let material = db.resolve(ob.game_object.file, props.get("PhysicMaterial").and_then(Value::pptr).unwrap()).map(|m| crate::assets::components::PhysicsMaterial2D::read(db.data(m)).unwrap());
                if let Some(m) = material {
                    for f in g.world.body(b).fixtures.clone() {
                        let fixture = g.world.fixtures[f].as_mut().unwrap();
                        (fixture.friction, fixture.restitution) = (m.friction, m.bounciness);
                    }
                }
            }
        }
        // ResizeHandles.Update (the first frame): no targets, SetActive(false).
        let handles = scene.roots.iter_mut().find(|r| r.prefab.objects[0].name == "Resizing handles").unwrap();
        g.set_active(handles, 0, false);
        // The recorded snapshot is long after the first steps: Solve clears the created-asleep flag, and an awake
        // kinematic body's sweep angle becomes angle + dt * 0, then so does its previous angle.
        for _ in 0..2 {
            g.world.step(0.02, 32, 32);
        }
        let mut out = Vec::new();
        let tool = scene.roots.iter().find(|r| r.bodies.values().any(|&b| b == scene.tool_body)).unwrap();
        let name = |b: BodyId| match b {
            b if Some(b) == g.world.ground => "ground".to_string(),
            b if b == scene.tool_body => body_name(tool, &tool.prefab, b),
            b => body_name(&inst, &prefab, b),
        };
        let ours: Vec<String> = g.world.body_list.iter().rev().map(|&b| name(b)).collect();
        let world = first.of("b2World").next().unwrap();
        let theirs: Vec<&str> = world.get("bodies").unwrap().split(';').filter(|n| ours.iter().any(|o| o == n)).collect();
        if theirs != ours {
            out.push(format!("bodies: game {theirs:?} ours {ours:?}"));
        }
        compare_bodies(&g, &inst, &prefab, &first, true, &mut out);
        compare_bodies(&g, tool, &tool.prefab, &first, true, &mut out);
        assert!(out.is_empty(), "{} mismatches:
{}", out.len(), out.join("
"));
    }

    /// The broad-phase leaf ids the scene's, the map's and the two removals leave free: the knife and the person
    /// spawned after them take the game's recorded proxy ids.
    #[test]
    fn spawned_proxy_ids_follow_scene_and_map() {
        let Some((_, _, person, mut db, first)) = spawn_recorded("dump26stabhead2") else { return };
        let mut map = None;
        let mut knife = None;
        let catalog = db.find_scripts("CatalogData").unwrap()[0];
        let (_, _, cat) = db.mono(catalog).unwrap().unwrap();
        for item in cat.get("Items").and_then(Value::array).unwrap().to_vec() {
            let asset = db.resolve(catalog.file, item.pptr().unwrap()).unwrap();
            let (_, _, sp) = db.mono(asset).unwrap().unwrap();
            if let Some(p) = sp.get("Prefab").and_then(Value::pptr).and_then(|p| db.resolve(asset.file, p)) {
                if db.game_object(p).unwrap().name == "Knife" {
                    knife = Some(p);
                }
            }
        }
        for m in db.find_scripts("Map").unwrap() {
            let (_, _, v) = db.mono(m).unwrap().unwrap();
            let p = v.get("Prefab").and_then(Value::pptr).and_then(|p| db.resolve(m.file, p)).unwrap();
            if db.game_object(p).unwrap().name == "Default map" {
                map = Some(p);
            }
        }
        let (map, knife) = (db.prefab(map.unwrap()).unwrap(), db.prefab(knife.unwrap()).unwrap());
        let ids = db.resource_load_order().unwrap();
        let mut g = Game::default();
        g.apply_physics2d_settings(&Game::load_physics2d_settings(&db).unwrap());
        let mut scene = g.load_scene_natives(&mut db).unwrap();
        g.next_instance_id = -12588;
        let (mut m, _) = g.instantiate_native(&map, [0.0; 3], [0.0, 0.0, 0.0, 1.0], &ids);
        let tutorial = map.objects.iter().position(|o| o.name == "tutorial").unwrap();
        g.set_active(&mut m, tutorial, false);
        g.set_active(scene.roots.iter_mut().find(|r| r.prefab.objects[0].name == "Resizing handles").unwrap(), 0, false);
        // The recorded knife collider is -62000; the clones' ids descend in pre-order, each GameObject first.
        let mut pos = 0;
        'find: for o in &knife.objects {
            pos += 1;
            for (_, c) in &o.components {
                if matches!(c, Component::Collider2D(_)) {
                    break 'find;
                }
                pos += 1;
            }
        }
        g.next_instance_id = -62000 + 2 * pos;
        let (k, _) = g.instantiate_native(&knife, [0.0; 3], [0.0, 0.0, 0.0, 1.0], &ids);
        let root = first.of("TransformChain").find(|r| r.name == "Human").unwrap();
        g.next_instance_id = root.id + 2;
        let (p, _) = g.instantiate_native(&person, [0.0; 3], [0.0, 0.0, 0.0, 1.0], &ids);
        let mut out = Vec::new();
        for (inst, prefab) in [(&k, &knife), (&p, &person)] {
            for (&(o, c), f) in &inst.fixtures {
                let name = format!("{}#{}:0", prefab.objects[o].name.replace(' ', "_"), inst.objects[o].components[c]);
                let Some(rec) = first.of("b2Fixture").find(|r| r.name == name) else {
                    out.push(format!("{name}: no b2Fixture"));
                    continue;
                };
                let ours = g.world.fixtures[f[0]].as_ref().unwrap().proxies[0].1.to_string();
                let game = rec.get("proxies").unwrap().split_once(',').unwrap().0;
                if game != ours {
                    out.push(format!("{name}: game {game} ours {ours}"));
                }
            }
        }
        assert!(out.is_empty(), "{} mismatches:
{}", out.len(), out.join("
"));
    }
}
