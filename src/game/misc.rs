//! The small per-limb scripts: ConnectedNodeBehaviour, SkinMaterialHandler, GForceMeasureBehaviour,
//! GripBehaviour, BleedingParticleBehaviour and FancyBloodSplatController.
use super::{d, mathf, script, Game};

/// FixedIntervalDistributor
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FixedIntervalDistributor {
    pub timeStepAccumulator: f32,
    pub MaxCycles: i32,
    pub InterpolationTime: f32,
    pub IntervalSeconds: f32,
}

impl FixedIntervalDistributor {
    pub fn with_rate(hz: f32) -> Self {
        Self { timeStepAccumulator: 0.0, MaxCycles: 256, InterpolationTime: 0.0, IntervalSeconds: 1.0 / hz }
    }
    pub fn calculate_cycle_count(&mut self, dt: f32) -> i32 {
        self.timeStepAccumulator += dt;
        let mut num = 0;
        let interval = self.IntervalSeconds;
        while self.timeStepAccumulator > interval {
            self.timeStepAccumulator -= interval;
            num += 1;
        }
        self.InterpolationTime = self.timeStepAccumulator / interval;
        num.min(self.MaxCycles)
    }
}

script! {
    Node {
        IsRoot: bool = false,
        Value: i32 = 0,
        ShouldLoadSerialisableConnections: bool = false,
        IsConnectedToRoot: bool = true,
    }
    links {
        /// Connections as limb indices; a destroyed or disconnected slot is None.
        Connections: Vec<Option<usize>> = Vec::new(),
    }
}

/// DamageType
pub mod damage_type {
    pub const BLUNT: i32 = 0;
    pub const BULLET: i32 = 1;
    pub const STAB: i32 = 2;
    pub const DISMEMBERMENT: i32 = 3;
    pub const BURN: i32 = 4;
    pub const NONE: i32 = 5;
}

script! {
    Skin {
        BluntDamageRegen: bool = true,
        GunshotDamageRegen: bool = true,
        BurnWoundRegen: bool = true,
        TrackWoundAge: bool = true,
        ShouldRecolourWhenCold: bool = false,
        ShouldGlowWhenHot: bool = false,
        MaxGlowTemperature: i32 = 1000,
        MinFreezeTemperature: i32 = -30,
        currentDamagePointCount: i32 = 0,
        intensityMultiplier: f32 = 1.0,
        rottenProgress: f32 = 0.0,
        oldRottenProgress: f32 = 0.0,
        acidProgress: f32 = 0.0,
        oldAcidProgress: f32 = 0.0,
        ShouldRot: bool = true,
    }
    links {
        /// Vector4 (x, y, intensity, DamageType)
        damagePoints: Vec<[f32; 4]> = vec![[0.0, 0.0, 0.0, damage_type::NONE as f32]; 128],
        damagePointTimeStamps: Vec<f32> = vec![0.0; 128],
        intervalDistributor: FixedIntervalDistributor = FixedIntervalDistributor::with_rate(16.0),
        adjacentLimbs: Vec<usize> = Vec::new(),
    }
}

impl Skin {
    /// RottenProgress setter (the material always exists).
    pub fn set_rotten(&mut self, value: f32) {
        let num = mathf::clamp01(value);
        if (self.oldRottenProgress - num).abs() > 0.01 {
            self.oldRottenProgress = num;
        }
        self.rottenProgress = num;
    }
    /// AcidProgress setter; the original stores the previous value as oldAcidProgress.
    pub fn set_acid(&mut self, value: f32) {
        let num = mathf::clamp01(value);
        if (self.oldAcidProgress - num).abs() > 0.01 {
            self.oldAcidProgress = self.acidProgress;
        }
        self.acidProgress = num;
    }
}

script! {
    GForce {
        SustainedAccelerationRatio: f32 = 0.3,
        velocity: [f32; 2] = [0.0, 0.0],
        velocityLastFrame: [f32; 2] = [0.0, 0.0],
        acceleration: [f32; 2] = [0.0, 0.0],
        sustainedAcceleration: [f32; 2] = [0.0, 0.0],
    }
    links {}
}

