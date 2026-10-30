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
        /// The live entries of BehavioursToToggle (class names) and GameObjectsToToggle.
        BehavioursToToggle: Vec<String> = Vec::new(),
        GameObjectsToToggle: usize = 0,
        /// OnDamaged's persistent calls reach only audio and particle components.
        OnDamagedPresentation: bool = true,
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

    /// DamagableMachineryBehaviour.ActOnDamage
    fn machinery_act_on_damage(&mut self, m: usize) {
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
        if let Some(class) = mc.BehavioursToToggle.first() {
            unimplemented!("DamagableMachineryBehaviour.SetAsBroken: {class}.enabled = false");
        }
        if mc.GameObjectsToToggle != 0 {
            unimplemented!("DamagableMachineryBehaviour.SetAsBroken: GameObject.SetActive(false)");
        }
        if run_events && !mc.OnDamagedPresentation {
            unimplemented!("DamagableMachineryBehaviour.OnDamaged: a persistent call beyond audio and particles");
        }
    }
}
