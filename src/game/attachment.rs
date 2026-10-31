//! FirearmAttachmentBehaviour and its subclasses (Scope, Battery, Explosive, Flashlight, Incendiary) and the
//! FirearmAttachmentPointBehaviour on the guns. Awake caches the PhysicalBehaviour and adds a CheckOnDestroyAttachment
//! (one instance id).
//!
//! A point's OnTriggerEnter2D, ConnectAttachment, DisconnectAttachment and DisconnectOnAttachmentDestroyed, and
//! CheckOnDestroyAttachment.OnDestroy are rewritten: the attachment takes the firearm's scale and rotation, a
//! HingeJoint2D (limits 0..0) is built into the b2 world between the firearm and the attachment, the attachment's
//! Rigidbody2D object becomes a child of the firearm's Transform (`reparent.rs`) and its listeners and OnConnect run.
//! The HingeJoint2D is the point's: it is re-created when either body's Transform changes, cleaned up with a
//! deactivated body and destroyed (delayed, as Object.Destroy is) on disconnect.
#![allow(non_snake_case)]

use super::explosion::{EffectSize, ExplosionParameters};
use super::{d, mathf, script, Game};
use crate::b2::math::Vec2 as V;
use crate::b2::revolute::RevoluteDef;
use crate::b2::world::JointId;
use crate::unity::transform::{global_position, inverse, Trs};

/// The subclasses of FirearmAttachmentBehaviour that item prefabs use.
pub const CLASSES: [&str; 5] = ["ScopeAttachmentBehaviour", "BatteryAttachmentBehaviour", "ExplosiveAttachmentBehaviour", "FlashlightAttachmentBehaviour", "IncendiaryAttachmentBehaviour"];

/// Which subclass an attachment is.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Kind {
    #[default]
    Scope,
    Battery,
    Explosive,
    Flashlight,
    Incendiary,
}

script! {
    FirearmAttachment {
        /// AttachmentType (FirearmAttachmentType.AttachmentType: Scope 0, Barrel 1, Other 2)
        AttachmentType: i32 = 0,
        /// AttachmentOffset
        AttachmentOffset: [f32; 2] = [0.0, 0.0],
        /// Connected
        Connected: bool = false,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// Behaviour.enabled
        enabled: bool = true,
        kind: Kind = Kind::Scope,
        /// PhysicalBehaviour (Awake's GetComponent).
        physical: Option<usize> = None,
        /// The SpriteRenderer's sprite rect height in pixels (GetLocalJointAnchor).
        sprite_height: f32 = 0.0,
        /// FirearmBehaviour (Game::firearms) and AttachmentPoint (Game::attachment_points).
        firearm: Option<usize> = None,
        point: Option<usize> = None,
        /// Scope.AccuracyPercent, Battery.BaseCharge, Incendiary.FireChance.
        AccuracyPercent: i32 = 25,
        BaseCharge: f32 = 5.0,
        FireChance: i32 = 10,
        /// Explosive.ExplosionParams.
        ExplosionParams: ExplosionParameters = ExplosionParameters::default(),
        /// The LaserBehaviour on the same object (ScopeAttachmentBehaviour's TryGetComponent).
        laser: Option<usize> = None,
    }
}

/// A FirearmAttachmentPointBehaviour.
#[derive(Clone, Debug, Default)]
pub struct AttachmentPoint {
    /// The component's instance id.
    pub id: i32,
    /// The BoxCollider2D trigger(s) on the point's own object.
    pub colliders: Vec<i32>,
    pub AttachmentType: i32,
    /// firearmBehaviour (Start: transform.parent.GetComponent<FirearmBehaviour>(), an index into Game::firearms).
    pub firearm: Option<usize>,
    /// FirearmAttachment
    pub FirearmAttachment: Option<usize>,
    pub ignoreConnect: bool,
    /// The HingeJoint2D.
    pub joint: Option<JointId>,
    /// The HingeJoint2D's anchor (in the firearm's space) and the angles HingeJoint2D::Cleanup(false) kept for its next Create.
    pub anchor: [f32; 2],
    pub kept: Option<KeptAngles>,
    /// The component's attached object (the firearm) and connectedBody (the attachment), as PhysicalBehaviours: they stay
    /// the joint's after the point forgets the attachment.
    pub jointed: Option<(usize, usize)>,
}

