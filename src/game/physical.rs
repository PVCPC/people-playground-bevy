//! PhysicalBehaviour and the Utils energy/temperature helpers.
use super::{mathf, script, Game, Pose, Rigidbody};

/// PhysicalProperties (ScriptableObject): the material values the scripts read.
#[derive(Clone, Debug, Default)]
pub struct PhysicalProperties {
    pub name: String,
    /// PhysicMaterial friction, None without a material.
    pub friction: Option<f32>,
    pub Softness: f32,
    pub Flammability: f32,
    pub Conducting: bool,
    pub Burnrate: f32,
    pub BurningTemperatureThreshold: f32,
    pub HeatTransferSpeedMultiplier: f32,
    pub ImpactIntensityMutliplier: f32,
    pub HitVolumeMultiplier: f32,
    pub SparksOnSlide: bool,
    pub SoftImpact: usize,
    pub HardImpact: usize,
}

/// PhysicalBehaviour.ColliderBoolPair; the collider is identified by its PhysicalBehaviour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColliderBoolPair {
    pub Active: bool,
    pub Coll: usize,
}

script! {
    Physical {
        SimulateTemperature: bool = true,
        AmbientTemperatureTransfer: bool = true,
        OnFire: bool = false,
        BurnProgress: f32 = 0.0,
        isSliding: bool = false,
        EnergyWireResistance: f32 = 1.0,
        BuoyancyModifier: f32 = 1.0,
        IsWeightless: bool = false,
        charge: f32 = 0.0,
        ConductOverride: bool = true,
        ForceNoCharge: bool = false,
        StabCausesWound: bool = true,
        IsUnderWater: bool = false,
        underwaterMarkings: i32 = 0,
        IsInLava: bool = false,
        CurrentWaterSurfaceLevel: f32 = 0.0,
        seed: f32 = 0.0,
        intSeed: i32 = 0,
        burnIntensity: f32 = 0.0,
        slideIntensity: f32 = 0.0,
        previousVelocity: [f32; 2] = [0.0, 0.0],
        Temperature: f32 = 0.0,
        beingHeldByGripper: bool = false,
        Wetness: f32 = 0.0,
        GridRes: i32 = 4,
        ObjectArea: f32 = 0.0,
        TemperatureWhenBurningLerpMultiplier: f32 = 1.0,
        BurningProgressionMultiplier: f32 = 1.0,
        Circumference: f32 = -1.0,
        outOfBoundsDuration: f32 = 0.0,
        affectSurroundingTimer: f32 = 0.0,
        sizzleAudioHeat: f32 = 0.0,
        TrueInitialMass: f32 = 0.0,
        InitialMass: f32 = 0.0,
        InitialGravityScale: f32 = 0.0,
        isDisintegrated: bool = false,
    }
    links {
        name: String = String::new(),
        id: i32 = 0,
        /// gameObject.isStatic
        isStatic: bool = false,
        Properties: usize = 0,
        rigidbody: Rigidbody = Rigidbody { body: 0, bodyType: 0, collisionDetectionMode: 0, interpolation: 0 },
        transform: Pose = Pose::default(),
        onCollisionStayBuffer: [Option<ColliderBoolPair>; 8] = [None; 8],
        LocalColliderGridPoints: Option<Vec<[f32; 2]>> = None,
        stabWoundCount: u32 = 0,
        /// Penetration and stab lists stay empty until stabbing is rewritten.
        penetrations: Vec<usize> = Vec::new(),
        victimPenetrations: Vec<usize> = Vec::new(),
        /// The collider's GameObject is active (Collider.gameObject.activeSelf).
        active: bool = true,
        enabled: bool = true,
        /// Set when the oracle loads a component it has no data for.
        has_fire_map: bool = false,
        decal_controllers: usize = 0,
    }
}

impl Physical {
    /// Charge getter
    pub fn Charge(&self) -> f32 {
        if !self.ForceNoCharge { self.charge } else { 0.0 }
    }
    pub fn GetChargeWithWireResistance(&self) -> f32 {
        self.Charge() * self.EnergyWireResistance
    }
}

impl Game {
    pub fn heat_capacity(&self, p: usize) -> f32 {
        let ph = &self.physicals[p];
        if !ph.SimulateTemperature {
            return 1.0;
        }
        ph.ObjectArea * mathf::max(0.1, 0.025 * ph.rigidbody.mass(&self.world))
    }

    fn buffer_entry_live(&self, pair: &Option<ColliderBoolPair>) -> Option<ColliderBoolPair> {
        pair.filter(|c| c.Active && self.physicals[c.Coll].active)
    }

