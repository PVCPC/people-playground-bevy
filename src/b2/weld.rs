//! b2WeldJoint (Unity's FixedJoint2D), transcribed from UnityPlayer.dll (InitVelocityConstraints 180bc34b0,
//! SolveVelocityConstraints 180bc6590, SolvePositionConstraints 180bc4e00, GetReactionForce 180bc1d50,
//! GetReactionTorque 180bc1da0, b2Mat33 helpers 180bb6660/180bb66f0/180bb69e0/180bb6a60), plus the native
//! FixedJoint2D / AnchoredJoint2D side that builds its definition (FixedJoint2D::Create 180c04160,
//! AnchoredJoint2D::ConfigureAnchors 180beaf30, FixedJoint2D::SetFrequency/SetDampingRatio, Joint2D::SetBreakForce).

use super::math::{Rot, Vec2};
use super::revolute::JointBody;
use super::solver::{Position, TimeStep, Velocity};

/// b2_linearSlop and b2_angularSlop (2 degrees) as the solver's final comparisons load them.
const LINEAR_SLOP: f32 = f32::from_bits(0x3ba3_d70a);
const ANGULAR_SLOP: f32 = f32::from_bits(0x3d0e_fa36);
const TWO_PI: f32 = f32::from_bits(0x40c9_0fdb);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeldDef {
    pub local_anchor_a: Vec2,
    pub local_anchor_b: Vec2,
    pub reference_angle: f32,
    pub frequency_hz: f32,
    pub damping_ratio: f32,
}

/// b2Mat33 as nine floats in memory order: ex.x ex.y ex.z ey.x ey.y ey.z ez.x ez.y ez.z.
pub type Mat33 = [f32; 9];

/// b2Mat33::GetInverse22 (180bb6660): the upper 2x2 inverted, the rest zero.
pub fn get_inverse22(m: &Mat33) -> Mat33 {
    let (ex_x, ex_y, ey_x, ey_y) = (m[0], m[1], m[3], m[4]);
    let mut det = ey_y * ex_x - ex_y * ey_x;
    if det != 0.0 {
        det = 1.0 / det;
    }
    [det * ey_y, -(det * ex_y), 0.0, -(det * ey_x), det * ex_x, 0.0, 0.0, 0.0, 0.0]
}

/// b2Mat33::GetSymInverse33 (180bb66f0)
pub fn get_sym_inverse33(m: &Mat33) -> Mat33 {
    let (ex_x, ex_y, ex_z) = (m[0], m[1], m[2]);
    let (ey_x, ey_y, ey_z) = (m[3], m[4], m[5]);
    let (ez_x, ez_y, ez_z) = (m[6], m[7], m[8]);
    let c_x = ez_z * ey_y - ez_y * ey_z;
    let c_y = ez_x * ey_z - ez_z * ey_x;
    let c_z = ey_x * ez_y - ez_x * ey_y;
    let mut det = ex_y * c_y + ex_x * c_x + ex_z * c_z;
    if det != 0.0 {
        det = 1.0 / det;
    }
    let (a11, a12, a13, a22, a23, a33) = (ex_x, ey_x, ez_x, ey_y, ez_y, ez_z);
    let m00 = det * (a33 * a22 - a23 * a23);
    let m01 = det * (a23 * a13 - a33 * a12);
    let m02 = det * (a23 * a12 - a22 * a13);
    let m11 = det * (a33 * a11 - a13 * a13);
    let m12 = det * (a13 * a12 - a23 * a11);
    let m22 = det * (a22 * a11 - a12 * a12);
    [m00, m01, m02, m01, m11, m12, m02, m12, m22]
}

/// b2Mat33::Solve22 (180bb69e0)
pub fn solve22(m: &Mat33, b: [f32; 2]) -> [f32; 2] {
    let (ex_x, ex_y, ey_x, ey_y) = (m[0], m[1], m[3], m[4]);
    let mut det = ey_y * ex_x - ex_y * ey_x;
    if det != 0.0 {
        det = 1.0 / det;
    }
    [(b[0] * ey_y - b[1] * ey_x) * det, (b[1] * ex_x - b[0] * ex_y) * det]
}

