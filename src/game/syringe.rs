//! SyringeBehaviour and its 15 subclasses (the subclasses only name their liquid), SyringeExplosionBehaviour, and the
//! liquids the syringes carry that translated/ lacks (Acid, Instant death, Life, Ultra strength, Mending, Painkiller
//! and the Pink syringe's Mass, Explosion, Distortion, Circulation and Tissue deconstruction).
//! Awake and OnWillRenderObject only touch the SpriteRenderer's property block (_LiquidColour, _Direction): rendering.
//! Start's AddButtons builds context menu buttons: UI.
#![allow(non_snake_case)]

use super::circulation::{liquid, LIQUIDS};
use super::container::{BloodContainer, Owner};
use super::coroutine::Yield;
use super::explosion::DEFAULT_RAYCAST_LAYERS;
use super::flask::BLOOD_CONTAINERS;
use super::{d, mathf, script, Game};
use crate::b2::math::Vec2 as V;

/// SyringeBehaviour and its subclasses.
pub const CLASSES: &[&str] = &[
    "SyringeBehaviour", "AcidSyringe", "AdrenalineSyringe", "BoneEatingPoisonSyringe", "CoagulationSyringe", "DeathSyringe", "EmptySyringe",
    "FreezeSyringe", "KnockoutSyringe", "LifeSyringe", "MendingSyringe", "PainKillerSyringe", "PinkSyringe", "UltraStrengthSyringe",
    "WaterBreathingSyringe", "ZombieSyringe",
];

pub fn is_syringe(class: &str) -> bool {
    CLASSES.contains(&class)
}

/// GetLiquidID() of each subclass (SyringeBehaviour itself is abstract; EmptySyringe and PinkSyringe return null).
pub fn liquid_id(class: &str) -> Option<&'static str> {
    Some(match class {
        "AcidSyringe" => "ACID",
        "AdrenalineSyringe" => "ADRENALINE",
        "BoneEatingPoisonSyringe" => "BONE EATING POISON",
        "CoagulationSyringe" => "COAGULATION SERUM",
        "DeathSyringe" => "INSTANT DEATH POISON",
        "FreezeSyringe" => "FREEZE POISON",
        "KnockoutSyringe" => "KNOCKOUT POISON",
        "LifeSyringe" => "LIFE SERUM",
        "MendingSyringe" => "MENDING SERUM",
        "PainKillerSyringe" => "PAIN KILLER",
        "UltraStrengthSyringe" => "ULTRA STRENGTH SERUM",
        "WaterBreathingSyringe" => "WATER BREATHING SERUM",
        "ZombieSyringe" => "REANIMATION AGENT",
        "EmptySyringe" | "PinkSyringe" => return None,
        c => unreachable!("{c} is not a syringe"),
    })
}

/// PressureDirection
const PUSH: i32 = 0;
const PULL: i32 = 1;

/// Limits.y
const CAPACITY: f32 = 1.4;

script! {
    Syringe {
        Finite: bool = false,
        CanToggleInfinite: bool = true,
        PressureMode: i32 = PUSH,
        NewlySpawned: bool = true,
        TransferRate: f32 = 0.01,
    }
    base { base: BloodContainer }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// GetComponent<PhysicalBehaviour>()
        Phys: usize = 0,
        /// GetLiquidID() as a `LIQUIDS` index (spawnLiquid once Start has run).
        liquid: Option<usize> = None,
        /// PinkSyringe (overrides Start and FixedUpdate).
        pink: bool = false,
        /// PinkSyringe.Fingerprint (empty: Start draws a new one) and fingerprintReadPos.
        Fingerprint: Vec<f32> = Vec::new(),
        fingerprintReadPos: usize = 0,
        /// PinkSyringe.liquids
        liquids: Vec<(usize, f32)> = Vec::new(),
        /// pushTargets, a HashSet<BloodContainer>: .NET enumerates its entries in slot order, and an Add reuses the most
        /// recently freed slot first.
        pushTargets: Vec<Option<Owner>> = Vec::new(),
        pushFree: Vec<usize> = Vec::new(),
    }
}

script! {
    SyringeExplosion {
        Radius: f32 = 0.0,
        Amount: f32 = 0.1,
    }
    links {
        id: i32 = 0,
        /// transform.position (nothing moves it).
        Position: [f32; 3] = [0.0, 0.0, 0.0],
        /// Liquids = the syringe's LiquidDistribution.Keys, a live view of it.
        Syringe: Option<usize> = None,
    }
}