/// What HingeJoint2D::Cleanup(false) keeps: the reference angle and both body angles.
#[derive(Clone, Copy, Debug)]
pub struct KeptAngles {
    pub reference_angle: f32,
    pub body_angle_a: f32,
    pub body_angle_b: f32,
}

impl Game {
    /// FirearmAttachmentBehaviour.Awake: GetComponent<PhysicalBehaviour>() (a link), then GetOrAddComponent
    /// <CheckOnDestroyAttachment>(), which adds one component.
    pub fn firearm_attachment_awake(&mut self) {
        self.allocate_instance_id();
    }

    /// FirearmAttachmentBehaviour.Use: `if (AttachmentPoint)` and the propagation's Target is this object, which
    /// disconnects it. `target` is the physical a direct activation was made on (ActivationPropagation.Target): a
    /// firearm's activation broadcasts to its attachment too, with the firearm as Target.
    pub fn firearm_attachment_use(&mut self, a: usize, target: usize) {
        let Some(pt) = self.firearm_attachments[a].point.filter(|&pt| !self.destroyed_components.contains(&self.attachment_points[pt].id)) else { return };
        if self.firearm_attachments[a].physical == Some(target) {
            self.attachment_point_disconnect(pt);
        }
    }

    /// BatteryAttachmentBehaviour.FixedUpdate: `if (FirearmBehaviour) Phys.Charge += BaseCharge`, and FirearmBehaviour is
    /// only set while connected.
    pub fn battery_attachment_fixed_update(&mut self, a: usize) {
        let x = &self.firearm_attachments[a];
        if let (true, Some(f)) = (x.Connected, x.firearm) {
            let base = x.BaseCharge;
            let p = self.firearms[f].physical.expect("a firearm without a PhysicalBehaviour");
            let charge = self.physicals[p].Charge();
            self.physicals[p].charge = charge + base;
        }
    }

    /// The attachment whose own GameObject holds `collider` (collider.gameObject.TryGetComponent).
    fn attachment_of_collider(&self, collider: i32) -> Option<usize> {
        self.firearm_attachments.iter().position(|a| a.physical.is_some_and(|p| !self.physicals[p].destroyed && self.physicals[p].colliders.contains(&collider)))
    }

    /// The point's FirearmAttachment as Unity's null test reads it (a destroyed attachment is null).
    fn point_attachment(&self, pt: usize) -> Option<usize> {
        self.attachment_points[pt].FirearmAttachment.filter(|&a| self.firearm_attachments[a].physical.is_some_and(|p| !self.physicals[p].destroyed))
    }

    /// FirearmAttachmentPointBehaviour.OnTriggerEnter2D(collider): `collider` entered a trigger of point `pt`.
    pub fn attachment_point_trigger_enter(&mut self, pt: usize, collider: i32) {
        if self.attachment_points[pt].ignoreConnect || self.point_attachment(pt).is_some() {
            return;
        }
        // TryGetComponent assigns FirearmAttachment (null when the GameObject has none).
        let found = self.attachment_of_collider(collider);
        self.attachment_points[pt].FirearmAttachment = found;
        let Some(a) = found else { return };
        if self.firearm_attachments[a].AttachmentType != self.attachment_points[pt].AttachmentType {
            self.attachment_points[pt].FirearmAttachment = None;
            return;
        }
        self.firearm_attachments[a].firearm = self.attachment_points[pt].firearm;
        self.firearm_attachments[a].point = Some(pt);
        self.connect_attachment(pt);
    }

