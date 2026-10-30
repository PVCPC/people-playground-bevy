//! BallisticsEmitter (the bullets of FirearmBehaviour and GlobalShrapnelEmitter), BulletTracerPool and
//! CameraShakeBehaviour.
#![allow(non_snake_case)]

use super::destroyable::{Coroutine, Step};
use super::physical::dot2;
use super::{d, mathf, Behaviour, Game};
use crate::b2::math::Vec2 as V;

/// Cartridge (ScriptableObject): the values the shot reads.
#[derive(Clone, Copy, Debug, Default)]
pub struct Cartridge {
    pub Damage: f32,
    pub Recoil: f32,
    pub ImpactForce: f32,
    pub StartSpeed: f32,
    pub PenetrationRandomAngleMultiplier: f32,
}

const THICKNESS: f32 = 0.08;
const MATERIAL_HARDNESS_MULTIPLIER: f32 = 0.35;
const IMPACT_FORCE_MULTIPLIER: f32 = 18.0;
const BULLET_DROP_MULTIPLIER: f32 = 0.25;
const CRACK_SPEED_THRESHOLD: f32 = 5.0;
const CRACK_CHANCE: f32 = 0.8;
const CRACK_COOLDOWN: f32 = 0.25;
const WATER_LAYER: u32 = 1 << 4;

/// The classes with a Shot(Shot) / ExitShot(Shot) method, and their subclasses.
const SHOT_RECEIVERS: &[&str] = &[
    "PhysicalBehaviour", "DestroyableBehaviour", "ExplosiveBehaviour", "DamagableMachineryBehaviour", "TyreDeflationBehaviour", "ActOnShot", "BalloonBehaviour",
    "BreakJointOnShotBehaviour", "BulbBehaviour", "CathodeLightBehaviour", "CirculationBehaviour", "DebugTargetDamageMeter", "DroneBehaviour", "FireworksBehaviour",
    "GlowtubeBehaviour", "JavelinMissileBehaviour", "LimbBehaviour", "LiquidContainerLeakingBehaviour", "OldSyringeBehaviour", "PersonBehaviour",
    "SentryTurretBehaviour", "ShotMessagePropagator", "SyringeBehaviour", "TelevisionBehaviour", "WaterImpactBehaviour", "VaseBehaviour",
    "AcidSyringe", "AdrenalineSyringe", "BoneEatingPoisonSyringe", "CoagulationSyringe", "DeathSyringe", "EmptySyringe", "FreezeSyringe", "KnockoutSyringe", "LifeSyringe",
    "MendingSyringe", "PainKillerSyringe", "PinkSyringe", "UltraStrengthSyringe", "WaterBreathingSyringe", "ZombieSyringe",
];
const EXIT_SHOT_RECEIVERS: &[&str] = &[
    "PhysicalBehaviour", "BalloonBehaviour", "CirculationBehaviour", "LimbBehaviour", "LiquidContainerLeakingBehaviour", "OldSyringeBehaviour", "SyringeBehaviour",
    "AcidSyringe", "AdrenalineSyringe", "BoneEatingPoisonSyringe", "CoagulationSyringe", "DeathSyringe", "EmptySyringe", "FreezeSyringe", "KnockoutSyringe", "LifeSyringe",
    "MendingSyringe", "PainKillerSyringe", "PinkSyringe", "UltraStrengthSyringe", "WaterBreathingSyringe", "ZombieSyringe",
];

impl Cartridge {
    /// A Cartridge asset's serialized fields.
    pub fn read(v: &crate::assets::mono::Value) -> Result<Self, String> {
        let f = |name: &str| v.get(name).and_then(crate::assets::mono::Value::f32).ok_or_else(|| format!("Cartridge.{name}"));
        Ok(Self { Damage: f("Damage")?, Recoil: f("Recoil")?, ImpactForce: f("ImpactForce")?, StartSpeed: f("StartSpeed")?, PenetrationRandomAngleMultiplier: f("PenetrationRandomAngleMultiplier")? })
    }
}

/// A BallisticsEmitter: a FirearmBehaviour's (made in Start) or the GlobalShrapnelEmitter's.
#[derive(Clone, Debug)]
pub struct Emitter {
    pub cartridge: Cartridge,
    pub MaxBallisticsIterations: u32,
    pub MaxTotalDistance: f32,
    pub RicochetChanceMultiplier: f32,
    pub RenderTracersIfTraversal: bool,
    /// BulletRicochetCallback is FirearmBehaviour.OnRicochet.
    pub ricochet_callback: bool,
    pub LayersToHit: u32,
    pub WaterLayer: u32,
    pub MaxRange: f32,
    pub DoTraversal: bool,
    pub Charge: f32,
    pub lastCrackPlayTime: f32,
    /// `tracers`: how many of the pool's tracers it holds (which ones is rendering).
    pub tracers: usize,
}

impl Emitter {
    pub fn new(cartridge: Cartridge, layers: u32, water: u32) -> Self {
        Self {
            cartridge,
            MaxBallisticsIterations: 128,
            MaxTotalDistance: 1000.0,
            RicochetChanceMultiplier: 1.0,
            RenderTracersIfTraversal: true,
            ricochet_callback: false,
            LayersToHit: layers,
            WaterLayer: water,
            MaxRange: 1000.0,
            DoTraversal: false,
            Charge: 0.0,
            lastCrackPlayTime: f32::MIN,
            tracers: 0,
        }
    }
}

/// The level1 GlobalShrapnelEmitter (GlobalShrapnelEmitter.Instance), when its Awake ran.
#[derive(Clone, Debug)]
pub struct ShrapnelEmitter {
    pub emitter: Emitter,
}

/// The coroutine owner of the GlobalShrapnelEmitter: a scene object that is never destroyed.
pub const SHRAPNEL_OWNER: i32 = i32::MIN;

/// The arguments of one BallisticIteration.
#[derive(Clone, Copy, Debug)]
pub struct Bullet {
    /// The ConnectedBehaviour: a FirearmBehaviour (`Game::firearms`), or None for the GlobalShrapnelEmitter.
    pub firearm: Option<usize>,
    pub origin: [f32; 2],
    pub direction: [f32; 2],
    pub speed: f32,
    pub iteration: u32,
    pub total: f32,
    pub previous: Option<i32>,
    pub tracer: bool,
    pub splash: bool,
    pub can_explode: bool,
    pub charge: f32,
}