/// PinkSyringe.Start's liquids, each added when the next fingerprint value is above the threshold.
const PINK_LIQUIDS: &[(&str, f32)] = &[
    ("INERT PINK LIQUID", 0.05), ("VESTIBULAR POISON", 0.6), ("MUSCLE POISON", 0.6), ("NUMBING POISON", 0.6), ("MIRRORISING AGENT", 0.85),
    ("TRANSPARENCY AGENT", 0.85), ("MASS AGENT", 0.95), ("EXPLOSION POISON", 0.995), ("CRUSHING POISON", 0.995), ("BONE EATING POISON", 0.9),
    ("LIFE SERUM", 0.9), ("ULTRA STRENGTH SERUM", 0.6), ("DURABILITY SERUM", 0.6), ("REGENERATION SERUM", 0.8), ("DISTORTION POISON", 0.9),
    ("FREEZE POISON", 0.95), ("INSTANT DEATH POISON", 0.99), ("CIRCULATION POISON", 0.4), ("ENHANCING SERUM", 0.4), ("COMBUSTION AGENT", 0.95),
    ("TISSUE DECONSTRUCTION AGENT", 0.95),
];

/// The liquid classes written by hand here.
pub fn is_syringe_liquid(class: &str) -> bool {
    matches!(
        class,
        "AcidLiquid" | "InstantDeathPoisonLiquid" | "LifeSerumLiquid" | "UltraStrengthSerumLiquid" | "MendingSerum" | "PainKillerLiquid" | "MassManipulationPoison"
            | "ExplosionPoison" | "SizeManipulationPoison" | "CirculationPoison" | "TissueDeconstructionAgent"
    )
}

impl Game {
    /// Unity's `(bool)container`: the component is not destroyed.
    fn container_alive(&self, o: Owner) -> bool {
        !self.destroyed_components.contains(&self.container_id(o))
    }

    /// The BloodContainer component's instance id.
    pub(super) fn container_id(&self, o: Owner) -> i32 {
        match o {
            Owner::Circulation(l) => self.limbs[l].id,
            Owner::Flask(f) => self.flasks[f].id,
            Owner::Syringe(s) => self.syringes[s].id,
            Owner::Valve(v) => self.valves[v].id,
            Owner::BloodTank(t) => self.blood_tanks[t].id,
            Owner::LiquidOutlet(l) => self.liquid_outlets[l].id,
        }
    }

    /// physical.TryGetComponent<BloodContainer>(): a limb's CirculationBehaviour, a flask or a syringe.
    pub(super) fn physical_container(&mut self, q: usize) -> Option<Owner> {
        if let Some(l) = self.limbs.iter().position(|x| x.PhysicalBehaviour == q) {
            return Some(Owner::Circulation(l));
        }
        if let Some(f) = self.flasks.iter().position(|x| x.Phys == Some(q)) {
            return Some(Owner::Flask(f));
        }
        if let Some(s) = self.syringes.iter().position(|x| x.Phys == q) {
            return Some(Owner::Syringe(s));
        }
        // BloodTankBehaviour (blood_tank.rs).
        if let Some(t) = self.blood_tanks.iter().position(|x| x.physical == q) {
            return Some(Owner::BloodTank(t));
        }
        // LiquidOutletBehaviour (liquid_outlet.rs).
        if let Some(l) = self.liquid_outlets.iter().position(|x| x.physical == q) {
            return Some(Owner::LiquidOutlet(l));
        }
        // ValveBehaviour (valve.rs).
        if let Some(v) = self.valves.iter().position(|x| x.physical == q) {
            return Some(Owner::Valve(v));
        }
        let class = self.with_component_instance(self.physicals[q].id, |_, inst, o| {
            inst.prefab.objects[o].components.iter().find_map(|(_, c)| match c {
                crate::assets::db::Component::Script { class, .. } if BLOOD_CONTAINERS.contains(&class.as_str()) => Some(class.clone()),
                _ => None,
            })
        });
        if let Some(class) = class {
            unimplemented!("SyringeBehaviour pushing into a {class}");
        }
        None
    }

    /// pushTargets.Add
    fn push_target_add(&mut self, s: usize, o: Owner) {
        let x = &mut self.syringes[s];
        if x.pushTargets.contains(&Some(o)) {
            return;
        }
        match x.pushFree.pop() {
            Some(i) => x.pushTargets[i] = Some(o),
            None => x.pushTargets.push(Some(o)),
        }
    }

    /// pushTargets.Remove
    fn push_target_remove(&mut self, s: usize, o: Owner) {
        let x = &mut self.syringes[s];
        if let Some(i) = x.pushTargets.iter().position(|t| *t == Some(o)) {
            x.pushTargets[i] = None;
            x.pushFree.push(i);
        }
    }

