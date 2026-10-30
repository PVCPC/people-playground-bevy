//! `--physics-test <dump>`: each FixedUpdate's physics step (FM -> FE) replayed in a b2 world restored from
//! the recorder's b2World/b2Edges/b2Fixture/b2Contact/b2RevoluteJoint records, then compared with the next
//! snapshot word for word. Native lists are restored tail first, so every list matches the game's order.
use super::*;
use crate::b2::broadphase::Aabb;
use crate::b2::collide::{ContactFeature, Manifold, ManifoldPoint, ManifoldType};
use crate::b2::contacts2d::{Collision2D, ContactState, Manifold2D, Message, Report};
use crate::b2::shape::{MassData, Polygon};
use crate::b2::world::{Body, BodyId, ColliderRef, ContactId, Filter, FixtureDef, FixtureId, JointId, RigidbodyRef, Shape, World, CONTACT_TOUCHING};

pub struct Restored {
    pub world: World,
    pub bodies: HashMap<String, BodyId>,
    pub fixtures: HashMap<String, FixtureId>,
    /// Revolute joints by "HingeJoint2D name#id".
    pub joints: HashMap<String, JointId>,
    /// GameObjects by name (the recorder names components by their object); ids are this restore's own.
    pub objects: HashMap<String, i32>,
}

/// "Name#id" (or "Name#id:shape") -> (Name, id).
fn object_ref(text: &str) -> Result<(&str, i32), String> {
    let (name, rest) = text.rsplit_once('#').ok_or(format!("bad reference {text}"))?;
    let id = rest.split(':').next().unwrap().parse().map_err(|_| format!("bad reference {text}"))?;
    Ok((name, id))
}

fn object_id(objects: &mut HashMap<String, i32>, name: &str) -> i32 {
    let next = objects.len() as i32 + 1;
    *objects.entry(name.to_string()).or_insert(next)
}

fn names(text: &str) -> Vec<&str> {
    if text.is_empty() { Vec::new() } else { text.split(';').collect() }
}

/// b2Fixture words 0x00-0x18 (cached mass data, density), 0x30-0x38 (friction, restitution) and 0x40-0x4c
/// (proxy count, filter, isSensor); shape words from b2Shape::m_type to b2PolygonShape::m_count.
fn fixture_def(r: &Record) -> Result<(FixtureDef, MassData), String> {
    let s = words(r.req("shape")?);
    if s[0] != 2 {
        return Err(format!("{}: shape type {} is not a polygon", r.name, s[0]));
    }
    let fs = |i: usize| f32::from_bits(s[i]);
    let mut p = Polygon { radius: fs(1), centroid: V::new(fs(2), fs(3)), count: s[36] as usize, ..Default::default() };
    for i in 0..8 {
        p.vertices[i] = V::new(fs(4 + 2 * i), fs(5 + 2 * i));
        p.normals[i] = V::new(fs(20 + 2 * i), fs(21 + 2 * i));
    }
    let w = words(r.req("fixture")?);
    let fw = |i: usize| f32::from_bits(w[i]);
    let def = FixtureDef {
        shape: Shape::Polygon(p),
        density: fw(5),
        friction: fw(6),
        restitution: fw(7),
        is_sensor: (w[10] >> 16) & 0xff != 0,
        filter: Filter { category_bits: w[9] as u16, mask_bits: (w[9] >> 16) as u16, group_index: w[10] as u16 as i16 },
        collider: ColliderRef::default(),
    };
    Ok((def, MassData { mass: fw(0), center: V::new(fw(1), fw(2)), inertia: fw(3), area: fw(4) }))
}

/// b2Manifold (0x48 bytes): two points, local normal and point, type, count; Unity's two contact offsets follow.
fn manifold(w: &[u32]) -> Manifold {
    let fl = |i: usize| f32::from_bits(w[i]);
    let point = |o: usize| {
        let [index_a, index_b, type_a, type_b] = w[o + 4].to_le_bytes();
        ManifoldPoint {
            local_point: V::new(fl(o), fl(o + 1)),
            normal_impulse: fl(o + 2),
            tangent_impulse: fl(o + 3),
            id: ContactFeature { index_a, index_b, type_a, type_b },
        }
    };
    Manifold {
        points: [point(0), point(5)],
        local_normal: V::new(fl(10), fl(11)),
        local_point: V::new(fl(12), fl(13)),
        kind: match w[14] {
            0 => ManifoldType::Circles,
            1 => ManifoldType::FaceA,
            _ => ManifoldType::FaceB,
        },
        point_count: w[15] as usize,
    }
}

