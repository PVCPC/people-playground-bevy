//! Translated from IndustrialGyrostabiliserBehaviour in Assembly-CSharp.dll by tools/iltrans: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, unreachable_patterns, dead_code, unused_imports, unused_assignments)]

use super::rt;
use crate::assets::db::ObjRef;
use crate::game::explosion::RayHit2D;
use crate::game::{mathf, script, Game};

script! {
    IndustrialGyrostabiliserBehaviour {
        Activated: bool = false,
    }
    links {
        /// The component's instance id.
        this_id: i32 = 0,
        /// Behaviour.enabled.
        enabled: bool = false,
        /// The PhysicalBehaviour on its GameObject (its Transform's pose).
        this_physical: Option<usize> = None,
        /// Its GameObject or a descendant has a Collider2D or Rigidbody2D.
        this_physics: bool = false,
        physicalBehaviour: Option<usize> = None,
        Light: Option<rt::Go> = None,
    }
}

impl Game {
    /// IndustrialGyrostabiliserBehaviour.Awake
    pub fn industrial_gyrostabiliser_behaviour_awake(&mut self, this: usize) {
        self.tr.industrial_gyrostabiliser_behaviour[this].physicalBehaviour = self.tr.industrial_gyrostabiliser_behaviour[this].this_physical;
        return;
    }

    /// IndustrialGyrostabiliserBehaviour.Start
    pub fn industrial_gyrostabiliser_behaviour_start(&mut self, this: usize) {
        self.industrial_gyrostabiliser_behaviour_set_activated(this, self.tr.industrial_gyrostabiliser_behaviour[this].Activated);
        return;
    }

    /// IndustrialGyrostabiliserBehaviour.FixedUpdate
    pub fn industrial_gyrostabiliser_behaviour_fixed_update(&mut self, this: usize) {
        self.physicals[rt::nn(self.tr.industrial_gyrostabiliser_behaviour[this].physicalBehaviour)].rigidbody.bodyType;
        return;
    }

    /// IndustrialGyrostabiliserBehaviour.Use
    pub fn industrial_gyrostabiliser_behaviour_use(&mut self, this: usize, mut activation: rt::Activation) {
        if self.tr.industrial_gyrostabiliser_behaviour[this].enabled {
            self.industrial_gyrostabiliser_behaviour_set_activated(this, !self.tr.industrial_gyrostabiliser_behaviour[this].Activated);
        }
        return;
    }

    /// IndustrialGyrostabiliserBehaviour.SetActivated
    pub fn industrial_gyrostabiliser_behaviour_set_activated(&mut self, this: usize, mut activated: bool) {
        self.tr.industrial_gyrostabiliser_behaviour[this].Activated = activated;
        self.tr_set_active(rt::nn(self.tr.industrial_gyrostabiliser_behaviour[this].Light), self.tr.industrial_gyrostabiliser_behaviour[this].Activated);
        { let v = (if self.tr.industrial_gyrostabiliser_behaviour[this].Activated { 4 } else { 0 }); self.physicals[rt::nn(self.tr.industrial_gyrostabiliser_behaviour[this].physicalBehaviour)].rigidbody.set_constraints(&mut self.world, v); };
        return;
    }

    /// IndustrialGyrostabiliserBehaviour.OnDisable
    pub fn industrial_gyrostabiliser_behaviour_on_disable(&mut self, this: usize) {
        self.industrial_gyrostabiliser_behaviour_set_activated(this, false);
        return;
    }
}
