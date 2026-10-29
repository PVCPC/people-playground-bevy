//! Bit comparisons of the b2 port against UnityPlayer.dll through NativeProbe's raw-call mode.

use super::collide::*;
use super::math::{Transform, Vec2};
use super::shape::*;
use serde::{Deserialize, Serialize};

const SET: u32 = 0xbb4b90;
const COMPUTE_MASS: u32 = 0xbb35a0;
const COLLIDE_CIRCLES: u32 = 0xbaeff0;
const COLLIDE_POLYGON_AND_CIRCLE: u32 = 0xbaf500;
const COLLIDE_POLYGONS: u32 = 0xbaf8f0;
const COLLIDE_RADIAL_POLYGONS: u32 = 0xbaffe0;
const WORLD_MANIFOLD: u32 = 0xbadb50;

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
}

#[derive(Deserialize)]
struct Reply {
    buffers: Vec<String>,
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
    Call { rva, buffers: buffers.iter().map(|b| hex(b)).collect(), args: args.iter().map(|a| a.to_string()).collect(), returns_float: false }
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

pub fn compare_reference(executable: &std::path::Path) -> Result<(), String> {
    compare_shapes(executable)?;
    compare_manifolds(executable)
}
