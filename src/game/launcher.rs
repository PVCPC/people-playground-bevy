//! ProjectileLauncherBehaviour (the Crossbow, the launchers, the Ion Cannon and the Stunner).
#![allow(non_snake_case)]

use super::destroyable::{Coroutine, Step};
use super::spawn::Instance;
use super::{d, mathf, script, Game};
use crate::assets::db::{Component, ObjRef};

script! {
    Launcher {
        barrelPosition: [f32; 2] = [0.0; 2],
        barrelDirection: [f32; 2] = [0.0; 2],
        IsAutomatic: bool = false,
        AutomaticInterval: f32 = 0.0,
        projectileLaunchStrength: f32 = 0.0,
        recoilMultiplier: f32 = 1.0,
        ScreenShake: f32 = 0.0,
        ShouldPutProjectileInOtherLayer: bool = false,
        LayerToPutProjectileOn: i32 = -1,
        RemoveLaunchedObjectsWithMe: bool = false,
    }
    links {
        /// The component's instance id (its coroutines' owner).
        id: i32 = 0,
        enabled: bool = true,
        /// phys and rb (GetComponent in Awake).
        physical: usize = 0,
        /// projectileAsset's Prefab and name, else projectilePrefab with None; None for a null prefab.
        projectile: Option<(ObjRef, Option<String>)> = None,
        /// muzzleFlashPrefab and whether it is tagged Poolable.
        muzzle_flash: Option<(ObjRef, bool)> = None,
        /// launchSound is set (LaunchClips is then not drawn from).
        launch_sound: bool = false,
        /// LaunchClips length.
        LaunchClips: usize = 0,
        automaticTimer: f32 = 0.0,
        /// linkedObjects: the launched roots' Transform ids.
        linkedObjects: Vec<i32> = Vec::new(),
    }
}

/// Quaternion operator* (managed), each component one double chain.
fn qmul(l: [f32; 4], r: [f32; 4]) -> [f32; 4] {
    let [lx, ly, lz, lw] = l;
    let [rx, ry, rz, rw] = r;
    [
        (d(lw) * rx + d(lx) * rw + d(ly) * rz - d(lz) * ry).f(),
        (d(lw) * ry + d(ly) * rw + d(lz) * rx - d(lx) * rz).f(),
        (d(lw) * rz + d(lz) * rw + d(lx) * ry - d(ly) * rx).f(),
        (d(lw) * rw - d(lx) * rx - d(ly) * ry - d(lz) * rz).f(),
    ]
}

impl Game {
    /// ProjectileLauncherBehaviour.Use
    pub fn launcher_use(&mut self, l: usize) {
        if self.launchers[l].enabled {
            self.launchers[l].automaticTimer = 0.0;
            self.launcher_launch(l);
        }
    }

    /// ProjectileLauncherBehaviour.Update
    pub fn launcher_update(&mut self, l: usize) {
        let x = &self.launchers[l];
        if self.used_continuously(x.physical) && x.IsAutomatic {
            self.launchers[l].automaticTimer += self.time.deltaTime;
        }
    }

    /// ProjectileLauncherBehaviour.FixedUpdate
    pub fn launcher_fixed_update(&mut self, l: usize) {
        let x = &self.launchers[l];
        if x.IsAutomatic && x.automaticTimer > x.AutomaticInterval {
            self.launchers[l].automaticTimer = 0.0;
            self.launcher_launch(l);
        }
    }

    /// ProjectileLauncherBehaviour.Launch: StartCoroutine(LaunchRoutine()) up to its WaitForFixedUpdate.
    pub fn launcher_launch(&mut self, l: usize) {
        self.coroutine_seq += 1;
        let owner = self.launchers[l].id;
        self.fixed_update_waits.push(Coroutine { seq: self.coroutine_seq, at: self.time.timeAsDouble, owner, step: Step::LaunchRoutine(l) });
    }

    /// GetBarrelPosition: transform.TransformPoint(barrelPosition), as a Vector2.
    fn launcher_barrel_position(&self, l: usize) -> [f32; 3] {
        let b = self.launchers[l].barrelPosition;
        let p = self.transform_point3(self.launchers[l].physical, [b[0], b[1], 0.0]);
        [p[0], p[1], 0.0]
    }

    /// GetBarrelDirection: transform.TransformDirection(barrelDirection) * transform.localScale.x.
    fn launcher_barrel_direction(&self, l: usize) -> [f32; 2] {
        let p = self.launchers[l].physical;
        let b = self.launchers[l].barrelDirection;
        let v = self.transform_direction(p, [b[0], b[1], 0.0]);
        let s = self.physicals[p].transform.localScale[0];
        [(d(v[0]) * s).f(), (d(v[1]) * s).f()]
    }