    /// FirearmAttachmentPointBehaviour.GetLocalJointAnchor:
    /// firearm.InverseTransformPoint(point.TransformPoint(AttachmentOffset.x, AttachmentOffset.y + num, 0)).
    fn attachment_local_joint_anchor(&mut self, pt: usize, a: usize, gun: usize) -> [f32; 2] {
        let x = &self.firearm_attachments[a];
        let (offset, height) = (x.AttachmentOffset, x.sprite_height);
        let num = (d(height) * 0.5f32 * (1.0f32 / 35.0f32) - 1.0f32 / 70.0f32).f();
        if self.physicals[gun].parent.is_some() {
            unimplemented!("FirearmAttachmentPointBehaviour.GetLocalJointAnchor on a firearm with a parent Transform");
        }
        let gun_id = self.physicals[gun].id;
        let t = &self.physicals[gun].transform;
        let gun_trs = Trs { t: [t.position[0], t.position[1], t.position[2], 0.0], q: t.rotation, s: [t.localScale[0], t.localScale[1], t.localScale[2], 0.0] };
        let point_id = self.attachment_points[pt].id;
        let local = self.with_component_instance(point_id, |_, inst, o| {
            // TransformPoint through the point's chain up to (not including) the firearm's object.
            let mut v = [offset[0], offset[1] + num, 0.0, 0.0];
            let mut x = o;
            while !inst.objects[x].components.contains(&gun_id) {
                v = global_position(&inst.objects[x].trs, v);
                x = inst.objects[x].parent.unwrap_or_else(|| unimplemented!("a FirearmAttachmentPointBehaviour outside its firearm's object"));
            }
            v
        });
        let world = global_position(&gun_trs, local);
        let (back, _) = inverse(&gun_trs, world, gun_trs.q);
        [back[0], back[1]]
    }

    /// The world position of point `pt`'s Transform (the test harness drops an attachment on it).
    pub fn attachment_point_position(&mut self, pt: usize) -> [f32; 3] {
        let gun = self.attachment_points[pt].firearm.and_then(|f| self.firearms[f].physical).expect("a point without a firearm");
        let gun_id = self.physicals[gun].id;
        let t = &self.physicals[gun].transform;
        let gun_trs = Trs { t: [t.position[0], t.position[1], t.position[2], 0.0], q: t.rotation, s: [t.localScale[0], t.localScale[1], t.localScale[2], 0.0] };
        let point_id = self.attachment_points[pt].id;
        let local = self.with_component_instance(point_id, |_, inst, o| {
            let mut v = [0.0; 4];
            let mut x = o;
            while !inst.objects[x].components.contains(&gun_id) {
                v = global_position(&inst.objects[x].trs, v);
                x = inst.objects[x].parent.expect("a point outside its firearm's object");
            }
            v
        });
        let w = global_position(&gun_trs, local);
        [w[0], w[1], w[2]]
    }

    /// A direct activation of physical `p` (the test harness; the tool does it for the hovered or selected one).
    pub fn activate_for_test(&mut self, p: usize) {
        self.activate_direct(p);
    }

    /// FirearmAttachmentPointBehaviour.ConnectAttachment.
    fn connect_attachment(&mut self, pt: usize) {
        let a = self.attachment_points[pt].FirearmAttachment.expect("ConnectAttachment without an attachment");
        if self.firearm_attachments[a].Connected {
            return;
        }
        let Some(f) = self.attachment_points[pt].firearm else {
            unimplemented!("FirearmAttachmentPointBehaviour.ConnectAttachment: the point's parent has no FirearmBehaviour (NullReferenceException)");
        };
        let gun = self.firearms[f].physical.expect("a firearm without a PhysicalBehaviour");
        let att = self.firearm_attachments[a].physical.expect("an attachment without a PhysicalBehaviour");
        if self.physicals[att].parent.is_some() {
            unimplemented!("FirearmAttachmentPointBehaviour.ConnectAttachment of an attachment that already has a parent Transform");
        }
        // FirearmAttachment.transform.localScale = firearm.localScale; .rotation = firearm.rotation.
        let (scale, rotation) = (self.physicals[gun].transform.localScale, self.physicals[gun].transform.rotation);
        let att_id = self.physicals[att].id;
        self.with_component_instance(att_id, |g, inst, o| {
            if inst.objects[o].parent.is_some() {
                unimplemented!("an attachment that is not the root of its object");
            }
            // The clone's Transform is the pose only until the body first moves: the live pose is the Physical's.
            let live = g.physicals[att].transform;
            inst.objects[o].trs.t = [live.position[0], live.position[1], live.position[2], 0.0];
            inst.objects[o].trs.q = live.rotation;
            inst.set_local_scale(o, scale);
            inst.set_world_rotation(o, rotation);
            g.physicals[att].transform = super::link::pose(inst, o);
        });
        self.unsync_instance_of(att_id);
        // AddComponent<HingeJoint2D>() (one instance id), and the setters; connectedBody flushes the transform sync.
        self.allocate_instance_id();
        let anchor = self.attachment_local_joint_anchor(pt, a, gun);
        self.sync_pending();
        self.attachment_points[pt].anchor = anchor;
        self.attachment_points[pt].kept = None;
        self.attachment_points[pt].jointed = Some((gun, att));
        self.world.wake_body(self.physicals[gun].rigidbody.body);
        self.attachment_create_joint(pt);
        // FirearmAttachment.transform.SetParent(firearm.transform), then the listeners and Connect().
        self.physical_set_parent(att, gun);
        self.connect_attachment_listeners(a, f);
    }

