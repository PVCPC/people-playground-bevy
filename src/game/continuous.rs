//! ContinuousActivationBehaviour and PhysicalBehaviour's ContinuousActivationTracker: DragTool's UseContinuous
//! broadcast (40 Hz) and the IsBeingUsedContinuously family the scripts read.
#![allow(non_snake_case)]

use super::misc::FixedIntervalDistributor;
use super::{Behaviour, Game};
use crate::assets::db::Component;

/// DeltaInt
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DeltaInt {
    pub Value: i32,
    pub PreviousValue: i32,
    pub PreviousValueFixed: i32,
}

/// ContinuousActivationBehaviour: activationSignalProcessor (RateHz 40, MaxCycles 4) and the oscillation counter.
/// The cleanup of the ActivationPropagation signal set only returns pooled path sets.
#[derive(Clone, Debug)]
pub struct ContinuousActivation {
    pub signals: FixedIntervalDistributor,
    pub oscillation: u32,
}

impl Default for ContinuousActivation {
    fn default() -> Self {
        Self { signals: FixedIntervalDistributor { MaxCycles: 4, ..FixedIntervalDistributor::with_rate(40.0) }, oscillation: 0 }
    }
}

/// ActivationPropagation.AllChannels
const CHANNELS: usize = 3;

/// The classes with a UseContinuous(ActivationPropagation) method, and their subclasses.
const USE_CONTINUOUS_RECEIVERS: &[&str] = &[
    "ActivatorElectrodeBehaviour", "BeamRepeaterBehaviour", "BlasterBehaviour", "ElectricityTransformerBehaviour",
    "PhysicalBehaviour", "SignalConverterBehaviour", "TemperatureRayGunBehaviour", "TransmitterBehaviour", "UseBubbler",
    "UseWireBehaviour",
];

/// A receiver of the broadcast, in BroadcastMessage's order.
enum Receiver {
    Physical(usize),
    Behaviour(Behaviour),
}

impl Game {
    /// PhysicalBehaviour.IsBeingUsedContinuously(channel)
    pub fn used_continuously_on(&self, p: usize, channel: usize) -> bool {
        self.physicals[p].ContinuousActivationTracker[channel].Value > 0
    }

    /// PhysicalBehaviour.IsBeingUsedContinuously()
    pub fn used_continuously(&self, p: usize) -> bool {
        (0..CHANNELS).any(|c| self.used_continuously_on(p, c))
    }

    /// PhysicalBehaviour.StartedBeingUsedContinuously()
    pub fn started_used_continuously(&self, p: usize) -> bool {
        self.physicals[p].ContinuousActivationTracker.iter().any(|t| t.Value > 0 && t.PreviousValue == 0)
    }

    /// PhysicalBehaviour.StoppedBeingUsedContinuously()
    pub fn stopped_used_continuously(&self, p: usize) -> bool {
        self.physicals[p].ContinuousActivationTracker.iter().any(|t| t.Value == 0 && t.PreviousValue > 0)
    }

    /// Global.PhysicalObjectsInWorld
    fn physicals_in_world(&self) -> Vec<usize> {
        (0..self.physicals.len()).filter(|&p| self.physicals[p].in_world && !self.physicals[p].destroyed).collect()
    }

    /// ContinuousActivationBehaviour.LateUpdate (execution order -10, before every other LateUpdate).
    pub fn continuous_activation_late_update(&mut self) {
        for p in self.physicals_in_world() {
            for t in self.physicals[p].ContinuousActivationTracker.iter_mut() {
                t.PreviousValue = t.Value;
            }
        }
        // The loop condition calls CalculateCycleCount(Time.deltaTime) on every pass.
        let mut k = 0;
        while k < self.continuous.signals.calculate_cycle_count(self.time.deltaTime) {
            self.drag_continuous_update();
            if self.continuous.oscillation % 2 == 0 {
                for p in self.physicals_in_world() {
                    for t in self.physicals[p].ContinuousActivationTracker.iter_mut() {
                        if t.PreviousValueFixed == t.Value {
                            t.Value = 0;
                        }
                        t.PreviousValueFixed = t.Value;
                    }
                }
            }
            self.continuous.oscillation = self.continuous.oscillation.wrapping_add(1);
            k += 1;
        }
    }

    /// DragTool.OnContinuousUpdate: the ActivationPropagation(direct: true, 0) goes to the selected objects.
    fn drag_continuous_update(&mut self) {
        let Some(t) = self.tool.as_ref() else { return };
        if !t.shouldSendContinuousUseMessage {
            return;
        }
        for p in t.rawSelection.clone() {
            if self.physicals[p].destroyed {
                continue;
            }
            // SendContinuous: OnContinuousUse.Invoke. Its only listener is ActivationToggleBehaviour, which link rejects.
            self.broadcast_use_continuous(p, 0);
        }
    }

    /// BroadcastMessage("UseContinuous", activation) on a physical object's GameObject: it and its active
    /// descendants, each object's components in order.
    pub fn broadcast_use_continuous(&mut self, p: usize, channel: usize) {
        let id = self.physicals[p].id;
        let receivers: Vec<Receiver> = self.with_component_instance(id, |g, inst, start| {
            let mut out = Vec::new();
            for x in std::iter::once(start).chain(inst.descendants(start)) {
                if !inst.objects[x].active {
                    continue;
                }
                for (k, (_, c)) in inst.prefab.objects[x].components.iter().enumerate() {
                    let Component::Script { class, .. } = c else { continue };
                    if !USE_CONTINUOUS_RECEIVERS.contains(&class.as_str()) {
                        continue;
                    }
                    let cid = inst.objects[x].components[k];
                    if class == "PhysicalBehaviour" {
                        let q = g.physicals.iter().position(|ph| ph.id == cid).expect("a PhysicalBehaviour in the arena");
                        out.push(Receiver::Physical(q));
                        continue;
                    }
                    match g.behaviour_of.get(&cid) {
                        Some(&(_, b @ Behaviour::RayGun(_))) => out.push(Receiver::Behaviour(b)),
                        Some(&(_, b @ Behaviour::Translated(t))) if t.has_use_continuous() => out.push(Receiver::Behaviour(b)),
                        _ => unimplemented!("{class}.UseContinuous"),
                    }
                }
            }
            out
        });
        for r in receivers {
            match r {
                // PhysicalBehaviour.UseContinuous: AssertState (true inside the LateUpdate), then the tracker.
                Receiver::Physical(q) => self.physicals[q].ContinuousActivationTracker[channel].Value += 1,
                Receiver::Behaviour(Behaviour::RayGun(i)) => self.ray_gun_use_continuous(i),
                Receiver::Behaviour(Behaviour::Translated(t)) => self.tr_use_continuous(t, super::translated::rt::Activation { Channel: channel as i32 }),
                Receiver::Behaviour(_) => unreachable!(),
            }
        }
    }
}