fn manifold_words(m: &Manifold) -> Vec<u32> {
    let mut out = Vec::new();
    for p in &m.points {
        out.extend([p.local_point.x.to_bits(), p.local_point.y.to_bits(), p.normal_impulse.to_bits(), p.tangent_impulse.to_bits(), p.id.key()]);
    }
    let kind = match m.kind {
        ManifoldType::Circles => 0,
        ManifoldType::FaceA => 1,
        ManifoldType::FaceB => 2,
    };
    out.extend([m.local_normal.x.to_bits(), m.local_normal.y.to_bits(), m.local_point.x.to_bits(), m.local_point.y.to_bits(), kind, m.point_count as u32]);
    out
}

/// load_body's words back from a body (b2Body 0x00-0x58).
fn body_words(b: &Body) -> Vec<u32> {
    let kind = match b.kind {
        BodyType::Static => 0,
        BodyType::Kinematic => 1,
        BodyType::Dynamic => 2,
    };
    let f = [
        b.xf.p.x, b.xf.p.y, b.xf.q.s, b.xf.q.c, b.sweep.local_center.x, b.sweep.local_center.y, b.sweep.c0.x, b.sweep.c0.y, b.sweep.c.x,
        b.sweep.c.y, b.sweep.a0, b.sweep.a, b.sweep.alpha0, b.linear_velocity.x, b.linear_velocity.y, b.angular_velocity, b.force.x,
        b.force.y, b.torque,
    ];
    [kind, b.flags as u32, b.island_index as u32].into_iter().chain(f.iter().map(|v| v.to_bits())).collect()
}

