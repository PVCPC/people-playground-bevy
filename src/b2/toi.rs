//! Conservative advancement as UnityPlayer.dll compiles it: b2SeparationFunction and b2TimeOfImpact (180bb1c40).

use super::distance::{distance, DistanceProxy, SimplexCache};
use super::math::{Rot, Transform, Vec2};

const LINEAR_SLOP: f32 = 0.005;
const MAX_ITERATIONS: usize = 20;
const MAX_ROOT_ITERATIONS: usize = 50;
const MAX_PUSH_BACK_ITERATIONS: usize = 8;
const INV_TWO_PI: f32 = f32::from_bits(0x3e22_f983);
const TWO_PI: f32 = f32::from_bits(0x40c9_0fdb);

/// b2Sweep: a body's motion over the step, about its centre of mass.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sweep {
    pub local_center: Vec2,
    pub c0: Vec2,
    pub c: Vec2,
    pub a0: f32,
    pub a: f32,
    pub alpha0: f32,
}

impl Sweep {
    /// b2Sweep::GetTransform at fraction `beta`.
    pub fn transform(&self, beta: f32) -> Transform {
        let k = 1.0 - beta;
        let p = Vec2::new(k * self.c0.x + beta * self.c.x, k * self.c0.y + beta * self.c.y);
        let q = Rot::new(k * self.a0 + beta * self.a);
        Transform { p: p - q.apply(self.local_center), q }
    }

