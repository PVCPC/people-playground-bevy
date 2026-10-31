//! ToggleableMirrorBehaviour: its FixedUpdate sets the physical's ReflectsLasers and AbsorbsLasers to whether its Charge is
//! under 0.67 (the two flags are equal). Awake and OnWillRenderObject only set the shader's progress: presentation.
#![allow(non_snake_case)]

use super::{script, Game};

script! {
    ToggleableMirror {}
    links {
        /// PhysicalBehaviour (GetComponent in Awake).
        physical: usize = 0,
    }
}

impl Game {
    /// ToggleableMirrorBehaviour.FixedUpdate
    pub fn toggleable_mirror_fixed_update(&mut self, m: usize) {
        let p = self.toggleable_mirrors[m].physical;
        // (double)Charge < 0.67
        let reflects = (self.physicals[p].Charge() as f64) < 0.67;
        self.physicals[p].ReflectsLasers = reflects;
        self.physicals[p].AbsorbsLasers = reflects;
    }
}
