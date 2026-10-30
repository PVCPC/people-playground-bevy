//! DestroyableBehaviour, and Object.Destroy of a spawned GameObject.
#![allow(non_snake_case)]

use super::{d, mathf, script, BodyOwner, Game};
use crate::assets::db::{Component, ObjRef};
use crate::b2::contacts2d::ScriptCollision;

script! {
    Destroyable {
        OverallChance: f32 = 1.0,
        ShotBreakChance: f32 = 1.0,
        MinimumImpactForce: f32 = 5.0,
        MinimumShotDamage: f32 = 15.0,
        MinimumExplosionFragmentDamage: f32 = 2.0,
        ReactToCollision: bool = true,
        ReactToBullets: bool = true,
        ReactToExplosions: bool = true,
        RestoreIncomingVelocityWhenDestroyedRatio: f32 = 0.5,
        TemperatureLimit: f32 = f32::INFINITY,
        DestroyWhenBroken: bool = true,
        DeactivateWhenBroken: bool = false,
        TransferUndoRegistry: bool = true,
        IndestructibilityCanBeToggled: bool = true,
        Indestructible: bool = false,
        broken: bool = false,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// GetComponent<PhysicalBehaviour>()
        PhysicalBehaviour: usize = 0,
        DebrisPrefab: Option<ObjRef> = None,
        /// The physical whose Rigidbody2D VelocityAncestor is.
        VelocityAncestor: Option<usize> = None,
        /// OnBreak's persistent calls reach only audio and particle components.
        OnBreakPresentation: bool = true,
        /// The component comes after the PhysicalBehaviour on its GameObject (SendMessage order).
        after_physical: bool = true,
        /// The component's index on its GameObject.
        index: usize = 0,
    }
}

/// Vector2.ClampMagnitude: sqrMagnitude is stored as a float, the comparison and the division are Mono doubles.
fn clamp_magnitude(v: [f32; 2], max: f32) -> [f32; 2] {
    let sqr = (d(v[0]) * v[0] + d(v[1]) * v[1]).f();
    if (sqr as f64) > d(max).0 * max as f64 {
        let n = (sqr as f64).sqrt() as f32;
        let (x, y) = (v[0] / n, v[1] / n);
        return [x * max, y * max];
    }
    v
}

impl Game {
    /// DestroyableBehaviour.Start: a broken one deactivates itself; the context menu button is UI.
    pub fn destroyable_start(&mut self, x: usize) {
        if self.destroyables[x].broken {
            let id = self.destroyables[x].id;
            self.with_component_instance(id, |g, inst, o| g.set_active_scripts(inst, o, false));
        }
    }

    /// The ExplosiveBehaviour on the destroyable's object (TryGetComponent<ExplosiveBehaviour>).
    pub(super) fn destroyable_explosive(&self, x: usize) -> Option<usize> {
        let p = self.destroyables[x].PhysicalBehaviour;
        self.explosives.iter().position(|e| e.physical == Some(p))
    }

    /// DestroyableBehaviour.Update
    pub fn destroyable_update(&mut self, x: usize) {
        let s = &self.destroyables[x];
        let p = &self.physicals[s.PhysicalBehaviour];
        if p.SimulateTemperature && p.Temperature >= s.TemperatureLimit && !self.destroyable_explosive(x).is_some_and(|e| self.explosives[e].TemperatureLimit >= p.Temperature) {
            self.destroyable_break(x, [0.0, 0.0]);
        }
    }

    /// DestroyableBehaviour.OnFragmentHit
    pub fn destroyable_on_fragment_hit(&mut self, x: usize, force: f32) {
        let s = &self.destroyables[x];
        if s.ReactToExplosions && !(force < s.MinimumExplosionFragmentDamage) {
            match self.destroyable_explosive(x) {
                Some(e) if self.explosives[e].ExplodesOnFragmentHit => self.explosive_force_immediate(e),
                _ => self.destroyable_break(x, [0.0, 0.0]),
            }
        }
    }

