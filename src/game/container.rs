//! BloodContainer: the liquid store of CirculationBehaviour and FlaskBehaviour/GasTankBehaviour, with its virtual
//! members dispatched on the subclass that owns it.
#![allow(non_snake_case)]

use super::circulation::{change_red_to_orange, liquid_color, mix_instructions, RefFloat, LIQUIDS};
use super::{d, mathf, script, CsDict, Game};

script! {
    BloodContainer {
        TotalLiquidAmount: f32 = 0.0,
        isComputedColourCached: bool = false,
        cachedComputedColour: [f32; 4] = [0.0, 0.0, 0.0, 0.0],
        MeasuredPressure: f32 = 0.0,
        shouldDeleteEmptyEntries: bool = false,
        timer: f32 = 0.0,
    }
    links {
        LiquidDistribution: CsDict<usize, RefFloat> = CsDict::default(),
    }
}

/// A BloodContainer reference: the subclass component that is it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    /// CirculationBehaviour: the limb index.
    Circulation(usize),
    /// FlaskBehaviour or GasTankBehaviour: the `Game::flasks` index.
    Flask(usize),
}

impl Game {
    pub fn bc(&self, o: Owner) -> &BloodContainer {
        match o {
            Owner::Circulation(l) => &self.circulations[l].base,
            Owner::Flask(f) => &self.flasks[f].base,
        }
    }

    pub fn bc_mut(&mut self, o: Owner) -> &mut BloodContainer {
        match o {
            Owner::Circulation(l) => &mut self.circulations[l].base,
            Owner::Flask(f) => &mut self.flasks[f].base,
        }
    }

    /// Behaviour.enabled. No flask code disables one.
    fn container_enabled(&self, o: Owner) -> bool {
        match o {
            Owner::Circulation(l) => self.circulations[l].enabled,
            Owner::Flask(_) => true,
        }
    }

    /// Limits: CirculationBehaviour's (0, 1), FlaskBehaviour's (0, Capacity), GasTankBehaviour's (0, 20).
    pub fn container_limits(&self, o: Owner) -> [f32; 2] {
        match o {
            Owner::Circulation(_) => [0.0, 1.0],
            Owner::Flask(f) => [0.0, self.flasks[f].limit],
        }
    }

    /// AllowsOverflow: false in FlaskBehaviour and GasTankBehaviour.
    pub fn allows_overflow(&self, o: Owner) -> bool {
        matches!(o, Owner::Circulation(_))
    }

    fn upper_limit(&self, o: Owner) -> f32 {
        if !self.allows_overflow(o) { self.container_limits(o)[1] } else { f32::MAX }
    }

    /// ScaledLiquidAmount
    pub fn scaled_liquid_amount(&self, o: Owner) -> f32 {
        let [x, y] = self.container_limits(o);
        mathf::map_range(x, y, 0.0, 1.0, self.bc(o).TotalLiquidAmount)
    }

    pub fn get_amount(&self, o: Owner, liquid: usize) -> f32 {
        self.bc(o).LiquidDistribution.get(liquid).map_or(0.0, |r| r.raw)
    }

    pub fn add_liquid(&mut self, o: Owner, liquid: usize, amount: f32) -> f32 {
        if amount <= mathf::EPSILON {
            return 0.0;
        }
        let (lower, upper) = (self.container_limits(o)[0], self.upper_limit(o));
        let c = self.bc_mut(o);
        let num = mathf::clamp(c.TotalLiquidAmount + amount, lower, upper) - c.TotalLiquidAmount;
        if num.is_nan() || num <= mathf::EPSILON {
            return 0.0;
        }
        c.isComputedColourCached = false;
        c.TotalLiquidAmount += num;
        if let Some(r) = c.LiquidDistribution.get_mut(liquid) {
            let v = r.raw + num;
            r.set_raw(v);
        } else {
            c.LiquidDistribution.insert(liquid, RefFloat::new(num));
            self.on_liquid_enter(o, liquid);
        }
        num
    }

