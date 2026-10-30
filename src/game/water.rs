//! WaterBehaviour and WaterBehaviourManager: the water volumes (their trigger collider, buoyancy and drag on
//! every physical object, the temperature states) and the IsUnderWater flag they keep.
use super::misc::{ObjectPool, PendingStart, SlotSet};
use super::{d, mathf, script, BodyOwner, Game};
use crate::assets::db::{AssetDb, Component, ObjRef, Prefab};
use crate::assets::mono::Value;
use std::collections::HashMap;
use std::sync::Arc;
use crate::b2::{math::Vec2 as V, world::FixtureId};
use crate::unity::transform::{global_position, Trs};

script! {
    Water {
        Drag: f32 = 0.0,
        Buoyancy: f32 = 0.0,
        LocalSurfaceLevel: f32 = 0.0,
        FreezingPointCelsius: f32 = 0.0,
        BoilingPointCelsius: f32 = 100.0,
    }
    links {
        /// The component's instance id, and its (object, component) in the clone's prefab.
        id: i32 = 0,
        object: usize = 0,
        component: usize = 0,
        enabled: bool = false,
        /// GameObject.activeInHierarchy.
        active: bool = false,
        /// The object's own Transform, then each parent's, for TransformPoint (water never moves).
        chain: Vec<Trs> = Vec::new(),
        /// The Trigger collider's instance id and fixtures.
        trigger: i32 = 0,
        trigger_fixtures: Vec<FixtureId> = Vec::new(),
        /// The Ice PhysicalBehaviour (its `physicals` index) and its object in the prefab.
        ice: usize = 0,
        ice_object: usize = 0,
        /// BoilingWaterSound and ObjectsToBeActiveIfWater, as prefab objects.
        boiling_sound: usize = 0,
        objects_to_be_active: Vec<usize> = Vec::new(),
        /// waterSizzles.Length (Resources.LoadAll<AudioClip>("Audio/water_sizzle")).
        waterSizzles: usize = 0,
        /// The pool of its Splash prefab (`Game::splash_pools`); None when Splash is null.
        splash_pool: Option<usize> = None,
        /// wasFrozen and frozenInIce (a HashSet<PhysicalBehaviour>, in slot order), as `physicals` indices.
        was_frozen: bool = false,
        frozen_in_ice: SlotSet<usize> = SlotSet::default(),
        /// currentlyInWater[0..currentlyInWaterCount), as `physicals` indices.
        currentlyInWater: Vec<usize> = Vec::new(),
    }
}

/// A FrozenInIceBehaviour a water added to a physical object.
#[derive(Clone, Debug)]
pub struct FrozenInIce {
    pub physical: usize,
    /// The SkinMaterialHandler on the same object (its limb) and the ShouldRot it had.
    pub skin: Option<usize>,
    pub shouldRotOriginal: bool,
    /// A pending Object.Destroy(component): the DelayedCallManager time it runs at.
    pub destroy_at: Option<f64>,
    pub destroyed: bool,
}

/// TagManager's custom tag "Poolable" (tags[8]).
const POOLABLE_TAG: u16 = 20008;

/// A Splash clone of an ObjectPoolBehaviour: gameObject.activeSelf and the root ParticleSystem's time.
#[derive(Clone, Debug)]
pub struct SplashItem {
    pub active: bool,
    /// The root ParticleSystem (lengthInSec 5, not looping, one burst at 0 whose particles and sub-emitter particles
    /// live at most 2 s) stops when its time reaches 5 s (stopAction Callback: PoolableParticleSystemHelper returns the
    /// clone). The clock is a double, as BloodPuff's.
    pub particle_time: f64,
    /// Destroyed by ObjectPoolBehaviour.Clear.
    pub destroyed: bool,
}

