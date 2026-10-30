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
}

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
    /// Returns the MonoBehaviours of queue 0x14 in remap order, (object, component).
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
        let behaviours = queues[0x14].iter().map(|&(o, c)| (o, c.unwrap())).collect();
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
            let mut q = inst.rotation(o);
            if q[3] < 0.0 {
                (q[2], q[3]) = (neg(q[2]), neg(q[3]));
            }
            def.position = V::new(p[0], p[1]);
            let a = crate::unity::atan2f(q[2], q[3]);
            def.angle = a + a;
        }
        let body = self.world.create_body(&def);
        self.world.reset_mass_data(body);
        inst.bodies.insert(o, body);
        if rb.interpolate != 0 && rb.interpolate != 1 {
            unimplemented!("Rigidbody2D extrapolation");
        }
    }

    /// Collider2D::AwakeFromLoad (180c0c370) in instantiate mode: CreateFixtures on an active object.
    fn collider2d_awake(&mut self, inst: &mut Instance, prefab: &Prefab, o: usize, c: usize) {
        if inst.objects[o].active {
            self.collider2d_create_fixtures(inst, prefab, o, c);
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
    /// Joint2D::FinalizeCreateJoint (180c048c0) for a clone (no previous state, anchors not yet configured).
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
        let reference_angle = self.world.body(b).sweep.a - self.world.body(a).sweep.a;
        let s = inst.lossy_scale(o);
        let anchor_a = V::new(s[0] * j.anchor[0], s[1] * j.anchor[1]);
        let anchor_b = if !j.auto_configure_connected_anchor {
            let sb = match connected {
                Some(b) => inst.lossy_scale(b),
                None => [1.0, 1.0, 1.0],
            };
            V::new(sb[0] * j.connected_anchor[0], sb[1] * j.connected_anchor[1])
        } else {
            let xa = self.world.body(a).xf;
            let wx = (anchor_a.x * xa.q.c - anchor_a.y * xa.q.s) + xa.p.x;
            let wy = xa.q.s * anchor_a.x + anchor_a.y * xa.q.c + xa.p.y;
            match connected {
                None => V::new(wx, wy),
                Some(_) => {
                    let xb = self.world.body(b).xf;
                    let (dx, dy) = (wx - xb.p.x, wy - xb.p.y);
                    V::new(xb.q.c * dx + xb.q.s * dy, xb.q.c * dy - xb.q.s * dx)
                }
            }
        };
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::db::AssetDb;
    use crate::assets::mono::Value;

    /// The Person prefab instantiated natively against dump26stabhead2's first snapshot: every limb body's words,
    /// its fixture proxies and its hinge's b2 definition.
    #[test]
    fn person_spawn_matches_game() {
        let Ok(game) = crate::game_assets::find_game(None) else { return };
        let dump = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("reference-probe/game/dump26stabhead2.txt");
        let Ok(mut snaps) = dump::snapshots(&dump) else { return };
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
        let mut prefab = db.prefab(person.unwrap()).unwrap();
        // PersonBehaviour.Awake (queue 0x14, not part of this native check) rescales the root at random: the
        // recorded root scale stands in for it here, and the recorded spawn point for the recorder's.
        let root = first.of("TransformChain").find(|r| r.name == "Human").unwrap();
        let hex3 = |k: &str| -> [f32; 3] {
            let w: Vec<f32> = root.get(k).unwrap().split(',').map(|w| f32::from_bits(u32::from_str_radix(w, 16).unwrap())).collect();
            [w[0], w[1], w[2]]
        };
        if let Some((_, Component::Transform(t))) = prefab.objects[0].components.first_mut() {
            t.local_scale = hex3("lscale");
        }
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
        let human_go = first.of("TransformChain").find(|r| r.name == "Human").unwrap().id + 2;
        let mut g = Game::default();
        g.world = World::new(V::new(0.0, -9.81));
        g.next_instance_id = human_go;
        let (inst, _) = g.instantiate_native(&prefab, spawn, [0.0, 0.0, 0.0, 1.0], &original_ids);
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
