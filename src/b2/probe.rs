//! Bit comparisons of the b2 rewrite against UnityPlayer.dll through NativeProbe's raw-call mode.

use super::broadphase::*;
use super::collide::*;
use super::distance::*;
use super::math::{Transform, Vec2};
use super::shape::*;
use super::solver::*;
use super::toi::*;
use serde::{Deserialize, Serialize};

const SET: u32 = 0xbb4b90;
const COMPUTE_MASS: u32 = 0xbb35a0;
const COLLIDE_CIRCLES: u32 = 0xbaeff0;
const COLLIDE_POLYGON_AND_CIRCLE: u32 = 0xbaf500;
const COLLIDE_POLYGONS: u32 = 0xbaf8f0;
const COLLIDE_RADIAL_POLYGONS: u32 = 0xbaffe0;
const WORLD_MANIFOLD: u32 = 0xbadb50;
const INITIALIZE_VELOCITY: u32 = 0xbbe5f0;
const WARM_START: u32 = 0xbc0780;
const SOLVE_VELOCITY: u32 = 0xbbf920;
const SOLVE_POSITION: u32 = 0xbbee60;
const SOLVE_TOI_POSITION: u32 = 0xbbf3a0;
const DISTANCE: u32 = 0xbb08e0;
const TIME_OF_IMPACT: u32 = 0xbb1c40;
const CREATE_PROXY: u32 = 0xbac6e0;
const DESTROY_PROXY: u32 = 0xbac7d0;
const MOVE_PROXY: u32 = 0xbae4a0;
const QUERY_BROAD_PHASE: u32 = 0xbb72b0;
/// b2_velocityThreshold, b2_maxLinearCorrection, b2_baumgarte and b2_toiBaugarte in .data.
const SOLVER_GLOBALS: [[u32; 2]; 4] = [
    [0x1999c2c, VELOCITY_THRESHOLD.to_bits()],
    [0x1999c30, MAX_LINEAR_CORRECTION.to_bits()],
    [0x1999c40, BAUMGARTE.to_bits()],
    [0x1999c44, TOI_BAUMGARTE.to_bits()],
];

#[derive(Serialize)]
struct Batch {
    globals: Vec<[u32; 2]>,
    calls: Vec<Call>,
}

#[derive(Serialize)]
struct Call {
    rva: u32,
    buffers: Vec<String>,
    args: Vec<String>,
    returns_float: bool,
    links: Vec<[usize; 4]>,
}

#[derive(Deserialize)]
struct Reply {
    buffers: Vec<String>,
    ret: i64,
    #[serde(default)]
    ret_f: u32,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap()).collect()
}

fn put(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_vec(bytes: &mut [u8], offset: usize, v: Vec2) {
    put(bytes, offset, v.x.to_bits());
    put(bytes, offset + 4, v.y.to_bits());
}

fn get(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn get_vec(bytes: &[u8], offset: usize) -> Vec2 {
    Vec2::new(f32::from_bits(get(bytes, offset)), f32::from_bits(get(bytes, offset + 4)))
}

fn polygon_bytes(polygon: &Polygon) -> Vec<u8> {
    let mut b = vec![0; 160];
    put(&mut b, 8, 2);
    put(&mut b, 0xc, polygon.radius.to_bits());
    put_vec(&mut b, 0x10, polygon.centroid);
    for i in 0..polygon.count {
        put_vec(&mut b, 0x18 + 8 * i, polygon.vertices[i]);
        put_vec(&mut b, 0x58 + 8 * i, polygon.normals[i]);
    }
    put(&mut b, 0x98, polygon.count as u32);
    b
}

fn read_polygon(b: &[u8]) -> Polygon {
    let count = get(b, 0x98) as usize;
    let mut polygon = Polygon { radius: f32::from_bits(get(b, 0xc)), centroid: get_vec(b, 0x10), count, ..Polygon::default() };
    for i in 0..count.min(MAX_POLYGON_VERTICES) {
        polygon.vertices[i] = get_vec(b, 0x18 + 8 * i);
        polygon.normals[i] = get_vec(b, 0x58 + 8 * i);
    }
    polygon
}

fn circle_bytes(circle: &Circle) -> Vec<u8> {
    let mut b = vec![0; 24];
    put(&mut b, 0xc, circle.radius.to_bits());
    put_vec(&mut b, 0x10, circle.p);
    b
}

fn transform_bytes(xf: &Transform) -> Vec<u8> {
    let mut b = vec![0; 16];
    put_vec(&mut b, 0, xf.p);
    put(&mut b, 8, xf.q.s.to_bits());
    put(&mut b, 12, xf.q.c.to_bits());
    b
}

fn manifold_bytes(manifold: &Manifold) -> Vec<u8> {
    let mut b = vec![0; 72];
    for (i, point) in manifold.points.iter().enumerate() {
        put_vec(&mut b, 20 * i, point.local_point);
        put(&mut b, 20 * i + 8, point.normal_impulse.to_bits());
        put(&mut b, 20 * i + 12, point.tangent_impulse.to_bits());
        put(&mut b, 20 * i + 16, point.id.key());
    }
    put_vec(&mut b, 0x28, manifold.local_normal);
    put_vec(&mut b, 0x30, manifold.local_point);
    put(&mut b, 0x38, manifold.kind as u32);
    put(&mut b, 0x3c, manifold.point_count as u32);
    b
}

/// The fields a manifold consumer reads: nothing beyond the point count.
fn manifold_bits(b: &[u8]) -> Vec<u32> {
    let count = get(b, 0x3c) as usize;
    let mut bits = vec![count as u32];
    if count > 0 {
        bits.extend([get(b, 0x38), get(b, 0x28), get(b, 0x2c), get(b, 0x30), get(b, 0x34)]);
        for i in 0..count.min(2) {
            bits.extend([get(b, 20 * i), get(b, 20 * i + 4), get(b, 20 * i + 16)]);
        }
    }
    bits
}

fn polygon_bits(polygon: &Polygon) -> Vec<u32> {
    let mut bits = vec![polygon.count as u32, polygon.radius.to_bits(), polygon.centroid.x.to_bits(), polygon.centroid.y.to_bits()];
    for i in 0..polygon.count.min(MAX_POLYGON_VERTICES) {
        bits.extend([polygon.vertices[i].x, polygon.vertices[i].y, polygon.normals[i].x, polygon.normals[i].y].map(f32::to_bits));
    }
    bits
}

struct Random(u32);

impl Random {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
        self.0 >> 8
    }
    /// Uniform in [lo, hi), with full mantissa noise so roundings are exercised.
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * (self.next() as f32 / (1u32 << 24) as f32)
    }
    fn vec(&mut self, extent: f32) -> Vec2 {
        Vec2::new(self.range(-extent, extent), self.range(-extent, extent))
    }
}

fn random_polygon(random: &mut Random, index: usize) -> Polygon {
    let radius = [POLYGON_RADIUS, POLYGON_RADIUS + 0.1, POLYGON_RADIUS][index % 3];
    if index % 4 == 0 {
        let size = Vec2::new(random.range(0.05, 1.5), random.range(0.05, 1.5));
        return Polygon::from_points(&box_corners(random.vec(0.3), size), radius);
    }
    let count = 3 + random.next() as usize % 6;
    let points: Vec<Vec2> = (0..count).map(|_| random.vec(0.8)).collect();
    Polygon::from_points(&points, radius)
}

fn random_transform(random: &mut Random, extent: f32) -> Transform {
    Transform::new(random.vec(extent), random.range(-std::f32::consts::PI, std::f32::consts::PI))
}

fn call(rva: u32, buffers: Vec<Vec<u8>>, args: &[&str]) -> Call {
    Call { rva, buffers: buffers.iter().map(|b| hex(b)).collect(), args: args.iter().map(|a| a.to_string()).collect(), returns_float: false, links: vec![] }
}

fn report(name: &str, mismatches: usize, total: usize, first: Option<String>) -> Result<(), String> {
    if mismatches != 0 {
        return Err(format!("Native {name} comparison failed: {mismatches}/{total} differ; first: {}", first.unwrap_or_default()));
    }
    println!("NATIVE {name} PASSED: {total} cases match UnityPlayer.dll bit-for-bit.");
    Ok(())
}

fn compare_shapes(executable: &std::path::Path) -> Result<(), String> {
    let mut random = Random(0x5eed_0001);
    let mut point_sets = Vec::new();
    for i in 0..4096 {
        let count = 3 + random.next() as usize % 6;
        let mut points: Vec<Vec2> = (0..count).map(|_| random.vec(1.0)).collect();
        match i % 5 {
            // Duplicates within the weld tolerance and exactly collinear runs exercise Unity's extra passes.
            1 => points[1] = points[0] + Vec2::new(0.001, 0.0),
            2 => points.push(points[0] * 0.5 + points[1] * 0.5),
            3 => points = box_corners(random.vec(0.5), Vec2::new(random.range(0.01, 2.0), random.range(0.01, 2.0))).to_vec(),
            _ => {}
        }
        point_sets.push(points);
    }
    let mut calls = Vec::new();
    for points in &point_sets {
        let mut b = Vec::new();
        for p in points {
            b.extend(p.x.to_le_bytes());
            b.extend(p.y.to_le_bytes());
        }
        calls.push(call(SET, vec![vec![0; 160], b], &["b0", "b1", &format!("i{}", points.len())]));
    }
    let polygons: Vec<Polygon> = point_sets.iter().map(|p| Polygon::from_points(p, 0.0)).collect();
    for polygon in &polygons {
        let density = 1.25f32;
        calls.push(call(COMPUTE_MASS, vec![polygon_bytes(polygon), vec![0; 20]], &["b0", "b1", &format!("f{}", density.to_bits())]));
    }
    let count = calls.len();
    let replies: Vec<Reply> = crate::motor::run_probe(executable, &Batch { globals: vec![], calls }, count)?;
    let (mut set_bad, mut set_first) = (0, None);
    for (i, reply) in replies[..point_sets.len()].iter().enumerate() {
        let native = read_polygon(&unhex(&reply.buffers[0]));
        if polygon_bits(&native) != polygon_bits(&polygons[i]) {
            set_bad += 1;
            set_first.get_or_insert_with(|| format!("points={:?} native={native:?} rust={:?}", point_sets[i], polygons[i]));
        }
    }
    report("POLYGON SET", set_bad, point_sets.len(), set_first)?;
    let (mut mass_bad, mut mass_first) = (0, None);
    for (i, reply) in replies[point_sets.len()..].iter().enumerate() {
        let b = unhex(&reply.buffers[1]);
        let m = polygons[i].compute_mass(1.25);
        let rust = [m.mass, m.center.x, m.center.y, m.inertia, m.area].map(f32::to_bits);
        let native = [0, 4, 8, 12, 16].map(|o| get(&b, o));
        if rust != native {
            mass_bad += 1;
            mass_first.get_or_insert_with(|| format!("polygon={:?} native={native:x?} rust={rust:x?}", polygons[i]));
        }
    }
    report("POLYGON MASS", mass_bad, polygons.len(), mass_first)
}

