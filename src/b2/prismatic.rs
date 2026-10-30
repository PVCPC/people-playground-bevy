//! b2PrismaticJoint (Unity's SliderJoint2D), transcribed from UnityPlayer.dll (InitVelocityConstraints 180bc2270,
//! SolveVelocityConstraints 180bc56f0, SolvePositionConstraints 180bc3e10, GetReactionForce 180bc1c90,
//! GetReactionTorque 180bc1d90, GetJointTranslation 180bc1b60, GetJointSpeed 180bc1950, EnableLimit 180bc1550,
//! EnableMotor 180bc1690, SetLimits 180bc3a70, SetMaxMotorForce 180bc3bc0, SetMotorSpeed 180bc3cf0), plus the native
//! SliderJoint2D side that builds its definition (SliderJoint2D::Create 180c04400, Reset 180c05740, SetAngle
//! 180c05890, SetLimits 180c06000, SetMotor 180c06190, SetUseLimits 180c062d0, SetUseMotor 180c062f0).

use super::math::{Rot, Vec2};
use super::revolute::JointBody;
use super::settings::globals;
use super::solver::{Position, TimeStep, Velocity};
use super::weld::{arm, solve22, solve33, Mat33};
use super::world::{Body, JointId, World};

/// b2_linearSlop and b2_angularSlop (2 degrees) as the solver's final comparisons load them.
const LINEAR_SLOP: f32 = f32::from_bits(0x3ba3_d70a);
const ANGULAR_SLOP: f32 = f32::from_bits(0x3d0e_fa36);
/// 2 * b2_linearSlop, the equal-limits test in InitVelocityConstraints.
const TWO_SLOP: f32 = f32::from_bits(0x3c23_d70a);
/// Degrees to radians as SliderJoint2D::Create multiplies the angle.
const DEG2RAD: f32 = f32::from_bits(0x3c8e_fa35);
/// Radians to degrees as SliderJoint2D::Create multiplies the auto-configured angle.
const RAD2DEG: f32 = f32::from_bits(0x4265_2ee1);

/// minss dst, src: the second operand when the comparison fails or is unordered.
fn minss(dst: f32, src: f32) -> f32 {
    if dst < src { dst } else { src }
}