    /// DestroyableBehaviour.OnCollisionEnter2D / OnCollisionStay2D: EvaluateCollision.
    pub fn destroyable_on_collision(&mut self, x: usize, c: &ScriptCollision) {
        let s = &self.destroyables[x];
        if !s.ReactToCollision {
            return;
        }
        // collision.GetContacts(buffer[8]), Utils.GetMinImpulse (NaN skipped, float.MaxValue without contacts).
        let min = c.contacts.iter().take(8).map(|p| p.normal_impulse).filter(|i| !i.is_nan()).fold(f32::MAX, mathf::min);
        if min < s.MinimumImpactForce {
            return;
        }
        if let Some(e) = self.destroyable_explosive(x).filter(|&e| self.explosives[e].ImpactForceThreshold > 0.0) {
            self.explosive_force_immediate(e);
            return;
        }
        let ratio = self.destroyables[x].RestoreIncomingVelocityWhenDestroyedRatio;
        if let Some(o) = self.physical_of_collider(c.collider).filter(|&o| self.physicals[o].rigidbody.bodyType == 0) {
            let rb = self.physicals[o].rigidbody.clone();
            let (v, prev) = (rb.velocity(&self.world), self.physicals[o].previousVelocity);
            rb.set_velocity(&mut self.world, [mathf::lerp(v[0], prev[0], ratio), mathf::lerp(v[1], prev[1], ratio)]);
        }
        let own = c.other_rigidbody.unwrap_or_else(|| unimplemented!("NullReferenceException: DestroyableBehaviour collision without otherRigidbody"));
        let p = self.physicals.iter().position(|p| !p.destroyed && p.rigidbody.component == own).unwrap_or_else(|| unimplemented!("DestroyableBehaviour: otherRigidbody {own} is not a physical's"));
        let v = self.physicals[p].rigidbody.velocity(&self.world);
        self.destroyable_break(x, [v[0] - c.relative_velocity.x, v[1] - c.relative_velocity.y]);
    }

    /// DestroyableBehaviour.Break(velocity)
    pub fn destroyable_break(&mut self, x: usize, velocity: [f32; 2]) {
        let s = &self.destroyables[x];
        if s.Indestructible || s.broken || s.OverallChance < self.random.value() {
            return;
        }
        self.destroyables[x].broken = true;
        let s = &self.destroyables[x];
        if !s.OnBreakPresentation {
            unimplemented!("DestroyableBehaviour.OnBreak: a persistent call beyond audio and particles");
        }
        let source = s.DebrisPrefab.unwrap_or_else(|| unimplemented!("DestroyableBehaviour: Instantiate(null DebrisPrefab) (ArgumentException)"));
        let pose = self.physicals[s.PhysicalBehaviour].transform.clone();
        let mut db = self.assets.take().expect("DestroyableBehaviour.Break without the game's data");
        let spawned = db.prefab(source).and_then(|prefab| self.spawn(&mut db, &prefab, pose.position, pose.rotation, None));
        self.assets = Some(db);
        let (inst, linked) = spawned.unwrap_or_else(|e| panic!("DestroyableBehaviour: Instantiate: {e}"));
        self.destroyable_on_debris_created(x, inst, linked, velocity);
    }

    /// DestroyableBehaviour.OnDebrisCreated
    fn destroyable_on_debris_created(&mut self, x: usize, mut inst: super::spawn::Instance, linked: super::link::Linked, velocity: [f32; 2]) {
        // GetOrAddComponent<DebrisComponent>: a new one takes an instance id (it has no messages).
        let has_debris = inst.prefab.objects[0].components.iter().any(|(_, c)| matches!(c, Component::Script { class, .. } if class == "DebrisComponent"));
        if !has_debris {
            self.allocate_instance_id();
        }
        let s = &self.destroyables[x];
        let p = s.PhysicalBehaviour;
        // createdDebris.transform.localScale = base.transform.lossyScale
        let scale = if self.physicals[p].parent.is_some() { self.lossy_scale(p) } else { self.physicals[p].transform.localScale };
        inst.set_local_scale(0, scale);
        self.refresh_transforms(&inst, &linked);
        self.unsynced.push(inst);
        if let Some(a) = self.destroyables[x].VelocityAncestor {
            self.destroyable_apply_ancestor_properties(a, p, velocity, &linked);
        }
        let s = &self.destroyables[x];
        let id = s.id;
        if s.DeactivateWhenBroken {
            self.with_component_instance(id, |g, inst, o| g.set_active_scripts(inst, o, false));
        } else if s.DestroyWhenBroken {
            self.destroy_object(id);
        } else {
            unimplemented!("DestroyableBehaviour: AddComponent<ActOnDestroy>");
        }
        // TransferUndoRegistry: the undo history is UI.
    }

