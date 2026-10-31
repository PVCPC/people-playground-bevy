//! ArchelixCasterBehaviour (the Archelix Caster) and ArchelixCasterBoltBehaviour (its pooled bolt). A Use starts the
//! BurstShot coroutine: BurstShotCount bursts of BulletsPerShot bolts from the pool (pool.rs), BurstShotInterval apart.
//! A bolt wobbles along its Direction (a sine of Frequency/Amplitude), homes on warm objects every 4th FixedUpdate,
//! and on a hit pulses, requests its Impact effect, slices what it hit and may cut a limb. It then despawns: its
//! trail must have faded (TrailRenderer.positionCount == 0) before it is Returned to the pool, so the trail's points are
//! modelled here (the native TrailRenderer: AddPoint on a moved transform, RemoveOldPoints against m_Time).
//! Sounds, the muzzle flash and the particles are presentation.
#![allow(non_snake_case)]

use super::coroutine::{Routine, Yield};
use super::explosion::RayHit2D;
use super::translated::rt::Xf;
use super::{d, mathf, script, Behaviour, Game};
use crate::assets::db::{Component, ObjRef};
use crate::b2::math::Vec2 as V;
use std::collections::VecDeque;

script! {
    ArchelixCaster {
        BurstShotCount: i32 = 3,
        BulletsPerShot: i32 = 2,
        BurstShotInterval: f32 = 0.1,
        LocalBarrelPosition: [f32; 2] = [0.0; 2],
        LocalBarrelDirection: [f32; 2] = [0.0; 2],
        ScreenShakeIntensity: f32 = 0.0,
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// Behaviour.enabled
        enabled: bool = true,
        /// PhysicalBehaviour (and the Rigidbody2D of the same object).
        physical: usize = 0,
        /// ShootSounds.Length (PickRandom draws).
        ShootSounds: usize = 0,
        /// MuzzleFlash is assigned (a null one is a NullReferenceException at Play).
        MuzzleFlash: bool = false,
        /// projectilePrefab
        projectilePrefab: Option<ObjRef> = None,
        /// The private phaseOffset.
        phaseOffset: bool = false,
    }
}

script! {
    ArchelixBolt {
        Phase: f32 = 0.0,
        Frequency: f32 = 5.0,
        Amplitude: f32 = 1.0,
        AoeRadius: f32 = 10.0,
        HomingForce: f32 = 1.0,
        MaxTimeUntilDeath: f32 = 0.0,
        HomingTemperatureThreshold: f32 = 30.0,
        Speed: f32 = 0.0,
        Direction: [f32; 2] = [0.0; 2],
    }
    links {
        /// The component's instance id.
        id: i32 = 0,
        /// BaseBoltBehaviour.Layers (LayerMask bits).
        layers: u32 = 0,
        ImpactPoolable: Option<ObjRef> = None,
        frame: i32 = 0,
        timeAlive: f32 = 0.0,
        isDespawning: bool = false,
        /// The pool (`Game::generic_pools`) that made the bolt; set by OnPoolableInitialised.
        pool: Option<usize> = None,
        /// The TrailRenderer's m_Time and m_MinVertexDistance.
        trail_time: f32 = 0.0,
        trail_min_distance: f32 = 0.0,
        /// TrailRenderer.positionCount's points: (time, x, y), oldest first.
        trail: VecDeque<(f32, [f32; 2])> = VecDeque::new(),
        /// The trail's Transform changed since the renderer's last update.
        trail_dirty: bool = false,
    }
}

impl Game {
    // ---- ArchelixCasterBehaviour ----------------------------------------------------------------------------

    /// ArchelixCasterBehaviour.Start: a projectile prefab without a pool gets one with MaxPoolSize 1024.
    pub fn archelix_caster_start(&mut self, c: usize) {
        let prefab = self.archelix_casters[c].projectilePrefab.unwrap_or_else(|| unimplemented!("ArchelixCasterBehaviour.Start: GetFor(null projectilePrefab) (NullReferenceException)"));
        if self.pool_get_for(prefab).is_none() {
            let s = self.pool_ensure(prefab);
            self.generic_pools[s].MaxPoolSize = 1024;
        }
    }

