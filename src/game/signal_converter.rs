//! SignalConverterBehaviour (the BG, BR and RG Signal Converters): Use and UseContinuous swap ChannelA and ChannelB of
//! a wired (not direct) activation and SendMessage it to the converter's own GameObject as IsolatedActivation or
//! IsolatedContinuousActivation. Only UseWireBehaviour has those methods, and link refuses one on the converter's
//! object. The activation the converter builds joins ActivationPropagation.ActiveSignalSet (statistics only).
#![allow(non_snake_case)]

use super::{script, Game};

script! {
    SignalConverter {
        ChannelA: u16 = 0,
        ChannelB: u16 = 0,
    }
    links {
        /// A UseWireBehaviour on the converter's GameObject (it would receive the isolated activation).
        wired: bool = false,
    }
}

impl SignalConverter {
    /// SignalConverterBehaviour.ConvertChannel: the outgoing channel, when the incoming one is converted.
    pub fn convert_channel(&self, direct: bool, channel: u16) -> Option<u16> {
        if direct {
            None
        } else if channel == self.ChannelA {
            Some(self.ChannelB)
        } else if channel == self.ChannelB {
            Some(self.ChannelA)
        } else {
            None
        }
    }
}

impl Game {
    /// SignalConverterBehaviour.Use
    pub fn signal_converter_use(&mut self, i: usize, direct: bool, channel: u16) {
        if self.signal_converters[i].convert_channel(direct, channel).is_some() && self.signal_converters[i].wired {
            unimplemented!("SignalConverterBehaviour.IsolatedActivation received by a UseWireBehaviour");
        }
    }

    /// SignalConverterBehaviour.UseContinuous (ContinuousActivationBehaviour.AssertState holds inside its LateUpdate).
    pub fn signal_converter_use_continuous(&mut self, i: usize, direct: bool, channel: u16) {
        if self.signal_converters[i].convert_channel(direct, channel).is_some() && self.signal_converters[i].wired {
            unimplemented!("SignalConverterBehaviour.IsolatedContinuousActivation received by a UseWireBehaviour");
        }
    }
}
