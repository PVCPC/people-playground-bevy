//! The soft connections of ChainsawBehaviour (and the other sawing scripts): a Dictionary<PhysicalBehaviour,
//! SoftConnection> in .NET's entry order, SoftConnect's FrictionJoint2D, GetHitPoint, and the Slice and Shot
//! broadcasts to a victim.

use super::explosion::normalized3;
use super::{BodyOwner, Game};
use crate::b2::math::Vec2 as V;

/// SoftConnection: the FrictionJoint2D (`fixed_joints` index), the victim's PhysicalBehaviour and the Collider2D.
#[derive(Clone, Copy, Debug)]
pub struct SoftConnection {
    pub joint: usize,
    pub phys: usize,
    pub coll: i32,
    pub should_be_deleted: bool,
}

/// Dictionary<PhysicalBehaviour, SoftConnection>: the entry array enumerates in slot order, Remove frees a slot and
/// Add takes the most recently freed one before growing (the free list is LIFO).
#[derive(Clone, Debug, Default)]
pub struct SoftConnections {
    entries: Vec<Option<(usize, SoftConnection)>>,
    free: Vec<usize>,
}

impl SoftConnections {
    /// The keys in enumeration order.
    pub fn keys(&self) -> Vec<usize> {
        self.entries.iter().flatten().map(|&(k, _)| k).collect()
    }

    pub fn get(&self, key: usize) -> Option<&SoftConnection> {
        self.entries.iter().flatten().find(|(k, _)| *k == key).map(|(_, v)| v)
    }

    pub fn get_mut(&mut self, key: usize) -> Option<&mut SoftConnection> {
        self.entries.iter_mut().flatten().find(|(k, _)| *k == key).map(|(_, v)| v)
    }

    /// Dictionary.Add (the callers have checked the key is absent).
    pub fn add(&mut self, key: usize, v: SoftConnection) {
        assert!(self.get(key).is_none(), "Dictionary.Add: the key is present (ArgumentException)");
        match self.free.pop() {
            Some(i) => self.entries[i] = Some((key, v)),
            None => self.entries.push(Some((key, v))),
        }
    }

    pub fn remove(&mut self, key: usize) {
        if let Some(i) = self.entries.iter().position(|e| matches!(e, Some((k, _)) if *k == key)) {
            self.entries[i] = None;
            self.free.push(i);
        }
    }
}

impl Game {
    /// `(bool)joint`: the component is not destroyed (Destroy is delayed), and neither is its GameObject's
    /// PhysicalBehaviour.
    pub(super) fn soft_joint_alive(&self, f: usize) -> bool {
        let j = &self.fixed_joints[f];
        !j.destroyed && !matches!(j.attached, BodyOwner::Physical(p) if self.physicals[p].destroyed)
    }

    /// `!coll || !joint || !phys`
    pub(super) fn soft_connection_broken(&self, c: &SoftConnection) -> bool {
        !self.collider_exists(c.coll) || !self.soft_joint_alive(c.joint) || self.physicals[c.phys].destroyed
    }

    /// GetHitPoint: SawCollider.ClosestPoint(coll.ClosestPoint(transform.position)), each with its AutoSyncTransforms.
    pub(super) fn saw_hit_point(&mut self, saw: usize, saw_collider: i32, coll: i32) -> [f32; 2] {
        self.sync_pending();
        let pos = self.physicals[saw].transform.position;
        let p = self.world.collider_closest_point(coll, V::new(pos[0], pos[1]));
        let p = self.world.collider_closest_point(saw_collider, p);
        [p.x, p.y]
    }

    /// (transform.position - (Vector3)hit).normalized: the Shot normal and the push direction, toward the saw.
    pub(super) fn saw_direction(&self, saw: usize, hit: [f32; 2]) -> [f32; 3] {
        let pos = self.physicals[saw].transform.position;
        normalized3([pos[0] - hit[0], pos[1] - hit[1], pos[2] - 0.0])
    }

