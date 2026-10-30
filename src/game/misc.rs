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

/// HashSet<T> (System.Core) in slot order: enumeration walks the slots, Remove pushes its slot onto the free list,
/// Add takes the latest freed slot, and removing the last element resets both.
#[derive(Clone, Debug, PartialEq)]
pub struct SlotSet<T> {
    pub slots: Vec<Option<T>>,
    /// The free list; the next Add takes the last.
    pub free: Vec<usize>,
}

impl<T> Default for SlotSet<T> {
    fn default() -> Self {
        Self { slots: Vec::new(), free: Vec::new() }
    }
}

impl<T> SlotSet<T> {
    pub fn add(&mut self, v: T) {
        match self.free.pop() {
            Some(i) => self.slots[i] = Some(v),
            None => self.slots.push(Some(v)),
        }
    }

    pub fn clear(&mut self) {
        self.slots.clear();
        self.free.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(Option::is_none)
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.slots.iter().flatten()
    }

    /// HashSet.RemoveWhere: Remove for each match in slot order.
    pub fn remove_where(&mut self, f: impl Fn(&T) -> bool) {
        for i in 0..self.slots.len() {
            if self.slots[i].as_ref().is_some_and(&f) {
                self.slots[i] = None;
                if self.is_empty() {
                    self.clear();
                    return;
                }
                self.free.push(i);
            }
        }
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
        particles: SlotSet<SplatParticle> = SlotSet::default(),
        /// initialParent world position and the controller's own rotation (read live when attached).
        parent_position: [f32; 3] = [0.0, 0.0, 0.0],
        rotation: [f32; 4] = [0.0, 0.0, 0.0, 1.0],
        /// A bleeding particle's splat: the limb's physical object, the bleeding particle's local Transform and
        /// the controller's local rotation under it. None for a BloodExplosion's (unparented at Start).
        attached: Option<(usize, crate::unity::transform::Trs, [f32; 4])> = None,
        /// False between Instantiate and the delayed Start (loaded scenes are already started).
        started: bool = true,
        destroyed: bool = false,
        CollisionLayers: u32 = 0,
        /// The person whose hierarchy initialParent is in; None for a BloodExplosion, a root of its own.
        parent_person: Option<usize> = None,
    }
}

/// One DecalControllerBehaviour decal: its local position (localDecalPositions), size, sprite and colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Decal {
    pub position: [f32; 2],
    pub size: f32,
    pub sprite: usize,
    pub colour: [f32; 4],
}

/// A DecalDescriptor asset: the fields DecalControllerBehaviour reads (Color is white in both).
#[derive(Debug)]
pub struct DecalDescriptor {
    pub name: &'static str,
    pub IgnoreRadius: f32,
    /// Sprites.Length
    pub Sprites: usize,
    pub MaxDripCount: i32,
}

pub static SPARSE_BLOOD: DecalDescriptor = DecalDescriptor { name: "Sparse blood", IgnoreRadius: 0.2, Sprites: 32, MaxDripCount: 0 };
pub static DENSE_BLOOD: DecalDescriptor = DecalDescriptor { name: "Dense blood", IgnoreRadius: 0.3, Sprites: 32, MaxDripCount: 0 };

/// The descriptor asset of a recorded reference name ("Dense_blood").
pub fn decal_descriptor(name: &str) -> &'static DecalDescriptor {
    [&SPARSE_BLOOD, &DENSE_BLOOD].into_iter().find(|d| d.name.replace(' ', "_") == name).unwrap_or_else(|| unimplemented!("DecalDescriptor {name}"))
}

/// A DecalControllerBehaviour: its descriptor and its decals (localDecalPositions with the sprites' data).
#[derive(Clone, Debug)]
pub struct DecalController {
    pub descriptor: &'static DecalDescriptor,
    pub decals: Vec<Decal>,
}

/// A BloodPuff poolable (the Human PhysicalProperties' ShotImpact), from the pool PoolGenerator made for it.
#[derive(Clone, Debug)]
pub struct BloodPuff {
    /// gameObject.activeSelf: false while it waits in the pool.
    pub active: bool,
    /// The Extra decal emitter's FancyBloodSplatController and whether that child is active (ActivateIfFancyEffects).
    pub splat: usize,
    pub emitter_active: bool,
    /// The root ParticleSystem's time: 3 s, not looping, one burst at 0 whose particles live 0.1-0.3 s, so it
    /// stops (stopAction Callback: PoolableParticleSystemHelper returns it) when the time reaches 3 s. The clock
    /// is a double: thirty 0.1 s frames stop it in the game (dump18), where a float sum stays under 3.
    pub particle_time: f64,
}