fn compare_manifolds(executable: &std::path::Path) -> Result<(), String> {
    let mut random = Random(0x5eed_0002);
    let mut calls = Vec::new();
    let mut expected = Vec::new();
    let mut names = Vec::new();
    for i in 0..8192 {
        let xf_a = random_transform(&mut random, 0.4);
        let xf_b = random_transform(&mut random, 0.4);
        let mut manifold = Manifold::default();
        match i % 4 {
            0 | 1 => {
                let a = random_polygon(&mut random, i);
                let b = random_polygon(&mut random, i / 4);
                let rva = if i % 4 == 0 {
                    collide_polygons(&mut manifold, &a, &xf_a, &b, &xf_b);
                    COLLIDE_POLYGONS
                } else {
                    collide_radial_polygons(&mut manifold, &a, &xf_a, &b, &xf_b);
                    COLLIDE_RADIAL_POLYGONS
                };
                calls.push(call(rva, vec![vec![0; 72], polygon_bytes(&a), transform_bytes(&xf_a), polygon_bytes(&b), transform_bytes(&xf_b)], &["b0", "b1", "b2", "b3", "b4"]));
                names.push(if rva == COLLIDE_POLYGONS { "polygons" } else { "radial polygons" });
            }
            2 => {
                let a = random_polygon(&mut random, i);
                let b = Circle { radius: random.range(0.02, 0.6), p: random.vec(0.3) };
                collide_polygon_and_circle(&mut manifold, &a, &xf_a, &b, &xf_b);
                calls.push(call(COLLIDE_POLYGON_AND_CIRCLE, vec![vec![0; 72], polygon_bytes(&a), transform_bytes(&xf_a), circle_bytes(&b), transform_bytes(&xf_b)], &["b0", "b1", "b2", "b3", "b4", "i0"]));
                names.push("polygon-circle");
            }
            _ => {
                let a = Circle { radius: random.range(0.02, 0.6), p: random.vec(0.3) };
                let b = Circle { radius: random.range(0.02, 0.6), p: random.vec(0.3) };
                collide_circles(&mut manifold, &a, &xf_a, &b, &xf_b);
                calls.push(call(COLLIDE_CIRCLES, vec![vec![0; 72], circle_bytes(&a), transform_bytes(&xf_a), circle_bytes(&b), transform_bytes(&xf_b)], &["b0", "b1", "b2", "b3", "b4"]));
                names.push("circles");
            }
        }
        expected.push((manifold, xf_a, xf_b));
    }
    let count = calls.len();
    let replies: Vec<Reply> = crate::motor::run_probe(executable, &Batch { globals: vec![], calls }, count)?;
    let (mut bad, mut first, mut touching) = (0, None, 0);
    let mut world_calls = Vec::new();
    let mut world_expected = Vec::new();
    for (i, reply) in replies.iter().enumerate() {
        let native = unhex(&reply.buffers[0]);
        let rust = manifold_bytes(&expected[i].0);
        if manifold_bits(&native) != manifold_bits(&rust) {
            bad += 1;
            first.get_or_insert_with(|| format!("{} case {i}: native={:x?} rust={:x?}", names[i], manifold_bits(&native), manifold_bits(&rust)));
        }
        if expected[i].0.point_count > 0 {
            touching += 1;
            let (manifold, xf_a, xf_b) = &expected[i];
            let (ra, rb) = (random.range(0.0, 0.2), random.range(0.0, 0.2));
            world_expected.push(WorldManifold::new(manifold, xf_a, ra, xf_b, rb));
            world_calls.push(call(WORLD_MANIFOLD, vec![vec![0; 32], rust, transform_bytes(xf_a), transform_bytes(xf_b)], &["b0", "b1", "b2", &format!("f{}", ra.to_bits()), "b3", &format!("f{}", rb.to_bits())]));
        }
    }
    report("MANIFOLD", bad, replies.len(), first)?;
    println!("  ({touching} of them touching)");
    let count = world_calls.len();
    let replies: Vec<Reply> = crate::motor::run_probe(executable, &Batch { globals: vec![], calls: world_calls }, count)?;
    let (mut bad, mut first) = (0, None);
    for (i, reply) in replies.iter().enumerate() {
        let b = unhex(&reply.buffers[0]);
        let w = &world_expected[i];
        let rust = [w.normal.x, w.normal.y, w.points[0].x, w.points[0].y, w.points[1].x, w.points[1].y, w.separations[0], w.separations[1]].map(f32::to_bits);
        let native: Vec<u32> = (0..8).map(|k| get(&b, 4 * k)).collect();
        if native != rust {
            bad += 1;
            first.get_or_insert_with(|| format!("case {i}: native={native:x?} rust={rust:x?}"));
        }
    }
    report("WORLD MANIFOLD", bad, replies.len(), first)
}

fn velocity_constraint_bytes(vc: &VelocityConstraint) -> Vec<u8> {
    let mut b = vec![0; 0xac];
    for (j, p) in vc.points.iter().enumerate() {
        let o = 36 * j;
        put_vec(&mut b, o, p.r_a);
        put_vec(&mut b, o + 8, p.r_b);
        for (k, v) in [p.normal_impulse, p.tangent_impulse, p.normal_mass, p.tangent_mass, p.velocity_bias].iter().enumerate() {
            put(&mut b, o + 16 + 4 * k, v.to_bits());
        }
    }
    put_vec(&mut b, 0x48, vc.normal);
    for (o, m) in [(0x50, vc.normal_mass), (0x60, vc.k)] {
        put_vec(&mut b, o, m.ex);
        put_vec(&mut b, o + 8, m.ey);
    }
    put(&mut b, 0x70, vc.index_a as u32);
    put(&mut b, 0x74, vc.index_b as u32);
    put_vec(&mut b, 0x78, vc.axis_a);
    put_vec(&mut b, 0x80, vc.axis_b);
    for (k, v) in [vc.inv_mass_a, vc.inv_mass_b, vc.inv_i_a, vc.inv_i_b, vc.friction, vc.restitution, vc.tangent_speed].iter().enumerate() {
        put(&mut b, 0x88 + 4 * k, v.to_bits());
    }
    put(&mut b, 0xa4, vc.point_count as u32);
    put(&mut b, 0xa8, vc.contact_index as u32);
    b
}

fn position_constraint_bytes(pc: &PositionConstraint) -> Vec<u8> {
    let mut b = vec![0; 0x68];
    put_vec(&mut b, 0, pc.local_points[0]);
    put_vec(&mut b, 8, pc.local_points[1]);
    put_vec(&mut b, 0x10, pc.local_normal);
    put_vec(&mut b, 0x18, pc.local_point);
    put(&mut b, 0x20, pc.index_a as u32);
    put(&mut b, 0x24, pc.index_b as u32);
    put_vec(&mut b, 0x28, pc.axis_a);
    put_vec(&mut b, 0x30, pc.axis_b);
    put(&mut b, 0x38, pc.inv_mass_a.to_bits());
    put(&mut b, 0x3c, pc.inv_mass_b.to_bits());
    put_vec(&mut b, 0x40, pc.local_center_a);
    put_vec(&mut b, 0x48, pc.local_center_b);
    put(&mut b, 0x50, pc.inv_i_a.to_bits());
    put(&mut b, 0x54, pc.inv_i_b.to_bits());
    put(&mut b, 0x58, pc.kind as u32);
    put(&mut b, 0x5c, pc.radius_a.to_bits());
    put(&mut b, 0x60, pc.radius_b.to_bits());
    put(&mut b, 0x64, pc.point_count as u32);
    b
}

fn island_bytes(positions: &[Position], velocities: &[Velocity]) -> (Vec<u8>, Vec<u8>) {
    let mut p = vec![0; 12 * positions.len()];
    let mut v = vec![0; 12 * velocities.len()];
    for (i, (x, y)) in positions.iter().zip(velocities).enumerate() {
        put_vec(&mut p, 12 * i, x.c);
        put(&mut p, 12 * i + 8, x.a.to_bits());
        put_vec(&mut v, 12 * i, y.v);
        put(&mut v, 12 * i + 8, y.w.to_bits());
    }
    (p, v)
}

/// The island state a solver pass leaves: positions, velocities, position and velocity constraints.
fn solver_state(solver: &ContactSolver, positions: &[Position], velocities: &[Velocity]) -> [Vec<u8>; 4] {
    let (p, v) = island_bytes(positions, velocities);
    let pcs = solver.position_constraints.iter().flat_map(position_constraint_bytes).collect();
    let vcs = solver.velocity_constraints.iter().flat_map(velocity_constraint_bytes).collect();
    [p, v, pcs, vcs]
}

/// A b2ContactSolver call over the whole island state: buffers 0 solver, 1 positions, 2 velocities,
/// 3 position constraints, 4 velocity constraints, 5 contact pointers, 6 contacts (manifold at 0x80).
fn solver_call(rva: u32, solver: &ContactSolver, contacts: &[SolverContact], positions: &[Position], velocities: &[Velocity], args: &[&str]) -> Call {
    let mut s = vec![0; 0x58];
    let step = solver.step;
    let fields = [step.dt.to_bits(), step.inv_dt.to_bits(), step.dt_ratio.to_bits(), step.velocity_iterations as u32, step.position_iterations as u32, step.warm_starting as u32];
    for (k, v) in fields.iter().enumerate() {
        put(&mut s, 8 + 4 * k, *v);
    }
    put(&mut s, 0x50, contacts.len() as u32);
    let [p, v, pcs, vcs] = solver_state(solver, positions, velocities);
    let mut records = vec![0; 312 * contacts.len()];
    for (i, c) in contacts.iter().enumerate() {
        records[312 * i + 0x80..312 * i + 0x80 + 72].copy_from_slice(&manifold_bytes(&c.manifold));
    }
    let mut call = call(rva, vec![s, p, v, pcs, vcs, vec![0; 8 * contacts.len()], records], args);
    call.links = vec![[0, 0x20, 1, 0], [0, 0x28, 2, 0], [0, 0x38, 3, 0], [0, 0x40, 4, 0], [0, 0x48, 5, 0]];
    call.links.extend((0..contacts.len()).map(|i| [5, 8 * i, 6, 312 * i]));
    call
}

