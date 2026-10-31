//! LightningGunBehaviour (the Stormbaan). While its physical is used continuously Update counts heldTime up and
//! FixedUpdate shakes the camera, vibrates the gun and performs the lightning: a ray from the particle system's
//! Transform along `localToWorldMatrix * (1, 0, 0)` ends the bolt, and every collider within Radius of its end that
//! belongs to another root on PriorityLayer and is in line of sight may be charged, pushed and set alight. The
//! particles, sounds and the `target` Transform's position are presentation.
#![allow(non_snake_case)]

use super::{mathf, script, Game};
use crate::b2::math::Vec2 as V;
use crate::unity::transform::{global_position, local_to_world, Trs};

script! {
    LightningGun {
        Range: f32 = 0.0,
        Radius: f32 = 0.0,
        ChargeOnHit: f32 = 5.0,
        PushForce: f32 = 1.0,
        VibrateIntensity: f32 = 1.0,
        CameraShakeIntensity: f32 = 0.01,
        IgnitionChance: f32 = 0.01,
        PriorityLayer: i32 = 0,
        heldTime: f32 = 0.0,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// Behaviour.enabled
        enabled: bool = true,
        /// Phys: the gun's own PhysicalBehaviour.
        Phys: usize = 0,
        /// ParticleSystem.transform: the local TRS of its object and of each parent below the gun's own object.
        RayOrigin: Vec<Trs> = Vec::new(),
        /// Layers (LayerMask bits)
        Layers: u32 = 0,
    }
}

impl Game {
    /// LightningGunBehaviour.RayOrigin: ParticleSystem.transform.position.
    fn lightning_gun_ray_origin(&self, i: usize) -> [f32; 3] {
        let s = &self.lightning_guns[i];
        let nodes: Vec<Trs> = s.RayOrigin.iter().copied().chain(self.local_chain(s.Phys).iter().rev().copied()).collect();
        let mut v = [nodes[0].t[0], nodes[0].t[1], nodes[0].t[2], 0.0];
        for n in &nodes[1..] {
            v = global_position(n, v);
        }
        [v[0], v[1], v[2]]
    }

    /// LightningGunBehaviour.Direction: base.transform.localToWorldMatrix * new Vector3(1f, 0f, 0f), the matrix's
    /// first column (the Vector4 has w = 0), so it keeps the lossy scale.
    fn lightning_gun_direction(&self, i: usize) -> [f32; 3] {
        let nodes: Vec<Trs> = self.local_chain(self.lightning_guns[i].Phys).into_iter().rev().collect();
        let m = local_to_world(&nodes);
        [m[0], m[1], m[2]]
    }

    /// The GameObject and Transform ids of `transform.root` of a physical's object.
    fn physical_root(&mut self, p: usize) -> (i32, i32) {
        let id = self.physicals[p].id;
        let attached: Vec<i32> = self.attached_clones.values().flatten().map(|a| a.id).collect();
        self.with_component_instance(id, |_, inst, _| {
            if attached.contains(&inst.objects[0].components[0]) {
                unimplemented!("Transform.root of a clone parented under another GameObject");
            }
            (inst.objects[0].game_object, inst.objects[0].components[0])
        })
    }

    /// `collider.transform.root` as (GameObject id, Transform id).
    fn collider_root(&mut self, collider: i32) -> (i32, i32) {
        match self.physical_of_collider(collider) {
            Some(p) => self.physical_root(p),
            None => {
                let root = self.bare_colliders[&collider].root;
                if self.attached_clones.values().flatten().any(|a| a.id == root.1) {
                    unimplemented!("Transform.root of a clone parented under another GameObject");
                }
                root
            }
        }
    }

    /// LightningGunBehaviour.Update: heldTime restarts with the use and counts while it lasts (the particles and the
    /// loop sound follow the use).
    pub fn lightning_gun_update(&mut self, i: usize) {
        let p = self.lightning_guns[i].Phys;
        if self.started_used_continuously(p) {
            self.lightning_guns[i].heldTime = 0.0;
        }
        if self.used_continuously(p) {
            self.lightning_guns[i].heldTime += self.time.deltaTime;
        }
    }

    /// LightningGunBehaviour.OnDisable: heldTime = 0 (the particles and the loop sound stop).
    pub fn lightning_gun_on_disable(&mut self, i: usize) {
        self.lightning_guns[i].heldTime = 0.0;
    }

