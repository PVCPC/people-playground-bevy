//! BloodContainer, CirculationBehaviour and the Liquid registry.
use super::{d, mathf, script, CsDict, Game};
use super::misc::{BleedingParticle, FancyBloodSplat, FixedIntervalDistributor};

/// Liquid.Registry / Liquid.LiquidSet order (Global.Awake); a liquid is its index here.
pub const LIQUIDS: &[(&str, &str)] = &[
    ("BLOOD", "Blood"),
    ("GORSE BLOOD", "GorseBlood"),
    ("OIL", "Oil"),
    ("NITRO", "Nitroglycerine"),
    ("EXOTIC LIQUID", "ExoticLiquid"),
    ("TRITIUM", "Tritium"),
    ("INERT LIQUID", "InertLiquid"),
    ("BEVERAGE M04", "BeverageM04"),
    ("COOLANT", "Coolant"),
    ("REANIMATION AGENT", "ZombiePoisonLiquid"),
    ("ADRENALINE", "AdrenalineLiquid"),
    ("ACID", "AcidLiquid"),
    ("BONE EATING POISON", "BoneHurtingJuiceLiquid"),
    ("COAGULATION SERUM", "CoagulationLiquid"),
    ("INSTANT DEATH POISON", "InstantDeathPoisonLiquid"),
    ("FREEZE POISON", "FreezePoisonLiquid"),
    ("KNOCKOUT POISON", "KnockoutPoisonLiquid"),
    ("LIFE SERUM", "LifeSerumLiquid"),
    ("ULTRA STRENGTH SERUM", "UltraStrengthSerumLiquid"),
    ("MENDING SERUM", "MendingSerum"),
    ("WATER BREATHING SERUM", "WaterBreathingSerum"),
    ("OSTEOMORPHOSIS AGENT", "OsteomorphosisAgent"),
    ("IMMORTALITY SERUM", "ImmortalitySerum"),
    ("PAIN KILLER", "PainKillerLiquid"),
    ("INERT PINK LIQUID", "PinkDormantLiquid"),
    ("VESTIBULAR POISON", "VestibularPoison"),
    ("MUSCLE POISON", "MusclePoison"),
    ("NUMBING POISON", "NumbingPoison"),
    ("MIRRORISING AGENT", "ReflectionPoison"),
    ("TRANSPARENCY AGENT", "TransparencyPoison"),
    ("MASS AGENT", "MassManipulationPoison"),
    ("EXPLOSION POISON", "ExplosionPoison"),
    ("CRUSHING POISON", "CrushingPoison"),
    ("DURABILITY SERUM", "DurabilitySerum"),
    ("REGENERATION SERUM", "RegenerationSerum"),
    ("DISTORTION POISON", "SizeManipulationPoison"),
    ("CIRCULATION POISON", "CirculationPoison"),
    ("COMBUSTION AGENT", "CombustionAgent"),
    ("ENHANCING SERUM", "MuscleEnhancementSerum"),
    ("TISSUE DECONSTRUCTION AGENT", "TissueDeconstructionAgent"),
    ("DEBUG LIQUID 001", "DebugDiscolorationLiquid"),
];