/// The ObjectPoolBehaviour PoolGenerator makes for a Splash prefab (once it is first requested), and what the prefab
/// holds that the simulation reads.
#[derive(Clone, Debug)]
pub struct SplashPool {
    pub source: ObjRef,
    pub prefab: Arc<Prefab>,
    pub original_ids: Arc<HashMap<ObjRef, i32>>,
    /// The prefab root's tag is Poolable.
    pub poolable: bool,
    /// ExplosionSoundBehviour.Play's `Near.PickRandom()` draw: Near.Length, when the behaviour is enabled and has an
    /// AudioSource.
    pub sound: Option<usize>,
    /// GetComponentsInChildren<ParticleSystem>().Count: each gets a Random.Range in DoSplash.
    pub particle_systems: usize,
    /// PoolGenerator.Ensure ran.
    pub ensured: bool,
    pub pool: ObjectPool,
    pub items: Vec<SplashItem>,
}

/// PoolGenerator.PoolSize
const POOL_SIZE: usize = 256;
/// ParticleSystem class id.
const PARTICLE_SYSTEM: i32 = 198;
/// AudioSource class id.
const AUDIO_SOURCE: i32 = 82;

impl Game {
    /// The pool of the Splash prefab `source`, read from the data the first time a water names it.
    pub fn splash_pool_for(&mut self, db: &mut AssetDb, source: ObjRef) -> Result<usize, String> {
        if let Some(i) = self.splash_pools.iter().position(|p| p.source == source) {
            return Ok(i);
        }
        let prefab = db.prefab(source)?;
        let root = &prefab.objects[0];
        let sound = root.components.iter().find_map(|(_, c)| match c {
            Component::Script { class, enabled, fields, .. } if class == "ExplosionSoundBehviour" => {
                let source_set = fields.get("audioSource").and_then(Value::pptr).is_some_and(|p| !p.is_null());
                let has_source = source_set || root.components.iter().any(|(_, c)| matches!(c, Component::Other(AUDIO_SOURCE)));
                // DistantSound (SoundType.None = 0) is checked in Play too.
                assert!(fields.get("DistantSound").and_then(Value::i64) == Some(0), "ExplosionSoundBehviour.DistantSound");
                (*enabled && has_source).then(|| fields.get("Near").and_then(Value::array).expect("Near").len())
            }
            _ => None,
        });
        let particle_systems = prefab.objects.iter().enumerate().filter(|(o, _)| self.prefab_object_active(&prefab, *o)).map(|(_, ob)| ob.components.iter().filter(|(_, c)| matches!(c, Component::Other(PARTICLE_SYSTEM))).count()).sum();
        self.splash_pools.push(SplashPool {
            source,
            poolable: root.tag == POOLABLE_TAG,
            sound,
            particle_systems,
            original_ids: Arc::new(db.resource_load_order()?),
            prefab: Arc::new(prefab),
            ensured: false,
            pool: ObjectPool::default(),
            items: Vec::new(),
        });
        Ok(self.splash_pools.len() - 1)
    }

    /// activeInHierarchy of a prefab object.
    fn prefab_object_active(&self, prefab: &Prefab, mut o: usize) -> bool {
        loop {
            if !prefab.objects[o].active {
                return false;
            }
            match prefab.objects[o].parent {
                Some(p) => o = p,
                None => return true,
            }
        }
    }

    /// PoolGenerator.RequestPrefab(Splash, position): ObjectPoolBehaviour.Request, returning the item.
    fn splash_request(&mut self, s: usize) -> Option<usize> {
        if !self.splash_pools[s].ensured {
            // Ensure: new GameObject(name + " pool").AddComponent<ObjectPoolBehaviour>() takes the GameObject's,
            // its Transform's and the behaviour's instance ids.
            for _ in 0..3 {
                self.allocate_instance_id();
            }
            self.splash_pools[s].ensured = true;
        }
        let pool = &mut self.splash_pools[s];
        pool.pool.inactivityTimer = 0.0;
        let first = pool.pool.available.iter().next().copied();
        if let Some(i) = first {
            pool.pool.available.remove_where(|&x| x == i);
            // SetActive(true) replays the particle systems (playOnAwake); OnPoolableReinitialised is
            // ExplosionSoundBehviour.Start (Play).
            pool.items[i].active = true;
            pool.items[i].particle_time = 0.0;
            let sound = pool.sound;
            if let Some(n) = sound {
                self.pick_random(n);
            }
            return Some(i);
        }
        if pool.pool.created.len() >= POOL_SIZE {
            return None;
        }
        // Instantiate(Prefab, position, identity); the position and rotation only place the particles and the sound.
        let (prefab, ids, sound) = (pool.prefab.clone(), pool.original_ids.clone(), pool.sound);
        self.instantiate_native(&prefab, [0.0; 3], [0.0, 0.0, 0.0, 1.0], &ids);
        let pool = &mut self.splash_pools[s];
        pool.items.push(SplashItem { active: true, particle_time: 0.0, destroyed: false });
        let i = pool.items.len() - 1;
        pool.pool.created.push(i);
        // Start of the clone's ExplosionSoundBehviour (Play).
        if let Some(n) = sound {
            self.pending_starts.push(PendingStart::ExplosionSound(n));
        }
        Some(i)
    }

