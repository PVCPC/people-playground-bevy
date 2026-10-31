//! BeamformerBehaviour (the Beam Rifle) and GenericScifiWeapon40Behaviour (the Disassembler). Audio, muzzle flashes,
//! the beam's LineRenderer and the glow shader property are presentation.
#![allow(non_snake_case)]

use super::explosion::{ExplosionParameters, RayHit2D, DEFAULT_RAYCAST_LAYERS};
use super::{script, Game};
use crate::assets::db::ObjRef;
use crate::b2::math::Vec2 as V;

/// Physics2D.CircleCastNonAlloc's buffer in BeamformerBehaviour.
const BUFFER: usize = 1024;

/// The classes with a Slice() method, and their subclasses.
pub(super) const SLICE_RECEIVERS: &[&str] = &[
    "AppliedBandageBehaviour", "BloodWireBehaviour", "BreakJointOnShotBehaviour", "CathodeLightBehaviour", "CopperWireBehaviour",
    "DamagableMachineryBehaviour", "DistanceJointWireBehaviour", "EnergyWireBehaviour", "FixedCableBehaviour", "FixedJointWireBehaviour",
    "GlowtubeBehaviour", "HeatPipeBehaviour", "LimbBehaviour", "NewUseWireBehaviour", "PinBehaviour", "RigidWireBehaviour", "RopeBehaviour",
    "SliderJointWireBehaviour", "SpringCableBehaviour", "SteelBindingBehaviour", "UseWireBehaviour", "WireBehaviour", "WoodStrutBehaviour",
    "WoodenBindingBehaviour",
];

script! {
    Beamformer {
        barrelPosition: [f32; 3] = [0.0; 3],
        barrelDirection: [f32; 3] = [0.0; 3],
        TemperatureCooldown: f32 = 1.0,
        BeamRange: f32 = 1100.0,
        ExplosionForce: f32 = 1.0,
        DirectionalForce: f32 = 1.0,
        RecoilForce: f32 = 0.0,
        BeamWidth: f32 = 0.05,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// GetComponent<PhysicalBehaviour>() (Awake); GetComponent<Rigidbody2D>() is its rigidbody.
        phys: usize = 0,
        enabled: bool = true,
        /// LayersToHit (LayerMask bits), which CircleCastNonAlloc takes as its distance.
        LayersToHit: i32 = 0,
        /// The private `temperature`.
        temperature: f32 = 0.0,
    }
}

script! {
    Scifi40 {
        RecoilForce: f32 = 0.0,
        ImpactForce: f32 = 0.0,
        barrelPosition: [f32; 3] = [0.0; 3],
        barrelDirection: [f32; 3] = [0.0; 3],
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// The physical on the same GameObject: its Rigidbody2D is `Rigidbody`.
        physical: usize = 0,
        enabled: bool = true,
        /// LayersToHit (LayerMask bits)
        LayersToHit: u32 = 0,
        /// Impact: instantiated on a disintegration (no physics).
        Impact: Option<ObjRef> = None,
    }
}

impl Game {
    fn beamformer_barrel_position(&self, i: usize) -> [f32; 2] {
        let s = &self.beamformers[i];
        let p = self.transform_point3(s.phys, s.barrelPosition);
        [p[0], p[1]]
    }

    fn beamformer_barrel_direction(&self, i: usize) -> [f32; 2] {
        let s = &self.beamformers[i];
        let v = self.transform_direction(s.phys, s.barrelDirection);
        let k = self.physicals[s.phys].transform.localScale[0];
        [v[0] * k, v[1] * k]
    }

    /// BeamformerBehaviour.Update
    pub fn beamformer_update(&mut self, i: usize) {
        let s = &mut self.beamformers[i];
        if s.temperature > 0.0 {
            s.temperature -= self.time.deltaTime;
        }
    }

    /// BeamformerBehaviour.OnDisable (the aim line and the glow are presentation).
    pub fn beamformer_on_disable(&mut self, i: usize) {
        self.beamformers[i].temperature = 0.0;
    }

