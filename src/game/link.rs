//! An instantiated prefab's MonoBehaviours as the Game's component arenas. Each script gets its arena entry with
//! its serialized fields, and the rewrite's links (arena indices) stand for its component references: the
//! serialized ones resolve through the prefab, the GetComponent ones through the object's components. Runtime
//! state stays at its field initializer for Awake/OnEnable/Start to set.

use super::circulation::Circulation;
use super::limb::{GoreString, Limb};
use super::misc::{AutofixJoint, AutofixJoints, GForce, Grip, Node, Skin};
use super::person::{LimbPose, Person, RagdollPose};
use super::physical::{Physical, PhysicalProperties, SharpAxis};
use super::water::Water;
use super::spawn::{anchored_initial_state, joint_connected_object, load_values, Instance};
use super::*;
use crate::assets::classes::PPtr;
use crate::assets::components::{JointKind, PhysicsMaterial2D, Sprite};
use crate::assets::db::{AssetDb, Component, ObjRef, Prefab, PrefabObject};
use crate::assets::mono::Value;
use crate::game_assets::{Curve, CurveCache};
use crate::unity::transform::Node as TransformNode;
use std::collections::HashMap;

/// AudioSource's class id.
const AUDIO_SOURCE: i32 = 82;

/// The arena index of each prefab object's scripts, in prefab (GetComponentsInChildren) order.
#[derive(Default)]
struct Arenas {
    physical: HashMap<usize, usize>,
    limb: HashMap<usize, usize>,
    grip: HashMap<usize, usize>,
    hinge: HashMap<(usize, usize), usize>,
    /// Transform nodes by object (the Rigidbody-less parents of bodies).
    node: HashMap<usize, usize>,
}

/// What `link_instance` built: the Person's index (a root PersonBehaviour), and the arena index of each limb,
/// PhysicalBehaviour and MapLightBehaviour object and of each Rigidbody-less Transform above a body.
pub struct Linked {
    pub person: Option<usize>,
    pub limbs: HashMap<usize, usize>,
    pub nodes: HashMap<usize, usize>,
    pub physicals: HashMap<usize, usize>,
    /// Game::hinges index by (object, component).
    pub hinges: HashMap<(usize, usize), usize>,
    pub map_lights: HashMap<usize, usize>,
    /// Game::waters index by object.
    pub waters: HashMap<usize, usize>,
    /// Game::node_rigidbodies index by object.
    pub node_rigidbodies: HashMap<usize, usize>,
    /// The PlayAudioAtRandom on the instance's parent (`ambient_audio`), which MapConfig.Reinitialise configures.
    pub parent_audio: Option<usize>,
}

/// The clone's Transform as a physical object's.
fn pose(inst: &Instance, o: usize) -> Pose {
    let trs = inst.objects[o].trs;
    Pose { position: inst.position(o), rotation: inst.rotation(o), localScale: [trs.s[0], trs.s[1], trs.s[2]], local: Some(([trs.t[0], trs.t[1], trs.t[2]], trs.q)) }
}

fn script<'a>(prefab: &'a Prefab, o: usize, class: &str) -> Option<(usize, bool, &'a Value)> {
    prefab.objects[o].components.iter().enumerate().find_map(|(c, (_, comp))| match comp {
        Component::Script { class: k, enabled, fields, .. } if k == class => Some((c, *enabled, fields)),
        _ => None,
    })
}

fn field<'a>(v: &'a Value, name: &str) -> Result<&'a Value, String> {
    v.get(name).ok_or_else(|| format!("no serialized {name}"))
}

fn float(v: &Value, name: &str) -> Result<f32, String> {
    field(v, name)?.f32().ok_or_else(|| format!("{name}: not a float"))
}

fn array<'a>(v: &'a Value, name: &str) -> Result<&'a [Value], String> {
    field(v, name)?.array().ok_or_else(|| format!("{name}: not an array"))
}

fn string(v: &Value, name: &str) -> Result<String, String> {
    match field(v, name)? {
        Value::Str(s) => Ok(s.clone()),
        x => Err(format!("{name}: not a string: {x:?}")),
    }
}

fn floats<const N: usize>(v: &Value) -> Result<[f32; N], String> {
    let mut out = [0.0; N];
    out.set(v)?;
    Ok(out)
}

/// A serialized Bounds as center then extents.
fn bounds(v: &Value) -> Result<[f32; 6], String> {
    let [cx, cy, cz] = floats::<3>(field(v, "m_Center")?)?;
    let [ex, ey, ez] = floats::<3>(field(v, "m_Extent")?)?;
    Ok([cx, cy, cz, ex, ey, ez])
}

/// A LineSegment (A, B).
fn line(v: &Value) -> Result<[f32; 4], String> {
    let [ax, ay] = floats::<2>(field(v, "A")?)?;
    let [bx, by] = floats::<2>(field(v, "B")?)?;
    Ok([ax, ay, bx, by])
}

fn curve(v: &Value) -> Result<Curve, String> {
    let mut keys = Vec::new();
    for k in array(v, "m_Curve")? {
        if field(k, "weightedMode")?.i64() != Some(0) {
            unimplemented!("a weighted AnimationCurve key");
        }
        keys.push([float(k, "time")?, float(k, "value")?, float(k, "inSlope")?, float(k, "outSlope")?]);
    }
    let int = |n: &str| -> Result<i32, String> { Ok(field(v, n)?.i64().ok_or(n.to_string())? as i32) };
    Ok(Curve { keys, pre: int("m_PreInfinity")?, post: int("m_PostInfinity")? })
}

/// Sprite.bounds (Sprite::GetBounds): the sprite rect around its pivot in units, 0.1 deep either side.
/// A UnityEngine.Bounds field as the 2D box the map scripts use.
fn map_bounds(v: &Value) -> Result<Bounds, String> {
    let b = bounds(field(v, "BoundingBox")?)?;
    Ok(Bounds { center: [b[0], b[1]], extents: [b[3], b[4]] })
}

fn sprite_bounds(s: &Sprite) -> ([f32; 3], [f32; 3]) {
    let (w, h) = (s.rect[2] / s.pixels_to_units, s.rect[3] / s.pixels_to_units);
    let (ex, ey) = (w * 0.5, h * 0.5);
    ([ex - s.pivot[0] * w, ey - s.pivot[1] * h, 0.0], [ex, ey, 0.1])
}

/// SpriteRenderer::GetSpriteBounds (1806e6840): Simple takes the sprite's bounds, mirrored by the flips
/// (Sprite::GetBounds negates the corners); Sliced and Tiled the renderer's size around the pivot, the flips as
/// signs, encapsulated from an empty AABB with z -0.1 and 0.1.
fn renderer_bounds(s: &Sprite, r: &crate::assets::components::SpriteRenderer) -> ([f32; 3], [f32; 3]) {
    if r.draw_mode == 0 {
        let (mut c, e) = sprite_bounds(s);
        if r.flip_x {
            c[0] = -c[0];
        }
        if r.flip_y {
            c[1] = -c[1];
        }
        return (c, e);
    }
    let sign = |flip: bool| if flip { -1.0f32 } else { 1.0 };
    let corner = |size: f32, pivot: f32, f: f32| {
        let below = size * pivot * f;
        (size * f - below, -below)
    };
    let (hx, lx) = corner(r.size[0], s.pivot[0], sign(r.flip_x));
    let (hy, ly) = corner(r.size[1], s.pivot[1], sign(r.flip_y));
    let (min, max) = ([hx.min(lx), hy.min(ly), -0.1f32], [hx.max(lx), hy.max(ly), 0.1f32]);
    (std::array::from_fn(|i| (min[i] + max[i]) * 0.5), std::array::from_fn(|i| (max[i] - min[i]) * 0.5))
}

struct Linker<'a> {
    prefab: &'a Prefab,
    inst: &'a Instance,
    file: usize,
    arenas: Arenas,
}

impl Linker<'_> {
    /// The prefab object a local reference points at (its GameObject or one of its components).
    fn object(&self, v: &Value) -> Result<Option<usize>, String> {
        self.object_of(v.pptr().ok_or_else(|| format!("not a reference: {v:?}"))?)
    }

    fn object_of(&self, p: PPtr) -> Result<Option<usize>, String> {
        if p.is_null() {
            return Ok(None);
        }
        if p.file != 0 {
            return Err(format!("a reference outside the prefab: {p:?}"));
        }
        let (o, _) = self.prefab.index.get(&ObjRef { file: self.file, path_id: p.path_id }).ok_or_else(|| format!("dangling reference {p:?}"))?;
        Ok(Some(*o))
    }

    fn in_arena(&self, v: &Value, arena: &HashMap<usize, usize>, what: &str) -> Result<Option<usize>, String> {
        match self.object(v)? {
            None => Ok(None),
            Some(o) => arena.get(&o).copied().map(Some).ok_or_else(|| format!("{} has no {what}", self.prefab.objects[o].name)),
        }
    }

    fn limb(&self, v: &Value) -> Result<Option<usize>, String> {
        self.in_arena(v, &self.arenas.limb, "LimbBehaviour")
    }

    fn limbs(&self, v: &Value, name: &str) -> Result<Vec<usize>, String> {
        array(v, name)?.iter().map(|x| self.limb(x)?.ok_or_else(|| format!("{name}: a null entry"))).collect()
    }

    fn physical(&self, v: &Value) -> Result<Option<usize>, String> {
        self.in_arena(v, &self.arenas.physical, "PhysicalBehaviour")
    }
}

