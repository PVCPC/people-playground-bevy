//! StunnerBehaviour (the Stunner's bolt, Stunbolt.prefab): a Transform that steps along its right vector, sinking by
//! `t` each frame, until a ray finds something. A hit destroys the bolt, spawns its ImpactEffect and sends StunImpact
//! to the hit object. LimbBehaviour is the only class with a StunImpact method (`limb_stun_impact`).
#![allow(non_snake_case)]

use super::explosion::{normalized3, RayHit2D};
use super::{d, mathf, script, Game};
use crate::assets::db::{Component, ObjRef};
use crate::b2::math::Vec2 as V;

script! {
    Stunner {
        Speed: f32 = 1.0,
        Gravity: f32 = 1.0,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// ImpactEffect: instantiated on a hit.
        ImpactEffect: Option<ObjRef> = None,
        /// mask (LayerMask bits)
        mask: u32 = 0,
        /// The private `t`: how far the bolt has sunk.
        t: f32 = 0.0,
    }
}

impl Game {
    /// The limb a SendMessage("StunImpact") reaches on the hit transform's GameObject: LimbBehaviour is the only class
    /// with a StunImpact method (DontRequireReceiver, so no other receiver is needed).
    fn stunner_hit_limb(&mut self, hit: &RayHit2D) -> Option<usize> {
        match self.hit_transform_physical(hit).or_else(|| self.physical_of_collider(hit.collider)) {
            Some(p) => {
                let has = self.with_component_instance(self.physicals[p].id, |_, inst, o| {
                    inst.prefab.objects[o].components.iter().any(|(_, c)| matches!(c, Component::Script { class, .. } if class == "LimbBehaviour"))
                });
                if !has {
                    return None;
                }
                Some(self.limbs.iter().position(|x| x.PhysicalBehaviour == p).unwrap_or_else(|| unimplemented!("LimbBehaviour without its limb")))
            }
            None => {
                if self.bare_colliders[&hit.collider].receives(&["LimbBehaviour"]) {
                    unimplemented!("StunImpact on a LimbBehaviour without a PhysicalBehaviour");
                }
                None
            }
        }
    }

    /// LimbBehaviour.StunImpact
    pub fn limb_stun_impact(&mut self, l: usize) {
        let (person, p) = (self.limbs[l].Person, self.limbs[l].PhysicalBehaviour);
        if self.limbs[l].IsAndroid {
            self.physicals[p].charge = self.physicals[p].Charge() + 10.0;
            return;
        }
        self.physicals[p].charge = self.physicals[p].Charge() + 1.0;
        if self.limb_is_considered_alive(l) && !self.limb_is_paralysed(l) {
            self.add_pain(person, 150.0);
            if self.prefs.StopAnimationOnDamage {
                self.people[person].OverridePoseIndex = -1;
            }
        }
        // StartCoroutine(Utils.DelayCoroutine(UnityEngine.Random.value * 0.8f, action)): the delay is drawn here.
        let delay = self.random.value() * 0.8;
        let owner = self.limbs[l].id;
        self.start_wait(owner, delay, super::destroyable::Step::LimbStunNumb(l));
    }

    /// The DelayCoroutine action of LimbBehaviour.StunImpact: Numbness = 1f, then Person.Consciousness = 0f while
    /// IsConsideredAlive.
    pub fn limb_stun_numb(&mut self, l: usize) {
        self.limbs[l].Numbness = 1.0;
        if self.limb_is_considered_alive(l) {
            let person = self.limbs[l].Person;
            self.people[person].Consciousness = 0.0;
        }
    }

    /// StunnerBehaviour.Update
    pub fn stunner_update(&mut self, s: usize) {
        let st = &self.stunners[s];
        let (id, speed, gravity, t, mask) = (st.id, st.Speed, st.Gravity, st.t, st.mask);
        let dt = self.time.deltaTime;
        let (position, rotation) = self.with_component_instance(id, |_, inst, o| (inst.position(o), inst.rotation(o)));
        // Vector3 right = base.transform.right; right.y -= t; right.Normalize();
        let right0 = mathf::rotate(rotation, [1.0, 0.0, 0.0]);
        let right = normalized3([right0[0], right0[1] - t, right0[2]]);
        let num = speed * dt;
        // Physics2D.Raycast(position, right, num, mask): the Vector3s convert to Vector2 by dropping z.
        match self.raycast(V::new(position[0], position[1]), [right[0], right[1]], num, mask, f32::NEG_INFINITY, f32::INFINITY) {
            Some(hit) => {
                self.destroy_object(id);
                let impact = self.stunners[s].ImpactEffect.expect("StunnerBehaviour: Instantiate(null ImpactEffect) (ArgumentException)");
                self.spawn_prefab(impact, position);
                // SendMessage("StunImpact", new Shot(normal, point, 0f), DontRequireReceiver) on the hit transform.
                if let Some(l) = self.stunner_hit_limb(&hit) {
                    self.limb_stun_impact(l);
                }
            }
            None => {
                let to = [position[0] + right[0] * num, position[1] + right[1] * num, position[2] + right[2] * num];
                self.with_component_instance(id, |_, inst, o| inst.set_world_position(o, to));
            }
        }
        // t += Time.deltaTime * 0.1f * Gravity
        self.stunners[s].t = (d(t) + d(dt) * 0.1f32 * gravity).f();
    }
}
