//! GJK as UnityPlayer.dll compiles it: b2DistanceProxy, b2Simplex and b2Distance (180bb08e0).

use super::math::{cross, Transform, Vec2};

const MAX_ITERATIONS: usize = 20;

/// A convex vertex set with a rounding radius: a polygon, a circle centre, or a capsule/edge segment.
#[derive(Clone, Copy, Debug)]
pub struct DistanceProxy<'a> {
    pub vertices: &'a [Vec2],
    pub radius: f32,
}

impl DistanceProxy<'_> {
    /// b2DistanceProxy::GetSupport (180bad290): the first vertex with the greatest projection on `d`.
    pub fn support(&self, d: Vec2) -> usize {
        let mut best = 0;
        let mut best_value = d.y * self.vertices[0].y + d.x * self.vertices[0].x;
        for (i, v) in self.vertices.iter().enumerate().skip(1) {
            let value = d.y * v.y + d.x * v.x;
            if value > best_value {
                best = i;
                best_value = value;
            }
        }
        best
    }
}

/// b2SimplexCache: GJK's last simplex, carried between calls to warm start.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SimplexCache {
    pub metric: f32,
    pub count: usize,
    pub index_a: [u8; 3],
    pub index_b: [u8; 3],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DistanceOutput {
    pub point_a: Vec2,
    pub point_b: Vec2,
    pub distance: f32,
    pub iterations: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct SimplexVertex {
    w_a: Vec2,
    w_b: Vec2,
    w: Vec2,
    a: f32,
    index_a: usize,
    index_b: usize,
}

impl SimplexVertex {
    fn new(proxy_a: &DistanceProxy, xf_a: &Transform, index_a: usize, proxy_b: &DistanceProxy, xf_b: &Transform, index_b: usize) -> Self {
        let w_a = xf_a.apply(proxy_a.vertices[index_a]);
        let w_b = xf_b.apply(proxy_b.vertices[index_b]);
        Self { w_a, w_b, w: w_b - w_a, a: 0.0, index_a, index_b }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Simplex {
    v: [SimplexVertex; 3],
    count: usize,
}

impl Simplex {
    /// b2Simplex::ReadCache (180bae730).
    fn read_cache(cache: &SimplexCache, proxy_a: &DistanceProxy, xf_a: &Transform, proxy_b: &DistanceProxy, xf_b: &Transform) -> Self {
        let mut s = Self { count: cache.count, ..Self::default() };
        for i in 0..cache.count {
            s.v[i] = SimplexVertex::new(proxy_a, xf_a, cache.index_a[i] as usize, proxy_b, xf_b, cache.index_b[i] as usize);
        }
        if s.count > 1 {
            let metric1 = cache.metric;
            let metric2 = s.metric();
            if metric2 < metric1 * 0.5 || metric1 + metric1 < metric2 || metric2 < f32::EPSILON {
                s.count = 0;
            }
        }
        if s.count == 0 {
            s.v[0] = SimplexVertex { a: 1.0, ..SimplexVertex::new(proxy_a, xf_a, 0, proxy_b, xf_b, 0) };
            s.count = 1;
        }
        s
    }

    fn write_cache(&self, cache: &mut SimplexCache) {
        cache.metric = self.metric();
        cache.count = self.count;
        for i in 0..self.count {
            cache.index_a[i] = self.v[i].index_a as u8;
            cache.index_b[i] = self.v[i].index_b as u8;
        }
    }

    fn search_direction(&self) -> Vec2 {
        match self.count {
            1 => -self.v[0].w,
            2 => {
                let e12 = self.v[1].w - self.v[0].w;
                if cross(e12, -self.v[0].w) > 0.0 { Vec2::new(-e12.y, e12.x) } else { Vec2::new(e12.y, -e12.x) }
            }
            _ => Vec2::ZERO,
        }
    }

    /// b2Simplex::GetWitnessPoints (180bad4d0).
    fn witness_points(&self) -> (Vec2, Vec2) {
        let [v1, v2, v3] = &self.v;
        match self.count {
            1 => (v1.w_a, v1.w_b),
            2 => (
                Vec2::new(v1.a * v1.w_a.x + v2.a * v2.w_a.x, v1.a * v1.w_a.y + v2.a * v2.w_a.y),
                Vec2::new(v1.a * v1.w_b.x + v2.a * v2.w_b.x, v1.a * v1.w_b.y + v2.a * v2.w_b.y),
            ),
            3 => {
                let p = Vec2::new(v1.a * v1.w_a.x + v2.a * v2.w_a.x + v3.a * v3.w_a.x, v1.a * v1.w_a.y + v2.a * v2.w_a.y + v3.a * v3.w_a.y);
                (p, p)
            }
            _ => (Vec2::ZERO, Vec2::ZERO),
        }
    }

    /// b2Simplex::GetMetric (180bad210).
    fn metric(&self) -> f32 {
        match self.count {
            2 => (self.v[0].w - self.v[1].w).length(),
            3 => cross(self.v[1].w - self.v[0].w, self.v[2].w - self.v[0].w),
            _ => 0.0,
        }
    }

    /// b2Simplex::Solve2, inlined into b2Distance.
    fn solve2(&mut self) {
        let (w1, w2) = (self.v[0].w, self.v[1].w);
        let e12 = w2 - w1;
        let d12_2 = -(e12.y * w1.y + e12.x * w1.x);
        if d12_2 <= 0.0 {
            self.v[0].a = 1.0;
            self.count = 1;
            return;
        }
        let d12_1 = e12.y * w2.y + e12.x * w2.x;
        if d12_1 <= 0.0 {
            self.v[1].a = 1.0;
            self.v[0] = self.v[1];
            self.count = 1;
            return;
        }
        let inv = 1.0 / (d12_1 + d12_2);
        self.v[0].a = inv * d12_1;
        self.v[1].a = inv * d12_2;
        self.count = 2;
    }

    /// b2Simplex::Solve3 (180baebf0).
    fn solve3(&mut self) {
        let (w1, w2, w3) = (self.v[0].w, self.v[1].w, self.v[2].w);
        let e12 = w2 - w1;
        let d12_1 = w2.x * e12.x + w2.y * e12.y;
        let d12_2 = -(w1.x * e12.x + w1.y * e12.y);
        let e13 = w3 - w1;
        let d13_1 = w3.x * e13.x + w3.y * e13.y;
        let d13_2 = -(w1.x * e13.x + w1.y * e13.y);
        let e23 = w3 - w2;
        let d23_1 = w3.x * e23.x + w3.y * e23.y;
        let d23_2 = -(w2.x * e23.x + w2.y * e23.y);
        let n123 = cross(e12, e13);
        let d123_1 = cross(w2, w3) * n123;
        let d123_2 = cross(w3, w1) * n123;
        let d123_3 = cross(w1, w2) * n123;
        if d12_2 <= 0.0 && d13_2 <= 0.0 {
            self.v[0].a = 1.0;
            self.count = 1;
        } else if d12_1 > 0.0 && d12_2 > 0.0 && d123_3 <= 0.0 {
            let inv = 1.0 / (d12_1 + d12_2);
            self.v[0].a = inv * d12_1;
            self.v[1].a = inv * d12_2;
            self.count = 2;
        } else if d13_1 > 0.0 && d13_2 > 0.0 && d123_2 <= 0.0 {
            let inv = 1.0 / (d13_1 + d13_2);
            self.v[0].a = inv * d13_1;
            self.v[2].a = inv * d13_2;
            self.v[1] = self.v[2];
            self.count = 2;
        } else if d12_1 <= 0.0 && d23_2 <= 0.0 {
            self.v[1].a = 1.0;
            self.v[0] = self.v[1];
            self.count = 1;
        } else if d13_1 <= 0.0 && d23_1 <= 0.0 {
            self.v[2].a = 1.0;
            self.v[0] = self.v[2];
            self.count = 1;
        } else if d23_1 > 0.0 && d23_2 > 0.0 && d123_1 <= 0.0 {
            let inv = 1.0 / (d23_1 + d23_2);
            self.v[1].a = inv * d23_1;
            self.v[2].a = inv * d23_2;
            self.v[0] = self.v[2];
            self.count = 2;
        } else {
            let inv = 1.0 / (d123_1 + d123_2 + d123_3);
            self.v[0].a = inv * d123_1;
            self.v[1].a = inv * d123_2;
            self.v[2].a = inv * d123_3;
            self.count = 3;
        }
    }
}

/// b2Distance (180bb08e0): closest points between two proxies, warm-started from and written back to `cache`.
pub fn distance(cache: &mut SimplexCache, proxy_a: &DistanceProxy, xf_a: &Transform, proxy_b: &DistanceProxy, xf_b: &Transform, use_radii: bool) -> DistanceOutput {
    let mut simplex = Simplex::read_cache(cache, proxy_a, xf_a, proxy_b, xf_b);
    let mut iterations = 0;
    loop {
        let saved: Vec<(usize, usize)> = simplex.v[..simplex.count].iter().map(|v| (v.index_a, v.index_b)).collect();
        match simplex.count {
            2 => simplex.solve2(),
            3 => simplex.solve3(),
            _ => {}
        }
        if simplex.count == 3 {
            break;
        }
        let d = simplex.search_direction();
        if d.length_squared() < f32::EPSILON * f32::EPSILON {
            break;
        }
        let index_a = proxy_a.support(xf_a.q.apply_t(-d));
        let index_b = proxy_b.support(xf_b.q.apply_t(d));
        simplex.v[simplex.count] = SimplexVertex::new(proxy_a, xf_a, index_a, proxy_b, xf_b, index_b);
        iterations += 1;
        if saved.contains(&(index_a, index_b)) {
            break;
        }
        simplex.count += 1;
        if iterations >= MAX_ITERATIONS {
            break;
        }
    }
    let (mut point_a, mut point_b) = simplex.witness_points();
    let mut output = DistanceOutput { point_a, point_b, distance: (point_a - point_b).length(), iterations };
    simplex.write_cache(cache);
    if use_radii {
        let (r_a, r_b) = (proxy_a.radius, proxy_b.radius);
        if output.distance <= r_b + r_a || output.distance <= f32::EPSILON {
            let p = Vec2::new((point_a.x + point_b.x) * 0.5, (point_b.y + point_a.y) * 0.5);
            output = DistanceOutput { point_a: p, point_b: p, distance: 0.0, iterations };
        } else {
            output.distance = output.distance - (r_b + r_a);
            let normal = (point_b - point_a).normalized();
            point_a = Vec2::new(normal.x * r_a + point_a.x, normal.y * r_a + point_a.y);
            point_b = Vec2::new(point_b.x - normal.x * r_b, point_b.y - normal.y * r_b);
            output.point_a = point_a;
            output.point_b = point_b;
        }
    }
    output
}
