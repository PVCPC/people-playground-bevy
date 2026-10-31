//! ExcaliburBehaviour: a Use starts a spell charge (performingSpell, chargeTime = 0; the particles and audio are
//! presentation). Update advances the charge at 2 per second and casts at 1, which instantiates the fireball
//! (CastSpell: not rewritten, see excalibur_cast). Lodged: the Excalibur stabbed into a victim breaks it, or for an
//! android limb sets its acid to at least 0.6 and damages it to death. The glow, curve and sounds are presentation.
#![allow(non_snake_case)]

use super::{mathf, script, Game};

/// Unity's "World" tag (TagManager: index 11 after the builtin 20000 base; poolable 20008 and nitro 20010 agree).
pub(super) const WORLD_TAG: u16 = 20011;

script! {
    Excalibur {}
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// Behaviour.enabled
        enabled: bool = true,
        /// The private performingSpell and chargeTime.
        performingSpell: bool = false,
        chargeTime: f32 = 0.0,
    }
}

impl Game {
    /// ExcaliburBehaviour.Use: starts a charge unless one is running.
    pub fn excalibur_use(&mut self, e: usize) {
        if !self.excaliburs[e].performingSpell {
            self.excaliburs[e].performingSpell = true;
            self.excaliburs[e].chargeTime = 0.0;
        }
    }

    /// ExcaliburBehaviour.Update: while performing, the charge advances by dt * 2 and casts at 1.
    pub fn excalibur_update(&mut self, e: usize) {
        if !self.excaliburs[e].performingSpell {
            return;
        }
        self.excaliburs[e].chargeTime += self.time.deltaTime * 2.0;
        if self.excaliburs[e].chargeTime >= 1.0 {
            self.excalibur_cast(e);
        }
    }

    /// ExcaliburBehaviour.CastSpell: Instantiate(FireBallPrefab) with an impulse up the blade, and IgnoreCollision with
    /// every collider of the Excalibur. The fireball's FireballBehaviour (Ignite, an overlap explosion, a kinematic
    /// freeze) is not rewritten, so the cast stops before it is made.
    fn excalibur_cast(&mut self, _e: usize) {
        unimplemented!("ExcaliburBehaviour.CastSpell: Instantiate(FireBallPrefab) with its FireballBehaviour (PhysicalBehaviour.Ignite, the OverlapCircle explosion, RigidbodyConstraints2D.FreezeAll), which is not rewritten");
    }

    /// ExcaliburBehaviour.Lodged(stabbing) with the victim: `if (victim && !victim.transform.root.CompareTag("World"))`,
    /// then a DestroyableBehaviour breaks, a DamagableMachinery force-breaks, and an android's limb burns (acid 0.6 at
    /// least) and takes Health + 1 damage; anything else gets SendMessage("Break", Vector2.zero).
    pub(super) fn excalibur_lodged(&mut self, victim: usize) {
        let root = self.root_id(victim);
        let root_tag = self.with_component_instance(root, |_, inst, o| inst.prefab.objects[o].tag);
        if root_tag == WORLD_TAG {
            return;
        }
        if let Some(x) = self.destroyables.iter().position(|d| d.PhysicalBehaviour == victim) {
            self.destroyable_break(x, [0.0, 0.0]);
        }
        if let Some(m) = self.machinery.iter().position(|m| m.PhysicalBehaviour == victim) {
            self.machinery_force_break(m);
        }
        match self.limbs.iter().position(|l| l.PhysicalBehaviour == victim) {
            Some(l) if self.limbs[l].IsAndroid => {
                let acid = mathf::max(self.skins[l].acidProgress, 0.6);
                self.skins[l].set_acid(acid);
                let damage = self.limbs[l].Health + 1.0;
                self.limb_damage(l, damage);
            }
            _ => {
                // SendMessage("Break", Vector2.zero) to the victim's GameObject (its collider only matters for a bare one).
                let collider = self.physicals[victim].all_colliders.first().copied().unwrap_or(0);
                self.send_break(collider, Some(victim), [0.0, 0.0]);
            }
        }
    }
}
