//! The script oracle: loads the game's own state from a tools/recorder PPG_DUMP file into `Game`,
//! runs the rewritten scripts for one segment of the player loop and compares every field with the game.
use super::circulation::{RefFloat, LIQUIDS};
use super::misc::FixedIntervalDistributor;
use super::person::{LimbPose, RagdollPose};
use super::physical::ColliderBoolPair;
use super::*;
use crate::unity::transform::{Node as TransformNode, Trs};
use crate::b2::{math::Rot, world::BodyDef};
use crate::game_assets::{CurveCache, HumanRig};
use std::collections::HashMap;
use std::path::Path;

pub mod physics;

/// Transform.eulerAngles.z: Internal_ToEulerRad normalizes (divides by the length), then
/// QuaternionToEuler (kOrderUnityDefault, CRT atan2f of the z row), then Internal_MakePositive
/// on the degrees. Matches all 1694 dumped transforms bit for bit.
pub fn euler_z(q: [f32; 4]) -> f32 {
    let length = (((q[0] * q[0] + q[1] * q[1]) + q[2] * q[2]) + q[3] * q[3]).sqrt();
    let [x, y, z, w] = q.map(|v| v / length);
    let (x2, y2, z2, w2) = (x * x, y * y, z * z, w * w);
    let (xy, wz) = (x * y, w * z);
    let a = wz + xy;
    let sin = a + a;
    let cos = ((y2 - z2) - x2) + w2;
    let degrees = crate::unity::atan2f(sin, cos) * mathf::RAD2DEG;
    let negative_flip = -0.0001 * mathf::RAD2DEG;
    if degrees < negative_flip {
        degrees + 360.0
    } else if degrees > 360.0 + negative_flip {
        degrees - 360.0
    } else {
        degrees
    }
}

pub struct Record {
    pub ty: String,
    pub name: String,
    pub id: i32,
    pub fields: Vec<(String, String)>,
}

impl Record {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
    fn req(&self, key: &str) -> Result<&str, String> {
        self.get(key).ok_or_else(|| format!("{} {}: no field {key}", self.ty, self.name))
    }
}

pub struct Snapshot {
    pub pt: String,
    pub snap: Vec<(String, String)>,
    pub records: Vec<Record>,
}

impl Snapshot {
    fn snap(&self, key: &str) -> &str {
        self.snap.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str()).unwrap()
    }
    pub fn of<'a>(&'a self, ty: &'a str) -> impl Iterator<Item = &'a Record> + 'a {
        self.records.iter().filter(move |r| r.ty == ty)
    }
}

/// Splits on spaces outside (), [] and {}.
pub fn split_top(s: &str, sep: char) -> Vec<&str> {
    let (mut depth, mut start, mut out) = (0i32, 0, Vec::new());
    for (i, c) in s.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ if c == sep && depth == 0 => {
                out.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&s[start..]);
    out
}

fn key_value(item: &str) -> (String, String) {
    let (k, v) = item.split_once('=').unwrap_or((item, ""));
    let k = k.strip_prefix('<').and_then(|k| k.strip_suffix(">k__BackingField")).unwrap_or(k);
    (k.to_string(), v.to_string())
}

pub fn parse(text: &str) -> Vec<Snapshot> {
    let mut out: Vec<Snapshot> = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("snap ") {
            let parts = split_top(rest, ' ');
            out.push(Snapshot { pt: parts[0].into(), snap: parts[1..].iter().map(|p| key_value(p)).collect(), records: Vec::new() });
        } else if let Some(rest) = line.strip_prefix("s ") {
            let parts = split_top(rest, ' ');
            let snap = out.last_mut().unwrap();
            assert_eq!(parts[0], snap.pt);
            snap.records.push(Record {
                ty: parts[1].into(),
                name: parts[2].into(),
                id: parts[3].parse().unwrap(),
                fields: parts[4..].iter().map(|p| key_value(p)).collect(),
            });
        }
    }
    out
}

/// `@Name#id` -> id; `null` -> None.
fn reference(text: &str) -> Option<i32> {
    if text == "null" {
        return None;
    }
    Some(text.rsplit_once('#').unwrap().1.parse().unwrap())
}

fn list(text: &str) -> Vec<&str> {
    let inner = &text[1..text.len() - 1];
    if inner.is_empty() { Vec::new() } else { split_top(inner, ';') }
}

/// A PhysicsMaterial2D asset's friction from the exported project.
fn material_friction(name: &str) -> Result<f32, String> {
    let path = format!("unity-export/ExportedProject/Assets/PhysicsMaterial2D/{name}.physicsMaterial2D");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    let v = text.lines().find_map(|l| l.trim().strip_prefix("friction: ")).ok_or(format!("{path}: no friction"))?;
    v.parse().map_err(|e| format!("{path}: {e}"))
}

fn f(text: &str) -> f32 {
    f32::from_bits(u32::from_str_radix(text, 16).unwrap_or_else(|_| panic!("bad float {text}")))
}

fn words(text: &str) -> Vec<u32> {
    text.split(',').map(|w| u32::from_str_radix(w, 16).unwrap()).collect()
}

fn floats<const N: usize>(text: &str) -> [f32; N] {
    let mut v = [0.0; N];
    v.parse(text).unwrap();
    v
}

/// The inner `Key=value` list of a `Type(Key=value ...)` dump.
fn inner_fields(text: &str) -> Vec<(String, String)> {
    let open = text.find('(').unwrap();
    split_top(&text[open + 1..text.len() - 1], ' ').into_iter().map(key_value).collect()
}

fn distributor(text: &str) -> FixedIntervalDistributor {
    let fields = inner_fields(text);
    let get = |k: &str| fields.iter().find(|(n, _)| n == k).unwrap().1.clone();
    FixedIntervalDistributor {
        timeStepAccumulator: f(&get("timeStepAccumulator")),
        MaxCycles: get("MaxCycles").parse().unwrap(),
        InterpolationTime: f(&get("InterpolationTime")),
        IntervalSeconds: f(&get("IntervalSeconds")),
    }
}