    /// Joint2D::Create of the point's HingeJoint2D on the firearm (anchor A, connectedAnchor zero on the attachment,
    /// limits 0..0): the reference angle is the attached body's angle difference, or what Cleanup kept (moved a turn when a
    /// body angle wrapped, as hinge_joint2d_create does).
    fn attachment_create_joint(&mut self, pt: usize) {
        let (gun, att) = self.attachment_points[pt].jointed.expect("a HingeJoint2D without its bodies");
        let (ph_g, ph_a) = (&self.physicals[gun], &self.physicals[att]);
        if !ph_a.active || ph_g.rigidbody.detached.is_some() || ph_a.rigidbody.detached.is_some() {
            unimplemented!("FirearmAttachmentPointBehaviour: a HingeJoint2D to an inactive object");
        }
        let (ba, bb) = (ph_g.rigidbody.body, ph_a.rigidbody.body);
        let (angle_a, angle_b) = (self.world.body(ba).sweep.a, self.world.body(bb).sweep.a);
        let reference_angle = match self.attachment_points[pt].kept {
            None => angle_b - angle_a,
            Some(k) => {
                const PI: f32 = f32::from_bits(0x4049_0fdb);
                const TAU: f32 = f32::from_bits(0x40c9_0fdb);
                const TOLERANCE: f32 = f32::from_bits(0x3d0e_fa36);
                let near = |x: f32, y: f32| TOLERANCE >= (x - y).abs();
                let (prev_a, prev_b, mut r) = (k.body_angle_a, k.body_angle_b, k.reference_angle);
                if prev_a > PI && near(angle_a, prev_a - TAU) {
                    r += TAU;
                } else if -PI > prev_a && near(angle_a, prev_a + TAU) {
                    r -= TAU;
                }
                if prev_b > PI && near(angle_b, prev_b - TAU) {
                    r -= TAU;
                } else if -PI > prev_a && near(angle_b, prev_b + TAU) {
                    r += TAU;
                }
                r
            }
        };
        let anchor = self.attachment_points[pt].anchor;
        let s = self.lossy_scale(gun);
        let def = RevoluteDef {
            local_anchor_a: V::new(s[0] * anchor[0], s[1] * anchor[1]),
            local_anchor_b: V::ZERO,
            reference_angle,
            enable_limit: true,
            lower_angle: 0.0,
            upper_angle: 0.0,
            enable_motor: false,
            motor_speed: 0.0,
            max_motor_torque: 10000.0,
        };
        let joint = self.world.create_joint(def, ba, bb, false);
        self.world.wake_joined(ba);
        self.world.wake_joined(bb);
        self.attachment_points[pt].joint = Some(joint);
    }

    /// The point whose HingeJoint2D is the b2 joint `j`.
    fn point_of_joint(&self, j: JointId) -> Option<usize> {
        self.attachment_points.iter().position(|p| p.joint == Some(j))
    }

    /// HingeJoint2D::Cleanup(false) of the point's joint `j` (Rigidbody2D::Cleanup, Deactivate, ReCreate): the angles are
    /// kept for the next Create and the b2 joint goes. False when `j` is not a point's.
    pub(super) fn attachment_joint_cleanup(&mut self, j: JointId) -> bool {
        let Some(pt) = self.point_of_joint(j) else { return false };
        let joint = self.world.joints[j].as_ref().expect("a point's HingeJoint2D without its b2 joint");
        let (a, b, reference_angle) = (joint.body_a, joint.body_b, joint.revolute().def.reference_angle);
        self.attachment_points[pt].kept = Some(KeptAngles { reference_angle, body_angle_a: self.world.body(a).sweep.a, body_angle_b: self.world.body(b).sweep.a });
        self.attachment_points[pt].joint = None;
        self.world.destroy_joint(j);
        true
    }

