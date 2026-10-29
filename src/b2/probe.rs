//! Bit comparisons of the b2 port against UnityPlayer.dll through NativeProbe's raw-call mode.

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

pub fn compare_reference(executable: &std::path::Path) -> Result<(), String> {
    compare_shapes(executable)?;
    compare_manifolds(executable)?;
    compare_solver(executable)?;
    compare_distance(executable)?;
    compare_toi(executable)?;
    compare_broad_phase(executable)
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

/// Random create/move/destroy/query sequences; each native call starts from the port's state before the op.
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
