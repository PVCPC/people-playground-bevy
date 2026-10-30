//! FlaskBehaviour and GasTankBehaviour (a BloodContainer outside a limb), and the LiquidContainerController of a flask.
//! Only liquids whose container hooks are empty outside a CirculationBehaviour (OIL, BLOOD, GORSE BLOOD) can enter
//! one; any other liquid stops at its hook. The flasks' PointLiquidTransferBehaviour acts only while the object is being used
//! continuously (FixedUpdate).
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
        /// Limits.y: GasTankBehaviour's 20 or FlaskBehaviour's Capacity; AllowsOverflow is false.
        limit: f32 = 20.0,
        /// StartLiquid
        StartLiquid: String = String::new(),
        StartAmount: f32 = 0.0,
        LiquidDistribution: CsDict<usize, RefFloat> = CsDict::default(),
        /// LiquidContainer, the LiquidContainerController whose fill Update sets.
        LiquidContainer: Option<usize> = None,
        /// GetComponent<PhysicalBehaviour>()
        Phys: Option<usize> = None,
    }
}

script! {
    PointLiquidTransfer {
        Point: [f32; 2] = [0.0, 0.0],
        /// UnityEngine.Space: World 0, Self 1.
        Space: i32 = 0,
        UnitsPerSecond: f32 = 0.56,
    }
    links {
        /// GetComponent<PhysicalBehaviour>()
        Phys: usize = 0,
        /// GetComponent<BloodContainer>(): the `Game::flasks` index (the prefabs put it on a flask).
        Flask: usize = 0,
        /// Layers (LayerMask bits)
        Layers: u32 = 0,
    }
}

/// BloodContainer's subclasses.
const BLOOD_CONTAINERS: &[&str] = &[
    "BloodTankBehaviour", "CentrifugeBehaviour", "CirculationBehaviour", "FlaskBehaviour", "GasTankBehaviour", "InspectorLiquidContainer",
    "LiquidCanisterBehaviour", "LiquidDuplicatorBehaviour", "LiquidFilterBehaviour", "LiquidOutletBehaviour", "LiquidPumpBehaviour",
    "LiquidentifierBehaviour", "ReactorConduitBehaviour", "SyringeBehaviour", "ValveBehaviour",
];

/// The BloodContainer a collider's GameObject carries.
enum Container {
    Limb(usize),
    Flask(usize),
}

/// LiquidContainerController: its particles, colour and sound are rendering and audio; the Random draws and the
/// FillPercentage that gates SimulateLiquid stay.
#[derive(Clone, Debug, Default)]
pub struct LiquidContainer {
    pub id: i32,
    pub FillPercentage: f32,
    /// Container.min and max
    pub min: [f32; 2],
    pub max: [f32; 2],
    /// UserPreferenceManager.Current.FancyEffects at Awake
    pub fancy: bool,
}

impl Game {
    /// Liquid.GetLiquid(id) for a liquid whose BloodContainer hooks outside a limb are empty.
    fn flask_liquid(id: &str) -> usize {
        let l = LIQUIDS.iter().position(|(n, _)| *n == id).unwrap_or_else(|| unimplemented!("Liquid.GetLiquid({id}) is null"));
        Self::flask_hooks_empty(l);
        l
    }

