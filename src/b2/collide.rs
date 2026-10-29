//! Narrow-phase manifolds: Box2D 2.3 plus Unity's rounded-polygon path, in UnityPlayer.dll's operand grouping.

use super::math::{dot, Transform, Vec2};
use super::shape::{Circle, Polygon};

const LINEAR_SLOP_TOLERANCE: f32 = 0.1 * 0.005;

pub const FEATURE_VERTEX: u8 = 0;
pub const FEATURE_FACE: u8 = 1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContactFeature {
    pub index_a: u8,
    pub index_b: u8,
    pub type_a: u8,
    pub type_b: u8,
}

impl ContactFeature {
    pub fn key(self) -> u32 {
        u32::from_le_bytes([self.index_a, self.index_b, self.type_a, self.type_b])
    }

    fn swapped(self) -> Self {
        Self { index_a: self.index_b, index_b: self.index_a, type_a: self.type_b, type_b: self.type_a }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ManifoldPoint {
    pub local_point: Vec2,
    pub normal_impulse: f32,
    pub tangent_impulse: f32,
    pub id: ContactFeature,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ManifoldType {
    #[default]
    Circles,
    FaceA,
    FaceB,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Manifold {
    pub points: [ManifoldPoint; 2],
    pub local_normal: Vec2,
    pub local_point: Vec2,
    pub kind: ManifoldType,
    pub point_count: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct ClipVertex {
    v: Vec2,
    id: ContactFeature,
}

/// b2CollideCircles (180baeff0).
pub fn collide_circles(manifold: &mut Manifold, a: &Circle, xf_a: &Transform, b: &Circle, xf_b: &Transform) {
    manifold.point_count = 0;
    let d = xf_b.apply(b.p) - xf_a.apply(a.p);
    let radius = a.radius + b.radius;
    if d.length_squared() > radius * radius {
        return;
    }
    manifold.kind = ManifoldType::Circles;
    manifold.local_point = a.p;
    manifold.local_normal = Vec2::ZERO;
    manifold.point_count = 1;
    manifold.points[0].local_point = b.p;
    manifold.points[0].id = ContactFeature::default();
}

/// b2CollidePolygonAndCircle (180baf500) with Unity's extra flag false, as b2PolygonAndCircleContact passes it.
pub fn collide_polygon_and_circle(manifold: &mut Manifold, a: &Polygon, xf_a: &Transform, b: &Circle, xf_b: &Transform) {
    manifold.point_count = 0;
    let c_local = xf_a.apply_t(xf_b.apply(b.p));
    let radius = a.radius + b.radius;
    let mut normal_index = 0;
    let mut separation = -f32::MAX;
    for i in 0..a.count {
        let s = dot(a.normals[i], c_local - a.vertices[i]);
        if s > radius {
            return;
        }
        if s > separation {
            separation = s;
            normal_index = i;
        }
    }
    let v1 = a.vertices[normal_index];
    let v2 = a.vertices[a.next(normal_index)];
    let mut finish = |normal: Vec2, point: Vec2| {
        manifold.point_count = 1;
        manifold.kind = ManifoldType::FaceA;
        manifold.local_normal = normal;
        manifold.local_point = point;
        manifold.points[0].local_point = b.p;
        manifold.points[0].id = ContactFeature::default();
    };
    if separation < f32::EPSILON {
        return finish(a.normals[normal_index], Vec2::new((v1.x + v2.x) * 0.5, (v1.y + v2.y) * 0.5));
    }
    let u1 = dot(c_local - v1, v2 - v1);
    let u2 = dot(c_local - v2, v1 - v2);
    if u1 <= 0.0 {
        if (c_local - v1).length_squared() > radius * radius {
            return;
        }
        finish((c_local - v1).normalized(), v1);
    } else if u2 <= 0.0 {
        if (c_local - v2).length_squared() > radius * radius {
            return;
        }
        finish((c_local - v2).normalized(), v2);
    } else {
        let center = Vec2::new((v1.x + v2.x) * 0.5, (v1.y + v2.y) * 0.5);
        if dot(c_local - center, a.normals[normal_index]) > radius {
            return;
        }
        finish(a.normals[normal_index], center);
    }
}

/// b2FindMaxSeparation (180bb14b0): the face of `a` with the deepest separation from `b`'s vertices.
fn find_max_separation(a: &Polygon, xf_a: &Transform, b: &Polygon, xf_b: &Transform) -> (usize, f32) {
    let s = xf_a.q.s * xf_b.q.c - xf_b.q.s * xf_a.q.c;
    let c = xf_b.q.s * xf_a.q.s + xf_a.q.c * xf_b.q.c;
    let d = xf_a.p - xf_b.p;
    let p = Vec2::new(xf_b.q.s * d.y + xf_b.q.c * d.x, xf_b.q.c * d.y - xf_b.q.s * d.x);
    let mut best = 0;
    let mut max_separation = -f32::MAX;
    for i in 0..a.count {
        let (nv, vv) = (a.normals[i], a.vertices[i]);
        let n = Vec2::new(c * nv.x - s * nv.y, s * nv.x + c * nv.y);
        let v1 = Vec2::new((c * vv.x - s * vv.y) + p.x, (s * vv.x + c * vv.y) + p.y);
        let mut si = f32::MAX;
        for j in 0..b.count {
            let sij = dot(n, b.vertices[j] - v1);
            if sij < si {
                si = sij;
            }
        }
        if si > max_separation {
            max_separation = si;
            best = i;
        }
    }
    (best, max_separation)
}

/// b2FindIncidentEdge (180bb1210).
fn find_incident_edge(a: &Polygon, xf_a: &Transform, edge: usize, b: &Polygon, xf_b: &Transform) -> [ClipVertex; 2] {
    let normal = xf_b.q.apply_t(xf_a.q.apply(a.normals[edge]));
    let mut index = 0;
    let mut min_dot = f32::MAX;
    for i in 0..b.count {
        let d = dot(normal, b.normals[i]);
        if d < min_dot {
            min_dot = d;
            index = i;
        }
    }
    let next = b.next(index);
    let feature = |index_b: usize| ContactFeature {
        index_a: edge as u8,
        index_b: index_b as u8,
        type_a: FEATURE_FACE,
        type_b: FEATURE_VERTEX,
    };
    [
        ClipVertex { v: xf_b.apply(b.vertices[index]), id: feature(index) },
        ClipVertex { v: xf_b.apply(b.vertices[next]), id: feature(next) },
    ]
}

/// b2ClipSegmentToLine, inlined in both polygon colliders.
fn clip_segment_to_line(v_in: [ClipVertex; 2], normal: Vec2, offset: f32, vertex_index_a: usize) -> ([ClipVertex; 2], usize) {
    let mut v_out = [ClipVertex::default(); 2];
    let mut count = 0;
    let distance0 = dot(normal, v_in[0].v) - offset;
    let distance1 = dot(normal, v_in[1].v) - offset;
    if distance0 <= 0.0 {
        v_out[count] = v_in[0];
        count += 1;
    }
    if distance1 <= 0.0 {
        v_out[count] = v_in[1];
        count += 1;
    }
    if distance0 * distance1 < 0.0 {
        let interp = distance0 / (distance0 - distance1);
        v_out[count] = ClipVertex {
            v: Vec2::new(
                (v_in[1].v.x - v_in[0].v.x) * interp + v_in[0].v.x,
                (v_in[1].v.y - v_in[0].v.y) * interp + v_in[0].v.y,
            ),
            id: ContactFeature {
                index_a: vertex_index_a as u8,
                index_b: v_in[0].id.index_b,
                type_a: FEATURE_VERTEX,
                type_b: FEATURE_FACE,
            },
        };
        count += 1;
    }
    (v_out, count)
}

/// Shared by b2CollidePolygons (180baf8f0) and b2CollideRadialPolygons (180baffe0); `rounded` selects the latter, whose
/// side planes ignore the radius and which falls back to the closest vertex pair when clipping leaves no point.
fn collide_polygon_pair(
    manifold: &mut Manifold,
    poly_a: &Polygon,
    xf_a: &Transform,
    poly_b: &Polygon,
    xf_b: &Transform,
    rounded: bool,
) {
    manifold.point_count = 0;
    let total_radius = poly_a.radius + poly_b.radius;
    let (edge_a, separation_a) = find_max_separation(poly_a, xf_a, poly_b, xf_b);
    if separation_a > total_radius {
        return;
    }
    let (edge_b, separation_b) = find_max_separation(poly_b, xf_b, poly_a, xf_a);
    if separation_b > total_radius {
        return;
    }
    let flip = separation_b > separation_a + LINEAR_SLOP_TOLERANCE;
    let (poly1, xf1, poly2, xf2, edge1) =
        if flip { (poly_b, xf_b, poly_a, xf_a, edge_b) } else { (poly_a, xf_a, poly_b, xf_b, edge_a) };
    manifold.kind = if flip { ManifoldType::FaceB } else { ManifoldType::FaceA };
    let incident = find_incident_edge(poly1, xf1, edge1, poly2, xf2);
    let iv2 = poly1.next(edge1);
    let v11 = poly1.vertices[edge1];
    let v12 = poly1.vertices[iv2];
    let mut local_tangent = v12 - v11;
    let length = local_tangent.length();
    if length >= f32::EPSILON {
        local_tangent = local_tangent * (1.0 / length);
    }
    let local_normal = Vec2::new(local_tangent.y, local_tangent.x * -1.0);
    let plane_point = Vec2::new((v12.x + v11.x) * 0.5, (v12.y + v11.y) * 0.5);
    let tangent = xf1.q.apply(local_tangent);
    let normal = Vec2::new(tangent.y, tangent.x * -1.0);
    let w11 = xf1.apply(v11);
    let w12 = xf1.apply(v12);
    let front_offset = dot(normal, w11);
    let side_radius = if rounded { 0.0 } else { total_radius };
    let side_offset1 = side_radius - dot(tangent, w11);
    let side_offset2 = dot(tangent, w12) + side_radius;

    let (clip1, count1) = clip_segment_to_line(incident, -tangent, side_offset1, edge1);
    if count1 >= 2 {
        let (clip2, count2) = clip_segment_to_line(clip1, tangent, side_offset2, iv2);
        if count2 >= 2 {
            manifold.local_normal = local_normal;
            manifold.local_point = plane_point;
            let mut count = 0;
            for clip in clip2 {
                if dot(normal, clip.v) - front_offset <= total_radius {
                    let point = &mut manifold.points[count];
                    point.local_point = xf2.apply_t(clip.v);
                    point.id = if flip { clip.id.swapped() } else { clip.id };
                    count += 1;
                }
            }
            manifold.point_count = count;
            if !rounded || count != 0 {
                return;
            }
        }
    }
    if !rounded {
        return;
    }

    let d0_11 = (incident[0].v - w11).length_squared();
    let d0_12 = (incident[0].v - w12).length_squared();
    let d1_11 = (incident[1].v - w11).length_squared();
    let d1_12 = (incident[1].v - w12).length_squared();
    let nearest_11 = if d1_11 <= d0_11 { d1_11 } else { d0_11 };
    let nearest_12 = if d1_12 <= d0_12 { d1_12 } else { d0_12 };
    let (reference, use_incident1) = if nearest_12 <= nearest_11 {
        if total_radius * total_radius < nearest_12 {
            return;
        }
        (w12, d1_12 <= d0_12)
    } else {
        if total_radius * total_radius < nearest_11 {
            return;
        }
        (w11, d1_11 <= d0_11)
    };
    let local1 = xf1.apply_t(reference);
    let local2 = xf2.apply_t(incident[usize::from(use_incident1)].v);
    let (local_a, local_b) = if flip { (local2, local1) } else { (local1, local2) };
    manifold.local_point = local_a;
    manifold.kind = ManifoldType::Circles;
    manifold.local_normal = Vec2::ZERO;
    manifold.point_count = 1;
    manifold.points[0].local_point = local_b;
    manifold.points[0].id = ContactFeature::default();
}

pub fn collide_polygons(manifold: &mut Manifold, a: &Polygon, xf_a: &Transform, b: &Polygon, xf_b: &Transform) {
    collide_polygon_pair(manifold, a, xf_a, b, xf_b, false);
}

pub fn collide_radial_polygons(manifold: &mut Manifold, a: &Polygon, xf_a: &Transform, b: &Polygon, xf_b: &Transform) {
    collide_polygon_pair(manifold, a, xf_a, b, xf_b, true);
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WorldManifold {
    pub normal: Vec2,
    pub points: [Vec2; 2],
    pub separations: [f32; 2],
}

impl WorldManifold {
    /// b2WorldManifold::Initialize (180badb50).
    pub fn new(manifold: &Manifold, xf_a: &Transform, radius_a: f32, xf_b: &Transform, radius_b: f32) -> Self {
        let mut world = Self::default();
        if manifold.point_count == 0 {
            return world;
        }
        let midpoint = |a: Vec2, b: Vec2| Vec2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
        match manifold.kind {
            ManifoldType::Circles => {
                world.normal = Vec2::new(1.0, 0.0);
                let point_a = xf_a.apply(manifold.local_point);
                let point_b = xf_b.apply(manifold.points[0].local_point);
                if (point_a - point_b).length_squared() > f32::EPSILON * f32::EPSILON {
                    world.normal = point_b - point_a;
                    world.normal.normalize();
                }
                let c_a = point_a + world.normal * radius_a;
                let c_b = point_b - world.normal * radius_b;
                world.points[0] = midpoint(c_a, c_b);
                world.separations[0] = dot(c_b - c_a, world.normal);
            }
            ManifoldType::FaceA => {
                world.normal = xf_a.q.apply(manifold.local_normal);
                let plane_point = xf_a.apply(manifold.local_point);
                for i in 0..manifold.point_count {
                    let clip_point = xf_b.apply(manifold.points[i].local_point);
                    let c_a = clip_point + world.normal * (radius_a - dot(world.normal, clip_point - plane_point));
                    let c_b = clip_point - world.normal * radius_b;
                    world.points[i] = midpoint(c_a, c_b);
                    world.separations[i] = dot(c_b - c_a, world.normal);
                }
            }
            ManifoldType::FaceB => {
                world.normal = xf_b.q.apply(manifold.local_normal);
                let plane_point = xf_b.apply(manifold.local_point);
                for i in 0..manifold.point_count {
                    let clip_point = xf_a.apply(manifold.points[i].local_point);
                    let c_b = clip_point + world.normal * (radius_b - dot(world.normal, clip_point - plane_point));
                    let c_a = clip_point - world.normal * radius_a;
                    world.points[i] = midpoint(c_a, c_b);
                    world.separations[i] = dot(c_a - c_b, world.normal);
                }
                world.normal = -world.normal;
            }
        }
        world
    }
}