    /// BeamformerBehaviour.Use
    pub fn beamformer_use(&mut self, i: usize) {
        if self.beamformers[i].enabled {
            self.beamformer_shoot(i);
        }
    }

    /// BeamformerBehaviour.Shoot: the audio and the muzzle flash are presentation; BeamRenderRoutine runs up to its
    /// WaitForSeconds, which only hides the beam, so DoLaserDamage runs here.
    pub fn beamformer_shoot(&mut self, i: usize) {
        if self.beamformers[i].temperature > 0.05 {
            return;
        }
        let s = &mut self.beamformers[i];
        s.temperature += s.TemperatureCooldown;
        let (phys, recoil) = (s.phys, s.RecoilForce);
        let position = self.beamformer_barrel_position(i);
        self.camera_shake(5.0, position);
        let direction = self.beamformer_barrel_direction(i);
        let k = -1.0 * recoil;
        let rb = self.physicals[phys].rigidbody.clone();
        rb.add_force_impulse(&mut self.world, [k * direction[0], k * direction[1]]);
        // StartCoroutine(BeamRenderRoutine()): BarrelPosition and BarrelDirection again.
        let position = self.beamformer_barrel_position(i);
        let direction = self.beamformer_barrel_direction(i);
        self.beamformer_do_laser_damage(i, position, direction);
        self.physicals[phys].Temperature += 10.0;
    }

    /// BeamformerBehaviour.DoLaserDamage: the loop condition casts again before each body, which then reads the
    /// i-th result of that cast.
    fn beamformer_do_laser_damage(&mut self, i: usize, position: [f32; 2], forward: [f32; 2]) {
        let s = &self.beamformers[i];
        let (width, layers, own, explosion_force, directional_force) = (s.BeamWidth, s.LayersToHit, s.phys, s.ExplosionForce, s.DirectionalForce);
        let k = 0.02f32 + width;
        let origin = [position[0] + forward[0] * k, position[1] + forward[1] * k];
        let mut n = 0;
        loop {
            // Physics2D.CircleCastNonAlloc(origin, BeamWidth, forward, buffer, (int)LayersToHit): the layer mask is
            // the default one.
            let hits = self.circle_cast_all(origin, width, forward, layers as f32, DEFAULT_RAYCAST_LAYERS);
            if n >= hits.len() {
                break;
            }
            let hit = hits[n];
            n += 1;
            let Some(value) = self.hit_transform_physical(&hit).or_else(|| self.physical_of_collider(hit.collider)).filter(|&p| self.physicals[p].in_world) else { continue };
            if self.root_of(value) == self.root_of(own) {
                continue;
            }
            let point = [hit.point.x, hit.point.y];
            let flag = self.random.value() > 0.25;
            self.ignite_at(value, flag, point);
            self.physicals[value].Temperature += 100.0;
            self.send_break(hit.collider, Some(value), [forward[0] * directional_force, forward[1] * directional_force]);
            let id = self.physicals[value].id;
            if self.tr_has_script(id, "LimbBehaviour") {
                let l = self.limbs.iter().position(|x| x.PhysicalBehaviour == value).unwrap_or_else(|| unimplemented!("LimbBehaviour without its limb"));
                if self.limbs[l].HasJoint {
                    self.limb_slice(l);
                } else {
                    self.limb_crush(l);
                }
            } else {
                self.send_slice(value);
            }
            self.explosion_creator_explode(ExplosionParameters {
                FragmentationRayCount: 5,
                Position: [point[0], point[1], 0.0],
                Range: 2.0,
                FragmentForce: explosion_force,
                CreateParticlesAndSound: true,
                ..Default::default()
            });
            let rb = self.physicals[value].rigidbody.clone();
            let m = rb.mass(&self.world);
            let num = directional_force * m;
            rb.add_impulse_at_position(&mut self.world, [num * forward[0], num * forward[1]], point);
        }
    }

