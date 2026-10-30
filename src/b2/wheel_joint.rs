//! b2WheelJoint (Unity's WheelJoint2D), transcribed from UnityPlayer.dll (constructor 180bc6c60,
//! InitVelocityConstraints 180bc88b0, SolveVelocityConstraints 180bcaa40, SolvePositionConstraints 180bc9be0,
//! GetReactionForce 180bc73c0, GetReactionTorque / GetMotorTorque 180bc1da0), plus the native WheelJoint2D side that
//! builds its definition (WheelJoint2D::Create 180bec010).

use super::math::{Rot, Vec2};
use super::revolute::JointBody;
use super::solver::{Position, TimeStep, Velocity};

/// b2_linearSlop as the position solve's final comparison loads it.
const LINEAR_SLOP: f32 = f32::from_bits(0x3ba3_d70a);
/// Degrees to radians as WheelJoint2D::Create multiplies the motor speed.
const DEG2RAD: f32 = f32::from_bits(0x3c8e_fa35);
/// 2 * pi, the spring's omega factor.
const TWO_PI: f32 = f32::from_bits(0x40c9_0fdb);

/// minss dst, src: the second operand when the comparison fails or is unordered.
fn minss(dst: f32, src: f32) -> f32 {
    if dst < src { dst } else { src }
}