/// ObjectPoolBehaviour: createdObjects and availableObjects (HashSets in slot order).
#[derive(Clone, Debug, Default)]
pub struct ObjectPool {
    pub created: Vec<usize>,
    pub available: SlotSet<usize>,
    pub inactivityTimer: f32,
}

/// A behaviour whose Start is still due.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PendingStart {
    Splat(usize),
    /// ExplosionSoundBehviour on a BloodExplosion (three Near clips, StartWhenAwake, no distant sound).
    BloodExplosionSound,
    /// A new BloodPuff's ExplosionSoundBehviour (eight Near clips).
    BloodPuffSound,
    /// ExistifUnderwater.Start of a new BloodPuff's Extra decal emitter (Invert: off under water).
    BloodPuffEmitterWater(usize),
}

impl Game {
    /// ConnectedNodeBehaviour.DisconnectFromEverything: each neighbour forgets its first link back (SetToNull).
    pub fn node_disconnect_from_everything(&mut self, n: usize) {
        for i in 0..self.nodes[n].Connections.len() {
            if let Some(other) = self.nodes[n].Connections[i] {
                if let Some(at) = self.nodes[other].Connections.iter().position(|&c| c == Some(n)) {
                    self.nodes[other].Connections[at] = None;
                }
            }
        }
        self.nodes[n].Connections.fill(None);
    }

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
        // A child of its limb: no Update once the limb is deactivated (disintegrated).
        if self.bleeding[b].destroyed || !self.limb_active(self.bleeding[b].CirculationBehaviour) {
            return;
        }
        let bp = &self.bleeding[b];
        let l = bp.CirculationBehaviour;
        let c = &self.circulations[l];
        let num = if bp.IsOnlyForInternalBleeding { c.InternalBleedingIntensity } else { c.BleedingRate };
        let scaled = mathf::map_range(0.0, 1.0, 0.0, 1.0, c.TotalLiquidAmount);
        let pushing_ok = bp.PushingTo.map_or(true, |p| self.circulations[p].IsDisconnected);
        if pushing_ok && num > 0.05 && scaled > 0.05 && self.physicals[self.limbs[l].PhysicalBehaviour].Temperature > 0.0 {
            let fallback = self.original_blood_color(l);
            let mut color = self.get_computed_color(l, fallback);
            if self.prefs.GorelessMode {
                color = super::circulation::change_red_to_orange(color);
            }
            // transform.position: the bleeding particle's world position, which its splat tracks as initialParent.
            let s = self.bleeding[b].BloodSplatController.expect("BleedingParticle.prefab has its FancyBloodSplatController");
            self.update_splat_pose(s);
            let at = self.splats[s].parent_position;
            let flag = self.is_point_under_water([at[0], at[1]]);
            let dt = self.time.deltaTime;
            let c = &self.circulations[l];
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
                splat.DecalColourMultiplier = color;
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
        for start in std::mem::take(&mut self.pending_starts) {
            match start {
                PendingStart::Splat(s) => {
                    if !self.splats[s].started && !self.splats[s].destroyed {
                        self.splat_start(s);
                    }
                }
                // Play(Delay): audioSource.clip = Near.PickRandom(); DistantSound is None.
                PendingStart::BloodExplosionSound => {
                    self.pick_random(3);
                }
                PendingStart::BloodPuffSound => {
                    self.pick_random(8);
                }
                PendingStart::BloodPuffEmitterWater(b) => {
                    let s = self.blood_puffs[b].splat;
                    let p = self.splats[s].parent_position;
                    if self.is_point_under_water([p[0], p[1]]) {
                        self.blood_puffs[b].emitter_active = false;
                    }
                }
            }
        }
    }

    /// WaterBehaviour.IsPointUnderWater: the WaterBehaviour.waters list is empty, as no water object exists (spawning
    /// one is not rewritten).
    pub fn is_point_under_water(&self, _point: [f32; 2]) -> bool {
        false
    }

    /// PhysicalBehaviour.CreateImpactEffect of a Human (ShotImpact: BloodPuff poolable, tagged Poolable).
    pub fn create_blood_puff(&mut self, p: usize, position: [f32; 2], z: f32, size: f32) {
        let Some(b) = self.pool_request_blood_puff(position) else { return };
        // localScale *= size (presentation); eulerAngles = (0, 0, z): the emitter's rotation (its local one is identity).
        let half = z * mathf::DEG2RAD * 0.5;
        self.splats[self.blood_puffs[b].splat].rotation = [0.0, 0.0, half.sin(), half.cos()];
        // BroadcastMessage("OnImpactCreated") down the object: a limb's CirculationBehaviour.
        if let Some(l) = self.limbs.iter().position(|l| l.PhysicalBehaviour == p) {
            let original = self.original_blood_color(l);
            let color = if mathf::map_range(0.0, 1.0, 0.0, 1.0, self.circulations[l].TotalLiquidAmount) > 0.05 {
                self.get_computed_color(l, original)
            } else {
                original
            };
            // BloodImpactBehaviour.SetColor: the particles' start colour and the emitter's decal colour.
            let mut color = color;
            if self.prefs.GorelessMode {
                color = super::circulation::change_red_to_orange(color);
            }
            color[3] = 0.4;
            self.splats[self.blood_puffs[b].splat].DecalColourMultiplier = color;
        }
    }