/// b2Mat33::Solve33 (180bb6a60)
pub fn solve33(m: &Mat33, b: [f32; 3]) -> [f32; 3] {
    let (ex_x, ex_y, ex_z) = (m[0], m[1], m[2]);
    let (ey_x, ey_y, ey_z) = (m[3], m[4], m[5]);
    let (ez_x, ez_y, ez_z) = (m[6], m[7], m[8]);
    // cross(ey, ez)
    let c_x = ez_z * ey_y - ez_y * ey_z;
    let c_y = ez_x * ey_z - ez_z * ey_x;
    let c_z = ey_x * ez_y - ez_x * ey_y;
    let mut det = ex_x * c_x + ex_y * c_y + ex_z * c_z;
    if det != 0.0 {
        det = 1.0 / det;
    }
    let (bx, by, bz) = (b[0], b[1], b[2]);
    let x = (bx * c_x + by * c_y + bz * c_z) * det;
    // cross(b, ez)
    let d_x = ez_z * by - ez_y * bz;
    let d_y = ez_x * bz - ez_z * bx;
    let d_z = bx * ez_y - by * ez_x;
    let y = (ex_x * d_x + ex_y * d_y + ex_z * d_z) * det;
    // cross(ey, b)
    let e_x = ey_y * bz - ey_z * by;
    let e_y = ey_z * bx - ey_x * bz;
    let e_z = ey_x * by - ey_y * bx;
    let z = (ex_x * e_x + ex_y * e_y + ex_z * e_z) * det;
    [x, y, z]
}

/// b2Rot applied to (anchor - local center), in the DLL's term order.
fn arm(q: Rot, x: f32, y: f32) -> Vec2 {
    Vec2::new(x * q.c - y * q.s, x * q.s + y * q.c)
}

/// The 3x3 K matrix both InitVelocityConstraints and SolvePositionConstraints build.
fn stiffness(ra: Vec2, rb: Vec2, ma: f32, mb: f32, ia: f32, ib: f32) -> Mat33 {
    let ex_x = ra.y * ra.y * ia + (mb + ma) + rb.y * rb.y * ib;
    let ey_x = -(ra.y * ra.x * ia) - rb.y * rb.x * ib;
    let ez_x = -(ra.y * ia) - rb.y * ib;
    let ey_y = ra.x * ra.x * ia + (mb + ma) + rb.x * rb.x * ib;
    let ez_y = ra.x * ia + rb.x * ib;
    let ez_z = ib + ia;
    [ex_x, ey_x, ez_x, ey_x, ey_y, ez_y, ez_x, ez_y, ez_z]
}

#[derive(Clone, Debug, Default)]
pub struct WeldJoint {
    pub def: WeldDef,
    /// m_impulse
    pub impulse: [f32; 3],
    pub(crate) gamma: f32,
    pub(crate) bias: f32,
    pub(crate) a: JointBody,
    pub(crate) b: JointBody,
    pub(crate) r_a: Vec2,
    pub(crate) r_b: Vec2,
    /// m_mass
    pub(crate) mass: Mat33,
}

impl WeldJoint {
    /// b2Joint::Create's weld case (180bc0e60): the impulse starts at zero.
    pub fn new(def: WeldDef) -> Self {
        Self { def, ..Self::default() }
    }

    /// b2WeldJoint::GetReactionForce (180bc1d50)
    pub fn reaction_force(&self, inv_dt: f32) -> Vec2 {
        Vec2::new(self.impulse[0] * inv_dt, self.impulse[1] * inv_dt)
    }

    /// b2WeldJoint::GetReactionTorque (180bc1da0)
    pub fn reaction_torque(&self, inv_dt: f32) -> f32 {
        inv_dt * self.impulse[2]
    }