    /// SyringeBehaviour.Start and PinkSyringe.Start.
    pub fn syringe_start(&mut self, s: usize) {
        let o = Owner::Syringe(s);
        if self.syringes[s].pink {
            self.pink_syringe_start(s);
            return;
        }
        if let Some(spawn) = self.syringes[s].liquid {
            if self.syringes[s].NewlySpawned {
                self.add_liquid(o, spawn, CAPACITY);
            }
        }
        let p = self.syringes[s].Phys;
        for pen in self.physicals[p].penetrations.clone() {
            let victim = self.penetration_arena[pen].Victim.unwrap_or_else(|| unimplemented!("SyringeBehaviour.Start: a Penetration without a Victim (NullReferenceException)"));
            if let Some(target) = self.physical_container(victim) {
                self.push_target_add(s, target);
            }
        }
        self.syringes[s].NewlySpawned = false;
    }

    /// PinkSyringe.GetFingerprintValue
    fn fingerprint_value(&mut self, s: usize) -> f32 {
        let x = &mut self.syringes[s];
        let result = x.Fingerprint[x.fingerprintReadPos];
        x.fingerprintReadPos += 1;
        if x.fingerprintReadPos >= x.Fingerprint.len() {
            x.fingerprintReadPos = 0;
        }
        result
    }

    /// PinkSyringe.Start (it does not call SyringeBehaviour.Start: no pushTargets from the penetrations).
    fn pink_syringe_start(&mut self, s: usize) {
        let o = Owner::Syringe(s);
        if self.syringes[s].Fingerprint.is_empty() {
            // CreateNewFingerprint
            self.syringes[s].fingerprintReadPos = 0;
            let mut fingerprint = Vec::with_capacity(64);
            for _ in 0..64 {
                fingerprint.push(self.random.range(-1.0, 1.0));
            }
            self.syringes[s].Fingerprint = fingerprint;
        }
        let v = self.fingerprint_value(s);
        let item = mathf::map_range(-1.0, 1.0, 0.05, 0.4, v);
        let mut num = 0;
        let mut total = 0.0f32;
        while total < CAPACITY - mathf::EPSILON && num < 5000 {
            for &(name, threshold) in PINK_LIQUIDS {
                if self.fingerprint_value(s) > threshold {
                    // addToList
                    let l = liquid(name);
                    total += item;
                    self.syringes[s].liquids.push((l, item));
                    if self.syringes[s].NewlySpawned {
                        self.add_liquid(o, l, item);
                    }
                }
            }
            num += 1;
        }
        self.syringes[s].NewlySpawned = false;
    }

    /// SyringeBehaviour.FixedUpdate, after PinkSyringe.FixedUpdate's top-up.
    pub fn syringe_fixed_update(&mut self, s: usize) {
        let o = Owner::Syringe(s);
        if self.syringes[s].pink && !self.syringes[s].Finite {
            let num = CAPACITY - self.bc(o).TotalLiquidAmount;
            let num2 = self.syringes[s].TransferRate * num;
            for (l, amount) in self.syringes[s].liquids.clone() {
                self.add_liquid(o, l, amount * num2);
            }
        }
        let finite = self.syringes[s].Finite;
        let spawn = self.syringes[s].liquid;
        if !finite {
            if let Some(spawn) = spawn {
                let amount = CAPACITY - self.get_amount(o, spawn);
                self.add_liquid(o, spawn, amount);
            }
        }
        let rate = self.syringes[s].TransferRate;
        for other in self.syringes[s].pushTargets.iter().flatten().copied().collect::<Vec<_>>() {
            if !self.container_alive(other) {
                continue;
            }
            self.average_pressure(self.time.fixedDeltaTime, o, other);
            if !finite || self.bc(o).MeasuredPressure > self.bc(other).MeasuredPressure {
                // push()
                if !(self.bc(o).TotalLiquidAmount <= self.container_limits(o)[0]) && !(self.bc(other).TotalLiquidAmount >= self.upper_limit(other)) {
                    match (spawn, other) {
                        (Some(spawn), Owner::Circulation(_)) => {
                            if self.get_amount(other, spawn) < 0.5 {
                                self.transfer_to(o, rate, other);
                            }
                        }
                        _ => {
                            self.transfer_to(o, rate, other);
                        }
                    }
                }
            } else {
                // pull()
                if !(self.bc(other).TotalLiquidAmount <= self.container_limits(other)[0]) && !(self.bc(o).TotalLiquidAmount >= self.upper_limit(o)) {
                    self.transfer_to(other, rate, o);
                }
            }
        }
    }