/// The locals of a BallisticIteration in its WaitForSeconds(0.01) after a raycast hit.
#[derive(Clone, Copy, Debug)]
pub struct BulletHit {
    pub physical: usize,
    pub hardness: f32,
    pub damage: f32,
    pub collider: i32,
    pub point: [f32; 2],
    pub normal: [f32; 2],
    pub distance: f32,
}

/// The scene's BulletTracerPool (ObjectPoolBehaviour of "Tracer poolable"): its clones are render-only, so the
/// counts are what matters (instance ids, MaxPoolSize).
#[derive(Clone, Debug, Default)]
pub struct TracerPool {
    pub created: usize,
    pub available: usize,
    pub inactivityTimer: f32,
    pub MaxPoolSize: usize,
    pub InactivityThresholdInSeconds: f32,
    /// The ids an Instantiate of the prefab takes.
    pub clone_ids: usize,
}

/// Global.main.camera and CameraShakeBehaviour.
#[derive(Clone, Debug)]
pub struct CameraState {
    pub shakeIntensity: f32,
    /// transform.root.position, which CameraShakeBehaviour.Update writes.
    pub root: [f32; 3],
    /// The Camera's orthographicSize as level1 sets it (headless).
    pub orthographic_size: f32,
}

impl Default for CameraState {
    fn default() -> Self {
        Self { shakeIntensity: 0.0, root: [0.0; 3], orthographic_size: 5.0 }
    }
}

fn add(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] + b[0], a[1] + b[1]]
}
fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
fn mul(a: [f32; 2], s: f32) -> [f32; 2] {
    [a[0] * s, a[1] * s]
}

impl Game {
    // ---- Camera ---------------------------------------------------------------------------------------------------

    /// Global.main.camera.orthographicSize: the window's view, else level1's.
    pub fn camera_orthographic_size(&self) -> f32 {
        match self.input.view {
            Some((lo, hi)) => (hi[1] - lo[1]) * 0.5,
            None => self.camera.orthographic_size,
        }
    }

    /// Global.main.camera.transform.position (xy): the shaken root plus the camera's place under it.
    pub fn camera_position(&self) -> [f32; 2] {
        let at = match self.input.view {
            Some((lo, hi)) => [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5],
            None => [0.0, 0.0],
        };
        [self.camera.root[0] + at[0], self.camera.root[1] + at[1]]
    }

    /// CameraShakeBehaviour.FixedUpdate
    pub fn camera_shake_fixed_update(&mut self) {
        self.camera.shakeIntensity *= 0.85;
    }

    /// CameraShakeBehaviour.Update: the root's position (its own rotation does not move it).
    pub fn camera_shake_update(&mut self) {
        let i = self.camera.shakeIntensity;
        let i = mathf::clamp(if i.is_nan() { 0.0 } else { i }, 0.0, 5.0);
        self.camera.shakeIntensity = i;
        let t = self.time.time;
        let noise = |s: f32| (d(mathf::perlin(t * 24.0, s)) * 2.0f32 - 1.0f32).f();
        let num = (d(noise(1000.0)) * i * 0.1f32).f();
        let y = (d(mathf::sin(t * 80.0)) * i * 0.04f32).f();
        let num = num * num;
        let s = (d(0.5f32) * self.prefs.ShakeIntensity).f();
        self.camera.root = [s * num, s * y, s * 0.0];
    }

    /// CameraShakeBehaviour.Shake(intensity, position)
    pub fn camera_shake(&mut self, intensity: f32, position: [f32; 2]) {
        // CameraListenerBehaviour.NormalisedDistance
        let nd = (d(mathf::clamp(self.camera_orthographic_size(), 5.0, 200.0)) - 5.0f32).f() / (200.0 - 5.0);
        let num = (1.0 - d(nd) * 0.5f32).f();
        let num2 = mathf::max(1.0, mathf::magnitude2(sub(position, self.camera_position())));
        self.camera.shakeIntensity = (d(self.camera.shakeIntensity) + d(intensity) * num / num2).f();
    }

    // ---- BulletTracerPool -----------------------------------------------------------------------------------------

    /// ObjectPoolBehaviour.Request: false when the pool is full.
    fn tracer_request(&mut self) -> bool {
        let pool = &mut self.tracer_pool;
        pool.inactivityTimer = 0.0;
        if pool.available > 0 {
            pool.available -= 1;
            return true;
        }
        if pool.created >= pool.MaxPoolSize {
            return false;
        }
        pool.created += 1;
        for _ in 0..self.tracer_pool.clone_ids {
            self.allocate_instance_id();
        }
        true
    }

    /// ObjectPoolBehaviour.Return
    fn tracer_return(&mut self) {
        self.tracer_pool.available += 1;
    }

    /// ObjectPoolBehaviour.Update of the BulletTracerPool
    pub fn tracer_pool_update(&mut self) {
        let dt = self.time.deltaTime;
        let pool = &mut self.tracer_pool;
        if pool.created != 0 {
            pool.inactivityTimer += dt;
            if pool.inactivityTimer > pool.InactivityThresholdInSeconds && pool.available == pool.created {
                // Clear: Destroy each clone.
                pool.created = 0;
                pool.available = 0;
                pool.inactivityTimer = 0.0;
            }
        }
    }

    /// BallisticsEmitter.Dispose (FirearmBehaviour.OnDestroy): the tracers it still holds go back.
    pub fn emitter_dispose(&mut self, f: usize) {
        if let Some(e) = self.firearms[f].emitter.as_mut() {
            let n = std::mem::take(&mut e.tracers);
            for _ in 0..n {
                self.tracer_return();
            }
        }
    }

    // ---- BallisticsEmitter ----------------------------------------------------------------------------------------

    fn emitter(&self, f: Option<usize>) -> &Emitter {
        match f {
            Some(f) => self.firearms[f].emitter.as_ref().unwrap_or_else(|| unimplemented!("FirearmBehaviour.Shoot before Start (NullReferenceException)")),
            None => &self.shrapnel.as_ref().expect("the GlobalShrapnelEmitter").emitter,
        }
    }