script! {
    Grip {
        isHolding: bool = false,
    }
    links {
        PhysicalBehaviour: usize = 0,
        /// The held PhysicalBehaviour.
        CurrentlyHolding: Option<usize> = None,
    }
}

script! {
    BleedingParticle {
        IsOnlyForInternalBleeding: bool = false,
        SpeedMultiplier: f32 = 1.0,
        RateMultiplier: f32 = 1.0,
        Laminarity: f32 = 0.0,
        DripSoundRateThreshold: f32 = 2.0,
        ShouldBecomeSmokeInWater: bool = false,
        PulseSpeed: f32 = 12.0,
        PulseIntensity: f32 = 2.0,
        fakeHeartClock: f32 = 0.0,
    }
    links {
        /// CirculationBehaviour / PushingTo as limb indices.
        CirculationBehaviour: usize = 0,
        PushingTo: Option<usize> = None,
        BloodSplatController: Option<usize> = None,
        dripSoundClock: FixedIntervalDistributor = FixedIntervalDistributor::with_rate(0.5),
        /// DripSounds length; None when DripSounds is null.
        DripSounds: Option<usize> = None,
        /// ParticleSystemRenderer.isVisible, owned by the renderer.
        visible: bool = false,
        /// ParticleSystem main/emission values the renderer draws from.
        startSpeedMultiplier: f32 = 0.0,
        rateOverTimeMultiplier: f32 = 0.0,
        emission_enabled: bool = false,
        destroyed: bool = false,
    }
}

/// FancyBloodSplatController.Particle
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SplatParticle {
    pub VelocityX: f32,
    pub VelocityY: f32,
    pub PositionX: f32,
    pub PositionY: f32,
    pub Life: f32,
}

script! {
    FancyBloodSplat {
        UnparentAtStart: bool = true,
        DirectionMode: i32 = 0,
        Continuous: bool = false,
        Direction: [f32; 2] = [1.0, 0.0],
        SpawningChance: f32 = 0.1,
        AmountRange: [f32; 2] = [1.0, 4.0],
        AngleRandomisation: f32 = 15.0,
        LifetimeInSeconds: [f32; 2] = [1.0, 2.0],
        EjectionForce: [f32; 2] = [1.0, 5.0],
        DecalColourMultiplier: [f32; 4] = [1.0, 1.0, 1.0, 1.0],
        Size: f32 = 0.2,
        DestroyWhenNoParticles: bool = true,
        TriggerOnCollide: bool = false,
    }
    links {
        particles: Vec<SplatParticle> = Vec::new(),
        /// initialParent world position and the controller's own rotation.
        parent_position: [f32; 3] = [0.0, 0.0, 0.0],
        rotation: [f32; 4] = [0.0, 0.0, 0.0, 1.0],
        /// False between Instantiate and the delayed Start (loaded scenes are already started).
        started: bool = true,
        destroyed: bool = false,
    }
}

impl Game {
    /// ConnectedNodeBehaviour.IsConnectedTo
    pub fn node_is_connected_to(&self, a: usize, b: usize) -> bool {
        self.nodes[a].Connections.contains(&Some(b))
    }

    /// NodeController.Update over NodeController.Nodes (HashSet, insertion order).
    pub fn node_controller_update(&mut self) {
        for n in &mut self.nodes {
            n.IsConnectedToRoot = false;
            n.Value = if n.IsRoot { 1_000_000 } else { 0 };
        }
        for i in 0..self.nodes.len() {
            if self.nodes[i].IsRoot {
                self.propagate_value(i);
            }
        }
    }

    fn propagate_value(&mut self, me: usize) {
        let n = &mut self.nodes[me];
        n.IsConnectedToRoot = n.IsRoot || n.Value > 0;
        for i in 0..self.nodes[me].Connections.len() {
            if let Some(c) = self.nodes[me].Connections[i] {
                let v = self.nodes[me].Value;
                if self.nodes[c].Value < v {
                    self.nodes[c].Value = v - 1;
                    self.propagate_value(c);
                }
            }
        }
    }

