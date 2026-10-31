//! PowerHammerBehaviour (the Power Hammer): a hard hit on its impact collider throws a hammer effect where the contact
//! is, shakes the camera, pushes the contact's collider and every collider in the area in front of the blow, and pulses
//! an explosion. A hit is ignored for a second after the last one. The audio is presentation.
#![allow(non_snake_case)]

use super::explosion::normalized3;
use super::physical::{dot2, get_min_impulse};
use super::{mathf, script, Game};
use crate::assets::db::ObjRef;
use crate::b2::contacts2d::ScriptCollision;
use crate::b2::math::Vec2 as V;

script! {
    PowerHammer {
        ForceMultiplier: f32 = 1.0,
        Range: f32 = 1.0,
        CameraShakeIntensity: f32 = 3.0,
        RequiredImpactForce: f32 = 15.0,
        ImpactCharge: f32 = 0.0,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// GetComponent<PhysicalBehaviour>() of the object: the collision messages reach it.
        physical: Option<usize> = None,
        /// The component's index on its GameObject, and whether it comes after the PhysicalBehaviour (`side` ordering).
        index: usize = 0,
        after_physical: bool = false,
        /// powerImpactCollider's instance id (None: a null reference, which no collider equals).
        powerImpactCollider: Option<i32> = None,
        /// physicalBehaviour: the physical whose charge and rigidbody the hammer reads.
        PhysicalBehaviour: usize = 0,
        /// ImpactClips.Length
        ImpactClipCount: usize = 0,
        /// HammerEffectPrefab
        HammerEffectPrefab: Option<ObjRef> = None,
        /// LayersToConsider bits
        LayersToConsider: u32 = 0,
        /// lastImpactTime (float.MinValue at the start)
        lastImpactTime: f32 = f32::MIN,
    }
}

/// Vector2.SignedAngle(from, to) > 0, decided without the angle's value: the angle is zero exactly when the normalised
/// dot clamps to 1 (or the denominator is below kEpsilonNormalSqrt), and the sign is Mathf.Sign(cross) (0 counts as +).
fn signed_angle_positive(from: [f32; 2], to: [f32; 2]) -> bool {
    // Vector2.Angle: denominator = sqrt(from.sqrMagnitude * to.sqrMagnitude); kEpsilonNormalSqrt = 1e-15.
    let (sq_from, sq_to) = (dot2(from, from), dot2(to, to));
    let denominator = (sq_from * sq_to).sqrt();
    if denominator < 1e-15 {
        return false;
    }
    let dot = dot2(from, to) / denominator;
    let unsigned_positive = dot.clamp(-1.0, 1.0) < 1.0;
    let cross = from[0] * to[1] - from[1] * to[0];
    unsigned_positive && cross >= 0.0
}