impl Game {
    /// The instance's scripts as arena entries with their serialized fields and links, in prefab order (the
    /// order GetComponentsInChildren lists them, and the queue 0x14 order within one script class). Limb-owned
    /// components (CirculationBehaviour, ConnectedNodeBehaviour, SkinMaterialHandler, GForceMeasureBehaviour)
    /// share their limb's index. Rigidbody-less Transforms above bodies become `transform_nodes` in the order
    /// the bodies reach them.
    pub fn link_instance(&mut self, db: &mut AssetDb, prefab: &Prefab, inst: &Instance) -> Result<Linked, String> {
        let file = prefab.objects[0].game_object.file;
        let mut l = Linker { prefab, inst, file, arenas: Arenas::default() };
        let n = prefab.objects.len();
        let person = script(prefab, 0, "PersonBehaviour").map(|_| self.people.len());

        // TransformChain: each Rigidbody2D's parents up to the root (an inactive one's too: it has no body yet), the
        // first to reach one listing it.
        let mut body_parents = std::collections::BTreeSet::new();
        for o in (0..n).filter(|&o| prefab.objects[o].components.iter().any(|(_, x)| matches!(x, Component::Rigidbody2D(_)))) {
            let mut p = inst.objects[o].parent;
            while let Some(t) = p {
                // A static body never moves in the simulation, so its Transform serves as a plain node (the
                // Default map's walls under Root); a moving body's node follows its physical.
                let static_body = prefab.objects[t].components.iter().any(|(_, x)| matches!(x, Component::Rigidbody2D(rb) if rb.body_type == 2));
                if inst.bodies.contains_key(&t) && !static_body {
                    body_parents.insert(t);
                }
                if l.arenas.node.contains_key(&t) {
                    break;
                }
                l.arenas.node.insert(t, self.transform_nodes.len());
                let transform = prefab.objects[t].components.iter().position(|(_, c)| matches!(c, Component::Transform(_))).ok_or("an object without a Transform")?;
                self.transform_nodes.push(TransformNode { id: inst.objects[t].components[transform], trs: inst.objects[t].trs, parent: None });
                p = inst.objects[t].parent;
            }
        }
        // A Rigidbody2D without a PhysicalBehaviour: its own Transform is a node its body writes.
        let bare_bodies: Vec<usize> = (0..n).filter(|&o| script(prefab, o, "PhysicalBehaviour").is_none() && prefab.objects[o].components.iter().any(|(_, x)| matches!(x, Component::Rigidbody2D(_)))).collect();
        for &o in &bare_bodies {
            if !l.arenas.node.contains_key(&o) {
                l.arenas.node.insert(o, self.transform_nodes.len());
                let transform = prefab.objects[o].components.iter().position(|(_, c)| matches!(c, Component::Transform(_))).ok_or("an object without a Transform")?;
                self.transform_nodes.push(TransformNode { id: inst.objects[o].components[transform], trs: inst.objects[o].trs, parent: None });
            }
        }
        for (&o, &i) in &l.arenas.node {
            self.transform_nodes[i].parent = inst.objects[o].parent.map(|t| l.arenas.node[&t]);
        }

        // Arena slots first, so every reference resolves.
        let mut next = (self.physicals.len(), self.limbs.len(), self.grips.len(), self.hinges.len());
        for o in 0..n {
            if script(prefab, o, "PhysicalBehaviour").is_some() {
                l.arenas.physical.insert(o, next.0);
                next.0 += 1;
            }
            if script(prefab, o, "LimbBehaviour").is_some() {
                l.arenas.limb.insert(o, next.1);
                next.1 += 1;
            }
            if script(prefab, o, "GripBehaviour").is_some() {
                l.arenas.grip.insert(o, next.2);
                next.2 += 1;
            }
            for (c, (_, comp)) in prefab.objects[o].components.iter().enumerate() {
                match comp {
                    Component::Joint2D(j) if matches!(j.kind, JointKind::Hinge { .. }) => {
                        l.arenas.hinge.insert((o, c), next.3);
                        next.3 += 1;
                    }
                    _ => {}
                }
            }
        }
        for t in body_parents {
            let p = *l.arenas.physical.get(&t).ok_or_else(|| format!("{}: a body under a Rigidbody2D without a PhysicalBehaviour", prefab.objects[t].name))?;
            self.node_bodies.insert(l.arenas.node[&t], p);
        }
        let mut node_rigidbodies = HashMap::new();
        for &o in &bare_bodies {
            let Some(Component::Rigidbody2D(rb)) = prefab.objects[o].components.iter().map(|(_, x)| x).find(|x| matches!(x, Component::Rigidbody2D(_))) else { unreachable!() };
            // An inactive object's Rigidbody2D has no body (m_Body null) until it is activated.
            let (body, detached) = match inst.bodies.get(&o) {
                Some(&body) => (body, None),
                None if !inst.objects[o].active => (usize::MAX, Some(crate::b2::world::RigidbodyMass { use_auto_mass: rb.use_auto_mass, mass: rb.mass, inertia: None, center_of_mass: None })),
                None => unreachable!("an active Rigidbody2D without a body"),
            };
            let rigidbody = Rigidbody { body, bodyType: rb.body_type, collisionDetectionMode: rb.collision_detection, interpolation: rb.interpolate, movement: MovementState::default(), detached };
            self.rigidbody_hierarchy_changed |= detached.is_none() && rb.body_type != 2;
            let mut driven_by = None;
            if rb.body_type != 2 && detached.is_none() {
                let mut up = inst.objects[o].parent;
                while let Some(t) = up {
                    if prefab.objects[t].components.iter().any(|(_, x)| matches!(x, Component::Rigidbody2D(_))) {
                        driven_by = Some(*inst.bodies.get(&t).ok_or_else(|| format!("{}: driven by a Rigidbody2D without a body", prefab.objects[o].name))?);
                        break;
                    }
                    up = inst.objects[t].parent;
                }
            }
            node_rigidbodies.insert(o, self.node_rigidbodies.len());
            self.node_rigidbodies.push(NodeBody { node: l.arenas.node[&o], rigidbody, driven_by });
        }
        // Colliders and sprites without a PhysicalBehaviour on their GameObject: the Transforms down from the
        // nearest physical or node.
        let anchor_of = |o: usize| {
            let (mut chain, mut up) = (Vec::new(), Some(o));
            let anchor = loop {
                let Some(t) = up else { break BareAnchor::Root };
                if let Some(&p) = l.arenas.physical.get(&t) {
                    break BareAnchor::Physical(p);
                }
                if let Some(&i) = l.arenas.node.get(&t) {
                    break BareAnchor::Node(i);
                }
                chain.push(inst.objects[t].trs);
                up = inst.objects[t].parent;
            };
            (anchor, chain)
        };
        for o in (0..n).filter(|&o| !l.arenas.physical.contains_key(&o)) {
            for (c, (_, comp)) in prefab.objects[o].components.iter().enumerate() {
                match comp {
                    Component::Collider2D(_) => {
                        let (anchor, chain) = anchor_of(o);
                        let scripts = prefab.objects[o].components.iter().filter_map(|(_, x)| if let Component::Script { class, .. } = x { Some(class.clone()) } else { None }).collect();
                        self.bare_colliders.insert(inst.objects[o].components[c], BareCollider { anchor, chain, scripts, active_self: prefab.objects[o].active });
                    }
                    Component::SpriteRenderer(r) => {
                        let Some(sprite) = db.resolve(file, r.sprite) else { continue };
                        let (anchor, chain) = anchor_of(o);
                        let visible = inst.objects[o].active && r.enabled;
                        self.renderers.push(Renderer { anchor, chain, sprite: (sprite, r.color, r.sorting_order), draw: ([r.flip_x, r.flip_y], r.draw_mode, r.size), visible });
                    }
                    _ => {}
                }
            }
        }
        for class in ["CirculationBehaviour", "ConnectedNodeBehaviour", "SkinMaterialHandler"] {
            for o in 0..n {
                if (script(prefab, o, class).is_some()) != l.arenas.limb.contains_key(&o) {
                    return Err(format!("{}: {class} without its LimbBehaviour (or the reverse)", prefab.objects[o].name));
                }
            }
        }

        for o in 0..n {
            let Some((c, enabled, v)) = script(prefab, o, "PhysicalBehaviour") else { continue };
            let mut p = Physical { name: prefab.objects[o].name.clone(), id: inst.objects[o].components[c], ..Default::default() };
            load_values(p.fields(), v)?;
            p.enabled = enabled;
            p.active = inst.objects[o].active;
            // GameObject.isStatic: player data carries no static editor flags.
            p.isStatic = false;
            p.colliders = prefab.objects[o].components.iter().enumerate().filter(|(_, (_, x))| matches!(x, Component::Collider2D(_))).map(|(k, _)| inst.objects[o].components[k]).collect();
            p.collider_defs = prefab.objects[o].components.iter().filter_map(|(_, x)| match x {
                Component::Collider2D(c) => Some(c.clone()),
                _ => None,
            }).collect();
            p.all_colliders = p.colliders.clone();
            p.all_colliders.sort_by(|a, b| b.cmp(a));
            // Start adds it to Global.PhysicalObjectsInWorld (an inactive object never starts).
            p.in_world = false;
            p.Properties = match db.resolve(file, field(v, "Properties")?.pptr().ok_or("Properties")?) {
                Some(asset) => self.physical_properties(db, asset)?,
                None => unimplemented!("{}: PhysicalBehaviour without Properties", p.name),
            };
            let Some(Component::Rigidbody2D(rb)) = prefab.objects[o].components.iter().map(|(_, x)| x).find(|x| matches!(x, Component::Rigidbody2D(_))) else {
                return Err(format!("{}: PhysicalBehaviour without a Rigidbody2D", p.name));
            };
            // An inactive object's Rigidbody2D has no body (m_Body null) until it is activated.
            let (body, detached) = match inst.bodies.get(&o) {
                Some(&body) => (body, None),
                None if !p.active => (usize::MAX, Some(crate::b2::world::RigidbodyMass { use_auto_mass: rb.use_auto_mass, mass: rb.mass, inertia: None, center_of_mass: None })),
                None => unreachable!("an active Rigidbody2D without a body"),
            };
            p.rigidbody = Rigidbody { body, bodyType: rb.body_type, collisionDetectionMode: rb.collision_detection, interpolation: rb.interpolate, movement: MovementState::default(), detached };
            // Rigidbody2D::Create set the scene's m_RigidbodyHierarchyChanged.
            self.rigidbody_hierarchy_changed |= detached.is_none() && rb.body_type != 2;
            p.transform = pose(inst, o);
            p.parent = inst.objects[o].parent.map(|t| l.arenas.node[&t]);
            // Rigidbody2D::Create's CheckForDrivenByParentRigidbody.
            if rb.body_type != 2 && detached.is_none() {
                let mut up = inst.objects[o].parent;
                while let Some(t) = up {
                    if prefab.objects[t].components.iter().any(|(_, x)| matches!(x, Component::Rigidbody2D(_))) {
                        p.driven_by = Some(*inst.bodies.get(&t).ok_or_else(|| format!("{}: driven by a Rigidbody2D without a body", p.name))?);
                        break;
                    }
                    up = inst.objects[t].parent;
                }
            }
            p.LocalColliderGridPoints = Some(array(v, "LocalColliderGridPoints")?.iter().map(floats::<2>).collect::<Result<_, _>>()?);
            // Awake's GetComponent<LocalFirePropagationMap>() and Start's GetComponents<DecalControllerBehaviour>().
            p.has_fire_map = script(prefab, o, "LocalFirePropagationMap").is_some();
            p.prefab_decal_controllers = prefab.objects[o].components.iter().filter(|(_, x)| matches!(x, Component::Script { class, .. } if class == "DecalControllerBehaviour")).count();
            p.audio_time_scale = script(prefab, o, "AudioSourceTimeScaleBehaviour").is_some();
            p.audioSource = prefab.objects[o].components.iter().any(|(_, x)| matches!(x, Component::Other(AUDIO_SOURCE)));
            p.MaximumStabVictims = field(v, "MaximumStabVictims")?.i64().ok_or("MaximumStabVictims")? as i32;
            p.LooseEndSize = float(v, "LooseEndSize")?;
            p.StabReleaseSpeedModifier = float(v, "StabReleaseSpeedModifier")?;
            p.StabWoundSizeMultiplier = float(v, "StabWoundSizeMultiplier")?;
            p.OverrideImpactSounds = array(v, "OverrideImpactSounds")?.len();
            let renderer = prefab.objects[o].components.iter().find_map(|(_, x)| match x {
                Component::SpriteRenderer(r) => Some(r),
                _ => None,
            });
            if let Some(r) = renderer {
                let sprite = db.resolve(file, r.sprite).ok_or("a SpriteRenderer without a sprite")?;
                p.sprite_bounds = Some(renderer_bounds(&Sprite::read(db.data(sprite))?, r));
                p.sprite = Some((sprite, r.color, r.sorting_order));
                p.sprite_draw = ([r.flip_x, r.flip_y], r.draw_mode, r.size);
            }
            self.physicals.push(p);
        }

        for o in 0..n {
            for (c, (_, comp)) in prefab.objects[o].components.iter().enumerate() {
                let Component::Joint2D(j) = comp else { continue };
                let JointKind::Hinge { use_motor, motor_speed, max_motor_torque, use_limits, lower, upper } = j.kind else { continue };
                let connected = l.object_of(j.connected_body)?;
                let h = Hinge {
                    joint: inst.joints.get(&(o, c)).copied(),
                    useMotor: use_motor,
                    motor: [motor_speed, max_motor_torque],
                    useLimits: use_limits,
                    limits: [lower, upper],
                    breakForce: j.break_force,
                    breakTorque: j.break_torque,
                    connected: connected.map(|b| inst.bodies.get(&b).copied().ok_or("a hinge to an object without a Rigidbody2D")).transpose()?,
                    anchor: j.anchor,
                    connectedAnchor: inst.hinges.get(&(o, c)).map_or(j.connected_anchor, |s| s.connected_anchor),
                    enabled: j.enabled,
                    destroyed: false,
                };
                // The create put the joint in m_JointBreakLimits as BreakJoint::Anchored(slot): it becomes Hinge(h), in place.
                if let Some(slot) = inst.hinges.get(&(o, c)).and_then(|s| s.slot) {
                    let anchored = BreakJoint::Anchored(slot);
                    for k in self.joint_break_limits.iter_mut().filter(|k| **k == anchored) {
                        *k = BreakJoint::Hinge(self.hinges.len());
                    }
                    self.anchored_joints[slot].joint = None;
                    self.anchored_joints[slot].hinge = Some(self.hinges.len());
                }
                self.hinges.push(h);
            }
        }

        for o in 0..n {
            let Some((c, enabled, v)) = script(prefab, o, "LimbBehaviour") else { continue };
            let mut b = Limb { name: prefab.objects[o].name.clone(), id: inst.objects[o].components[c], ..Default::default() };
            load_values(b.fields(), v)?;
            b.enabled = enabled;
            // Awake's GetComponent<PhysicalBehaviour/HingeJoint2D/GripBehaviour>() (the first of each on the object),
            // and PersonBehaviour.Awake's limb.Person = this.
            b.PhysicalBehaviour = *l.arenas.physical.get(&o).ok_or_else(|| format!("{}: LimbBehaviour without PhysicalBehaviour", b.name))?;
            b.Person = person.ok_or_else(|| format!("{}: a LimbBehaviour outside a Person", b.name))?;
            b.Joint = (0..prefab.objects[o].components.len()).find_map(|k| l.arenas.hinge.get(&(o, k)).copied());
            b.GripBehaviour = l.arenas.grip.get(&o).copied();
            b.ConnectedLimbs = l.limbs(v, "ConnectedLimbs")?;
            b.NearestLimbToBrain = l.limb(field(v, "NearestLimbToBrain")?)?;
            b.StrengthMultipliers = array(v, "StrengthMultipliers")?.iter().map(|x| x.f32().ok_or("StrengthMultipliers")).collect::<Result<_, _>>()?;
            b.SpeciesIdentity = string(v, "SpeciesIdentity")?;
            b.BloodLiquidType = string(v, "BloodLiquidType")?;
            let decal = db.resolve(file, field(v, "BloodDecal")?.pptr().ok_or("BloodDecal")?).ok_or("LimbBehaviour without a BloodDecal")?;
            let decal = self.load_decal_descriptor(db, decal)?;
            b.BloodDecal = self.decal_descriptors[decal].name.clone();
            self.vital_parts.insert(self.limbs.len(), array(v, "VitalParts")?.iter().map(bounds).collect::<Result<_, _>>()?);
            // Awake's GetComponent<GoreStringBehaviour>().
            b.goreString = false;
            if let Some((_, gore_enabled, gv)) = script(prefab, o, "GoreStringBehaviour") {
                b.goreString = true;
                b.goreStringEnabled = gore_enabled;
                // GoreStringBehaviour.Joint is not serialized: Start makes it (CreateJointOnStart).
                b.goreStringJoint = false;
                b.gore = GoreString {
                    Other: l.limb(field(gv, "Other")?)?,
                    JointDampening: float(gv, "JointDampening")?,
                    JointFreq: float(gv, "JointFreq")?,
                    JointBreakThreshold: float(gv, "JointBreakThreshold")?,
                    InvincibilityTime: float(gv, "InvincibilityTime")?,
                    OriginLine: line(field(gv, "OriginLine")?)?,
                    OtherLine: line(field(gv, "OtherLine")?)?,
                    HadJointWhenSerialised: field(gv, "HadJointWhenSerialised")?.bool().ok_or("HadJointWhenSerialised")?,
                    TissueSnapClips: array(gv, "TissueSnapClips")?.len(),
                    AudioSource: l.object(field(gv, "AudioSource")?)?.is_some(),
                    ..Default::default()
                };
            }
            self.limbs.push(b);
        }

        for o in 0..n {
            let Some((_, _, v)) = script(prefab, o, "GripBehaviour") else { continue };
            let mut g = Grip::default();
            load_values(g.fields(), v)?;
            // Awake's GetComponent<PhysicalBehaviour>().
            g.PhysicalBehaviour = *l.arenas.physical.get(&o).ok_or("GripBehaviour without PhysicalBehaviour")?;
            g.CurrentlyHolding = l.physical(field(v, "CurrentlyHolding")?)?;
            self.grips.push(g);
        }

        for o in 0..n {
            let Some((_, enabled, v)) = script(prefab, o, "CirculationBehaviour") else { continue };
            let mut c = Circulation::default();
            load_values(c.fields(), v)?;
            c.enabled = enabled;
            c.Source = l.limb(field(v, "Source")?)?;
            c.PushesTo = l.limbs(v, "PushesTo")?;
            c.LocalArteryRects = Some(array(v, "LocalArteryRects")?.iter().map(bounds).collect::<Result<_, _>>()?);
            self.circulations.push(c);
        }
        for o in 0..n {
            let Some((_, _, v)) = script(prefab, o, "ConnectedNodeBehaviour") else { continue };
            let mut d = Node::default();
            load_values(d.fields(), v)?;
            d.Connections = array(v, "Connections")?.iter().map(|x| l.limb(x)).collect::<Result<_, _>>()?;
            self.nodes.push(d);
        }
        for o in 0..n {
            let Some((_, _, v)) = script(prefab, o, "SkinMaterialHandler") else { continue };
            let mut k = Skin::default();
            load_values(k.fields(), v)?;
            k.adjacentLimbs = l.limbs(v, "adjacentLimbs")?;
            self.skins.push(k);
        }
        // LimbBehaviour.Awake's GetOrAddComponent<GForceMeasureBehaviour>() adds the ones the prefab lacks; the
        // arena keeps the limb index only if the prefab has all or none.
        // Stored by ascending instance id, as a recording lists them.
        let mut map_lights = HashMap::new();
        let mut lights: Vec<(i32, usize)> = (0..n).filter_map(|o| script(prefab, o, "MapLightBehaviour").map(|(c, _, _)| (inst.objects[o].components[c], o))).collect();
        lights.sort();
        for (_, o) in lights {
            let (_, _, v) = script(prefab, o, "MapLightBehaviour").unwrap();
            let mut m = MapLight::default();
            load_values(m.fields(), v)?;
            // A serialized array is never null.
            m.switches = Some(array(v, "Switches")?.len());
            m.hasAudio = l.object(field(v, "AudioSource")?)?.is_some();
            map_lights.insert(o, self.map_lights.len());
            self.map_lights.push(m);
        }
        let mut waters = HashMap::new();
        for o in 0..n {
            let Some((c, enabled, v)) = script(prefab, o, "WaterBehaviour") else { continue };
            let component = |name: &str| -> Result<(usize, usize), String> {
                let pp = field(v, name)?.pptr().ok_or_else(|| name.to_string())?;
                prefab.index.get(&ObjRef { file, path_id: pp.path_id }).copied().filter(|_| !pp.is_null() && pp.file == 0).ok_or_else(|| format!("WaterBehaviour.{name} outside the prefab or null"))
            };
            let mut w = Water::default();
            load_values(w.fields(), v)?;
            (w.id, w.object, w.component, w.enabled, w.active) = (inst.objects[o].components[c], o, c, enabled, inst.objects[o].active);
            w.chain = inst.chain(o);
            let (to, tc) = component("Trigger")?;
            w.trigger = inst.objects[to].components[tc];
            w.trigger_fixtures = inst.fixtures.get(&(to, tc)).cloned().unwrap_or_default();
            w.ice_object = component("Ice")?.0;
            w.ice = *l.arenas.physical.get(&w.ice_object).ok_or("WaterBehaviour.Ice without a PhysicalBehaviour")?;
            w.boiling_sound = l.object(field(v, "BoilingWaterSound")?)?.ok_or("WaterBehaviour.BoilingWaterSound is null")?;
            for x in array(v, "ObjectsToBeActiveIfWater")? {
                w.objects_to_be_active.push(l.object(x)?.ok_or("a null ObjectsToBeActiveIfWater entry")?);
            }
            if let Some(r) = field(v, "Splash")?.pptr().and_then(|pp| db.resolve(file, pp)) {
                w.splash_pool = Some(self.splash_pool_for(db, r)?);
            }
            w.waterSizzles = db.resources_in("Audio/water_sizzle", 83)?.len();
            waters.insert(o, self.waters.len());
            self.waters.push(w);
        }
        let gforces = (0..n).filter(|&o| script(prefab, o, "GForceMeasureBehaviour").is_some()).count();
        if gforces != 0 && gforces != l.arenas.limb.len() {
            unimplemented!("a prefab with GForceMeasureBehaviour on some limbs only");
        }
        for o in 0..n {
            let Some((_, _, v)) = script(prefab, o, "GForceMeasureBehaviour") else { continue };
            let mut gf = GForce::default();
            load_values(gf.fields(), v)?;
            self.gforces.push(gf);
        }

        if let Some((c, _, v)) = script(prefab, 0, "PersonBehaviour") {
            let mut p = Person { name: prefab.objects[0].name.clone(), id: inst.objects[0].components[c], ..Default::default() };
            load_values(p.fields(), v)?;
            p.Limbs = l.limbs(v, "Limbs")?;
            p.BoneBreakClips = array(v, "BoneBreakClips")?.len();
            p.DismembermentClips = array(v, "DismembermentClips")?.len();
            // CirculationBehaviour copies the BleedingParticle prefab's DripSounds (a serialized array, never null).
            if let Some(bp) = field(v, "BleedingParticlePrefab")?.pptr().filter(|p| !p.is_null()) {
                let go = db.resolve(file, bp).ok_or("BleedingParticlePrefab outside the data")?;
                let mut drips = None;
                for c in db.game_object(go)?.components {
                    let c = db.resolve(go.file, c).ok_or("a BleedingParticlePrefab component outside the data")?;
                    if db.class(c) != 114 {
                        continue;
                    }
                    if let Some((script, _, bv)) = db.mono(c)? {
                        if script.class_name == "BleedingParticleBehaviour" {
                            drips = Some(array(&bv, "DripSounds")?.len());
                        }
                    }
                }
                p.BleedingDripSounds = Some(drips.ok_or("BleedingParticlePrefab without BleedingParticleBehaviour")?);
            }
            let s = inst.objects[0].trs.s;
            p.localScale = [s[0], s[1], s[2]];
            for pose in array(v, "Poses")? {
                let mut angles = Vec::new();
                for a in array(pose, "Angles")? {
                    let animated = field(a, "Animated")?.bool().ok_or("Animated")?;
                    angles.push(LimbPose {
                        Limb: l.limb(field(a, "Limb")?)?.ok_or("a LimbPose without its limb")?,
                        Animated: animated,
                        PoseRigidityModifier: float(a, "PoseRigidityModifier")?,
                        Angle: float(a, "Angle")?,
                        StartAngle: float(a, "StartAngle")?,
                        EndAngle: float(a, "EndAngle")?,
                        AnimationDuration: float(a, "AnimationDuration")?,
                        RandomInfluence: float(a, "RandomInfluence")?,
                        RandomSpeed: float(a, "RandomSpeed")?,
                        TimeOffset: float(a, "TimeOffset")?,
                        AnimationCurve: Some(curve(field(a, "AnimationCurve")?)?),
                        cache: CurveCache::default(),
                    });
                }
                p.Poses.push(RagdollPose {
                    Name: string(pose, "Name")?,
                    Rigidity: float(pose, "Rigidity")?,
                    ForceMultiplier: float(pose, "ForceMultiplier")?,
                    ShouldStandUpright: field(pose, "ShouldStandUpright")?.bool().ok_or("ShouldStandUpright")?,
                    DragInfluence: float(pose, "DragInfluence")?,
                    UprightForceMultiplier: float(pose, "UprightForceMultiplier")?,
                    ShouldStumble: field(pose, "ShouldStumble")?.bool().ok_or("ShouldStumble")?,
                    State: field(pose, "State")?.i64().ok_or("State")? as i32,
                    AnimationSpeedMultiplier: float(pose, "AnimationSpeedMultiplier")?,
                    Angles: angles,
                });
            }
            self.people.push(p);
        }
        Ok(Linked { person, limbs: l.arenas.limb, nodes: l.arenas.node, physicals: l.arenas.physical, hinges: l.arenas.hinge, map_lights, waters, node_rigidbodies, parent_audio: None })
    }

