//! FlamethrowerBehaviour (the flamethrower and the fire extinguisher). Audio, the particle's pose and the context menu are
//! presentation; the continuous-use timer, the particle request, the recoil, the overlap queries and their burn or
//! extinguish effects stay. AffectAoe (DestroyableBehaviour.OnBreak) is not rewritten: the break event stops in
//! `destroyable_break`.
#![allow(non_snake_case)]

use super::explosion::DEFAULT_RAYCAST_LAYERS;
use super::{d, mathf, script, Game};
use crate::assets::db::ObjRef;
use crate::b2::math::Vec2 as V;
use crate::unity::transform::{global_position, global_rotation, Trs};

script! {
    Flamethrower {
        /// SprayEffect: Ignite 0, Extinguish 1
        Effect: i32 = 0,
        Point: [f32; 2] = [0.0, 0.0],
        RayCount: i32 = 16,
        Range: f32 = 5.0,
        Angle: f32 = 30.0,
        AoeRange: f32 = 3.0,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// Behaviour.enabled
        enabled: bool = true,
        /// GetComponent<PhysicalBehaviour>()
        phys: usize = 0,
        /// particlePrefab
        particlePrefab: Option<ObjRef> = None,
        /// Muzzle: the local TRS of its object and of each parent below the flamethrower's own object.
        Muzzle: Vec<Trs> = Vec::new(),
        /// LayerMask bits
        LayerMask: u32 = 0,
        time: f32 = 0.0,
    }
}

impl Game {
    /// Muzzle's position and rotation: its local TRS up to the flamethrower's object, then the object's chain.
    fn flamethrower_muzzle_pose(&self, i: usize) -> ([f32; 3], [f32; 4]) {
        let s = &self.flamethrowers[i];
        let nodes: Vec<Trs> = s.Muzzle.iter().copied().chain(self.local_chain(s.phys).iter().rev().copied()).collect();
        let (mut v, mut r) = ([nodes[0].t[0], nodes[0].t[1], nodes[0].t[2], 0.0], nodes[0].q);
        for n in &nodes[1..] {
            (v, r) = (global_position(n, v), global_rotation(n, r));
        }
        ([v[0], v[1], v[2]], r)
    }

    /// FlamethrowerBehaviour.Update
    pub fn flamethrower_update(&mut self, i: usize) {
        let p = self.flamethrowers[i].phys;
        let dt = self.time.deltaTime;
        self.flamethrowers[i].time += dt;
        if self.started_used_continuously(p) {
            // IgnitionSource.time = Random.value * clip.length (the audio itself is presentation)
            self.random.value();
            self.flamethrowers[i].time = 0.0;
        }
        if self.used_continuously(p) && self.physicals[p].Wetness <= 0.0 {
            self.flamethrowers[i].time += dt;
            if self.flamethrowers[i].time > 0.02 {
                self.flamethrowers[i].time = 0.0;
                let effect = self.flamethrowers[i].particlePrefab.expect("FlamethrowerBehaviour: RequestPrefab(null)");
                self.request_pooled_effect(effect, "FlamethrowerBehaviour.particlePrefab");
                let (muzzle, muzzle_rotation) = self.flamethrower_muzzle_pose(i);
                let scale = self.physicals[p].transform.localScale[0];
                let right = mathf::rotate(muzzle_rotation, [1.0, 0.0, 0.0]);
                let mine = mathf::rotate(self.physicals[p].transform.rotation, [1.0, 0.0, 0.0]);
                let k = -0.75f32 * scale;
                let rb = self.physicals[p].rigidbody.clone();
                rb.add_force_at_position(&mut self.world, [mine[0] * k, mine[1] * k], [muzzle[0], muzzle[1]]);
                let point = self.flamethrowers[i].Point;
                let at = self.transform_point3(p, [point[0], point[1], 0.0]);
                self.flamethrower_affect_point(i, [at[0], at[1]], [right[0] * scale, right[1] * scale]);
                let (range, mask) = (self.flamethrowers[i].Range, self.flamethrowers[i].LayerMask);
                for (origin, direction) in self.flamethrower_rays(i) {
                    if let Some(hit) = self.circle_cast_first(origin, 0.05, direction, range, mask) {
                        self.flamethrower_affect_point(i, hit, direction);
                    }
                }
            }
        }
        // StoppedBeingUsedContinuously && IgnitionSource.isPlaying: IgnitionSource.Stop is audio.
    }

