//! What translated code (tools/translator/cs2rs.py) calls: Unity value types with Mono's rounding, null checks, and
//! the engine members it maps (tools/translator/api.py).

use crate::b2::math::Vec2 as V;
use crate::b2::shape::Polygon;
use crate::b2::world::Shape;
use crate::game::explosion::RayHit2D;
use crate::game::Game;

/// ActivationPropagation as Use receives it: its Channel (Green 0, Red 1, Blue 2).
#[derive(Clone, Copy, Debug)]
pub struct Activation {
    pub Channel: i32,
}

/// A Transform as the translated scripts hold one.
#[derive(Clone, Copy, Debug)]
pub enum Xf {
    /// A PhysicalBehaviour's GameObject.
    Physical(usize),
    /// A GameObject without a PhysicalBehaviour; whether it or a descendant has a Collider2D or Rigidbody2D.
    Bare { physics: bool },
    /// A Collider2D's GameObject.
    Collider(i32),
    /// A clone's GameObject with no PhysicalBehaviour, Collider2D or Rigidbody2D on or below it, by its Transform id.
    Object(i32),
}

/// A translated component that takes collision messages: its place among the scripts on its PhysicalBehaviour's
/// GameObject (SendMessage goes in component order).
#[derive(Clone, Copy, Debug)]
pub struct CollisionScript {
    pub physical: usize,
    /// Its component index on the GameObject, and whether that comes after the PhysicalBehaviour's.
    pub index: usize,
    pub after_physical: bool,
    pub b: super::TBehaviour,
}

/// Shot, as SendMessage("Shot"/"ExitShot") passes it.
#[derive(Clone, Copy, Debug)]
pub struct Shot {
    pub normal: [f32; 2],
    pub point: [f32; 2],
    pub damage: f32,
}

/// A GameObject as the translated scripts hold one.
#[derive(Clone, Copy, Debug)]
pub enum Go {
    /// An object of a clone, by the instance id of one of its components.
    Local(i32),
    /// A prefab asset.
    Asset(crate::assets::db::ObjRef),
}

impl Go {
    fn local(self) -> i32 {
        match self {
            Go::Local(id) => id,
            Go::Asset(r) => unimplemented!("a prefab asset ({r:?}) used as a scene GameObject"),
        }
    }
}

impl Xf {
    /// `base.transform` of a translated component.
    pub fn this(physical: Option<usize>, physics: bool) -> Xf {
        match physical {
            Some(p) => Xf::Physical(p),
            None => Xf::Bare { physics },
        }
    }
}

/// A null reference dereferenced.
pub fn nn<T>(o: Option<T>) -> T {
    o.unwrap_or_else(|| unimplemented!("NullReferenceException in a translated script"))
}

/// A call or assignment on a presentation component (`present`: the field is set).
pub fn pres(present: bool) {
    if !present {
        unimplemented!("NullReferenceException: a null presentation component in a translated script");
    }
}

/// `transform.localRotation = ...` or `localPosition = ...` that only shows: the object must not move anything physical.
pub fn presentation_pose(xf: Xf) {
    if !matches!(xf, Xf::Bare { physics: false } | Xf::Object(_)) {
        unimplemented!("Transform.localRotation/localPosition of an object with physics ({xf:?})");
    }
}

/// An array index.
pub fn index(i: i32) -> usize {
    usize::try_from(i).unwrap_or_else(|_| unimplemented!("IndexOutOfRangeException in a translated script"))
}

/// `new T[n]` (zero or null entries).
pub fn new_array<T: Clone + Default>(n: i32) -> Vec<T> {
    vec![T::default(); usize::try_from(n).unwrap_or_else(|_| unimplemented!("OverflowException: a negative array size in a translated script"))]
}

/// `(int)` of a double: cvttsd2si, int.MinValue when out of range or NaN.
pub fn to_int(v: f64) -> i32 {
    if v.is_nan() || v >= 2147483648.0 || v <= -2147483649.0 { i32::MIN } else { v as i32 }
}

/// A NonAlloc query's results copied into the script's buffer: the count that fits.
pub fn fill(buffer: &mut [i32], hits: Vec<i32>) -> i32 {
    let n = hits.len().min(buffer.len());
    buffer[..n].copy_from_slice(&hits[..n]);
    n as i32
}

/// RaycastHit2D.point (a default RaycastHit2D's is zero).
pub fn hit_point(h: &Option<RayHit2D>) -> [f32; 2] {
    h.as_ref().map_or([0.0; 2], |h| [h.point.x, h.point.y])
}

