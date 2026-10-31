//! PowerGaugeBehaviour (GaugeBehaviour's Start, Awake and Update for the power gauge). Start instantiates LabelCount label
//! objects, whose text is presentation and which carry no physics. Their position under GaugeCenter is presentation too,
//! so each label is spawned at GaugeCenter's position. Update and Awake only move the arm and read the charge.
#![allow(non_snake_case)]

use super::{script, Game};
use crate::assets::db::ObjRef;

script! {
    PowerGauge {
        /// LabelCount (GaugeBehaviour)
        LabelCount: i32 = 2,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// GaugeLabelPrefab (None: Instantiate(null) would throw).
        label: Option<ObjRef> = None,
        /// GaugeCenter's object index in the prefab.
        center: usize = 0,
    }
}

impl Game {
    /// GaugeBehaviour.Start: CreateLabels, LabelCount instances of GaugeLabelPrefab under GaugeCenter.
    pub fn power_gauge_start(&mut self, g: usize) {
        let (id, center, count) = (self.power_gauges[g].id, self.power_gauges[g].center, self.power_gauges[g].LabelCount);
        let label = self.power_gauges[g].label.unwrap_or_else(|| unimplemented!("GaugeBehaviour.CreateLabels: Instantiate(null GaugeLabelPrefab) (ArgumentException)"));
        let at = self.with_component_instance(id, |_, inst, _| inst.position(center));
        for _ in 0..count.max(0) {
            self.spawn_prefab(label, at);
        }
    }
}