    /// DestroyableBehaviour.ApplyAncestorProperties over the debris' active Rigidbody2Ds and PhysicalBehaviours
    /// (GetComponentsInChildren: hierarchy order).
    fn destroyable_apply_ancestor_properties(&mut self, ancestor: usize, original: usize, velocity: [f32; 2], debris: &super::link::Linked) {
        let mut parts: Vec<(usize, usize)> = debris.physicals.iter().map(|(&o, &p)| (o, p)).filter(|&(_, p)| self.physicals[p].active).collect();
        parts.sort();
        // The active Rigidbody2Ds (an inactive object's has no body), physical or not.
        let mut bodies: Vec<(usize, BodyOwner)> = parts.iter().map(|&(o, p)| (o, BodyOwner::Physical(p))).collect();
        bodies.extend(debris.node_rigidbodies.iter().filter(|&(_, &k)| self.node_rigidbodies[k].rigidbody.detached.is_none()).map(|(&o, &k)| (o, BodyOwner::Node(k))));
        bodies.sort_by_key(|&(o, _)| o);
        let arb = self.physicals[ancestor].rigidbody.clone();
        for &(_, owner) in &bodies {
            // GetPointVelocity starts with the AutoSyncTransforms.
            self.sync_pending();
            let pos = match owner {
                BodyOwner::Physical(p) => self.physicals[p].transform.position,
                BodyOwner::Node(k) => self.node_world(self.node_rigidbodies[k].node).0,
            };
            let pv = arb.point_velocity(&self.world, [pos[0], pos[1]]);
            let max = mathf::magnitude2(arb.velocity(&self.world));
            let v = clamp_magnitude([pv[0] + velocity[0], pv[1] + velocity[1]], max);
            let rb = self.owner_rigidbody(owner).clone();
            rb.set_velocity(&mut self.world, v);
            let w = arb.angular_velocity(&self.world) * 0.2;
            rb.set_angular_velocity(&mut self.world, w);
            // AddComponent<AudioSourceTimeScaleBehaviour>: audio registration only, but it takes an instance id.
            self.allocate_instance_id();
        }
        let n = parts.len() as i32;
        let o = &self.physicals[original];
        let (burn, on_fire, charge, intensity, simulate, temperature) = (o.BurnProgress, o.OnFire, o.charge, o.burnIntensity, o.SimulateTemperature, o.Temperature);
        for &(_, p) in &parts {
            let ph = &mut self.physicals[p];
            ph.BurnProgress = burn;
            if on_fire {
                unimplemented!("DestroyableBehaviour.ApplyAncestorProperties: PhysicalBehaviour.Ignite");
            }
            ph.charge = charge / 1.max(n / 4) as f32;
            ph.burnIntensity = intensity / n as f32;
            if ph.SimulateTemperature && simulate {
                ph.Temperature = temperature;
            }
        }
    }

    /// Object.Destroy(gameObject) of the object holding component `id`: DestroyObjectDelayed queues it at
    /// min(fixedTimeAsDouble, the frame's time); a second Destroy finds it queued.
    pub fn destroy_object(&mut self, id: i32) {
        self.destroy_object_after(id, 0.0);
    }

