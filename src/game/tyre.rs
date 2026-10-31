//! TyreDeflationBehaviour and the native CircleCollider2D.radius / Collider2D.offset setters it calls.
#![allow(non_snake_case)]

use super::spawn::Instance;
use super::{script, Game};
use crate::assets::components::ColliderShape;
use crate::assets::db::Component;

/// DeflationParticlePrefabPath
const PARTICLE_PREFAB: &str = "Prefabs/Tyre deflation effect poolable";

script! {
    TyreDeflationBehaviour {
        Deflated: bool = false,
        Indestructible: bool = false,
        TemperatureThreshold: f32 = 398.0,
        ShotDamageThreshold: f32 = 8.0,
        FragmentForceThreshold: f32 = 15.0,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// Phys
        Phys: usize = 0,
        /// Collider as (object, component) of its clone.
        collider: (usize, usize) = (0, 0),
        /// DeflatedColliderSettings
        DeflatedRadius: f32 = 0.0,
        DeflatedOffset: [f32; 2] = [0.0; 2],
        /// initial, set by Awake.
        initialRadius: f32 = 0.0,
        initialOffset: [f32; 2] = [0.0; 2],
        /// TyreDeflationSound.Length
        sounds: usize = 0,
        /// pooledDeflationParticles as (`Game::splash_pools` index, item).
        pooled: Option<(usize, usize)> = None,
    }
}

/// The circle a CircleCollider2D currently has: its radius and offset.
fn circle_of(inst: &Instance, (o, c): (usize, usize)) -> (f32, [f32; 2]) {
    if let Some(&x) = inst.circles.get(&(o, c)) {
        return x;
    }
    let Component::Collider2D(col) = &inst.prefab.objects[o].components[c].1 else { unreachable!() };
    let ColliderShape::Circle { radius } = col.shape else { unreachable!("TyreDeflationBehaviour.Collider is a CircleCollider2D") };
    (radius, col.offset)
}

impl Game {
    /// CircleCollider2D.radius = v (CircleCollider2D::SetRadius 180c15c30): unchanged within 1e-6, else clamped to
    /// 0.0001..1e6 and the collider re-created.
    pub(super) fn circle_set_radius(&mut self, inst: &mut Instance, key: (usize, usize), v: f32) {
        let (radius, offset) = circle_of(inst, key);
        if 0.000_001f32 >= (radius - v).abs() {
            return;
        }
        let v = if 0.0001f32 > v { 0.0001 } else { f32::min(1_000_000.0, v) };
        inst.circles.insert(key, (v, offset));
        self.circle_recreate(inst, key);
    }

    /// Collider2D.offset = v (Collider2D::SetOffset 180c159e0): unchanged within 1e-5, else stored and the
    /// collider re-created.
    fn circle_set_offset(&mut self, inst: &mut Instance, key: (usize, usize), v: [f32; 2]) {
        let (radius, offset) = circle_of(inst, key);
        let (dx, dy) = (v[0] - offset[0], v[1] - offset[1]);
        let eps = 0.000_01f32;
        if eps * eps >= dx * dx + dy * dy {
            return;
        }
        inst.circles.insert(key, (radius, v));
        self.circle_recreate(inst, key);
    }

    /// The RecreateCollider(null) the setters end with; a collider of an inactive object has no fixtures to remake.
    fn circle_recreate(&mut self, inst: &mut Instance, (o, c): (usize, usize)) {
        if inst.objects[o].active {
            let prefab = inst.prefab.clone();
            self.collider2d_recreate(inst, &prefab, o, c);
        }
    }

    /// TyreDeflationBehaviour.Awake
    pub fn tyre_awake(&mut self, t: usize, inst: &Instance) {
        let (radius, offset) = circle_of(inst, self.tyres[t].collider);
        self.tyres[t].initialRadius = radius;
        self.tyres[t].initialOffset = offset;
    }

    /// TyreDeflationBehaviour.Repair: Deflated = false; Collider.radius = initial.Radius; Collider.offset = initial.Offset.
    /// The deflated material is rendering.
    pub fn tyre_repair(&mut self, t: usize) {
        self.tyres[t].Deflated = false;
        let s = &self.tyres[t];
        let (id, key, radius, offset) = (s.id, s.collider, s.initialRadius, s.initialOffset);
        self.with_component_instance(id, |g, inst, _| {
            g.circle_set_radius(inst, key, radius);
            g.circle_set_offset(inst, key, offset);
        });
    }