/// The world as the game holds it at one snapshot.
pub fn restore(s: &Snapshot, gravity: V) -> Result<Restored, String> {
    let mut world = World::new(gravity);
    let mut objects = HashMap::new();
    let edges: HashMap<&str, &Record> = s.of("b2Edges").map(|r| (r.name.as_str(), r)).collect();
    let fixture_records: HashMap<&str, &Record> = s.of("b2Fixture").map(|r| (r.name.as_str(), r)).collect();
    let order = names(s.of("b2World").next().ok_or("no b2World record")?.req("bodies")?);
    let (mut bodies, mut fixtures) = (HashMap::new(), HashMap::new());
    let mut proxies = Vec::new();
    // Bodies in creation order (the native list is head first), so Unity's body arrays keep it too.
    for &name in order.iter().rev() {
        let e = edges.get(name).ok_or(format!("no b2Edges for {name}"))?;
        let kind = match words(e.req("body")?)[0] {
            0 => BodyType::Static,
            1 => BodyType::Kinematic,
            _ => BodyType::Dynamic,
        };
        let user = if name == "ground" {
            None
        } else {
            let (object, id) = object_ref(name)?;
            Some(RigidbodyRef { id, game_object: object_id(&mut objects, object) })
        };
        let id = world.create_body(&BodyDef { kind, user, ..Default::default() });
        load_body(world.body_mut(id), e)?;
        bodies.insert(name.to_string(), id);
        for &f in names(e.req("fixtures")?).iter().rev() {
            let (mut def, mass) = fixture_def(fixture_records.get(f).ok_or(format!("no b2Fixture {f}"))?)?;
            let (object, collider) = object_ref(f)?;
            def.collider = ColliderRef { id: collider, game_object: object_id(&mut objects, object) };
            let fid = world.create_fixture(id, &def, false);
            world.fixtures[fid].as_mut().unwrap().mass_data = mass;
            fixtures.insert(f.to_string(), fid);
            for p in names(fixture_records[f].req("proxies")?) {
                // "id,aabb,fat": the id is decimal, the boxes hex words.
                let (id, rest) = p.split_once(',').ok_or("bad proxy")?;
                let w = words(rest);
                let aabb = |o: usize| Aabb { lower: V::new(f32::from_bits(w[o]), f32::from_bits(w[o + 1])), upper: V::new(f32::from_bits(w[o + 2]), f32::from_bits(w[o + 3])) };
                proxies.push((id.parse::<i32>().map_err(|_| "bad proxy id")?, fid, aabb(0), aabb(4)));
            }
        }
    }
    proxies.sort_by_key(|p| p.0);
    let ours = world.restore_proxies(&proxies.iter().map(|&(_, f, a, fat)| (f, a, fat)).collect::<Vec<_>>());
    let proxy_ids: HashMap<i32, i32> = proxies.iter().map(|p| p.0).zip(ours).collect();
    let state = s.of("b2World").next().unwrap();
    let moves = names(state.req("moves")?).iter().map(|m| proxy_ids.get(&m.parse::<i32>().unwrap()).copied().ok_or(format!("move of unknown proxy {m}"))).collect::<Result<Vec<_>, _>>()?;
    let world_flags = words(state.req("flags")?)[0];
    let inv_dt0 = f(state.req("invDt0")?);
    // PersonBehaviour.SetCollisionIgnores: every limb collider pair of a person.
    let limb_fixtures: Vec<FixtureId> = s
        .of("Rigidbody2D")
        .flat_map(|r| world.body(bodies[&format!("{}#{}", r.name, r.id)]).fixtures.clone())
        .collect();
    for &a in &limb_fixtures {
        for &b in &limb_fixtures {
            let (ca, cb) = (world.fixture(a).collider.id, world.fixture(b).collider.id);
            if ca < cb {
                world.ignored.insert((ca, cb));
            }
        }
    }
    // Hinges: body A is the joint's own Rigidbody2D, body B the connected one.
    let mut joints: HashMap<String, JointId> = HashMap::new();
    for (h, raw) in s.of("HingeJoint2D").zip(s.of("b2RevoluteJoint")) {
        let j = raw.req("joint")?;
        if j == "null" {
            continue;
        }
        let own = s.of("Rigidbody2D").find(|r| r.name == h.name).ok_or(format!("hinge {} without a body", h.name))?;
        let connected = h.req("connected")?.trim_start_matches('@');
        let a = bodies[&format!("{}#{}", own.name, own.id)];
        let b = *bodies.get(connected).ok_or(format!("hinge {} connects to {connected}", h.name))?;
        joints.insert(format!("{}#{}", h.name, h.id), restore_revolute(&mut world, &words(j), a, b));
    }
    // DragTool's HingeJoint2D on the ToolController, made after the scene's hinges.
    if let Some(tb) = s.of("ToolBody").next() {
        let a = bodies[&format!("ToolController#{}", tb.id)];
        for h in s.of("ToolJoint") {
            let j = h.req("joint")?;
            if j == "null" {
                continue;
            }
            let connected = h.req("connected")?.trim_start_matches('@');
            let b = *bodies.get(connected).ok_or(format!("drag joint connects to {connected}"))?;
            joints.insert(format!("{}#{}", h.name, h.id), restore_revolute(&mut world, &words(j), a, b));
        }
    }
    // Contacts, list tail first.
    let records: Vec<&Record> = s.of("b2Contact").collect();
    let mut contacts: Vec<ContactId> = vec![usize::MAX; records.len()];
    for (i, r) in records.iter().enumerate().rev() {
        let fa = *fixtures.get(r.req("a")?).ok_or(format!("contact {i}: no fixture {}", r.req("a")?))?;
        let fb = *fixtures.get(r.req("b")?).ok_or(format!("contact {i}: no fixture {}", r.req("b")?))?;
        let id = world.restore_contact(fa, fb);
        let toi = words(r.req("toi")?);
        let index = words(r.req("index")?);
        let c = world.contact_mut(id);
        if (c.fixture_a, c.fixture_b) != (fa, fb) {
            return Err(format!("contact {i}: fixtures swapped"));
        }
        c.flags = words(r.req("flags")?)[0];
        c.child_a = index[0] as usize;
        c.child_b = index[1] as usize;
        c.manifold = manifold(&words(r.req("manifold")?));
        c.toi_count = toi[0] as i32;
        c.toi = f32::from_bits(toi[1]);
        c.friction = f32::from_bits(toi[2]);
        c.restitution = f32::from_bits(toi[3]);
        c.tangent_speed = f32::from_bits(toi[4]);
        c.manager_index = toi[5] as usize;
        c.user_index = toi.get(6).map_or(-1, |&u| u as i32);
        contacts[i] = id;
    }
    if let Some(r) = s.of("Contacts2D").next() {
        world.contacts2d.simulation_id = r.req("sim")?.parse().map_err(|_| "bad sim")?;
    }
    // Unity's Collision2D records in m_Collisions order; touching contacts point at their pair's record.
    for r in s.of("Collision2DRecord") {
        let col = collision_record(r, &bodies, &fixtures, &contacts, &mut objects)?;
        let rid = world.contacts2d.restore(col);
        world.contacts2d.simulation_id = r.req("sim")?.parse().map_err(|_| "bad sim")?;
        let _ = rid;
    }
    let recorded = s.of("Collision2DRecord").next().is_some();
    for &id in &contacts {
        let c = world.contact(id);
        if c.flags & CONTACT_TOUCHING == 0 {
            continue;
        }
        // Dumps from before the record recorder: rebuild the records as the listener would have begun them.
        if !recorded {
            world.begin_contact(id);
            continue;
        }
        let (a, b) = (world.fixture(c.fixture_a).collider.id, world.fixture(c.fixture_b).collider.id);
        let at = *world.contacts2d.index.get(&(a.min(b), a.max(b))).ok_or(format!("touching contact without a Collision2D record"))?;
        let rid = world.contacts2d.collisions[at];
        world.contact_mut(id).user_data = Some(rid);
    }
    // Edge lists in native order; contact creation already wakes bodies, so their state is loaded again.
    for (name, &id) in &bodies {
        let e = edges[name.as_str()];
        let want_contacts: Vec<ContactId> = names(e.req("contacts")?).iter().rev().map(|i| contacts[i.parse::<usize>().unwrap()]).collect();
        let want_joints: Vec<JointId> = names(e.req("joints")?).iter().rev().map(|j| joints.get(*j).copied().ok_or(format!("unknown joint {j}"))).collect::<Result<_, _>>()?;
        let b = world.body_mut(id);
        let mut cs = std::mem::take(&mut b.contacts);
        let mut js = std::mem::take(&mut b.joints);
        if cs.len() != want_contacts.len() || js.len() != want_joints.len() {
            return Err(format!("{name}: edge counts differ"));
        }
        b.contacts = want_contacts.iter().map(|c| cs.remove(cs.iter().position(|e| e.1 == *c).unwrap())).collect();
        b.joints = want_joints.iter().map(|j| js.remove(js.iter().position(|e| e.1 == *j).unwrap())).collect();
        load_body(b, e)?;
    }
    world.finish_restore(inv_dt0, world_flags & 1 != 0, moves)?;
    Ok(Restored { world, bodies, fixtures, joints, objects })
}