    /// b2WeldJoint::InitVelocityConstraints (180bc34b0).
    pub fn init_velocity_constraints(&mut self, a: JointBody, b: JointBody, step: &TimeStep, positions: &[Position], velocities: &mut [Velocity]) {
        self.a = a;
        self.b = b;
        let (pa, pb) = (positions[a.index], positions[b.index]);
        let Velocity { v: mut va, w: mut wa } = velocities[a.index];
        let Velocity { v: mut vb, w: mut wb } = velocities[b.index];
        let (qa, qb) = (Rot::new(pa.a), Rot::new(pb.a));
        let d = self.def;
        self.r_a = arm(qa, d.local_anchor_a.x - a.local_center.x, d.local_anchor_a.y - a.local_center.y);
        self.r_b = arm(qb, d.local_anchor_b.x - b.local_center.x, d.local_anchor_b.y - b.local_center.y);
        let (ra, rb) = (self.r_a, self.r_b);
        let k = stiffness(ra, rb, a.inv_mass, b.inv_mass, a.inv_i, b.inv_i);
        if d.frequency_hz > 0.0 {
            self.mass = get_inverse22(&k);
            let mut inv_m = k[8];
            let m = if inv_m > 0.0 { 1.0 / inv_m } else { 0.0 };
            let c = pb.a - pa.a - d.reference_angle;
            let omega = d.frequency_hz * TWO_PI;
            let damping = (m + m) * d.damping_ratio * omega;
            let stiff = omega * m * omega;
            let h = step.dt;
            let gamma = (damping + h * stiff) * h;
            self.gamma = if gamma != 0.0 { 1.0 / gamma } else { 0.0 };
            self.bias = h * c * stiff * self.gamma;
            inv_m += self.gamma;
            self.mass[8] = if inv_m != 0.0 { 1.0 / inv_m } else { 0.0 };
        } else {
            if k[8] != 0.0 {
                self.mass = get_sym_inverse33(&k);
            } else {
                self.mass = get_inverse22(&k);
            }
            self.gamma = 0.0;
            self.bias = 0.0;
        }
        if step.warm_starting {
            let ratio = step.dt_ratio;
            self.impulse = [self.impulse[0] * ratio, self.impulse[1] * ratio, self.impulse[2] * ratio];
            let [px, py, pz] = self.impulse;
            va = Vec2::new(va.x - px * a.inv_mass, va.y - py * a.inv_mass);
            wa -= ((py * ra.x - px * ra.y) + pz) * a.inv_i;
            vb = Vec2::new(vb.x + px * b.inv_mass, vb.y + py * b.inv_mass);
            wb += ((py * rb.x - px * rb.y) + pz) * b.inv_i;
        } else {
            self.impulse = [0.0; 3];
        }
        self.store(velocities, va, wa, vb, wb);
    }

    /// b2WeldJoint::SolveVelocityConstraints (180bc6590).
    pub fn solve_velocity_constraints(&mut self, velocities: &mut [Velocity]) {
        let (a, b, ra, rb) = (self.a, self.b, self.r_a, self.r_b);
        let Velocity { v: mut va, w: mut wa } = velocities[a.index];
        let Velocity { v: mut vb, w: mut wb } = velocities[b.index];
        let (vb_x, vb_y) = (vb.x, vb.y);
        let cdot2 = wb - wa;
        let (px, py, term_a, term_b);
        if self.def.frequency_hz > 0.0 {
            let sum = self.impulse[2] * self.gamma + (cdot2 + self.bias);
            let impulse2 = -(sum * self.mass[8]);
            self.impulse[2] += impulse2;
            wa -= impulse2 * a.inv_i;
            wb += impulse2 * b.inv_i;
            let cx = (-(wb * rb.y) + vb_x - va.x) - -(wa * ra.y);
            let cy = ((wb * rb.x + vb_y) - va.y) - wa * ra.x;
            let m = &self.mass;
            let ix = -(cy * m[3] + cx * m[0]);
            let iy = -(cy * m[4] + cx * m[1]);
            self.impulse[1] += iy;
            self.impulse[0] += ix;
            (px, py) = (ix, iy);
            va = Vec2::new(va.x - px * a.inv_mass, va.y - py * a.inv_mass);
            term_a = (py * ra.x - px * ra.y) * a.inv_i;
            term_b = (py * rb.x - px * rb.y) * b.inv_i;
        } else {
            let cx = (-(wb * rb.y) + vb_x - va.x) - -(wa * ra.y);
            let cy = ((wb * rb.x + vb_y) - va.y) - wa * ra.x;
            let m = &self.mass;
            let ix = -((cx * m[0] + cy * m[3]) + cdot2 * m[6]);
            let iy = -((cx * m[1] + cy * m[4]) + cdot2 * m[7]);
            let iz = -((cx * m[2] + cy * m[5]) + cdot2 * m[8]);
            self.impulse[0] += ix;
            self.impulse[1] += iy;
            self.impulse[2] += iz;
            (px, py) = (ix, iy);
            va = Vec2::new(va.x - px * a.inv_mass, va.y - py * a.inv_mass);
            term_a = ((py * ra.x - px * ra.y) + iz) * a.inv_i;
            term_b = ((py * rb.x - px * rb.y) + iz) * b.inv_i;
        }
        wa -= term_a;
        wb += term_b;
        vb = Vec2::new(b.inv_mass * px + vb_x, b.inv_mass * py + vb_y);
        self.store(velocities, va, wa, vb, wb);
    }

