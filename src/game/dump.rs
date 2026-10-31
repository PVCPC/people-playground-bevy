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
use std::io::BufRead;
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

#[derive(Clone)]
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
    pub(crate) fn req(&self, key: &str) -> Result<&str, String> {
        self.get(key).ok_or_else(|| format!("{} {}: no field {key}", self.ty, self.name))
    }
}

#[derive(Clone)]
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
    Snapshots { lines: text.as_bytes().lines(), pending: None, header: Vec::new() }.collect()
}

/// A recording's snapshots read one at a time (a drag recording is too large to hold parsed); the other lines
/// (the recorder's header) go to `header`.
pub struct Snapshots<R> {
    lines: std::io::Lines<R>,
    pending: Option<Snapshot>,
    pub header: Vec<String>,
}

pub fn snapshots(path: &Path) -> Result<Snapshots<std::io::BufReader<std::fs::File>>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Snapshots { lines: std::io::BufReader::with_capacity(1 << 20, file).lines(), pending: None, header: Vec::new() })
}

/// Only the recorder's header lines (collider and body layers come after the first snapshots).
pub fn header(path: &Path) -> Result<Vec<String>, String> {
    let mut s = snapshots(path)?;
    while let Some(Ok(line)) = s.lines.next() {
        if !line.starts_with("snap ") && !line.starts_with("s ") {
            s.header.push(line);
        }
    }
    Ok(s.header)
}

/// The recorder's `random_state=<s0> <s1> <s2> <s3>` header line: UnityEngine.Random.state just before the Person's Instantiate.
pub fn spawn_random_state(header: &[String]) -> Option<[u32; 4]> {
    let w: Vec<u32> = header.iter().find_map(|l| l.strip_prefix("random_state="))?.split(' ').map(|w| u32::from_str_radix(w, 16).unwrap()).collect();
    Some([w[0], w[1], w[2], w[3]])
}

impl<R: BufRead> Iterator for Snapshots<R> {
    type Item = Snapshot;
    fn next(&mut self) -> Option<Snapshot> {
        while let Some(Ok(line)) = self.lines.next() {
            if let Some(rest) = line.strip_prefix("snap ") {
                let parts = split_top(rest, ' ');
                let snap = Snapshot { pt: parts[0].into(), snap: parts[1..].iter().map(|p| key_value(p)).collect(), records: Vec::new() };
                if let Some(done) = self.pending.replace(snap) {
                    return Some(done);
                }
            } else if let Some(rest) = line.strip_prefix("s ") {
                let parts = split_top(rest, ' ');
                let snap = self.pending.as_mut().unwrap();
                assert_eq!(parts[0], snap.pt);
                snap.records.push(Record {
                    ty: parts[1].into(),
                    name: parts[2].into(),
                    id: parts[3].parse().unwrap(),
                    fields: parts[4..].iter().map(|p| key_value(p)).collect(),
                });
            } else {
                self.header.push(line);
            }
        }
        self.pending.take()
    }
}