    /// Physics2D.CircleCastNonAlloc(origin, radius, direction, results[1024], distance, layerMask): the hits in order of
    /// time of impact. PhysicsQuery2D::CircleCast normalizes the direction (a zero one stays zero); the sort is
    /// Cast2DQueryBase::RunQuery's std::sort.
    pub(super) fn circle_cast_all(&mut self, origin: [f32; 2], radius: f32, direction: [f32; 2], distance: f32, mask: u32) -> Vec<RayHit2D> {
        if 0.0001f32 >= radius {
            return Vec::new();
        }
        if distance == f32::INFINITY {
            unimplemented!("Physics2D.CircleCastNonAlloc with an infinite distance (it casts 100000 and scales the fractions back)");
        }
        self.sync_pending();
        let sqr = direction[0] * direction[0] + direction[1] * direction[1];
        let dir = if sqr != 0.0 {
            let inv = 1.0f32 / sqr.sqrt();
            [direction[0] * inv, direction[1] * inv]
        } else {
            [0.0, 0.0]
        };
        let end = V::new(origin[0] + dir[0] * distance, origin[1] + dir[1] * distance);
        let mut candidates = self.world.circle_cast_candidates(V::new(origin[0], origin[1]), end, radius, mask);
        super::sort::msvc_sort(&mut candidates, |a, b| a.fraction < b.fraction);
        let mut r = radius;
        let hits: Vec<RayHit2D> = candidates
            .iter()
            .filter_map(|c| self.world.circle_cast_hit(c, &mut r))
            .map(|h| RayHit2D { collider: h.collider, game_object: h.game_object, body: h.body, point: h.point, normal: h.normal, distance: h.distance, fraction: h.fraction })
            .collect();
        if hits.len() > BUFFER {
            unimplemented!("Physics2D.CircleCastNonAlloc with more hits than its buffer holds");
        }
        // Whether a collider's several fixtures report once or each is not known.
        if hits.iter().enumerate().any(|(k, h)| hits[..k].iter().any(|o| o.collider == h.collider)) {
            unimplemented!("Physics2D.CircleCastNonAlloc: two hits on one collider (whether the fixtures of a collider report once)");
        }
        hits
    }

    /// SendMessage("Slice", DontRequireReceiver) to a physical object's GameObject: every component with the method, in
    /// order.
    pub(super) fn send_slice(&mut self, p: usize) {
        let classes: Vec<(String, i32)> = self.with_component_instance(self.physicals[p].id, |_, inst, o| {
            inst.prefab.objects[o].components.iter().enumerate().filter_map(|(k, (_, c))| match c {
                crate::assets::db::Component::Script { class, .. } if SLICE_RECEIVERS.contains(&class.as_str()) => Some((class.clone(), inst.objects[o].components[k])),
                _ => None,
            }).collect()
        });
        for (class, id) in classes {
            match class.as_str() {
                "BreakJointOnShotBehaviour" => self.break_joint_on_shot_slice(id),
                "LimbBehaviour" => {
                    let l = self.limbs.iter().position(|x| x.PhysicalBehaviour == p).expect("the object's LimbBehaviour");
                    self.limb_slice(l);
                }
                "DamagableMachineryBehaviour" => {
                    let m = self.machinery.iter().position(|m| m.PhysicalBehaviour == p).expect("the object's DamagableMachineryBehaviour");
                    let mc = &mut self.machinery[m];
                    if !mc.Indestructible && !mc.Destroyed {
                        mc.Health = 0.0;
                        self.machinery_act_on_damage(m);
                    }
                }
                "GlowtubeBehaviour" => {
                    let g = self.glowtubes.iter().position(|g| g.Phys == p).expect("the object's GlowtubeBehaviour");
                    self.glowtube_break(g);
                }
                c => unimplemented!("{c}.Slice"),
            }
        }
        // The BloodWireBehaviours the tool added come after the prefab's components.
        self.blood_wire_slice(p);
    }