/// A b2RevoluteJoint from its recorded words (0x98..0x130), with the warm-start impulses and limit state.
fn restore_revolute(world: &mut World, w: &[u32], a: BodyId, b: BodyId) -> JointId {
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
    let id = world.create_joint(def, a, b, false);
    let rev = world.revolute_mut(id);
    rev.impulse = [fl(4), fl(5), fl(6)];
    rev.motor_impulse = fl(7);
    rev.limit_state = w[37];
    id
}

fn contact_state(v: u32) -> Result<ContactState, String> {
    Ok(match v {
        0 => ContactState::Invalid,
        1 => ContactState::Enter,
        2 => ContactState::Exit,
        3 => ContactState::EnterAndExit,
        4 => ContactState::Stay,
        _ => return Err(format!("contact state {v}")),
    })
}

/// A Collision2DRecord line: the record's state, pair, flags and manifolds (0x70 bytes each).
fn collision_record(r: &Record, bodies: &HashMap<String, BodyId>, fixtures: &HashMap<String, FixtureId>, contacts: &[ContactId], objects: &mut HashMap<String, i32>) -> Result<Collision2D, String> {
    let collider = |t: &str, objects: &mut HashMap<String, i32>| -> Result<ColliderRef, String> {
        let (object, id) = object_ref(t)?;
        Ok(ColliderRef { id, game_object: object_id(objects, object) })
    };
    let rb = |t: &str| -> Result<Option<BodyId>, String> { if t == "null" { Ok(None) } else { bodies.get(t).copied().map(Some).ok_or(format!("no body {t}")) } };
    let state = words(r.req("state")?);
    let flags = words(r.req("flags")?)[0].to_le_bytes();
    let mut manifolds = Vec::new();
    for m in names(r.req("manifolds")?) {
        let parts: Vec<&str> = m.split(',').collect();
        let w = words(&parts[4..].join(","));
        let fl = |i: usize| f32::from_bits(w[i]);
        let v = |i: usize| V::new(fl(i), fl(i + 1));
        let fixture = |t: &str| fixtures.get(t).copied().ok_or(format!("no fixture {t}"));
        manifolds.push(Manifold2D {
            enabled: u32::from_str_radix(parts[0], 16).map_err(|_| "bad manifold")? & 0xff != 0,
            contact: if parts[1] == "null" { None } else { Some(contacts[parts[1].parse::<usize>().map_err(|_| "bad manifold contact")?]) },
            fixture_a: fixture(parts[2])?,
            fixture_b: fixture(parts[3])?,
            index_a: w[0] as usize,
            index_b: w[1] as usize,
            simulation_id: w[2] as i32,
            point_count: w[3] as usize,
            normal: v(4),
            points: [v(6), v(8)],
            relative_velocity: [v(10), v(12)],
            separations: [fl(14), fl(15)],
            normal_impulses: [fl(16), fl(17)],
            tangent_impulses: [fl(18), fl(19)],
        });
    }
    Ok(Collision2D {
        manifolds,
        state: contact_state(state[0])?,
        contact_count: state[1] as i32,
        collider_a: collider(r.req("a")?, objects)?,
        collider_b: collider(r.req("b")?, objects)?,
        rigidbody_a: rb(r.req("rba")?)?,
        rigidbody_b: rb(r.req("rbb")?)?,
        enabled: flags[0] != 0,
        is_trigger: flags[1] != 0,
        flag_for_recreate: flags[2] != 0,
        swapped: flags[3] != 0,
    })
}

