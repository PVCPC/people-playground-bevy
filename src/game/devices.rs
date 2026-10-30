//! GeneratorBehaviour, SingleFloodlightBehaviour, SpikeGrenadeBehaviour, HeatingElementBehaviour,
//! TemperatureRayGunBehaviour and BulbBehaviour. Audio, particles, sprite and light toggles, property blocks and the context menu
//! button are presentation; the activation, charge, temperature, coroutine, spawn and raycast state stays.
#![allow(non_snake_case)]

use super::destroyable::Step;
use super::explosion::{EffectSize, ExplosionParameters};
use super::{d, mathf, script, Game};
use crate::assets::db::ObjRef;
use crate::b2::math::Vec2 as V;
use crate::unity::transform::{from_to_rotation, global_position, global_rotation, Trs};

script! {
    Generator {
        Activated: bool = false,
        TargetCharge: f32 = 100.0,
        TargetEnergyReachSpeed: f32 = 1.0,
        Vibrate: bool = false,
        VibrateForce: f32 = 0.0,
        VibrateRotationSpeed: f32 = 0.0,
        WarmupProgress: f32 = 0.0,
        InstantChargeChange: bool = false,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// physicalBehaviour
        Phys: usize = 0,
        /// Behaviour.enabled
        enabled: bool = true,
    }
}

script! {
    Floodlight {
        Activated: bool = false,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// Behaviour.enabled
        enabled: bool = true,
        /// ToToggle: the clone's object index.
        ToToggle: Option<usize> = None,
        /// SwitchSounds and OnSwitchSound lengths (PickRandom draws from them).
        SwitchSounds: usize = 0,
        OnSwitchSound: usize = 0,
    }
}

script! {
    SpikeGrenade {
        SpikeCount: i32 = 12,
        DirectRange: f32 = 4.0,
        SpikeForce: f32 = 10.0,
        Delay: f32 = 2.0,
        Ignited: bool = false,
    }
    links {
        /// The component's instance id (its coroutine's owner).
        id: i32 = 0,
        /// Behaviour.enabled
        enabled: bool = true,
        /// GetComponent<PhysicalBehaviour>()
        Phys: usize = 0,
        SpikePrefab: Option<ObjRef> = None,
        ExplosionVFX: Option<ObjRef> = None,
        /// CollisionLayers (LayerMask bits)
        CollisionLayers: u32 = 0,
    }
}

script! {
    HeatingElement {
        AllowedTemperatureRange: [f32; 2] = [0.0, 1000.0],
        HeatTargetSpeed: f32 = 0.1,
        ActiveCooling: bool = false,
        TargetTemperature: f32 = 1.0,
        Activated: bool = false,
    }
    links {
        /// PhysicalBehaviour
        Phys: usize = 0,
        /// DamagableMachineryBehaviour (the `Game::machinery` index)
        Machinery: usize = 0,
    }
}

script! {
    RayGun {
        TemperatureDelta: f32 = 5.0,
    }
    links {
        useHeat: f32 = 0.0,
        /// The component's instance id.
        id: i32 = 0,
        /// PhysicalBehaviour
        Phys: usize = 0,
        /// Behaviour.enabled
        enabled: bool = true,
        /// Layers (LayerMask bits)
        Layers: u32 = 0,
        /// The local TRS of the LineRenderer's object and of each parent below the gun's own object.
        Barrel: Vec<Trs> = Vec::new(),
    }
}

script! {
    Bulb {
        ChargeInfluence: f32 = 1.0,
        MaximumTemperature: f32 = 300.0,
        TargetFilamentTemperature: f32 = 2800.0,
        MinimumImpactForce: f32 = 0.0,
        FilamentHealth: f32 = 1.0,
        FilamentTemp: f32 = 0.0,
        Broken: bool = false,
        Activated: bool = false,
    }
    links {
        /// The component's instance id (its coroutine's owner).
        id: i32 = 0,
        /// PhysicalBehaviour
        Phys: usize = 0,
        /// OtherPart: the clone's object index.
        OtherPart: Option<usize> = None,
        /// Explosion's ExplosionSoundBehviour (GetComponentInChildren): its instance id, the Near count when Play
        /// draws (enabled, with an AudioSource) and its DistantSound.
        sound: Option<(i32, Option<usize>, i64)> = None,
        /// ColliderToDisable's instance id (a collider on the bulb's own object).
        collider: Option<i32> = None,
        /// private flicker
        flicker: bool = false,
        /// The component's index on its GameObject, and whether the PhysicalBehaviour precedes it.
        index: usize = 0,
        after_physical: bool = true,
    }
}