    fn emitter_mut(&mut self, f: Option<usize>) -> &mut Emitter {
        match f {
            Some(f) => self.firearms[f].emitter.as_mut().unwrap(),
            None => &mut self.shrapnel.as_mut().expect("the GlobalShrapnelEmitter").emitter,
        }
    }

    /// The ConnectedBehaviour's instance id (its coroutines' owner).
    fn emitter_owner(&self, f: Option<usize>) -> i32 {
        f.map_or(SHRAPNEL_OWNER, |f| self.firearms[f].id)
    }

    /// BallisticsEmitter.Emit(origin, direction)
    pub fn emitter_emit(&mut self, f: Option<usize>, origin: [f32; 2], direction: [f32; 2]) {
        let e = self.emitter(f);
        let (traversal, charge, speed) = (e.DoTraversal && e.RenderTracersIfTraversal, e.Charge, e.cartridge.StartSpeed);
        let mut tracer = false;
        if traversal && self.tracer_request() {
            self.emitter_mut(f).tracers += 1;
            tracer = true;
        }
        let b = Bullet { firearm: f, origin, direction: mathf::normalized2(direction), speed, iteration: 0, total: 0.0, previous: None, tracer, splash: true, can_explode: true, charge };
        self.bullet_iterate(b);
    }

    /// The transform.root of a physical's GameObject, as its Transform id.
    pub(super) fn root_id(&self, p: usize) -> i32 {
        match self.root_of(p) {
            (true, n) => self.transform_nodes[n].id,
            (false, p) => self.physicals[p].transform_id,
        }
    }

    fn remove_tracer(&mut self, b: &Bullet) {
        if b.tracer {
            self.emitter_mut(b.firearm).tracers -= 1;
            self.tracer_return();
        }
    }

    /// Random insideUnitCircle * PenetrationRandomAngleMultiplier
    fn bullet_random_angle(&mut self, f: Option<usize>) -> [f32; 2] {
        let m = self.emitter(f).cartridge.PenetrationRandomAngleMultiplier;
        mul(self.random.inside_unit_circle(), m)
    }

    /// GetDamageForDistance(totalDistance)
    fn damage_for_distance(&self, f: Option<usize>, distance: f32) -> f32 {
        // DamageMultiplierByDistance
        let m = mathf::clamp((d(1.5f32) / distance + d(0.7f32) / mathf::lerp(distance, 0.8, 0.992)).f(), 0.5, 10.0);
        self.emitter(f).cartridge.Damage * m
    }

    /// CalculateHardness(phys)
    fn hardness(&self, p: usize) -> f32 {
        let ph = &self.physicals[p];
        let props = &self.properties[ph.Properties];
        let num = 1.0 - mathf::clamp01(props.Softness + props.Brittleness);
        let num2 = (d(2.0f32) * MATERIAL_HARDNESS_MULTIPLIER * (d(4.5f32) * num) + props.BulletSpeedAbsorptionPower).f();
        if !ph.BulletDeflectChanceOverride.is_nan() {
            return num2 + mathf::max(ph.BulletDeflectChanceOverride, 0.0);
        }
        num2
    }

    /// PhysicalObjectsInWorldByTransform.TryGetValue(transform) for a transform that is the collider's own (overlap)
    /// or its attached Rigidbody2D's (raycast), with the root check before it. The GlobalShrapnelEmitter's root is a
    /// level1 object, never a physical's root.
    fn bullet_target(&self, f: Option<usize>, p: Option<usize>) -> Option<usize> {
        let root = f.map(|f| self.root_id(self.firearms[f].physical.expect("a FirearmBehaviour without a Rigidbody2D")));
        p.filter(|&p| Some(self.root_id(p)) != root && self.physicals[p].in_world && !self.physicals[p].destroyed)
    }

