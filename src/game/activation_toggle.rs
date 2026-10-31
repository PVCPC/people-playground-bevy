//! ActivationToggleBehaviour (the Activation Toggle): a direct activation of its physical flips Activated, and the light
//! object follows it. While Activated it sends IsolatedContinuousActivation on every channel each continuous step, to its
//! own object's receivers (link stops if there is one). OnDisable clears Activated. The light's own scripts start with
//! its SetActive, as with any GameObject.
#![allow(non_snake_case)]

use super::{script, Game};

script! {
    ActivationToggle {
        Activated: bool = false,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// Behaviour.enabled
        enabled: bool = true,
        /// GetComponent<PhysicalBehaviour>() (Awake subscribes to its OnSingleUse).
        physical: usize = 0,
        /// LightObject's object index in the prefab (the instance shares it).
        light: usize = 0,
    }
}

impl Game {
    /// ActivationToggleBehaviour.UpdateActivation: LightObject.SetActive(Activated).
    fn activation_toggle_update(&mut self, t: usize) {
        let (id, light, active) = (self.activation_toggles[t].id, self.activation_toggles[t].light, self.activation_toggles[t].Activated);
        self.with_component_instance(id, |g, inst, _| g.set_active_scripts(inst, light, active));
    }

    /// ActivationToggleBehaviour.Start: the continuous listener (sends nothing but the own-object messages) and UpdateActivation.
    pub fn activation_toggle_start(&mut self, t: usize) {
        self.activation_toggle_update(t);
    }

    /// ActivationToggleBehaviour.OnUse (the physical's OnSingleUse): Activated flips while enabled.
    fn activation_toggle_on_use(&mut self, t: usize) {
        if self.activation_toggles[t].enabled {
            self.activation_toggles[t].Activated = !self.activation_toggles[t].Activated;
            self.activation_toggle_update(t);
        }
    }

    /// The OnSingleUse invocation of a direct activation of physical `p`: every live toggle subscribed from its Awake,
    /// in subscription order. A destroyed toggle's OnDestroy removed its listener.
    pub fn activation_toggle_use_direct(&mut self, p: usize) {
        for t in 0..self.activation_toggles.len() {
            let s = &self.activation_toggles[t];
            if s.physical == p && !self.destroyed_components.contains(&s.id) {
                self.activation_toggle_on_use(t);
            }
        }
    }

    /// ActivationToggleBehaviour.OnDisable: Activated = false, then UpdateActivation.
    pub fn activation_toggle_on_disable(&mut self, t: usize) {
        self.activation_toggles[t].Activated = false;
        self.activation_toggle_update(t);
    }

    /// The same OnDisable while the toggle's clone is in hand (the SetActive(false) path, water.rs): UpdateActivation is
    /// LightObject.SetActive(false) on that clone.
    pub(super) fn activation_toggle_on_disable_in(&mut self, inst: &mut super::spawn::Instance, t: usize) {
        self.activation_toggles[t].Activated = false;
        let light = self.activation_toggles[t].light;
        self.set_active_scripts(inst, light, false);
    }
}
