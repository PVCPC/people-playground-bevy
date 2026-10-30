//! Shapes as UnityPlayer.dll builds them: b2PolygonShape::Set with Unity's collinear-vertex pass and |area| centroid,
//! BoxCollider2D's corner layout and PolygonCollider2D::ValidatePolygonShape.

use super::math::Vec2;

pub const MAX_POLYGON_VERTICES: usize = 8;
/// b2_polygonRadius; Unity sets it from Physics2DSettings.defaultContactOffset.
const WELD_TOLERANCE: f32 = f32::from_bits(0x36d1_b717);
const COLLINEAR_COSINE: f32 = f32::from_bits(0x3f7f_ff58);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Polygon {
    pub radius: f32,
    pub centroid: Vec2,
    pub vertices: [Vec2; MAX_POLYGON_VERTICES],
    pub normals: [Vec2; MAX_POLYGON_VERTICES],
    pub count: usize,
}

/// b2EdgeShape: the segment v1-v2, with the neighbouring vertices v0 / v3 when it is a chain's child.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Edge {
    pub radius: f32,
    pub v1: Vec2,
    pub v2: Vec2,
    pub v0: Vec2,
    pub v3: Vec2,
    pub has_v0: bool,
    pub has_v3: bool,
}

/// b2ChainShape as EdgeCollider2D makes it (CreateChain, never a loop): the vertices and the ghost vertices before
/// the first and after the last.
#[derive(Clone, Debug, PartialEq)]
pub struct Chain {
    pub radius: f32,
    pub vertices: std::sync::Arc<[Vec2]>,
    pub prev: Option<Vec2>,
    pub next: Option<Vec2>,
}