    /// PhysicalBehaviour.IsTouchingSomething
    pub fn is_touching_something(&self, p: usize) -> bool {
        self.physicals[p].onCollisionStayBuffer.iter().any(|c| self.buffer_entry_live(c).is_some())
    }

    /// Utils.TransferEnergyFixedRate
    pub fn transfer_energy_fixed_rate(&mut self, a: usize, b: usize, rate: f32) {
        let ca = self.physicals[a].GetChargeWithWireResistance();
        let cb = self.physicals[b].GetChargeWithWireResistance();
        if ca > cb {
            let v = mathf::lerp(self.physicals[b].Charge(), ca, rate);
            self.physicals[b].charge = v;
        } else {
            let v = mathf::lerp(self.physicals[a].Charge(), cb, rate);
            self.physicals[a].charge = v;
        }
    }

    /// Utils.AverageTemperature
    pub fn average_temperature(&mut self, a: usize, b: usize, transfer_factor: f32) {
        if !(self.physicals[a].SimulateTemperature && self.physicals[b].SimulateTemperature) {
            return;
        }
        let pa = &self.properties[self.physicals[a].Properties];
        let pb = &self.properties[self.physicals[b].Properties];
        let num = mathf::min(pa.HeatTransferSpeedMultiplier, pb.HeatTransferSpeedMultiplier) * transfer_factor;
        let (ta, tb) = (self.physicals[a].Temperature, self.physicals[b].Temperature);
        let ha = self.heat_capacity(a);
        self.physicals[a].Temperature = mathf::lerp(ta, tb, num / ha);
        let hb = self.heat_capacity(b);
        self.physicals[b].Temperature = mathf::lerp(tb, ta, num / hb);
    }

    /// PhysicalBehaviour.IgniteInternal
    fn ignite_internal(&mut self, p: usize, ignore_flammability: bool) {
        let ph = &self.physicals[p];
        let flammability = self.properties[ph.Properties].Flammability;
        if !ph.isStatic
            && (ignore_flammability || !(mathf::perlin(self.time.time * 100.0, ph.seed + 235.42) > flammability))
            && !(flammability <= mathf::EPSILON)
        {
            self.physicals[p].OnFire = true;
        }
    }

    /// PhysicalBehaviour.Ignite(bool, Vector2)
    pub fn ignite_at(&mut self, p: usize, ignore_flammability: bool, _position: [f32; 2]) {
        self.ignite_internal(p, ignore_flammability);
        // ponytail: LocalFirePropagationMap.IgniteAt; humans and the map have none, add with burnable props.
        assert!(!self.physicals[p].has_fire_map, "LocalFirePropagationMap is not rewritten yet");
    }

    pub fn extinguish(&mut self, p: usize) {
        let threshold = self.properties[self.physicals[p].Properties].BurningTemperatureThreshold;
        let ph = &mut self.physicals[p];
        ph.Temperature = mathf::min(ph.Temperature, threshold);
        ph.OnFire = false;
    }

    /// PhysicalBehaviour.GetRandomGridPoint
    fn random_grid_point(&mut self, p: usize) -> [f32; 2] {
        match &self.physicals[p].LocalColliderGridPoints {
            None => [0.0, 0.0],
            Some(points) => {
                let n = points.len() as i32;
                // Utils.PickRandom
                if n == 0 {
                    return [0.0, 0.0];
                }
                let i = self.random.range_int(0, n) as usize;
                self.physicals[p].LocalColliderGridPoints.as_ref().unwrap()[i]
            }
        }
    }

