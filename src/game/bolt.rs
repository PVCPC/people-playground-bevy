//! BaseBoltBehaviour's flight: Update steps the bolt along its right vector by Speed * dt * GetSpeedMultiplier(). A
//! Physics2D.Raycast over Layers | EnergyLayers (not material-aware) finds the hit. An energy-layer hit (ShouldReflect)
//! reflects the bolt's right, places it at the hit and it keeps moving this frame; any other hit goes to the subclass's
//! OnHit, after which the bolt is destroyed (the caller does both).
#![allow(non_snake_case)]

use super::explosion::RayHit2D;
use super::translated::rt::Xf;
use super::{d, mathf, Game};
use crate::b2::math::Vec2 as V;

/// Global.main.EnergyLayers (Main.unity): layer 17.
pub(super) const ENERGY_LAYERS: u32 = 1 << 17;

impl Game {
    /// BaseBoltBehaviour.Update's DoHitCheck and step. Returns the hit for the subclass's OnHit (the caller then sets
    /// the position to hit.point and destroys the bolt); None when the bolt moved, or reflected and moved.
    /// `should_reflect` is the subclass's ShouldReflect.
    pub(super) fn base_bolt_move(&mut self, id: i32, speed: f32, layers: u32, multiplier: f32, should_reflect: fn(&Game, &RayHit2D) -> bool) -> Option<RayHit2D> {
        let num = speed * self.time.deltaTime * multiplier;
        let (position, rotation) = self.with_component_instance(id, |_, inst, o| (inst.position(o), inst.rotation(o)));
        let right = mathf::rotate(rotation, [1.0, 0.0, 0.0]);
        let hit = self.raycast(V::new(position[0], position[1]), [right[0], right[1]], num, layers | ENERGY_LAYERS, f32::NEG_INFINITY, f32::INFINITY);
        // A NaN hit point is no hit at all.
        let Some(hit) = hit.filter(|h| !h.point.x.is_nan()) else {
            self.base_bolt_advance(id, num);
            return None;
        };
        if !should_reflect(self, &hit) {
            return Some(hit);
        }
        let normal = [hit.normal.x, hit.normal.y];
        // Vector2.Reflect(transform.right, normal): transform.right = that; transform.position = hit.point (z = 0).
        let dir = [right[0], right[1]];
        let k = (d(-2.0) * (d(normal[0]) * dir[0] + d(normal[1]) * dir[1]).f()).f();
        let reflected = [(d(k) * normal[0] + dir[0]).f(), (d(k) * normal[1] + dir[1]).f()];
        self.tr_set_right(Xf::Object(id), [reflected[0], reflected[1], 0.0]);
        let point = [hit.point.x, hit.point.y, 0.0];
        self.with_component_instance(id, |_, inst, o| inst.set_world_position(o, point));
        self.base_bolt_advance(id, num);
        None
    }

    /// BaseBoltBehaviour.DoHitCheck's raycast for a bolt whose ShouldReflect is false: the hit for OnHit (the caller then
    /// sets the position to hit.point), None for no hit or a NaN hit point.
    pub(super) fn base_bolt_cast(&mut self, id: i32, distance: f32, layers: u32) -> Option<RayHit2D> {
        let (position, rotation) = self.with_component_instance(id, |_, inst, o| (inst.position(o), inst.rotation(o)));
        let right = mathf::rotate(rotation, [1.0, 0.0, 0.0]);
        let hit = self.raycast(V::new(position[0], position[1]), [right[0], right[1]], distance, layers | ENERGY_LAYERS, f32::NEG_INFINITY, f32::INFINITY);
        hit.filter(|h| !h.point.x.is_nan())
    }

    /// transform.position += transform.right * num
    pub(super) fn base_bolt_advance(&mut self, id: i32, num: f32) {
        let (position, rotation) = self.with_component_instance(id, |_, inst, o| (inst.position(o), inst.rotation(o)));
        let right = mathf::rotate(rotation, [1.0, 0.0, 0.0]);
        let to = [position[0] + right[0] * num, position[1] + right[1] * num, position[2] + right[2] * num];
        self.with_component_instance(id, |_, inst, o| inst.set_world_position(o, to));
    }
}