fn manifold2d_words(m: &Manifold2D) -> Vec<u32> {
    let mut w = vec![m.enabled as u32, m.index_a as u32, m.index_b as u32, m.simulation_id as u32, m.point_count as u32, m.normal.x.to_bits(), m.normal.y.to_bits()];
    for v in m.points.iter().chain(&m.relative_velocity) {
        w.extend([v.x.to_bits(), v.y.to_bits()]);
    }
    w.extend(m.separations.iter().chain(&m.normal_impulses).chain(&m.tangent_impulses).map(|f| f.to_bits()));
    w
}

/// Unity's Collision2D records after ProcessContacts against the snapshot's, in array order. Point words past
/// pointCount are stale in the game, so only live points compare.
pub fn compare_contacts2d(r: &Restored, s: &Snapshot) -> Result<Vec<String>, String> {
    let w = &r.world;
    let mut out = Vec::new();
    let fixture_name: HashMap<FixtureId, &str> = r.fixtures.iter().map(|(n, &f)| (f, n.as_str())).collect();
    let contact_index: HashMap<ContactId, usize> = w.contact_list.iter().rev().enumerate().map(|(i, &c)| (c, i)).collect();
    let game: Vec<&Record> = s.of("Collision2DRecord").collect();
    let ours: Vec<&Collision2D> = w.contacts2d.collisions.iter().map(|&id| w.contacts2d.records[id].as_ref().unwrap()).collect();
    if game.len() != ours.len() {
        out.push(format!("Collision2D.count: game {} ours {}", game.len(), ours.len()));
    }
    for (i, (g, o)) in game.iter().zip(&ours).enumerate() {
        let key = format!("{}/{}", g.req("a")?, g.req("b")?);
        let ok_pair = (o.collider_a.id, o.collider_b.id) == (object_ref(g.req("a")?)?.1, object_ref(g.req("b")?)?.1);
        if !ok_pair {
            out.push(format!("Collision2D[{i}] pair: game {key} ours {}/{}", o.collider_a.id, o.collider_b.id));
            continue;
        }
        let state = words(g.req("state")?);
        let flags = words(g.req("flags")?)[0];
        let our_flags = u32::from_le_bytes([o.enabled as u8, o.is_trigger as u8, o.flag_for_recreate as u8, o.swapped as u8]);
        let our_state = [ContactState::Invalid, ContactState::Enter, ContactState::Exit, ContactState::EnterAndExit, ContactState::Stay].iter().position(|s| *s == o.state).unwrap() as u32;
        for (k, gv, ov) in [("state", state[0], our_state), ("count", state[1], o.contact_count as u32), ("flags", flags, our_flags)] {
            if gv != ov {
                out.push(format!("Collision2D[{i}] {key}.{k}: game {gv:08x} ours {ov:08x}"));
            }
        }
        let gm = names(g.req("manifolds")?);
        if gm.len() != o.manifolds.len() {
            out.push(format!("Collision2D[{i}] {key}.manifolds: game {} ours {}", gm.len(), o.manifolds.len()));
            continue;
        }
        for (k, (gm, om)) in gm.iter().zip(&o.manifolds).enumerate() {
            let parts: Vec<&str> = gm.split(',').collect();
            let oc = om.contact.map_or("null".to_string(), |c| contact_index[&c].to_string());
            let names_ours = [oc, fixture_name[&om.fixture_a].to_string(), fixture_name[&om.fixture_b].to_string()];
            for (n, (gv, ov)) in ["contact", "fixtureA", "fixtureB"].iter().zip(parts[1..4].iter().zip(&names_ours)) {
                if gv != ov {
                    out.push(format!("Collision2D[{i}] {key}.m{k}.{n}: game {gv} ours {ov}"));
                }
            }
            let mut gw = vec![u32::from_str_radix(parts[0], 16).unwrap() & 0xff];
            gw.extend(words(&parts[4..].join(",")));
            let ow = manifold2d_words(om);
            let count = gw[4] as usize;
            // Words: enabled, indexA/B, simulationId, pointCount, normal, points, relativeVelocity, separations, normal and tangent impulses.
            let live = |j: usize| match j {
                0..=6 => true,
                7..=10 => (j - 7) / 2 < count,
                11..=14 => (j - 11) / 2 < count,
                15..=16 => j - 15 < count,
                17..=18 => j - 17 < count,
                _ => j - 19 < count,
            };
            for j in (0..gw.len()).filter(|&j| live(j)) {
                if gw[j] != ow[j] {
                    out.push(format!("Collision2D[{i}] {key}.m{k}.w{j}: game {:08x} ours {:08x}", gw[j], ow[j]));
                }
            }
        }
    }
    for c in s.of("b2Contact") {
        let key = format!("{}/{}", c.req("a")?, c.req("b")?);
        let Some(&id) = w.contact_list.iter().find(|&&id| format!("{}/{}", fixture_name[&w.contact(id).fixture_a], fixture_name[&w.contact(id).fixture_b]) == key) else { continue };
        if let Some(&u) = words(c.req("toi")?).get(6) {
            if u as i32 != w.contact(id).user_index {
                out.push(format!("b2Contact {key}.userIndex: game {} ours {}", u as i32, w.contact(id).user_index));
            }
        }
    }
    Ok(out)
}

