//! b2MotorJoint (Unity's RelativeJoint2D), transcribed from UnityPlayer.dll (constructor 180bc6ad0,
//! InitVelocityConstraints 180bc79d0, SolveVelocityConstraints 180bca1d0, SolvePositionConstraints 1800be020 returns
//! true, GetReactionForce 180bc7350, GetReactionTorque 180bc7420, SetMaxForce 180bc9110, SetMaxTorque 180bc3bb0,
//! SetCorrectionFactor 180bc3ce0).

use super::math::{Rot, Vec2};
use super::revolute::JointBody;
use super::solver::{Position, TimeStep, Velocity};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MotorDef {
    pub linear_offset: Vec2,
    pub angular_offset: f32,
    pub max_force: f32,
    pub max_torque: f32,
    pub correction_factor: f32,
}

#[derive(Clone, Debug, Default)]
pub struct MotorJoint {
    pub def: MotorDef,
    pub linear_impulse: Vec2,
    pub angular_impulse: f32,
    pub(crate) a: JointBody,
    pub(crate) b: JointBody,
    pub(crate) r_a: Vec2,
    pub(crate) r_b: Vec2,
    pub(crate) linear_error: Vec2,
    pub(crate) angular_error: f32,
    /// b2Mat22 linearMass: ex.x, ex.y, ey.x, ey.y.
    pub(crate) linear_mass: [f32; 4],
    pub(crate) angular_mass: f32,
}

impl MotorJoint {
    pub fn new(def: MotorDef) -> Self {
        Self { def, ..Self::default() }
    }

    /// b2MotorJoint::GetReactionForce (180bc7350).
    pub fn reaction_force(&self, inv_dt: f32) -> Vec2 {
        Vec2::new(inv_dt * self.linear_impulse.x, inv_dt * self.linear_impulse.y)
    }

    /// b2MotorJoint::GetReactionTorque (180bc7420).
    pub fn reaction_torque(&self, inv_dt: f32) -> f32 {
        inv_dt * self.angular_impulse
    }

    /// b2MotorJoint::InitVelocityConstraints (180bc79d0).
    pub fn init_velocity_constraints(&mut self, a: JointBody, b: JointBody, step: &TimeStep, positions: &[Position], velocities: &mut [Velocity]) {
        self.a = a;
        self.b = b;
        let Velocity { v: mut va, w: mut wa } = velocities[a.index];
        let Velocity { v: mut vb, w: mut wb } = velocities[b.index];
        let (ca, aa) = (positions[a.index].c, positions[a.index].a);
        let (cb, ab) = (positions[b.index].c, positions[b.index].a);
        let (qa, qb) = (Rot::new(aa), Rot::new(ab));
        let d = &self.def;
        // rA = Mul(qA, linearOffset - localCenterA), rB = Mul(qB, -localCenterB)
        let (dx, dy) = (d.linear_offset.x - a.local_center.x, d.linear_offset.y - a.local_center.y);
        self.r_a = Vec2::new(qa.c * dx - qa.s * dy, qa.s * dx + qa.c * dy);
        let (nx, ny) = (-b.local_center.x, -b.local_center.y);
        self.r_b = Vec2::new(qb.c * nx - qb.s * ny, qb.s * nx + qb.c * ny);
        let (ra, rb) = (self.r_a, self.r_b);
        let (ma, mb, ia, ib) = (a.inv_mass, b.inv_mass, a.inv_i, b.inv_i);
        let mab = mb + ma;
        let exx = ra.y * ia * ra.y + mab + rb.y * ib * rb.y;
        let exy = -(ra.y * (ra.x * ia)) - rb.y * (rb.x * ib);
        let eyy = ra.x * (ra.x * ia) + mab + rb.x * (rb.x * ib);
        let mut det = exx * eyy - exy * exy;
        if det != 0.0 {
            det = 1.0 / det;
        }
        self.linear_mass = [det * eyy, -(det * exy), -(det * exy), det * exx];
        self.angular_mass = ib + ia;
        if self.angular_mass > 0.0 {
            self.angular_mass = 1.0 / self.angular_mass;
        }
        // linearError = cB + rB - cA - rA, angularError = aB - aA - angularOffset
        self.linear_error = Vec2::new(cb.x + rb.x - ca.x - ra.x, cb.y + rb.y - ca.y - ra.y);
        self.angular_error = ab - aa - d.angular_offset;
        if step.warm_starting {
            self.linear_impulse = Vec2::new(step.dt_ratio * self.linear_impulse.x, step.dt_ratio * self.linear_impulse.y);
            self.angular_impulse = step.dt_ratio * self.angular_impulse;
            let (p, ang) = (self.linear_impulse, self.angular_impulse);
            va = Vec2::new(va.x - p.x * ma, va.y - p.y * ma);
            wa -= (ra.x * p.y - ra.y * p.x + ang) * ia;
            vb = Vec2::new(vb.x + p.x * mb, vb.y + p.y * mb);
            wb += (p.y * rb.x - p.x * rb.y + ang) * ib;
        } else {
            self.linear_impulse = Vec2::new(0.0, 0.0);
            self.angular_impulse = 0.0;
        }
        self.store(velocities, va, wa, vb, wb);
    }

