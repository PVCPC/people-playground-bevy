//! DamagableMachineryBehaviour: health, heat/fire/water damage and the broken state of machines.
#![allow(non_snake_case)]

use super::{script, Game};

script! {
    Machinery {
        FirearmDamageMultiplier: f32 = 0.0,
        ExplosionDamageMultiplier: f32 = 0.0,
        StabDamageMultiplier: f32 = 0.0,
        BreakDamage: f32 = 0.0,
        ExplodesOnBreak: bool = false,
        Waterproof: bool = true,
        IndestructibilityCanBeToggled: bool = true,
        TemperatureThreshold: f32 = f32::INFINITY,
        FireDamageThreshold: f32 = 1.0,
        Health: f32 = 10.0,
        CanRepair: bool = true,
        Destroyed: bool = false,
        Indestructible: bool = false,
        HeatShielded: bool = false,
    }
    links {
        /// GetComponent<PhysicalBehaviour>()
        PhysicalBehaviour: usize = 0,
        initialHealth: f32 = 0.0,
        RandomOffset: [f32; 4] = [0.0; 4],
        t: f32 = 0.0,
        /// SparkPrefab (or the Resources fallback) and ExplosionPrefab are set.
        SparkPrefab: bool = false,
        ExplosionPrefab: bool = false,
        /// The live entries of BehavioursToToggle (class name and component id) and GameObjectsToToggle (the object's
        /// Transform id).
        BehavioursToToggle: Vec<(String, i32)> = Vec::new(),
        GameObjectsToToggle: Vec<i32> = Vec::new(),
        /// OnDamaged's persistent calls reach only audio and particle components, or are all in `OnDamagedCalls`.
        OnDamagedPresentation: bool = true,
        OnDamagedCalls: Vec<super::link::EventCall> = Vec::new(),
        /// The component's instance id.
        id: i32 = 0,
    }
}

impl Game {
    /// DamagableMachineryBehaviour.Awake (the materials and the spark prefab are Resources loads).
    pub fn machinery_awake(&mut self, m: usize) {
        self.machinery[m].initialHealth = self.machinery[m].Health;
        let x = self.random.value() * 1000.0;
        let y = self.random.value() * 1000.0;
        self.machinery[m].RandomOffset = [x, y, 0.0, 0.0];
    }

    /// DamagableMachineryBehaviour.Start: the context menu buttons are UI.
    pub fn machinery_start(&mut self, m: usize) {
        if self.machinery[m].Destroyed {
            self.machinery_set_as_broken(m, false);
        }
    }

    /// DamagableMachineryBehaviour.FixedUpdate
    pub fn machinery_fixed_update(&mut self, m: usize) {
        let mc = &self.machinery[m];
        if mc.Indestructible || mc.Destroyed {
            return;
        }
        let ph = &self.physicals[mc.PhysicalBehaviour];
        if !mc.HeatShielded && (ph.BurnProgress > mc.FireDamageThreshold || ph.Temperature > mc.TemperatureThreshold) {
            self.machinery[m].Health = 0.0;
            self.machinery_act_on_damage(m);
        }
        let mc = &self.machinery[m];
        if !mc.Waterproof && self.physicals[mc.PhysicalBehaviour].Wetness as f64 > 0.95 && self.random.value() as f64 > 0.9 {
            self.machinery[m].Health = 0.0;
            self.machinery_act_on_damage(m);
        }
    }

    /// DamagableMachineryBehaviour.Update
    pub fn machinery_update(&mut self, m: usize) {
        self.machinery[m].t += self.time.deltaTime;
        let mc = &self.machinery[m];
        let points = |g: &Game| g.physicals[g.machinery[m].PhysicalBehaviour].LocalColliderGridPoints.as_ref().expect("LocalColliderGridPoints before PhysicalBehaviour.Start").len();
        if mc.Destroyed && points(self) != 0 && mc.t > 1.0 && mc.SparkPrefab && self.random.value() < 4e-5 {
            self.machinery[m].t = 0.0;
            let n = points(self) as i32;
            self.random.range_int(0, n);
            unimplemented!("DamagableMachineryBehaviour.Update: Instantiate(SparkPrefab)");
        }
    }

    /// DamagableMachineryBehaviour.Stabbed
    pub fn machinery_stabbed(&mut self, m: usize, stabber: usize) {
        let mc = &self.machinery[m];
        if !mc.Indestructible && !mc.Destroyed && self.physicals[stabber].StabCausesWound {
            self.machinery[m].Health -= mc.StabDamageMultiplier;
            self.machinery_act_on_damage(m);
        }
    }

    /// DamagableMachineryBehaviour.OnFragmentHit
    pub fn machinery_on_fragment_hit(&mut self, m: usize, force: f32) {
        let mc = &mut self.machinery[m];
        if !mc.Indestructible {
            mc.Health -= force * mc.ExplosionDamageMultiplier * 250.0;
            self.machinery_act_on_damage(m);
        }
    }

    /// DamagableMachineryBehaviour.ForceBreak
    pub(super) fn machinery_force_break(&mut self, m: usize) {
        let mc = &mut self.machinery[m];
        if !mc.Indestructible && !mc.Destroyed {
            mc.Health = -1.0;
            self.machinery_act_on_damage(m);
        }
    }

    /// DamagableMachineryBehaviour.ActOnDamage
    pub(super) fn machinery_act_on_damage(&mut self, m: usize) {
        let mc = &self.machinery[m];
        if !(mc.Health > 0.0) && !mc.Destroyed {
            self.machinery_set_as_broken(m, true);
            if self.machinery[m].ExplodesOnBreak {
                unimplemented!("DamagableMachineryBehaviour: ExplosionCreator.CreateExplosionWithWater");
            }
            if self.machinery[m].ExplosionPrefab {
                unimplemented!("DamagableMachineryBehaviour: Instantiate(ExplosionPrefab)");
            }
        }
    }

