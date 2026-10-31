//! DecimatorBehaviour. Use switches DecimateTrigger on and off; a non-trigger collider of a deletable physical object
//! that enters it destroys that object's root (once per root in a row) and leaves a DeleteEffect; a charge over 50
//! turns the Decimator into a BlackHole. The particles and the audio are presentation.
#![allow(non_snake_case)]

use super::{script, Game};
use crate::assets::db::ObjRef;

script! {
    Decimator {
        Activated: bool = false,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        enabled: bool = true,
        /// PhysicalBehaviour
        physical: usize = 0,
        /// DecimateTrigger: (object, component) in its instance.
        DecimateTrigger: (usize, usize) = (0, 0),
        /// The GameObject's Collider2Ds and Rigidbody2D (0 without one): the trigger messages it receives.
        colliders: Vec<i32> = Vec::new(),
        rigidbody: i32 = 0,
        DeleteEffect: ObjRef = ObjRef { file: 0, path_id: 0 },
        BlackHole: ObjRef = ObjRef { file: 0, path_id: 0 },
    }
}

/// Whether a script whose GameObject has `colliders` and Rigidbody2D `own_rigidbody` (0 for none) gets a trigger
/// message of collider `own`: SendMessage to the collider's GameObject, then to its rigidbody's when that is another.
pub(super) fn trigger_receives(colliders: &[i32], own_rigidbody: i32, own: i32, rigidbody: Option<crate::b2::world::RigidbodyRef>) -> bool {
    colliders.contains(&own) || rigidbody.is_some_and(|rb| own_rigidbody != 0 && rb.id == own_rigidbody)
}

impl Game {
    /// DecimatorBehaviour.Start
    pub fn decimator_start(&mut self, i: usize) {
        self.decimator_update_activation(i);
    }

    /// DecimatorBehaviour.Use
    pub fn decimator_use(&mut self, i: usize) {
        if self.decimators[i].enabled {
            self.decimators[i].Activated = !self.decimators[i].Activated;
            self.decimator_update_activation(i);
        }
    }

    /// DecimatorBehaviour.FixedUpdate
    pub fn decimator_fixed_update(&mut self, i: usize) {
        let p = self.decimators[i].physical;
        if self.physicals[p].charge > 50.0 {
            let position = self.physicals[p].transform.position;
            self.spawn_prefab(self.decimators[i].BlackHole, position);
            // StatManager.UnlockAchievement("BH") is Steam only.
            let id = self.decimators[i].id;
            self.destroy_object(id);
        }
    }

    /// DecimatorBehaviour.OnDisable
    pub fn decimator_on_disable(&mut self, i: usize, inst: &mut super::spawn::Instance) {
        self.decimators[i].Activated = false;
        let s = &self.decimators[i];
        let (key, on) = (s.DecimateTrigger, s.Activated);
        self.collider_set_enabled(inst, key, on);
    }

    /// DecimatorBehaviour.UpdateActivation: DecimateTrigger.enabled = Activated.
    fn decimator_update_activation(&mut self, i: usize) {
        let s = &self.decimators[i];
        let (id, key, on) = (s.id, s.DecimateTrigger, s.Activated);
        self.with_component_instance(id, |g, inst, _| g.collider_set_enabled(inst, key, on));
    }

    /// Whether DecimatorBehaviour `i` gets the trigger messages of collider `own` (on its GameObject, or on another
    /// GameObject whose collider is attached to its Rigidbody2D).
    pub(super) fn decimator_receives(&self, i: usize, own: i32, rigidbody: Option<crate::b2::world::RigidbodyRef>) -> bool {
        let s = &self.decimators[i];
        trigger_receives(&s.colliders, s.rigidbody, own, rigidbody)
    }

    /// DecimatorBehaviour.OnTriggerEnter2D(collision)
    pub fn decimator_trigger_enter(&mut self, i: usize, collision: i32) {
        let s = &self.decimators[i];
        if !s.enabled || !s.Activated || self.world.collider_is_trigger(collision) != Some(false) {
            return;
        }
        // PhysicalObjectsInWorldByTransform.TryGetValue(collision.transform)
        let Some(v) = self.physicals.iter().position(|p| !p.destroyed && p.in_world && p.all_colliders.contains(&collision)) else { return };
        if !self.physicals[v].Deletable || self.physicals[v].isDisintegrated {
            return;
        }
        let root = self.root_id(v);
        if self.decimator_last_root.unwrap_or(-1) != root {
            self.decimator_last_root = Some(root);
            self.destroy_object(root);
            let position = self.physicals[v].transform.position;
            self.spawn_prefab(self.decimators[i].DeleteEffect, position);
        }
    }
}