    /// ArchelixCasterBehaviour.Use: while enabled, Shoot.
    pub fn archelix_caster_use(&mut self, c: usize) {
        if self.archelix_casters[c].enabled {
            self.archelix_caster_shoot(c);
        }
    }

    /// ArchelixCasterBehaviour.Shoot: StartCoroutine(BurstShot()).
    fn archelix_caster_shoot(&mut self, c: usize) {
        if self.archelix_casters[c].enabled {
            let id = self.archelix_casters[c].id;
            self.start_coroutine(id, id, Self::archelix_burst_routine(c));
        }
    }

    /// BarrelPosition: Rigidbody.GetRelativePoint(LocalBarrelPosition * lossyScale).
    fn archelix_barrel_position(&mut self, c: usize) -> [f32; 2] {
        let p = self.archelix_casters[c].physical;
        self.rigidbody_relative_point(p, self.archelix_casters[c].LocalBarrelPosition)
    }

    /// BarrelDirection: transform.TransformVector(LocalBarrelDirection).
    fn archelix_barrel_direction(&self, c: usize) -> [f32; 2] {
        let p = self.archelix_casters[c].physical;
        self.transform_vector_chain(p, self.archelix_casters[c].LocalBarrelDirection)
    }

    /// ArchelixCasterBehaviour.BurstShot: the shoot sound (a draw), then for each burst while enabled the muzzle flash,
    /// the camera shake and BulletsPerShot bolts, each from the pool (null is a NullReferenceException), turned along
    /// BarrelDirection and given that Direction and a Phase that alternates 0 and PI / 2; then WaitForSeconds.
    fn archelix_burst_routine(c: usize) -> Routine {
        let mut started = false;
        let mut i = 0;
        Box::new(move |g| {
            if !started {
                started = true;
                // PlayClipOnce(ShootSounds.PickRandom()): audio, the draw stays.
                g.pick_random(g.archelix_casters[c].ShootSounds);
            } else {
                i += 1;
            }
            if i >= g.archelix_casters[c].BurstShotCount || !g.archelix_casters[c].enabled {
                return Yield::Done;
            }
            if !g.archelix_casters[c].MuzzleFlash {
                unimplemented!("ArchelixCasterBehaviour.BurstShot: MuzzleFlash is null (NullReferenceException)");
            }
            let (p, shake) = (g.archelix_casters[c].physical, g.archelix_casters[c].ScreenShakeIntensity);
            let at = g.physicals[p].transform.position;
            g.camera_shake(shake, [at[0], at[1]]);
            for _ in 0..g.archelix_casters[c].BulletsPerShot {
                let prefab = g.archelix_casters[c].projectilePrefab.expect("Start ran");
                let position = g.archelix_barrel_position(c);
                let (s, item) = g.pool_request_prefab(prefab, position).unwrap_or_else(|| unimplemented!("ArchelixCasterBehaviour.BurstShot: the pool is full, RequestPrefab returned null (NullReferenceException)"));
                let root = g.generic_pools[s].items[item].root;
                let dir = g.archelix_barrel_direction(c);
                g.tr_set_right(Xf::Object(root), [dir[0], dir[1], 0.0]);
                if let Some(b) = g.archelix_bolt_of_clone(root) {
                    let caster = &mut g.archelix_casters[c];
                    caster.phaseOffset = !caster.phaseOffset;
                    let phase = if caster.phaseOffset { std::f32::consts::PI / 2.0 } else { 0.0 };
                    let dir = g.archelix_barrel_direction(c);
                    g.archelix_bolts[b].Direction = dir;
                    g.archelix_bolts[b].Phase = phase;
                }
            }
            Yield::WaitForSeconds(g.archelix_casters[c].BurstShotInterval)
        })
    }