/// Vector3.normalized: (float)Math.Sqrt(x * x + y * y + z * z) in double, then a division.
fn normalized3(v: [f32; 3]) -> [f32; 3] {
    let num = (d(v[0]) * v[0] + d(v[1]) * v[1] + d(v[2]) * v[2]).0.sqrt() as f32;
    if num > 1e-5 { [v[0] / num, v[1] / num, v[2] / num] } else { [0.0; 3] }
}

impl Game {
    // ---- GeneratorBehaviour ----

    /// GeneratorBehaviour.Use (UpdateActivation is audio and particles)
    pub fn generator_use(&mut self, i: usize) {
        let s = &mut self.generators[i];
        if s.enabled {
            s.WarmupProgress = 0.0;
            s.Activated = !s.Activated;
        }
    }

    /// GeneratorBehaviour.OnDisable (the audio and particles stop)
    pub fn generator_on_disable(&mut self, i: usize) {
        self.generators[i].Activated = false;
    }

    /// GeneratorBehaviour.Update (the audio volume is audio)
    pub fn generator_update(&mut self, i: usize) {
        let s = &mut self.generators[i];
        s.WarmupProgress = mathf::clamp01(s.WarmupProgress);
        let (p, target, warmup) = (s.Phys, s.TargetCharge, s.WarmupProgress);
        if s.Activated {
            let charge = self.physicals[p].Charge();
            let new = if s.InstantChargeChange { mathf::max(charge, target) } else { mathf::max(charge, mathf::lerp(0.0, target, warmup)) };
            self.physicals[p].charge = new;
            let smooth = self.time.smoothDeltaTime;
            let s = &mut self.generators[i];
            s.WarmupProgress = (d(s.WarmupProgress) + d(smooth) * 60.0f32 * s.TargetEnergyReachSpeed).f();
        } else if s.InstantChargeChange {
            self.physicals[p].charge = 0.0;
        }
    }

    /// GeneratorBehaviour.FixedUpdate
    pub fn generator_fixed_update(&mut self, i: usize) {
        let s = &self.generators[i];
        if !s.Activated {
            return;
        }
        let (p, target) = (s.Phys, s.TargetCharge);
        let num = target / 4.0;
        if self.physicals[p].Temperature < num {
            let t = self.physicals[p].Temperature;
            self.physicals[p].Temperature = mathf::lerp(t, num, 0.02 / self.heat_capacity(p));
        }
        let s = &self.generators[i];
        if s.Vibrate {
            // ForceVector: new Vector2(Mathf.Cos(f), Mathf.Sin(f)) * VibrateForce, f = Time.time * VibrateRotationSpeed.
            let f = self.time.time * s.VibrateRotationSpeed;
            let v = [((f as f64).cos() as f32) * s.VibrateForce, mathf::sin(f) * s.VibrateForce];
            let force = [v[0] * s.WarmupProgress, v[1] * s.WarmupProgress];
            let rb = self.physicals[p].rigidbody.clone();
            rb.add_relative_force(&mut self.world, force);
        }
    }

    // ---- SingleFloodlightBehaviour ----

    /// SingleFloodlightBehaviour.Start
    pub fn floodlight_start(&mut self, i: usize) {
        self.floodlight_update_activation(i);
    }

    /// SingleFloodlightBehaviour.Use: PlayOneShot is audio, the draw stays.
    pub fn floodlight_use(&mut self, i: usize) {
        if self.floodlights[i].enabled {
            self.floodlights[i].Activated = !self.floodlights[i].Activated;
            self.floodlight_update_activation(i);
            self.pick_random(self.floodlights[i].SwitchSounds);
        }
    }