    /// SoftConnect(otherPhys, coll, connectionStrength): a FrictionJoint2D on the victim's GameObject, connected to
    /// the saw's Rigidbody2D at the hit point, and the victim's collider stops colliding with the saw's. `stab`
    /// (SharpOnAllSidesBehaviour) then sends Stabbed to the victim and Lodged to the blade's GameObject.
    #[allow(clippy::too_many_arguments)]
    /// `hit` is the script's GetHitPoint (and what precedes it).
    pub(super) fn soft_connect(&mut self, connections: &mut SoftConnections, saw: usize, saw_collider: i32, other: usize, coll: i32, strength: f32, stab: bool, hit: &mut dyn FnMut(&mut Game) -> [f32; 2]) {
        if self.physicals[other].destroyed || !self.collider_exists(coll) {
            return;
        }
        let hit = hit(self);
        // AddComponent<FrictionJoint2D>() takes an instance id.
        self.allocate_instance_id();
        let f = self.add_friction_joint_component(other);
        self.fixed_joint_set_auto_configure(f, true);
        self.fixed_joint_set_enable_collision(f, true);
        self.friction_joint_set_max_force(f, strength);
        self.fixed_joint_set_connected_body(f, Some(BodyOwner::Physical(saw)));
        self.friction_joint_set_max_torque(f, strength);
        let a = self.inverse_transform_point(other, [hit[0], hit[1], 0.0]);
        self.fixed_joint_set_anchor(f, [a[0], a[1]]);
        self.ignore_collision_substitute(coll, saw_collider, true);
        if stab {
            // new Stabbing(PhysicalBehaviour, otherPhys, (transform.position - otherPhys.transform.position).normalized, hitPoint)
            let (a, b) = (self.physicals[saw].transform.position, self.physicals[other].transform.position);
            let n = normalized3([a[0] - b[0], a[1] - b[1], a[2] - b[2]]);
            let stabbing = super::stab::Stabbing { stabber: saw, victim: other, normal: [n[0], n[1]], point: hit };
            self.send_stabbed(stabbing);
            self.send_lodged(saw, stabbing);
        }
        connections.add(other, SoftConnection { joint: f, phys: other, coll, should_be_deleted: false });
    }

    /// The deletion loop of ProcessSoftConnections: the first entry marked for deletion collides with the saw again,
    /// its joint is destroyed (delayed) and it leaves the dictionary, until none is marked.
    pub(super) fn soft_connections_prune(&mut self, connections: &mut SoftConnections, saw_collider: i32) {
        loop {
            let Some(key) = connections.keys().into_iter().find(|&k| connections.get(k).unwrap().should_be_deleted) else { break };
            let c = *connections.get(key).unwrap();
            if self.collider_exists(c.coll) {
                self.ignore_collision_substitute(c.coll, saw_collider, false);
            }
            if self.soft_joint_alive(c.joint) {
                self.destroy_joint_delayed(c.joint);
            }
            connections.remove(key);
        }
    }

    /// Whether an active descendant of a physical's GameObject has one of `receivers` (BroadcastMessage reaches them
    /// after the object itself).
    fn descendant_receivers(&mut self, p: usize, receivers: &[&str]) -> bool {
        self.with_component_instance(self.physicals[p].id, |_, inst, o| {
            inst.descendants(o).into_iter().filter(|&x| inst.objects[x].active).any(|x| {
                inst.prefab.objects[x].components.iter().any(|(_, c)| matches!(c, crate::assets::db::Component::Script { class, .. } if receivers.contains(&class.as_str())))
            })
        })
    }

    /// value.BroadcastMessage("Slice", DontRequireReceiver)
    pub(super) fn broadcast_slice(&mut self, p: usize) {
        if self.descendant_receivers(p, super::scifi::SLICE_RECEIVERS) {
            unimplemented!("BroadcastMessage Slice to a descendant with a Slice method");
        }
        self.send_slice(p);
    }

    /// value.BroadcastMessage("Shot", new Shot(normal, point, damage, triggerExplosiveOverride: false) { CanCrush = false })
    pub(super) fn broadcast_saw_shot(&mut self, p: usize, coll: i32, normal: [f32; 2], point: [f32; 2], damage: f32) {
        if self.descendant_receivers(p, super::ballistics::SHOT_RECEIVERS) {
            unimplemented!("BroadcastMessage Shot to a descendant with a Shot method");
        }
        self.send_shot_flags(coll, Some(p), normal, point, damage, false, None, false, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freed_slot_is_reused_lifo() {
        let c = |k| SoftConnection { joint: k, phys: k, coll: 0, should_be_deleted: false };
        let mut d = SoftConnections::default();
        for k in [1, 2, 3, 4] {
            d.add(k, c(k));
        }
        d.remove(2);
        d.remove(4);
        d.add(5, c(5));
        d.add(6, c(6));
        d.add(7, c(7));
        assert_eq!(d.keys(), vec![1, 6, 3, 5, 7]);
    }
}