    pub fn remove_liquid(&mut self, o: Owner, liquid: usize, amount: f32) -> f32 {
        if amount <= mathf::EPSILON {
            return 0.0;
        }
        let (lower, upper) = (self.container_limits(o)[0], self.upper_limit(o));
        let c = self.bc_mut(o);
        let mut num = mathf::clamp(c.TotalLiquidAmount - amount, lower, upper) - c.TotalLiquidAmount;
        if num.is_nan() || num >= -1e-45 {
            return 0.0;
        }
        if let Some(r) = c.LiquidDistribution.get_mut(liquid) {
            if r.raw + num < 0.0 {
                num = 0.0 - r.raw;
            }
            c.TotalLiquidAmount += num;
            let v = r.raw + num;
            r.set_raw(v);
        }
        c.isComputedColourCached = false;
        c.shouldDeleteEmptyEntries = true;
        num
    }

    pub fn drain(&mut self, o: Owner, amount: f32) {
        let count = self.bc(o).LiquidDistribution.len();
        if amount <= mathf::EPSILON || count == 0 {
            return;
        }
        let amount2 = amount / count as f32;
        for liquid in self.bc(o).LiquidDistribution.keys() {
            self.remove_liquid(o, liquid, amount2);
        }
        self.bc_mut(o).shouldDeleteEmptyEntries = true;
    }

    pub fn delete_empty_liquid_entries(&mut self, o: Owner) {
        self.bc_mut(o).shouldDeleteEmptyEntries = false;
        // Liquid.LiquidSet: the registry order.
        for item in 0..LIQUIDS.len() {
            let Some(r) = self.bc(o).LiquidDistribution.get(item).copied() else { continue };
            if r.raw <= 0.02 && (r.raw <= mathf::EPSILON || r.has_decreased() || !r.has_changed()) {
                self.bc_mut(o).TotalLiquidAmount -= r.raw;
                self.on_liquid_exit(o, item);
                self.bc_mut(o).LiquidDistribution.remove(item);
            }
            // The removed RefFloat is still updated; it is unreachable afterwards.
            if let Some(r) = self.bc_mut(o).LiquidDistribution.get_mut(item) {
                r.Previous = r.raw;
            }
        }
    }

    /// TransferTo(Liquid, float, BloodContainer)
    pub fn transfer_liquid_to(&mut self, o: Owner, liquid: usize, amount: f32, target: Owner) -> f32 {
        if !self.container_enabled(o) || !self.container_enabled(target) || amount <= mathf::EPSILON {
            return 0.0;
        }
        let num = self.remove_liquid(o, liquid, amount).abs();
        if num > mathf::EPSILON {
            self.add_liquid(target, liquid, num);
        }
        num
    }

    /// TransferTo(float, BloodContainer, proportional = true)
    pub fn transfer_to(&mut self, o: Owner, amount: f32, target: Owner) -> f32 {
        if !self.container_enabled(o) || !self.container_enabled(target) || amount <= mathf::EPSILON {
            return 0.0;
        }
        let total = self.bc(o).TotalLiquidAmount;
        let mut num = 0.0;
        // The enumerator throws if the dictionary changes; RemoveLiquid only changes values.
        for key in self.bc(o).LiquidDistribution.keys() {
            let raw = self.bc(o).LiquidDistribution.get(key).unwrap().raw;
            // Evaluated in double, rounded at the call.
            let amount2 = (d(amount) * (d(raw) / total)).f();
            num += self.transfer_liquid_to(o, key, amount2, target);
        }
        num
    }

    /// GetComputedColor(Color)
    pub fn get_computed_color(&mut self, o: Owner, fallback: [f32; 4]) -> [f32; 4] {
        let goreless = self.prefs.GorelessMode;
        let c = self.bc_mut(o);
        if c.isComputedColourCached {
            return c.cachedComputedColour;
        }
        if c.LiquidDistribution.len() == 0 || mathf::approximately(0.0, c.TotalLiquidAmount) {
            return fallback;
        }
        let mut v = [0.0f32; 4];
        for (liquid, amount) in c.LiquidDistribution.iter() {
            let num = amount.raw / c.TotalLiquidAmount;
            let color = liquid_color(*liquid);
            for i in 0..4 {
                v[i] += color[i] * num;
            }
        }
        v[3] = mathf::clamp(v[3], 0.1, 1.0);
        for x in &mut v[..3] {
            *x = mathf::clamp(*x, 0.0, 10.0);
        }
        if goreless {
            v = change_red_to_orange(v);
        }
        c.cachedComputedColour = v;
        c.isComputedColourCached = true;
        v
    }