    /// The ArchelixCasterBoltBehaviour arena entry of the clone whose root Transform is `id` (TryGetComponent).
    fn archelix_bolt_of_clone(&mut self, id: i32) -> Option<usize> {
        self.with_component_instance(id, |g, inst, _| {
            for x in 0..inst.prefab.objects.len() {
                for (k, (_, c)) in inst.prefab.objects[x].components.iter().enumerate() {
                    if matches!(c, Component::Script { class, .. } if class == "ArchelixCasterBoltBehaviour") {
                        return match g.behaviour_of.get(&inst.objects[x].components[k]) {
                            Some(&(_, Behaviour::ArchelixBolt(i))) => Some(i),
                            _ => None,
                        };
                    }
                }
            }
            None
        })
    }

    // ---- ArchelixCasterBoltBehaviour ------------------------------------------------------------------------

    /// transform.right of the bolt.
    fn archelix_bolt_right(&mut self, b: usize) -> [f32; 2] {
        let id = self.archelix_bolts[b].id;
        let rotation = self.with_component_instance(id, |_, inst, o| inst.rotation(o));
        let right = mathf::rotate(rotation, [1.0, 0.0, 0.0]);
        [right[0], right[1]]
    }

    /// ArchelixCasterBoltBehaviour.Start
    pub fn archelix_bolt_start(&mut self, b: usize) {
        self.archelix_bolts[b].frame = self.random.range_int(0, 10000);
        self.archelix_bolts[b].Direction = self.archelix_bolt_right(b);
    }

    /// ArchelixCasterBoltBehaviour.OnPoolableInitialised: the trail is cleared and the transform it will see change
    /// is the Request's.
    pub fn archelix_bolt_on_poolable_initialised(&mut self, b: usize, pool: usize) {
        self.archelix_bolts[b].timeAlive = 0.0;
        self.archelix_bolts[b].trail.clear();
        self.archelix_bolts[b].pool = Some(pool);
        self.archelix_bolts[b].Direction = self.archelix_bolt_right(b);
        self.archelix_bolts[b].frame = self.random.range_int(0, 10000);
        self.archelix_bolts[b].isDespawning = false;
        self.archelix_bolts[b].trail_dirty = true;
    }

    /// ArchelixCasterBoltBehaviour.Update
    pub fn archelix_bolt_update(&mut self, b: usize) {
        if self.archelix_bolts[b].isDespawning {
            return;
        }
        let dt = self.time.deltaTime;
        self.archelix_bolts[b].timeAlive += dt;
        let s = &self.archelix_bolts[b];
        let (id, direction, speed, layers) = (s.id, s.Direction, s.Speed, s.layers);
        let arg = (d(self.time.time) * s.Frequency + s.Phase).f();
        let num = (d(mathf::sin(arg)) * s.Amplitude).f();
        // transform.right = Direction + Vector2.Perpendicular(Direction) * num
        let perp = [0.0 - direction[1], direction[0]];
        let right = [direction[0] + perp[0] * num, direction[1] + perp[1] * num];
        self.tr_set_right(Xf::Object(id), [right[0], right[1], 0.0]);
        self.archelix_bolts[b].trail_dirty = true;
        let num2 = speed * dt;
        let hit = self.base_bolt_cast(id, num2, layers);
        let hit_something = hit.is_some();
        if let Some(hit) = hit {
            self.archelix_bolt_on_hit(b, &hit);
            let point = [hit.point.x, hit.point.y, 0.0];
            self.with_component_instance(id, |_, inst, o| inst.set_world_position(o, point));
        }
        if hit_something || self.archelix_bolts[b].timeAlive > self.archelix_bolts[b].MaxTimeUntilDeath {
            self.start_coroutine(id, id, Self::archelix_despawn_routine(b));
        } else {
            self.base_bolt_advance(id, num2);
        }
    }

