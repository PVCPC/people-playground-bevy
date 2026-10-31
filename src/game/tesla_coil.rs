//! TeslaCoilBehaviour and its TeslaCoilLightningBoltBehaviour children. Awake instantiates BoltCount bolts under the
//! coil at local (0, 1, 0); Use switches them on and off. An active bolt picks the nearest conducting physical object
//! within Radius each time its life runs out, charges it and nudges it every frame, and now and then leaves a Spark.
//! Two coils facing each other, each held by two charged Immobility Fields, open a Wh. The bolts' LineRenderer,
//! the audio and the light are presentation (the PickRandom of the spark clips stays).
#![allow(non_snake_case)]

use super::{script, Behaviour, Game};
use crate::assets::db::ObjRef;
use crate::b2::math::Vec2 as V;

script! {
    TeslaCoil {
        /// A uint.
        BoltCount: i32 = 2,
        Activated: bool = false,
        unfit: bool = false,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        enabled: bool = true,
        /// physicalBehaviour
        physical: usize = 0,
        /// The Collider2Ds on its GameObject: the OnImmobilityCapture/Release messages it gets.
        colliders: Vec<i32> = Vec::new(),
        /// sparkClips.Length
        sparkClips: usize = 0,
        /// The `Game::tesla_bolts` entries of `bolts`.
        bolts: Vec<usize> = Vec::new(),
        /// whCapturers (a HashSet): `Game::immobility_fields` indices.
        whCapturers: Vec<usize> = Vec::new(),
        whReady: bool = false,
    }
}

/// A TeslaCoilLightningBoltBehaviour clone under a coil.
#[derive(Clone, Debug, Default)]
pub struct TeslaBolt {
    /// The `Game::tesla_coils` entry (parent).
    pub coil: usize,
    /// The script's instance id and execution order.
    pub id: i32,
    pub order: i32,
    /// activeSelf of the clone.
    pub active: bool,
    pub seed: f32,
    pub life: f32,
    pub layer: u32,
    pub Radius: f32,
    pub Spark: Option<ObjRef>,
    pub target: Option<usize>,
    pub t: f32,
    pub randomOffset: [f32; 2],
    pub colliderArray: Vec<i32>,
}

/// What the link reads from the bolt prefab: its script's values and the ids its Instantiate takes.
#[derive(Clone, Debug, Default)]
pub struct BoltSource {
    pub bolt: TeslaBolt,
    /// Each GameObject and component of the clone, and the bolt script's place among them.
    pub ids: usize,
    pub script_at: usize,
}

impl Game {
    /// TeslaCoilBehaviour.Awake: Instantiate(bolt, position, identity, transform) BoltCount times; each clone's Awake
    /// only sets up its LineRenderer, and an active enabled bolt joins the Update list.
    pub fn tesla_coil_awake(&mut self, i: usize, source: &BoltSource) {
        for _ in 0..self.tesla_coils[i].BoltCount {
            let first = self.allocate_instance_id();
            for _ in 1..source.ids {
                self.allocate_instance_id();
            }
            let mut b = source.bolt.clone();
            b.coil = i;
            b.id = first + source.script_at as i32;
            b.active = true;
            self.tesla_bolts.push(b);
            let k = self.tesla_bolts.len() - 1;
            self.tesla_coils[i].bolts.push(k);
            self.behaviour_of.insert(self.tesla_bolts[k].id, (source.bolt.order, Behaviour::TeslaBolt(k)));
            self.behaviours.add(source.bolt.order, Behaviour::TeslaBolt(k));
        }
    }

    /// TeslaCoilBehaviour.Start
    pub fn tesla_coil_start(&mut self, i: usize) {
        self.tesla_coil_update_bolts(i);
    }

    /// TeslaCoilBehaviour.Use
    pub fn tesla_coil_use(&mut self, i: usize) {
        if self.tesla_coils[i].enabled {
            self.tesla_coils[i].Activated = !self.tesla_coils[i].Activated;
            self.tesla_coil_update_bolts(i);
        }
    }

    /// GameObject.SetActive on a bolt clone: an inactive one leaves the Update list, an active one rejoins it at the end.
    fn tesla_bolt_set_active(&mut self, k: usize, active: bool) {
        if self.tesla_bolts[k].active == active {
            return;
        }
        self.tesla_bolts[k].active = active;
        let b = Behaviour::TeslaBolt(k);
        if active {
            self.behaviours.add(self.tesla_bolts[k].order, b);
        } else {
            self.behaviours.remove(b);
        }
    }

    /// TeslaCoilBehaviour.UpdateBolts (the audio plays or stops)
    fn tesla_coil_update_bolts(&mut self, i: usize) {
        let on = self.tesla_coils[i].Activated;
        for k in self.tesla_coils[i].bolts.clone() {
            self.tesla_bolt_set_active(k, on);
        }
    }

