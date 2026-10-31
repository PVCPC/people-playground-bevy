//! DisplayBehaviour (the Holographic Display and the Text Display). Awake draws Random.value and, above 0.9, takes Value
//! from randomStartStrings. Use flips Activated and OnDisable clears it, and Activated only shows or hides the text and
//! the light (presentation). Start writes the text and the context menu (UI). The text object holds only TextMeshPro
//! and marker scripts, so its SetActive is presentation. Edit text sets Value from a dialog (UI) through
//! `display_set_text`, whose DontMindMe events run their persistent calls, audio aside.
#![allow(non_snake_case)]

use super::acts::ActTarget;
use super::{script, Game};

/// DisplayBehaviour.TotallyNotAnEasterEgg: a Match and the persistent calls of its Event.
#[derive(Clone, Debug)]
pub struct EasterEgg {
    pub Match: String,
    pub Event: Vec<ActTarget>,
}

script! {
    Display {
        Activated: bool = true,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// Behaviour.enabled
        enabled: bool = true,
        /// Value. A null Value (an empty randomStartStrings) is kept as "": TextMesh shows nothing for either, and no
        /// Match is empty.
        Value: String = String::new(),
        randomStartStrings: Vec<String> = Vec::new(),
        DontMindMe: Vec<EasterEgg> = Vec::new(),
    }
}

impl Game {
    /// DisplayBehaviour.Awake: `if ((double)Random.value > 0.9) Value = randomStartStrings.PickRandom();`. PickRandom
    /// of an empty list draws nothing and gives null.
    pub fn display_awake(&mut self, d: usize) {
        if self.random.value() as f64 > 0.9 {
            let n = self.displays[d].randomStartStrings.len();
            self.displays[d].Value = match self.pick_random(n) {
                Some(i) => self.displays[d].randomStartStrings[i].clone(),
                None => String::new(),
            };
        }
    }

    /// DisplayBehaviour.Use: `if (base.enabled)` Activated flips (UpdateActivation is presentation).
    pub fn display_use(&mut self, d: usize) {
        if self.displays[d].enabled {
            self.displays[d].Activated = !self.displays[d].Activated;
        }
    }

    /// DisplayBehaviour.OnDisable: Activated = false (UpdateActivation is presentation).
    pub fn display_on_disable(&mut self, d: usize) {
        self.displays[d].Activated = false;
    }

    /// DisplayBehaviour.setTextFromUserInput, the Edit text dialog's callback (UI, so nothing in the rewrite calls it yet):
    /// Value is set, then each DontMindMe whose Match is the new Value runs its Event, in order.
    #[allow(dead_code)]
    pub fn display_set_text(&mut self, d: usize, value: String) {
        self.displays[d].Value = value;
        for i in 0..self.displays[d].DontMindMe.len() {
            if self.displays[d].DontMindMe[i].Match == self.displays[d].Value {
                for t in self.displays[d].DontMindMe[i].Event.clone() {
                    self.run_act_target(t);
                }
            }
        }
    }
}
