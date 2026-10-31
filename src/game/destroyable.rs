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
        /// OnBreak's persistent calls that are not audio or particles.
        OnBreak: Vec<super::link::EventCall> = Vec::new(),
        /// The component comes after the PhysicalBehaviour on its GameObject (SendMessage order).
        after_physical: bool = true,
        /// The component's index on its GameObject.
        index: usize = 0,
    }
}

/// Vector2.ClampMagnitude: sqrMagnitude is stored as a float, the comparison and the division are Mono doubles.
pub(super) fn clamp_magnitude(v: [f32; 2], max: f32) -> [f32; 2] {
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
    pub(super) fn destroyable_explosive(&mut self, x: usize) -> Option<usize> {
        let p = self.destroyables[x].PhysicalBehaviour;
        self.explosives_on(p).first().copied()
    }

    /// DestroyableBehaviour.Update
    pub fn destroyable_update(&mut self, x: usize) {
        let explosive = self.destroyable_explosive(x);
        let s = &self.destroyables[x];
        let p = &self.physicals[s.PhysicalBehaviour];
        if p.SimulateTemperature && p.Temperature >= s.TemperatureLimit && !explosive.is_some_and(|e| self.explosives[e].TemperatureLimit >= p.Temperature) {
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
        for call in self.destroyables[x].OnBreak.clone() {
            self.event_call(call);
        }
        let s = &self.destroyables[x];
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
    pub(super) fn destroyable_apply_ancestor_properties(&mut self, ancestor: usize, original: usize, velocity: [f32; 2], debris: &super::link::Linked) {
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
    // DestroyObjectHighLevel_Internal (180758b40): Deactivate, then the WillDestroy walk (18075a230) runs
    // GameObject::WillDestroyGameObject (18058a140, each component in order) on the object before recursing into each
    // child in order: parent-first pre-order, the prefab's depth-first order `descendants` keeps.
    pub fn run_object_destroy(&mut self, id: i32) {
        self.object_destroys.retain(|&(_, i)| i != id);
        if self.destroyed_components.contains(&id) {
            return;
        }
        let mut orphans = Vec::new();
        let root = self.with_component_instance(id, |g, inst, o| {
            let prefab = inst.prefab.clone();
            let objects: Vec<usize> = std::iter::once(o).chain(inst.descendants(o)).collect();
            for &x in &objects {
                g.destroyed_components.extend(inst.objects[x].components.iter().copied());
            }
            // Clones instantiated under these Transforms go with them (taken first: SetActive(false) below would
            // otherwise find them).
            for &x in &objects {
                let key = inst.objects[x].components[0];
                orphans.extend(g.attached_clones.remove(&key).unwrap_or_default().into_iter().map(|c| c.id));
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
                        // Its OnSingleUse listener goes with it: activate_direct skips a destroyed toggle.
                        "ActivationToggleBehaviour" => {}
                        // OnDestroy removes an empty ContinuousActivation listener.
                        "ActivatorElectrodeBehaviour" => {}
                        "NoCollide" => g.no_collide_destroy(cid),
                        "NoCollideGm" => g.no_collide_gm_destroy(cid, inst),
                        "TyreDeflationBehaviour" => g.tyre_destroy(cid),
                        "SharpOnAllSidesBehaviour" => {
                            let i = g.sharp_on_all_sides.iter().position(|s| s.id == cid).expect("the SharpOnAllSidesBehaviour");
                            g.sharp_on_all_sides_destroy(i);
                        }
                        // Destroys its trail mesh (rendering).
                        "EnergySwordMotionBlur" => {}
                        "ImmobilityFieldBehaviour" => {
                            let i = g.immobility_fields.iter().position(|s| s.id == cid).expect("the ImmobilityFieldBehaviour");
                            g.immobility_field_destroy_all_joints(i);
                        }
                        // OnDestroy: instantiatedWhO is never set; the bolt children go with the GameObject.
                        "TeslaCoilBehaviour" => {
                            let i = g.tesla_coils.iter().position(|s| s.id == cid).expect("the TeslaCoilBehaviour");
                            g.tesla_coil_on_destroy(i);
                        }
                        "EnergySwordBehaviour" => {
                            let i = g.energy_swords.iter().position(|s| s.id == cid).expect("the EnergySwordBehaviour");
                            let blur = inst.objects[g.energy_swords[i].MotionBlur].components[0];
                            g.energy_sword_destroy(i, blur);
                        }
                        "FreezeBehaviour" => {
                            let i = g.freezes.iter().position(|s| s.id == cid).expect("the FreezeBehaviour");
                            g.freeze_destroy(i);
                        }
                        "ChainsawBehaviour" => {
                            let i = g.chainsaws.iter().position(|s| s.id == cid).expect("the ChainsawBehaviour");
                            g.chainsaw_destroy(i);
                        }
                        // Global.RemoveAudioSource
                        "ExplosionSoundBehviour" | "AudioSourceTimeScaleBehaviour" | "ExistInDetailView" => {}
                        // DeregisterBehaviour.OnDestroy: FindRelevantAction / DeregisterAction of this object.
                        "DeregisterBehaviour" => g.undo_deregister_related(key),
                        "GearBehaviour" => g.gear_destroy(cid),
                        // DetectorBehaviour.OnDestroy removes its ContinuousActivationBehaviour listener, which sends nothing received.
                        "DetectorBehaviour" => {}
                        // ButtonBehaviour.OnDestroy removes its ContinuousActivationBehaviour listener, which sends nothing received (button.rs).
                        "ButtonBehaviour" => {}
                        // LaserBehaviour.OnDestroy destroys its egg effect, which only the refused hoverbox branch creates (laser.rs).
                        "LaserBehaviour" => {}
                        // BulbBehaviour.OnDestroy: Object.Destroy(OtherPart), a child (destroyed with the hierarchy already) until Break detaches it.
                        "BulbBehaviour" => {}
                        // FirearmAttachmentPointBehaviour.OnDestroy: Object.Destroy(FirearmAttachment), the component, queued: its object
                        // is destroyed in this pass already (a firearm's child) or it was never connected (null).
                        "FirearmAttachmentPointBehaviour" => {}
                        // GetComponentsInChildren on the destroyed (inactive) GameObject finds no collider.
                        "NoChildrenCollide" => {}
                        // PersonBehaviour.OnDestroy: AliveBehaviour.AliveByTransform.Remove, a lookup nothing rewritten reads.
                        "PersonBehaviour" => {
                            let i = g.people.iter().position(|p| p.id == cid).expect("the PersonBehaviour");
                            g.people[i].destroy_at = None;
                            g.people[i].destroyed = true;
                        }
                        // ShatteredShardBehaviour.OnDestroy is empty.
                        "ShatteredShardBehaviour" => {}
                        // LimbBehaviour.OnDestroy: LimbBehaviourManager.Limbs.Remove (Destroy(myStatus) is UI), then Destroy(item)
                        // for each DestroyWith. Its joints and bleeding particles go with the GameObject.
                        "LimbBehaviour" => {
                            let l = g.limb_of_object(key);
                            g.limbs[l].enabled = false;
                            for item in g.limbs[l].destroy_with.clone() {
                                g.destroy_object(item);
                            }
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
                // The BloodWireBehaviours the BloodWireTool added come after the prefab's components.
                g.blood_wire_object_destroy(&inst.objects[x].components);
                // The CheckOnDestroyAttachment that FirearmAttachmentBehaviour.Awake added.
                g.check_on_destroy_attachment(key);
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
        for child in orphans {
            self.run_object_destroy(child);
        }
        // The whole clone went: nothing can reach its objects any more.
        if root {
            let owns = |i: &super::spawn::Instance| i.objects.iter().any(|x| x.components.contains(&id));
            self.unsynced.retain(|i| !owns(i));
            self.retained.retain(|i| !owns(i));
        }
    }
}

/// What a clone made by Instantiate(prefab, position, rotation, parent) hangs from.
#[derive(Clone, Copy, Debug)]
pub enum AttachParent {
    /// hit.transform of a PhysicalBehaviour's Rigidbody2D.
    Physical(usize),
    /// hit.transform of a Collider2D without a Rigidbody2D (by the collider's id).
    Bare(i32),
}

/// A clone parented under an existing GameObject: its root Transform's id and local TRS under the parent.
#[derive(Clone, Copy, Debug)]
pub struct AttachedClone {
    pub id: i32,
    pub parent: AttachParent,
    pub local: crate::unity::transform::Trs,
    /// A live Rigidbody2D object re-parented under it (`physical_set_parent`): its own body moves it, not the parent's
    /// Transform; only SetActive and Destroy go through the parent.
    pub driven: bool,
}

impl Game {
    /// The Transforms of an AttachParent's GameObject, the object first and then each ancestor up to the root.
    fn attach_parent_nodes(&self, parent: AttachParent) -> Vec<crate::unity::transform::Trs> {
        match parent {
            AttachParent::Physical(p) => self.local_chain(p).into_iter().rev().collect(),
            AttachParent::Bare(c) => {
                let b = &self.bare_colliders[&c];
                self.anchored_nodes(b.anchor, &b.chain)
            }
        }
    }

    /// Instantiate(prefab, position, rotation, parent) for the clone with root Transform `id`: position and rotation are
    /// world values, stored as the local TRS under the parent (the prefab root's own local scale stays).
    pub(super) fn attach_clone(&mut self, id: i32, parent: AttachParent, position: [f32; 3]) {
        let nodes = self.attach_parent_nodes(parent);
        let ((t, q), _) = crate::unity::transform::set_world(&nodes, position, [0.0, 0.0, 0.0, 1.0]);
        let s = self.with_component_instance(id, |_, inst, o| inst.objects[o].trs.s);
        let local = crate::unity::transform::Trs { t: [t[0], t[1], t[2], 0.0], q, s };
        let key = match parent {
            AttachParent::Physical(p) => self.physicals[p].transform_id,
            AttachParent::Bare(c) => self.with_component_instance(c, |_, inst, o| inst.objects[o].components[0]),
        };
        self.attached_clones.entry(key).or_default().push(AttachedClone { id, parent, local, driven: false });
    }

    /// The attached clones' world poses: the parent's Transform chain applied to the stored local TRS, after the
    /// physics write-back has moved the parents.
    pub(super) fn update_attached_clones(&mut self) {
        use crate::unity::transform::{global_position, global_rotation, lossy_scale};
        if self.attached_clones.is_empty() {
            return;
        }
        let gone = &self.destroyed_components;
        self.attached_clones.values_mut().for_each(|v| v.retain(|c| !gone.contains(&c.id)));
        self.attached_clones.retain(|_, v| !v.is_empty());
        let mut all: Vec<AttachedClone> = self.attached_clones.values().flatten().copied().collect();
        // The map's order is arbitrary: ids give the spawn order.
        all.sort_by_key(|c| std::cmp::Reverse(c.id));
        for c in all.into_iter().filter(|c| !c.driven) {
            let parents = self.attach_parent_nodes(c.parent);
            let (mut v, mut r) = (c.local.t, c.local.q);
            for n in &parents {
                (v, r) = (global_position(n, v), global_rotation(n, r));
            }
            let mut nodes = vec![c.local];
            nodes.extend(parents);
            let s = lossy_scale(&nodes, r);
            self.with_component_instance(c.id, |_, inst, o| {
                inst.set_world_rotation(o, r);
                inst.set_world_position(o, [v[0], v[1], v[2]]);
                inst.set_local_scale(o, s);
            });
        }
    }

    /// GameObject.SetActive(`active`) on object `o` of `inst`: the clones parented under its subtree follow in
    /// activeInHierarchy (their own activeSelf stays). A deactivation reaches them all; an activation only those whose
    /// parent ended up active.
    pub(super) fn attached_set_active(&mut self, inst: &super::spawn::Instance, o: usize, active: bool) {
        if self.attached_clones.is_empty() {
            return;
        }
        let mut clones = Vec::new();
        for x in std::iter::once(o).chain(inst.descendants(o)) {
            if let Some(v) = self.attached_clones.get(&inst.objects[x].components[0]).filter(|_| !active || inst.objects[x].active) {
                clones.extend(v.iter().map(|c| (c.id, c.driven)));
            }
        }
        for (id, driven) in clones {
            if self.destroyed_components.contains(&id) {
                continue;
            }
            self.with_component_instance(id, |g, clone, o| {
                if driven {
                    // A live Rigidbody2D object under it: a full SetActive(false) of the clone with its own OnDisable and body
                    // cleanup, though only its activeInHierarchy changes (its activeSelf stays).
                    if !active {
                        g.set_active_scripts(clone, o, false);
                        clone.objects[o].self_active = true;
                    } else if clone.objects[o].self_active && !clone.objects[o].active {
                        unimplemented!("GameObject.SetActive(true) of a parent whose child Rigidbody2D object was deactivated with it (Joint2D::Activate of the HingeJoint2D it holds)");
                    }
                    return;
                }
                let prefab = clone.prefab.clone();
                if !active {
                    let mut scripts = Vec::new();
                    g.deactivate_recursively(clone, &prefab, 0, &mut scripts);
                    for (d, c) in scripts {
                        let Component::Script { class, .. } = &prefab.objects[d].components[c].1 else { unreachable!() };
                        if super::declares(class, "OnDisable") {
                            unimplemented!("{class}.OnDisable of a clone whose parent (Instantiate(.., parent)) was deactivated");
                        }
                    }
                } else if clone.objects[0].self_active && !clone.objects[0].active {
                    let mut queued = Vec::new();
                    g.activate_recursively(clone, &prefab, 0, &mut queued);
                    for (d, c) in queued {
                        if let Component::Script { class, .. } = &prefab.objects[d].components[c].1
                            && (!clone.objects[d].activated || super::declares(class, "OnEnable"))
                        {
                            unimplemented!("{class}: Awake or OnEnable of a clone whose parent (Instantiate(.., parent)) was activated");
                        }
                    }
                }
            });
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
            Step::SentryTurretExplode(i) => self.sentry_turret_explode(i),
            Step::Bullet(b) => self.bullet_resume(b),
            Step::AttachmentConnectDelay(pt) => self.attachment_points[pt].ignoreConnect = false,
            Step::LimbCrushEndOfFrame(l) => {
                self.coroutine_seq += 1;
                let owner = self.limbs[l].id;
                self.fixed_update_waits.push(Coroutine { seq: self.coroutine_seq, at: self.time.timeAsDouble, owner, step: Step::LimbCrush(l) });
            }
            Step::LimbCrush(l) => self.limb_crush(l),
            Step::LimbStunNumb(l) => self.limb_stun_numb(l),
            Step::LimbDelayedCrush(l) => self.limb_crush(l),
            Step::ExoticFlash(p) => self.exotic_flash(p),
            Step::FirearmShoot(f) => self.firearm_shoot(f),
            Step::LaunchRoutine(l) => self.launcher_launch_routine(l),
            Step::SpikeIgnite(i) => self.spike_grenade_resume(i),
            Step::BulbBreak(i) => self.bulb_break(i),
            Step::SyringeBreak(s) => self.break_syringe(s),
            Step::BalloonPop(b) => self.balloon_pop(b),
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
    /// SentryTurretBehaviour.Shot's DelayCoroutine(Random.value, Explode): the `Game::sentry_turrets` index.
    SentryTurretExplode(usize),
    /// A BallisticIteration coroutine (`Game::bullets`).
    Bullet(usize),
    /// FirearmAttachmentPointBehaviour.ConnectDelay after its WaitForSeconds(0.75): `ignoreConnect = false` (attachment point).
    AttachmentConnectDelay(usize),
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
    /// BalloonBehaviour.Shot's WaitAFrame after its WaitForSeconds(0.05): the `Game::balloons` index.
    BalloonPop(usize),
    /// SyringeBehaviour.Shot's WaitAFrame after its WaitForSeconds(0.05): the `Game::syringes` index.
    SyringeBreak(usize),
    /// LimbBehaviour.StunImpact's Utils.DelayCoroutine action after its WaitForSeconds: the `Game::limbs` index.
    LimbStunNumb(usize),
    /// ExplosionCreator.FragmentationRay's Utils.DelayCoroutine(0.01f, limb.Crush): the `Game::limbs` index.
    LimbDelayedCrush(usize),
    /// ExoticLiquid.OnUpdate's Utils.DelayCoroutine action: the container's physical.
    ExoticFlash(usize),
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
