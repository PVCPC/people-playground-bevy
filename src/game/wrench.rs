//! WrenchBehaviour's OnCollisionEnter2D: a hit at or over MinImpulse draws a 1 in 4 roll for SendMessageUpwards("Repair")
//! to the hit object and its ancestors. The limb heal is only for androids. Rewritten: the draw and the no-receiver,
//! non-android path. A Repair receiver anywhere above the hit, a bare collider, and an android limb stop with
//! unimplemented!, since their repairs are not rewritten.
#![allow(non_snake_case)]

use crate::b2::contacts2d::ScriptCollision;
use super::{script, Behaviour, Game};
use crate::assets::db::Component;

script! {
    Wrench {
        MinImpulse: f32 = 3.0,
        HealingPower: f32 = 0.2,
        DamagePointHealingPower: f32 = 0.2,
        BulletHoleHealingPower: f32 = 0.2,
        BurnProgressHealingPower: f32 = 0.2,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// Behaviour.enabled
        enabled: bool = true,
        /// PhysicalBehaviour: the collisions it gets are its physical's.
        physical: usize = 0,
        /// The component's index on its GameObject, and whether PhysicalBehaviour comes before it (collision order).
        index: usize = 0,
        after_physical: bool = false,
    }
}

/// The classes with a Repair() method (Messages.IRepair): SendMessageUpwards("Repair") reaches them.
const REPAIR_RECEIVERS: &[&str] = &["CathodeLightBehaviour", "DamagableMachineryBehaviour", "LiquidContainerLeakingBehaviour", "TelevisionBehaviour", "TyreDeflationBehaviour"];

impl Game {
    /// WrenchBehaviour.OnCollisionEnter2D
    pub(super) fn wrench_on_collision_enter(&mut self, w: usize, c: &ScriptCollision) {
        if !self.wrenches[w].enabled {
            return;
        }
        let first = c.contacts.first().unwrap_or_else(|| unimplemented!("WrenchBehaviour.OnCollisionEnter2D: collision.GetContact(0) without contacts (IndexOutOfRangeException)"));
        if first.normal_impulse < self.wrenches[w].MinImpulse {
            return;
        }
        if self.random.value() > 0.75 {
            self.wrench_send_repair_upwards(c.collider);
        }
        // collision.transform.GetComponent<LimbBehaviour>(): an android's limb is healed (not rewritten).
        if let Some(p) = self.physical_of_collider(c.collider) {
            if let Some(l) = self.limbs.iter().position(|l| l.PhysicalBehaviour == p) {
                if self.limbs[l].IsAndroid {
                    unimplemented!("WrenchBehaviour healing an android's LimbBehaviour (HealBone, circulation, person and skin damage points)");
                }
            }
        }
    }

    /// SendMessageUpwards("Repair") from the hit collider's object: each Repair receiver from the object up through its
    /// ancestors, in component order, gets Repair() (disabled ones too). Television, DamagableMachinery and TyreDeflation
    /// are rewritten; CathodeLight and LiquidContainerLeaking stop.
    fn wrench_send_repair_upwards(&mut self, collider: i32) {
        let Some(p) = self.physical_of_collider(collider) else {
            unimplemented!("WrenchBehaviour: SendMessageUpwards Repair from a collider without a PhysicalBehaviour (its ancestors are not checked)");
        };
        let id = self.physicals[p].id;
        let receivers = self.with_component_instance(id, |_, inst, o| {
            let mut out = Vec::new();
            let mut x = Some(o);
            while let Some(i) = x {
                for (k, (_, comp)) in inst.prefab.objects[i].components.iter().enumerate() {
                    if let Component::Script { class, .. } = comp {
                        if REPAIR_RECEIVERS.contains(&class.as_str()) {
                            out.push((class.clone(), inst.objects[i].components[k]));
                        }
                    }
                }
                x = inst.objects[i].parent;
            }
            out
        });
        for (class, cid) in receivers {
            match self.behaviour_of.get(&cid).map(|&(_, b)| b) {
                Some(Behaviour::Television(t)) => self.television_repair(t),
                Some(Behaviour::Machinery(m)) => self.machinery_repair(m),
                Some(Behaviour::Tyre(t)) => self.tyre_repair(t),
                _ => unimplemented!("WrenchBehaviour: SendMessageUpwards Repair to {class}.Repair (not rewritten)"),
            }
        }
    }
}
