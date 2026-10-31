//! TemperatureTransferBehaviour (the Ceiling Turret and the Reactor A5). Awake caches the object's PhysicalBehaviour;
//! FixedUpdate, while both physical objects simulate temperature, moves each one's temperature towards the other's by
//! GuideSpeed (both new values come from the temperatures read before either write).
#![allow(non_snake_case)]

use super::{mathf, script, Game};

script! {
    TemperatureTransfer {
        GuideSpeed: f32 = 0.001,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        enabled: bool = true,
        /// phys: GetComponent<PhysicalBehaviour>() (Awake).
        phys: usize = 0,
        /// Other: the serialized PhysicalBehaviour.
        Other: usize = 0,
    }
}

impl Game {
    /// TemperatureTransferBehaviour.FixedUpdate.
    pub fn temperature_transfer_fixed_update(&mut self, i: usize) {
        let (p, other, guide) = (self.temperature_transfers[i].phys, self.temperature_transfers[i].Other, self.temperature_transfers[i].GuideSpeed);
        if !self.physicals[p].SimulateTemperature {
            return;
        }
        // `Other.SimulateTemperature` is read only when the first check passed; a destroyed Other throws there.
        if self.physicals[other].destroyed {
            unimplemented!("TemperatureTransferBehaviour.Other destroyed (MissingReferenceException in FixedUpdate)");
        }
        if self.physicals[other].SimulateTemperature {
            let (temperature, temperature2) = (self.physicals[p].Temperature, self.physicals[other].Temperature);
            self.physicals[p].Temperature = mathf::lerp(temperature, temperature2, guide);
            self.physicals[other].Temperature = mathf::lerp(temperature2, temperature, guide);
        }
    }
}