    /// SingleFloodlightBehaviour.UpdateActivation: the sounds are audio, the draw and ToToggle.SetActive stay.
    fn floodlight_update_activation(&mut self, i: usize) {
        let s = &self.floodlights[i];
        let (active, to_toggle, id, on_switch) = (s.Activated, s.ToToggle, s.id, s.OnSwitchSound);
        if active {
            self.pick_random(on_switch);
        }
        let to_toggle = to_toggle.unwrap_or_else(|| unimplemented!("SingleFloodlightBehaviour.ToToggle is null (NullReferenceException)"));
        self.with_component_instance(id, |g, inst, _| g.set_active_scripts(inst, to_toggle, active));
    }

    /// SingleFloodlightBehaviour.OnDisable on the clone `inst` (the audio and the highlights' property blocks are
    /// presentation): ToToggle.SetActive(false).
    pub fn floodlight_on_disable(&mut self, i: usize, inst: &mut super::spawn::Instance) {
        self.floodlights[i].Activated = false;
        let to_toggle = self.floodlights[i].ToToggle.unwrap_or_else(|| unimplemented!("SingleFloodlightBehaviour.ToToggle is null (NullReferenceException)"));
        self.set_active_scripts(inst, to_toggle, false);
    }

    // ---- SpikeGrenadeBehaviour ----

    /// SpikeGrenadeBehaviour.Start (the Light is a sprite)
    pub fn spike_grenade_start(&mut self, i: usize) {
        if self.spike_grenades[i].Ignited {
            self.spike_grenade_start_routine(i);
        }
    }

    /// SpikeGrenadeBehaviour.Use
    pub fn spike_grenade_use(&mut self, i: usize) {
        if self.spike_grenades[i].enabled {
            self.spike_grenade_ignite(i);
        }
    }

    /// SpikeGrenadeBehaviour.Ignite
    pub fn spike_grenade_ignite(&mut self, i: usize) {
        if !self.spike_grenades[i].Ignited {
            self.spike_grenades[i].Ignited = true;
            self.spike_grenade_start_routine(i);
        }
    }

    /// StartCoroutine(IgniteRoutine()) up to its WaitForSeconds(Delay): the Light and PlayClipOnce (no draws) are
    /// presentation and audio.
    fn spike_grenade_start_routine(&mut self, i: usize) {
        let s = &self.spike_grenades[i];
        let (id, delay) = (s.id, s.Delay);
        self.start_wait(id, delay, Step::SpikeIgnite(i));
    }

    /// SpikeGrenadeBehaviour.OnDisable: StopCoroutine("IgniteRoutine") does not stop a coroutine started from an
    /// IEnumerator (SetActive(false) does, in `set_active_scripts`); the rest is audio and the Light.
    pub fn spike_grenade_on_disable(&mut self, i: usize) {
        self.spike_grenades[i].Ignited = false;
    }