    /// GForceMeasureBehaviour.FixedUpdate
    pub fn gforce_fixed_update(&mut self, l: usize) {
        let v = self.physicals[self.limbs[l].PhysicalBehaviour].rigidbody.velocity(&self.world);
        let g = &mut self.gforces[l];
        g.velocityLastFrame = g.velocity;
        g.velocity = v;
        g.acceleration = [g.velocity[0] - g.velocityLastFrame[0], g.velocity[1] - g.velocityLastFrame[1]];
        g.sustainedAcceleration = mathf::v2_lerp(g.sustainedAcceleration, g.acceleration, 0.5);
    }

    /// GripBehaviour.FixedUpdate
    pub fn grip_fixed_update(&mut self, g: usize) {
        let grip = &self.grips[g];
        if let Some(held) = grip.CurrentlyHolding {
            let me = grip.PhysicalBehaviour;
            if self.physicals[me].SimulateTemperature && self.physicals[held].SimulateTemperature {
                self.average_temperature(me, held, 0.02);
            }
        }
    }

    /// GripBehaviour.Update
    pub fn grip_update(&mut self, g: usize) {
        if self.grips[g].isHolding && self.grips[g].CurrentlyHolding.is_none() {
            // DropObject without a held object: only the (unrewritten) FixedJoint2D goes.
            self.grips[g].isHolding = false;
        }
    }

    /// SkinMaterialHandler.Update
    pub fn skin_update(&mut self, l: usize) {
        let dt = self.time.deltaTime;
        // The loop condition re-runs CalculateCycleCount (adding dt again) every iteration.
        let mut i = 0;
        while i < self.skins[l].intervalDistributor.calculate_cycle_count(dt) {
            i += 1;
            let interval = self.skins[l].intervalDistributor.IntervalSeconds;
            if !self.limbs[l].IsZombie && self.prefs.AutoHealWounds && self.limb_is_considered_alive(l) {
                // RegenerateWounds; Sync only uploads to the material.
                self.regenerate_wounds(l, interval);
            }
            assert!(self.physicals[self.limbs[l].PhysicalBehaviour].victimPenetrations.is_empty());
        }
    }

    fn regenerate_wounds(&mut self, l: usize, dt: f32) -> bool {
        use damage_type::*;
        let num = mathf::pow(0.95, dt);
        let num2 = mathf::pow(0.995, dt);
        let num3 = mathf::pow(0.97, dt);
        let mut result = false;
        let flag = self.skins[l].GunshotDamageRegen
            && self.circulations[l].BleedingRate < 1.0
            && self.limbs[l].Health > self.limbs[l].InitialHealth / 2.0;
        let s = &mut self.skins[l];
        for i in 0..128 {
            let p = s.damagePoints[i];
            let ty = mathf::floor(p[3]) as i32;
            let k = match ty {
                BLUNT if s.BluntDamageRegen && p[2] > 0.001 => num,
                BULLET | STAB if flag && p[2] > 0.001 => num2,
                BURN if s.BurnWoundRegen && p[2] > 0.001 => num3,
                _ => continue,
            };
            s.damagePoints[i][2] *= k;
            result = true;
        }
        result
    }

    /// SkinMaterialHandler.LateUpdate
    pub fn skin_late_update(&mut self, l: usize) {
        let limb = &self.limbs[l];
        if limb.SpeciesIdentity != "Android"
            && self.skins[l].ShouldRot
            && !(self.physicals[limb.PhysicalBehaviour].Temperature < -50.0)
        {
            let dt = self.time.deltaTime;
            let mut num = 0.0f32;
            if limb.Health < 0.05 {
                num = (d(num) + d(dt) * 0.002f32).f();
            }
            if limb.IsZombie {
                num = (d(num) + d(dt) * 0.02f32).f();
            }
            let v = self.skins[l].rottenProgress + num;
            self.skins[l].set_rotten(v);
        }
    }