    /// Object.Instantiate of a Person prefab up to the end of the Instantiate call: the native clone and its
    /// AwakeFromLoad queues, the scripts' deserialization (`link_instance`), then queue 0x14's Awake calls.
    /// The Starts are queued: Unity runs them in the spawn frame, PersonBehaviour's before its LateUpdate and the
    /// rest after the LateUpdate phase (`update_scripts`). The b2 bodies see PersonBehaviour.Awake's rescale at the
    /// next SyncTransforms.
    /// `parent_audio` is the PlayAudioAtRandom of the parent the clone goes under (`ambient_audio`).
    pub fn spawn(&mut self, db: &mut AssetDb, prefab: &Prefab, position: [f32; 3], rotation: [f32; 4], parent_audio: Option<usize>) -> Result<(Instance, Linked), String> {
        let original_ids = db.resource_load_order()?;
        let (mut inst, behaviours) = self.instantiate_native(prefab, position, rotation, &original_ids);
        let first = self.physicals.len();
        let mut linked = self.link_instance(db, prefab, &inst)?;
        linked.parent_audio = parent_audio;
        // GetHashCode (Random.InitState in PhysicalBehaviour.Start) is Mono's object hash, (address >> 3) *
        // 0x9e3779b1 (mono_object_hash, 180116520), of a managed heap address that differs per run in the game too.
        // A PhysicalBehaviour is a Boehm large object, 2048-aligned (every recorded hash has a zero low byte).
        for p in first..self.physicals.len() {
            let address = std::hash::BuildHasher::hash_one(&std::collections::hash_map::RandomState::new(), p) << 11;
            self.physicals[p].hash_code = ((address >> 3) as u32).wrapping_mul(0x9e37_79b1) as i32;
        }
        self.awake_behaviours(prefab, &mut inst, &linked, &behaviours);
        self.queue_starts(db, prefab, &inst, &linked, &behaviours)?;
        Ok((inst, linked))
    }