    /// IgniteRoutine after its WaitForSeconds.
    pub fn spike_grenade_resume(&mut self, i: usize) {
        if !self.spike_grenades[i].Ignited {
            return;
        }
        let s = self.spike_grenades[i].clone();
        let position = self.physicals[s.Phys].transform.position;
        let origin = V::new(position[0], position[1]);
        let spike = s.SpikePrefab.unwrap_or_else(|| unimplemented!("SpikeGrenadeBehaviour: Instantiate(null) (ArgumentException)"));
        for _ in 0..s.SpikeCount {
            let vector = mathf::normalized2(self.random.inside_unit_circle());
            // Physics2D.Raycast: the result is unused, its transform sync is not.
            self.raycast(origin, vector, s.DirectRange, s.CollisionLayers, f32::NEG_INFINITY, f32::INFINITY);
            let mut db = self.assets.take().expect("SpikeGrenadeBehaviour without the game's data");
            let spawned = db.prefab(spike).and_then(|prefab| {
                let (inst, linked) = self.spawn(&mut db, &prefab, position, [0.0, 0.0, 0.0, 1.0], None)?;
                Ok((prefab, inst, linked))
            });
            self.assets = Some(db);
            let (prefab, mut inst, linked) = spawned.unwrap_or_else(|e| panic!("SpikeGrenadeBehaviour: Instantiate: {e}"));
            // GetOrAddComponent<AudioSourceTimeScaleBehaviour>() finds the prefab's own.
            if !prefab.objects[0].components.iter().any(|(_, c)| matches!(c, crate::assets::db::Component::Script { class, .. } if class == "AudioSourceTimeScaleBehaviour")) {
                unimplemented!("SpikeGrenadeBehaviour: AddComponent<AudioSourceTimeScaleBehaviour> to the spike");
            }
            let q = *linked.physicals.get(&0).unwrap_or_else(|| unimplemented!("SpikeGrenadeBehaviour: the spike has no PhysicalBehaviour (NullReferenceException)"));
            // obj.transform.right = vector: rotation = Quaternion.FromToRotation(Vector3.right, vector).
            inst.set_world_rotation(0, from_to_rotation([1.0, 0.0, 0.0], [vector[0], vector[1], 0.0]));
            self.refresh_transforms(&inst, &linked);
            let k = self.random.range(0.5, 1.3) * s.SpikeForce;
            let rb = self.physicals[q].rigidbody.clone();
            rb.add_force_impulse(&mut self.world, [vector[0] * k, vector[1] * k]);
            self.physicals[q].Temperature = self.physicals[s.Phys].Temperature;
            self.unsynced.push(inst);
        }
        // ExplosionCreator.CreateExplosionWithWater(IsPointUnderWater(position), new ExplosionParameters(1u, position, 0f, 0f, createFx: false))
        let under_water = self.is_point_under_water([position[0], position[1]]);
        let ex = ExplosionParameters {
            FragmentationRayCount: 1,
            Position: position,
            Range: 0.0,
            FragmentForce: 0.0,
            CreateParticlesAndSound: false,
            EffectSize: EffectSize::Medium,
            DismemberChance: 0.0,
            BallisticShrapnelCount: 0,
        };
        self.create_explosion_with_water(under_water, ex);
        let vfx = s.ExplosionVFX.unwrap_or_else(|| unimplemented!("SpikeGrenadeBehaviour: Instantiate(null) (ArgumentException)"));
        self.spawn_prefab(vfx, position);
        self.destroy_object(s.id);
    }

    // ---- HeatingElementBehaviour ----

    /// HeatingElementBehaviour.Use (the ActivationLight is a sprite)
    pub fn heating_element_use(&mut self, i: usize) {
        if !self.machinery[self.heating_elements[i].Machinery].Destroyed {
            let s = &mut self.heating_elements[i];
            s.Activated = !s.Activated;
        }
    }

    /// HeatingElementBehaviour.FixedUpdate
    pub fn heating_element_fixed_update(&mut self, i: usize) {
        let s = &self.heating_elements[i];
        let p = s.Phys;
        let flag = s.ActiveCooling ^ (self.physicals[p].Temperature <= s.TargetTemperature);
        if s.Activated && !self.machinery[s.Machinery].Destroyed && flag {
            let (target, speed) = (s.TargetTemperature, s.HeatTargetSpeed);
            let t = self.physicals[p].Temperature;
            self.physicals[p].Temperature = mathf::lerp(t, target, speed);
        }
    }

    // ---- TemperatureRayGunBehaviour ----

    /// TemperatureRayGunBehaviour.UseContinuous: ContinuousActivationBehaviour.AssertState() only checks, and the
    /// clip and AudioSource.Play are audio. (`continuous.rs` broadcasts it.)
    pub fn ray_gun_use_continuous(&mut self, i: usize) {
        if self.ray_guns[i].enabled {
            self.ray_guns[i].useHeat = 0.1;
        }
    }

    /// TemperatureRayGunBehaviour.OnDisable (the audio, line and particles are presentation)
    pub fn ray_gun_on_disable(&mut self, i: usize) {
        self.ray_guns[i].useHeat = 0.0;
    }

