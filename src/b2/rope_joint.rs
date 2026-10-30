//! b2RopeJoint (Unity's DistanceJoint2D with maxDistanceOnly), transcribed from UnityPlayer.dll
//! (InitVelocityConstraints 180bc2fb0, SolveVelocityConstraints 180bc6320, SolvePositionConstraints 180bc4ac0,
//! GetReactionForce 180bc1d20, GetReactionTorque 1802b4ba0).

use super::math::{Rot, Vec2};
use super::revolute::JointBody;
use super::settings::globals;
use super::solver::{Position, TimeStep, Velocity};

/// b2_linearSlop.
const LINEAR_SLOP: f32 = f32::from_bits(0x3ba3_d70a);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RopeDef {
    pub local_anchor_a: Vec2,
    pub local_anchor_b: Vec2,
    pub max_length: f32,
}

#[derive(Clone, Debug, Default)]
pub struct RopeJoint {
    pub def: RopeDef,
    pub impulse: f32,
    pub(crate) a: JointBody,
    pub(crate) b: JointBody,
    pub u: Vec2,
    pub(crate) r_a: Vec2,
    pub(crate) r_b: Vec2,
    pub(crate) length: f32,
    pub(crate) mass: f32,
    /// e_inactiveLimit (0) or e_atUpperLimit (2).
    pub(crate) state: i32,
}

/// b2Rot applied to (anchor - local center), in the DLL's term order.
fn arm(q: Rot, x: f32, y: f32) -> Vec2 {
    Vec2::new(x * q.c - y * q.s, x * q.s + y * q.c)
}

impl RopeJoint {
    pub fn new(def: RopeDef) -> Self {
        Self { def, ..Self::default() }
    }

    /// b2RopeJoint::GetReactionForce (180bc1d20).
    pub fn reaction_force(&self, inv_dt: f32) -> Vec2 {
        let f = inv_dt * self.impulse;
        Vec2::new(f * self.u.x, f * self.u.y)
    }

    /// b2RopeJoint::InitVelocityConstraints (180bc2fb0).
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
        self.length = (self.u.x * self.u.x + self.u.y * self.u.y).sqrt();
        self.state = if self.length - d.max_length > 0.0 { 2 } else { 0 };
        if !(self.length > LINEAR_SLOP) {
            self.u = Vec2::new(0.0, 0.0);
            self.mass = 0.0;
            self.impulse = 0.0;
            return;
        }
        let inv = 1.0 / self.length;
        self.u = Vec2::new(inv * self.u.x, inv * self.u.y);
        let u = self.u;
        let cr_a = ra.x * u.y - ra.y * u.x;
        let cr_b = rb.x * u.y - rb.y * u.x;
        let inv_mass = a.inv_i * cr_a * cr_a + a.inv_mass + b.inv_mass + b.inv_i * cr_b * cr_b;
        self.mass = if inv_mass != 0.0 { 1.0 / inv_mass } else { 0.0 };
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

    /// b2RopeJoint::SolveVelocityConstraints (180bc6320).
    pub fn solve_velocity_constraints(&mut self, step: &TimeStep, velocities: &mut [Velocity]) {
        let (a, b, ra, rb, u) = (self.a, self.b, self.r_a, self.r_b, self.u);
        let Velocity { v: va, w: wa } = velocities[a.index];
        let Velocity { v: vb, w: wb } = velocities[b.index];
        let vpa = Vec2::new(va.x + -(wa * ra.y), va.y + wa * ra.x);
        let vpb = Vec2::new(vb.x + -(wb * rb.y), vb.y + wb * rb.x);
        let c = self.length - self.def.max_length;
        let mut cdot = (vpb.y - vpa.y) * u.y + (vpb.x - vpa.x) * u.x;
        if 0.0 > c {
            cdot += c * step.inv_dt;
        }
        let old = self.impulse;
        let new = -(cdot * self.mass) + old;
        self.impulse = if 0.0 < new { 0.0 } else { new };
        let impulse = self.impulse - old;
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

    /// b2RopeJoint::SolvePositionConstraints (180bc4ac0).
    pub fn solve_position_constraints(&self, positions: &mut [Position]) -> bool {
        let d = &self.def;
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
        let c = length - d.max_length;
        let c = if c < max_correction { c } else { max_correction };
        let c = if 0.0 > c { 0.0 } else { c };
        let impulse = -(c * self.mass);
        let p = Vec2::new(u.x * impulse, u.y * impulse);
        ca = Vec2::new(ca.x - a.inv_mass * (p.x * a.axis.x), ca.y - a.inv_mass * (p.y * a.axis.y));
        aa -= (ra.x * p.y - ra.y * p.x) * a.inv_i;
        cb = Vec2::new(cb.x + b.inv_mass * (p.x * b.axis.x), cb.y + b.inv_mass * (p.y * b.axis.y));
        ab += (rb.x * p.y - rb.y * p.x) * b.inv_i;
        positions[a.index] = Position { c: ca, a: aa };
        positions[b.index] = Position { c: cb, a: ab };
        LINEAR_SLOP > length - d.max_length
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step() -> TimeStep {
        TimeStep { dt: 0.02, inv_dt: 50.0, dt_ratio: 1.0, velocity_iterations: 8, position_iterations: 8, warm_starting: true }
    }

    /// A slack rope does nothing much; a taut one stops the body moving away and pulls it back.
    #[test]
    fn rope_only_pulls() {
        let a = JointBody { index: 0, local_center: Vec2::new(0.0, 0.0), inv_mass: 0.0, inv_i: 0.0, axis: Vec2::new(1.0, 1.0) };
        let b = JointBody { index: 1, local_center: Vec2::new(0.0, 0.0), inv_mass: 1.0, inv_i: 1.0, axis: Vec2::new(1.0, 1.0) };
        let mut j = RopeJoint::new(RopeDef { max_length: 1.0, ..Default::default() });
        let mut positions = vec![Position::default(), Position { c: Vec2::new(0.5, 0.0), a: 0.0 }];
        let mut velocities = vec![Velocity::default(), Velocity { v: Vec2::new(3.0, 0.0), w: 0.0 }];
        j.init_velocity_constraints(a, b, &step(), &positions, &mut velocities);
        assert_eq!(j.state, 0);
        positions[1].c = Vec2::new(1.5, 0.0);
        j.init_velocity_constraints(a, b, &step(), &positions, &mut velocities);
        assert_eq!(j.state, 2);
        for _ in 0..8 {
            j.solve_velocity_constraints(&step(), &mut velocities);
        }
        assert!(velocities[1].v.x.abs() < 1e-3, "{:?}", velocities[1]);
        assert!(j.impulse < 0.0);
        let mut solved = false;
        for _ in 0..20 {
            solved = j.solve_position_constraints(&mut positions);
        }
        assert!(solved && positions[1].c.x < 1.01);
    }
}
