//! Transform.SetParent between two spawned clones' Rigidbody2D objects, with and without worldPositionStays.
//!
//! A prefab's Transform hierarchy is fixed at spawn: `Physical::parent` names the `transform_nodes` entry above a body, a
//! `node_bodies` entry is a physical's own Transform, and `driven_by` the body of the nearest ancestor Rigidbody2D.
//! Re-parenting a live body under another physical's Transform (an attachment under its firearm) moves the same pieces
//! at run time: the parent becomes a node, the child's `parent`/local pose/local scale follow, and its clone stays a
//! root whose Transform is refreshed to the world pose (the clones' Transforms are stale once a body moved). The
//! child's body keeps simulating on its own; the parent only drives its Transform (`write_back_transforms`), and the
//! child follows the parent's SetActive and Destroy through `attached_clones`.
use super::destroyable::{AttachParent, AttachedClone};
use super::{BodyOwner, Game};
use crate::unity::transform::{set_world, Node, Trs};

impl Game {
    /// The clone holding component `id` is edited: it joins the unsynced list so the next sync sees its changes.
    pub(super) fn unsync_instance_of(&mut self, id: i32) {
        if let Some(i) = self.retained.iter().position(|i| i.objects.iter().any(|x| x.components.contains(&id))) {
            let inst = self.retained.remove(i);
            self.unsynced.push(inst);
        }
    }

    /// The `transform_nodes` entry that is physical `p`'s own Transform; a physical nothing hung below until now gets
    /// one (its local pose is the physical's, its scale the local scale it has now).
    pub(super) fn ensure_body_node(&mut self, p: usize) -> usize {
        if let Some(n) = self.body_node(p) {
            return n;
        }
        let ph = &self.physicals[p];
        let (position, rotation, s, id) = (ph.transform.position, ph.transform.rotation, ph.transform.localScale, ph.transform_id);
        let local = ph.transform.local.unwrap_or_else(|| set_world(&self.parent_chain(p), position, rotation).0);
        let parent = ph.parent;
        let n = self.transform_nodes.len();
        self.transform_nodes.push(Node { id, trs: Trs { t: [local.0[0], local.0[1], local.0[2], 0.0], q: local.1, s: [s[0], s[1], s[2], 0.0] }, parent });
        self.node_bodies.insert(n, p);
        self.physicals[p].transform.local = Some(local);
        n
    }

    /// Transform.SetParent(parent's Transform) (worldPositionStays) of physical `child`'s root object under physical
    /// `parent`'s: the world pose and lossy scale stay, so the local pose is the inverse of the parent chain applied to
    /// it and the local scale the lossy scale over the parent's. A Rigidbody2D that is not static is then driven by the
    /// parent's body (Rigidbody2D::CheckForDrivenByParentRigidbody) and its interpolation restarts.
    pub(super) fn physical_set_parent(&mut self, child: usize, parent: usize) {
        if self.physicals[child].parent.is_some() {
            unimplemented!("Transform.SetParent of an object that already has a parent Transform");
        }
        if self.physicals[parent].rigidbody.detached.is_some() {
            unimplemented!("Transform.SetParent under an object whose Rigidbody2D has no body (CheckForDrivenByParentRigidbody)");
        }
        let id = self.physicals[child].id;
        let live = self.physicals[child].transform;
        let scale = self.lossy_scale(child);
        let parent_scale = self.lossy_scale(parent);
        if parent_scale.contains(&0.0) {
            unimplemented!("Transform.SetParent under a Transform with a zero lossy scale (singular matrix)");
        }
        let node = self.ensure_body_node(parent);
        // The clone: a root whose Transform is the live world pose, flagged changed (SetParent reports its Transform and its
        // children's as changed).
        let root = self.with_component_instance(id, |_, inst, o| {
            if inst.objects[o].parent.is_some() {
                unimplemented!("Transform.SetParent of an object that is not the root of its clone");
            }
            if inst.descendants(o).iter().any(|d| inst.bodies.contains_key(d)) {
                unimplemented!("Transform.SetParent of an object with a child Rigidbody2D (their drivers change too)");
            }
            inst.set_parent_null_at(o, (live.position, live.rotation, scale));
            inst.objects[o].components[0]
        });
        self.unsync_instance_of(id);
        self.physicals[child].parent = Some(node);
        let chain = self.parent_chain(child);
        let (local, world) = set_world(&chain, live.position, live.rotation);
        let local_scale = [scale[0] / parent_scale[0], scale[1] / parent_scale[1], scale[2] / parent_scale[2]];
        let ph = &mut self.physicals[child];
        (ph.transform.position, ph.transform.rotation, ph.transform.local, ph.transform.localScale) = (world.0, world.1, Some(local), local_scale);
        // Rigidbody2D::TransformParentChanged.
        let owner = BodyOwner::Physical(child);
        let driven = (self.owner_rigidbody(owner).bodyType != 2).then(|| self.physicals[parent].rigidbody.body);
        self.set_driven_by(owner, driven);
        self.owner_rigidbody_mut(owner).movement.interpolating = false;
        self.rigidbody_hierarchy_changed = true;
        // Destroy and SetActive of the parent reach it; its body, not the parent's Transform, moves it.
        let local = Trs { t: [local.0[0], local.0[1], local.0[2], 0.0], q: local.1, s: [local_scale[0], local_scale[1], local_scale[2], 0.0] };
        let key = self.physicals[parent].transform_id;
        self.attached_clones.entry(key).or_default().push(AttachedClone { id: root, parent: AttachParent::Physical(parent), local, driven: true });
    }

    /// Transform.SetParent(null) (worldPositionStays) of physical `child`'s root object: its world pose and lossy scale
    /// become the local ones, nothing drives its Rigidbody2D any more, and the parent's SetActive and Destroy no longer
    /// reach it. The parent's node stays (other bodies may hang below it).
    pub(super) fn physical_set_parent_null(&mut self, child: usize) {
        if self.physicals[child].parent.is_none() {
            return;
        }
        let id = self.physicals[child].id;
        let live = self.physicals[child].transform;
        let scale = self.lossy_scale(child);
        let (local, world) = set_world(&[], live.position, live.rotation);
        let root = self.with_component_instance(id, |_, inst, o| {
            inst.set_parent_null_at(o, (world.0, world.1, scale));
            inst.objects[o].components[0]
        });
        self.unsync_instance_of(id);
        let ph = &mut self.physicals[child];
        ph.parent = None;
        (ph.transform.position, ph.transform.rotation, ph.transform.local, ph.transform.localScale) = (world.0, world.1, Some(local), scale);
        let owner = BodyOwner::Physical(child);
        self.set_driven_by(owner, None);
        self.owner_rigidbody_mut(owner).movement.interpolating = false;
        self.rigidbody_hierarchy_changed = true;
        self.attached_clones.values_mut().for_each(|v| v.retain(|c| c.id != root));
        self.attached_clones.retain(|_, v| !v.is_empty());
    }
}