    /// TemperatureRayGunBehaviour.FixedUpdate (the line renderer, impact and muzzle are presentation)
    pub fn ray_gun_fixed_update(&mut self, i: usize) {
        let s = &self.ray_guns[i];
        let p = s.Phys;
        if s.useHeat > 0.0 {
            let (delta, layers) = (s.TemperatureDelta, s.Layers);
            let at = self.physicals[p].transform.position;
            self.camera_shake(0.05, [at[0], at[1]]);
            let num = (d(delta) * mathf::max(1.0, self.physicals[p].Charge())).f();
            // LineRenderer.transform.position and .right
            let (position, rotation) = self.ray_gun_barrel_pose(i);
            let right = mathf::rotate(rotation, [1.0, 0.0, 0.0]);
            let scale = self.physicals[p].transform.localScale[0];
            let normalized = normalized3([right[0] * scale, right[1] * scale, right[2] * scale]);
            let hit = self.raycast(V::new(position[0], position[1]), [normalized[0], normalized[1]], f32::MAX, layers, f32::NEG_INFINITY, f32::INFINITY);
            if let Some(hit) = hit {
                self.camera_shake(0.05, [hit.point.x, hit.point.y]);
                // PhysicalObjectsInWorldByTransform.TryGetValue(raycastHit2D.transform)
                if let Some(v) = self.physical_of_collider(hit.collider).filter(|&v| self.physicals[v].in_world)
                    && self.physicals[v].SimulateTemperature
                {
                    self.physicals[v].Temperature += num;
                    // TryGetComponent<BulbBehaviour>: FilamentTemp += num * 4f
                    if let Some(b) = self.bulbs.iter().position(|b| b.Phys == v) {
                        self.bulbs[b].FilamentTemp += num * 4.0;
                    }
                }
            }
        }
        let dt = self.time.deltaTime;
        self.ray_guns[i].useHeat -= dt;
    }

    // ---- BulbBehaviour ----

    /// BulbBehaviour.Start (`seed` is only stored)
    pub fn bulb_start(&mut self, i: usize) {
        self.random.range_int(0, 1000);
        if self.bulbs[i].Broken {
            self.bulb_break(i);
        }
    }

    /// BulbBehaviour.Update (UpdateColor is rendering)
    pub fn bulb_update(&mut self, i: usize) {
        let s = &mut self.bulbs[i];
        s.FilamentTemp = mathf::max(0.0, s.FilamentTemp);
        let (p, dt) = (s.Phys, self.time.deltaTime);
        if self.physicals[p].Temperature > self.bulbs[i].MaximumTemperature {
            self.bulb_break(i);
        }
        let s = &mut self.bulbs[i];
        let lerp_factor = mathf::lerp_factor_dt(0.1, dt);
        let temperature = self.physicals[p].Temperature;
        let b = (d(temperature) * 0.6f32 + d(s.FilamentTemp) * 0.4f32).f();
        s.FilamentTemp = mathf::lerp(s.FilamentTemp, b, lerp_factor);
        self.physicals[p].Temperature = mathf::lerp(temperature, b, lerp_factor);
        if s.Activated && !s.Broken && !s.flicker {
            let lerp_factor = mathf::lerp_factor_dt(0.1, dt);
            s.FilamentTemp = mathf::max(s.FilamentTemp, s.TargetFilamentTemperature * 0.4);
            let charge = self.physicals[p].Charge();
            let target = (d(s.TargetFilamentTemperature) * (0.5f32 + d(mathf::sqrt(charge * s.ChargeInfluence)))).f();
            s.FilamentTemp = mathf::max(s.FilamentTemp, mathf::lerp(s.FilamentTemp, target, lerp_factor));
        } else {
            let lerp_factor = mathf::lerp_factor_dt(0.4, dt);
            s.FilamentTemp = mathf::lerp(s.FilamentTemp, 0.0, lerp_factor);
        }
        s.flicker = false;
    }

    /// BulbBehaviour.FixedUpdate: `|=` evaluates the draw whatever flicker is.
    pub fn bulb_fixed_update(&mut self, i: usize) {
        let flicker = self.random.value() <= 0.005;
        self.bulbs[i].flicker |= flicker;
    }

    /// BulbBehaviour.Use (Toggle)
    pub fn bulb_use(&mut self, i: usize) {
        let s = &mut self.bulbs[i];
        s.Activated = !s.Activated;
    }

    /// BulbBehaviour.OnFragmentHit
    pub fn bulb_on_fragment_hit(&mut self, i: usize) {
        if !self.bulbs[i].Broken {
            self.bulb_break(i);
        }
    }

    /// BulbBehaviour.Shot: StartCoroutine(WaitAFrame()), which breaks it after WaitForSeconds(0.05).
    pub fn bulb_shot(&mut self, i: usize) {
        if !self.bulbs[i].Broken {
            let id = self.bulbs[i].id;
            self.start_wait(id, 0.05, Step::BulbBreak(i));
        }
    }

