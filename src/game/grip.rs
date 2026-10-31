//! GripBehaviour (a hand's grip): a Use picks up the nearest holding point of an object within GripPosition.z, moves the
//! object there and welds it to the hand with a FixedJoint2D; the next Use, the hand's death (DropOnDeath) or the
//! object's destruction drops it, and the two ends collide again once the object is 1 unit away or 15 s passed.
//! OnGripped / OnDrop have no receiver; originalRotation and OnChangePause (Global.Paused is never set) are unused.
#![allow(non_snake_case)]

use super::coroutine::{Routine, Yield};
use super::explosion::DEFAULT_RAYCAST_LAYERS;
use super::{d, mathf, BodyOwner, Game};
use crate::b2::math::Vec2 as V;
use crate::unity::transform::set_world;

impl Game {
    /// `(bool)CurrentlyHolding`: the held PhysicalBehaviour, unless it was destroyed.
    fn grip_held(&self, g: usize) -> Option<usize> {
        self.grips[g].CurrentlyHolding.filter(|&h| !self.physicals[h].destroyed)
    }

    /// GripBehaviour.Start: CollidersToIgnore = transform.root.GetComponentsInChildren<Collider2D>(); a saved grip
    /// holding something attaches again.
    pub fn grip_start(&mut self, g: usize) {
        let id = self.grips[g].id;
        self.grips[g].CollidersToIgnore = self.with_component_instance(id, |_, inst, mut root| {
            while let Some(up) = inst.objects[root].parent {
                root = up;
            }
            super::link::colliders_in_children(inst, root).into_iter().map(|(c, _)| c).collect()
        });
        if self.grips[g].isHolding {
            if let Some(h) = self.grip_held(g) {
                let at = self.grips[g].NearestHoldingPos;
                self.grip_attach(g, h, at);
            }
        }
    }

    /// GripBehaviour.Use
    pub fn grip_use(&mut self, g: usize) {
        if !self.grips[g].isHolding {
            self.grip_pick_up_nearest(g);
        } else {
            self.grip_drop(g);
        }
    }

    /// GripBehaviour.FixedUpdate
    pub fn grip_fixed_update(&mut self, g: usize) {
        if let Some(held) = self.grip_held(g) {
            let me = self.grips[g].PhysicalBehaviour;
            if self.physicals[me].SimulateTemperature && self.physicals[held].SimulateTemperature {
                self.average_temperature(me, held, 0.02);
            }
        }
    }

    /// GripBehaviour.Update
    pub fn grip_update(&mut self, g: usize) {
        if self.grips[g].isHolding && self.grip_held(g).is_none() {
            self.grip_drop(g);
        }
    }

    /// transform.root of physical `p`'s Transform, as its component id.
    fn transform_root_id(&self, p: usize) -> i32 {
        match self.physicals[p].parent {
            None => self.physicals[p].transform_id,
            Some(mut n) => {
                while let Some(up) = self.transform_nodes[n].parent {
                    n = up;
                }
                self.transform_nodes[n].id
            }
        }
    }

    /// PhysicalBehaviour.GetNearestLocalHoldingPoint: the holding position (local, z dropped) nearest the world point and
    /// its squared distance; float.MaxValue and zero without any.
    fn nearest_local_holding_point(&self, v: usize, world: [f32; 2]) -> ([f32; 2], f32) {
        let local = self.inverse_transform_point(v, [world[0], world[1], 0.0]);
        let mut result = [0.0; 2];
        let mut distance = f32::MAX;
        for h in &self.physicals[v].HoldingPositions {
            let sq = mathf::sqr_magnitude([local[0] - h[0], local[1] - h[1]]);
            if !(distance < sq) {
                distance = sq;
                result = [h[0], h[1]];
            }
        }
        (result, distance)
    }