    /// BloodWireBehaviour.AveragePressure (a closed ValveBehaviour does not allow pressure transfer).
    fn average_pressure(&mut self, rate: f32, from: Owner, to: Owner) {
        if !(self.container_allows_pressure(from) && self.container_allows_pressure(to)) {
            return;
        }
        let (a, b) = (self.bc(from).MeasuredPressure, self.bc(to).MeasuredPressure);
        let mean = ((d(a) + b) * 0.5f32).f();
        self.bc_mut(from).MeasuredPressure = mathf::lerp(a, mean, rate);
        self.bc_mut(to).MeasuredPressure = mathf::lerp(b, mean, rate);
    }

    /// SyringeBehaviour.Update is BloodContainer.Update (the Push and Pull pressure is `container_pressure`).
    pub fn syringe_update(&mut self, s: usize) {
        self.blood_container_update(Owner::Syringe(s));
    }

    /// SyringeBehaviour.Use
    pub fn syringe_use(&mut self, s: usize, channel: i32) {
        let x = &mut self.syringes[s];
        match channel {
            1 => x.PressureMode = PUSH,
            2 => {
                if x.Finite {
                    x.PressureMode = PULL;
                }
            }
            _ => x.PressureMode = if x.PressureMode == PUSH { PULL } else { PUSH },
        }
    }

    /// SyringeBehaviour.Lodged
    pub fn syringe_lodged(&mut self, s: usize, victim: usize) {
        if !self.physicals[victim].destroyed {
            if let Some(target) = self.physical_container(victim) {
                self.push_target_add(s, target);
            }
        }
    }

    /// SyringeBehaviour.Dislodged
    pub fn syringe_dislodged(&mut self, s: usize, pen: usize) {
        if let Some(victim) = self.penetration_arena[pen].Victim.filter(|&v| !self.physicals[v].destroyed) {
            if let Some(target) = self.physical_container(victim) {
                self.push_target_remove(s, target);
            }
        }
    }

    /// SyringeBehaviour.Shot: StartCoroutine(WaitAFrame()), which breaks it after WaitForSeconds(0.05).
    pub fn syringe_shot(&mut self, s: usize) {
        let id = self.syringes[s].id;
        self.start_wait(id, 0.05, super::destroyable::Step::SyringeBreak(s));
    }

    /// SyringeBehaviour.BreakSyringe. The explosion's particle colour (GetComputedColor) is rendering.
    pub fn break_syringe(&mut self, s: usize) {
        let o = Owner::Syringe(s);
        let total = self.bc(o).TotalLiquidAmount;
        let count = self.bc(o).LiquidDistribution.len();
        if total > mathf::EPSILON && count > 0 {
            let position = self.physicals[self.syringes[s].Phys].transform.position;
            let before = self.syringe_explosions.len();
            self.spawn_resource("Prefabs/SyringeExplosion", position);
            assert_eq!(self.syringe_explosions.len(), before + 1, "the SyringeExplosion prefab has one SyringeExplosionBehaviour");
            let e = &mut self.syringe_explosions[before];
            e.Position = position;
            e.Syringe = Some(s);
            e.Amount = total / count as f32;
            let id = self.syringes[s].id;
            self.destroy_object(id);
        }
    }

    /// SyringeExplosionBehaviour.Start: the particles' colour and Play are rendering.
    pub fn syringe_explosion_start(&mut self, e: usize) {
        let x = &self.syringe_explosions[e];
        let (radius, amount, position) = (x.Radius, x.Amount, x.Position);
        let syringe = x.Syringe.expect("SyringeExplosionBehaviour.Liquids is set by BreakSyringe");
        let mut hits = self.overlap_circle(V::new(position[0], position[1]), radius, DEFAULT_RAYCAST_LAYERS);
        // Mathf.Min(results.Length, OverlapCircleNonAlloc(...)): the query's first 64 colliders (180bf4620).
        hits.truncate(64);
        let num = hits.len();
        let amount = (d(amount) / num as f32 * 3.0f32).f();
        for (physical, collider) in hits {
            // results[i].GetComponent<LimbBehaviour>(): a LimbBehaviour on the collider's own GameObject.
            let Some(l) = physical.and_then(|p| self.limbs.iter().position(|x| x.PhysicalBehaviour == p)) else { continue };
            if self.world.collider_game_object(collider) != Some(self.limb_object(l).0) || self.limbs[l].IsAndroid {
                continue;
            }
            for liquid in self.syringes[syringe].base.LiquidDistribution.keys() {
                self.add_liquid(Owner::Circulation(l), liquid, amount);
            }
        }
    }

    // ---- Liquid's virtual members for the liquids above ----

    /// Liquid.OnEnterContainer: empty in every one.
    pub(super) fn syringe_liquid_on_enter_container(&mut self, _liquid: usize, _o: Owner) {}

