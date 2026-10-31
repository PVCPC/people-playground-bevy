//! BlasterBehaviour (the Blaster, Blaster Rifle and Machine Blaster) and BlasterboltBehaviour (their bolts): a bare
//! Transform the script steps along its right vector with Utils.MaterialAwareRaycast until a ray finds something.
//! The muzzle flash, the audio, the trail and the particle children are presentation. The bolt's TrailRenderer has
//! autodestruct set once it closed: the native renderer destroys the GameObject when its trail (`m_Time`) has faded.
#![allow(non_snake_case)]

use super::explosion::{ExplosionParameters, RayHit2D};
use super::translated::rt::Xf;
use super::{d, mathf, script, Game};
use crate::assets::db::ObjRef;

/// Global.main.EnergyLayers (Main.unity): layer 17.
const ENERGY_LAYERS: u32 = 1 << 17;
/// Utils.MaterialAwareRaycast's maxSteps default.
const MAX_STEPS: i32 = 8;

script! {
    Blaster {
        Interval: f32 = 0.05,
        Recoil: f32 = 1.0,
        InaccuracyMultiplier: f32 = 0.05,
        Automatic: bool = false,
        barrelPosition: [f32; 2] = [0.0; 2],
        barrelDirection: [f32; 2] = [0.0; 2],
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// GetComponent<PhysicalBehaviour>() (Awake).
        physical: usize = 0,
        enabled: bool = true,
        /// Bolt: instantiated by Shoot; its pose is the script's own (link checks it has no physics).
        Bolt: Option<ObjRef> = None,
        /// Clips.Length
        Clips: usize = 0,
        ScreenShakeMultiplier: f32 = 1.0,
        /// The private `t`: the seconds since the last shot.
        t: f32 = 0.0,
    }
}

script! {
    Blasterbolt {
        Speed: f32 = 1.0,
        ImpactStrength: f32 = 0.0,
        damage: f32 = 1.0,
        createHitDecal: bool = true,
        SpawnExplosionEffect: bool = false,
        CreateExplosion: bool = false,
        DecalSizeMultiplier: f32 = 1.0,
        TemperatureTarget: f32 = 900.0,
        ExplosionDamage: f32 = 0.0,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// layers (LayerMask bits)
        layers: u32 = 0,
        ImpactEffect: Option<ObjRef> = None,
        HitDecal: Option<ObjRef> = None,
        /// ImpactDecal: an arena index of `Game::decal_descriptors`; None for a null asset.
        ImpactDecal: Option<usize> = None,
        /// The Trail's m_Time: 0.025 in all three bolt prefabs (TrailRenderer data is not read).
        trail_time: f32 = 0.025,
        /// The private `closing`, and the time it was set.
        closing: bool = false,
        closed_at: f64 = 0.0,
    }
}

impl Game {
    /// BarrelPosition: transform.TransformPoint(barrelPosition), as a Vector2.
    fn blaster_barrel_position(&self, i: usize) -> [f32; 2] {
        let b = self.blasters[i].barrelPosition;
        let p = self.transform_point3(self.blasters[i].physical, [b[0], b[1], 0.0]);
        [p[0], p[1]]
    }

    /// BarrelDirection: transform.TransformDirection(barrelDirection) * transform.localScale.x.
    fn blaster_barrel_direction(&self, i: usize) -> [f32; 2] {
        let p = self.blasters[i].physical;
        let b = self.blasters[i].barrelDirection;
        let v = self.transform_direction(p, [b[0], b[1], 0.0]);
        let s = self.physicals[p].transform.localScale[0];
        [v[0] * s, v[1] * s]
    }

    /// BlasterBehaviour.Use
    pub fn blaster_use(&mut self, i: usize) {
        self.blaster_shoot(i);
    }

    /// BlasterBehaviour.UseContinuous: ContinuousActivationBehaviour.AssertState() only checks.
    pub fn blaster_use_continuous(&mut self, i: usize) {
        let s = &self.blasters[i];
        if s.Automatic && !(s.t < s.Interval) && s.enabled {
            self.blaster_shoot(i);
        }
    }

    /// BlasterBehaviour.Update
    pub fn blaster_update(&mut self, i: usize) {
        self.blasters[i].t += self.time.deltaTime;
    }