    /// The final stores mask linear velocity by each end's Unity constraint axis.
    fn store(&self, velocities: &mut [Velocity], va: Vec2, wa: f32, vb: Vec2, wb: f32) {
        let (a, b) = (self.a, self.b);
        velocities[a.index] = Velocity { v: Vec2::new(va.x * a.axis.x, va.y * a.axis.y), w: wa };
        velocities[b.index] = Velocity { v: Vec2::new(vb.x * b.axis.x, vb.y * b.axis.y), w: wb };
    }

    /// b2WeldJoint::SolvePositionConstraints (180bc4e00).
    pub fn solve_position_constraints(&self, positions: &mut [Position]) -> bool {
        let (a, b, d) = (self.a, self.b, &self.def);
        let (Position { c: mut ca, a: mut aa }, Position { c: mut cb, a: mut ab }) = (positions[a.index], positions[b.index]);
        let (qa, qb) = (Rot::new(aa), Rot::new(ab));
        let ra = arm(qa, d.local_anchor_a.x - a.local_center.x, d.local_anchor_a.y - a.local_center.y);
        let rb = arm(qb, d.local_anchor_b.x - b.local_center.x, d.local_anchor_b.y - b.local_center.y);
        let k = stiffness(ra, rb, a.inv_mass, b.inv_mass, a.inv_i, b.inv_i);
        let c1 = [cb.x + rb.x - ca.x - ra.x, cb.y + rb.y - ca.y - ra.y];
        let (position_error, angular_error, p, z);
        if d.frequency_hz > 0.0 {
            position_error = (c1[1] * c1[1] + c1[0] * c1[0]).sqrt();
            angular_error = 0.0;
            let s = solve22(&k, c1);
            p = [-s[0], -s[1]];
            z = None;
        } else {
            let c2 = ab - aa - d.reference_angle;
            position_error = (c1[0] * c1[0] + c1[1] * c1[1]).sqrt();
            angular_error = c2.abs();
            if k[8] > 0.0 {
                let s = solve33(&k, [c1[0], c1[1], c2]);
                p = [-s[0], -s[1]];
                z = Some(-s[2]);
            } else {
                let s = solve22(&k, c1);
                p = [-s[0], -s[1]];
                z = Some(0.0);
            }
        }
        ca = Vec2::new(ca.x - a.inv_mass * (p[0] * a.axis.x), ca.y - a.inv_mass * (p[1] * a.axis.y));
        let (term_a, term_b) = match z {
            None => ((ra.x * p[1] - ra.y * p[0]) * a.inv_i, (rb.x * p[1] - rb.y * p[0]) * b.inv_i),
            Some(z) => (((p[1] * ra.x - p[0] * ra.y) + z) * a.inv_i, ((p[1] * rb.x - p[0] * rb.y) + z) * b.inv_i),
        };
        aa -= term_a;
        cb = Vec2::new(b.inv_mass * (p[0] * b.axis.x) + cb.x, b.inv_mass * (p[1] * b.axis.y) + cb.y);
        ab = term_b + ab;
        positions[a.index] = Position { c: ca, a: aa };
        positions[b.index] = Position { c: cb, a: ab };
        position_error <= LINEAR_SLOP && angular_error <= ANGULAR_SLOP
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The native FixedJoint2D / AnchoredJoint2D / Joint2D side.

/// A b2Body's transform as FetchBodyA/FetchBodyB expose it: position (+0xc), rotation sin/cos (+0x14/+0x18) and
/// sweep angle (+0x38).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BodyFrame {
    pub position: Vec2,
    pub q: Rot,
    pub angle: f32,
}

/// The connected end of a joint: a Rigidbody2D's b2Body and its Transform.lossyScale (x, y), or the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Connected {
    /// connectedBody is null: the ground body, scale (1, 1), angle 0.
    World,
    Body { frame: BodyFrame, lossy_scale: Vec2 },
}

