//! ExplosiveBehaviour, ExplosionCreator and DeleteAfterTime, with the Physics2D ray, line and circle queries they use.
#![allow(non_snake_case)]

use super::{d, mathf, script, Behaviour, Game};
use crate::assets::db::{Component, ObjRef};
use crate::b2::math::{Transform, Vec2 as V};
use crate::b2::shape::Circle;
use crate::b2::world::{BodyId, Shape};
use crate::b2::contacts2d::ScriptCollision;

/// Physics2D.DefaultRaycastLayers: every layer but Ignore Raycast (2).
pub(super) const DEFAULT_RAYCAST_LAYERS: u32 = !(1 << 2);

script! {
    Explosive {
        Range: f32 = 27.0,
        ShockwaveStrength: f32 = 1.0,
        Delay: f32 = 2.0,
        /// A uint; the serialized values are small.
        FragmentationRayCount: i32 = 32,
        FragmentForce: f32 = 2.0,
        ShockwaveLiftForce: f32 = 0.0,
        ArmOnAwake: bool = false,
        DestroyOnExplode: bool = true,
        DisintegrateOnExplode: bool = false,
        ParticleEffectAndSound: bool = true,
        BurnPower: f32 = 0.0,
        TemperatureLimit: f32 = 500.0,
        BigExplosion: bool = false,
        ArmOnUse: bool = true,
        BallisticShrapnelCount: i32 = 0,
        AffectVictimTemperature: f32 = 0.0,
        ShotExplodeChance: f32 = 1.0,
        ExplodesOnFragmentHit: bool = false,
        ImpactForceThreshold: f32 = -1.0,
        ExplodeBurnProgressThreshold: f32 = 2.0,
        DismemberChance: f32 = 0.0,
        ShouldCreateKillzone: bool = true,
        MinChargeToExplode: f32 = f32::INFINITY,
        CanExplodeMultipleTimes: bool = false,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// GetComponent<PhysicalBehaviour>()
        physical: Option<usize> = None,
        /// BlockingLayers (the Awake default when serialized as 0).
        BlockingLayers: u32 = 0,
        /// OnExplode's CarBehaviour and DetachFromParent calls, in order (the rest are audio and particles).
        OnExplodeCalls: Vec<super::link::EventCall> = Vec::new(),
        /// OnArm's persistent calls reach only audio and particle components.
        OnArmPresentation: bool = true,
        VFXOverride: Option<ObjRef> = None,
        /// The Killzone clone's instance id.
        Killzone: Option<i32> = None,
        Armed: bool = false,
        hasExploded: bool = false,
        /// The pending Explode coroutines.
        coroutines: Vec<Coroutine> = Vec::new(),
        /// The component comes after the PhysicalBehaviour on its GameObject (SendMessage order).
        after_physical: bool = true,
        /// The component comes before the object's DestroyableBehaviour.
        before_destroyable: bool = false,
        /// The component's index on its GameObject.
        index: usize = 0,
    }
}

/// A started Explode: it waits for `wake` (WaitForSeconds), then for the end of the frame.
#[derive(Clone, Debug)]
pub struct Coroutine {
    pub wake: f64,
    pub seq: u64,
    pub end_of_frame: bool,
}

script! {
    DeleteAfter {
        Life: f32 = 1.0,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        time: f32 = 0.0,
        /// OnEndOfLife's persistent calls reach only audio and particle components.
        OnEndOfLifePresentation: bool = true,
    }
}

/// ExplosionCreator.EffectSize
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum EffectSize {
    #[default]
    Medium,
    Small,
    Large,
}

/// ExplosionCreator.ExplosionParameters
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ExplosionParameters {
    pub FragmentationRayCount: u32,
    pub Position: [f32; 3],
    pub Range: f32,
    pub FragmentForce: f32,
    pub CreateParticlesAndSound: bool,
    pub EffectSize: EffectSize,
    pub DismemberChance: f32,
    pub BallisticShrapnelCount: i32,
}

/// A fixture a ray crossed, as Raycast2DQuery::ReportFixture records it.
#[derive(Clone, Copy, Debug)]
pub struct RayHit2D {
    pub collider: i32,
    pub game_object: i32,
    pub body: BodyId,
    pub point: V,
    pub normal: V,
    pub distance: f32,
    pub fraction: f32,
}

