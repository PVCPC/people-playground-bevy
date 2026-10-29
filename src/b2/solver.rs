//! b2ContactSolver as UnityPlayer.dll compiles it, with Unity's per-body axis constraint (RigidbodyConstraints2D):
//! normal-dependent inverse-mass scaling, velocities multiplied by the axis after each pass and position corrections
//! scaled by it.

use super::collide::{Manifold, ManifoldPoint, ManifoldType, WorldManifold};
use super::math::{cross, Rot, Transform, Vec2};

pub const VELOCITY_THRESHOLD: f32 = 1.0;
pub const BAUMGARTE: f32 = 0.2;
pub const TOI_BAUMGARTE: f32 = 0.75;
pub const MAX_LINEAR_CORRECTION: f32 = 0.2;
const LINEAR_SLOP: f32 = 0.005;
const MAX_CONDITION_NUMBER: f32 = 1000.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Position {
    pub c: Vec2,
    pub a: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Velocity {
    pub v: Vec2,
    pub w: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TimeStep {
    pub dt: f32,
    pub inv_dt: f32,
    pub dt_ratio: f32,
    pub velocity_iterations: i32,
    pub position_iterations: i32,
    pub warm_starting: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Mat22 {
    pub ex: Vec2,
    pub ey: Vec2,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VelocityConstraintPoint {
    pub r_a: Vec2,
    pub r_b: Vec2,
    pub normal_impulse: f32,
    pub tangent_impulse: f32,
    pub normal_mass: f32,
    pub tangent_mass: f32,
    pub velocity_bias: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VelocityConstraint {
    pub points: [VelocityConstraintPoint; 2],
    pub normal: Vec2,
    pub normal_mass: Mat22,
    pub k: Mat22,
    pub index_a: usize,
    pub index_b: usize,
    pub axis_a: Vec2,
    pub axis_b: Vec2,
    pub inv_mass_a: f32,
    pub inv_mass_b: f32,
    pub inv_i_a: f32,
    pub inv_i_b: f32,
    pub friction: f32,
    pub restitution: f32,
    pub tangent_speed: f32,
    pub point_count: usize,
    pub contact_index: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PositionConstraint {
    pub local_points: [Vec2; 2],
    pub local_normal: Vec2,
    pub local_point: Vec2,
    pub index_a: usize,
    pub index_b: usize,
    pub axis_a: Vec2,
    pub axis_b: Vec2,
    pub inv_mass_a: f32,
    pub inv_mass_b: f32,
    pub local_center_a: Vec2,
    pub local_center_b: Vec2,
    pub inv_i_a: f32,
    pub inv_i_b: f32,
    pub kind: ManifoldType,
    pub radius_a: f32,
    pub radius_b: f32,
    pub point_count: usize,
}

/// The body fields the constructor copies: invMass, axisConstraint, invI and sweep.localCenter.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SolverBody {
    pub inv_mass: f32,
    pub axis: Vec2,
    pub inv_i: f32,
    pub local_center: Vec2,
}

/// The b2Contact fields the constructor reads; index_a/b are the island indices Unity caches in the contact.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SolverContact {
    pub manifold: Manifold,
    pub radius_a: f32,
    pub radius_b: f32,
    pub friction: f32,
    pub restitution: f32,
    pub tangent_speed: f32,
    pub index_a: usize,
    pub index_b: usize,
    pub body_a: SolverBody,
    pub body_b: SolverBody,
}

pub struct ContactSolver {
    pub step: TimeStep,
    pub position_constraints: Vec<PositionConstraint>,
    pub velocity_constraints: Vec<VelocityConstraint>,
}

/// The body transform both solvers rebuild from a position: p = c - q·localCenter.
fn body_transform(position: Position, local_center: Vec2) -> Transform {
    let q = Rot::new(position.a);
    Transform { p: position.c - q.apply(local_center), q }
}

/// vB + w×rB - vA - w×rA with the natives' grouping.
fn relative_velocity(va: Velocity, ra: Vec2, vb: Velocity, rb: Vec2) -> Vec2 {
    Vec2::new(((-(vb.w * rb.y) + vb.v.x) - va.v.x) - -(va.w * ra.y), ((vb.w * rb.x + vb.v.y) - va.v.y) - va.w * ra.x)
}

/// Unity's axis-constraint extension: a body free on only some axes resists along the normal by 1/|axis·n|.
fn axis_scaled_mass(inv_mass: f32, axis: Vec2, n: Vec2) -> f32 {
    if !(0.0 < inv_mass && (axis.x < 1.0 || axis.y < 1.0)) {
        return inv_mass;
    }
    let t = (axis.y * n.y + axis.x * n.x).abs();
    let t = if 1.0 <= t { 1.0 } else { t };
    let t = if 0.0 <= t { t } else { 0.0 };
    inv_mass * if t <= 0.0 { 0.0 } else { 1.0 / t }
}

/// The effective mass along `dir`: (rA×d)² iA + (mA + mB) + (rB×d)² iB, grouped as the natives write it.
fn effective_mass(ra: Vec2, rb: Vec2, dir: Vec2, mass_sum: f32, inv_i_a: f32, inv_i_b: f32) -> f32 {
    let rn_a = cross(ra, dir);
    let rn_b = cross(rb, dir);
    let k = rn_a * inv_i_a * rn_a + mass_sum + rn_b * inv_i_b * rn_b;
    if k <= 0.0 { 0.0 } else { 1.0 / k }
}

/// Applies impulse `p` at the contact arms of both bodies.
fn apply_impulse(vc: &VelocityConstraint, va: &mut Velocity, ra: Vec2, vb: &mut Velocity, rb: Vec2, p: Vec2) {
    va.v.x = va.v.x - p.x * vc.inv_mass_a;
    va.v.y = va.v.y - p.y * vc.inv_mass_a;
    va.w = va.w - cross(ra, p) * vc.inv_i_a;
    vb.v.x = vb.v.x + p.x * vc.inv_mass_b;
    vb.v.y = vb.v.y + p.y * vc.inv_mass_b;
    vb.w = vb.w + cross(rb, p) * vc.inv_i_b;
}

/// Writes both bodies back with Unity's axis multiply.
fn store_velocities(vc: &VelocityConstraint, velocities: &mut [Velocity], va: Velocity, vb: Velocity) {
    velocities[vc.index_a] = Velocity { v: Vec2::new(va.v.x * vc.axis_a.x, va.v.y * vc.axis_a.y), w: va.w };
    velocities[vc.index_b] = Velocity { v: Vec2::new(vb.v.x * vc.axis_b.x, vb.v.y * vc.axis_b.y), w: vb.w };
}

/// b2PositionSolverManifold::Initialize (180bbe210): (normal, point, separation).
fn position_manifold(pc: &PositionConstraint, xf_a: &Transform, xf_b: &Transform, index: usize) -> (Vec2, Vec2, f32) {
    match pc.kind {
        ManifoldType::Circles => {
            let point_a = xf_a.apply(pc.local_point);
            let point_b = xf_b.apply(pc.local_points[0]);
            let d = point_b - point_a;
            let normal = d.normalized();
            let point = Vec2::new((point_b.x + point_a.x) * 0.5, (point_b.y + point_a.y) * 0.5);
            (normal, point, (d.y * normal.y + d.x * normal.x - pc.radius_a) - pc.radius_b)
        }
        ManifoldType::FaceA | ManifoldType::FaceB => {
            let (xf_plane, xf_clip) = if pc.kind == ManifoldType::FaceA { (xf_a, xf_b) } else { (xf_b, xf_a) };
            let normal = xf_plane.q.apply(pc.local_normal);
            let plane = xf_plane.apply(pc.local_point);
            let clip = xf_clip.apply(pc.local_points[index]);
            let d = clip - plane;
            let separation = (d.y * normal.y + d.x * normal.x - pc.radius_a) - pc.radius_b;
            (if pc.kind == ManifoldType::FaceA { normal } else { -normal }, clip, separation)
        }
    }
}

impl ContactSolver {
    /// b2ContactSolver::b2ContactSolver (180bbdd00).
    pub fn new(step: TimeStep, contacts: &[SolverContact]) -> Self {
        let mut position_constraints = Vec::with_capacity(contacts.len());
        let mut velocity_constraints = Vec::with_capacity(contacts.len());
        for (contact_index, contact) in contacts.iter().enumerate() {
            let manifold = &contact.manifold;
            let (a, b) = (&contact.body_a, &contact.body_b);
            let mut vc = VelocityConstraint {
                index_a: contact.index_a,
                index_b: contact.index_b,
                axis_a: a.axis,
                axis_b: b.axis,
                inv_mass_a: a.inv_mass,
                inv_mass_b: b.inv_mass,
                inv_i_a: a.inv_i,
                inv_i_b: b.inv_i,
                friction: contact.friction,
                restitution: contact.restitution,
                tangent_speed: contact.tangent_speed,
                point_count: manifold.point_count,
                contact_index,
                ..VelocityConstraint::default()
            };
            let mut pc = PositionConstraint {
                local_normal: manifold.local_normal,
                local_point: manifold.local_point,
                index_a: contact.index_a,
                index_b: contact.index_b,
                axis_a: a.axis,
                axis_b: b.axis,
                inv_mass_a: a.inv_mass,
                inv_mass_b: b.inv_mass,
                local_center_a: a.local_center,
                local_center_b: b.local_center,
                inv_i_a: a.inv_i,
                inv_i_b: b.inv_i,
                kind: manifold.kind,
                radius_a: contact.radius_a,
                radius_b: contact.radius_b,
                point_count: manifold.point_count,
                ..PositionConstraint::default()
            };
            for (j, mp) in manifold.points[..manifold.point_count].iter().enumerate() {
                if step.warm_starting {
                    vc.points[j].normal_impulse = mp.normal_impulse * step.dt_ratio;
                    vc.points[j].tangent_impulse = mp.tangent_impulse * step.dt_ratio;
                }
                pc.local_points[j] = mp.local_point;
            }
            velocity_constraints.push(vc);
            position_constraints.push(pc);
        }
        Self { step, position_constraints, velocity_constraints }
    }

    /// InitializeVelocityConstraints (180bbe5f0).
    pub fn initialize_velocity_constraints(&mut self, positions: &[Position], velocities: &[Velocity]) {
        for (vc, pc) in self.velocity_constraints.iter_mut().zip(&mut self.position_constraints) {
            let (pa, pb) = (positions[vc.index_a], positions[vc.index_b]);
            let (va, vb) = (velocities[vc.index_a], velocities[vc.index_b]);
            let xf_a = body_transform(pa, pc.local_center_a);
            let xf_b = body_transform(pb, pc.local_center_b);
            let mut manifold = Manifold {
                local_normal: pc.local_normal,
                local_point: pc.local_point,
                kind: pc.kind,
                point_count: pc.point_count,
                ..Manifold::default()
            };
            for j in 0..pc.point_count {
                manifold.points[j] = ManifoldPoint { local_point: pc.local_points[j], ..ManifoldPoint::default() };
            }
            let world = WorldManifold::new(&manifold, &xf_a, pc.radius_a, &xf_b, pc.radius_b);
            let n = world.normal;
            vc.normal = n;
            vc.inv_mass_a = axis_scaled_mass(vc.inv_mass_a, vc.axis_a, n);
            pc.inv_mass_a = vc.inv_mass_a;
            vc.inv_mass_b = axis_scaled_mass(vc.inv_mass_b, vc.axis_b, n);
            pc.inv_mass_b = vc.inv_mass_b;
            let mass_sum = vc.inv_mass_b + vc.inv_mass_a;
            let tangent = Vec2::new(n.y, n.x * -1.0);
            for j in 0..vc.point_count {
                let p = &mut vc.points[j];
                p.r_a = world.points[j] - pa.c;
                p.r_b = world.points[j] - pb.c;
                p.normal_mass = effective_mass(p.r_a, p.r_b, n, mass_sum, vc.inv_i_a, vc.inv_i_b);
                p.tangent_mass = effective_mass(p.r_a, p.r_b, tangent, mass_sum, vc.inv_i_a, vc.inv_i_b);
                p.velocity_bias = 0.0;
                let dv = relative_velocity(va, p.r_a, vb, p.r_b);
                let v_rel = dv.y * n.y + dv.x * n.x;
                if v_rel < -VELOCITY_THRESHOLD {
                    p.velocity_bias = -(v_rel * vc.restitution);
                }
            }
            if vc.point_count == 2 {
                let [p1, p2] = vc.points;
                let rn1a = cross(p1.r_a, n);
                let rn1b = cross(p1.r_b, n);
                let rn2a = cross(p2.r_a, n);
                let rn2b = cross(p2.r_b, n);
                let k11 = rn1a * vc.inv_i_a * rn1a + mass_sum + rn1b * vc.inv_i_b * rn1b;
                let k12 = rn1a * vc.inv_i_a * rn2a + mass_sum + rn1b * vc.inv_i_b * rn2b;
                let k22 = rn2a * vc.inv_i_a * rn2a + mass_sum + rn2b * vc.inv_i_b * rn2b;
                if (k22 * k11 - k12 * k12) * MAX_CONDITION_NUMBER <= k11 * k11 {
                    vc.point_count = 1;
                } else {
                    vc.k = Mat22 { ex: Vec2::new(k11, k12), ey: Vec2::new(k12, k22) };
                    let mut det = k22 * k11 - k12 * k12;
                    if det != 0.0 {
                        det = 1.0 / det;
                    }
                    vc.normal_mass = Mat22 { ex: Vec2::new(det * k22, -(det * k12)), ey: Vec2::new(-(det * k12), det * k11) };
                }
            }
        }
    }

    /// WarmStart (180bc0780).
    pub fn warm_start(&self, velocities: &mut [Velocity]) {
        for vc in &self.velocity_constraints {
            let (mut va, mut vb) = (velocities[vc.index_a], velocities[vc.index_b]);
            let n = vc.normal;
            for p in &vc.points[..vc.point_count] {
                let impulse = Vec2::new(n.x * p.normal_impulse + n.y * p.tangent_impulse, n.y * p.normal_impulse + n.x * -1.0 * p.tangent_impulse);
                apply_impulse(vc, &mut va, p.r_a, &mut vb, p.r_b, impulse);
            }
            store_velocities(vc, velocities, va, vb);
        }
    }

    /// SolveVelocityConstraints (180bbf920): friction, then the normal point or the two-point block LCP.
    pub fn solve_velocity_constraints(&mut self, velocities: &mut [Velocity]) {
        for vc in &mut self.velocity_constraints {
            let (mut va, mut vb) = (velocities[vc.index_a], velocities[vc.index_b]);
            let n = vc.normal;
            let tangent = Vec2::new(n.y, n.x * -1.0);
            for j in 0..vc.point_count {
                let p = vc.points[j];
                let max_friction = vc.friction * p.normal_impulse;
                let dv = relative_velocity(va, p.r_a, vb, p.r_b);
                let vt = dv.y * tangent.y + dv.x * tangent.x;
                let x = -((vt - vc.tangent_speed) * p.tangent_mass) + p.tangent_impulse;
                let x = if max_friction <= x { max_friction } else { x };
                let new_impulse = if -max_friction <= x { x } else { -max_friction };
                vc.points[j].tangent_impulse = new_impulse;
                let lambda = new_impulse - p.tangent_impulse;
                apply_impulse(vc, &mut va, p.r_a, &mut vb, p.r_b, Vec2::new(tangent.x * lambda, tangent.y * lambda));
            }
            if vc.point_count == 1 {
                let p = vc.points[0];
                let dv = relative_velocity(va, p.r_a, vb, p.r_b);
                let vn = dv.y * n.y + dv.x * n.x;
                let x = -((vn - p.velocity_bias) * p.normal_mass) + p.normal_impulse;
                let new_impulse = if x <= 0.0 { 0.0 } else { x };
                vc.points[0].normal_impulse = new_impulse;
                let lambda = new_impulse - p.normal_impulse;
                apply_impulse(vc, &mut va, p.r_a, &mut vb, p.r_b, Vec2::new(n.x * lambda, n.y * lambda));
            } else {
                let [p1, p2] = vc.points;
                let a = Vec2::new(p1.normal_impulse, p2.normal_impulse);
                let dv1 = relative_velocity(va, p1.r_a, vb, p1.r_b);
                let dv2 = relative_velocity(va, p2.r_a, vb, p2.r_b);
                let vn1 = dv1.y * n.y + dv1.x * n.x;
                let vn2 = dv2.y * n.y + dv2.x * n.x;
                let (k, m) = (vc.k, vc.normal_mass);
                let b = Vec2::new((vn1 - p1.velocity_bias) - (a.y * k.ey.x + a.x * k.ex.x), (vn2 - p2.velocity_bias) - (a.y * k.ey.y + a.x * k.ex.y));
                let full = Vec2::new(-(b.y * m.ey.x + b.x * m.ex.x), -(b.y * m.ey.y + b.x * m.ex.y));
                let x1 = -(b.x * p1.normal_mass);
                let x2 = -(b.y * p2.normal_mass);
                let x = if 0.0 <= full.x && 0.0 <= full.y {
                    Some(full)
                } else if 0.0 <= x1 && 0.0 <= x1 * k.ex.y + b.y {
                    Some(Vec2::new(x1, 0.0))
                } else if 0.0 <= x2 && 0.0 <= x2 * k.ey.x + b.x {
                    Some(Vec2::new(0.0, x2))
                } else if 0.0 <= b.x && 0.0 <= b.y {
                    Some(Vec2::ZERO)
                } else {
                    None
                };
                if let Some(x) = x {
                    let d = x - a;
                    let (i1, i2) = (Vec2::new(d.x * n.x, d.x * n.y), Vec2::new(d.y * n.x, d.y * n.y));
                    let sum = i1 + i2;
                    va.v.x = va.v.x - sum.x * vc.inv_mass_a;
                    va.v.y = va.v.y - sum.y * vc.inv_mass_a;
                    va.w = va.w - (cross(p1.r_a, i1) + cross(p2.r_a, i2)) * vc.inv_i_a;
                    vb.v.x = vb.v.x + sum.x * vc.inv_mass_b;
                    vb.v.y = vb.v.y + sum.y * vc.inv_mass_b;
                    vb.w = vb.w + (cross(p1.r_b, i1) + cross(p2.r_b, i2)) * vc.inv_i_b;
                    vc.points[0].normal_impulse = x.x;
                    vc.points[1].normal_impulse = x.y;
                }
            }
            store_velocities(vc, velocities, va, vb);
        }
    }

    /// StoreImpulses (180bc06f0): back into each contact's manifold.
    pub fn store_impulses(&self, manifolds: &mut [Manifold]) {
        for vc in &self.velocity_constraints {
            let manifold = &mut manifolds[vc.contact_index];
            for j in 0..vc.point_count {
                manifold.points[j].normal_impulse = vc.points[j].normal_impulse;
                manifold.points[j].tangent_impulse = vc.points[j].tangent_impulse;
            }
        }
    }

    /// SolvePositionConstraints (180bbee60).
    pub fn solve_position_constraints(&self, positions: &mut [Position]) -> bool {
        self.solve_positions(positions, BAUMGARTE, None) >= -3.0 * LINEAR_SLOP
    }

    /// SolveTOIPositionConstraints (180bbf3a0): only the two TOI bodies move.
    pub fn solve_toi_position_constraints(&self, positions: &mut [Position], toi_index_a: usize, toi_index_b: usize) -> bool {
        self.solve_positions(positions, TOI_BAUMGARTE, Some((toi_index_a, toi_index_b))) >= -1.5 * LINEAR_SLOP
    }

    /// The shared Gauss-Seidel position pass; returns the minimum separation seen.
    fn solve_positions(&self, positions: &mut [Position], baumgarte: f32, toi: Option<(usize, usize)>) -> f32 {
        let mut min_separation = 0.0f32;
        for pc in &self.position_constraints {
            let moves = |index: usize| toi.is_none_or(|(a, b)| index == a || index == b);
            let (m_a, i_a) = if moves(pc.index_a) { (pc.inv_mass_a, pc.inv_i_a) } else { (0.0, 0.0) };
            let (m_b, i_b) = if moves(pc.index_b) { (pc.inv_mass_b, pc.inv_i_b) } else { (0.0, 0.0) };
            let (mut pa, mut pb) = (positions[pc.index_a], positions[pc.index_b]);
            for j in 0..pc.point_count {
                let xf_a = body_transform(pa, pc.local_center_a);
                let xf_b = body_transform(pb, pc.local_center_b);
                let (n, point, separation) = position_manifold(pc, &xf_a, &xf_b, j);
                let r_a = point - pa.c;
                let r_b = point - pb.c;
                if separation <= min_separation {
                    min_separation = separation;
                }
                let rn_a = cross(r_a, n);
                let rn_b = cross(r_b, n);
                let k = rn_a * i_a * rn_a + (m_b + m_a) + rn_b * i_b * rn_b;
                let impulse = if k <= 0.0 {
                    0.0
                } else {
                    let c = (separation + LINEAR_SLOP) * baumgarte;
                    let c = if 0.0 <= c { 0.0 } else { c };
                    let c = if -MAX_LINEAR_CORRECTION <= c { c } else { -MAX_LINEAR_CORRECTION };
                    -(c / k)
                };
                let p = Vec2::new(n.x * impulse, n.y * impulse);
                pa.c.x = pa.c.x - pc.axis_a.x * (p.x * m_a);
                pa.c.y = pa.c.y - pc.axis_a.y * (p.y * m_a);
                pb.a = pb.a + cross(r_b, p) * i_b;
                pb.c.x = pb.c.x + pc.axis_b.x * (p.x * m_b);
                pa.a = pa.a - cross(r_a, p) * i_a;
                pb.c.y = pb.c.y + pc.axis_b.y * (p.y * m_b);
            }
            positions[pc.index_a] = pa;
            positions[pc.index_b] = pb;
        }
        min_separation
    }
}