/// Joint2D's break limits setter (180c05980, SetBreakTorque is the same clamp): below zero is zero, above
/// infinity is infinity.
pub fn clamp_break(v: f32) -> f32 {
    if 0.0 > v {
        0.0
    } else {
        // minss xmm0 = +inf, xmm1 = v: a NaN is returned as is.
        if f32::INFINITY < v { f32::INFINITY } else { v }
    }
}

/// Joint2D::FinalizeCreateJoint's refusals (180c048c0), each of which logs and leaves the joint uncreated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FinalizeError {
    /// message 0x10d: the GameObject has no Rigidbody2D body.
    NoRigidbody,
    /// message 0x118: the connected body is the attached body.
    SameBody,
    /// message 0x120: the two bodies belong to different b2Worlds.
    DifferentWorlds,
}

/// The checks FinalizeCreateJoint makes on body A (`None` without a Rigidbody2D body) and body B, in the order
/// the DLL tests them; each body is (identity, world identity).
pub fn check_bodies<T: PartialEq, W: PartialEq>(a: Option<(T, W)>, b: (T, W)) -> Result<(), FinalizeError> {
    let Some(a) = a else { return Err(FinalizeError::NoRigidbody) };
    if a.0 == b.0 {
        return Err(FinalizeError::SameBody);
    }
    if a.1 != b.1 {
        return Err(FinalizeError::DifferentWorlds);
    }
    Ok(())
}

/// FixedJoint2D's serialized state and the Joint2D/AnchoredJoint2D fields its native Create reads.
#[derive(Clone, Debug, PartialEq)]
pub struct FixedJoint2d {
    /// AnchoredJoint2D.anchor (+0x80)
    pub anchor: Vec2,
    /// AnchoredJoint2D.connectedAnchor (+0x88)
    pub connected_anchor: Vec2,
    /// AnchoredJoint2D.autoConfigureConnectedAnchor (+0x90)
    pub auto_configure_connected_anchor: bool,
    /// Joint2D +0x50: set to Application.isPlaying by FinalizeCreateJoint; once set, auto-configure keeps the
    /// stored connected anchor instead of recomputing it.
    pub anchor_configured: bool,
    /// FixedJoint2D.dampingRatio (+0x98) and frequency (+0x9c)
    pub damping_ratio: f32,
    pub frequency: f32,
    /// FixedJoint2D +0xa0/+0xa4: a serialized reference angle, absent for a component added from script.
    pub stored_reference_angle: Option<f32>,
    pub enable_collision: bool,
    pub break_force: f32,
    pub break_torque: f32,
}

impl FixedJoint2d {
    /// FixedJoint2D::Reset (180c05670) over AnchoredJoint2D::Reset (180bf78d0) and Joint2D::Reset (180c05690).
    pub fn reset() -> Self {
        Self {
            anchor: Vec2::new(0.0, 0.0),
            connected_anchor: Vec2::new(0.0, 0.0),
            auto_configure_connected_anchor: true,
            anchor_configured: false,
            damping_ratio: 0.0,
            frequency: 0.0,
            stored_reference_angle: None,
            enable_collision: false,
            break_force: f32::INFINITY,
            break_torque: f32::INFINITY,
        }
    }

    /// The end of Joint2D::FinalizeCreateJoint after b2World::CreateJoint: `+0x50 = Application.isPlaying`.
    pub fn finish_create(&mut self) {
        self.anchor_configured = true;
    }

