//! ShatteredObjectGenerator and the helpers it calls: ProceduralShatterMaterialPool, ShatteredColliderShapeCache,
//! Utils.ComputeConvexHull / GetMin and the mscorlib introsort behind Array.Sort(T[], Comparison<T>).
//!
//! The Unity-object operations (new GameObject, AddComponent, SetPath, joints, ClosestPoint ...) have no Rust state
//! yet: `ShatterHost` lists each one the generator performs, in the C# order, with the values the C# computes.
//! The `Game` implementation builds each shard as a one-object prefab instance on the simulation's own state.

use super::{d, mathf};
use crate::assets::db::{Component, ObjRef, Prefab, PrefabObject};
use crate::assets::mono::Value;
use crate::unity::Random;
use std::collections::HashMap;

pub type Vec2 = [f32; 2];

// ---------------------------------------------------------------------------------------------------------------
// mscorlib ArraySortHelper<T>.IntrospectiveSort (Array.Sort(T[], Comparison<T>)).

fn floor_log2(mut n: usize) -> usize {
    let mut r = 0;
    while n >= 1 {
        r += 1;
        n /= 2;
    }
    r
}

fn swap_if_greater<T: Copy>(keys: &mut [T], cmp: &mut impl FnMut(&T, &T) -> i32, a: usize, b: usize) {
    if a != b && cmp(&keys[a], &keys[b]) > 0 {
        keys.swap(a, b);
    }
}

fn insertion_sort<T: Copy>(keys: &mut [T], lo: usize, hi: usize, cmp: &mut impl FnMut(&T, &T) -> i32) {
    for i in lo..hi {
        let mut j = i as isize;
        let val = keys[i + 1];
        while j >= lo as isize && cmp(&val, &keys[j as usize]) < 0 {
            keys[j as usize + 1] = keys[j as usize];
            j -= 1;
        }
        keys[(j + 1) as usize] = val;
    }
}

fn down_heap<T: Copy>(keys: &mut [T], mut i: usize, n: usize, lo: usize, cmp: &mut impl FnMut(&T, &T) -> i32) {
    let val = keys[lo + i - 1];
    while i <= n / 2 {
        let mut child = 2 * i;
        if child < n && cmp(&keys[lo + child - 1], &keys[lo + child]) < 0 {
            child += 1;
        }
        if cmp(&val, &keys[lo + child - 1]) >= 0 {
            break;
        }
        keys[lo + i - 1] = keys[lo + child - 1];
        i = child;
    }
    keys[lo + i - 1] = val;
}

fn heapsort<T: Copy>(keys: &mut [T], lo: usize, hi: usize, cmp: &mut impl FnMut(&T, &T) -> i32) {
    let n = hi - lo + 1;
    for i in (1..=n / 2).rev() {
        down_heap(keys, i, n, lo, cmp);
    }
    for i in (2..=n).rev() {
        keys.swap(lo, lo + i - 1);
        down_heap(keys, 1, i - 1, lo, cmp);
    }
}

fn pick_pivot_and_partition<T: Copy>(keys: &mut [T], lo: usize, hi: usize, cmp: &mut impl FnMut(&T, &T) -> i32) -> usize {
    let mid = lo + (hi - lo) / 2;
    swap_if_greater(keys, cmp, lo, mid);
    swap_if_greater(keys, cmp, lo, hi);
    swap_if_greater(keys, cmp, mid, hi);
    let val = keys[mid];
    keys.swap(mid, hi - 1);
    let mut left = lo;
    let mut right = hi - 1;
    while left < right {
        loop {
            left += 1;
            if cmp(&keys[left], &val) >= 0 {
                break;
            }
        }
        loop {
            right -= 1;
            if cmp(&val, &keys[right]) >= 0 {
                break;
            }
        }
        if left >= right {
            break;
        }
        keys.swap(left, right);
    }
    keys.swap(left, hi - 1);
    left
}

fn intro_sort<T: Copy>(keys: &mut [T], lo: isize, mut hi: isize, mut depth_limit: usize, cmp: &mut impl FnMut(&T, &T) -> i32) {
    while hi > lo {
        let n = hi - lo + 1;
        if n <= 16 {
            let (l, h) = (lo as usize, hi as usize);
            match n {
                1 => {}
                2 => swap_if_greater(keys, cmp, l, h),
                3 => {
                    swap_if_greater(keys, cmp, l, h - 1);
                    swap_if_greater(keys, cmp, l, h);
                    swap_if_greater(keys, cmp, h - 1, h);
                }
                _ => insertion_sort(keys, l, h, cmp),
            }
            break;
        }
        if depth_limit == 0 {
            heapsort(keys, lo as usize, hi as usize, cmp);
            break;
        }
        depth_limit -= 1;
        let p = pick_pivot_and_partition(keys, lo as usize, hi as usize, cmp) as isize;
        intro_sort(keys, p + 1, hi, depth_limit, cmp);
        hi = p - 1;
    }
}