pub fn v2(v: [f32; 3]) -> [f32; 2] {
    [v[0], v[1]]
}

pub fn v3(v: [f32; 2]) -> [f32; 3] {
    [v[0], v[1], 0.0]
}

/// Vector2/Vector3 op_Addition (each component a float add: rounding the double sum gives the same).
pub fn add<const N: usize>(a: [f32; N], b: [f32; N]) -> [f32; N] {
    std::array::from_fn(|k| a[k] + b[k])
}

pub fn sub<const N: usize>(a: [f32; N], b: [f32; N]) -> [f32; N] {
    std::array::from_fn(|k| a[k] - b[k])
}

pub fn mul<const N: usize>(a: [f32; N], d: f32) -> [f32; N] {
    std::array::from_fn(|k| a[k] * d)
}

pub fn div<const N: usize>(a: [f32; N], d: f32) -> [f32; N] {
    std::array::from_fn(|k| a[k] / d)
}

pub fn neg<const N: usize>(a: [f32; N]) -> [f32; N] {
    std::array::from_fn(|k| -a[k])
}

pub fn normalized3(v: [f32; 3]) -> [f32; 3] {
    crate::game::explosion::normalized3(v)
}

pub fn dot2(a: [f32; 2], b: [f32; 2]) -> f32 {
    crate::game::physical::dot2(a, b)
}

/// Mathf.Atan2: (float)Math.Atan2 (the CRT's atan2, as Rust's on this target).
pub fn atan2(y: f32, x: f32) -> f32 {
    (y as f64).atan2(x as f64) as f32
}

impl Game {
    /// A HingeJoint2D reference used: a broken joint is a destroyed component.
    pub fn tr_hinge(&self, h: Option<usize>) -> usize {
        let h = nn(h);
        if self.hinges[h].destroyed {
            unimplemented!("MissingReferenceException: a destroyed HingeJoint2D in a translated script");
        }
        h
    }

    fn tr_physical_xf(&self, xf: Xf) -> usize {
        match xf {
            Xf::Physical(p) => p,
            _ => unimplemented!("a Transform without a PhysicalBehaviour in a translated script ({xf:?})"),
        }
    }

    /// `a == b` for Transforms (null equals only null).
    pub fn tr_xf_eq(&self, a: Option<Xf>, b: Option<Xf>) -> bool {
        let (Some(a), Some(b)) = (a, b) else { return a.is_none() && b.is_none() };
        let owner = |x: Xf| match x {
            Xf::Physical(p) => Some(Some(p)),
            Xf::Collider(c) => self.physical_of_collider(c).map(Some),
            Xf::Bare { .. } | Xf::Object(_) => None,
        };
        match (a, b) {
            (Xf::Collider(x), Xf::Collider(y)) if x == y => true,
            (Xf::Object(x), Xf::Object(y)) => x == y,
            _ => match (owner(a), owner(b)) {
                (Some(x), Some(y)) => x == y,
                // A bare object against a physical's: never the same GameObject.
                (Some(_), None) | (None, Some(_)) => false,
                (None, None) => unimplemented!("comparing the Transforms of two objects without a PhysicalBehaviour"),
            },
        }
    }

    pub fn tr_position(&self, xf: Xf) -> [f32; 3] {
        self.physicals[self.tr_physical_xf(xf)].transform.position
    }

    pub fn tr_local_scale(&self, xf: Xf) -> [f32; 3] {
        self.physicals[self.tr_physical_xf(xf)].transform.localScale
    }

    pub fn tr_lossy_scale(&self, xf: Xf) -> [f32; 3] {
        self.lossy_scale(self.tr_physical_xf(xf))
    }

    /// Transform.eulerAngles of a rotation about z.
    pub fn tr_euler_angles(&self, xf: Xf) -> [f32; 3] {
        let q = self.physicals[self.tr_physical_xf(xf)].transform.rotation;
        if q[0] != 0.0 || q[1] != 0.0 {
            unimplemented!("Transform.eulerAngles of a rotation off the z axis");
        }
        [0.0, 0.0, crate::game::dump::euler_z(q)]
    }

    pub fn tr_transform_point(&self, xf: Xf, v: [f32; 3]) -> [f32; 3] {
        self.transform_point3(self.tr_physical_xf(xf), v)
    }

    pub fn tr_transform_direction(&self, xf: Xf, v: [f32; 3]) -> [f32; 3] {
        self.transform_direction(self.tr_physical_xf(xf), v)
    }