    /// FixedJoint2D::Cleanup(bool destroyed) (180c03480). `live_reference_angle` is the b2 joint's
    /// referenceAngle when one exists. A recreate (`destroyed` false) keeps it as the stored reference angle;
    /// a destruction forgets the stored one. Either way a live joint is destroyed (the caller does that) and the
    /// anchors read as unconfigured again.
    pub fn cleanup(&mut self, destroyed: bool, live_reference_angle: Option<f32>) {
        if destroyed {
            self.stored_reference_angle = None;
        } else if let Some(angle) = live_reference_angle {
            self.stored_reference_angle = Some(angle);
        }
        if live_reference_angle.is_some() {
            self.anchor_configured = false;
        }
    }

    /// FixedJoint2D::SetFrequency (180c05fa0): clamped to [0, 1e6], written through to the live b2 joint.
    pub fn set_frequency(&mut self, frequency: f32, live: Option<&mut WeldJoint>) {
        let v = if 0.0 > frequency { 0.0 } else if 1_000_000.0 < frequency { 1_000_000.0 } else { frequency };
        self.frequency = v;
        if let Some(j) = live {
            j.def.frequency_hz = v;
        }
    }

    /// FixedJoint2D::SetDampingRatio (180c05cf0): clamped to [0, 1], written through to the live b2 joint.
    pub fn set_damping_ratio(&mut self, damping_ratio: f32, live: Option<&mut WeldJoint>) {
        let v = if 0.0 > damping_ratio { 0.0 } else if 1.0 < damping_ratio { 1.0 } else { damping_ratio };
        self.damping_ratio = v;
        if let Some(j) = live {
            j.def.damping_ratio = v;
        }
    }

    /// AnchoredJoint2D::ConfigureAnchors (180beaf30): the b2 local anchors for body A and body B. `scale_a` is
    /// the joint transform's lossyScale (x, y). Updates the stored connected anchor when it auto-configures.
    pub fn configure_anchors(&mut self, scale_a: Vec2, body_a: &BodyFrame, connected: &Connected) -> (Vec2, Vec2) {
        let anchor = Vec2::new(scale_a.x * self.anchor.x, scale_a.y * self.anchor.y);
        let scale_b = match connected {
            Connected::World => Vec2::new(1.0, 1.0),
            Connected::Body { lossy_scale, .. } => *lossy_scale,
        };
        if self.auto_configure_connected_anchor && !self.anchor_configured {
            let q = body_a.q;
            let world = Vec2::new(anchor.x * q.c - anchor.y * q.s + body_a.position.x, q.s * anchor.x + anchor.y * q.c + body_a.position.y);
            match connected {
                Connected::World => {
                    self.connected_anchor = world;
                    (anchor, world)
                }
                Connected::Body { frame, .. } => {
                    let (vx, vy) = (world.x - frame.position.x, world.y - frame.position.y);
                    let (s, c) = (frame.q.s, frame.q.c);
                    let local = Vec2::new(c * vx + s * vy, c * vy - s * vx);
                    self.connected_anchor = Vec2::new(local.x / scale_b.x, local.y / scale_b.y);
                    (anchor, local)
                }
            }
        } else {
            (anchor, Vec2::new(scale_b.x * self.connected_anchor.x, scale_b.y * self.connected_anchor.y))
        }
    }

