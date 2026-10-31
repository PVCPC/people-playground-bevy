//! SharpOnAllSidesBehaviour (the Sword's side blade). Each FixedUpdate: the tip's velocity decides whether it saws
//! (speed above MinSpeed: the connection strength is 10, otherwise ConnectionStrength and no new connections). The
//! SharpCollider's overlaps are walked in buffer order: the same root is skipped, a victim not in the world is skipped,
//! a connected victim's FrictionJoint2D follows the hit point, and a new one is soft-connected (soft_connect.rs, with
//! the Stabbed and Lodged messages) when it is saw-able (its softness or the stab-resistance rule).
#![allow(non_snake_case)]

use super::soft_connect::SoftConnections;
use super::{mathf, script, Game};

/// The buffer's length (`new Collider2D[16]`): OverlapCollider keeps its first 16 hits.
const BUFFER: usize = 16;

script! {
    SharpOnAllSides {
        /// ConnectionStrength (the strength of a soft connection when not sawing).
        ConnectionStrength: f32 = 300.0,
        /// MinSpeed: the tip speed above which the blade saws.
        MinSpeed: f32 = 10.0,
        /// MinSoftness: a victim this soft or softer can be saw-connected.
        MinSoftness: f32 = 0.5,
        /// Tip: the local point whose velocity is tested.
        Tip: [f32; 2] = [0.0, 1.0],
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// LayerMask (LayerMask bits): the overlap's mask.
        LayerMask: u32 = 0,
        /// Behaviour.enabled
        enabled: bool = true,
        /// PhysicalBehaviour (its rigidbody is the tip's and the soft connections' connected body).
        physical: usize = 0,
        /// SharpCollider's fixtures (the query shape of OverlapCollider).
        SharpCollider: Vec<crate::b2::world::FixtureId> = Vec::new(),
        /// SharpCollider's instance id.
        sharp_collider_id: i32 = 0,
        softConnections: SoftConnections = SoftConnections::default(),
    }
}

impl Game {
    /// SharpOnAllSidesBehaviour.FixedUpdate
    pub fn sharp_on_all_sides_fixed_update(&mut self, s: usize) {
        let p = self.sharp_on_all_sides[s].physical;
        let tip = self.sharp_on_all_sides[s].Tip;
        let world_tip = self.tip_world_point(p, tip);
        let v = self.physicals[p].rigidbody.clone().point_velocity(&self.world, world_tip);
        let flag = mathf::magnitude2(v) > self.sharp_on_all_sides[s].MinSpeed;
        let strength = if flag { 10.0 } else { self.sharp_on_all_sides[s].ConnectionStrength };
        self.sharp_on_all_sides_process(s, flag, strength);
    }

    /// Rigidbody2D.GetRelativePointVelocity's point: the body's transform applied to the local point (no scale).
    fn tip_world_point(&mut self, p: usize, local: [f32; 2]) -> [f32; 2] {
        self.sync_pending();
        let xf = self.world.body(self.physicals[p].rigidbody.body).xf;
        [xf.q.c * local[0] - xf.q.s * local[1] + xf.p.x, xf.q.s * local[0] + xf.q.c * local[1] + xf.p.y]
    }

    /// SharpOnAllSidesBehaviour.ProcessSoftConnections(shouldSaw, connectionStrength)
    pub(super) fn sharp_on_all_sides_process(&mut self, s: usize, should_saw: bool, strength: f32) {
        let fixtures = self.sharp_on_all_sides[s].SharpCollider.clone();
        let (mask, sharp) = (self.sharp_on_all_sides[s].LayerMask, self.sharp_on_all_sides[s].sharp_collider_id);
        let mut hits = self.overlap_collider(&fixtures, mask);
        hits.truncate(BUFFER);
        let mut conns = std::mem::take(&mut self.sharp_on_all_sides[s].softConnections);
        for k in conns.keys() {
            conns.get_mut(k).unwrap().should_be_deleted = true;
        }
        let me = self.sharp_on_all_sides[s].physical;
        let my_root = self.root_id(me);
        for (victim, coll) in hits {
            // collider.transform.root != base.transform.root, then PhysicalObjectsInWorldByTransform.TryGetValue.
            let Some(v) = victim.filter(|&v| self.root_id(v) != my_root && self.physicals[v].in_world) else { continue };
            if let Some(c) = conns.get(v).copied() {
                let broken = self.soft_connection_broken(&c);
                conns.get_mut(v).unwrap().should_be_deleted = broken;
                if !broken {
                    let hit = self.saw_hit_point(me, sharp, coll);
                    let a = self.inverse_transform_point(c.phys, [hit[0], hit[1], 0.0]);
                    self.fixed_joint_set_anchor(c.joint, [a[0], a[1]]);
                    self.friction_joint_set_max_force(c.joint, strength);
                    self.friction_joint_set_max_torque(c.joint, strength);
                }
            } else if should_saw
                && (self.properties[self.physicals[v].Properties].Softness >= self.sharp_on_all_sides[s].MinSoftness
                    || (self.properties[self.physicals[me].Properties].IgnoreStabResistance && !self.properties[self.physicals[v].Properties].IgnoreStabResistance))
            {
                self.soft_connect(&mut conns, me, sharp, v, coll, strength, true, &mut |g| g.saw_hit_point(me, sharp, coll));
            }
        }
        self.soft_connections_prune(&mut conns, sharp);
        self.sharp_on_all_sides[s].softConnections = conns;
    }

    /// SharpOnAllSidesBehaviour.OnDestroy (and OnDisable, which calls it): Object.Destroy of every connection's joint,
    /// then the dictionary is cleared.
    pub fn sharp_on_all_sides_destroy(&mut self, s: usize) {
        let conns = std::mem::take(&mut self.sharp_on_all_sides[s].softConnections);
        for k in conns.keys() {
            let f = conns.get(k).unwrap().joint;
            if self.soft_joint_alive(f) {
                self.destroy_joint_delayed(f);
            }
        }
    }
}