    /// TeslaCoilBehaviour.OnDisable (instantiatedWhO is never set: see AffectOpposingNode)
    pub fn tesla_coil_on_disable(&mut self, i: usize) {
        self.tesla_coils[i].Activated = false;
        for k in self.tesla_coils[i].bolts.clone() {
            self.tesla_bolt_set_active(k, false);
        }
        self.tesla_coils[i].whReady = false;
    }

    /// The coil's GameObject is destroyed: its bolt children go with it.
    pub fn tesla_coil_on_destroy(&mut self, i: usize) {
        for k in self.tesla_coils[i].bolts.clone() {
            self.behaviours.remove(Behaviour::TeslaBolt(k));
            self.tesla_bolts[k].active = false;
            self.destroyed_components.insert(self.tesla_bolts[k].id);
        }
    }

    /// TeslaCoilBehaviour.FixedUpdate
    pub fn tesla_coil_fixed_update(&mut self, i: usize) {
        self.tesla_coils[i].whReady = self.tesla_coil_calculate_wh_ready(i);
        if self.tesla_coils[i].whReady {
            self.tesla_coil_affect_opposing_node(i);
        }
    }

    /// TeslaCoilBehaviour.CalculateWhReady
    fn tesla_coil_calculate_wh_ready(&self, i: usize) -> bool {
        let s = &self.tesla_coils[i];
        if s.unfit || !s.Activated || !s.enabled || s.whCapturers.len() < 2 || self.physicals[s.physical].Charge() < 25.0 {
            return false;
        }
        let mut num = 0;
        for &f in &s.whCapturers {
            let field = &self.immobility_fields[f];
            if !self.destroyed_components.contains(&field.id) && field.Activated && self.physicals[field.physical].Charge() > 25.0 {
                num += 1;
            }
            if num >= 2 {
                return true;
            }
        }
        false
    }

    /// TeslaCoilBehaviour.AffectOpposingNode: Physics2D.OverlapPoint(transform.TransformPoint(0, 6)) on the default
    /// raycast layers; a ready coil there makes the first one Instantiate the Wh (an opening whose WhOpening script
    /// is not rewritten), and only then do the forces and the Wh's position follow.
    fn tesla_coil_affect_opposing_node(&mut self, i: usize) {
        if self.tesla_coils[i].unfit {
            return;
        }
        let p = self.tesla_coils[i].physical;
        let point = self.transform_point(p, [0.0, 6.0]);
        let Some((_, collider)) = self.overlap_point_first(point, !(1 << 2)) else { return };
        let other = self.tesla_coils.iter().position(|c| !self.destroyed_components.contains(&c.id) && c.colliders.contains(&collider));
        if other.is_some_and(|o| self.tesla_coils[o].whReady) {
            unimplemented!("TeslaCoilBehaviour.AffectOpposingNode: Instantiate(Wh) (WhOpening is not rewritten)");
        }
    }

    /// The coil on the GameObject of `collider`, for SendMessage("OnImmobilityCapture"/"OnImmobilityRelease").
    fn tesla_coil_of_collider(&self, collider: i32) -> Option<usize> {
        self.tesla_coils.iter().position(|c| !self.destroyed_components.contains(&c.id) && c.colliders.contains(&collider))
    }

    /// TeslaCoilBehaviour.OnImmobilityCapture(sender)
    pub(super) fn tesla_coil_on_immobility_capture(&mut self, collider: i32, field: usize) {
        if let Some(i) = self.tesla_coil_of_collider(collider) {
            if !self.tesla_coils[i].whCapturers.contains(&field) {
                self.tesla_coils[i].whCapturers.push(field);
            }
        }
    }

    /// TeslaCoilBehaviour.OnImmobilityRelease(sender)
    pub(super) fn tesla_coil_on_immobility_release(&mut self, collider: i32, field: usize) {
        if let Some(i) = self.tesla_coil_of_collider(collider) {
            self.tesla_coils[i].whCapturers.retain(|&f| f != field);
        }
    }

    /// The bolt's transform.position: its local (0, 1, 0) under the coil.
    fn tesla_bolt_position(&self, k: usize) -> [f32; 3] {
        let p = self.tesla_coils[self.tesla_bolts[k].coil].physical;
        self.transform_point3(p, [0.0, 1.0, 0.0])
    }

    /// TeslaCoilLightningBoltBehaviour.Init
    fn tesla_bolt_init(&mut self, k: usize) {
        self.tesla_bolt_find_new_target(k);
        self.tesla_bolts[k].seed = self.random.range(-9000.0, 9000.0);
        let long = (self.random.value() as f64) > 0.95;
        self.tesla_bolts[k].life = if long { self.random.range(0.1, 0.3) } else { self.random.range(0.02, 0.08) };
        // lineRenderer.widthMultiplier = Random.Range(1f, 2f)
        self.random.range(1.0, 2.0);
        let c = self.random.inside_unit_circle();
        self.tesla_bolts[k].randomOffset = [c[0] * 4.0, c[1] * 4.0];
    }