    /// FlamethrowerBehaviour.GetAllRays: Ray2D normalizes the direction.
    fn flamethrower_rays(&self, i: usize) -> Vec<([f32; 2], [f32; 2])> {
        let s = &self.flamethrowers[i];
        let p = s.phys;
        let c = self.transform_point3(p, [s.Point[0], s.Point[1], 0.0]);
        let t = &self.physicals[p].transform;
        let base_angle = (d(super::dump::euler_z(t.rotation)) + if t.localScale[0] > 0.0 { 0.0f32 } else { 180.0f32 }).f();
        let to_radians = std::f64::consts::PI as f32 / 180.0f32;
        (0..s.RayCount)
            .map(|k| {
                let f = (d(to_radians) * (d(base_angle) + d(s.Angle) * (d(k as f32) / s.RayCount as f32) - d(s.Angle) / 2.0f32)).f();
                ([c[0], c[1]], mathf::normalized2([mathf::cos(f), mathf::sin(f)]))
            })
            .collect()
    }

    /// Physics2D.CircleCast(origin, radius, direction, distance, layerMask): the point of the nearest hit.
    fn circle_cast_first(&mut self, _origin: [f32; 2], _radius: f32, _direction: [f32; 2], _distance: f32, _mask: u32) -> Option<[f32; 2]> {
        unimplemented!("Physics2D.CircleCast: Cast2DQueryBase::RunQuery (ShapeCast2DQuery, 180bf7af0) is not rewritten")
    }

    /// FlamethrowerBehaviour.AffectPoint: Physics2D.OverlapCircleNonAlloc(worldPoint, 0.1f, buffer[32]).
    fn flamethrower_affect_point(&mut self, i: usize, world_point: [f32; 2], dir: [f32; 2]) {
        let hits = self.overlap_circle(V::new(world_point[0], world_point[1]), 0.1, DEFAULT_RAYCAST_LAYERS);
        if hits.len() > 32 {
            unimplemented!("Physics2D.OverlapCircleNonAlloc with more hits than its buffer holds");
        }
        for (physical, collider) in hits {
            self.flamethrower_affect_collider(i, physical, collider, dir, world_point);
        }
    }

    /// FlamethrowerBehaviour.AffectCollider
    fn flamethrower_affect_collider(&mut self, i: usize, physical: Option<usize>, collider: i32, dir: [f32; 2], point: [f32; 2]) {
        let Some(v) = physical else { return };
        let own = self.flamethrowers[i].phys;
        // collider.transform.root == base.transform.root; PhysicalObjectsInWorldByTransform.TryGetValue(collider.transform)
        if self.root_of(v) == self.root_of(own) || !self.physicals[v].colliders.contains(&collider) || !self.physicals[v].in_world {
            return;
        }
        if self.flamethrowers[i].Effect == 0 {
            let dt = self.time.deltaTime;
            let heat = self.heat_capacity(v);
            let temperature = self.physicals[v].Temperature;
            self.physicals[v].Temperature = (d(temperature) + d(dt) / heat * 2.0f32).f();
            self.ignite_at(v, false, point);
            if self.physicals[v].OnFire {
                self.physicals[v].burnIntensity = 1.0;
            }
        } else {
            self.extinguish(v);
            self.physicals[v].burnIntensity = 0.0;
        }
        let r = self.random.range(0.0, 0.2);
        let rb = self.physicals[v].rigidbody.clone();
        rb.add_force(&mut self.world, [dir[0] * r, dir[1] * r]);
    }
}