/// A small island of overlapping bodies whose manifolds come from the ported narrow phase at the body transforms.
fn random_island(random: &mut Random, case: usize) -> (Vec<Position>, Vec<Velocity>, Vec<SolverContact>) {
    const BODIES: usize = 4;
    let axes = [Vec2::new(1.0, 1.0), Vec2::new(1.0, 1.0), Vec2::new(0.0, 1.0), Vec2::new(1.0, 0.0), Vec2::new(0.0, 0.0)];
    let bodies: Vec<SolverBody> = (0..BODIES)
        .map(|i| {
            let fixed = (case + i) % 7 == 0;
            SolverBody {
                inv_mass: if fixed { 0.0 } else { random.range(0.1, 3.0) },
                axis: axes[random.next() as usize % axes.len()],
                inv_i: if fixed { 0.0 } else { random.range(0.0, 20.0) },
                local_center: random.vec(0.2),
            }
        })
        .collect();
    let positions: Vec<Position> = (0..BODIES).map(|_| Position { c: random.vec(0.3), a: random.range(-3.2, 3.2) }).collect();
    let velocities: Vec<Velocity> = (0..BODIES).map(|_| Velocity { v: random.vec(4.0), w: random.range(-6.0, 6.0) }).collect();
    let xf = |i: usize| {
        let q = super::math::Rot::new(positions[i].a);
        Transform { p: positions[i].c - q.apply(bodies[i].local_center), q }
    };
    let mut contacts = Vec::new();
    while contacts.len() < 3 {
        let index_a = random.next() as usize % BODIES;
        let index_b = (index_a + 1 + random.next() as usize % (BODIES - 1)) % BODIES;
        let (xf_a, xf_b) = (xf(index_a), xf(index_b));
        let mut manifold = Manifold::default();
        let k = random.next() as usize;
        let (radius_a, radius_b) = match k % 4 {
            0 | 1 => {
                let a = random_polygon(random, k);
                let b = random_polygon(random, k / 4);
                collide_polygons(&mut manifold, &a, &xf_a, &b, &xf_b);
                (a.radius, b.radius)
            }
            2 => {
                let a = random_polygon(random, k);
                let b = Circle { radius: random.range(0.02, 0.6), p: random.vec(0.3) };
                collide_polygon_and_circle(&mut manifold, &a, &xf_a, &b, &xf_b);
                (a.radius, b.radius)
            }
            _ => {
                let a = Circle { radius: random.range(0.02, 0.6), p: random.vec(0.3) };
                let b = Circle { radius: random.range(0.02, 0.6), p: random.vec(0.3) };
                collide_circles(&mut manifold, &a, &xf_a, &b, &xf_b);
                (a.radius, b.radius)
            }
        };
        if manifold.point_count == 0 {
            continue;
        }
        for point in &mut manifold.points[..manifold.point_count] {
            point.normal_impulse = random.range(0.0, 0.5);
            point.tangent_impulse = random.range(-0.2, 0.2);
        }
        contacts.push(SolverContact {
            manifold,
            radius_a,
            radius_b,
            friction: random.range(0.0, 1.0),
            restitution: if k % 3 == 0 { 0.0 } else { random.range(0.0, 1.0) },
            tangent_speed: if k % 5 == 0 { random.range(-1.0, 1.0) } else { 0.0 },
            index_a,
            index_b,
            body_a: bodies[index_a],
            body_b: bodies[index_b],
        });
    }
    (positions, velocities, contacts)
}

/// Each native solver pass runs on the Rust state just before the matching Rust pass and is compared after it.
fn compare_solver(executable: &std::path::Path) -> Result<(), String> {
    const NAMES: [&str; 5] = ["INITIALIZE VELOCITY", "WARM START", "SOLVE VELOCITY", "SOLVE POSITION", "SOLVE TOI POSITION"];
    let mut random = Random(0x5eed_0003);
    let mut calls = Vec::new();
    let mut expected = Vec::new();
    for case in 0..2048 {
        let (mut positions, mut velocities, contacts) = random_island(&mut random, case);
        let step = TimeStep { dt: 1.0 / 60.0, inv_dt: 60.0, dt_ratio: [1.0, 0.75][case % 2], velocity_iterations: 8, position_iterations: 3, warm_starting: case % 8 != 0 };
        let mut solver = ContactSolver::new(step, &contacts);
        calls.push(solver_call(INITIALIZE_VELOCITY, &solver, &contacts, &positions, &velocities, &["b0"]));
        solver.initialize_velocity_constraints(&positions, &velocities);
        expected.push((solver_state(&solver, &positions, &velocities), false));
        calls.push(solver_call(WARM_START, &solver, &contacts, &positions, &velocities, &["b0"]));
        solver.warm_start(&mut velocities);
        expected.push((solver_state(&solver, &positions, &velocities), false));
        for _ in 0..case % 3 {
            solver.solve_velocity_constraints(&mut velocities);
        }
        calls.push(solver_call(SOLVE_VELOCITY, &solver, &contacts, &positions, &velocities, &["b0"]));
        solver.solve_velocity_constraints(&mut velocities);
        expected.push((solver_state(&solver, &positions, &velocities), false));
        // Integrate a little so separations land on both sides of the slop.
        for (p, v) in positions.iter_mut().zip(&velocities) {
            p.c = p.c + v.v * (1.0 / 60.0);
            p.a = p.a + v.w * (1.0 / 60.0);
        }
        calls.push(solver_call(SOLVE_POSITION, &solver, &contacts, &positions, &velocities, &["b0"]));
        let ok = solver.solve_position_constraints(&mut positions);
        expected.push((solver_state(&solver, &positions, &velocities), ok));
        let (toi_a, toi_b) = (contacts[0].index_a, contacts[0].index_b);
        calls.push(solver_call(SOLVE_TOI_POSITION, &solver, &contacts, &positions, &velocities, &["b0", &format!("i{toi_a}"), &format!("i{toi_b}")]));
        let ok = solver.solve_toi_position_constraints(&mut positions, toi_a, toi_b);
        expected.push((solver_state(&solver, &positions, &velocities), ok));
    }
    let count = calls.len();
    let replies: Vec<Reply> = crate::motor::run_probe(executable, &Batch { globals: SOLVER_GLOBALS.to_vec(), calls }, count)?;
    let words = |b: &[u8]| (0..b.len() / 4).map(|k| get(b, 4 * k)).collect::<Vec<u32>>();
    let mut bad = [0usize; 5];
    let mut first: [Option<String>; 5] = Default::default();
    for (i, reply) in replies.iter().enumerate() {
        let (state, ret) = &expected[i];
        let mut diffs = Vec::new();
        for (k, name) in ["positions", "velocities", "pcs", "vcs"].iter().enumerate() {
            let (rust, native) = (words(&state[k]), words(&unhex(&reply.buffers[k + 1])));
            if let Some(at) = (0..rust.len()).find(|&w| rust[w] != native[w]) {
                diffs.push(format!("{name}@0x{:x} native={:08x} rust={:08x}", 4 * at, native[at], rust[at]));
            }
        }
        if i % 5 >= 3 && ((reply.ret & 0xff) != 0) != *ret {
            diffs.push(format!("ret native={} rust={ret}", reply.ret & 0xff));
        }
        if !diffs.is_empty() {
            bad[i % 5] += 1;
            first[i % 5].get_or_insert_with(|| format!("case {}: {}", i / 5, diffs.join(", ")));
        }
    }
    let failures: Vec<String> = (0..5).filter_map(|k| report(NAMES[k], bad[k], replies.len() / 5, first[k].take()).err()).collect();
    if failures.is_empty() { Ok(()) } else { Err(failures.join("\n")) }
}

fn vertex_bytes(vertices: &[Vec2]) -> Vec<u8> {
    let mut b = vec![0; 8 * vertices.len()];
    for (i, v) in vertices.iter().enumerate() {
        put_vec(&mut b, 8 * i, *v);
    }
    b
}

fn cache_bytes(cache: &SimplexCache) -> Vec<u8> {
    let mut b = vec![0; 12];
    put(&mut b, 0, cache.metric.to_bits());
    b[4] = cache.count as u8;
    b[6..9].copy_from_slice(&cache.index_a);
    b[9..12].copy_from_slice(&cache.index_b);
    b
}

/// A polygon, a circle centre or a capsule segment, as (vertices, radius).
fn random_proxy(random: &mut Random, index: usize) -> (Vec<Vec2>, f32) {
    match index % 3 {
        0 => {
            let polygon = random_polygon(random, index / 3);
            (polygon.vertices[..polygon.count].to_vec(), polygon.radius)
        }
        1 => (vec![random.vec(0.3)], random.range(0.02, 0.6)),
        _ => (vec![random.vec(0.6), random.vec(0.6)], random.range(0.0, 0.3)),
    }
}

/// b2Distance from a cold cache, then again warm-started from the Rust cache at nudged transforms.
fn compare_distance(executable: &std::path::Path) -> Result<(), String> {
    let mut random = Random(0x5eed_0004);
    let mut calls = Vec::new();
    let mut expected = Vec::new();
    for i in 0..4096 {
        let (a, ra) = random_proxy(&mut random, i);
        let (b, rb) = random_proxy(&mut random, i / 3 + 1);
        let proxy_a = DistanceProxy { vertices: &a, radius: ra };
        let proxy_b = DistanceProxy { vertices: &b, radius: rb };
        let use_radii = i % 4 != 0;
        let mut cache = SimplexCache::default();
        let mut xf_a = random_transform(&mut random, 1.0);
        let mut xf_b = random_transform(&mut random, 1.0);
        for warm in [false, true] {
            if warm {
                xf_a = Transform::new(xf_a.p + random.vec(0.05), random.range(-3.2, 3.2));
                xf_b = Transform::new(xf_b.p + random.vec(0.05), random.range(-3.2, 3.2));
            }
            let mut input = vec![0; 0x68];
            for (o, proxy) in [(0, &proxy_a), (0x20, &proxy_b)] {
                put(&mut input, o + 0x18, proxy.vertices.len() as u32);
                put(&mut input, o + 0x1c, proxy.radius.to_bits());
            }
            input[0x40..0x50].copy_from_slice(&transform_bytes(&xf_a));
            input[0x50..0x60].copy_from_slice(&transform_bytes(&xf_b));
            input[0x60] = use_radii as u8;
            let mut c = call(DISTANCE, vec![vec![0; 24], cache_bytes(&cache), input, vertex_bytes(&a), vertex_bytes(&b)], &["b0", "b1", "b2"]);
            c.links = vec![[2, 0x10, 3, 0], [2, 0x30, 4, 0]];
            calls.push(c);
            let output = distance(&mut cache, &proxy_a, &xf_a, &proxy_b, &xf_b, use_radii);
            let mut out = vec![0; 24];
            put_vec(&mut out, 0, output.point_a);
            put_vec(&mut out, 8, output.point_b);
            put(&mut out, 0x10, output.distance.to_bits());
            put(&mut out, 0x14, output.iterations as u32);
            expected.push((out, cache_bytes(&cache), output.distance > 0.0));
        }
    }
    let count = calls.len();
    let replies: Vec<Reply> = crate::motor::run_probe(executable, &Batch { globals: vec![], calls }, count)?;
    let (mut bad, mut first) = (0, None);
    for (i, reply) in replies.iter().enumerate() {
        let (out, cache, _) = &expected[i];
        let (native_out, native_cache) = (unhex(&reply.buffers[0]), unhex(&reply.buffers[1]));
        if &native_out != out || &native_cache != cache {
            bad += 1;
            first.get_or_insert_with(|| format!("case {i}: native out={native_out:02x?} cache={native_cache:02x?} rust out={out:02x?} cache={cache:02x?}"));
        }
    }
    report("DISTANCE", bad, replies.len(), first)?;
    println!("  ({} of them separated)", expected.iter().filter(|e| e.2).count());
    Ok(())
}