/// `@Name#id` -> id; `null` and `destroyed` (a destroyed object, which reads as null) -> None.
fn reference(text: &str) -> Option<i32> {
    if text == "null" || text == "destroyed" {
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
    material_value(name, "friction")
}

/// A PhysicsMaterial2D asset's float field from the exported project.
fn material_value(name: &str, key: &str) -> Result<f32, String> {
    let path = format!("unity-export/ExportedProject/Assets/PhysicsMaterial2D/{name}.physicsMaterial2D");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    let v = text.lines().find_map(|l| l.trim().strip_prefix(&format!("{key}: "))).ok_or(format!("{path}: no {key}"))?;
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
        g.world = World::new(V::new(-0.0, -9.81));
        // The whole b2 world when the dump has it (the map's colliders, contacts and listener records too).
        let mut restored = match s.of("b2World").next() {
            Some(_) => Some(physics::restore(s, V::new(-0.0, -9.81))?),
            None => None,
        };
        if let Some(r) = restored.as_mut() {
            g.world = std::mem::replace(&mut r.world, World::new(V::ZERO));
            g.objects = r.objects.clone();
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
            p.Buoyancy = f(r.req("Buoyancy")?);
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
            // SharpAxis(Axis=x,y LowerLimit=f UpperLimit=f LooseUpperLimit=b LooseLowerLimit=b)
            for item in list(r.req("SharpAxes")?) {
                let v = inner_fields(item);
                let get = |k: &str| -> Result<&str, String> { v.iter().find(|(n, _)| n == k).map(|(_, x)| x.as_str()).ok_or(format!("SharpAxis.{k}")) };
                p.SharpAxes.push(super::physical::SharpAxis {
                    Axis: floats(get("Axis")?),
                    LowerLimit: f(get("LowerLimit")?),
                    UpperLimit: f(get("UpperLimit")?),
                    LooseUpperLimit: get("LooseUpperLimit")? == "1",
                    LooseLowerLimit: get("LooseLowerLimit")? == "1",
                });
            }
            p.LodgeStrengthMultiplier = f(r.req("LodgeStrengthMultiplier")?);
            p.StabSound = match r.req("StabSound")? {
                "null" => None,
                t => Some(list(t).len()),
            };
            p.friction = match (r.get("friction"), r.req("PhysicMaterial")?) {
                (Some(bits), _) => Some(f(bits)),
                (None, "null") => None,
                // Older recordings: the material asset's serialized value.
                (None, m) => Some(material_friction(&m[1..m.find('#').unwrap()])?),
            };
            p.bounciness = match r.req("PhysicMaterial")? {
                "null" => 0.0,
                m => material_value(&m[1..m.find('#').unwrap()], "bounciness")?,
            };
            // Older recordings lack these (only the ballistics read them).
            if let Some(bits) = r.get("Brittleness") {
                p.Brittleness = f(bits);
                p.BulletSpeedAbsorptionPower = f(r.req("BulletSpeedAbsorptionPower")?);
                p.ShotImpact = match r.req("ShotImpact")? {
                    "null" => None,
                    m => Some((None, &m[1..m.find('#').unwrap()] == "BloodPuff_poolable")),
                };
            }
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
            p.hash_code = r.get("hash").map_or(0, |h| h.parse().unwrap());
            p.Properties = *props.get(&reference(r.req("Properties")?).unwrap()).ok_or("unknown PhysicalProperties")?;
            p.LocalColliderGridPoints = match r.req("LocalColliderGridPoints")? {
                "null" => None,
                t => Some(list(t).into_iter().map(floats::<2>).collect()),
            };
            p.has_fire_map = r.req("LocalFirePropagationMap")? != "null";
            p.MaximumStabVictims = r.req("MaximumStabVictims")?.parse().map_err(|e| format!("{} MaximumStabVictims: {e}", r.name))?;
            p.LooseEndSize = f(r.req("LooseEndSize")?);
            p.StabReleaseSpeedModifier = f(r.req("StabReleaseSpeedModifier")?);
            p.StabWoundSizeMultiplier = f(r.req("StabWoundSizeMultiplier")?);
            p.OverrideImpactSounds = list(r.req("OverrideImpactSounds")?).len();
            p.audioSource = r.req("audioSource")? != "null";
            // From dump17 on: the SpriteRenderer's local AABB (center, extents).
            if let Some(b) = r.get("sbounds") {
                if r.req("sdraw")? != "0,0,0" {
                    return Err(format!("{}: a sliced, tiled or flipped sprite ({})", r.name, r.req("sdraw")?));
                }
                let [cx, cy, cz, ex, ey, ez]: [f32; 6] = floats(b);
                p.sprite_bounds = Some(([cx, cy, cz], [ex, ey, ez]));
            }
            // PhysicalBehaviour.colliders, in component order (the recorder dumps the field).
            p.colliders = list(r.req("colliders")?).into_iter().map(|c| c.rsplit_once('#').unwrap().1.parse().unwrap()).collect();
            p.all_colliders = p.colliders.clone();
            if let Some(rs) = &restored {
                let collider = |k: &String| k.split(':').next()?.rsplit_once('#').filter(|(o, _)| *o == r.name).map(|(_, id)| id.parse::<i32>().unwrap());
                p.all_colliders = rs.fixtures.keys().filter_map(collider).fold(Vec::new(), |mut v, c| {
                    if !v.contains(&c) {
                        v.push(c);
                    }
                    v
                });
                // Instance ids fall in creation (= component) order.
                p.all_colliders.sort_by(|a, b| b.cmp(a));
            }
            // A limb, or a PPG_STAB item (ItemRigidbody2D/ItemB2Body records: a dynamic body of its own).
            let owner: Option<(&Record, Option<&Record>, crate::b2::world::BodyId)> = match limb_by_name(&r.name) {
                Some(l) => Some((rbs[l], s.of("b2Body").nth(l), limb_bodies[l])),
                None => match s.of("ItemRigidbody2D").find(|x| x.name.replace(' ', "_") == r.name) {
                    Some(rb) => {
                        let body = restored.as_ref().and_then(|rs| rs.bodies.get(&format!("{}#{}", r.name, rb.id))).ok_or("an item without the b2 world")?;
                        Some((rb, s.of("ItemB2Body").find(|x| x.id == rb.id), *body))
                    }
                    None => None,
                },
            };
            match owner {
                Some((rb, raw, body)) => {
                    p.rigidbody = Rigidbody {
                        body,
                        bodyType: rb.req("bodyType")?.parse().unwrap(),
                        collisionDetectionMode: rb.req("cdm")?.parse().unwrap(),
                        interpolation: rb.req("interpolation")?.parse().unwrap(),
                        movement: raw.and_then(|r| r.get("move")).map_or(MovementState::default(), |w| movement_state(&words(w))),
                        detached: None,
                        component: 0,
                        simulated: true,
                        detached_props: [0.0; 3], constraints: 0,
                    };
                    // Older recordings lack the scene's flag: a non-static body out of the list was just (re)added.
                    let changed = raw.and_then(|r| r.get("hierarchyChanged"));
                    g.rigidbody_hierarchy_changed |= changed.map_or(!p.rigidbody.movement.listed && p.rigidbody.bodyType != 2, |v| v == "1");
                    p.parent = rb.get("parent").and_then(|t| node_of(&g, t));
                    p.transform = Pose {
                        position: floats(rb.req("tpos")?),
                        rotation: floats(rb.req("trot")?),
                        localScale: floats(rb.req("scale")?),
                        local: rb.get("tlpos").zip(rb.get("tlrot")).map(|(p, q)| (floats(p), floats(q))),
                    };
                }
                None => {
                    let body = match restored.as_ref().and_then(|rs| rs.bodies.iter().find(|(n, _)| n.rsplit_once('#').map(|(o, _)| o) == Some(r.name.as_str()))) {
                        Some((_, &b)) => b,
                        None => g.world.create_body(&BodyDef::default()),
                    };
                    p.rigidbody = Rigidbody { body, bodyType: 2, collisionDetectionMode: 0, interpolation: 0, movement: MovementState::default(), detached: None, component: 0, simulated: true, detached_props: [0.0; 3], constraints: 0 };
                    // From dump16 on the record carries the object's Transform.
                    if let Some(pos) = r.get("tpos") {
                        p.parent = r.get("parent").and_then(|t| node_of(&g, t));
                        p.transform = Pose {
                            position: floats(pos),
                            rotation: floats(r.req("trot")?),
                            localScale: floats(r.req("scale")?),
                            local: Some((floats(r.req("tlpos")?), floats(r.req("tlrot")?))),
                        };
                    }
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
                g.physicals[i].onCollisionStayBuffer[slot] = Some(ColliderBoolPair { Active: active, Coll: g.physicals[other].colliders[0] });
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
                connected: Some(limb_bodies[connected]),
                anchor: [0.0; 2],
                connectedAnchor: [0.0; 2],
                enabled: r.req("enabled")? == "1",
                destroyed: false,
                destroy_at: None,
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
                let rev = g.world.joints[id].as_mut().unwrap().revolute_mut();
                rev.impulse = [fl(4), fl(5), fl(6)];
                rev.motor_impulse = fl(7);
                h.joint = Some(id);
            }
            let _ = rig;
            // ponytail: the recorder does not dump m_JointBreakLimits; finite-limit hinges list in hinge order.
            if h.breakForce != f32::INFINITY || h.breakTorque != f32::INFINITY {
                g.joint_break_limits.push(super::BreakJoint::Hinge(g.hinges.len()));
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
            if let Some(v) = r.get("VitalParts").filter(|v| *v != "null") {
                g.vital_parts.insert(i, list(v).into_iter().map(bounds).collect());
            }
            l.ConnectedLimbs = list(r.req("ConnectedLimbs")?).into_iter().map(|t| limb_ids[&reference(t).unwrap()]).collect();
            l.NearestLimbToBrain = reference(r.req("NearestLimbToBrain")?).map(|id| limb_ids[&id]);
            l.StrengthMultipliers = list(r.req("StrengthMultipliers")?).into_iter().map(f).collect();
            l.SpeciesIdentity = r.req("SpeciesIdentity")?.trim_matches('"').into();
            l.BloodLiquidType = r.req("BloodLiquidType")?.trim_matches('"').into();
            l.BloodDecal = r.req("BloodDecal")?.trim_start_matches('@').split('#').next().unwrap().into();
            l.KillShotParticles = r.get("KillShotParticles").is_some_and(|t| t != "null");
            if circ_ids.len() != 0 && i >= circ_ids.len() {
                return Err("limb without circulation".into());
            }
            g.limbs.push(l);
        }
        let grip_ids = ids(s, "GripBehaviour");
        for r in s.of("GripBehaviour") {
            let mut gr = Grip::default();
            load_fields(gr.fields(), r)?;
            gr.id = r.id;
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
                c.base.LiquidDistribution.insert(id, RefFloat { Previous: get("Previous"), raw: get("raw") });
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
            // From dump16 on: the HashSet's slots, each "VelocityX,VelocityY,PositionX,PositionY,Life" or "-" (free),
            // then "free=" and its free list; before, the particles' type names.
            let text = r.req("particles")?;
            match text.split_once("free=") {
                Some((slots, free)) => {
                    for item in list(slots) {
                        sp.particles.slots.push((item != "-").then(|| {
                            let [VelocityX, VelocityY, PositionX, PositionY, Life] = floats::<5>(item);
                            super::misc::SplatParticle { VelocityX, VelocityY, PositionX, PositionY, Life }
                        }));
                    }
                    sp.particles.free = free.split(',').filter(|i| !i.is_empty()).rev().map(|i| i.parse().unwrap()).collect();
                }
                None if list(text).is_empty() => {}
                None => return Err("splat particles without their slots".into()),
            }
            if !r.req("Decal")?.starts_with("@Sparse_blood#") || !r.req("initialParent")?.starts_with("@BleedingParticle") {
                return Err(format!("splat {}: only bleeding particles' Sparse blood splats are loaded", r.id));
            }
            // The local rotation under the bleeding particle until the bleeding particle attaches it.
            if let Some(q) = r.get("tlrot") {
                sp.rotation = floats(q);
            }
            let mask = r.req("CollisionLayers")?;
            sp.CollisionLayers = mask.trim_start_matches("LayerMask(m_Mask=").trim_end_matches(')').parse().map_err(|_| format!("bad mask {mask}"))?;
            g.splats.push(sp);
        }
        for r in s.of("BleedingParticleBehaviour") {
            let mut b = BleedingParticle::default();
            load_fields(b.fields(), r)?;
            let circ = circ_ids[&reference(r.req("CirculationBehaviour")?).unwrap()];
            b.CirculationBehaviour = Some(circ);
            b.Parent = g.limbs[circ].PhysicalBehaviour;
            b.PushingTo = reference(r.req("PushingTo")?).map(|id| circ_ids[&id]);
            b.BloodSplatController = reference(r.req("BloodSplatController")?).map(|id| splat_ids[&id]);
            if let Some(s) = b.BloodSplatController {
                let limb = &g.limbs[circ];
                g.splats[s].parent_person = Some(limb.Person);
                // From dump16 on: the bleeding particle's Transform under its limb.
                if let Some(t) = r.get("tlpos") {
                    let parent = r.req("tparent")?;
                    if !parent.starts_with(&format!("{}#", g.physicals[limb.PhysicalBehaviour].name)) {
                        return Err(format!("bleeding particle {} under {parent}", r.id));
                    }
                    let [x, y, z]: [f32; 3] = floats(t);
                    let [sx, sy, sz]: [f32; 3] = floats(r.req("tscale")?);
                    let trs = crate::unity::transform::Trs { t: [x, y, z, 0.0], q: floats(r.req("tlrot")?), s: [sx, sy, sz, 0.0] };
                    g.splats[s].attached = Some((limb.PhysicalBehaviour, trs, g.splats[s].rotation));
                }
            }
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
        // Older recordings lack it (only shots read it).
        p.PoolableImpactEffect = r.get("PoolableImpactEffect").filter(|t| *t != "null").map(|_| None);
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
        // The recorded Person's behaviours (all execution order 0): the root's, each limb's components, the
        // snapshot's splats, the GForceMeasureBehaviours LimbBehaviour.Start added, the bleeding particles.
        for p in 0..g.people.len() {
            g.behaviours.add(0, super::Behaviour::Person(p));
        }
        for l in 0..g.limbs.len() {
            g.behaviours.add(0, super::Behaviour::Skin(l));
            g.behaviours.add(0, super::Behaviour::Circulation(l));
            if let Some(gr) = g.limbs[l].GripBehaviour {
                g.behaviours.add(0, super::Behaviour::Grip(gr));
            }
            g.behaviours.add(0, super::Behaviour::GoreString(l));
        }
        for s in 0..g.splats.len() {
            g.behaviours.add(0, super::Behaviour::Splat(s));
        }
        for l in 0..g.gforces.len() {
            g.behaviours.add(0, super::Behaviour::GForce(l));
        }
        for b in 0..g.bleeding.len() {
            g.behaviours.add(0, super::Behaviour::Bleeding(b));
        }
        if let Some(r) = s.of("ToolControllerBehaviour").next() {
            g.tool = Some(load_tool(&g, s, r, restored.as_ref().ok_or("a tool without the b2 world")?)?);
        }
        Ok(g)
    }

    /// GameObject layers from the recorder's header: `collider <name> layer=<n> ...` and
    /// `body <name> ... layer=<n>`.
    pub fn load_layers(&mut self, header: &[String]) {
        for line in header {
            // `stab item=<name> ... comps=<Type>:<enabled>;...`: the item's components, for the Lodged/Dislodged messages.
            if let Some(rest) = line.strip_prefix("stab item=") {
                let name = rest.split(' ').next().unwrap().replace(' ', "_");
                let comps = rest.split(" comps=").nth(1).unwrap_or("");
                for c in comps.split(';').filter(|c| !c.is_empty()) {
                    let (ty, enabled) = c.rsplit_once(':').unwrap();
                    if ty == "SharpOnAllSidesBehaviour" && enabled == "1" {
                        unimplemented!("SharpOnAllSidesBehaviour.FixedUpdate: enabled on {name}");
                    }
                    if ty == "ExcaliburBehaviour" || ty.contains("Syringe") {
                        if let Some(p) = self.physicals.iter().position(|p| p.name == name) {
                            self.physicals[p].lodge_receiver = ty.to_string();
                        }
                    }
                }
                continue;
            }
            let (name, layer) = if let Some(rest) = line.strip_prefix("collider ") {
                let Some((name, tail)) = rest.split_once(" layer=") else { continue };
                (name, tail.split(' ').next().unwrap())
            } else if let Some(rest) = line.strip_prefix("body ") {
                let Some((name, tail)) = rest.split_once(' ') else { continue };
                let Some(layer) = tail.split(' ').find_map(|kv| kv.strip_prefix("layer=")) else { continue };
                (name, layer)
            } else {
                continue;
            };
            // Snapshot records name objects with spaces as underscores.
            if let Some(&id) = self.objects.get(&name.replace(' ', "_")) {
                self.world.layers.insert(id, layer.parse().unwrap());
            }
        }
    }

    /// InputSystem's controls and the cursor from a recording's snapshot header (`mouse=` and `drag=D,H,U`).
    pub fn load_input(&mut self, s: &Snapshot) {
        if let Some((_, m)) = s.snap.iter().find(|(k, _)| k == "mouse") {
            self.input.mouse = floats(m);
        }
        if let Some((_, d)) = s.snap.iter().find(|(k, _)| k == "drag") {
            let b: Vec<bool> = d.split(',').map(|x| x == "1").collect();
            self.input.controls.insert("drag".into(), super::tool::Control { down: b[0], held: b[1], up: b[2] });
        }
    }

    /// The renderer state scripts read: each bleeding particle's ParticleSystemRenderer.isVisible (dump16 on).
    pub fn load_render_state(&mut self, s: &Snapshot) {
        for (b, r) in self.bleeding.iter_mut().zip(s.of("BleedingParticleBehaviour")) {
            if let Some(v) = r.get("visible") {
                b.visible = v == "1";
            }
        }
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
            unscaledDeltaTime: f(s.snap("unscaledDt")),
            smoothDeltaTime: f(s.snap("dt")),
            smoothCount: 1.0,
            // The recordings lack the frame's time during FixedUpdate: its own.
            dynamicTimeAsDouble: s.of("Time").next().and_then(|r| r.get("timeAsDouble")).map_or(f(s.snap("time")) as f64, |w| f64::from_bits(u64::from_str_radix(w, 16).unwrap())),
            realtimeSinceStartup: 0.0,
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
        self.camera_shake_fixed_update();
        self.run_pending_starts();
        self.tool_fixed_update();
        self.ambient_grid_fixed_update();
        for p in 0..self.physicals.len() {
            if self.physicals[p].enabled && !self.physicals[p].isDisintegrated && self.physicals[p].in_world {
                self.physical_fixed_update(p);
            }
        }
        for l in 0..self.limbs.len() {
            if self.limbs[l].enabled {
                self.limb_fixed_update(l);
            }
        }
        self.water_manager_fixed_update();
        self.run_behaviours(super::Phase::Fixed);
    }

    /// One pass of a behaviour manager over the list as it stood at the start (IntegrateLists); a behaviour removed
    /// during the pass is skipped (its SafeIterator).
    pub fn run_behaviours(&mut self, phase: super::Phase) {
        use super::{Behaviour as B, Phase};
        let list = self.behaviours.list(phase).clone();
        for entry in list {
            if !self.behaviours.list(phase).contains(&entry) {
                continue;
            }
            // Unity's own messages skip a deactivated (disintegrated) limb's components; the managed updates only
            // check enabled.
            match (phase, entry.1) {
                (_, B::Circulation(l) | B::Skin(l) | B::GoreString(l) | B::GForce(l)) if !self.limb_active(l) => {}
                (_, B::Grip(g)) if !self.physicals[self.grips[g].PhysicalBehaviour].active => {}
                (_, B::Machinery(m)) if { let p = &self.physicals[self.machinery[m].PhysicalBehaviour]; !p.active || p.destroyed } => {}
                (Phase::Fixed, B::Machinery(m)) => self.machinery_fixed_update(m),
                (Phase::Fixed, B::Lava(l)) => self.lava_fixed_update(l),
                (Phase::Fixed, B::HeatRect(h)) => self.heat_rect_fixed_update(h),
                (Phase::Update, B::Spawner(s)) => self.spawner_update(s),
                (Phase::Update, B::Machinery(m)) => self.machinery_update(m),
                (Phase::Update, B::Destroyable(x)) => self.destroyable_update(x),
                (Phase::Fixed, B::Explosive(e)) => self.explosive_fixed_update(e),
                (Phase::Fixed, B::PointToVelocity(i)) => self.point_to_velocity_fixed_update(i),
                (Phase::Fixed, B::AndroidSpark(i)) => self.android_spark_fixed_update(i),
                (Phase::Update, B::Car(i)) => self.car_update(i),
                (_, B::Tyre(t)) if { let p = &self.physicals[self.tyres[t].Phys]; !p.active || p.destroyed } => {}
                (Phase::Fixed, B::Tyre(t)) => self.tyre_fixed_update(t),
                // Update is the sag of a deflated tyre's material (rendering).
                (Phase::Update, B::Tyre(_)) => {}
                (Phase::Update, B::Flask(f)) => self.flask_update(f),
                (Phase::Fixed, B::LiquidContainer(c)) => self.liquid_container_fixed_update(c),
                // LiquidContainerController.Update: the sloshing volume and the particle positions the shader reads.
                (Phase::Update, B::LiquidContainer(_)) => {}
                (Phase::Fixed, B::Glowtube(g)) => self.glowtube_fixed_update(g),
                (Phase::Update, B::Glowtube(g)) => self.glowtube_update(g),
                (Phase::Update, B::Led(i)) => self.led_update(i),
                (Phase::Fixed, B::Detector(i)) => self.detector_fixed_update(i),
                // DetectorBehaviour.Update: the glow intensity.
                (Phase::Update, B::Detector(_)) => {}
                (Phase::Fixed, B::Generator(i)) => self.generator_fixed_update(i),
                (Phase::Update, B::Generator(i)) => self.generator_update(i),
                // SingleFloodlightBehaviour.Update: the glow property blocks and the highlight colours.
                (Phase::Update, B::Floodlight(_)) => {}
                (Phase::Fixed, B::HeatingElement(i)) => self.heating_element_fixed_update(i),
                (Phase::Fixed, B::RayGun(i)) => self.ray_gun_fixed_update(i),
                (Phase::Fixed, B::Bulb(i)) => self.bulb_fixed_update(i),
                (Phase::Update, B::Bulb(i)) => self.bulb_update(i),
                (Phase::Update, B::Television(t)) => self.television_update(t),
                (Phase::Fixed, B::Thrusterbed(i)) => self.thrusterbed_fixed_update(i),
                (Phase::Fixed, B::PointLiquid(i)) => self.point_liquid_fixed_update(i),
                (Phase::Fixed, B::Piston(i)) => self.piston_fixed_update(i),
                (Phase::Update, B::Flamethrower(i)) => self.flamethrower_update(i),
                (Phase::Fixed, B::Rocket(i)) => self.rocket_fixed_update(i),
                (Phase::Update, B::Rocket(i)) => self.rocket_update(i),
                (Phase::Update, B::BreakJointOnBurn(i)) => self.break_joint_on_burn_update(i),
                (Phase::Fixed, B::SoundOnShake(i)) => self.sound_on_shake_fixed_update(i),
                (Phase::Fixed, B::GorseDrooling(i)) => self.gorse_drooling_fixed_update(i),
                (Phase::Fixed, B::Airfoil(i)) => self.airfoil_fixed_update(i),
                (Phase::Update, B::Stunner(i)) => self.stunner_update(i),
                (Phase::Update, B::Blaster(i)) => self.blaster_update(i),
                (Phase::Update, B::Blasterbolt(i)) => self.blasterbolt_update(i),
                (Phase::Fixed, B::Torch(t)) => self.torch_fixed_update(t),
                // ButtonBehaviour.Update drives the light (button.rs).
                (Phase::Update, B::Button(_)) => {}
                (Phase::Fixed, B::Button(b)) => self.button_fixed_update(b),
                (Phase::Fixed, B::BatteryAttachment(a)) => self.battery_attachment_fixed_update(a),
                (Phase::Update, B::Beamformer(i)) => self.beamformer_update(i),
                (Phase::Update, B::Balloon(i)) => self.balloon_update(i),
                (Phase::Update, B::Metronome(i)) => self.metronome_update(i),
                // Thermometer's FixedUpdate writes readouts only (thermometer.rs).
                (Phase::Fixed, B::Thermometer(_)) => {}
                (Phase::Update, B::InfraredThermometer(i)) => self.infrared_update(i),
                (Phase::Fixed, B::Syringe(i)) => self.syringe_fixed_update(i),
                (Phase::Fixed, B::JetEngine(i)) => self.jet_engine_fixed_update(i),
                (Phase::Fixed, B::Chainsaw(i)) => self.chainsaw_fixed_update(i),
                (Phase::Update, B::Chainsaw(i)) => self.chainsaw_update(i),
                (Phase::Update, B::Syringe(i)) => self.syringe_update(i),
                (Phase::Update, B::Piston(i)) => self.piston_update(i),
                // TemperatureRayGunBehaviour.Update: the line renderer's start.
                (Phase::Update, B::RayGun(_)) => {}
                (Phase::Fixed, B::Sticky(i)) => self.sticky_grenade_fixed_update(i),
                (Phase::Fixed, B::TruckDoor(t)) => self.truck_door_fixed_update(t, self.truck_doors[t].0),
                (Phase::Update, B::DeleteAfter(i)) => self.delete_after_update(i),
                (_, B::FireMap(f)) if { let p = &self.physicals[self.fire_maps[f].physical]; !p.active || p.destroyed } => {}
                (Phase::Fixed, B::FireMap(f)) => self.fire_map_fixed_update(f),
                (Phase::Update, B::Firearm(f)) => self.firearm_update(f),
                (Phase::Fixed, B::Launcher(l)) => self.launcher_fixed_update(l),
                (Phase::Update, B::Launcher(l)) => self.launcher_update(l),
                (Phase::Update, B::MachineGun(m)) => self.machine_gun_update(m),
                (Phase::Update, B::Minigun(m)) => self.minigun_update(m),
                // FixedUpdate only decays the barrel's spin (presentation).
                (Phase::Fixed, B::Minigun(_)) => {}
                (Phase::Update, B::Accelerator(a)) => self.accelerator_update(a),
                (Phase::Update, B::AcceleratorBolt(b)) => self.accelerator_bolt_update(b),
                (Phase::Fixed, B::AcceleratorBolt(b)) => self.accelerator_bolt_fixed_update(b),
                (Phase::Update, B::ArchelixBolt(b)) => self.archelix_bolt_update(b),
                (Phase::Fixed, B::ArchelixBolt(b)) => self.archelix_bolt_fixed_update(b),
                (Phase::Update, B::UniversalBell(b)) => self.universal_bell_update(b),
                // ValveBehaviour.Update: the container update (valve.rs).
                (Phase::Update, B::Valve(v)) => self.valve_update(v),
                (Phase::Update, B::HeartMonitor(h)) => self.heart_monitor_update(h),
                (Phase::Update, B::Excalibur(e)) => self.excalibur_update(e),
                (Phase::Fixed, B::SharpOnAllSides(s)) => self.sharp_on_all_sides_fixed_update(s),
                (Phase::Update, B::BloodTank(t)) => self.blood_tank_update(t),
                // LagboxBehaviour.Update only turns the knob (presentation).
                (Phase::Update, B::TimedGate(_)) => {}
                (Phase::Fixed, B::UniversalBell(b)) => self.universal_bell_fixed_update(b),
                (Phase::Fixed, B::ToggleableMirror(m)) => self.toggleable_mirror_fixed_update(m),
                // FireSoundEmitter.FixedUpdate only sets the fire loop's volume at its position (sound only, as
                // PhysicalBehaviour's FuzzySetVolumeAt call is); the translated runtime cannot read a bare Transform's position.
                (Phase::Fixed, B::Translated(super::translated::TBehaviour::FireSoundEmitter(_))) => {}
                (phase, B::Translated(t)) => self.tr_phase(phase, t),
                (Phase::Fixed, B::Person(p)) => self.person_fixed_update(p),
                (Phase::Fixed, B::Circulation(l)) => self.circulation_fixed_update(l),
                (Phase::Fixed, B::Grip(g)) => self.grip_fixed_update(g),
                (Phase::Fixed, B::Splat(s)) => self.splat_fixed_update(s),
                (Phase::Fixed, B::GForce(l)) => self.gforce_fixed_update(l),
                (Phase::Update, B::Person(p)) => self.person_update(p),
                (Phase::Update, B::Skin(l)) => self.skin_update(l),
                (Phase::Update, B::Circulation(l)) => self.blood_container_update(super::container::Owner::Circulation(l)),
                (Phase::Update, B::Grip(g)) => self.grip_update(g),
                (Phase::Update, B::GoreString(l)) => self.gore_string_update(l),
                (Phase::Update, B::Bleeding(b)) => self.bleeding_update(b),
                (Phase::Late, B::Person(p)) => {
                    // A behaviour's Start runs right before its first LateUpdate.
                    if let Some(i) = self.pending_starts.iter().position(|s| *s == super::misc::PendingStart::Person(p)) {
                        self.pending_starts.remove(i);
                        self.person_start(p);
                    }
                    self.person_late_update(p);
                }
                (Phase::Late, B::Skin(l)) => self.skin_late_update(l),
                (Phase::Late, B::Laser(l)) => self.laser_late_update(l),
                (phase, b) => unreachable!("{b:?} in the {phase:?} manager"),
            }
        }
    }

    /// The Update and LateUpdate scripts between SteamworksInitialiser.Update (US) and end of frame (UE),
    /// in the recorded call order.
    pub fn update_scripts(&mut self) {
        self.run_pending_starts();
        self.camera_shake_update();
        // UndoControllerBehaviour.Update, then the map's ambience emitter.
        self.undo_controller_update();
        for a in 0..self.ambient_audio.len() {
            self.play_audio_at_random_update(a);
        }
        self.tool_update();
        // level1's order-0 behaviours update in load order (UndoControllerBehaviour, WORLD's PlayAudioAtRandom,
        // ToolControllerBehaviour, Managers' GearManagerBehaviour, then ResizeHandles), before the map's and the
        // spawned ones.
        self.gear_manager_update();
        self.resize_handles_update();
        // DragTool.Update (added to the ToolController in ToolControllerBehaviour.Start, after the scene's behaviours).
        self.drag_tool_update();
        for p in 0..self.physicals.len() {
            if self.physicals[p].enabled && !self.physicals[p].isDisintegrated && self.physicals[p].in_world {
                self.physical_update(p);
            }
        }
        for l in 0..self.limbs.len() {
            if self.limbs[l].enabled {
                self.limb_update(l);
            }
        }
        self.run_behaviours(super::Phase::Update);
        self.node_controller_update();
        // Global.Update: UpdateMousePosition (timeScale stays at its target).
        self.MousePosition = self.input.mouse;
        // The BloodPuff pool (made at runtime: after the scene's behaviours).
        self.blood_puff_pool_update();
        self.splash_pool_update();
        self.tracer_pool_update();
        self.pool_update();
        // Execution order 1050, above every spawnable script's.
        self.catalog_update();
        // CameraControlBehaviour.Update is the last Update; then the coroutines.
        self.map_light_coroutines();
        self.explosive_coroutines();
        self.gore_string_delayed_calls(true);
        // LateUpdate: ContinuousActivationBehaviour (execution order -10) first; LimbBehaviour.ManagedLateUpdate is empty.
        self.continuous_activation_late_update();
        // Global's ManagedLateUpdate walks PhysicalObjectsInWorld, which a spawned PhysicalBehaviour joins in Start.
        for p in 0..self.physicals.len() {
            if self.physicals[p].enabled && !self.physicals[p].isDisintegrated && self.physicals[p].in_world {
                self.physical_late_update(p);
            }
        }
        self.run_behaviours(super::Phase::Late);
        // Global.LateUpdate
        if self.time.frameCount % 2 == 0 {
            self.chunksUpToDate = false;
        }
        self.blood_puff_particles_update();
        self.splash_particles_update();
        // The renderers' update: the bolts' trails.
        self.archelix_trails_update();
        // The Starts still due after the LateUpdate phase (a Person spawned this frame), before the render.
        self.run_pending_starts();
    }

    /// The render after LateUpdate: SkinMaterialHandler.OnWillRenderObject of each limb the camera drew, in call
    /// order (the UE snapshot's `rendered`; before dump17 every limb was on screen). bloodColor is render state.
    pub fn render_callbacks(&mut self, s: &Snapshot) -> Result<(), String> {
        if !self.foliages.is_empty() {
            unimplemented!("FoliageBehaviour.OnWillRenderObject in a recording (its leaves aren't in `rendered`)");
        }
        let drawn: Vec<usize> = match s.snap.iter().find(|(k, _)| k == "rendered") {
            Some((_, names)) => names.split(';').filter(|n| !n.is_empty()).map(|n| {
                self.limbs.iter().position(|l| self.physicals[l.PhysicalBehaviour].name == n).ok_or(format!("rendered: no limb {n}"))
            }).collect::<Result<_, _>>()?,
            None => (0..self.limbs.len()).filter(|&l| self.limb_active(l)).collect(),
        };
        for l in drawn {
            self.skin_will_render(l);
        }
        Ok(())
    }

    /// SkinMaterialHandler.OnWillRenderObject
    pub fn skin_will_render(&mut self, l: usize) {
        let blood = super::circulation::liquid_color(super::circulation::liquid(&self.limbs[l].BloodLiquidType));
        self.get_computed_color(super::container::Owner::Circulation(l), blood);
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
    let mut particles = Vec::new();
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
    // The recorder's order: the person's hierarchy (each limb's bleeding particles, their extraDecals splats),
    // then the other splats in creation order.
    let mut bleeding: Vec<usize> = (0..g.bleeding.len()).filter(|&b| !g.bleeding[b].destroyed).collect();
    bleeding.sort_by_key(|&b| g.bleeding[b].CirculationBehaviour);
    let mut splats: Vec<usize> = bleeding.iter().filter_map(|&b| g.bleeding[b].BloodSplatController).collect();
    splats.extend((0..g.splats.len()).filter(|i| !splats.contains(i)).collect::<Vec<_>>());
    splats.retain(|&i| !g.splats[i].destroyed);
    let inactive: Vec<usize> = g.blood_puffs.iter().filter(|b| !(b.active && b.emitter_active)).map(|b| b.splat).collect();
    for (&i, r) in splats.iter().zip(s.of("FancyBloodSplatController")) {
        if let Some(a) = r.get("active") {
            if (a == "1") == inactive.contains(&i) {
                particles.push(format!("Splat {}.active: game {a}", r.id));
            }
        }
        let x = &mut g.splats[i];
        check(format!("Splat {}", r.id), x.fields(), r);
        let theirs = r.get("particles").unwrap_or("");
        if theirs.contains("free=") {
            let slots = x.particles.slots.iter().map(|p| match p {
                Some(p) => [p.VelocityX, p.VelocityY, p.PositionX, p.PositionY, p.Life].map(|f| format!("{:08x}", f.to_bits())).join(","),
                None => "-".into(),
            });
            let free = x.particles.free.iter().rev().map(|i| i.to_string());
            let ours = format!("[{}]free={}", slots.collect::<Vec<_>>().join(";"), free.collect::<Vec<_>>().join(","));
            if theirs != ours {
                particles.push(format!("Splat {}.particles: game {theirs} ours {ours}", r.id));
            }
        }
    }
    for (&b, r) in bleeding.iter().zip(s.of("BleedingParticleBehaviour")) {
        check(format!("Bleeding {}", r.id), g.bleeding[b].fields(), r);
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
    // Rigidbody2D, b2Body and HingeJoint2D records leave out inactive objects (a crushed limb); the scripts stay.
    let limb_named = |name: &str| g.limbs.iter().position(|l| g.physicals[l.PhysicalBehaviour].name == name);
    let live_hinges = g.limbs.iter().filter(|l| g.physicals[l.PhysicalBehaviour].active).filter_map(|l| l.Joint).filter(|&h| !g.hinges[h].destroyed).count();
    if live_hinges != s.of("HingeJoint2D").count() {
        out.push(format!("HingeJoint2D count: game {} ours {live_hinges}", s.of("HingeJoint2D").count()));
    }
    for r in s.of("HingeJoint2D") {
        let Some(h) = limb_named(&r.name).and_then(|l| g.limbs[l].Joint) else {
            out.push(format!("Hinge {}: ours missing", r.name));
            continue;
        };
        let h = &g.hinges[h];
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
    for r in s.of("b2Body").chain(s.of("ItemB2Body")) {
        let Some(p) = g.limbs.iter().map(|l| l.PhysicalBehaviour).chain(g.physicals.iter().position(|x| x.name == r.name && s.of("ItemB2Body").any(|i| i.id == r.id))).find(|&p| g.physicals[p].name == r.name && !g.physicals[p].destroyed) else {
            out.push(format!("b2Body {}: ours destroyed", r.name));
            continue;
        };
        let b = g.world.body(g.physicals[p].rigidbody.body);
        let w = words(r.req("body").unwrap());
        let ours = [b.flags as u32, b.linear_velocity.x.to_bits(), b.linear_velocity.y.to_bits(), b.angular_velocity.to_bits(), b.force.x.to_bits(), b.force.y.to_bits(), b.torque.to_bits()];
        let theirs = [w[1] & 0xffff, w[16], w[17], w[18], w[19], w[20], w[21]];
        if ours != theirs {
            out.push(format!("b2Body {}: game {theirs:08x?} ours {ours:08x?}", r.name));
        }
        if let Some(m) = r.get("move") {
            let w = words(m);
            let theirs = [&[(w[0] != u32::MAX) as u32], &w[4..25], &[w[31] & 0xff]].concat();
            let ours = movement_words(&g.physicals[p].rigidbody.movement);
            if ours != theirs {
                out.push(format!("RigidbodyMovementState2D {}: game {theirs:08x?} ours {ours:08x?}", r.name));
            }
        }
        let sleep = words(r.req("mass").unwrap())[9];
        if sleep != b.sleep_time.to_bits() {
            out.push(format!("b2Body {}.sleepTime: game {sleep:08x} ours {:08x}", r.name, b.sleep_time.to_bits()));
        }
    }
    out.extend(compare_stabs(g, s));
    let w = words(s.snap("random"));
    if g.random.state != [w[0], w[1], w[2], w[3]] {
        out.push(format!("random: game {w:08x?} ours {:08x?}", g.random.state));
    }
    // SpriteRenderer.bounds from the sprite's local AABB and our transforms.
    for r in s.of("PhysicalBehaviour") {
        let (Some(theirs), Some(p)) = (r.get("rbounds"), g.physicals.iter().position(|x| x.id == r.id)) else { continue };
        if g.physicals[p].sprite_bounds.is_none() {
            continue;
        }
        let (c, e) = g.sprite_world_bounds(p);
        let ours = [c, e].concat().iter().map(|v| format!("{:08x}", v.to_bits())).collect::<Vec<_>>().join(",");
        if theirs != ours {
            particles.push(format!("{}.bounds: game {theirs} ours {ours}", r.name));
        }
    }
    out.extend(particles);
    out
}

/// The stab bookkeeping against the PhysicalBehaviour records (stabWoundCount, beingStabbedBy, penetrations and
/// victimPenetrations as the recorder formats them) and the SliderJoint2D records (Joint2D values, translation, speed).
fn compare_stabs(g: &Game, s: &Snapshot) -> Vec<String> {
    let mut out = Vec::new();
    let hex = |v: f32| format!("{:08x}", v.to_bits());
    let at = |p: usize| format!("@{}#{}", g.physicals[p].name, g.physicals[p].id);
    let collider = |c: i32| format!("@{}#{c}", g.physical_of_collider(c).map_or("?".to_string(), |p| g.physicals[p].name.clone()));
    let pen_text = |i: usize| {
        let p = &g.penetration_arena[i];
        let a = &p.Axis;
        let joint = match p.Joint {
            None => "null",
            Some(j) if g.fixed_joints[j].destroyed => "destroyed",
            Some(_) => "@",
        };
        format!(
            "Penetration(Stabber={} Victim={} Joint={joint} Duration={} Axis=SharpAxis(Axis={},{} LowerLimit={} UpperLimit={} LooseUpperLimit={} LooseLowerLimit={}) RelevantColliders=[{}] CurrentConnectedAnchor={},{} Active={})",
            at(p.Stabber),
            p.Victim.map_or("null".to_string(), at),
            hex(p.Duration),
            hex(a.Axis[0]),
            hex(a.Axis[1]),
            hex(a.LowerLimit),
            hex(a.UpperLimit),
            a.LooseUpperLimit as u8,
            a.LooseLowerLimit as u8,
            p.RelevantColliders.iter().map(|&c| collider(c)).collect::<Vec<_>>().join(";"),
            hex(p.CurrentConnectedAnchor[0]),
            hex(p.CurrentConnectedAnchor[1]),
            p.Active as u8
        )
    };
    // The game's Joint is "@name#id" of a live component; only whether it is set is compared.
    let loose = |t: &str| -> String {
        let mut o = String::new();
        let mut rest = t;
        while let Some(i) = rest.find("Joint=@") {
            o.push_str(&rest[..i + 7]);
            let end = rest[i..].find(' ').map_or(rest.len(), |e| i + e);
            rest = &rest[end..];
        }
        o + rest
    };
    for r in s.of("PhysicalBehaviour") {
        let Some(p) = g.physicals.iter().position(|x| x.id == r.id) else { continue };
        let ph = &g.physicals[p];
        let join = |v: Vec<String>| format!("[{}]", v.join(";"));
        let mut check = |k: &str, ours: String| {
            if let Some(theirs) = r.get(k) {
                if loose(theirs) != ours {
                    out.push(format!("Physical {}.{k}: game {} ours {ours}", r.name, theirs));
                }
            }
        };
        check("stabWoundCount", ph.stabWoundCount.to_string());
        check("beingStabbedBy", join(ph.beingStabbedBy.iter().map(|&q| at(q)).collect()));
        check("penetrations", join(ph.penetrations.iter().map(|&i| pen_text(i)).collect()));
        check("victimPenetrations", join(ph.victimPenetrations.iter().map(|&i| pen_text(i)).collect()));
    }
    for r in s.of("SliderJoint2D") {
        let name = |t: &str| t.trim_start_matches('@').rsplit_once('#').map_or(t.to_string(), |x| x.0.to_string());
        let connected = name(r.get("connected").unwrap_or("null"));
        let Some(f) = (0..g.fixed_joints.len()).find(|&f| {
            let j = &g.fixed_joints[f];
            !j.destroyed && j.slider.is_some() && matches!(j.attached, BodyOwner::Physical(a) if g.physicals[a].name == r.name) && matches!(j.connected, Some(BodyOwner::Physical(c)) if g.physicals[c].name == connected)
        }) else {
            out.push(format!("SliderJoint2D {} -> {connected}: ours missing", r.name));
            continue;
        };
        let j = &g.fixed_joints[f];
        let (st, sl) = (&j.state, j.slider.as_ref().unwrap());
        let ours = [
            ("enabled", (j.enabled as u8).to_string()),
            ("anchor", format!("{},{}", hex(st.anchor.x), hex(st.anchor.y))),
            ("connectedAnchor", format!("{},{}", hex(st.connected_anchor.x), hex(st.connected_anchor.y))),
            ("auto", format!("{},{}", st.auto_configure_connected_anchor as u8, sl.auto_configure_angle as u8)),
            ("angle", hex(sl.angle)),
            ("motor", format!("{},{},{}", sl.use_motor as u8, hex(sl.motor_speed), hex(sl.max_motor_torque))),
            ("limits", format!("{},{},{}", sl.use_limits as u8, hex(sl.lower), hex(sl.upper))),
            ("breakForce", hex(st.break_force)),
            ("breakTorque", hex(st.break_torque)),
            ("collide", (st.enable_collision as u8).to_string()),
            ("translation", hex(g.slider_joint_translation(f))),
            ("speed", hex(g.slider_joint_speed(f))),
        ];
        for (k, ours) in ours {
            let theirs = r.get(k).unwrap_or("");
            if theirs != ours {
                out.push(format!("SliderJoint2D {} -> {connected}.{k}: game {theirs} ours {ours}", r.name));
            }
        }
    }
    let live = g.fixed_joints.iter().filter(|j| !j.destroyed && j.slider.is_some()).count();
    if live != s.of("SliderJoint2D").count() {
        out.push(format!("SliderJoint2D count: game {} ours {live}", s.of("SliderJoint2D").count()));
    }
    out
}

/// RigidbodyMovementState2D from the recorder's `move` words (Rigidbody2D+0x98 on).
/// The ToolController's script state and drag joint against a snapshot's records.
fn compare_tool(g: &Game, s: &Snapshot) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let (Some(t), Some(r)) = (g.tool.as_ref(), s.of("ToolControllerBehaviour").next()) else { return Ok(out) };
    let d = s.of("DragTool").next().ok_or("no DragTool")?;
    let sel = s.of("Selection").next().ok_or("no Selection")?;
    let name = |p: Option<usize>| p.map_or("null".to_string(), |p| g.physicals[p].name.clone());
    let reffed = |v: &str| if v == "null" { "null".to_string() } else { v.trim_start_matches('@').rsplit_once('#').unwrap().0.to_string() };
    let mut check = |what: &str, game: String, ours: String| {
        if game != ours {
            out.push(format!("{what}: game {game} ours {ours}"));
        }
    };
    let hex = |v: &[f32]| v.iter().map(|x| format!("{:08x}", x.to_bits())).collect::<Vec<_>>().join(",");
    if let Some((_, m)) = s.snap.iter().find(|(k, _)| k == "mouse") {
        check("Global.MousePosition", m.clone(), hex(&g.MousePosition));
    }
    check("isCurrentlyHolding", r.req("isCurrentlyHolding")?.into(), (t.isCurrentlyHolding as u8).to_string());
    check("currentlyHovering", reffed(r.req("currentlyHovering")?), name(t.currentlyHovering));
    check("hover", reffed(sel.req("hovering")?), name(t.hover));
    let selected: Vec<String> = list(sel.req("selected")?).into_iter().map(reffed).collect();
    check("rawSelection", selected.join(";"), t.rawSelection.iter().map(|&p| name(Some(p))).collect::<Vec<_>>().join(";"));
    check("ActiveSingleSelected", reffed(d.req("ActiveSingleSelected")?), name(t.ActiveSingleSelected));
    check("selectedRigidbody", reffed(d.req("selectedRigidbody")?), name(t.selectedRigidbody));
    check("offset", d.req("offset")?.into(), hex(&t.offset));
    check("prevMousePos", d.req("prevMousePos")?.into(), hex(&t.prevMousePos));
    check("snappedMouseDelta", d.req("snappedMouseDelta")?.into(), hex(&t.snappedMouseDelta));
    check("currentRotation", d.req("currentRotation")?.into(), hex(&[t.currentRotation]));
    check("controlRotation", d.req("controlRotation")?.into(), (t.controlRotation as u8).to_string());
    check("initialSnappingFinished", d.req("initialSnappingFinished")?.into(), (t.initialSnappingFinished as u8).to_string());
    if let Some(rb) = s.of("ToolBody").next() {
        check("ToolController position", rb.req("tpos")?.into(), hex(&t.position));
    }
    let game_joint = reference(d.req("dragJoint")?);
    check("dragJoint", game_joint.map_or("null", |_| "joint").into(), t.dragJoint.as_ref().map_or("null", |_| "joint").into());
    if let (Some(id), Some(j)) = (game_joint, t.dragJoint.as_ref()) {
        let h = s.of("ToolJoint").find(|h| h.id == id).ok_or(format!("no ToolJoint #{id}"))?;
        let m = h.req("motor")?;
        check("drag motor", m.into(), format!("{},{}", j.useMotor as u8, hex(&[j.motorSpeed, j.maxMotorTorque])));
        let w = words(h.req("joint")?);
        match j.joint {
            None => check("drag b2 joint", "live".into(), "none".into()),
            Some(jid) => {
                let def = g.world.revolute(jid).def;
                let ours = [def.local_anchor_a.x, def.local_anchor_a.y, def.local_anchor_b.x, def.local_anchor_b.y];
                check("drag joint anchors", hex(&w[0..4].iter().map(|&x| f32::from_bits(x)).collect::<Vec<_>>()), hex(&ours));
                check("drag joint motor", format!("{},{:08x},{:08x}", w[8] & 0xff, w[9], w[10]), format!("{},{}", def.enable_motor as u8, hex(&[def.max_motor_torque, def.motor_speed])));
                check("drag joint angles", format!("{:08x},{:08x},{:08x}", w[12], w[13], w[14]), hex(&[def.reference_angle, def.lower_angle, def.upper_angle]));
            }
        }
    }
    Ok(out)
}

/// The ToolController's components from a recording.
fn load_tool(g: &Game, s: &Snapshot, r: &Record, restored: &physics::Restored) -> Result<super::tool::Tool, String> {
    let rb = s.of("ToolBody").next().ok_or("no ToolBody")?;
    let d = s.of("DragTool").next().ok_or("no DragTool")?;
    let sel = s.of("Selection").next().ok_or("no Selection")?;
    let physical = |t: &str| -> Result<Option<usize>, String> {
        match reference(t) {
            None => Ok(None),
            Some(id) => g.physicals.iter().position(|p| p.id == id).map(Some).ok_or(format!("no physical #{id}")),
        }
    };
    // A Rigidbody2D reference: the physical on its object.
    let rigidbody = |t: &str| -> Result<Option<usize>, String> {
        if t == "null" {
            return Ok(None);
        }
        let name = t.trim_start_matches('@').rsplit_once('#').unwrap().0;
        g.physicals.iter().position(|p| p.name == name).map(Some).ok_or(format!("no physical {name}"))
    };
    let b = |k: &str| -> Result<bool, String> { Ok(d.req(k)? == "1") };
    let drag_joint = match reference(d.req("dragJoint")?) {
        None => None,
        Some(id) => {
            let h = s.of("ToolJoint").find(|h| h.id == id).ok_or(format!("no ToolJoint #{id}"))?;
            let m = h.req("motor")?;
            let (on, rest) = m.split_once(',').unwrap();
            let [speed, torque]: [f32; 2] = floats(rest);
            let joint = restored.joints.get(&format!("{}#{}", h.name, h.id)).copied();
            Some(super::tool::DragJoint { joint, motorSpeed: speed, maxMotorTorque: torque, useMotor: on == "1", destroy_at: None })
        }
    };
    let mask = r.req("SelectionLayer")?;
    let mask = mask.trim_start_matches("LayerMask(m_Mask=").trim_end_matches(')').parse().map_err(|e| format!("SelectionLayer: {e}"))?;
    Ok(super::tool::Tool {
        body: *restored.bodies.get(&format!("ToolController#{}", rb.id)).ok_or("no ToolController body")?,
        position: floats(rb.req("tpos")?),
        rotation: floats(rb.req("trot")?),
        SelectionLayer: mask,
        isCurrentlyHolding: r.req("isCurrentlyHolding")? == "1",
        currentlyHovering: physical(r.req("currentlyHovering")?)?,
        isSelecting: r.req("isSelecting")? == "1",
        selectionDragStart: floats(r.req("selectionDragStart")?),
        rawSelection: list(sel.req("selected")?).into_iter().map(|t| physical(t).map(Option::unwrap)).collect::<Result<_, _>>()?,
        hover: physical(sel.req("hovering")?)?,
        offset: floats(d.req("offset")?),
        dragJoint: drag_joint,
        destroying: Vec::new(),
        controlRotation: b("controlRotation")?,
        selectedRigidbody: rigidbody(d.req("selectedRigidbody")?)?,
        rotationSpeed: f(d.req("rotationSpeed")?),
        currentRotation: f(d.req("currentRotation")?),
        snappedMouseDelta: floats(d.req("snappedMouseDelta")?),
        prevMousePos: floats(d.req("prevMousePos")?),
        initialSnappingFinished: b("initialSnappingFinished")?,
        isActive: b("isActive")?,
        shouldSendContinuousUseMessage: b("shouldSendContinuousUseMessage")?,
        initialTranslationSnapOffset: floats(d.req("initialTranslationSnapOffset")?),
        ActiveSingleSelected: physical(d.req("ActiveSingleSelected")?)?,
    })
}

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
    for rb in to.of("Rigidbody2D").chain(to.of("ItemRigidbody2D")).filter(|_| chain) {
        let p = g.limbs.iter().map(|l| l.PhysicalBehaviour).chain(g.physicals.iter().position(|x| x.name == rb.name && to.of("ItemRigidbody2D").any(|i| i.id == rb.id))).find(|&p| g.physicals[p].name == rb.name).ok_or(format!("no limb {}", rb.name))?;
        let t = &g.physicals[p].transform;
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
    // A second restore of the snapshot names its bodies and fixtures; ours are found by the same Rigidbody2D
    // (ground: none), their fixtures in list order (ids drift once something was destroyed).
    let mut r = physics::restore(from, V::new(-0.0, -9.81))?;
    let ours: HashMap<Option<i32>, crate::b2::world::BodyId> = g.world.body_list.iter().map(|&b| (g.world.body(b).user.map(|u| u.id), b)).collect();
    let fixture_name: HashMap<_, _> = r.fixtures.iter().map(|(n, &f)| (f, n.clone())).collect();
    let (mut bodies, mut fixtures) = (HashMap::new(), HashMap::new());
    for (name, &b) in &r.bodies {
        let Some(&o) = ours.get(&r.world.body(b).user.map(|u| u.id)) else { continue };
        bodies.insert(name.clone(), o);
        for (f, of) in r.world.body(b).fixtures.iter().zip(&g.world.body(o).fixtures) {
            fixtures.insert(fixture_name[f].clone(), *of);
        }
    }
    (r.bodies, r.fixtures) = (bodies, fixtures);
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
    let mut snaps = snapshots(dump)?.skip_while(|s| !(s.pt == "FS" && s.of("b2World").next().is_some()));
    let first = snaps.next().ok_or("no FS snapshot")?;
    if first.of("Contacts2D").next().is_none() {
        return Err("--loop-test needs a recording with Contacts2D (dump12 on)".into());
    }
    let mut g = Game::from_snapshot(&first, rig)?;
    let header = header(dump)?;
    g.load_layers(&header);
    let item = header.iter().find_map(|l| l.strip_prefix("stab item=")).map(|r| r.split(' ').next().unwrap().replace(' ', "_"));
    g.load_input(&first);
    g.MousePosition = g.input.mouse;
    let mut prev = first.clone();
    let mut count = 1;
    for (n, cur) in snaps.enumerate() {
        count += 1;
        // Only Time comes from the recording: the scripts' Random state carries over.
        let random = g.random.clone();
        g.load_time(&prev);
        g.random = random;
        g.load_input(&cur);
        g.load_render_state(&prev);
        let mut diffs = Vec::new();
        match (prev.pt.as_str(), cur.pt.as_str()) {
            ("FS", "FM") => g.fixed_update_scripts(),
            ("FM", "FE") => {
                // The recorder's scripted LimbBehaviour.Crush (PPG_CRUSH) before the physics step.
                if let Some((_, limb)) = prev.snap.iter().find(|(k, _)| k == "crush") {
                    let l = g.limbs.iter().position(|l| &g.physicals[l.PhysicalBehaviour].name == limb).ok_or(format!("no limb {limb}"))?;
                    g.limb_crush(l);
                }
                // The recorder's scripted pull on the PPG_STAB item (PPG_STAB_PULL): its velocity set before the step.
                if let Some((_, v)) = prev.snap.iter().find(|(k, _)| k == "pull") {
                    let p = g.physicals.iter().position(|p| Some(&p.name) == item.as_ref()).ok_or("pull without a stab item")?;
                    let [vx, vy]: [f32; 2] = floats(v);
                    let rb = g.physicals[p].rigidbody.clone();
                    rb.set_velocity(&mut g.world, [vx, vy]);
                }
                g.simulate_physics();
                diffs = compare_physics(&mut g, &prev, &cur)?;
            }
            ("US", "UE") => {
                g.update_scripts();
                g.render_callbacks(&cur)?;
                g.end_of_frame_calls();
            }
            // PreUpdate's Physics2DUpdate at the frame's time.
            ("FE", "US") => {
                let random = g.random.clone();
                g.load_time(&cur);
                g.random = random;
                g.interpolate_bodies();
                diffs = compare_transforms(&g, &first, &cur)?;
            }
            _ => {}
        }
        diffs.extend(compare(&mut g, &cur));
        diffs.extend(compare_tool(&g, &cur)?);
        if !diffs.is_empty() {
            eprintln!("diverged at {} frame {} (after {n} matching points, {}→{}):", cur.pt, cur.snap("frame"), prev.pt, cur.pt);
            for d in diffs.iter().take(25) {
                eprintln!("  {d}");
            }
            return Err(format!("{} diffs", diffs.len()));
        }
        prev = cur;
    }
    println!("loop points {count}, all match");
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
            g.render_callbacks(&pair[1])?;
            g.end_of_frame_calls();
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
