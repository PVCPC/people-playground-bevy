//! BellBehaviour
#![allow(non_snake_case)]

use super::{mathf, script, Game};

script! {
    Bell {
        Counter: i32 = 0,
        Intensity: f32 = 1.0,
        Offset: f32 = 0.1,
    }
    links {
        /// PhysicalBehaviour, on the bell's own GameObject.
        physical: usize = 0,
    }
}

impl Game {
    /// BellBehaviour.Use; PlayClipOnce is audio.
    pub fn bell_use(&mut self, b: usize) {
        self.bells[b].Counter += 1;
        let p = self.bells[b].physical;
        self.pick_random(self.physicals[p].OverrideImpactSounds);
        let x = if self.bells[b].Counter % 2 == 0 { -1.0 } else { 1.0 };
        let (k, offset) = (self.bells[b].Intensity, self.bells[b].Offset);
        let dir = self.transform_direction(p, [x, 0.0, 0.0]);
        let t = self.physicals[p].transform;
        let up = mathf::rotate(t.rotation, [0.0, 1.0, 0.0]);
        let force = [dir[0] * k, dir[1] * k];
        let at = [t.position[0] - up[0] * offset, t.position[1] - up[1] * offset];
        let rb = self.physicals[p].rigidbody.clone();
        rb.add_force_at_position(&mut self.world, force, at);
    }
}
