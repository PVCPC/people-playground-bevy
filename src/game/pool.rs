//! PoolGenerator and ObjectPoolBehaviour for pooled prefabs that carry scripts (full clones that are Instantiated,
//! deactivated on Return and reactivated on Request). One pool per prefab, as PoolGenerator.pools (keyed by the
//! prefab's instance id): GetFor, Ensure (a new GameObject + ObjectPoolBehaviour take 3 instance ids; MaxPoolSize =
//! PoolGenerator.PoolSize 256), RequestPrefab, Return and the 300 s inactivity Clear.
//!
//! Request: a pool with an available object takes `availableObjects.FirstOrDefault()` (HashSet slot order, `SlotSet`),
//! removes it, sets its position, SetActive(true), then BroadcastMessage OnPoolableInitialised and
//! OnPoolableReinitialised; otherwise a full pool returns null and a pool with room Instantiates the prefab, SetActive(true)
//! and broadcasts OnPoolableInitialised only. The particle-only prefabs of water.rs (SplashPool), the BloodPuff pool
//! (misc.rs) and the BulletTracerPool (ballistics.rs) are models of the same ObjectPoolBehaviour specialised to what
//! their clones hold (no scripts to message, or a particle clock for the OnParticleSystemStopped Return), so they stay
//! as they are; a prefab goes through exactly one of them.
#![allow(non_snake_case)]

use super::misc::ObjectPool;
use super::translated::rt::Go;
use super::Game;
use crate::assets::db::{Component, ObjRef};

/// PoolGenerator.PoolSize
const POOL_SIZE: usize = 256;

/// One Instantiated clone of a pool: its root Transform's instance id.
#[derive(Clone, Debug)]
pub struct PoolItem {
    pub root: i32,
}

/// An ObjectPoolBehaviour PoolGenerator made.
#[derive(Clone, Debug)]
pub struct PrefabPool {
    pub prefab: ObjRef,
    pub MaxPoolSize: usize,
    pub pool: ObjectPool,
    pub items: Vec<PoolItem>,
}

impl Game {
    /// PoolGenerator.GetFor
    pub fn pool_get_for(&self, prefab: ObjRef) -> Option<usize> {
        self.generic_pools.iter().position(|p| p.prefab == prefab)
    }

    /// PoolGenerator.Ensure: new GameObject(prefab.name + " pool").AddComponent<ObjectPoolBehaviour>() takes the
    /// GameObject's, its Transform's and the behaviour's instance ids.
    pub fn pool_ensure(&mut self, prefab: ObjRef) -> usize {
        if let Some(i) = self.pool_get_for(prefab) {
            return i;
        }
        for _ in 0..3 {
            self.allocate_instance_id();
        }
        self.generic_pools.push(PrefabPool { prefab, MaxPoolSize: POOL_SIZE, pool: ObjectPool::default(), items: Vec::new() });
        self.generic_pools.len() - 1
    }

    /// PoolGenerator.RequestPrefab(prefab, position): the pool and the item (None: the pool is full, null).
    pub fn pool_request_prefab(&mut self, prefab: ObjRef, position: [f32; 2]) -> Option<(usize, usize)> {
        let s = self.pool_ensure(prefab);
        self.pool_request(s, position).map(|i| (s, i))
    }

