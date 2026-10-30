//! FlaskBehaviour and GasTankBehaviour (a BloodContainer outside a limb), and the LiquidContainerController of a flask.
//! Only liquids whose container hooks are empty outside a CirculationBehaviour (OIL, BLOOD, GORSE BLOOD) can enter
//! one; any other liquid stops at its hook. The flasks' PointLiquidTransferBehaviour acts only while the object is being used
//! continuously (FixedUpdate).
#![allow(non_snake_case)]

use super::circulation::{mix_instructions, RefFloat, LIQUIDS};
use super::{d, mathf, script, CsDict, Game};

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

/// One of the 16 Particles: Position and Velocity.
#[derive(Clone, Copy, Debug, Default)]
pub struct Particle {
    pub Position: [f32; 2],
    pub Velocity: [f32; 2],
}

/// LiquidContainerController: its colour, sound and shader inputs (particlePositions, the Update lerp) are rendering
/// and audio; the particles, preVelocity and the Random draws stay.
#[derive(Clone, Debug, Default)]
pub struct LiquidContainer {
    pub id: i32,
    pub FillPercentage: f32,
    /// Container.min and max
    pub min: [f32; 2],
    pub max: [f32; 2],
    /// Container.center and extents
    pub center: [f32; 3],
    pub extents: [f32; 3],
    /// UserPreferenceManager.Current.FancyEffects at Awake
    pub fancy: bool,
    /// The Rigidbody2D component's instance id (null: None).
    pub Rigidbody: Option<i32>,
    /// base.transform: where it hangs and its Transforms below that (object first).
    pub anchor: super::BareAnchor,
    pub chain: Vec<crate::unity::transform::Trs>,
    pub particles: Vec<Particle>,
    pub preVelocity: [f32; 2],
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
    pub fn liquid_container_awake(&mut self, c: usize) {
        let fancy = self.prefs.FancyEffects;
        self.liquid_containers[c].fancy = fancy;
        if !fancy {
            return;
        }
        let (min, max) = (self.liquid_containers[c].min, self.liquid_containers[c].max);
        let mut particles = Vec::new();
        for _ in 0..16 {
            let x = self.random.range(min[0], max[0]);
            let y = self.random.range(min[1], max[1]);
            particles.push(Particle { Position: [x, y], Velocity: [0.0, 0.0] });
        }
        self.liquid_containers[c].particles = particles;
    }

    /// LiquidContainerController.Start (the AudioSource and particlePositions are not state).
    pub fn liquid_container_start(&mut self, c: usize) {
        if self.liquid_containers[c].fancy {
            self.liquid_simulate(c);
        }
    }

    /// LiquidContainerController.FixedUpdate: `!NoNeedToSimulate && (!renderComponent || renderComponent.isVisible)`.
    /// renderComponent is assigned by OnWillRenderObject, which only a rendering camera calls, so here it is null.
    pub fn liquid_container_fixed_update(&mut self, c: usize) {
        let s = &self.liquid_containers[c];
        if s.fancy && !mathf::approximately(s.FillPercentage, 0.0) && !mathf::approximately(s.FillPercentage, 1.0) {
            self.liquid_simulate(c);
        }
    }

