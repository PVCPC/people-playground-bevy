//! ActOnCollide and PointToVelocityBehaviour.
#![allow(non_snake_case)]

use super::physical::get_average_impulse_remove_outliers;
use super::{d, mathf, script, Game};
use crate::b2::contacts2d::ScriptCollision;

script! {
    ActOnCollide {
        ImpactForceThreshold: f32 = 0.0,
        DispatchChance: f32 = 1.0,
    }
    links {
        /// Collider: the Collider2D's instance id; None allows any.
        Collider: Option<i32> = None,
        /// GetComponent<PhysicalBehaviour>() (the collision messages reach the object's physical).
        physical: Option<usize> = None,
        enabled: bool = true,
        /// Actions: the ExplosiveBehaviours (`Game::explosives`) it activates.
        targets: Vec<usize> = Vec::new(),
        after_physical: bool = true,
        /// The component's index on its GameObject.
        index: usize = 0,
    }
}

script! {
    PointToVelocity {
        intensity: f32 = 1.0,
        direction: [f32; 2] = [1.0, 0.0],
    }
    links {
        /// The physical whose Rigidbody2D is GetComponent<Rigidbody2D>().
        physical: usize = 0,
    }
}

/// A script that takes a physical object's collision messages besides the PhysicalBehaviour and LimbBehaviour.
#[derive(Clone, Copy, Debug)]
pub enum CollisionScript {
    Destroyable(usize),
    Explosive(usize),
    ActOnCollide(usize),
    Sticky(usize),
}

impl Game {
    pub(super) fn collision_enter_script(&mut self, s: CollisionScript, c: &ScriptCollision) {
        match s {
            CollisionScript::Destroyable(x) => self.destroyable_on_collision(x, c),
            CollisionScript::Explosive(e) => self.explosive_on_collision_enter(e, c),
            CollisionScript::ActOnCollide(a) => self.act_on_collide_enter(a, c),
            CollisionScript::Sticky(s) => self.sticky_grenade_collision_enter(s, c),
        }
    }

    pub(super) fn collision_stay_script(&mut self, s: CollisionScript, c: &ScriptCollision) {
        if let CollisionScript::Destroyable(x) = s {
            self.destroyable_on_collision(x, c);
        }
    }

    /// ActOnCollide.OnCollisionEnter2D: GetContacts fills the 4-entry buffer.
    fn act_on_collide_enter(&mut self, a: usize, c: &ScriptCollision) {
        let s = &self.act_on_collides[a];
        if !s.enabled || self.random.value() > s.DispatchChance {
            return;
        }
        let mut buffer = self.physical_contact_buffer;
        let contacts = c.contacts.len().min(4);
        buffer[..contacts].copy_from_slice(&c.contacts[..contacts]);
        let s = &self.act_on_collides[a];
        if s.Collider.is_none_or(|x| c.other_collider == x) && get_average_impulse_remove_outliers(&buffer, contacts) > s.ImpactForceThreshold {
            for e in s.targets.clone() {
                self.explosive_activate(e);
            }
        }
    }

    /// PointToVelocityBehaviour.FixedUpdate
    pub fn point_to_velocity_fixed_update(&mut self, i: usize) {
        let s = &self.point_to_velocities[i];
        let p = s.physical;
        let rb = self.physicals[p].rigidbody.clone();
        if rb.bodyType != 0 {
            return;
        }
        if self.physicals[p].parent.is_some() {
            unimplemented!("PointToVelocityBehaviour: TransformVector of a parented object");
        }
        let vector = self.transform_vector(p, s.direction);
        let velocity = rb.velocity(&self.world);
        let intensity = s.intensity;
        let atan2_degrees = |y: f32, x: f32| (d((y as f64).atan2(x as f64) as f32) * 57.29578f32).f();
        let current = atan2_degrees(vector[1], vector[0]);
        let target = atan2_degrees(velocity[1], velocity[0]);
        let num = mathf::delta_angle(current, target);
        if num.abs() < 25.0 {
            let av = rb.angular_velocity(&self.world);
            rb.set_angular_velocity(&mut self.world, (d(av) * mathf::clamp(num / 25.0, 0.95, 1.0)).f());
        }
        rb.add_torque(&mut self.world, (d(num) * intensity * mathf::magnitude2(velocity)).f());
    }
}

/// NoCollide: the Collider2D instance ids of its two sets (None for a null entry).
#[derive(Clone, Debug)]
pub struct NoCollide {
    pub id: i32,
    pub a: Vec<Option<i32>>,
    pub b: Vec<Option<i32>>,
    pub bis_a: bool,
}

impl Game {
    /// NoCollide.Start
    pub fn no_collide_start(&mut self, id: i32) {
        let n = self.no_collides.iter().find(|n| n.id == id).expect("a NoCollide").clone();
        let other = if n.bis_a { &n.a } else { &n.b };
        for &a in n.a.iter().flatten() {
            for &b in other.iter().flatten() {
                if a != b {
                    self.ignore_collision_substitute(a, b, true);
                }
            }
        }
    }

    /// NoCollide.OnDestroy
    pub fn no_collide_destroy(&mut self, id: i32) {
        let n = self.no_collides.iter().find(|n| n.id == id).expect("a NoCollide").clone();
        for &a in n.a.iter().flatten() {
            for &b in n.b.iter().flatten() {
                self.ignore_collision_substitute(a, b, false);
            }
        }
    }
}

script! {
    StickyGrenade {
        LightInterval: f32 = 1.0,
    }
    links {
        physical: Option<usize> = None,
        /// Explosive: `Game::explosives` index.
        explosive: usize = 0,
        /// unstickyPropertySet: `Game::properties` indices.
        unsticky: Vec<usize> = Vec::new(),
        after_physical: bool = true,
        /// The component's index on its GameObject.
        index: usize = 0,
    }
}

impl Game {
    /// StickyGrenadeBehaviour.OnCollisionEnter2D. The state it keeps (isStuck, joint) only changes in CreateJoint, and
    /// FixedUpdate and OnJointBreak2D do nothing until then.
    pub(super) fn sticky_grenade_collision_enter(&mut self, s: usize, c: &ScriptCollision) {
        let g = &self.sticky_grenades[s];
        if !self.explosives[g.explosive].Armed {
            return;
        }
        // Global.PhysicalObjectsInWorldByTransform[collision.transform]: the other body's transform, else the collider's.
        let phys = match c.rigidbody {
            Some(rb) => self.physicals.iter().position(|p| !p.destroyed && p.in_world && p.rigidbody.component == rb),
            None => self.physical_of_collider(c.collider).filter(|&p| self.physicals[p].in_world),
        };
        if let Some(p) = phys.filter(|&p| !g.unsticky.contains(&self.physicals[p].Properties)) {
            let _ = p;
            unimplemented!("StickyGrenadeBehaviour.CreateJoint: GameObject.layer = Debris (Collider2D re-filter) and a runtime FixedJoint2D");
        }
    }
}