/// maxss dst, src
fn maxss(dst: f32, src: f32) -> f32 {
    if dst > src { dst } else { src }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PrismaticDef {
    pub local_anchor_a: Vec2,
    pub local_anchor_b: Vec2,
    pub local_axis_a: Vec2,
    pub reference_angle: f32,
    pub enable_limit: bool,
    pub lower_translation: f32,
    pub upper_translation: f32,
    pub enable_motor: bool,
    pub max_motor_force: f32,
    pub motor_speed: f32,
}

#[derive(Clone, Debug, Default)]
pub struct PrismaticJoint {
    /// The live definition: the Enable/Set calls change it.
    pub def: PrismaticDef,
    /// m_localXAxisA (0xa8) and m_localYAxisA (0xb0)
    pub(crate) local_x_axis_a: Vec2,
    pub(crate) local_y_axis_a: Vec2,
    /// m_impulse (0xbc)
    pub impulse: [f32; 3],
    /// m_motorImpulse (0xc8)
    pub motor_impulse: f32,
    /// m_limitState (0xe0): 0 inactive, 1 at lower, 2 at upper, 3 equal.
    pub limit_state: u32,
    pub(crate) a: JointBody,
    pub(crate) b: JointBody,
    pub(crate) axis: Vec2,
    pub(crate) perp: Vec2,
    pub(crate) s1: f32,
    pub(crate) s2: f32,
    pub(crate) a1: f32,
    pub(crate) a2: f32,
    pub(crate) k: Mat33,
    pub(crate) motor_mass: f32,
}

/// b2Vec2::Normalize (180bae620): a vector shorter than epsilon is left alone (and the length returned as zero).
fn normalize(v: Vec2) -> Vec2 {
    let length = (v.x * v.x + v.y * v.y).sqrt();
    if f32::from_bits(0x3400_0000) > length {
        return v;
    }
    let inv = 1.0 / length;
    Vec2::new(v.x * inv, v.y * inv)
}

impl PrismaticJoint {
    /// b2PrismaticJoint's constructor (b2Joint::Create's case 2): the axis is normalized, its perpendicular is
    /// (-y, x), and every impulse starts at zero.
    pub fn new(def: PrismaticDef) -> Self {
        let x = normalize(def.local_axis_a);
        Self { def, local_x_axis_a: x, local_y_axis_a: Vec2::new(x.y * -1.0, x.x), ..Self::default() }
    }

    /// m_localXAxisA
    pub fn local_axis(&self) -> Vec2 {
        self.local_x_axis_a
    }

    /// b2PrismaticJoint::GetReactionForce (180bc1c90)
    pub fn reaction_force(&self, inv_dt: f32) -> Vec2 {
        let axial = self.motor_impulse + self.impulse[2];
        let ix = self.impulse[0];
        Vec2::new((ix * self.perp.x + axial * self.axis.x) * inv_dt, (ix * self.perp.y + axial * self.axis.y) * inv_dt)
    }

    /// b2PrismaticJoint::GetReactionTorque (180bc1d90)
    pub fn reaction_torque(&self, inv_dt: f32) -> f32 {
        inv_dt * self.impulse[1]
    }

    /// b2PrismaticJoint::GetMotorForce (180bc1c50)
    pub fn motor_force(&self, inv_dt: f32) -> f32 {
        inv_dt * self.motor_impulse
    }

    /// b2PrismaticJoint::GetJointTranslation (180bc1b60) from the two bodies' transforms.
    pub fn joint_translation(&self, ba: &Body, bb: &Body) -> f32 {
        let d = &self.def;
        let (xa, xb) = (&ba.xf, &bb.xf);
        let pa = Vec2::new((d.local_anchor_a.x * xa.q.c - xa.q.s * d.local_anchor_a.y) + xa.p.x, (xa.q.s * d.local_anchor_a.x + d.local_anchor_a.y * xa.q.c) + xa.p.y);
        let pb = Vec2::new((d.local_anchor_b.x * xb.q.c - xb.q.s * d.local_anchor_b.y) + xb.p.x, (xb.q.s * d.local_anchor_b.x + d.local_anchor_b.y * xb.q.c) + xb.p.y);
        let (dx, dy) = (pb.x - pa.x, pb.y - pa.y);
        let (lx, ly) = (self.local_x_axis_a.x, self.local_x_axis_a.y);
        let ax = lx * xa.q.c - xa.q.s * ly;
        let ay = xa.q.s * lx + ly * xa.q.c;
        ay * dy + ax * dx
    }

    /// b2PrismaticJoint::GetJointSpeed (180bc1950) from the two bodies' state.
    pub fn joint_speed(&self, ba: &Body, bb: &Body) -> f32 {
        let d = &self.def;
        let (xa, xb) = (&ba.xf, &bb.xf);
        let (ax, ay) = (d.local_anchor_a.x - ba.sweep.local_center.x, d.local_anchor_a.y - ba.sweep.local_center.y);
        let ra = Vec2::new(xa.q.c * ax - xa.q.s * ay, xa.q.s * ax + xa.q.c * ay);
        let (bx, by) = (d.local_anchor_b.x - bb.sweep.local_center.x, d.local_anchor_b.y - bb.sweep.local_center.y);
        let rb = Vec2::new(xb.q.c * bx - xb.q.s * by, xb.q.s * bx + xb.q.c * by);
        let (dx, dy) = ((rb.x + bb.sweep.c.x) - (ra.x + ba.sweep.c.x), (rb.y + bb.sweep.c.y) - (ra.y + ba.sweep.c.y));
        let (lx, ly) = (self.local_x_axis_a.x, self.local_x_axis_a.y);
        let axis = Vec2::new(lx * xa.q.c - xa.q.s * ly, xa.q.s * lx + ly * xa.q.c);
        let (wa, wb) = (ba.angular_velocity, bb.angular_velocity);
        let vx = (bb.linear_velocity.x + -(rb.y * wb)) - ba.linear_velocity.x - -(ra.y * wa);
        let vy = (bb.linear_velocity.y + rb.x * wb) - ba.linear_velocity.y - ra.x * wa;
        (axis.x * vx + axis.y * vy) + (dx * -(axis.y * wa) + dy * (axis.x * wa))
    }

    /// b2PrismaticJoint::InitVelocityConstraints (180bc2270).
    pub fn init_velocity_constraints(&mut self, a: JointBody, b: JointBody, step: &TimeStep, positions: &[Position], velocities: &mut [Velocity]) {
        self.a = a;
        self.b = b;
        let (pa, pb) = (positions[a.index], positions[b.index]);
        let Velocity { v: mut va, w: mut wa } = velocities[a.index];
        let Velocity { v: mut vb, w: mut wb } = velocities[b.index];
        let (qa, qb) = (Rot::new(pa.a), Rot::new(pb.a));
        let d = self.def;
        let ra = arm(qa, d.local_anchor_a.x - a.local_center.x, d.local_anchor_a.y - a.local_center.y);
        let rb = arm(qb, d.local_anchor_b.x - b.local_center.x, d.local_anchor_b.y - b.local_center.y);
        let (dcx, dcy) = (pb.c.x - pa.c.x, pb.c.y - pa.c.y);
        let (dx, dy) = ((rb.x + dcx) - ra.x, (rb.y + dcy) - ra.y);
        let axis = arm(qa, self.local_x_axis_a.x, self.local_x_axis_a.y);
        let (ex, ey) = (dx + ra.x, dy + ra.y);
        let sum = b.inv_mass + a.inv_mass;
        let a1 = ex * axis.y - ey * axis.x;
        let a2 = rb.x * axis.y - rb.y * axis.x;
        let k33 = ((a1 * a.inv_i) * a1 + sum) + (a2 * b.inv_i) * a2;
        self.motor_mass = if k33 > 0.0 { 1.0 / k33 } else { k33 };
        let perp = arm(qa, self.local_y_axis_a.x, self.local_y_axis_a.y);
        let s1 = ex * perp.y - ey * perp.x;
        let s2 = rb.x * perp.y - rb.y * perp.x;
        let s1ia = s1 * a.inv_i;
        let s2ib = s2 * b.inv_i;
        let k11 = (s1 * s1ia + sum) + s2 * s2ib;
        let k12 = s2ib + s1ia;
        let k13 = a2 * s2ib + a1 * s1ia;
        let mut k22 = b.inv_i + a.inv_i;
        if k22 == 0.0 {
            k22 = 1.0;
        }
        let k23 = a1 * a.inv_i + a2 * b.inv_i;
        self.axis = axis;
        self.perp = perp;
        self.s1 = s1;
        self.s2 = s2;
        self.a1 = a1;
        self.a2 = a2;
        self.k = [k11, k12, k13, k12, k22, k23, k13, k23, k33];
        if d.enable_limit {
            let t = dy * axis.y + dx * axis.x;
            let (lower, upper) = (d.lower_translation, d.upper_translation);
            if TWO_SLOP > (upper - lower).abs() {
                self.limit_state = 3;
            } else {
                if lower >= t {
                    if self.limit_state != 1 {
                        self.limit_state = 1;
                        self.impulse[2] = 0.0;
                    }
                } else if t >= upper {
                    if self.limit_state != 2 {
                        self.limit_state = 2;
                        self.impulse[2] = 0.0;
                    }
                } else {
                    self.limit_state = 0;
                    self.impulse[2] = 0.0;
                }
            }
        } else {
            self.limit_state = 0;
            self.impulse[2] = 0.0;
        }
        if !d.enable_motor {
            self.motor_impulse = 0.0;
        }
        if step.warm_starting {
            let ratio = step.dt_ratio;
            self.impulse = [self.impulse[0] * ratio, self.impulse[1] * ratio, self.impulse[2] * ratio];
            self.motor_impulse *= ratio;
            let ix = self.impulse[0];
            let axial = self.motor_impulse + self.impulse[2];
            let px = ix * perp.x + axial * axis.x;
            let py = ix * perp.y + axial * axis.y;
            let la = (ix * s1 + self.impulse[1]) + axial * a1;
            let lb = (ix * s2 + self.impulse[1]) + axial * a2;
            va = Vec2::new(va.x - px * a.inv_mass, va.y - py * a.inv_mass);
            wa -= la * a.inv_i;
            vb = Vec2::new(vb.x + px * b.inv_mass, vb.y + py * b.inv_mass);
            wb += lb * b.inv_i;
        } else {
            self.impulse = [0.0; 3];
            self.motor_impulse = 0.0;
        }
        self.store(velocities, va, wa, vb, wb);
    }

    /// The final stores mask linear velocity by each end's Unity constraint axis.
    fn store(&self, velocities: &mut [Velocity], va: Vec2, wa: f32, vb: Vec2, wb: f32) {
        let (a, b) = (self.a, self.b);
        velocities[a.index] = Velocity { v: Vec2::new(va.x * a.axis.x, va.y * a.axis.y), w: wa };
        velocities[b.index] = Velocity { v: Vec2::new(vb.x * b.axis.x, vb.y * b.axis.y), w: wb };
    }

    /// b2PrismaticJoint::SolveVelocityConstraints (180bc56f0).
    pub fn solve_velocity_constraints(&mut self, step: &TimeStep, velocities: &mut [Velocity]) {
        let (a, b, axis, perp) = (self.a, self.b, self.axis, self.perp);
        let Velocity { v: mut va, w: mut wa } = velocities[a.index];
        let Velocity { v: mut vb, w: mut wb } = velocities[b.index];
        if self.def.enable_motor && self.limit_state != 3 {
            let old = self.motor_impulse;
            let max_impulse = self.def.max_motor_force * step.dt;
            let dot = (vb.y - va.y) * axis.y + (vb.x - va.x) * axis.x;
            let cdot = (dot + self.a2 * wb) - self.a1 * wa;
            let unclamped = (self.def.motor_speed - cdot) * self.motor_mass + old;
            let new = maxss(-max_impulse, minss(unclamped, max_impulse));
            self.motor_impulse = new;
            let p = new - old;
            let (px, py) = (p * axis.x, p * axis.y);
            va = Vec2::new(va.x - px * a.inv_mass, va.y - py * a.inv_mass);
            wa -= (self.a1 * p) * a.inv_i;
            vb = Vec2::new(vb.x + px * b.inv_mass, vb.y + py * b.inv_mass);
            wb += (self.a2 * p) * b.inv_i;
        }
        let (dvx, dvy) = (vb.x - va.x, vb.y - va.y);
        let c1x = ((dvy * perp.y + dvx * perp.x) + wb * self.s2) - wa * self.s1;
        let c1y = wb - wa;
        let (px, py, la, lb);
        if self.def.enable_limit && self.limit_state != 0 {
            let cdot = ((dvy * axis.y + dvx * axis.x) + wb * self.a2) - wa * self.a1;
            let f1 = self.impulse;
            let rhs = [-c1x, -c1y, -cdot];
            let df = solve33(&self.k, rhs);
            self.impulse[0] = df[0] + f1[0];
            self.impulse[1] = df[1] + f1[1];
            self.impulse[2] = df[2] + f1[2];
            match self.limit_state {
                1 => self.impulse[2] = maxss(self.impulse[2], 0.0),
                2 => self.impulse[2] = minss(self.impulse[2], 0.0),
                _ => {}
            }
            let dz = self.impulse[2] - f1[2];
            let reduced = [rhs[0] - dz * self.k[6], rhs[1] - dz * self.k[7]];
            let f2 = solve22(&self.k, reduced);
            self.impulse[0] = f1[0] + f2[0];
            self.impulse[1] = f1[1] + f2[1];
            let (dfx, dfy) = (self.impulse[0] - f1[0], self.impulse[1] - f1[1]);
            let dz = self.impulse[2] - f1[2];
            py = dfx * perp.y + dz * axis.y;
            px = dfx * perp.x + dz * axis.x;
            la = (dfx * self.s1 + dfy) + dz * self.a1;
            lb = (dfx * self.s2 + dfy) + dz * self.a2;
        } else {
            let f = solve22(&self.k, [-c1x, -c1y]);
            self.impulse[0] = f[0] + self.impulse[0];
            self.impulse[1] = f[1] + self.impulse[1];
            la = f[0] * self.s1 + f[1];
            lb = f[0] * self.s2 + f[1];
            py = f[0] * perp.y;
            px = f[0] * perp.x;
        }
        va = Vec2::new(va.x - px * a.inv_mass, va.y - py * a.inv_mass);
        wa -= la * a.inv_i;
        vb = Vec2::new(b.inv_mass * px + vb.x, b.inv_mass * py + vb.y);
        wb = lb * b.inv_i + wb;
        self.store(velocities, va, wa, vb, wb);
    }

    /// b2PrismaticJoint::SolvePositionConstraints (180bc3e10).
    pub fn solve_position_constraints(&self, positions: &mut [Position]) -> bool {
        let max_linear_correction = globals().max_linear_correction;
        let (a, b, d) = (self.a, self.b, &self.def);
        let (Position { c: ca, a: aa }, Position { c: cb, a: ab }) = (positions[a.index], positions[b.index]);
        let (qa, qb) = (Rot::new(aa), Rot::new(ab));
        let ra = arm(qa, d.local_anchor_a.x - a.local_center.x, d.local_anchor_a.y - a.local_center.y);
        let rb = arm(qb, d.local_anchor_b.x - b.local_center.x, d.local_anchor_b.y - b.local_center.y);
        let (dx, dy) = (((cb.x + rb.x) - ca.x) - ra.x, ((cb.y + rb.y) - ca.y) - ra.y);
        let (ex, ey) = (ra.x + dx, ra.y + dy);
        let axis = arm(qa, self.local_x_axis_a.x, self.local_x_axis_a.y);
        let a1 = axis.y * ex - axis.x * ey;
        let a2 = rb.x * axis.y - rb.y * axis.x;
        let perp = arm(qa, self.local_y_axis_a.x, self.local_y_axis_a.y);
        let s1 = perp.y * ex - perp.x * ey;
        let s2 = rb.x * perp.y - rb.y * perp.x;
        let c1x = dx * perp.x + dy * perp.y;
        let c1y = (ab - aa) - d.reference_angle;
        let mut linear_error = c1x.abs();
        let angular_error = c1y.abs();
        let mut c2 = 0.0f32;
        let mut limited = false;
        if d.enable_limit {
            let t = dx * axis.x + dy * axis.y;
            let (lower, upper) = (d.lower_translation, d.upper_translation);
            if TWO_SLOP > (upper - lower).abs() {
                c2 = maxss(-max_linear_correction, minss(t, max_linear_correction));
                if !(linear_error > t.abs()) {
                    linear_error = t.abs();
                }
                limited = true;
            } else if lower >= t {
                c2 = maxss(-max_linear_correction, minss((t - lower) + LINEAR_SLOP, 0.0));
                if !(linear_error > lower - t) {
                    linear_error = lower - t;
                }
                limited = true;
            } else if t >= upper {
                let over = t - upper;
                c2 = maxss(0.0, minss(over - LINEAR_SLOP, max_linear_correction));
                if !(linear_error > over) {
                    linear_error = over;
                }
                limited = true;
            }
        }
        let sum = b.inv_mass + a.inv_mass;
        let s1ia = s1 * a.inv_i;
        let s2ib = s2 * b.inv_i;
        let k11 = (s1ia * s1 + sum) + s2ib * s2;
        let k12 = s1ia + s2ib;
        let mut k22 = b.inv_i + a.inv_i;
        if k22 == 0.0 {
            k22 = 1.0;
        }
        let (ix, iy, iz);
        if limited {
            let k13 = s2ib * a2 + s1ia * a1;
            let k23 = a1 * a.inv_i + a2 * b.inv_i;
            let k33 = ((a1 * a.inv_i) * a1 + sum) + (a2 * b.inv_i) * a2;
            let k: Mat33 = [k11, k12, k13, k12, k22, k23, k13, k23, k33];
            [ix, iy, iz] = solve33(&k, [-c1x, -c1y, -c2]);
        } else {
            let mut det = k22 * k11 - k12 * k12;
            let (bx, by) = (-c1x, -c1y);
            if det != 0.0 {
                det = 1.0 / det;
            }
            ix = (k22 * bx - k12 * by) * det;
            iy = (k11 * by - k12 * bx) * det;
            iz = 0.0;
        }
        let px = iz * axis.x + ix * perp.x;
        let py = iz * axis.y + ix * perp.y;
        let la = (ix * s1 + iy) + iz * a1;
        let lb = (ix * s2 + iy) + iz * a2;
        positions[a.index] = Position {
            c: Vec2::new(ca.x - (px * a.axis.x) * a.inv_mass, ca.y - (py * a.axis.y) * a.inv_mass),
            a: aa - la * a.inv_i,
        };
        positions[b.index] = Position {
            c: Vec2::new(cb.x + (px * b.axis.x) * b.inv_mass, cb.y + (py * b.axis.y) * b.inv_mass),
            a: lb * b.inv_i + ab,
        };
        LINEAR_SLOP >= linear_error && ANGULAR_SLOP >= angular_error
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The live b2 setters SliderJoint2D calls on an existing joint: each wakes both bodies (b2Body::SetAwake(true)).

fn wake_ends(world: &mut World, id: JointId) {
    let j = world.joints[id].as_ref().unwrap();
    let (a, b) = (j.body_a, j.body_b);
    world.wake_body(a);
    world.wake_body(b);
}

fn prismatic_mut(world: &mut World, id: JointId) -> &mut PrismaticJoint {
    world.joints[id].as_mut().unwrap().prismatic_mut()
}

/// b2PrismaticJoint::EnableLimit (180bc1550)
pub fn enable_limit(world: &mut World, id: JointId, flag: bool) {
    if flag != prismatic_mut(world, id).def.enable_limit {
        wake_ends(world, id);
        let p = prismatic_mut(world, id);
        p.def.enable_limit = flag;
        p.impulse[2] = 0.0;
    }
}

/// b2PrismaticJoint::EnableMotor (180bc1690)
pub fn enable_motor(world: &mut World, id: JointId, flag: bool) {
    if flag != prismatic_mut(world, id).def.enable_motor {
        wake_ends(world, id);
        prismatic_mut(world, id).def.enable_motor = flag;
    }
}

/// b2PrismaticJoint::SetLimits (180bc3a70)
pub fn set_limits(world: &mut World, id: JointId, lower: f32, upper: f32) {
    let d = prismatic_mut(world, id).def;
    if lower != d.lower_translation || upper != d.upper_translation {
        wake_ends(world, id);
        let p = prismatic_mut(world, id);
        p.def.lower_translation = lower;
        p.def.upper_translation = upper;
        p.impulse[2] = 0.0;
    }
}

/// b2PrismaticJoint::SetMaxMotorForce (180bc3bc0)
pub fn set_max_motor_force(world: &mut World, id: JointId, force: f32) {
    if force != prismatic_mut(world, id).def.max_motor_force {
        wake_ends(world, id);
        prismatic_mut(world, id).def.max_motor_force = force;
    }
}

/// b2PrismaticJoint::SetMotorSpeed (180bc3cf0)
pub fn set_motor_speed(world: &mut World, id: JointId, speed: f32) {
    if speed != prismatic_mut(world, id).def.motor_speed {
        wake_ends(world, id);
        prismatic_mut(world, id).def.motor_speed = speed;
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The native SliderJoint2D side.

/// SliderJoint2D's own serialized state (the AnchoredJoint2D/Joint2D fields live in weld::FixedJoint2d, which
/// the slider component shares with FixedJoint2D).
#[derive(Clone, Debug, PartialEq)]
pub struct SliderJoint2d {
    /// +0x98, degrees
    pub angle: f32,
    /// +0x9c / +0xa0: JointMotor2D
    pub motor_speed: f32,
    pub max_motor_torque: f32,
    /// +0xa4 / +0xa8: JointTranslationLimits2D
    pub lower: f32,
    pub upper: f32,
    /// +0xac, +0xad, +0xae
    pub use_motor: bool,
    pub use_limits: bool,
    pub auto_configure_angle: bool,
}

/// SliderJoint2D's clamp of angle, speed and limits: [-1e6, 1e6], a NaN passing through.
fn clamp_1e6(v: f32) -> f32 {
    if -1_000_000.0 > v { -1_000_000.0 } else { minss(1_000_000.0, v) }
}

impl SliderJoint2d {
    /// SliderJoint2D::Reset (180c05740) over AnchoredJoint2D::Reset: motor 0 / 10000, limits 0 / 0, motor and
    /// limits off, auto-configured angle on.
    pub fn reset() -> Self {
        Self { angle: 0.0, motor_speed: 0.0, max_motor_torque: 10000.0, lower: 0.0, upper: 0.0, use_motor: false, use_limits: false, auto_configure_angle: true }
    }

    /// SliderJoint2D::SetAngle (180c05890): the clamped value is stored; a live joint is then destroyed and, when the
    /// GameObject is active and the component enabled, created again by the caller.
    pub fn set_angle(&mut self, v: f32) {
        self.angle = clamp_1e6(v);
    }

    /// SliderJoint2D::SetMotor (180c06190): speed clamped to [-1e6, 1e6], maximum torque to [0, 1e6]; useMotor is set.
    pub fn set_motor(&mut self, speed: f32, max_torque: f32) {
        self.motor_speed = clamp_1e6(speed);
        self.max_motor_torque = if 0.0 > max_torque { 0.0 } else { minss(1_000_000.0, max_torque) };
        self.use_motor = true;
    }

    /// SliderJoint2D::SetLimits (180c06000): lower clamped, upper clamped and raised to lower; useLimits is set.
    pub fn set_limits(&mut self, lower: f32, upper: f32) {
        self.lower = clamp_1e6(lower);
        self.upper = clamp_1e6(upper);
        if self.lower > self.upper {
            self.upper = self.lower;
        }
        self.use_limits = true;
    }

    /// The b2 definition SliderJoint2D::Create (180c04400) builds, up to Joint2D::FinalizeCreateJoint. `base` is
    /// the shared AnchoredJoint2D/Joint2D state; `angle_a`/`angle_b` are the bodies' sweep angles and `frames` their
    /// transforms (body B is the ground body when `connected` is `World`).
    pub fn create_def(&mut self, base: &mut super::weld::FixedJoint2d, scale_a: Vec2, body_a: &super::weld::BodyFrame, connected: &super::weld::Connected) -> PrismaticDef {
        use super::weld::{BodyFrame, Connected};
        let frame_b = match connected {
            Connected::World => BodyFrame::default(),
            Connected::Body { frame, .. } => *frame,
        };
        let reference_angle = match base.stored_reference_angle {
            Some(angle) => angle,
            None => frame_b.angle - body_a.angle,
        };
        let (anchor_a, anchor_b) = base.configure_anchors(scale_a, body_a, connected);
        if self.auto_configure_angle && !base.anchor_configured {
            let (qa, qb) = (body_a.q, frame_b.q);
            let wa = Vec2::new((qa.c * anchor_a.x - qa.s * anchor_a.y) + body_a.position.x, (qa.s * anchor_a.x + qa.c * anchor_a.y) + body_a.position.y);
            let wb = Vec2::new((qb.c * anchor_b.x - qb.s * anchor_b.y) + frame_b.position.x, (qb.s * anchor_b.x + qb.c * anchor_b.y) + frame_b.position.y);
            self.angle = clamp_1e6(crate::unity::atan2f(wb.y - wa.y, wb.x - wa.x) * RAD2DEG);
        }
        let q = Rot::new(self.angle * DEG2RAD);
        let (lower, upper) = if self.lower > self.upper { (self.upper, self.lower) } else { (self.lower, self.upper) };
        PrismaticDef {
            local_anchor_a: anchor_a,
            local_anchor_b: anchor_b,
            local_axis_a: Vec2::new(q.c, q.s),
            reference_angle,
            enable_limit: self.use_limits,
            lower_translation: lower,
            upper_translation: upper,
            enable_motor: self.use_motor,
            max_motor_force: self.max_motor_torque,
            motor_speed: self.motor_speed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bodies() -> (JointBody, JointBody) {
        let a = JointBody { index: 0, local_center: Vec2::new(0.1, 0.0), inv_mass: 1.0, inv_i: 2.0, axis: Vec2::new(1.0, 1.0) };
        let b = JointBody { index: 1, local_center: Vec2::new(0.0, -0.2), inv_mass: 0.5, inv_i: 1.0, axis: Vec2::new(1.0, 1.0) };
        (a, b)
    }

    fn step() -> TimeStep {
        TimeStep { dt: 0.02, inv_dt: 50.0, dt_ratio: 1.0, velocity_iterations: 8, position_iterations: 8, warm_starting: true }
    }

    /// A slider to a static body: sideways velocity and rotation are removed, motion along the axis is kept.
    #[test]
    fn slider_removes_perpendicular_motion() {
        let (mut a, b) = bodies();
        a.inv_mass = 0.0;
        a.inv_i = 0.0;
        let def = PrismaticDef { local_axis_a: Vec2::new(1.0, 0.0), ..Default::default() };
        let mut joint = PrismaticJoint::new(def);
        let positions = vec![Position::default(), Position { c: Vec2::new(0.5, 0.0), a: 0.0 }];
        let mut velocities = vec![Velocity::default(), Velocity { v: Vec2::new(1.0, 2.0), w: 3.0 }];
        joint.init_velocity_constraints(a, b, &step(), &positions, &mut velocities);
        for _ in 0..8 {
            joint.solve_velocity_constraints(&step(), &mut velocities);
        }
        assert!(velocities[1].v.y.abs() < 1e-3 && velocities[1].w.abs() < 1e-3, "{:?}", velocities[1]);
        assert!((velocities[1].v.x - 1.0).abs() < 1e-3, "{:?}", velocities[1]);
    }

    /// A motor drives the speed along the axis, bounded by maxMotorForce * dt per step.
    #[test]
    fn motor_limits_impulse() {
        let (mut a, b) = bodies();
        a.inv_mass = 0.0;
        a.inv_i = 0.0;
        let def = PrismaticDef { local_axis_a: Vec2::new(1.0, 0.0), enable_motor: true, max_motor_force: 5.0, motor_speed: 10.0, ..Default::default() };
        let mut joint = PrismaticJoint::new(def);
        let positions = vec![Position::default(), Position::default()];
        let mut velocities = vec![Velocity::default(); 2];
        joint.init_velocity_constraints(a, b, &step(), &positions, &mut velocities);
        joint.solve_velocity_constraints(&step(), &mut velocities);
        assert!((joint.motor_impulse - 5.0 * 0.02).abs() < 1e-6, "{}", joint.motor_impulse);
        assert!(velocities[1].v.x > 0.0);
    }

    /// Position correction pulls a body that left the axis back, and pushes it back inside its limit.
    #[test]
    fn position_solve_restores_axis_and_limit() {
        let (mut a, b) = bodies();
        a.inv_mass = 0.0;
        a.inv_i = 0.0;
        let def = PrismaticDef { local_axis_a: Vec2::new(1.0, 0.0), enable_limit: true, lower_translation: -0.5, upper_translation: 0.5, ..Default::default() };
        let mut joint = PrismaticJoint::new(def);
        let mut positions = vec![Position::default(), Position { c: Vec2::new(0.9, 0.15), a: 0.0 }];
        let mut velocities = vec![Velocity::default(); 2];
        joint.init_velocity_constraints(a, b, &step(), &positions, &mut velocities);
        assert_eq!(joint.limit_state, 2);
        let mut solved = false;
        for _ in 0..60 {
            solved = joint.solve_position_constraints(&mut positions);
            if solved {
                break;
            }
        }
        assert!(solved);
        // Anchors sit at the body origins, so b's centre rests 0.2 below a's anchor line and 0.1 short of the limit.
        assert!((positions[1].c.y + 0.2).abs() < 0.02 && positions[1].c.x < 0.42, "{:?}", positions[1]);
    }

    #[test]
    fn setters_clamp_like_the_native_side() {
        let mut s = SliderJoint2d::reset();
        s.set_limits(2.0, 1.0);
        assert_eq!((s.lower, s.upper, s.use_limits), (2.0, 2.0, true));
        s.set_motor(1e9, -3.0);
        assert_eq!((s.motor_speed, s.max_motor_torque, s.use_motor), (1e6, 0.0, true));
        s.set_angle(f32::NAN);
        assert!(s.angle.is_nan());
        s.set_angle(-1e9);
        assert_eq!(s.angle, -1e6);
    }
}