    /// BallisticIteration up to its next yield; the overlap path's StartCoroutine runs its iteration at once.
    fn bullet_iterate(&mut self, mut b: Bullet) {
        loop {
            let f = b.firearm;
            let e = self.emitter(f);
            if b.iteration >= e.MaxBallisticsIterations || b.total >= e.MaxTotalDistance {
                self.remove_tracer(&b);
                return;
            }
            if self.is_point_in_lava(b.origin) {
                self.randomise_course(1.0, 20.0, &mut b);
            } else if self.is_point_under_water(b.origin) {
                self.randomise_course(0.99999, 10.0, &mut b);
            }
            let e = self.emitter(f);
            let (start_speed, layers, traversal, max_range) = (e.cartridge.StartSpeed, e.LayersToHit, e.DoTraversal, e.MaxRange);
            // SpeedMultiplierByDistance
            let by_distance = 1.0 / mathf::max(0.1, mathf::sqrt((d(b.total) * 0.2f32).f()));
            let num = mathf::clamp(b.speed / start_speed, 0.05, 1.0) * by_distance;
            if num <= 0.1 {
                self.remove_tracer(&b);
                return;
            }
            let hit = self.overlap_point_depth(b.origin, layers, -0.1, 0.1);
            if let Some((p, collider)) = hit.and_then(|(p, c)| self.bullet_target(f, p).map(|p| (p, c))) {
                let num2 = self.hardness(p);
                if b.previous != Some(collider) {
                    let damage = self.damage_for_distance(f, THICKNESS + b.total) * mathf::clamp01(num);
                    if let Some(prev) = b.previous.filter(|&c| self.collider_exists(c)) {
                        let at = sub(b.origin, mul(b.direction, THICKNESS));
                        self.send_shot(prev, None, b.direction, at, damage, true);
                    }
                    self.send_shot(collider, Some(p), mul(b.direction, -1.0), b.origin, damage, false);
                }
                let impact = self.emitter(f).cartridge.ImpactForce;
                let force = mul(mul(mul(mul(mul(b.direction, impact), num2), THICKNESS), IMPACT_FORCE_MULTIPLIER), 4.0);
                let rb = self.physicals[p].rigidbody.clone();
                rb.add_force_at_position(&mut self.world, force, b.origin);
                let angle = self.bullet_random_angle(f);
                b = Bullet {
                    origin: add(b.origin, mul(b.direction, THICKNESS)),
                    direction: add(b.direction, angle),
                    speed: b.speed - num2,
                    iteration: b.iteration + 1,
                    total: b.total + THICKNESS,
                    previous: Some(collider),
                    splash: true,
                    ..b
                };
                continue;
            }
            if self.prefs.BulletCrackWhiz && self.bullet_crack_clips != 0 && self.can_play_cracks(f, b.origin, b.speed, b.total) {
                // CameraAudioSource.PlayOneShot(BulletCrackClips.PickRandom(), Random.Range(0.05f, 0.3f))
                self.pick_random(self.bullet_crack_clips);
                self.random.range(0.05, 0.3);
            }
            if let Some(prev) = b.previous.filter(|&c| self.collider_exists(c)) {
                let at = sub(b.origin, mul(b.direction, THICKNESS));
                let damage = self.damage_for_distance(f, THICKNESS + b.total) * mathf::clamp01(num);
                self.send_shot(prev, None, b.direction, at, damage, true);
            }
            let ray = self.raycast(V::new(b.origin[0], b.origin[1]), b.direction, max_range, layers, -0.1, 0.1);
            let target = ray.and_then(|h| self.bullet_target(f, self.physical_of_body(h.body)).map(|p| (h, p)));
            let Some((hit, p)) = target else {
                if traversal {
                    let dt = self.time.deltaTime;
                    let traversal = mathf::min(max_range, (d(dt) * 60.0f32 * max_range * mathf::sqrt(num)).f());
                    let damage = self.damage_for_distance(f, (d(b.total) + d(traversal) * 0.1f32).f()) * num;
                    b.splash = !self.test_for_water_hit(f, b.origin, b.direction, damage, b.splash);
                    let drop = ((1.0 - d(mathf::pow(0.999, dt * 60.0))) * BULLET_DROP_MULTIPLIER * 0.7f32).f();
                    b.direction = mathf::normalized2(mathf::v2_lerp(b.direction, [0.0, -1.0], drop));
                    // WaitForEndOfFrame, then StartCoroutine(BallisticIteration(..)).
                    b = Bullet { origin: add(b.origin, mul(b.direction, traversal)), total: (d(b.total) + d(traversal) * 0.1f32).f(), previous: None, ..b };
                    let slot = self.bullet_slot(b, None);
                    let owner = self.emitter_owner(f);
                    self.coroutine_seq += 1;
                    self.end_of_frame.push(Coroutine { seq: self.coroutine_seq, at: self.time.timeAsDouble, owner, step: Step::Bullet(slot) });
                }
                return;
            };
            let hardness = self.hardness(p);
            let impact = self.emitter(f).cartridge.ImpactForce;
            let value = mathf::max(impact * hardness, impact);
            let damage2 = self.damage_for_distance(f, b.total + hit.distance) * mathf::clamp01(num);
            let value = (d(IMPACT_FORCE_MULTIPLIER) * mathf::clamp(value, 0.0, 1.5) * 1.5f32).f();
            let rb = self.physicals[p].rigidbody.clone();
            rb.add_force_at_position(&mut self.world, mul(b.direction, value), b.origin);
            let (point, normal) = ([hit.point.x, hit.point.y], [hit.normal.x, hit.normal.y]);
            self.send_shot(hit.collider, Some(p), normal, point, damage2, false);
            let charge = self.emitter(f).Charge;
            let ph = &mut self.physicals[p];
            ph.charge = mathf::max(charge * ph.EnergyWireResistance, ph.Charge());
            let h = BulletHit { physical: p, hardness, damage: damage2, collider: hit.collider, point, normal, distance: hit.distance };
            let slot = self.bullet_slot(b, Some(h));
            let owner = self.emitter_owner(f);
            self.start_wait(owner, 0.01, Step::Bullet(slot));
            return;
        }
    }

    fn bullet_slot(&mut self, b: Bullet, h: Option<BulletHit>) -> usize {
        match self.bullets.iter().position(|s| s.is_none()) {
            Some(i) => {
                self.bullets[i] = Some((b, h));
                i
            }
            None => {
                self.bullets.push(Some((b, h)));
                self.bullets.len() - 1
            }
        }
    }

    /// A BallisticIteration past its WaitForEndOfFrame (the next iteration) or WaitForSeconds(0.01) (after a hit).
    pub(super) fn bullet_resume(&mut self, slot: usize) {
        let (b, h) = self.bullets[slot].take().expect("a resumed bullet");
        let Some(h) = h else {
            self.bullet_iterate(b);
            return;
        };
        let f = b.firearm;
        let mut b = b;
        b.splash = !self.test_for_water_hit(f, b.origin, b.direction, h.damage, b.splash);
        if self.should_ricochet(f, b.direction, &h) {
            // RicochetBullet: OnRicochet (FirearmBehaviour) instantiates Prefabs/Ricochet at the hit point.
            if self.emitter(f).ricochet_callback {
                self.spawn_resource("Prefabs/Ricochet", [h.point[0], h.point[1], 0.0]);
            }
            let dot = dot2(h.normal, b.direction);
            let k = -2.0 * dot;
            let reflect = [(d(k) * h.normal[0] + b.direction[0]).f(), (d(k) * h.normal[1] + b.direction[1]).f()];
            let angle = mul(self.bullet_random_angle(f), 0.1);
            let next = Bullet {
                origin: sub(h.point, mul(h.normal, 0.02)),
                direction: add(reflect, angle),
                iteration: b.iteration + 1,
                total: b.total + h.distance,
                previous: Some(h.collider),
                splash: true,
                can_explode: false,
                ..b
            };
            self.bullet_iterate(next);
        } else if !self.physicals[h.physical].BulletPenetration {
            self.remove_tracer(&b);
        } else {
            let next = Bullet {
                origin: add(h.point, mul(b.direction, THICKNESS)),
                speed: b.speed - h.hardness,
                iteration: b.iteration + 1,
                total: b.total + h.distance,
                previous: Some(h.collider),
                ..b
            };
            self.bullet_iterate(next);
        }
    }

