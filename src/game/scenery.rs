//! The Hybrid map's scenery scripts: LavaBehaviour, RandomPrefabSpawnerBehaviour, TemperatureTargetRectangleBehaviour
//! and ExistByChance.
#![allow(non_snake_case)]

use super::misc::PendingStart;
use super::{d, mathf, script, Game};
use crate::assets::db::ObjRef;
use crate::b2::{math::Vec2 as V, world::FixtureId};
use crate::unity::transform::{global_position, Trs};

/// `Dictionary<PhysicalBehaviour, RefBool>` as Mono's mscorlib keeps it: entries in slot order, a removed slot goes on
/// the free list and the next Add takes the latest one (nothing resets when it empties). Value: Marked.
#[derive(Clone, Debug, Default)]
pub struct Victims {
    entries: Vec<Option<(usize, bool)>>,
    free: Vec<usize>,
}

impl Victims {
    fn find(&self, p: usize) -> Option<usize> {
        self.entries.iter().position(|e| matches!(e, Some((k, _)) if *k == p))
    }

    fn add(&mut self, p: usize) {
        match self.free.pop() {
            Some(i) => self.entries[i] = Some((p, true)),
            None => self.entries.push(Some((p, true))),
        }
    }

    fn remove(&mut self, p: usize) {
        if let Some(i) = self.find(p) {
            self.entries[i] = None;
            self.free.push(i);
        }
    }
}

script! {
    Lava {
        LavaTemperature: f32 = 975.0,
        TransferRate: f32 = 0.01,
        Dampening: f32 = 0.05,
        Buoyancy: f32 = 0.05,
    }
    links {
        /// The Trigger collider's fixtures.
        trigger_fixtures: Vec<FixtureId> = Vec::new(),
        /// ContactFilter.layerMask (all layers without useLayerMask).
        layer_mask: u32 = !0,
        /// ImpactPrefab as a `Game::splash_pools` entry (its clone count and sound); None when null.
        impact: Option<usize> = None,
        lastImpactSplashTime: f32 = 0.0,
        previousVictims: Victims = Victims::default(),
        /// The shouldRemove stack.
        shouldRemove: Vec<usize> = Vec::new(),
    }
}

script! {
    Spawner {
        SpawnChancePerPeriod: f32 = 1.0,
        PeriodInSeconds: f32 = 1.0,
        SpawnShape: i32 = 0,
        Offset: [f32; 2] = [0.0; 2],
        LocalFrom: [f32; 2] = [0.0; 2],
        LocalTo: [f32; 2] = [0.0; 2],
    }
    links {
        /// Prefabs (None for a null entry).
        prefabs: Vec<Option<ObjRef>> = Vec::new(),
        /// The object's own Transform, then each parent's (the spawner never moves).
        chain: Vec<Trs> = Vec::new(),
        t: f32 = 0.0,
    }
}

script! {
    HeatRect {
        TargetAmbientTemperature: f32 = 975.0,
        TransferRateMultiplier: f32 = 1.0,
    }
    links {
        /// Size and Offset (Vector3Int).
        Size: [i32; 3] = [0; 3],
        Offset: [i32; 3] = [0; 3],
        /// transform.position (the rectangle never moves).
        position: [f32; 3] = [0.0; 3],
    }
}

impl Game {
    /// Trigger.OverlapPoint (Collider2D::OverlapPoint starts with the AutoSyncTransforms).
    pub(super) fn lava_is_point_inside(&mut self, lv: usize, point: [f32; 2]) -> bool {
        self.sync_pending();
        let p = V::new(point[0], point[1]);
        self.lavas[lv].trigger_fixtures.iter().map(|&f| self.world.fixture(f)).any(|fx| super::tool::test_point(&fx.shape, &self.world.body(fx.body).xf, p))
    }

    /// LavaBehaviour.DoSplash: the gate and its Random.value, then Instantiate(ImpactPrefab, Trigger.ClosestPoint(point)).
    /// The clone's position and its ParticleSystem's shape radius are presentation; its ExplosionSoundBehviour.Start
    /// draws the sound (as the water's Splash).
    fn lava_do_splash(&mut self, lv: usize) {
        let time = self.time.time;
        if time - self.lavas[lv].lastImpactSplashTime > 0.25 || self.random.value() as f64 > 0.9 {
            self.lavas[lv].lastImpactSplashTime = time;
            let Some(s) = self.lavas[lv].impact else { unimplemented!("LavaBehaviour.DoSplash with a null ImpactPrefab (ArgumentException)") };
            let pool = &self.splash_pools[s];
            let (prefab, ids, sound) = (pool.prefab.clone(), pool.original_ids.clone(), pool.sound);
            self.instantiate_native(&prefab, [0.0; 3], [0.0, 0.0, 0.0, 1.0], &ids);
            if let Some(n) = sound {
                self.pending_starts.push(PendingStart::ExplosionSound(n));
            }
        }
    }

