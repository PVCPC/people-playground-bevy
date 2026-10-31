//! TelevisionBehaviour (the Television). Awake draws the glow's seed; Update breaks the set when its PhysicalBehaviour is
//! hotter than TemperatureThreshold; OnCollisionEnter2D breaks it when the first contact's normal impulse exceeds
//! BreakingTreshold; Shot breaks it; Break (Broken, unless Indestructible) draws one of BreakSounds. Use toggles Activated
//! while not broken. Start, UpdateActivation, the clock (the Christmas and night sprites), the special-position clips, the
//! screen, glow, video and audio are presentation. The context menu's Make indestructible and Repair are UI.
#![allow(non_snake_case)]

use super::{script, Game};
use crate::b2::contacts2d::ScriptCollision;

script! {
    Television {
        Broken: bool = false,
        Activated: bool = false,
        BreakingTreshold: f32 = 2.0,
        Indestructible: bool = false,
        TemperatureThreshold: f32 = 300.0,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// Behaviour.enabled
        enabled: bool = true,
        /// PhysicalBehaviour (GetComponent in Awake)
        physical: usize = 0,
        /// The component's index in its object (its order against the PhysicalBehaviour).
        index: usize = 0,
        /// Whether the PhysicalBehaviour is before it in the object's components (collision order).
        after_physical: bool = false,
        /// BreakSounds' length (PickRandom draws only when it is non-empty).
        break_sounds: usize = 0,
    }
}

impl Game {
    /// TelevisionBehaviour.Awake: `seed = Random.value * 1000f` (the glow's noise, presentation). The draw stays.
    pub fn television_awake(&mut self, _t: usize) {
        self.random.value();
    }

    /// TelevisionBehaviour.Use: `if (!Broken)` Activated flips (UpdateActivation is presentation).
    pub fn television_use(&mut self, t: usize) {
        if !self.televisions[t].Broken {
            self.televisions[t].Activated = !self.televisions[t].Activated;
        }
    }

    /// TelevisionBehaviour.Update: a hot set breaks. The video time, playback speed, glow and clip swaps are presentation.
    pub fn television_update(&mut self, t: usize) {
        let (p, threshold) = (self.televisions[t].physical, self.televisions[t].TemperatureThreshold);
        if !self.televisions[t].Broken && self.physicals[p].Temperature > threshold {
            self.television_break(t);
        }
    }

    /// TelevisionBehaviour.OnCollisionEnter2D: `GetContact(0).normalImpulse > BreakingTreshold` breaks it. Unity delivers
    /// no collision to a disabled behaviour.
    pub(super) fn television_on_collision_enter(&mut self, t: usize, c: &ScriptCollision) {
        if !self.televisions[t].enabled {
            return;
        }
        let first = c.contacts.first().unwrap_or_else(|| unimplemented!("TelevisionBehaviour.OnCollisionEnter2D: collision.GetContact(0) without contacts (IndexOutOfRangeException)"));
        if first.normal_impulse > self.televisions[t].BreakingTreshold {
            self.television_break(t);
        }
    }

    /// TelevisionBehaviour.Shot: Break(default).
    pub fn television_shot(&mut self, t: usize) {
        self.television_break(t);
    }

    /// TelevisionBehaviour.Break(velocity): `if (!Broken && !Indestructible)` the set breaks, its particles start, and
    /// BreakSounds.PickRandom() is played (the draw is kept; the sound is audio).
    pub fn television_break(&mut self, t: usize) {
        if !self.televisions[t].Broken && !self.televisions[t].Indestructible {
            self.televisions[t].Broken = true;
            let n = self.televisions[t].break_sounds;
            self.pick_random(n);
        }
    }
}
