//! LiquidSpreadAoEBehaviour (the Bottle): Spread pours the container's liquids into the BloodContainers of the colliders
//! in a circle (the limbs' only, with LimbsOnly), from DestroyableBehaviour.OnBreak or Start.
#![allow(non_snake_case)]

use super::container::Owner;
use super::Game;
use crate::b2::math::Vec2 as V;

#[derive(Clone, Debug, Default)]
pub struct LiquidSpreadAoE {
    /// The component's instance id.
    pub id: i32,
    /// LiquidContainer: the BloodContainer component's instance id (None: null).
    pub LiquidContainer: Option<i32>,
    pub AmountMultiplier: f32,
    pub LimbsOnly: bool,
    pub SpreadOnStart: bool,
    pub Range: f32,
    pub Mask: u32,
    /// The PhysicalBehaviour on its GameObject (transform.position).
    pub phys: usize,
}

impl Game {
    /// The BloodContainer subclass component with this instance id.
    fn container_by_id(&self, id: i32) -> Owner {
        if let Some(f) = self.flasks.iter().position(|x| x.id == id) {
            return Owner::Flask(f);
        }
        if let Some(s) = self.syringes.iter().position(|x| x.id == id) {
            return Owner::Syringe(s);
        }
        unimplemented!("LiquidSpreadAoEBehaviour.LiquidContainer: BloodContainer component {id} outside the flasks and syringes")
    }

    /// LiquidSpreadAoEBehaviour.Start
    pub fn liquid_spread_start(&mut self, s: usize) {
        if self.liquid_spreads[s].SpreadOnStart {
            self.liquid_spread(self.liquid_spreads[s].id);
        }
    }

    /// LiquidSpreadAoEBehaviour.Spread of component `id`.
    pub fn liquid_spread(&mut self, id: i32) {
        let s = self.liquid_spreads.iter().find(|x| x.id == id).expect("a LiquidSpreadAoEBehaviour").clone();
        let p = self.physicals[s.phys].transform.position;
        let mut hits = self.overlap_circle(V::new(p[0], p[1]), s.Range, s.Mask);
        // Mathf.Min(results.Length, OverlapCircleNonAlloc(...)): the 64-entry buffer keeps the first hits.
        hits.truncate(64);
        let source = s.LiquidContainer.filter(|c| !self.destroyed_components.contains(c));
        let source = self.container_by_id(source.unwrap_or_else(|| unimplemented!("LiquidSpreadAoEBehaviour.Spread with a null LiquidContainer (NullReferenceException)")));
        let amount = self.bc(source).TotalLiquidAmount * s.AmountMultiplier / hits.len() as f32 * 3.0;
        for (q, collider) in hits {
            // collider.TryGetComponent<BloodContainer>(): only a physical's own object holds one here.
            let Some(q) = q.filter(|&q| self.collider_on_physical_object(collider, q)) else {
                if self.collider_object_has_container(collider) {
                    unimplemented!("LiquidSpreadAoEBehaviour: a BloodContainer on a collider's object other than a physical's");
                }
                continue;
            };
            let Some(target) = self.physical_container(q) else { continue };
            if s.LimbsOnly && !self.limbs.iter().any(|l| l.PhysicalBehaviour == q) {
                continue;
            }
            // Adding to a liquid it already holds keeps the dictionary's keys, so enumerating while adding is safe.
            let liquids: Vec<usize> = self.bc(source).LiquidDistribution.keys();
            for liquid in liquids {
                self.add_liquid(target, liquid, amount);
            }
        }
    }

    /// The collider sits on the GameObject of physical `q` itself.
    fn collider_on_physical_object(&mut self, collider: i32, q: usize) -> bool {
        let go = self.with_component_instance(self.physicals[q].id, |_, inst, o| inst.objects[o].game_object);
        self.world.collider_game_object(collider) == Some(go)
    }

    /// A BloodContainer subclass among the scripts of the collider's GameObject.
    fn collider_object_has_container(&mut self, collider: i32) -> bool {
        self.with_component_instance(collider, |_, inst, o| {
            inst.prefab.objects[o].components.iter().any(|(_, c)| matches!(c, crate::assets::db::Component::Script { class, .. } if super::flask::BLOOD_CONTAINERS.contains(&class.as_str())))
        })
    }
}