    /// Joint2D::ReCreate of the point's joint `j` (a body of it had its Transform changed): Cleanup, then Create while
    /// both objects are active. False when `j` is not a point's.
    pub(super) fn attachment_joint_recreate(&mut self, j: JointId) -> bool {
        let Some(pt) = self.point_of_joint(j) else { return false };
        self.attachment_joint_cleanup(j);
        self.attachment_create_joint(pt);
        true
    }

    /// Object.Destroy(joint): DestroyObjectDelayed queues it at min(fixedTimeAsDouble, the frame's time); a second Destroy
    /// finds it queued.
    fn attachment_destroy_joint(&mut self, pt: usize) {
        let Some(j) = self.attachment_points[pt].joint else { return };
        if !self.attachment_joint_destroys.iter().any(|&(_, k)| k == j) {
            let at = self.time.fixedTimeAsDouble.min(self.time.timeAsDouble);
            self.attachment_joint_destroys.push((at, j));
        }
    }

    /// The delayed destroy of a point's HingeJoint2D: its Joint2D::Cleanup destroys the b2 joint; the component is gone, so
    /// nothing is kept for a next Create.
    pub(super) fn attachment_run_joint_destroy(&mut self, j: JointId) {
        self.attachment_joint_destroys.retain(|&(_, k)| k != j);
        if let Some(pt) = self.point_of_joint(j) {
            self.attachment_joint_cleanup(j);
            self.attachment_points[pt].kept = None;
        }
    }

    /// FirearmAttachmentPointBehaviour.DisconnectAttachment (the attachment was activated directly).
    fn attachment_point_disconnect(&mut self, pt: usize) {
        self.attachment_points[pt].ignoreConnect = true;
        self.attachment_destroy_joint(pt);
        if let Some(a) = self.point_attachment(pt) {
            self.attachment_on_disconnect(a);
            let (f, att) = (self.attachment_points[pt].firearm, self.firearm_attachments[a].physical);
            let x = &mut self.firearm_attachments[a];
            (x.point, x.firearm, x.Connected) = (None, None, false);
            // OnFire.RemoveListener and BulletEntryEvent.RemoveListener: the first entry.
            if let Some(f) = f {
                let list = &mut self.firearms[f].attachments;
                if let Some(i) = list.iter().position(|&k| k == a) {
                    list.remove(i);
                }
            }
            self.physical_set_parent_null(att.expect("an attachment without a PhysicalBehaviour"));
            self.attachment_points[pt].FirearmAttachment = None;
        }
        // detailViewBlob.enabled = true: rendering. StartCoroutine(ConnectDelay()): WaitForSeconds(0.75).
        let id = self.attachment_points[pt].id;
        self.start_wait(id, 0.75, super::destroyable::Step::AttachmentConnectDelay(pt));
    }

    /// CheckOnDestroyAttachment.OnDestroy of the object whose Transform id is `key` (the component FirearmAttachmentBehaviour.
    /// Awake added): a connected attachment disconnects itself from its point, whose joint goes.
    pub(super) fn check_on_destroy_attachment(&mut self, key: i32) {
        let Some(a) = self.firearm_attachments.iter().position(|a| a.physical.is_some_and(|p| self.physicals[p].transform_id == key)) else { return };
        if !self.firearm_attachments[a].Connected {
            return;
        }
        self.attachment_on_disconnect(a);
        // (bool)Attachment.AttachmentPoint: the point's object was not destroyed with it.
        if let Some(pt) = self.firearm_attachments[a].point.filter(|&pt| !self.destroyed_components.contains(&self.attachment_points[pt].id)) {
            // DisconnectOnAttachmentDestroyed: ignoreConnect = false, Destroy(joint).
            self.attachment_points[pt].ignoreConnect = false;
            self.attachment_destroy_joint(pt);
        }
    }

    /// ConnectAttachment behind the SetParent: the OnFire and BulletEntryEvent listeners (always added and removed
    /// together, so one list serves both), then Connect().
    fn connect_attachment_listeners(&mut self, a: usize, f: usize) {
        self.firearms[f].attachments.push(a);
        if !self.firearm_attachments[a].Connected {
            self.firearm_attachments[a].Connected = true;
            // ConnectClip: audio only.
            self.attachment_on_connect(a);
        }
    }

