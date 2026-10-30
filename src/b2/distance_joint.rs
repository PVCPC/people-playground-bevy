//! b2DistanceJoint (Unity's SpringJoint2D), transcribed from UnityPlayer.dll.

use super::math::{Rot, Vec2};
use super::revolute::JointBody;
use super::settings::globals;
use super::solver::{Position, TimeStep, Velocity};

/// b2_linearSlop.
const LINEAR_SLOP: f32 = f32::from_bits(0x3ba3_d70a);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DistanceDef {
    pub local_anchor_a: Vec2,
    pub local_anchor_b: Vec2,
    pub length: f32,
    pub frequency_hz: f32,
    pub damping_ratio: f32,
}

#[derive(Clone, Debug, Default)]
pub struct DistanceJoint {
    pub def: DistanceDef,
    pub impulse: f32,
    pub(crate) gamma: f32,
    pub(crate) bias: f32,
    pub(crate) a: JointBody,
    pub(crate) b: JointBody,
    pub u: Vec2,
    pub(crate) r_a: Vec2,
    pub(crate) r_b: Vec2,
    pub(crate) mass: f32,
}

/// b2Rot applied to (anchor - local center), in the DLL's term order.
fn arm(q: Rot, x: f32, y: f32) -> Vec2 {
    Vec2::new(x * q.c - y * q.s, x * q.s + y * q.c)
}

impl DistanceJoint {
    /// b2DistanceJoint::b2DistanceJoint (180bc6a50).
    pub fn new(def: DistanceDef) -> Self {
        Self { def, ..Self::default() }
    }

    /// b2DistanceJoint::GetReactionForce (180bc7320).
    pub fn reaction_force(&self, inv_dt: f32) -> Vec2 {
        let f = inv_dt * self.impulse;
        Vec2::new(f * self.u.x, f * self.u.y)
    }

    /// b2DistanceJoint::InitVelocityConstraints (180bc7440).
    pub fn init_velocity_constraints(&mut self, a: JointBody, b: JointBody, step: &TimeStep, positions: &[Position], velocities: &mut [Velocity]) {
        self.a = a;
        self.b = b;
        let (pa, pb) = (positions[a.index], positions[b.index]);
        let Velocity { v: mut va, w: mut wa } = velocities[a.index];
        let Velocity { v: mut vb, w: mut wb } = velocities[b.index];
        let (qa, qb) = (Rot::new(pa.a), Rot::new(pb.a));
        let d = &self.def;
        self.r_a = arm(qa, d.local_anchor_a.x - a.local_center.x, d.local_anchor_a.y - a.local_center.y);
        self.r_b = arm(qb, d.local_anchor_b.x - b.local_center.x, d.local_anchor_b.y - b.local_center.y);
        let (ra, rb) = (self.r_a, self.r_b);
        self.u = Vec2::new(pb.c.x + rb.x - pa.c.x - ra.x, pb.c.y + rb.y - pa.c.y - ra.y);
        let length = (self.u.x * self.u.x + self.u.y * self.u.y).sqrt();
        if length > LINEAR_SLOP {
            let inv = 1.0 / length;
            self.u = Vec2::new(inv * self.u.x, inv * self.u.y);
        } else {
            self.u = Vec2::new(0.0, 0.0);
        }
        let u = self.u;
        let cr_a = u.y * ra.x - ra.y * u.x;
        let cr_b = u.y * rb.x - rb.y * u.x;
        let mut inv_mass = a.inv_i * cr_a * cr_a + a.inv_mass + b.inv_mass + b.inv_i * cr_b * cr_b;
        self.mass = if inv_mass != 0.0 { 1.0 / inv_mass } else { 0.0 };
        if d.frequency_hz > 0.0 {
            let c = length - d.length;
            let omega = d.frequency_hz * f32::from_bits(0x40c9_0fdb);
            let damping = (self.mass + self.mass) * d.damping_ratio * omega;
            let k = self.mass * omega * omega;
            let h = step.dt;
            let gamma = (damping + h * k) * h;
            self.gamma = if gamma != 0.0 { 1.0 / gamma } else { 0.0 };
            self.bias = h * c * k * self.gamma;
            inv_mass += self.gamma;
            self.mass = if inv_mass != 0.0 { 1.0 / inv_mass } else { 0.0 };
        } else {
            self.gamma = 0.0;
            self.bias = 0.0;
        }
        if step.warm_starting {
            self.impulse *= step.dt_ratio;
            let p = Vec2::new(self.impulse * u.x, self.impulse * u.y);
            va = Vec2::new(va.x - p.x * a.inv_mass, va.y - p.y * a.inv_mass);
            wa -= (p.y * ra.x - p.x * ra.y) * a.inv_i;
            vb = Vec2::new(vb.x + p.x * b.inv_mass, vb.y + p.y * b.inv_mass);
            wb += (p.y * rb.x - p.x * rb.y) * b.inv_i;
        } else {
            self.impulse = 0.0;
        }
        self.store(velocities, va, wa, vb, wb);
    }