/// Vector3.normalized: the magnitude is (float)Math.Sqrt(x * x + y * y + z * z), in double.
pub(super) fn normalized3(v: [f32; 3]) -> [f32; 3] {
    let m = (d(v[0]) * v[0] + d(v[1]) * v[1] + d(v[2]) * v[2]).0.sqrt() as f32;
    if m > 1e-5 { [v[0] / m, v[1] / m, v[2] / m] } else { [0.0; 3] }
}

impl Game {
    /// Raycast2DQuery::RunQuery (180bf8250) from `p1` to `p2` (QueriesStartInColliders is off): b2World::RayCast with
    /// ReportFixture's filters. Sensors are skipped (QueriesHitTriggers is off), then the layer mask on the collider's
    /// GameObject and the depth range on its Transform's z; a collider is recorded once, at its nearest fraction; the
    /// ray never clips. The hits go by fraction.
    fn ray_query(&mut self, p1: V, p2: V, mask: u32, min_depth: f32, max_depth: f32) -> Vec<RayHit2D> {
        self.sync_pending();
        let (dx, dy) = (p2.x - p1.x, p2.y - p1.y);
        let mut hits: Vec<RayHit2D> = Vec::new();
        if !(dy * dy + dx * dx > 1.4210855e-14_f32) {
            return hits;
        }
        let depths = (min_depth, max_depth) != (f32::NEG_INFINITY, f32::INFINITY);
        let mut z_of: Vec<(i32, f32)> = Vec::new();
        if depths {
            // The colliders' Transform z, read outside the world's callback.
            let mut ids = Vec::new();
            self.world.ray_cast(p1, p2, |(f, _, _, _, _)| {
                ids.push(self.world.fixture(f).collider.id);
                1.0
            });
            for c in ids {
                if !z_of.iter().any(|&(x, _)| x == c) {
                    let z = match self.physical_of_collider(c) {
                        Some(ph) => self.physicals[ph].transform.position[2],
                        None => self.bare_position(c)[2],
                    };
                    z_of.push((c, z));
                }
            }
        }
        self.world.ray_cast(p1, p2, |(f, _, point, normal, fraction)| {
            let fx = self.world.fixture(f);
            if fx.is_sensor {
                return -1.0;
            }
            let Some(&layer) = self.world.layers.get(&fx.collider.game_object) else {
                panic!("Physics2D.Raycast: no layer for collider {}", fx.collider.id);
            };
            if mask & (1 << layer) == 0 {
                return -1.0;
            }
            if depths {
                let z = z_of.iter().find(|&&(x, _)| x == fx.collider.id).unwrap().1;
                if z < min_depth || max_depth < z {
                    return -1.0;
                }
            }
            let distance = if fraction > 0.0 {
                let (px, py) = (point.x - p1.x, point.y - p1.y);
                (py * py + px * px).sqrt()
            } else {
                0.0
            };
            let hit = RayHit2D { collider: fx.collider.id, game_object: fx.collider.game_object, body: fx.body, point, normal, distance, fraction };
            match hits.iter_mut().find(|h| h.collider == hit.collider) {
                Some(old) if old.fraction > fraction => *old = hit,
                Some(_) => {}
                None => hits.push(hit),
            }
            1.0
        });
        hits.sort_by(|a, b| a.fraction.partial_cmp(&b.fraction).unwrap());
        hits
    }

    /// Physics2D.Raycast(origin, direction, distance): PhysicsQuery2D::Raycast (180bf6290) scales the direction by the
    /// reciprocal of its length and casts to origin + distance * direction; the nearest hit.
    pub fn raycast_first(&mut self, origin: V, direction: [f32; 2], distance: f32) -> Option<RayHit2D> {
        self.raycast(origin, direction, distance, DEFAULT_RAYCAST_LAYERS, f32::NEG_INFINITY, f32::INFINITY)
    }

    /// Physics2D.Raycast(origin, direction, distance, layerMask, minDepth, maxDepth).
    pub(super) fn raycast(&mut self, origin: V, direction: [f32; 2], distance: f32, mask: u32, min_depth: f32, max_depth: f32) -> Option<RayHit2D> {
        if distance == f32::INFINITY {
            unimplemented!("Physics2D.Raycast with an infinite distance");
        }
        let len2 = direction[0] * direction[0] + direction[1] * direction[1];
        let dir = if len2 != 0.0 {
            let inv = 1.0 / len2.sqrt();
            [direction[0] * inv, direction[1] * inv]
        } else {
            direction
        };
        let end = V::new(origin.x + distance * dir[0], origin.y + distance * dir[1]);
        let hits = self.ray_query(origin, end, mask, min_depth, max_depth);
        if hits.len() > 32 {
            unimplemented!("Physics2D.Raycast with more than 32 hits (std::sort is not a stable insertion sort)");
        }
        hits.first().copied()
    }