    /// Liquid.OnExitContainer: empty in every one.
    pub(super) fn syringe_liquid_on_exit_container(&mut self, _liquid: usize, _o: Owner) {}

    /// Liquid.OnEnterLimb
    pub(super) fn syringe_liquid_on_enter_limb(&mut self, liquid: usize, l: usize) {
        match LIQUIDS[liquid].1 {
            // GorseBlood.OnEnterLimb, inherited.
            "AcidLiquid" => {
                if self.limbs[l].SpeciesIdentity != "Gorse" {
                    self.acid_routine(liquid, l);
                }
            }
            // `_ = limb.SpeciesIdentity == "Android"`
            "InstantDeathPoisonLiquid" => {}
            "LifeSerumLiquid" => self.life_serum_on_enter_limb(liquid, l),
            "UltraStrengthSerumLiquid" => self.ultra_strength_on_enter_limb(l),
            "MendingSerum" => self.mending_serum_on_enter_limb(l),
            "PainKillerLiquid" => self.kill_pain(liquid, l),
            _ if self.limbs[l].SpeciesIdentity == "Android" => {}
            "MassManipulationPoison" => {
                let rb = self.physicals[self.limbs[l].PhysicalBehaviour].rigidbody.clone();
                if rb.detached.is_some() {
                    unimplemented!("MassManipulationPoison: Rigidbody2D.mass on a body Cleanup destroyed");
                }
                let mass = rb.mass(&self.world) * self.random.range_int(-1, 1) as f32;
                self.body_set_mass(rb.body, mass);
                let scale = rb.gravity_scale(&self.world) * self.random.range_int(-1, 1) as f32;
                rb.set_gravity_scale(&mut self.world, scale);
            }
            "ExplosionPoison" => {
                let position = self.physicals[self.limbs[l].PhysicalBehaviour].transform.position;
                let under_water = self.is_point_under_water([position[0], position[1]]);
                let big = self.random.value() > 0.9;
                let dismember = self.random.value();
                self.create_explosion_with_water(
                    under_water,
                    super::explosion::ExplosionParameters {
                        FragmentationRayCount: 32,
                        Position: position,
                        Range: 9.0,
                        FragmentForce: 25.0,
                        CreateParticlesAndSound: true,
                        EffectSize: if big { super::explosion::EffectSize::Large } else { super::explosion::EffectSize::Medium },
                        DismemberChance: dismember,
                        BallisticShrapnelCount: 0,
                    },
                );
            }
            "SizeManipulationPoison" => {
                let p = self.limbs[l].PhysicalBehaviour;
                let x = self.random.range(0.952381, 1.05) * self.physicals[p].transform.localScale[0];
                let y = self.random.range(0.952381, 1.05) * self.physicals[p].transform.localScale[1];
                // new Vector3(x, y): z is 0.
                self.physical_set_local_scale(p, [x, y, 0.0]);
            }
            "CirculationPoison" => {
                let r = self.random.range_int(-1, 1).abs() as f32;
                self.circulations[l].BleedingRate *= r;
                let r = self.random.range_int(-1, 1) as f32;
                self.circulations[l].BloodLossRateMultiplier += r * 0.2;
                let r = self.random.range_int(-1, 1) as f32;
                self.circulations[l].IsPump ^= r > 0.9;
                let r = self.random.range_int(-1, 1) as f32;
                self.circulations[l].WasInitiallyPumping ^= r > 0.7;
                let r = self.random.range_int(0, 8) as f32;
                self.circulations[l].InternalBleedingIntensity *= r;
                let r = (self.random.range_int(-1, 1) * 8) as f32;
                self.limbs[l].Vitality *= r;
                let r = (self.random.range_int(-1, 1) * 8) as f32;
                self.limbs[l].ImpactPainMultiplier *= r;
                let r = self.random.range_int(-1, 1) as f32;
                let acid = self.skins[l].acidProgress + r * 0.05;
                self.skins[l].set_acid(acid);
                let r = self.random.range_int(-1, 1) as f32;
                self.skins[l].intensityMultiplier += r * 0.5;
                let r = self.random.range_int(-1, 1) as f32;
                let rotten = self.skins[l].rottenProgress + r * 0.05;
                self.skins[l].set_rotten(rotten);
                // (double)Random.value > 0.5
                let r = self.random.value();
                self.limbs[l].HasLungs ^= d(r).0 > 0.5;
                let r = self.random.value();
                self.limbs[l].LungsPunctured ^= d(r).0 > 0.5;
            }
            "TissueDeconstructionAgent" => {
                if self.random.value() > 0.8 {
                    self.limb_slice(l);
                }
            }
            c => unimplemented!("{c}.OnEnterLimb"),
        }
    }

