//! GlowInTheDarkBehaviour, which Tritium adds to the containers it enters: only its instance ids reach the simulation
//! (the light is rendering).
#![allow(non_snake_case)]

use super::container::Owner;
use super::Game;

#[derive(Clone, Copy, Debug)]
pub struct GlowInTheDark {
    /// The component's instance id.
    pub id: i32,
    /// The BloodContainer on its GameObject.
    pub owner: Owner,
}

impl Game {
    /// Tritium.OnEnterContainer: container.gameObject.GetOrAddComponent<GlowInTheDarkBehaviour>() (hand-written for
    /// tools/iltrans, HANDWRITTEN_MEMBERS).
    pub fn tritium_on_enter_container(&mut self, _this: usize, o: Owner) {
        // A destroyed component stays until its delayed Destroy runs.
        if self.glows.iter().any(|g| g.owner == o && !self.destroyed_components.contains(&g.id)) {
            return;
        }
        let id = self.allocate_instance_id();
        self.glows.push(GlowInTheDark { id, owner: o });
        self.pending_starts.push(super::misc::PendingStart::Glow(self.glows.len() - 1));
    }

    /// Tritium.OnExitContainer: Object.Destroy of its GlowInTheDarkBehaviour (OnDestroy removes the light).
    pub fn tritium_on_exit_container(&mut self, _this: usize, o: Owner) {
        if let Some(g) = self.glows.iter().find(|g| g.owner == o && !self.destroyed_components.contains(&g.id)) {
            let id = g.id;
            self.tr_destroy_component(id, 0.0);
        }
    }

    /// GlowInTheDarkBehaviour.Start: new GameObject(name, typeof(Optout)) takes the GameObject's, Transform's and
    /// Optout's ids, then ModdingApi.CreateLight instantiates Prefabs/ModLightPrefab under it (LightSprite.Awake only
    /// reads its renderer).
    pub fn glow_start(&mut self, g: usize) {
        if self.destroyed_components.contains(&self.glows[g].id) {
            return;
        }
        for _ in 0..3 {
            self.allocate_instance_id();
        }
        let mut db = self.assets.take().expect("GlowInTheDarkBehaviour without the game's data");
        let count = db.resource("Prefabs/ModLightPrefab", 1).ok().flatten().and_then(|r| db.prefab(r).ok()).map(|p| p.objects.iter().map(|o| 1 + o.components.len()).sum::<usize>());
        self.assets = Some(db);
        let count = count.unwrap_or_else(|| panic!("Resources.Load(Prefabs/ModLightPrefab) found no GameObject"));
        for _ in 0..count {
            self.allocate_instance_id();
        }
    }
}
