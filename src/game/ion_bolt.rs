//! IonBoltBehaviour (the Ion Cannon's bolt, a bare object without a PhysicalBehaviour). Update moves the bolt along its right
//! vector by Speed * deltaTime, unless a raycast of that step hits: the bolt turns to the reflection and stops on the hit
//! point, a pulse explosion (no sound or effects) pushes what is around, every physical object within AoERadius of the hit
//! may break (a coin toss), is pushed by ImpactStrength and burns by 0.5 * Random.value, and the DeleteAfterTime loses a
//! tenth of its life. Awake's end-of-life listener (particles reparented to live a second), Start's impact effect and the
//! impact decal are presentation.
#![allow(non_snake_case)]

use super::translated::rt::Xf;
use super::{d, mathf, script, Game};
use crate::b2::math::Vec2 as V;

script! {
    IonBolt {
        Speed: f32 = 1.0,
        ImpactStrength: f32 = 0.0,
        AoERadius: f32 = 1.0,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        enabled: bool = true,
        /// Layers.m_Bits
        Layers: u32 = 0,
        /// DeleteAfterTime (`Game::delete_afters` index).
        delete_after: usize = 0,
    }
}

impl Game {
    /// IonBoltBehaviour.Update
    pub fn ion_bolt_update(&mut self, b: usize) {
        let id = self.ion_bolts[b].id;
        let num = self.ion_bolts[b].Speed * self.time.deltaTime;
        if !self.ion_bolt_do_hit_check(b, num) {
            let (position, rotation) = self.with_component_instance(id, |_, inst, o| (inst.position(o), inst.rotation(o)));
            let right = mathf::rotate(rotation, [1.0, 0.0, 0.0]);
            let to = [position[0] + right[0] * num, position[1] + right[1] * num, position[2] + right[2] * num];
            self.with_component_instance(id, |_, inst, o| inst.set_world_position(o, to));
        }
    }

    /// IonBoltBehaviour.DoHitCheck(distance): true when the step hit something (the bolt is then at the hit point).
    fn ion_bolt_do_hit_check(&mut self, b: usize, distance: f32) -> bool {
        let id = self.ion_bolts[b].id;
        let (position, rotation) = self.with_component_instance(id, |_, inst, o| (inst.position(o), inst.rotation(o)));
        let right = mathf::rotate(rotation, [1.0, 0.0, 0.0]);
        let mask = self.ion_bolts[b].Layers;
        // Physics2D.Raycast(position, right, distance, Layers)
        let Some(hit) = self.raycast(V::new(position[0], position[1]), [right[0], right[1]], distance, mask, f32::NEG_INFINITY, f32::INFINITY) else {
            return false;
        };
        let (point, normal) = ([hit.point.x, hit.point.y], [hit.normal.x, hit.normal.y]);
        // Vector2.Reflect(right, normal)
        let dir = [right[0], right[1]];
        let num = (d(-2.0) * (d(normal[0]) * dir[0] + d(normal[1]) * dir[1]).f()).f();
        let reflected = [(d(num) * normal[0] + dir[0]).f(), (d(num) * normal[1] + dir[1]).f()];
        // Instantiate(ImpactEffect, hit.point, rotation).transform.up = normal: presentation.
        let strength = self.ion_bolts[b].ImpactStrength;
        // ExplosionCreator.CreatePulseExplosion(hit.point, ImpactStrength, 1f, soundAndEffects: false): breakObjects defaults to true.
        self.create_pulse_explosion_with([point[0], point[1], 0.0], strength, 1.0, false, true);
        // Physics2D.OverlapCircleNonAlloc(hit.point, AoERadius, buffer[16], Layers)
        let radius = self.ion_bolts[b].AoERadius;
        let hits = self.overlap_circle(V::new(point[0], point[1]), radius, mask);
        for (victim, collider) in hits.into_iter().take(16) {
            // PhysicalObjectsInWorldByTransform.TryGetValue(collider.transform)
            let Some(p) = victim.filter(|&p| self.physicals[p].in_world) else { continue };
            if (self.random.value() as f64) > 0.5 {
                self.send_break(collider, Some(p), [-normal[0] * strength, -normal[1] * strength]);
            }
            let rb = self.physicals[p].rigidbody.clone();
            rb.add_impulse_at_position(&mut self.world, [-1.0 * strength * normal[0], -1.0 * strength * normal[1]], point);
            let bp = self.physicals[p].BurnProgress;
            self.physicals[p].BurnProgress = (d(bp) + d(0.5f32) * self.random.value()).f();
        }
        // ImpactDecal: the decal is rendering.
        // base.transform.right = vector
        self.tr_set_right(Xf::Object(id), [reflected[0], reflected[1], 0.0]);
        // DeleteAfterTime.Life *= 0.9f
        let dat = self.ion_bolts[b].delete_after;
        self.delete_afters[dat].Life = (d(self.delete_afters[dat].Life) * d(0.9f32)).f();
        // base.transform.position = hit.point
        self.with_component_instance(id, |_, inst, o| inst.set_world_position(o, [point[0], point[1], 0.0]));
        true
    }
}