    /// Liquid.OnUpdate
    pub(super) fn syringe_liquid_on_update(&mut self, liquid: usize, o: Owner) {
        match LIQUIDS[liquid].1 {
            // Blood.OnUpdate, inherited from GorseBlood.
            "AcidLiquid" => self.blood_on_update(liquid, o),
            "InstantDeathPoisonLiquid" => {
                self.temporary_body_liquid_on_update(liquid, o, 0.8, true);
                if let Owner::Circulation(l) = o {
                    self.limbs[l].Health = 0.0;
                    self.circulations[l].IsPump = false;
                }
            }
            "LifeSerumLiquid" | "UltraStrengthSerumLiquid" => self.temporary_body_liquid_on_update(liquid, o, 0.8, true),
            // RemovalChancePerSecond => base.RemovalChancePerSecond * 0.8f
            "MendingSerum" => self.temporary_body_liquid_on_update(liquid, o, 0.8f32 * 0.8f32, true),
            "PainKillerLiquid" => {
                self.temporary_body_liquid_on_update(liquid, o, 0.8, true);
                if let Owner::Circulation(l) = o {
                    self.kill_pain(liquid, l);
                }
            }
            // ShouldCallOnEnterEveryUpdate is false: only the removal.
            "MassManipulationPoison" | "ExplosionPoison" | "SizeManipulationPoison" | "CirculationPoison" | "TissueDeconstructionAgent" => {
                self.temporary_body_liquid_on_update(liquid, o, 0.8, false)
            }
            c => unimplemented!("{c}.OnUpdate"),
        }
    }

    /// TemporaryBodyLiquid.OnUpdate
    fn temporary_body_liquid_on_update(&mut self, liquid: usize, o: Owner, rcps: f32, call_on_enter_limb: bool) {
        let Owner::Circulation(l) = o else { return };
        if (self.random.value() as f64) < (rcps as f64) * (rcps as f64) {
            let amount = self.get_amount(o, liquid);
            self.remove_liquid(o, liquid, amount);
        }
        if call_on_enter_limb {
            self.liquid_on_enter_limb(liquid, l);
        }
    }

    /// LimbBehaviour.HealBone
    pub fn limb_heal_bone(&mut self, l: usize) {
        self.limbs[l].Broken = false;
        if let Some(h) = self.limbs[l].Joint.filter(|&h| !self.hinges[h].destroyed && self.hinges[h].useLimits) {
            self.limbs[l].JointStress = 0.0;
            let [min, max] = self.limbs[l].originalJointLimits;
            self.hinges[h].set_limits(&mut self.world, min, max);
        }
    }

    /// The limb's object: its GameObject id and its CirculationBehaviour's instance id (StartCoroutine's owner).
    pub(super) fn limb_object(&mut self, l: usize) -> (i32, i32) {
        self.with_component_instance(self.limbs[l].id, |_, inst, o| {
            let k = inst.prefab.objects[o].components.iter().position(|(_, c)| matches!(c, crate::assets::db::Component::Script { class, .. } if class == "CirculationBehaviour"));
            (inst.objects[o].game_object, inst.objects[o].components[k.expect("a limb's CirculationBehaviour")])
        })
    }

    fn circulation_id(&mut self, l: usize) -> i32 {
        self.limb_object(l).1
    }

    /// GorseBlood.AcidRoutine: the coroutine on the limb's CirculationBehaviour.
    pub(super) fn acid_routine(&mut self, liquid: usize, l: usize) {
        let owner = self.circulation_id(l);
        let (mut resumed, mut concentration) = (false, 0.0f32);
        self.start_coroutine(owner, owner, Box::new(move |g| {
            if resumed && concentration <= mathf::EPSILON {
                return Yield::Done;
            }
            if !(g.skins[l].acidProgress < 0.99) {
                g.limbs[l].Health = 0.0;
                g.circulations[l].IsPump = false;
                g.limbs[l].BreakingThreshold = 0.0;
                return Yield::Done;
            }
            concentration = mathf::clamp01((d(g.get_amount(Owner::Circulation(l), liquid)) * 500.0f32).f());
            if g.limbs[l].SpeciesIdentity == "Android" {
                concentration = (d(concentration) * 0.1f32).f();
            }
            let acid = (d(g.skins[l].acidProgress) + d(0.002f32) * concentration).f();
            g.skins[l].set_acid(acid);
            let intensity = (d(150.0f32) * concentration * g.skins[l].acidProgress).f();
            g.limb_wince(l, intensity);
            if g.skins[l].acidProgress > 0.5 {
                if g.nodes[l].IsConnectedToRoot {
                    let pain = (d(0.1f32) * concentration * g.skins[l].acidProgress).f();
                    let person = g.limbs[l].Person;
                    g.add_pain(person, pain);
                }
                let bleeding = (d(g.circulations[l].InternalBleedingIntensity) + d(0.005f32) * concentration).f();
                g.circulations[l].InternalBleedingIntensity = bleeding;
                if g.skins[l].acidProgress > 0.9 {
                    g.break_bone(l);
                }
            }
            resumed = true;
            Yield::WaitForSeconds(0.03)
        }));
    }

