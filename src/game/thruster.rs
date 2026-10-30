//! ThrusterbedBehaviour. Audio, the particle system's module settings and the context menu are presentation; the
//! activation, thrust, burn rays, temperature and Random draws stay.
#![allow(non_snake_case)]

use super::{d, mathf, script, Game};
use crate::b2::math::Vec2 as V;
use crate::game_assets::{Curve, CurveCache};

script! {
    Thrusterbed {
        Activated: bool = false,
        ThrustingForce: f32 = 320.0,
        RelativeDirection: [f32; 2] = [0.0, 1.0],
        DoBurnOthers: bool = true,
        TemperatureTarget: f32 = 600.0,
        BurnRayCount: i32 = 3,
        BurnRayWidth: f32 = 1.0,
        BurnRayLength: f32 = 5.0,
        BurnRayOffset: [f32; 2] = [0.0, 0.0],
        BurnRayPerpendicular: [f32; 2] = [0.0, 1.0],
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// Behaviour.enabled
        enabled: bool = true,
        /// GetComponent<PhysicalBehaviour>() (its Rigidbody2D is GetComponent<Rigidbody2D>())
        Phys: usize = 0,
        /// LayersToBurn (LayerMask bits)
        LayersToBurn: u32 = 0,
        StartupStrengthCurve: Option<Curve> = None,
        cache: CurveCache = CurveCache::default(),
        time: f32 = 0.0,
    }
}

impl Game {
    /// ThrusterbedBehaviour.Start
    pub fn thrusterbed_start(&mut self, i: usize) {
        self.thrusterbed_update_activated(i);
    }

    /// ThrusterbedBehaviour.Use
    pub fn thrusterbed_use(&mut self, i: usize) {
        if self.thrusterbeds[i].enabled {
            self.thrusterbeds[i].Activated = !self.thrusterbeds[i].Activated;
            self.thrusterbed_update_activated(i);
        }
    }

    /// ThrusterbedBehaviour.UpdateActivated (the audio and the effect's Play and Stop are presentation)
    fn thrusterbed_update_activated(&mut self, i: usize) {
        self.thrusterbed_update_particle_system();
        self.thrusterbeds[i].time = 0.0;
    }

    /// ThrusterbedBehaviour.UpdateParticleSystem: startSpeedMultiplier = progress * Random.Range(5f, 12f) + ...
    fn thrusterbed_update_particle_system(&mut self) {
        self.random.range(5.0, 12.0);
    }

    /// ThrusterbedBehaviour.OnDisable (the effect and the audio stop)
    pub fn thrusterbed_on_disable(&mut self, i: usize) {
        self.thrusterbeds[i].Activated = false;
    }

    /// ThrusterbedBehaviour.FixedUpdate
    pub fn thrusterbed_fixed_update(&mut self, i: usize) {
        if !self.thrusterbeds[i].Activated {
            return;
        }
        let dt = self.time.deltaTime;
        let s = &mut self.thrusterbeds[i];
        s.time += dt;
        let t = mathf::clamp01(s.time);
        let curve = s.StartupStrengthCurve.as_ref().expect("ThrusterbedBehaviour.StartupStrengthCurve");
        let num = curve.evaluate(&mut s.cache, t);
        let (p, target, burn) = (s.Phys, s.TemperatureTarget, s.DoBurnOthers);
        self.thrusterbed_update_particle_system();
        if burn {
            self.thrusterbed_burn_others(i, num);
        }
        self.thrusterbed_thrust(i, num);
        let charge = self.physicals[p].Charge();
        let temperature = self.physicals[p].Temperature;
        let heat = self.heat_capacity(p);
        self.physicals[p].Temperature = mathf::lerp(temperature, target + charge, 0.005 / heat);
        let at = self.physicals[p].transform.position;
        self.camera_shake(((d(num) + d(charge) * 0.1f32) * 0.1f32).f(), [at[0], at[1]]);
    }

