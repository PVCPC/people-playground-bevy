//! EnergySwordBehaviour. Use switches the blade (BladeObject and MotionBlur) on and off; FixedUpdate cuts what the
//! BladeCollider overlaps through soft connections (soft_connect.rs), heating, burning and damaging what stays
//! connected and slicing it at random; Update cools the impact sound's cooldown. The loop, impact and rain audio, the
//! particles and the hit particle's position are presentation; the impact loop's isPlaying is kept (its Play draws).
#![allow(non_snake_case)]

use super::soft_connect::SoftConnections;
use super::{d, mathf, script, Game};

/// The overlap buffer (`new Collider2D[16]`) and the cast buffer (`new RaycastHit2D[16]`).
const BUFFER: usize = 16;

script! {
    EnergySword {
        SpeedCutThreshold: f32 = 1.0,
        MinSoftness: f32 = 0.0,
        Damage: f32 = 256.0,
        BladeTemperature: f32 = 900.0,
        Activated: bool = false,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        enabled: bool = true,
        /// PhysicalBehaviour
        physical: usize = 0,
        /// MainCollider's instance id.
        MainCollider: i32 = 0,
        /// BladeCollider: (object, component) in its instance, and its instance id.
        BladeCollider: (usize, usize) = (0, 0),
        blade_collider_id: i32 = 0,
        /// LocalBladeLine.A and .B
        LocalBladeLine: [[f32; 2]; 2] = [[0.0; 2]; 2],
        /// BladeObject and MotionBlur: objects of the instance.
        BladeObject: usize = 0,
        MotionBlur: usize = 0,
        /// LayerMask.m_Bits
        LayerMask: u32 = 0,
        /// ImpactSound.Length
        ImpactSound: usize = 0,
        /// ImpactSource.isPlaying (a looping AudioSource plays until Stop).
        impact_playing: bool = false,
        /// currentMapConfig != null
        map_config: bool = false,
        impactSoundHeat: f32 = 0.0,
        softConnections: SoftConnections = SoftConnections::default(),
    }
}

impl Game {
    /// EnergySwordBehaviour.Start: FindObjectOfType<MapConfig>, then SetActivation(playActivationSounds: false).
    pub fn energy_sword_start(&mut self, i: usize) {
        self.energy_swords[i].map_config = self.map_config_present;
        self.energy_sword_set_activation(i);
    }

    /// EnergySwordBehaviour.Use
    pub fn energy_sword_use(&mut self, i: usize) {
        if !self.energy_swords[i].enabled {
            return;
        }
        self.energy_swords[i].Activated = !self.energy_swords[i].Activated;
        self.energy_sword_set_activation(i);
        if self.energy_swords[i].Activated {
            // AddRelativeForce(Vector2.down * 0.1f, ForceMode2D.Impulse)
            let rb = self.physicals[self.energy_swords[i].physical].rigidbody.clone();
            rb.add_relative_force_impulse(&mut self.world, [0.0, -0.1]);
        }
    }

    /// SetActivation on the clone that holds the sword.
    fn energy_sword_set_activation(&mut self, i: usize) {
        let id = self.energy_swords[i].id;
        self.with_component_instance(id, |g, inst, _| g.energy_sword_set_activation_in(i, inst));
    }

    /// EnergySwordBehaviour.SetActivation: MotionBlur and BladeObject follow Activated; switching off stops the impact
    /// loop and destroys every connection's joint (the entries stay until the next FixedUpdate prunes them).
    pub fn energy_sword_set_activation_in(&mut self, i: usize, inst: &mut super::spawn::Instance) {
        let s = &self.energy_swords[i];
        let (on, blur, blade) = (s.Activated, s.MotionBlur, s.BladeObject);
        // `if ((bool)MotionBlur)`: OnDestroy destroys it, and it can be detached and destroyed with its parent first.
        if !self.destroyed_components.contains(&inst.objects[blur].components[0]) {
            self.set_active_scripts(inst, blur, on);
        }
        self.set_active_scripts(inst, blade, on);
        if on {
            return;
        }
        self.energy_swords[i].impact_playing = false;
        for k in self.energy_swords[i].softConnections.keys() {
            let f = self.energy_swords[i].softConnections.get(k).unwrap().joint;
            if self.soft_joint_alive(f) {
                self.destroy_joint_delayed(f);
            }
        }
    }

    /// EnergySwordBehaviour.OnDisable
    pub fn energy_sword_on_disable(&mut self, i: usize, inst: &mut super::spawn::Instance) {
        if self.energy_swords[i].Activated {
            self.energy_swords[i].Activated = false;
            self.energy_sword_set_activation_in(i, inst);
        }
    }

    /// EnergySwordBehaviour.Update: the impact sound's cooldown (the rain particles are presentation).
    pub fn energy_sword_update(&mut self, i: usize) {
        let s = &mut self.energy_swords[i];
        if !s.Activated {
            return;
        }
        if !s.map_config {
            unimplemented!("EnergySwordBehaviour.Update: currentMapConfig is null (NullReferenceException)");
        }
        s.impactSoundHeat -= self.time.deltaTime;
        if s.impactSoundHeat < f32::from_bits(1) {
            s.impactSoundHeat = 0.0;
        }
    }

    /// EnergySwordBehaviour.FixedUpdate: ProcessSoftConnections(shouldCut: true, 15f).
    pub fn energy_sword_fixed_update(&mut self, i: usize) {
        self.energy_sword_process(i, true, 15.0);
    }

