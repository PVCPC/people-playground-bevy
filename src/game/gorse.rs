//! GorseDroolingBehaviour (the Gorse's spitting head) and GorseProjectileBehaviour (its Gorseball).
#![allow(non_snake_case)]

use super::{d, BareAnchor, Game};
use crate::assets::db::ObjRef;
use crate::b2::contacts2d::{Message, ScriptCollision};
use crate::b2::math::Vec2 as V;
use crate::unity::transform::Trs;

#[derive(Clone, Debug)]
pub struct GorseDrooling {
    /// The component's instance id.
    pub id: i32,
    /// LimbBehaviour
    pub limb: usize,
    /// Where its (bare) Transform hangs: base.transform.position.
    pub anchor: BareAnchor,
    pub chain: Vec<Trs>,
    pub CanSee: u32,
    /// Projectile (None: null).
    pub Projectile: Option<ObjRef>,
    /// FireClip.Length: PickRandom draws over it; the clip itself is audio.
    pub FireClips: usize,
    pub Range: f32,
    pub Force: f32,
    pub step: i32,
    pub isAlive: bool,
}

#[derive(Clone, Debug)]
pub struct GorseProjectile {
    /// The component's instance id.
    pub id: i32,
    pub Damage: f32,
    pub DisperseEffect: Option<ObjRef>,
    pub AcidChance: f32,
    /// The colliders on its own GameObject (the OnCollisionEnter2D receivers).
    pub colliders: Vec<i32>,
    /// Its own Rigidbody2D's node body (transform.position).
    pub node_body: usize,
}

impl Game {
    fn drooling_position(&self, g: usize) -> [f32; 3] {
        let s = &self.gorse_droolings[g];
        self.anchored_world(s.anchor, &s.chain).0
    }

    /// GorseDroolingBehaviour.FixedUpdate (Start only caches the emission module).
    pub fn gorse_drooling_fixed_update(&mut self, g: usize) {
        self.gorse_droolings[g].step += 1;
        if self.gorse_droolings[g].step > 30 {
            let l = self.gorse_droolings[g].limb;
            let alive = self.person_is_alive(self.limbs[l].Person);
            // emission.enabled = isAlive: presentation.
            self.gorse_droolings[g].isAlive = alive;
            if alive && self.limb_is_capable(l) {
                self.gorse_drooling_attack_nearby(g);
            }
        }
    }

    /// The person whose AliveBehaviour.AliveByTransform entry is this collider's transform.root.
    fn alive_of_collider(&self, collider: i32, physical: Option<usize>) -> Option<usize> {
        if let Some(p) = physical {
            return self.person_of_root(p);
        }
        match self.bare_colliders.get(&collider).map(|b| b.anchor) {
            Some(BareAnchor::Physical(p)) => self.person_of_root(p),
            Some(BareAnchor::Node(_)) => unimplemented!("GorseDroolingBehaviour: AliveByTransform of a bare collider under a node"),
            _ => None,
        }
    }

    /// GorseDroolingBehaviour.AttackNearby
    fn gorse_drooling_attack_nearby(&mut self, g: usize) {
        self.gorse_droolings[g].step = 0;
        let position = self.drooling_position(g);
        let s = self.gorse_droolings[g].clone();
        let mut hits = self.overlap_circle(V::new(position[0], position[1]), s.Range, s.CanSee);
        // OverlapCircleNonAlloc into the static 32-entry buffer.
        hits.truncate(32);
        if hits.is_empty() {
            return;
        }
        let (physical, collider) = hits[self.random.range_int(0, hits.len() as i32) as usize];
        let Some(value) = self.alive_of_collider(collider, physical) else { return };
        let own = self.limbs[s.limb].PhysicalBehaviour;
        let own_root = self.root_id(own);
        let value_limb = self.people[value].Limbs[0];
        let value_phys = self.limbs[value_limb].PhysicalBehaviour;
        let value_root = self.root_id(value_phys);
        if self.object_name(value_phys, value_root) == self.object_name(own, own_root) || !self.person_is_alive(value) {
            return;
        }
        let at = self.collider_position(collider);
        let vector = [(d(position[0]) - at[0]).f(), (d(position[1]) - at[1]).f(), (d(position[2]) - at[2]).f()];
        let hit = self.material_aware_raycast([at[0], at[1]], [vector[0], vector[1]], s.Range, s.CanSee, 8, 0.0);
        if hit.physical.is_some_and(|p| self.root_of(p) == self.root_of(own)) {
            let person = self.limbs[s.limb].Person;
            self.person_wince(person, 15.0);
            let n = super::person::normalized(vector);
            let magnitude = (d(vector[0]) * vector[0] + d(vector[1]) * vector[1] + d(vector[2]) * vector[2]).0.sqrt() as f32;
            self.gorse_drooling_fire(g, [-n[0], -n[1], -n[2]], (d(magnitude) * s.Force).f());
        }
    }