fn message_name(m: Message) -> &'static str {
    match m {
        Message::CollisionEnter => "OnCollisionEnter2D",
        Message::CollisionStay => "OnCollisionStay2D",
        Message::CollisionExit => "OnCollisionExit2D",
        Message::TriggerEnter => "OnTriggerEnter2D",
        Message::TriggerStay => "OnTriggerStay2D",
        Message::TriggerExit => "OnTriggerExit2D",
    }
}

/// The reports against the game's OnCollision*2D log: consecutive calls on one object with one collision are
/// one delivery (one per receiving collider), compared by message, the receiving pair and every Collision2D field.
fn compare_reports(r: &Restored, reports: &[Report], s: &Snapshot) -> Result<Vec<String>, String> {
    let w = &r.world;
    let mut out = Vec::new();
    let mut game: Vec<(String, String)> = Vec::new();
    for c in s.of("Collision2D") {
        let method = c.name.split(':').next().unwrap().rsplit('.').next().unwrap();
        let rest: Vec<String> = ["collider", "other", "rigidbody", "otherRigidbody", "rv", "enabled", "contacts"].iter().map(|k| format!("{k}={}", c.get(k).unwrap_or("?"))).collect();
        let line = (method.to_string(), rest.join(" "));
        if game.last() != Some(&line) {
            game.push(line);
        }
    }
    let collider_name: HashMap<i32, String> = r.fixtures.keys().map(|n| (object_ref(n).unwrap().1, n.rsplit_once(':').unwrap().0.to_string())).collect();
    let rb_name: HashMap<i32, String> = r.bodies.keys().filter(|n| *n != "ground").map(|n| (object_ref(n).unwrap().1, n.clone())).collect();
    let h = |f: f32| format!("{:08x}", f.to_bits());
    let mut ours = Vec::new();
    for rep in reports {
        let Some(col) = &rep.collision else { continue };
        for receiving in [rep.collider_a.id, rep.collider_b.id] {
            let sc = w.script_collision(col, receiving);
            let rbn = |b: Option<i32>| b.map_or("null".to_string(), |id| rb_name[&id].clone());
            let contacts: Vec<String> = sc
                .contacts
                .iter()
                .map(|p| {
                    [h(p.point.x), h(p.point.y), h(p.normal.x), h(p.normal.y), h(p.separation), h(p.normal_impulse), h(p.tangent_impulse), h(p.relative_velocity.x), h(p.relative_velocity.y), (p.enabled as u8).to_string(), collider_name[&p.collider].clone(), collider_name[&p.other_collider].clone()].join(",")
                })
                .collect();
            let line = format!(
                "collider={} other={} rigidbody={} otherRigidbody={} rv={},{} enabled={} contacts=[{}]",
                collider_name[&sc.collider],
                collider_name[&sc.other_collider],
                rbn(sc.rigidbody),
                rbn(sc.other_rigidbody),
                h(sc.relative_velocity.x),
                h(sc.relative_velocity.y),
                sc.enabled as u8,
                contacts.join(";")
            );
            ours.push((message_name(rep.message).to_string(), line));
        }
    }
    for (i, (g, o)) in game.iter().zip(&ours).enumerate() {
        if g != o {
            out.push(format!("Callback[{i}] {}: game {} ours {} {}", g.0, g.1, o.0, o.1));
        }
    }
    if game.len() != ours.len() {
        out.push(format!("Callback.count: game {} ours {}", game.len(), ours.len()));
    }
    Ok(out)
}