impl Game {
    /// PowerHammerBehaviour.OnCollisionEnter2D
    pub fn power_hammer_on_collision_enter(&mut self, h: usize, c: &ScriptCollision) {
        let s = &self.power_hammers[h];
        let Some(impact_collider) = s.powerImpactCollider else { return };
        if self.time.time - s.lastImpactTime < 1.0 || c.other_collider != impact_collider {
            return;
        }
        // Utils.GetMinImpulse(buffer[4], collision.GetContacts(buffer)); a limb takes 12 times the impulse.
        let contacts = c.contacts.len().min(4);
        let mut buffer = self.physical_contact_buffer;
        buffer[..contacts].copy_from_slice(&c.contacts[..contacts]);
        let mut num = get_min_impulse(&buffer, contacts);
        let other = self.physical_of_collider(c.collider);
        if other.is_some_and(|p| self.limbs.iter().any(|l| l.PhysicalBehaviour == p)) {
            num *= 12.0;
        }
        if num <= self.power_hammers[h].RequiredImpactForce {
            return;
        }
        let contact = c.contacts.first().unwrap_or_else(|| unimplemented!("PowerHammerBehaviour.OnCollisionEnter2D: collision.GetContact(0) without contacts"));
        let normal = [contact.normal.x, contact.normal.y];
        let point = [contact.point.x, contact.point.y];
        // Quaternion.Euler(0, 0, z) with z = Atan2(normal) * 57.29578 - 90.
        let z = normal[1].atan2(normal[0]) * 57.29578f32 - 90.0;
        let half = z * mathf::DEG2RAD * 0.5;
        let rotation = [0.0, 0.0, crate::unity::sinf(half), crate::unity::cosf(half)];
        let Some(effect) = self.power_hammers[h].HammerEffectPrefab else { unimplemented!("PowerHammerBehaviour: Instantiate(null HammerEffectPrefab) (ArgumentException)") };
        let mut db = self.assets.take().expect("PowerHammerBehaviour without the game's data");
        let spawned = db.prefab(effect).and_then(|prefab| {
            let (inst, linked) = self.spawn(&mut db, &prefab, [point[0], point[1], 0.0], rotation, None)?;
            Ok((prefab, inst, linked))
        });
        self.assets = Some(db);
        let (_, mut inst, linked) = spawned.unwrap_or_else(|e| panic!("PowerHammerBehaviour: Instantiate: {e}"));
        // base.transform.up (Vector3 to Vector2 drops z) and the hammer's own scale.
        let hammer = self.power_hammers[h].id;
        let (up, scale) = self.with_component_instance(hammer, |_, inst, o| (mathf::rotate(inst.rotation(o), [0.0, 1.0, 0.0]), inst.objects[o].trs.s));
        let hammer_scale = [scale[0], scale[1]];
        if signed_angle_positive(normal, [up[0], up[1]]) {
            inst.set_local_scale(0, [-1.0, 1.0, 1.0]);
        }
        let num2 = mathf::max(hammer_scale[0], hammer_scale[1]);
        let right = normalized3(mathf::rotate(inst.rotation(0), [1.0, 0.0, 0.0]));
        let sign = if inst.objects[0].trs.s[0] > 0.0 { 1.0 } else { -1.0 };
        let dir = [right[0] * sign, right[1] * sign];
        let s = inst.objects[0].trs.s;
        inst.set_local_scale(0, [s[0] * num2, s[1] * num2, s[2] * num2]);
        self.refresh_transforms(&inst, &linked);
        // mainAudioSource.PlayOneShot(ImpactClips.PickRandom()): the draw; the playback is audio.
        let clips = self.power_hammers[h].ImpactClipCount;
        self.pick_random(clips);
        let shake = self.power_hammers[h].CameraShakeIntensity * num2;
        let at = self.with_component_instance(hammer, |_, inst, o| inst.position(o));
        self.camera_shake(shake, [at[0], at[1]]);
        // affectingForce = dir * ForceMultiplier * num2, then *= max(1, Charge): each Vector2 operation rounds.
        let force_multiplier = self.power_hammers[h].ForceMultiplier;
        let phys = self.power_hammers[h].PhysicalBehaviour;
        let mut affecting = [dir[0] * force_multiplier * num2, dir[1] * force_multiplier * num2];
        let charge = mathf::max(1.0, self.physicals[phys].Charge());
        affecting = [affecting[0] * charge, affecting[1] * charge];
        let impact_charge = self.power_hammers[h].ImpactCharge;
        // applyForce(collision.collider)
        self.power_hammer_apply_force(c.collider, dir, affecting, impact_charge);
        // pointA = point - normal; pointB = point + dir * Range * num2 + normal * 3.
        let range = self.power_hammers[h].Range;
        let point_a = [point[0] - normal[0], point[1] - normal[1]];
        let t2 = [dir[0] * range * num2, dir[1] * range * num2];
        let t3 = [point[0] + t2[0], point[1] + t2[1]];
        let point_b = [t3[0] + normal[0] * 3.0, t3[1] + normal[1] * 3.0];
        let hits = self.power_hammer_area(V::new(point_a[0], point_a[1]), V::new(point_b[0], point_b[1]), self.power_hammers[h].LayersToConsider);
        if hits.len() > 64 {
            unimplemented!("Physics2D.OverlapAreaNonAlloc with more colliders than its buffer holds");
        }
        for (_, collider) in hits {
            // collbuffer[i].gameObject != base.gameObject: the hammer's own colliders are skipped.
            if self.physicals[phys].colliders.contains(&collider) {
                continue;
            }
            self.power_hammer_apply_force(collider, dir, affecting, impact_charge);
        }
        // CreatePulseExplosion(point, 2f * (Charge + 1f), 2f, soundAndEffects: false)
        let force = 2.0f32 * (self.physicals[phys].Charge() + 1.0);
        self.create_pulse_explosion([point[0], point[1], 0.0], force, 2.0);
        self.power_hammers[h].lastImpactTime = self.time.time;
        self.unsynced.push(inst);
    }

    /// Physics2D.OverlapAreaNonAlloc(a, b, buffer, mask) over every collider: the OverlapAreaAll query with its hits
    /// kept whole (tool.rs's overlap_area_all keeps only the physical ones).
    fn power_hammer_area(&mut self, a: V, b: V, mask: u32) -> Vec<(Option<usize>, i32)> {
        use crate::b2::math::{Rot, Transform};
        use crate::b2::world::Shape;
        self.sync_pending();
        let point = V::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
        let size = [(a.x - b.x).abs(), (a.y - b.y).abs()];
        let min = f32::from_bits(0x38d1b717);
        let shape = Shape::Polygon(crate::b2::shape::Polygon::set_as_box((size[0] * 0.5).max(min), (size[1] * 0.5).max(min), point, crate::b2::settings::globals().polygon_radius));
        let Shape::Polygon(poly) = &shape else { unreachable!() };
        let (mut lower, mut upper) = (poly.vertices[0], poly.vertices[0]);
        for v in &poly.vertices[1..4] {
            (lower, upper) = (V::new(lower.x.min(v.x), lower.y.min(v.y)), V::new(upper.x.max(v.x), upper.y.max(v.y)));
        }
        let r = poly.radius;
        let aabb = crate::b2::broadphase::Aabb { lower: V::new(lower.x - r, lower.y - r), upper: V::new(upper.x + r, upper.y + r) };
        let identity = Transform { p: V::ZERO, q: Rot::new(0.0) };
        self.overlap_query(self.world.query_aabb(&aabb), mask, f32::NEG_INFINITY, f32::INFINITY, |other, child, xf| shape.test_overlap(0, &identity, other, child, xf))
    }

    /// The applyForce local function: Break on the collider, its rigidbody's impulse (times its mass), and the charge of
    /// its physical.
    fn power_hammer_apply_force(&mut self, collider: i32, dir: [f32; 2], affecting: [f32; 2], impact_charge: f32) {
        let phys = self.physical_of_collider(collider);
        self.send_break(collider, phys, dir);
        if let Some(body) = self.world.collider_body(collider) {
            if let Some(rb) = self.rigidbody_of_body(body) {
                let mass = rb.mass(&self.world);
                rb.add_force_impulse(&mut self.world, [affecting[0] * mass, affecting[1] * mass]);
            }
        }
        if let Some(p) = phys {
            self.physicals[p].charge = self.physicals[p].Charge() + impact_charge;
        }
    }
}