const PERLIN_NOISE: u32 = 0x975490;

/// Mathf.PerlinNoise (crate::unity) on random, negative, large, integer and half-integer inputs.
fn compare_perlin(executable: &std::path::Path) -> Result<(), String> {
    let mut random = Random(0x5eed);
    let mut inputs: Vec<(f32, f32)> = vec![(0.0, 0.0), (0.5, 0.5), (1.0, 1.0), (255.5, 256.25), (-3.7, 2.2), (1e6, 1e-3), (-0.0, 7.0)];
    for i in 0..20000 {
        let scale = [1.0, 10.0, 300.0, 1e4, 1e7][i % 5];
        inputs.push((random.range(-scale, scale), random.range(-scale, scale)));
    }
    let calls: Vec<Call> = inputs
        .iter()
        .map(|(x, y)| call(PERLIN_NOISE, vec![], &[&format!("f{}", x.to_bits()), &format!("f{}", y.to_bits())]))
        .collect();
    let replies: Vec<Reply> = crate::motor::run_probe(executable, &Batch { globals: vec![], calls }, inputs.len())?;
    let mut first = None;
    let mismatches = inputs
        .iter()
        .zip(&replies)
        .filter(|((x, y), r)| {
            let ours = crate::unity::perlin_noise(*x, *y).to_bits();
            let bad = ours != r.ret_f;
            if bad && first.is_none() {
                first = Some(format!("PerlinNoise({x}, {y}): engine {:08x}, rewrite {ours:08x}", r.ret_f));
            }
            bad
        })
        .count();
    report("Mathf.PerlinNoise", mismatches, inputs.len(), first)
}

const CURVE_EVALUATE: u32 = 0x746eb0;

/// AnimationCurveTpl<float>::Evaluate on random curves and wrap modes (incl. infinite slopes and repeated key
/// times), one cache carried through each curve's evaluation sequence; compares the result and the cache it leaves.
fn compare_curve(executable: &std::path::Path) -> Result<(), String> {
    use crate::game_assets::{CLAMP, Curve, CurveCache, PING_PONG, REPEAT, ping_pong, repeat};
    let mut random = Random(0x5eed_c0e5);
    let (mut calls, mut expected) = (vec![], vec![]);
    for c in 0..600 {
        let n = 1 + c % 6;
        let mut time = random.range(-3.0, 3.0);
        let keys: Vec<[f32; 4]> = (0..n)
            .map(|_| {
                let slope = |r: &mut Random| if r.next() % 23 == 0 { f32::INFINITY } else { r.range(-4.0, 4.0) };
                let key = [time, random.range(-2.0, 2.0), slope(&mut random), slope(&mut random)];
                time += if random.next() % 9 == 0 { 0.0 } else { random.range(1e-5, 2.0) };
                key
            })
            .collect();
        let curve = Curve { keys, pre: (random.next() % 4) as i32, post: (random.next() % 4) as i32 };
        let (first, last) = (curve.keys[0][0], curve.keys[n - 1][0]);
        let mut keys = vec![0; n * 0x1c];
        for (i, k) in curve.keys.iter().enumerate() {
            for (j, v) in k.iter().enumerate() {
                put(&mut keys, i * 0x1c + j * 4, v.to_bits());
            }
        }
        let mut cache = CurveCache::default();
        let mut t = first;
        for e in 0..24 {
            t = match e % 4 {
                0 => random.range(first - 3.0 * (last - first) - 1.0, last + 3.0 * (last - first) + 1.0),
                1 => t + random.range(-0.05, 0.05),
                2 => curve.keys[random.next() as usize % n][0],
                _ => random.range(first, last),
            };
            // Skip times whose wrap rounds below the first key: the engine reads in front of its key array there.
            let mode = if t < first { curve.pre } else { curve.post };
            if (t < first || t > last) && mode != CLAMP && !(t > last && mode == REPEAT) {
                let w = if mode == PING_PONG { ping_pong(t, first, last) } else { repeat(t, first, last) };
                if w < first {
                    continue;
                }
            }
            let mut object = vec![0; 0x68];
            put(&mut object, 0, cache.index as u32);
            put(&mut object, 4, cache.time.to_bits());
            put(&mut object, 8, cache.time_end.to_bits());
            for (i, v) in cache.coeff.iter().enumerate() {
                put(&mut object, 0xc + i * 4, v.to_bits());
            }
            put(&mut object, 0x20, f32::INFINITY.to_bits());
            put(&mut object, 0x48, n as u32);
            put(&mut object, 0x50, n as u32);
            put(&mut object, 0x58, curve.pre as u32);
            put(&mut object, 0x5c, curve.post as u32);
            let mut call = call(CURVE_EVALUATE, vec![object, keys.clone()], &["b0", &format!("f{}", t.to_bits()), "i0"]);
            call.returns_float = true;
            call.links = vec![[0, 0x38, 1, 0]];
            calls.push(call);
            let value = curve.evaluate(&mut cache, t);
            expected.push((c, t, value, cache));
        }
    }
    let replies: Vec<Reply> = crate::motor::run_probe(executable, &Batch { globals: vec![], calls }, expected.len())?;
    let mut first = None;
    let mismatches = expected
        .iter()
        .zip(&replies)
        .filter(|((c, t, value, cache), r)| {
            let b = unhex(&r.buffers[0]);
            let engine = [get(&b, 0), get(&b, 4), get(&b, 8), get(&b, 0xc), get(&b, 0x10), get(&b, 0x14), get(&b, 0x18)];
            let ours = [cache.index as u32, cache.time.to_bits(), cache.time_end.to_bits(), cache.coeff[0].to_bits(), cache.coeff[1].to_bits(), cache.coeff[2].to_bits(), cache.coeff[3].to_bits()];
            let bad = value.to_bits() != r.ret_f || engine != ours;
            if bad && first.is_none() {
                first = Some(format!("curve {c} Evaluate({t}): engine {:08x} cache {engine:08x?}, rewrite {:08x} cache {ours:08x?}", r.ret_f, value.to_bits()));
            }
            bad
        })
        .count();
    report("AnimationCurve.Evaluate", mismatches, expected.len(), first)
}

pub fn compare_reference(executable: &std::path::Path) -> Result<(), String> {
    compare_shapes(executable)?;
    compare_manifolds(executable)?;
    compare_solver(executable)?;
    compare_distance(executable)?;
    compare_toi(executable)?;
    compare_broad_phase(executable)?;
    compare_island_solve(executable, false)?;
    compare_island_solve(executable, true)?;
    compare_revolute(executable)?;
    compare_distance_joint(executable)?;
    compare_weld(executable)?;
    compare_perlin(executable)?;
    compare_curve(executable)
}

fn sweep_bytes(b: &mut [u8], offset: usize, sweep: &Sweep) {
    put_vec(b, offset, sweep.local_center);
    put_vec(b, offset + 8, sweep.c0);
    put_vec(b, offset + 0x10, sweep.c);
    put(b, offset + 0x18, sweep.a0.to_bits());
    put(b, offset + 0x1c, sweep.a.to_bits());
    put(b, offset + 0x20, sweep.alpha0.to_bits());
}

/// Fast sweeps that pass through, graze or miss each other, with wound-up angles for Sweep::Normalize.
fn compare_toi(executable: &std::path::Path) -> Result<(), String> {
    let mut random = Random(0x5eed_0005);
    let mut calls = Vec::new();
    let mut expected = Vec::new();
    for i in 0..4096 {
        let (a, ra) = random_proxy(&mut random, i);
        let (b, rb) = random_proxy(&mut random, i / 3 + 1);
        let sweep = |random: &mut Random| {
            let c0 = random.vec(2.0);
            let a0 = random.range(-20.0, 20.0);
            Sweep { local_center: random.vec(0.2), c0, c: c0 * -1.0 + random.vec(0.5), a0, a: a0 + random.range(-2.0, 2.0), alpha0: 0.0 }
        };
        let (sweep_a, sweep_b) = (sweep(&mut random), sweep(&mut random));
        let t_max = if i % 5 == 0 { random.range(0.2, 1.0) } else { 1.0 };
        let mut input = vec![0; 0x90];
        put(&mut input, 0x18, a.len() as u32);
        put(&mut input, 0x1c, ra.to_bits());
        put(&mut input, 0x38, b.len() as u32);
        put(&mut input, 0x3c, rb.to_bits());
        sweep_bytes(&mut input, 0x40, &sweep_a);
        sweep_bytes(&mut input, 0x64, &sweep_b);
        put(&mut input, 0x88, t_max.to_bits());
        let mut c = call(TIME_OF_IMPACT, vec![vec![0; 8], input, vertex_bytes(&a), vertex_bytes(&b)], &["b0", "b1"]);
        c.links = vec![[1, 0x10, 2, 0], [1, 0x30, 3, 0]];
        calls.push(c);
        let output = time_of_impact(&DistanceProxy { vertices: &a, radius: ra }, &sweep_a, &DistanceProxy { vertices: &b, radius: rb }, &sweep_b, t_max);
        expected.push(output);
    }
    let count = calls.len();
    let replies: Vec<Reply> = crate::motor::run_probe(executable, &Batch { globals: vec![], calls }, count)?;
    let (mut bad, mut first, mut states) = (0, None, [0usize; 5]);
    for (i, reply) in replies.iter().enumerate() {
        let native = unhex(&reply.buffers[0]);
        let output = expected[i];
        states[output.state as usize] += 1;
        let rust = [output.state as u32, output.t.to_bits()];
        if [get(&native, 0), get(&native, 4)] != rust {
            bad += 1;
            first.get_or_insert_with(|| format!("case {i}: native=[{}, {:08x}] rust=[{}, {:08x}]", get(&native, 0), get(&native, 4), rust[0], rust[1]));
        }
    }
    report("TIME OF IMPACT", bad, replies.len(), first)?;
    println!("  (states unknown/failed/overlapped/touching/separated: {states:?})");
    Ok(())
}

const PROBE_TREE_CAPACITY: usize = 64;
const PROBE_BUFFER_CAPACITY: usize = 256;

fn aabb_bytes(aabb: &Aabb) -> Vec<u8> {
    let mut b = vec![0; 16];
    put_vec(&mut b, 0, aabb.lower);
    put_vec(&mut b, 8, aabb.upper);
    b
}