    /// BloodContainer.Update. Pressure is None and AllowPressureTransfer true in every subclass rewritten.
    pub fn blood_container_update(&mut self, o: Owner) {
        let dt = self.time.deltaTime;
        let scaled = self.scaled_liquid_amount(o);
        let c = self.bc_mut(o);
        c.timer += dt;
        // CalculatePressureFromContents
        c.MeasuredPressure = mathf::lerp(c.MeasuredPressure, scaled, mathf::lerp_factor_dt(0.6, dt));
        if c.timer > 1.0 {
            c.timer = 0.0;
            self.delete_empty_liquid_entries(o);
            for liquid in self.bc(o).LiquidDistribution.keys() {
                self.liquid_on_update(liquid, o);
            }
            for (sources, target, rate, outside_circulations) in mix_instructions() {
                // ContainerFilter: !(c is CirculationBehaviour)
                if outside_circulations && matches!(o, Owner::Circulation(_)) {
                    continue;
                }
                // Utils.LiquidMixProcess(container, Liquid[], result, delta)
                if sources.iter().any(|&s| self.get_amount(o, s) < rate) {
                    continue;
                }
                for &s in &sources {
                    self.remove_liquid(o, s, rate);
                }
                self.add_liquid(o, target, rate * sources.len() as f32);
            }
        }
        if self.bc(o).shouldDeleteEmptyEntries {
            self.delete_empty_liquid_entries(o);
        }
    }

    /// OnLiquidEnter: OnDistributionChange (no subclass rewritten has a persistent listener), the liquid's
    /// OnEnterContainer, and CirculationBehaviour's OnEnterLimb.
    fn on_liquid_enter(&mut self, o: Owner, liquid: usize) {
        self.liquid_on_enter_container(liquid, o);
        if let Owner::Circulation(l) = o {
            self.liquid_on_enter_limb(liquid, l);
        }
    }

    /// OnLiquidExit
    fn on_liquid_exit(&mut self, o: Owner, liquid: usize) {
        self.liquid_on_exit_container(liquid, o);
    }

    // ---- Liquid's virtual hooks ----

    fn liquid_unwritten(liquid: usize, hook: &str) -> ! {
        unimplemented!("{}.{hook}", LIQUIDS[liquid].1)
    }

    /// Liquid.OnEnterContainer: empty in Blood, GorseBlood and Oil.
    fn liquid_on_enter_container(&mut self, liquid: usize, _o: Owner) {
        match LIQUIDS[liquid].1 {
            "Blood" | "GorseBlood" | "Oil" => {}
            _ => Self::liquid_unwritten(liquid, "OnEnterContainer"),
        }
    }

    /// Liquid.OnExitContainer: empty in Blood, GorseBlood and Oil.
    fn liquid_on_exit_container(&mut self, liquid: usize, _o: Owner) {
        match LIQUIDS[liquid].1 {
            "Blood" | "GorseBlood" | "Oil" => {}
            _ => Self::liquid_unwritten(liquid, "OnExitContainer"),
        }
    }

    /// Liquid.OnEnterLimb
    fn liquid_on_enter_limb(&mut self, liquid: usize, l: usize) {
        match LIQUIDS[liquid].1 {
            "Blood" => {}
            "GorseBlood" if self.limbs[l].SpeciesIdentity == "Gorse" => {}
            "GorseBlood" => unimplemented!("GorseBlood.AcidRoutine (a coroutine on the limb's CirculationBehaviour)"),
            "Oil" => {
                if self.limbs[l].SpeciesIdentity != "Android" {
                    self.limbs[l].Numbness += 0.5;
                    self.circulations[l].IsPump = false;
                }
            }
            _ => Self::liquid_unwritten(liquid, "OnEnterLimb"),
        }
    }

    /// Liquid.OnUpdate
    fn liquid_on_update(&mut self, liquid: usize, o: Owner) {
        match LIQUIDS[liquid].1 {
            // Blood.OnUpdate (GorseBlood inherits it)
            "Blood" | "GorseBlood" => {
                if let Owner::Circulation(l) = o
                    && self.limbs[l].SpeciesIdentity == "Android"
                    && self.random.value() > 0.995
                {
                    let p = self.limbs[l].PhysicalBehaviour;
                    let c = self.physicals[p].Charge() + 5.0;
                    self.physicals[p].charge = c;
                }
            }
            "Oil" => {
                if let Owner::Circulation(l) = o {
                    self.liquid_on_enter_limb(liquid, l);
                }
            }
            _ => Self::liquid_unwritten(liquid, "OnUpdate"),
        }
    }
}
