//! ValveBehaviour: a BloodContainer whose ValvePosition (0 closed, 1 open) is driven by TurnRoutine along ValveTurnCurve.
//! Use (a direct activation, channel 0) turns it toward !LastTarget. Its container is limited to 0..0.5 and does not
//! overflow; below 0.5 the container keeps no pressure (AllowPressureTransfer). Awake, Start, the turn sound, the
//! display and OnWillRenderObject are presentation.
#![allow(non_snake_case)]

use super::container::{BloodContainer, Owner};
use super::coroutine::{Routine, Yield};
use super::{mathf, script, Game};
use crate::game_assets::{Curve, CurveCache};

script! {
    Valve {
        TurnDuration: f32 = 0.25,
        MinAngle: f32 = 0.0,
        MaxAngle: f32 = 90.0,
        ValvePosition: f32 = 0.0,
        LastTarget: bool = false,
    }
    base { base: BloodContainer }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// Behaviour.enabled
        enabled: bool = true,
        /// PhysicalBehaviour (GetComponent in Awake): only the turn sound reads it.
        physical: usize = 0,
        /// ValveTurnCurve and its native evaluation cache.
        ValveTurnCurve: Option<Curve> = None,
        cache: CurveCache = CurveCache::default(),
    }
}


impl Game {
    /// ValveBehaviour.Use: channel 1 opens, channel 2 closes, and any other channel toggles toward !LastTarget.
    pub fn valve_use(&mut self, v: usize, channel: i32) {
        let open = match channel {
            1 => true,
            2 => false,
            _ => !self.valves[v].LastTarget,
        };
        let id = self.valves[v].id;
        self.start_coroutine(id, id, Self::valve_turn_routine(v, open));
    }

    /// ValveBehaviour.TurnRoutine(open): each frame sets ValvePosition to the curve at t (1 - t when closing), waits for
    /// the end of the frame, then advances t by dt / TurnDuration. It ends once ValvePosition is approximately the target.
    fn valve_turn_routine(v: usize, open: bool) -> Routine {
        let target = if open { 1.0f32 } else { 0.0 };
        let mut t = 0.0f32;
        let mut started = false;
        Box::new(move |g| {
            if !started {
                started = true;
                g.valves[v].LastTarget = open;
                // PhysicalBehaviour.PlayClipOnce(TurnSound): audio.
                let p = g.valves[v].ValvePosition;
                t = if open { p } else { 1.0 - p };
            } else {
                t += g.time.deltaTime / g.valves[v].TurnDuration;
            }
            if mathf::approximately(g.valves[v].ValvePosition, target) {
                g.valves[v].ValvePosition = target;
                // SetValveRotationToValue: presentation.
                return Yield::Done;
            }
            let s = &mut g.valves[v];
            let x = if open { t } else { 1.0 - t };
            let next = s.ValveTurnCurve.as_ref().expect("ValveBehaviour.ValveTurnCurve").evaluate(&mut s.cache, x);
            s.ValvePosition = next;
            Yield::WaitForEndOfFrame
        })
    }

    /// ValveBehaviour.Update: the BloodContainer's update (the display offset is presentation).
    pub fn valve_update(&mut self, v: usize) {
        self.blood_container_update(Owner::Valve(v));
    }
}