    /// LifeSyringe.LifeSerumLiquid.OnEnterLimb
    fn life_serum_on_enter_limb(&mut self, liquid: usize, l: usize) {
        if self.limbs[l].SpeciesIdentity == "Android" {
            return;
        }
        let person = self.limbs[l].Person;
        if self.limbs[l].HasBrain {
            let p = &mut self.people[person];
            (p.Braindead, p.BrainDamaged, p.BrainDamagedTime, p.SeizureTime) = (false, false, 0.0, 0.0);
        }
        // GeneralHealing
        self.limb_heal_bone(l);
        self.limbs[l].Health = self.limbs[l].InitialHealth;
        self.limbs[l].Numbness = 0.0;
        self.heal_bleeding(l);
        self.circulations[l].IsPump = self.circulations[l].WasInitiallyPumping;
        self.circulations[l].BloodFlow = 1.0;
        let blood = self.blood_type(l);
        let amount = mathf::max(0.0, 1.0 - self.get_amount(Owner::Circulation(l), blood));
        self.add_liquid(Owner::Circulation(l), blood, amount);
        self.limbs[l].BruiseCount = 0;
        self.circulations[l].GunshotWoundCount = 0;
        self.circulations[l].StabWoundCount = 0;
        if self.limbs[l].RoughClassification == 0 {
            let p = &mut self.people[person];
            (p.Consciousness, p.ShockLevel, p.PainLevel, p.OxygenLevel, p.AdrenalineLevel) = (1.0, 0.0, 0.0, 1.0, 1.0);
        }
        self.heal_burn(liquid, l);
        self.heal_wounds(liquid, l);
        self.heal_rotten_flesh(liquid, l);
        self.heal_acid_flesh(liquid, l);
        self.limbs[l].LungsPunctured = false;
    }

    /// LifeSerumLiquid.HealBurn
    fn heal_burn(&mut self, liquid: usize, l: usize) {
        let (owner, p) = (self.circulation_id(l), self.limbs[l].PhysicalBehaviour);
        let mut resumed = false;
        self.start_coroutine(owner, owner, Box::new(move |g| {
            if !resumed {
                g.physicals[p].Temperature = g.limbs[l].BodyTemperature;
            } else if g.get_amount(Owner::Circulation(l), liquid) <= mathf::EPSILON {
                return Yield::Done;
            }
            if g.physicals[p].BurnProgress > 0.1 {
                g.physicals[p].BurnProgress = (d(g.physicals[p].BurnProgress) - d(g.time.deltaTime) * 0.16f32).f();
                resumed = true;
                return Yield::WaitForEndOfFrame;
            }
            g.physicals[p].BurnProgress = 0.0;
            Yield::Done
        }));
    }

    /// LifeSerumLiquid.HealAcidFlesh
    fn heal_acid_flesh(&mut self, liquid: usize, l: usize) {
        let owner = self.circulation_id(l);
        let mut resumed = false;
        self.start_coroutine(owner, owner, Box::new(move |g| {
            if resumed && g.get_amount(Owner::Circulation(l), liquid) <= mathf::EPSILON {
                return Yield::Done;
            }
            if g.skins[l].acidProgress > 0.1 {
                let v = (d(g.skins[l].acidProgress) - d(g.time.deltaTime) * 0.8f32).f();
                g.skins[l].set_acid(v);
                resumed = true;
                return Yield::WaitForEndOfFrame;
            }
            g.skins[l].set_acid(0.0);
            Yield::Done
        }));
    }

    /// LifeSerumLiquid.HealRottenFlesh
    fn heal_rotten_flesh(&mut self, liquid: usize, l: usize) {
        let owner = self.circulation_id(l);
        let mut resumed = false;
        self.start_coroutine(owner, owner, Box::new(move |g| {
            if resumed && g.get_amount(Owner::Circulation(l), liquid) <= mathf::EPSILON {
                return Yield::Done;
            }
            if g.skins[l].rottenProgress > 0.1 {
                let v = (d(g.skins[l].rottenProgress) - d(g.time.deltaTime) * 0.8f32).f();
                g.skins[l].set_rotten(v);
                resumed = true;
                return Yield::WaitForEndOfFrame;
            }
            g.skins[l].set_rotten(0.0);
            Yield::Done
        }));
    }

