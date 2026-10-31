//! SimpleLightningEmitterBehaviour.Emit: a random walk from the origin that bends toward nearby physical objects, and
//! stops at the first one under it. Its onHit is AcceleratorBoltBehaviour.OnLightningHit, the only one the rewritten
//! classes pass. Start only sets the static Instance. The LineRenderer vertices and the pooled line are presentation.
#![allow(non_snake_case)]

use super::explosion::ExplosionParameters;
use super::{mathf, Game};
use crate::b2::math::Vec2 as V;

impl Game {
    /// SimpleLightningEmitterBehaviour.Emit (exclude is null). Each iteration: the point query (a hit ends the walk),
    /// Vector2.Lerp toward a Random.insideUnitCircle, then every object GetPhysicsObjectsNearPosition lists on the
    /// layers pulls the walk (a Random.value roll unless it conducts), and the step is clamped to maxStep.
    pub(super) fn lightning_emit(&mut self, origin: [f32; 2], direction: [f32; 2], max_range: f32, attraction: f32, max_iterations: i32, max_step: f32, randomness: f32, layers: u32) {
        let range2 = max_range * max_range;
        let mut vector = origin;
        let n = mathf::normalized2(direction);
        let mut vector2 = [n[0] * max_step, n[1] * max_step];
        for _ in 0..max_iterations {
            if let Some((Some(p), _)) = self.overlap_point_first(vector, layers) {
                if self.physicals[p].in_world {
                    self.accelerator_lightning_hit(p, vector);
                    break;
                }
            }
            let r = self.random.inside_unit_circle();
            vector2 = [mathf::lerp(vector2[0], r[0], randomness), mathf::lerp(vector2[1], r[1], randomness)];
            for item in self.physics_objects_near_position(vector, max_range) {
                if layers & (1 << self.physical_layer(item)) == 0 {
                    continue;
                }
                let closest = self.physical_closest_point(item, V::new(vector[0], vector[1]));
                let v3 = [closest[0] - vector[0], closest[1] - vector[1]];
                let sq = v3[0] * v3[0] + v3[1] * v3[1];
                if sq > 0.0 && sq <= range2 && (self.properties[self.physicals[item].Properties].Conducting || self.random.value() > 0.1) {
                    let u = mathf::normalized2(v3);
                    vector2 = [vector2[0] + u[0] / sq * attraction, vector2[1] + u[1] / sq * attraction];
                }
            }
            vector2 = clamp_magnitude(vector2, max_step);
            vector = [vector[0] + vector2[0], vector[1] + vector2[1]];
        }
    }

    /// AcceleratorBoltBehaviour.OnLightningHit: a finite point gains 20 charge and 100 temperature and explodes with force 2.
    fn accelerator_lightning_hit(&mut self, p: usize, point: [f32; 2]) {
        if point[0].is_nan() || point[1].is_nan() {
            return;
        }
        let ph = &mut self.physicals[p];
        ph.charge = ph.Charge() + 20.0;
        ph.Temperature += 100.0;
        // ExplosionCreator.Explode(center, force): DismemberChance 0.025, Range force * 2.5, 24 fragment rays.
        let force = 2.0f32;
        self.explosion_creator_explode(ExplosionParameters {
            FragmentationRayCount: 24,
            Position: [point[0], point[1], 0.0],
            Range: force * 2.5,
            FragmentForce: force,
            CreateParticlesAndSound: true,
            DismemberChance: 0.025,
            ..Default::default()
        });
    }

    /// PhysicalBehaviour.GetClosestPointTo: the nearest Collider2D.ClosestPoint over its non-trigger colliders, else the
    /// object's position.
    fn physical_closest_point(&mut self, p: usize, point: V) -> [f32; 2] {
        self.sync_pending();
        let pos = self.physicals[p].transform.position;
        let mut best = [pos[0], pos[1]];
        let mut num = f32::MAX;
        for c in self.physicals[p].colliders.clone() {
            if !self.collider_alive(c) {
                continue;
            }
            let v = self.world.collider_closest_point(c, point);
            let sq = (v.x - point.x) * (v.x - point.x) + (v.y - point.y) * (v.y - point.y);
            if sq < num {
                num = sq;
                best = [v.x, v.y];
            }
        }
        best
    }
}

/// Vector2.ClampMagnitude: a vector longer than max becomes its direction scaled to max (in double precision).
fn clamp_magnitude(v: [f32; 2], max: f32) -> [f32; 2] {
    let sq = v[0] * v[0] + v[1] * v[1];
    if sq > max * max {
        let m = (sq as f64).sqrt();
        [((v[0] as f64 / m) * max as f64) as f32, ((v[1] as f64 / m) * max as f64) as f32]
    } else {
        v
    }
}