/// Liquid.Color per LIQUIDS entry, as each Liquid constructor builds it.
pub fn liquid_color(l: usize) -> [f32; 4] {
    const PINK: [f32; 4] = [1.0, 0.4196079, 0.7944902, 1.0];
    match LIQUIDS[l].1 {
        "Blood" => [0.65, 0.0, 0.0, 1.0],
        "GorseBlood" => [0.03, 0.57, 0.0, 1.0],
        "Oil" => [0.0, 0.04689861, 0.1803922, 1.0],
        "Nitroglycerine" => [245.0 / 255.0, 152.0 / 255.0, 66.0 / 255.0, 1.0],
        "ExoticLiquid" => [0.0, 0.0, 1.0, 1.0],
        "Tritium" => [0.08 * 2.0, 0.62 * 2.0, 0.28 * 2.0, 2.0],
        "InertLiquid" => [1.0, 1.0, 1.0, 0.1],
        "BeverageM04" => [0.91, 0.25, 0.0, 0.5],
        "Coolant" => [0.0, 0.22, 0.6, 0.74],
        "ZombiePoisonLiquid" => [0.8835177, 1.0, 0.7122642, 1.0],
        "AdrenalineLiquid" => [0.45, 0.27, 0.77, 1.0],
        "AcidLiquid" => [0.49, 0.44, 0.52, 1.0],
        "BoneHurtingJuiceLiquid" => [1.0, 0.55, 0.22, 1.0],
        "CoagulationLiquid" => [1.0, 1.0, 0.0, 1.0],
        "InstantDeathPoisonLiquid" => [1.0, 0.65, 0.63, 1.0],
        "FreezePoisonLiquid" => [0.8, 1.0, 1.0, 1.0],
        "KnockoutPoisonLiquid" => [0.68, 0.62, 0.541, 1.0],
        "LifeSerumLiquid" => [0.6745283, 1.0, 0.7830077, 1.0],
        "UltraStrengthSerumLiquid" => [0.7604228, 0.7137255, 1.0, 1.0],
        "MendingSerum" => [0.0, 0.67, 0.5, 1.0],
        "WaterBreathingSerum" => [0.21, 0.57, 0.96, 1.0],
        "OsteomorphosisAgent" => [1.0, 0.84, 0.56, 1.0],
        "ImmortalitySerum" => [0.87, 1.0, 0.0, 1.0],
        "PainKillerLiquid" => [0.71, 0.75, 0.65, 0.1],
        "DebugDiscolorationLiquid" => [1.0, 0.0, 1.0, 1.0],
        // PinkSyringe.PinkSyringeColor
        _ => PINK,
    }
}

/// Color.RGBToHSV
pub fn rgb_to_hsv(c: [f32; 4]) -> [f32; 3] {
    let (offset, dominant, one, two) = if c[2] > c[1] && c[2] > c[0] {
        (4.0, c[2], c[0], c[1])
    } else if c[1] > c[0] {
        (2.0, c[1], c[2], c[0])
    } else {
        (0.0, c[0], c[1], c[2])
    };
    let v = dominant;
    if v == 0.0 {
        return [0.0, 0.0, v];
    }
    let num = if one > two { two } else { one };
    let num2 = v - num;
    let (mut h, s) = if num2 != 0.0 {
        ((d(offset) + (d(one) - two) / num2).f(), num2 / v)
    } else {
        ((d(offset) + (d(one) - two)).f(), 0.0)
    };
    h /= 6.0;
    if h < 0.0 {
        h += 1.0;
    }
    [h, s, v]
}

pub fn liquid(id: &str) -> usize {
    LIQUIDS.iter().position(|(n, _)| *n == id).unwrap_or_else(|| panic!("no liquid {id}"))
}

/// LiquidMixingController.MixInstructions: (sources, target, rate, only outside circulations).
pub fn mix_instructions() -> Vec<(Vec<usize>, usize, f32, bool)> {
    let l = liquid;
    vec![
        (vec![l("TRITIUM"), l("MENDING SERUM")], l("IMMORTALITY SERUM"), 0.01, true),
        (vec![l("ACID"), l("MENDING SERUM")], l("OSTEOMORPHOSIS AGENT"), 0.05, true),
        (vec![l("WATER BREATHING SERUM"), l("TRITIUM")], l("EXOTIC LIQUID"), 0.05, false),
        (vec![l("ADRENALINE"), l("MENDING SERUM"), l("INERT LIQUID")], l("BEVERAGE M04"), 0.05, false),
    ]
}

/// BloodContainer.RefFloat
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RefFloat {
    pub Previous: f32,
    pub raw: f32,
}

impl RefFloat {
    pub fn new(v: f32) -> Self {
        let mut r = Self { Previous: 0.0, raw: 0.0 };
        r.set_raw(v);
        r
    }
    pub fn set_raw(&mut self, v: f32) {
        self.raw = mathf::clamp(v, 0.0, f32::MAX);
    }
    pub fn has_decreased(&self) -> bool {
        self.Previous - self.raw > 0.0001
    }
    pub fn has_changed(&self) -> bool {
        (self.Previous - self.raw).abs() > 0.0001
    }
}