    /// LifeSerumLiquid.HealWounds: every damage point's intensity (z) halves per second until all are gone; Sync is rendering.
    fn heal_wounds(&mut self, liquid: usize, l: usize) {
        let owner = self.circulation_id(l);
        let factor = mathf::pow(0.5, self.time.deltaTime);
        let mut resumed = false;
        self.start_coroutine(owner, owner, Box::new(move |g| {
            if resumed && g.get_amount(Owner::Circulation(l), liquid) <= mathf::EPSILON {
                return Yield::Done;
            }
            if !g.skins[l].damagePoints.iter().any(|p| p[2] > 0.0) {
                return Yield::Done;
            }
            for p in g.skins[l].damagePoints.iter_mut() {
                p[2] *= factor;
            }
            resumed = true;
            Yield::WaitForEndOfFrame
        }));
    }

    /// UltraStrengthSyringe.UltraStrengthSerumLiquid.OnEnterLimb
    fn ultra_strength_on_enter_limb(&mut self, l: usize) {
        if self.limbs[l].SpeciesIdentity == "Android" {
            return;
        }
        let limb = &mut self.limbs[l];
        limb.Vitality *= 0.1;
        limb.RegenerationSpeed += 8.0;
        limb.ImpactPainMultiplier *= 0.1;
        limb.InitialHealth += 350.0;
        limb.Health = limb.InitialHealth;
        limb.BreakingThreshold += 20.0;
        limb.BaseStrength = mathf::min(15.0, limb.BaseStrength + 5.0);
        limb.ShotDamageMultiplier *= 0.05;
        let c = &mut self.circulations[l];
        c.BloodLossRateMultiplier *= 0.5;
        c.BloodRegenerationPerSecond += 5.0;
        c.BleedingRate *= 0.2;
        c.InternalBleedingIntensity *= 0.2;
        self.limb_wince(l, 5.0);
        let p = self.limbs[l].PhysicalBehaviour;
        let value = self.random.value();
        let ph = &mut self.physicals[p];
        ph.BulletDeflectChanceOverride = if ph.BulletDeflectChanceOverride.is_nan() { value } else { ph.BulletDeflectChanceOverride + value };
        if self.limbs[l].Color[1] > 0.95 && self.random.value() < 0.1 {
            for (c, k) in self.limbs[l].Color.iter_mut().zip([0.95, 0.92, 1.0, 1.0]) {
                *c *= k;
            }
        }
    }

    /// MendingSyringe.MendingSerum.OnEnterLimb
    fn mending_serum_on_enter_limb(&mut self, l: usize) {
        if self.limbs[l].SpeciesIdentity == "Android" {
            return;
        }
        if self.limbs[l].HasBrain {
            let person = self.limbs[l].Person;
            self.people[person].Braindead = false;
            if self.random.value() > 0.995 {
                self.people[person].BrainDamaged = false;
            }
        }
        self.limbs[l].Health = self.limbs[l].InitialHealth;
        self.circulations[l].BleedingRate *= 0.8;
        self.circulations[l].InternalBleedingIntensity *= 0.8;
        let rotten = self.skins[l].rottenProgress * 0.8;
        self.skins[l].set_rotten(rotten);
        self.limbs[l].LungsPunctured = false;
        self.limb_heal_bone(l);
    }

    /// PainKillerSyringe.PainKillerLiquid.KillPain
    fn kill_pain(&mut self, liquid: usize, l: usize) {
        if self.limbs[l].SpeciesIdentity == "Android" {
            return;
        }
        self.limbs[l].Numbness += 0.1;
        if !self.nodes[l].IsConnectedToRoot {
            return;
        }
        let person = self.limbs[l].Person;
        let p = &mut self.people[person];
        p.PainLevel *= 0.1;
        p.ShockLevel *= 0.1;
        p.SeizureTime *= 0.5;
        let amount = self.get_amount(Owner::Circulation(l), liquid);
        if amount > 0.4 {
            self.influence_motor_speed(l, 0.0, 0.5);
            let intensity = self.random.value() * 300.0;
            self.person_wince(person, intensity);
            if amount > 0.5 {
                let acid = self.skins[l].acidProgress + 0.05;
                self.skins[l].set_acid(acid);
                let damage = (d(self.limbs[l].Health) * 0.5f32 + 0.1f32).f();
                self.limb_damage(l, damage);
            }
        }
    }
}
