//! PointEffector2D (UnityPlayer's Effector2D, 2020.3): the Behaviour on a trigger collider that pushes the bodies
//! touching it (the contact array lives in `World::effectors`, kept by the b2 contact listener), run by
//! PhysicsScene2D::UpdateEffectors (180c24c70) after the b2 step and before UpdateJoints.

use super::spawn::Instance;
use super::Game;
use crate::assets::components::PointEffector2D as PointEffectorData;
use crate::assets::db::{Component, Prefab};
use crate::b2::contacts2d::Effector2D;
use crate::b2::world::{BodyType, BODY_AWAKE, BODY_FIXED_ROTATION};

/// A PointEffector2D component (a Behaviour: +0x38 enabled, +0x39 added to the manager).
#[derive(Clone, Debug)]
pub struct PointEffector {
    /// The component's instance id.
    pub id: i32,
    /// Its object in its clone.
    pub object: usize,
    /// Behaviour.enabled
    pub enabled: bool,
    /// Behaviour::m_IsAdded: the effector is in the scene's UpdateEffectors list.
    pub added: bool,
    /// forceMagnitude (+0x98), forceVariation (+0x9c), distanceScale (+0xa0), drag (+0xa4), angularDrag (+0xa8).
    pub force_magnitude: f32,
    pub force_variation: f32,
    pub distance_scale: f32,
    pub drag: f32,
    pub angular_drag: f32,
    /// forceSource (+0xac) and forceTarget (+0xb0): EffectorSelection2D Rigidbody 0, Collider 1; forceMode (+0xb4):
    /// Constant 0, InverseLinear 1, InverseSquared 2.
    pub force_source: i32,
    pub force_target: i32,
    pub force_mode: i32,
    /// The xorshift128 state (+0x88..+0x94) forceVariation draws from.
    pub rng: [u32; 4],
}

/// The setters' clamp: lower bound, upper bound, else the value (a NaN passes through).
fn clamp(lo: f32, hi: f32, v: f32) -> f32 {
    if lo > v {
        lo
    } else if hi < v {
        hi
    } else {
        v
    }
}

impl Game {
    fn point_effector(&self, id: i32) -> usize {
        self.point_effectors.iter().position(|e| e.id == id).unwrap_or_else(|| panic!("component {id}: not a PointEffector2D"))
    }

    /// The Effector2D of `o` as Collider2D::AwakeFromLoad finds it (GetComponent<Effector2D>, +0xc8): its component id.
    pub(super) fn object_effector(inst: &Instance, prefab: &Prefab, o: usize) -> Option<i32> {
        let mut found = None;
        for (c, (_, comp)) in prefab.objects[o].components.iter().enumerate() {
            match comp {
                Component::PointEffector2D(_) => {
                    found = found.or(Some(inst.objects[o].components[c]));
                }
                // The other Effector2D classes (Area 249, Platform 251, Surface 252, Buoyancy 253) are not rewritten.
                Component::Other(249 | 251..=253) => unimplemented!("{}: a Collider2D used by an Area/Platform/Surface/BuoyancyEffector2D", prefab.objects[o].name),
                _ => {}
            }
        }
        found
    }

    /// PointEffector2D construction from the prefab: Effector2D::Effector2D, the PointEffector2D produce function
    /// (180c05290) with its generator's seed, then the serialized fields.
    pub(super) fn point_effector_create(&mut self, inst: &Instance, o: usize, c: usize, e: &PointEffectorData) {
        let id = inst.objects[o].components[c];
        self.world.effectors.insert(id, Effector2D { use_collider_mask: e.use_collider_mask, collider_mask: e.collider_mask, contacts: Vec::new() });
        self.point_effectors.push(PointEffector {
            id,
            object: o,
            enabled: e.enabled,
            added: false,
            force_magnitude: e.force_magnitude,
            force_variation: e.force_variation,
            distance_scale: e.distance_scale,
            drag: e.drag,
            angular_drag: e.angular_drag,
            force_source: e.force_source,
            force_target: e.force_target,
            force_mode: e.force_mode,
            rng: [0, 1, 0x6c078966, 0x714acb3f],
        });
    }

    /// Behaviour::AwakeFromLoad (180632400) then Effector2D::AwakeFromLoad (180bfba30) in the instantiate and activate
    /// modes: AddToManager (180bfb9b0) on an active, enabled effector, and RecalculateContacts.
    pub(super) fn point_effector_awake(&mut self, inst: &Instance, prefab: &Prefab, o: usize, c: usize) {
        let e = self.point_effector(inst.objects[o].components[c]);
        let wanted = inst.objects[o].active && self.point_effectors[e].enabled;
        self.point_effector_set_added(e, wanted);
        self.effector_recalculate_contacts(inst, prefab, e);
    }