script! {
    Circulation {
        IsPump: bool = false,
        BloodLossRateMultiplier: f32 = 1.0,
        BleedingRate: f32 = 0.0,
        IsDisconnected: bool = false,
        WasInitiallyPumping: bool = false,
        BloodFlow: f32 = 0.0,
        BloodRegenerationPerSecond: f32 = 0.0025,
        BleedingPointCount: u8 = 0,
        StabWoundCount: u16 = 0,
        GunshotWoundCount: u16 = 0,
        InternalBleedingIntensity: f32 = 0.0,
        ImmuneToDamage: bool = false,
        diffuseOscillator: bool = false,
        canUnlockGlowingHumanAchievement: bool = true,
        NewlySpawned: bool = true,
        ArtificialHeartbeat: f32 = 0.0,
        cachedHeartRate: Option<f32> = None,
        TotalLiquidAmount: f32 = 0.0,
        isComputedColourCached: bool = false,
        cachedComputedColour: [f32; 4] = [0.0, 0.0, 0.0, 0.0],
        MeasuredPressure: f32 = 0.0,
        shouldDeleteEmptyEntries: bool = false,
        timer: f32 = 0.0,
    }
    links {
        /// Source / PushesTo as limb indices.
        Source: Option<usize> = None,
        PushesTo: Vec<usize> = Vec::new(),
        LiquidDistribution: CsDict<usize, RefFloat> = CsDict::default(),
        /// Bounds (center xyz, extents xyz) in limb space.
        LocalArteryRects: Option<Vec<[f32; 6]>> = None,
        /// Bleeding particle indices (Game::bleeding).
        BleedingParticles: Vec<usize> = Vec::new(),
        enabled: bool = true,
    }
}

/// Liquid virtual hooks. Only blood exists until syringes are rewritten.
fn liquid_hook_unwritten(l: usize, hook: &str) -> ! {
    // ponytail: non-blood liquid behaviours arrive with syringes and containers.
    unimplemented!("{}.{hook}", LIQUIDS[l].1)
}

impl Game {
    fn blood_type(&self, l: usize) -> usize {
        liquid(&self.limbs[l].BloodLiquidType)
    }

    /// CirculationBehaviour.GetAmountOfBlood
    pub fn amount_of_blood(&self, l: usize) -> f32 {
        let c = &self.circulations[l];
        c.LiquidDistribution.get(self.blood_type(l)).map_or(0.0, |r| r.raw)
    }

    /// CirculationBehaviour.HasCirculation
    pub fn has_circulation(&self, l: usize) -> bool {
        self.circulations[l].BloodFlow > 0.01 && self.amount_of_blood(l) > 0.01
    }

    /// CirculationBehaviour.GetHeartRate
    pub fn heart_rate(&mut self, l: usize) -> f32 {
        if !self.circulations[l].IsPump || !self.limb_is_considered_alive(l) {
            return self.circulations[l].ArtificialHeartbeat;
        }
        if let Some(v) = self.circulations[l].cachedHeartRate {
            return v;
        }
        let connected = self.nodes[l].IsConnectedToRoot;
        let c = &self.circulations[l];
        let p = &self.people[self.limbs[l].Person];
        let num = mathf::clamp01(c.BloodFlow);
        let t = mathf::clamp01(c.TotalLiquidAmount);
        let pick = |v: f32| if connected { mathf::clamp01(v) } else { 0.0 };
        let (t2, num2, num3, num4, num5) =
            (pick(p.OxygenLevel), pick(p.Consciousness), pick(p.PainLevel), pick(p.AdrenalineLevel), pick(p.ShockLevel));
        let mut num6 = mathf::lerp(0.0, 95.0, num4 + num5 + num3);
        num6 += if connected { mathf::clamp(p.SeizureTime * 0.5, 0.0, 30.0) } else { 0.0 };
        num6 = (d(num6) + d(mathf::perlin(self.time.time / 7.0, self.limbs[l].randomOffset)) * 10.0f32).f();
        let v = mathf::max(
            c.ArtificialHeartbeat,
            (mathf::lerp(0.0, 70.0 - (1.0 - num2) * 22.0, t) + num6) * mathf::lerp(0.5, 1.0, t2) * num,
        );
        self.circulations[l].cachedHeartRate = Some(v);
        v
    }

    // ---- BloodContainer (Limits (0, 1), AllowsOverflow true, Pressure None) ----

