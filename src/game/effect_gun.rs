//! EffectGunBehaviour and its two subclasses, HealthGunBehaviour (the Reconstructor) and FreezeGunBehaviour (the
//! Rigidifier): Use fires an OverlapCircleNonAlloc in front of the barrel and Affect runs on every collider found.
//! The AudioSource's settings, the shot sound and the muzzle flash's effects are presentation.
#![allow(non_snake_case)]

use super::container::Owner;
use super::explosion::DEFAULT_RAYCAST_LAYERS;
use super::limb::HEAD;
use super::translated::rt::Xf;
use super::{d, script, Game};
use crate::assets::db::ObjRef;
use crate::b2::math::Vec2 as V;

/// EffectGunBehaviour.buffer's length.
const BUFFER: usize = 16;

script! {
    EffectGun {
        barrelPosition: [f32; 3] = [0.0; 3],
        barrelDirection: [f32; 3] = [0.0; 3],
        range: f32 = 1.0,
        offset: f32 = 1.0,
        /// FreezeGunBehaviour.SkinColour
        SkinColour: [f32; 4] = [0.0; 4],
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        enabled: bool = true,
        /// PhysicalBehaviour, the gun's own (link checks).
        physical: usize = 0,
        /// FreezeGunBehaviour, else HealthGunBehaviour.
        freeze: bool = false,
        /// Muzzleflash: instantiated by Use; its pose is the script's own (link checks it has no physics).
        Muzzleflash: Option<ObjRef> = None,
        /// ShotSounds.Length
        ShotSounds: usize = 0,
    }
}

impl Game {
    /// BarrelPosition: transform.TransformPoint(barrelPosition).
    fn effect_gun_barrel_position(&self, i: usize) -> [f32; 2] {
        let s = &self.effect_guns[i];
        let p = self.transform_point3(s.physical, s.barrelPosition);
        [p[0], p[1]]
    }

    /// BarrelDirection: transform.TransformDirection(barrelDirection) * transform.localScale.x.
    fn effect_gun_barrel_direction(&self, i: usize) -> [f32; 2] {
        let s = &self.effect_guns[i];
        let v = self.transform_direction(s.physical, s.barrelDirection);
        let k = self.physicals[s.physical].transform.localScale[0];
        [v[0] * k, v[1] * k]
    }

    /// EffectGunBehaviour.Awake: AddComponent<AudioSource>() takes an instance id; its settings are audio.
    pub fn effect_gun_awake(&mut self) {
        self.allocate_instance_id();
    }

    /// EffectGunBehaviour.Use: Instantiate(Muzzleflash, BarrelPosition, transform.rotation).transform.right =
    /// BarrelDirection, then Shoot.
    pub fn effect_gun_use(&mut self, i: usize) {
        if !self.effect_guns[i].enabled {
            return;
        }
        let position = self.effect_gun_barrel_position(i);
        let rotation = self.physicals[self.effect_guns[i].physical].transform.rotation;
        let flash = self.effect_guns[i].Muzzleflash.unwrap_or_else(|| unimplemented!("EffectGunBehaviour: Instantiate(null Muzzleflash) (ArgumentException)"));
        let id = self.spawn_prefab_rotated(flash, [position[0], position[1], 0.0], rotation);
        let direction = self.effect_gun_barrel_direction(i);
        self.tr_set_right(Xf::Object(id), [direction[0], direction[1], 0.0]);
        self.effect_gun_shoot(i);
    }

    /// EffectGunBehaviour.Shoot: PlayOneShot(ShotSounds.PickRandom()) (audio, the draw stays), then Affect on each
    /// collider OverlapCircleNonAlloc puts in the buffer (the first 16 of the query's results).
    fn effect_gun_shoot(&mut self, i: usize) {
        self.pick_random(self.effect_guns[i].ShotSounds);
        let position = self.effect_gun_barrel_position(i);
        let direction = self.effect_gun_barrel_direction(i);
        let s = &self.effect_guns[i];
        let (offset, range) = (s.offset, s.range);
        let center = V::new(position[0] + direction[0] * offset, position[1] + direction[1] * offset);
        let mut hits = self.overlap_circle(center, range, DEFAULT_RAYCAST_LAYERS);
        hits.truncate(BUFFER);
        for (physical, collider) in hits {
            // TryGetLimb: coll.transform.gameObject.TryGetComponent<LimbBehaviour>(), the collider's own GameObject.
            let Some(l) = physical.and_then(|p| self.limbs.iter().position(|x| x.PhysicalBehaviour == p)) else { continue };
            if self.world.collider_game_object(collider) != Some(self.limb_object(l).0) || self.limbs[l].IsAndroid {
                continue;
            }
            if self.effect_guns[i].freeze {
                self.freeze_gun_affect(i, l);
            } else {
                self.health_gun_affect(l);
            }
        }
    }

    /// FreezeGunBehaviour.Affect on a non-android limb (the Color setter's SpriteRenderer colour is rendering).
    fn freeze_gun_affect(&mut self, i: usize, l: usize) {
        self.limbs[l].Frozen = true;
        self.limbs[l].Color = self.effect_guns[i].SkinColour;
    }

    /// HealthGunBehaviour.Affect on a non-android limb.
    fn health_gun_affect(&mut self, l: usize) {
        self.limb_heal_bone(l);
        self.limbs[l].Health = self.limbs[l].InitialHealth;
        self.heal_bleeding(l);
        let acid = self.skins[l].acidProgress;
        self.skins[l].set_acid((d(acid) * 0.25f32).f());
        self.skins[l].set_rotten(0.0);
        let p = self.limbs[l].PhysicalBehaviour;
        self.physicals[p].BurnProgress = (d(self.physicals[p].BurnProgress) * 0.8f32).f();
        self.limbs[l].Numbness = 0.0;
        let c = &mut self.circulations[l];
        c.IsPump = c.WasInitiallyPumping;
        c.BloodFlow = 1.0;
        c.InternalBleedingIntensity = (d(c.InternalBleedingIntensity) * 0.5f32).f();
        self.clear_liquid(Owner::Circulation(l));
        let blood = self.blood_type(l);
        self.add_liquid(Owner::Circulation(l), blood, 1.0);
        self.limbs[l].Frozen = false;
        if self.limbs[l].RoughClassification == HEAD {
            let person = &mut self.people[self.limbs[l].Person];
            person.ShockLevel = 0.0;
            person.PainLevel = 0.0;
            person.AdrenalineLevel = 1.0;
            person.Consciousness = (d(person.Consciousness) + 0.5f32).f();
            person.Braindead = false;
            if self.random.value() > 0.95 {
                self.people[self.limbs[l].Person].BrainDamaged = false;
            }
        }
    }
}