    /// ShouldRicochet(incoming, hit, phys)
    fn should_ricochet(&mut self, f: Option<usize>, incoming: [f32; 2], h: &BulletHit) -> bool {
        let m = self.emitter(f).RicochetChanceMultiplier;
        let ph = &self.physicals[h.physical];
        // !hit.transform: the physical's GameObject is gone.
        if ph.destroyed {
            return false;
        }
        if !ph.BulletDeflectChanceOverride.is_nan() {
            let chance = m * ph.BulletDeflectChanceOverride;
            return chance > self.random.value();
        }
        let props = &self.properties[ph.Properties];
        let num = dot2(h.normal, incoming).abs();
        let num2 = mathf::clamp01(props.Softness + props.Brittleness);
        let num3 = (1.0 - (d(num2) * 150.0f32 + num)).f();
        if num3 > 0.05 {
            return num3 * m > self.random.value();
        }
        false
    }

    /// RandomiseCourse(dampening, diversion, ref direction, ref speed)
    fn randomise_course(&mut self, dampening: f32, diversion: f32, b: &mut Bullet) {
        let dt = self.time.deltaTime;
        let r = self.random.inside_unit_circle();
        let s = diversion * dt;
        b.direction = mathf::normalized2(add(b.direction, mul(r, s)));
        b.speed = mathf::lerp(b.speed, 0.0, mathf::lerp_factor_dt(dampening, dt));
    }

    /// CanPlayCracks(origin, speed, distance); Global.main.Paused is never set.
    fn can_play_cracks(&mut self, f: Option<usize>, origin: [f32; 2], speed: f32, distance: f32) -> bool {
        if self.camera_orthographic_size() > 8.0 || distance < 5.0 {
            return false;
        }
        let time = self.time.time;
        if time - self.emitter(f).lastCrackPlayTime < CRACK_COOLDOWN || speed < CRACK_SPEED_THRESHOLD {
            return false;
        }
        if mathf::sqr_magnitude(sub(origin, self.camera_position())) > 400.0 {
            return false;
        }
        if CRACK_CHANCE < self.random.value() {
            self.emitter_mut(f).lastCrackPlayTime = time;
            return false;
        }
        true
    }

    /// TestForWaterHit(origin, direction, damage, sendMessage)
    fn test_for_water_hit(&mut self, f: Option<usize>, origin: [f32; 2], direction: [f32; 2], damage: f32, send: bool) -> bool {
        let e = self.emitter(f);
        let (range, mask) = (e.MaxRange, e.WaterLayer);
        let Some(hit) = self.raycast(V::new(origin[0], origin[1]), direction, range, mask, f32::NEG_INFINITY, f32::INFINITY) else { return false };
        if !(hit.distance > 0.5) {
            return false;
        }
        // hit.transform: the attached Rigidbody2D's GameObject, else the collider's.
        let p = self.physical_of_body(hit.body);
        let layer = match p {
            Some(p) => self.physical_layer(p),
            None if self.node_rigidbodies.iter().any(|n| n.rigidbody.body == hit.body) => unimplemented!("TestForWaterHit on a Transform node's Rigidbody2D"),
            None => self.world.layers[&hit.game_object],
        };
        if layer != 4 {
            return false;
        }
        if send {
            self.send_shot(hit.collider, p, [hit.normal.x, hit.normal.y], [hit.point.x, hit.point.y], damage, false);
        }
        true
    }

    /// The physical whose Rigidbody2D owns a b2 body.
    fn physical_of_body(&self, body: crate::b2::world::BodyId) -> Option<usize> {
        self.physicals.iter().position(|p| !p.destroyed && p.rigidbody.body == body)
    }

    /// The layer of a physical's GameObject (that of its own colliders).
    fn physical_layer(&self, p: usize) -> i32 {
        let c = *self.physicals[p].all_colliders.first().unwrap_or_else(|| unimplemented!("the layer of a physical without its own collider"));
        let go = self.world.collider_game_object(c).expect("a collider's fixture");
        self.world.layers[&go]
    }

    /// SendMessage("Shot"/"ExitShot", shot) to a GameObject: `physical` when the target is a physical's object, else
    /// the collider's bare object. Every component on it with the method runs (disabled ones too), in order.
    fn send_shot(&mut self, collider: i32, physical: Option<usize>, normal: [f32; 2], point: [f32; 2], damage: f32, exit: bool) {
        let receivers = if exit { EXIT_SHOT_RECEIVERS } else { SHOT_RECEIVERS };
        let Some(p) = physical.or_else(|| self.physical_of_collider(collider)) else {
            if self.bare_colliders[&collider].receives(receivers) {
                unimplemented!("SendMessage Shot to the scripts of a collider without a PhysicalBehaviour");
            }
            return;
        };
        let classes: Vec<String> = self.with_component_instance(self.physicals[p].id, |_, inst, o| {
            inst.prefab.objects[o].components.iter().filter_map(|(_, c)| match c {
                crate::assets::db::Component::Script { class, .. } if receivers.contains(&class.as_str()) => Some(class.clone()),
                _ => None,
            }).collect()
        });
        for class in classes {
            match (class.as_str(), exit) {
                ("PhysicalBehaviour", false) => self.physical_shot(p, normal, point, damage),
                ("PhysicalBehaviour", true) => self.physical_exit_shot(p, normal, point, damage),
                ("DestroyableBehaviour", _) => {
                    let x = self.destroyables.iter().position(|x| x.PhysicalBehaviour == p).expect("the object's DestroyableBehaviour");
                    self.destroyable_shot(x, normal, damage);
                }
                ("ExplosiveBehaviour", _) => {
                    let e = self.explosives.iter().position(|e| e.physical == Some(p)).expect("the object's ExplosiveBehaviour");
                    self.explosive_shot(e);
                }
                ("DamagableMachineryBehaviour", _) => {
                    let m = self.machinery.iter().position(|m| m.PhysicalBehaviour == p).expect("the object's DamagableMachineryBehaviour");
                    self.machinery_shot(m, damage);
                }
                ("LimbBehaviour", _) | ("CirculationBehaviour", _) => {
                    // A limb's CirculationBehaviour shares its index.
                    let l = self.limbs.iter().position(|x| x.PhysicalBehaviour == p).expect("the object's LimbBehaviour");
                    match (class.as_str(), exit) {
                        ("LimbBehaviour", false) => self.limb_shot(l, normal, point, damage),
                        ("LimbBehaviour", true) => self.limb_exit_shot(l, normal, point, damage),
                        (_, false) => self.circulation_shot(l, normal, point, damage),
                        (_, true) => self.circulation_exit_shot(l, normal, point),
                    }
                }
                ("TyreDeflationBehaviour", _) => {
                    let t = self.tyres.iter().position(|t| t.Phys == p).expect("the object's TyreDeflationBehaviour");
                    self.tyre_shot(t, damage, point);
                }
                ("GlowtubeBehaviour", _) => {
                    let g = self.glowtubes.iter().position(|g| g.Phys == p).expect("the object's GlowtubeBehaviour");
                    self.glowtube_shot(g, damage);
                }
                (c, false) => unimplemented!("{c}.Shot"),
                (c, true) => unimplemented!("{c}.ExitShot"),
            }
        }
    }