    /// Object.Destroy(gameObject, delay). A destroyed object reads null, and `(bool)gameObject` checks skip it.
    pub fn destroy_object_after(&mut self, id: i32, delay: f32) {
        if !self.destroyed_components.contains(&id) && !self.object_destroys.iter().any(|&(_, i)| i == id) {
            let at = self.time.fixedTimeAsDouble.min(self.time.timeAsDouble) + delay as f64;
            self.object_destroys.push((at, id));
        }
    }

    /// The queued Object.Destroy calls due by now, as (time, component id).
    pub fn object_due_destroys(&self, now: f64) -> Vec<(f64, i32)> {
        self.object_destroys.iter().copied().filter(|&(at, _)| at <= now).collect()
    }

    /// The limb whose GameObject has the Transform `key`.
    fn limb_of_object(&self, key: i32) -> usize {
        self.limbs.iter().position(|l| self.physicals[l.PhysicalBehaviour].transform_id == key).expect("the object's LimbBehaviour")
    }

    /// DestroyObjectHighLevel of the object holding component `id`: its hierarchy deactivates (OnDisable, the
    /// Physics2D cleanup), then each script gets OnDestroy, the object's components first, then its descendants.
    // ponytail: OnDestroy order across a hierarchy is parent-first pre-order; no rewritten OnDestroy reads another's state.
    pub fn run_object_destroy(&mut self, id: i32) {
        self.object_destroys.retain(|&(_, i)| i != id);
        if self.destroyed_components.contains(&id) {
            return;
        }
        let root = self.with_component_instance(id, |g, inst, o| {
            let prefab = inst.prefab.clone();
            let objects: Vec<usize> = std::iter::once(o).chain(inst.descendants(o)).collect();
            for &x in &objects {
                g.destroyed_components.extend(inst.objects[x].components.iter().copied());
            }
            // Its coroutines stop.
            let gone = &g.destroyed_components;
            g.coroutines.retain(|c| !gone.contains(&c.owner));
            g.end_of_frame.retain(|c| !gone.contains(&c.owner));
            g.fixed_update_waits.retain(|c| !gone.contains(&c.owner));
            for &x in &objects {
                for &c in &inst.objects[x].components {
                    g.stop_all_coroutines(c);
                }
            }
            g.set_active_scripts(inst, o, false);
            for &x in &objects {
                let key = inst.objects[x].components[0];
                for k in 0..prefab.objects[x].components.len() {
                    let Component::Script { class, .. } = &prefab.objects[x].components[k].1 else { continue };
                    let cid = inst.objects[x].components[k];
                    match class.as_str() {
                        "PhysicalBehaviour" => {
                            let p = g.physicals.iter().position(|p| p.id == cid).expect("a PhysicalBehaviour in the arena");
                            // DestroyStabJoint for each penetration; PhysicalObjectsInWorld(ByTransform) and the camera
                            // list drop it, so it leaves every manager loop.
                            g.stab_destroy(p);
                            g.physicals[p].enabled = false;
                            g.physicals[p].destroyed = true;
                        }
                        "ActOnDestroy" => g.act_on_destroy_run(key),
                        "NoCollide" => g.no_collide_destroy(cid),
                        "NoCollideGm" => g.no_collide_gm_destroy(cid, inst),
                        "TyreDeflationBehaviour" => g.tyre_destroy(cid),
                        // Global.RemoveAudioSource
                        "ExplosionSoundBehviour" | "AudioSourceTimeScaleBehaviour" | "ExistInDetailView" => {}
                        // DeregisterBehaviour.OnDestroy: FindRelevantAction / DeregisterAction of this object.
                        "DeregisterBehaviour" => g.undo_deregister_related(key),
                        // DetectorBehaviour.OnDestroy removes its ContinuousActivationBehaviour listener, which sends nothing received.
                        "DetectorBehaviour" => {}
                        // BulbBehaviour.OnDestroy: Object.Destroy(OtherPart), a child (destroyed with the hierarchy already) until Break detaches it.
                        "BulbBehaviour" => {}
                        // FirearmAttachmentPointBehaviour.OnDestroy: Object.Destroy(FirearmAttachment); Start rejects a prefab with one.
                        "FirearmAttachmentPointBehaviour" => {}
                        // GetComponentsInChildren on the destroyed (inactive) GameObject finds no collider.
                        "NoChildrenCollide" => {}
                        // PersonBehaviour.OnDestroy: AliveBehaviour.AliveByTransform.Remove, a lookup nothing rewritten reads.
                        "PersonBehaviour" => {
                            let i = g.people.iter().position(|p| p.id == cid).expect("the PersonBehaviour");
                            g.people[i].destroy_at = None;
                            g.people[i].destroyed = true;
                        }
                        // LimbBehaviour.OnDestroy: LimbBehaviourManager.Limbs.Remove (Destroy(myStatus) is UI, DestroyWith only
                        // filled by attachments). Its joints and bleeding particles go with the GameObject.
                        "LimbBehaviour" => {
                            let l = g.limb_of_object(key);
                            g.limbs[l].enabled = false;
                            g.limb_destroy_joints(l);
                        }
                        // ConnectedNodeBehaviour.OnDestroy: DisconnectFromEverything, then NodeController.Nodes.Remove.
                        "ConnectedNodeBehaviour" => {
                            let l = g.limb_of_object(key);
                            g.node_disconnect_from_everything(l);
                        }
                        // GoreStringBehaviour.OnDestroy: DestroyJoint.
                        "GoreStringBehaviour" => {
                            let l = g.limb_of_object(key);
                            if g.limbs[l].goreString {
                                g.gore_string_destroy_joint(l);
                            }
                        }
                        // SkinMaterialHandler.OnDestroy: Destroy(material), rendering. GripBehaviour.OnDestroy: modding listener.
                        // DisintegrationCounterBehaviour.OnDestroy: its OnDisintegration handlers go with the objects.
                        "SkinMaterialHandler" | "GripBehaviour" | "DisintegrationCounterBehaviour" => {}
                        "FirearmBehaviour" => {
                            let f = g.firearms.iter().position(|x| x.id == cid).expect("the FirearmBehaviour");
                            g.emitter_dispose(super::ballistics::Shooter::Firearm(f));
                        }
                        "ProjectileLauncherBehaviour" => g.launcher_destroy(cid),
                        "MachineGunBehaviour" => {
                            let m = g.machine_guns.iter().position(|x| x.id == cid).expect("the MachineGunBehaviour");
                            g.emitter_dispose(super::ballistics::Shooter::MachineGun(m));
                        }
                        name if super::translated::CLASSES.contains(&name) => {
                            // Only an awakened component (in behaviour_of) gets OnDestroy.
                            if let Some(&(_, super::Behaviour::Translated(t))) = g.behaviour_of.get(&cid) {
                                g.tr_on_destroy(t);
                            }
                        }
                        c if !super::declares(c, "OnDestroy") => {}
                        c => unimplemented!("{c}.OnDestroy"),
                    }
                    g.behaviour_of.remove(&cid);
                }
                // An ActOnDestroy added at run time is the object's last component.
                if g.act_on_destroy.contains_key(&key) {
                    g.act_on_destroy_run(key);
                }
                // The DeregisterBehaviour CatalogBehaviour.Spawn added to its root.
                if g.deregister_behaviours.remove(&key) {
                    g.undo_deregister_related(key);
                }
            }
            inst.objects.iter().all(|x| g.destroyed_components.contains(&x.components[0]))
        });
        // The whole clone went: nothing can reach its objects any more.
        if root {
            let owns = |i: &super::spawn::Instance| i.objects.iter().any(|x| x.components.contains(&id));
            self.unsynced.retain(|i| !owns(i));
            self.retained.retain(|i| !owns(i));
        }
    }
}