    /// b2MotorJoint::SolveVelocityConstraints (180bca1d0).
    pub fn solve_velocity_constraints(&mut self, step: &TimeStep, velocities: &mut [Velocity]) {
        let (a, b, ra, rb) = (self.a, self.b, self.r_a, self.r_b);
        let (ma, mb, ia, ib) = (a.inv_mass, b.inv_mass, a.inv_i, b.inv_i);
        let Velocity { v: mut va, w: mut wa } = velocities[a.index];
        let Velocity { v: mut vb, w: mut wb } = velocities[b.index];
        let h = step.dt;
        let k = step.inv_dt * self.def.correction_factor;
        // Angular.
        let max_impulse = h * self.def.max_torque;
        let old = self.angular_impulse;
        let cdot = k * self.angular_error + (wb - wa);
        let new = -(cdot * self.angular_mass) + old;
        let new = if new < max_impulse { new } else { max_impulse };
        self.angular_impulse = if -max_impulse > new { -max_impulse } else { new };
        let impulse = self.angular_impulse - old;
        wa -= impulse * ia;
        wb += impulse * ib;
        // Linear.
        let cy = wb * rb.x + vb.y - va.y - wa * ra.x + k * self.linear_error.y;
        let cx = -(wb * rb.y) + vb.x - va.x - -(wa * ra.y) + k * self.linear_error.x;
        let m = self.linear_mass;
        let ix = -(cy * m[2] + cx * m[0]);
        let iy = -(cy * m[3] + cx * m[1]);
        let old = self.linear_impulse;
        self.linear_impulse = Vec2::new(ix + old.x, iy + old.y);
        let max_impulse = h * self.def.max_force;
        let li = self.linear_impulse;
        if li.x * li.x + li.y * li.y > max_impulse * max_impulse {
            self.linear_impulse.normalize();
            self.linear_impulse = Vec2::new(max_impulse * self.linear_impulse.x, max_impulse * self.linear_impulse.y);
        }
        let (ix, iy) = (self.linear_impulse.x - old.x, self.linear_impulse.y - old.y);
        va = Vec2::new(va.x - ix * ma, va.y - iy * ma);
        wa -= (iy * ra.x - ix * ra.y) * ia;
        vb = Vec2::new(vb.x + ix * mb, vb.y + iy * mb);
        wb += (iy * rb.x - ix * rb.y) * ib;
        self.store(velocities, va, wa, vb, wb);
    }

    /// The final stores mask linear velocity by each end's Unity constraint axis.
    fn store(&self, velocities: &mut [Velocity], va: Vec2, wa: f32, vb: Vec2, wb: f32) {
        let (a, b) = (self.a, self.b);
        velocities[a.index] = Velocity { v: Vec2::new(va.x * a.axis.x, va.y * a.axis.y), w: wa };
        velocities[b.index] = Velocity { v: Vec2::new(vb.x * b.axis.x, vb.y * b.axis.y), w: wb };
    }
}