    /// The linked objects' Transforms after a script changed the clone's.
    fn refresh_transforms(&mut self, inst: &Instance, linked: &Linked) {
        for (&o, &n) in &linked.nodes {
            self.transform_nodes[n].trs = inst.objects[o].trs;
        }
        for (&o, &p) in &linked.physicals {
            self.physicals[p].transform = pose(inst, o);
        }
        if let Some(person) = linked.person {
            let s = inst.objects[0].trs.s;
            self.people[person].localScale = [s[0], s[1], s[2]];
        }
    }

    /// CatalogBehaviour.Update (no pause menu, dialog or UI block; no contraption is ever selected): releasing
    /// "spawnRight" spawns SelectedItem facing right, else releasing "spawnLeft" spawns it flipped.
    pub fn catalog_update(&mut self) {
        let Some(item) = self.catalog_selected.clone() else { return };
        let up = |k: &str| self.input.controls.get(k).is_some_and(|c| c.up);
        let flipped = match (up("spawnRight"), up("spawnLeft")) {
            (true, _) => false,
            (false, true) => true,
            _ => return,
        };
        let mut db = self.assets.take().expect("CatalogBehaviour.Update without the game's data");
        let spawned = self.catalog_spawn(&mut db, &item, self.MousePosition, flipped);
        self.assets = Some(db);
        match spawned {
            Ok((inst, _)) => self.unsynced.push(inst),
            Err(e) => panic!("CatalogBehaviour.Spawn({item}): {e}"),
        }
    }