    /// GripBehaviour.PickUpNearestObject
    fn grip_pick_up_nearest(&mut self, g: usize) {
        let me = self.grips[g].PhysicalBehaviour;
        let grip = self.grips[g].GripPosition;
        let point = self.transform_point3(me, grip);
        let world = [point[0], point[1]];
        // OverlapCircleNonAlloc into a 32-element buffer.
        let hits = self.overlap_circle(V::new(world[0], world[1]), grip[2], DEFAULT_RAYCAST_LAYERS);
        if hits.len() > 32 {
            unimplemented!("GripBehaviour.PickUpNearestObject: more colliders than its buffer of 32");
        }
        let root = self.transform_root_id(me);
        let mut nearest = f32::MAX;
        let mut found = None;
        for (physical, _) in hits {
            // PhysicalObjectsInWorldByTransform[collider.transform]: a collider on a PhysicalBehaviour's own Transform.
            let Some(v) = physical.filter(|&v| self.physicals[v].in_world) else { continue };
            if self.transform_root_id(v) == root {
                continue;
            }
            let (at, distance) = self.nearest_local_holding_point(v, world);
            if distance < nearest {
                nearest = distance;
                self.grips[g].NearestHoldingPos = at;
                found = Some(v);
            }
        }
        if let Some(v) = found {
            let at = self.grips[g].NearestHoldingPos;
            self.grip_attach(g, v, at);
        }
    }

    /// GripBehaviour.Attach
    fn grip_attach(&mut self, g: usize, phys: usize, other_local: [f32; 2]) {
        self.grips[g].isHolding = true;
        self.grips[g].CurrentlyHolding = Some(phys);
        let me = self.grips[g].PhysicalBehaviour;
        let a = self.transform_point3(me, self.grips[g].GripPosition);
        let b = self.transform_point3(phys, [other_local[0], other_local[1], 0.0]);
        let at = self.physicals[phys].transform.position;
        self.physical_set_position(phys, [at[0] + (a[0] - b[0]), at[1] + (a[1] - b[1]), at[2] + (a[2] - b[2])]);
        self.grip_create_joint(g, phys, other_local);
        // SendMessage("OnGripped"): no component has one.
    }

    /// GripBehaviour.CreateJoint
    fn grip_create_joint(&mut self, g: usize, phys: usize, other_local: [f32; 2]) {
        let me = self.grips[g].PhysicalBehaviour;
        // AddComponent<FixedJoint2D> takes the component's instance id.
        self.allocate_instance_id();
        let f = self.add_fixed_joint_component(me);
        self.grips[g].joint = Some(f);
        self.fixed_joint_set_connected_body(f, Some(BodyOwner::Physical(phys)));
        let grip = self.grips[g].GripPosition;
        self.fixed_joint_set_anchor(f, [grip[0], grip[1]]);
        self.fixed_joint_set_connected_anchor(f, other_local);
        self.fixed_joint_set_enable_collision(f, false);
        // originalRotation is only read by OnChangePause.
        self.grip_refresh_no_collide(g, true);
    }

    /// GripBehaviour.RefreshNoCollide
    fn grip_refresh_no_collide(&mut self, g: usize, ignore: bool) {
        let Some(held) = self.grip_held(g) else { return };
        let held_colliders = self.with_component_instance(self.physicals[held].id, |_, inst, o| super::link::colliders_in_children(inst, o));
        for (c, _) in held_colliders {
            for item in self.grips[g].CollidersToIgnore.clone() {
                if item != c {
                    self.ignore_collision_substitute(c, item, ignore);
                }
            }
        }
    }

    /// GripBehaviour.DropObject
    pub fn grip_drop(&mut self, g: usize) {
        if !self.grips[g].isHolding {
            return;
        }
        self.grips[g].isHolding = false;
        // Object.Destroy(joint)
        if let Some(f) = self.grips[g].joint.filter(|&f| !self.fixed_joints[f].destroyed) {
            self.destroy_joint_delayed(f);
        }
        if let Some(held) = self.grip_held(g) {
            let (id, me) = (self.grips[g].id, self.grips[g].PhysicalBehaviour);
            let go = self.physicals[me].id;
            self.start_coroutine(id, go, Self::grip_reactivate_routine(g, held));
            self.physicals[held].beingHeldByGripper = false;
            // SendMessage("OnDrop"): no component has one.
            self.grips[g].CurrentlyHolding = None;
        }
    }