    /// FixedJoint2D::Create (180c04160) up to Joint2D::FinalizeCreateJoint: the b2WeldJointDef (which may already
    /// have stored an auto-configured connected anchor, even if FinalizeCreateJoint then refuses). Body A is the
    /// attached rigidbody, body B the connected one (or the ground body). FinalizeCreateJoint creates the joint
    /// with collideConnected = `enable_collision` and then calls `finish_create`.
    pub fn create_def(&mut self, scale_a: Vec2, body_a: &BodyFrame, connected: &Connected) -> WeldDef {
        let reference_angle = match self.stored_reference_angle {
            Some(angle) => angle,
            None => {
                let angle_b = match connected {
                    Connected::World => 0.0,
                    Connected::Body { frame, .. } => frame.angle,
                };
                angle_b - body_a.angle
            }
        };
        let (local_anchor_a, local_anchor_b) = self.configure_anchors(scale_a, body_a, connected);
        WeldDef { local_anchor_a, local_anchor_b, reference_angle, frequency_hz: self.frequency, damping_ratio: self.damping_ratio }
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

    #[test]
    fn mat33_inverses_invert() {
        let k: Mat33 = [4.0, 1.0, 0.5, 1.0, 3.0, 0.25, 0.5, 0.25, 2.0];
        let inv = get_sym_inverse33(&k);
        for col in 0..3 {
            let v = [inv[3 * col], inv[3 * col + 1], inv[3 * col + 2]];
            let r: Vec<f32> = (0..3).map(|row| k[row] * v[0] + k[3 + row] * v[1] + k[6 + row] * v[2]).collect();
            for (row, x) in r.iter().enumerate() {
                assert!((x - if row == col { 1.0 } else { 0.0 }).abs() < 1e-5, "{r:?}");
            }
        }
        let s = solve33(&k, [1.0, 2.0, 3.0]);
        let back = [k[0] * s[0] + k[3] * s[1] + k[6] * s[2], k[1] * s[0] + k[4] * s[1] + k[7] * s[2], k[2] * s[0] + k[5] * s[1] + k[8] * s[2]];
        assert!((back[0] - 1.0).abs() < 1e-5 && (back[1] - 2.0).abs() < 1e-5 && (back[2] - 3.0).abs() < 1e-5);
        let s22 = solve22(&k, [1.0, 2.0]);
        assert!((k[0] * s22[0] + k[3] * s22[1] - 1.0).abs() < 1e-5);
        let i22 = get_inverse22(&k);
        assert_eq!(i22[2], 0.0);
        assert_eq!(i22[8], 0.0);
    }

    /// A rigid weld between a static and a moving body pulls the moving body's anchor and angle back.
    #[test]
    fn rigid_weld_drives_error_to_zero() {
        let (mut a, b) = bodies();
        a.inv_mass = 0.0;
        a.inv_i = 0.0;
        let def = WeldDef { local_anchor_a: Vec2::new(0.5, 0.0), local_anchor_b: Vec2::new(0.0, 0.0), reference_angle: 0.0, ..Default::default() };
        let mut joint = WeldJoint::new(def);
        let mut positions = vec![Position { c: Vec2::new(0.0, 0.0), a: 0.0 }, Position { c: Vec2::new(0.9, 0.1), a: 0.3 }];
        let mut velocities = vec![Velocity::default(), Velocity { v: Vec2::new(1.0, -2.0), w: 3.0 }];
        joint.init_velocity_constraints(a, b, &step(), &positions, &mut velocities);
        for _ in 0..8 {
            joint.solve_velocity_constraints(&mut velocities);
        }
        assert!(velocities[1].w.abs() < 1e-3, "{:?}", velocities[1]);
        let mut solved = false;
        for _ in 0..50 {
            solved = joint.solve_position_constraints(&mut positions);
            if solved {
                break;
            }
        }
        assert!(solved);
        assert!(positions[1].a.abs() < 0.0349);
        assert_eq!(joint.reaction_force(50.0), Vec2::new(joint.impulse[0] * 50.0, joint.impulse[1] * 50.0));
        assert_eq!(joint.reaction_torque(50.0), 50.0 * joint.impulse[2]);
    }

    #[test]
    fn soft_weld_never_corrects_angle_and_uses_gamma() {
        let (a, b) = bodies();
        let def = WeldDef { frequency_hz: 8.0, damping_ratio: 0.0, ..Default::default() };
        let mut joint = WeldJoint::new(def);
        let positions = vec![Position { c: Vec2::new(0.0, 0.0), a: 0.0 }, Position { c: Vec2::new(0.0, 0.0), a: 0.2 }];
        let mut velocities = vec![Velocity::default(), Velocity::default()];
        joint.init_velocity_constraints(a, b, &step(), &positions, &mut velocities);
        assert!(joint.gamma > 0.0 && joint.bias != 0.0);
        assert_eq!(joint.mass[2], 0.0);
        joint.solve_velocity_constraints(&mut velocities);
        assert!(velocities[1].w < 0.0, "angle error pushes B back: {:?}", velocities[1]);
    }

    #[test]
    fn setters_clamp_and_write_through() {
        let mut f = FixedJoint2d::reset();
        let mut live = WeldJoint::new(WeldDef::default());
        f.set_frequency(8.0, Some(&mut live));
        f.set_damping_ratio(5.0, Some(&mut live));
        assert_eq!((f.frequency, f.damping_ratio), (8.0, 1.0));
        assert_eq!((live.def.frequency_hz, live.def.damping_ratio), (8.0, 1.0));
        f.set_frequency(-1.0, None);
        assert_eq!(f.frequency, 0.0);
        assert_eq!(clamp_break(-3.0), 0.0);
        assert_eq!(clamp_break(25.0), 25.0);
        assert_eq!(clamp_break(f32::INFINITY), f32::INFINITY);
    }

    #[test]
    fn create_def_auto_configures_once() {
        let mut f = FixedJoint2d::reset();
        f.anchor = Vec2::new(0.5, 0.0);
        let a = BodyFrame { position: Vec2::new(1.0, 2.0), q: Rot::new(0.0), angle: 0.0 };
        let b = BodyFrame { position: Vec2::new(3.0, 2.0), q: Rot::new(0.0), angle: 0.5 };
        let conn = Connected::Body { frame: b, lossy_scale: Vec2::new(1.0, 1.0) };
        let def = f.create_def(Vec2::new(1.0, 1.0), &a, &conn);
        assert_eq!(def.local_anchor_a, Vec2::new(0.5, 0.0));
        assert_eq!(def.local_anchor_b, Vec2::new(-1.5, 0.0));
        assert_eq!(def.reference_angle, 0.5);
        assert!(!f.anchor_configured);
        f.finish_create();
        // A second creation keeps the stored connected anchor even if the body moved.
        let moved = Connected::Body { frame: BodyFrame { position: Vec2::new(9.0, 9.0), ..b }, lossy_scale: Vec2::new(1.0, 1.0) };
        assert_eq!(f.create_def(Vec2::new(1.0, 1.0), &a, &moved).local_anchor_b, Vec2::new(-1.5, 0.0));
        // Connected to the world: the connected anchor is the world point.
        let mut g = FixedJoint2d::reset();
        g.anchor = Vec2::new(0.5, 0.0);
        let def = g.create_def(Vec2::new(1.0, 1.0), &a, &Connected::World);
        assert_eq!(g.connected_anchor, Vec2::new(1.5, 2.0));
        assert_eq!(def.local_anchor_b, Vec2::new(1.5, 2.0));
        assert_eq!(def.reference_angle, 0.0);
    }

    #[test]
    fn cleanup_keeps_reference_angle_on_recreate_only() {
        let mut f = FixedJoint2d::reset();
        f.finish_create();
        // No live joint: nothing changes.
        f.cleanup(false, None);
        assert!(f.anchor_configured && f.stored_reference_angle.is_none());
        // Recreate with a live joint: its reference angle is kept and the anchors are configured afresh.
        f.cleanup(false, Some(0.75));
        assert_eq!(f.stored_reference_angle, Some(0.75));
        assert!(!f.anchor_configured);
        let a = BodyFrame::default();
        let b = BodyFrame { angle: 2.0, ..a };
        let def = f.create_def(Vec2::new(1.0, 1.0), &a, &Connected::Body { frame: b, lossy_scale: Vec2::new(1.0, 1.0) });
        assert_eq!(def.reference_angle, 0.75);
        // Destruction forgets it.
        f.cleanup(true, Some(0.75));
        assert_eq!(f.stored_reference_angle, None);
    }

    #[test]
    fn finalize_checks_follow_native_order() {
        assert_eq!(check_bodies::<u8, u8>(None, (1, 0)), Err(FinalizeError::NoRigidbody));
        assert_eq!(check_bodies(Some((1, 0)), (1, 5)), Err(FinalizeError::SameBody));
        assert_eq!(check_bodies(Some((1, 0)), (2, 5)), Err(FinalizeError::DifferentWorlds));
        assert_eq!(check_bodies(Some((1, 0)), (2, 0)), Ok(()));
    }
}