    /// PoolGenerator.RequestPrefab(BloodPuff poolable): ObjectPoolBehaviour.Request.
    fn pool_request_blood_puff(&mut self, position: [f32; 2]) -> Option<usize> {
        let pool = self.blood_puff_pool.get_or_insert_with(Default::default);
        pool.inactivityTimer = 0.0;
        let first = pool.available.iter().next().copied();
        if let Some(b) = first {
            pool.available.remove_where(|&x| x == b);
            // transform.position = position; SetActive(true) replays the particle systems (playOnAwake).
            let puff = &mut self.blood_puffs[b];
            puff.active = true;
            puff.particle_time = 0.0;
            let s = puff.splat;
            let emitter = puff.emitter_active;
            self.splats[s].parent_position = [position[0], position[1], 0.0];
            // OnPoolableInitialised, then OnPoolableReinitialised in hierarchy order: ExplosionSoundBehviour.Start
            // (Play), PoolableSizeHelper (localScale), the emitter's FancyBloodSplatController.Start if active.
            self.pick_random(8);
            if emitter {
                self.splat_start(s);
            }
            return Some(b);
        }
        // PoolGenerator.PoolSize
        if self.blood_puff_pool.as_ref().unwrap().created.len() >= 256 {
            return None;
        }
        // Instantiate(Prefab, position, identity). Awake in hierarchy order: the root's ExplosionSoundBehviour,
        // PoolableParticleSystemHelper and PoolableSizeHelper; the Extra decal emitter's and water puff's
        // ActivateIfFancyEffects each draw `Chance(1) > Random.value`.
        let emitter_active = 1.0 > self.random.value() && self.prefs.FancyEffects;
        let _water_puff = 1.0 > self.random.value() && self.prefs.FancyEffects;
        self.splats.push(FancyBloodSplat {
            UnparentAtStart: false,
            DirectionMode: 0,
            Continuous: false,
            Direction: [0.0, 1.0],
            SpawningChance: 0.1,
            AmountRange: [1.0, 5.0],
            AngleRandomisation: 90.0,
            LifetimeInSeconds: [1.0, 2.0],
            EjectionForce: [0.5, 3.0],
            DecalColourMultiplier: [1.0, 1.0, 1.0, 1.0],
            Size: 0.2,
            DestroyWhenNoParticles: false,
            TriggerOnCollide: false,
            particles: Default::default(),
            parent_position: [position[0], position[1], 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            started: false,
            destroyed: false,
            CollisionLayers: 10752,
            attached: None,
            parent_person: None,
        });
        let s = self.splats.len() - 1;
        let b = self.blood_puffs.len();
        self.blood_puffs.push(BloodPuff { active: true, splat: s, emitter_active, particle_time: 0.0 });
        // Starts in the same order: ExplosionSoundBehviour (Play), the Debris and Trail ExistifUnderwaters (on
        // above water), then the emitter's FancyBloodSplatController and ExistifUnderwater.
        self.pending_starts.push(PendingStart::BloodPuffSound);
        if emitter_active {
            self.pending_starts.push(PendingStart::Splat(s));
            self.pending_starts.push(PendingStart::BloodPuffEmitterWater(b));
        }
        let pool = self.blood_puff_pool.as_mut().unwrap();
        pool.created.push(b);
        Some(b)
    }

    /// ObjectPoolBehaviour.Update of the BloodPuff pool: Clear after 300 s with every object back.
    pub fn blood_puff_pool_update(&mut self) {
        let dt = self.time.deltaTime;
        let Some(pool) = &mut self.blood_puff_pool else { return };
        if pool.created.is_empty() {
            return;
        }
        pool.inactivityTimer += dt;
        if pool.inactivityTimer > 300.0 && pool.available.iter().count() == pool.created.len() {
            for &b in &pool.created {
                self.splats[self.blood_puffs[b].splat].destroyed = true;
            }
            pool.created.clear();
            pool.available.clear();
            pool.inactivityTimer = 0.0;
        }
    }

    /// The particle systems' update after LateUpdate: a BloodPuff whose root system ran its 3 s stops, and
    /// PoolableParticleSystemHelper.OnParticleSystemStopped returns it to the pool (SetActive(false)).
    pub fn blood_puff_particles_update(&mut self) {
        let dt = self.time.deltaTime;
        for b in 0..self.blood_puffs.len() {
            let puff = &mut self.blood_puffs[b];
            if !puff.active {
                continue;
            }
            puff.particle_time += dt as f64;
            if puff.particle_time >= 3.0 {
                puff.active = false;
                self.blood_puff_pool.as_mut().unwrap().available.add(b);
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
        // A pooled BloodPuff's emitter: no FixedUpdate while it or the puff is inactive.
        if self.blood_puffs.iter().any(|b| b.splat == s && !(b.active && b.emitter_active)) {
            return;
        }
        // A bleeding particle's splat is inactive with its limb.
        if self.splats[s].attached.is_some_and(|(p, _, _)| !self.physicals[p].active) {
            return;
        }
        self.splats[s].particles.remove_where(|p| p.Life <= 0.0);
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
        let sp = &self.splats[s];
        let (mask, size, colour, root) = (sp.CollisionLayers, sp.Size, sp.DecalColourMultiplier, sp.parent_person);
        for i in 0..self.splats[s].particles.slots.len() {
            let Some(p) = &mut self.splats[s].particles.slots[i] else { continue };
            p.PositionX += p.VelocityX;
            p.PositionY += p.VelocityY;
            p.VelocityX = (d(p.VelocityX) + d(g.x) * fdt / 10.0f32).f();
            p.VelocityY = (d(p.VelocityY) + d(g.y) * fdt / 10.0f32).f();
            p.Life -= fdt;
            let position = [p.PositionX, p.PositionY];
            // initialParent outlives the splat: a bleeding particle destroys its splat with it, and a
            // BloodExplosion's DeleteAfterTime (5 s) comes after its drops (2 s at most) are gone.
            let Some(hit) = self.overlap_point_first(position, mask) else { continue };
            let same_root = root.is_some_and(|person| self.limbs.iter().any(|l| l.PhysicalBehaviour == hit && l.Person == person));
            if colour[3] > 0.05 && !same_root {
                self.physical_decal(hit, &SPARSE_BLOOD, position, size, colour);
                self.splats[s].particles.slots[i].as_mut().unwrap().Life = -1.0;
                if self.splats[s].TriggerOnCollide {
                    unimplemented!("FancyBloodSplatController.OnCollide");
                }
            }
        }
    }

    /// FancyBloodSplatController.CreateParticle
    pub fn create_splat_particle(&mut self, s: usize, index: i32) {
        let life = {
            let v = self.splats[s].LifetimeInSeconds;
            self.random.range(v[0], v[1])
        };
        self.update_splat_pose(s);
        let position = self.splats[s].parent_position;
        let force = self.splat_ejection_force(s, index);
        self.splats[s].particles.add(SplatParticle {
            VelocityX: 0.15 * force[0],
            VelocityY: 0.15 * force[1],
            PositionX: position[0],
            PositionY: position[1],
            Life: life,
        });
    }

    /// initialParent.position of a splat that moves with its limb.
    pub(super) fn update_splat_pose(&mut self, s: usize) {
        let Some((p, bleeding, _)) = self.splats[s].attached else { return };
        let mut v = bleeding.t;
        for node in self.local_chain(p).iter().rev() {
            v = crate::unity::transform::global_position(node, v);
        }
        self.splats[s].parent_position = [v[0], v[1], v[2]];
    }

    /// The splat's transform.TransformDirection(v).
    fn splat_transform_direction(&self, s: usize, v: [f32; 2]) -> [f32; 3] {
        use crate::unity::transform::rotate_direction;
        let sp = &self.splats[s];
        let mut v = [v[0], v[1], 0.0, 0.0];
        match sp.attached {
            None => v = rotate_direction(sp.rotation, None, v),
            Some((p, bleeding, local_rotation)) => {
                v = rotate_direction(bleeding.q, Some(bleeding.s), rotate_direction(local_rotation, None, v));
                for node in self.local_chain(p).iter().rev() {
                    v = rotate_direction(node.q, Some(node.s), v);
                }
            }
        }
        [v[0], v[1], v[2]]
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
            _ => self.splat_transform_direction(s, sp.Direction),
        };
        let r = mathf::rotate(q, dir);
        let e = self.splats[s].EjectionForce;
        let k = self.random.range(e[0], e[1]);
        [r[0] * k, r[1] * k]
    }

    /// Quaternion.Lerp(identity, Quaternion.Euler(0, 0, Random.value * 360), AngleRandomisation / 360)
    fn randomised_angle(&mut self, s: usize) -> [f32; 4] {
        let z = self.random.value() * 360.0 * mathf::DEG2RAD * 0.5;
        // EulerToQuaternion (18073d0c0) calls the CRT's single-precision sinf/cosf.
        let target = [0.0, 0.0, z.sin(), z.cos()];
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
