//! AirfoilBehaviour (the Wing's regular wing and stabiliser): Awake picks the first wing type's name, Start swaps the
//! sprite to the wing type it names, and FixedUpdate applies the lift and drag of the current wing type. The outline
//! refresh and the context menu buttons are UI.
#![allow(non_snake_case)]

use super::physical::dot2;
use super::{d, mathf, script, Game};
use crate::assets::db::ObjRef;

/// AirfoilBehaviour.WingType
#[derive(Clone, Debug)]
pub struct WingType {
    pub Name: String,
    /// Sprite (None for a null one) and the bounds its renderer gives (SpriteRenderer::GetSpriteBounds).
    pub Sprite: Option<(ObjRef, ([f32; 3], [f32; 3]))>,
    pub Lifts: bool,
    pub LiftMultiplier: f32,
    pub DragMultiplier: f32,
}

script! {
    Airfoil {}
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// PhysicalBehaviour: its Rigidbody2D and sprite.
        PhysicalBehaviour: usize = 0,
        /// WingTypes, in serialised order.
        WingTypes: Vec<WingType> = Vec::new(),
        /// CurrentWingTypeName
        CurrentWingTypeName: String = String::new(),
        /// CurrentWingType as an index into WingTypes; None is the serialised default struct (no lift, no drag).
        CurrentWingType: Option<usize> = None,
    }
}

impl Game {
    /// AirfoilBehaviour.Awake
    pub fn airfoil_awake(&mut self, a: usize) {
        if let Some(first) = self.airfoils[a].WingTypes.first() {
            self.airfoils[a].CurrentWingTypeName = first.Name.clone();
        }
    }

    /// AirfoilBehaviour.Start: SetWingType(GetWingType(CurrentWingTypeName)). GetWingType's default struct has no
    /// Sprite, so an unknown name changes nothing. The context menu buttons are UI.
    pub fn airfoil_start(&mut self, a: usize) {
        let s = &self.airfoils[a];
        if let Some(i) = s.WingTypes.iter().position(|w| w.Name == s.CurrentWingTypeName) {
            self.airfoil_set_wing_type(a, i);
        }
    }

    /// AirfoilBehaviour.SetWingType: only a wing type with a Sprite applies. RefreshOutline is UI.
    fn airfoil_set_wing_type(&mut self, a: usize, i: usize) {
        let Some((sprite, bounds)) = self.airfoils[a].WingTypes[i].Sprite.clone() else { return };
        let s = &mut self.airfoils[a];
        s.CurrentWingType = Some(i);
        s.CurrentWingTypeName = s.WingTypes[i].Name.clone();
        let p = &mut self.physicals[s.PhysicalBehaviour];
        // SpriteRenderer.sprite = wingType.Sprite: the physical's renderer, which is on the same object.
        p.sprite.as_mut().expect("AirfoilBehaviour without a SpriteRenderer").0 = sprite;
        p.sprite_bounds = Some(bounds);
    }

    /// AirfoilBehaviour.FixedUpdate: the lift (when the wing type lifts) and the drag, both as forces on the body.
    pub fn airfoil_fixed_update(&mut self, a: usize) {
        let s = &self.airfoils[a];
        let (id, p) = (s.id, s.PhysicalBehaviour);
        // base.transform.up and right: the Vector3s that convert to Vector2 by dropping z.
        let (up, right, scale) = self.with_component_instance(id, |_, inst, o| {
            let q = inst.rotation(o);
            (mathf::rotate(q, [0.0, 1.0, 0.0]), mathf::rotate(q, [1.0, 0.0, 0.0]), inst.objects[o].trs.s)
        });
        let vector = [up[0], up[1]];
        let num = scale[0] * scale[1];
        let rb = self.physicals[p].rigidbody.clone();
        let velocity = rb.velocity(&self.world);
        let magnitude = mathf::magnitude2(velocity);
        let rhs = if magnitude == 0.0 { [1.0, 0.0] } else { [velocity[0] / magnitude, velocity[1] / magnitude] };
        let num2 = dot2(vector, rhs) * -1.0;
        let (lifts, drag) = match self.airfoils[a].CurrentWingType {
            Some(i) => {
                let w = &self.airfoils[a].WingTypes[i];
                (w.Lifts, w.DragMultiplier)
            }
            None => (false, 0.0),
        };
        if lifts {
            let num3 = dot2([right[0], right[1]], rhs);
            let s = (d(drag) * num3 * num * magnitude).f();
            rb.add_force(&mut self.world, [vector[0] * s, vector[1] * s]);
        }
        let s = (d(drag) * num2 * num.abs() * magnitude).f();
        rb.add_force(&mut self.world, [vector[0] * s, vector[1] * s]);
    }
}