    /// LavaBehaviour.FixedUpdate
    pub fn lava_fixed_update(&mut self, lv: usize) {
        let (fixtures, mask) = (self.lavas[lv].trigger_fixtures.clone(), self.lavas[lv].layer_mask);
        let hits = self.overlap_collider(&fixtures, mask);
        for e in self.lavas[lv].previousVictims.entries.iter_mut().flatten() {
            e.1 = false;
        }
        for (physical, _) in hits {
            // PhysicalObjectsInWorldByTransform.TryGetValue(collider.transform) and rigidbody.bodyType.
            let Some(p) = physical else { continue };
            if self.physicals[p].rigidbody.bodyType != 0 {
                continue;
            }
            let victims = &mut self.lavas[lv].previousVictims;
            match victims.find(p) {
                Some(i) => victims.entries[i].as_mut().unwrap().1 = true,
                None => victims.add(p),
            }
            let grid = self.physicals[p].LocalColliderGridPoints.clone().unwrap_or_else(|| unimplemented!("NullReferenceException: LocalColliderGridPoints"));
            let length = grid.len() as i32;
            let mut num2 = length;
            let rb = self.physicals[p].rigidbody.clone();
            for local in &grid {
                let point = self.transform_point3(p, [local[0], local[1], 0.0]);
                let point = [point[0], point[1]];
                if !self.lava_is_point_inside(lv, point) {
                    continue;
                }
                // GetPointVelocity starts with the AutoSyncTransforms.
                self.sync_pending();
                let v = rb.point_velocity(&self.world, point);
                let mass = rb.mass(&self.world);
                let k = self.lavas[lv].Dampening * mathf::clamp01(mass);
                rb.add_force_at_position(&mut self.world, [k * -v[0], k * -v[1]], point);
                let num3 = 0.995 * mathf::clamp(mathf::pow(mass, 0.1), 0.8, 1.0);
                let velocity = rb.velocity(&self.world);
                rb.set_velocity(&mut self.world, [velocity[0] * num3, velocity[1] * num3]);
                let angular = rb.angular_velocity(&self.world);
                rb.set_angular_velocity(&mut self.world, angular * num3);
                if self.lavas[lv].Buoyancy > mathf::EPSILON {
                    // -1f * Buoyancy * mass * Physics2D.gravity / LocalGridPointLength
                    let a = (d(-1.0) * self.lavas[lv].Buoyancy * mass).f();
                    let g = self.world.gravity;
                    let n = length as f32;
                    rb.add_force_at_position(&mut self.world, [a * g.x / n, a * g.y / n], point);
                }
                num2 -= 1;
            }
            if num2 == 0 {
                let velocity = rb.velocity(&self.world);
                rb.set_velocity(&mut self.world, [velocity[0] * 0.95, velocity[1] * 0.95]);
                let angular = rb.angular_velocity(&self.world);
                rb.set_angular_velocity(&mut self.world, angular * 0.95);
            }
            if length == 0 {
                unimplemented!("DivideByZeroException: LocalGridPointLength is 0");
            }
            // 1f - (float)(num2 / LocalGridPointLength): the division is an int one.
            let num4 = 1.0 - (num2 / length) as f32;
            let was = self.physicals[p].IsInLava;
            self.physicals[p].IsInLava = true;
            if !was {
                self.lava_do_splash(lv);
            }
            if self.physicals[p].SimulateTemperature {
                self.ignite(p, false);
                let t = (d(self.lavas[lv].TransferRate) / self.heat_capacity(p) * num4).f();
                let temperature = self.physicals[p].Temperature;
                self.physicals[p].Temperature = mathf::lerp(temperature, self.lavas[lv].LavaTemperature, t);
            }
        }
        for i in 0..self.lavas[lv].previousVictims.entries.len() {
            let Some((p, marked)) = self.lavas[lv].previousVictims.entries[i] else { continue };
            if marked {
                continue;
            }
            // A destroyed key is only removed.
            if !self.physicals[p].destroyed {
                self.physicals[p].IsInLava = false;
                self.lava_do_splash(lv);
            }
            self.lavas[lv].shouldRemove.push(p);
        }
        while let Some(p) = self.lavas[lv].shouldRemove.pop() {
            self.lavas[lv].previousVictims.remove(p);
        }
    }