    /// The physical of a RaycastHit2D.transform (the attached Rigidbody2D's GameObject, else the collider's); None for
    /// a collider without a PhysicalBehaviour there.
    pub(super) fn hit_transform_physical(&self, hit: &super::explosion::RayHit2D) -> Option<usize> {
        if let Some(p) = self.physical_of_body(hit.body) {
            return Some(p);
        }
        if self.node_rigidbodies.iter().any(|n| n.rigidbody.body == hit.body) {
            unimplemented!("RaycastHit2D.transform of a Transform node's Rigidbody2D");
        }
        None
    }

    /// The classes of a script's GameObject that take a message (`receivers`), in component order.
    fn receivers_of(&mut self, id: i32, receivers: &[&str]) -> Vec<String> {
        self.with_component_instance(id, |_, inst, o| {
            inst.prefab.objects[o].components.iter().filter_map(|(_, c)| match c {
                crate::assets::db::Component::Script { class, .. } if receivers.contains(&class.as_str()) => Some(class.clone()),
                _ => None,
            }).collect()
        })
    }

    /// LimbBehaviour.Shot
    fn limb_shot(&mut self, l: usize, normal: [f32; 2], point: [f32; 2], damage: f32) {
        use super::misc::damage_type::BULLET;
        if self.limbs[l].ImmuneToDamage {
            return;
        }
        let mut damage = damage * self.limbs[l].ShotDamageMultiplier;
        damage /= self.mass_strength_ratio(l);
        damage *= self.prefs.FragilityMultiplier;
        if self.limbs[l].IsAndroid {
            if damage < 40.0 {
                return;
            }
            damage *= 0.2;
        } else {
            self.limbs[l].shotHeat += 1.0;
        }
        if self.limbs[l].HasLungs && !self.limbs[l].IsAndroid && self.random.value() > 0.9 {
            self.limbs[l].LungsPunctured = true;
        }
        let flag = self.is_world_point_in_vital_part(l, point) && self.random.value() > 0.05;
        let num = if flag { 7.0 } else { 0.1 } * damage;
        let (person, p) = (self.limbs[l].Person, self.limbs[l].PhysicalBehaviour);
        if let Some(effect) = self.people[person].PoolableImpactEffect.filter(|_| !self.prefs.GorelessMode && self.prefs.ChunkyShotParticles)
            && (self.physicals[p].BurnProgress as f64) < 0.6
            && self.skins[l].acidProgress < 0.7
            && num > self.people[person].ImpactEffectShotDamageThreshold
            && self.random.value() > 0.8
        {
            // transform.right = shot.normal: presentation.
            let effect = effect.expect("LimbBehaviour.Shot: a recording names the PoolableImpactEffect but holds no game data");
            self.request_pooled_effect(effect, "LimbBehaviour.Shot");
        }
        for i in 0..self.limbs[l].ConnectedLimbs.len() {
            let c = self.limbs[l].ConnectedLimbs[i];
            if !self.physicals[self.limbs[c].PhysicalBehaviour].destroyed {
                self.limbs[c].Numbness += 0.5;
            }
        }
        if self.limb_is_considered_alive(l) {
            let m = self.limbs[l].ShotDamageMultiplier;
            let a = self.people[person].AdrenalineLevel;
            self.people[person].AdrenalineLevel = (d(a) + d(self.random.value()) * m).f();
            let c = self.people[person].Consciousness;
            self.people[person].Consciousness = (d(c) - d(self.random.range(0.02, 0.1)) * m).f();
            if !self.limbs[l].IsAndroid && !self.limbs[l].IsZombie {
                let s = self.people[person].ShockLevel;
                self.people[person].ShockLevel = (d(s) + d(num) * self.random.value() * 0.0025f32).f();
                self.person_wince(person, 300.0);
                self.limbs[l].Numbness = 1.0;
                if d(self.random.value()) * self.limbs[l].Vitality > d(0.5) && self.random.value() > 0.6 && !self.limb_is_paralysed(l) {
                    let pain = self.random.value() * 2.0;
                    self.add_pain(person, pain);
                }
                if d(num) > d(2.0) * self.random.value() {
                    let w = self.people[person].DesiredWalkingDirection;
                    let delta = d(self.random.value()) * 3.0f32;
                    let same_side = (normal[0] > 0.0) == (self.people[person].localScale[0] > 0.0);
                    self.people[person].DesiredWalkingDirection = if same_side { d(w) - delta } else { d(w) + delta }.f();
                }
                // Person.SendMessage("Shot", shot)
                for class in self.receivers_of(self.people[person].id, SHOT_RECEIVERS) {
                    match class.as_str() {
                        "PersonBehaviour" => self.person_shot(person),
                        c => unimplemented!("{c}.Shot on a person root"),
                    }
                }
            }
            if self.limbs[l].HasBrain && !self.limbs[l].IsAndroid && (!self.limbs[l].IsZombie || !(self.random.value() > 0.2)) {
                let num2 = if flag { 0.1 } else { 0.8 };
                if self.random.value() > num2 {
                    self.limbs[l].Health = 0.0;
                }
                if self.random.value() > num2 {
                    self.people[person].Consciousness = 0.0;
                }
            }
        }
        // Shot.CanCrush is true for the emitter's shots.
        if self.prefs.LimbCrushing && damage > 149.0 && mathf::clamp(((d(damage) - 149.0f32) * 0.005f32).f(), 0.3, 0.8) > self.random.value() {
            if self.prefs.StopAnimationOnDamage {
                self.people[person].OverridePoseIndex = -1;
            }
            self.limb_crush_next_frame(l);
        } else if self.limbs[l].shotHeat > 5.0 && self.random.value() > 0.35 {
            if self.limbs[l].HasJoint && self.random.value() > 0.8 {
                self.limb_slice(l);
            } else if self.prefs.LimbCrushing {
                if self.prefs.StopAnimationOnDamage {
                    self.people[person].OverridePoseIndex = -1;
                }
                self.limb_crush_next_frame(l);
            }
        }
        if self.limbs[l].IsZombie || self.random.value() < 0.01 {
            self.limb_damage(l, num * 0.01);
        } else {
            if !self.limbs[l].IsAndroid {
                self.circulations[l].InternalBleedingIntensity += num;
                if flag {
                    self.circulations[l].InternalBleedingIntensity += 5.0;
                }
            }
            let damage = mathf::min(self.limbs[l].InitialHealth / 2.0, num * 2.0);
            self.limb_damage(l, damage);
        }
        let b = damage * 0.1;
        self.skin_add_damage_point(l, BULLET, point, mathf::max(50.0, b));
    }

