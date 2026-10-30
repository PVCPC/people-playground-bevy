//! Ray casts as UnityPlayer.dll compiles them: b2World::RayCast over b2DynamicTree::RayCast and each shape's RayCast
//! (b2PolygonShape 180bb47f0, b2CircleShape 180bb4520, b2EdgeShape 180bb46d0, b2ChainShape 180bb4380,
//! b2CapsuleShape 180bb3f40, b2TestSegmentSegment 180bb1a30, b2RaycastSegment 180bb1850).

use super::broadphase::{Aabb, DynamicTree, NULL};
use super::math::{Transform, Vec2};
use super::world::{FixtureId, Shape, World};

#[derive(Clone, Copy, Debug)]
pub struct RayInput {
    pub p1: Vec2,
    pub p2: Vec2,
    pub max_fraction: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct RayOutput {
    pub normal: Vec2,
    pub fraction: f32,
}

/// The 0.01 the shapes' RayCasts compare their radius with (b2_polygonRadius as Physics2DSettings sets it).
const POLYGON_RADIUS: f32 = f32::from_bits(0x3c23_d70a);

/// b2Vec2::Normalize applied to `v`.
fn normalize(v: Vec2) -> Vec2 {
    v.normalized()
}

/// b2RaycastSegment (180bb1850): the ray against the segment a-b, with Unity's tolerances.
fn raycast_segment(input: &RayInput, a: Vec2, b: Vec2) -> Option<RayOutput> {
    let (rx, ry) = (input.p2.x - input.p1.x, input.p2.y - input.p1.y);
    let (ey, ex) = (b.y - a.y, (b.x - a.x) * -1.0);
    let den = -(ex * ry + rx * ey);
    if den <= f32::from_bits(0x37480000) {
        return None;
    }
    let (qx, qy) = (input.p1.x - a.x, input.p1.y - a.y);
    let t = ex * qy + qx * ey;
    if t < 0.0 || den * input.max_fraction < t {
        return None;
    }
    let s = ry * qx - rx * qy;
    if s < den * f32::from_bits(0xb7480000) || den * f32::from_bits(0x3f800064) < s {
        return None;
    }
    let normal = normalize(Vec2::new(ey, ex));
    Some(RayOutput { normal, fraction: t / den })
}

/// b2TestSegmentSegment (180bb1a30): the segment p1-p2 against q1-q2. The normal comes out in the caller's frame.
fn test_segment_segment(p1: Vec2, p2: Vec2, q1: Vec2, q2: Vec2) -> Option<RayOutput> {
    let a = (p1.x - q2.x) * (p2.y - q2.y) - (p2.x - q2.x) * (p1.y - q2.y);
    let b = (p1.x - q1.x) * (p2.y - q1.y) - (p2.x - q1.x) * (p1.y - q1.y);
    if 0.0 < b * a {
        return None;
    }
    let c = (q1.x - p1.x) * (q2.y - p1.y) - (q1.y - p1.y) * (q2.x - p1.x);
    let d = c + b - a;
    if 0.0 <= d * c {
        return None;
    }
    let fraction = c / (c - d);
    let (vx, vy) = (-(q2.x - q1.x), q2.y - q1.y);
    let mut n = Vec2::new(vy, vx);
    let length = (vy * vy + vx * vx).sqrt();
    if !(f32::EPSILON > length) {
        let inv = 1.0 / length;
        n = Vec2::new(vy * inv, vx * inv);
    }
    let side = (q1.x - p1.x) * n.x + (q1.y - p1.y) * n.y;
    let normal = if side > 0.0 { Vec2::new(-n.x, -n.y) } else { n };
    Some(RayOutput { normal, fraction })
}

/// b2CapsuleShape::RayCast (180bb3f40): the two side segments, then the end circles nearer end first.
fn capsule_ray_cast(radius: f32, vertices: [Vec2; 2], input: &RayInput, xf: &Transform) -> Option<RayOutput> {
    let capsule = super::shape::Capsule { radius, vertices };
    if capsule.test_point(xf, input.p1) {
        return None;
    }
    let (c, s) = (xf.q.c, xf.q.s);
    let w = |v: Vec2| Vec2::new((v.x * c - s * v.y) + xf.p.x, (s * v.x + v.y * c) + xf.p.y);
    let (p0, p1) = (w(vertices[0]), w(vertices[1]));
    let r2 = radius * radius;
    let (dx, dy) = (p1.x - p0.x, p1.y - p0.y);
    let k = (p0.y - input.p1.y) * dx - (p0.x - input.p1.x) * dy;
    let l2 = dy * dy + dx * dx;
    if !(r2 * l2 >= k * k) {
        let scale = radius / l2.sqrt();
        let (nx, ny) = (dy * scale, -(dx * scale));
        let (a, b) = if 0.0 > k {
            (Vec2::new(p1.x - nx, p1.y - ny), Vec2::new(p0.x - nx, p0.y - ny))
        } else {
            (Vec2::new(nx + p0.x, ny + p0.y), Vec2::new(nx + p1.x, ny + p1.y))
        };
        if let Some(out) = raycast_segment(input, a, b) {
            return Some(out);
        }
    }
    let (rx, ry) = (input.p2.x - input.p1.x, input.p2.y - input.p1.y);
    let rr = ry * ry + rx * rx;
    if f32::EPSILON > rr {
        return None;
    }
    let near0 = {
        let d1 = (p1.y - input.p1.y) * (p1.y - input.p1.y) + (p1.x - input.p1.x) * (p1.x - input.p1.x);
        let d0 = (p0.y - input.p1.y) * (p0.y - input.p1.y) + (p0.x - input.p1.x) * (p0.x - input.p1.x);
        // comiss + cmovbe (180bb4230): an unordered compare (a NaN) picks p0 first.
        !(d0 > d1)
    };
    let centers = if near0 { [p0, p1] } else { [p1, p0] };
    for center in centers {
        let (sx, sy) = (input.p1.x - center.x, input.p1.y - center.y);
        let c = sy * ry + sx * rx;
        let b = (sy * sy + sx * sx) - r2;
        let b = if 0.0 > b { 0.0 } else { b };
        let sigma = c * c - b * rr;
        if 0.0 > sigma {
            continue;
        }
        let a = -(sigma.sqrt() + c);
        // comiss + jb (180bb42b0) and comiss + jae (180bb42be): an unordered compare rejects.
        if !(a >= 0.0) || !(rr * input.max_fraction >= a) {
            continue;
        }
        let a = a / rr;
        let normal = normalize(Vec2::new(rx * a + sx, ry * a + sy));
        return Some(RayOutput { normal, fraction: a });
    }
    None
}

/// b2EdgeShape::RayCast (180bb46d0) on the segment v1-v2 of `radius`.
fn edge_ray_cast(radius: f32, v1: Vec2, v2: Vec2, input: &RayInput, xf: &Transform) -> Option<RayOutput> {
    if POLYGON_RADIUS < radius {
        return capsule_ray_cast(radius - POLYGON_RADIUS, [v1, v2], input, xf);
    }
    test_segment_segment(xf.apply_t(input.p1), xf.apply_t(input.p2), v1, v2)
}

impl Shape {
    /// Shape::RayCast(output, input, xf, child).
    pub fn ray_cast(&self, input: &RayInput, xf: &Transform, child: usize) -> Option<RayOutput> {
        match self {
            Shape::Circle(circle) => {
                let (c, s) = (xf.q.c, xf.q.s);
                let pos = Vec2::new((circle.p.x * c - s * circle.p.y) + xf.p.x, (s * circle.p.x + circle.p.y * c) + xf.p.y);
                let (sx, sy) = (input.p1.x - pos.x, input.p1.y - pos.y);
                let b = (sx * sx + sy * sy) - circle.radius * circle.radius;
                let (rx, ry) = (input.p2.x - input.p1.x, input.p2.y - input.p1.y);
                let cc = sx * rx + sy * ry;
                let rr = rx * rx + ry * ry;
                let sigma = cc * cc - b * rr;
                if 0.0 > sigma || f32::EPSILON > rr {
                    return None;
                }
                let a = -(sigma.sqrt() + cc);
                // comiss + jb (180bb464f, 180bb465d): an unordered compare (a NaN from an infinite rr) rejects.
                if !(a >= 0.0) || !(rr * input.max_fraction >= a) {
                    return None;
                }
                let a = a / rr;
                Some(RayOutput { normal: normalize(Vec2::new(sx + rx * a, sy + ry * a)), fraction: a })
            }
            Shape::Polygon(poly) => {
                let p1 = xf.apply_t(input.p1);
                let p2 = xf.apply_t(input.p2);
                let d = Vec2::new(p2.x - p1.x, p2.y - p1.y);
                if POLYGON_RADIUS < poly.radius {
                    // A capsule of radius - b2_polygonRadius along each edge not facing away from the ray; a hit
                    // nearer than the best so far becomes the best and caps the next casts' maxFraction.
                    let mut input = *input;
                    let mut best = None;
                    for i in 0..poly.count {
                        let n = poly.normals[i];
                        if d.x * n.x + d.y * n.y > 0.0 {
                            continue;
                        }
                        let edge = [poly.vertices[i], poly.vertices[(i + 1) % poly.count]];
                        if let Some(out) = capsule_ray_cast(poly.radius - POLYGON_RADIUS, edge, &input, xf) {
                            if input.max_fraction > out.fraction {
                                input.max_fraction = out.fraction;
                                best = Some(out);
                            } else if best.is_none() {
                                // The output keeps the last cast's values only until the best is written back,
                                // which is the input's maxFraction and an uninitialised normal.
                                unimplemented!("b2PolygonShape::RayCast: a rounded edge hit at exactly maxFraction");
                            }
                        }
                    }
                    return best;
                }
                let (mut lower, mut upper) = (0.0f32, input.max_fraction);
                let mut index: i32 = -1;
                for i in 0..poly.count {
                    let n = poly.normals[i];
                    let v = poly.vertices[i];
                    let numerator = n.x * (v.x - p1.x) + n.y * (v.y - p1.y);
                    let denominator = n.x * d.x + n.y * d.y;
                    if denominator == 0.0 {
                        if numerator < 0.0 {
                            return None;
                        }
                    } else if denominator < 0.0 && numerator < lower * denominator {
                        lower = numerator / denominator;
                        index = i as i32;
                    } else if denominator > 0.0 && numerator < upper * denominator {
                        upper = numerator / denominator;
                    }
                    if lower > upper {
                        return None;
                    }
                }
                if index < 0 {
                    return None;
                }
                let n = poly.normals[index as usize];
                Some(RayOutput { normal: Vec2::new(n.x * xf.q.c - xf.q.s * n.y, xf.q.s * n.x + n.y * xf.q.c), fraction: lower })
            }
            Shape::Capsule(c) => capsule_ray_cast(c.radius, c.vertices, input, xf),
            Shape::Chain(chain) => edge_ray_cast(chain.radius, chain.vertices[child], chain.vertices[child + 1], input, xf),
        }
    }
}

impl DynamicTree {
    /// b2DynamicTree::RayCast<b2WorldRayCastWrapper> (180bb7520): leaves along the ray, child2 subtrees first;
    /// `callback(proxy, max_fraction)` returns 0 to stop, a positive value to clip the ray, otherwise carries on.
    pub fn ray_cast(&self, input: &RayInput, mut callback: impl FnMut(i32, f32) -> f32) {
        let (p1, p2) = (input.p1, input.p2);
        let d = Vec2::new(p2.x - p1.x, p2.y - p1.y);
        let mut r = d;
        let length = (r.y * r.y + r.x * r.x).sqrt();
        if !(f32::EPSILON > length) {
            let inv = 1.0 / length;
            r = Vec2::new(r.x * inv, r.y * inv);
        }
        let v = Vec2::new(r.y * -1.0, r.x);
        let abs_v = Vec2::new(v.x.abs(), v.y.abs());
        let mut max_fraction = input.max_fraction;
        let segment = |max_fraction: f32| {
            let t = Vec2::new(d.x * max_fraction + p1.x, d.y * max_fraction + p1.y);
            let lo = |a: f32, b: f32| if a < b { a } else { b };
            let hi = |a: f32, b: f32| if a > b { a } else { b };
            Aabb { lower: Vec2::new(lo(p1.x, t.x), lo(p1.y, t.y)), upper: Vec2::new(hi(p1.x, t.x), hi(p1.y, t.y)) }
        };
        let mut seg = segment(max_fraction);
        let mut stack = vec![self.root];
        while let Some(id) = stack.pop() {
            if id == NULL {
                continue;
            }
            let node = &self.nodes[id as usize];
            if !seg.overlaps(&node.aabb) {
                continue;
            }
            let c = Vec2::new((node.aabb.lower.x + node.aabb.upper.x) * 0.5, (node.aabb.lower.y + node.aabb.upper.y) * 0.5);
            let h = Vec2::new((node.aabb.upper.x - node.aabb.lower.x) * 0.5, (node.aabb.upper.y - node.aabb.lower.y) * 0.5);
            let separation = (v.x * (p1.x - c.x) + v.y * (p1.y - c.y)).abs() - (abs_v.y * h.y + abs_v.x * h.x);
            if separation > 0.0 {
                continue;
            }
            if node.child1 == NULL {
                let value = callback(id, max_fraction);
                if value == 0.0 {
                    return;
                }
                if value > 0.0 {
                    max_fraction = value;
                    seg = segment(max_fraction);
                }
            } else {
                stack.push(node.child1);
                stack.push(node.child2);
            }
        }
    }
}

/// A fixture the ray crossed: (fixture, child, point, normal, fraction).
pub type RayHit = (FixtureId, usize, Vec2, Vec2, f32);

impl World {
    /// b2World::RayCast (180bb9c80) from p1 to p2: each fixture the ray crosses goes through `report`, whose
    /// return value (b2RayCastCallback::ReportFixture) steers the cast.
    pub fn ray_cast(&self, p1: Vec2, p2: Vec2, mut report: impl FnMut(RayHit) -> f32) {
        let input = RayInput { p1, p2, max_fraction: 1.0 };
        let tree = &self.broad_phase.tree;
        tree.ray_cast(&input, |proxy, max_fraction| {
            let (f, child) = super::world::proxy_owner(tree.user_data(proxy));
            let fx = self.fixture(f);
            let sub = RayInput { p1, p2, max_fraction };
            match fx.shape.ray_cast(&sub, &self.body(fx.body).xf, child) {
                Some(out) => {
                    let fraction = out.fraction;
                    let inv = 1.0 - fraction;
                    let point = Vec2::new(p2.x * fraction + p1.x * inv, p2.y * fraction + p1.y * inv);
                    report((f, child, point, out.normal, fraction))
                }
                None => max_fraction,
            }
        });
    }
}
