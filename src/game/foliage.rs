//! FoliageBehaviour (the bushes' and trees' leaves): Start's z jitter (a Random.value draw), and the burn progress its
//! OnWillRenderObject copies from the parent PhysicalBehaviour, which deactivates the leaf at 0.95. The property block
//! is presentation.
#![allow(non_snake_case)]

use super::{script, Game};

script! {
    Foliage {
        BurnProgress: f32 = 0.0,
        SyncWithParentPhysicalBehaviour: bool = true,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// phys: GetComponentInParent<PhysicalBehaviour>() in Awake when syncing.
        phys: Option<usize> = None,
        /// The `Game.renderers` entry of the object's own SpriteRenderer (the one OnWillRenderObject follows).
        renderer: usize = 0,
    }
}

impl Game {
    /// FoliageBehaviour.SetBurnProgressInternal: the property block, the field, and SetActive(false) from 0.95.
    fn foliage_set_burn_progress_internal(&mut self, f: usize, burn_progress: f32) {
        self.foliages[f].BurnProgress = burn_progress;
        if burn_progress >= 0.95 {
            self.with_component_instance(self.foliages[f].id, |g, inst, o| g.set_active_scripts(inst, o, false));
        }
    }

    /// FoliageBehaviour.Start
    pub fn foliage_start(&mut self, f: usize) {
        let id = self.foliages[f].id;
        self.foliage_set_burn_progress_internal(f, self.foliages[f].BurnProgress);
        let r = self.random.value();
        if !self.unsynced.iter().any(|i| i.objects.iter().any(|x| x.components.contains(&id))) {
            unimplemented!("FoliageBehaviour.Start: transform.position on a clone Physics2D already synced (its Rigidbody2D follows at the next SyncTransforms)");
        }
        self.with_component_instance(id, |_, inst, o| {
            let p = inst.position(o);
            inst.set_world_position(o, [p[0], p[1], p[2] + r * 0.5]);
        });
    }

    /// OnWillRenderObject of the leaves the camera draws, in renderer order: their SpriteRenderer is enabled and active
    /// in the hierarchy, and its bounds overlap the orthographic view (`view` None: everything is drawn).
    // ponytail: arena (instantiation) order stands in for Unity's culling order; it matters only when two leaves
    // deactivate in one frame (b2 free-list order), so compare against a recording of a burning bush if that shows.
    pub fn foliage_will_render(&mut self, view: Option<([f32; 2], [f32; 2])>) {
        for f in 0..self.foliages.len() {
            let r = &self.renderers[self.foliages[f].renderer];
            if !r.visible {
                continue;
            }
            let drawn = view.is_none_or(|(min, max)| {
                let (c, e) = crate::unity::transform::transform_aabb(r.bounds.0, r.bounds.1, &self.anchored_nodes(r.anchor, &r.chain));
                (0..2).all(|i| c[i] + e[i] >= min[i] && c[i] - e[i] <= max[i])
            });
            if !drawn {
                continue;
            }
            // if (SyncWithParentPhysicalBehaviour && (bool)phys) SetBurnProgress(phys.BurnProgress)
            let s = &self.foliages[f];
            let Some(p) = s.phys.filter(|_| s.SyncWithParentPhysicalBehaviour) else { continue };
            if self.destroyed_components.contains(&self.physicals[p].id) {
                continue;
            }
            let burn_progress = self.physicals[p].BurnProgress;
            if !((burn_progress - self.foliages[f].BurnProgress).abs() < 0.01) {
                self.foliage_set_burn_progress_internal(f, burn_progress);
            }
        }
    }
}