    /// LaunchRoutine after its WaitForFixedUpdate.
    pub fn launcher_launch_routine(&mut self, l: usize) {
        let p = self.launchers[l].physical;
        let t = self.physicals[p].transform.rotation;
        let rotation = if self.lossy_scale(p)[0] < 0.0 {
            // Quaternion.Euler(0f, 0f, 180f): EulerToQuaternion (18073d0c0) with the CRT's sinf/cosf.
            let z = 180.0 * mathf::DEG2RAD * 0.5;
            qmul([0.0, 0.0, crate::unity::sinf(z), crate::unity::cosf(z)], t)
        } else {
            t
        };
        let position = self.launcher_barrel_position(l);
        let (source, asset) = self.launchers[l].projectile.clone().unwrap_or_else(|| unimplemented!("ProjectileLauncherBehaviour: Instantiate(null) (ArgumentException)"));
        let mut db = self.assets.take().expect("ProjectileLauncherBehaviour without the game's data");
        let spawned = db.prefab(source).and_then(|prefab| {
            let (inst, linked) = self.spawn(&mut db, &prefab, position, rotation, None)?;
            if let Some(name) = &asset {
                // SpawnSpawnParticles = false before its Start; AddComponent AudioSourceTimeScaleBehaviour,
                // TexturePackApplier and SerialiseInstructions; gameObject.name.
                if let Some(&q) = linked.physicals.get(&0) {
                    self.physicals[q].SpawnSpawnParticles = false;
                }
                self.spawnable_asset_components(&prefab, &linked, name, 3);
            }
            Ok((inst, linked))
        });
        self.assets = Some(db);
        let (inst, linked) = spawned.unwrap_or_else(|e| panic!("ProjectileLauncherBehaviour: Instantiate: {e}"));
        let x = &self.launchers[l];
        if x.ShouldPutProjectileInOtherLayer {
            let layer = x.LayerToPutProjectileOn;
            self.set_root_layer(&inst, layer);
        }
        // OnLaunch: only JavelinBehaviour subscribes (not rewritten); OnLaunchEvent is presentation (link checks).
        if let Some(&q) = linked.physicals.get(&0) {
            if self.physicals[q].SimulateTemperature && self.physicals[p].SimulateTemperature {
                self.physicals[q].Temperature = self.physicals[p].Temperature;
            }
            let id = self.launchers[l].id;
            let a = self.with_component_instance(id, |_, inst, o| super::link::colliders_in_children(inst, o));
            let b = super::link::colliders_in_children(&inst, 0);
            for &(ca, _) in &a {
                for &(cb, _) in &b {
                    self.ignore_collision_substitute(ca, cb, true);
                }
            }
            let direction = self.launcher_barrel_direction(l);
            let k = self.launchers[l].projectileLaunchStrength;
            let rb = self.physicals[q].rigidbody.clone();
            rb.add_force_impulse(&mut self.world, [(d(direction[0]) * k).f(), (d(direction[1]) * k).f()]);
        }
        // rb.AddForce((0f - projectileLaunchStrength) * recoilMultiplier * gameObject.transform.right, Impulse)
        let x = &self.launchers[l];
        let k = (d(0.0) - x.projectileLaunchStrength).0 * x.recoilMultiplier as f64;
        let right = mathf::rotate(inst.rotation(0), [1.0, 0.0, 0.0]);
        let rb = self.physicals[p].rigidbody.clone();
        rb.add_force_impulse(&mut self.world, [(k as f32 as f64 * right[0] as f64) as f32, (k as f32 as f64 * right[1] as f64) as f32]);
        if self.launchers[l].RemoveLaunchedObjectsWithMe {
            let root = inst.objects[0].components[0];
            self.launchers[l].linkedObjects.push(root);
        }
        self.unsynced.push(inst);
        // The clip: launchSound, else LaunchClips.PickRandom(); playing it is audio.
        if !self.launchers[l].launch_sound {
            self.pick_random(self.launchers[l].LaunchClips);
        }
        let at = self.physicals[p].transform.position;
        let shake = self.launchers[l].ScreenShake;
        self.camera_shake(shake, [at[0], at[1]]);
        // The muzzle flash: its pose is presentation (link checks it has no physics).
        match self.launchers[l].muzzle_flash {
            None => {}
            Some((effect, true)) => self.request_pooled_effect(effect, "ProjectileLauncherBehaviour.muzzleFlashPrefab"),
            Some((effect, false)) => {
                let at = self.launcher_barrel_position(l);
                self.spawn_prefab(effect, at);
            }
        }
    }

    /// GameObject.layer = layer on a clone's root: kLayerChanged to its components in order. Collider2D's handler
    /// (180c0cfb0) is RecalculateContacts (`refilter_collider`) and Rigidbody2D's (180c1cdc0) wakes its body; the
    /// others' are rendering.
    fn set_root_layer(&mut self, inst: &Instance, layer: i32) {
        if !(0..32).contains(&layer) {
            unimplemented!("GameObject.layer = {layer} (outside [0...31]: an error and no change)");
        }
        self.world.layers.insert(inst.objects[0].game_object, layer);
        let prefab = inst.prefab.clone();
        for (c, (_, comp)) in prefab.objects[0].components.iter().enumerate() {
            match comp {
                Component::Collider2D(_) => self.world.refilter_collider(inst.objects[0].components[c]),
                Component::Rigidbody2D(_) => {
                    if let Some(&b) = inst.bodies.get(&0) {
                        self.world.wake_body(b);
                    }
                }
                _ => {}
            }
        }
    }

    /// ProjectileLauncherBehaviour.OnDestroy: Object.Destroy of each linked object.
    pub fn launcher_destroy(&mut self, id: i32) {
        let l = self.launchers.iter().position(|x| x.id == id).expect("the ProjectileLauncherBehaviour");
        for root in self.launchers[l].linkedObjects.clone() {
            self.destroy_object(root);
        }
    }
}