/// ActOnDestroy on a GameObject (by its Transform's instance id): from the prefab, or added by GetOrAddComponent.
#[derive(Clone, Debug, Default)]
pub struct ActOnDestroy {
    /// Event's runtime listeners, in AddListener order.
    pub listeners: Vec<Listener>,
    /// DestroyWith: a component id on each object.
    pub destroy_with: Vec<i32>,
}

#[derive(Clone, Copy, Debug)]
pub enum Listener {
    /// DestroyWhenAllChildrenDestroyed.Check of this one.
    Check(usize),
    /// DetachFromParentOnStart.DestroyThis of the object holding this component.
    DestroyThis(i32),
}

/// DestroyWhenAllChildrenDestroyed: its own component id and the count Check counts down.
#[derive(Clone, Debug)]
pub struct DestroyWhenAllChildrenDestroyed {
    pub id: i32,
    pub l: i32,
}

impl Game {
    /// ActOnDestroy.OnDestroy: DestroyWith, then Event (its persistent calls were checked to reach only audio and
    /// particles; the runtime listeners follow them).
    fn act_on_destroy_run(&mut self, key: i32) {
        let Some(a) = self.act_on_destroy.remove(&key) else { return };
        for id in a.destroy_with {
            self.destroy_object(id);
        }
        for l in a.listeners {
            match l {
                Listener::Check(d) => self.dwacd_check(d),
                // DetachFromParentOnStart.DestroyThis
                Listener::DestroyThis(id) => self.destroy_object(id),
            }
        }
    }