/// The world against the snapshot after its step: bodies, contacts (by fixture pair) and hinges.
pub fn compare_world(r: &Restored, s: &Snapshot) -> Result<Vec<String>, String> {
    let w = &r.world;
    let mut out = Vec::new();
    let body_name: HashMap<BodyId, &str> = r.bodies.iter().map(|(n, &b)| (b, n.as_str())).collect();
    let fixture_name: HashMap<FixtureId, &str> = r.fixtures.iter().map(|(n, &f)| (f, n.as_str())).collect();
    let theirs = s.of("b2World").next().ok_or("no b2World record")?.req("bodies")?.to_string();
    let ours = w.body_list.iter().rev().map(|b| body_name[b]).collect::<Vec<_>>().join(";");
    if theirs != ours {
        out.push(format!("b2World.bodies: game {theirs} ours {ours}"));
    }
    for e in s.of("b2Edges") {
        let Some(&id) = r.bodies.get(&e.name) else {
            out.push(format!("b2Body {}: missing", e.name));
            continue;
        };
        let b = w.body(id);
        let mut game = words(e.req("body")?);
        game[1] &= 0xffff;
        let ours = body_words(b);
        for (k, (g, o)) in game.iter().zip(&ours).enumerate() {
            if g != o {
                out.push(format!("b2Body {}.w{k}: game {g:08x} ours {o:08x}", e.name));
            }
        }
        let sleep = words(e.req("mass")?)[9];
        if sleep != b.sleep_time.to_bits() {
            out.push(format!("b2Body {}.sleepTime: game {sleep:08x} ours {:08x}", e.name, b.sleep_time.to_bits()));
        }
    }
    let pair = |c: ContactId| {
        let c = w.contact(c);
        format!("{}/{}", fixture_name[&c.fixture_a], fixture_name[&c.fixture_b])
    };
    let ours_list: Vec<String> = w.contact_list.iter().rev().map(|&c| pair(c)).collect();
    let mut game_list = Vec::new();
    for c in s.of("b2Contact") {
        let key = format!("{}/{}", c.req("a")?, c.req("b")?);
        game_list.push(key.clone());
        let Some(&id) = w.contact_list.iter().find(|&&id| pair(id) == key) else {
            out.push(format!("b2Contact {key}: missing"));
            continue;
        };
        let ct = w.contact(id);
        let toi = words(c.req("toi")?);
        let flags = words(c.req("flags")?)[0];
        let mut fields = vec![("flags", flags, ct.flags), ("toiCount", toi[0], ct.toi_count as u32), ("managerIndex", toi[5], ct.manager_index as u32)];
        // m_toi only means something under the TOI flag; otherwise it is a stale value.
        if flags & crate::b2::world::CONTACT_TOI != 0 {
            fields.push(("toi", toi[1], ct.toi.to_bits()));
        }
        for (k, g, o) in fields {
            if g != o {
                out.push(format!("b2Contact {key}.{k}: game {g:08x} ours {o:08x}"));
            }
        }
        // Only the live manifold words: a separated b2Collide* sets pointCount 0 and leaves the rest stale.
        let gm = words(c.req("manifold")?);
        let count = gm[15] as usize;
        let live = |k: usize| k == 15 || (count > 0 && (k >= 10 || k < 5 * count));
        for (k, (g, o)) in gm.iter().zip(manifold_words(&ct.manifold)).enumerate().filter(|&(k, _)| live(k) && k < 16) {
            if *g != o {
                out.push(format!("b2Contact {key}.m{k}: game {g:08x} ours {o:08x}"));
            }
        }
    }
    for k in ours_list.iter().filter(|k| !game_list.contains(k)) {
        out.push(format!("b2Contact {k}: extra"));
    }
    // The proxies' AABBs and fat tree AABBs (history-dependent: they decide which pairs the broad-phase finds).
    for rec in s.of("b2Fixture") {
        let Some(&f) = r.fixtures.get(&rec.name) else { continue };
        let theirs: Vec<Vec<u32>> = names(rec.req("proxies")?).iter().map(|p| words(p.split_once(',').map_or("", |x| x.1))).collect();
        let ours: Vec<Vec<u32>> = w.fixtures[f].as_ref().unwrap().proxies.iter().map(|(a, id)| {
            let fat = w.broad_phase.tree.fat_aabb(*id);
            [a.lower.x, a.lower.y, a.upper.x, a.upper.y, fat.lower.x, fat.lower.y, fat.upper.x, fat.upper.y].map(f32::to_bits).to_vec()
        }).collect();
        if theirs != ours {
            out.push(format!("b2Fixture {}.proxies: game {theirs:08x?} ours {ours:08x?}", rec.name));
        }
    }
    if out.is_empty() && ours_list != game_list {
        out.push(format!("b2World.contacts: game {} ours {}", game_list.join(";"), ours_list.join(";")));
    }
    for (h, raw) in s.of("HingeJoint2D").zip(s.of("b2RevoluteJoint")) {
        let j = raw.req("joint")?;
        if j == "null" {
            continue;
        }
        let own = s.of("Rigidbody2D").find(|r| r.name == h.name).unwrap();
        let a = r.bodies[&format!("{}#{}", own.name, own.id)];
        let Some(&(_, id)) = w.body(a).joints.iter().find(|&&(o, id)| w.joints[id].as_ref().unwrap().body_a == a && Some(o) == r.bodies.get(h.req("connected").unwrap().trim_start_matches('@')).copied()) else {
            out.push(format!("b2RevoluteJoint {}: missing", h.name));
            continue;
        };
        let rev = &w.joints[id].as_ref().unwrap().revolute();
        let g = words(j);
        let ours = [rev.impulse[0].to_bits(), rev.impulse[1].to_bits(), rev.impulse[2].to_bits(), rev.motor_impulse.to_bits()];
        for (k, (gw, o)) in g[4..8].iter().zip(ours).enumerate() {
            if *gw != o {
                out.push(format!("b2RevoluteJoint {}.w{}: game {gw:08x} ours {o:08x}", h.name, k + 4));
            }
        }
        if g[37] != rev.limit_state {
            out.push(format!("b2RevoluteJoint {}.limitState: game {} ours {}", h.name, g[37], rev.limit_state));
        }
    }
    Ok(out)
}