    /// CatalogBehaviour.Spawn(SelectedItem, flipped) at Global.MousePosition: PerformInstantiation (no
    /// InstantiateOverride without mods), then AddComponent DeregisterBehaviour, TexturePackApplier,
    /// AudioSourceTimeScaleBehaviour and SerialiseInstructions, and the asset's name. SpawnedGameObjects, the undo
    /// action and the stats are not simulation state; DeregisterBehaviour and TexturePackApplier have no messages
    /// and AudioSourceTimeScaleBehaviour.Start only registers audio.
    pub fn catalog_spawn(&mut self, db: &mut AssetDb, asset: &str, position: [f32; 3], flipped: bool) -> Result<(Instance, Linked), String> {
        let catalog = *db.find_scripts("CatalogData")?.first().ok_or("no CatalogData")?;
        let (_, _, items) = db.mono(catalog)?.ok_or("CatalogData")?;
        let mut prefab = None;
        for item in items.get("Items").and_then(Value::array).ok_or("CatalogData.Items")?.to_vec() {
            let r = db.resolve(catalog.file, item.pptr().ok_or("CatalogData.Items")?).ok_or("a SpawnableAsset outside the data")?;
            let (_, mb, v) = db.mono(r)?.ok_or("SpawnableAsset")?;
            if mb.name == asset {
                prefab = Some(db.resolve(r.file, v.get("Prefab").and_then(Value::pptr).ok_or("SpawnableAsset.Prefab")?).ok_or("SpawnableAsset.Prefab")?);
                break;
            }
        }
        let prefab = db.prefab(prefab.ok_or_else(|| format!("no catalog item {asset:?}"))?)?;
        let (mut inst, linked) = self.spawn(db, &prefab, position, [0.0, 0.0, 0.0, 1.0], None)?;
        if flipped {
            let s = inst.objects[0].trs.s;
            inst.set_local_scale(0, [s[0] * -1.0, s[1], s[2]]);
            self.refresh_transforms(&inst, &linked);
        }
        for _ in 0..4 {
            self.allocate_instance_id();
        }
        // The root's AudioSourceTimeScaleBehaviour, which PhysicalBehaviour.Start's GetComponentInParent finds.
        match (linked.physicals.get(&0), linked.nodes.get(&0)) {
            (Some(&p), _) => self.physicals[p].audio_time_scale = true,
            (None, Some(&n)) => self.audio_time_scale_roots.push(n),
            (None, None) => unimplemented!("a catalog item whose root is neither physical nor a parent of physicals"),
        }
        // gameObject.name = e.name
        if let Some(&p) = linked.physicals.get(&0) {
            self.physicals[p].name = asset.to_string();
        }
        if let Some(p) = linked.person {
            self.people[p].name = asset.to_string();
        }
        // SerialiseInstructions.Start: a SerialisableIdentity on the root and on every Transform below without an
        // Optout, unless one is there (what Starts add first, the outlines and particles, all carry Optout).
        let has = |o: &PrefabObject, class: &str| o.components.iter().any(|(_, c)| matches!(c, Component::Script { class: k, .. } if k == class));
        let identities = prefab.objects.iter().enumerate().filter(|&(o, ob)| !has(ob, "SerialisableIdentity") && (o == 0 || !has(ob, "Optout"))).count();
        self.pending_starts.push(super::misc::PendingStart::SerialiseInstructions(identities));
        Ok((inst, linked))
    }

