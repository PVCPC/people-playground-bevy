//! WaterBehaviour and WaterBehaviourManager: the water volumes (their trigger collider, buoyancy and drag on
//! every physical object, the temperature states) and the IsUnderWater flag they keep.
use super::{d, mathf, script, Game};
use crate::assets::db::Component;
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
        /// currentlyInWater[0..currentlyInWaterCount), as `physicals` indices.
        currentlyInWater: Vec<usize> = Vec::new(),
    }
}

impl Game {
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

    /// GameObject.SetActive on one of the water's prefab objects. SetActive(true) of an active object does nothing.
    fn water_set_active(&mut self, w: usize, o: usize, active: bool) {
        self.with_water_instance(w, |g, inst| {
            if active {
                if !inst.objects[o].active {
                    g.set_active(inst, o, true);
                }
                return;
            }
            let prefab = inst.prefab.clone();
            for (d, k) in g.set_active(inst, o, false) {
                let Component::Script { class, enabled, .. } = &prefab.objects[d].components[k].1 else { unreachable!() };
                let id = inst.objects[d].components[k];
                match class.as_str() {
                    "PhysicalBehaviour" => {
                        let p = g.physicals.iter().position(|p| p.id == id).expect("a PhysicalBehaviour in the arena");
                        g.physicals[p].active = false;
                        if *enabled {
                            g.physical_on_disable(p);
                        }
                    }
                    // No OnDisable.
                    "AudioSourceTimeScaleBehaviour" | "WaterSoundBehaviour" | "WaterImpactBehaviour" => {}
                    _ => unimplemented!("{class}.OnDisable"),
                }
            }
        });
    }

    /// WaterBehaviour.UpdateObjectsToBeActiveIfWater: the objects, then SpriteRenderer.enabled and
    /// SpriteMask.enabled (rendering).
    fn water_update_objects_to_be_active(&mut self, w: usize, active: bool) {
        for i in 0..self.waters[w].objects_to_be_active.len() {
            let o = self.waters[w].objects_to_be_active[i];
            self.water_set_active(w, o, active);
        }
    }

    /// WaterBehaviour.UpdateTemperatureState. SetFrozen is the only thing that sets wasFrozen and fills
    /// frozenInIce, so the unfreezing loop has nothing to do.
    pub fn water_update_temperature_state(&mut self, w: usize) {
        let ice = self.waters[w].ice_object;
        self.water_set_active(w, ice, false);
        if self.water_is_frozen(w) {
            // BoilingWaterSound off, then each physical in the water gets FreezeStackController.RequestFreeze and a
            // FrozenInIceBehaviour, the objects go off and Ice.gameObject.SetActive(true).
            unimplemented!("WaterBehaviour.SetFrozen (FreezeStackController, FrozenInIceBehaviour, GameObject.SetActive(true))");
        }
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
        // Splash.CompareTag("Poolable") ? PoolGenerator.Instance.RequestPrefab(Splash, ...) : Object.Instantiate(...),
        // then the clone's rotation, AudioSource volume and each ParticleSystem's shape radius and start size
        // (Random.Range(1f, 1.4f)) and speed.
        unimplemented!("WaterBehaviour.DoSplash: the Splash prefab (PoolGenerator.RequestPrefab / Object.Instantiate, ParticleSystem modules)");
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
            unimplemented!("WaterBehaviour.ManagedFixedUpdate while frozen (Ice.Temperature, frozenInIce)");
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