pub fn physics_test(dump: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(dump).map_err(|e| e.to_string())?;
    let settings: HashMap<&str, &str> = text
        .lines()
        .find(|l| l.starts_with("physics "))
        .ok_or("no physics line")?
        .split(' ')
        .skip(1)
        .filter_map(|kv| kv.split_once('='))
        .collect();
    let iterations = |k: &str| -> Result<i32, String> { settings.get(k).ok_or(format!("physics {k}"))?.parse().map_err(|_| format!("physics {k}")) };
    let (vel, pos) = (iterations("vel")?, iterations("pos")?);
    // "gravity=x|hex y|hex": the y component is the token after the key.
    let line = text.lines().find(|l| l.starts_with("physics ")).unwrap();
    let g: Vec<f32> = line[line.find("gravity=").ok_or("physics gravity")? + 8..].split(' ').take(2).map(|c| f(c.rsplit('|').next().unwrap())).collect();
    let snaps = parse(&text);
    drop(text);
    let (mut segments, mut failed) = (0, 0);
    let mut kinds: std::collections::BTreeMap<String, usize> = Default::default();
    for pair in snaps.windows(2) {
        if pair[0].pt != "FM" || pair[1].pt != "FE" {
            continue;
        }
        segments += 1;
        let dt = f(pair[0].snap("fixedDt"));
        let mut r = restore(&pair[0], V::new(g[0], g[1]))?;
        r.world.step(dt, vel, pos);
        let reports = r.world.process_contacts(true);
        let mut diffs = compare_world(&r, &pair[1])?;
        if pair[0].of("Collision2DRecord").next().is_some() {
            diffs.extend(compare_contacts2d(&r, &pair[1])?);
            diffs.extend(compare_reports(&r, &reports, &pair[0])?);
        }
        if let Ok(path) = std::env::var("PPG_DIFFS") {
            use std::io::Write;
            let mut out = std::fs::OpenOptions::new().create(true).append(true).open(path).unwrap();
            for d in &diffs {
                writeln!(out, "{segments} {d}").unwrap();
            }
        }
        for d in &diffs {
            let head = d.split(':').next().unwrap();
            let ty = head.split(' ').next().unwrap();
            let field = head.rsplit('.').next().unwrap();
            *kinds.entry(format!("{ty}.{field}")).or_default() += 1;
        }
        if !diffs.is_empty() {
            failed += 1;
            if failed <= 1 {
                eprintln!("segment FM {} ({} diffs):", pair[0].snap("frame"), diffs.len());
                for d in diffs.iter().take(25) {
                    eprintln!("  {d}");
                }
            }
        }
    }
    for (k, n) in &kinds {
        println!("  {n:5} {k}");
    }
    println!("physics segments {segments}, failed {failed}");
    if failed == 0 && segments > 0 { Ok(()) } else { Err(format!("{failed} of {segments} segments differ")) }
}