    /// Awake of each queue 0x14 behaviour, in `instantiate_native`'s order, skipping objects inactive by then.
    /// The classes left out have no Awake that touches simulation state (ConnectedNodeBehaviour, GripBehaviour
    /// and GoreStringBehaviour only read components; MapConfig, MapLightBehaviour and WindControllerBehaviour
    /// only fetch components), and none of these scripts has OnEnable.
    pub fn awake_behaviours(&mut self, prefab: &Prefab, inst: &mut Instance, linked: &Linked, order: &[(usize, usize)]) {
        // The behaviours whose OnEnable has run (awake and enabled), for OnDisable.
        let mut enabled_behaviours = std::collections::HashSet::new();
        for &(o, c) in order {
            if !inst.objects[o].active {
                continue;
            }
            let Component::Script { class, enabled, .. } = &prefab.objects[o].components[c].1 else { unreachable!() };
            if *enabled {
                enabled_behaviours.insert((o, c));
            }
            let limb = || linked.limbs[&o];
            match class.as_str() {
                // SetActive(ShowTutorial); a tutorial left active clears the preference.
                "ActivateIfTutorial" if self.prefs.ShowTutorial => self.prefs.ShowTutorial = false,
                "ActivateIfTutorial" => {
                    for (d, k) in self.set_active(inst, o, false) {
                        if !enabled_behaviours.remove(&(d, k)) {
                            continue;
                        }
                        let Component::Script { class, .. } = &prefab.objects[d].components[k].1 else { unreachable!() };
                        match class.as_str() {
                            "PhysicalBehaviour" => self.physical_on_disable(linked.physicals[&d]),
                            // No OnDisable; TextMeshPro's only touches its mesh and the font atlas.
                            "ActivateIfTutorial" | "AudioSourceTimeScaleBehaviour" | "DetachFromParentOnStart" | "Optout" | "MaintainScaleSignBehaviour" | "TextMeshProControlReplacer" | "TextMeshPro" => {}
                            _ => unimplemented!("{class}.OnDisable"),
                        }
                    }
                    for (&d, &p) in &linked.physicals {
                        self.physicals[p].active = inst.objects[d].active;
                    }
                    for (&d, &w) in &linked.waters {
                        self.waters[w].active = inst.objects[d].active;
                    }
                }
                "PersonBehaviour" => {
                    if let Some(s) = self.person_awake(linked.person.unwrap()) {
                        inst.set_local_scale(0, s);
                        self.refresh_transforms(&inst, linked);
                    }
                }
                "SkinMaterialHandler" => self.skin_awake(limb()),
                "PhysicalBehaviour" => self.physical_awake(linked.physicals[&o]),
                "WaterBehaviour" => self.water_awake(linked.waters[&o]),
                "LimbBehaviour" => self.limb_awake(limb()),
                "CirculationBehaviour" => self.circulation_awake(limb()),
                _ => {}
            }
        }
    }

    /// `host.GetComponentsInChildren<HingeJoint2D>()`, `<SliderJoint2D>()` and `<FixedJoint2D>()`: the host and its active
    /// descendants, depth first, each object's components in order (disabled components included).
    fn autofix_components(&self, prefab: &Prefab, inst: &Instance, linked: &Linked, host: usize) -> AutofixJoints {
        let mut out = AutofixJoints::default();
        let physical = |o: usize| match linked.physicals.get(&o) {
            Some(&p) => BodyOwner::Physical(p),
            None => BodyOwner::Node(linked.node_rigidbodies[&o]),
        };
        let mut stack = vec![host];
        while let Some(o) = stack.pop() {
            if !inst.objects[o].active {
                continue;
            }
            stack.extend(prefab.objects[o].children.iter().rev());
            for (c, (_, comp)) in prefab.objects[o].components.iter().enumerate() {
                let Component::Joint2D(j) = comp else { continue };
                let list = match j.kind {
                    JointKind::Hinge { .. } => {
                        out.hinges.push(linked.hinges[&(o, c)]);
                        continue;
                    }
                    JointKind::Slider { .. } => &mut out.sliders,
                    JointKind::Fixed { .. } => &mut out.fixeds,
                    _ => continue,
                };
                let state = inst.anchored.get(&(o, c)).cloned().unwrap_or_else(|| anchored_initial_state(j));
                list.push(AutofixJoint {
                    object: o,
                    component: c,
                    state: state.base,
                    slider: state.slider,
                    joint: inst.joints.get(&(o, c)).copied(),
                    slot: state.slot,
                    attached: physical(o),
                    connected: joint_connected_object(prefab, o, c, j).map(physical),
                    enabled: j.enabled,
                });
            }
        }
        out
    }

    /// Queues the Start of each behaviour in `order` (`run_pending_starts`, or PersonBehaviour's before its first
    /// LateUpdate). Each PhysicalBehaviour's `hash_code` must be set by then: Random.InitState(GetHashCode()) takes
    /// Mono's object hash. SkinMaterialHandler, DisintegrationCounterBehaviour and IncreaseStatOnStart's Starts only
    /// touch rendering, event subscriptions and the stat manager.
    pub fn queue_starts(&mut self, db: &mut AssetDb, prefab: &Prefab, inst: &Instance, linked: &Linked, order: &[(usize, usize)]) -> Result<(), String> {
        use super::misc::PendingStart;
        for &(o, c) in order {
            let Component::Script { class, fields: v, enabled, .. } = &prefab.objects[o].components[c].1 else { unreachable!() };
            if !inst.objects[o].active || !enabled {
                continue;
            }
            let limb = || linked.limbs[&o];
            let flag = |name: &str| field(v, name).and_then(|x| x.bool().ok_or_else(|| name.to_string())).unwrap();
            let start = match class.as_str() {
                "PersonBehaviour" => PendingStart::Person(linked.person.unwrap()),
                "HingeJointLimitAutofixBehaviour" => match linked.person {
                    Some(p) => PendingStart::HingeJointLimitAutofix(p),
                    None => {
                        let joints = self.autofix_components(prefab, inst, linked, o);
                        // Without a limited hinge or slider, or a FixedJoint2D, below it neither a flip nor Start touches anything.
                        if !joints.hinges.iter().any(|&h| self.hinges[h].useLimits) && !joints.sliders.iter().any(|j| j.slider.as_ref().unwrap().use_limits) && joints.fixeds.is_empty() {
                            continue;
                        }
                        let host = linked.physicals.get(&o).copied().ok_or_else(|| {
                            *linked.nodes.get(&o).unwrap_or_else(|| unimplemented!("HingeJointLimitAutofixBehaviour on an object that is neither physical nor a Transform node"))
                        });
                        self.autofix_joints.push(joints);
                        PendingStart::HingeJointLimitAutofixItem(host, self.autofix_joints.len() - 1)
                    }
                },
                "CirculationBehaviour" => PendingStart::Circulation(limb()),
                "PhysicalBehaviour" => PendingStart::Physical(linked.physicals[&o]),
                "LimbBehaviour" => PendingStart::Limb(limb()),
                "ConnectedNodeBehaviour" => PendingStart::Node(limb(), v.get("SerialisableConnectedNodes").and_then(Value::array).map(|a| a.iter().map(|x| x.bool().unwrap()).collect())),
                "GripBehaviour" => PendingStart::Grip(self.limbs[limb()].GripBehaviour.unwrap()),
                "GoreStringBehaviour" => PendingStart::GoreString(limb(), flag("CreateJointOnStart")),
                "ShatteredObjectGenerator" if flag("GenerateOnStart") => unimplemented!("ShatteredObjectGenerator.Start with GenerateOnStart"),
                "MapLightBehaviour" => PendingStart::MapLight(linked.map_lights[&o]),
                "WaterBehaviour" => PendingStart::Water(linked.waters[&o]),
                // CameraControlBehaviour takes the box.
                "BoundingBoxBehaviour" => PendingStart::BoundingBox(map_bounds(v)?),
                "MapConfig" => {
                    let file = prefab.objects[o].game_object.file;
                    let mut weather = Vec::new();
                    for prefab_field in ["SnowPrefab", "RainPrefab", "FogPrefab", "LightningCreatorPrefab"] {
                        let p = field(v, prefab_field)?.pptr().ok_or(prefab_field)?;
                        weather.push(match p.is_null() {
                            true => None,
                            false => Some(std::sync::Arc::new(db.prefab(db.resolve(file, p).ok_or_else(|| format!("{prefab_field} outside the data"))?)?)),
                        });
                    }
                    let lightning = weather.pop().unwrap();
                    // Awake's GetComponent<BoundingBoxBehaviour>().
                    let (_, _, b) = script(prefab, o, "BoundingBoxBehaviour").ok_or("MapConfig without a BoundingBoxBehaviour")?;
                    let ambience = field(v, "Ambience")?;
                    let settings = field(v, "Settings")?;
                    PendingStart::MapConfig(super::map::MapConfigStart {
                        bounds: map_bounds(b)?,
                        audio: linked.parent_audio,
                        ambience: field(v, "OverrideAmbientSounds")?.bool().ok_or("OverrideAmbientSounds")?.then(|| -> Result<_, String> { Ok((float(ambience, "Chance")?, array(ambience, "Clips")?.len())) }).transpose()?,
                        gravity: float(settings, "Gravity")?,
                        ambient_temperature: float(settings, "Ambient_temperature")?,
                        lightning_chance: float(settings, "Lightning_chance")?,
                        weather,
                        lightning,
                        original_ids: std::sync::Arc::new(db.resource_load_order()?),
                    })
                }
                _ => continue,
            };
            self.pending_starts.push(start);
        }
        Ok(())
    }