    /// WaterBehaviour.DoSplash after its gates: the pooled Splash and the Random.Range(1f, 1.4f) of each of its
    /// ParticleSystems (their start size). The clone's position (Trigger.bounds.ClosestPoint), rotation, AudioSource
    /// volume and the particle modules are presentation only.
    fn water_splash(&mut self, w: usize) {
        let Some(s) = self.waters[w].splash_pool else { unimplemented!("WaterBehaviour.DoSplash with a null Splash (NullReferenceException)") };
        if !self.splash_pools[s].poolable {
            unimplemented!("WaterBehaviour.DoSplash: Object.Instantiate of a Splash prefab that is not tagged Poolable");
        }
        // Trigger.bounds starts with the AutoSyncTransforms.
        self.sync_pending();
        if self.splash_request(s).is_none() {
            return;
        }
        for _ in 0..self.splash_pools[s].particle_systems {
            self.random.range(1.0, 1.4);
        }
    }

    /// ObjectPoolBehaviour.Update of the Splash pools: Clear after 300 s with every object back.
    pub fn splash_pool_update(&mut self) {
        let dt = self.time.deltaTime;
        for pool in &mut self.splash_pools {
            if pool.pool.created.is_empty() {
                continue;
            }
            pool.pool.inactivityTimer += dt;
            if pool.pool.inactivityTimer > 300.0 && pool.pool.available.iter().count() == pool.pool.created.len() {
                for &i in &pool.pool.created {
                    pool.items[i].destroyed = true;
                }
                pool.pool.created.clear();
                pool.pool.available.clear();
                pool.pool.inactivityTimer = 0.0;
            }
        }
    }

    /// The particle systems' update after LateUpdate: a Splash whose root system ran its 5 s stops, and
    /// PoolableParticleSystemHelper.OnParticleSystemStopped returns it to the pool.
    pub fn splash_particles_update(&mut self) {
        let dt = self.time.deltaTime;
        for pool in &mut self.splash_pools {
            for i in 0..pool.items.len() {
                let item = &mut pool.items[i];
                if !item.active || item.destroyed {
                    continue;
                }
                item.particle_time += dt as f64;
                if item.particle_time >= 5.0 {
                    item.active = false;
                    pool.pool.available.add(i);
                }
            }
        }
    }

    /// WaterBehaviour.IsEvaporated
    fn water_is_evaporated(&self, w: usize) -> bool {
        self.AmbientTemperature > (d(self.waters[w].BoilingPointCelsius) + 100.0).f()
    }

    /// WaterBehaviour.IsFrozen
    fn water_is_frozen(&self, w: usize) -> bool {
        self.AmbientTemperature < self.waters[w].FreezingPointCelsius
    }

    /// WaterBehaviour.GetGlobalSurfaceLevel: transform.TransformPoint(0, LocalSurfaceLevel).y
    fn water_global_surface_level(&self, w: usize) -> f32 {
        let mut v = [0.0, self.waters[w].LocalSurfaceLevel, 0.0, 0.0];
        for node in &self.waters[w].chain {
            v = global_position(node, v);
        }
        v[1]
    }