    /// RandomPrefabSpawnerBehaviour.Update
    pub fn spawner_update(&mut self, s: usize) {
        self.spawners[s].t += self.time.deltaTime;
        if !(self.spawners[s].t > self.spawners[s].PeriodInSeconds) {
            return;
        }
        self.spawners[s].t = 0.0;
        if !(self.random.value() <= self.spawners[s].SpawnChancePerPeriod) {
            return;
        }
        let sp = &self.spawners[s];
        let transform_point = |p: [f32; 2]| {
            let mut v = [p[0], p[1], 0.0, 0.0];
            for node in &sp.chain {
                v = global_position(node, v);
            }
            [v[0], v[1], v[2]]
        };
        let (from, to) = match sp.SpawnShape {
            0 => (transform_point(sp.Offset), None),
            1 => (transform_point([sp.Offset[0] + sp.LocalFrom[0], sp.Offset[1] + sp.LocalFrom[1]]), Some(transform_point([sp.Offset[0] + sp.LocalTo[0], sp.Offset[1] + sp.LocalTo[1]]))),
            n => unimplemented!("RandomPrefabSpawnerBehaviour.SpawnShape {n}"),
        };
        // Vector3.Lerp(from, to, Random.value)
        let position = match to {
            None => from,
            Some(to) => {
                let t = self.random.value();
                [mathf::lerp(from[0], to[0], t), mathf::lerp(from[1], to[1], t), mathf::lerp(from[2], to[2], t)]
            }
        };
        self.spawner_instantiate(s, position);
    }

    /// Object.Instantiate(Prefabs.PickRandom(), position, Quaternion.identity).
    fn spawner_instantiate(&mut self, s: usize, position: [f32; 3]) {
        let Some(i) = self.pick_random(self.spawners[s].prefabs.len()) else { unimplemented!("RandomPrefabSpawnerBehaviour with no Prefabs (ArgumentException from Instantiate(null))") };
        let source = self.spawners[s].prefabs[i].unwrap_or_else(|| unimplemented!("RandomPrefabSpawnerBehaviour: a null Prefabs entry (ArgumentException from Instantiate(null))"));
        let mut db = self.assets.take().expect("RandomPrefabSpawnerBehaviour.Update without the game's data");
        let spawned = db.prefab(source).and_then(|prefab| self.spawn(&mut db, &prefab, position, [0.0, 0.0, 0.0, 1.0], None));
        self.assets = Some(db);
        match spawned {
            Ok((inst, _)) => self.unsynced.push(inst),
            Err(e) => panic!("RandomPrefabSpawnerBehaviour: Instantiate: {e}"),
        }
    }

    /// TemperatureTargetRectangleBehaviour.FixedUpdate (Start only caches AmbientTemperatureGridBehaviour.Instance).
    pub fn heat_rect_fixed_update(&mut self, h: usize) {
        if !self.prefs.AmbientTemperatureTransfer {
            return;
        }
        let r = &self.heat_rects[h];
        let (size, target, rate) = (r.Size, r.TargetAmbientTemperature, r.TransferRateMultiplier);
        let num = size[0] as f32 / 2.0;
        let num2 = size[1] as f32 / 2.0;
        let vector = [r.position[0] + r.Offset[0] as f32, r.position[1] + r.Offset[1] as f32];
        let grid_size = self.ambient_grid.GridSize;
        let g = grid_size as f32;
        for i in 0..size[0] {
            for j in 0..size[1] {
                let num3 = (d(vector[0]) + i as f32 - num).f();
                let num4 = (d(vector[1]) + j as f32 - num2).f();
                let key = (super::temperature::round_to_int(num3 / g) * grid_size, super::temperature::round_to_int(num4 / g) * grid_size);
                let cell = match self.ambient_grid.World.find(key) {
                    Some(c) => c,
                    None => self.ambient_grid.World.try_add(key, super::temperature::Cell::new(self.AmbientTemperature)),
                };
                let t = self.ambient_grid.ObjectToAirTransferRate * rate;
                let value = self.ambient_grid.World.cell_mut(cell);
                let lerped = mathf::lerp(value.Get(), target, t);
                value.ForceSet(lerped);
            }
        }
    }

    /// ExistByChance.Start of the component `id`: SetActive(false) of its object.
    pub fn exist_by_chance_start(&mut self, id: i32, chance: f32) {
        self.with_component_instance(id, |g, inst, o| {
            if inst.objects[o].active && g.random.value() > chance {
                g.set_active_scripts(inst, o, false);
            }
        });
    }
}