    /// DestroyWhenAllChildrenDestroyed.Awake over `children` (GetComponentsInChildren<Transform>: the active
    /// objects of its hierarchy, itself first), each with its Transform id and whether it is in Exclude.
    pub fn dwacd_awake(&mut self, id: i32, children: &[(i32, bool)]) {
        let d = self.dwacds.len();
        let mut l = children.len() as i32;
        for &(key, excluded) in children {
            if excluded {
                l -= 1;
                continue;
            }
            // GetOrAddComponent<ActOnDestroy>: a new one takes an instance id.
            if !self.act_on_destroy.contains_key(&key) {
                self.allocate_instance_id();
            }
            self.act_on_destroy.entry(key).or_default().listeners.push(Listener::Check(d));
        }
        self.dwacds.push(DestroyWhenAllChildrenDestroyed { id, l });
    }

    /// DestroyWhenAllChildrenDestroyed.Check
    fn dwacd_check(&mut self, d: usize) {
        self.dwacds[d].l -= 1;
        if self.dwacds[d].l <= 1 {
            self.destroy_object(self.dwacds[d].id);
        }
    }

    /// The position, rotation and localScale of the Transform holding component `id`: a physical's or node's from
    /// the simulation; another's from its clone, which is current only before the bodies move (`fresh`).
    fn object_pose(&mut self, id: i32, fresh: bool) -> ([f32; 3], [f32; 4], [f32; 3]) {
        self.with_component_instance(id, |g, inst, o| {
            let components = &inst.objects[o].components;
            if let Some(p) = g.physicals.iter().position(|p| components.contains(&p.id)) {
                let t = &g.physicals[p].transform;
                return (t.position, t.rotation, t.localScale);
            }
            if let Some(n) = g.transform_nodes.iter().position(|n| n.id == components[0]) {
                let (position, rotation) = g.node_world(n);
                let s = g.transform_nodes[n].trs.s;
                return (position, rotation, [s[0], s[1], s[2]]);
            }
            if !fresh {
                unimplemented!("the pose of a bare Transform after its clone was synced");
            }
            let s = inst.objects[o].trs.s;
            (inst.position(o), inst.rotation(o), [s[0], s[1], s[2]])
        })
    }

