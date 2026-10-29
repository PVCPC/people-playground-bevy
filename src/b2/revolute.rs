//! b2RevoluteJoint (Unity's HingeJoint2D) on the island's solver arrays. The arithmetic is crate::hinge's,
//! already verified against the DLL; this keeps the b2Joint state between steps and feeds those solvers.

use super::math::{Rot, Vec2};
use super::solver::{Position, TimeStep, Velocity};
use crate::hinge;

/// b2_maxAngularCorrection as PhysicsManager2D sets it: 8 degrees.
const MAX_ANGULAR_CORRECTION: f32 = 8.0 * f32::from_bits(0x3c8e_fa35);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RevoluteDef {
    pub local_anchor_a: Vec2,
    pub local_anchor_b: Vec2,
    pub reference_angle: f32,
    pub enable_limit: bool,
    pub lower_angle: f32,
    pub upper_angle: f32,
    pub enable_motor: bool,
    pub motor_speed: f32,
    pub max_motor_torque: f32,
}

/// The body data b2RevoluteJoint::InitVelocityConstraints copies from b2Body.
#[derive(Clone, Copy, Debug, Default)]
pub struct JointBody {
    pub index: usize,
    pub local_center: Vec2,
    pub inv_mass: f32,
    pub inv_i: f32,
    pub axis: Vec2,
}

#[derive(Clone, Debug, Default)]
pub struct RevoluteJoint {
    pub def: RevoluteDef,
    pub impulse: [f32; 3],
    pub motor_impulse: f32,
    pub limit_state: u32,
    pub(crate) a: JointBody,
    pub(crate) b: JointBody,
    pub(crate) velocity: hinge::VelocityConstraint,
}

impl RevoluteJoint {
    pub fn new(def: RevoluteDef) -> Self {
        Self { def, ..Self::default() }
    }

    fn limits(&self) -> Option<[f32; 2]> {
        self.def.enable_limit.then_some([self.def.lower_angle, self.def.upper_angle])
    }

    /// b2RevoluteJoint::InitVelocityConstraints (180bc29c0).
    pub fn init_velocity_constraints(&mut self, a: JointBody, b: JointBody, step: &TimeStep, positions: &[Position], velocities: &mut [Velocity]) {
        self.a = a;
        self.b = b;
        let arm = |body: &JointBody, anchor: Vec2| {
            let r = Rot::new(positions[body.index].a).apply(anchor - body.local_center);
            [r.x, r.y]
        };
        let (va, vb) = (velocities[a.index], velocities[b.index]);
        self.velocity = hinge::VelocityConstraint {
            velocity_a: [va.v.x, va.v.y, va.w],
            velocity_b: [vb.v.x, vb.v.y, vb.w],
            inverse_mass_a: a.inv_mass,
            inverse_mass_b: b.inv_mass,
            inverse_inertia_a: a.inv_i,
            inverse_inertia_b: b.inv_i,
            anchor_a: arm(&a, self.def.local_anchor_a),
            anchor_b: arm(&b, self.def.local_anchor_b),
            impulse: self.impulse,
            motor_impulse: self.motor_impulse,
            axis_a: [a.axis.x, a.axis.y],
            axis_b: [b.axis.x, b.axis.y],
            motor_enabled: self.def.enable_motor,
            limit_state: self.limit_state,
            target_velocity: self.def.motor_speed,
            max_torque: self.def.max_motor_torque,
            dt: step.dt,
            ..Default::default()
        };
        let angle = hinge::joint_angle(positions[a.index].a, positions[b.index].a, self.def.reference_angle);
        self.velocity.initialize(angle, self.limits(), step.dt_ratio, step.warm_starting);
        self.store(velocities);
    }

    /// b2RevoluteJoint::SolveVelocityConstraints (180bc5d10).
    pub fn solve_velocity_constraints(&mut self, velocities: &mut [Velocity]) {
        let (va, vb) = (velocities[self.a.index], velocities[self.b.index]);
        self.velocity.velocity_a = [va.v.x, va.v.y, va.w];
        self.velocity.velocity_b = [vb.v.x, vb.v.y, vb.w];
        self.velocity.solve();
        self.store(velocities);
    }

    fn store(&mut self, velocities: &mut [Velocity]) {
        let c = &self.velocity;
        velocities[self.a.index] = Velocity { v: Vec2::new(c.velocity_a[0], c.velocity_a[1]), w: c.velocity_a[2] };
        velocities[self.b.index] = Velocity { v: Vec2::new(c.velocity_b[0], c.velocity_b[1]), w: c.velocity_b[2] };
        self.impulse = c.impulse;
        self.motor_impulse = c.motor_impulse;
        self.limit_state = c.limit_state;
    }

    /// b2RevoluteJoint::SolvePositionConstraints (180bc4560).
    pub fn solve_position_constraints(&self, positions: &mut [Position]) -> bool {
        let (pa, pb) = (positions[self.a.index], positions[self.b.index]);
        let mut c = hinge::PositionConstraint {
            position_a: [pa.c.x, pa.c.y, pa.a],
            position_b: [pb.c.x, pb.c.y, pb.a],
            inverse_mass_a: self.a.inv_mass,
            inverse_mass_b: self.b.inv_mass,
            inverse_inertia_a: self.a.inv_i,
            inverse_inertia_b: self.b.inv_i,
            anchor_a: [self.def.local_anchor_a.x, self.def.local_anchor_a.y],
            anchor_b: [self.def.local_anchor_b.x, self.def.local_anchor_b.y],
            local_center_a: [self.a.local_center.x, self.a.local_center.y],
            local_center_b: [self.b.local_center.x, self.b.local_center.y],
            axis_a: [self.a.axis.x, self.a.axis.y],
            axis_b: [self.b.axis.x, self.b.axis.y],
            motor_mass: self.velocity.motor_mass,
            reference_angle: self.def.reference_angle,
            limits: [self.def.lower_angle, self.def.upper_angle],
            limit_enabled: self.def.enable_limit,
            limit_state: self.limit_state,
            max_angular_correction: MAX_ANGULAR_CORRECTION,
        };
        let solved = c.solve();
        positions[self.a.index] = Position { c: Vec2::new(c.position_a[0], c.position_a[1]), a: c.position_a[2] };
        positions[self.b.index] = Position { c: Vec2::new(c.position_b[0], c.position_b[1]), a: c.position_b[2] };
        solved
    }
}
