//! An instantiated prefab's MonoBehaviours as the Game's component arenas. Each script gets its arena entry with
//! its serialized fields, and the rewrite's links (arena indices) stand for its component references: the
//! serialized ones resolve through the prefab, the GetComponent ones through the object's components. Runtime
//! state stays at its field initializer for Awake/OnEnable/Start to set.

use super::circulation::Circulation;
use super::limb::{GoreString, Limb};
use super::misc::{GForce, Grip, Node, Skin};
use super::person::{LimbPose, Person, RagdollPose};
use super::physical::{Physical, PhysicalProperties, SharpAxis};
use super::spawn::{load_values, Instance};
use super::*;
use crate::assets::classes::PPtr;
use crate::assets::components::{JointKind, PhysicsMaterial2D, Sprite};
use crate::assets::db::{AssetDb, Component, ObjRef, Prefab};
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
fn sprite_bounds(s: &Sprite) -> ([f32; 3], [f32; 3]) {
    let (w, h) = (s.rect[2] / s.pixels_to_units, s.rect[3] / s.pixels_to_units);
    let (ex, ey) = (w * 0.5, h * 0.5);
    ([ex - s.pivot[0] * w, ey - s.pivot[1] * h, 0.0], [ex, ey, 0.1])
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
    /// the bodies reach them. Returns the Person's index.
    pub fn link_instance(&mut self, db: &mut AssetDb, prefab: &Prefab, inst: &Instance) -> Result<usize, String> {
        let file = prefab.objects[0].game_object.file;
        let mut l = Linker { prefab, inst, file, arenas: Arenas::default() };
        let n = prefab.objects.len();
        let person = self.people.len();

        // TransformChain: each body's parents up to the root, the first body to reach one listing it.
        for o in (0..n).filter(|o| inst.bodies.contains_key(o)) {
            let mut p = inst.objects[o].parent;
            while let Some(t) = p {
                if inst.bodies.contains_key(&t) {
                    return Err(format!("{}: a body under a body", prefab.objects[o].name));
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
                    Component::Joint2D(_) => unimplemented!("{}: a non-hinge Joint2D in a spawned prefab", prefab.objects[o].name),
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
            p.all_colliders = p.colliders.clone();
            p.all_colliders.sort_by(|a, b| b.cmp(a));
            p.Properties = match db.resolve(file, field(v, "Properties")?.pptr().ok_or("Properties")?) {
                Some(asset) => self.physical_properties(db, asset)?,
                None => unimplemented!("{}: PhysicalBehaviour without Properties", p.name),
            };
            let body = *inst.bodies.get(&o).ok_or_else(|| format!("{}: PhysicalBehaviour without a Rigidbody2D", p.name))?;
            let Some(Component::Rigidbody2D(rb)) = prefab.objects[o].components.iter().map(|(_, x)| x).find(|x| matches!(x, Component::Rigidbody2D(_))) else { unreachable!() };
            p.rigidbody = Rigidbody { body, bodyType: rb.body_type, collisionDetectionMode: rb.collision_detection, interpolation: rb.interpolate, movement: MovementState::default(), detached: None };
            // Rigidbody2D::Create set the scene's m_RigidbodyHierarchyChanged.
            self.rigidbody_hierarchy_changed |= rb.body_type != 2;
            let trs = inst.objects[o].trs;
            p.transform = Pose {
                position: inst.position(o),
                rotation: inst.rotation(o),
                localScale: [trs.s[0], trs.s[1], trs.s[2]],
                local: Some(([trs.t[0], trs.t[1], trs.t[2]], trs.q)),
            };
            p.parent = inst.objects[o].parent.map(|t| l.arenas.node[&t]);
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
                if r.draw_mode != 0 || r.flip_x || r.flip_y {
                    unimplemented!("{}: a sliced, tiled or flipped sprite", p.name);
                }
                let sprite = db.resolve(file, r.sprite).ok_or("a SpriteRenderer without a sprite")?;
                p.sprite_bounds = Some(sprite_bounds(&Sprite::read(db.data(sprite))?));
            }
            self.physicals.push(p);
        }

        for o in 0..n {
            for (c, (_, comp)) in prefab.objects[o].components.iter().enumerate() {
                let Component::Joint2D(j) = comp else { continue };
                let JointKind::Hinge { use_motor, motor_speed, max_motor_torque, use_limits, lower, upper } = j.kind else { unreachable!() };
                let connected = l.object_of(j.connected_body)?.ok_or("an unconnected hinge")?;
                let h = Hinge {
                    joint: inst.joints.get(&(o, c)).copied(),
                    useMotor: use_motor,
                    motor: [motor_speed, max_motor_torque],
                    useLimits: use_limits,
                    limits: [lower, upper],
                    breakForce: j.break_force,
                    breakTorque: j.break_torque,
                    connected: *l.arenas.limb.get(&connected).ok_or("a hinge to a body without a LimbBehaviour")?,
                    anchor: j.anchor,
                    connectedAnchor: inst.hinges.get(&(o, c)).map_or(j.connected_anchor, |s| s.connected_anchor),
                    enabled: j.enabled,
                    destroyed: false,
                };
                if h.breakForce != f32::INFINITY || h.breakTorque != f32::INFINITY {
                    unimplemented!("PhysicsScene2D m_JointBreakLimits order for a spawned hinge with a finite break limit");
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
            b.Person = person;
            b.Joint = (0..prefab.objects[o].components.len()).find_map(|k| l.arenas.hinge.get(&(o, k)).copied());
            b.GripBehaviour = l.arenas.grip.get(&o).copied();
            b.ConnectedLimbs = l.limbs(v, "ConnectedLimbs")?;
            b.NearestLimbToBrain = l.limb(field(v, "NearestLimbToBrain")?)?;
            b.StrengthMultipliers = array(v, "StrengthMultipliers")?.iter().map(|x| x.f32().ok_or("StrengthMultipliers")).collect::<Result<_, _>>()?;
            b.SpeciesIdentity = string(v, "SpeciesIdentity")?;
            b.BloodLiquidType = string(v, "BloodLiquidType")?;
            let decal = db.resolve(file, field(v, "BloodDecal")?.pptr().ok_or("BloodDecal")?).ok_or("LimbBehaviour without a BloodDecal")?;
            b.BloodDecal = db.mono(decal)?.ok_or("BloodDecal is not a MonoBehaviour")?.1.name;
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

        let (c, _, v) = script(prefab, 0, "PersonBehaviour").ok_or("the root has no PersonBehaviour")?;
        let mut p = Person { name: prefab.objects[0].name.clone(), id: inst.objects[0].components[c], ..Default::default() };
        load_values(p.fields(), v)?;
        p.Limbs = l.limbs(v, "Limbs")?;
        p.BoneBreakClips = array(v, "BoneBreakClips")?.len();
        p.DismembermentClips = array(v, "DismembermentClips")?.len();
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
        Ok(person)
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
        let person = g.link_instance(&mut db, &prefab, &inst).unwrap();
        // The Awake and Start passes over the queue 0x14 behaviours, sorted as AwakeFromLoadQueue sorts them (script
        // execution order, then the higher clone id).
        let header = super::super::dump::header(&root.join("reference-probe/game/dump31spawnrand.txt")).unwrap();
        g.random = crate::unity::Random { state: super::super::dump::spawn_random_state(&header).unwrap() };
        // The BleedingParticle prefab's DripSounds (Person.BleedingParticlePrefab) is 11 clips.
        g.people[person].BleedingDripSounds = Some(11);
        let mut order: Vec<(usize, usize)> = (0..prefab.objects.len())
            .flat_map(|o| (0..prefab.objects[o].components.len()).map(move |c| (o, c)))
            .filter(|&(o, c)| matches!(prefab.objects[o].components[c].1, Component::Script { .. }))
            .collect();
        order.sort_by_key(|&(o, c)| {
            let Component::Script { execution_order, .. } = &prefab.objects[o].components[c].1 else { unreachable!() };
            (*execution_order, std::cmp::Reverse(inst.objects[o].components[c]))
        });
        let class_of = |o: usize, c: usize| match &prefab.objects[o].components[c].1 {
            Component::Script { class, .. } => class.clone(),
            _ => unreachable!(),
        };
        let limb_of = |g: &Game, o: usize| (0..g.limbs.len()).find(|&l| g.limbs[l].Person == person && g.limbs[l].name == prefab.objects[o].name).unwrap();
        for &(o, c) in &order {
            match class_of(o, c).as_str() {
                "PersonBehaviour" => {
                    let scale = g.person_awake(person).unwrap();
                    let s = inst.objects[0].trs.s;
                    assert_eq!(scale.map(f32::to_bits), [s[0].to_bits(), s[1].to_bits(), s[2].to_bits()], "PersonBehaviour.Awake scale");
                }
                "SkinMaterialHandler" => g.skin_awake(limb_of(&g, o)),
                "PhysicalBehaviour" => g.physical_awake(g.limbs[limb_of(&g, o)].PhysicalBehaviour),
                "LimbBehaviour" => g.limb_awake(limb_of(&g, o)),
                "CirculationBehaviour" => g.circulation_awake(limb_of(&g, o)),
                _ => {}
            }
        }
        // PhysicalBehaviour.SetupSpawnPrefab (Instantiate of the Resources prefab) is not rewritten: off for the Start pass.
        // Mono's GetHashCode (Random.InitState(GetHashCode()) in PhysicalBehaviour.Start) is a heap property: from the recording.
        for p in 0..g.physicals.len() {
            let id = g.physicals[p].id;
            g.physicals[p].hash_code = want.physicals.iter().find(|a| a.id == id).map_or(0, |a| a.hash_code);
        }
        let spawn_particles: Vec<bool> = g.physicals.iter().map(|p| p.SpawnSpawnParticles).collect();
        g.physicals.iter_mut().for_each(|p| p.SpawnSpawnParticles = false);
        for &(o, c) in &order {
            let Some((_, _, v)) = script(&prefab, o, &class_of(o, c)) else { unreachable!() };
            match class_of(o, c).as_str() {
                "PersonBehaviour" => g.person_start(person),
                "HingeJointLimitAutofixBehaviour" => g.hinge_joint_limit_autofix_start(person),
                "CirculationBehaviour" => g.circulation_start(limb_of(&g, o)),
                "PhysicalBehaviour" => g.physical_start(g.limbs[limb_of(&g, o)].PhysicalBehaviour),
                "LimbBehaviour" => g.limb_start(limb_of(&g, o)),
                "ConnectedNodeBehaviour" => {
                    let keep: Option<Vec<bool>> = v.get("SerialisableConnectedNodes").and_then(Value::array).map(|a| a.iter().map(|x| x.bool().unwrap()).collect());
                    g.node_start(limb_of(&g, o), keep.as_deref());
                }
                "GripBehaviour" => g.grip_start(g.limbs[limb_of(&g, o)].GripBehaviour.unwrap()),
                "GoreStringBehaviour" => g.gore_string_start(limb_of(&g, o), field(v, "CreateJointOnStart").unwrap().bool().unwrap()),
                "ShatteredObjectGenerator" => assert!(!field(v, "GenerateOnStart").unwrap().bool().unwrap(), "ShatteredObjectGenerator.Start generates"),
                _ => {}
            }
        }
        for (p, on) in g.physicals.iter_mut().zip(spawn_particles) {
            p.SpawnSpawnParticles = on;
        }
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
            let show = |x: &Hinge| format!("{} {:?} {} {:?} {} {} {} {}", x.useMotor, x.motor, x.useLimits, x.limits, x.breakForce, x.breakTorque, x.connected, x.enabled);
            check(format!("hinge {h}"), show(a), show(b));
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