    /// ThrusterbedBehaviour.Thrust
    fn thrusterbed_thrust(&mut self, i: usize, progress: f32) {
        let s = &self.thrusterbeds[i];
        let (p, force, direction) = (s.Phys, s.ThrustingForce, s.RelativeDirection);
        let num = mathf::max(1.0, self.physicals[p].Charge() * 2.0);
        let scalar = (d(num) * progress * force).f();
        let v = self.transform_vector(p, direction);
        let rb = self.physicals[p].rigidbody.clone();
        rb.add_force(&mut self.world, [scalar * v[0], scalar * v[1]]);
    }

    /// ThrusterbedBehaviour.BurnOthers
    fn thrusterbed_burn_others(&mut self, i: usize, progress: f32) {
        let s = &self.thrusterbeds[i];
        let (p, direction, count, width, length, offset, perpendicular, mask) =
            (s.Phys, s.RelativeDirection, s.BurnRayCount, s.BurnRayWidth, s.BurnRayLength, s.BurnRayOffset, s.BurnRayPerpendicular, s.LayersToBurn);
        let num = (d(self.physicals[p].Charge()) * 0.2f32 * progress).f();
        let t = self.transform_vector(p, direction);
        let vector = [t[0], t[1]];
        let vector2 = [vector[0] * num, vector[1] * num];
        let step = width / mathf::max(1.0, (count - 1) as f32);
        let vector3 = [step * perpendicular[0], step * perpendicular[1]];
        let half = width / 2.0;
        for k in 0..count {
            let k = k as f32;
            let local = [(offset[0] + vector3[0] * k) - half * perpendicular[0], (offset[1] + vector3[1] * k) - half * perpendicular[1]];
            let start = self.transform_point3(p, [local[0], local[1], 0.0]);
            let back = 0.0 - length;
            let end = [(start[0] + vector[0] * back) - vector2[0], (start[1] + vector[1] * back) - vector2[1]];
            let hits = self.linecast_hits(V::new(start[0], start[1]), V::new(end[0], end[1]), mask, 32);
            if !hits.is_empty() {
                self.thrusterbed_burn_in_buffer(i, progress, &hits);
            }
        }
    }

    /// ThrusterbedBehaviour.BurnInBuffer
    fn thrusterbed_burn_in_buffer(&mut self, i: usize, progress: f32, hits: &[super::explosion::RayHit2D]) {
        let s = &self.thrusterbeds[i];
        let (p, direction, target, id) = (s.Phys, s.RelativeDirection, s.TemperatureTarget, s.id);
        let vector = self.transform_vector(p, direction);
        let own = self.with_component_instance(id, |_, inst, o| inst.objects[o].game_object);
        for hit in hits {
            // collider.transform == base.transform
            if hit.game_object == own {
                continue;
            }
            let charge = self.physicals[p].Charge();
            if let Some(rb) = self.rigidbody_of_body(hit.body) {
                let k = (d(-15.0f32) - d(charge) * 2.0f32).f();
                let force = [vector[0] * progress * k, vector[1] * progress * k];
                rb.add_force(&mut self.world, force);
                let torque = self.random.range(-0.1, 0.1);
                rb.add_torque(&mut self.world, torque);
            }
            // PhysicalObjectsInWorldByTransform.TryGetValue(collider.transform)
            let Some(v) = self.physical_of_collider(hit.collider).filter(|&v| self.physicals[v].in_world) else { continue };
            let vid = self.physicals[v].id;
            if self.with_component_instance(vid, |_, inst, o| inst.objects[o].game_object) != hit.game_object {
                continue;
            }
            let heat = self.heat_capacity(v);
            let temperature = self.physicals[v].Temperature;
            self.physicals[v].Temperature = mathf::lerp(temperature, target + charge, 0.01 / heat);
            let r = self.random.value();
            let burn = self.physicals[v].BurnProgress;
            self.physicals[v].BurnProgress = (d(burn) + (d(0.02f32) * r + d(charge) * 0.01f32)).f();
            if self.random.value() as f64 > 0.95 {
                self.ignite(v, false);
            }
        }
    }
}