    /// GorseDroolingBehaviour.Fire
    fn gorse_drooling_fire(&mut self, g: usize, direction: [f32; 3], force: f32) {
        // PhysicalBehaviour.PlayClipOnce(FireClip.PickRandom()): the draw; the one-shot is audio.
        self.pick_random(self.gorse_droolings[g].FireClips);
        let position = self.drooling_position(g);
        let source = self.gorse_droolings[g].Projectile.unwrap_or_else(|| unimplemented!("GorseDroolingBehaviour: Instantiate(null Projectile) (ArgumentException)"));
        let mut db = self.assets.take().expect("GorseDroolingBehaviour without the game's data");
        let spawned = db.prefab(source).and_then(|prefab| self.spawn(&mut db, &prefab, position, [0.0, 0.0, 0.0, 1.0], None));
        self.assets = Some(db);
        let (inst, linked) = spawned.unwrap_or_else(|e| panic!("GorseDroolingBehaviour: Instantiate: {e}"));
        // gameObject.GetComponent<Rigidbody2D>().AddForce(direction * force, Impulse)
        let k = *linked.node_rigidbodies.get(&0).unwrap_or_else(|| unimplemented!("GorseDroolingBehaviour.Projectile without a bare Rigidbody2D on its root"));
        let rb = self.node_rigidbodies[k].rigidbody.clone();
        rb.add_force_impulse(&mut self.world, [(d(direction[0]) * force).f(), (d(direction[1]) * force).f()]);
        let own = super::link::colliders_in_children(&inst, 0).into_iter().find(|&(_, o)| o == 0).map(|(c, _)| c);
        let own = own.unwrap_or_else(|| unimplemented!("GorseDroolingBehaviour.Projectile without a Collider2D (null in IgnoreCollision)"));
        self.unsynced.push(inst);
        let id = self.gorse_droolings[g].id;
        let root = self.with_component_instance(id, |_, inst, _| super::link::colliders_in_children(inst, 0));
        for (b, _) in root {
            self.ignore_collision_substitute(own, b, true);
        }
    }

    /// OnCollisionEnter2D to a GorseProjectileBehaviour on the collider's GameObject.
    pub fn gorse_projectile_collision(&mut self, collider: i32, message: Message, c: &ScriptCollision) {
        if message != Message::CollisionEnter {
            return;
        }
        if let Some(x) = self.gorse_projectiles.iter().position(|x| x.colliders.contains(&collider) && !self.destroyed_components.contains(&x.id)) {
            self.gorse_projectile_on_collision_enter(x, c);
        }
    }

    /// GorseProjectileBehaviour.OnCollisionEnter2D
    fn gorse_projectile_on_collision_enter(&mut self, x: usize, c: &ScriptCollision) {
        let s = self.gorse_projectiles[x].clone();
        // collision.transform.TryGetComponent<LimbBehaviour>()
        let limb = self.collision_physical(c).and_then(|p| self.limbs.iter().position(|l| l.PhysicalBehaviour == p));
        if let Some(l) = limb {
            self.limb_wince(l, s.Damage);
            self.limb_damage(l, s.Damage);
            if !self.limbs[l].IsAndroid && s.AcidChance > self.random.value() {
                self.add_liquid(super::container::Owner::Circulation(l), super::circulation::liquid("GORSE BLOOD"), 25.0);
                if self.nodes[l].IsConnectedToRoot {
                    let person = self.limbs[l].Person;
                    self.add_pain(person, 50.0);
                }
            }
        }
        let node = self.node_rigidbodies[s.node_body].node;
        let position = self.node_world(node).0;
        let effect = s.DisperseEffect.unwrap_or_else(|| unimplemented!("GorseProjectileBehaviour: Instantiate(null DisperseEffect) (ArgumentException)"));
        self.spawn_prefab(effect, position);
        self.destroy_object(s.id);
    }
}
