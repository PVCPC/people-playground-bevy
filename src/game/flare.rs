//! FlareBehaviour (the Signal Flare): once lit (Use, or Start while Activated) it enables its TorchBehaviour. The sprite,
//! sound and particles are presentation.
#![allow(non_snake_case)]

use super::{script, Game};

script! {
    Flare {
        Activated: bool = false,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// Behaviour.enabled (Use does not read it)
        enabled: bool = true,
        /// TorchBehaviour's instance id (`Game::torches` is found by it at run time).
        torch: i32 = 0,
    }
}

impl Game {
    /// The TorchBehaviour this flare enables.
    fn flare_torch(&self, f: usize) -> usize {
        let id = self.flares[f].torch;
        self.torches.iter().position(|t| t.id == id).expect("FlareBehaviour.TorchBehaviour without its TorchBehaviour")
    }

    /// FlareBehaviour.Start: a flare that starts Activated enables its torch.
    pub fn flare_start(&mut self, f: usize) {
        if self.flares[f].Activated {
            let t = self.flare_torch(f);
            self.torch_set_enabled(t);
        }
    }

    /// FlareBehaviour.Use: a flare that is not yet lit lights its torch and becomes Activated.
    pub fn flare_use(&mut self, f: usize) {
        if !self.flares[f].Activated {
            let t = self.flare_torch(f);
            self.torch_set_enabled(t);
            self.flares[f].Activated = true;
        }
    }
}
