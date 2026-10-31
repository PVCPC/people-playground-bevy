//! ButtonBehaviour (the Button and the Small Button). FixedUpdate reads its SliderJoint2D's jointTranslation and is pressed
//! while |translation| < Threshold * lossyScale.y. A press (or, with TriggerOnExit, a release) sends IsolatedActivation
//! to the button's own GameObject, and the continuous listener sends IsolatedContinuousActivation there too. SendMessage
//! reaches that object's components only, and link.rs refuses any receiver on it, so those sends do nothing. Awake and
//! OnDestroy only add and remove the listener, Start is the context menu (UI), and Update and OnDisable drive the light.
#![allow(non_snake_case)]

use super::{script, Game};

script! {
    Button {
        Threshold: f32 = 0.1,
        TriggerOnExit: bool = false,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// slider: the SliderJoint2D's object and component index in the prefab (its joint lives on the clone).
        slider_object: usize = 0,
        slider_component: usize = 0,
        /// ButtonBehaviour.isPressed
        isPressed: bool = false,
    }
}

impl Game {
    /// slider.jointTranslation (Joint2D, then SliderJoint2D's native read): the slider's `fixed_joints` entry when a
    /// script adopted it, otherwise the clone's live b2 prismatic joint (no live joint is unimplemented).
    fn button_slider_translation(&mut self, id: i32, object: usize, component: usize) -> f32 {
        let (slot, joint) = self.with_component_instance(id, |_, inst, _| (inst.anchored.get(&(object, component)).and_then(|s| s.slot), inst.joints.get(&(object, component)).copied()));
        if let Some(f) = slot.and_then(|s| self.adopted_slots.get(&s).copied()) {
            return self.slider_joint_translation(f);
        }
        match joint {
            None => unimplemented!("ButtonBehaviour.slider.jointTranslation without a live SliderJoint2D (broken or destroyed slider)"),
            Some(j) => {
                let j = self.world.joints[j].as_ref().unwrap();
                j.prismatic().joint_translation(self.world.body(j.body_a), self.world.body(j.body_b))
            }
        }
    }

    /// ButtonBehaviour.FixedUpdate. The press sends (and the TriggerOnExit release) reach no receiver, as noted above.
    pub fn button_fixed_update(&mut self, b: usize) {
        let s = &self.buttons[b];
        let (id, object, component, threshold) = (s.id, s.slider_object, s.slider_component, s.Threshold);
        let translation = self.button_slider_translation(id, object, component);
        // base.transform.lossyScale.y of the button's own object.
        let scale = self.with_component_instance(id, |_, inst, o| inst.lossy_scale(o)[1]);
        self.buttons[b].isPressed = translation.abs() < threshold * scale;
    }
}
