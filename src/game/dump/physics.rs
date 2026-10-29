//! `--physics-test <dump>`: each FixedUpdate's physics step (FM -> FE) replayed in a b2 world restored from
//! the recorder's b2World/b2Edges/b2Fixture/b2Contact/b2RevoluteJoint records, then compared with the next
//! snapshot word for word. Native lists are restored tail first, so every list matches the game's order.
use super::*;
use crate::b2::broadphase::Aabb;
use crate::b2::collide::{ContactFeature, Manifold, ManifoldPoint, ManifoldType};
use crate::b2::shape::{MassData, Polygon};
use crate::b2::world::{Body, BodyId, ContactId, Filter, FixtureDef, FixtureId, JointId, Shape, World};

struct Restored {
    world: World,
    bodies: HashMap<String, BodyId>,
    fixtures: HashMap<String, FixtureId>,
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
fn restore(s: &Snapshot, gravity: V) -> Result<Restored, String> {
    let mut world = World::new(gravity);
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
        let id = world.create_body(&BodyDef { kind, ..Default::default() });
        load_body(world.body_mut(id), e)?;
        bodies.insert(name.to_string(), id);
        for &f in names(e.req("fixtures")?).iter().rev() {
            let (def, mass) = fixture_def(fixture_records.get(f).ok_or(format!("no b2Fixture {f}"))?)?;
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
            if a < b {
                world.ignored.insert((a, b));
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
        let a = bodies[&format!("{}#{}", own.name, own.id)];
        let b = *bodies.get(connected).ok_or(format!("hinge {} connects to {connected}", h.name))?;
        let id = world.create_joint(def, a, b, false);
        let rev = &mut world.joints[id].as_mut().unwrap().revolute;
        rev.impulse = [fl(4), fl(5), fl(6)];
        rev.motor_impulse = fl(7);
        rev.limit_state = w[37];
        joints.insert(format!("{}#{}", h.name, h.id), id);
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
        contacts[i] = id;
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
    Ok(Restored { world, bodies, fixtures })
}

/// The world against the snapshot after its step: bodies, contacts (by fixture pair) and hinges.
fn compare_world(r: &Restored, s: &Snapshot) -> Result<Vec<String>, String> {
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
        let rev = &w.joints[id].as_ref().unwrap().revolute;
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
        let diffs = compare_world(&r, &pair[1])?;
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
