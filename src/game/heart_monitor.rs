//! HeartMonitorBehaviour: a Use toggles Activated. While activated and connected (IsConnected) it measures the connected
//! limb's heartbeat each 1/24 s, and draws Random for each history sample. BloodWireBehaviour sets IsConnected and
//! ConnectedLimb (blood_wire.rs). The text, beeps, line and history display are presentation.
#![allow(non_snake_case)]

use super::{mathf, script, Game};

script! {
    HeartMonitor {
        /// Activated: toggled by Use.
        Activated: bool = false,
        /// IsConnected: set by a BloodWire (not rewritten), so false in every run here.
        IsConnected: bool = false,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// Behaviour.enabled
        enabled: bool = true,
        /// ConnectedLimb: the limb whose heartbeat is measured (set by the wire; a null one is an NRE in Measure).
        ConnectedLimb: Option<usize> = None,
        /// The private t and t2: the measurement timer and the beat timer.
        t: f32 = 0.0,
        t2: f32 = 0.0,
        /// The history of 20 samples (their line is presentation).
        history: [f32; 20] = [0.0; 20],
    }
}

impl Game {
    /// HeartMonitorBehaviour.Use: `if (base.enabled)` Activated flips (UpdateActivation is presentation).
    pub fn heart_monitor_use(&mut self, h: usize) {
        if self.heart_monitors[h].enabled {
            self.heart_monitors[h].Activated = !self.heart_monitors[h].Activated;
        }
    }

    /// HeartMonitorBehaviour.Update: nothing while not activated. Connected, the timers run and Measure runs; unconnected
    /// it only shows the text.
    pub fn heart_monitor_update(&mut self, h: usize) {
        if !self.heart_monitors[h].Activated {
            return;
        }
        if self.heart_monitors[h].IsConnected {
            let dt = self.time.deltaTime;
            self.heart_monitors[h].t += dt;
            self.heart_monitors[h].t2 += dt;
            self.heart_monitor_measure(h);
        }
    }

    /// HeartMonitorBehaviour.Measure: every 1/24 s, the BPM eases toward the connected limb's heartbeat (0 unless it is
    /// connected to the root), and each beat (t2 past 60 / BPM) samples the blood flow, other frames sample noise.
    /// currentBPM is never assigned, so its lerp starts from 0.
    fn heart_monitor_measure(&mut self, h: usize) {
        if self.heart_monitors[h].t < 1.0 / 24.0 {
            return;
        }
        self.heart_monitors[h].t = 0.0;
        let limb = self.heart_monitors[h].ConnectedLimb.unwrap_or_else(|| unimplemented!("HeartMonitorBehaviour.Measure: a null ConnectedLimb (NullReferenceException)"));
        let person = self.limbs[limb].Person;
        let target = if self.nodes[limb].IsConnectedToRoot { self.people[person].Heartbeat } else { 0.0 };
        let dt = self.time.deltaTime;
        // Mathf.Exp is (float)Math.Exp in double.
        let factor = 3.0f32 - ((-0.01f32 * dt) as f64).exp() as f32;
        let num = mathf::lerp(0.0, target, factor);
        let num2 = 60.0f32 / num;
        // The beeps and the BPM text (Mathf.RoundToInt(num)) are audio and presentation.
        if self.heart_monitors[h].t2 > num2 {
            self.heart_monitors[h].t2 = 0.0;
            let flow = mathf::clamp01(self.circulations[limb].BloodFlow);
            let sample = self.random.range(0.8, 1.0) * flow;
            self.heart_monitor_add_to_history(h, sample);
        } else {
            let sample = self.random.value() * 0.02;
            self.heart_monitor_add_to_history(h, sample);
        }
    }

    /// AddToHistory: shift the samples down and append (the line is presentation).
    fn heart_monitor_add_to_history(&mut self, h: usize, value: f32) {
        let hist = &mut self.heart_monitors[h].history;
        hist.copy_within(1..20, 0);
        hist[19] = value;
    }

    /// HeartMonitorBehaviour.OnDisable: Activated = false.
    pub fn heart_monitor_on_disable(&mut self, h: usize) {
        self.heart_monitors[h].Activated = false;
    }
}