/// b2BroadPhase (0x78) with its node array, move buffer and pair buffer as separate linked buffers.
fn broad_phase_bytes(bp: &BroadPhase, query_id: i32, pairs: &[(i32, i32)]) -> [Vec<u8>; 4] {
    let tree = &bp.tree;
    let mut b = vec![0; 0x78];
    put(&mut b, 0, tree.root as u32);
    put(&mut b, 0x10, tree.node_count as u32);
    put(&mut b, 0x14, tree.nodes.len() as u32);
    put(&mut b, 0x18, tree.free_list as u32);
    put(&mut b, 0x20, tree.insertion_count as u32);
    put(&mut b, 0x28, bp.proxy_count as u32);
    put(&mut b, 0x38, 0x2a);
    put(&mut b, 0x40, pairs.len() as u32);
    put(&mut b, 0x48, (PROBE_BUFFER_CAPACITY << 1) as u32);
    put(&mut b, 0x58, 0x2a);
    put(&mut b, 0x60, bp.move_buffer.len() as u32);
    put(&mut b, 0x68, (PROBE_BUFFER_CAPACITY << 1) as u32);
    put(&mut b, 0x70, query_id as u32);
    let mut nodes = vec![0; tree.nodes.len() * 0x28];
    for (i, n) in tree.nodes.iter().enumerate() {
        let o = i * 0x28;
        nodes[o..o + 16].copy_from_slice(&aabb_bytes(&n.aabb));
        nodes[o + 0x10..o + 0x18].copy_from_slice(&(n.user_data as u64).to_le_bytes());
        put(&mut nodes, o + 0x18, n.parent as u32);
        put(&mut nodes, o + 0x1c, n.child1 as u32);
        put(&mut nodes, o + 0x20, n.child2 as u32);
        put(&mut nodes, o + 0x24, n.height as u32);
    }
    let mut moves = vec![0; PROBE_BUFFER_CAPACITY * 4];
    for (i, &id) in bp.move_buffer.iter().enumerate() {
        put(&mut moves, i * 4, id as u32);
    }
    let mut pair_bytes = vec![0; PROBE_BUFFER_CAPACITY * 8];
    for (i, &(a, b)) in pairs.iter().enumerate() {
        put(&mut pair_bytes, i * 8, a as u32);
        put(&mut pair_bytes, i * 8 + 4, b as u32);
    }
    [b, nodes, moves, pair_bytes]
}

/// The comparable state: struct fields without pointers, every node byte, live move ids and pairs.
fn broad_phase_state(buffers: &[Vec<u8>]) -> Vec<u32> {
    let b = &buffers[0];
    let mut state: Vec<u32> = [0, 0x10, 0x14, 0x18, 0x20, 0x28, 0x40, 0x60].iter().map(|&o| get(b, o)).collect();
    state.extend(buffers[1].chunks(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())));
    state.extend((0..get(b, 0x60) as usize).map(|i| get(&buffers[2], i * 4)));
    state.extend((0..get(b, 0x40) as usize * 2).map(|i| get(&buffers[3], i * 4)));
    state
}

fn broad_phase_call(rva: u32, bp: &BroadPhase, query_id: i32, aabb: &Aabb, displacement: Vec2, args: &[&str]) -> Call {
    let mut d = vec![0; 8];
    put_vec(&mut d, 0, displacement);
    let [b, nodes, moves, pairs] = broad_phase_bytes(bp, query_id, &[]);
    let mut c = call(rva, vec![b, nodes, moves, pairs, aabb_bytes(aabb), d], args);
    c.links = vec![[0, 8, 1, 0], [0, 0x30, 3, 0], [0, 0x50, 2, 0]];
    c
}

/// Random create/move/destroy/query sequences; each native call starts from the rewrite's state before the op.
fn compare_broad_phase(executable: &std::path::Path) -> Result<(), String> {
    let mut random = Random(0x5eed_0006);
    let mut calls = Vec::new();
    // (op name, expected state, expected returned proxy id)
    let mut expected: Vec<(&str, Vec<u32>, Option<i32>)> = Vec::new();
    let random_aabb = |random: &mut Random| {
        let c = random.vec(5.0);
        let h = Vec2::new(random.range(0.05, 1.0), random.range(0.05, 1.0));
        Aabb { lower: c - h, upper: c + h }
    };
    let mut pairs_found = 0;
    for _ in 0..96 {
        let mut bp = BroadPhase::with_capacity(PROBE_TREE_CAPACITY);
        let mut live: Vec<(i32, Aabb)> = Vec::new();
        for step in 0..48 {
            let roll = random.next() % 10;
            if live.len() < 4 || (roll < 3 && live.len() < 24) {
                let aabb = random_aabb(&mut random);
                let user = 1000 + step;
                calls.push(broad_phase_call(CREATE_PROXY, &bp, NULL, &aabb, Vec2::ZERO, &["b0", "b4", &format!("i{user}")]));
                let id = bp.create_proxy(&aabb, user);
                live.push((id, aabb));
                expected.push(("create", broad_phase_state(&broad_phase_bytes(&bp, NULL, &[])), Some(id)));
            } else if roll < 4 {
                let (id, _) = live.swap_remove(random.next() as usize % live.len());
                calls.push(broad_phase_call(DESTROY_PROXY, &bp, NULL, &Aabb::default(), Vec2::ZERO, &["b0", &format!("i{id}")]));
                bp.destroy_proxy(id);
                expected.push(("destroy", broad_phase_state(&broad_phase_bytes(&bp, NULL, &[])), None));
            } else if roll < 8 {
                let k = random.next() as usize % live.len();
                let (id, old) = live[k];
                let d = if roll == 4 { random.vec(0.02) } else { random.vec(0.6) };
                let grow = if roll == 7 { random.range(-0.04, 0.3) } else { 0.0 };
                let aabb = Aabb { lower: old.lower + d - Vec2::new(grow, grow), upper: old.upper + d + Vec2::new(grow, grow) };
                live[k].1 = aabb;
                calls.push(broad_phase_call(MOVE_PROXY, &bp, NULL, &aabb, d, &["b0", &format!("i{id}"), "b4", "b5"]));
                bp.move_proxy(id, &aabb, d);
                expected.push(("move", broad_phase_state(&broad_phase_bytes(&bp, NULL, &[])), None));
            } else {
                let id = live[random.next() as usize % live.len()].0;
                let fat = *bp.tree.fat_aabb(id);
                calls.push(broad_phase_call(QUERY_BROAD_PHASE, &bp, id, &fat, Vec2::ZERO, &["b0", "b0", "b4"]));
                let mut pairs = Vec::new();
                bp.query_pairs(id, &mut pairs);
                pairs_found += pairs.len();
                expected.push(("query", broad_phase_state(&broad_phase_bytes(&bp, id, &pairs)), None));
            }
            if step % 12 == 11 {
                bp.update_pairs(|_, _| {});
            }
        }
    }
    let count = calls.len();
    let replies: Vec<Reply> = crate::motor::run_probe(executable, &Batch { globals: vec![], calls }, count)?;
    let (mut bad, mut first) = (0, None);
    for (i, reply) in replies.iter().enumerate() {
        let native: Vec<Vec<u8>> = reply.buffers.iter().map(|b| unhex(b)).collect();
        let (op, rust, id) = &expected[i];
        let state = broad_phase_state(&native);
        let id_ok = id.map_or(true, |id| reply.ret as i32 == id);
        if state != *rust || !id_ok {
            bad += 1;
            let at = state.iter().zip(rust).position(|(a, b)| a != b);
            first.get_or_insert_with(|| format!("call {i} {op}: word {at:?} native={:?} rust={:?} id native={} rust={id:?}", at.map(|a| state[a]), at.map(|a| rust[a]), reply.ret as i32));
        }
    }
    report("BROAD PHASE", bad, replies.len(), first)?;
    println!("  ({pairs_found} query pairs)");
    Ok(())
}

const ISLAND_SOLVE: u32 = 0xbba770;
const ISLAND_SOLVE_TOI: u32 = 0xbbb360;
const STACK_ALLOCATOR_SIZE: usize = 102_936;
const DEG: f32 = f32::from_bits(0x3c8e_fa35);
/// SOLVER_GLOBALS plus the Island::Solve limits and sleep tolerances.
fn island_globals() -> Vec<[u32; 2]> {
    let mut g = SOLVER_GLOBALS.to_vec();
    let max_rotation = 360.0 * DEG;
    g.extend([
        [0x1999c38, 100.0f32.to_bits()],
        [0x1a43874, 10000.0f32.to_bits()],
        [0x1999c3c, max_rotation.to_bits()],
        [0x1a43878, (max_rotation * max_rotation).to_bits()],
        [0x1999c48, 0.5f32.to_bits()],
        [0x1999c34, (8.0 * DEG).to_bits()],
        [0x1999c4c, 0.01f32.to_bits()],
        [0x1999c50, (2.0 * DEG).to_bits()],
    ]);
    g
}

fn body_bytes(b: &mut [u8], o: usize, body: &super::world::Body) {
    put(b, o, body.kind as u32);
    put(b, o + 4, body.flags as u32);
    put(b, o + 8, body.island_index as u32);
    put_vec(b, o + 0xc, body.xf.p);
    put(b, o + 0x14, body.xf.q.s.to_bits());
    put(b, o + 0x18, body.xf.q.c.to_bits());
    sweep_bytes(b, o + 0x1c, &body.sweep);
    put_vec(b, o + 0x40, body.linear_velocity);
    put(b, o + 0x48, body.angular_velocity.to_bits());
    put_vec(b, o + 0x4c, body.force);
    put(b, o + 0x54, body.torque.to_bits());
    let fields = [body.mass, body.inv_mass, body.axis.x, body.axis.y, body.i, body.inv_i, body.linear_damping, body.angular_damping, body.gravity_scale, body.sleep_time];
    for (k, v) in fields.iter().enumerate() {
        put(b, o + 0x90 + 4 * k, v.to_bits());
    }
}