    pub fn tr_inverse_transform_point(&self, xf: Xf, v: [f32; 3]) -> [f32; 3] {
        self.inverse_transform_point(self.tr_physical_xf(xf), v)
    }

    /// Transform.TransformVector (1807f6f50): scale then rotation, from the object up through its parents.
    pub fn tr_transform_vector(&self, xf: Xf, v: [f32; 3]) -> [f32; 3] {
        let mut w = [v[0], v[1], v[2], 0.0];
        for node in self.local_chain(self.tr_physical_xf(xf)).iter().rev() {
            w = crate::unity::transform::global_vector(node, w);
        }
        [w[0], w[1], w[2]]
    }

    /// Collision2D.rigidbody: the PhysicalBehaviour of the Rigidbody2D (by its instance id), None for none.
    pub fn tr_rigidbody_physical(&self, rb: Option<i32>) -> Option<usize> {
        let id = rb?;
        let w = &self.world;
        Some(self.physicals.iter().position(|p| p.rigidbody.detached.is_none() && w.body(p.rigidbody.body).user.is_some_and(|u| u.id == id))
            .unwrap_or_else(|| unimplemented!("Collision2D.rigidbody: a Rigidbody2D without a PhysicalBehaviour")))
    }

    /// Collision2D.transform: the rigidbody's Transform, else the collider's.
    pub fn tr_collision_transform(&self, c: &crate::b2::contacts2d::ScriptCollision) -> Xf {
        match self.tr_rigidbody_physical(c.rigidbody) {
            Some(p) => Xf::Physical(p),
            None => Xf::Collider(c.collider),
        }
    }

    /// GameObject.activeInHierarchy
    pub fn tr_active_in_hierarchy(&mut self, go: Go) -> bool {
        self.with_component_instance(go.local(), |_, inst, o| inst.objects[o].active)
    }

    /// GameObject.activeSelf
    pub fn tr_active_self(&mut self, go: Go) -> bool {
        self.with_component_instance(go.local(), |_, inst, o| inst.objects[o].self_active)
    }

    /// GameObject.SetActive (nothing when activeSelf already is `v`), with the scripts' OnEnable/OnDisable.
    pub fn tr_set_active(&mut self, go: Go, v: bool) {
        self.with_component_instance(go.local(), |g, inst, o| {
            if inst.objects[o].self_active != v {
                if g.awaking {
                    unimplemented!("GameObject.SetActive({v}) in an Awake/OnEnable during Instantiate (the Awake pass skips inactive objects)");
                }
                g.set_active_scripts(inst, o, v);
            }
        });
    }

    /// GetComponent<T>() != null for a presentation component (its Unity class id) on the GameObject of component `id`.
    pub fn tr_has_component(&mut self, id: i32, class_id: i32) -> bool {
        use crate::assets::db::Component;
        self.with_component_instance(id, |_, inst, o| {
            inst.prefab.objects[o].components.iter().any(|(_, c)| match c {
                Component::SpriteRenderer(_) => class_id == 212,
                Component::Other(k) => *k == class_id,
                _ => false,
            })
        })
    }

    /// Object.Destroy(gameObject, t): the GameObject goes at the end of the frame (or after `t`).
    pub fn tr_destroy_go(&mut self, go: Option<Go>, t: f32) {
        let go = go.unwrap_or_else(|| unimplemented!("Object.Destroy(null)"));
        self.destroy_object_after(go.local(), t);
    }

    /// Object.Destroy(this) of a translated script: it goes at the end of the frame (or after `t`).
    pub fn tr_destroy_component(&mut self, id: i32, t: f32) {
        if !self.destroyed_components.contains(&id) && !self.component_destroys.iter().any(|&(_, i)| i == id) {
            let at = self.time.fixedTimeAsDouble.min(self.time.timeAsDouble) + t as f64;
            self.component_destroys.push((at, id));
        }
    }

    /// The delayed Destroy of a translated script: RemoveFromManager with OnDisable if it was running, OnDestroy if
    /// it woke, its coroutines stop, and `(bool)` reads false.
    pub fn tr_run_component_destroy(&mut self, id: i32) {
        self.component_destroys.retain(|&(_, i)| i != id);
        if !self.destroyed_components.insert(id) {
            return;
        }
        self.stop_all_coroutines(id);
        if let Some((_, b)) = self.behaviour_of.remove(&id) {
            let super::super::Behaviour::Translated(t) = b else { unreachable!("Destroy(this) of a hand-written script") };
            let running = self.tr_enabled(t) && self.with_component_instance(id, |_, inst, o| inst.objects[o].active);
            self.behaviours.remove(b);
            if running {
                self.tr_on_disable(t);
            }
            self.tr_on_destroy(t);
        }
    }

