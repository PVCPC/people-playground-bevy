//! EMPBehaviour (the EMP Generator). Use, while enabled, runs Activate: every physical object in the world, in list
//! order, draws Random.value, and unless that draw is below DamageChance or the object lies outside Range it is sent
//! OnEMPHit. An android's limb loses all its Health (LimbBehaviour.OnEMPHit). Awake's Killzone clone (a SpriteRenderer
//! and rendering scripts, no collider) and the Effect clone (particles) are presentation.
#![allow(non_snake_case)]

use super::{script, Game};

/// The classes that declare OnEMPHit (besides LimbBehaviour and DamagableMachineryBehaviour, handled below). None of them is rewritten, so one on the
/// hit object stops the run.
const EMP_RECEIVERS: &[&str] = &[
    "BatteryAttachmentBehaviour", "BulbBehaviour", "CarBehaviour", "CathodeLightBehaviour",
    "GlowtubeBehaviour", "StickyGrenadeBehaviour",
    "TelevisionBehaviour",
];

script! {
    Emp {
        DamageChance: f32 = 0.25,
        Range: f32 = 0.0,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        enabled: bool = true,
        /// The GameObject's PhysicalBehaviour (its transform is base.transform).
        physical: usize = 0,
    }
}

impl Game {
    /// EMPBehaviour.Use(activation): Activate while enabled.
    pub fn emp_use(&mut self, i: usize) {
        if self.emps[i].enabled {
            self.emp_activate(i);
        }
    }

    /// EMPBehaviour.Activate. The objects are the world's physical objects in index order (the order they entered the
    /// world); the draw comes before the test, so every object uses one draw.
    fn emp_activate(&mut self, i: usize) {
        let (p, chance, range) = (self.emps[i].physical, self.emps[i].DamageChance, self.emps[i].Range);
        let position = self.physicals[p].transform.position;
        let sqr_range = range * range;
        let items: Vec<usize> = (0..self.physicals.len()).filter(|&q| self.physicals[q].in_world && !self.physicals[q].destroyed).collect();
        for item in items {
            let roll = self.random.value();
            let at = self.physicals[item].transform.position;
            let sqr = (position[0] - at[0]) * (position[0] - at[0]) + (position[1] - at[1]) * (position[1] - at[1]) + (position[2] - at[2]) * (position[2] - at[2]);
            if !(roll < chance) && sqr < sqr_range {
                self.emp_send_on_hit(item);
            }
        }
    }

    /// SendMessage("OnEMPHit") on the item's GameObject, its own components only.
    pub(super) fn emp_send_on_hit(&mut self, p: usize) {
        if let Some(l) = self.limbs.iter().position(|x| x.PhysicalBehaviour == p) {
            // LimbBehaviour.OnEMPHit: an android's limb loses all its Health.
            if self.limbs[l].IsAndroid {
                self.limbs[l].Health = 0.0;
            }
        }
        // DamagableMachineryBehaviour.OnEMPHit on the same GameObject.
        if let Some(m) = self.machinery.iter().position(|x| x.PhysicalBehaviour == p) {
            self.machinery_on_emp_hit(m);
        }
        let id = self.physicals[p].id;
        // LaserTurretBehaviour.OnEMPHit on a turret of this GameObject: SetAsBroken (laser_turret.rs).
        let components = self.with_component_instance(id, |_, inst, o| inst.objects[o].components.clone());
        let lasers: Vec<usize> = (0..self.laser_turrets.len()).filter(|&l| components.contains(&self.laser_turrets[l].id)).collect();
        for l in lasers {
            self.laser_turret_set_as_broken(l);
        }
        // JavelinMissileBehaviour.OnEMPHit (javelin.rs).
        let missiles: Vec<usize> = (0..self.javelin_missiles.len()).filter(|&m| components.contains(&self.javelin_missiles[m].id)).collect();
        for m in missiles {
            self.javelin_missile_on_emp_hit(m);
        }
        // SentryTurretBehaviour.OnEMPHit: Explode (sentry_turret.rs).
        let sentries: Vec<usize> = (0..self.sentry_turrets.len()).filter(|&s| components.contains(&self.sentry_turrets[s].id)).collect();
        for s in sentries {
            self.sentry_turret_on_emp_hit(s);
        }
        let hit = self.with_component_instance(id, |_, inst, o| {
            inst.prefab.objects[o].components.iter().find_map(|(_, c)| match c {
                crate::assets::db::Component::Script { class, .. } if EMP_RECEIVERS.contains(&class.as_str()) => Some(class.clone()),
                _ => None,
            })
        });
        if let Some(class) = hit {
            unimplemented!("{class}.OnEMPHit (the EMP's receiver is not rewritten)");
        }
    }
}