    fn flask_hooks_empty(l: usize) {
        if !matches!(LIQUIDS[l].0, "OIL" | "BLOOD" | "GORSE BLOOD") {
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

    /// FlaskBehaviour.Update: BloodContainer.Update, then the LiquidContainer's fill (its colour is rendering).
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
        if let Some(c) = self.flasks[f].LiquidContainer {
            // FillPercentage = Mathf.Clamp01(ScaledLiquidAmount); the Color is rendering.
            let s = &self.flasks[f];
            let scaled = mathf::map_range(0.0, s.limit, 0.0, 1.0, s.TotalLiquidAmount);
            self.liquid_containers[c].FillPercentage = mathf::clamp(scaled, 0.0, 1.0);
        }
    }

    /// BloodContainer.RemoveLiquid in a flask: UpperLimit is Limits.y (it does not allow overflow).
    fn flask_remove_liquid(&mut self, f: usize, liquid: usize, amount: f32) -> f32 {
        if amount <= mathf::EPSILON {
            return 0.0;
        }
        let s = &mut self.flasks[f];
        let mut num = mathf::clamp(s.TotalLiquidAmount - amount, 0.0, s.limit) - s.TotalLiquidAmount;
        if num.is_nan() || num >= -1e-45 {
            return 0.0;
        }
        if let Some(r) = s.LiquidDistribution.get_mut(liquid) {
            if r.raw + num < 0.0 {
                num = 0.0 - r.raw;
            }
            s.TotalLiquidAmount += num;
            let v = r.raw + num;
            r.set_raw(v);
        }
        s.isComputedColourCached = false;
        s.shouldDeleteEmptyEntries = true;
        num
    }

    /// BloodContainer.TransferTo(float, BloodContainer, proportional = true) from a flask. Neither behaviour is ever
    /// disabled (the flask scripts have no code that does it); a limb's `enabled` is read.
    fn flask_transfer_to(&mut self, f: usize, amount: f32, to: &Container) -> f32 {
        if let Container::Limb(l) = *to {
            if !self.circulations[l].enabled {
                return 0.0;
            }
        }
        if amount <= mathf::EPSILON {
            return 0.0;
        }
        let total = self.flasks[f].TotalLiquidAmount;
        let mut num = 0.0;
        // The enumerator throws if the dictionary changes; a removal only changes values.
        let keys: Vec<usize> = self.flasks[f].LiquidDistribution.iter().map(|(k, _)| *k).collect();
        for key in keys {
            let raw = self.flasks[f].LiquidDistribution.get(key).unwrap().raw;
            // Evaluated in double, rounded at the store.
            let amount2 = (super::d(amount) * (super::d(raw) / total)).f();
            // TransferTo(Liquid, float, BloodContainer)
            if amount2 <= mathf::EPSILON {
                continue;
            }
            let removed = self.flask_remove_liquid(f, key, amount2).abs();
            if removed > mathf::EPSILON {
                match *to {
                    Container::Limb(l) => {
                        self.add_liquid(l, key, removed);
                    }
                    Container::Flask(t) => {
                        self.flask_add_liquid(t, key, removed);
                    }
                }
            }
            num += removed;
        }
        num
    }

    /// PointLiquidTransferBehaviour.FixedUpdate
    pub fn point_liquid_fixed_update(&mut self, i: usize) {
        let phys = self.point_liquids[i].Phys;
        if !self.used_continuously(phys) {
            return;
        }
        // Transfer(Time.fixedDeltaTime)
        let s = &self.point_liquids[i];
        let (flask, units, point, space, mask) = (s.Flask, s.UnitsPerSecond, s.Point, s.Space, s.Layers);
        let amount = units * self.time.fixedDeltaTime;
        // GetPoint: Space.World is the point, anything else base.transform.TransformPoint(Point).
        let at = if space == 0 {
            point
        } else {
            let w = self.transform_point3(phys, [point[0], point[1], 0.0]);
            [w[0], w[1]]
        };
        // Physics2D.OverlapPoint(point, Layers); the lowest-z collider, whose GameObject must not be this one and must
        // carry a BloodContainer (a limb's CirculationBehaviour or a flask).
        let Some((hit, collider)) = self.overlap_point_first(at, mask) else { return };
        let Some(q) = hit else {
            if self.bare_colliders[&collider].scripts.iter().any(|s| BLOOD_CONTAINERS.contains(&s.as_str())) {
                unimplemented!("PointLiquidTransferBehaviour: a BloodContainer on a collider without a PhysicalBehaviour");
            }
            return;
        };
        if q == phys {
            return;
        }
        let target = if let Some(l) = self.limbs.iter().position(|x| x.PhysicalBehaviour == q) {
            Container::Limb(l)
        } else if let Some(t) = self.flasks.iter().position(|x| x.Phys == Some(q)) {
            Container::Flask(t)
        } else {
            let other = self.with_component_instance(self.physicals[q].id, |_, inst, o| {
                inst.prefab.objects[o].components.iter().find_map(|(_, c)| match c {
                    crate::assets::db::Component::Script { class, .. } if BLOOD_CONTAINERS.contains(&class.as_str()) => Some(class.clone()),
                    _ => None,
                })
            });
            if let Some(class) = other {
                unimplemented!("PointLiquidTransferBehaviour into a {class}");
            }
            return;
        };
        // !component.IsFull() || component.AllowsOverflow: a limb allows overflow, a flask does not.
        if let Container::Flask(t) = target {
            let s = &self.flasks[t];
            if mathf::map_range(0.0, s.limit, 0.0, 1.0, s.TotalLiquidAmount) >= 0.999 {
                return;
            }
        }
        self.flask_transfer_to(flask, amount, &target);
    }

    /// LiquidContainerController.Awake: with FancyEffects the 16 particles take a random position each, x then y.
    /// (Start's SimulateLiquid draws only for two coincident particles.)
    pub fn liquid_container_awake(&mut self, c: usize) {
        let fancy = self.prefs.FancyEffects;
        self.liquid_containers[c].fancy = fancy;
        if !fancy {
            return;
        }
        let (min, max) = (self.liquid_containers[c].min, self.liquid_containers[c].max);
        let mut positions = Vec::new();
        for _ in 0..16 {
            let x = self.random.range(min[0], max[0]);
            let y = self.random.range(min[1], max[1]);
            if positions.contains(&[x, y]) {
                unimplemented!("LiquidContainerController.SimulateLiquid: Random.insideUnitCircle for coincident particles");
            }
            positions.push([x, y]);
        }
    }

    /// LiquidContainerController.FixedUpdate: the simulation only moves the particles (rendering) and the sloshing
    /// volume, but a particle clamped onto another's position would draw Random.insideUnitCircle.
    pub fn liquid_container_fixed_update(&mut self, c: usize) {
        let s = &self.liquid_containers[c];
        if s.fancy && !mathf::approximately(s.FillPercentage, 0.0) && !mathf::approximately(s.FillPercentage, 1.0) {
            unimplemented!("LiquidContainerController.SimulateLiquid with a part-filled container");
        }
    }
}
