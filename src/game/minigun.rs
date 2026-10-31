//! MinigunBehaviour: while the trigger is held past 0.2 s it fires its FirearmBehaviour (Shoot the first time, then
//! ShootContinuous); Use resets the firearm's shot index. The spin-up rotation, its sound routine and the barrel's
//! property block are presentation.
#![allow(non_snake_case)]

use super::{script, Game};

script! {
    Minigun {
        UpperTemperatureBound: f32 = 500.0,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// Behaviour.enabled
        enabled: bool = true,
        /// PhysicalBehaviour (GetComponent in Awake).
        physical: usize = 0,
        /// `Game::firearms` index of the FirearmBehaviour field.
        firearm: usize = 0,
        /// timeHeld: seconds the trigger has been held (reset when it is released).
        timeHeld: f32 = 0.0,
    }
}

impl Game {
    /// MinigunBehaviour.Use: FirearmBehaviour.ResetIndex(). Its StartCoroutine(SoundRoutine) is audio.
    pub fn minigun_use(&mut self, m: usize) {
        let f = self.miniguns[m].firearm;
        self.firearms[f].shotIndex = 0;
    }

    /// MinigunBehaviour.Update's firing and hold timer. The rotation, the fire loop and reflection sounds, and the
    /// StartedBeingUsedContinuously coroutine are presentation. `directlyUsed` is written only.
    pub fn minigun_update(&mut self, m: usize) {
        let (f, p) = (self.miniguns[m].firearm, self.miniguns[m].physical);
        if self.miniguns[m].timeHeld > 0.2 {
            if self.firearms[f].shotIndex == 0 {
                self.firearm_shoot(f);
            } else {
                self.firearm_shoot_continuous(f);
            }
        }
        if self.used_continuously(p) {
            self.miniguns[m].timeHeld += self.time.deltaTime;
        } else {
            self.miniguns[m].timeHeld = 0.0;
        }
    }
}