    /// WaterBehaviour.IsPointInsideWater: Trigger.OverlapPoint (Collider2D::OverlapPoint, 180c12440, starts with
    /// the AutoSyncTransforms and asks each of the collider's shapes).
    fn water_is_point_inside(&mut self, w: usize, point: [f32; 2]) -> bool {
        self.sync_pending();
        let p = V::new(point[0], point[1]);
        self.waters[w].trigger_fixtures.iter().map(|&f| self.world.fixture(f)).any(|fx| super::tool::test_point(&fx.shape, &self.world.body(fx.body).xf, p))
    }

    /// WaterBehaviour.IsPointUnderWater
    pub fn is_point_under_water(&mut self, point: [f32; 2]) -> bool {
        for i in 0..self.water_list.len() {
            let w = self.water_list[i];
            if self.waters[w].active && !self.water_is_evaporated(w) && !self.water_is_frozen(w) && self.water_is_point_inside(w, point) {
                return true;
            }
        }
        false
    }

    /// WaterBehaviour.Awake: waters.Add(this). (OnDestroy's Remove: no water is ever destroyed.)
    pub fn water_awake(&mut self, w: usize) {
        self.water_list.push(w);
    }

    /// WaterBehaviour.Start: SpriteRenderer = GetComponent (an engine reference), waterSizzles (loaded at link) and
    /// UpdateTemperatureState.
    pub fn water_start(&mut self, w: usize) {
        self.water_update_temperature_state(w);
    }

    /// Runs `f` on the clone the water belongs to (still unsynced, or retained by the sync).
    fn with_water_instance<R>(&mut self, w: usize, f: impl FnOnce(&mut Game, &mut super::spawn::Instance) -> R) -> R {
        let (o, c, id) = (self.waters[w].object, self.waters[w].component, self.waters[w].id);
        let owns = |i: &super::spawn::Instance| i.objects.get(o).and_then(|x| x.components.get(c)) == Some(&id);
        if let Some(i) = self.unsynced.iter().position(owns) {
            let mut inst = self.unsynced.remove(i);
            let r = f(self, &mut inst);
            self.unsynced.insert(i, inst);
            return r;
        }
        let i = self.retained.iter().position(owns).expect("the clone of a WaterBehaviour");
        let mut inst = self.retained.remove(i);
        let r = f(self, &mut inst);
        self.retained.insert(i, inst);
        r
    }

    /// Runs `f` on the clone that holds the component `id` (still unsynced, or retained by the sync), with the
    /// index of its object.
    pub(super) fn with_component_instance<R>(&mut self, id: i32, f: impl FnOnce(&mut Game, &mut super::spawn::Instance, usize) -> R) -> R {
        let owns = |i: &super::spawn::Instance| i.objects.iter().any(|x| x.components.contains(&id));
        let (list, i) = match self.unsynced.iter().position(owns) {
            Some(i) => (true, i),
            None => (false, self.retained.iter().position(owns).unwrap_or_else(|| unimplemented!("component {id}: no clone kept for it"))),
        };
        let mut inst = if list { self.unsynced.remove(i) } else { self.retained.remove(i) };
        let o = inst.objects.iter().position(|x| x.components.contains(&id)).unwrap();
        let r = f(self, &mut inst, o);
        if list { self.unsynced.insert(i, inst) } else { self.retained.insert(i, inst) }
        r
    }

    /// GameObject.SetActive on one of the water's prefab objects. SetActive(true) of an active object does nothing.
    fn water_set_active(&mut self, w: usize, o: usize, active: bool) {
        self.with_water_instance(w, |g, inst| g.set_active_scripts(inst, o, active));
    }

