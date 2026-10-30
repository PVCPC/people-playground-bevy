//! Translated from TargetLineRenderer in Assembly-CSharp.dll by tools/iltrans: do not edit.
#![allow(non_snake_case, unused_mut, unused_variables, unused_labels, unused_parens, unused_braces, unreachable_code, unreachable_patterns, dead_code, unused_imports, unused_assignments)]

use super::rt;
use crate::assets::db::ObjRef;
use crate::game::explosion::RayHit2D;
use crate::game::{mathf, script, Game};

script! {
    TargetLineRenderer {
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
        LineRenderer: bool = false,
        Target: Option<rt::Xf> = None,
    }
}

impl Game {
    /// TargetLineRenderer.Awake
    pub fn target_line_renderer_awake(&mut self, this: usize) {
        { rt::pres(self.tr.target_line_renderer[this].LineRenderer); };
        { rt::pres(self.tr.target_line_renderer[this].LineRenderer); };
        { let _ = rt::v3([0.0, 0.0]); rt::pres(self.tr.target_line_renderer[this].LineRenderer); };
        self.target_line_renderer_update(this);
        return;
    }

    /// TargetLineRenderer.Update
    pub fn target_line_renderer_update(&mut self, this: usize) {
        { let _ = { let a1 = self.tr_position(rt::nn(self.tr.target_line_renderer[this].Target)); self.tr_inverse_transform_point(rt::nn(Some(rt::Xf::this(self.tr.target_line_renderer[this].this_physical, self.tr.target_line_renderer[this].this_physics))), a1) }; rt::pres(self.tr.target_line_renderer[this].LineRenderer); };
        return;
    }
}