    /// BleedingParticleBehaviour.Update
    pub fn bleeding_update(&mut self, b: usize) {
        if self.bleeding[b].destroyed {
            return;
        }
        let bp = &self.bleeding[b];
        let l = bp.CirculationBehaviour;
        let c = &self.circulations[l];
        let num = if bp.IsOnlyForInternalBleeding { c.InternalBleedingIntensity } else { c.BleedingRate };
        let scaled = mathf::map_range(0.0, 1.0, 0.0, 1.0, c.TotalLiquidAmount);
        let pushing_ok = bp.PushingTo.map_or(true, |p| self.circulations[p].IsDisconnected);
        if pushing_ok && num > 0.05 && scaled > 0.05 && self.physicals[self.limbs[l].PhysicalBehaviour].Temperature > 0.0 {
            // GetComputedColor / goreless recolouring and the shape/drag modules are visual only.
            // ponytail: WaterBehaviour.IsPointUnderWater; water is not rewritten yet, so never under water.
            let flag = false;
            let dt = self.time.deltaTime;
            let (flow, mult, total) = (c.BloodFlow, c.BloodLossRateMultiplier, c.TotalLiquidAmount);
            let bp = &mut self.bleeding[b];
            bp.fakeHeartClock = (d(bp.fakeHeartClock) + d(dt) * flow * bp.PulseSpeed).f();
            let num2 = (d(bp.SpeedMultiplier) * (d(flow) * flow * flow * 2.0f32) * mult * mathf::lerp(0.7, 1.0, bp.Laminarity)).f();
            let num3 = bp.PulseIntensity * flow;
            bp.startSpeedMultiplier = (d(num2) * (d(1.0) + d(num3) * (d(1.0) - bp.fakeHeartClock % 1.0))).f();
            let num4 = if total > mathf::EPSILON { mathf::min(mult * num, 1.0) } else { 0.0 };
            bp.rateOverTimeMultiplier = if flag { 0.0 } else { (d(mathf::lerp(18.0, 32.0, bp.Laminarity)) * num4 * bp.RateMultiplier).f() };
            bp.emission_enabled = true;
            if let Some(s) = bp.BloodSplatController {
                let speed = bp.startSpeedMultiplier;
                let splat = &mut self.splats[s];
                splat.EjectionForce = [0.25 * speed, 0.35 * speed];
                splat.SpawningChance = if flag { 0.0 } else { total * 0.5 };
            }
            let bp = &mut self.bleeding[b];
            let flag2 = bp.DripSounds.is_some() && bp.visible && (d(num4) * bp.RateMultiplier).0 >= bp.DripSoundRateThreshold as f64;
            if !flag2 {
                return;
            }
            // The loop condition re-runs CalculateCycleCount (adding dt again) every iteration.
            let mut i = 0;
            while i < self.bleeding[b].dripSoundClock.calculate_cycle_count(dt) {
                if self.random.value() > 0.2 {
                    // PlayClipOnce(DripSounds.PickRandom(), volume)
                    let n = self.bleeding[b].DripSounds.unwrap();
                    self.pick_random(n);
                }
                i += 1;
            }
        } else {
            let bp = &mut self.bleeding[b];
            bp.startSpeedMultiplier = 0.0;
            bp.rateOverTimeMultiplier = 0.0;
            bp.emission_enabled = false;
            if let Some(s) = bp.BloodSplatController {
                self.splats[s].SpawningChance = 0.0;
            }
        }
    }

    /// The delayed Start of every object instantiated since the last pass. Unity's player loop runs it
    /// at FixedUpdate.ScriptRunDelayedFixedFrameRate (before each step's FixedUpdates) and
    /// Update.ScriptRunDelayedDynamicFrameRate (before Update), whichever comes first after the Instantiate.
    pub fn run_pending_starts(&mut self) {
        for s in 0..self.splats.len() {
            if !self.splats[s].started && !self.splats[s].destroyed {
                self.splat_start(s);
            }
        }
    }

    /// FancyBloodSplatController.Start (initialParent is the bleeding particle: parent_position).
    pub fn splat_start(&mut self, s: usize) {
        self.splats[s].started = true;
        // UnparentAtStart only changes the hierarchy.
        let v = self.splats[s].AmountRange;
        let num = (self.random.range(v[0], v[1]) as f64).round_ties_even() as i32;
        self.splats[s].particles.clear();
        if !self.splats[s].Continuous {
            for i in 0..num {
                self.create_splat_particle(s, i);
            }
        }
    }