    /// LightningGunBehaviour.FixedUpdate
    pub fn lightning_gun_fixed_update(&mut self, i: usize) {
        let p = self.lightning_guns[i].Phys;
        if !self.used_continuously(p) {
            return;
        }
        let s = &self.lightning_guns[i];
        let intensity = s.CameraShakeIntensity + mathf::min(s.heldTime * 0.1f32, 0.4f32);
        let vibrate = s.VibrateIntensity;
        let at = self.physicals[p].transform.position;
        self.camera_shake(intensity, [at[0], at[1]]);
        let c = self.random.inside_unit_circle();
        let rb = self.physicals[p].rigidbody.clone();
        rb.add_force(&mut self.world, [c[0] * vibrate, c[1] * vibrate]);
        self.lightning_gun_perform(i);
    }

    /// LightningGunBehaviour.PerformLightning
    fn lightning_gun_perform(&mut self, i: usize) {
        let direction = self.lightning_gun_direction(i);
        let ray_origin = self.lightning_gun_ray_origin(i);
        let (range, radius, layers, priority, p) = {
            let s = &self.lightning_guns[i];
            (s.Range, s.Radius, s.Layers, s.PriorityLayer, s.Phys)
        };
        let hit = self.raycast(V::new(ray_origin[0], ray_origin[1]), [direction[0], direction[1]], range, layers, f32::NEG_INFINITY, f32::INFINITY);
        let end = match hit {
            None => [ray_origin[0] + direction[0] * range, ray_origin[1] + direction[1] * range],
            Some(h) => [h.point.x, h.point.y],
        };
        let mut hits = self.overlap_circle(V::new(end[0], end[1]), radius, layers);
        hits.truncate(64);
        let mine = self.physical_root(p);
        for (q, collider) in hits {
            // The buffer holds colliders found a moment ago: one destroyed since has no transform.
            if !self.collider_exists(collider) {
                unimplemented!("LightningGunBehaviour.PerformLightning: a destroyed Collider2D in the buffer (MissingReferenceException)");
            }
            let root = self.collider_root(collider);
            if root.1 == mine.1 || priority != self.world.layers[&root.0] {
                continue;
            }
            let at = self.collider_position(collider);
            let vector2 = [at[0] - ray_origin[0], at[1] - ray_origin[1]];
            // Physics2D.Raycast(rayOrigin, vector2) with the default layers and an infinite distance.
            let hit2 = self.raycast_first(V::new(ray_origin[0], ray_origin[1]), vector2, f32::INFINITY).unwrap_or_else(|| unimplemented!("LightningGunBehaviour.PerformLightning: the line of sight hits nothing (NullReferenceException)"));
            let hit_root = self.collider_root(hit2.collider);
            // PhysicalObjectsInWorldByTransform.TryGetValue(collider.transform): the collider is on the physical's own object.
            let value = q.filter(|&q| self.physicals[q].in_world && !self.physicals[q].destroyed && Some(self.physical_game_object(q)) == self.world.collider_game_object(collider));
            let Some(value) = value.filter(|_| hit_root.1 == root.1) else { continue };
            let point = [hit2.point.x, hit2.point.y];
            let properties = &self.properties[self.physicals[value].Properties];
            let (conducting, magnetic) = (properties.Conducting, properties.MagneticAttractionIntensity);
            if (conducting && self.random.value() > 0.5f32) || self.random.value() < magnetic.unwrap_or_else(|| unimplemented!("PhysicalProperties.MagneticAttractionIntensity of a recorded asset")) * 0.05f32 {
                self.lightning_gun_do_damage_to(i, value, point, vector2);
            } else if self.random.value() < 0.05f32 {
                self.lightning_gun_do_damage_to(i, value, point, vector2);
            }
        }
        // TargetPosition.position = position: presentation.
    }

    /// LightningGunBehaviour.DoDamageTo
    fn lightning_gun_do_damage_to(&mut self, i: usize, phys: usize, point: [f32; 2], dir: [f32; 2]) {
        let s = &self.lightning_guns[i];
        let (held, charge, push, ignition) = (s.heldTime, s.ChargeOnHit, s.PushForce, s.IgnitionChance);
        let add = charge * mathf::min(held * 2.0f32, 200.0f32);
        self.physicals[phys].charge = self.physicals[phys].Charge() + add;
        let n = mathf::normalized2(dir);
        let rb = self.physicals[phys].rigidbody.clone();
        rb.add_force(&mut self.world, [n[0] * push, n[1] * push]);
        if self.random.value() < ignition + held / 10.0f32 {
            self.ignite_at(phys, false, point);
        }
    }
}