    /// EnergySwordBehaviour.GetHitPoint: the blade line's circle cast hit on `coll`, else BladeCollider's closest
    /// point to coll's closest point to the sword.
    fn energy_sword_hit_point(&mut self, i: usize, coll: i32) -> [f32; 2] {
        let s = &self.energy_swords[i];
        let (p, line, blade) = (s.physical, s.LocalBladeLine, s.blade_collider_id);
        let a = self.transform_point3(p, [line[0][0], line[0][1], 0.0]);
        let b = self.transform_point3(p, [line[1][0], line[1][1], 0.0]);
        let v = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let magnitude = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        let mut hits = self.circle_cast_all([a[0], a[1]], 0.05, [v[0], v[1]], magnitude, super::explosion::DEFAULT_RAYCAST_LAYERS);
        hits.truncate(BUFFER);
        if let Some(h) = hits.iter().find(|h| h.collider == coll) {
            return [h.point.x, h.point.y];
        }
        self.saw_hit_point(p, blade, coll)
    }

    /// EnergySwordBehaviour.ProcessSoftConnections(shouldCut, connectionStrength)
    fn energy_sword_process(&mut self, i: usize, should_cut: bool, strength: f32) {
        let s = &self.energy_swords[i];
        let (id, key, blade, main, p, mask) = (s.id, s.BladeCollider, s.blade_collider_id, s.MainCollider, s.physical, s.LayerMask);
        let fixtures = self.with_component_instance(id, |_, inst, _| inst.fixtures.get(&key).cloned().unwrap_or_default());
        let mut hits = self.overlap_collider(&fixtures, mask);
        hits.truncate(BUFFER);
        let mut conns = std::mem::take(&mut self.energy_swords[i].softConnections);
        for k in conns.keys() {
            conns.get_mut(k).unwrap().should_be_deleted = true;
        }
        let num2 = self.physicals[p].Charge() * 0.1;
        let num3 = self.time.fixedDeltaTime * 0.02;
        let mut flag = false;
        let my_root = self.root_id(p);
        for (victim, coll) in hits {
            // collider2D.transform.root != base.transform.root, then PhysicalObjectsInWorldByTransform.TryGetValue.
            let Some(v) = victim.filter(|&v| self.root_id(v) != my_root && self.physicals[v].in_world) else { continue };
            if let Some(c) = conns.get(v).copied() {
                let broken = self.soft_connection_broken(&c);
                conns.get_mut(v).unwrap().should_be_deleted = broken;
                if broken {
                    continue;
                }
                flag = true;
                let hit = self.energy_sword_hit_point(i, coll);
                let a = self.inverse_transform_point(c.phys, [hit[0], hit[1], 0.0]);
                self.fixed_joint_set_anchor(c.joint, [a[0], a[1]]);
                self.friction_joint_set_max_force(c.joint, strength);
                self.friction_joint_set_max_torque(c.joint, strength);
                if self.physicals[c.phys].SimulateTemperature {
                    let t = self.physicals[c.phys].Temperature;
                    let target = self.energy_swords[i].BladeTemperature;
                    self.physicals[c.phys].Temperature = mathf::lerp(t, target, 0.00025 / self.heat_capacity(c.phys));
                }
                self.physicals[c.phys].BurnProgress += num3;
                let damage = self.energy_swords[i].Damage * self.time.fixedDeltaTime;
                self.send_damage(c.phys, damage);
                if should_cut && d(self.random.value()) > d(0.995) - num2 {
                    self.broadcast_slice(v);
                }
            } else if should_cut && !self.world.ignored.contains(&(coll.min(main), coll.max(main))) {
                self.soft_connect(&mut conns, p, blade, v, coll, strength, false, &mut |g| {
                    if g.energy_swords[i].impactSoundHeat <= f32::from_bits(1) {
                        // LoopSource.PlayOneShot(ImpactSound.PickRandom(), 1.5f)
                        g.pick_random(g.energy_swords[i].ImpactSound).unwrap_or_else(|| unimplemented!("EnergySwordBehaviour: ImpactSound.PickRandom() of an empty array (IndexOutOfRangeException)"));
                        g.energy_swords[i].impactSoundHeat = 0.5;
                    }
                    g.energy_sword_hit_point(i, coll)
                });
            }
        }
        if flag {
            // ImpactSource.Play(), then ImpactSource.time = clip.length * Random.value.
            if self.energy_swords[i].Activated && !self.energy_swords[i].impact_playing {
                self.energy_swords[i].impact_playing = true;
                self.random.value();
            }
        } else {
            self.energy_swords[i].impact_playing = false;
        }
        self.soft_connections_prune(&mut conns, blade);
        self.energy_swords[i].softConnections = conns;
    }

    /// EnergySwordBehaviour.OnDestroy: Destroy(MotionBlur), then each connection collides with the blade again and
    /// its joint is destroyed.
    pub fn energy_sword_destroy(&mut self, i: usize, blur: i32) {
        self.destroy_object(blur);
        let blade = self.energy_swords[i].blade_collider_id;
        for k in self.energy_swords[i].softConnections.keys() {
            let c = *self.energy_swords[i].softConnections.get(k).unwrap();
            self.ignore_collision_substitute(c.coll, blade, false);
            if self.soft_joint_alive(c.joint) {
                self.destroy_joint_delayed(c.joint);
            }
        }
    }
}
