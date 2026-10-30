//! BalloonBehaviour: a balloon that pops on a shot (a frame after it), an exit shot, a break, a stab, a fragment hit, a
//! burn or charge in Update, and a collision above its pressure threshold. Popping shows the PoppedEffect (unparented)
//! and destroys the balloon; the sprite texture and the audio are presentation. The vacuum message has no receiver in
//! the rewrite, so it is not wired.
#![allow(non_snake_case)]

use super::physical::get_average_impulse_remove_outliers;
use super::{script, Game};
use crate::b2::contacts2d::ScriptCollision;

script! {
    Balloon {
        CollisionPressureThreshold: f32 = 5.0,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// PhysicalBehaviour (its burn and charge).
        PhysicalBehaviour: usize = 0,
        /// GetComponent<PhysicalBehaviour>() of the object, which the messages reach: None without one.
        physical: Option<usize> = None,
        /// The component's index on its GameObject, and whether it comes after the PhysicalBehaviour (`side` ordering).
        index: usize = 0,
        after_physical: bool = false,
        /// PoppedEffect: its GameObject's first component id (the Transform the parent is cleared on).
        PoppedEffect: i32 = 0,
        /// popped
        popped: bool = false,
    }
}

impl Game {
    /// BalloonBehaviour.Update: a burn or a charge pops it.
    pub fn balloon_update(&mut self, b: usize) {
        let p = self.balloons[b].PhysicalBehaviour;
        if self.physicals[p].BurnProgress > 0.02 || self.physicals[p].Charge() > 0.01 {
            self.balloon_pop(b);
        }
    }

    /// BalloonBehaviour.OnCollisionStay2D: the average impulse (outliers removed) of up to four contacts.
    pub fn balloon_on_collision_stay(&mut self, b: usize, c: &ScriptCollision) {
        let contacts = c.contacts.len().min(4);
        let mut buffer = self.physical_contact_buffer;
        buffer[..contacts].copy_from_slice(&c.contacts[..contacts]);
        if get_average_impulse_remove_outliers(&buffer, contacts) > self.balloons[b].CollisionPressureThreshold {
            self.balloon_pop(b);
        }
    }

    /// BalloonBehaviour.Shot: StartCoroutine(WaitAFrame()), which pops after WaitForSeconds(0.05).
    pub fn balloon_shot(&mut self, b: usize) {
        let id = self.balloons[b].id;
        self.start_wait(id, 0.05, super::destroyable::Step::BalloonPop(b));
    }

    /// BalloonBehaviour.ExitShot: StopAllCoroutines(), then Pop.
    pub fn balloon_exit_shot(&mut self, b: usize) {
        let id = self.balloons[b].id;
        self.stop_all_coroutines(id);
        self.balloon_pop(b);
    }

    /// BalloonBehaviour.Pop: once. PoppedEffect is set active and unparented, then the balloon is destroyed.
    pub fn balloon_pop(&mut self, b: usize) {
        if self.balloons[b].popped {
            return;
        }
        self.balloons[b].popped = true;
        let effect = self.balloons[b].PoppedEffect;
        self.with_component_instance(effect, |g, inst, o| g.set_active_scripts(inst, o, true));
        self.set_parent_null_of(effect, |_, _, _| {});
        let id = self.balloons[b].id;
        self.destroy_object(id);
    }
}
