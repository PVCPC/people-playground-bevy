//! The map options' clear buttons: ClearDebrisBehaviour ("Clear Mess"), ClearLivingBehaviour ("Clear Living") and
//! ClearButtonBehaviour ("Clear Everything").
use super::Game;

impl Game {
    /// ClearDebrisBehaviour.Clear: every DecalControllerBehaviour clears (localDecalPositions; the holder objects are
    /// rendering), every DebrisComponent's GameObject is destroyed and its undo action dropped.
    pub fn clear_debris(&mut self) {
        // FindObjectsOfType finds the components of GameObjects active in the hierarchy.
        for p in 0..self.physicals.len() {
            if self.physicals[p].active && !self.physicals[p].destroyed {
                for c in &mut self.physicals[p].decal_controllers {
                    c.decals.clear();
                }
            }
        }
        let mut debris = Vec::new();
        for inst in self.unsynced.iter().chain(&self.retained) {
            for (x, o) in inst.objects.iter().enumerate() {
                let key = o.components[0];
                let carried = inst.prefab.objects[x].components.iter().any(|(_, c)| matches!(c, crate::assets::db::Component::Script { class, .. } if class == "DebrisComponent"));
                if o.active && !self.destroyed_components.contains(&key) && (carried || self.debris_added.contains(&key)) {
                    debris.push(key);
                }
            }
        }
        for key in debris {
            self.destroy_object(key);
            self.undo_deregister_related(key);
        }
        self.notify("Cleared debris".into());
    }

    /// ClearLivingBehaviour.Clear: Object.Destroy(transform.root.gameObject) of every AliveBehaviour (only people are
    /// spawnable; DroneBehaviour is refused at spawn).
    pub fn clear_living(&mut self) {
        for i in 0..self.people.len() {
            // A person whose root is already queued (all limbs disintegrated) has its Destroy pending: a second one does nothing.
            // No script deactivates a person's root, so every other one is found.
            if self.people[i].destroyed || self.people[i].destroy_at.is_some() {
                continue;
            }
            let Some(&l) = self.people[i].Limbs.first() else { continue };
            let root = self.root_id(self.limbs[l].PhysicalBehaviour);
            self.destroy_object(root);
        }
        self.notify("Cleared living".into());
    }

    /// ClearButtonBehaviour.ClearEverything -> Global.ClearMap(ToRemove, false).
    pub fn clear_everything(&mut self) {
        unimplemented!("ClearButtonBehaviour.ClearEverything: Global.ClearMap (ObjectPoolBehaviour.Clear, MapLoaderBehaviour.Load destroying and re-instantiating the map)");
    }
}