    /// BlasterBehaviour.Shoot (the sound, the muzzle flash and the screen-shake's audio are presentation).
    pub fn blaster_shoot(&mut self, i: usize) {
        if !self.blasters[i].enabled {
            return;
        }
        let vector = self.blaster_barrel_position(i);
        let vector2 = self.blaster_barrel_direction(i);
        self.blasters[i].t = 0.0;
        let s = &self.blasters[i];
        let (p, recoil, inaccuracy, bolt, clips, shake) = (s.physical, s.Recoil, s.InaccuracyMultiplier, s.Bolt, s.Clips, s.ScreenShakeMultiplier);
        let k = 0.0 - recoil;
        let rb = self.physicals[p].rigidbody.clone();
        rb.add_impulse_at_position(&mut self.world, [vector2[0] * k, vector2[1] * k], vector);
        // Object.Instantiate(Bolt, vector, identity).transform.right = vector2 + InaccuracyMultiplier * insideUnitCircle
        let bolt = bolt.unwrap_or_else(|| unimplemented!("BlasterBehaviour: Instantiate(null Bolt) (ArgumentException)"));
        let id = self.spawn_prefab(bolt, [vector[0], vector[1], 0.0]);
        let r = self.random.inside_unit_circle();
        self.tr_set_right(Xf::Object(id), [vector2[0] + inaccuracy * r[0], vector2[1] + inaccuracy * r[1], 0.0]);
        // PlayOneShot(Clips.PickRandom()): audio, the draw stays.
        self.pick_random(clips);
        self.camera_shake(0.5 * shake, vector);
    }

    /// The physical a hit's transform is (Global.PhysicalObjectsInWorldByTransform), if it is in the world.
    pub(super) fn hit_world_physical(&self, hit: &RayHit2D) -> Option<usize> {
        self.hit_transform_physical(hit).or_else(|| self.physical_of_collider(hit.collider)).filter(|&p| self.physicals[p].in_world)
    }

    /// BlasterboltBehaviour.Update
    pub fn blasterbolt_update(&mut self, b: usize) {
        let id = self.blasterbolts[b].id;
        if self.blasterbolts[b].closing {
            // Trail.autodestruct = true: the renderer destroys the object once its trail has faded.
            if self.time.timeAsDouble - self.blasterbolts[b].closed_at >= self.blasterbolts[b].trail_time as f64 {
                self.destroy_object(id);
            }
            return;
        }
        let (position, rotation) = self.with_component_instance(id, |_, inst, o| (inst.position(o), inst.rotation(o)));
        let right = mathf::rotate(rotation, [1.0, 0.0, 0.0]);
        let s = &self.blasterbolts[b];
        let (speed, layers, strength, damage) = (s.Speed, s.layers, s.ImpactStrength, s.damage);
        let num = speed * self.time.deltaTime;
        let laser = self.material_aware_raycast([position[0], position[1]], [right[0], right[1]], num, layers, MAX_STEPS, 0.0);
        let Some(hit) = laser.hit else {
            let to = [position[0] + right[0] * num, position[1] + right[1] * num, position[2] + right[2] * num];
            self.with_component_instance(id, |_, inst, o| inst.set_world_position(o, to));
            return;
        };
        let (point, normal) = ([hit.point.x, hit.point.y], [hit.normal.x, hit.normal.y]);
        let flag = ENERGY_LAYERS & (1 << self.world.layers[&hit.game_object]) != 0;
        match laser.physical {
            Some(p) if self.physicals[p].ReflectsLasers || flag => {
                let k = 0.0 - strength;
                let rb = self.physicals[p].rigidbody.clone();
                rb.add_impulse_at_position(&mut self.world, [normal[0] * k, normal[1] * k], point);
                // Vector2.Reflect(transform.right, normal)
                let dir = [right[0], right[1]];
                let num = (d(-2.0) * (d(normal[0]) * dir[0] + d(normal[1]) * dir[1]).f()).f();
                let reflected = [(d(num) * normal[0] + dir[0]).f(), (d(num) * normal[1] + dir[1]).f()];
                // transform.position = value.point (z becomes 0); transform.right = vector.
                self.with_component_instance(id, |_, inst, o| inst.set_world_position(o, [point[0], point[1], 0.0]));
                self.tr_set_right(Xf::Object(id), [reflected[0], reflected[1], 0.0]);
                if self.physicals[p].SimulateTemperature {
                    let t = self.physicals[p].Temperature;
                    self.physicals[p].Temperature = (d(t) + d(damage) * self.random.value() * 0.02f32).f();
                }
                if !flag {
                    let bp = self.physicals[p].BurnProgress;
                    self.physicals[p].BurnProgress = (d(bp) + d(0.002f32) * self.random.value()).f();
                }
            }
            Some(p) if self.physicals[p].AbsorbsLasers => {
                self.with_component_instance(id, |_, inst, o| inst.set_world_position(o, [point[0], point[1], 0.0]));
                // value.collider.attachedRigidbody.AddForceAtPosition(normal * -ImpactStrength, point) (ForceMode2D.Force)
                let rb = self.rigidbody_of_body(hit.body).unwrap_or_else(|| unimplemented!("BlasterboltBehaviour: the hit collider's attachedRigidbody is null (NullReferenceException)"));
                let k = 0.0 - strength;
                rb.add_force_at_position(&mut self.world, [normal[0] * k, normal[1] * k], point);
                let bp = self.physicals[p].BurnProgress;
                self.physicals[p].BurnProgress = (d(bp) + d(0.2f32) * self.random.value()).f();
                self.blasterbolt_impact(b, &hit);
            }
            _ => self.blasterbolt_impact(b, &hit),
        }
    }