    /// Physics2D.LinecastNonAlloc(start, end, buffer[8], layerMask): the number of hits.
    fn linecast_count(&mut self, start: V, end: V, mask: u32) -> usize {
        self.ray_query(start, end, mask, f32::NEG_INFINITY, f32::INFINITY).len().min(8)
    }

    /// Physics2D.LinecastNonAlloc(start, end, buffer[n], layerMask): the hits, nearest first.
    pub(super) fn linecast_hits(&mut self, start: V, end: V, mask: u32, buffer: usize) -> Vec<RayHit2D> {
        let hits = self.ray_query(start, end, mask, f32::NEG_INFINITY, f32::INFINITY);
        if hits.len() > buffer {
            unimplemented!("Physics2D.LinecastNonAlloc with more hits than its buffer holds");
        }
        hits
    }

    /// Physics2D.OverlapCircleAll / OverlapCircleNonAlloc (OverlapCircleQuery2D, 180bf11a0): AutoSyncTransforms, a
    /// b2CircleShape at `center` against the fixtures in its AABB (b2TestOverlap), ReportFixture's filters as
    /// `overlap_query`. The hits as (the collider's physical, the Collider2D id).
    pub(super) fn overlap_circle(&mut self, center: V, radius: f32, mask: u32) -> Vec<(Option<usize>, i32)> {
        if radius < 1e-5 {
            unimplemented!("Physics2D.OverlapCircle with a point radius");
        }
        self.sync_pending();
        let shape = Shape::Circle(Circle { radius, p: center });
        let identity = Transform::default_identity();
        let aabb = shape.compute_aabb(&identity, 0);
        self.overlap_query(self.world.query_aabb(&aabb), mask, f32::NEG_INFINITY, f32::INFINITY, |other, child, xf| shape.test_overlap(0, &identity, other, child, xf))
    }

    /// The Rigidbody2D a b2 body belongs to (Collider2D.attachedRigidbody); None for a static collider's own body.
    pub(super) fn rigidbody_of_body(&self, body: BodyId) -> Option<super::Rigidbody> {
        self.physicals.iter().find(|p| !p.destroyed && p.rigidbody.body == body).map(|p| p.rigidbody.clone()).or_else(|| self.node_rigidbodies.iter().find(|n| n.rigidbody.body == body).map(|n| n.rigidbody.clone()))
    }

    /// Transform.position of a collider's GameObject.
    pub(super) fn collider_position(&self, collider: i32) -> [f32; 3] {
        match self.physical_of_collider(collider) {
            Some(p) => self.physicals[p].transform.position,
            None => self.bare_position(collider),
        }
    }

    /// Transform.position of the explosive's GameObject.
    fn explosive_position(&mut self, e: usize) -> [f32; 3] {
        match self.explosives[e].physical {
            Some(p) => self.physicals[p].transform.position,
            None => {
                let id = self.explosives[e].id;
                self.with_component_instance(id, |_, inst, o| inst.position(o))
            }
        }
    }

    /// GameObject.activeInHierarchy of the explosive's object.
    fn explosive_active(&mut self, e: usize) -> bool {
        match self.explosives[e].physical {
            Some(p) => self.physicals[p].active,
            None => {
                let id = self.explosives[e].id;
                self.with_component_instance(id, |_, inst, o| inst.objects[o].active)
            }
        }
    }

    /// Whether the explosive's GameObject is active and not destroyed (a coroutine dies with it).
    fn explosive_alive(&mut self, e: usize) -> bool {
        !self.destroyed_components.contains(&self.explosives[e].id) && self.explosive_active(e)
    }

    /// StartCoroutine(Explode(delay, forceDoubleCheck)) up to its first yield (WaitForSeconds).
    pub(super) fn explosive_start_explode(&mut self, e: usize, delay: f32, force_double_check: bool) {
        let s = &self.explosives[e];
        if (!s.CanExplodeMultipleTimes && s.hasExploded) || (force_double_check && s.hasExploded) {
            return;
        }
        if !s.OnArmPresentation {
            unimplemented!("ExplosiveBehaviour.OnArm: a persistent call beyond audio and particles");
        }
        let wake = self.time.timeAsDouble + delay as f64;
        self.coroutine_seq += 1;
        let seq = self.coroutine_seq;
        let s = &mut self.explosives[e];
        s.Armed = true;
        s.coroutines.push(Coroutine { wake, seq, end_of_frame: false });
    }