    /// GameObject.SetActive on object `o` of a clone, then the OnEnable or OnDisable of the scripts it passes.
    pub(super) fn set_active_scripts(&mut self, inst: &mut super::spawn::Instance, o: usize, active: bool) {
        let prefab = inst.prefab.clone();
        if active {
            for (d, k) in self.set_active(inst, o, true) {
                let Component::Script { class, .. } = &prefab.objects[d].components[k].1 else { unreachable!() };
                let id = inst.objects[d].components[k];
                match class.as_str() {
                    "PhysicalBehaviour" => {
                        let p = self.physicals.iter().position(|p| p.id == id).expect("a PhysicalBehaviour in the arena");
                        self.physicals[p].active = true;
                    }
                    // No OnEnable.
                    "AudioSourceTimeScaleBehaviour" | "WaterSoundBehaviour" | "WaterImpactBehaviour" | "PoliceBoxBehaviour" | "ExistByChance" => {}
                    _ => unimplemented!("{class}.OnEnable"),
                }
            }
            return;
        }
        for (d, k) in self.set_active(inst, o, false) {
            let Component::Script { class, enabled, .. } = &prefab.objects[d].components[k].1 else { unreachable!() };
            let id = inst.objects[d].components[k];
            match class.as_str() {
                "PhysicalBehaviour" => {
                    let p = self.physicals.iter().position(|p| p.id == id).expect("a PhysicalBehaviour in the arena");
                    self.physicals[p].active = false;
                    if *enabled {
                        self.physical_on_disable(p);
                    }
                }
                // No OnDisable.
                "AudioSourceTimeScaleBehaviour" | "WaterSoundBehaviour" | "WaterImpactBehaviour" | "PoliceBoxBehaviour" | "ExistByChance" => {}
                _ => unimplemented!("{class}.OnDisable"),
            }
        }
    }

    /// WaterBehaviour.UpdateObjectsToBeActiveIfWater: the objects, then SpriteRenderer.enabled and
    /// SpriteMask.enabled (rendering).
    fn water_update_objects_to_be_active(&mut self, w: usize, active: bool) {
        for i in 0..self.waters[w].objects_to_be_active.len() {
            let o = self.waters[w].objects_to_be_active[i];
            self.water_set_active(w, o, active);
        }
    }

    /// FreezeStackController.RequestFreeze(a): the first request makes the Rigidbody2D static.
    fn freeze_request(&mut self, p: usize) {
        if self.physicals[p].destroyed {
            return;
        }
        if let Some(n) = self.freeze_stack.get_mut(&p) {
            *n += 1;
            return;
        }
        self.freeze_stack.insert(p, 1);
        self.rigidbody_set_body_type(p, 2);
    }

    /// FreezeStackController.RequestUnfreeze(a): the last release makes the Rigidbody2D dynamic again.
    fn freeze_request_unfreeze(&mut self, p: usize) {
        if self.physicals[p].destroyed {
            return;
        }
        let Some(n) = self.freeze_stack.get_mut(&p) else { return };
        *n -= 1;
        if *n <= 0 {
            self.freeze_stack.remove(&p);
            self.rigidbody_set_body_type(p, 0);
        }
    }

    /// Rigidbody2D.bodyType = t (0 Dynamic, 2 Static).
    fn rigidbody_set_body_type(&mut self, p: usize, body_type: i32) {
        let rb = &mut self.physicals[p].rigidbody;
        if rb.detached.is_some() {
            rb.bodyType = body_type;
            return;
        }
        let component = rb.component;
        if component == 0 {
            unimplemented!("{}: Rigidbody2D.bodyType of a recorded body (no clone)", self.physicals[p].name);
        }
        self.with_component_instance(component, |g, inst, o| g.rigidbody2d_set_body_type(inst, BodyOwner::Physical(p), o, body_type));
    }

    /// physical.gameObject.AddComponent<FrozenInIceBehaviour>(): its Awake takes the SkinMaterialHandler's ShouldRot.
    fn frozen_in_ice_add(&mut self, p: usize) {
        self.allocate_instance_id();
        let skin = self.limbs.iter().position(|l| l.PhysicalBehaviour == p);
        let mut f = FrozenInIce { physical: p, skin, shouldRotOriginal: false, destroy_at: None, destroyed: false };
        if let Some(l) = skin {
            f.shouldRotOriginal = self.skins[l].ShouldRot;
            self.skins[l].ShouldRot = false;
        }
        self.frozen_in_ice.push(f);
    }

    /// `item.TryGetComponent<FrozenInIceBehaviour>(out c)` then Object.Destroy(c) and RequestUnfreeze(item.rigidbody).
    fn frozen_in_ice_release(&mut self, p: usize) {
        if let Some(i) = self.frozen_in_ice.iter().position(|f| f.physical == p && !f.destroyed) {
            // DestroyObjectDelayed: queued at min(fixedTimeAsDouble, the frame's time); a second Destroy finds it queued.
            let at = self.time.fixedTimeAsDouble.min(self.time.timeAsDouble);
            self.frozen_in_ice[i].destroy_at.get_or_insert(at);
            self.freeze_request_unfreeze(p);
        }
    }