/// b2Island::Solve over one island: buffers 0 island, 1 stack allocator, 2 arena (body and contact pointer
/// arrays, positions, velocities, bodies, contacts, two fixtures per contact), 3 step, 4 gravity. Returns the
/// call, the arena and the arena byte ranges holding pointers or solver scratch.
fn island_call(world: &super::world::World, bodies: &[usize], contacts: &[usize], step: &TimeStep) -> (Call, Vec<u8>, Vec<(usize, usize)>) {
    let (n, m) = (bodies.len(), contacts.len());
    let positions = 8 * n + 8 * m;
    let body_at = positions + 24 * n;
    let contact_at = body_at + 200 * n;
    let fixture_at = contact_at + 312 * m;
    let mut arena = vec![0; fixture_at + 2 * 88 * m];
    let mut links = Vec::new();
    let mut skip = vec![(0, body_at)];
    let slot = |id: usize| bodies.iter().position(|&b| b == id).unwrap();
    for (i, &id) in bodies.iter().enumerate() {
        body_bytes(&mut arena, body_at + 200 * i, world.body(id));
        links.push([2, 8 * i, 2, body_at + 200 * i]);
    }
    for (j, &id) in contacts.iter().enumerate() {
        let c = world.contact(id);
        let o = contact_at + 312 * j;
        links.push([2, 8 * n + 8 * j, 2, o]);
        put(&mut arena, o + 8, c.flags);
        put(&mut arena, o + 0x78, c.island_index_a as u32);
        put(&mut arena, o + 0x7c, c.island_index_b as u32);
        arena[o + 0x80..o + 0x80 + 72].copy_from_slice(&manifold_bytes(&c.manifold));
        put(&mut arena, o + 0xc0, c.radius_a.to_bits());
        put(&mut arena, o + 0xc4, c.radius_b.to_bits());
        put(&mut arena, o + 0x118, c.friction.to_bits());
        put(&mut arena, o + 0x11c, c.restitution.to_bits());
        put(&mut arena, o + 0x120, c.tangent_speed.to_bits());
        for (k, f) in [c.fixture_a, c.fixture_b].into_iter().enumerate() {
            let fo = fixture_at + 88 * (2 * j + k);
            links.push([2, o + 0x60 + 8 * k, 2, fo]);
            links.push([2, fo + 0x20, 2, body_at + 200 * slot(world.fixture(f).body)]);
            skip.push((fo + 0x20, fo + 0x28));
        }
        skip.push((o + 0x60, o + 0x70));
    }
    let mut island = vec![0; 88];
    put(&mut island, 0x40, n as u32);
    put(&mut island, 0x48, m as u32);
    put(&mut island, 0x4c, n as u32);
    put(&mut island, 0x50, m as u32);
    let mut s = vec![0; 24];
    let fields = [step.dt.to_bits(), step.inv_dt.to_bits(), step.dt_ratio.to_bits(), step.velocity_iterations as u32, step.position_iterations as u32, step.warm_starting as u32];
    for (k, v) in fields.iter().enumerate() {
        put(&mut s, 4 * k, *v);
    }
    let mut g = vec![0; 8];
    put_vec(&mut g, 0, world.gravity);
    let sleep = if world.allow_sleep { "i1" } else { "i0" };
    let mut c = call(ISLAND_SOLVE, vec![island, vec![0; STACK_ALLOCATOR_SIZE], arena.clone(), s, g], &["b0", "b3", "b4", sleep]);
    c.links = vec![[0, 0, 1, 0], [0, 0x18, 2, 0], [0, 0x20, 2, 8 * n], [0, 0x30, 2, positions], [0, 0x38, 2, positions + 12 * n]];
    c.links.extend(links);
    (c, arena, skip)
}

/// Random stacks of boxes and discs on a ground box, run for a while in the ported world; each case solves
/// every touching contact as one island, natively and in Rust, from the same state. With `toi`, the island is
/// solved as a TOI sub-step about its first contact's bodies (b2Island::SolveTOI).
fn compare_island_solve(executable: &std::path::Path, toi: bool) -> Result<(), String> {
    use super::world::*;
    let mut random = Random(if toi { 0x5eed_0008 } else { 0x5eed_0007 });
    let mut calls = Vec::new();
    let mut expected = Vec::new();
    for case in 0..192 {
        let mut world = World::default();
        let fixture = |shape, random: &mut Random| FixtureDef { shape, friction: random.range(0.0, 1.0), restitution: random.range(0.0, 0.5), density: random.range(0.5, 3.0), is_sensor: false, filter: Filter::default(), collider: Default::default() };
        let ground = world.create_body(&BodyDef::default());
        let ground_box = Polygon::from_points(&box_corners(Vec2::ZERO, Vec2::new(30.0, 1.0)), POLYGON_RADIUS);
        world.create_fixture(ground, &fixture(Shape::Polygon(ground_box), &mut random), true);
        for i in 0..2 + random.next() as usize % 6 {
            let def = BodyDef {
                kind: if (case + i) % 9 == 0 { BodyType::Kinematic } else { BodyType::Dynamic },
                position: Vec2::new(random.range(-1.5, 1.5), 0.6 + 0.9 * i as f32 + random.range(0.0, 0.3)),
                angle: random.range(-0.5, 0.5),
                linear_velocity: random.vec(1.0),
                angular_velocity: random.range(-2.0, 2.0),
                linear_damping: random.range(0.0, 0.5),
                angular_damping: random.range(0.0, 0.5),
                fixed_rotation: random.next() % 8 == 0,
                gravity_scale: random.range(0.5, 1.5),
                ..BodyDef::default()
            };
            let id = world.create_body(&def);
            let shape = if random.next() % 3 == 0 {
                Shape::Circle(Circle { p: random.vec(0.1), radius: random.range(0.2, 0.5) })
            } else {
                Shape::Polygon(Polygon::from_points(&box_corners(random.vec(0.1), Vec2::new(random.range(0.4, 1.2), random.range(0.4, 1.0))), POLYGON_RADIUS))
            };
            world.create_fixture(id, &fixture(shape, &mut random), true);
        }
        for _ in 0..random.next() % 120 {
            world.step(1.0 / 60.0, 8, 8);
        }
        let touching = CONTACT_TOUCHING | CONTACT_ENABLED;
        let contacts: Vec<usize> = world.contact_list.iter().copied().filter(|&c| world.contact(c).flags & touching == touching).collect();
        let mut bodies = world.body_list.clone();
        if toi {
            let Some(&first) = contacts.first() else { continue };
            let pair = [world.fixture(world.contact(first).fixture_a).body, world.fixture(world.contact(first).fixture_b).body];
            bodies.retain(|b| !pair.contains(b));
            bodies.splice(0..0, pair);
        }
        for &id in &bodies {
            let b = world.body_mut(id);
            if b.kind == BodyType::Dynamic && random.next() % 2 == 0 {
                b.force = random.vec(20.0);
                b.torque = random.range(-5.0, 5.0);
            }
        }
        for &c in &contacts {
            let slot = |f| bodies.iter().position(|&x| x == world.fixture(f).body).unwrap();
            let (a, b) = (slot(world.contact(c).fixture_a), slot(world.contact(c).fixture_b));
            let c = world.contacts[c].as_mut().unwrap();
            c.island_index_a = a;
            c.island_index_b = b;
        }
        let mut dt = [1.0 / 60.0, 0.02, 1.0 / 30.0][random.next() as usize % 3];
        if toi {
            dt = (1.0 - random.range(0.0, 1.0)) * dt;
        }
        let mut step = TimeStep {
            dt,
            inv_dt: 1.0 / dt,
            dt_ratio: random.range(0.5, 1.5),
            velocity_iterations: 1 + random.next() as i32 % 16,
            position_iterations: 1 + random.next() as i32 % 16,
            warm_starting: random.next() % 4 != 0,
        };
        world.allow_sleep = random.next() % 4 != 0;
        if toi {
            step = TimeStep { dt_ratio: 1.0, position_iterations: 20, warm_starting: false, ..step };
        }
        let (mut call, _, skip) = island_call(&world, &bodies, &contacts, &step);
        if toi {
            call.rva = ISLAND_SOLVE_TOI;
            call.args = ["b0", "b3", "i0", "i1"].map(String::from).to_vec();
            world.solve_toi_island(&bodies, &contacts, &step, 0, 1);
        } else {
            world.solve_island(&bodies, &contacts, &[], &step);
        }
        calls.push(call);
        let (_, after, _) = island_call(&world, &bodies, &contacts, &step);
        expected.push((after, skip, contacts.len()));
    }
    let count = calls.len();
    let replies: Vec<Reply> = crate::motor::run_probe(executable, &Batch { globals: island_globals(), calls }, count)?;
    let (mut bad, mut first, mut touching) = (0, None, 0);
    for (i, reply) in replies.iter().enumerate() {
        let native = unhex(&reply.buffers[2]);
        let (rust, skip, m) = &expected[i];
        touching += m;
        let at = (0..rust.len() / 4).map(|k| 4 * k).find(|&o| !skip.iter().any(|&(a, b)| a <= o && o < b) && get(&native, o) != get(rust, o));
        if let Some(o) = at {
            bad += 1;
            first.get_or_insert_with(|| format!("case {i}: arena byte {o:#x} native={:#x} rust={:#x}", get(&native, o), get(rust, o)));
        }
    }
    report(if toi { "ISLAND SOLVE TOI" } else { "ISLAND SOLVE" }, bad, replies.len(), first)?;
    println!("  ({touching} touching contacts)");
    Ok(())
}

const REVOLUTE_INIT: u32 = 0xbc29c0;
const REVOLUTE_SOLVE_VELOCITY: u32 = 0xbc5d10;
const REVOLUTE_SOLVE_POSITION: u32 = 0xbc4560;

/// b2RevoluteJoint (304 bytes) with the solver temporaries the rewrite keeps in its hinge constraint.
fn revolute_bytes(j: &super::revolute::RevoluteJoint) -> Vec<u8> {
    let mut b = vec![0; 304];
    let v = &j.velocity;
    put(&mut b, 0x70, j.a.index as u32);
    put(&mut b, 0x74, j.b.index as u32);
    put_vec(&mut b, 0x78, j.a.axis);
    put_vec(&mut b, 0x80, j.b.axis);
    put_vec(&mut b, 0x98, j.def.local_anchor_a);
    put_vec(&mut b, 0xa0, j.def.local_anchor_b);
    for (k, x) in j.impulse.iter().enumerate() {
        put(&mut b, 0xa8 + 4 * k, x.to_bits());
    }
    put(&mut b, 0xb4, j.motor_impulse.to_bits());
    b[0xb8] = j.def.enable_motor as u8;
    put(&mut b, 0xbc, j.def.max_motor_torque.to_bits());
    put(&mut b, 0xc0, j.def.motor_speed.to_bits());
    b[0xc4] = j.def.enable_limit as u8;
    put(&mut b, 0xc8, j.def.reference_angle.to_bits());
    put(&mut b, 0xcc, j.def.lower_angle.to_bits());
    put(&mut b, 0xd0, j.def.upper_angle.to_bits());
    put_vec(&mut b, 0xd4, Vec2::new(v.anchor_a[0], v.anchor_a[1]));
    put_vec(&mut b, 0xdc, Vec2::new(v.anchor_b[0], v.anchor_b[1]));
    put_vec(&mut b, 0xe4, j.a.local_center);
    put_vec(&mut b, 0xec, j.b.local_center);
    for (k, x) in [j.a.inv_mass, j.b.inv_mass, j.a.inv_i, j.b.inv_i].iter().enumerate() {
        put(&mut b, 0xf4 + 4 * k, x.to_bits());
    }
    for (k, x) in v.matrix.iter().enumerate() {
        put(&mut b, 0x104 + 4 * k, x.to_bits());
    }
    put(&mut b, 0x128, v.motor_mass.to_bits());
    put(&mut b, 0x12c, j.limit_state);
    b
}

