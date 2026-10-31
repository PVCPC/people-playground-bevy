//! PhysicsGunBehaviour (the Physics Gun). While its physical is used continuously and holds nothing, Update casts a ray
//! from LineStart along transform.right * localScale.x; a hit on a PhysicalBehaviour in LayersToGrab becomes the Target,
//! held by a RelativeJoint2D on the gun's GameObject (maxForce 1200, maxTorque 300, enableCollision) to the target's
//! Rigidbody2D. Stopping the use, a disintegrated target and OnDisable let go (Object.Destroy of the joint). The line
//! (LineRenderer, LineTarget), the sounds and Awake's hiding of the line are presentation.
#![allow(non_snake_case)]

use super::explosion::normalized3;
use super::{script, BodyOwner, Game};
use crate::b2::math::Vec2 as V;
use crate::unity::transform::{global_position, Trs};

script! {
    PhysicsGun {
        MaxRange: f32 = 0.0,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// Behaviour.enabled
        enabled: bool = true,
        /// GetComponent<PhysicalBehaviour>() (Awake)
        physical: usize = 0,
        /// LineStart: the local TRS of its object and of each parent below the gun's own object.
        LineStart: Vec<Trs> = Vec::new(),
        /// LayersToHit and LayersToGrab (LayerMask bits)
        LayersToHit: u32 = 0,
        LayersToGrab: u32 = 0,
        /// The held PhysicalBehaviour (a `physicals` index).
        Target: Option<usize> = None,
        /// The RelativeJoint2D (`fixed_joints` index).
        joint: Option<usize> = None,
    }
}

impl Game {
    /// `(bool)Target`: Unity's null check, false once the PhysicalBehaviour was destroyed.
    fn physics_gun_has_target(&self, i: usize) -> bool {
        self.physics_guns[i].Target.is_some_and(|t| !self.physicals[t].destroyed)
    }

    /// PhysicsGunBehaviour.Update. StartedBeingUsedContinuously and StoppedBeingUsedContinuously also drive the line,
    /// LineTarget and the sounds (presentation); LineTarget.position never reaches the simulation.
    pub fn physics_gun_update(&mut self, i: usize) {
        let p = self.physics_guns[i].physical;
        if self.used_continuously(p) && !self.physics_gun_has_target(i) {
            self.physics_gun_query_target(i);
        }
        if self.stopped_used_continuously(p) {
            self.physics_gun_set_target(i, None);
        }
        if self.physics_gun_has_target(i) && self.physicals[self.physics_guns[i].Target.unwrap()].isDisintegrated {
            self.physics_gun_set_target(i, None);
        }
    }

    /// PhysicsGunBehaviour.OnDisable: SetTarget(null) (the line and the sound stop).
    pub fn physics_gun_on_disable(&mut self, i: usize) {
        self.physics_gun_set_target(i, None);
    }

    /// PhysicsGunBehaviour.QueryTarget: Physics2D.Raycast(LineStart.position, direction, MaxRange, LayersToHit); a hit
    /// transform whose layer is in LayersToGrab and that has a PhysicalBehaviour in the world becomes the Target.
    fn physics_gun_query_target(&mut self, i: usize) {
        let s = &self.physics_guns[i];
        let (p, layers, grab, range) = (s.physical, s.LayersToHit, s.LayersToGrab, s.MaxRange);
        self.sync_pending();
        // LineStart.transform.position: its local TRS, then the gun's chain.
        let nodes: Vec<Trs> = self.physics_guns[i].LineStart.iter().copied().chain(self.local_chain(p).iter().rev().copied()).collect();
        let mut origin = [nodes[0].t[0], nodes[0].t[1], nodes[0].t[2], 0.0];
        for n in &nodes[1..] {
            origin = global_position(n, origin);
        }
        // (base.transform.right * base.transform.localScale.x).normalized
        let right = self.transform_direction(p, [1.0, 0.0, 0.0]);
        let scale = self.physicals[p].transform.localScale[0];
        let dir = normalized3([right[0] * scale, right[1] * scale, right[2] * scale]);
        let Some(hit) = self.raycast(V::new(origin[0], origin[1]), [dir[0], dir[1]], range, layers, f32::NEG_INFINITY, f32::INFINITY) else { return };
        // raycastHit2D.transform: the attached Rigidbody2D's GameObject; one without a PhysicalBehaviour is not found.
        let Some(target) = self.hit_transform_physical(&hit).filter(|&t| self.physicals[t].in_world) else { return };
        if grab & (1 << self.physical_layer(target)) != 0 {
            self.physics_gun_set_target(i, Some(target));
        }
    }

    /// PhysicsGunBehaviour.SetTarget: null destroys the joint; otherwise a RelativeJoint2D on the gun's GameObject
    /// holds the target's Rigidbody2D (LineTarget.position is presentation).
    fn physics_gun_set_target(&mut self, i: usize, target: Option<usize>) {
        self.physics_guns[i].Target = target;
        let Some(t) = target.filter(|&t| !self.physicals[t].destroyed) else {
            // Object.Destroy(joint): a null or destroyed component is ignored.
            if let Some(f) = self.physics_guns[i].joint.filter(|&f| !self.fixed_joints[f].destroyed) {
                self.destroy_joint_delayed(f);
            }
            return;
        };
        let gun = self.physics_guns[i].physical;
        // AddComponent<RelativeJoint2D>() takes an instance id.
        self.allocate_instance_id();
        let f = self.add_relative_joint_component(gun);
        self.fixed_joint_set_connected_body(f, Some(BodyOwner::Physical(t)));
        self.relative_joint_set_max_force(f, 1200.0);
        self.relative_joint_set_max_torque(f, 300.0);
        self.fixed_joint_set_enable_collision(f, true);
        self.physics_guns[i].joint = Some(f);
    }
}