    /// LiquidContainerController.SimulateLiquid. Every Vector2 operator is one float operation per component;
    /// the fused expressions (sqrMagnitude, OffsetRepulsionDistance, edgeRepulse) are double-evaluated.
    fn liquid_simulate(&mut self, c: usize) {
        let Some(rb) = self.liquid_containers[c].Rigidbody else { unimplemented!("LiquidContainerController.SimulateLiquid with a null Rigidbody") };
        let (anchor, chain) = (self.liquid_containers[c].anchor, self.liquid_containers[c].chain.clone());
        let g = self.world.gravity;
        // base.transform.InverseTransformVector(Physics2D.gravity)
        let vector = self.anchored_inverse_vector(anchor, &chain, [g.x, g.y, 0.0]);
        let owner = self.rigidbody_owner(rb).expect("LiquidContainerController.Rigidbody");
        let r = self.owner_rigidbody(owner);
        let velocity = r.velocity(&self.world);
        let gravity_scale = if r.detached.is_some() { r.detached_props[2] } else { self.world.body(r.body).gravity_scale };
        let s = &self.liquid_containers[c];
        let acceleration = [s.preVelocity[0] - velocity[0], s.preVelocity[1] - velocity[1]];
        let vector2 = self.anchored_inverse_vector(anchor, &chain, [acceleration[0], acceleration[1], 0.0]);
        let s = &mut self.liquid_containers[c];
        s.preVelocity = velocity;
        let (fill, min, max, center, extents) = (s.FillPercentage, s.min, s.max, s.center, s.extents);
        let mut particles = std::mem::take(&mut s.particles);
        // OffsetRepulsionDistance => 0.01f + FillPercentage * 0.2f
        let offset = (d(0.01f32) + d(fill) * 0.2f32).f();
        for i in 0..16 {
            for j in 0..16 {
                if j == i {
                    continue;
                }
                let vector3 = [particles[j].Position[0] - particles[i].Position[0], particles[j].Position[1] - particles[i].Position[1]];
                let sqr_magnitude = mathf::sqr_magnitude(vector3);
                if sqr_magnitude == 0.0 {
                    let u = self.random.inside_unit_circle();
                    particles[i].Position[0] += u[0] * 0.01;
                    particles[i].Position[1] += u[1] * 0.01;
                    continue;
                }
                let num = mathf::sqrt(sqr_magnitude);
                if (num as f64) < offset as f64 * 1.5 {
                    let k = sqr_magnitude + 1.0;
                    for a in 0..2 {
                        particles[i].Velocity[a] += -(vector3[a] / num / k) * 0.07;
                    }
                }
            }
        }
        for p in particles.iter_mut() {
            // Container.Contains(Vector3): center -+ extents compared per axis (a NaN is inside).
            let at = [p.Position[0], p.Position[1], 0.0];
            let inside = (0..3).all(|a| !(center[a] - extents[a] > at[a] || at[a] > extents[a] + center[a]));
            if !inside {
                if p.Position[0] < min[0] || p.Position[0] > max[0] {
                    p.Velocity[0] = 0.0;
                }
                if p.Position[1] < min[1] || p.Position[1] > max[1] {
                    p.Velocity[1] = 0.0;
                }
                let closest = crate::unity::transform::bounds_closest_point(at, center, extents).0;
                p.Position = [closest[0], closest[1]];
            }
            for a in 0..2 {
                p.Position[a] += p.Velocity[a] * 0.1;
                p.Velocity[a] *= 0.95;
                p.Velocity[a] += vector2[a] * 0.25;
            }
            // 0.002f * Rigidbody.gravityScale * vector
            let scale = 0.002f32 * gravity_scale;
            p.Velocity[0] += scale * vector[0];
            p.Velocity[1] += scale * vector[1];
            let vector4 = [p.Position[0] - min[0], p.Position[1] - min[1]];
            let vector5 = [max[0] - p.Position[0], max[1] - p.Position[1]];
            // edgeRepulse: 0.5f / (num3 * num3) * 0.07f in one double expression.
            let edge_repulse = |delta: f32| {
                let num3 = 1.0 + delta.abs();
                (d(0.5f32) / (d(num3) * num3) * 0.07f32).f()
            };
            if vector4[0] < offset {
                p.Velocity[0] += edge_repulse(vector4[0]);
            }
            if vector4[1] < offset {
                p.Velocity[1] += edge_repulse(vector4[1]);
            }
            if vector5[0] < offset {
                p.Velocity[0] -= edge_repulse(vector5[0]);
            }
            if vector5[1] < offset {
                p.Velocity[1] -= edge_repulse(vector5[1]);
            }
        }
        self.liquid_containers[c].particles = particles;
    }
}