/// Buffers: 0 joint, 1 solver data, 2 positions, 3 velocities, 4 body A, 5 body B.
fn joint_call(rva: u32, joint: Vec<u8>, bodies: [&super::revolute::JointBody; 2], step: &TimeStep, positions: &[Position], velocities: &[Velocity]) -> Call {
    let mut data = vec![0; 40];
    let fields = [step.dt.to_bits(), step.inv_dt.to_bits(), step.dt_ratio.to_bits(), step.velocity_iterations as u32, step.position_iterations as u32, step.warm_starting as u32];
    for (k, v) in fields.iter().enumerate() {
        put(&mut data, 4 * k, *v);
    }
    let (p, v) = island_bytes(positions, velocities);
    let body = |jb: &super::revolute::JointBody| {
        let mut b = vec![0; 200];
        put_vec(&mut b, 0x1c, jb.local_center);
        put(&mut b, 0x94, jb.inv_mass.to_bits());
        put(&mut b, 0xa4, jb.inv_i.to_bits());
        b
    };
    let mut c = call(rva, vec![joint, data, p, v, body(bodies[0]), body(bodies[1])], &["b0", "b1"]);
    c.links = vec![[0, 0x60, 4, 0], [0, 0x68, 5, 0], [1, 0x18, 2, 0], [1, 0x20, 3, 0]];
    c
}

/// Random hinges (limits at, inside, past and equal; motors; static and fixed-rotation sides; axis locks)
/// stepped through init, velocity and position solves; each native call starts from the rewrite's state.
fn compare_revolute(executable: &std::path::Path) -> Result<(), String> {
    use super::revolute::*;
    let mut random = Random(0x5eed_0009);
    let mut calls = Vec::new();
    let mut expected: Vec<(&str, Vec<u8>, Vec<u8>, Vec<u8>)> = Vec::new();
    let axes = [Vec2::new(1.0, 1.0), Vec2::new(1.0, 1.0), Vec2::new(1.0, 1.0), Vec2::new(0.0, 1.0), Vec2::new(1.0, 0.0)];
    for case in 0..1024 {
        let body = |random: &mut Random, index: usize, side: usize| {
            let fixed = (case + side) % 11 == 0;
            JointBody {
                index,
                local_center: random.vec(0.3),
                inv_mass: if fixed { 0.0 } else { random.range(0.2, 4.0) },
                inv_i: if fixed || random.next() % 9 == 0 { 0.0 } else { random.range(0.5, 40.0) },
                axis: axes[random.next() as usize % axes.len()],
            }
        };
        let (ia, ib) = if random.next() % 2 == 0 { (0, 1) } else { (2, 0) };
        let (a, b) = (body(&mut random, ia, 0), body(&mut random, ib, 1));
        let lower = random.range(-2.0, 0.5);
        let def = RevoluteDef {
            local_anchor_a: random.vec(0.5),
            local_anchor_b: random.vec(0.5),
            reference_angle: random.range(-1.0, 1.0),
            enable_limit: random.next() % 3 != 0,
            lower_angle: lower,
            upper_angle: if random.next() % 8 == 0 { lower + random.range(0.0, 0.1) } else { lower + random.range(0.0, 2.5) },
            enable_motor: random.next() % 2 == 0,
            motor_speed: random.range(-10.0, 10.0),
            max_motor_torque: random.range(0.0, 50.0),
        };
        let mut joint = RevoluteJoint::new(def);
        joint.impulse = [random.range(-1.0, 1.0), random.range(-1.0, 1.0), random.range(-0.5, 0.5)];
        joint.motor_impulse = random.range(-0.5, 0.5);
        joint.limit_state = random.next() % 4;
        let mut positions: Vec<Position> = (0..3).map(|_| Position { c: random.vec(1.0), a: random.range(-3.0, 3.0) }).collect();
        let mut velocities: Vec<Velocity> = (0..3).map(|_| Velocity { v: random.vec(5.0), w: random.range(-8.0, 8.0) }).collect();
        let dt = [1.0 / 60.0, 0.02][random.next() as usize % 2];
        let step = TimeStep { dt, inv_dt: 1.0 / dt, dt_ratio: random.range(0.5, 1.5), velocity_iterations: 8, position_iterations: 8, warm_starting: random.next() % 4 != 0 };
        let record = |op, joint: &RevoluteJoint, positions: &[Position], velocities: &[Velocity]| {
            let (p, v) = island_bytes(positions, velocities);
            (op, revolute_bytes(joint), p, v)
        };
        // Init reads the bodies, so its starting joint carries their data too.
        joint.a = a;
        joint.b = b;
        calls.push(joint_call(REVOLUTE_INIT, revolute_bytes(&joint), [&a, &b], &step, &positions, &velocities));
        joint.init_velocity_constraints(a, b, &step, &positions, &mut velocities);
        expected.push(record("init", &joint, &positions, &velocities));
        for _ in 0..3 {
            calls.push(joint_call(REVOLUTE_SOLVE_VELOCITY, revolute_bytes(&joint), [&a, &b], &step, &positions, &velocities));
            joint.solve_velocity_constraints(&mut velocities);
            expected.push(record("velocity", &joint, &positions, &velocities));
        }
        for _ in 0..3 {
            calls.push(joint_call(REVOLUTE_SOLVE_POSITION, revolute_bytes(&joint), [&a, &b], &step, &positions, &velocities));
            let solved = joint.solve_position_constraints(&mut positions);
            let mut r = record("position", &joint, &positions, &velocities);
            r.1.push(solved as u8);
            expected.push(r);
        }
    }
    let count = calls.len();
    let replies: Vec<Reply> = crate::motor::run_probe(executable, &Batch { globals: island_globals(), calls }, count)?;
    let (mut bad, mut first) = (0, None);
    for (i, reply) in replies.iter().enumerate() {
        let (op, joint, p, v) = &expected[i];
        let mut native_joint = unhex(&reply.buffers[0]);
        native_joint[0x60..0x70].fill(0);
        if *op == "position" {
            native_joint.push((reply.ret & 0xff != 0) as u8);
        }
        let native = [native_joint, unhex(&reply.buffers[2]), unhex(&reply.buffers[3])];
        for (k, (n, r)) in native.iter().zip([joint, p, v]).enumerate() {
            if n != r {
                bad += 1;
                let at = n.iter().zip(r).position(|(x, y)| x != y).unwrap_or(n.len().min(r.len()));
                let o = at & !3;
                first.get_or_insert_with(|| format!("call {i} {op}: buffer {k} byte {at:#x} native={:#x} rust={:#x}", get(n, o.min(n.len() - 4)), get(r, o.min(r.len() - 4))));
                break;
            }
        }
    }
    report("REVOLUTE JOINT", bad, replies.len(), first)
}

const DISTANCE_INIT: u32 = 0xbc7440;
const DISTANCE_SOLVE_VELOCITY: u32 = 0xbc9f80;
const DISTANCE_SOLVE_POSITION: u32 = 0xbc9370;

/// b2DistanceJoint (252 bytes).
fn distance_bytes(j: &super::distance_joint::DistanceJoint) -> Vec<u8> {
    let mut b = vec![0; 252];
    put(&mut b, 0x70, j.a.index as u32);
    put(&mut b, 0x74, j.b.index as u32);
    put_vec(&mut b, 0x78, j.a.axis);
    put_vec(&mut b, 0x80, j.b.axis);
    put(&mut b, 0x98, j.def.frequency_hz.to_bits());
    put(&mut b, 0x9c, j.def.damping_ratio.to_bits());
    put(&mut b, 0xa0, j.bias.to_bits());
    put_vec(&mut b, 0xa4, j.def.local_anchor_a);
    put_vec(&mut b, 0xac, j.def.local_anchor_b);
    put(&mut b, 0xb4, j.gamma.to_bits());
    put(&mut b, 0xb8, j.impulse.to_bits());
    put(&mut b, 0xbc, j.def.length.to_bits());
    put_vec(&mut b, 0xc0, j.u);
    put_vec(&mut b, 0xc8, j.r_a);
    put_vec(&mut b, 0xd0, j.r_b);
    put_vec(&mut b, 0xd8, j.a.local_center);
    put_vec(&mut b, 0xe0, j.b.local_center);
    for (k, x) in [j.a.inv_mass, j.b.inv_mass, j.a.inv_i, j.b.inv_i, j.mass].iter().enumerate() {
        put(&mut b, 0xe8 + 4 * k, x.to_bits());
    }
    b
}