    /// InstantiateObjectBehaviour.Instantiate of component `id` (`fresh`: from its Start, before anything moved).
    pub fn instantiate_object(&mut self, id: i32, fresh: bool) {
        let (source, f) = self.instantiate_objects[&id].clone();
        let source = source.unwrap_or_else(|| unimplemented!("InstantiateObjectBehaviour: Instantiate(null Prefab) (ArgumentException)"));
        let pose = self.object_pose(id, fresh);
        let position = if f.WorldSpace { f.Offset } else { [pose.0[0] + f.Offset[0], pose.0[1] + f.Offset[1], pose.0[2] + f.Offset[2]] };
        let rotation = if f.CopyRotation { pose.1 } else { [0.0, 0.0, 0.0, 1.0] };
        let mut db = self.assets.take().expect("InstantiateObjectBehaviour without the game's data");
        let spawned = db.prefab(source).and_then(|prefab| self.spawn(&mut db, &prefab, position, rotation, None));
        self.assets = Some(db);
        let (mut inst, linked) = spawned.unwrap_or_else(|e| panic!("InstantiateObjectBehaviour: Instantiate: {e}"));
        if f.CopyScale {
            inst.set_local_scale(0, pose.2);
            self.refresh_transforms(&inst, &linked);
        }
        self.unsynced.push(inst);
        if f.DestroySelf {
            self.destroy_object_after(id, f.DestroySelfDelay);
        }
    }

    /// StartCoroutine up to a `yield return new WaitForSeconds(seconds)`, by the MonoBehaviour `owner`.
    pub fn start_wait(&mut self, owner: i32, seconds: f32, step: Step) {
        self.coroutine_seq += 1;
        self.coroutines.push(Coroutine { seq: self.coroutine_seq, at: self.time.timeAsDouble + seconds as f64, owner, step });
    }

    /// A coroutine past its WaitForSeconds.
    pub fn resume_coroutine(&mut self, c: Coroutine) {
        match c.step {
            // DistantSoundBehaviour.Play(type): Play<Type>'s clips.PickRandom(); PlayOneShot is audio.
            Step::DistantSound(kind) => {
                if self.prefs.DistantSoundEffects {
                    let count = self.distant_sounds[kind as usize - 1];
                    self.pick_random(count);
                }
            }
            Step::ExplosiveEndOfFrame(e, seq) => self.explosive_end_of_frame(e, seq),
            Step::DestroyableShotBreak(x, normal) => self.destroyable_shot_break(x, normal),
            Step::Bullet(b) => self.bullet_resume(b),
            Step::LimbCrushEndOfFrame(l) => {
                self.coroutine_seq += 1;
                let owner = self.limbs[l].id;
                self.fixed_update_waits.push(Coroutine { seq: self.coroutine_seq, at: self.time.timeAsDouble, owner, step: Step::LimbCrush(l) });
            }
            Step::LimbCrush(l) => self.limb_crush(l),
            Step::FirearmShoot(f) => self.firearm_shoot(f),
            Step::LaunchRoutine(l) => self.launcher_launch_routine(l),
            Step::SpikeIgnite(i) => self.spike_grenade_resume(i),
            Step::BulbBreak(i) => self.bulb_break(i),
            Step::ShrapnelEndOfFrame(at, n) => {
                self.coroutine_seq += 1;
                self.fixed_update_waits.push(Coroutine { seq: self.coroutine_seq, at: self.time.timeAsDouble, owner: c.owner, step: Step::ShrapnelEmit(at, n) });
            }
            Step::ShrapnelEmit(at, n) => {
                for _ in 0..n {
                    let direction = self.random.inside_unit_circle();
                    self.emitter_emit(super::ballistics::Shooter::Shrapnel, at, direction);
                }
            }
            Step::DoAtTimeOnceInvoke(d, k) => {
                // action.Event.Invoke(): a destroyed target's call is skipped.
                for target in self.do_at_time_onces[d].actions[k].instantiates.clone() {
                    if !self.destroyed_components.contains(&target) {
                        self.instantiate_object(target, false);
                    }
                }
                let (id, duration) = (self.do_at_time_onces[d].id, self.do_at_time_onces[d].actions[k].Duration);
                self.start_wait(id, duration, Step::DoAtTimeOnceNext(d, k + 1));
            }
            Step::DoAtTimeOnceNext(d, i) => self.do_at_time_once_next(d, i),
            Step::Routine(id, _) => self.co_run(id),
        }
    }

    /// DoAtTimeOnce.StartSequence from action `i`: WaitForSeconds(Delay) of the next enabled one.
    pub fn do_at_time_once_next(&mut self, d: usize, i: usize) {
        let s = &self.do_at_time_onces[d];
        if let Some(k) = (i..s.actions.len()).find(|&k| s.actions[k].Enabled) {
            let (id, delay) = (s.id, s.actions[k].Delay);
            self.start_wait(id, delay, Step::DoAtTimeOnceInvoke(d, k));
        }
    }
}