/// Array.Sort<T>(T[] array, Comparison<T> comparison)
pub fn array_sort<T: Copy>(keys: &mut [T], mut cmp: impl FnMut(&T, &T) -> i32) {
    if keys.len() >= 2 {
        let depth = 2 * floor_log2(keys.len());
        intro_sort(keys, 0, keys.len() as isize - 1, depth, &mut cmp);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Utils

/// Utils.ccw (the chain evaluates in double; the result is returned as float).
fn ccw(p1: Vec2, p2: Vec2, p3: Vec2) -> f32 {
    ((d(p2[0]) - p1[0]) * (d(p3[1]) - p1[1]) - (d(p2[1]) - p1[1]) * (d(p3[0]) - p1[0])).f()
}

/// Utils.ComputeConvexHull(Vector2[] points, List<Vector2> hull): sorts `points` in place, fills `hull`.
pub fn compute_convex_hull(points: &mut [Vec2], hull: &mut Vec<Vec2>) -> i32 {
    let num = points.len() as i32;
    let mut num2: i32 = 0;
    array_sort(points, |a, b| if a[0] > b[0] { -1 } else { 1 });
    hull.clear();
    let mut i: i32 = 0;
    while i < num {
        while num2 >= 2 && ccw(hull[(num2 - 2) as usize], hull[(num2 - 1) as usize], points[i as usize]) <= 0.0 {
            num2 -= 1;
            hull.remove(num2 as usize);
        }
        hull.push(points[i as usize]);
        num2 += 1;
        i += 1;
    }
    i = num - 2;
    let num4 = num2 + 1;
    while i >= 0 {
        while num2 >= num4 && ccw(hull[(num2 - 2) as usize], hull[(num2 - 1) as usize], points[i as usize]) <= 0.0 {
            num2 -= 1;
            hull.remove(num2 as usize);
        }
        hull.push(points[i as usize]);
        num2 += 1;
        i -= 1;
    }
    num2
}

/// Utils.GetMin(sprite.uv, v => v.sqrMagnitude): the first element with the smallest value (default when empty).
pub fn get_min_uv(uv: &[Vec2]) -> Vec2 {
    let mut num = f32::MAX;
    let mut result = [0.0, 0.0];
    for v in uv {
        let sqr = (d(v[0]) * v[0] + d(v[1]) * v[1]).f();
        if sqr < num {
            num = sqr;
            result = *v;
        }
    }
    result
}

// ---------------------------------------------------------------------------------------------------------------
// Assets the generator reads.

/// A Texture2D as ShatteredColliderShapeCache reads it: GetInstanceID, width, height and GetPixels().r per pixel
/// (row-major from the bottom row).
#[derive(Clone, Debug, Default)]
pub struct Texture {
    pub instance_id: i32,
    pub width: usize,
    pub height: usize,
    pub red: Vec<f32>,
}

/// The Sprite values Generate and the material pool read.
#[derive(Clone, Debug, Default)]
pub struct Sprite {
    /// Identity of the Sprite object (the pool's dictionary key).
    pub id: i32,
    pub rect_width: f32,
    pub rect_height: f32,
    pub pixels_per_unit: f32,
    pub uv: Vec<Vec2>,
    pub texture_width: f32,
    pub texture_height: f32,
}

/// ShatteredColliderShapeCache.Shard
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Shard {
    pub min_range: u8,
    pub size: u8,
    pub texture_asset_id: i32,
}

/// One entry of ProceduralShatterMaterialPool.Pool: the Material's _AtlasTransform and its current _ShardIDMask
/// (an index into ShatterMasks).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PooledMaterial {
    pub atlas_transform: [f32; 4],
    pub shard_id_mask: usize,
}

/// The statics of ProceduralShatterMaterialPool and ShatteredColliderShapeCache.
#[derive(Clone, Debug, Default)]
pub struct ShatterState {
    /// ProceduralShatterMaterialPool.ShatterMasks
    pub shatter_masks: Vec<Texture>,
    /// ProceduralShatterMaterialPool.Pool, keyed by Sprite.id
    pub pool: HashMap<i32, PooledMaterial>,
    /// ShatteredColliderShapeCache.Polygons
    pub polygons: HashMap<Shard, Vec<Vec2>>,
}

impl ShatterState {
    /// ProceduralShatterMaterialPool.Get(sprite): the Random draw is Utils.PickRandom over ShatterMasks.
    pub fn material_get(&mut self, sprite: &Sprite, random: &mut Random) -> PooledMaterial {
        let count = self.shatter_masks.len();
        // PickRandom: null/empty lists return default (null texture) without a draw.
        let pick = |random: &mut Random| if count == 0 { None } else { Some(random.range_int(0, count as i32) as usize) };
        if let Some(entry) = self.pool.get_mut(&sprite.id) {
            entry.shard_id_mask = pick(random).unwrap_or(usize::MAX);
            return *entry;
        }
        let min = get_min_uv(&sprite.uv);
        let atlas = [min[0], min[1], sprite.rect_width / sprite.texture_width, sprite.rect_height / sprite.texture_height];
        let entry = PooledMaterial { atlas_transform: atlas, shard_id_mask: pick(random).unwrap_or(usize::MAX) };
        self.pool.insert(sprite.id, entry);
        entry
    }

    /// ShatteredColliderShapeCache.GetPolygon(texture, min, size)
    pub fn get_polygon(&mut self, texture: &Texture, min: u8, size: u8) -> &Vec<Vec2> {
        let key = Shard { size, min_range: min, texture_asset_id: texture.instance_id };
        self.polygons.entry(key).or_insert_with(|| {
            let mut array = Vec::new();
            let mut idx = 0;
            for i in 0..texture.height {
                for j in 0..texture.width {
                    let b = (texture.red[idx] * 255.0) as i32 as u8;
                    idx += 1;
                    if b as i32 > min as i32 && (b as i32) < min as i32 + size as i32 {
                        array.push([j as f32 / texture.width as f32 - 0.5, i as f32 / texture.height as f32 - 0.5]);
                    }
                }
            }
            let mut hull = Vec::new();
            compute_convex_hull(&mut array, &mut hull);
            hull
        })
    }

    /// ShatteredColliderShapeCache.Release()
    pub fn release(&mut self) {
        self.polygons.clear();
    }
}

// ---------------------------------------------------------------------------------------------------------------
// ShatteredObjectGenerator

/// ShatteredObjectGenerator's fields (the scene's defaults).
#[derive(Clone, Debug)]
pub struct ShatteredObjectGenerator {
    pub name: String,
    pub PartMass: f32,
    pub Sprite: Sprite,
    pub PartCount: i32,
    pub PartSizeRatio: f32,
    pub PartsLayer: i32,
    /// PartsPhysicalProperties: an index into Game.properties.
    pub PartsPhysicalProperties: usize,
    pub RandomForceIntensity: f32,
    pub RandomTorqueIntensity: f32,
    pub GenerateSoftJointsChance: f32,
    pub GenerateOnStart: bool,
    /// ConnectTo: the host's handle of the Rigidbody2D.
    pub ConnectTo: Option<usize>,
    pub ConnectToRange: f32,
    pub SoftJointStrengthRange: Vec2,
    pub Brightness: f32,
}

impl Default for ShatteredObjectGenerator {
    fn default() -> Self {
        Self {
            name: String::new(),
            PartMass: 0.05,
            Sprite: Sprite::default(),
            PartCount: 5,
            PartSizeRatio: 1.0,
            PartsLayer: 0,
            PartsPhysicalProperties: 0,
            RandomForceIntensity: 0.1,
            RandomTorqueIntensity: 0.1,
            GenerateSoftJointsChance: 0.0,
            GenerateOnStart: false,
            ConnectTo: None,
            ConnectToRange: 0.1,
            SoftJointStrengthRange: [25.0, 50.0],
            Brightness: 1.0,
        }
    }
}

/// A ShatteredObjectGenerator as its prefab serialized it, with the references Generate hands to the shards it
/// makes. `file` is the prefab's file (the references resolve against it).
#[derive(Clone, Debug, Default)]
pub struct GeneratorSource {
    pub g: ShatteredObjectGenerator,
    /// PartsPhysicalProperties
    pub properties: super::physical::PhysicalProperties,
    /// The Sprite (the shards' SpriteRenderer.sprite) and its local bounds (center, extents): a Simple SpriteRenderer's.
    pub sprite: Option<crate::assets::db::ObjRef>,
    pub sprite_bounds: ([f32; 3], [f32; 3]),
}

/// `toReplace` (Transform): position, rotation and lossyScale.
#[derive(Clone, Copy, Debug)]
pub struct ToReplace {
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub lossy_scale: [f32; 3],
}

/// `source` (Rigidbody2D): name, velocity and angularVelocity.
#[derive(Clone, Debug)]
pub struct ShatterSource {
    pub name: String,
    pub velocity: Vec2,
    pub angular_velocity: f32,
}

/// Everything one shard's GameObject is built with, in the C# order of operations.
#[derive(Clone, Debug)]
pub struct ShardSpec {
    /// new GameObject(name, SpriteRenderer, AudioSourceTimeScaleBehaviour, Optout)
    pub name: String,
    pub layer: i32,
    /// SpriteRenderer.color
    pub color: [f32; 4],
    pub sprite_id: i32,
    /// SpriteRenderer.material: the pooled material's _AtlasTransform and _ShardIDMask
    pub material: PooledMaterial,
    /// PolygonCollider2D.SetPath(0, ...)
    pub path: Vec<Vec2>,
    /// Rigidbody2D.mass
    pub mass: f32,
    /// AddForce(insideUnitCircle * RandomForceIntensity, Impulse)
    pub impulse: Vec2,
    /// AddTorque(RandomTorqueIntensity, Impulse)
    pub torque_impulse: f32,
    /// PhysicalBehaviour.Properties (SpawnSpawnParticles/HasOutline/DisplayBloodDecals/PlaySliderSound false,
    /// OverrideShotSounds/OverrideImpactSounds empty)
    pub properties: usize,
    pub to_replace: Option<ToReplace>,
    /// velocity += source.velocity; angularVelocity += source.angularVelocity
    pub add_velocity: Option<(Vec2, f32)>,
}

/// SpringJoint2D settings on shard `a` (the rest is auto-configured by the engine; `distance *= 0.95` follows).
#[derive(Clone, Copy, Debug)]
pub struct SpringSpec {
    pub connected: usize,
    pub frequency: f32,
    pub damping_ratio: f32,
    pub break_force: f32,
    pub anchor: Vec2,
    pub connected_anchor: Vec2,
}

/// The Unity-object operations Generate performs. Handles are the host's: shard handles come from `spawn_shard`.
pub trait ShatterHost {
    fn random(&mut self) -> &mut Random;
    /// The GameObject, its components and its Rigidbody2D impulses (the per-shard block of the first loop).
    fn spawn_shard(&mut self, spec: &ShardSpec) -> usize;
    /// `b`: Global.main.PhysicalObjectsInWorldByTransform lookup of ConnectTo.transform, then
    /// `spriteRenderer.bounds.ClosestPoint(toReplace.position)` or `(ConnectTo.position + toReplace.position) * 0.5f`.
    fn connect_point(&mut self, connect_to: usize, to_replace: Option<&ToReplace>) -> Vec2;
    /// Rigidbody2D.position
    fn position(&mut self, shard: usize) -> Vec2;
    /// Utils.ClosestPointFix(Rigidbody2D, point)
    fn closest_point_fix(&mut self, shard: usize, point: Vec2) -> Vec2;
    /// (Vector2)rb.transform.InverseTransformPoint(point)
    fn inverse_transform_point(&mut self, shard: usize, point: Vec2) -> Vec2;
    /// AddComponent<SpringJoint2D> on `shard`: the joint's handle. Its Create (to the ground body) happens here.
    fn add_spring_component(&mut self, shard: usize) -> usize;
    /// The joint's setters in order (connectedBody, frequency, dampingRatio, breakForce, anchor, connectedAnchor),
    /// then `distance *= 0.95f`.
    fn configure_spring(&mut self, joint: usize, spec: &SpringSpec);
    /// Rigidbody2D.worldCenterOfMass
    fn world_center_of_mass(&mut self, shard: usize) -> Vec2;
    /// AddComponent<FixedJoint2D>: connectedBody = ConnectTo, frequency, breakForce, anchor = attachedRigidbody.centerOfMass,
    /// autoConfigureConnectedAnchor = true.
    fn add_fixed_joint(&mut self, shard: usize, connect_to: usize, frequency: f32, break_force: f32);
}

/// Vector2.Distance
pub fn v2_distance(a: Vec2, b: Vec2) -> f32 {
    let (x, y) = (a[0] - b[0], a[1] - b[1]);
    (d(x) * x + d(y) * y).0.sqrt() as f32
}

/// ShatteredObjectGenerator.Generate(toReplace, source, creationBuffer): returns the number of shards created.
/// `creation_buffer[i]` receives shard i's handle (for i < its length).
pub fn generate<H: ShatterHost>(
    host: &mut H,
    st: &mut ShatterState,
    g: &ShatteredObjectGenerator,
    to_replace: Option<ToReplace>,
    source: Option<&ShatterSource>,
    mut creation_buffer: Option<&mut [Option<usize>]>,
) -> i32 {
    let material = st.material_get(&g.Sprite, host.random());
    let num = 1.0 / g.PartCount as f32;
    let y = g.PartSizeRatio * num;
    let mut array: Vec<usize> = Vec::with_capacity(g.PartCount.max(0) as usize);
    let texture = st.shatter_masks.get(material.shard_id_mask).cloned();
    let vector = [g.Sprite.rect_width / g.Sprite.pixels_per_unit, g.Sprite.rect_height / g.Sprite.pixels_per_unit];
    let mut num2 = 0;
    for i in 0..g.PartCount {
        let v2 = [num * i as f32, y];
        // GetPolygon(texture, ...) with a null texture is a NullReferenceException in the game.
        let texture = texture.as_ref().expect("NullReferenceException: _ShardIDMask");
        let polygon = st.get_polygon(texture, (v2[0] * 255.0) as i32 as u8, (v2[1] * 255.0) as i32 as u8);
        let path: Vec<Vec2> = polygon.iter().map(|p| [p[0] * vector[0], p[1] * vector[1]]).collect();
        let impulse = {
            let c = host.random().inside_unit_circle();
            [c[0] * g.RandomForceIntensity, c[1] * g.RandomForceIntensity]
        };
        let spec = ShardSpec {
            name: source.map_or_else(|| g.name.clone(), |s| s.name.clone()),
            layer: g.PartsLayer,
            color: [v2[0], v2[1], g.Brightness, 1.0],
            sprite_id: g.Sprite.id,
            material,
            path,
            mass: g.PartMass,
            impulse,
            torque_impulse: g.RandomTorqueIntensity,
            properties: g.PartsPhysicalProperties,
            to_replace,
            add_velocity: source.map(|s| (s.velocity, s.angular_velocity)),
        };
        let shard = host.spawn_shard(&spec);
        array.push(shard);
        if let Some(buf) = creation_buffer.as_deref_mut() {
            if (i as usize) < buf.len() {
                buf[i as usize] = Some(shard);
            }
        }
        num2 += 1;
    }
    let mut b = [0.0, 0.0];
    if let Some(connect_to) = g.ConnectTo {
        b = host.connect_point(connect_to, to_replace.as_ref());
    }
    if g.GenerateSoftJointsChance > f32::from_bits(1) {
        for k in 0..g.PartCount as usize {
            for l in k + 1..g.PartCount as usize {
                if !(host.random().value() > g.GenerateSoftJointsChance) {
                    let joint = host.add_spring_component(array[k]);
                    let (pk, pl) = (host.position(array[k]), host.position(array[l]));
                    let point = [(pk[0] + pl[0]) * 0.5, (pk[1] + pl[1]) * 0.5];
                    let v4 = host.closest_point_fix(array[k], point);
                    let v5 = host.closest_point_fix(array[l], point);
                    let r = host.random().value();
                    let break_force = mathf::lerp(g.SoftJointStrengthRange[0], g.SoftJointStrengthRange[1], r) * 0.2;
                    let a = host.inverse_transform_point(array[k], v4);
                    let c1 = host.random().inside_unit_circle();
                    let anchor = [a[0] + c1[0] * 0.04, a[1] + c1[1] * 0.04];
                    let ca = host.inverse_transform_point(array[l], v5);
                    let c2 = host.random().inside_unit_circle();
                    let connected_anchor = [ca[0] + c2[0] * 0.04, ca[1] + c2[1] * 0.04];
                    host.configure_spring(joint, &SpringSpec { connected: array[l], frequency: 16.0, damping_ratio: 1.0, break_force, anchor, connected_anchor });
                }
            }
            if host.random().value() < g.GenerateSoftJointsChance && g.ConnectTo.is_some() && source.is_some() {
                let com = host.world_center_of_mass(array[k]);
                if v2_distance(com, b) < g.ConnectToRange {
                    let r = host.random().value();
                    let break_force = mathf::lerp(g.SoftJointStrengthRange[0], g.SoftJointStrengthRange[1], r);
                    host.add_fixed_joint(array[k], g.ConnectTo.unwrap(), 8.0, break_force);
                }
            }
        }
    }
    num2
}

impl super::Game {
    /// ShatteredObjectGenerator.Generate(Transform toReplace, Rigidbody2D source, GameObject[] creationBuffer).
    /// `st` is ProceduralShatterMaterialPool/ShatteredColliderShapeCache's static state (keep one in the caller).
    pub fn shattered_object_generate(
        &mut self,
        st: &mut ShatterState,
        g: &ShatteredObjectGenerator,
        to_replace: Option<ToReplace>,
        source: Option<&ShatterSource>,
        creation_buffer: Option<&mut [Option<usize>]>,
    ) -> i32 {
        generate(self, st, g, to_replace, source, creation_buffer)
    }

    /// ShatteredObjectGenerator.Start
    pub fn shattered_object_generator_start(&mut self, st: &mut ShatterState, g: &ShatteredObjectGenerator, own: ToReplace) {
        if g.GenerateOnStart {
            self.shattered_object_generate(st, g, Some(own), None, None);
        }
    }
}

/// A shard's GameObject as the one-object Prefab its Instance runs on: the components `Generate` adds, in its order
/// (PhysicalBehaviour's Awake additions, ActivationNode and ContextMenuOptions, are not components of the prefab).
fn shard_prefab(name: &str, layer: i32, sprite: ObjRef) -> Prefab {
    use crate::assets::classes::{PPtr, Transform};
    use crate::assets::components::{Collider2D, ColliderShape, Rigidbody2D, SpriteRenderer, SpriteTiling};
    let script = |class: &str, fields: Vec<(String, Value)>| Component::Script { class: class.into(), execution_order: 0, enabled: true, fields: Value::Struct(fields) };
    let ast = || script("AudioSourceTimeScaleBehaviour", vec![("IsAmbience".into(), Value::Bool(false))]);
    let none = PPtr::default();
    let components = vec![
        Component::Transform(Transform { game_object: none, local_rotation: [0.0, 0.0, 0.0, 1.0], local_position: [0.0; 3], local_scale: [1.0; 3], children: Vec::new(), father: none }),
        Component::SpriteRenderer(SpriteRenderer {
            game_object: none,
            enabled: true,
            materials: Vec::new(),
            sorting_layer_id: 0,
            sorting_order: 0,
            sprite: PPtr { file: 0, path_id: sprite.path_id },
            color: [1.0; 4],
            flip_x: false,
            flip_y: false,
            draw_mode: 0,
            adaptive_mode_threshold: 0.0,
            sprite_tile_mode: 0,
            size: [1.0, 1.0],
            mask_interaction: 0,
            sprite_sort_point: 0,
        }),
        ast(),
        script("Optout", Vec::new()),
        script("ShatteredShardBehaviour", Vec::new()),
        script("DebrisComponent", Vec::new()),
        // PolygonCollider2D::Reset: density 1, no offset, no paths.
        Component::Collider2D(Collider2D {
            game_object: none,
            enabled: true,
            density: 1.0,
            material: none,
            is_trigger: false,
            used_by_effector: false,
            used_by_composite: false,
            offset: [0.0; 2],
            shape: ColliderShape::Polygon {
                tiling: SpriteTiling { border: [0.0; 4], pivot: [0.0; 2], old_size: [0.0; 2], new_size: [0.0; 2], adaptive_tiling_threshold: 0.0, draw_mode: 0, adaptive_tiling: false },
                auto_tiling: false,
                paths: Vec::new(),
            },
        }),
        // Rigidbody2D::Reset (180c22c10)
        Component::Rigidbody2D(Rigidbody2D {
            game_object: none,
            body_type: 0,
            simulated: true,
            use_full_kinematic_contacts: false,
            use_auto_mass: false,
            mass: 1.0,
            linear_drag: 0.0,
            angular_drag: 0.05,
            gravity_scale: 1.0,
            material: none,
            interpolate: 0,
            sleeping_mode: 1,
            collision_detection: 0,
            constraints: 0,
        }),
        script("PhysicalBehaviour", Vec::new()),
        ast(),
    ];
    let components = components.into_iter().enumerate().map(|(k, c)| (ObjRef { file: usize::MAX, path_id: k as i64 + 1 }, c)).collect();
    let game_object = ObjRef { file: usize::MAX, path_id: 0 };
    Prefab { objects: vec![PrefabObject { game_object, name: name.into(), layer: layer as u32, tag: 0, active: true, parent: None, children: Vec::new(), components }], index: HashMap::new(), materials: HashMap::new() }
}

/// Polygon2D::CheckConsistency (180635ab0): a point with a non-finite coordinate becomes (0, 0).
fn check_consistency(path: &mut [Vec2]) {
    for p in path {
        if !(p[0].is_finite() && p[1].is_finite()) {
            *p = [0.0, 0.0];
        }
    }
}

impl super::Game {
    /// `new GameObject(...)` and the AddComponent calls of one shard in `ShatteredObjectGenerator.Generate`, with the
    /// instance ids in the order the C# and the natives take them. Returns the shard's PhysicalBehaviour.
    fn shatter_spawn_shard(&mut self, spec: &ShardSpec) -> usize {
        use super::spawn::Clone as CloneObject;
        use crate::unity::transform::Trs;
        let (sprite, sprite_bounds) = {
            let g = self.shatter_generators.iter().find(|g| g.g.Sprite.id == spec.sprite_id).expect("a shard of a generator the game did not load");
            (g.sprite.expect("a generator loaded without its Sprite"), g.sprite_bounds)
        };
        // new GameObject(name, SpriteRenderer, AudioSourceTimeScaleBehaviour, Optout): the GameObject, its Transform, then the three.
        let game_object = self.allocate_instance_id();
        let mut ids: Vec<i32> = (0..4).map(|_| self.allocate_instance_id()).collect();
        // gameObject.layer = PartsLayer
        self.world.layers.insert(game_object, spec.layer);
        // SpriteRenderer.color/sprite/material are rendering. AddComponent<ShatteredShardBehaviour>, <DebrisComponent>, <PolygonCollider2D>.
        ids.extend((0..3).map(|_| self.allocate_instance_id()));
        let prefab = std::sync::Arc::new(shard_prefab(&spec.name, spec.layer, sprite));
        let trs = Trs { t: [0.0; 4], q: [0.0, 0.0, 0.0, 1.0], s: [1.0, 1.0, 1.0, 0.0] };
        let mut inst = super::spawn::Instance {
            prefab: prefab.clone(),
            objects: vec![CloneObject { game_object, components: ids, trs, parent: None, active: true, self_active: true, activated: true }],
            interest: vec![0],
            changed: vec![0],
            ..Default::default()
        };
        self.collider2d_awake(&mut inst, &prefab, 0, 6);
        // polygonCollider2D.SetPath(0, path): Polygon2D::SetPath, CheckConsistency, RecreateCollider. An empty path is ignored.
        let mut collider = match &prefab.objects[0].components[6].1 {
            Component::Collider2D(c) => c.clone(),
            _ => unreachable!(),
        };
        if !spec.path.is_empty() {
            let mut path = spec.path.clone();
            check_consistency(&mut path);
            let crate::assets::components::ColliderShape::Polygon { paths, .. } = &mut collider.shape else { unreachable!() };
            *paths = vec![path];
            let mut shard = (*prefab).clone();
            shard.objects[0].components[6].1 = Component::Collider2D(collider.clone());
            inst.prefab = std::sync::Arc::new(shard);
            let prefab = inst.prefab.clone();
            self.collider2d_recreate(&mut inst, &prefab, 0, 6);
        }
        // AddComponent<Rigidbody2D>: its AwakeFromLoad makes the body; the collider's shapes stay on the ground body until SyncTransforms.
        let prefab = inst.prefab.clone();
        let rb_id = self.allocate_instance_id();
        inst.objects[0].components.push(rb_id);
        let Component::Rigidbody2D(rb_data) = &prefab.objects[0].components[7].1 else { unreachable!() };
        self.rigidbody2d_awake(&mut inst, &prefab, 0, 7, rb_data);
        let body = inst.bodies[&0];
        let rigidbody = super::Rigidbody {
            body,
            bodyType: 0,
            collisionDetectionMode: 0,
            interpolation: 0,
            movement: super::MovementState::default(),
            detached: None,
            component: rb_id,
            simulated: true,
            detached_props: [0.0, 0.05, 1.0],
            constraints: 0,
        };
        // rigidbody2D.mass = PartMass; AddForce(..., Impulse); AddTorque(..., Impulse)
        self.body_set_mass(body, spec.mass);
        rigidbody.add_force_impulse(&mut self.world, spec.impulse);
        rigidbody.add_torque_impulse(&mut self.world, spec.torque_impulse);
        // AddComponent<PhysicalBehaviour>: its fields at their initial values, then Awake.
        let physical_id = self.allocate_instance_id();
        inst.objects[0].components.push(physical_id);
        let p = self.physicals.len();
        let mut ph = super::Physical { name: spec.name.clone(), id: physical_id, ..Default::default() };
        ph.enabled = true;
        ph.active = true;
        ph.isStatic = false;
        ph.colliders = vec![inst.objects[0].components[6]];
        ph.collider_defs = vec![collider];
        ph.all_colliders = ph.colliders.clone();
        ph.transform_id = inst.objects[0].components[0];
        ph.rigidbody = rigidbody.clone();
        ph.transform = super::link::pose(&inst, 0);
        // The AudioSourceTimeScaleBehaviour the GameObject was made with.
        ph.audio_time_scale = true;
        ph.sprite_bounds = Some(sprite_bounds);
        ph.sprite = Some((sprite, spec.color, 0));
        ph.sprite_draw = ([false, false], 0, [1.0, 1.0], 0.0, false);
        let address = self.heap_base.unwrap_or(super::link::MODELLED_HEAP_BASE) + ((p as u64) << 11);
        ph.hash_code = ((address >> 3) as u32).wrapping_mul(0x9e37_79b1) as i32;
        self.physicals.push(ph);
        self.rigidbody_hierarchy_changed = true;
        self.physical_awake(p);
        // physicalBehaviour.Properties = PartsPhysicalProperties; SpawnSpawnParticles, HasOutline, DisplayBloodDecals, PlaySliderSound = false.
        let ph = &mut self.physicals[p];
        ph.Properties = spec.properties;
        ph.SpawnSpawnParticles = false;
        ph.DisplayBloodDecals = false;
        ph.PlaySliderSound = false;
        // AddComponent<AudioSourceTimeScaleBehaviour>() (a second one); OverrideShotSounds/OverrideImpactSounds are empty (0).
        let second = self.allocate_instance_id();
        inst.objects[0].components.push(second);
        // toReplace: SetPositionAndRotation(position, rotation), localScale = lossyScale.
        if let Some(t) = &spec.to_replace {
            inst.set_world_position(0, t.position);
            inst.set_world_rotation(0, t.rotation);
            inst.set_local_scale(0, t.lossy_scale);
            self.physicals[p].transform = super::link::pose(&inst, 0);
        }
        // source: rigidbody.velocity += source.velocity; angularVelocity += source.angularVelocity.
        if let Some((velocity, angular)) = spec.add_velocity {
            let v = rigidbody.velocity(&self.world);
            rigidbody.set_velocity(&mut self.world, [v[0] + velocity[0], v[1] + velocity[1]]);
            let w = rigidbody.angular_velocity(&self.world);
            rigidbody.set_angular_velocity(&mut self.world, w + angular);
        }
        self.unsynced.push(inst);
        self.pending_starts.push(super::misc::PendingStart::Physical(p));
        p
    }

    /// Rigidbody2D.position: AutoSyncTransforms, then the b2 body's origin.
    fn rigidbody_position(&mut self, p: usize) -> Vec2 {
        self.sync_pending();
        let xf = self.world.body(self.physicals[p].rigidbody.body).xf;
        [xf.p.x, xf.p.y]
    }
}

impl ShatterHost for super::Game {
    fn random(&mut self) -> &mut Random {
        &mut self.random
    }
    fn spawn_shard(&mut self, spec: &ShardSpec) -> usize {
        self.shatter_spawn_shard(spec)
    }
    fn connect_point(&mut self, connect_to: usize, to_replace: Option<&ToReplace>) -> Vec2 {
        let t = to_replace.expect("NullReferenceException: ShatteredObjectGenerator.Generate without toReplace");
        // PhysicalObjectsInWorldByTransform.TryGetValue(ConnectTo.transform): in the world since its Start, until destroyed.
        if self.physicals[connect_to].in_world && !self.physicals[connect_to].destroyed {
            let (c, e) = self.sprite_world_bounds(connect_to);
            let (closest, _) = crate::unity::transform::bounds_closest_point(t.position, c, e);
            return [closest[0], closest[1]];
        }
        let at = self.rigidbody_position(connect_to);
        [(at[0] + t.position[0]) * 0.5, (at[1] + t.position[1]) * 0.5]
    }
    fn position(&mut self, shard: usize) -> Vec2 {
        self.rigidbody_position(shard)
    }
    fn closest_point_fix(&mut self, shard: usize, point: Vec2) -> Vec2 {
        // GetAttachedColliders, then Collider2D.ClosestPoint (AutoSyncTransforms) on each.
        self.sync_pending();
        let body = self.physicals[shard].rigidbody.body;
        let mut best = f32::MAX;
        let mut result = point;
        for c in self.physicals[shard].all_colliders.clone() {
            if self.world.collider_body(c) != Some(body) {
                continue;
            }
            let v = self.world.collider_closest_point(c, crate::b2::math::Vec2::new(point[0], point[1]));
            let sqr = mathf::sqr_magnitude([v.x - point[0], v.y - point[1]]);
            if sqr < best {
                best = sqr;
                result = [v.x, v.y];
            }
        }
        result
    }
    fn inverse_transform_point(&mut self, shard: usize, point: Vec2) -> Vec2 {
        let r = super::Game::inverse_transform_point(self, shard, [point[0], point[1], 0.0]);
        [r[0], r[1]]
    }
    fn add_spring_component(&mut self, shard: usize) -> usize {
        self.allocate_instance_id();
        self.add_spring_joint_component(shard)
    }
    fn configure_spring(&mut self, joint: usize, spec: &SpringSpec) {
        self.fixed_joint_set_connected_body(joint, Some(super::BodyOwner::Physical(spec.connected)));
        self.fixed_joint_set_frequency(joint, spec.frequency);
        self.fixed_joint_set_damping_ratio(joint, spec.damping_ratio);
        self.fixed_joint_set_break_force(joint, spec.break_force);
        self.fixed_joint_set_anchor(joint, spec.anchor);
        self.fixed_joint_set_connected_anchor(joint, spec.connected_anchor);
        // springJoint2D.distance *= 0.95f
        let distance = self.spring_joint_distance(joint) * 0.95;
        self.spring_joint_set_distance(joint, distance);
    }
    fn world_center_of_mass(&mut self, shard: usize) -> Vec2 {
        let c = self.world.body(self.physicals[shard].rigidbody.body).sweep.c;
        [c.x, c.y]
    }
    fn add_fixed_joint(&mut self, shard: usize, connect_to: usize, frequency: f32, break_force: f32) {
        self.allocate_instance_id();
        let f = self.add_fixed_joint_component(shard);
        self.fixed_joint_set_connected_body(f, Some(super::BodyOwner::Physical(connect_to)));
        self.fixed_joint_set_frequency(f, frequency);
        self.fixed_joint_set_break_force(f, break_force);
        // fixedJoint2D.anchor = fixedJoint2D.attachedRigidbody.centerOfMass
        let c = self.world.body(self.physicals[shard].rigidbody.body).sweep.local_center;
        self.fixed_joint_set_anchor(f, [c.x, c.y]);
        self.fixed_joint_set_auto_configure(f, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bits(v: Vec2) -> [u32; 2] {
        [v[0].to_bits(), v[1].to_bits()]
    }

    #[test]
    fn sort_descending_by_x_all_sizes() {
        for n in [0usize, 1, 2, 3, 5, 16, 17, 40, 200] {
            let mut pts: Vec<Vec2> = (0..n).map(|i| [((i * 7919) % 31) as f32, i as f32]).collect();
            array_sort(&mut pts, |a, b| if a[0] > b[0] { -1 } else { 1 });
            assert!(pts.windows(2).all(|w| w[0][0] >= w[1][0]), "n={n}");
        }
    }

    #[test]
    fn hull_of_square() {
        // Points of a 3x3 grid; the monotone chain over x-descending input.
        let mut pts: Vec<Vec2> = Vec::new();
        for i in 0..3 {
            for j in 0..3 {
                pts.push([j as f32, i as f32]);
            }
        }
        let mut hull = Vec::new();
        let n = compute_convex_hull(&mut pts, &mut hull);
        assert_eq!(n as usize, hull.len());
        for corner in [[0.0, 0.0], [2.0, 0.0], [0.0, 2.0], [2.0, 2.0]] {
            assert!(hull.contains(&corner), "{corner:?} in {hull:?}");
        }
        assert!(!hull.contains(&[1.0, 1.0]));
    }

    #[test]
    fn get_min_first_smallest() {
        assert_eq!(get_min_uv(&[[0.5, 0.5], [0.25, 0.5], [0.25, 0.5], [0.9, 0.0]]), [0.25, 0.5]);
        assert_eq!(get_min_uv(&[]), [0.0, 0.0]);
    }

    fn mask() -> Texture {
        // 4x4: red bytes 0..=15 * 16
        Texture { instance_id: 7, width: 4, height: 4, red: (0..16).map(|i| (i * 16) as f32 / 255.0).collect() }
    }

    #[test]
    fn polygon_range_and_cache() {
        let mut st = ShatterState::default();
        let t = mask();
        // bytes strictly between 16 and 16+64: 32, 48, 64 (index 2,3,4)
        let poly = st.get_polygon(&t, 16, 64).clone();
        assert!(!poly.is_empty());
        assert!(poly.iter().all(|p| p[0] >= -0.5 && p[0] < 0.5));
        assert_eq!(st.polygons.len(), 1);
        let again = st.get_polygon(&t, 16, 64).clone();
        assert_eq!(poly, again);
        st.release();
        assert!(st.polygons.is_empty());
    }

    fn sprite() -> Sprite {
        Sprite { id: 1, rect_width: 32.0, rect_height: 16.0, pixels_per_unit: 16.0, uv: vec![[0.5, 0.5], [0.25, 0.25]], texture_width: 64.0, texture_height: 64.0 }
    }

    #[test]
    fn material_pool_draws_each_time() {
        let mut st = ShatterState { shatter_masks: vec![mask(), mask(), mask()], ..Default::default() };
        let mut rng = Random::new(5);
        let mut mirror = Random::new(5);
        let m1 = st.material_get(&sprite(), &mut rng);
        assert_eq!(m1.atlas_transform, [0.25, 0.25, 0.5, 0.25]);
        assert_eq!(m1.shard_id_mask, mirror.range_int(0, 3) as usize);
        let m2 = st.material_get(&sprite(), &mut rng);
        assert_eq!(m2.atlas_transform, m1.atlas_transform);
        assert_eq!(m2.shard_id_mask, mirror.range_int(0, 3) as usize);
        assert_eq!(rng, mirror);
    }

    #[derive(Default)]
    struct Mock {
        rng: Option<Random>,
        specs: Vec<ShardSpec>,
        springs: Vec<(usize, SpringSpec)>,
        fixed: Vec<(usize, usize, f32)>,
    }

    impl ShatterHost for Mock {
        fn random(&mut self) -> &mut Random {
            self.rng.as_mut().unwrap()
        }
        fn spawn_shard(&mut self, spec: &ShardSpec) -> usize {
            self.specs.push(spec.clone());
            self.specs.len() - 1
        }
        fn connect_point(&mut self, _c: usize, _t: Option<&ToReplace>) -> Vec2 {
            [0.0, 0.0]
        }
        fn position(&mut self, shard: usize) -> Vec2 {
            [shard as f32, 0.0]
        }
        fn closest_point_fix(&mut self, _s: usize, p: Vec2) -> Vec2 {
            p
        }
        fn inverse_transform_point(&mut self, _s: usize, p: Vec2) -> Vec2 {
            p
        }
        fn add_spring_component(&mut self, shard: usize) -> usize {
            shard
        }
        fn configure_spring(&mut self, joint: usize, spec: &SpringSpec) {
            self.springs.push((joint, *spec));
        }
        fn world_center_of_mass(&mut self, _s: usize) -> Vec2 {
            [0.0, 0.0]
        }
        fn add_fixed_joint(&mut self, shard: usize, c: usize, _f: f32, bf: f32) {
            self.fixed.push((shard, c, bf));
        }
    }

    fn setup() -> (ShatterState, ShatteredObjectGenerator) {
        let st = ShatterState { shatter_masks: vec![mask(), mask()], ..Default::default() };
        let g = ShatteredObjectGenerator { name: "gen".into(), Sprite: sprite(), PartCount: 4, ..Default::default() };
        (st, g)
    }

    #[test]
    fn generate_draw_order_without_joints() {
        let (mut st, g) = setup();
        let mut host = Mock { rng: Some(Random::new(11)), ..Default::default() };
        let mut buf = [None; 3];
        let n = generate(&mut host, &mut st, &g, None, None, Some(&mut buf));
        assert_eq!(n, 4);
        assert_eq!(buf, [Some(0), Some(1), Some(2)]);
        let mut m = Random::new(11);
        m.range_int(0, 2);
        for spec in &host.specs {
            let c = m.inside_unit_circle();
            assert_eq!(bits(spec.impulse), bits([c[0] * 0.1, c[1] * 0.1]));
            assert_eq!(spec.name, "gen");
            assert_eq!(spec.color[2], 1.0);
        }
        assert_eq!(host.rng.unwrap(), m);
        assert_eq!(host.specs[2].color[..2], [0.5, 0.25]);
        assert!(host.springs.is_empty());
    }

    #[test]
    fn generate_soft_joints_draw_order() {
        let (mut st, mut g) = setup();
        g.GenerateSoftJointsChance = 1.0;
        g.ConnectTo = Some(9);
        let src = ShatterSource { name: "src".into(), velocity: [1.0, 2.0], angular_velocity: 3.0 };
        let to = ToReplace { position: [0.0; 3], rotation: [0.0, 0.0, 0.0, 1.0], lossy_scale: [1.0; 3] };
        let mut host = Mock { rng: Some(Random::new(3)), ..Default::default() };
        generate(&mut host, &mut st, &g, Some(to), Some(&src), None);
        assert_eq!(host.specs[0].name, "src");
        assert_eq!(host.specs[0].add_velocity, Some(([1.0, 2.0], 3.0)));
        let mut m = Random::new(3);
        m.range_int(0, 2);
        for _ in 0..4 {
            m.inside_unit_circle();
        }
        let mut springs = 0;
        for k in 0..4 {
            for _l in k + 1..4 {
                assert!(!(m.value() > 1.0));
                let r = m.value();
                let bf = mathf::lerp(25.0, 50.0, r) * 0.2;
                assert_eq!(host.springs[springs].1.break_force.to_bits(), bf.to_bits());
                m.inside_unit_circle();
                m.inside_unit_circle();
                springs += 1;
            }
            // value < 1.0 almost surely; the mock's centre of mass (0,0) is within range of b (0,0)
            let v = m.value();
            if v < 1.0 {
                m.value();
            }
        }
        assert_eq!(springs, 6);
        assert_eq!(host.fixed.len(), 4);
        assert_eq!(host.rng.unwrap(), m);
    }
}