/// Loads every dumped field a script declares; a declared field missing from the dump is an error.
fn load_fields(fields: Vec<(&'static str, &mut dyn Field)>, r: &Record) -> Result<(), String> {
    for (name, field) in fields {
        field.parse(r.req(name)?).map_err(|e| format!("{} {} {name}: {e}", r.ty, r.name))?;
    }
    Ok(())
}

/// Indices of the records of one type, by instance id.
fn ids(s: &Snapshot, ty: &str) -> HashMap<i32, usize> {
    s.of(ty).enumerate().map(|(i, r)| (r.id, i)).collect()
}

fn body_type(t: i32) -> BodyType {
    match t {
        0 => BodyType::Dynamic,
        1 => BodyType::Kinematic,
        _ => BodyType::Static,
    }
}

impl Game {
    /// Builds the game from one snapshot. Limb-owned components keep the LimbBehaviour order.
    pub fn from_snapshot(s: &Snapshot, rig: &HumanRig) -> Result<Game, String> {
        let mut g = Game::default();
        g.DynamicSqrdVelocityThreshold = 2500.0;
        (g.velocityIterations, g.positionIterations) = (32, 32);
        g.load_time(s);
        g.world = World::new(V::new(0.0, -9.81));
        // The whole b2 world when the dump has it (the map's colliders, contacts and listener records too).
        let mut restored = match s.of("b2World").next() {
            Some(_) => Some(physics::restore(s, V::new(0.0, -9.81))?),
            None => None,
        };
        if let Some(r) = restored.as_mut() {
            g.world = std::mem::replace(&mut r.world, World::new(V::ZERO));
        }

        let props = ids(s, "PhysicalProperties");
        for r in s.of("PhysicalProperties") {
            let mut p = PhysicalProperties { name: r.name.clone(), ..Default::default() };
            p.Softness = f(r.req("Softness")?);
            p.Flammability = f(r.req("Flammability")?);
            p.Conducting = r.req("Conducting")? == "1";
            p.Burnrate = f(r.req("Burnrate")?);
            p.BurningTemperatureThreshold = f(r.req("BurningTemperatureThreshold")?);
            p.HeatTransferSpeedMultiplier = f(r.req("HeatTransferSpeedMultiplier")?);
            p.ImpactIntensityMutliplier = f(r.req("ImpactIntensityMutliplier")?);
            p.HitVolumeMultiplier = f(r.req("HitVolumeMultiplier")?);
            p.SparksOnSlide = r.req("SparksOnSlide")? == "1";
            p.SoftImpact = list(r.req("SoftImpact")?).len();
            p.HardImpact = list(r.req("HardImpact")?).len();
            p.SizzleSounds = match r.req("SizzleSounds")? {
                "null" => None,
                t => Some(list(t).len()),
            };
            p.Sharp = r.req("Sharp")? == "1";
            p.IgnoreStabResistance = r.req("IgnoreStabResistance")? == "1";
            p.SharpForceThresholdMultiplier = f(r.req("SharpForceThresholdMultiplier")?);
            // The dump has only ever listed empty SharpAxes; a non-empty element format is not known.
            if list(r.req("SharpAxes")?).len() != 0 {
                return Err("non-empty SharpAxes are not loaded yet".into());
            }
            p.friction = match (r.get("friction"), r.req("PhysicMaterial")?) {
                (Some(bits), _) => Some(f(bits)),
                (None, "null") => None,
                // Older recordings: the material asset's serialized value.
                (None, m) => Some(material_friction(&m[1..m.find('#').unwrap()])?),
            };
            g.properties.push(p);
        }

        let limb_ids = ids(s, "LimbBehaviour");
        let limb_names: Vec<String> = s.of("LimbBehaviour").map(|r| r.name.clone()).collect();
        let limb_by_name = |n: &str| limb_names.iter().position(|l| l == n);
        // Every limb-owned component must list in LimbBehaviour order.
        for ty in ["CirculationBehaviour", "ConnectedNodeBehaviour", "SkinMaterialHandler", "GForceMeasureBehaviour", "Rigidbody2D", "b2Body"] {
            let names: Vec<&String> = s.of(ty).map(|r| &r.name).collect();
            if names.iter().map(|n| n.as_str()).ne(limb_names.iter().map(|n| n.as_str())) {
                return Err(format!("{ty} order differs from LimbBehaviour"));
            }
        }
        let by_limb_component = |ty: &str| -> HashMap<i32, usize> { ids(s, ty) };
        let circ_ids = by_limb_component("CirculationBehaviour");
        let node_ids = by_limb_component("ConnectedNodeBehaviour");
        let skin_ids = by_limb_component("SkinMaterialHandler");

        // Bodies: the human's from b2Body, the map's as static placeholders.
        let rbs: Vec<&Record> = s.of("Rigidbody2D").collect();
        let mut limb_bodies = Vec::new();
        for (rb, raw) in rbs.iter().zip(s.of("b2Body")) {
            let id = match &restored {
                Some(r) => r.bodies[&format!("{}#{}", rb.name, rb.id)],
                None => {
                    let id = g.world.create_body(&BodyDef { kind: body_type(rb.req("bodyType")?.parse().unwrap()), ..Default::default() });
                    load_body(g.world.body_mut(id), raw)?;
                    id
                }
            };
            limb_bodies.push(id);
        }

        // dump10's chain was the root alone, without parent links: treated as an older recording.
        let chain: Vec<&Record> = s.of("TransformChain").filter(|r| r.get("parent").is_some()).collect();
        for r in &chain {
            let v3 = |k: &str| -> Result<[f32; 4], String> {
                let [x, y, z]: [f32; 3] = floats(r.req(k)?);
                Ok([x, y, z, 0.0])
            };
            let trs = Trs { t: v3("lpos")?, q: floats(r.req("lrot")?), s: v3("lscale")? };
            g.transform_nodes.push(TransformNode { id: r.id, trs, parent: None });
        }
        let node_of = |g: &Game, text: &str| text.contains('#').then(|| reference(text)).flatten().and_then(|id| g.transform_nodes.iter().position(|n| n.id == id));
        for (i, r) in chain.iter().enumerate() {
            g.transform_nodes[i].parent = node_of(&g, r.req("parent")?);
        }
        let phys_ids = ids(s, "PhysicalBehaviour");
        for r in s.of("PhysicalBehaviour") {
            let mut p = Physical { name: r.name.clone(), id: r.id, ..Default::default() };
            load_fields(p.fields(), r)?;
            p.isStatic = r.get("isStatic") == Some("1");
            p.Properties = *props.get(&reference(r.req("Properties")?).unwrap()).ok_or("unknown PhysicalProperties")?;
            p.LocalColliderGridPoints = match r.req("LocalColliderGridPoints")? {
                "null" => None,
                t => Some(list(t).into_iter().map(floats::<2>).collect()),
            };
            p.has_fire_map = r.req("LocalFirePropagationMap")? != "null";
            p.MaximumStabVictims = r.req("MaximumStabVictims")?.parse().map_err(|e| format!("{} MaximumStabVictims: {e}", r.name))?;
            p.LooseEndSize = f(r.req("LooseEndSize")?);
            p.OverrideImpactSounds = list(r.req("OverrideImpactSounds")?).len();
            p.audioSource = r.req("audioSource")? != "null";
            if let Some(rs) = &restored {
                let collider = |k: &String| k.split(':').next()?.rsplit_once('#').filter(|(o, _)| *o == r.name).map(|(_, id)| id.parse::<i32>().unwrap());
                p.collider = rs.fixtures.keys().find_map(collider).unwrap_or(0);
            }
            match limb_by_name(&r.name) {
                Some(l) => {
                    let rb = rbs[l];
                    p.rigidbody = Rigidbody {
                        body: limb_bodies[l],
                        bodyType: rb.req("bodyType")?.parse().unwrap(),
                        collisionDetectionMode: rb.req("cdm")?.parse().unwrap(),
                        interpolation: rb.req("interpolation")?.parse().unwrap(),
                        movement: s.of("b2Body").nth(l).and_then(|r| r.get("move")).map_or(MovementState::default(), |w| movement_state(&words(w))),
                    };
                    // Older recordings lack the scene's flag: a non-static body out of the list was just (re)added.
                    let changed = s.of("b2Body").nth(l).and_then(|r| r.get("hierarchyChanged"));
                    g.rigidbody_hierarchy_changed |= changed.map_or(!p.rigidbody.movement.listed && p.rigidbody.bodyType != 2, |v| v == "1");
                    p.parent = rb.get("parent").and_then(|t| node_of(&g, t));
                    p.transform = Pose {
                        position: floats(rb.req("tpos")?),
                        rotation: floats(rb.req("trot")?),
                        localScale: floats(rb.req("scale")?),
                    };
                }
                None => {
                    let body = match restored.as_ref().and_then(|rs| rs.bodies.iter().find(|(n, _)| n.rsplit_once('#').map(|(o, _)| o) == Some(r.name.as_str()))) {
                        Some((_, &b)) => b,
                        None => g.world.create_body(&BodyDef::default()),
                    };
                    p.rigidbody = Rigidbody { body, bodyType: 2, collisionDetectionMode: 0, interpolation: 0, movement: MovementState::default() };
                }
            }
            g.physicals.push(p);
        }
        // Collider references resolve by GameObject name (each object has one collider).
        let phys_names: Vec<String> = g.physicals.iter().map(|p| p.name.clone()).collect();
        for (i, r) in s.of("PhysicalBehaviour").enumerate() {
            for (slot, item) in list(r.req("onCollisionStayBuffer")?).into_iter().enumerate() {
                if item == "null" {
                    continue;
                }
                let fields = inner_fields(item);
                let coll = &fields.iter().find(|(k, _)| k == "Coll").unwrap().1;
                let name = coll.trim_start_matches('@').rsplit_once('#').unwrap().0;
                let other = phys_names.iter().position(|n| n == name).ok_or_else(|| format!("collider {name}"))?;
                let active = fields.iter().find(|(k, _)| k == "Active").unwrap().1 == "1";
                g.physicals[i].onCollisionStayBuffer[slot] = Some(ColliderBoolPair { Active: active, Coll: other });
            }
        }

        // Hinges, from the HingeJoint2D, b2RevoluteJoint and the rig's anchors.
        let hinge_names: Vec<String> = s.of("HingeJoint2D").map(|r| r.name.clone()).collect();
        for (r, raw) in s.of("HingeJoint2D").zip(s.of("b2RevoluteJoint")) {
            let l = limb_by_name(&r.name).ok_or("hinge without limb")?;
            let connected = limb_ids[&reference(r.req("connected")?).map(|id| rb_owner(s, id)).ok_or("unconnected hinge")??];
            let motor: Vec<&str> = r.req("motor")?.split(',').collect();
            let limits: Vec<&str> = r.req("limits")?.split(',').collect();
            let mut h = Hinge {
                joint: None,
                useMotor: motor[0] == "1",
                motor: [f(motor[1]), f(motor[2])],
                useLimits: limits[0] == "1",
                limits: [f(limits[1]), f(limits[2])],
                breakForce: f(r.req("breakForce")?),
                breakTorque: f(r.req("breakTorque")?),
                connected,
                anchor: [0.0; 2],
                connectedAnchor: [0.0; 2],
                enabled: r.req("enabled")? == "1",
                destroyed: false,
            };
            let j = raw.req("joint")?;
            if let Some(rs) = &restored {
                h.joint = rs.joints.get(&format!("{}#{}", r.name, r.id)).copied();
            } else if j != "null" {
                let w = words(j);
                let fl = |i: usize| f32::from_bits(w[i]);
                let def = crate::b2::revolute::RevoluteDef {
                    local_anchor_a: V::new(fl(0), fl(1)),
                    local_anchor_b: V::new(fl(2), fl(3)),
                    enable_motor: w[8] & 0xff != 0,
                    max_motor_torque: fl(9),
                    motor_speed: fl(10),
                    enable_limit: w[11] & 0xff != 0,
                    reference_angle: fl(12),
                    lower_angle: fl(13),
                    upper_angle: fl(14),
                };
                let id = g.world.create_joint(def, limb_bodies[l], limb_bodies[connected], false);
                let rev = &mut g.world.joints[id].as_mut().unwrap().revolute;
                rev.impulse = [fl(4), fl(5), fl(6)];
                rev.motor_impulse = fl(7);
                h.joint = Some(id);
            }
            let _ = rig;
            // ponytail: the recorder does not dump m_JointBreakLimits; finite-limit hinges list in hinge order.
            if h.breakForce != f32::INFINITY || h.breakTorque != f32::INFINITY {
                g.joint_break_limits.push(g.hinges.len());
            }
            g.hinges.push(h);
        }

        for (i, r) in s.of("LimbBehaviour").enumerate() {
            let mut l = Limb { name: r.name.clone(), id: r.id, ..Default::default() };
            load_fields(l.fields(), r)?;
            l.PhysicalBehaviour = phys_ids[&reference(r.req("PhysicalBehaviour")?).unwrap()];
            l.Person = 0;
            l.Joint = hinge_names.iter().position(|n| *n == r.name);
            l.GripBehaviour = None;
            let gore = reference(r.req("goreString")?);
            l.goreString = gore.is_some();
            // GoreStringBehaviour.enabled is not dumped; nothing in the game disables it.
            l.goreStringEnabled = true;
            l.goreStringJoint = gore.is_some_and(|id| s.of("GoreStringBehaviour").any(|g| g.id == id && g.req("Joint").is_ok_and(|j| j != "null")));
            if let Some(g) = gore.and_then(|id| s.of("GoreStringBehaviour").find(|g| g.id == id)) {
                let line = |k: &str| -> Result<[f32; 4], String> {
                    let fl = inner_fields(g.req(k)?);
                    let part = |n: &str| -> Result<[f32; 2], String> { Ok(floats(&fl.iter().find(|(m, _)| m == n).ok_or("LineSegment point")?.1)) };
                    let (a, b) = (part("A")?, part("B")?);
                    Ok([a[0], a[1], b[0], b[1]])
                };
                l.gore = super::limb::GoreString {
                    Other: reference(g.req("Other")?).map(|id| rb_owner(s, id).map(|lid| limb_ids[&lid])).transpose()?,
                    JointDampening: f(g.req("JointDampening")?),
                    JointFreq: f(g.req("JointFreq")?),
                    JointBreakThreshold: f(g.req("JointBreakThreshold")?),
                    InvincibilityTime: f(g.req("InvincibilityTime")?),
                    OriginLine: line("OriginLine")?,
                    OtherLine: line("OtherLine")?,
                    HadJointWhenSerialised: g.req("HadJointWhenSerialised")? == "1",
                    TissueSnapClips: match g.req("TissueSnapClips")? {
                        "null" => 0,
                        t => list(t).len(),
                    },
                    AudioSource: g.req("AudioSource")? != "null",
                    ..Default::default()
                };
            }
            l.ConnectedLimbs = list(r.req("ConnectedLimbs")?).into_iter().map(|t| limb_ids[&reference(t).unwrap()]).collect();
            l.NearestLimbToBrain = reference(r.req("NearestLimbToBrain")?).map(|id| limb_ids[&id]);
            l.StrengthMultipliers = list(r.req("StrengthMultipliers")?).into_iter().map(f).collect();
            l.SpeciesIdentity = r.req("SpeciesIdentity")?.trim_matches('"').into();
            l.BloodLiquidType = r.req("BloodLiquidType")?.trim_matches('"').into();
            if circ_ids.len() != 0 && i >= circ_ids.len() {
                return Err("limb without circulation".into());
            }
            g.limbs.push(l);
        }
        let grip_ids = ids(s, "GripBehaviour");
        for r in s.of("GripBehaviour") {
            let mut gr = Grip::default();
            load_fields(gr.fields(), r)?;
            gr.PhysicalBehaviour = phys_ids[&reference(r.req("PhysicalBehaviour")?).unwrap()];
            gr.CurrentlyHolding = reference(r.req("CurrentlyHolding")?).map(|id| phys_ids[&id]);
            let l = limb_by_name(&r.name).ok_or("grip without limb")?;
            g.limbs[l].GripBehaviour = Some(g.grips.len());
            g.grips.push(gr);
        }
        let _ = grip_ids;
        // ponytail: gore string joints (severed limbs) are not rewritten; their Update returns while Joint is null.
        if s.of("GoreStringBehaviour").any(|r| r.get("Joint").is_some_and(|j| j != "null")) {
            return Err("gore string joints are not rewritten yet".into());
        }

        for r in s.of("CirculationBehaviour") {
            let mut c = Circulation::default();
            load_fields(c.fields(), r)?;
            c.Source = reference(r.req("Source")?).map(|id| circ_ids[&id]);
            c.PushesTo = list(r.req("PushesTo")?).into_iter().map(|t| circ_ids[&reference(t).unwrap()]).collect();
            c.LocalArteryRects = match r.req("LocalArteryRects")? {
                "null" => None,
                t => Some(list(t).into_iter().map(bounds).collect()),
            };
            for entry in list(r.req("LiquidDistribution")?) {
                let (key, value) = entry.split_once("):RefFloat(").ok_or("bad liquid entry")?;
                let class = key.split('(').next().unwrap();
                let id = LIQUIDS.iter().position(|(_, c)| *c == class).ok_or_else(|| format!("liquid class {class}"))?;
                let v = inner_fields(&format!("RefFloat({value}"));
                let get = |k: &str| f(&v.iter().find(|(n, _)| n == k).unwrap().1);
                c.LiquidDistribution.insert(id, RefFloat { Previous: get("Previous"), raw: get("raw") });
            }
            g.circulations.push(c);
        }
        for r in s.of("ConnectedNodeBehaviour") {
            let mut n = Node::default();
            load_fields(n.fields(), r)?;
            n.Connections = list(r.req("Connections")?).into_iter().map(|t| reference(t).map(|id| node_ids[&id])).collect();
            g.nodes.push(n);
        }
        for r in s.of("SkinMaterialHandler") {
            let mut k = Skin::default();
            load_fields(k.fields(), r)?;
            // Older recordings cut lists at 64 entries ("..."); the rest keep their Awake values.
            for (i, t) in list(r.req("damagePoints")?).into_iter().enumerate().filter(|(_, t)| *t != "...") {
                k.damagePoints[i] = floats::<4>(t);
            }
            for (i, t) in list(r.req("damagePointTimeStamps")?).into_iter().enumerate().filter(|(_, t)| *t != "...") {
                k.damagePointTimeStamps[i] = f(t);
            }
            k.intervalDistributor = distributor(r.req("intervalDistributor")?);
            k.adjacentLimbs = list(r.req("adjacentLimbs")?).into_iter().map(|t| skin_ids[&reference(t).unwrap()]).collect();
            g.skins.push(k);
        }
        for r in s.of("GForceMeasureBehaviour") {
            let mut gf = GForce::default();
            load_fields(gf.fields(), r)?;
            g.gforces.push(gf);
        }
        let splat_ids = ids(s, "FancyBloodSplatController");
        for r in s.of("FancyBloodSplatController") {
            let mut sp = FancyBloodSplat::default();
            load_fields(sp.fields(), r)?;
            if list(r.req("particles")?).len() != 0 {
                return Err("splat particles are not loaded yet".into());
            }
            g.splats.push(sp);
        }
        for r in s.of("BleedingParticleBehaviour") {
            let mut b = BleedingParticle::default();
            load_fields(b.fields(), r)?;
            b.CirculationBehaviour = circ_ids[&reference(r.req("CirculationBehaviour")?).unwrap()];
            b.PushingTo = reference(r.req("PushingTo")?).map(|id| circ_ids[&id]);
            b.BloodSplatController = reference(r.req("BloodSplatController")?).map(|id| splat_ids[&id]);
            b.DripSounds = match r.req("DripSounds")? {
                "null" => None,
                t => Some(list(t).len()),
            };
            b.dripSoundClock = distributor(r.req("dripSoundClock")?);
            g.bleeding.push(b);
        }
        // Older recordings lack the map scripts.
        let array_len = |text: &str| if text == "null" { None } else { Some(list(text).len()) };
        for r in s.of("PlayAudioAtRandom") {
            let mut a = super::PlayAudioAtRandom::default();
            load_fields(a.fields(), r)?;
            a.clips = array_len(r.req("Clips")?);
            a.isPlaying = r.req("playing")? == "1";
            g.ambient_audio.push(a);
        }
        for r in s.of("MapLightBehaviour") {
            let mut m = super::MapLight::default();
            load_fields(m.fields(), r)?;
            m.switches = array_len(r.req("Switches")?);
            m.hasAudio = r.req("audio")? == "1";
            m.wake = match r.req("wake")? {
                "none" => None,
                w => Some(f64::from_bits(u64::from_str_radix(w, 16).map_err(|e| e.to_string())?)),
            };
            g.map_lights.push(m);
        }

        let r = s.of("PersonBehaviour").next().ok_or("no person")?;
        let mut p = Person { name: r.name.clone(), id: r.id, ..Default::default() };
        load_fields(p.fields(), r)?;
        p.Limbs = list(r.req("Limbs")?).into_iter().map(|t| limb_ids[&reference(t).unwrap()]).collect();
        p.Head = reference(r.req("Head")?).map(|id| limb_ids[&id]);
        p.BoneBreakClips = list(r.req("BoneBreakClips")?).len();
        p.DismembermentClips = list(r.req("DismembermentClips")?).len();
        p.Poses = rig_poses(rig, &g.limbs)?;
        let pose_named = |t: &str| -> Result<usize, String> {
            let name = t.trim_start_matches("RagdollPose:").trim_matches('"').replace('_', " ");
            p.Poses.iter().position(|q| q.Name == name).ok_or(format!("pose {name}"))
        };
        p.ActivePose = match r.req("ActivePose")? {
            "null" => None,
            t => Some(pose_named(t)?),
        };
        let linked = r.req("LinkedPoses")?;
        for entry in list(&format!("[{}]", &linked[1..linked.len() - 1])) {
            let (k, v) = entry.split_once(':').unwrap();
            p.LinkedPoses[k.parse::<usize>().unwrap()] = pose_named(v)?;
        }
        p.localScale = match s.snap.iter().find(|(k, _)| k == "rootScale") {
            Some((_, v)) => floats(v),
            // Older recordings: a unit-scale limb's lossyScale, which can be 1 ulp off the root's localScale.
            None => {
                let lossy: [f32; 3] = floats(rbs[p.Head.unwrap()].req("lossy")?);
                [lossy[0], lossy[1], 1.0]
            }
        };
        // Older recordings lack the chain: the limbs hang off the root, spawned at the origin unrotated.
        // ponytail: their group objects (FrontLeg, ...) are missing, so limb Transforms can be an ulp off there.
        if g.transform_nodes.is_empty() {
            let [sx, sy, sz] = p.localScale;
            let trs = Trs { t: [0.0; 4], q: [0.0, 0.0, 0.0, 1.0], s: [sx, sy, sz, 0.0] };
            g.transform_nodes.push(TransformNode { id: 0, trs, parent: None });
            for &l in &p.Limbs {
                g.physicals[g.limbs[l].PhysicalBehaviour].parent = Some(0);
            }
        }
        g.people.push(p);
        Ok(g)
    }

    fn load_time(&mut self, s: &Snapshot) {
        self.time = Time {
            time: f(s.snap("time")),
            timeAsDouble: s.of("Time").next().and_then(|r| r.get("timeAsDouble")).map_or(f(s.snap("time")) as f64, |w| f64::from_bits(u64::from_str_radix(w, 16).unwrap())),
            fixedTime: f(s.snap("fixedTime")),
            fixedTimeAsDouble: s.of("Time").next().and_then(|r| r.get("fixedTimeAsDouble")).map_or(f(s.snap("fixedTime")) as f64, |w| f64::from_bits(u64::from_str_radix(w, 16).unwrap())),
            deltaTime: f(s.snap("dt")),
            fixedDeltaTime: f(s.snap("fixedDt")),
            maximumDeltaTime: f(s.snap("maxDt")),
            timeScale: f(s.snap("timeScale")),
            frameCount: s.snap("frame").parse().unwrap(),
        };
        // Older recordings lack it: the Default map prefab sets Ambient_temperature 20.
        self.AmbientTemperature = s.snap.iter().find(|(k, _)| k == "ambient").map_or(20.0, |(_, v)| f(v));
        // Older recordings lack it: the Default map's BoundingBoxBehaviour.
        if let Some((_, v)) = s.snap.iter().find(|(k, _)| k == "bounds") {
            let b = words(v).iter().map(|&w| f32::from_bits(w)).collect::<Vec<_>>();
            self.BoundingBox = super::Bounds { center: [b[0], b[1]], extents: [b[2], b[3]] };
        }
        let w = words(s.snap("random"));
        self.random = Random { state: [w[0], w[1], w[2], w[3]] };
    }

    /// The FixedUpdate scripts between CameraShakeBehaviour (FS) and CameraControlBehaviour (FM).
    pub fn fixed_update_scripts(&mut self) {
        self.run_pending_starts();
        for p in 0..self.physicals.len() {
            if self.physicals[p].enabled && !self.physicals[p].isDisintegrated {
                self.physical_fixed_update(p);
            }
        }
        for l in 0..self.limbs.len() {
            if self.limbs[l].enabled {
                self.limb_fixed_update(l);
            }
        }
        for p in 0..self.people.len() {
            self.person_fixed_update(p);
        }
        // Each limb's GripBehaviour runs right after its CirculationBehaviour (recorded call order).
        for l in 0..self.limbs.len() {
            self.circulation_fixed_update(l);
            if let Some(g) = self.limbs[l].GripBehaviour {
                self.grip_fixed_update(g);
            }
        }
        for s in 0..self.splats.len() {
            self.splat_fixed_update(s);
        }
        for l in 0..self.limbs.len() {
            self.gforce_fixed_update(l);
        }
    }

    /// The Update and LateUpdate scripts between SteamworksInitialiser.Update (US) and end of frame (UE),
    /// in the recorded call order.
    pub fn update_scripts(&mut self) {
        self.run_pending_starts();
        // UndoControllerBehaviour.Update, then the map's ambience emitter.
        for a in 0..self.ambient_audio.len() {
            self.play_audio_at_random_update(a);
        }
        for p in 0..self.physicals.len() {
            if self.physicals[p].enabled && !self.physicals[p].isDisintegrated {
                self.physical_update(p);
            }
        }
        for l in 0..self.limbs.len() {
            if self.limbs[l].enabled {
                self.limb_update(l);
            }
        }
        for p in 0..self.people.len() {
            self.person_update(p);
        }
        // Per limb: SkinMaterialHandler, BloodContainer, GripBehaviour, GoreStringBehaviour (no joints: returns).
        for l in 0..self.limbs.len() {
            self.skin_update(l);
            self.blood_container_update(l);
            if let Some(g) = self.limbs[l].GripBehaviour {
                self.grip_update(g);
            }
            self.gore_string_update(l);
        }
        for b in 0..self.bleeding.len() {
            self.bleeding_update(b);
        }
        self.node_controller_update();
        // CameraControlBehaviour.Update is the last Update; then the coroutines.
        self.map_light_coroutines();
        self.gore_string_coroutines();
        // LateUpdate; LimbBehaviour.ManagedLateUpdate is empty.
        for p in 0..self.physicals.len() {
            if self.physicals[p].enabled && !self.physicals[p].isDisintegrated {
                self.physical_late_update(p);
            }
        }
        for p in 0..self.people.len() {
            self.person_late_update(p);
        }
        for l in 0..self.limbs.len() {
            self.skin_late_update(l);
        }
        // Global.LateUpdate
        if self.time.frameCount % 2 == 0 {
            self.chunksUpToDate = false;
        }
        // SkinMaterialHandler.OnWillRenderObject (every limb is on screen in the recordings); bloodColor is render state.
        for l in 0..self.limbs.len() {
            let blood = super::circulation::liquid_color(super::circulation::liquid(&self.limbs[l].BloodLiquidType));
            self.get_computed_color(l, blood);
        }
    }
}

/// The Rigidbody2D record of a referenced Rigidbody2D id -> the owning LimbBehaviour id.
fn rb_owner(s: &Snapshot, rb_id: i32) -> Result<i32, String> {
    let name = &s.of("Rigidbody2D").find(|r| r.id == rb_id).ok_or("unknown rigidbody")?.name;
    Ok(s.of("LimbBehaviour").find(|r| &r.name == name).ok_or("rigidbody without limb")?.id)
}

fn bounds(text: &str) -> [f32; 6] {
    let fields = inner_fields(text);
    let c: [f32; 3] = floats(&fields[0].1);
    let e: [f32; 3] = floats(&fields[1].1);
    [c[0], c[1], c[2], e[0], e[1], e[2]]
}

/// b2Body words 0x00-0x58 and 0x90-0xb8 (UnityPlayer PDB layout).
fn load_body(b: &mut crate::b2::world::Body, r: &Record) -> Result<(), String> {
    let w = words(r.req("body")?);
    let m = words(r.req("mass")?);
    let fl = |v: u32| f32::from_bits(v);
    b.kind = match w[0] {
        0 => BodyType::Static,
        1 => BodyType::Kinematic,
        _ => BodyType::Dynamic,
    };
    b.flags = w[1] as u16;
    b.xf.p = V::new(fl(w[3]), fl(w[4]));
    b.xf.q = Rot { s: fl(w[5]), c: fl(w[6]) };
    b.sweep.local_center = V::new(fl(w[7]), fl(w[8]));
    b.sweep.c0 = V::new(fl(w[9]), fl(w[10]));
    b.sweep.c = V::new(fl(w[11]), fl(w[12]));
    b.sweep.a0 = fl(w[13]);
    b.sweep.a = fl(w[14]);
    b.sweep.alpha0 = fl(w[15]);
    b.linear_velocity = V::new(fl(w[16]), fl(w[17]));
    b.angular_velocity = fl(w[18]);
    b.force = V::new(fl(w[19]), fl(w[20]));
    b.torque = fl(w[21]);
    b.mass = fl(m[0]);
    b.inv_mass = fl(m[1]);
    b.axis = V::new(fl(m[2]), fl(m[3]));
    b.i = fl(m[4]);
    b.inv_i = fl(m[5]);
    b.linear_damping = fl(m[6]);
    b.angular_damping = fl(m[7]);
    b.gravity_scale = fl(m[8]);
    b.sleep_time = fl(m[9]);
    b.rigidbody.mass = fl(m[0]);
    Ok(())
}

/// The prefab's RagdollPoses with LimbPose entries bound to limbs by name.
fn rig_poses(rig: &HumanRig, limbs: &[Limb]) -> Result<Vec<RagdollPose>, String> {
    rig.poses
        .iter()
        .map(|p| {
            let angles = p
                .angles
                .iter()
                .map(|a| {
                    let limb = limbs.iter().position(|l| l.name == a.part).ok_or(format!("pose limb {}", a.part))?;
                    let anim = a.animation.as_ref();
                    Ok(LimbPose {
                        Limb: limb,
                        Animated: anim.is_some(),
                        PoseRigidityModifier: a.rigidity_modifier,
                        Angle: a.degrees(),
                        StartAngle: anim.map_or(a.degrees(), |x| x.start),
                        EndAngle: anim.map_or(a.degrees(), |x| x.end),
                        AnimationDuration: anim.map_or(1.0, |x| x.duration),
                        RandomInfluence: anim.map_or(0.0, |x| x.random_influence),
                        RandomSpeed: anim.map_or(1.0, |x| x.random_speed),
                        TimeOffset: anim.map_or(0.0, |x| x.time_offset),
                        AnimationCurve: anim.map(|x| x.curve.clone()),
                        cache: CurveCache::default(),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(RagdollPose {
                Name: p.name.clone(),
                Rigidity: p.rigidity,
                ForceMultiplier: p.force_multiplier,
                ShouldStandUpright: p.stand_upright,
                DragInfluence: p.drag_influence,
                UprightForceMultiplier: p.upright_multiplier,
                ShouldStumble: p.should_stumble,
                State: p.state,
                AnimationSpeedMultiplier: p.animation_speed,
                Angles: angles,
            })
        })
        .collect()
}

/// Compares every dumped field of the rewritten scripts with a snapshot; returns the mismatches.
pub fn compare(g: &mut Game, s: &Snapshot) -> Vec<String> {
    let mut out = Vec::new();
    let mut check = |what: String, fields: Vec<(&'static str, &mut dyn Field)>, r: &Record| {
        for (name, field) in fields {
            let ours = field.show();
            match r.get(name) {
                Some(theirs) if theirs == ours => {}
                Some(theirs) => out.push(format!("{what}.{name}: game {theirs} ours {ours}")),
                None => out.push(format!("{what}.{name}: not dumped")),
            }
        }
    };
    for (p, r) in g.physicals.iter_mut().zip(s.of("PhysicalBehaviour")) {
        check(format!("Physical {}", r.name), p.fields(), r);
    }
    for (l, r) in g.limbs.iter_mut().zip(s.of("LimbBehaviour")) {
        check(format!("Limb {}", r.name), l.fields(), r);
    }
    for (c, r) in g.circulations.iter_mut().zip(s.of("CirculationBehaviour")) {
        check(format!("Circulation {}", r.name), c.fields(), r);
    }
    for (n, r) in g.nodes.iter_mut().zip(s.of("ConnectedNodeBehaviour")) {
        check(format!("Node {}", r.name), n.fields(), r);
    }
    for (k, r) in g.skins.iter_mut().zip(s.of("SkinMaterialHandler")) {
        check(format!("Skin {}", r.name), k.fields(), r);
    }
    for (x, r) in g.gforces.iter_mut().zip(s.of("GForceMeasureBehaviour")) {
        check(format!("GForce {}", r.name), x.fields(), r);
    }
    for (x, r) in g.grips.iter_mut().zip(s.of("GripBehaviour")) {
        check(format!("Grip {}", r.name), x.fields(), r);
    }
    for (x, r) in g.splats.iter_mut().zip(s.of("FancyBloodSplatController")) {
        check(format!("Splat {}", r.id), x.fields(), r);
    }
    for (x, r) in g.bleeding.iter_mut().zip(s.of("BleedingParticleBehaviour")) {
        check(format!("Bleeding {}", r.id), x.fields(), r);
    }
    for (x, r) in g.people.iter_mut().zip(s.of("PersonBehaviour")) {
        check(format!("Person {}", r.name), x.fields(), r);
    }
    for (x, r) in g.ambient_audio.iter_mut().zip(s.of("PlayAudioAtRandom")) {
        check(format!("PlayAudioAtRandom {}", r.name), x.fields(), r);
    }
    for (x, r) in g.map_lights.iter_mut().zip(s.of("MapLightBehaviour")) {
        check(format!("MapLight {}", r.name), x.fields(), r);
    }
    for (x, r) in g.map_lights.iter().zip(s.of("MapLightBehaviour")) {
        let theirs = r.get("wake").unwrap_or("");
        let ours = x.wake.map_or("none".to_string(), |w| format!("{:016x}", w.to_bits()));
        if theirs != ours {
            out.push(format!("MapLight {}.wake: game {theirs} ours {ours}", r.name));
        }
    }
    // Hinge motors, limits and break values.
    for (h, r) in g.hinges.iter().zip(s.of("HingeJoint2D")) {
        let motor = format!("{},{},{}", h.useMotor as i32, h.motor[0].show(), h.motor[1].show());
        let limits = format!("{},{},{}", h.useLimits as i32, h.limits[0].show(), h.limits[1].show());
        for (k, ours) in [("motor", motor), ("limits", limits), ("breakForce", h.breakForce.show()), ("breakTorque", h.breakTorque.show())] {
            let theirs = r.get(k).unwrap_or("");
            if theirs != ours {
                out.push(format!("Hinge {}.{k}: game {theirs} ours {ours}", r.name));
            }
        }
    }
    // Body velocities and forces the scripts write.
    for (i, r) in s.of("b2Body").enumerate() {
        let b = g.world.body(g.physicals[g.limbs[i].PhysicalBehaviour].rigidbody.body);
        let w = words(r.req("body").unwrap());
        let ours = [b.flags as u32, b.linear_velocity.x.to_bits(), b.linear_velocity.y.to_bits(), b.angular_velocity.to_bits(), b.force.x.to_bits(), b.force.y.to_bits(), b.torque.to_bits()];
        let theirs = [w[1] & 0xffff, w[16], w[17], w[18], w[19], w[20], w[21]];
        if ours != theirs {
            out.push(format!("b2Body {}: game {theirs:08x?} ours {ours:08x?}", r.name));
        }
        if let Some(m) = r.get("move") {
            let w = words(m);
            let theirs = [&[(w[0] != u32::MAX) as u32], &w[4..25], &[w[31] & 0xff]].concat();
            let ours = movement_words(&g.physicals[g.limbs[i].PhysicalBehaviour].rigidbody.movement);
            if ours != theirs {
                out.push(format!("RigidbodyMovementState2D {}: game {theirs:08x?} ours {ours:08x?}", r.name));
            }
        }
        let sleep = words(r.req("mass").unwrap())[9];
        if sleep != b.sleep_time.to_bits() {
            out.push(format!("b2Body {}.sleepTime: game {sleep:08x} ours {:08x}", r.name, b.sleep_time.to_bits()));
        }
    }
    let w = words(s.snap("random"));
    if g.random.state != [w[0], w[1], w[2], w[3]] {
        out.push(format!("random: game {w:08x?} ours {:08x?}", g.random.state));
    }
    out
}

/// RigidbodyMovementState2D from the recorder's `move` words (Rigidbody2D+0x98 on).
fn movement_state(w: &[u32]) -> MovementState {
    let v = |i: usize| f32::from_bits(w[i]);
    let pose = |i: usize| ([v(i), v(i + 1), v(i + 2)], [v(i + 3), v(i + 4), v(i + 5), v(i + 6)]);
    MovementState { previous: pose(4), current: pose(11), body: pose(18), interpolating: w[31] & 0xff != 0, listed: w[0] != u32::MAX }
}

/// The inverse of movement_state: listed, words 4-24 and the flag byte.
fn movement_words(m: &MovementState) -> Vec<u32> {
    let mut out = vec![m.listed as u32];
    for (p, q) in [m.previous, m.current, m.body] {
        out.extend(p.iter().chain(&q).map(|f| f.to_bits()));
    }
    out.push(m.interpolating as u32);
    out
}

/// Each limb's Transform against the recording; needs the group objects between limb and root (TransformChain
/// parents, from dump11 on).
fn compare_transforms(g: &Game, chained: &Snapshot, to: &Snapshot) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let chain = chained.of("TransformChain").any(|r| r.get("parent").is_some());
    for (l, rb) in to.of("Rigidbody2D").enumerate().filter(|_| chain) {
        let t = &g.physicals[g.limbs[l].PhysicalBehaviour].transform;
        let (pos, rot): ([f32; 3], [f32; 4]) = (floats(rb.req("tpos")?), floats(rb.req("trot")?));
        let bits = |v: &[f32]| v.iter().map(|f| f.to_bits()).collect::<Vec<_>>();
        if bits(&pos) != bits(&t.position) || bits(&rot) != bits(&t.rotation) {
            out.push(format!("Transform {}: game {:08x?} {:08x?} ours {:08x?} {:08x?}", rb.name, bits(&pos), bits(&rot), bits(&t.position), bits(&t.rotation)));
        }
    }
    Ok(out)
}

/// The physics pass's results at FE: the b2 world, the Collision2D records and each awake body's Transform.
fn compare_physics(g: &mut Game, from: &Snapshot, to: &Snapshot) -> Result<Vec<String>, String> {
    // A second restore of the same snapshot builds the same ids, so its name maps fit the game's world.
    let mut r = physics::restore(from, V::new(0.0, -9.81))?;
    r.world = std::mem::replace(&mut g.world, World::new(V::ZERO));
    let mut out = physics::compare_world(&r, to)?;
    out.extend(physics::compare_contacts2d(&r, to)?);
    g.world = std::mem::replace(&mut r.world, World::new(V::ZERO));
    out.extend(compare_transforms(g, from, to)?);
    Ok(out)
}

/// --loop-test: load the first snapshot with the b2 world once, then run the game loop through the rest of the
/// recording (FixedUpdate scripts, physics pass, Update) taking only Time from it, comparing at every point.
pub fn loop_test(dump: &Path, rig: &HumanRig) -> Result<(), String> {
    let text = std::fs::read_to_string(dump).map_err(|e| e.to_string())?;
    let snaps = parse(&text);
    if !snaps.iter().any(|s| s.of("Contacts2D").next().is_some()) {
        return Err("--loop-test needs a recording with Contacts2D (dump12 on)".into());
    }
    let start = snaps.iter().position(|s| s.pt == "FS" && s.of("b2World").next().is_some()).ok_or("no FS snapshot")?;
    let mut g = Game::from_snapshot(&snaps[start], rig)?;
    for (n, pair) in snaps[start..].windows(2).enumerate() {
        // Only Time comes from the recording: the scripts' Random state carries over.
        let random = g.random.clone();
        g.load_time(&pair[0]);
        g.random = random;
        let mut diffs = Vec::new();
        match (pair[0].pt.as_str(), pair[1].pt.as_str()) {
            ("FS", "FM") => g.fixed_update_scripts(),
            ("FM", "FE") => {
                g.simulate_physics();
                diffs = compare_physics(&mut g, &pair[0], &pair[1])?;
            }
            ("US", "UE") => g.update_scripts(),
            // PreUpdate's Physics2DUpdate at the frame's time.
            ("FE", "US") => {
                let random = g.random.clone();
                g.load_time(&pair[1]);
                g.random = random;
                g.interpolate_bodies();
                diffs = compare_transforms(&g, &snaps[start], &pair[1])?;
            }
            _ => {}
        }
        diffs.extend(compare(&mut g, &pair[1]));
        if !diffs.is_empty() {
            eprintln!("diverged at {} frame {} (after {n} matching points, {}→{}):", pair[1].pt, pair[1].snap("frame"), pair[0].pt, pair[1].pt);
            for d in diffs.iter().take(25) {
                eprintln!("  {d}");
            }
            return Err(format!("{} diffs", diffs.len()));
        }
    }
    println!("loop points {}, all match", snaps.len() - start);
    Ok(())
}

/// --script-test: for every FS snapshot, load the game's state, run the FixedUpdate scripts and compare at FM;
/// for every FM snapshot with the b2 world, run the physics pass with its callbacks and compare at FE.
pub fn script_test(dump: &Path, rig: &HumanRig) -> Result<(), String> {
    let text = std::fs::read_to_string(dump).map_err(|e| e.to_string())?;
    let snaps = parse(&text);
    let (mut segments, mut failed) = (0, 0);
    let mut kinds: std::collections::BTreeMap<String, usize> = Default::default();
    // The physics pass needs the contacts' Collision2D records and the scene's simulation id even on contactless
    // frames (recorded from dump12 on).
    let records = snaps.iter().any(|s| s.of("Contacts2D").next().is_some());
    for pair in snaps.windows(2) {
        let update = pair[0].pt == "US" && pair[1].pt == "UE";
        let physics = records && pair[0].pt == "FM" && pair[1].pt == "FE" && pair[0].of("b2World").next().is_some();
        if !update && !physics && (pair[0].pt != "FS" || pair[1].pt != "FM") {
            continue;
        }
        segments += 1;
        let mut g = Game::from_snapshot(&pair[0], rig)?;
        // Transform.eulerAngles as the scripts read it.
        for r in pair[0].of("Rigidbody2D") {
            let q: [f32; 4] = floats(r.req("trot")?);
            let theirs = f(r.req("euler")?);
            if euler_z(q).to_bits() != theirs.to_bits() {
                return Err(format!("euler {}: game {:08x} ours {:08x}", r.name, theirs.to_bits(), euler_z(q).to_bits()));
            }
        }
        let mut diffs = Vec::new();
        if update {
            g.update_scripts();
        } else if physics {
            g.simulate_physics();
            diffs = compare_physics(&mut g, &pair[0], &pair[1])?;
        } else {
            g.fixed_update_scripts();
        }
        diffs.extend(compare(&mut g, &pair[1]));
        if let Ok(path) = std::env::var("PPG_DIFFS") {
            use std::io::Write;
            let mut out = std::fs::OpenOptions::new().create(true).append(true).open(path).unwrap();
            for d in &diffs {
                writeln!(out, "{segments} {d}").unwrap();
            }
        }
        for d in &diffs {
            // "Type Name.field: ..." -> "Type.field"
            let head = d.split(':').next().unwrap();
            let ty = head.split(' ').next().unwrap();
            let field = head.rsplit('.').next().unwrap();
            *kinds.entry(format!("{} {ty}.{field}", pair[0].pt)).or_default() += 1;
        }
        if !diffs.is_empty() {
            failed += 1;
            if failed <= 1 {
                eprintln!("segment {} {} ({} diffs):", pair[0].pt, pair[0].snap("frame"), diffs.len());
                for d in diffs.iter().take(25) {
                    eprintln!("  {d}");
                }
            }
        }
    }
    for (k, n) in &kinds {
        println!("  {n:5} {k}");
    }
    println!("script segments {segments}, failed {failed}");
    if failed == 0 && segments > 0 { Ok(()) } else { Err(format!("{failed} of {segments} segments differ")) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_nested() {
        assert_eq!(split_top("a=1 b=X(c=1 d=2) e=[f;g h]", ' '), ["a=1", "b=X(c=1 d=2)", "e=[f;g h]"]);
        assert_eq!(key_value("<IsOnFloor>k__BackingField=0"), ("IsOnFloor".into(), "0".into()));
        assert_eq!(super::super::circulation::liquid("BLOOD"), 0);
    }
}