impl Chain {
    /// b2ChainShape::GetChildEdge (180bb3df0): Unity drops a neighbour vertex lying ahead of the edge's end it
    /// joins (its dot with the edge's direction away from that end under -FLT_EPSILON).
    pub fn child_edge(&self, i: usize) -> Edge {
        let v = &self.vertices;
        let mut e = Edge { radius: self.radius, v1: v[i], v2: v[i + 1], ..Edge::default() };
        (e.v0, e.has_v0) = if i > 0 { (v[i - 1], true) } else { (self.prev.unwrap_or_default(), self.prev.is_some()) };
        (e.v3, e.has_v3) = if i + 2 < v.len() { (v[i + 2], true) } else { (self.next.unwrap_or_default(), self.next.is_some()) };
        let d = e.v2 - e.v1;
        let min = f32::from_bits(0xb400_0000);
        if e.has_v0 {
            e.has_v0 = super::math::dot(-d, e.v0 - e.v1) >= min;
        }
        if e.has_v3 {
            e.has_v3 = super::math::dot(e.v3 - e.v2, d) >= min;
        }
        e
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Circle {
    pub radius: f32,
    pub p: Vec2,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MassData {
    pub mass: f32,
    pub center: Vec2,
    pub inertia: f32,
    pub area: f32,
}

/// The weld and gift-wrap shared by b2PolygonShape::Set (180bb4b90) and ValidatePolygonShape (180c16400).
fn weld_and_hull(points: &[Vec2]) -> Option<Vec<Vec2>> {
    let mut ps: Vec<Vec2> = Vec::with_capacity(MAX_POLYGON_VERTICES);
    for &v in points.iter().take(MAX_POLYGON_VERTICES) {
        if ps.iter().all(|p| (v - *p).length_squared() >= WELD_TOLERANCE) {
            ps.push(v);
        }
    }
    if ps.len() < 3 {
        return None;
    }
    let mut i0 = 0;
    let mut x0 = ps[0].x;
    for (i, p) in ps.iter().enumerate().skip(1) {
        if p.x > x0 || (p.x == x0 && p.y < ps[i0].y) {
            i0 = i;
            x0 = p.x;
        }
    }
    let mut hull = Vec::with_capacity(MAX_POLYGON_VERTICES);
    let mut ih = i0;
    loop {
        hull.push(ih);
        let mut ie = 0;
        for j in 1..ps.len() {
            if ie == ih {
                ie = j;
                continue;
            }
            let r = ps[ie] - ps[ih];
            let v = ps[j] - ps[ih];
            let c = v.y * r.x - v.x * r.y;
            if c < 0.0 || (c == 0.0 && r.y * r.y + r.x * r.x < v.y * v.y + v.x * v.x) {
                ie = j;
            }
        }
        ih = ie;
        if ie == i0 {
            break;
        }
        if hull.len() == MAX_POLYGON_VERTICES {
            return None;
        }
    }
    Some(hull.into_iter().map(|i| ps[i]).collect())
}

fn average(vertices: &[Vec2]) -> Vec2 {
    let mut s = Vec2::ZERO;
    for v in vertices {
        s.x += v.x;
        s.y += v.y;
    }
    let inv = 1.0 / vertices.len() as f32;
    Vec2::new(s.x * inv, s.y * inv)
}

/// The triangle-fan cross product (e2.y e1.x - e2.x e1.y) around `s`, as all three natives write it.
fn fan_cross(s: Vec2, p2: Vec2, p3: Vec2) -> (Vec2, Vec2, f32) {
    let e1 = p2 - s;
    let e2 = p3 - s;
    (e1, e2, e2.y * e1.x - e2.x * e1.y)
}

/// ComputeCentroid (180bb2ed0): fans from the vertex average and divides by |area|.
fn compute_centroid(vertices: &[Vec2]) -> Vec2 {
    let s = average(vertices);
    let mut c = Vec2::ZERO;
    let mut area = 0.0f32;
    for i in 0..vertices.len() {
        let p2 = vertices[i];
        let p3 = vertices[if i + 1 < vertices.len() { i + 1 } else { 0 }];
        let (_, _, d) = fan_cross(s, p2, p3);
        let triangle_area = d * 0.5;
        area += triangle_area;
        let k = triangle_area * (1.0 / 3.0);
        c.x += ((p2.x + s.x) + p3.x) * k;
        c.y += ((p2.y + s.y) + p3.y) * k;
    }
    let inv = 1.0 / area.abs();
    Vec2::new(inv * c.x, inv * c.y)
}

/// Unity's unit-box fallback for degenerate input.
fn unit_box() -> Polygon {
    let mut polygon = Polygon { count: 4, ..Polygon::default() };
    polygon.vertices[..4].copy_from_slice(&[Vec2::new(-1.0, -1.0), Vec2::new(1.0, -1.0), Vec2::new(1.0, 1.0), Vec2::new(-1.0, 1.0)]);
    polygon.normals[..4].copy_from_slice(&[Vec2::new(0.0, -1.0), Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0), Vec2::new(-1.0, 0.0)]);
    polygon
}

/// Unity's collinear pass in Set and PreparePolygonShapes: every vertex but the last against its stored predecessor
/// and its successor, then the last against the first; stops as soon as fewer than 3 remain.
fn drop_collinear(vertices: &mut Vec<Vec2>) {
    let mut prev = vertices[vertices.len() - 1];
    let mut cur = 0;
    for _ in 0..vertices.len() - 1 {
        if collinear(prev, vertices[cur], vertices[cur + 1]) {
            vertices.remove(cur);
            if vertices.len() < 3 {
                return;
            }
        } else {
            prev = vertices[cur];
            cur += 1;
        }
    }
    if collinear(prev, vertices[cur], vertices[0]) {
        vertices.pop();
    }
}

/// b2AreCollinear (180bb61e0), also inlined into Set's vertex pass.
fn collinear(a: Vec2, b: Vec2, c: Vec2) -> bool {
    let d1 = b - a;
    let d2 = c - b;
    let l1 = (d1.y * d1.y + d1.x * d1.x).sqrt();
    let l2 = (d2.y * d2.y + d2.x * d2.x).sqrt();
    l1 * l2 < f32::EPSILON || (d2.y * d1.y + d2.x * d1.x) / (l1 * l2) > COLLINEAR_COSINE
}

impl Polygon {
    /// b2PolygonShape::SetAsBox (180bb5870) at angle 0 about `center`, keeping the shape's radius.
    pub fn set_as_box(hx: f32, hy: f32, center: Vec2, radius: f32) -> Self {
        let mut p = Polygon { radius, centroid: center, count: 4, ..Default::default() };
        let corners = [Vec2::new(-hx, -hy), Vec2::new(hx, -hy), Vec2::new(hx, hy), Vec2::new(-hx, hy)];
        let normals = [Vec2::new(0.0, -1.0), Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0), Vec2::new(-1.0, 0.0)];
        for i in 0..4 {
            p.vertices[i] = Vec2::new(corners[i].x + center.x, corners[i].y + center.y);
            p.normals[i] = normals[i];
        }
        p
    }

    /// b2PolygonShape::Set (180bb4b90). The radius stays at the caller's value.
    pub fn from_points(points: &[Vec2], radius: f32) -> Self {
        let Some(mut vertices) = (points.len() >= 3).then(|| weld_and_hull(points)).flatten() else {
            return Self { radius, ..unit_box() };
        };
        drop_collinear(&mut vertices);
        if vertices.len() < 3 {
            return Self { radius, ..unit_box() };
        }
        let mut polygon = Self { radius, count: vertices.len(), ..Self::default() };
        polygon.vertices[..vertices.len()].copy_from_slice(&vertices);
        for i in 0..polygon.count {
            let next = polygon.vertices[if i + 1 < polygon.count { i + 1 } else { 0 }];
            let edge = next - polygon.vertices[i];
            let mut normal = Vec2::new(edge.y, edge.x * -1.0);
            normal.normalize();
            polygon.normals[i] = normal;
        }
        polygon.centroid = compute_centroid(&vertices);
        polygon
    }

    /// b2PolygonShape::SetAsEdge (180bb59e0), which Unity uses to collide capsules as rounded two-sided segments.
    pub fn segment(v1: Vec2, v2: Vec2, radius: f32) -> Self {
        let mut normal = Vec2::new(v2.y - v1.y, (v2.x - v1.x) * -1.0);
        normal.normalize();
        let mut polygon = Self { radius, count: 2, ..Self::default() };
        polygon.vertices[0] = v1;
        polygon.vertices[1] = v2;
        polygon.centroid = Vec2::new((v1.x + v2.x) * 0.5, (v1.y + v2.y) * 0.5);
        polygon.normals[0] = normal;
        polygon.normals[1] = -normal;
        polygon
    }

    pub fn next(&self, index: usize) -> usize {
        if index + 1 < self.count { index + 1 } else { 0 }
    }

    /// b2PolygonShape::ComputeMass (180bb35a0).
    pub fn compute_mass(&self, density: f32) -> MassData {
        let vertices = &self.vertices[..self.count];
        let s = average(vertices);
        let mut area = 0.0f32;
        let mut center = Vec2::ZERO;
        let mut inertia = 0.0f32;
        for i in 0..self.count {
            let (e1, e2, d) = fan_cross(s, vertices[i], vertices[self.next(i)]);
            let triangle_area = d * 0.5;
            area += triangle_area;
            let k = triangle_area * f32::from_bits(0x3eaa_aaab);
            center.y += (e2.y + e1.y) * k;
            center.x += (e2.x + e1.x) * k;
            let integral = (e2.y * e1.y + e1.y * e1.y + e2.y * e2.y) + (e2.x * e1.x + e1.x * e1.x + e2.x * e2.x);
            inertia += integral * (d * f32::from_bits(0x3daa_aaab));
        }
        let area = area.abs();
        let inv = 1.0 / area;
        let local = Vec2::new(center.x * inv, center.y * inv);
        let center = Vec2::new(s.x + local.x, s.y + local.y);
        let shift = (center.y * center.y + center.x * center.x) - (local.y * local.y + local.x * local.x);
        let mass = area * density;
        MassData { mass, center, inertia: shift * mass + inertia * density, area }
    }
}

/// BoxCollider2D::PrepareShapes (180c210d0) corner order around the collider offset.
pub fn box_corners(offset: Vec2, size: Vec2) -> [Vec2; 4] {
    let hx = size.x * 0.5;
    let hy = size.y * 0.5;
    [
        Vec2::new(offset.x - hx, offset.y - hy),
        Vec2::new(hx + offset.x, offset.y - hy),
        Vec2::new(hx + offset.x, hy + offset.y),
        Vec2::new(offset.x - hx, hy + offset.y),
    ]
}

/// The collider-to-body 2D part of a column-major Matrix4x4f: m[0], m[1], m[4], m[5], m[8], m[9], m[12], m[13].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColliderMatrix(pub [f32; 8]);

impl ColliderMatrix {
    pub const IDENTITY: Self = Self([1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0]);