    /// Object.Instantiate(original, position, rotation) of a prefab asset: the clone's Awake/OnEnable run inside.
    pub fn tr_instantiate(&mut self, original: Option<Go>, position: [f32; 3], rotation: [f32; 4]) -> Option<Go> {
        let r = match original {
            Some(Go::Asset(r)) => r,
            Some(Go::Local(id)) => unimplemented!("Object.Instantiate of a scene GameObject (component {id})"),
            None => unimplemented!("Object.Instantiate(null) (ArgumentException)"),
        };
        let mut db = self.assets.take().expect("Object.Instantiate without the game's data");
        let spawned = db.prefab(r).and_then(|prefab| self.spawn(&mut db, &prefab, position, rotation, None));
        self.assets = Some(db);
        let (inst, _) = spawned.unwrap_or_else(|e| panic!("Object.Instantiate: {e}"));
        let root = inst.objects[0].components[0];
        self.unsynced.push(inst);
        Some(Go::Local(root))
    }

    /// GetComponent<T>() != null for a MonoBehaviour class T (exactly that class: none of the game's are subclassed).
    pub fn tr_has_script(&mut self, id: i32, class: &str) -> bool {
        use crate::assets::db::Component;
        self.with_component_instance(id, |_, inst, o| {
            inst.prefab.objects[o].components.iter().any(|(_, c)| matches!(c, Component::Script { class: k, .. } if k == class))
        })
    }

    /// `transform.right = v`: Transform.rotation = Quaternion.FromToRotation(Vector3.right, v).
    pub fn tr_set_right(&mut self, xf: Xf, v: [f32; 3]) {
        self.tr_set_rotation(xf, crate::unity::transform::from_to_rotation([1.0, 0.0, 0.0], v));
    }

    /// `transform.up = v`: Transform.rotation = Quaternion.FromToRotation(Vector3.up, v).
    pub fn tr_set_up(&mut self, xf: Xf, v: [f32; 3]) {
        self.tr_set_rotation(xf, crate::unity::transform::from_to_rotation_up(v));
    }

    /// Transform.rotation = q on a clone's object: the PhysicalBehaviours below it see their new poses, and the
    /// bodies at the next SyncTransforms.
    pub fn tr_set_rotation(&mut self, xf: Xf, q: [f32; 4]) {
        if let Xf::Object(id) = xf {
            return self.with_component_instance(id, |_, inst, o| inst.set_world_rotation(o, q));
        }
        let Xf::Physical(p) = xf else { unimplemented!("Transform.rotation = on a Transform without a PhysicalBehaviour ({xf:?})") };
        let id = self.physicals[p].transform_id;
        self.with_component_instance(id, |g, inst, o| {
            inst.set_world_rotation(o, q);
            for x in std::iter::once(o).chain(inst.descendants(o)) {
                let t = inst.objects[x].components[0];
                if let Some(ph) = g.physicals.iter().position(|ph| ph.transform_id == t) {
                    g.physicals[ph].transform = crate::game::link::pose(inst, x);
                }
            }
        });
        if let Some(i) = self.retained.iter().position(|i| i.objects.iter().any(|x| x.components.contains(&id))) {
            let inst = self.retained.remove(i);
            self.unsynced.push(inst);
        }
    }

    /// Transform.parent
    pub fn tr_parent(&mut self, xf: Xf) -> Option<Xf> {
        let Xf::Physical(p) = xf else { unimplemented!("Transform.parent of a Transform without a PhysicalBehaviour ({xf:?})") };
        let parent = self.with_component_instance(self.physicals[p].transform_id, |_, inst, o| inst.objects[o].parent.map(|q| inst.objects[q].components[0]))?;
        Some(self.tr_go_transform(Go::Local(parent)))
    }

    /// `transform.parent = v` (SetParent(v, true)).
    pub fn tr_set_parent(&mut self, xf: Xf, v: Option<Xf>) {
        let Xf::Physical(p) = xf else { unimplemented!("Transform.parent = of a Transform without a PhysicalBehaviour ({xf:?})") };
        if v.is_some() {
            unimplemented!("Transform.parent = a Transform");
        }
        self.set_parent_null_of(self.physicals[p].transform_id, |_, _, _| {});
    }