/// Random springs (soft and rigid, stretched, slack and coincident anchors; static and fixed-rotation sides;
/// axis locks) stepped through init, velocity and position solves.
fn compare_distance_joint(executable: &std::path::Path) -> Result<(), String> {
    use super::distance_joint::*;
    use super::revolute::JointBody;
    let mut random = Random(0x5eed_000a);
    let mut calls = Vec::new();
    let mut expected: Vec<(&str, Vec<u8>, Vec<u8>, Vec<u8>)> = Vec::new();
    let axes = [Vec2::new(1.0, 1.0), Vec2::new(1.0, 1.0), Vec2::new(1.0, 1.0), Vec2::new(0.0, 1.0), Vec2::new(1.0, 0.0)];
    for case in 0..1024 {
        let body = |random: &mut Random, index: usize, side: usize| {
            let fixed = (case + side) % 11 == 0;
            JointBody {
                index,
                local_center: random.vec(0.3),
                inv_mass: if fixed { 0.0 } else { random.range(0.2, 4.0) },
                inv_i: if fixed || random.next() % 9 == 0 { 0.0 } else { random.range(0.5, 40.0) },
                axis: axes[random.next() as usize % axes.len()],
            }
        };
        let (ia, ib) = if random.next() % 2 == 0 { (0, 1) } else { (2, 0) };
        let (a, b) = (body(&mut random, ia, 0), body(&mut random, ib, 1));
        let mut positions: Vec<Position> = (0..3).map(|_| Position { c: random.vec(1.0), a: random.range(-3.0, 3.0) }).collect();
        let def = DistanceDef {
            local_anchor_a: random.vec(0.5),
            local_anchor_b: random.vec(0.5),
            length: [0.05, 0.005, random.range(0.005, 2.0)][random.next() as usize % 3],
            frequency_hz: [0.0, random.range(0.0, 10.0), random.range(0.0, 10.0)][random.next() as usize % 3],
            damping_ratio: random.range(0.0, 1.0),
        };
        if case % 13 == 0 {
            // Anchors on top of each other: the axis falls back to zero.
            let (qa, qb) = (super::math::Rot::new(positions[ia].a), super::math::Rot::new(positions[ib].a));
            let wa = qa.apply(def.local_anchor_a - a.local_center);
            let rb = qb.apply(def.local_anchor_b - b.local_center);
            positions[ib].c = Vec2::new(positions[ia].c.x + wa.x - rb.x, positions[ia].c.y + wa.y - rb.y);
        }
        let mut joint = DistanceJoint::new(def);
        joint.impulse = random.range(-1.0, 1.0);
        let mut velocities: Vec<Velocity> = (0..3).map(|_| Velocity { v: random.vec(5.0), w: random.range(-8.0, 8.0) }).collect();
        let dt = [1.0 / 60.0, 0.02][random.next() as usize % 2];
        let step = TimeStep { dt, inv_dt: 1.0 / dt, dt_ratio: random.range(0.5, 1.5), velocity_iterations: 8, position_iterations: 8, warm_starting: random.next() % 4 != 0 };
        let record = |op, joint: &DistanceJoint, positions: &[Position], velocities: &[Velocity]| {
            let (p, v) = island_bytes(positions, velocities);
            (op, distance_bytes(joint), p, v)
        };
        joint.a = a;
        joint.b = b;
        calls.push(joint_call(DISTANCE_INIT, distance_bytes(&joint), [&a, &b], &step, &positions, &velocities));
        joint.init_velocity_constraints(a, b, &step, &positions, &mut velocities);
        expected.push(record("init", &joint, &positions, &velocities));
        for _ in 0..3 {
            calls.push(joint_call(DISTANCE_SOLVE_VELOCITY, distance_bytes(&joint), [&a, &b], &step, &positions, &velocities));
            joint.solve_velocity_constraints(&mut velocities);
            expected.push(record("velocity", &joint, &positions, &velocities));
        }
        for _ in 0..3 {
            calls.push(joint_call(DISTANCE_SOLVE_POSITION, distance_bytes(&joint), [&a, &b], &step, &positions, &velocities));
            let solved = joint.solve_position_constraints(&mut positions);
            let mut r = record("position", &joint, &positions, &velocities);
            r.1.push(solved as u8);
            expected.push(r);
        }
    }
    let count = calls.len();
    let replies: Vec<Reply> = crate::motor::run_probe(executable, &Batch { globals: island_globals(), calls }, count)?;
    let (mut bad, mut first) = (0, None);
    for (i, reply) in replies.iter().enumerate() {
        let (op, joint, p, v) = &expected[i];
        let mut native_joint = unhex(&reply.buffers[0]);
        native_joint[0x60..0x70].fill(0);
        if *op == "position" {
            native_joint.push((reply.ret & 0xff != 0) as u8);
        }
        let native = [native_joint, unhex(&reply.buffers[2]), unhex(&reply.buffers[3])];
        for (k, (n, r)) in native.iter().zip([joint, p, v]).enumerate() {
            if n != r {
                bad += 1;
                let at = n.iter().zip(r).position(|(x, y)| x != y).unwrap_or(n.len().min(r.len()));
                let o = at & !3;
                first.get_or_insert_with(|| format!("call {i} {op}: buffer {k} byte {at:#x} native={:#x} rust={:#x}", get(n, o.min(n.len() - 4)), get(r, o.min(r.len() - 4))));
                break;
            }
        }
    }
    report("DISTANCE JOINT", bad, replies.len(), first)
}

const WELD_INIT: u32 = 0xbc34b0;
const WELD_SOLVE_VELOCITY: u32 = 0xbc6590;
const WELD_SOLVE_POSITION: u32 = 0xbc4e00;

/// b2WeldJoint (0x120 bytes).
fn weld_bytes(j: &super::weld::WeldJoint) -> Vec<u8> {
    let mut b = vec![0; 0x120];
    put(&mut b, 0x70, j.a.index as u32);
    put(&mut b, 0x74, j.b.index as u32);
    put_vec(&mut b, 0x78, j.a.axis);
    put_vec(&mut b, 0x80, j.b.axis);
    put(&mut b, 0x98, j.def.frequency_hz.to_bits());
    put(&mut b, 0x9c, j.def.damping_ratio.to_bits());
    put(&mut b, 0xa0, j.bias.to_bits());
    put_vec(&mut b, 0xa4, j.def.local_anchor_a);
    put_vec(&mut b, 0xac, j.def.local_anchor_b);
    put(&mut b, 0xb4, j.def.reference_angle.to_bits());
    put(&mut b, 0xb8, j.gamma.to_bits());
    for (k, x) in j.impulse.iter().enumerate() {
        put(&mut b, 0xbc + 4 * k, x.to_bits());
    }
    put_vec(&mut b, 0xc8, j.r_a);
    put_vec(&mut b, 0xd0, j.r_b);
    put_vec(&mut b, 0xd8, j.a.local_center);
    put_vec(&mut b, 0xe0, j.b.local_center);
    for (k, x) in [j.a.inv_mass, j.b.inv_mass, j.a.inv_i, j.b.inv_i].iter().enumerate() {
        put(&mut b, 0xe8 + 4 * k, x.to_bits());
    }
    for (k, x) in j.mass.iter().enumerate() {
        put(&mut b, 0xf8 + 4 * k, x.to_bits());
    }
    b
}

/// Random welds (rigid and soft; static, fixed-rotation and axis-locked sides) stepped through init, velocity and
/// position solves; each native call starts from the rewrite's state.
fn compare_weld(executable: &std::path::Path) -> Result<(), String> {
    use super::revolute::JointBody;
    use super::weld::*;
    let mut random = Random(0x5eed_000b);
    let mut calls = Vec::new();
    let mut expected: Vec<(&str, Vec<u8>, Vec<u8>, Vec<u8>)> = Vec::new();
    let axes = [Vec2::new(1.0, 1.0), Vec2::new(1.0, 1.0), Vec2::new(1.0, 1.0), Vec2::new(0.0, 1.0), Vec2::new(1.0, 0.0)];
    for case in 0..1024 {
        let body = |random: &mut Random, index: usize, side: usize| {
            let fixed = (case + side) % 11 == 0;
            JointBody {
                index,
                local_center: random.vec(0.3),
                inv_mass: if fixed { 0.0 } else { random.range(0.2, 4.0) },
                // Zero inertia on both sides exercises the K.ez.z == 0 (2x2) paths.
                inv_i: if fixed || random.next() % 9 == 0 { 0.0 } else { random.range(0.5, 40.0) },
                axis: axes[random.next() as usize % axes.len()],
            }
        };
        let (ia, ib) = if random.next() % 2 == 0 { (0, 1) } else { (2, 0) };
        let (a, b) = (body(&mut random, ia, 0), body(&mut random, ib, 1));
        let positions: Vec<Position> = (0..3).map(|_| Position { c: random.vec(1.0), a: random.range(-3.0, 3.0) }).collect();
        let mut positions = positions;
        let def = WeldDef {
            local_anchor_a: random.vec(0.5),
            local_anchor_b: random.vec(0.5),
            reference_angle: random.range(-3.0, 3.0),
            frequency_hz: [0.0, 0.0, random.range(0.0, 20.0), 8.0][random.next() as usize % 4],
            damping_ratio: [0.0, random.range(0.0, 1.0)][random.next() as usize % 2],
        };
        if case % 7 == 0 {
            // Nearly satisfied, so the final slop comparisons and tiny errors are exercised.
            let (qa, qb) = (super::math::Rot::new(positions[ia].a), super::math::Rot::new(positions[ib].a));
            let wa = qa.apply(def.local_anchor_a - a.local_center);
            let rb = qb.apply(def.local_anchor_b - b.local_center);
            let jitter = random.range(-0.004, 0.004);
            positions[ib].c = Vec2::new(positions[ia].c.x + wa.x - rb.x + jitter, positions[ia].c.y + wa.y - rb.y);
            positions[ib].a = positions[ia].a + def.reference_angle + random.range(-0.05, 0.05);
        }
        let mut joint = WeldJoint::new(def);
        joint.impulse = [random.range(-1.0, 1.0), random.range(-1.0, 1.0), random.range(-1.0, 1.0)];
        let mut velocities: Vec<Velocity> = (0..3).map(|_| Velocity { v: random.vec(5.0), w: random.range(-8.0, 8.0) }).collect();
        let dt = [1.0 / 60.0, 0.02][random.next() as usize % 2];
        let step = TimeStep { dt, inv_dt: 1.0 / dt, dt_ratio: random.range(0.5, 1.5), velocity_iterations: 8, position_iterations: 8, warm_starting: random.next() % 4 != 0 };
        let record = |op, joint: &WeldJoint, positions: &[Position], velocities: &[Velocity]| {
            let (p, v) = island_bytes(positions, velocities);
            (op, weld_bytes(joint), p, v)
        };
        joint.a = a;
        joint.b = b;
        calls.push(joint_call(WELD_INIT, weld_bytes(&joint), [&a, &b], &step, &positions, &velocities));
        joint.init_velocity_constraints(a, b, &step, &positions, &mut velocities);
        expected.push(record("init", &joint, &positions, &velocities));
        for _ in 0..3 {
            calls.push(joint_call(WELD_SOLVE_VELOCITY, weld_bytes(&joint), [&a, &b], &step, &positions, &velocities));
            joint.solve_velocity_constraints(&mut velocities);
            expected.push(record("velocity", &joint, &positions, &velocities));
        }
        for _ in 0..3 {
            calls.push(joint_call(WELD_SOLVE_POSITION, weld_bytes(&joint), [&a, &b], &step, &positions, &velocities));
            let solved = joint.solve_position_constraints(&mut positions);
            let mut r = record("position", &joint, &positions, &velocities);
            r.1.push(solved as u8);
            expected.push(r);
        }
    }
    let count = calls.len();
    let replies: Vec<Reply> = crate::motor::run_probe(executable, &Batch { globals: island_globals(), calls }, count)?;
    let (mut bad, mut first) = (0, None);
    for (i, reply) in replies.iter().enumerate() {
        let (op, joint, p, v) = &expected[i];
        let mut native_joint = unhex(&reply.buffers[0]);
        native_joint[0x60..0x70].fill(0);
        if *op == "position" {
            native_joint.push((reply.ret & 0xff != 0) as u8);
        }
        let native = [native_joint, unhex(&reply.buffers[2]), unhex(&reply.buffers[3])];
        for (k, (n, r)) in native.iter().zip([joint, p, v]).enumerate() {
            if n != r {
                bad += 1;
                let at = n.iter().zip(r).position(|(x, y)| x != y).unwrap_or(n.len().min(r.len()));
                let o = at & !3;
                first.get_or_insert_with(|| format!("call {i} {op}: buffer {k} byte {at:#x} native={:#x} rust={:#x}", get(n, o.min(n.len() - 4)), get(r, o.min(r.len() - 4))));
                break;
            }
        }
    }
    report("WELD JOINT", bad, replies.len(), first)
}