    /// ObjectPoolBehaviour.Request
    fn pool_request(&mut self, s: usize, position: [f32; 2]) -> Option<usize> {
        let pool = &mut self.generic_pools[s];
        pool.pool.inactivityTimer = 0.0;
        let first = pool.pool.available.iter().next().copied();
        if let Some(i) = first {
            pool.pool.available.remove_where(|&x| x == i);
            let root = pool.items[i].root;
            if self.destroyed_components.contains(&root) {
                unimplemented!("ObjectPoolBehaviour.Request: a poolable object has been destroyed (error log, RemoveWhere, null)");
            }
            self.with_component_instance(root, |_, inst, o| inst.set_world_position(o, [position[0], position[1], 0.0]));
            self.tr_set_active(Go::Local(root), true);
            self.pool_broadcast(root, s, false);
            self.pool_broadcast(root, s, true);
            return Some(i);
        }
        if pool.pool.created.len() >= pool.MaxPoolSize {
            return None;
        }
        let prefab = pool.prefab;
        let root = self.spawn_prefab(prefab, [position[0], position[1], 0.0]);
        if !self.tr_active_self(Go::Local(root)) {
            unimplemented!("ObjectPoolBehaviour.Request: SetActive(true) on an inactive prefab root (Awake and OnEnable then run)");
        }
        self.pool_broadcast(root, s, false);
        let pool = &mut self.generic_pools[s];
        pool.items.push(PoolItem { root });
        let i = pool.items.len() - 1;
        pool.pool.created.push(i);
        Some(i)
    }

    /// BroadcastMessage("OnPoolableInitialised" or "OnPoolableReinitialised", pool) to the clone's active objects.
    fn pool_broadcast(&mut self, root: i32, s: usize, reinitialised: bool) {
        let receivers: Vec<(String, i32)> = self.with_component_instance(root, |_, inst, _| {
            let mut out = Vec::new();
            for (x, ob) in inst.prefab.objects.iter().enumerate() {
                if !inst.objects[x].active {
                    continue;
                }
                for (k, (_, c)) in ob.components.iter().enumerate() {
                    if let Component::Script { class, .. } = c {
                        out.push((class.clone(), inst.objects[x].components[k]));
                    }
                }
            }
            out
        });
        for (class, id) in receivers {
            match (class.as_str(), reinitialised) {
                ("ArchelixCasterBoltBehaviour", false) => {
                    let b = match self.behaviour_of.get(&id) {
                        Some(&(_, super::Behaviour::ArchelixBolt(b))) => b,
                        _ => unreachable!("ArchelixCasterBoltBehaviour in the arena"),
                    };
                    self.archelix_bolt_on_poolable_initialised(b, s);
                }
                // OnPoolableInitialised only stores the pool (OnParticleSystemStopped Returns to it).
                ("PoolableParticleSystemHelper", false) => {}
                ("PoolOriginBehaviour" | "ReturnToPoolAfterTime", false) | ("ExplosionSoundBehviour" | "FancyBloodSplatController" | "PoolableSizeHelper", true) => {
                    unimplemented!("{class}.OnPoolable{}initialised in a pooled script clone", if reinitialised { "Re" } else { "" })
                }
                _ => {}
            }
        }
    }

    /// ObjectPoolBehaviour.Return: SetActive(false), then availableObjects.Add.
    /// `go` is the instance id of any component of the pooled object's root GameObject.
    pub fn pool_return(&mut self, s: usize, go: i32) {
        let root = self.with_component_instance(go, |_, inst, _| inst.objects[0].components[0]);
        self.tr_set_active(Go::Local(root), false);
        let pool = &mut self.generic_pools[s];
        let i = pool.items.iter().position(|x| x.root == root).expect("a Return of an object this pool made");
        pool.pool.available.add(i);
    }

    /// ObjectPoolBehaviour.Update of these pools: Clear (Object.Destroy of every clone) after 300 s with every object back.
    pub fn pool_update(&mut self) {
        let dt = self.time.deltaTime;
        for s in 0..self.generic_pools.len() {
            let pool = &mut self.generic_pools[s];
            if pool.pool.created.is_empty() {
                continue;
            }
            pool.pool.inactivityTimer += dt;
            if pool.pool.inactivityTimer > 300.0 && pool.pool.available.iter().count() == pool.pool.created.len() {
                let roots: Vec<i32> = pool.pool.created.iter().map(|&i| pool.items[i].root).collect();
                pool.pool.created.clear();
                pool.pool.available.clear();
                pool.pool.inactivityTimer = 0.0;
                for root in roots {
                    self.destroy_object(root);
                }
            }
        }
    }
}