    /// PhysicalBehaviour.ManagedFixedUpdate
    pub fn physical_fixed_update(&mut self, p: usize) {
        if self.physicals[p].isStatic {
            return;
        }
        self.physicals[p].charge *= 0.95;
        if self.prefs.CollisionQuality == super::COLLISION_DYNAMIC {
            let rb = &self.physicals[p].rigidbody;
            let fast = mathf::sqr_magnitude(rb.velocity(&self.world)) > self.DynamicSqrdVelocityThreshold;
            let mode = rb.collisionDetectionMode;
            if fast {
                if mode != 1 {
                    let mut rb = rb.clone();
                    rb.set_collision_detection_mode(&mut self.world, 1);
                    self.physicals[p].rigidbody = rb;
                }
            } else if mode != 0 {
                let mut rb = rb.clone();
                rb.set_collision_detection_mode(&mut self.world, 0);
                self.physicals[p].rigidbody = rb;
            }
        }
        let props = self.properties[self.physicals[p].Properties].clone();
        if self.physicals[p].SimulateTemperature {
            if self.physicals[p].AmbientTemperatureTransfer {
                if self.prefs.AmbientTemperatureTransfer {
                    // ponytail: AmbientTemperatureGridBehaviour.TransferHeat; the preference is off by default.
                    unimplemented!("AmbientTemperatureGridBehaviour");
                } else {
                    let h = self.heat_capacity(p);
                    let ambient = self.AmbientTemperature;
                    let ph = &mut self.physicals[p];
                    ph.Temperature = mathf::lerp(ph.Temperature, ambient, 0.0001 / h);
                }
            }
            let ph = &mut self.physicals[p];
            if ph.OnFire && ph.Temperature < props.BurningTemperatureThreshold * 3.0 {
                ph.Temperature = mathf::lerp(ph.Temperature, props.BurningTemperatureThreshold * 3.0, 0.01 * ph.TemperatureWhenBurningLerpMultiplier);
            }
            let flag = ph.BurnProgress <= 0.98;
            if flag && ph.Temperature > props.BurningTemperatureThreshold && props.Flammability > mathf::EPSILON {
                if ph.BurnProgress < 0.2 {
                    ph.BurnProgress += 0.001;
                }
                if self.random.value() as f64 > 0.9995 && props.Flammability > mathf::EPSILON {
                    let local = self.random_grid_point(p);
                    let point = self.transform_point(p, local);
                    self.ignite_at(p, true, point);
                }
            }
            let ph = &mut self.physicals[p];
            if ph.OnFire {
                ph.BurnProgress += mathf::max(0.0, ph.Temperature - props.BurningTemperatureThreshold)
                    * 6e-5
                    * (props.Burnrate / 2.0)
                    * ph.BurningProgressionMultiplier;
            }
            if ph.Temperature < props.BurningTemperatureThreshold {
                if flag && ph.OnFire {
                    ph.BurnProgress += 2.5e-5 * (props.Burnrate / 2.0) * ph.BurningProgressionMultiplier;
                }
                if ph.OnFire && self.random.value() > 0.995 {
                    self.extinguish(p);
                }
            }
            let ph = &mut self.physicals[p];
            ph.Temperature += ph.Charge() * 0.0008 / props.HeatTransferSpeedMultiplier;
            // The original passes its arguments as Clamp(-273.15, Temperature, 5000000).
            ph.Temperature = mathf::clamp(-273.15, ph.Temperature, 5_000_000.0);
        }
        self.on_collision_stay_without_sleep(p);
        if self.physicals[p].IsUnderWater && self.random.value() > 0.95 && self.physicals[p].decal_controllers > 0 {
            // ponytail: underwater decal fading is visual only; decals are not rewritten yet.
        }
        // ponytail: penetrations stay empty until stabbing is rewritten.
        assert!(self.physicals[p].penetrations.is_empty());
        if self.physicals[p].OnFire {
            if self.random.value() > 0.9993 {
                self.extinguish(p);
            }
            // FireLoopSoundControllerBehaviour.FuzzySetVolumeAt: sound only.
        }
    }

    /// PhysicalBehaviour.OnCollisionStayWithoutSleep
    fn on_collision_stay_without_sleep(&mut self, p: usize) {
        if self.physicals[p].isStatic {
            return;
        }
        for i in 0..8 {
            let Some(pair) = self.physicals[p].onCollisionStayBuffer[i] else { continue };
            if !pair.Active {
                continue;
            }
            if !self.physicals[pair.Coll].active {
                self.physicals[p].onCollisionStayBuffer[i].as_mut().unwrap().Active = false;
                continue;
            }
            let v = pair.Coll;
            if self.physicals[p].SimulateTemperature && self.physicals[v].SimulateTemperature {
                self.average_temperature(p, v, 0.01);
            }
            let (me, other) = (&self.physicals[p], &self.physicals[v]);
            if !me.ForceNoCharge
                && me.Charge() > other.Charge()
                && me.ConductOverride
                && other.ConductOverride
                && self.properties[other.Properties].Conducting
                && self.properties[me.Properties].Conducting
            {
                let c = mathf::lerp(other.Charge(), me.Charge(), 0.8);
                self.physicals[v].charge = c;
            }
        }
    }

    /// Transform.TransformPoint for a 2D point of a physical object's transform.
    pub fn transform_point(&self, p: usize, local: [f32; 2]) -> [f32; 2] {
        let t = &self.physicals[p].transform;
        let s = [local[0] * t.localScale[0], local[1] * t.localScale[1], 0.0];
        let r = mathf::rotate(t.rotation, s);
        [r[0] + t.position[0], r[1] + t.position[1]]
    }
}