    /// Transform.root: the topmost Transform above it (SetParent included).
    pub fn tr_root(&mut self, xf: Xf) -> Xf {
        let Xf::Physical(p) = xf else { unimplemented!("Transform.root of a Transform without a PhysicalBehaviour ({xf:?})") };
        let root = self.with_component_instance(self.physicals[p].transform_id, |_, inst, o| {
            let mut r = o;
            while let Some(q) = inst.objects[r].parent {
                r = q;
            }
            inst.objects[r].components[0]
        });
        self.tr_go_transform(Go::Local(root))
    }

    /// GameObject.transform
    pub fn tr_go_transform(&mut self, go: Go) -> Xf {
        let id = go.local();
        if let Some(p) = self.physicals.iter().position(|p| p.transform_id == id) {
            return Xf::Physical(p);
        }
        self.with_component_instance(id, |_, inst, o| {
            let prefab = inst.prefab.clone();
            let physics = std::iter::once(o).chain(inst.descendants(o)).any(|d| prefab.objects[d].components.iter().any(|(_, x)| matches!(x, crate::assets::db::Component::Collider2D(_) | crate::assets::db::Component::Rigidbody2D(_))));
            if physics { Xf::Bare { physics } } else { Xf::Object(id) }
        })
    }

    /// PhysicalObjectsInWorldByTransform.TryGetValue
    pub fn tr_physical_by_transform(&self, xf: Xf) -> Option<usize> {
        let p = match xf {
            Xf::Physical(p) => Some(p),
            Xf::Collider(c) => self.physical_of_collider(c),
            Xf::Bare { .. } | Xf::Object(_) => None,
        };
        p.filter(|&p| self.physicals[p].in_world && !self.physicals[p].destroyed)
    }

    /// Physics2D.OverlapBoxNonAlloc's hits (PhysicsQuery2D::OverlapBoxAll, 180bf3610): the angle times Deg2Rad,
    /// OverlapBoxQuery2D (180bf0fd0) SetAsBox(max(size / 2, 1e-4), point, angle) with b2_polygonRadius at the
    /// identity, then the overlap filter and its sort.
    pub fn tr_overlap_box(&mut self, point: [f32; 2], size: [f32; 2], angle: f32, mask: u32) -> Vec<i32> {
        self.sync_pending();
        let min = f32::from_bits(0x38d1b717);
        let center = V::new(point[0], point[1]);
        let poly = Polygon::set_as_box_angle((size[0] * 0.5).max(min), (size[1] * 0.5).max(min), center, angle * crate::game::mathf::DEG2RAD, crate::b2::settings::globals().polygon_radius);
        let (mut lower, mut upper) = (poly.vertices[0], poly.vertices[0]);
        for v in &poly.vertices[1..4] {
            (lower, upper) = (V::new(lower.x.min(v.x), lower.y.min(v.y)), V::new(upper.x.max(v.x), upper.y.max(v.y)));
        }
        let r = poly.radius;
        let aabb = crate::b2::broadphase::Aabb { lower: V::new(lower.x - r, lower.y - r), upper: V::new(upper.x + r, upper.y + r) };
        let identity = crate::b2::math::Transform { p: V::ZERO, q: crate::b2::math::Rot::new(0.0) };
        let shape = Shape::Polygon(poly);
        let hits = self.overlap_query(self.world.query_aabb(&aabb), mask, f32::NEG_INFINITY, f32::INFINITY, |other, child, xf| shape.test_overlap(0, &identity, other, child, xf));
        if hits.len() > 32 {
            unimplemented!("Physics2D.OverlapBox with more than 32 hits (std::sort is not a stable insertion sort)");
        }
        hits.into_iter().map(|h| h.1).collect()
    }

    /// Physics2D.Raycast(origin, direction, distance, layerMask)
    pub fn tr_raycast(&mut self, origin: [f32; 2], direction: [f32; 2], distance: f32, mask: u32) -> Option<RayHit2D> {
        self.raycast(V::new(origin[0], origin[1]), direction, distance, mask, f32::NEG_INFINITY, f32::INFINITY)
    }
}

/// UnityEngine.Object == on two Collider2Ds (Object.CompareBaseObjects): two references are equal by instance id;
/// a null one (0) equals the other only when that one is destroyed.
pub fn collider_eq(a: i32, b: i32) -> bool {
    if (a == 0) != (b == 0) {
        unimplemented!("Collider2D == null: whether a collider is still alive");
    }
    a == b
}