    /// DamagableMachineryBehaviour.SetAsBroken: the broken material and its property block are rendering.
    fn machinery_set_as_broken(&mut self, m: usize, run_events: bool) {
        let mc = &mut self.machinery[m];
        mc.Destroyed = true;
        mc.Health = 0.0;
        for (class, id) in self.machinery[m].BehavioursToToggle.clone() {
            // (bool)monoBehaviour: an earlier toggle may have destroyed it.
            if !self.destroyed_components.contains(&id) {
                self.behaviour_disable(&class, id);
            }
        }
        for id in self.machinery[m].GameObjectsToToggle.clone() {
            if !self.destroyed_components.contains(&id) {
                self.with_component_instance(id, |g, inst, o| g.set_active_scripts(inst, o, false));
            }
        }
        let mc = &self.machinery[m];
        if run_events && !mc.OnDamagedPresentation {
            unimplemented!("DamagableMachineryBehaviour.OnDamaged: a persistent call beyond audio, particles and the kept event calls");
        }
        if run_events {
            for call in self.machinery[m].OnDamagedCalls.clone() {
                self.event_call(call);
            }
        }
    }

    /// Behaviour.enabled = false on component `id`: RemoveFromManager, with the OnDisable of an active GameObject.
    fn behaviour_disable(&mut self, class: &str, id: i32) {
        let active = self.with_component_instance(id, |_, inst, o| inst.objects[o].active);
        match class {
            "LEDBulbBehaviour" => {
                let i = self.leds.iter().position(|x| x.id == id).expect("a LEDBulbBehaviour in the arena");
                if !std::mem::replace(&mut self.leds[i].enabled, false) {
                    return;
                }
                self.behaviours.remove(super::Behaviour::Led(i));
                if active {
                    self.led_on_disable(i);
                }
            }
            "DetectorBehaviour" => {
                let i = self.detectors.iter().position(|x| x.id == id).expect("a DetectorBehaviour in the arena");
                if !std::mem::replace(&mut self.detectors[i].enabled, false) {
                    return;
                }
                self.behaviours.remove(super::Behaviour::Detector(i));
                if active {
                    self.detector_on_disable(i);
                }
            }
            "GeneratorBehaviour" => {
                let i = self.generators.iter().position(|x| x.id == id).expect("a GeneratorBehaviour in the arena");
                if !std::mem::replace(&mut self.generators[i].enabled, false) {
                    return;
                }
                self.behaviours.remove(super::Behaviour::Generator(i));
                if active {
                    self.generator_on_disable(i);
                }
            }
            "SingleFloodlightBehaviour" => {
                let i = self.floodlights.iter().position(|x| x.id == id).expect("a SingleFloodlightBehaviour in the arena");
                if !std::mem::replace(&mut self.floodlights[i].enabled, false) {
                    return;
                }
                self.behaviours.remove(super::Behaviour::Floodlight(i));
                if active {
                    self.with_component_instance(id, |g, inst, _| g.floodlight_on_disable(i, inst));
                }
            }
            "SpikeGrenadeBehaviour" => {
                let i = self.spike_grenades.iter().position(|x| x.id == id).expect("a SpikeGrenadeBehaviour in the arena");
                if !std::mem::replace(&mut self.spike_grenades[i].enabled, false) {
                    return;
                }
                self.behaviours.remove(super::Behaviour::SpikeGrenade(i));
                if active {
                    self.spike_grenade_on_disable(i);
                }
            }
            "TemperatureRayGunBehaviour" => {
                let i = self.ray_guns.iter().position(|x| x.id == id).expect("a TemperatureRayGunBehaviour in the arena");
                if !std::mem::replace(&mut self.ray_guns[i].enabled, false) {
                    return;
                }
                self.behaviours.remove(super::Behaviour::RayGun(i));
                if active {
                    self.ray_gun_on_disable(i);
                }
            }
            "ThrusterbedBehaviour" => {
                let i = self.thrusterbeds.iter().position(|x| x.id == id).expect("a ThrusterbedBehaviour in the arena");
                if !std::mem::replace(&mut self.thrusterbeds[i].enabled, false) {
                    return;
                }
                self.behaviours.remove(super::Behaviour::Thrusterbed(i));
                if active {
                    self.thrusterbed_on_disable(i);
                }
            }
            _ => unimplemented!("DamagableMachineryBehaviour.SetAsBroken: {class}.enabled = false"),
        }
    }

    /// Whether script `id` is enabled for an OnEnable: a DeleteAfterTime's runtime flag (a UseEventTrigger enables it),
    /// else the prefab's m_Enabled unless it was disabled at run time.
    pub(super) fn script_enabled(&self, id: i32, prefab_enabled: bool) -> bool {
        match self.delete_afters.iter().find(|d| d.id == id) {
            Some(d) => d.enabled,
            None => prefab_enabled && !self.script_disabled(id),
        }
    }

    /// Behaviour.enabled was set to false at run time (SetAsBroken), over the prefab's m_Enabled.
    pub(super) fn script_disabled(&self, id: i32) -> bool {
        self.leds.iter().any(|x| x.id == id && !x.enabled)
            || self.detectors.iter().any(|x| x.id == id && !x.enabled)
            || self.generators.iter().any(|x| x.id == id && !x.enabled)
            || self.floodlights.iter().any(|x| x.id == id && !x.enabled)
            || self.spike_grenades.iter().any(|x| x.id == id && !x.enabled)
            || self.ray_guns.iter().any(|x| x.id == id && !x.enabled)
            || self.thrusterbeds.iter().any(|x| x.id == id && !x.enabled)
    }
}