    /// StartCoroutine(LimbBehaviour.CrushNextFrame()): the dismemberment damage point, then WaitForEndOfFrame.
    fn limb_crush_next_frame(&mut self, l: usize) {
        let p = self.limbs[l].PhysicalBehaviour;
        let at = self.physicals[p].transform.position;
        self.skin_add_damage_point(l, super::misc::damage_type::DISMEMBERMENT, [at[0], at[1]], 15.0);
        self.coroutine_seq += 1;
        let owner = self.limbs[l].id;
        self.end_of_frame.push(Coroutine { seq: self.coroutine_seq, at: self.time.timeAsDouble, owner, step: Step::LimbCrushEndOfFrame(l) });
    }

    /// LimbBehaviour.Slice
    pub(super) fn limb_slice(&mut self, l: usize) {
        if self.limbs[l].ImmuneToDamage {
            return;
        }
        let person = self.limbs[l].Person;
        if self.prefs.StopAnimationOnDamage && self.limb_is_considered_alive(l) {
            self.people[person].OverridePoseIndex = -1;
        }
        if self.limb_is_considered_alive(l) {
            self.people[person].AdrenalineLevel += 1.0;
        }
        if self.limbs[l].HasJoint {
            self.limbs[l].RegenerationSpeed = 0.0;
            self.limbs[l].Health = 0.0;
            let h = self.limbs[l].Joint.expect("MissingReferenceException: HasJoint without a Joint");
            let anchor = self.hinges[h].anchor;
            let at = self.transform_point3(self.limbs[l].PhysicalBehaviour, [anchor[0], anchor[1], 0.0]);
            self.act_on_impact(l, 15.0, at);
            self.limbs[l].BreakingThreshold = 0.0;
        }
    }

    /// LimbBehaviour.ExitShot
    fn limb_exit_shot(&mut self, l: usize, normal: [f32; 2], point: [f32; 2], damage: f32) {
        use super::misc::damage_type::BULLET;
        if self.limbs[l].ImmuneToDamage {
            return;
        }
        let mut damage = damage / self.mass_strength_ratio(l);
        damage *= self.prefs.FragilityMultiplier;
        self.skin_add_damage_point(l, BULLET, point, mathf::max(60.0, damage * 0.4));
        let (person, p) = (self.limbs[l].Person, self.limbs[l].PhysicalBehaviour);
        if let Some(effect) = self.people[person].PoolableImpactEffect.filter(|_| !self.prefs.GorelessMode && self.prefs.ChunkyShotParticles)
            && (self.physicals[p].BurnProgress as f64) < 0.6
            && self.skins[l].acidProgress < 0.7
            && damage > self.people[person].ImpactEffectShotDamageThreshold
            && self.random.value() > 0.6
        {
            let effect = effect.expect("LimbBehaviour.ExitShot: a recording names the PoolableImpactEffect but holds no game data");
            self.request_pooled_effect(effect, "LimbBehaviour.ExitShot");
        }
        if self.limbs[l].HasLungs && !self.limbs[l].IsAndroid && self.random.value() > 0.9 {
            self.limbs[l].LungsPunctured = true;
        }
        if self.prefs.StopAnimationOnDamage && self.limb_is_considered_alive(l) {
            self.people[person].OverridePoseIndex = -1;
        }
        if !self.prefs.GorelessMode && !self.limbs[l].KillShotParticlesEmitted && self.limbs[l].Health <= mathf::EPSILON && self.limbs[l].KillShotParticles && self.random.value() > 0.7 {
            // KillShotParticles.Play() and PlayClipOnce: presentation and audio, the draw stays.
            self.pick_random(self.people[person].DismembermentClips);
            self.limbs[l].KillShotParticlesEmitted = true;
        }
        if self.limbs[l].IsAndroid || self.amount_of_blood(l) < 0.05 {
            return;
        }
        let fallback = self.original_blood_color(l);
        let colour = self.get_computed_color(l, fallback);
        // The loop bound Random.Range(1, 4) is drawn again before each pass.
        let mut i = 0;
        while i < self.random.range_int(1, 4) {
            let r = self.random.inside_unit_circle();
            let direction = [normal[0] + r[0] * 0.4, normal[1] + r[1] * 0.4];
            let hit = self.raycast(V::new(point[0], point[1]), direction, 3.0, super::explosion::DEFAULT_RAYCAST_LAYERS, f32::NEG_INFINITY, f32::INFINITY);
            if let Some(hit) = hit {
                // raycastHit2D.transform.gameObject.SendMessage("Decal", new DecalInstruction(BloodDecal, point, colour))
                match self.hit_transform_physical(&hit) {
                    Some(other) => {
                        let ty = self.decal_descriptor(&self.limbs[l].BloodDecal.clone());
                        self.physical_decal(other, ty, [hit.point.x, hit.point.y], 1.0, colour);
                    }
                    None if self.bare_colliders[&hit.collider].receives(&["DecalControllerBehaviour", "DecalRenderer", "DecalRendererController"]) => {
                        unimplemented!("SendMessage Decal to the scripts of a collider without a PhysicalBehaviour")
                    }
                    None => {}
                }
            }
            i += 1;
        }
    }

