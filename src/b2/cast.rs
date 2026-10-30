//! Cast2DQueryBase::RunQuery (180bf7af0) for a circle cast with the legacy ContactFilter2D (CircleCast2DQuery,
//! vtable 181712628) and Physics2D.queriesStartInColliders off, and CircleCast2DQuery's ConvertToRaycastHit (180beba60).
//! The query is split like the native: the candidates in report order (`circle_cast_candidates`), then, once the caller
//! has sorted them by fraction, each one's hit (`circle_cast_hit`) until one converts.

use super::broadphase::Aabb;
use super::collide::{Manifold, WorldManifold};
use super::math::{Rot, Transform, Vec2};
use super::shape::Circle;
use super::toi::{time_of_impact, Sweep, ToiState};
use super::world::{evaluate, BodyId, FixtureId, Shape, World};

/// 10 * FLT_EPSILON (1817112c0): a touching time at or below it is re-tested for overlap at the start.
const START_TIME: f32 = f32::from_bits(0x35a0_0000);
/// The radius bump of ConvertToRaycastHit's retry (1817112d0).
const RADIUS_BUMP: f32 = f32::from_bits(0x3b23_d70a);
/// Two manifold points whose separations differ by less pick their midpoint (181712c84).
const POINT_TOLERANCE: f32 = f32::from_bits(0x37d1_b717);

/// A FixtureContact the query kept: the swept circle touches `fixture`'s `child` at `fraction` of the cast.
#[derive(Clone, Copy, Debug)]
pub struct CastCandidate {
    pub fixture: FixtureId,
    pub child: usize,
    pub fraction: f32,
    pub distance: f32,
    /// The circle's transform at the time of impact, and the body's.
    pub cast_xf: Transform,
    pub body_xf: Transform,
}

/// A RaycastHit2D.
#[derive(Clone, Copy, Debug)]
pub struct CastHit {
    pub collider: i32,
    pub game_object: i32,
    pub body: BodyId,
    pub centroid: Vec2,
    pub point: Vec2,
    pub normal: Vec2,
    pub distance: f32,
    pub fraction: f32,
}

impl World {
    /// RunQuery up to its std::sort: the circle (radius `radius`, no rotation) swept from `origin` to `end` against
    /// every proxy in the sweep's AABB, in the broadphase's report order. ReportFixtureProxy skips sensors
    /// (QueriesHitTriggers is off) and layers outside `layer_mask`; the depth range is unbounded. A fixture is kept when
    /// its time of impact is touching (a start overlap, from the sweep's start, is dropped).
    pub fn circle_cast_candidates(&self, origin: Vec2, end: Vec2, radius: f32, layer_mask: u32) -> Vec<CastCandidate> {
        let circle = Shape::Circle(Circle { radius, p: Vec2::ZERO });
        let q = Rot::new(0.0);
        let start_xf = Transform { p: origin, q };
        let end_xf = Transform { p: end, q };
        let aabb = Aabb::combine(&circle.compute_aabb(&start_xf, 0), &circle.compute_aabb(&end_xf, 0));
        let sweep_a = Sweep { local_center: Vec2::ZERO, c0: origin, c: end, a0: 0.0, a: 0.0, alpha0: 0.0 };
        let mut out = Vec::new();
        for (f, child) in self.query_aabb(&aabb) {
            let fx = self.fixture(f);
            if fx.is_sensor {
                continue;
            }
            let Some(&layer) = self.layers.get(&fx.collider.game_object) else {
                panic!("Physics2D.CircleCast: no layer for collider {}", fx.collider.id);
            };
            if layer_mask & (1 << layer) == 0 {
                continue;
            }
            // Sweep B: the body, standing still at its centre of mass.
            let body = self.body(fx.body);
            let (xf, lc) = (body.xf, body.sweep.local_center);
            let c = Vec2::new((lc.x * xf.q.c - lc.y * xf.q.s) + xf.p.x, (lc.y * xf.q.c + lc.x * xf.q.s) + xf.p.y);
            let sweep_b = Sweep { local_center: lc, c0: c, c, a0: body.sweep.a, a: body.sweep.a, alpha0: 0.0 };
            let toi = time_of_impact(&circle.proxy(0), &sweep_a, &fx.shape.proxy(child), &sweep_b, 1.0);
            if toi.state != ToiState::Touching {
                continue;
            }
            if toi.t <= START_TIME && circle.test_overlap(0, &start_xf, &fx.shape, child, &xf) {
                continue;
            }
            let cast_xf = sweep_a.transform(toi.t);
            let (dx, dy) = (cast_xf.p.x - origin.x, cast_xf.p.y - origin.y);
            out.push(CastCandidate { fixture: f, child, fraction: toi.t, distance: (dx * dx + dy * dy).sqrt(), cast_xf, body_xf: xf });
        }
        out
    }

    /// ConvertToRaycastHit: a b2Contact between the query's circle and the fixture is evaluated at the time of impact;
    /// with no manifold point the circle's radius is bumped for a second try (and restored by a subtraction, so
    /// `radius` carries the rounding to the next candidate). None when it still has no point.
    pub fn circle_cast_hit(&self, c: &CastCandidate, radius: &mut f32) -> Option<CastHit> {
        let fx = self.fixture(c.fixture);
        // b2Contact::Create's order: the polygon, capsule or chain is A and the circle B; two circles keep the query A.
        let query_is_b = 0 < fx.shape.contact_order();
        let evaluate_with = |r: f32| {
            let circle = Shape::Circle(Circle { radius: r, p: Vec2::ZERO });
            let mut m = Manifold::default();
            if query_is_b {
                evaluate(&mut m, &fx.shape, c.child, &c.body_xf, &circle, &c.cast_xf);
            } else {
                evaluate(&mut m, &circle, 0, &c.cast_xf, &fx.shape, &c.body_xf);
            }
            m
        };
        let (radius_a, radius_b) = if query_is_b { (fx.shape.radius(), *radius) } else { (*radius, fx.shape.radius()) };
        let mut m = evaluate_with(*radius);
        if m.point_count == 0 {
            let bumped = *radius + RADIUS_BUMP;
            m = evaluate_with(bumped);
            *radius = bumped - RADIUS_BUMP;
        }
        if m.point_count == 0 {
            return None;
        }
        let (xf_a, xf_b) = if query_is_b { (&c.body_xf, &c.cast_xf) } else { (&c.cast_xf, &c.body_xf) };
        let wm = WorldManifold::new(&m, xf_a, radius_a, xf_b, radius_b);
        // ContactFilter2D::IsOutsideNormalAngle (180bf1900) is off in the legacy filter.
        let normal = if query_is_b { wm.normal } else { -wm.normal };
        let point = if m.point_count == 1 {
            wm.points[0]
        } else if POINT_TOLERANCE < (wm.separations[0] - wm.separations[1]).abs() {
            if wm.separations[1] <= wm.separations[0] { wm.points[1] } else { wm.points[0] }
        } else {
            Vec2::new((wm.points[1].x + wm.points[0].x) * 0.5, (wm.points[1].y + wm.points[0].y) * 0.5)
        };
        Some(CastHit { collider: fx.collider.id, game_object: fx.collider.game_object, body: fx.body, centroid: c.cast_xf.p, point, normal, distance: c.distance, fraction: c.fraction })
    }
}