    /// TeslaCoilLightningBoltBehaviour.FindNewTarget: OverlapCircleNonAlloc into a 32-collider buffer, then the
    /// conducting in-world physical whose collider is nearest a jittered point (one insideUnitSphere per candidate).
    fn tesla_bolt_find_new_target(&mut self, k: usize) {
        self.tesla_bolts[k].target = None;
        let position = self.tesla_bolt_position(k);
        let (radius, layer) = (self.tesla_bolts[k].Radius, self.tesla_bolts[k].layer);
        let mut hits = self.overlap_circle(V::new(position[0], position[1]), radius, layer);
        hits.truncate(32);
        if hits.is_empty() {
            return;
        }
        let root = self.root_id(self.tesla_coils[self.tesla_bolts[k].coil].physical);
        let mut best = 10_000_000.0f32;
        for (q, collider) in hits {
            if !self.collider_exists(collider) {
                continue;
            }
            // collider.transform.root != transform.root, PhysicalObjectsInWorldByTransform.TryGetValue(collider.transform)
            let Some(q) = q.filter(|&q| self.physicals[q].in_world && !self.physicals[q].destroyed) else { continue };
            if self.root_id(q) == root || !self.properties[self.physicals[q].Properties].Conducting {
                continue;
            }
            let at = self.physicals[q].transform.position;
            let s = self.random.inside_unit_sphere();
            let d = [at[0] - (position[0] + s[0]), at[1] - (position[1] + s[1]), at[2] - (position[2] + s[2])];
            let sqr = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
            if sqr < best {
                self.tesla_bolts[k].target = Some(q);
                best = sqr;
            }
        }
        if let Some(q) = self.tesla_bolts[k].target {
            // target.GetComponents<Collider2D>()
            self.tesla_bolts[k].colliderArray = self.physicals[q].colliders.clone();
        }
    }

    /// TeslaCoilLightningBoltBehaviour.Update (the line positions are rendering)
    pub fn tesla_bolt_update(&mut self, k: usize) {
        self.tesla_bolts[k].t += self.time.smoothDeltaTime;
        if self.tesla_bolts[k].t > self.tesla_bolts[k].life {
            self.tesla_bolts[k].t = 0.0;
            self.tesla_bolt_init(k);
        }
        let position = self.tesla_bolt_position(k);
        match self.tesla_bolts[k].target.filter(|&q| !self.physicals[q].destroyed) {
            Some(q) => {
                let mut vector = self.physicals[q].transform.position;
                let mut best = f32::MAX;
                let off = self.tesla_bolts[k].randomOffset;
                let jittered = [position[0] + off[0], position[1] + off[1], position[2]];
                for collider in self.tesla_bolts[k].colliderArray.clone() {
                    if !self.collider_exists(collider) {
                        continue;
                    }
                    // Collider2D.ClosestPoint (AutoSyncTransforms first)
                    self.sync_pending();
                    let c = self.world.collider_closest_point(collider, V::new(position[0], position[1]));
                    let d = [c.x - jittered[0], c.y - jittered[1], 0.0 - jittered[2]];
                    let sqr = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
                    if sqr < best {
                        best = sqr;
                        vector = [c.x, c.y, 0.0];
                    }
                }
                self.physicals[q].charge = self.physicals[q].Charge() + self.time.deltaTime * 4.0;
                let c = self.random.inside_unit_circle();
                let rb = self.physicals[q].rigidbody.clone();
                rb.add_force_impulse(&mut self.world, [c[0] * 0.01, c[1] * 0.01]);
                // SmoothRandom(10f).x
                let seed = self.tesla_bolts[k].seed;
                if crate::unity::perlin_noise(10.0 * self.time.time, seed) * 2.0 - 1.0 > 0.99 {
                    // parent.PlaySparkSound(): PlayOneShot(sparkClips.PickRandom())
                    let clips = self.tesla_coils[self.tesla_bolts[k].coil].sparkClips;
                    self.pick_random(clips).unwrap_or_else(|| unimplemented!("TeslaCoilBehaviour.PlaySparkSound: sparkClips.PickRandom() of an empty array (IndexOutOfRangeException)"));
                    let spark = self.tesla_bolts[k].Spark.unwrap_or_else(|| unimplemented!("TeslaCoilLightningBoltBehaviour: Instantiate(null Spark) (ArgumentException)"));
                    self.spawn_prefab(spark, vector);
                }
            }
            // vector = Random.insideUnitCircle * Radius + position: only the line reads it.
            None => {
                self.random.inside_unit_circle();
            }
        }
    }
}