    /// CirculationBehaviour.Shot
    fn circulation_shot(&mut self, l: usize, normal: [f32; 2], point: [f32; 2], damage: f32) {
        if self.circulations[l].ImmuneToDamage || (self.limbs[l].IsAndroid && damage < 50.0) {
            return;
        }
        let damage = damage * self.limbs[l].ShotDamageMultiplier;
        if damage <= mathf::EPSILON {
            return;
        }
        let softness = self.properties[self.physicals[self.limbs[l].PhysicalBehaviour].Properties].Softness;
        if !self.limbs[l].IsZombie && d(self.random.value()) < d(softness) + 0.001f32 {
            let flag = self.is_world_point_in_artery_rect(l, point);
            let rate = self.circulations[l].BleedingRate;
            self.circulations[l].BleedingRate = (d(rate) + d(mathf::max(0.5, damage / 3.5)) * if flag { 2.0f32 } else { 1.0 }).f();
            self.create_bleeding_particle(l, point, normal, if flag { 1.0 } else { 0.0 });
        }
        if !self.limbs[l].IsAndroid && !self.limbs[l].IsZombie && self.random.value() > 0.2 && self.is_world_point_in_vital_part(l, point) {
            self.circulations[l].IsPump = false;
        }
        self.circulations[l].GunshotWoundCount = self.circulations[l].GunshotWoundCount.wrapping_add(1);
    }

    /// CirculationBehaviour.ExitShot (shot.damage = Limb.ShotDamageMultiplier)
    fn circulation_exit_shot(&mut self, l: usize, normal: [f32; 2], point: [f32; 2]) {
        if self.circulations[l].ImmuneToDamage || self.limbs[l].IsZombie || self.limbs[l].IsAndroid {
            return;
        }
        let damage = self.limbs[l].ShotDamageMultiplier;
        if damage <= mathf::EPSILON {
            return;
        }
        let flag = self.is_world_point_in_artery_rect(l, point);
        let rate = self.circulations[l].BleedingRate;
        self.circulations[l].BleedingRate = (d(rate) + d(mathf::max(0.5, damage / 10.0)) * if flag { 4.0f32 } else { 1.0 }).f();
        self.create_bleeding_particle(l, point, normal, if flag { 1.0 } else { 0.0 });
        if self.random.value() > 0.2 && self.is_world_point_in_vital_part(l, point) {
            self.circulations[l].IsPump = false;
        }
    }

    /// PhysicalBehaviour.Shot
    fn physical_shot(&mut self, p: usize, normal: [f32; 2], point: [f32; 2], damage: f32) {
        // PlayClipOnce(OverrideShotSounds.PickRandom()): audio, the draw stays.
        let n = self.physicals[p].OverrideShotSounds;
        if n != 0 {
            self.pick_random(n);
        }
        if self.properties[self.physicals[p].Properties].ShotImpact.is_some() {
            let size = mathf::clamp(mathf::pow(damage, 0.6) / 6.0, 0.1, 3.0) / 2.0;
            self.create_impact_effect(p, point, normal, size);
        }
    }

    /// PhysicalBehaviour.ExitShot
    fn physical_exit_shot(&mut self, p: usize, normal: [f32; 2], point: [f32; 2], damage: f32) {
        if self.properties[self.physicals[p].Properties].ShotImpact.is_some() {
            let size = mathf::clamp(mathf::pow(damage, 0.6) / 3.0, 0.1, 4.0);
            self.create_impact_effect(p, point, normal, size);
        }
    }

    /// DestroyableBehaviour.Shot
    fn destroyable_shot(&mut self, x: usize, normal: [f32; 2], damage: f32) {
        let s = &self.destroyables[x];
        if s.ReactToBullets && !(damage < s.MinimumShotDamage) && !(s.ShotBreakChance < self.random.value()) {
            match self.destroyable_explosive(x) {
                Some(e) if self.explosives[e].ShotExplodeChance > mathf::EPSILON => self.explosive_force_immediate(e),
                _ => {
                    let owner = self.destroyables[x].id;
                    self.start_wait(owner, 0.05, Step::DestroyableShotBreak(x, normal));
                }
            }
        }
    }

    /// DestroyableBehaviour.Shot's WaitAFrame after its 0.05 s.
    pub(super) fn destroyable_shot_break(&mut self, x: usize, normal: [f32; 2]) {
        let m = 0.0 - self.destroyables[x].MinimumShotDamage;
        self.destroyable_break(x, [normal[0] * m / 20.0, normal[1] * m / 20.0]);
    }

    /// ExplosiveBehaviour.Shot (triggersExplosiveOverride is always true for bullets).
    fn explosive_shot(&mut self, e: usize) {
        if self.random.value() < self.explosives[e].ShotExplodeChance {
            let delay = self.random.value() * 0.2;
            self.explosive_start_explode(e, delay, true);
        }
    }

    /// DamagableMachineryBehaviour.Shot
    fn machinery_shot(&mut self, m: usize, damage: f32) {
        let mc = &mut self.machinery[m];
        if !mc.Indestructible && !mc.Destroyed {
            mc.Health = (d(mc.Health) - d(damage) * mc.FirearmDamageMultiplier).f();
            self.machinery_act_on_damage(m);
        }
    }

    /// LavaBehaviour.IsPointInLava: the lavas whose objects are active (their FixedUpdate is listed).
    fn is_point_in_lava(&mut self, point: [f32; 2]) -> bool {
        for lv in 0..self.lavas.len() {
            let active = self.behaviours.list(super::Phase::Fixed).iter().any(|e| matches!(e.1, Behaviour::Lava(x) if x == lv));
            if active && self.lava_is_point_inside(lv, point) {
                return true;
            }
        }
        false
    }
}