    /// ArchelixCasterBoltBehaviour.DespawnRoutine: isDespawning, ParticleSystem.Stop (presentation), wait until the
    /// trail has no points, then pool.Return(gameObject).
    fn archelix_despawn_routine(b: usize) -> Routine {
        let mut waiting = false;
        Box::new(move |g| {
            if !waiting {
                waiting = true;
                g.archelix_bolts[b].isDespawning = true;
                return Yield::WaitUntil(Box::new(move |g| g.archelix_bolts[b].trail.is_empty()));
            }
            let (pool, id) = (g.archelix_bolts[b].pool.expect("a bolt Requested from its pool"), g.archelix_bolts[b].id);
            g.pool_return(pool, id);
            Yield::Done
        })
    }

    /// ArchelixCasterBoltBehaviour.FixedUpdate
    pub fn archelix_bolt_fixed_update(&mut self, b: usize) {
        if self.archelix_bolts[b].isDespawning {
            return;
        }
        self.archelix_bolts[b].frame += 1;
        if self.archelix_bolts[b].frame % 4 != 0 {
            return;
        }
        let id = self.archelix_bolts[b].id;
        let position = self.with_component_instance(id, |_, inst, o| inst.position(o));
        if self.is_point_under_water([position[0], position[1]]) {
            // Direction += 6f * Time.deltaTime * (Vector2)Utils.GetPerlin2Mapped(position.x * 23.324f, position.y * 23.324f)
            let k = 6.0f32 * self.time.deltaTime;
            let n = mathf::perlin2_mapped(position[0] * 23.324f32, position[1] * 23.324f32);
            let s = &mut self.archelix_bolts[b];
            s.Direction = [s.Direction[0] + k * n[0], s.Direction[1] + k * n[1]];
        }
        let direction = self.archelix_bolts[b].Direction;
        let (radius, threshold, force) = (self.archelix_bolts[b].AoeRadius, self.archelix_bolts[b].HomingTemperatureThreshold, self.archelix_bolts[b].HomingForce);
        for item in self.physics_objects_near_position_accurate([position[0], position[1]], radius) {
            if self.physicals[item].destroyed {
                continue;
            }
            let temperature = self.physicals[item].Temperature;
            if temperature < (d(self.AmbientTemperature) + threshold).f() {
                continue;
            }
            let closest = self.physical_closest_point(item, V::new(position[0], position[1]));
            let vector = [closest[0] - position[0], closest[1] - position[1]];
            let sq = mathf::sqr_magnitude(vector);
            let num = mathf::sqrt(sq);
            let v2 = [vector[0] / num, vector[1] / num];
            let dot = (d(v2[0]) * direction[0] + d(v2[1]) * direction[1]).f();
            if !(num < 1.0) && !(dot < -0.7f32) {
                let v2 = [v2[0] / sq, v2[1] / sq];
                // HomingForce * Clamp01(Temperature) * Clamp01(timeAlive * 1.5f) * sqrMagnitude, times the vector.
                let c = mathf::clamp01(self.archelix_bolts[b].timeAlive * 1.5f32);
                let k = (d(force) * mathf::clamp01(temperature) * c * sq).f();
                let s = &mut self.archelix_bolts[b];
                s.Direction = [s.Direction[0] + k * v2[0], s.Direction[1] + k * v2[1]];
            }
        }
    }

    /// ArchelixCasterBoltBehaviour.OnHit: a pulse explosion, the Impact effect, Slice on what was hit, and for a
    /// limb a Random.value > 0.5 freeze and a Cut at a random point of its area.
    fn archelix_bolt_on_hit(&mut self, b: usize, hit: &RayHit2D) {
        let point = [hit.point.x, hit.point.y, 0.0];
        // CreatePulseExplosion(hit.point, 1f, 1f, soundAndEffects: false): breakObjects defaults to true.
        self.create_pulse_explosion_with(point, 1.0, 1.0, false, true);
        let impact = self.archelix_bolts[b].ImpactPoolable.unwrap_or_else(|| unimplemented!("ArchelixCasterBoltBehaviour.OnHit: RequestPrefab(null ImpactPoolable) (NullReferenceException)"));
        self.request_pooled_effect(impact, "ArchelixCasterBoltBehaviour.ImpactPoolable");
        // hit.collider.SendMessage("Slice"): only physical objects have receivers.
        let Some(p) = self.physical_of_collider(hit.collider) else { return };
        self.send_slice(p);
        let id = self.physicals[p].id;
        if self.tr_has_script(id, "LimbBehaviour") {
            let l = self.limbs.iter().position(|x| x.PhysicalBehaviour == p).unwrap_or_else(|| unimplemented!("LimbBehaviour without its limb"));
            if self.random.value() > 0.5 {
                self.physicals[p].Temperature = -273.0;
                self.limbs[l].InternalTemperature = -273.0;
            }
            let position = self.physicals[p].transform.position;
            let area = self.physicals[p].ObjectArea;
            let r = self.random.inside_unit_circle();
            let cut_at = [position[0] + area * r[0], position[1] + area * r[1]];
            let direction = self.random.inside_unit_circle();
            self.cut(l, cut_at, direction);
        }
    }