    /// ExplosiveBehaviour.Activate
    pub fn explosive_activate(&mut self, e: usize) {
        let delay = self.explosives[e].Delay;
        self.explosive_start_explode(e, delay, false);
    }

    /// ExplosiveBehaviour.Use
    pub fn explosive_use(&mut self, e: usize) {
        if !self.explosives[e].Armed && self.explosives[e].ArmOnUse {
            self.explosive_activate(e);
        }
    }

    /// ExplosiveBehaviour.ForceImmediateExplosion
    pub fn explosive_force_immediate(&mut self, e: usize) {
        self.explosives[e].coroutines.clear();
        self.explosive_start_explode(e, 0.0, false);
    }

    /// ExplosiveBehaviour.Start: the Killzone clone under the object (IgnoreParentSize.DesiredSize is rendering), then
    /// Activate for ArmOnAwake.
    pub fn explosive_start(&mut self, e: usize) {
        if !self.explosive_active(e) {
            return;
        }
        if self.explosives[e].ShouldCreateKillzone {
            let position = self.explosive_position(e);
            let id = self.spawn_resource("Prefabs/Killzone", position);
            self.explosives[e].Killzone = Some(id);
        }
        if self.explosives[e].ArmOnAwake {
            self.explosive_activate(e);
        }
    }

    /// ExplosiveBehaviour.FixedUpdate
    pub fn explosive_fixed_update(&mut self, e: usize) {
        let s = &self.explosives[e];
        let Some(p) = s.physical.map(|p| &self.physicals[p]) else { return };
        if p.Temperature >= s.TemperatureLimit || (s.ExplodeBurnProgressThreshold < 1.0 && p.BurnProgress > s.ExplodeBurnProgressThreshold) || p.Charge() > s.MinChargeToExplode {
            self.explosive_start_explode(e, 0.0, false);
        }
    }

    /// ExplosiveBehaviour.OnCollisionEnter2D
    pub fn explosive_on_collision_enter(&mut self, e: usize, c: &ScriptCollision) {
        let threshold = self.explosives[e].ImpactForceThreshold;
        if !(threshold < 0.0) {
            let contact = c.contacts.first().unwrap_or_else(|| unimplemented!("ArgumentOutOfRangeException: Collision2D.GetContact(0) without contacts"));
            if contact.normal_impulse > threshold {
                self.explosive_start_explode(e, 0.05, false);
            }
        }
    }

    /// ExplosiveBehaviour.OnFragmentHit
    pub fn explosive_on_fragment_hit(&mut self, e: usize) {
        let s = &self.explosives[e];
        if s.ExplodesOnFragmentHit && !s.Armed {
            self.explosive_start_explode(e, 0.05, true);
        }
    }