    /// The FrozenInIceBehaviours' queued destroys due by now, as (time, index).
    pub fn frozen_in_ice_due_destroys(&self, now: f64) -> Vec<(f64, usize)> {
        (0..self.frozen_in_ice.len()).filter_map(|i| self.frozen_in_ice[i].destroy_at.filter(|&at| at <= now).map(|at| (at, i))).collect()
    }

    /// FrozenInIceBehaviour.OnDestroy: the skin gets its ShouldRot back.
    pub fn frozen_in_ice_run_destroy(&mut self, i: usize) {
        let f = &mut self.frozen_in_ice[i];
        f.destroy_at = None;
        f.destroyed = true;
        let (p, skin, rot) = (f.physical, f.skin, f.shouldRotOriginal);
        if let Some(l) = skin.filter(|_| !self.physicals[p].destroyed) {
            self.skins[l].ShouldRot = rot;
        }
    }

    /// WaterBehaviour.SetFrozen
    fn water_set_frozen(&mut self, w: usize) {
        let sound = self.waters[w].boiling_sound;
        self.water_set_active(w, sound, false);
        self.waters[w].was_frozen = true;
        // LayerMask.GetMask("Bounds") (layer 11); Ignore is empty.
        for p in 0..self.physicals.len() {
            let ph = &self.physicals[p];
            if !ph.in_world || ph.destroyed {
                continue;
            }
            let go = self.world.body(ph.rigidbody.body).user.expect("a physical's Rigidbody2D").game_object;
            if self.world.layers[&go] == 11 || self.waters[w].frozen_in_ice.iter().any(|&x| x == p) {
                continue;
            }
            let grid = ph.LocalColliderGridPoints.clone().unwrap_or_else(|| unimplemented!("NullReferenceException: LocalColliderGridPoints of {}", ph.name));
            let mut num = 0;
            for local in &grid {
                let point = self.transform_point3(p, [local[0], local[1], 0.0]);
                if self.water_is_point_inside(w, [point[0], point[1]]) {
                    num += 1;
                    if num as f32 / grid.len() as f32 >= 0.05 {
                        let rb = self.physicals[p].rigidbody.clone();
                        rb.set_velocity(&mut self.world, [0.0, 0.0]);
                        rb.set_angular_velocity(&mut self.world, 0.0);
                        self.freeze_request(p);
                        self.waters[w].frozen_in_ice.add(p);
                        self.frozen_in_ice_add(p);
                        let ph = &mut self.physicals[p];
                        ph.Temperature = mathf::min(ph.Temperature, self.AmbientTemperature);
                        break;
                    }
                }
            }
        }
        self.water_update_objects_to_be_active(w, false);
        // SpriteRenderer.enabled = true (rendering).
        let ice = self.waters[w].ice_object;
        self.water_set_active(w, ice, true);
    }

    /// WaterBehaviour.UpdateTemperatureState
    pub fn water_update_temperature_state(&mut self, w: usize) {
        let ice = self.waters[w].ice_object;
        self.water_set_active(w, ice, false);
        if self.water_is_frozen(w) {
            self.water_set_frozen(w);
            return;
        }
        if self.waters[w].was_frozen {
            let items: Vec<usize> = self.waters[w].frozen_in_ice.iter().copied().collect();
            for p in items {
                if !self.physicals[p].destroyed {
                    self.frozen_in_ice_release(p);
                }
            }
        }
        self.waters[w].was_frozen = false;
        self.waters[w].frozen_in_ice.clear();
        let sound = self.waters[w].boiling_sound;
        if self.water_is_evaporated(w) {
            // SetEvaporated
            self.water_set_active(w, sound, false);
            self.water_update_objects_to_be_active(w, false);
            return;
        }
        let boiling = self.AmbientTemperature >= self.waters[w].BoilingPointCelsius;
        self.water_set_active(w, sound, boiling);
        self.water_update_objects_to_be_active(w, true);
    }

