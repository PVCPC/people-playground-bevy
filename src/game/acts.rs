//! ActOnCollide and PointToVelocityBehaviour.
#![allow(non_snake_case)]

use super::physical::get_average_impulse_remove_outliers;
use super::{d, mathf, script, Game};
use crate::assets::db::Component;
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
        /// Actions: what it calls, in order.
        targets: Vec<ActTarget> = Vec::new(),
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

/// A persistent call of ActOnCollide.Actions.
#[derive(Clone, Copy, Debug)]
pub enum ActTarget {
    /// ExplosiveBehaviour.Activate (`Game::explosives`)
    Activate(usize),
    /// FirearmBehaviour.Shoot (`Game::firearms`)
    Shoot(usize),
    /// ProjectileLauncherBehaviour.Launch (`Game::launchers`)
    Launch(usize),
}

/// A script that takes a physical object's collision messages besides the PhysicalBehaviour and LimbBehaviour.
#[derive(Clone, Copy, Debug)]
pub enum CollisionScript {
    Destroyable(usize),
    Explosive(usize),
    ActOnCollide(usize),
    Sticky(usize),
    Bulb(usize),
}

impl Game {
    pub(super) fn collision_enter_script(&mut self, s: CollisionScript, c: &ScriptCollision) {
        match s {
            CollisionScript::Destroyable(x) => self.destroyable_on_collision(x, c),
            CollisionScript::Explosive(e) => self.explosive_on_collision_enter(e, c),
            CollisionScript::ActOnCollide(a) => self.act_on_collide_enter(a, c),
            CollisionScript::Sticky(s) => self.sticky_grenade_collision_enter(s, c),
            CollisionScript::Bulb(b) => self.bulb_on_collision_enter(b, c),
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
            for t in s.targets.clone() {
                match t {
                    ActTarget::Activate(e) => self.explosive_activate(e),
                    ActTarget::Shoot(f) => self.firearm_shoot(f),
                    ActTarget::Launch(l) => self.launcher_launch(l),
                }
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
        /// isStuck
        is_stuck: bool = false,
        /// joint: the `Game::fixed_joints` entry of the FixedJoint2D CreateJoint added (isStuckToHuman and
        /// aboutToExplode only reach the EMP_STICKY_GRENADE achievement).
        joint: Option<usize> = None,
    }
}

/// Layer indices of TagManager.asset.
const OBJECTS_LAYER: i32 = 9;
const DEBRIS_LAYER: i32 = 10;

impl Game {
    /// `(bool)joint`: the component exists.
    fn sticky_joint_alive(&self, s: usize) -> bool {
        self.sticky_grenades[s].joint.is_some_and(|f| !self.fixed_joints[f].destroyed)
    }

    /// GameObject.layer = layer on the grenade's object (GameObject::SetLayer, 180589640): kLayerChanged reaches its
    /// components in order, and only Collider2D (RecalculateContacts, 180c14640) and Rigidbody2D (180c1cdc0, a wake)
    /// act on it among the physics ones (a Renderer's is rendering).
    fn sticky_set_layer(&mut self, s: usize, layer: i32) {
        let p = self.sticky_grenades[s].physical.expect("a StickyGrenadeBehaviour receiving collisions has a PhysicalBehaviour");
        let id = self.physicals[p].id;
        self.with_component_instance(id, |g, inst, o| {
            g.world.layers.insert(inst.objects[o].game_object, layer);
            let prefab = inst.prefab.clone();
            for (k, (_, component)) in prefab.objects[o].components.iter().enumerate() {
                match component {
                    Component::Collider2D(_) => g.world.refilter_collider(inst.objects[o].components[k]),
                    Component::Rigidbody2D(_) => {
                        if let Some(&body) = inst.bodies.get(&o) {
                            g.world.wake_body(body);
                        }
                    }
                    _ => {}
                }
            }
        });
    }

    /// StickyGrenadeBehaviour.CreateJoint
    fn sticky_create_joint(&mut self, s: usize, phys: usize) {
        if self.sticky_joint_alive(s) {
            self.destroy_joint_delayed(self.sticky_grenades[s].joint.unwrap());
        }
        let p = self.sticky_grenades[s].physical.expect("a StickyGrenadeBehaviour receiving collisions has a PhysicalBehaviour");
        // AddComponent<FixedJoint2D> takes the component's instance id.
        self.allocate_instance_id();
        let f = self.add_fixed_joint_component(p);
        self.sticky_grenades[s].joint = Some(f);
        self.fixed_joint_set_connected_body(f, Some(super::BodyOwner::Physical(phys)));
        self.sticky_set_layer(s, DEBRIS_LAYER);
    }

    /// StickyGrenadeBehaviour.DestroyJoint
    fn sticky_destroy_joint(&mut self, s: usize) {
        self.sticky_set_layer(s, OBJECTS_LAYER);
        self.sticky_grenades[s].is_stuck = false;
        if self.sticky_joint_alive(s) {
            self.destroy_joint_delayed(self.sticky_grenades[s].joint.unwrap());
        }
    }

    /// StickyGrenadeBehaviour.FixedUpdate
    pub(super) fn sticky_grenade_fixed_update(&mut self, s: usize) {
        let alive = self.sticky_joint_alive(s);
        let stuck = self.sticky_grenades[s].is_stuck;
        // !joint.connectedBody: its Rigidbody2D is gone (or never set).
        let no_connected_body = alive
            && match self.fixed_joints[self.sticky_grenades[s].joint.unwrap()].connected {
                None => true,
                Some(super::BodyOwner::Physical(p)) => self.destroyed_components.contains(&self.physicals[p].rigidbody.component),
                Some(super::BodyOwner::Node(k)) => self.destroyed_components.contains(&self.node_rigidbodies[k].rigidbody.component),
            };
        if (stuck && !alive) || (!stuck && alive) || (stuck && alive && no_connected_body) {
            self.sticky_destroy_joint(s);
        }
    }

    /// StickyGrenadeBehaviour.OnJointBreak2D, sent to the broken joint's GameObject: `broken != joint` compares the
    /// components.
    pub(super) fn sticky_grenade_on_joint_break(&mut self, f: usize) {
        for s in 0..self.sticky_grenades.len() {
            if self.sticky_grenades[s].joint == Some(f) {
                self.sticky_destroy_joint(s);
            }
        }
    }

    /// Object.Destroy(FixedJoint2D): DestroyObjectDelayed queues it at min(fixedTimeAsDouble, the frame's time).
    fn destroy_joint_delayed(&mut self, f: usize) {
        let at = self.time.fixedTimeAsDouble.min(self.time.timeAsDouble);
        self.joint_destroys.push((at, f));
    }

    /// The queued joint Destroys due by now, as (time, `fixed_joints` index).
    pub fn joint_due_destroys(&self, now: f64) -> Vec<(f64, usize)> {
        self.joint_destroys.iter().copied().filter(|&(at, _)| at <= now).collect()
    }

    /// The delayed Destroy of a FixedJoint2D (a second one finds it gone).
    pub fn run_joint_destroy(&mut self, f: usize) {
        self.joint_destroys.retain(|&(_, j)| j != f);
        if !self.fixed_joints[f].destroyed {
            self.fixed_joint_destroy(f);
        }
    }

    /// StickyGrenadeBehaviour.OnCollisionEnter2D
    pub(super) fn sticky_grenade_collision_enter(&mut self, s: usize, c: &ScriptCollision) {
        let g = &self.sticky_grenades[s];
        if g.is_stuck || !self.explosives[g.explosive].Armed {
            return;
        }
        // Global.PhysicalObjectsInWorldByTransform[collision.transform]: the other body's transform, else the collider's.
        let phys = match c.rigidbody {
            Some(rb) => self.physicals.iter().position(|p| !p.destroyed && p.in_world && p.rigidbody.component == rb),
            None => self.physical_of_collider(c.collider).filter(|&p| self.physicals[p].in_world),
        };
        if let Some(p) = phys.filter(|&p| !g.unsticky.contains(&self.physicals[p].Properties)) {
            self.sticky_grenades[s].is_stuck = true;
            self.sticky_create_joint(s, p);
        }
    }
}

/// DisintegrationCounterBehaviour on a root that is not a person's: the root's top object (component 0 id) and the
/// PhysicalBehaviours (`Game::physicals`) Start subscribed OnChildDisintegrated to.
#[derive(Clone, Debug)]
pub struct DisintegrationCounter {
    pub id: i32,
    pub root: i32,
    pub DisintegrationCount: i32,
    pub children: Vec<usize>,
}

impl Game {
    /// DisintegrationCounterBehaviour.Start: GetComponentsInChildren<PhysicalBehaviour>() (the active objects below,
    /// depth first, each object's components in order).
    pub fn disintegration_counter_start(&mut self, id: i32) {
        let (root, ids) = self.with_component_instance(id, |_, inst, o| {
            fn collect(inst: &super::spawn::Instance, x: usize, out: &mut Vec<i32>) {
                if !inst.objects[x].active {
                    return;
                }
                for (k, (_, c)) in inst.prefab.objects[x].components.iter().enumerate() {
                    if matches!(c, crate::assets::db::Component::Script { class, .. } if class == "PhysicalBehaviour") {
                        out.push(inst.objects[x].components[k]);
                    }
                }
                for &child in &inst.prefab.objects[x].children {
                    collect(inst, child, out);
                }
            }
            let mut ids = Vec::new();
            collect(inst, o, &mut ids);
            let mut top = o;
            while let Some(parent) = inst.objects[top].parent {
                top = parent;
            }
            (inst.objects[top].components[0], ids)
        });
        let children = ids.iter().map(|&cid| self.physicals.iter().position(|p| p.id == cid).expect("a PhysicalBehaviour in the arena")).collect();
        self.disintegration_counters.push(DisintegrationCounter { id, root, DisintegrationCount: 0, children });
    }

    /// The OnDisintegration of physical `p` reaches the counters subscribed to it.
    pub fn disintegration_counters_on_disintegration(&mut self, p: usize) {
        for i in 0..self.disintegration_counters.len() {
            if self.disintegration_counters[i].children.contains(&p) {
                self.disintegration_counter_on_child_disintegrated(i);
            }
        }
    }

    /// DisintegrationCounterBehaviour.OnChildDisintegrated
    fn disintegration_counter_on_child_disintegrated(&mut self, i: usize) {
        self.disintegration_counters[i].DisintegrationCount += 1;
        let alive = self.disintegration_counters[i].children.iter().filter(|&&c| !self.physicals[c].destroyed).count() as i32;
        if self.disintegration_counters[i].DisintegrationCount >= alive {
            let root = self.disintegration_counters[i].root;
            self.destroy_object(root);
        }
    }

    /// DisintegrationCounterBehaviour.OnDestroy: the handlers go with it.
    pub fn disintegration_counter_destroy(&mut self, id: i32) {
        self.disintegration_counters.retain(|c| c.id != id);
    }
}
