//! TimedGateBehaviour, a LagboxBehaviour: a Use (while enabled and not busy) starts DoLag, which holds the physical's
//! ActivationPropagationDelay at -1 for DelayModifier seconds of frames and then restores it to 0. Start, OnEnable and
//! OnDisable keep the coroutine running or stop it. Lights, knob, labels and the bell and redlight sounds are presentation.
#![allow(non_snake_case)]

use super::coroutine::{Routine, Yield};
use super::{script, Game};

script! {
    TimedGate {
        /// LagboxBehaviour.DelayModifier (the gate's duration in seconds, from 0.04 to 16).
        DelayModifier: f32 = 1.0,
        /// isBusy: a DoLag is running.
        isBusy: bool = false,
        /// The private t: how far the gate has gone, 0 to 1.
        t: f32 = 0.0,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// Behaviour.enabled
        enabled: bool = true,
        /// PhysicalBehaviour (its ActivationPropagationDelay is the gate's state).
        physical: usize = 0,
    }
}

impl Game {
    /// TimedGateBehaviour.Awake (after LagboxBehaviour.Awake's MuteBehaviour lookup): PhysicalBehaviour.ActivationPropagationDelay = 0.
    pub fn timed_gate_awake(&mut self, g: usize) {
        let p = self.timed_gates[g].physical;
        self.physicals[p].ActivationPropagationDelay = 0.0;
    }

    /// TimedGateBehaviour.Start: a gate that was busy resumes DoLag.
    pub fn timed_gate_start(&mut self, g: usize) {
        if self.timed_gates[g].isBusy {
            self.timed_gate_start_lag(g);
        }
    }

    /// TimedGateBehaviour.OnEnable: a busy gate resumes DoLag.
    pub fn timed_gate_on_enable(&mut self, g: usize) {
        if self.timed_gates[g].isBusy {
            self.timed_gate_start_lag(g);
        }
    }

    /// TimedGateBehaviour.OnDisable: StopAllCoroutines (the physical's delay is left as it is).
    pub fn timed_gate_on_disable(&mut self, g: usize) {
        let id = self.timed_gates[g].id;
        self.stop_all_coroutines(id);
    }

    /// TimedGateBehaviour.Use: while enabled and not busy, t = 0 and DoLag starts (the redlight sound is audio).
    pub fn timed_gate_use(&mut self, g: usize) {
        if self.timed_gates[g].enabled && !self.timed_gates[g].isBusy {
            self.timed_gates[g].t = 0.0;
            self.timed_gate_start_lag(g);
        }
    }

    fn timed_gate_start_lag(&mut self, g: usize) {
        let id = self.timed_gates[g].id;
        self.start_coroutine(id, id, Self::timed_gate_lag_routine(g));
    }

    /// TimedGateBehaviour.DoLag. The first step yields a frame only when the delay is not yet -1; then the delay is -1
    /// while t runs from its value to 1 (each step adds dt / DelayModifier and waits for the end of the frame), and the
    /// gate ends with the delay back at 0 and isBusy false.
    fn timed_gate_lag_routine(g: usize) -> Routine {
        #[derive(Clone, Copy)]
        enum Stage {
            Begin,
            AfterFirstWait,
            Running,
        }
        let mut stage = Stage::Begin;
        Box::new(move |game| {
            let p = game.timed_gates[g].physical;
            match stage {
                Stage::Begin => {
                    game.timed_gates[g].isBusy = true;
                    // Knob and light changes are presentation.
                    if game.physicals[p].ActivationPropagationDelay >= 0.0 {
                        stage = Stage::AfterFirstWait;
                        return Yield::WaitForEndOfFrame;
                    }
                    game.physicals[p].ActivationPropagationDelay = -1.0;
                    stage = Stage::Running;
                }
                Stage::AfterFirstWait => {
                    game.physicals[p].ActivationPropagationDelay = -1.0;
                    stage = Stage::Running;
                }
                Stage::Running => {}
            }
            let s = &mut game.timed_gates[g];
            if s.t < 1.0 {
                s.t += game.time.deltaTime / s.DelayModifier;
                return Yield::WaitForEndOfFrame;
            }
            // The redlight is off and the greenlight on, the delay is restored, and the gate is no longer busy (the ding is audio).
            game.physicals[p].ActivationPropagationDelay = 0.0;
            game.timed_gates[g].isBusy = false;
            Yield::Done
        })
    }
}