    /// FancyBloodSplatController.FixedUpdate
    pub fn splat_fixed_update(&mut self, s: usize) {
        if self.splats[s].destroyed {
            return;
        }
        // Objects instantiated during this step have not had Start yet and are not in this FixedUpdate pass.
        if !self.splats[s].started {
            return;
        }
        self.splats[s].particles.retain(|p| !(p.Life <= 0.0));
        if self.splats[s].Continuous {
            if self.random.value() <= self.splats[s].SpawningChance {
                self.create_splat_particle(s, 0);
            }
        } else if self.splats[s].particles.is_empty() && self.splats[s].DestroyWhenNoParticles {
            self.splats[s].destroyed = true;
            return;
        }
        let fdt = self.time.fixedDeltaTime;
        let g = self.world.gravity;
        for p in &mut self.splats[s].particles {
            p.PositionX += p.VelocityX;
            p.PositionY += p.VelocityY;
            p.VelocityX = (d(p.VelocityX) + d(g.x) * fdt / 10.0f32).f();
            p.VelocityY = (d(p.VelocityY) + d(g.y) * fdt / 10.0f32).f();
            p.Life -= fdt;
            // ponytail: Physics2D.OverlapPoint + "Decal" message; decals are visual and not rewritten yet.
        }
    }

    /// FancyBloodSplatController.CreateParticle
    pub fn create_splat_particle(&mut self, s: usize, index: i32) {
        let life = {
            let v = self.splats[s].LifetimeInSeconds;
            self.random.range(v[0], v[1])
        };
        let position = self.splats[s].parent_position;
        let force = self.splat_ejection_force(s, index);
        self.splats[s].particles.push(SplatParticle {
            VelocityX: 0.15 * force[0],
            VelocityY: 0.15 * force[1],
            PositionX: position[0],
            PositionY: position[1],
            Life: life,
        });
    }

    fn splat_ejection_force(&mut self, s: usize, index: i32) -> [f32; 2] {
        let q = self.randomised_angle(s);
        let sp = &self.splats[s];
        let dir = match sp.DirectionMode {
            1 => [sp.Direction[0], sp.Direction[1], 0.0],
            2 => {
                let num = mathf::max(sp.AmountRange[0], sp.AmountRange[1]) as i32;
                let f = (d((360 / num) as f32) * index as f32 * (std::f32::consts::PI / 180.0)).f();
                [(f as f64).cos() as f32, (f as f64).sin() as f32, 0.0]
            }
            _ => mathf::rotate(sp.rotation, [sp.Direction[0], sp.Direction[1], 0.0]),
        };
        let r = mathf::rotate(q, dir);
        let e = self.splats[s].EjectionForce;
        let k = self.random.range(e[0], e[1]);
        [r[0] * k, r[1] * k]
    }

    /// Quaternion.Lerp(identity, Quaternion.Euler(0, 0, Random.value * 360), AngleRandomisation / 360)
    fn randomised_angle(&mut self, s: usize) -> [f32; 4] {
        let z = self.random.value() * 360.0 * mathf::DEG2RAD * 0.5;
        let target = [0.0, 0.0, (z as f64).sin() as f32, (z as f64).cos() as f32];
        let t = mathf::clamp01(self.splats[s].AngleRandomisation / 360.0);
        // QuaternionTpl Lerp: dot(identity, target) = target.w; negative flips the target.
        let sign = if target[3] < 0.0 { -1.0 } else { 1.0 };
        let q = [
            0.0 + t * (sign * target[0] - 0.0),
            0.0 + t * (sign * target[1] - 0.0),
            0.0 + t * (sign * target[2] - 0.0),
            1.0 + t * (sign * target[3] - 1.0),
        ];
        let n = mathf::sqrt(q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]);
        [q[0] / n, q[1] / n, q[2] / n, q[3] / n]
    }
}