    /// BlasterboltBehaviour.Impact
    fn blasterbolt_impact(&mut self, b: usize, hit: &RayHit2D) {
        let (point, normal) = ([hit.point.x, hit.point.y], [hit.normal.x, hit.normal.y]);
        let at = [point[0], point[1], 0.0];
        let s = &self.blasterbolts[b];
        let (strength, damage, target, effect, decal, create_decal, spawn_fx, explode, decal_multiplier, explosion_damage, impact_decal) =
            (s.ImpactStrength, s.damage, s.TemperatureTarget, s.ImpactEffect, s.HitDecal, s.createHitDecal, s.SpawnExplosionEffect, s.CreateExplosion, s.DecalSizeMultiplier, s.ExplosionDamage, s.ImpactDecal);
        let under_water = self.is_point_under_water(point);
        if under_water {
            unimplemented!("ExplosionCreator.CreateUnderwaterExplosionEffect");
        }
        // Instantiate(ImpactEffect, hit.point, Euler(0, 0, Atan2(normal.y, normal.x))).transform.up = normal. The effect
        // has no physics and nothing reads its pose before this, so only FromToRotation(up, normal) remains.
        let effect_id = self.spawn_prefab(effect.expect("BlasterboltBehaviour: Instantiate(null ImpactEffect) (ArgumentException)"), at);
        self.tr_set_up(Xf::Object(effect_id), [normal[0], normal[1], 0.0]);
        if create_decal {
            // Instantiate(HitDecal, hit.point, identity, hit.transform): it follows hit.transform's GameObject
            // (`update_attached_clones`) and goes with it.
            let decal_id = self.spawn_prefab(decal.expect("BlasterboltBehaviour: Instantiate(null HitDecal) (ArgumentException)"), at);
            let parent = match self.hit_transform_physical(hit) {
                Some(p) => super::destroyable::AttachParent::Physical(p),
                None => super::destroyable::AttachParent::Bare(hit.collider),
            };
            self.attach_clone(decal_id, parent, at);
        }
        self.create_pulse_explosion_with(at, strength, 1.0, !under_water && spawn_fx, false);
        if explode {
            let under_water = self.is_point_under_water(point);
            let ex = ExplosionParameters {
                FragmentationRayCount: 8,
                Position: at,
                Range: 4.0,
                FragmentForce: explosion_damage,
                CreateParticlesAndSound: false,
                EffectSize: super::explosion::EffectSize::Medium,
                DismemberChance: 0.2,
                BallisticShrapnelCount: 0,
            };
            self.create_explosion_with_water(under_water, ex);
        }
        // PhysicalObjectsInWorldByTransform.TryGetValue(hit.transform): Temperature = Lerp(Temperature, target, ...).
        let physical = self.hit_transform_physical(hit);
        if let Some(p) = self.hit_world_physical(hit).filter(|&p| self.physicals[p].SimulateTemperature) {
            let t = self.physicals[p].Temperature;
            self.physicals[p].Temperature = mathf::lerp(t, target, 0.015 / self.heat_capacity(p));
        }
        // hit.transform.CompareTag("Limb") && TryGetComponent<SkinMaterialHandler>: the limbs.
        if let Some(l) = self.physical_of_collider(hit.collider).and_then(|p| self.limbs.iter().position(|l| l.PhysicalBehaviour == p)) {
            self.skin_add_damage_point(l, super::misc::damage_type::BURN, point, target * 0.01);
        }
        self.send_shot(hit.collider, physical, normal, point, damage, false);
        if let Some(ty) = impact_decal {
            // calculateDecalSize(damage): Max(Pow(damage, 3.35) / Pow(10, 8), 0.15) * DecalSizeMultiplier
            let size = mathf::max(mathf::pow(damage, 3.35) / mathf::pow(10.0, 8.0), 0.15) * decal_multiplier;
            // SendMessage("Decal", DecalInstruction(ImpactDecal, point, size)) to hit.transform.
            match physical.or_else(|| self.physical_of_collider(hit.collider)) {
                Some(p) => self.physical_decal(p, ty, point, size, [1.0; 4]),
                None if self.bare_colliders[&hit.collider].receives(&["DecalControllerBehaviour", "DecalRenderer", "DecalRendererController"]) => {
                    unimplemented!("SendMessage Decal to the scripts of a collider without a PhysicalBehaviour")
                }
                None => {}
            }
        }
        let s = &mut self.blasterbolts[b];
        s.closing = true;
        s.closed_at = self.time.timeAsDouble;
    }
}
