//! GeneratorBehaviour, SingleFloodlightBehaviour, SpikeGrenadeBehaviour, HeatingElementBehaviour and
//! TemperatureRayGunBehaviour. Audio, particles, sprite and light toggles, property blocks and the context menu
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
    /// clip and AudioSource.Play are audio. (`tool.rs` has no UseContinuous yet.)
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
                    let vid = self.physicals[v].id;
                    if self.with_component_instance(vid, |_, inst, o| inst.prefab.objects[o].components.iter().any(|(_, c)| matches!(c, crate::assets::db::Component::Script { class, .. } if class == "BulbBehaviour"))) {
                        unimplemented!("TemperatureRayGunBehaviour: BulbBehaviour.FilamentTemp");
                    }
                }
            }
        }
        let dt = self.time.deltaTime;
        self.ray_guns[i].useHeat -= dt;
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
