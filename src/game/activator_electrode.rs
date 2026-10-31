//! ActivatorElectrodeBehaviour (the Activator Electrode). Use and UseContinuous look for the physical objects at its
//! emission point (an OverlapCircle of EmissionRadius on LayersToConsider) and pass the activation on to them as a branch
//! through the electrode's root: BroadcastMessage("Use"/"UseContinuous") to the object, then OnSingleUse/OnContinuousUse.
//! The glow, the lights and the sound are presentation; Awake's and OnDestroy's empty ContinuousActivation listener
//! sends nothing.
#![allow(non_snake_case)]

use super::{script, Game};
use crate::b2::math::Vec2 as V;

/// The parts of an ActivationPropagation that a receiver reads: Direct, Channel and the Path of root Transform ids.
#[derive(Clone, Debug, Default)]
pub struct Act {
    pub direct: bool,
    pub channel: u16,
    pub path: Vec<i32>,
}

impl Act {
    /// new ActivationPropagation(direct: true, channel, ..): a path without objects.
    pub fn direct(channel: u16) -> Self {
        Self { direct: true, channel, path: Vec::new() }
    }

    /// ActivationPropagation.Contains
    pub fn contains(&self, root: i32) -> bool {
        self.path.contains(&root)
    }

    /// ActivationPropagation.Branch: the path plus `root`, no longer direct.
    pub fn branch(&self, root: i32) -> Self {
        let mut path = self.path.clone();
        path.push(root);
        Self { direct: false, channel: self.channel, path }
    }
}

script! {
    ActivatorElectrode {
        EmissionPoint: [f32; 2] = [0.1, 0.0],
        EmissionRadius: f32 = 0.05,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// Behaviour.enabled
        enabled: bool = true,
        /// GetComponent<PhysicalBehaviour>() (Awake)
        physical: usize = 0,
        /// LayersToConsider (LayerMask bits)
        LayersToConsider: u32 = 0,
    }
}

/// Physics2D.OverlapCircleNonAlloc's buffer
const BUFFER: usize = 4;

impl Game {
    /// base.transform.TransformPoint(EmissionPoint)
    fn electrode_center(&mut self, e: usize) -> V {
        let s = &self.activator_electrodes[e];
        let (p, point) = (s.physical, s.EmissionPoint);
        let c = self.transform_point3(p, [point[0], point[1], 0.0]);
        V::new(c[0], c[1])
    }

    /// `collider.gameObject != base.gameObject`, `PhysicalObjectsInWorldByTransform.TryGetValue(collider.transform)` and
    /// `!activation.Contains(base.transform.root)`: the physical to pass the activation on to.
    fn electrode_target(&self, e: usize, hit: Option<usize>, act: &Act) -> Option<usize> {
        let me = self.activator_electrodes[e].physical;
        let v = hit.filter(|&v| v != me && self.physicals[v].in_world)?;
        (!act.contains(self.root_id(me))).then_some(v)
    }

    /// ActivatorElectrodeBehaviour.Use
    pub fn activator_electrode_use(&mut self, e: usize, act: &Act) {
        if !self.activator_electrodes[e].enabled {
            return;
        }
        let center = self.electrode_center(e);
        let (radius, layers, me) = (self.activator_electrodes[e].EmissionRadius, self.activator_electrodes[e].LayersToConsider, self.activator_electrodes[e].physical);
        let hits = self.overlap_circle(center, radius, layers);
        for (physical, _) in hits.into_iter().take(BUFFER) {
            let Some(value) = self.electrode_target(e, physical, act) else { continue };
            let branch = act.branch(self.root_id(me));
            self.broadcast_use(value, &branch);
            // Utils.Activations.SendOnce: OnSingleUse.Invoke.
            self.activation_toggle_use_direct(value);
        }
        // Glow, StartLights and PlayClipOnce(StartUseSound) are presentation and audio (no draws).
    }

    /// ActivatorElectrodeBehaviour.UseContinuous: the first collider of Physics2D.OverlapCircle.
    pub fn activator_electrode_use_continuous(&mut self, e: usize, act: &Act) {
        if !self.activator_electrodes[e].enabled {
            return;
        }
        let center = self.electrode_center(e);
        let (radius, layers, me) = (self.activator_electrodes[e].EmissionRadius, self.activator_electrodes[e].LayersToConsider, self.activator_electrodes[e].physical);
        let Some(&(physical, _)) = self.overlap_circle(center, radius, layers).first() else { return };
        if let Some(value) = self.electrode_target(e, physical, act) {
            let branch = act.branch(self.root_id(me));
            self.broadcast_use_continuous(value, &branch);
            // Utils.Activations.SendContinuous: OnContinuousUse.Invoke has no listener.
        }
    }
}
