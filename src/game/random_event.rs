//! RandomEventBehaviour (Substructure's "when on" group, under a map light's OnGroup). Update, while enabled and past its
//! cooldown, draws Random.value and dispatches its Events only when that draw is not above Time.deltaTime * ChancePerSecond.
//! ChancePerSecond and Cooldown are not in the export's serialized prefab, so they are the C# defaults, 0: every Update
//! draws once and dispatches only on an exact 0 draw, which is not ported (it stops the run).
#![allow(non_snake_case)]

use super::{script, Game};

script! {
    RandomEvent {}
    links {
        /// The component's instance id.
        id: i32 = 0,
        enabled: bool = true,
        /// lastDispatchTime
        lastDispatchTime: f32 = 0.0,
    }
}

impl Game {
    /// RandomEventBehaviour.Update
    pub fn random_event_update(&mut self, i: usize) {
        let now = self.time.time;
        if now - self.random_events[i].lastDispatchTime <= 0.0 {
            return;
        }
        // Time.deltaTime * ChancePerSecond, with ChancePerSecond 0.
        let num = self.time.deltaTime * 0.0;
        if self.random.value() > num {
            return;
        }
        self.random_events[i].lastDispatchTime = now;
        unimplemented!("RandomEventBehaviour: dispatching Events (Random.value is exactly 0): renderer toggles and PlayRandomClipBehaviour.Play are not ported");
    }
}
