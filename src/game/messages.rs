//! SendMessage of OnTriggerEnter2D/Stay2D/Exit2D and OnJointBreak2D to the translated scripts (src/game/translated).
//!
//! Native: PhysicsContacts2D::SendCallbackReports (180c15080) sends every trigger report (collider A, collider B,
//! rigidbody A, rigidbody B) as Component::SendMessageAny to A with B as the argument, to rigidbody A (when set,
//! i.e. on another GameObject) with B, then to B with A, then to rigidbody B with A; the argument is the other
//! Collider2D. PhysicsScene2D::UpdateJoints (180c24cc0) sends kJointBreak2D with the Joint2D to the joint's
//! GameObject (GameObject::SendMessageAny) and only then destroys it. SendMessage goes to every MonoBehaviour on
//! the GameObject in component order, disabled ones included.
//!
//! The translated classes that take these are listed by `translated::receives(class, "OnTriggerEnter2D" ...)`;
//! `Game::link` registers each as a `MessageScript` and the call sites in simulate.rs call `trigger_hook` and
//! `joint_break_hook` below, the one place that names the generated `tr_*` functions.

use super::translated::TBehaviour;
use super::*;
use crate::b2::contacts2d::Message;
use crate::b2::world::RigidbodyRef;

/// A translated component that takes OnTriggerEnter2D/Stay2D/Exit2D and/or OnJointBreak2D, with the component ids of
/// its GameObject that select it as a receiver.
#[derive(Clone, Debug)]
pub struct MessageScript {
    /// The instance ids of the Collider2Ds, Rigidbody2Ds and Joint2Ds on its GameObject.
    pub colliders: Vec<i32>,
    pub rigidbodies: Vec<i32>,
    pub joints: Vec<i32>,
    /// Its component index, and whether that comes after the PhysicalBehaviour's (handwritten receivers of the
    /// same message sit with the PhysicalBehaviour/LimbBehaviour/...: translated ones go before or after them).
    pub index: usize,
    pub after_physical: bool,
    pub b: TBehaviour,
    pub trigger: bool,
    pub joint_break: bool,
}

impl Game {
    /// The hook point for the generated per-class dispatch. It becomes
    /// `Enter => self.tr_on_trigger_enter2d(b, collider)`, `Stay => ..._stay2d`, `Exit => ..._exit2d`.
    fn trigger_hook(&mut self, m: Message, b: TBehaviour, collider: i32) {
        match m {
            Message::TriggerEnter => self.tr_on_trigger_enter2d(b, collider),
            Message::TriggerStay => self.tr_on_trigger_stay2d(b, collider),
            Message::TriggerExit => self.tr_on_trigger_exit2d(b, collider),
            _ => unreachable!("{m:?} is not a trigger message"),
        }
    }

    /// The hook point for `self.tr_on_joint_break2d(b, joint)`: `joint` is the Joint2D component's instance id.
    fn joint_break_hook(&mut self, b: TBehaviour, joint: i32) {
        self.tr_on_joint_break2d(b, joint);
    }

    /// One half of a trigger report: `m` to the GameObject of `own` and to that of its rigidbody (when on another
    /// object), argument `other`.
    pub(super) fn send_trigger_message(&mut self, m: Message, own: i32, rigidbody: Option<RigidbodyRef>, other: i32) {
        if self.message_scripts.is_empty() {
            return;
        }
        let mut targets: Vec<(usize, TBehaviour)> = Vec::new();
        for (i, s) in self.message_scripts.iter().enumerate().filter(|(_, s)| s.trigger && s.colliders.contains(&own)) {
            targets.push((i, s.b));
        }
        for (_, b) in targets {
            self.trigger_hook(m, b, other);
        }
        let Some(rb) = rigidbody else { return };
        let targets: Vec<TBehaviour> = self.message_scripts.iter().filter(|s| s.trigger && s.rigidbodies.contains(&rb.id)).map(|s| s.b).collect();
        for b in targets {
            self.trigger_hook(m, b, other);
        }
    }

    /// The Joint2D component id of a joint in the break-limit list, when the rewrite keeps one.
    fn break_joint_component(&self, j: BreakJoint) -> Option<i32> {
        match j {
            BreakJoint::Anchored(a) => Some(self.anchored_joints[a].component),
            BreakJoint::Hinge(h) => self.anchored_joints.iter().find(|a| a.hinge == Some(h)).map(|a| a.component),
            BreakJoint::Spring(_) | BreakJoint::Fixed(_) => None,
        }
    }

    /// SendMessage("OnJointBreak2D", joint) to the translated scripts on the joint's GameObject that sit before
    /// (`after == false`) or after the PhysicalBehaviour in component order, which is where the handwritten
    /// receivers sit.
    pub(super) fn send_joint_break_scripts(&mut self, j: BreakJoint, after: bool) {
        if !self.message_scripts.iter().any(|s| s.joint_break) {
            return;
        }
        let Some(id) = self.break_joint_component(j) else {
            unimplemented!("OnJointBreak2D to translated scripts for {j:?}: the runtime-created joint's GameObject is not tracked");
        };
        let mut targets: Vec<(usize, TBehaviour)> = self.message_scripts.iter().filter(|s| s.joint_break && s.after_physical == after && s.joints.contains(&id)).map(|s| (s.index, s.b)).collect();
        targets.sort_by_key(|&(i, _)| i);
        for (_, b) in targets {
            self.joint_break_hook(b, id);
        }
    }
}
