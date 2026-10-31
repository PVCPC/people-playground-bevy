//! NoCollideWithBodyPart: when its own collider takes a collision with a LimbBehaviour of the given body part, the pair
//! is set to ignore collisions (IgnoreCollisionStackController's substitute method, stab.rs). Nothing else.
#![allow(non_snake_case)]

use crate::b2::contacts2d::ScriptCollision;
use super::{script, Game};

script! {
    NoCollideWithBodyPart {
        /// LimbBehaviour.BodyPart (the RoughClassification it matches).
        BodyPart: i32 = 0,
    }
    links {
        /// Behaviour.enabled: a disabled one gets no collision callbacks.
        enabled: bool = true,
        /// PhysicalBehaviour of the object (the collision messages reach the object's physical).
        physical: usize = 0,
        /// The component's index on its GameObject, and whether PhysicalBehaviour comes before it (collision order).
        index: usize = 0,
        after_physical: bool = false,
        /// The Collider2D (its instance id).
        Collider: i32 = 0,
    }
}

impl Game {
    /// NoCollideWithBodyPart.OnCollisionEnter2D: `collision.otherCollider == Collider` (the receiver's own collider) and
    /// the hit object's LimbBehaviour has the body part, then the pair ignores collisions.
    pub(super) fn no_collide_body_part_enter(&mut self, n: usize, c: &ScriptCollision) {
        if !self.no_collide_body_parts[n].enabled || c.other_collider != self.no_collide_body_parts[n].Collider {
            return;
        }
        let body_part = self.no_collide_body_parts[n].BodyPart;
        let Some(p) = self.physical_of_collider(c.collider) else { return };
        let Some(l) = self.limbs.iter().position(|l| l.PhysicalBehaviour == p) else { return };
        if self.limbs[l].RoughClassification == body_part {
            let own = self.no_collide_body_parts[n].Collider;
            self.ignore_collision_substitute(own, c.collider, true);
        }
    }
}
