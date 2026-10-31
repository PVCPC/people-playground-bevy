//! ImmobilityFieldBehaviour. Use switches the field: CaptureObject and Trigger follow Activated. A physical object
//! whose collider enters the trigger (on one of Layers) is held by a FrictionJoint2D on the field's GameObject, its
//! strength 2 * (Strength + 8 * Charge), until it leaves, its collider is disabled or the field switches. The clips
//! are audio (the CaptureClips draw stays); OnImmobilityCapture/OnImmobilityRelease reach only TeslaCoilBehaviour.
#![allow(non_snake_case)]

use super::soft_connect::SoftConnections;
use super::{script, BodyOwner, Game};

/// A `joints` entry: the FrictionJoint2D (`fixed_joints` index), the Collider2D and the PhysicalBehaviour found by
/// the collider's transform (whose GameObject is the key).
#[derive(Clone, Copy, Debug)]
pub struct Capture {
    pub joint: usize,
    pub coll: i32,
    pub phys: usize,
}

script! {
    ImmobilityField {
        Activated: bool = false,
        Strength: f32 = 0.0,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        enabled: bool = true,
        /// PhysicalBehaviour, on the field's own GameObject.
        physical: usize = 0,
        /// CaptureObject: an object of the instance.
        CaptureObject: usize = 0,
        /// Trigger: (object, component) in its instance.
        Trigger: (usize, usize) = (0, 0),
        /// The GameObject's Collider2Ds and Rigidbody2D (0 without one): the trigger messages it receives.
        colliders: Vec<i32> = Vec::new(),
        rigidbody: i32 = 0,
        /// CaptureClips.Length
        CaptureClips: usize = 0,
        /// Layers.m_Bits
        Layers: u32 = 0,
        /// Dictionary<GameObject, (FrictionJoint2D, Collider2D)> by GameObject instance id.
        joints: SoftConnections<i32, Capture> = SoftConnections::default(),
    }
}

impl Game {
    /// FinalStrength
    fn immobility_final_strength(&self, i: usize) -> f32 {
        let s = &self.immobility_fields[i];
        2.0 * (s.Strength + 8.0 * self.physicals[s.physical].Charge())
    }

    /// ImmobilityFieldBehaviour.Start
    pub fn immobility_field_start(&mut self, i: usize) {
        self.immobility_field_update_activation(i);
    }

    /// ImmobilityFieldBehaviour.Use (PlayClipOnce(EnableClip) is audio).
    pub fn immobility_field_use(&mut self, i: usize) {
        if self.immobility_fields[i].enabled {
            self.immobility_fields[i].Activated = !self.immobility_fields[i].Activated;
            self.immobility_field_update_activation(i);
        }
    }

    /// UpdateActivation on the clone that holds the field.
    fn immobility_field_update_activation(&mut self, i: usize) {
        let id = self.immobility_fields[i].id;
        self.with_component_instance(id, |g, inst, _| g.immobility_field_update_activation_in(i, inst));
    }

    /// ImmobilityFieldBehaviour.UpdateActivation (OnEnable runs it too): CaptureObject.SetActive(Activated),
    /// Trigger.enabled = Activated, DestroyAllJoints.
    pub fn immobility_field_update_activation_in(&mut self, i: usize, inst: &mut super::spawn::Instance) {
        let s = &self.immobility_fields[i];
        let (on, capture, trigger) = (s.Activated, s.CaptureObject, s.Trigger);
        self.set_active_scripts(inst, capture, on);
        self.collider_set_enabled(inst, trigger, on);
        self.immobility_field_destroy_all_joints(i);
    }

    /// ImmobilityFieldBehaviour.OnDisable
    pub fn immobility_field_on_disable(&mut self, i: usize, inst: &mut super::spawn::Instance) {
        self.immobility_field_destroy_all_joints(i);
        let (capture, trigger) = (self.immobility_fields[i].CaptureObject, self.immobility_fields[i].Trigger);
        self.set_active_scripts(inst, capture, false);
        self.collider_set_enabled(inst, trigger, false);
    }

    /// ImmobilityFieldBehaviour.DestroyAllJoints (OnDestroy runs it too): every FrictionJoint2D on the GameObject is
    /// destroyed and the dictionary is cleared.
    pub fn immobility_field_destroy_all_joints(&mut self, i: usize) {
        for key in self.immobility_fields[i].joints.keys() {
            let coll = self.immobility_fields[i].joints.get(key).unwrap().coll;
            self.tesla_coil_on_immobility_release(coll, i);
        }
        let p = self.immobility_fields[i].physical;
        for f in 0..self.fixed_joints.len() {
            let j = &self.fixed_joints[f];
            if j.friction.is_some() && !j.destroyed && matches!(j.attached, BodyOwner::Physical(a) if a == p) {
                self.destroy_joint_delayed(f);
            }
        }
        self.immobility_fields[i].joints.clear();
    }