    /// GripBehaviour.WaitUntilCollisionReactivate: a second at a time, at most 15, until `other` is further than 1 from
    /// the grip; then the pairs are no longer ignored. A destroyed `other` is a MissingReferenceException, which ends it.
    fn grip_reactivate_routine(g: usize, other: usize) -> Routine {
        let mut seconds = 15;
        let mut started = false;
        Box::new(move |game| {
            if started {
                seconds -= 1;
                if game.physicals[other].destroyed {
                    return Yield::Done;
                }
                if seconds > 0 {
                    let (me, grip) = (game.grips[g].PhysicalBehaviour, game.grips[g].GripPosition);
                    let (a, b) = (game.physicals[other].transform.position, game.transform_point3(me, grip));
                    let (x, y) = (a[0] - b[0], a[1] - b[1]);
                    let distance = (d(x) * x + d(y) * y).0.sqrt() as f32;
                    if !(distance > 1.0) {
                        return Yield::WaitForSeconds(1.0);
                    }
                }
                let colliders = game.with_component_instance(game.physicals[other].id, |_, inst, o| super::link::colliders_in_children(inst, o));
                for (c, _) in colliders {
                    for item in game.grips[g].CollidersToIgnore.clone() {
                        if item != c {
                            game.ignore_collision_substitute(c, item, false);
                        }
                    }
                }
                return Yield::Done;
            }
            started = true;
            Yield::WaitForSeconds(1.0)
        })
    }

    /// Transform.position = pos on a running physical: its Transform and the nested physicals' follow, the clone's
    /// Transforms (stale once a body moved) are refreshed from the live poses first, and the bodies and joints follow
    /// at the next SyncTransforms.
    pub(super) fn physical_set_position(&mut self, p: usize, pos: [f32; 3]) {
        let id = self.physicals[p].id;
        self.with_component_instance(id, |g, inst, o| {
            for x in std::iter::once(o).chain(inst.descendants(o)) {
                let key = inst.objects[x].components[0];
                let Some(q) = g.physicals.iter().position(|ph| ph.transform_id == key && !ph.destroyed) else { continue };
                if g.physicals[q].parent.is_some() != inst.objects[x].parent.is_some() {
                    unimplemented!("Transform.position = on a body re-parented under another clone's Transform");
                }
                let t = &g.physicals[q].transform;
                let (position, rotation) = t.local.unwrap_or_else(|| set_world(&g.parent_chain(q), t.position, t.rotation).0);
                let trs = &mut inst.objects[x].trs;
                trs.t = [position[0], position[1], position[2], trs.t[3]];
                trs.q = rotation;
                trs.s = [t.localScale[0], t.localScale[1], t.localScale[2], trs.s[3]];
            }
            inst.set_world_position(o, pos);
        });
        let chain = self.parent_chain(p);
        let rotation = self.physicals[p].transform.rotation;
        let (local, world) = set_world(&chain, pos, rotation);
        let t = &mut self.physicals[p].transform;
        (t.position, t.rotation, t.local) = (world.0, world.1, Some(local));
        self.refresh_nested(p);
        self.unsync_instance_of(id);
    }

    /// Transform.localScale = s on a live physical: the clone's Transforms take its physicals' live poses (stale once a
    /// body moved; a joint partner's lossy scale reads its rotation), then SetLocalScale flags the body's joints and the
    /// colliders for the next SyncTransforms. The physicals hung below its Transform follow through the new scale.
    pub(super) fn physical_set_local_scale(&mut self, p: usize, s: [f32; 3]) {
        let id = self.physicals[p].id;
        self.with_component_instance(id, |g, inst, o| {
            for x in 0..inst.objects.len() {
                let key = inst.objects[x].components[0];
                let Some(q) = g.physicals.iter().position(|ph| ph.transform_id == key && !ph.destroyed) else { continue };
                if g.physicals[q].parent.is_some() != inst.objects[x].parent.is_some() {
                    unimplemented!("Transform.localScale = on a clone with a body re-parented under another clone's Transform");
                }
                let t = &g.physicals[q].transform;
                let (position, rotation) = t.local.unwrap_or_else(|| set_world(&g.parent_chain(q), t.position, t.rotation).0);
                let trs = &mut inst.objects[x].trs;
                trs.t = [position[0], position[1], position[2], trs.t[3]];
                trs.q = rotation;
                trs.s = [t.localScale[0], t.localScale[1], t.localScale[2], trs.s[3]];
            }
            inst.set_local_scale(o, s);
        });
        self.physicals[p].transform.localScale = s;
        if let Some(n) = self.body_node(p) {
            self.transform_nodes[n].trs.s = [s[0], s[1], s[2], 0.0];
        }
        self.refresh_nested(p);
        self.unsync_instance_of(id);
    }
}
