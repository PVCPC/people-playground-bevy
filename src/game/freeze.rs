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

script! {
    /// FreezeSerialisationHelper (the Ceiling Turret): Start keeps a FreezeBehaviour on its object when IsFrozen, else
    /// destroys it. It has no other messages.
    FreezeSerialisation {
        IsFrozen: bool = false,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
    }
}

impl Game {
    /// FreezeSerialisationHelper.Start: `IsFrozen` runs GetOrAddComponent<FreezeBehaviour>() (a present one is kept; a missing
    /// one is an AddComponent, unimplemented), else TryGetComponent then Object.Destroy of the one there.
    pub fn freeze_serialisation_start(&mut self, h: usize) {
        let (id, is_frozen) = (self.freeze_serialisations[h].id, self.freeze_serialisations[h].IsFrozen);
        let components = self.with_component_instance(id, |_, inst, o| inst.objects[o].components.clone());
        let freeze = self.freezes.iter().position(|f| components.contains(&f.id) && !self.destroyed_components.contains(&f.id));
        if is_frozen {
            if freeze.is_none() {
                unimplemented!("FreezeSerialisationHelper: GetOrAddComponent<FreezeBehaviour> adds the component (AddComponent)");
            }
        } else if let Some(f) = freeze {
            self.freeze_queue_destroy(f);
        }
    }

    /// Object.Destroy(FreezeBehaviour): queued like a DestroyObjectDelayed, so OnDestroy runs with the component's destroy
    /// (a second Destroy finds it queued).
    fn freeze_queue_destroy(&mut self, f: usize) {
        let id = self.freezes[f].id;
        if !self.destroyed_components.contains(&id) && !self.component_destroys.iter().any(|&(_, i)| i == id) {
            let at = self.time.fixedTimeAsDouble.min(self.time.timeAsDouble);
            self.component_destroys.push((at, id));
        }
    }

    /// The queued Object.Destroy of a FreezeBehaviour (false: `id` is not one): OnDestroy once.
    pub fn freeze_run_destroy(&mut self, id: i32) -> bool {
        let Some(f) = self.freezes.iter().position(|x| x.id == id) else { return false };
        self.component_destroys.retain(|&(_, i)| i != id);
        if self.destroyed_components.insert(id) {
            self.freeze_destroy(f);
        }
        true
    }

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