    /// FirearmAttachmentBehaviour.OnConnect of each subclass.
    fn attachment_on_connect(&mut self, a: usize) {
        let x = &self.firearm_attachments[a];
        let Some(f) = x.firearm else { return };
        match x.kind {
            Kind::Scope => {
                let value = 1.0f32 - x.AccuracyPercent as f32 / 100.0;
                let laser = x.laser;
                self.firearms[f].InaccuracyMultipliers.push(value);
                if let Some(l) = laser {
                    self.lasers[l].Activated = true;
                    self.laser_update_activation(l);
                }
            }
            Kind::Battery => self.firearms[f].ApplyCharge = true,
            Kind::Explosive => {
                let damage = self.firearms[f].Cartridge.Damage;
                let mut e = x.ExplosionParams;
                e.Range = (d(e.Range) * (d(damage) / 10.0f32)).f();
                e.FragmentForce = (d(e.FragmentForce) * (d(damage) / 10.0f32)).f();
                e.EffectSize = if damage > 100.0 {
                    EffectSize::Large
                } else if damage > 50.0 {
                    EffectSize::Medium
                } else {
                    EffectSize::Small
                };
                let em = self.firearms[f].emitter.as_mut().expect("a firearm that has not started");
                em.ExplosiveRoundParams = e;
                em.ExplosiveRound = true;
                em.ExplodeAtHitPoint = true;
            }
            Kind::Flashlight | Kind::Incendiary => {}
        }
    }

    /// FirearmAttachmentBehaviour.OnDisconnect of each subclass.
    fn attachment_on_disconnect(&mut self, a: usize) {
        let x = &self.firearm_attachments[a];
        let Some(f) = x.firearm else { return };
        match x.kind {
            Kind::Scope => {
                let value = 1.0f32 - x.AccuracyPercent as f32 / 100.0;
                let laser = x.laser;
                let list = &mut self.firearms[f].InaccuracyMultipliers;
                if let Some(i) = list.iter().position(|&m| m == value) {
                    list.remove(i);
                }
                if let Some(l) = laser {
                    self.lasers[l].Activated = false;
                    self.laser_update_activation(l);
                }
            }
            Kind::Battery => self.firearms[f].ApplyCharge = false,
            Kind::Explosive => {
                // The destroyed firearm's emitter is gone; the field writes on its dead object change nothing.
                if let Some(em) = self.firearms[f].emitter.as_mut() {
                    em.ExplosiveRound = false;
                    em.ExplodeAtHitPoint = false;
                }
            }
            Kind::Flashlight | Kind::Incendiary => {}
        }
    }

    /// BallisticsEmitter.BulletEntryEvent: every connected attachment's OnHit, in the order they were added. Only
    /// Incendiary acts.
    pub(super) fn bullet_entry_listeners(&mut self, f: usize, hit_collider: i32, position: [f32; 2]) {
        for a in self.firearms[f].attachments.clone() {
            self.firearm_attachment_on_hit(a, hit_collider, position);
        }
    }

    /// IncendiaryAttachmentBehaviour.OnHit.
    fn firearm_attachment_on_hit(&mut self, a: usize, hit_collider: i32, position: [f32; 2]) {
        let x = &self.firearm_attachments[a];
        if x.kind != Kind::Incendiary {
            return;
        }
        let (chance, f) = (x.FireChance, x.firearm.expect("a connected attachment without its firearm"));
        // Global.main.PhysicalObjectsInWorldByTransform[args.HitObject.transform]: the collider sits on the physical's own Transform.
        let Some(p) = self.physicals.iter().position(|p| !p.destroyed && p.in_world && p.colliders.contains(&hit_collider)) else { return };
        let damage = self.firearms[f].Cartridge.Damage;
        if self.random.value() <= chance as f32 / 100.0 {
            self.ignite_at(p, false, position);
            if self.physicals[p].OnFire {
                self.physicals[p].burnIntensity = 0.5;
            }
        }
        let ph = &mut self.physicals[p];
        let progress = ph.BurnProgress;
        ph.BurnProgress = (d(progress) + d(0.001f32) * d(damage) / (d(1.5f32) + d(mathf::max(0.01, progress)))).f();
        let heat_capacity = self.heat_capacity(p);
        let ph = &mut self.physicals[p];
        ph.Temperature = (d(ph.Temperature) + d(0.05f32) / d(heat_capacity) * 2.0f32 * (d(damage) / 10.0f32)).f();
    }
}