    /// Behaviour::Deactivate (180633d30): RemoveFromManager (180bfd8c0) takes the effector off the update list.
    pub(super) fn point_effector_deactivate(&mut self, inst: &Instance, o: usize, c: usize) {
        let e = self.point_effector(inst.objects[o].components[c]);
        self.point_effector_set_added(e, false);
    }

    /// The m_IsAdded flip of Behaviour::AwakeFromLoad / SetEnabled / Deactivate: AddToManager appends the effector to
    /// the scene's list (PhysicsScene2D::AddEffectorUpdates, 180c1bf30), RemoveFromManager unlinks it.
    fn point_effector_set_added(&mut self, e: usize, wanted: bool) {
        if self.point_effectors[e].added == wanted {
            return;
        }
        self.point_effectors[e].added = wanted;
        if wanted {
            self.effector_updates.push(e);
        } else {
            self.effector_updates.retain(|&x| x != e);
        }
    }

    /// Effector2D::RecalculateContacts (180bfd750): an enabled effector refilters the colliders of its GameObject that
    /// are used by an effector, enabled and have shapes, then wakes every body in its contact array (WakeAllContacts,
    /// 180bfede0).
    fn effector_recalculate_contacts(&mut self, inst: &Instance, prefab: &Prefab, e: usize) {
        if !self.point_effectors[e].enabled {
            return;
        }
        let (id, o) = (self.point_effectors[e].id, self.point_effectors[e].object);
        let mut colliders = Vec::new();
        for (c, (_, comp)) in prefab.objects[o].components.iter().enumerate() {
            let Component::Collider2D(col) = comp else { continue };
            if col.used_by_effector && inst.fixtures.get(&(o, c)).is_some_and(|f| f.iter().any(|&f| self.world.fixture_alive(f))) {
                colliders.push(inst.objects[o].components[c]);
            }
        }
        if colliders.is_empty() {
            return;
        }
        for collider in colliders {
            self.world.refilter_collider(collider);
        }
        for other in self.world.effectors[&id].contacts.iter().map(|c| c.other_body).collect::<Vec<_>>() {
            self.world.body_mut(other).wake();
        }
    }

    /// PhysicsScene2D::UpdateEffectors (180c24c70): every listed effector with contacts runs its ContactUpdateCallback.
    pub fn update_effectors(&mut self, dt: f32) {
        for k in 0..self.effector_updates.len() {
            let e = self.effector_updates[k];
            if !self.world.effectors[&self.point_effectors[e].id].contacts.is_empty() {
                self.point_effector_contact_update(e, dt);
            }
        }
    }