    /// TyreDeflationBehaviour.Start: a Deflated tyre takes the deflated circle. The material, the sag property block
    /// and the context menu buttons are rendering and UI.
    pub fn tyre_start(&mut self, t: usize) {
        if self.tyres[t].Deflated {
            self.tyre_deflated_circle(t);
        }
    }

    /// Collider.radius = DeflatedColliderSettings.Radius; Collider.offset = DeflatedColliderSettings.Offset;
    fn tyre_deflated_circle(&mut self, t: usize) {
        let s = &self.tyres[t];
        let (id, key, radius, offset) = (s.id, s.collider, s.DeflatedRadius, s.DeflatedOffset);
        self.with_component_instance(id, |g, inst, _| {
            g.circle_set_radius(inst, key, radius);
            g.circle_set_offset(inst, key, offset);
        });
    }

    /// TyreDeflationBehaviour.FixedUpdate
    pub fn tyre_fixed_update(&mut self, t: usize) {
        let s = &self.tyres[t];
        let p = &self.physicals[s.Phys];
        if !s.Indestructible && !s.Deflated && p.SimulateTemperature && p.Temperature >= s.TemperatureThreshold {
            let (position, key, id) = (p.transform.position, s.collider, s.id);
            let radius = self.with_component_instance(id, |_, inst, _| circle_of(inst, key).0);
            let c = self.random.inside_unit_circle();
            self.tyre_pop(t, Some([position[0] + c[0] * radius, position[1] + c[1] * radius]));
        }
    }

    /// TyreDeflationBehaviour.PopTyre
    pub fn tyre_pop(&mut self, t: usize, point: Option<[f32; 2]>) {
        let s = &self.tyres[t];
        if s.Indestructible || s.Deflated {
            return;
        }
        if point.is_some() {
            // Collider.ClosestPoint(point) starts with the AutoSyncTransforms; the effect's pose (the closest point,
            // right = away from the tyre, parent = the tyre) is rendering.
            self.sync_pending();
            let mut db = self.assets.take().expect("TyreDeflationBehaviour.PopTyre without the game's data");
            let pool = db.resource(PARTICLE_PREFAB, 1).and_then(|r| r.ok_or_else(|| format!("Resources.Load({PARTICLE_PREFAB}) found no GameObject"))).and_then(|r| self.splash_pool_for(&mut db, r));
            self.assets = Some(db);
            let pool = pool.unwrap_or_else(|e| panic!("TyreDeflationBehaviour.PopTyre: {e}"));
            if !self.splash_pools[pool].poolable {
                unimplemented!("TyreDeflationBehaviour.PopTyre: a deflation effect prefab not tagged Poolable");
            }
            let Some(item) = self.splash_request(pool) else { unimplemented!("TyreDeflationBehaviour.PopTyre: NullReferenceException (the pool is full)") };
            self.tyres[t].pooled = Some((pool, item));
        }
        // Phys.PlayClipOnce(TyreDeflationSound.PickRandom()): audio, the draw stays.
        self.pick_random(self.tyres[t].sounds);
        self.tyre_deflated_circle(t);
        self.tyres[t].Deflated = true;
    }

    /// TyreDeflationBehaviour.Shot
    pub fn tyre_shot(&mut self, t: usize, damage: f32, point: [f32; 2]) {
        let s = &self.tyres[t];
        if !s.Indestructible && damage >= s.ShotDamageThreshold {
            self.tyre_pop(t, Some(point));
        }
    }

    /// TyreDeflationBehaviour.OnFragmentHit
    pub fn tyre_on_fragment_hit(&mut self, t: usize, force: f32) {
        let s = &self.tyres[t];
        if !s.Indestructible && force >= s.FragmentForceThreshold {
            self.tyre_pop(t, None);
        }
    }

    /// TyreDeflationBehaviour.OnDestroy: the effect leaves the tyre and returns to its pool.
    pub fn tyre_destroy(&mut self, cid: i32) {
        let Some(t) = self.tyres.iter().position(|x| x.id == cid) else { return };
        let Some((pool, item)) = self.tyres[t].pooled else { return };
        // (bool)pooledDeflationParticles: the pool's Clear destroyed it.
        if self.splash_pools[pool].items[item].destroyed {
            return;
        }
        // ObjectPoolBehaviour.Return: SetActive(false), availableObjects.Add.
        let p = &mut self.splash_pools[pool];
        p.items[item].active = false;
        if !p.pool.available.iter().any(|&x| x == item) {
            p.pool.available.add(item);
        }
    }
}
