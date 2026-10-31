//! b2FrictionJoint (Unity's FrictionJoint2D), transcribed from UnityPlayer.dll (InitVelocityConstraints 180bc1db0,
//! SolveVelocityConstraints 180bc5390, SolvePositionConstraints 1800be020 returns true, GetReactionForce 180bc1c70,
//! GetReactionTorque 180bc1d80, SetMaxForce 180bc3bb0, SetMaxTorque 180bc3ce0).

use super::math::{Rot, Vec2};
use super::revolute::JointBody;
use super::solver::{Position, TimeStep, Velocity};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrictionDef {
    pub local_anchor_a: Vec2,
    pub local_anchor_b: Vec2,
    pub max_force: f32,
    pub max_torque: f32,
}

#[derive(Clone, Debug, Default)]
pub struct FrictionJoint {
    pub def: FrictionDef,
    pub linear_impulse: Vec2,
    pub angular_impulse: f32,
    pub(crate) a: JointBody,
    pub(crate) b: JointBody,
    pub(crate) r_a: Vec2,
    pub(crate) r_b: Vec2,
    /// b2Mat22 linearMass: ex.x, ex.y, ey.x, ey.y.
    pub(crate) linear_mass: [f32; 4],
    pub(crate) angular_mass: f32,
}

fn arm(q: Rot, x: f32, y: f32) -> Vec2 {
    Vec2::new(q.c * x - q.s * y, q.s * x + q.c * y)
}

impl FrictionJoint {
    pub fn new(def: FrictionDef) -> Self {
        Self { def, ..Self::default() }
    }

    /// b2FrictionJoint::GetReactionForce (180bc1c70).
    pub fn reaction_force(&self, inv_dt: f32) -> Vec2 {
        Vec2::new(inv_dt * self.linear_impulse.x, inv_dt * self.linear_impulse.y)
    }

    /// b2FrictionJoint::GetReactionTorque (180bc1d80).
    pub fn reaction_torque(&self, inv_dt: f32) -> f32 {
        inv_dt * self.angular_impulse
    }

    /// b2FrictionJoint::InitVelocityConstraints (180bc1db0).
    pub fn init_velocity_constraints(&mut self, a: JointBody, b: JointBody, step: &TimeStep, positions: &[Position], velocities: &mut [Velocity]) {
        self.a = a;
        self.b = b;
        let Velocity { v: mut va, w: mut wa } = velocities[a.index];
        let Velocity { v: mut vb, w: mut wb } = velocities[b.index];
        let (qa, qb) = (Rot::new(positions[a.index].a), Rot::new(positions[b.index].a));
        let d = &self.def;
        self.r_a = arm(qa, d.local_anchor_a.x - a.local_center.x, d.local_anchor_a.y - a.local_center.y);
        self.r_b = arm(qb, d.local_anchor_b.x - b.local_center.x, d.local_anchor_b.y - b.local_center.y);
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
        if step.warm_starting {
            self.linear_impulse = Vec2::new(step.dt_ratio * self.linear_impulse.x, step.dt_ratio * self.linear_impulse.y);
            self.angular_impulse *= step.dt_ratio;
            let (p, ang) = (self.linear_impulse, self.angular_impulse);
            va = Vec2::new(va.x - p.x * ma, va.y - p.y * ma);
            wa -= (p.y * ra.x - p.x * ra.y + ang) * ia;
            vb = Vec2::new(vb.x + p.x * mb, vb.y + p.y * mb);
            wb += (p.y * rb.x - p.x * rb.y + ang) * ib;
        } else {
            self.linear_impulse = Vec2::new(0.0, 0.0);
            self.angular_impulse = 0.0;
        }
        self.store(velocities, va, wa, vb, wb);
    }

    /// b2FrictionJoint::SolveVelocityConstraints (180bc5390).
    pub fn solve_velocity_constraints(&mut self, step: &TimeStep, velocities: &mut [Velocity]) {
        let (a, b, ra, rb) = (self.a, self.b, self.r_a, self.r_b);
        let (ma, mb, ia, ib) = (a.inv_mass, b.inv_mass, a.inv_i, b.inv_i);
        let Velocity { v: mut va, w: mut wa } = velocities[a.index];
        let Velocity { v: mut vb, w: mut wb } = velocities[b.index];
        let h = step.dt;
        // Angular friction.
        let max_impulse = h * self.def.max_torque;
        let old = self.angular_impulse;
        let new = -((wb - wa) * self.angular_mass) + old;
        let new = if new < max_impulse { new } else { max_impulse };
        self.angular_impulse = if -max_impulse > new { -max_impulse } else { new };
        let impulse = self.angular_impulse - old;
        wa -= impulse * ia;
        wb += impulse * ib;
        // Linear friction.
        let cy = wb * rb.x + vb.y - va.y - wa * ra.x;
        let cx = -(wb * rb.y) + vb.x - va.x - -(wa * ra.y);
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