    /// PointEffector2D::ContactUpdateCallback (180c039d0): the force draw, the contacts grouped by the other body, and
    /// for each dynamic body its averaged force at the averaged point, then drag and angular drag.
    fn point_effector_contact_update(&mut self, e: usize, dt: f32) {
        let p = self.point_effectors[e].clone();
        // The xorshift128 step.
        let [x, y, z, w] = p.rng;
        let t = x ^ (x << 11);
        let nw = (w ^ (w >> 19)) ^ (t ^ (t >> 8));
        self.point_effectors[e].rng = [y, z, w, nw];
        let force = (nw & 0x7fffff) as f32 * f32::from_bits(0x34000001) * p.force_variation + p.force_magnitude;
        // std::sort with PredecateContactGrouping: by the other body, then its fixture (their addresses; the
        // rewrite's creation order stands in for them). The array keeps the sorted order.
        // ponytail: creation order for heap addresses; only the order of the force sums within a group can differ.
        let mut contacts = std::mem::take(&mut self.world.effectors.get_mut(&p.id).unwrap().contacts);
        contacts.sort_by_key(|c| (c.other_body, c.other_fixture, c.other_child));
        self.world.effectors.get_mut(&p.id).unwrap().contacts = contacts.clone();
        // A Rigidbody source is the first contact's body position, or its GameObject's Transform without a Rigidbody2D.
        let (mut sx, mut sy) = (0.0f32, 0.0f32);
        if p.force_source == 0 {
            let body = self.world.body(contacts[0].body);
            if body.user.is_some() {
                (sx, sy) = (body.xf.p.x, body.xf.p.y);
            } else {
                let pos = self.with_component_instance(p.id, |_, inst, o| inst.position(o));
                (sx, sy) = (pos[0], pos[1]);
            }
        }
        let center = |g: &Game, fixture: usize, body: usize, child: usize| {
            let aabb = g.world.fixture(fixture).shape.compute_aabb(&g.world.body(body).xf, child);
            ((aabb.upper.x + aabb.lower.x) * 0.5, (aabb.upper.y + aabb.lower.y) * 0.5)
        };
        let mut i = 0;
        while i < contacts.len() {
            let mut end = i;
            while end + 1 < contacts.len() && contacts[end + 1].other_body == contacts[i].other_body {
                end += 1;
            }
            let group = &contacts[i..=end];
            i = end + 1;
            let other = group[0].other_body;
            if self.world.body(other).kind != BodyType::Dynamic {
                continue;
            }
            let (mut px, mut py, mut fx, mut fy) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for c in group {
                if p.force_source != 0 {
                    (sx, sy) = center(self, c.fixture, c.body, c.child);
                }
                let (tx, ty) = if p.force_target == 0 {
                    let b = self.world.body(other);
                    (b.sweep.c.x, b.sweep.c.y)
                } else {
                    center(self, c.other_fixture, other, c.other_child)
                };
                px += tx;
                py += ty;
                let (mut dx, mut dy) = (tx - sx, ty - sy);
                let mut len = (dx * dx + dy * dy).sqrt();
                if 1.1920928955078125e-07 > len {
                    len = 0.0;
                } else {
                    let inv = 1.0 / len;
                    dx *= inv;
                    dy *= inv;
                }
                len *= p.distance_scale;
                len = if len > 1.0 { len } else { 1.0 };
                match p.force_mode {
                    0 => {
                        dx *= force;
                        dy *= force;
                    }
                    1 => {
                        let k = force / len;
                        dx *= k;
                        dy *= k;
                    }
                    2 => {
                        let k = force / (len * len);
                        dx *= k;
                        dy *= k;
                    }
                    _ => {}
                }
                fx += dx;
                fy += dy;
            }
            let inv = 1.0 / group.len() as f32;
            (px, py, fx, fy) = (px * inv, py * inv, fx * inv, fy * inv);
            let b = self.world.body_mut(other);
            // b2Body::SetAwake(true), then ApplyForce(force, point).
            if b.flags & BODY_AWAKE == 0 {
                b.flags |= BODY_AWAKE;
                b.sleep_time = 0.0;
            }
            b.force.x = fx + b.force.x;
            b.force.y = fy + b.force.y;
            b.torque = fy * (px - b.sweep.c.x) - fx * (py - b.sweep.c.y) + b.torque;
            if 0.0 < p.drag {
                let k = 1.0 / (p.drag * dt + 1.0);
                let v = (b.linear_velocity.x * k, b.linear_velocity.y * k);
                if 0.0 < v.0 * v.0 + v.1 * v.1 {
                    b.flags |= BODY_AWAKE;
                    b.sleep_time = 0.0;
                }
                (b.linear_velocity.x, b.linear_velocity.y) = v;
            }
            if 0.0 < p.angular_drag && b.flags & BODY_FIXED_ROTATION == 0 {
                let w = 1.0 / (p.angular_drag * dt + 1.0) * b.angular_velocity;
                if 0.0 < w * w {
                    b.flags |= BODY_AWAKE;
                    b.sleep_time = 0.0;
                }
                b.angular_velocity = w;
            }
        }
    }

    /// Behaviour.enabled of a PointEffector2D.
    pub fn effector_enabled(&self, id: i32) -> bool {
        self.point_effectors[self.point_effector(id)].enabled
    }

    /// Behaviour::SetEnabled (180634d50): a change on an active GameObject adds the effector to or removes it from the
    /// update list.
    pub fn effector_set_enabled(&mut self, id: i32, v: bool) {
        let e = self.point_effector(id);
        if self.point_effectors[e].enabled == v {
            return;
        }
        self.point_effectors[e].enabled = v;
        let active = self.with_component_instance(id, |_, inst, o| inst.objects[o].active);
        self.point_effector_set_added(e, active && v);
    }

    /// PointEffector2D.forceMagnitude (+0x98).
    pub fn effector_force_magnitude(&self, id: i32) -> f32 {
        self.point_effectors[self.point_effector(id)].force_magnitude
    }

    /// PointEffector2D::SetForceMagnitude (180bfdd40): clamped to [-1e6, 1e6].
    pub fn effector_set_force_magnitude(&mut self, id: i32, v: f32) {
        let e = self.point_effector(id);
        self.point_effectors[e].force_magnitude = clamp(-1e6, 1e6, v);
    }

    pub fn effector_force_variation(&self, id: i32) -> f32 {
        self.point_effectors[self.point_effector(id)].force_variation
    }

