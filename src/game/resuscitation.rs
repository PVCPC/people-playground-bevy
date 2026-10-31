//! DefibrillatorBehaviour and CardiopulmonaryBypassMachineBehaviour.
//!
//! The defibrillator's Use runs DefibRoutine: busy for the two seconds of STAND BY plus the indicator clip's length, and
//! at the CLEAR both pads gain 0.2 charge and restart every circulation under them (IsPump back to what it started as,
//! full blood flow, 20 adrenaline, full consciousness). The text, the stand-clear clip and the indicator are
//! presentation, apart from the clip's length.
//!
//! The bypass machine's Use toggles Activated; UpdateActivation shows or hides its Cosmetics object (and so wakes or
//! stops what hangs under it). Its pumping is BloodWireBehaviour.ProcessCardiopulmonaryBypassWire (blood_wire.rs), the
//! only caller of GetFinalIntensity, PlayValidSound and PlayInvalidSound (audio, which draws no Random).
#![allow(non_snake_case)]

use super::coroutine::{Routine, Yield};
use super::spawn::Instance;
use super::{script, Game};
use crate::b2::math::Vec2 as V;

script! {
    Defibrillator {
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// Behaviour.enabled
        enabled: bool = true,
        /// Pad1 and Pad2 (PhysicalBehaviour).
        Pad1: usize = 0,
        Pad2: usize = 0,
        /// PadLayer (LayerMask bits).
        PadLayer: u32 = 0,
        /// AudioIndicator.length (None: a null clip, whose length throws).
        AudioIndicator: Option<f32> = None,
        /// busy: a DefibRoutine is running.
        busy: bool = false,
        /// bufferSize: the colliders of the last pad's overlap.
        bufferSize: i32 = 0,
    }
}

script! {
    CardiopulmonaryBypass {
        Activated: bool = false,
        PressureIntensity: f32 = 1.0,
        ValidConnectionCount: i32 = 0,
        TargetHeartrate: f32 = 70.0,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// The PhysicalBehaviour on its GameObject (a BloodWireBehaviour finds the machine through it).
        physical: usize = 0,
        /// Behaviour.enabled
        enabled: bool = true,
        /// Cosmetics: the index of its object in the prefab.
        Cosmetics: usize = 0,
    }
}

impl Game {
    /// DefibrillatorBehaviour.Use: while enabled and not busy, StartCoroutine(DefibRoutine()).
    pub fn defibrillator_use(&mut self, d: usize) {
        if self.defibrillators[d].enabled && !self.defibrillators[d].busy {
            let id = self.defibrillators[d].id;
            self.start_coroutine(id, id, Self::defibrillator_routine(d));
        }
    }

    /// DefibrillatorBehaviour.OnDisable: StopAllCoroutines, the text is cleared (presentation) and busy is false.
    pub fn defibrillator_on_disable(&mut self, d: usize) {
        let id = self.defibrillators[d].id;
        self.stop_all_coroutines(id);
        self.defibrillators[d].busy = false;
    }

    /// DefibrillatorBehaviour.DefibRoutine
    fn defibrillator_routine(d: usize) -> Routine {
        let mut stage = 0;
        Box::new(move |game| {
            stage += 1;
            match stage {
                // busy, "STAND BY" and the stand-clear clip; then two seconds.
                1 => {
                    game.defibrillators[d].busy = true;
                    Yield::WaitForSeconds(2.0)
                }
                // "CLEAR" and the indicator clip; both pads; then the clip's length.
                2 => {
                    let (pad1, pad2, layer) = (game.defibrillators[d].Pad1, game.defibrillators[d].Pad2, game.defibrillators[d].PadLayer);
                    game.defibrillate(d, pad1, layer);
                    game.defibrillate(d, pad2, layer);
                    let length = game.defibrillators[d].AudioIndicator.unwrap_or_else(|| unimplemented!("DefibrillatorBehaviour.DefibRoutine: AudioIndicator is null (NullReferenceException on .length)"));
                    Yield::WaitForSeconds(length)
                }
                _ => {
                    game.defibrillators[d].busy = false;
                    Yield::Done
                }
            }
        })
    }

    /// DefibrillatorBehaviour.Defibrillate
    pub(super) fn defibrillate(&mut self, d: usize, pad: usize, layer: u32) {
        let charge = self.physicals[pad].Charge();
        self.physicals[pad].charge = charge + 0.2;
        let at = self.transform_point3(pad, [-0.1, 0.0, 0.0]);
        // Physics2D.OverlapCircleNonAlloc into the 16-entry buffer.
        let mut hits = self.overlap_circle(V::new(at[0], at[1]), 0.1, layer);
        hits.truncate(16);
        self.defibrillators[d].bufferSize = hits.len() as i32;
        for (physical, _) in hits {
            // The collider's own GameObject holds the CirculationBehaviour, so the limb of its physical.
            let Some(l) = physical.and_then(|p| self.limbs.iter().position(|x| x.PhysicalBehaviour == p)) else { continue };
            self.circulations[l].IsPump = self.circulations[l].WasInitiallyPumping;
            self.circulations[l].BloodFlow = 1.0;
            let person = self.limbs[l].Person;
            self.people[person].AdrenalineLevel += 20.0;
            self.people[person].Consciousness = 1.0;
        }
    }

    /// CardiopulmonaryBypassMachineBehaviour.Use
    pub fn cardiopulmonary_bypass_use(&mut self, c: usize) {
        if self.cardiopulmonary_bypasses[c].enabled {
            self.cardiopulmonary_bypasses[c].Activated = !self.cardiopulmonary_bypasses[c].Activated;
        }
        let id = self.cardiopulmonary_bypasses[c].id;
        self.with_component_instance(id, |g, inst, _| g.cardiopulmonary_bypass_update_activation(c, inst));
    }

    /// CardiopulmonaryBypassMachineBehaviour.GetFinalIntensity: nothing while disabled or not activated.
    pub fn cardiopulmonary_bypass_final_intensity(&self, c: usize) -> f32 {
        let x = &self.cardiopulmonary_bypasses[c];
        if !x.enabled || !x.Activated {
            return 0.0;
        }
        x.PressureIntensity
    }

    /// CardiopulmonaryBypassMachineBehaviour.Start
    pub fn cardiopulmonary_bypass_start(&mut self, c: usize) {
        let id = self.cardiopulmonary_bypasses[c].id;
        self.with_component_instance(id, |g, inst, _| g.cardiopulmonary_bypass_update_activation(c, inst));
    }

    /// CardiopulmonaryBypassMachineBehaviour.UpdateActivation: Cosmetics.SetActive(Activated && enabled). The loop sound
    /// and the status text are presentation.
    pub fn cardiopulmonary_bypass_update_activation(&mut self, c: usize, inst: &mut Instance) {
        let x = &self.cardiopulmonary_bypasses[c];
        let (cosmetics, flag) = (x.Cosmetics, x.Activated && x.enabled);
        self.set_active_scripts(inst, cosmetics, flag);
    }
}
