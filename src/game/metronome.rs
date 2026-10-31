//! MetronomeBehaviour: while Activated it ticks at CurrentTempo Hz, and each tick sends IsolatedActivation on every
//! channel. Start draws the seed of the tempo's Perlin wobble; Use toggles it. The tick sound, the hand and the context
//! menu are presentation.
#![allow(non_snake_case)]

use super::misc::FixedIntervalDistributor;
use super::{mathf, script, Game};

script! {
    Metronome {
        TempoModifier: f32 = 1.0,
        Activated: bool = false,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// Behaviour.enabled
        enabled: bool = true,
        /// PhysicalBehaviour: the Charge that speeds the tempo up.
        phys: usize = 0,
        animationTimer: f32 = 0.0,
        seed: f32 = 0.0,
        distributor: FixedIntervalDistributor = FixedIntervalDistributor::with_rate(60.0),
    }
}

impl Game {
    /// MetronomeBehaviour.CurrentTempo
    fn metronome_tempo(&self, m: usize) -> f32 {
        let s = &self.metronomes[m];
        let charge = self.physicals[s.phys].Charge();
        let num = if charge > 0.01 { mathf::lerp(1.0, 25.0 * mathf::perlin(s.seed, self.time.time * 4.0), charge * 0.05) } else { 1.0 };
        mathf::clamp(s.TempoModifier * num, 0.125, 32.0)
    }

    /// MetronomeBehaviour.UpdateActivation: an activated metronome restarts its clock half a tick in and its animation.
    fn metronome_update_activation(&mut self, m: usize) {
        if self.metronomes[m].Activated {
            let tempo = self.metronome_tempo(m);
            self.metronomes[m].distributor.timeStepAccumulator = 0.5f32 / tempo;
            self.metronomes[m].animationTimer = 0.0;
        }
    }

    /// MetronomeBehaviour.Start: seed = Random.value * 1000; the context menu button is UI.
    pub fn metronome_start(&mut self, m: usize) {
        self.metronomes[m].seed = self.random.value() * 1000.0;
        self.metronome_update_activation(m);
    }

    /// MetronomeBehaviour.Update. The tick loop's condition is evaluated every pass, and each evaluation advances the
    /// distributor, exactly as the C# for-loop does.
    pub fn metronome_update(&mut self, m: usize) {
        let dt = self.time.deltaTime;
        let tempo = self.metronome_tempo(m);
        self.metronomes[m].animationTimer += dt * tempo;
        // distributor.RateHz = CurrentTempo: IntervalSeconds = 1f / value.
        let tempo = self.metronome_tempo(m);
        self.metronomes[m].distributor.IntervalSeconds = 1.0f32 / tempo;
        if self.metronomes[m].Activated {
            let mut i = 0;
            while i < self.metronomes[m].distributor.calculate_cycle_count(dt) {
                // SendMessage("IsolatedActivation", ..., DontRequireReceiver) for each channel: the metronome's own
                // object has no receiver (link stops on one), so nothing runs. The tick sound is audio.
                i += 1;
            }
        }
        // The hand's rotation is presentation.
    }

    /// MetronomeBehaviour.Use: `base.enabled && !activation.Contains(root)`; a direct activation's path holds no object.
    pub fn metronome_use(&mut self, m: usize) {
        if self.metronomes[m].enabled {
            self.metronomes[m].Activated = !self.metronomes[m].Activated;
            self.metronome_update_activation(m);
        }
    }

    /// MetronomeBehaviour.OnDisable: Activated = false, then StopAllCoroutines (it starts none).
    pub fn metronome_on_disable(&mut self, m: usize) {
        self.metronomes[m].Activated = false;
        let id = self.metronomes[m].id;
        self.stop_all_coroutines(id);
    }
}