    /// The PhysicalProperties asset's arena index, loading it on first use (assets are shared by name).
    fn physical_properties(&mut self, db: &mut AssetDb, asset: ObjRef) -> Result<usize, String> {
        let (_, mb, v) = db.mono(asset)?.ok_or("PhysicalProperties is not a MonoBehaviour")?;
        if let Some(i) = self.properties.iter().position(|p| p.name == mb.name) {
            return Ok(i);
        }
        let b = |n: &str| -> Result<bool, String> { field(&v, n)?.bool().ok_or(n.to_string()) };
        let mut p = PhysicalProperties { name: mb.name.clone(), ..Default::default() };
        p.Softness = float(&v, "Softness")?;
        p.Flammability = float(&v, "Flammability")?;
        p.Conducting = b("Conducting")?;
        p.Burnrate = float(&v, "Burnrate")?;
        p.BurningTemperatureThreshold = float(&v, "BurningTemperatureThreshold")?;
        p.HeatTransferSpeedMultiplier = float(&v, "HeatTransferSpeedMultiplier")?;
        p.Buoyancy = float(&v, "Buoyancy")?;
        p.ImpactIntensityMutliplier = float(&v, "ImpactIntensityMutliplier")?;
        p.HitVolumeMultiplier = float(&v, "HitVolumeMultiplier")?;
        p.SparksOnSlide = b("SparksOnSlide")?;
        p.SoftImpact = array(&v, "SoftImpact")?.len();
        p.HardImpact = array(&v, "HardImpact")?.len();
        // Serialized arrays load as arrays, never null.
        p.SizzleSounds = Some(array(&v, "SizzleSounds")?.len());
        p.Sharp = b("Sharp")?;
        p.IgnoreStabResistance = b("IgnoreStabResistance")?;
        p.SharpForceThresholdMultiplier = float(&v, "SharpForceThresholdMultiplier")?;
        for a in array(&v, "SharpAxes")? {
            p.SharpAxes.push(SharpAxis {
                Axis: floats(field(a, "Axis")?)?,
                LowerLimit: float(a, "LowerLimit")?,
                UpperLimit: float(a, "UpperLimit")?,
                LooseUpperLimit: field(a, "LooseUpperLimit")?.bool().ok_or("LooseUpperLimit")?,
                LooseLowerLimit: field(a, "LooseLowerLimit")?.bool().ok_or("LooseLowerLimit")?,
            });
        }
        p.LodgeStrengthMultiplier = float(&v, "LodgeStrengthMultiplier")?;
        p.StabSound = Some(array(&v, "StabSound")?.len());
        match db.resolve(asset.file, field(&v, "PhysicMaterial")?.pptr().ok_or("PhysicMaterial")?) {
            Some(m) => {
                let m = PhysicsMaterial2D::read(db.data(m))?;
                (p.friction, p.bounciness) = (Some(m.friction), m.bounciness);
            }
            None => (p.friction, p.bounciness) = (None, 0.0),
        }
        self.properties.push(p);
        Ok(self.properties.len() - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_assets::HumanRig;

    /// Every dumped field that differs, as "<label> <field>: game <x> ours <y>".
    fn diff(label: &str, mut game: Vec<(&'static str, &mut dyn Field)>, mut ours: Vec<(&'static str, &mut dyn Field)>, out: &mut Vec<String>) {
        for ((name, a), (_, b)) in game.iter_mut().zip(ours.iter_mut()) {
            let (a, b) = (a.show(), b.show());
            if a != b {
                out.push(format!("{label} {name}: game {a} ours {b}"));
            }
        }
    }

    /// The spawned Person's arenas against dump31spawnrand's first snapshot (after Awake and Start). The links
    /// must match, except those Awake/Start set (LimbBehaviour.NearestLimbToBrain, CirculationBehaviour.Source,
    /// PersonBehaviour.Limbs, the hinges' motors, PhysicalBehaviour's collision mode and grid jitter); dumped
    /// fields are printed where they differ. The recording's poses come from the asset cache, which normalizes
    /// unanimated angles, so only what an unanimated angle uses is compared there.
    #[test]
    fn person_links_match_game() {
        let Some((mut g, inst, prefab, mut db, first)) = super::super::spawn::tests::spawn_recorded("dump31spawnrand") else { return };
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let Ok(rig) = HumanRig::load(&root.join("local-cache"), &crate::game_assets::find_game(None).unwrap()) else { return };
        // The map's and pool's splats are not the Person's.
        let mut first = first;
        first.records.retain(|r| r.ty != "FancyBloodSplatController" && r.ty != "BleedingParticleBehaviour");
        let want = Game::from_snapshot(&first, &rig).unwrap();
        let linked = g.link_instance(&mut db, &prefab, &inst).unwrap();
        let person = linked.person.unwrap();
        assert_eq!(g.people[person].BleedingDripSounds, Some(11), "BleedingParticle DripSounds");
        // The queue 0x14 order, as instantiate_native returns it (script execution order, then the higher clone id).
        let mut order: Vec<(usize, usize)> = (0..prefab.objects.len())
            .flat_map(|o| (0..prefab.objects[o].components.len()).map(move |c| (o, c)))
            .filter(|&(o, c)| matches!(prefab.objects[o].components[c].1, Component::Script { .. }))
            .collect();
        order.sort_by_key(|&(o, c)| {
            let Component::Script { execution_order, .. } = &prefab.objects[o].components[c].1 else { unreachable!() };
            (*execution_order, std::cmp::Reverse(inst.objects[o].components[c]))
        });
        let header = super::super::dump::header(&root.join("reference-probe/game/dump31spawnrand.txt")).unwrap();
        g.random = crate::unity::Random { state: super::super::dump::spawn_random_state(&header).unwrap() };
        // spawn_recorded applied the recorded scale; PersonBehaviour.Awake must draw the same one.
        let recorded_scale = inst.objects[0].trs.s;
        let mut inst = inst;
        g.awake_behaviours(&prefab, &mut inst, &linked, &order);
        assert_eq!(g.people[person].localScale.map(f32::to_bits), [recorded_scale[0].to_bits(), recorded_scale[1].to_bits(), recorded_scale[2].to_bits()], "PersonBehaviour.Awake scale");
        // Mono's GetHashCode is a heap address: from the recording.
        for p in 0..g.physicals.len() {
            let id = g.physicals[p].id;
            g.physicals[p].hash_code = want.physicals.iter().find(|a| a.id == id).map_or(0, |a| a.hash_code);
        }
        g.queue_starts(&mut db, &prefab, &inst, &linked, &order).unwrap();
        g.run_pending_starts();
        assert!(g.physicals.iter().all(|p| p.in_world));
        // The first frame's render: OnWillRenderObject fills the blood colour caches.
        for l in 0..g.limbs.len() {
            g.skin_will_render(l);
        }
        let mut out = Vec::new();
        let mut links = Vec::new();
        let mut check = |label: String, a: String, b: String| {
            if a != b {
                // Long values: from just before the first difference.
                let at = a.bytes().zip(b.bytes()).position(|(x, y)| x != y).unwrap_or(a.len().min(b.len())).saturating_sub(80);
                let cut = |s: &str| s.get(at.min(s.len())..).unwrap_or("").chars().take(240).collect::<String>();
                links.push(format!("{label} @{at}: game {} ours {}", cut(&a), cut(&b)));
            }
        };

        let names = |g: &Game, v: &[usize]| v.iter().map(|&l| g.limbs[l].name.clone()).collect::<Vec<_>>().join(",");
        check("limb names".into(), names(&want, &(0..want.limbs.len()).collect::<Vec<_>>()), names(&g, &(0..g.limbs.len()).collect::<Vec<_>>()));
        for l in 0..g.limbs.len().min(want.limbs.len()) {
            let (a, b) = (&want.limbs[l], &g.limbs[l]);
            let n = b.name.clone();
            check(format!("{n} id"), a.id.to_string(), b.id.to_string());
            check(format!("{n} PhysicalBehaviour"), want.physicals[a.PhysicalBehaviour].id.to_string(), g.physicals[b.PhysicalBehaviour].id.to_string());
            check(format!("{n} Joint"), format!("{:?}", a.Joint), format!("{:?}", b.Joint));
            check(format!("{n} NearestLimbToBrain"), format!("{:?}", a.NearestLimbToBrain), format!("{:?}", b.NearestLimbToBrain));
            check(format!("{n} GripBehaviour"), format!("{:?}", a.GripBehaviour), format!("{:?}", b.GripBehaviour));
            check(format!("{n} ConnectedLimbs"), names(&want, &a.ConnectedLimbs), names(&g, &b.ConnectedLimbs));
            check(format!("{n} StrengthMultipliers"), format!("{:?}", a.StrengthMultipliers), format!("{:?}", b.StrengthMultipliers));
            check(format!("{n} species/blood/decal"), format!("{} {} {}", a.SpeciesIdentity, a.BloodLiquidType, a.BloodDecal), format!("{} {} {}", b.SpeciesIdentity, b.BloodLiquidType, b.BloodDecal.replace(' ', "_")));
            check(format!("{n} gore"), format!("{} {} {:?}", a.goreString, a.goreStringJoint, a.gore), format!("{} {} {:?}", b.goreString, b.goreStringJoint, b.gore));
            check(format!("{n} vital"), format!("{:?}", want.vital_parts.get(&l)), format!("{:?}", g.vital_parts.get(&l)));
            let (mut a, mut b) = (a.clone(), b.clone());
            diff(&n, a.fields(), b.fields(), &mut out);
            let (mut a, mut b) = (want.circulations[l].clone(), g.circulations[l].clone());
            check(format!("{n} circulation links"), format!("{:?} {:?} {:?}", a.PushesTo, a.LocalArteryRects, a.Source), format!("{:?} {:?} {:?}", b.PushesTo, b.LocalArteryRects, b.Source));
            diff(&format!("{n} circulation"), a.fields(), b.fields(), &mut out);
            let (mut a, mut b) = (want.nodes[l].clone(), g.nodes[l].clone());
            check(format!("{n} node links"), format!("{:?}", a.Connections), format!("{:?}", b.Connections));
            diff(&format!("{n} node"), a.fields(), b.fields(), &mut out);
            let (mut a, mut b) = (want.skins[l].clone(), g.skins[l].clone());
            check(format!("{n} skin links"), format!("{:?}", a.adjacentLimbs), format!("{:?}", b.adjacentLimbs));
            diff(&format!("{n} skin"), a.fields(), b.fields(), &mut out);
            if let (Some(a), Some(b)) = (want.gforces.get(l), g.gforces.get(l)) {
                let (mut a, mut b) = (a.clone(), b.clone());
                diff(&format!("{n} gforce"), a.fields(), b.fields(), &mut out);
            }
        }
        for (i, b) in g.physicals.iter().enumerate() {
            let Some(a) = want.physicals.iter().find(|a| a.id == b.id) else {
                check(format!("physical {}", b.name), "recorded".into(), b.id.to_string());
                continue;
            };
            let n = format!("{} physical", b.name);
            check(format!("{n} colliders"), format!("{:?} {:?}", a.colliders, a.all_colliders), format!("{:?} {:?}", b.colliders, b.all_colliders));
            check(format!("{n} Properties"), want.properties[a.Properties].name.clone(), g.properties[b.Properties].name.clone());
            check(format!("{n} rigidbody"), a.rigidbody.bodyType.to_string(), b.rigidbody.bodyType.to_string());
            check(format!("{n} transform"), format!("{:?}", a.transform), format!("{:?}", b.transform));
            check(format!("{n} parent"), format!("{:?}", a.parent.map(|t| want.transform_nodes[t].id)), format!("{:?}", b.parent.map(|t| g.transform_nodes[t].id)));
            check(format!("{n} grid count"), format!("{:?}", a.LocalColliderGridPoints.as_ref().map(Vec::len)), format!("{:?}", b.LocalColliderGridPoints.as_ref().map(Vec::len)));
            check(
                format!("{n} misc"),
                format!("{} {} {} {} {} {} {:?}", a.has_fire_map, a.MaximumStabVictims, a.LooseEndSize, a.StabReleaseSpeedModifier, a.StabWoundSizeMultiplier, a.OverrideImpactSounds, a.sprite_bounds),
                format!("{} {} {} {} {} {} {:?}", b.has_fire_map, b.MaximumStabVictims, b.LooseEndSize, b.StabReleaseSpeedModifier, b.StabWoundSizeMultiplier, b.OverrideImpactSounds, b.sprite_bounds),
            );
            let (mut a, mut b) = (a.clone(), g.physicals[i].clone());
            diff(&n, a.fields(), b.fields(), &mut out);
        }
        for b in &g.properties {
            match want.properties.iter().find(|a| a.name == b.name) {
                Some(a) => check(format!("properties {}", a.name), format!("{a:?}"), format!("{b:?}")),
                None => check(format!("properties {}", b.name), "recorded".into(), "missing".into()),
            }
        }
        check("hinge count".into(), want.hinges.len().to_string(), g.hinges.len().to_string());
        for (h, (a, b)) in want.hinges.iter().zip(&g.hinges).enumerate() {
            let show = |gm: &Game, x: &Hinge| format!("{} {:?} {} {:?} {} {} {:?} {}", x.useMotor, x.motor, x.useLimits, x.limits, x.breakForce, x.breakTorque, x.connected.map(|c| gm.limb_of_body(c)), x.enabled);
            check(format!("hinge {h}"), show(&want, a), show(&g, b));
        }
        check("grip count".into(), want.grips.len().to_string(), g.grips.len().to_string());
        for (k, (a, b)) in want.grips.iter().zip(&g.grips).enumerate() {
            check(format!("grip {k}"), format!("{} {:?}", want.physicals[a.PhysicalBehaviour].id, a.CurrentlyHolding), format!("{} {:?}", g.physicals[b.PhysicalBehaviour].id, b.CurrentlyHolding));
            let (mut a, mut b) = (a.clone(), b.clone());
            diff(&format!("grip {k}"), a.fields(), b.fields(), &mut out);
        }
        let node = |g: &Game, t: usize| format!("{} {:?} {:?}", g.transform_nodes[t].id, g.transform_nodes[t].trs, g.transform_nodes[t].parent.map(|p| g.transform_nodes[p].id));
        // The recording lists the map's nodes after the Person's.
        let k = g.transform_nodes.len().min(want.transform_nodes.len());
        check("transform nodes".into(), (0..k).map(|t| node(&want, t)).collect::<Vec<_>>().join(" | "), (0..g.transform_nodes.len()).map(|t| node(&g, t)).collect::<Vec<_>>().join(" | "));
        let (a, b) = (&want.people[0], &g.people[person]);
        check("person".into(), format!("{} {} {} {:?}", a.id, a.BoneBreakClips, a.DismembermentClips, a.localScale), format!("{} {} {} {:?}", b.id, b.BoneBreakClips, b.DismembermentClips, b.localScale));
        check("pose count".into(), a.Poses.len().to_string(), b.Poses.len().to_string());
        check("person runtime links".into(), format!("{} {:?} {:?} {:?}", names(&want, &a.Limbs), a.Head, a.LinkedPoses, a.ActivePose), format!("{} {:?} {:?} {:?}", names(&g, &b.Limbs), b.Head, b.LinkedPoses, b.ActivePose));
        for (pa, pb) in a.Poses.iter().zip(&b.Poses) {
            let show = |p: &RagdollPose| {
                let angles: Vec<String> = p.Angles.iter().map(|x| match x.Animated {
                    true => format!("{} anim {} {} {} {} {} {} {} {} {:?}", x.Limb, x.PoseRigidityModifier, x.StartAngle, x.EndAngle, x.AnimationDuration, x.RandomInfluence, x.RandomSpeed, x.TimeOffset, x.Angle, x.AnimationCurve.as_ref().map(|c| (&c.keys, c.pre, c.post))),
                    false => format!("{} {} {}", x.Limb, x.PoseRigidityModifier, x.Angle),
                }).collect();
                format!("{} {} {} {} {} {} {} {} {} {angles:?}", p.Name, p.Rigidity, p.ForceMultiplier, p.ShouldStandUpright, p.DragInfluence, p.UprightForceMultiplier, p.ShouldStumble, p.State, p.AnimationSpeedMultiplier)
            };
            check(format!("pose {}", pa.Name), show(pa), show(pb));
        }
        let (mut a, mut b) = (a.clone(), b.clone());
        diff("person", a.fields(), b.fields(), &mut out);
        for l in &links {
            eprintln!("LINK {l}");
        }
        for l in &out {
            eprintln!("FIELD {l}");
        }
        assert!(links.is_empty(), "{} link mismatches", links.len());
    }
}