    /// PointEffector2D::SetForceVariation (180c05f00): clamped to [-1e6, 1e6].
    pub fn effector_set_force_variation(&mut self, id: i32, v: f32) {
        let e = self.point_effector(id);
        self.point_effectors[e].force_variation = clamp(-1e6, 1e6, v);
    }

    pub fn effector_distance_scale(&self, id: i32) -> f32 {
        self.point_effectors[self.point_effector(id)].distance_scale
    }

    /// PointEffector2D::SetDistanceScale (180c05d50): clamped to [0.0001, 1e6].
    pub fn effector_set_distance_scale(&mut self, id: i32, v: f32) {
        let e = self.point_effector(id);
        self.point_effectors[e].distance_scale = clamp(0.0001, 1e6, v);
    }

    pub fn effector_drag(&self, id: i32) -> f32 {
        self.point_effectors[self.point_effector(id)].drag
    }

    /// PointEffector2D::SetDrag (180bfda00): clamped to [0, 1e6].
    pub fn effector_set_drag(&mut self, id: i32, v: f32) {
        let e = self.point_effector(id);
        self.point_effectors[e].drag = clamp(0.0, 1e6, v);
    }

    pub fn effector_angular_drag(&self, id: i32) -> f32 {
        self.point_effectors[self.point_effector(id)].angular_drag
    }

    /// PointEffector2D::SetAngularDrag (180c05910): clamped to [0, 1e6].
    pub fn effector_set_angular_drag(&mut self, id: i32, v: f32) {
        let e = self.point_effector(id);
        self.point_effectors[e].angular_drag = clamp(0.0, 1e6, v);
    }

    pub fn effector_force_source(&self, id: i32) -> i32 {
        self.point_effectors[self.point_effector(id)].force_source
    }

    /// PointEffector2D::SetForceSource (1805eb180): stored as given.
    pub fn effector_set_force_source(&mut self, id: i32, v: i32) {
        let e = self.point_effector(id);
        self.point_effectors[e].force_source = v;
    }

    pub fn effector_force_target(&self, id: i32) -> i32 {
        self.point_effectors[self.point_effector(id)].force_target
    }

    /// PointEffector2D::SetForceTarget (180c05f60): stored as given.
    pub fn effector_set_force_target(&mut self, id: i32, v: i32) {
        let e = self.point_effector(id);
        self.point_effectors[e].force_target = v;
    }

    pub fn effector_force_mode(&self, id: i32) -> i32 {
        self.point_effectors[self.point_effector(id)].force_mode
    }

    /// PointEffector2D::SetForceMode (180c05f30): stored as given.
    pub fn effector_set_force_mode(&mut self, id: i32, v: i32) {
        let e = self.point_effector(id);
        self.point_effectors[e].force_mode = v;
    }

    /// Effector2D.colliderMask (+0x64).
    pub fn effector_collider_mask(&self, id: i32) -> i32 {
        self.world.effectors[&id].collider_mask
    }

    /// Effector2D.useColliderMask (+0x60).
    pub fn effector_use_collider_mask(&self, id: i32) -> bool {
        self.world.effectors[&id].use_collider_mask
    }

    /// Effector2D::SetColliderMask (180bfda40): stored, then RecalculateContacts.
    pub fn effector_set_collider_mask(&mut self, id: i32, v: i32) {
        self.world.effectors.get_mut(&id).unwrap().collider_mask = v;
        self.effector_recalculate_contacts_of(id);
    }

    /// Effector2D::SetUseColliderMask (180bfdd70): stored, then RecalculateContacts.
    pub fn effector_set_use_collider_mask(&mut self, id: i32, v: bool) {
        self.world.effectors.get_mut(&id).unwrap().use_collider_mask = v;
        self.effector_recalculate_contacts_of(id);
    }

    fn effector_recalculate_contacts_of(&mut self, id: i32) {
        let e = self.point_effector(id);
        self.with_component_instance(id, |g, inst, _| {
            let prefab = inst.prefab.clone();
            g.effector_recalculate_contacts(inst, &prefab, e);
        });
    }