    /// WaterBehaviour.OnTriggerEnter2D / OnTriggerExit2D: DoSplash unless evaporated or frozen.
    pub fn water_on_trigger(&mut self, w: usize, collider: i32) {
        if self.water_is_evaporated(w) || self.water_is_frozen(w) {
            return;
        }
        // DoSplash. Ignore is empty (IgnoreObject has no caller).
        if !self.prefs.FancyEffects {
            return;
        }
        // collision.isTrigger, then PhysicalObjectsInWorldByTransform.TryGetValue(collision.transform).
        let Some(p) = self.physical_of_collider(collider) else { return };
        let ph = &self.physicals[p];
        let k = ph.colliders.iter().position(|&c| c == collider).expect("a collider of the physical");
        if ph.collider_defs[k].is_trigger || !ph.in_world {
            return;
        }
        if ph.rigidbody.velocity(&self.world)[1].abs() < 0.75 {
            return;
        }
        self.water_splash(w);
    }

    /// WaterBehaviour.GetDragFactor
    fn water_drag_factor(&self, w: usize, velocity: [f32; 2]) -> f32 {
        let num = (d(mathf::sqr_magnitude(velocity)) * 0.2).f();
        mathf::clamp(mathf::pow(num + 1.0, num * self.waters[w].Drag), 0.6, 1.0)
    }

    /// The person whose AliveBehaviour.AliveByTransform entry is this physical's transform.root.
    fn person_of_root(&self, p: usize) -> Option<usize> {
        let root = self.root_of(p);
        if !root.0 {
            return None;
        }
        (0..self.people.len()).find(|&i| !self.people[i].destroyed && self.people[i].Limbs.first().is_some_and(|&l| self.root_of(self.limbs[l].PhysicalBehaviour) == root))
    }