/// maxss dst, src
fn maxss(dst: f32, src: f32) -> f32 {
    if dst > src { dst } else { src }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WheelDef {
    pub local_anchor_a: Vec2,
    pub local_anchor_b: Vec2,
    pub local_axis_a: Vec2,
    pub enable_motor: bool,
    pub max_motor_torque: f32,
    pub motor_speed: f32,
    pub frequency_hz: f32,
    pub damping_ratio: f32,
}

#[derive(Clone, Debug, Default)]
pub struct WheelJoint {
    pub def: WheelDef,
    pub local_x_axis_a: Vec2,
    pub local_y_axis_a: Vec2,
    pub impulse: f32,
    pub motor_impulse: f32,
    pub spring_impulse: f32,
    pub(crate) a: JointBody,
    pub(crate) b: JointBody,
    pub(crate) ax: Vec2,
    pub(crate) ay: Vec2,
    pub(crate) s_ax: f32,
    pub(crate) s_bx: f32,
    pub(crate) s_ay: f32,
    pub(crate) s_by: f32,
    pub(crate) mass: f32,
    pub(crate) motor_mass: f32,
    pub(crate) spring_mass: f32,
    pub(crate) bias: f32,
    pub(crate) gamma: f32,
}

impl WheelJoint {
    /// b2WheelJoint::b2WheelJoint (180bc6c60): the y axis is the x axis turned a quarter circle.
    pub fn new(def: WheelDef) -> Self {
        let x = def.local_axis_a;
        Self { def, local_x_axis_a: x, local_y_axis_a: Vec2::new(x.y * -1.0, x.x), ..Self::default() }
    }

    /// b2WheelJoint::GetReactionForce (180bc73c0).
    pub fn reaction_force(&self, inv_dt: f32) -> Vec2 {
        let (i, s, ax, ay) = (self.impulse, self.spring_impulse, self.ax, self.ay);
        Vec2::new((i * ay.x + s * ax.x) * inv_dt, (i * ay.y + s * ax.y) * inv_dt)
    }

    /// b2WheelJoint::GetReactionTorque / GetMotorTorque (180bc1da0).
    pub fn motor_torque(&self, inv_dt: f32) -> f32 {
        inv_dt * self.motor_impulse
    }

    /// b2WheelJoint::InitVelocityConstraints (180bc88b0).
    pub fn init_velocity_constraints(&mut self, a: JointBody, b: JointBody, step: &TimeStep, positions: &[Position], velocities: &mut [Velocity]) {
        self.a = a;
        self.b = b;
        let (pa, pb) = (positions[a.index], positions[b.index]);
        let Velocity { v: mut va, w: mut wa } = velocities[a.index];
        let Velocity { v: mut vb, w: mut wb } = velocities[b.index];
        let (qa, qb) = (Rot::new(pa.a), Rot::new(pb.a));
        let d = &self.def;
        let (m_a, m_b, i_a, i_b) = (a.inv_mass, b.inv_mass, a.inv_i, b.inv_i);
        let ra = Vec2::new(qa.c * (d.local_anchor_a.x - a.local_center.x) - qa.s * (d.local_anchor_a.y - a.local_center.y), qa.s * (d.local_anchor_a.x - a.local_center.x) + qa.c * (d.local_anchor_a.y - a.local_center.y));
        let rb = Vec2::new(qb.c * (d.local_anchor_b.x - b.local_center.x) - qb.s * (d.local_anchor_b.y - b.local_center.y), qb.s * (d.local_anchor_b.x - b.local_center.x) + qb.c * (d.local_anchor_b.y - b.local_center.y));
        let dd = Vec2::new(((rb.x + pb.c.x) - pa.c.x) - ra.x, ((rb.y + pb.c.y) - pa.c.y) - ra.y);
        let dra = Vec2::new(dd.x + ra.x, dd.y + ra.y);
        // Point to line constraint.
        let y = self.local_y_axis_a;
        self.ay = Vec2::new(y.x * qa.c - y.y * qa.s, y.y * qa.c + y.x * qa.s);
        let ay = self.ay;
        self.s_ay = dra.x * ay.y - dra.y * ay.x;
        self.s_by = rb.x * ay.y - rb.y * ay.x;
        self.mass = ((self.s_ay * i_a) * self.s_ay + (m_b + m_a)) + (self.s_by * i_b) * self.s_by;
        if self.mass > 0.0 {
            self.mass = 1.0 / self.mass;
        }
        // Spring constraint.
        let freq = d.frequency_hz;
        self.spring_mass = 0.0;
        self.bias = 0.0;
        self.gamma = 0.0;
        if freq > 0.0 {
            let x = self.local_x_axis_a;
            self.ax = Vec2::new(x.x * qa.c - x.y * qa.s, x.y * qa.c + x.x * qa.s);
            let ax = self.ax;
            self.s_ax = dra.x * ax.y - dra.y * ax.x;
            self.s_bx = rb.x * ax.y - rb.y * ax.x;
            let inv_mass = ((self.s_ax * i_a) * self.s_ax + (m_b + m_a)) + (self.s_bx * i_b) * self.s_bx;
            if inv_mass > 0.0 {
                let spring_mass = 1.0 / inv_mass;
                let c = dd.y * ax.y + dd.x * ax.x;
                let omega = freq * TWO_PI;
                let damp = (spring_mass + spring_mass) * d.damping_ratio * omega;
                let k = (spring_mass * omega) * omega;
                let h = step.dt;
                self.gamma = (damp + h * k) * h;
                if self.gamma > 0.0 {
                    self.gamma = 1.0 / self.gamma;
                }
                self.bias = ((h * c) * k) * self.gamma;
                self.spring_mass = self.gamma + inv_mass;
                if self.spring_mass > 0.0 {
                    self.spring_mass = 1.0 / self.spring_mass;
                }
            }
        } else {
            self.spring_impulse = 0.0;
        }
        // Rotational motor.
        if d.enable_motor {
            self.motor_mass = i_b + i_a;
            if self.motor_mass > 0.0 {
                self.motor_mass = 1.0 / self.motor_mass;
            }
        } else {
            self.motor_mass = 0.0;
            self.motor_impulse = 0.0;
        }
        if step.warm_starting && (freq > 0.0 || d.enable_motor) {
            self.impulse = step.dt_ratio * self.impulse;
            self.spring_impulse = step.dt_ratio * self.spring_impulse;
            self.motor_impulse *= step.dt_ratio;
            let (i, s, m) = (self.impulse, self.spring_impulse, self.motor_impulse);
            let (ax, ay) = (self.ax, self.ay);
            let p = Vec2::new(i * ay.x + s * ax.x, i * ay.y + s * ax.y);
            let la = (s * self.s_ax + i * self.s_ay) + m;
            let lb = (s * self.s_bx + i * self.s_by) + m;
            va = Vec2::new(va.x - p.x * m_a, va.y - p.y * m_a);
            wa -= la * i_a;
            vb = Vec2::new(vb.x + p.x * m_b, vb.y + p.y * m_b);
            wb += lb * i_b;
        } else {
            self.impulse = 0.0;
            self.motor_impulse = 0.0;
            self.spring_impulse = 0.0;
        }
        self.store(velocities, va, wa, vb, wb);
    }

    /// The final stores mask linear velocity by each end's Unity constraint axis.
    fn store(&self, velocities: &mut [Velocity], va: Vec2, wa: f32, vb: Vec2, wb: f32) {
        let (a, b) = (self.a, self.b);
        velocities[a.index] = Velocity { v: Vec2::new(va.x * a.axis.x, va.y * a.axis.y), w: wa };
        velocities[b.index] = Velocity { v: Vec2::new(vb.x * b.axis.x, vb.y * b.axis.y), w: wb };
    }

    /// b2WheelJoint::SolveVelocityConstraints (180bcaa40).
    pub fn solve_velocity_constraints(&mut self, step: &TimeStep, velocities: &mut [Velocity]) {
        let (a, b) = (self.a, self.b);
        let (m_a, m_b, i_a, i_b) = (a.inv_mass, b.inv_mass, a.inv_i, b.inv_i);
        let Velocity { v: mut va, w: mut wa } = velocities[a.index];
        let Velocity { v: mut vb, w: mut wb } = velocities[b.index];
        // Spring constraint.
        if self.def.frequency_hz > 0.0 {
            let ax = self.ax;
            let cdot = (((vb.y - va.y) * ax.y + (vb.x - va.x) * ax.x) + self.s_bx * wb) - self.s_ax * wa;
            let impulse = -(((cdot + self.bias) + self.spring_impulse * self.gamma) * self.spring_mass);
            self.spring_impulse += impulse;
            let p = Vec2::new(impulse * ax.x, impulse * ax.y);
            va = Vec2::new(va.x - p.x * m_a, va.y - p.y * m_a);
            wa -= (self.s_ax * impulse) * i_a;
            vb = Vec2::new(vb.x + p.x * m_b, vb.y + p.y * m_b);
            wb += (self.s_bx * impulse) * i_b;
        }
        // Rotational motor.
        if self.def.enable_motor {
            let cdot = (wb - wa) - self.def.motor_speed;
            let impulse = -(cdot * self.motor_mass);
            let max_impulse = self.def.max_motor_torque * step.dt;
            let old = self.motor_impulse;
            self.motor_impulse = maxss(-max_impulse, minss(impulse + old, max_impulse));
            let impulse = self.motor_impulse - old;
            wa -= impulse * i_a;
            wb += impulse * i_b;
        }
        // Point to line constraint.
        let ay = self.ay;
        let cdot = (((vb.y - va.y) * ay.y + (vb.x - va.x) * ay.x) + self.s_by * wb) - self.s_ay * wa;
        let impulse = -(cdot * self.mass);
        self.impulse = impulse + self.impulse;
        let p = Vec2::new(impulse * ay.x, impulse * ay.y);
        va = Vec2::new(va.x - p.x * m_a, va.y - p.y * m_a);
        wa -= (self.s_ay * impulse) * i_a;
        vb = Vec2::new(vb.x + p.x * m_b, vb.y + p.y * m_b);
        wb = (self.s_by * impulse) * i_b + wb;
        self.store(velocities, va, wa, vb, wb);
    }

    /// b2WheelJoint::SolvePositionConstraints (180bc9be0): only the point to line constraint.
    pub fn solve_position_constraints(&self, positions: &mut [Position]) -> bool {
        let (a, b) = (self.a, self.b);
        let d = &self.def;
        let (Position { c: mut ca, a: mut aa }, Position { c: mut cb, a: mut ab }) = (positions[a.index], positions[b.index]);
        let (qa, qb) = (Rot::new(aa), Rot::new(ab));
        let ra = Vec2::new(qa.c * (d.local_anchor_a.x - a.local_center.x) - qa.s * (d.local_anchor_a.y - a.local_center.y), qa.c * (d.local_anchor_a.y - a.local_center.y) + qa.s * (d.local_anchor_a.x - a.local_center.x));
        let rb = Vec2::new(qb.c * (d.local_anchor_b.x - b.local_center.x) - qb.s * (d.local_anchor_b.y - b.local_center.y), qb.c * (d.local_anchor_b.y - b.local_center.y) + qb.s * (d.local_anchor_b.x - b.local_center.x));
        let dd = Vec2::new((rb.x + (cb.x - ca.x)) - ra.x, (rb.y + (cb.y - ca.y)) - ra.y);
        let y = self.local_y_axis_a;
        let ay = Vec2::new(y.x * qa.c - y.y * qa.s, y.x * qa.s + y.y * qa.c);
        let s_ay = (ra.x + dd.x) * ay.y - (ra.y + dd.y) * ay.x;
        let s_by = rb.x * ay.y - rb.y * ay.x;
        let c = dd.x * ay.x + dd.y * ay.y;
        let k = ((a.inv_i * self.s_ay) * self.s_ay + (a.inv_mass + b.inv_mass)) + (b.inv_i * self.s_by) * self.s_by;
        let impulse = if k != 0.0 { -(c / k) } else { 0.0 };
        let p = Vec2::new(ay.x * impulse, ay.y * impulse);
        ca = Vec2::new(ca.x - a.inv_mass * (p.x * a.axis.x), ca.y - a.inv_mass * (p.y * a.axis.y));
        aa -= (impulse * s_ay) * a.inv_i;
        cb = Vec2::new(cb.x + b.inv_mass * (p.x * b.axis.x), cb.y + b.inv_mass * (p.y * b.axis.y));
        ab += (impulse * s_by) * b.inv_i;
        positions[a.index] = Position { c: ca, a: aa };
        positions[b.index] = Position { c: cb, a: ab };
        LINEAR_SLOP >= c.abs()
    }
}

/// WheelJoint2D's serialized fields: JointSuspension2D (damping ratio, frequency, angle in degrees) and JointMotor2D.
#[derive(Clone, Copy, Debug)]
pub struct WheelJoint2d {
    pub damping_ratio: f32,
    pub frequency: f32,
    pub angle: f32,
    pub use_motor: bool,
    pub motor_speed: f32,
    pub max_motor_torque: f32,
}

impl WheelJoint2d {
    /// The b2 definition WheelJoint2D::Create (180bec010) builds, up to Joint2D::FinalizeCreateJoint. Unlike the
    /// other joints it does not auto-configure the connected anchor: the anchors are the serialized ones times the
    /// two transforms' lossy scales.
    pub fn create_def(&self, anchor: Vec2, connected_anchor: Vec2, scale_a: Vec2, scale_b: Vec2) -> WheelDef {
        let half = self.angle / 360.0;
        let radians = (half + half) * std::f32::consts::PI;
        WheelDef {
            local_anchor_a: Vec2::new(scale_a.x * anchor.x, scale_a.y * anchor.y),
            local_anchor_b: Vec2::new(scale_b.x * connected_anchor.x, scale_b.y * connected_anchor.y),
            local_axis_a: Vec2::new(radians.cos(), radians.sin()),
            enable_motor: self.use_motor,
            max_motor_torque: self.max_motor_torque,
            motor_speed: self.motor_speed * DEG2RAD,
            frequency_hz: self.frequency,
            damping_ratio: self.damping_ratio,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step() -> TimeStep {
        TimeStep { dt: 0.02, inv_dt: 50.0, dt_ratio: 1.0, velocity_iterations: 8, position_iterations: 8, warm_starting: true }
    }

    /// A wheel on a static chassis keeps its axle on the suspension line and turns freely, or at the motor speed.
    #[test]
    fn wheel_slides_along_axis_and_spins() {
        let a = JointBody { index: 0, local_center: Vec2::new(0.0, 0.0), inv_mass: 0.0, inv_i: 0.0, axis: Vec2::new(1.0, 1.0) };
        let b = JointBody { index: 1, local_center: Vec2::new(0.0, 0.0), inv_mass: 1.0, inv_i: 2.0, axis: Vec2::new(1.0, 1.0) };
        let def = WheelJoint2d { damping_ratio: 0.7, frequency: 0.0, angle: 90.0, use_motor: true, motor_speed: 90.0, max_motor_torque: 100.0 }
            .create_def(Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0), Vec2::new(1.0, 1.0));
        assert!((def.local_axis_a.x).abs() < 1e-6 && (def.local_axis_a.y - 1.0).abs() < 1e-6);
        let mut j = WheelJoint::new(def);
        let mut positions = vec![Position::default(), Position { c: Vec2::new(0.3, 0.5), a: 0.0 }];
        let mut velocities = vec![Velocity::default(), Velocity { v: Vec2::new(2.0, 1.0), w: 0.0 }];
        j.init_velocity_constraints(a, b, &step(), &positions, &mut velocities);
        for _ in 0..8 {
            j.solve_velocity_constraints(&step(), &mut velocities);
        }
        // The sideways velocity is gone, the one along the vertical axis kept, and the wheel spins at the motor speed.
        assert!(velocities[1].v.x.abs() < 1e-3, "{:?}", velocities[1]);
        assert!((velocities[1].v.y - 1.0).abs() < 1e-3);
        assert!((velocities[1].w - def.motor_speed).abs() < 1e-3, "{:?}", velocities[1]);
        let mut solved = false;
        for _ in 0..20 {
            solved = j.solve_position_constraints(&mut positions);
        }
        assert!(solved && positions[1].c.x.abs() < 0.01);
    }
}