    pub fn add_liquid(&mut self, l: usize, liquid: usize, amount: f32) -> f32 {
        if amount <= mathf::EPSILON {
            return 0.0;
        }
        let c = &mut self.circulations[l];
        let num = mathf::clamp(c.TotalLiquidAmount + amount, 0.0, f32::MAX) - c.TotalLiquidAmount;
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
            // OnLiquidEnter: OnDistributionChange (no listeners), OnEnterContainer, OnEnterLimb.
            if liquid != 0 {
                liquid_hook_unwritten(liquid, "OnEnterContainer");
            }
        }
        num
    }

    pub fn remove_liquid(&mut self, l: usize, liquid: usize, amount: f32) -> f32 {
        if amount <= mathf::EPSILON {
            return 0.0;
        }
        let c = &mut self.circulations[l];
        let mut num = mathf::clamp(c.TotalLiquidAmount - amount, 0.0, f32::MAX) - c.TotalLiquidAmount;
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

    pub fn drain(&mut self, l: usize, amount: f32) {
        let count = self.circulations[l].LiquidDistribution.len();
        if amount <= mathf::EPSILON || count == 0 {
            return;
        }
        let amount2 = amount / count as f32;
        for liquid in self.circulations[l].LiquidDistribution.keys() {
            self.remove_liquid(l, liquid, amount2);
        }
        self.circulations[l].shouldDeleteEmptyEntries = true;
    }

    pub fn delete_empty_liquid_entries(&mut self, l: usize) {
        let c = &mut self.circulations[l];
        c.shouldDeleteEmptyEntries = false;
        for item in 0..LIQUIDS.len() {
            let Some(r) = c.LiquidDistribution.get(item).copied() else { continue };
            if r.raw <= 0.02 && (r.raw <= mathf::EPSILON || r.has_decreased() || !r.has_changed()) {
                c.TotalLiquidAmount -= r.raw;
                if item != 0 {
                    liquid_hook_unwritten(item, "OnExitContainer");
                }
                c.LiquidDistribution.remove(item);
            }
            // The removed RefFloat is still updated; it is unreachable afterwards.
            if let Some(r) = c.LiquidDistribution.get_mut(item) {
                r.Previous = r.raw;
            }
        }
    }

    /// BloodContainer.TransferTo(Liquid, float, BloodContainer)
    pub fn transfer_liquid_to(&mut self, from: usize, liquid: usize, amount: f32, to: usize) -> f32 {
        if !self.circulations[from].enabled || !self.circulations[to].enabled || amount <= mathf::EPSILON {
            return 0.0;
        }
        let num = self.remove_liquid(from, liquid, amount).abs();
        if num > mathf::EPSILON {
            self.add_liquid(to, liquid, num);
        }
        num
    }

    /// BloodContainer.TransferTo(float, BloodContainer, proportional = true)
    pub fn transfer_to(&mut self, from: usize, amount: f32, to: usize) -> f32 {
        if !self.circulations[from].enabled || !self.circulations[to].enabled || amount <= mathf::EPSILON {
            return 0.0;
        }
        let total = self.circulations[from].TotalLiquidAmount;
        let mut num = 0.0;
        // The enumerator throws if the dictionary changes; a removal never happens here.
        let entries: Vec<(usize, f32)> = self.circulations[from].LiquidDistribution.iter().map(|(k, v)| (*k, v.raw)).collect();
        for (key, _) in entries {
            let raw = self.circulations[from].LiquidDistribution.get(key).unwrap().raw;
            let amount2 = amount * (raw / total);
            num += self.transfer_liquid_to(from, key, amount2, to);
        }
        num
    }

    /// BloodContainer.Update
    /// BloodContainer.GetComputedColor(Color)
    pub fn get_computed_color(&mut self, l: usize, fallback: [f32; 4]) -> [f32; 4] {
        let c = &mut self.circulations[l];
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
        for i in 0..3 {
            v[i] = mathf::clamp(v[i], 0.0, 10.0);
        }
        if self.prefs.GorelessMode {
            // Utils.ChangeRedToOrange
            let h = rgb_to_hsv(v)[0];
            if (h - 0.0).abs() < 0.03 || (h - 1.0).abs() < 0.03 {
                v = [0.87, 0.6, 0.18, 1.0];
            }
        }
        c.cachedComputedColour = v;
        c.isComputedColourCached = true;
        v
    }

    pub fn blood_container_update(&mut self, l: usize) {
        let dt = self.time.deltaTime;
        let c = &mut self.circulations[l];
        c.timer += dt;
        // ScaledLiquidAmount = MapRange(0, 1, 0, 1, TotalLiquidAmount)
        let scaled = mathf::map_range(0.0, 1.0, 0.0, 1.0, c.TotalLiquidAmount);
        c.MeasuredPressure = mathf::lerp(c.MeasuredPressure, scaled, mathf::lerp_factor_dt(0.6, dt));
        if c.timer > 1.0 {
            c.timer = 0.0;
            self.delete_empty_liquid_entries(l);
            for liquid in self.circulations[l].LiquidDistribution.keys() {
                if liquid == 0 {
                    // Blood.OnUpdate
                    if self.limbs[l].SpeciesIdentity == "Android" && self.random.value() > 0.995 {
                        let p = self.limbs[l].PhysicalBehaviour;
                        let c = self.physicals[p].Charge() + 5.0;
                        self.physicals[p].charge = c;
                    }
                } else {
                    liquid_hook_unwritten(liquid, "OnUpdate");
                }
            }
            for (sources, target, rate, outside_only) in mix_instructions() {
                let _ = target;
                if outside_only {
                    continue;
                }
                // Utils.LiquidMixProcess(container, Liquid[], result, delta)
                if sources.iter().any(|&s| self.circulations[l].LiquidDistribution.get(s).map_or(0.0, |r| r.raw) < rate) {
                    continue;
                }
                for &s in &sources {
                    self.remove_liquid(l, s, rate);
                }
                self.add_liquid(l, target, rate * sources.len() as f32);
            }
        }
        if self.circulations[l].shouldDeleteEmptyEntries {
            self.delete_empty_liquid_entries(l);
        }
    }

    // ---- CirculationBehaviour ----

    /// CirculationBehaviour.FixedUpdate
    pub fn circulation_fixed_update(&mut self, l: usize) {
        self.circulations[l].cachedHeartRate = None;
        let fdt = self.time.fixedDeltaTime;
        if self.limbs[l].InternalTemperature <= self.limbs[l].BodyTemperature - 9.0 && self.random.value() > 0.999 {
            self.circulations[l].IsPump = false;
        }
        let person = self.limbs[l].Person;
        if self.nodes[l].IsConnectedToRoot && self.people[person].SeizureTime > 0.0 && self.random.value() > 0.999 {
            self.circulations[l].IsPump = false;
        }
        self.pump_behaviour(l, fdt);
        self.handle_bleeding(l, fdt);
        // HandlePenetrationBleeding: victimPenetrations stay empty until stabbing is rewritten.
        assert!(self.physicals[self.limbs[l].PhysicalBehaviour].victimPenetrations.is_empty());
        self.handle_damage_edge_cases(l);
        let c = &mut self.circulations[l];
        c.BloodFlow = mathf::clamp01(c.BloodFlow);
        c.InternalBleedingIntensity = mathf::max(0.0, c.InternalBleedingIntensity - fdt * 0.01);
        self.diffuse_step(l);
        self.flow_step(l);
        let c = &self.circulations[l];
        if !c.IsDisconnected
            && c.BleedingRate < 1.0
            && c.BloodRegenerationPerSecond >= mathf::EPSILON
            && self.limbs[l].Health > 0.1
            && self.amount_of_blood(l) < 1.0
        {
            let amount = c.BloodRegenerationPerSecond * fdt;
            let blood = self.blood_type(l);
            self.add_liquid(l, blood, amount);
        }
        // GLOWING_HUMAN achievement: Steam only.
    }

    fn pump_behaviour(&mut self, l: usize, dt: f32) {
        let charge = self.physicals[self.limbs[l].PhysicalBehaviour].Charge();
        if self.circulations[l].IsPump {
            if charge > 0.5 && self.random.value() as f64 > 0.995 - (charge / 500.0) as f64 {
                self.circulations[l].IsPump = false;
            }
        } else if self.circulations[l].WasInitiallyPumping
            && self.random.value() as f64 > 0.999
            && charge > 0.001
            && self.circulations[l].BloodFlow < 0.1
        {
            self.circulations[l].IsPump = true;
        }
        let c = &mut self.circulations[l];
        if c.IsPump {
            c.BloodFlow = c.TotalLiquidAmount;
        } else if c.WasInitiallyPumping {
            c.BloodFlow = (d(c.BloodFlow) - d(dt) * 0.2f32).f();
        } else {
            c.BloodFlow = (d(c.BloodFlow) - d(dt) * 0.05f32).f();
        }
        c.BloodFlow = mathf::clamp(c.BloodFlow, 0.0, c.TotalLiquidAmount);
    }

    fn handle_damage_edge_cases(&mut self, l: usize) {
        let ph = &self.physicals[self.limbs[l].PhysicalBehaviour];
        let num = mathf::max(ph.BurnProgress, self.skins[l].acidProgress);
        if num > 0.8 && ph.Temperature > 0.0 {
            self.drain(l, mathf::lerp(0.02, 0.1, mathf::map_range(0.8, 1.0, 0.0, 1.0, num)));
        }
    }

    fn handle_bleeding(&mut self, l: usize, dt: f32) {
        let temperature = self.physicals[self.limbs[l].PhysicalBehaviour].Temperature;
        let c = &mut self.circulations[l];
        if c.BleedingRate > 0.05 && temperature > 0.0 {
            if c.BleedingRate < 1.0 {
                c.BleedingRate = (d(c.BleedingRate) - d(self.time.deltaTime) * 0.05f32).f();
            }
            let amount = dt / mathf::lerp(60.0, 20.0, c.BloodFlow) * c.BleedingRate * 0.2;
            self.drain(l, amount);
        }
    }

    fn flow_step(&mut self, l: usize) {
        for i in 0..self.circulations[l].PushesTo.len() {
            let other = self.circulations[l].PushesTo[i];
            let flow = self.circulations[l].BloodFlow;
            if !self.physicals[self.limbs[other].PhysicalBehaviour].isDisintegrated
                && self.node_is_connected_to(l, other)
                && self.circulations[other].BloodFlow < flow
                && !self.circulations[other].IsDisconnected
            {
                let c = &mut self.circulations[other];
                c.BloodFlow = mathf::lerp(c.BloodFlow, flow, 0.9);
            }
        }
    }

    fn diffuse_step(&mut self, l: usize) {
        let rate = self.time.fixedDeltaTime * 1.5;
        let n = self.limbs[l].ConnectedLimbs.len();
        if self.circulations[l].diffuseOscillator {
            for i in 0..n {
                self.diffuse_to_limb(l, self.limbs[l].ConnectedLimbs[i], rate);
            }
        } else {
            for i in (0..n).rev() {
                self.diffuse_to_limb(l, self.limbs[l].ConnectedLimbs[i], rate);
            }
        }
        let c = &mut self.circulations[l];
        c.diffuseOscillator = !c.diffuseOscillator;
    }

    fn diffuse_to_limb(&mut self, l: usize, other: usize, rate: f32) {
        if !self.physicals[self.limbs[other].PhysicalBehaviour].isDisintegrated && other != l && self.node_is_connected_to(l, other) {
            if !(self.circulations[other].TotalLiquidAmount > self.circulations[l].TotalLiquidAmount) {
                self.transfer_to(l, rate, other);
            }
        }
    }

    /// GetOriginalBloodType().Color
    pub fn original_blood_color(&self, l: usize) -> [f32; 4] {
        liquid_color(self.blood_type(l))
    }

    /// CirculationBehaviour.IsWorldPointInArteryRect (mindistance = 4f / 35f)
    pub fn is_world_point_in_artery_rect(&self, l: usize, world_point: [f32; 2]) -> bool {
        let Some(rects) = &self.circulations[l].LocalArteryRects else { return false };
        let mindistance = 4.0f32 / 35.0;
        let num = mindistance * mindistance;
        let vector = self.inverse_transform_point(self.limbs[l].PhysicalBehaviour, [world_point[0], world_point[1], 0.0]);
        for r in rects {
            // Bounds.ClosestPoint: the point clamped to center -+ extents.
            let mut closest = [0.0f32; 3];
            for i in 0..3 {
                let (min, max) = (r[i] - r[i + 3], r[i] + r[i + 3]);
                closest[i] = if vector[i] < min { min } else if vector[i] > max { max } else { vector[i] };
            }
            let dv = [closest[0] - vector[0], closest[1] - vector[1], closest[2] - vector[2]];
            if (d(dv[0]) * dv[0] + d(dv[1]) * dv[1] + d(dv[2]) * dv[2]).f() <= num {
                return true;
            }
        }
        false
    }

    /// CirculationBehaviour.CreateBleedingParticle
    /// Every caller passes makeSound = false, so DripSounds is always nulled.
    pub fn create_bleeding_particle(&mut self, l: usize, world_position: [f32; 2], direction: [f32; 2], laminarity: f32) {
        if !self.limbs[l].IsAndroid && !self.limbs[l].IsZombie && self.circulations[l].BleedingPointCount < 8 {
            // BleedingParticle.prefab (Resources/prefabs). The ClosestPoint onto the sprite bounds and
            // the rotation only feed the splat's decal positions and directions (presentation), so the
            // position is the unclamped worldPosition - direction * 0.1f and transform.up = direction
            // is a z rotation.
            let position = [(d(world_position[0]) - d(direction[0]) * 0.1f32).f(), (d(world_position[1]) - d(direction[1]) * 0.1f32).f(), 0.0];
            let angle = (direction[1] as f64).atan2(direction[0] as f64) - std::f64::consts::FRAC_PI_2;
            let rotation = [0.0, 0.0, (angle * 0.5).sin() as f32, (angle * 0.5).cos() as f32];
            let b = self.bleeding.len();
            // Awake of the clone's hierarchy (Trail, extraDecals, water puff): the two
            // ActivateIfFancyEffects each draw `Chance(1) > Random.value` before reading FancyEffects.
            let extra_decals = 1.0 > self.random.value() && self.prefs.FancyEffects;
            let _water_puff = 1.0 > self.random.value() && self.prefs.FancyEffects;
            // The extraDecals child is the FancyBloodSplatController (Start is delayed: started = false).
            let splat = if extra_decals {
                self.splats.push(FancyBloodSplat {
                    UnparentAtStart: false,
                    DirectionMode: 0,
                    Continuous: true,
                    Direction: [0.0, 1.0],
                    SpawningChance: 0.0,
                    AmountRange: [1.0, 4.0],
                    AngleRandomisation: 9.0,
                    LifetimeInSeconds: [1.0, 2.0],
                    EjectionForce: [0.0, 1.0],
                    DecalColourMultiplier: [1.0, 0.0, 0.0, 1.0],
                    Size: 0.1,
                    DestroyWhenNoParticles: true,
                    TriggerOnCollide: false,
                    particles: Vec::new(),
                    parent_position: position,
                    rotation,
                    started: false,
                    destroyed: false,
                });
                Some(self.splats.len() - 1)
            } else {
                None
            };
            // BleedingParticleBehaviour: Awake (clock, collision module) and Start (WaterPuffSystem) draw nothing.
            self.bleeding.push(BleedingParticle {
                IsOnlyForInternalBleeding: false,
                SpeedMultiplier: 0.6,
                RateMultiplier: 1.25,
                Laminarity: laminarity,
                DripSoundRateThreshold: 1.0,
                ShouldBecomeSmokeInWater: self.circulations[l].BleedingPointCount == 0,
                PulseSpeed: 2.0,
                PulseIntensity: 1.3,
                fakeHeartClock: 0.0,
                CirculationBehaviour: l,
                PushingTo: None,
                BloodSplatController: splat,
                dripSoundClock: FixedIntervalDistributor::with_rate(0.5),
                DripSounds: None,
                visible: false,
                startSpeedMultiplier: 0.0,
                rateOverTimeMultiplier: 0.0,
                emission_enabled: false,
                destroyed: false,
            });
            self.circulations[l].BleedingParticles.push(b);
            self.circulations[l].BleedingPointCount += 1;
        }
    }

    /// CirculationBehaviour.Cut
    pub fn cut(&mut self, l: usize, point: [f32; 2], direction: [f32; 2]) {
        if !self.circulations[l].ImmuneToDamage {
            let flag = self.is_world_point_in_artery_rect(l, point);
            self.circulations[l].BleedingRate += if flag { 1.0 } else { 0.25 };
            self.create_bleeding_particle(l, point, direction, if flag { 1.0 } else { 0.0 });
        }
    }

    /// CirculationBehaviour.HealBleeding
    pub fn heal_bleeding(&mut self, l: usize) {
        for i in std::mem::take(&mut self.circulations[l].BleedingParticles) {
            self.bleeding[i].destroyed = true;
            // The extraDecals splat is a child of the particle.
            if let Some(s) = self.bleeding[i].BloodSplatController {
                self.splats[s].destroyed = true;
            }
        }
        let c = &mut self.circulations[l];
        c.BleedingRate = 0.0;
        c.BleedingPointCount = 0;
        c.InternalBleedingIntensity = 0.0;
    }

    /// CirculationBehaviour.ActOnJointBreak2D (for this limb's own joint)
    pub fn circulation_on_joint_break(&mut self, l: usize) {
        self.circulations[l].BleedingRate += 20.0;
        if let Some(s) = self.circulations[l].Source {
            if self.node_is_connected_to(l, s) {
                self.circulations[s].BleedingRate += 20.0;
            }
        }
        self.circulations[l].IsDisconnected = true;
    }
}