    /// b2Sweep::Normalize with Unity's reciprocal multiply.
    pub fn normalize(&mut self) {
        let turns = (self.a0 * INV_TWO_PI).floor();
        self.a0 = self.a0 - turns * TWO_PI;
        self.a = self.a - turns * TWO_PI;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToiState {
    Unknown,
    Failed,
    Overlapped,
    Touching,
    Separated,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToiOutput {
    pub state: ToiState,
    pub t: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SeparationKind {
    Points,
    FaceA,
    FaceB,
}

/// The separation along the GJK witness axis as a function of time; unused support indices are usize::MAX.
struct SeparationFunction<'a> {
    proxy_a: &'a DistanceProxy<'a>,
    proxy_b: &'a DistanceProxy<'a>,
    sweep_a: Sweep,
    sweep_b: Sweep,
    kind: SeparationKind,
    local_point: Vec2,
    axis: Vec2,
}

fn dot(a: Vec2, b: Vec2) -> f32 {
    a.x * b.x + a.y * b.y
}

impl<'a> SeparationFunction<'a> {
    /// b2SeparationFunction::Initialize (180bad5c0).
    fn new(cache: &SimplexCache, proxy_a: &'a DistanceProxy<'a>, sweep_a: Sweep, proxy_b: &'a DistanceProxy<'a>, sweep_b: Sweep, t1: f32) -> Self {
        let xf_a = sweep_a.transform(t1);
        let xf_b = sweep_b.transform(t1);
        let mut f = Self { proxy_a, proxy_b, sweep_a, sweep_b, kind: SeparationKind::Points, local_point: Vec2::ZERO, axis: Vec2::ZERO };
        if cache.count == 1 {
            let point_a = xf_a.apply(proxy_a.vertices[cache.index_a[0] as usize]);
            let point_b = xf_b.apply(proxy_b.vertices[cache.index_b[0] as usize]);
            f.axis = point_b - point_a;
            f.axis.normalize();
            return f;
        }
        let (kind, proxy, xf, other_proxy, xf_other, indices, other_index) = if cache.index_a[0] == cache.index_a[1] {
            (SeparationKind::FaceB, proxy_b, &xf_b, proxy_a, &xf_a, cache.index_b, cache.index_a[0])
        } else {
            (SeparationKind::FaceA, proxy_a, &xf_a, proxy_b, &xf_b, cache.index_a, cache.index_b[0])
        };
        f.kind = kind;
        let p1 = proxy.vertices[indices[0] as usize];
        let p2 = proxy.vertices[indices[1] as usize];
        f.axis = Vec2::new(p2.y - p1.y, (p2.x - p1.x) * -1.0);
        f.axis.normalize();
        let normal = xf.q.apply(f.axis);
        f.local_point = Vec2::new((p1.x + p2.x) * 0.5, (p1.y + p2.y) * 0.5);
        let point = xf.apply(f.local_point);
        let other = xf_other.apply(other_proxy.vertices[other_index as usize]);
        if dot(other - point, normal) < 0.0 {
            f.axis = -f.axis;
        }
        f
    }

    /// FindMinSeparation (180bacc60): the deepest support points along the axis at time t.
    fn find_min_separation(&self, t: f32) -> (usize, usize, f32) {
        let xf_a = self.sweep_a.transform(t);
        let xf_b = self.sweep_b.transform(t);
        match self.kind {
            SeparationKind::Points => {
                let index_a = self.proxy_a.support(xf_a.q.apply_t(self.axis));
                let index_b = self.proxy_b.support(xf_b.q.apply_t(-self.axis));
                (index_a, index_b, self.evaluate_at(&xf_a, &xf_b, index_a, index_b))
            }
            SeparationKind::FaceA => {
                let normal = xf_a.q.apply(self.axis);
                let index_b = self.proxy_b.support(xf_b.q.apply_t(-normal));
                (usize::MAX, index_b, self.evaluate_at(&xf_a, &xf_b, usize::MAX, index_b))
            }
            SeparationKind::FaceB => {
                let normal = xf_b.q.apply(self.axis);
                let index_a = self.proxy_a.support(xf_a.q.apply_t(-normal));
                (index_a, usize::MAX, self.evaluate_at(&xf_a, &xf_b, index_a, usize::MAX))
            }
        }
    }

    /// Evaluate (180bac860).
    fn evaluate(&self, index_a: usize, index_b: usize, t: f32) -> f32 {
        self.evaluate_at(&self.sweep_a.transform(t), &self.sweep_b.transform(t), index_a, index_b)
    }

    fn evaluate_at(&self, xf_a: &Transform, xf_b: &Transform, index_a: usize, index_b: usize) -> f32 {
        match self.kind {
            SeparationKind::Points => {
                let point_a = xf_a.apply(self.proxy_a.vertices[index_a]);
                let point_b = xf_b.apply(self.proxy_b.vertices[index_b]);
                dot(point_b - point_a, self.axis)
            }
            SeparationKind::FaceA => {
                let normal = xf_a.q.apply(self.axis);
                let point_a = xf_a.apply(self.local_point);
                let point_b = xf_b.apply(self.proxy_b.vertices[index_b]);
                dot(point_b - point_a, normal)
            }
            SeparationKind::FaceB => {
                let normal = xf_b.q.apply(self.axis);
                let point_b = xf_b.apply(self.local_point);
                let point_a = xf_a.apply(self.proxy_a.vertices[index_a]);
                dot(point_a - point_b, normal)
            }
        }
    }
}

/// b2TimeOfImpact (180bb1c40): the first fraction of [0, t_max] at which the proxies come within the target distance.
pub fn time_of_impact(proxy_a: &DistanceProxy, sweep_a: &Sweep, proxy_b: &DistanceProxy, sweep_b: &Sweep, t_max: f32) -> ToiOutput {
    let (mut sweep_a, mut sweep_b) = (*sweep_a, *sweep_b);
    sweep_a.normalize();
    sweep_b.normalize();
    let total_radius = proxy_b.radius + proxy_a.radius;
    let target = (total_radius - f32::from_bits(0x3c75_c28f)).max(LINEAR_SLOP);
    let tolerance = 0.25 * LINEAR_SLOP;
    let mut t1 = 0.0f32;
    let mut cache = SimplexCache::default();
    let mut iterations = 0;
    loop {
        let output = distance(&mut cache, proxy_a, &sweep_a.transform(t1), proxy_b, &sweep_b.transform(t1), false);
        if output.distance <= 0.0 {
            return ToiOutput { state: ToiState::Overlapped, t: 0.0 };
        }
        if output.distance < target + tolerance {
            return ToiOutput { state: ToiState::Touching, t: t1 };
        }
        let f = SeparationFunction::new(&cache, proxy_a, sweep_a, proxy_b, sweep_b, t1);
        let mut t2 = t_max;
        for _ in 0..MAX_PUSH_BACK_ITERATIONS {
            let (index_a, index_b, mut s2) = f.find_min_separation(t2);
            if s2 > target + tolerance {
                return ToiOutput { state: ToiState::Separated, t: t_max };
            }
            if s2 > target - tolerance {
                t1 = t2;
                break;
            }
            let mut s1 = f.evaluate(index_a, index_b, t1);
            if s1 < target - tolerance {
                return ToiOutput { state: ToiState::Failed, t: t1 };
            }
            if s1 <= target + tolerance {
                return ToiOutput { state: ToiState::Touching, t: t1 };
            }
            let (mut a1, mut a2) = (t1, t2);
            for root_iteration in 0..MAX_ROOT_ITERATIONS {
                let t = if root_iteration & 1 == 1 { (a2 - a1) * (target - s1) / (s2 - s1) + a1 } else { (a2 + a1) * 0.5 };
                let s = f.evaluate(index_a, index_b, t);
                if (s - target).abs() < tolerance {
                    t2 = t;
                    break;
                }
                if target < s {
                    a1 = t;
                    s1 = s;
                } else {
                    a2 = t;
                    s2 = s;
                }
            }
        }
        iterations += 1;
        if iterations == MAX_ITERATIONS {
            return ToiOutput { state: ToiState::Failed, t: t1 };
        }
    }
}