    // ---- the bolt's TrailRenderer ----------------------------------------------------------------------------

    /// What the engine does to each active bolt's TrailRenderer after the frame's scripts (UnityPlayer.dll, the renderer
    /// job CalculateWorldMatrixAndBoundsJob 180eae530 and RendererCullingOutputReady 180eb1020): when the trail's
    /// Transform changed, a first point is added at the time `now`, or one past m_MinVertexDistance from the last; then
    /// RemoveOldPoints(now) drops the oldest while `now > second-oldest.time + m_Time` (the only point counts as
    /// the second-oldest). A trail whose Transform did not change is aged by the culling callback, which only runs
    /// for a renderer the camera sees.
    pub fn archelix_trails_update(&mut self) {
        let now = self.time.timeAsDouble as f32;
        for b in 0..self.archelix_bolts.len() {
            let id = self.archelix_bolts[b].id;
            if self.archelix_bolts[b].trail.is_empty() && !self.archelix_bolts[b].trail_dirty {
                continue;
            }
            if self.destroyed_components.contains(&id) || !self.tr_active_in_hierarchy(super::translated::rt::Go::Local(id)) {
                continue;
            }
            if self.archelix_bolts[b].trail_dirty {
                let position = self.with_component_instance(id, |_, inst, o| inst.position(o));
                let s = &mut self.archelix_bolts[b];
                let at = [position[0], position[1]];
                let add = match s.trail.back() {
                    None => true,
                    Some(&(_, last)) => {
                        let (dx, dy) = (last[0] - at[0], last[1] - at[1]);
                        s.trail_min_distance * s.trail_min_distance < dx * dx + dy * dy + position[2] * position[2]
                    }
                };
                if add {
                    s.trail.push_back((now, at));
                }
                s.trail_dirty = false;
            } else if !self.archelix_trail_visible(b) {
                continue;
            }
            let s = &mut self.archelix_bolts[b];
            while let Some(&(t0, _)) = s.trail.get(1).or(s.trail.front()) {
                if now > t0 + s.trail_time {
                    s.trail.pop_front();
                } else {
                    break;
                }
            }
        }
    }

    /// Whether the camera's culling result holds the bolt's trail: its bounds (the points, grown by half the line's
    /// width) against the view rectangle. Without a view every renderer is drawn; a trail within 2 units of the
    /// view's edge, where the unread width decides, is not decided.
    fn archelix_trail_visible(&self, b: usize) -> bool {
        let Some((min, max)) = self.input.view else { return true };
        let trail = &self.archelix_bolts[b].trail;
        let mut lo = [f32::MAX; 2];
        let mut hi = [f32::MIN; 2];
        for &(_, p) in trail {
            for i in 0..2 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
        }
        let overlaps = |m: f32| (0..2).all(|i| hi[i] + m >= min[i] && lo[i] - m <= max[i]);
        if overlaps(0.0) {
            true
        } else if !overlaps(2.0) {
            false
        } else {
            unimplemented!("ArchelixCasterBoltBehaviour: whether the TrailRenderer's bounds (its width) reach the camera's view")
        }
    }
}