    /// ImmobilityFieldBehaviour.FixedUpdate: a broken entry's joint is destroyed (the entry stays); a held one
    /// follows its collider's position with the current strength and wakes the body it holds.
    pub fn immobility_field_fixed_update(&mut self, i: usize) {
        let strength = self.immobility_final_strength(i);
        let p = self.immobility_fields[i].physical;
        for key in self.immobility_fields[i].joints.keys() {
            let c = *self.immobility_fields[i].joints.get(key).unwrap();
            if !self.soft_joint_alive(c.joint) || !self.collider_exists(c.coll) || !self.collider_enabled(c.coll) {
                self.tesla_coil_on_immobility_release(c.coll, i);
                if self.soft_joint_alive(c.joint) {
                    self.destroy_joint_delayed(c.joint);
                }
            } else {
                let pos = self.physicals[c.phys].transform.position;
                let a = self.inverse_transform_point(p, pos);
                self.fixed_joint_set_anchor(c.joint, [a[0], a[1]]);
                self.friction_joint_set_max_force(c.joint, strength);
                self.friction_joint_set_max_torque(c.joint, strength);
                // connectedBody.WakeUp()
                let rb = self.physicals[c.phys].rigidbody.clone();
                if rb.detached.is_none() {
                    self.world.body_mut(rb.body).set_sleeping_false();
                }
            }
        }
    }

    /// Whether ImmobilityFieldBehaviour `i` gets the trigger messages of collider `own`.
    pub(super) fn immobility_field_receives(&self, i: usize, own: i32, rigidbody: Option<crate::b2::world::RigidbodyRef>) -> bool {
        let s = &self.immobility_fields[i];
        super::decimator::trigger_receives(&s.colliders, s.rigidbody, own, rigidbody)
    }

    /// ImmobilityFieldBehaviour.OnTriggerEnter2D(collision)
    pub fn immobility_field_trigger_enter(&mut self, i: usize, collision: i32) {
        if !self.immobility_fields[i].Activated {
            return;
        }
        let Some(go) = self.world.collider_game_object(collision) else { return };
        if self.immobility_fields[i].joints.get(go).is_some() || self.immobility_fields[i].Layers & (1 << self.world.layers[&go]) == 0 {
            return;
        }
        // PhysicalObjectsInWorldByTransform.TryGetValue(collision.transform)
        let Some(v) = self.physicals.iter().position(|q| !q.destroyed && q.in_world && q.all_colliders.contains(&collision)) else { return };
        let p = self.immobility_fields[i].physical;
        let strength = self.immobility_final_strength(i);
        // AddComponent<FrictionJoint2D>() takes an instance id.
        self.allocate_instance_id();
        let f = self.add_friction_joint_component(p);
        self.fixed_joint_set_auto_configure(f, false);
        self.fixed_joint_set_enable_collision(f, true);
        self.friction_joint_set_max_force(f, strength);
        self.fixed_joint_set_connected_body(f, Some(BodyOwner::Physical(v)));
        self.friction_joint_set_max_torque(f, strength);
        let pos = self.physicals[v].transform.position;
        let a = self.inverse_transform_point(p, pos);
        self.fixed_joint_set_anchor(f, [a[0], a[1]]);
        self.fixed_joint_set_connected_anchor(f, [0.0, 0.0]);
        self.immobility_fields[i].joints.add(go, Capture { joint: f, coll: collision, phys: v });
        // PhysicalBehaviour.PlayClipOnce(CaptureClips.PickRandom())
        self.pick_random(self.immobility_fields[i].CaptureClips).unwrap_or_else(|| unimplemented!("ImmobilityFieldBehaviour: CaptureClips.PickRandom() of an empty array (IndexOutOfRangeException)"));
        self.tesla_coil_on_immobility_capture(collision, i);
    }

    /// ImmobilityFieldBehaviour.OnTriggerExit2D(collision)
    pub fn immobility_field_trigger_exit(&mut self, i: usize, collision: i32) {
        let Some(go) = self.world.collider_game_object(collision) else { return };
        if let Some(c) = self.immobility_fields[i].joints.get(go).copied() {
            self.tesla_coil_on_immobility_release(c.coll, i);
            if self.soft_joint_alive(c.joint) {
                self.destroy_joint_delayed(c.joint);
            }
            self.immobility_fields[i].joints.remove(go);
        }
    }
}