    /// BulbBehaviour.OnCollisionEnter2D
    pub(super) fn bulb_on_collision_enter(&mut self, i: usize, c: &crate::b2::contacts2d::ScriptCollision) {
        let s = &self.bulbs[i];
        if s.Broken {
            return;
        }
        let first = c.contacts.first().unwrap_or_else(|| unimplemented!("BulbBehaviour.OnCollisionEnter2D: collision.contacts[0] without contacts (IndexOutOfRangeException)"));
        if first.normal_impulse < s.MinimumImpactForce {
            return;
        }
        if let Some(o) = self.physical_of_collider(c.collider).filter(|&o| self.physicals[o].rigidbody.bodyType == 0) {
            let rb = self.physicals[o].rigidbody.clone();
            let (v, prev) = (rb.velocity(&self.world), self.physicals[o].previousVelocity);
            rb.set_velocity(&mut self.world, [mathf::lerp(v[0], prev[0], 1.0), mathf::lerp(v[1], prev[1], 1.0)]);
        }
        self.bulb_break(i);
    }

    /// BulbBehaviour.Break(velocity): the velocity is unused. The explosion's particles, the sprites and the Filament
    /// and LightSprite renderers are presentation.
    pub fn bulb_break(&mut self, i: usize) {
        if !self.bulbs[i].Broken {
            // Explosion.GetComponentInChildren<ExplosionSoundBehviour>().Play(): Near.PickRandom(), then the delayed
            // distant sound (audio is not simulated).
            let (id, near, distant) = self.bulbs[i].sound.unwrap_or_else(|| unimplemented!("BulbBehaviour.Break: an Explosion without an ExplosionSoundBehviour (NullReferenceException)"));
            if let Some(n) = near {
                self.pick_random(n);
                if self.prefs.DistantSoundEffects && distant != 0 {
                    self.start_wait(id, 0.0, Step::DistantSound(distant));
                }
            }
            self.bulbs[i].Broken = true;
        }
        if let Some(other) = self.bulbs[i].OtherPart {
            let id = self.bulbs[i].id;
            // OtherPart.SetActive(true): its first activation wakes the scripts it carries.
            let other_id = self.with_component_instance(id, |g, inst, _| {
                g.set_active_scripts(inst, other, true);
                inst.objects[other].components[0]
            });
            // OtherPart.transform.SetParent(null)
            self.set_parent_null_of(other_id, |_, _, _| {});
            // OtherPart.GetOrAddComponent<DebrisComponent>() and this GameObject's.
            self.with_component_instance(id, |g, inst, o| {
                g.get_or_add_debris(inst, other);
                g.get_or_add_debris(inst, o);
            });
        }
        // ColliderToDisable.enabled = false (Collider2D::SetEnabled).
        let collider = self.bulbs[i].collider.unwrap_or_else(|| unimplemented!("BulbBehaviour.Break: ColliderToDisable is null (NullReferenceException)"));
        let body = self.physicals[self.bulbs[i].Phys].rigidbody.body;
        self.collider_disable_one(collider, body);
    }

    /// GetOrAddComponent<DebrisComponent> on clone object `o`: a new one takes an instance id (it has no messages).
    fn get_or_add_debris(&mut self, inst: &super::spawn::Instance, o: usize) {
        let has = inst.prefab.objects[o].components.iter().any(|(_, c)| matches!(c, crate::assets::db::Component::Script { class, .. } if class == "DebrisComponent"));
        if !has && self.debris_added.insert(inst.objects[o].components[0]) {
            self.allocate_instance_id();
        }
    }

    /// LineRenderer.transform's position and rotation: its local TRS up to the gun's object, then the gun's chain.
    fn ray_gun_barrel_pose(&self, i: usize) -> ([f32; 3], [f32; 4]) {
        let s = &self.ray_guns[i];
        let nodes: Vec<Trs> = s.Barrel.iter().copied().chain(self.local_chain(s.Phys).iter().rev().copied()).collect();
        let (mut v, mut r) = ([nodes[0].t[0], nodes[0].t[1], nodes[0].t[2], 0.0], nodes[0].q);
        for n in &nodes[1..] {
            (v, r) = (global_position(n, v), global_rotation(n, r));
        }
        ([v[0], v[1], v[2]], r)
    }
}