    /// GetComponent<Collider2D>() on the GameObject of component `id` (an effector, say): the first Collider2D in
    /// component order, as its instance id.
    pub fn collider2d_of(&mut self, id: i32) -> Option<i32> {
        self.with_component_instance(id, |_, inst, o| {
            let c = inst.prefab.objects[o].components.iter().position(|(_, c)| matches!(c, Component::Collider2D(_)))?;
            Some(inst.objects[o].components[c])
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::b2::math::Vec2;
    use crate::b2::shape::{box_corners, Polygon};
    use crate::b2::world::{BodyDef, ColliderRef, Filter, FixtureDef, RigidbodyRef, Shape};

    /// A static effector body with a sensor box (collider 1) and a dynamic crate (collider 2, layer `layer`) inside it.
    fn setup(mask: i32, layer: i32, mode: i32) -> Game {
        let mut g = Game::default();
        g.world.gravity = Vec2::ZERO;
        g.world.layers.extend([(10, 0), (11, layer)]);
        let fixture = |size: f32, sensor: bool, collider: ColliderRef| FixtureDef {
            shape: Shape::Polygon(Polygon::from_points(&box_corners(Vec2::ZERO, Vec2::new(size, size)), crate::b2::settings::globals().polygon_radius)),
            friction: 0.4,
            restitution: 0.0,
            density: 1.0,
            is_sensor: sensor,
            filter: Filter::default(),
            collider,
        };
        let source = g.world.create_body(&BodyDef { user: Some(RigidbodyRef { id: 5, game_object: 10 }), ..BodyDef::default() });
        g.world.create_fixture(source, &fixture(3.0, true, ColliderRef { id: 1, game_object: 10 }), true);
        let target = g.world.create_body(&BodyDef { kind: crate::b2::world::BodyType::Dynamic, position: Vec2::new(1.0, 0.0), user: Some(RigidbodyRef { id: 6, game_object: 11 }), ..BodyDef::default() });
        g.world.create_fixture(target, &fixture(0.5, false, ColliderRef { id: 2, game_object: 11 }), true);
        g.world.effector_colliders.insert(1, Some(100));
        g.world.effectors.insert(100, Effector2D { use_collider_mask: true, collider_mask: mask, contacts: Vec::new() });
        g.point_effectors.push(PointEffector {
            id: 100,
            object: 0,
            enabled: true,
            added: true,
            force_magnitude: 10.0,
            force_variation: 0.0,
            distance_scale: 1.0,
            drag: 0.0,
            angular_drag: 0.0,
            force_source: 0,
            force_target: 0,
            force_mode: mode,
            rng: [0, 1, 0x6c078966, 0x714acb3f],
        });
        g.effector_updates.push(0);
        g
    }

    #[test]
    fn pushes_a_body_inside_the_trigger_away_from_the_source() {
        let mut g = setup(-1, 3, 0);
        g.world.step(0.02, 8, 3);
        assert_eq!(g.world.effectors[&100].contacts.len(), 1);
        g.update_effectors(0.02);
        let b = g.world.body(1);
        // Constant mode: the unit direction (1, 0) times forceMagnitude, at the centre of mass (no torque).
        assert_eq!((b.force.x, b.force.y, b.torque), (10.0, 0.0, 0.0));
    }

    #[test]
    fn inverse_squared_falls_off_with_distance() {
        let mut g = setup(-1, 3, 2);
        // The distance 1 times distanceScale 2: force / 2^2.
        g.point_effectors[0].distance_scale = 2.0;
        g.world.step(0.02, 8, 3);
        g.update_effectors(0.02);
        assert_eq!(g.world.body(1).force.x, 2.5);
        // Inside distance 1 the divisor stays 1.
        g.world.body_mut(1).force.x = 0.0;
        g.point_effectors[0].distance_scale = 0.5;
        g.update_effectors(0.02);
        assert_eq!(g.world.body(1).force.x, 10.0);
    }

    #[test]
    fn collider_mask_leaves_out_a_layer() {
        let mut g = setup(!(1 << 3), 3, 0);
        g.world.step(0.02, 8, 3);
        assert!(g.world.effectors[&100].contacts.is_empty());
        g.update_effectors(0.02);
        assert_eq!(g.world.body(1).force.x, 0.0);
    }

    #[test]
    fn variation_draws_from_the_xorshift_generator() {
        let mut g = setup(-1, 3, 0);
        g.point_effectors[0].force_variation = 4.0;
        g.world.step(0.02, 8, 3);
        g.update_effectors(0.02);
        // x = 0, y = 1, z = 0x6c078966, w = 0x714acb3f: t = 0, w' = w ^ (w >> 19) ^ 0.
        let w = 0x714acb3fu32 ^ (0x714acb3fu32 >> 19);
        let expected = (w & 0x7fffff) as f32 * f32::from_bits(0x34000001) * 4.0 + 10.0;
        assert_eq!(g.world.body(1).force.x, expected);
        assert_eq!(g.point_effectors[0].rng, [1, 0x6c078966, 0x714acb3f, w]);
    }
}
