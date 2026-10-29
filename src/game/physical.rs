//! PhysicalBehaviour and the Utils energy/temperature helpers.
use super::{d, mathf, script, Game, Pose, Rigidbody};

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
        ChargeBurns: bool = true,
        ForceNoChargeParticles: bool = false,
        Deletable: bool = true,
        Disintegratable: bool = true,
        dirtyFire: bool = false,
        dirtyCharged: bool = false,
        dirtySmoke: bool = false,
        dirtyBubble: bool = false,
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
                let m = mathf::max(0.0, ph.Temperature - props.BurningTemperatureThreshold);
                ph.BurnProgress = (d(ph.BurnProgress) + d(m) * 6e-5f32 * (d(props.Burnrate) / 2.0f32) * ph.BurningProgressionMultiplier).f();
            }
            if ph.Temperature < props.BurningTemperatureThreshold {
                if flag && ph.OnFire {
                    ph.BurnProgress = (d(ph.BurnProgress) + d(2.5e-5) * (d(props.Burnrate) / 2.0f32) * ph.BurningProgressionMultiplier).f();
                }
                if ph.OnFire && self.random.value() > 0.995 {
                    self.extinguish(p);
                }
            }
            let ph = &mut self.physicals[p];
            ph.Temperature = (d(ph.Temperature) + d(ph.Charge()) * 0.0008f32 / props.HeatTransferSpeedMultiplier).f();
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

    /// PhysicalBehaviour.Ignite(bool)
    pub fn ignite(&mut self, p: usize, ignore_flammability: bool) {
        self.ignite_internal(p, ignore_flammability);
        assert!(!self.physicals[p].has_fire_map, "LocalFirePropagationMap is not rewritten yet");
    }

    /// PhysicalBehaviour.ManagedUpdate
    pub fn physical_update(&mut self, p: usize) {
        let dt = self.time.deltaTime;
        // audioSource / HandleSlidingSounds: sound only.
        let ph = &mut self.physicals[p];
        if ph.sizzleAudioHeat > 0.0 {
            ph.sizzleAudioHeat -= dt;
        }
        if ph.isStatic {
            return;
        }
        let burnrate = self.properties[ph.Properties].Burnrate;
        if ph.BurnProgress < 0.3 && ph.ChargeBurns {
            ph.BurnProgress = (d(ph.BurnProgress) + d(burnrate) * ph.Charge() * 0.01f32 * self.time.timeScale).f();
        }
        ph.Wetness = mathf::clamp01(ph.Wetness - dt);
        if ph.OnFire {
            ph.burnIntensity = (d(ph.burnIntensity) + d(dt) * 0.1f32).f();
            if ph.BurnProgress >= 0.995 {
                ph.OnFire = false;
            }
            ph.BurnProgress = mathf::clamp01(ph.BurnProgress);
            if ph.Wetness as f64 > 0.1 {
                self.extinguish(p);
                self.physicals[p].burnIntensity = 0.0;
            }
        } else {
            ph.burnIntensity = (d(ph.burnIntensity) - d(dt) * 0.1f32).f();
        }
        let ph = &mut self.physicals[p];
        ph.burnIntensity = mathf::clamp01(ph.burnIntensity);
        self.set_particle_emission(p);
        let ph = &mut self.physicals[p];
        ph.affectSurroundingTimer += dt;
        if ph.affectSurroundingTimer > 1.0 {
            ph.affectSurroundingTimer = 0.0;
            self.affect_surroundings(p);
            let ph = &mut self.physicals[p];
            if ph.Deletable && ph.rigidbody.bodyType == 0 {
                let position = ph.transform.position;
                if self.BoundingBox.contains_expanded([position[0], position[1]], 50.0) {
                    ph.outOfBoundsDuration = 0.0;
                } else {
                    ph.outOfBoundsDuration += 1.0;
                    // ponytail: Disintegrate / Destroy(root) are not rewritten yet.
                    assert!(ph.outOfBoundsDuration < 5.0, "out-of-bounds removal is not rewritten yet");
                }
            }
        }
        assert!(self.physicals[p].penetrations.is_empty(), "penetrations are not rewritten yet");
    }

    /// PhysicalBehaviour.SetParticleEmission: only the flags and the fire emission's Random draws matter.
    fn set_particle_emission(&mut self, p: usize) {
        let flammability = self.properties[self.physicals[p].Properties].Flammability;
        let ph = &mut self.physicals[p];
        if ph.isStatic {
            return;
        }
        if ph.dirtyFire {
            assert!(!(ph.has_fire_map && ph.burnIntensity > 0.01), "fire propagation emission is not rewritten yet");
        } else if ph.burnIntensity > 0.05 {
            ph.dirtyFire = true;
        }
        if !ph.dirtyBubble && ph.Wetness as f64 > 0.01 {
            ph.dirtyBubble = true;
        }
        if !ph.ForceNoCharge && !ph.ForceNoChargeParticles {
            if !ph.dirtyCharged && ph.Charge() > 0.001 {
                ph.dirtyCharged = true;
            }
            if !ph.dirtySmoke && ph.Charge() * flammability > 0.001 {
                ph.dirtySmoke = true;
            }
        }
    }

    /// PhysicalBehaviour.AffectSurroundings
    fn affect_surroundings(&mut self, p: usize) {
        let ph = &self.physicals[p];
        if ph.isStatic || (!ph.OnFire && !(ph.Charge() > 0.001)) {
            return;
        }
        for item in self.physics_objects_near(p) {
            let (me, other) = (&self.physicals[p], &self.physicals[item]);
            if other.IsUnderWater && me.IsUnderWater && me.Charge() > other.Charge() {
                self.physicals[item].charge = me.Charge();
            }
            if self.physicals[p].OnFire && !self.physicals[item].OnFire && self.random.value() > 0.7 {
                self.ignite(item, false);
            }
        }
    }

    /// Global.GetChunkNear
    fn chunk_near(&mut self, position: [f32; 3], radius: f32) -> (i32, i32) {
        let num = radius / 2.0;
        let c = self.random.inside_unit_circle();
        let v = [(position[0] + c[0] * num) * (2.0f32 / 3.0), (position[1] + c[1] * num) * (2.0f32 / 3.0)];
        // Mathf.RoundToInt: Math.Round, ties to even.
        ((v[0] as f64).round_ties_even() as i32, (v[1] as f64).round_ties_even() as i32)
    }

    /// Global.GetChunkOf
    fn chunk_of(&mut self, p: usize) -> (i32, i32) {
        let ph = &self.physicals[p];
        self.chunk_near(ph.transform.position, ph.ObjectArea / 2.0)
    }

    /// Global.GetPhysicsObjectsNear. The game's iterator reads the chunk live; nothing in
    /// AffectSurroundings changes chunk membership, so a copy is the same.
    fn physics_objects_near(&mut self, me: usize) -> Vec<usize> {
        if !self.chunksUpToDate {
            for chunk in self.physical_chunks.values_mut() {
                chunk.clear();
            }
            for p in 0..self.physicals.len() {
                if !self.physicals[p].isDisintegrated {
                    let key = self.chunk_of(p);
                    let chunk = self.physical_chunks.entry(key).or_insert_with(|| Vec::with_capacity(128));
                    if chunk.len() < 128 {
                        chunk.push(p);
                    }
                }
            }
            self.chunksUpToDate = true;
        }
        let key = self.chunk_of(me);
        let Some(chunk) = self.physical_chunks.get(&key) else { return Vec::new() };
        chunk.iter().copied().filter(|&o| o != me && !self.physicals[o].isDisintegrated).collect()
    }

    /// PhysicalBehaviour.ManagedLateUpdate
    pub fn physical_late_update(&mut self, p: usize) {
        if !self.physicals[p].isStatic {
            let v = self.physicals[p].rigidbody.velocity(&self.world);
            self.physicals[p].previousVelocity = v;
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
    /// ponytail: managed-style TRS; the engine's native hierarchy math (parent scale) is not rewritten yet.
    pub fn transform_point(&self, p: usize, local: [f32; 2]) -> [f32; 2] {
        let t = &self.physicals[p].transform;
        let s = [local[0] * t.localScale[0], local[1] * t.localScale[1], 0.0];
        let r = mathf::rotate(t.rotation, s);
        [r[0] + t.position[0], r[1] + t.position[1]]
    }
}