    /// SendMessage("Damage", float.MaxValue, DontRequireReceiver): LimbBehaviour is the only class with a Damage method.
    fn send_damage_max(&mut self, p: usize) {
        self.send_damage(p, f32::MAX);
    }

    /// SendMessage("Damage", amount, DontRequireReceiver)
    pub(super) fn send_damage(&mut self, p: usize, amount: f32) {
        let id = self.physicals[p].id;
        if self.tr_has_script(id, "LimbBehaviour") {
            let l = self.limbs.iter().position(|x| x.PhysicalBehaviour == p).expect("the object's LimbBehaviour");
            self.limb_damage(l, amount);
        }
    }

    fn scifi40_barrel_position(&self, i: usize) -> [f32; 2] {
        let s = &self.scifi40s[i];
        let p = self.transform_point3(s.physical, s.barrelPosition);
        [p[0], p[1]]
    }

    fn scifi40_barrel_direction(&self, i: usize) -> [f32; 2] {
        let s = &self.scifi40s[i];
        let v = self.transform_direction(s.physical, s.barrelDirection);
        let k = self.physicals[s.physical].transform.localScale[0];
        [v[0] * k, v[1] * k]
    }

    /// GenericScifiWeapon40Behaviour.Awake: AddComponent<AudioSource>() takes an instance id; its settings are audio.
    pub fn scifi40_awake(&mut self) {
        self.allocate_instance_id();
    }

    /// GenericScifiWeapon40Behaviour.Use: the particles and the shot sound are presentation.
    pub fn scifi40_use(&mut self, i: usize) {
        if self.scifi40s[i].enabled {
            self.scifi40_fire(i);
        }
    }

    /// GenericScifiWeapon40Behaviour.Fire
    pub fn scifi40_fire(&mut self, i: usize) {
        if !self.scifi40s[i].enabled {
            return;
        }
        let s = &self.scifi40s[i];
        let (p, recoil, impact_force, layers, impact) = (s.physical, s.RecoilForce, s.ImpactForce, s.LayersToHit, s.Impact);
        // (0f - RecoilForce) * transform.lossyScale.x * transform.right: the Vector3 is a Vector2 as a force.
        let scale = self.lossy_scale(p)[0];
        let right = self.transform_right(p);
        let k = (0.0 - recoil) * scale;
        let rb = self.physicals[p].rigidbody.clone();
        let position = self.scifi40_barrel_position(i);
        rb.add_impulse_at_position(&mut self.world, [right[0] * k, right[1] * k], position);
        let position = self.scifi40_barrel_position(i);
        let direction = self.scifi40_barrel_direction(i);
        let Some(hit) = self.raycast(V::new(position[0], position[1]), direction, 2500.0, layers, f32::NEG_INFINITY, f32::INFINITY) else { return };
        let Some(value) = self.hit_transform_physical(&hit).or_else(|| self.physical_of_collider(hit.collider)).filter(|&p| self.physicals[p].in_world) else { return };
        let (point, normal) = ([hit.point.x, hit.point.y], [hit.normal.x, hit.normal.y]);
        let softness = self.properties[self.physicals[value].Properties].Softness;
        if self.random.value() * 0.1 <= softness || (self.random.value() > 0.9 && !self.physicals[value].isDisintegrated) {
            self.send_slice(value);
            self.send_damage_max(value);
            self.spawn_prefab(impact.expect("GenericScifiWeapon40Behaviour: Instantiate(null Impact) (ArgumentException)"), [point[0], point[1], 0.0]);
            self.physical_disintegrate(value);
            self.create_pulse_explosion_with([point[0], point[1], 0.0], impact_force / 2.0, 1.0, false, true);
        } else {
            self.send_shot(hit.collider, Some(value), normal, point, 15.0, false, None);
            let k = 0.0 - impact_force;
            let rb = self.physicals[value].rigidbody.clone();
            rb.add_impulse_at_position(&mut self.world, [normal[0] * k, normal[1] * k], point);
        }
    }
}