    /// b2DistanceJoint::SolveVelocityConstraints (180bc9f80).
    pub fn solve_velocity_constraints(&mut self, velocities: &mut [Velocity]) {
        let (a, b, ra, rb, u) = (self.a, self.b, self.r_a, self.r_b, self.u);
        let Velocity { v: va, w: wa } = velocities[a.index];
        let Velocity { v: vb, w: wb } = velocities[b.index];
        let vpa = Vec2::new(va.x - wa * ra.y, va.y + wa * ra.x);
        let vpb = Vec2::new(vb.x - wb * rb.y, vb.y + wb * rb.x);
        let cdot = (vpb.y - vpa.y) * u.y + (vpb.x - vpa.x) * u.x;
        let impulse = -((cdot + self.bias + self.impulse * self.gamma) * self.mass);
        self.impulse += impulse;
        let p = Vec2::new(impulse * u.x, impulse * u.y);
        let va = Vec2::new(va.x - a.inv_mass * p.x, va.y - a.inv_mass * p.y);
        let wa = wa - (p.y * ra.x - p.x * ra.y) * a.inv_i;
        let vb = Vec2::new(vb.x + b.inv_mass * p.x, vb.y + b.inv_mass * p.y);
        let wb = wb + (p.y * rb.x - p.x * rb.y) * b.inv_i;
        self.store(velocities, va, wa, vb, wb);
    }

    /// The final stores mask linear velocity by each end's Unity constraint axis.
    fn store(&self, velocities: &mut [Velocity], va: Vec2, wa: f32, vb: Vec2, wb: f32) {
        let (a, b) = (self.a, self.b);
        velocities[a.index] = Velocity { v: Vec2::new(va.x * a.axis.x, va.y * a.axis.y), w: wa };
        velocities[b.index] = Velocity { v: Vec2::new(vb.x * b.axis.x, vb.y * b.axis.y), w: wb };
    }

    /// b2DistanceJoint::SolvePositionConstraints (180bc9370): a soft (frequency > 0) joint never corrects.
    pub fn solve_position_constraints(&self, positions: &mut [Position]) -> bool {
        let d = &self.def;
        if d.frequency_hz > 0.0 {
            return true;
        }
        let (a, b) = (self.a, self.b);
        let (Position { c: mut ca, a: mut aa }, Position { c: mut cb, a: mut ab }) = (positions[a.index], positions[b.index]);
        let (qa, qb) = (Rot::new(aa), Rot::new(ab));
        let ra = arm(qa, d.local_anchor_a.x - a.local_center.x, d.local_anchor_a.y - a.local_center.y);
        let rb = arm(qb, d.local_anchor_b.x - b.local_center.x, d.local_anchor_b.y - b.local_center.y);
        let mut u = Vec2::new(cb.x + rb.x - ca.x - ra.x, cb.y + rb.y - ca.y - ra.y);
        // b2Vec2::Normalize: the length is 0 below epsilon.
        let mut length = (u.x * u.x + u.y * u.y).sqrt();
        if length < f32::EPSILON {
            length = 0.0;
        } else {
            let inv = 1.0 / length;
            u = Vec2::new(u.x * inv, u.y * inv);
        }
        let max_correction = globals().max_linear_correction;
        let c = max_correction.min(length - d.length).max(-max_correction);
        let impulse = -(c * self.mass);
        let p = Vec2::new(u.x * impulse, u.y * impulse);
        ca = Vec2::new(ca.x - a.inv_mass * (p.x * a.axis.x), ca.y - a.inv_mass * (p.y * a.axis.y));
        aa -= (ra.x * p.y - ra.y * p.x) * a.inv_i;
        cb = Vec2::new(cb.x + b.inv_mass * (p.x * b.axis.x), cb.y + b.inv_mass * (p.y * b.axis.y));
        ab += (rb.x * p.y - rb.y * p.x) * b.inv_i;
        positions[a.index] = Position { c: ca, a: aa };
        positions[b.index] = Position { c: cb, a: ab };
        c.abs() < LINEAR_SLOP
    }
}