/// A coroutine waiting in WaitForSeconds until `at`; it stops when its MonoBehaviour's GameObject is destroyed or
/// deactivated.
#[derive(Clone, Copy, Debug)]
pub struct Coroutine {
    pub seq: u64,
    pub at: f64,
    pub owner: i32,
    pub step: Step,
}

#[derive(Clone, Copy, Debug)]
pub enum Step {
    /// ExplosionSoundBehviour.Play's DelayCoroutine(DistantSoundBehaviour.Play(SoundType)).
    DistantSound(i64),
    /// DoAtTimeOnce.StartSequence before action k's Event.
    DoAtTimeOnceInvoke(usize, usize),
    /// DoAtTimeOnce.StartSequence after an action's Duration, going on from action i.
    DoAtTimeOnceNext(usize, usize),
    /// ExplosiveBehaviour.Explode after WaitForEndOfFrame: (explosive, its coroutine sequence).
    ExplosiveEndOfFrame(usize, u64),
    /// DestroyableBehaviour.Shot's WaitAFrame: (destroyable, shot.normal).
    DestroyableShotBreak(usize, [f32; 2]),
    /// A BallisticIteration coroutine (`Game::bullets`).
    Bullet(usize),
    /// LimbBehaviour.CrushNextFrame after WaitForEndOfFrame, then after WaitForFixedUpdate.
    LimbCrushEndOfFrame(usize),
    LimbCrush(usize),
    /// FirearmBehaviour.DelayedShot after its WaitForSeconds(FireDelay).
    FirearmShoot(usize),
    /// ProjectileLauncherBehaviour.LaunchRoutine after its WaitForFixedUpdate.
    LaunchRoutine(usize),
    /// GlobalShrapnelEmitter.EmitRoutine(position, amount) after its WaitForEndOfFrame, then after its
    /// WaitForFixedUpdate.
    ShrapnelEndOfFrame([f32; 2], i32),
    ShrapnelEmit([f32; 2], i32),
    /// SpikeGrenadeBehaviour.IgniteRoutine after its WaitForSeconds(Delay): the `Game::spike_grenades` index.
    SpikeIgnite(usize),
    /// BulbBehaviour.Shot's WaitAFrame after its WaitForSeconds(0.05): the `Game::bulbs` index.
    BulbBreak(usize),
    /// A translated routine (`coroutine::Routine`) by id, and for a yield null / WaitForSeconds the frame count from
    /// which DelayedCallManager::Update runs it (kWaitForNextFrame: the frame it was started in + 1; else i64::MIN).
    Routine(u64, i64),
}

#[derive(Clone, Debug)]
pub struct DoAtTimeOnce {
    pub id: i32,
    pub actions: Vec<Action>,
}

/// DoAtTimeOnce.Action; Event's InstantiateObjectBehaviour.Instantiate targets by component id.
#[derive(Clone, Debug)]
pub struct Action {
    pub Enabled: bool,
    pub Delay: f32,
    pub Duration: f32,
    pub instantiates: Vec<i32>,
}

script! {
    #[derive(PartialEq)]
    InstantiateObject {
        Offset: [f32; 3] = [0.0; 3],
        InstantiateOnStart: bool = false,
        WorldSpace: bool = false,
        DestroySelf: bool = false,
        CopyRotation: bool = false,
        CopyScale: bool = false,
        DestroySelfDelay: f32 = 0.0,
    }
    links {}
}

#[cfg(test)]
mod tests {
    use super::clamp_magnitude;

    #[test]
    fn clamp_magnitude_scales_only_above_max() {
        assert_eq!(clamp_magnitude([3.0, 4.0], 10.0), [3.0, 4.0]);
        assert_eq!(clamp_magnitude([3.0, 4.0], 2.5), [1.5, 2.0]);
    }
}