    pub fn apply(&self, v: Vec2) -> Vec2 {
        let m = &self.0;
        Vec2::new(m[0] * v.x + m[2] * v.y + m[4] * 0.0 + m[6], m[1] * v.x + m[3] * v.y + m[5] * 0.0 + m[7])
    }
}

/// PolygonCollider2D::ValidatePolygonShape (180c16400) at the weld tolerance BoxCollider2D passes.
pub fn validate_polygon(points: &[Vec2]) -> bool {
    if !(3..=MAX_POLYGON_VERTICES).contains(&points.len()) {
        return false;
    }
    let Some(hull) = weld_and_hull(points) else { return false };
    let next = |i: usize| if i + 1 < hull.len() { i + 1 } else { 0 };
    if (0..hull.len()).any(|i| (hull[next(i)] - hull[i]).length_squared() <= f32::EPSILON * f32::EPSILON) {
        return false;
    }
    let s = average(&hull);
    let mut area = 0.0f32;
    for i in 0..hull.len() {
        let (_, _, d) = fan_cross(s, hull[i], hull[next(i)]);
        area += d * 0.5;
    }
    area.is_finite() && area > f32::EPSILON
}

/// PolygonCollider2D::TransformPoints (180c16060): each point plus the offset through the collider matrix (z = 0),
/// near-zero x and y snapped to 0 and near-duplicates of the previous point dropped; nothing when any point is
/// non-finite or farther than 1e6.
fn transform_points(path: &[[f32; 2]], m: &[f32; 16], offset: Vec2, min_dist_sq: f32) -> Vec<[f32; 2]> {
    let snap = |v: f32| if f32::from_bits(0x35a0_0000) > v.abs() { 0.0 } else { v };
    let mut out: Vec<[f32; 2]> = Vec::with_capacity(path.len());
    for p in path {
        let (x, y) = (p[0] + offset.x, p[1] + offset.y);
        let tx = x * m[0] + y * m[4] + m[8] * 0.0 + m[12];
        let ty = x * m[1] + y * m[5] + m[9] * 0.0 + m[13];
        let tz = x * m[2] + y * m[6] + m[10] * 0.0 + m[14];
        if !(tx.is_finite() && ty.is_finite() && tz.is_finite()) || tx * tx + ty * ty + tz * tz > 1e12 {
            return Vec::new();
        }
        let t = [snap(tx), snap(ty)];
        if out.last().is_some_and(|l| min_dist_sq >= (l[0] - t[0]) * (l[0] - t[0]) + (l[1] - t[1]) * (l[1] - t[1])) {
            continue;
        }
        out.push(t);
    }
    out
}

/// PolygonCollider2D::PreparePolygonShapes (180c12b90) as PrepareShapes (180c133e0) calls it without tiling
/// (minimum vertex distance 0.0025): the paths of 3 or more points through TransformPoints, those keeping 3 or
/// more tesselated by libtess2 (odd winding, polygons of up to 8), each polygon's near-duplicate and collinear
/// vertices dropped, and the ones passing ValidatePolygonShape made b2PolygonShapes. Nothing without a first path
/// of 3 or more points.
pub fn polygon_collider_polygons(paths: &[Vec<[f32; 2]>], m: &[f32; 16], offset: Vec2) -> Vec<Polygon> {
    use crate::unity::tess::{tesselate, TESS_UNDEF};
    let min_distance = 0.0025f32;
    let min_dist_sq = min_distance * min_distance;
    if paths.first().map_or(true, |p| p.len() < 3) {
        return Vec::new();
    }
    let contours: Vec<_> = paths.iter().filter(|p| p.len() >= 3).map(|p| transform_points(p, m, offset, min_dist_sq)).filter(|c| c.len() >= 3).collect();
    let Some((vertices, elements)) = tesselate(&contours, MAX_POLYGON_VERTICES) else { return Vec::new() };
    let mut polygons = Vec::new();
    for element in elements.chunks(MAX_POLYGON_VERTICES) {
        let mut points: Vec<Vec2> = Vec::with_capacity(MAX_POLYGON_VERTICES);
        for &i in element.iter().take_while(|&&i| i != TESS_UNDEF) {
            let p = Vec2::new(vertices[i as usize][0], vertices[i as usize][1]);
            if points.last().is_some_and(|l| min_dist_sq >= (l.y - p.y) * (l.y - p.y) + (l.x - p.x) * (l.x - p.x)) {
                continue;
            }
            points.push(p);
        }
        if points.len() < 3 {
            continue;
        }
        drop_collinear(&mut points);
        if validate_polygon(&points) {
            polygons.push(Polygon::from_points(&points, super::settings::globals().polygon_radius));
        }
    }
    polygons
}

/// EdgeCollider2D::PrepareShapes (180c22090): the points plus the offset through the collider matrix (z = 0) by
/// TransformPoints (180c248b0), a point within 0.00709 of the previous one dropped, as a b2ChainShape of radius
/// b2_polygonRadius + edgeRadius with the adjacent points (when used) through the same matrix. None under 2 points.
pub fn edge_collider_chain(points: &[[f32; 2]], m: &[f32; 16], offset: Vec2, edge_radius: f32, start: Option<[f32; 2]>, end: Option<[f32; 2]>) -> Option<Chain> {
    let min_dist_sq = f32::from_bits(0x3852_c386);
    let apply = |p: [f32; 2]| {
        let (x, y) = (p[0] + offset.x, p[1] + offset.y);
        Vec2::new(x * m[0] + y * m[4] + m[8] * 0.0 + m[12], x * m[1] + y * m[5] + m[9] * 0.0 + m[13])
    };
    if points.len() < 2 {
        return None;
    }
    let mut vertices: Vec<Vec2> = Vec::with_capacity(points.len());
    for &p in points {
        let t = apply(p);
        if vertices.last().is_some_and(|l| min_dist_sq >= (l.x - t.x) * (l.x - t.x) + (l.y - t.y) * (l.y - t.y)) {
            continue;
        }
        vertices.push(t);
    }
    if vertices.len() < 2 || vertices.windows(2).any(|w| min_dist_sq > (w[0].x - w[1].x) * (w[0].x - w[1].x) + (w[0].y - w[1].y) * (w[0].y - w[1].y)) {
        return None;
    }
    Some(Chain { radius: super::settings::globals().polygon_radius + edge_radius, vertices: vertices.into(), prev: start.map(apply), next: end.map(apply) })
}

/// The polygons a BoxCollider2D contributes (non-tiled): none when the transformed box fails validation.
pub fn box_collider_polygon(offset: Vec2, size: Vec2, edge_radius: f32, matrix: &ColliderMatrix) -> Option<Polygon> {
    let corners = box_corners(offset, size).map(|c| matrix.apply(c));
    validate_polygon(&corners).then(|| Polygon::from_points(&corners, super::settings::globals().polygon_radius + edge_radius))
}
