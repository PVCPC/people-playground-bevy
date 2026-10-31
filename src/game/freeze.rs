//! FreezeBehaviour (the Ceiling Turret, Reactor A5 and Substructure). Start keeps the Rigidbody2D's velocity and
//! angularVelocity and requests a freeze (the first request makes the body static, water.rs); OnDestroy releases the
//! request and restores the saved velocities. The outline object and its one-second fade are rendering.
#![allow(non_snake_case)]

use super::{script, Game};

script! {
    Freeze {}
    links {
        /// The component's instance id.
        id: i32 = 0,
        enabled: bool = true,
        /// The GameObject's PhysicalBehaviour, whose rigidbody is GetComponent<Rigidbody2D>().
        physical: usize = 0,
        /// rb.velocity and rb.angularVelocity saved in Start (the Unity units: degrees per second for the latter).
        velocity: [f32; 2] = [0.0, 0.0],
        angularVelocity: f32 = 0.0,
    }
}

impl Game {
    /// FreezeBehaviour.Start: rb = GetComponent<Rigidbody2D>(), its velocities saved, FreezeStackController.RequestFreeze(rb).
    pub fn freeze_start(&mut self, i: usize) {
        let p = self.freezes[i].physical;
        let rb = self.physicals[p].rigidbody.clone();
        self.freezes[i].velocity = rb.velocity(&self.world);
        self.freezes[i].angularVelocity = rb.angular_velocity(&self.world);
        self.freeze_request(p);
    }

    /// FreezeBehaviour.OnDestroy: a live rigidbody is released from the freeze stack and gets its saved velocities back.
    pub fn freeze_destroy(&mut self, i: usize) {
        let (p, velocity, angular) = (self.freezes[i].physical, self.freezes[i].velocity, self.freezes[i].angularVelocity);
        // (bool)rb: a destroyed object's Rigidbody2D is gone.
        if self.physicals[p].destroyed {
            return;
        }
        self.freeze_request_unfreeze(p);
        let rb = self.physicals[p].rigidbody.clone();
        rb.set_velocity(&mut self.world, velocity);
        rb.set_angular_velocity(&mut self.world, angular);
    }
}
