//! AtomBombBehaviour and SeismicChargeBehaviour: Use, while enabled, instantiates ExplosionPrefab at the object's
//! position and destroys the GameObject (the NUCLEAR_EXPLOSIONS stat is not simulation state).

use super::Game;
use crate::assets::db::ObjRef;

#[derive(Clone, Debug, Default)]
pub struct PrefabBomb {
    /// The component's instance id.
    pub id: i32,
    pub enabled: bool,
    /// The PhysicalBehaviour on the same GameObject (its Transform).
    pub physical: usize,
    pub ExplosionPrefab: Option<ObjRef>,
}

impl Game {
    /// AtomBombBehaviour.Use / SeismicChargeBehaviour.Use
    pub fn prefab_bomb_use(&mut self, i: usize) {
        let b = self.prefab_bombs[i].clone();
        if b.enabled {
            let source = b.ExplosionPrefab.unwrap_or_else(|| unimplemented!("Object.Instantiate(null ExplosionPrefab) (ArgumentException)"));
            let position = self.physicals[b.physical].transform.position;
            self.spawn_prefab(source, position);
            self.destroy_object(b.id);
        }
    }
}