    /// WaterBehaviour.ManagedFixedUpdate (not frozen: SetFrozen is unimplemented, so Ice.Temperature and the
    /// frozenInIce loop never run).
    fn water_fixed_update(&mut self, w: usize) {
        self.waters[w].currentlyInWater.clear();
        if self.water_is_frozen(w) {
            let ice = self.waters[w].ice;
            self.physicals[ice].Temperature = self.AmbientTemperature;
            let even = self.time.frameCount % 2 == 0;
            let items: Vec<usize> = self.waters[w].frozen_in_ice.iter().copied().collect();
            for p in items {
                if self.physicals[p].destroyed {
                    continue;
                }
                if self.physicals[p].OnFire {
                    self.extinguish(p);
                }
                self.physicals[p].Wetness = 1.0;
                if !even {
                    continue;
                }
                let grid = self.physicals[p].LocalColliderGridPoints.clone().unwrap_or_else(|| unimplemented!("NullReferenceException: LocalColliderGridPoints"));
                let mut inside = false;
                for local in &grid {
                    let point = self.transform_point3(p, [local[0], local[1], 0.0]);
                    if self.water_is_point_inside(w, [point[0], point[1]]) {
                        inside = true;
                        break;
                    }
                }
                if !inside {
                    self.frozen_in_ice_release(p);
                }
            }
            return;
        }
        if self.water_is_evaporated(w) {
            return;
        }
        let buoyancy = self.waters[w].Buoyancy;
        // Vector3.up * Buoyancy
        let up = [0.0 * buoyancy, 1.0 * buoyancy];
        let level = self.water_global_surface_level(w);
        let num = mathf::min(self.AmbientTemperature, self.waters[w].BoilingPointCelsius);
        let boiling = self.AmbientTemperature >= self.waters[w].BoilingPointCelsius;
        for p in 0..self.physicals.len() {
            if !self.physicals[p].in_world || self.physicals[p].destroyed {
                continue;
            }
            let ph = &self.physicals[p];
            let num2 = mathf::clamp(mathf::pow((ph.transform.position[1] - level).abs(), 0.25), 1.0, 8.0);
            let num3 = mathf::clamp01(((d(num2) - 1.0) / 7.0).f()) + 1.0;
            let rb = ph.rigidbody.clone();
            let mass = rb.mass(&self.world);
            let grid = ph.LocalColliderGridPoints.clone().unwrap_or_default();
            let props = &self.properties[ph.Properties];
            // mass / (float)LocalGridPointLength * num2 * (Buoyancy * BuoyancyModifier) * Vector3
            let a = (d(mass) / grid.len() as f32 * num2 * (d(props.Buoyancy) * ph.BuoyancyModifier)).f();
            let force = [a * up[0], a * up[1]];
            self.physicals[p].CurrentWaterSurfaceLevel = level;
            let mut inside = false;
            for local in &grid {
                let point = self.transform_point3(p, [local[0], local[1], 0.0]);
                let point = [point[0], point[1]];
                if self.water_is_point_inside(w, point) {
                    inside = true;
                    if !boiling {
                        rb.add_force_at_position(&mut self.world, force, point);
                    }
                    // GetPointVelocity starts with the AutoSyncTransforms.
                    self.sync_pending();
                    let v = rb.point_velocity(&self.world, point);
                    let k = (d(0.4f32) * num3 * self.water_drag_factor(w, v) * mass).f();
                    rb.add_force_at_position(&mut self.world, [k * -v[0], k * -v[1]], point);
                }
            }
            let was_dry = !self.physicals[p].IsUnderWater;
            if !inside {
                continue;
            }
            self.physicals[p].underwaterMarkings += 1;
            self.waters[w].currentlyInWater.push(p);
            if boiling {
                if let Some(person) = self.person_of_root(p) {
                    self.physicals[p].BurnProgress = mathf::max(self.physicals[p].BurnProgress, 0.5);
                    // SendMessage("AddPain", 15f)
                    self.add_pain(person, 15.0);
                }
            }
            let ph = &mut self.physicals[p];
            let transfer = ph.Temperature;
            ph.Temperature = mathf::lerp(transfer, num, 0.05 * self.properties[ph.Properties].HeatTransferSpeedMultiplier);
            if was_dry && self.physicals[p].Temperature >= 100.0 && self.physicals[p].Temperature > num * 2.0 {
                // PlayClipOnce(waterSizzles.PickRandom()): audio, the draw stays.
                self.pick_random(self.waters[w].waterSizzles);
                self.sizzle(p, false);
            }
            self.physicals[p].Wetness += 0.1;
            // SendMessage("WaterImpact", rigidbody.velocity.magnitude): LimbBehaviour's, on an active object.
            let speed = mathf::magnitude2(rb.velocity(&self.world));
            if let Some(l) = self.limbs.iter().position(|l| l.PhysicalBehaviour == p) {
                if self.limb_active(l) {
                    self.limb_water_impact(l, speed);
                }
            }
        }
    }

    /// WaterBehaviourManager.FixedUpdate: every physical's underwaterMarkings cleared, each water that ShouldUpdate
    /// (active in the hierarchy and enabled) marks the ones in it, then IsUnderWater follows the markings.
    pub fn water_manager_fixed_update(&mut self) {
        let listed = |ph: &super::Physical| ph.in_world && !ph.destroyed;
        for ph in self.physicals.iter_mut().filter(|ph| listed(ph)) {
            ph.underwaterMarkings = 0;
        }
        for i in 0..self.water_list.len() {
            let w = self.water_list[i];
            if self.waters[w].active && self.waters[w].enabled {
                self.water_fixed_update(w);
            }
        }
        for ph in self.physicals.iter_mut().filter(|ph| listed(ph)) {
            ph.IsUnderWater = ph.underwaterMarkings > 0;
        }
    }

    /// LimbBehaviour.WaterImpact
    pub fn limb_water_impact(&mut self, l: usize, magnitude: f32) {
        if self.limbs[l].ImmuneToDamage {
            return;
        }
        let magnitude = magnitude * self.prefs.FragilityMultiplier;
        if magnitude > 30.0 {
            self.limb_damage(l, magnitude * 0.09);
            if !self.limbs[l].IsAndroid {
                let v = mathf::min((d(self.skins[l].acidProgress) + d(magnitude) * 0.05).f(), 0.6);
                self.skins[l].set_acid(v);
            }
        }
    }
}