    /// The pending Explode coroutines whose WaitForSeconds ran out (DelayedCallManager, between Update and
    /// LateUpdate, in wake order): each goes on to wait for the end of the frame.
    pub fn explosive_coroutines(&mut self) {
        let now = self.time.timeAsDouble;
        let mut due: Vec<(f64, u64, usize)> = Vec::new();
        for (e, s) in self.explosives.iter().enumerate() {
            due.extend(s.coroutines.iter().filter(|c| !c.end_of_frame && c.wake <= now).map(|c| (c.wake, c.seq, e)));
        }
        due.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.cmp(&b.1)));
        for (_, seq, e) in due {
            if !self.explosive_alive(e) {
                self.explosives[e].coroutines.clear();
                continue;
            }
            self.explosives[e].coroutines.iter_mut().find(|c| c.seq == seq).unwrap().end_of_frame = true;
            let owner = self.explosives[e].id;
            self.end_of_frame.push(super::destroyable::Coroutine { seq, at: now, owner, step: super::destroyable::Step::ExplosiveEndOfFrame(e, seq) });
        }
    }

    /// WaitForEndOfFrame: the coroutines that reached it run on, in the order they got there; one that waits for
    /// the end of a frame again here waits for the next.
    pub fn end_of_frame_calls(&mut self) {
        for c in std::mem::take(&mut self.end_of_frame) {
            // A coroutine stopped since is gone from the list.
            self.resume_coroutine(c);
        }
    }

    /// The rest of an Explode coroutine after WaitForEndOfFrame.
    pub(super) fn explosive_end_of_frame(&mut self, e: usize, seq: u64) {
        // StopAllCoroutines removed it.
        let Some(i) = self.explosives[e].coroutines.iter().position(|c| c.seq == seq) else { return };
        self.explosives[e].coroutines.remove(i);
        if self.explosive_alive(e) {
            self.explosive_explode(e);
        }
    }

    /// The rest of Explode after WaitForEndOfFrame.
    fn explosive_explode(&mut self, e: usize) {
        let s = &self.explosives[e];
        let (id, p) = (s.id, s.physical);
        let disintegrate = s.DisintegrateOnExplode;
        for call in s.OnExplodeCalls.clone() {
            self.event_call(call);
        }
        if self.explosives[e].DestroyOnExplode {
            self.explosive_disable_colliders(e);
        }
        // TryGetComponent<LimbBehaviour> is always false: an ExplosiveBehaviour on a limb stops at link time.
        let position = self.explosive_position(e);
        let under_water = p.is_some_and(|p| self.physicals[p].IsUnderWater) | self.is_point_under_water([position[0], position[1]]);
        let s = &self.explosives[e];
        let ex = ExplosionParameters {
            FragmentationRayCount: s.FragmentationRayCount as u32,
            Position: position,
            Range: s.Range,
            FragmentForce: s.FragmentForce,
            CreateParticlesAndSound: s.ParticleEffectAndSound,
            DismemberChance: s.DismemberChance,
            EffectSize: if s.BigExplosion { EffectSize::Large } else { EffectSize::Medium },
            BallisticShrapnelCount: s.BallisticShrapnelCount,
        };
        self.create_explosion_with_water(under_water, ex);
        let s = &self.explosives[e];
        if s.BurnPower > 0.001 || s.ShockwaveLiftForce > 0.01 || s.AffectVictimTemperature.abs() > 1.0 {
            self.explosive_affect_victims(e, position, ex.Position);
        }
        // TryGetComponent<BloodContainer>: only a CirculationBehaviour on a limb is one that can be spawned.
        let s = &self.explosives[e];
        if !under_water {
            if let Some(vfx) = s.VFXOverride {
                self.spawn_prefab(vfx, position);
            }
        }
        let s = &self.explosives[e];
        if s.DestroyOnExplode {
            self.destroy_object(id);
        } else if disintegrate {
            if let Some(p) = p.filter(|&p| self.physicals[p].Disintegratable && !self.physicals[p].isDisintegrated) {
                self.physical_disintegrate(p);
            }
            if let Some(k) = self.explosives[e].Killzone {
                self.destroy_object(k);
            }
        } else if let Some(x) = p.and_then(|p| self.destroyables.iter().position(|x| x.PhysicalBehaviour == p)) {
            self.destroyable_break(x, [0.0, 0.0]);
        }
        self.explosives[e].hasExploded = true;
    }

    /// Collider2D.enabled = false on GetComponentsInChildren<Collider2D>() (the active objects below).
    fn explosive_disable_colliders(&mut self, e: usize) {
        let id = self.explosives[e].id;
        // The active objects below, depth first, each object's components in order.
        let colliders: Vec<i32> = self.with_component_instance(id, |_, inst, o| {
            fn collect(inst: &super::spawn::Instance, x: usize, out: &mut Vec<i32>) {
                if !inst.objects[x].active {
                    return;
                }
                for (k, (_, c)) in inst.prefab.objects[x].components.iter().enumerate() {
                    if matches!(c, Component::Collider2D(_)) {
                        out.push(inst.objects[x].components[k]);
                    }
                }
                for &child in &inst.prefab.objects[x].children {
                    collect(inst, child, out);
                }
            }
            let mut out = Vec::new();
            collect(inst, o, &mut out);
            out
        });
        for collider in colliders {
            // A collider of this object's physical, or of a child's own PhysicalBehaviour.
            let body = match self.physicals.iter().find(|q| !q.destroyed && q.all_colliders.contains(&collider)) {
                Some(q) => q.rigidbody.body,
                None => match self.bare_colliders[&collider].anchor {
                    super::BareAnchor::Physical(q) => self.physicals[q].rigidbody.body,
                    super::BareAnchor::Node(n) => self.node_rigidbodies.iter().find(|r| r.node == n).map(|r| r.rigidbody.body).expect("a Transform node's body"),
                    super::BareAnchor::Root => unimplemented!("ExplosiveBehaviour: disabling a collider without a Rigidbody2D"),
                },
            };
            self.collider_disable_one(collider, body);
        }
    }

    /// The BurnPower / ShockwaveLiftForce / AffectVictimTemperature pass over the colliders in range that see the
    /// explosion.
    fn explosive_affect_victims(&mut self, e: usize, position: [f32; 3], ex_position: [f32; 3]) {
        let s = &self.explosives[e];
        let (range, mask) = (s.Range, s.BlockingLayers);
        let (lift, burn, temperature) = (s.ShockwaveLiftForce, s.BurnPower, s.AffectVictimTemperature);
        let centre = V::new(position[0], position[1]);
        for (victim, _) in self.overlap_circle(centre, range, mask) {
            // PhysicalObjectsInWorldByTransform: the collider sits on the physical's own Transform.
            let Some(v) = victim.filter(|&v| self.physicals[v].in_world) else { continue };
            let at = self.physicals[v].transform.position;
            if self.linecast_count(V::new(at[0], at[1]), centre, mask) > 0 {
                continue;
            }
            let rb = self.physicals[v].rigidbody.clone();
            let force = rb.mass(&self.world) * lift;
            rb.add_force(&mut self.world, [0.0 * force, 1.0 * force]);
            let num = burn / 8.0 * self.random.value();
            if self.physicals[v].BurnProgress < num {
                self.physicals[v].BurnProgress = num;
            }
            if self.random.value() < burn {
                self.ignite_at(v, true, [ex_position[0], ex_position[1]]);
            }
            let t = self.physicals[v].Temperature;
            if (temperature > 0.0 && t < temperature) || (temperature < 0.0 && t > temperature) {
                let heat = 0.035 / self.heat_capacity(v);
                self.physicals[v].Temperature = mathf::lerp(t, temperature, heat);
            }
        }
    }

    /// ExplosionCreator.Explode(ex)
    pub fn explosion_creator_explode(&mut self, ex: ExplosionParameters) {
        let under_water = self.is_point_under_water([ex.Position[0], ex.Position[1]]);
        self.create_explosion_with_water(under_water, ex);
    }

    /// OnlyExplosionBehaviour.Start (ExplodeOnStart): Explode with Position = transform.position.
    pub fn only_explosion_start(&mut self, id: i32, mut ex: ExplosionParameters) {
        ex.Position = self.with_component_instance(id, |_, inst, o| inst.position(o));
        self.explosion_creator_explode(ex);
    }

    /// ExplosionCreator.CreateExplosionWithWater(isUnderWater, ex)
    pub fn create_explosion_with_water(&mut self, under_water: bool, ex: ExplosionParameters) {
        if under_water {
            unimplemented!("ExplosionCreator.CreateUnderwaterExplosionEffect");
        }
        self.create_fragmentation_explosion(ex);
    }

    /// ExplosionCreator.CreatePulseExplosion(position, force, range, soundAndEffects: false, breakObjects: false)
    pub(super) fn create_pulse_explosion(&mut self, position: [f32; 3], force: f32, range: f32) {
        self.camera_shake((d(force) * range * 2.0f32).f(), [position[0], position[1]]);
        // OverlapCircleNonAlloc into the 64-entry hitBuffer: Objects, Debris, CollidingDebris.
        let hits = self.overlap_circle(V::new(position[0], position[1]), range, (1 << 9) | (1 << 10) | (1 << 13));
        if hits.len() > 64 {
            unimplemented!("ExplosionCreator.CreatePulseExplosion: more colliders than its buffer");
        }
        for (_, collider) in hits {
            let Some(body) = self.world.collider_body(collider) else { continue };
            let Some(rb) = self.rigidbody_of_body(body) else { continue };
            let at = self.collider_position(collider);
            let v = [at[0] - position[0], at[1] - position[1], at[2] - position[2]];
            let sqr = (d(v[0]) * v[0] + d(v[1]) * v[1] + d(v[2]) * v[2]).f();
            if sqr < mathf::EPSILON {
                continue;
            }
            let num2 = mathf::sqrt(sqr);
            let dir = [v[0] / num2, v[1] / num2];
            let num3 = (d(force) / mathf::max(1.0, (d(sqr) / (d(range) * range)).f()) * 3.0f32).f();
            let num4 = mathf::min(rb.mass(&self.world), 1.0);
            rb.add_force_impulse(&mut self.world, [num3 * dir[0] * num4, num3 * dir[1] * num4]);
        }
    }

    /// ExplosionCreator.CreateFragmentationExplosion; the camera shake is presentation.
    fn create_fragmentation_explosion(&mut self, ex: ExplosionParameters) {
        if ex.CreateParticlesAndSound {
            let path = match ex.EffectSize {
                EffectSize::Small => "Prefabs/SmallExplosion",
                EffectSize::Large => "Prefabs/BigExplosion",
                EffectSize::Medium => "Prefabs/Explosion",
            };
            self.spawn_resource(path, ex.Position);
        }
        for i in 0..ex.FragmentationRayCount {
            self.fragmentation_ray(i, ex);
        }
        // The loop condition runs OverlapCircleNonAlloc again before each message; nothing it reads changes.
        let centre = V::new(ex.Position[0], ex.Position[1]);
        let hits = self.overlap_circle(centre, mathf::max(ex.Range / 10.0, 1.0), DEFAULT_RAYCAST_LAYERS);
        if hits.len() > 64 {
            unimplemented!("ExplosionCreator.CreateFragmentationExplosion: more colliders than its buffer");
        }
        for (physical, collider) in hits {
            // SendMessage("Decal", DecalInstruction(BlastMarkDecal, ClosestPoint(Position), 2)) to the collider's GameObject.
            let point = self.world.collider_closest_point(collider, centre);
            match physical {
                Some(p) => {
                    let ty = self.decal_descriptor("Blast mark");
                    self.physical_decal(p, ty, [point.x, point.y], 2.0, [1.0; 4]);
                }
                None if self.bare_colliders[&collider].receives(&["DecalControllerBehaviour", "DecalRenderer", "DecalRendererController"]) => {
                    unimplemented!("SendMessage Decal to the scripts of a collider without a PhysicalBehaviour")
                }
                None => {}
            }
        }
        if ex.BallisticShrapnelCount > 0 && self.shrapnel.is_some() {
            // GlobalShrapnelEmitter.EmitShrapnel: StartCoroutine(EmitRoutine) up to its WaitForEndOfFrame.
            self.coroutine_seq += 1;
            let step = super::destroyable::Step::ShrapnelEndOfFrame([ex.Position[0], ex.Position[1]], ex.BallisticShrapnelCount);
            self.end_of_frame.push(super::destroyable::Coroutine { seq: self.coroutine_seq, at: self.time.timeAsDouble, owner: super::ballistics::SHRAPNEL_OWNER, step });
        }
    }

    /// ExplosionCreator.FragmentationRay
    fn fragmentation_ray(&mut self, index: u32, ex: ExplosionParameters) {
        let force = ex.FragmentForce;
        // 360f * ((float)index / (float)rayCount) * ((float)Math.PI / 180f), in double.
        let f = (360.0f64 * (index as f32 as f64 / ex.FragmentationRayCount as f32 as f64) * mathf::DEG2RAD as f64) as f32;
        let direction = [(f as f64).cos() as f32, (f as f64).sin() as f32];
        let origin = V::new(ex.Position[0], ex.Position[1]);
        let Some(hit) = self.raycast_first(origin, direction, ex.Range) else { return };
        self.broadcast_on_fragment_hit(hit.collider, hit.game_object, force);
        let position = self.collider_position(hit.collider);
        if let Some(rb) = self.rigidbody_of_body(hit.body) {
            let n = normalized3([position[0] - ex.Position[0], position[1] - ex.Position[1], position[2] - ex.Position[2]]);
            rb.add_impulse_at_position(&mut self.world, [n[0] * force, n[1] * force], [hit.point.x, hit.point.y]);
        }
        // transform.CompareTag("Limb"): the limbs.
        let Some(l) = self.physical_of_collider(hit.collider).and_then(|p| self.limbs.iter().position(|l| l.PhysicalBehaviour == p)) else { return };
        let kind = if self.random.value() > 0.5 { super::misc::damage_type::BULLET } else { super::misc::damage_type::BLUNT };
        let intensity = force * self.random.range_int(3, 8) as f32;
        self.skin_add_damage_point(l, kind, [hit.point.x, hit.point.y], intensity);
        self.limb_damage(l, force * 1.5);
        if self.limbs[l].SpeciesIdentity != "Android" && self.random.value() < ex.DismemberChance {
            if self.prefs.ProceduralFragments && hit.distance < 3.0 && (self.random.value() as f64) < 0.25 {
                unimplemented!("ExplosionCreator.FragmentationRay: LimbBehaviour.Crush after Utils.DelayCoroutine");
            }
            self.limb_slice(l);
        }
    }

    /// BroadcastMessage("OnFragmentHit", force) on the collider's GameObject: it and its active descendants, each
    /// object's components in order (DestroyableBehaviour, DamagableMachineryBehaviour and ExplosiveBehaviour are the
    /// receivers among the rewritten scripts).
    fn broadcast_on_fragment_hit(&mut self, collider: i32, game_object: i32, force: f32) {
        let Some(p) = self.physical_of_collider(collider) else {
            if self.bare_colliders[&collider].receives(&["DestroyableBehaviour", "DamagableMachineryBehaviour", "ExplosiveBehaviour", "TyreDeflationBehaviour", "GlowtubeBehaviour", "BulbBehaviour"]) {
                unimplemented!("BroadcastMessage OnFragmentHit to the scripts of a collider without a PhysicalBehaviour");
            }
            return;
        };
        let id = self.physicals[p].id;
        let receivers: Vec<Behaviour> = self.with_component_instance(id, |g, inst, _| {
            let start = inst.objects.iter().position(|x| x.game_object == game_object).expect("the collider's GameObject");
            let mut out = Vec::new();
            for x in std::iter::once(start).chain(inst.descendants(start)) {
                if !inst.objects[x].active {
                    continue;
                }
                for k in 0..inst.objects[x].components.len() {
                    if let Some(&(_, b)) = g.behaviour_of.get(&inst.objects[x].components[k]) {
                        if matches!(b, Behaviour::Destroyable(_) | Behaviour::Machinery(_) | Behaviour::Explosive(_) | Behaviour::Tyre(_) | Behaviour::Glowtube(_) | Behaviour::Bulb(_)) {
                            out.push(b);
                        }
                    }
                }
            }
            out
        });
        for b in receivers {
            match b {
                Behaviour::Destroyable(x) => self.destroyable_on_fragment_hit(x, force),
                Behaviour::Machinery(m) => self.machinery_on_fragment_hit(m, force),
                Behaviour::Explosive(e) => self.explosive_on_fragment_hit(e),
                Behaviour::Tyre(t) => self.tyre_on_fragment_hit(t, force),
                // GlowtubeBehaviour.OnFragmentHit
                Behaviour::Glowtube(g) => self.glowtube_break(g),
                Behaviour::Bulb(b) => self.bulb_on_fragment_hit(b),
                _ => unreachable!(),
            }
        }
    }

    /// Instantiate(Resources.Load<GameObject>(path), position, identity); the clone's first component id.
    pub(super) fn spawn_resource(&mut self, path: &str, position: [f32; 3]) -> i32 {
        let db = self.assets.take().expect("Instantiate without the game's data");
        let source = db.resource(path, 1).ok().flatten().unwrap_or_else(|| panic!("Resources.Load({path}) found no GameObject"));
        self.assets = Some(db);
        self.spawn_prefab(source, position)
    }

    /// Object.Instantiate(prefab, position, identity). Returns the clone root's Transform id.
    pub(super) fn spawn_prefab(&mut self, source: ObjRef, position: [f32; 3]) -> i32 {
        let mut db = self.assets.take().expect("Instantiate without the game's data");
        let spawned = db.prefab(source).and_then(|prefab| self.spawn(&mut db, &prefab, position, [0.0, 0.0, 0.0, 1.0], None));
        self.assets = Some(db);
        let (inst, _) = spawned.unwrap_or_else(|e| panic!("Instantiate: {e}"));
        let id = inst.objects[0].components[0];
        self.unsynced.push(inst);
        id
    }

    /// DeleteAfterTime.Update
    pub fn delete_after_update(&mut self, i: usize) {
        let dt = self.time.deltaTime;
        let s = &mut self.delete_afters[i];
        s.time += dt;
        if s.time > s.Life {
            if !s.OnEndOfLifePresentation {
                unimplemented!("DeleteAfterTime.OnEndOfLife: a persistent call beyond audio and particles");
            }
            let id = s.id;
            self.destroy_object(id);
        }
    }

    /// ExistInDetailView.Start: SetActive(Global.main.ShowLimbStatus), which is off.
    pub fn exist_in_detail_view_start(&mut self, id: i32) {
        self.with_component_instance(id, |g, inst, o| g.set_active_scripts(inst, o, false));
    }

    /// IgnoreParentSize.Start: it parents the object to constraints and the parent's ActOnDestroy, which only an
    /// active Killzone would reach (ExistInDetailView deactivates it first).
    pub fn ignore_parent_size_start(&mut self, id: i32) {
        if self.with_component_instance(id, |_, inst, o| inst.objects[o].active) {
            unimplemented!("IgnoreParentSize.Start");
        }
    }
}
