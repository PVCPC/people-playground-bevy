//! GasTankBehaviour (a FlaskBehaviour, hence a BloodContainer outside a limb). Only OIL, whose container hooks are
//! empty outside a CirculationBehaviour, can enter one; any other liquid stops at its hook.
#![allow(non_snake_case)]

use super::circulation::{mix_instructions, RefFloat, LIQUIDS};
use super::{mathf, script, CsDict, Game};

script! {
    Flask {
        MeasuredPressure: f32 = 0.0,
        Capacity: f32 = 2.8,
        NewlySpawned: bool = true,
        TotalLiquidAmount: f32 = 0.0,
        isComputedColourCached: bool = false,
        shouldDeleteEmptyEntries: bool = false,
        timer: f32 = 0.0,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// Limits.y of GasTankBehaviour (Limits is (0, 20)); AllowsOverflow is false.
        limit: f32 = 20.0,
        /// StartLiquid
        StartLiquid: String = String::new(),
        StartAmount: f32 = 0.0,
        LiquidDistribution: CsDict<usize, RefFloat> = CsDict::default(),
    }
}

impl Game {
    /// Liquid.GetLiquid(id) for a liquid whose BloodContainer hooks outside a limb are empty.
    fn flask_liquid(id: &str) -> usize {
        let l = LIQUIDS.iter().position(|(n, _)| *n == id).unwrap_or_else(|| unimplemented!("Liquid.GetLiquid({id}) is null"));
        Self::flask_hooks_empty(l);
        l
    }

    fn flask_hooks_empty(l: usize) {
        if LIQUIDS[l].0 != "OIL" {
            unimplemented!("{}.OnEnterContainer/OnUpdate/OnExitContainer in a FlaskBehaviour", LIQUIDS[l].1);
        }
    }

    /// FlaskBehaviour.Start
    pub fn flask_start(&mut self, f: usize) {
        let s = &self.flasks[f];
        if s.NewlySpawned && s.StartAmount > mathf::EPSILON {
            let liquid = Self::flask_liquid(&s.StartLiquid.clone());
            let amount = s.StartAmount;
            self.flask_add_liquid(f, liquid, amount);
        }
        self.flasks[f].NewlySpawned = false;
    }

    /// BloodContainer.AddLiquid: LowerLimit is Limits.x (0), UpperLimit Limits.y without overflow.
    fn flask_add_liquid(&mut self, f: usize, liquid: usize, amount: f32) -> f32 {
        if amount <= mathf::EPSILON {
            return 0.0;
        }
        let s = &mut self.flasks[f];
        let num = mathf::clamp(s.TotalLiquidAmount + amount, 0.0, s.limit) - s.TotalLiquidAmount;
        if num.is_nan() || num <= mathf::EPSILON {
            return 0.0;
        }
        s.isComputedColourCached = false;
        s.TotalLiquidAmount += num;
        if let Some(r) = s.LiquidDistribution.get_mut(liquid) {
            let v = r.raw + num;
            r.set_raw(v);
        } else {
            s.LiquidDistribution.insert(liquid, RefFloat::new(num));
            // OnLiquidEnter: OnDistributionChange (no persistent calls), Liquid.OnEnterContainer.
            Self::flask_hooks_empty(liquid);
        }
        num
    }

    /// BloodContainer.DeleteEmptyLiquidEntries
    fn flask_delete_empty_liquid_entries(&mut self, f: usize) {
        let s = &mut self.flasks[f];
        s.shouldDeleteEmptyEntries = false;
        for item in 0..LIQUIDS.len() {
            let Some(r) = s.LiquidDistribution.get(item).copied() else { continue };
            if r.raw <= 0.02 && (r.raw <= mathf::EPSILON || r.has_decreased() || !r.has_changed()) {
                s.TotalLiquidAmount -= r.raw;
                // OnLiquidExit
                Self::flask_hooks_empty(item);
                s.LiquidDistribution.remove(item);
            }
            if let Some(r) = s.LiquidDistribution.get_mut(item) {
                r.Previous = r.raw;
            }
        }
    }

    /// FlaskBehaviour.Update: BloodContainer.Update (the LiquidContainer's fill and colour are rendering, and null on
    /// the prefabs here).
    pub fn flask_update(&mut self, f: usize) {
        let dt = self.time.deltaTime;
        let s = &mut self.flasks[f];
        s.timer += dt;
        // CalculatePressureFromContents = ScaledLiquidAmount; AllowPressureTransfer, Pressure None.
        let scaled = mathf::map_range(0.0, s.limit, 0.0, 1.0, s.TotalLiquidAmount);
        s.MeasuredPressure = mathf::lerp(s.MeasuredPressure, scaled, mathf::lerp_factor_dt(0.6, dt));
        if s.timer > 1.0 {
            s.timer = 0.0;
            self.flask_delete_empty_liquid_entries(f);
            // liquid.OnUpdate(container): empty outside a CirculationBehaviour.
            for liquid in self.flasks[f].LiquidDistribution.keys() {
                Self::flask_hooks_empty(liquid);
            }
            // The mixes: no ContainerFilter rejects a container that is not a CirculationBehaviour.
            for (sources, _target, rate, _outside_only) in mix_instructions() {
                let s = &self.flasks[f];
                if sources.iter().all(|&l| s.LiquidDistribution.get(l).map_or(0.0, |r| r.raw) >= rate) {
                    unimplemented!("Utils.LiquidMixProcess in a FlaskBehaviour");
                }
            }
        }
        if self.flasks[f].shouldDeleteEmptyEntries {
            self.flask_delete_empty_liquid_entries(f);
        }
    }
}
