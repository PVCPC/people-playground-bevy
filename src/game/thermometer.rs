//! ThermometerBehaviour (the Thermometer) and InfraredThermometer. Their readouts and lights are presentation: Start,
//! OnEnable, FixedUpdate and OnDisable only show or hide them or write text. The one state that Use changes is the
//! Activated flag, which nothing in the simulation reads. InfraredThermometer's Update reads the temperature of the
//! first thing its laser hits, with the Physics2D raycasts and Debug drawing, and it writes only its text and line.
#![allow(non_snake_case)]

use super::{mathf, script, Game};
use crate::unity::transform::{global_position, global_rotation, Trs};

script! {
    Thermometer {
        Activated: bool = true,
        Min: i32 = -999,
        Max: i32 = 9999,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// Behaviour.enabled
        enabled: bool = true,
    }
}

script! {
    InfraredThermometer {
        Activated: bool = true,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// Behaviour.enabled
        enabled: bool = true,
        /// GetComponent<PhysicalBehaviour>(): base.transform.localScale.x.
        physical: usize = 0,
        /// RayPoint: the local TRS of its object and of each parent below the thermometer's own object.
        RayPoint: Vec<Trs> = Vec::new(),
        /// LayersToHit (LayerMask bits)
        LayersToHit: u32 = 0,
    }
}

impl Game {
    /// MaintainScaleSignBehaviour.Start (EnsureScaling): a negative lossy scale.x flips the local scale.x back, so the
    /// object's own x is positive in the world. The object's other components are unchanged.
    pub fn maintain_scale_start(&mut self, id: i32) {
        self.with_component_instance(id, |_, inst, o| {
            if inst.lossy_scale(o)[0] < 0.0 {
                let s = inst.objects[o].trs.s;
                inst.set_local_scale(o, [-s[0], s[1], s[2]]);
            }
        });
    }

    /// ThermometerBehaviour.Use: Activated flips while the component is enabled.
    pub fn thermometer_use(&mut self, t: usize) {
        if self.thermometers[t].enabled {
            self.thermometers[t].Activated = !self.thermometers[t].Activated;
        }
    }

    /// InfraredThermometer.Use
    pub fn infrared_use(&mut self, i: usize) {
        if self.infrared_thermometers[i].enabled {
            self.infrared_thermometers[i].Activated = !self.infrared_thermometers[i].Activated;
        }
    }

    /// InfraredThermometer.Update: while Activated, ReadTemperatureFromRay casts Utils.GetLaserEndPoint (whose
    /// Physics2D.Raycast syncs transforms). The temperature it returns only becomes the readout text, and the vertices
    /// and `finalHit` it keeps only feed the line renderer: presentation.
    pub fn infrared_update(&mut self, i: usize) {
        let s = &self.infrared_thermometers[i];
        if !s.Activated {
            return;
        }
        let (layers, p) = (s.LayersToHit, s.physical);
        // RayPoint.position and .right: its local TRS up to the thermometer's object, then the object's chain.
        let nodes: Vec<Trs> = s.RayPoint.iter().copied().chain(self.local_chain(p).iter().rev().copied()).collect();
        let (mut origin, mut rotation) = ([nodes[0].t[0], nodes[0].t[1], nodes[0].t[2], 0.0], nodes[0].q);
        for n in &nodes[1..] {
            (origin, rotation) = (global_position(n, origin), global_rotation(n, rotation));
        }
        // RayPoint.right * (localScale.x < 0 ? -1 : 1), as a Vector2.
        let sign = if self.physicals[p].transform.localScale[0] < 0.0 { -1.0 } else { 1.0 };
        let right = mathf::rotate(rotation, [1.0, 0.0, 0.0]);
        let mut hits = Vec::new();
        self.get_laser_end_point([origin[0], origin[1]], [right[0] * sign, right[1] * sign], &mut hits, layers, 8.495455, 8);
    }

    /// InfraredThermometer.OnDisable: Activated = false (UpdateActivation hides its text and line).
    pub fn infrared_on_disable(&mut self, i: usize) {
        self.infrared_thermometers[i].Activated = false;
    }
}
