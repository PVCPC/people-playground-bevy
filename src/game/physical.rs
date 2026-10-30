//! PhysicalBehaviour and the Utils energy/temperature helpers.
use super::{d, mathf, script, BareAnchor, Game, Pose, Rigidbody};
use crate::unity::Random;
use crate::b2::contacts2d::{ContactPoint2D, ScriptCollision};
use crate::b2::world::BodyType;

/// SharpAxis
#[derive(Clone, Copy, Debug, Default)]
pub struct SharpAxis {
    pub Axis: [f32; 2],
    pub LowerLimit: f32,
    pub UpperLimit: f32,
    pub LooseUpperLimit: bool,
    pub LooseLowerLimit: bool,
}

/// PhysicalProperties (ScriptableObject): the material values the scripts read.
#[derive(Clone, Debug, Default)]
pub struct PhysicalProperties {
    pub name: String,
    /// PhysicMaterial friction, None without a material.
    pub friction: Option<f32>,
    /// PhysicMaterial bounciness (0 without a material).
    pub bounciness: f32,
    pub Softness: f32,
    pub Flammability: f32,
    pub Conducting: bool,
    pub Burnrate: f32,
    pub BurningTemperatureThreshold: f32,
    pub HeatTransferSpeedMultiplier: f32,
    pub Buoyancy: f32,
    pub ImpactIntensityMutliplier: f32,
    pub HitVolumeMultiplier: f32,
    pub SparksOnSlide: bool,
    pub SoftImpact: usize,
    pub HardImpact: usize,
    /// SizzleSounds length; None when the array is null.
    pub SizzleSounds: Option<usize>,
    // The stabbing values.
    pub Sharp: bool,
    pub IgnoreStabResistance: bool,
    pub SharpForceThresholdMultiplier: f32,
    pub SharpAxes: Vec<SharpAxis>,
    /// LodgeStrengthMultiplier
    pub LodgeStrengthMultiplier: f32,
    /// StabSound length; None when the array is null.
    pub StabSound: Option<usize>,
    pub Brittleness: f32,
    pub BulletSpeedAbsorptionPower: f32,
    /// The ShotImpact prefab (None from a recording, which only names it) and whether it is "BloodPuff poolable"
    /// (`create_blood_puff`); None when null.
    pub ShotImpact: Option<(Option<crate::assets::db::ObjRef>, bool)>,
}

/// Instance ids of the components and objects PhysicalBehaviour.Awake/Start add (0 = none). A NodeBehaviour or
/// ContextMenuOptionComponent the prefab already carries is set by the spawner.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StartIds {
    pub ActivationNode: i32,
    pub ContextMenuOptions: i32,
    pub audioSource: i32,
    pub slidingAudioSource: i32,
    /// selectionOutlineObject (its Transform, Optout and SpriteRenderer take the next three ids)
    pub selectionOutlineObject: i32,
    pub outlineSpriteRenderer: i32,
}

/// PhysicalBehaviour.ColliderBoolPair; Coll is the Collider2D's instance id.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColliderBoolPair {
    pub Active: bool,
    pub Coll: i32,
}

script! {
    Physical {
        SimulateTemperature: bool = true,
        AmbientTemperatureTransfer: bool = true,
        OnFire: bool = false,
        DisplayBloodDecals: bool = true,
        BurnProgress: f32 = 0.0,
        isSliding: bool = false,
        EnergyWireResistance: f32 = 1.0,
        BuoyancyModifier: f32 = 1.0,
        IsWeightless: bool = false,
        charge: f32 = 0.0,
        ConductOverride: bool = true,
        ForceNoCharge: bool = false,
        StabCausesWound: bool = true,
        IsUnderWater: bool = false,
        underwaterMarkings: i32 = 0,
        IsInLava: bool = false,
        CurrentWaterSurfaceLevel: f32 = 0.0,
        seed: f32 = 0.0,
        intSeed: i32 = 0,
        burnIntensity: f32 = 0.0,
        slideIntensity: f32 = 0.0,
        previousVelocity: [f32; 2] = [0.0, 0.0],
        Temperature: f32 = 0.0,
        beingHeldByGripper: bool = false,
        Wetness: f32 = 0.0,
        GridRes: i32 = 4,
        ObjectArea: f32 = 0.0,
        TemperatureWhenBurningLerpMultiplier: f32 = 1.0,
        BurningProgressionMultiplier: f32 = 1.0,
        Circumference: f32 = -1.0,
        outOfBoundsDuration: f32 = 0.0,
        affectSurroundingTimer: f32 = 0.0,
        sizzleAudioHeat: f32 = 0.0,
        TrueInitialMass: f32 = 0.0,
        InitialMass: f32 = 0.0,
        InitialGravityScale: f32 = 0.0,
        isDisintegrated: bool = false,
        ChargeBurns: bool = true,
        ForceNoChargeParticles: bool = false,
        Deletable: bool = true,
        Selectable: bool = true,
        Disintegratable: bool = true,
        dirtyFire: bool = false,
        dirtyCharged: bool = false,
        dirtySmoke: bool = false,
        dirtyBubble: bool = false,
        SpawnSpawnParticles: bool = true,
        PlaySliderSound: bool = true,
        ForceContinuous: bool = false,
    }
    links {
        /// Object.GetHashCode(): Mono's object hash (UnityEngine.Object does not override it with the instance id),
        /// a heap-address value that differs from run to run; it seeds Start's Random.InitState, so it is an input
        /// (the recorder dumps it as `hash=`).
        hash_code: i32 = 0,
        /// The objects Awake and Start created, by instance id.
        ids: StartIds = StartIds::default(),
        /// ReflectsLasers and AbsorbsLasers (the dump format has neither).
        ReflectsLasers: bool = false,
        AbsorbsLasers: bool = true,
        /// An AudioSourceTimeScaleBehaviour sits on this object (the spawner sets it for prefab-carried ones).
        audio_time_scale: bool = false,
        /// rigidbody.sharedMaterial is not null in the prefab.
        rigidbody_has_material: bool = false,
        /// GetComponents<DecalControllerBehaviour>().Length at Start (prefab-carried ones).
        prefab_decal_controllers: usize = 0,
        /// spriteRenderer.bounds at Awake (center, extents).
        InitialBounds: ([f32; 3], [f32; 3]) = ([0.0; 3], [0.0; 3]),
        name: String = String::new(),
        id: i32 = 0,
        /// The GameObject's Transform instance id (0 for physicals loaded from a recording).
        transform_id: i32 = 0,
        BulletPenetration: bool = true,
        BulletDeflectChanceOverride: f32 = f32::NAN,
        /// OverrideShotSounds length.
        OverrideShotSounds: usize = 0,
        /// The Lodged/Dislodged receiver component on the GameObject (ExcaliburBehaviour, a SyringeBehaviour), if any.
        lodge_receiver: String = String::new(),
        /// PhysicalBehaviour.colliders: the GameObject's non-trigger Collider2D instance ids, in component order.
        colliders: Vec<i32> = Vec::new(),
        /// Every Collider2D on the GameObject (triggers too), as GetComponentsInChildren<Collider2D> sees them.
        all_colliders: Vec<i32> = Vec::new(),
        /// The serialized Collider2Ds behind `colliders` (BakeColliderGridPoints reads their geometry); empty for
        /// physicals loaded from a recording.
        collider_defs: Vec<crate::assets::components::Collider2D> = Vec::new(),
        /// gameObject.isStatic
        isStatic: bool = false,
        Properties: usize = 0,
        rigidbody: Rigidbody = Rigidbody { body: 0, bodyType: 0, collisionDetectionMode: 0, interpolation: 0, movement: super::MovementState::default(), detached: None, component: 0, simulated: true, detached_props: [0.0; 3] },
        transform: Pose = Pose::default(),
        /// The Transform's parent (index into Game.transform_nodes).
        parent: Option<usize> = None,
        /// Rigidbody2D m_DrivenByParentRigidbody (CheckForDrivenByParentRigidbody, 180c1dc80): for a non-static body,
        /// the b2 body of the nearest ancestor with a Rigidbody2D.
        driven_by: Option<usize> = None,
        onCollisionStayBuffer: [Option<ColliderBoolPair>; 8] = [None; 8],
        LocalColliderGridPoints: Option<Vec<[f32; 2]>> = None,
        stabWoundCount: u32 = 0,
        /// Indices into Game.penetration_arena.
        penetrations: Vec<usize> = Vec::new(),
        victimPenetrations: Vec<usize> = Vec::new(),
        /// The stabbers (physical indices) of the live penetrations into this object.
        beingStabbedBy: Vec<usize> = Vec::new(),
        StabReleaseSpeedModifier: f32 = 1.0,
        StabWoundSizeMultiplier: f32 = 1.0,
        /// The GameObject is active in the hierarchy (a recording's activeSelf: its physicals have no inactive
        /// parents).
        active: bool = true,
        enabled: bool = true,
        /// The GameObject was destroyed (Object.Destroy ran): removed from PhysicalObjectsInWorld.
        destroyed: bool = false,
        /// In Global.PhysicalObjectsInWorld: Start adds it (a spawned one is out until then).
        in_world: bool = true,
        /// Set when the oracle loads a component it has no data for.
        has_fire_map: bool = false,
        /// Its LocalFirePropagationMap (fire_maps); None in a recording.
        fire_map: Option<usize> = None,
        /// The DecalControllerBehaviours PhysicalBehaviour.Decal added, one per descriptor, in add order.
        decal_controllers: Vec<super::misc::DecalController> = Vec::new(),
        /// spriteRenderer's local AABB (center, extents); None when no sprite data was loaded.
        sprite_bounds: Option<([f32; 3], [f32; 3])> = None,
        /// For drawing: the spriteRenderer's Sprite, color and sorting order.
        sprite: Option<(crate::assets::db::ObjRef, [f32; 4], i16)> = None,
        /// For drawing: the spriteRenderer's flipX/flipY, drawMode (Simple 0, Sliced 1, Tiled 2) and size.
        sprite_draw: ([bool; 2], i32, [f32; 2]) = ([false; 2], 0, [0.0; 2]),
        MaximumStabVictims: i32 = 64,
        LooseEndSize: f32 = 0.49,
        /// OverrideImpactSounds length (0 when null).
        OverrideImpactSounds: usize = 0,
        /// The AudioSource AddComponent'ed in Awake (non-static objects).
        audioSource: bool = true,
        /// ContinuousActivationTracker, one DeltaInt per channel (Awake).
        ContinuousActivationTracker: [super::continuous::DeltaInt; 3] = [super::continuous::DeltaInt { Value: 0, PreviousValue: 0, PreviousValueFixed: 0 }; 3],
    }
}

/// Instance ids of a Prefabs/SpawnParticles clone: GameObject, Transform, ParticleSystem, ParticleSystemRenderer,
/// DeleteAfterTime.
const SPAWN_PARTICLES_OBJECTS: usize = 5;

impl Game {
    /// Global.PhysicalObjectsInWorldByTransform looked up by a collider (the collider's transform).
    pub fn physical_of_collider(&self, collider: i32) -> Option<usize> {
        self.physicals.iter().position(|p| !p.destroyed && p.all_colliders.contains(&collider))
    }
}

/// Utils.Mod
fn utils_mod(a: f32, b: f32) -> f32 {
    (d(a) - d(mathf::floor(a / b)) * b).f()
}

impl Game {
    /// Rigidbody2D.mass = v (Rigidbody2D::SetMass 180c23790): clamped, then b2Body::ResetMassData.
    pub(super) fn body_set_mass(&mut self, body: crate::b2::world::BodyId, v: f32) {
        let v = if 0.0001 <= v && v <= 1_000_000.0 { v } else if 0.0001 <= v { 1_000_000.0 } else { 0.0001 };
        self.world.body_mut(body).rigidbody.mass = v;
        self.world.reset_mass_data(body);
    }

    /// PhysicalBehaviour.Awake. The body, fixtures, colliders and Transform exist and the fields hold their prefab
    /// values. Not modelled: SortingGroup, ContinuousActivationTracker (activation propagation).
    pub fn physical_awake(&mut self, p: usize) {
        if self.physicals[p].ids.ActivationNode == 0 {
            self.physicals[p].ids.ActivationNode = self.allocate_instance_id();
        }
        if self.physicals[p].isStatic {
            self.physicals[p].PlaySliderSound = false;
        }
        if !self.physicals[p].enabled || !self.physicals[p].active {
            self.physicals[p].isDisintegrated = true;
        }
        self.physicals[p].Temperature = self.AmbientTemperature;
        self.physicals[p].InitialBounds = self.sprite_world_bounds(p);
        let body = self.physicals[p].rigidbody.body;
        self.physicals[p].TrueInitialMass = self.world.body(body).rigidbody.mass;
        // rigidbody.useAutoMass = false: a flag change resets the mass data.
        if self.world.body(body).rigidbody.use_auto_mass {
            self.world.body_mut(body).rigidbody.use_auto_mass = false;
            self.world.reset_mass_data(body);
        }
        if self.physicals[p].ids.ContextMenuOptions == 0 {
            self.physicals[p].ids.ContextMenuOptions = self.allocate_instance_id();
        }
    }

    /// The topmost Transform node above a physical object (None for a root).
    fn transform_root(&self, p: usize) -> Option<usize> {
        let (mut root, mut n) = (self.body_node(p), self.physicals[p].parent);
        while let Some(i) = n {
            root = Some(i);
            n = self.transform_nodes[i].parent;
        }
        root
    }

    /// GetComponentInParent<AudioSourceTimeScaleBehaviour>(): on the object or any ancestor (the root carries it).
    fn has_audio_time_scale_above(&self, p: usize) -> bool {
        let root = self.transform_root(p);
        self.physicals[p].audio_time_scale
            || root.is_some_and(|r| self.audio_time_scale_roots.contains(&r))
            || root.is_some() && (0..self.physicals.len()).any(|q| self.physicals[q].audio_time_scale && self.transform_root(q) == root)
    }

    /// PhysicalBehaviour.Start, run in Start order (`physicals` is Global.PhysicalObjectsInWorld, which Start
    /// appends to). Random.InitState(GetHashCode()) seeds with `hash_code`.
    pub fn physical_start(&mut self, p: usize) {
        self.physicals[p].in_world = true;
        if !self.has_audio_time_scale_above(p) {
            // transform.root.gameObject.AddComponent<AudioSourceTimeScaleBehaviour>()
            self.allocate_instance_id();
            self.physicals[p].audio_time_scale = true;
        }
        let body = self.physicals[p].rigidbody.body;
        if self.physicals[p].rigidbody.bodyType == 0 && self.physicals[p].LocalColliderGridPoints.as_ref().map_or(true, |g| g.is_empty()) {
            // The AutoSyncTransforms the first non-trigger collider's bounds starts with.
            if self.physicals[p].collider_defs.iter().any(|c| !c.is_trigger) {
                self.sync_pending();
            }
            let baked = self.physical_bake_collider_grid_points(p);
            self.physicals[p].LocalColliderGridPoints = Some(baked);
        }
        if self.physicals[p].prefab_decal_controllers != 0 {
            unimplemented!("PhysicalBehaviour.Start: decalControllers.AddRange(prefab DecalControllerBehaviours)");
        }
        self.random = Random::new(self.physicals[p].hash_code);
        let seed = self.random.range(-1500.0, 1500.0);
        let int_seed = self.random.range_int(-1500, 1500);
        self.physicals[p].seed = seed;
        self.physicals[p].intSeed = int_seed;
        self.physicals[p].affectSurroundingTimer = utils_mod(seed, 2.0);
        self.physicals[p].InitialGravityScale = self.world.body(body).gravity_scale;
        // Mathf.Sqrt(spriteRenderer.bounds.extents.sqrMagnitude * 4f)
        let e = self.sprite_world_bounds(p).1;
        let sqr = (d(e[0]) * e[0] + d(e[1]) * e[1] + d(e[2]) * e[2]).f();
        self.physicals[p].ObjectArea = mathf::sqrt(sqr * 4.0);
        // rigidbody.interpolation = Global.CurrentInterpolationMode (Interpolate: the game is not paused).
        self.physicals[p].rigidbody.interpolation = 1;
        self.physicals[p].InitialMass = self.physicals[p].rigidbody.mass(&self.world);
        if !self.physicals[p].isStatic {
            self.physicals[p].ids.audioSource = self.allocate_instance_id();
            self.physicals[p].audioSource = true;
            if self.physicals[p].PlaySliderSound {
                self.physicals[p].ids.slidingAudioSource = self.allocate_instance_id();
            }
            self.physical_set_properties(p);
            // CreateOutlineObject: new GameObject("Outline") (GameObject, Transform), Optout, SpriteRenderer.
            self.physicals[p].ids.selectionOutlineObject = self.allocate_instance_id();
            self.allocate_instance_id();
            self.allocate_instance_id();
            self.physicals[p].ids.outlineSpriteRenderer = self.allocate_instance_id();
        }
        // ShowOutline = false only deactivates the outline object (not modelled).
        // CollisionQuality.Continuous is 1.
        let mode = if self.prefs.CollisionQuality == 1 || self.physicals[p].ForceContinuous { 1 } else { 0 };
        let mut rb = self.physicals[p].rigidbody.clone();
        rb.set_collision_detection_mode(&mut self.world, mode);
        self.physicals[p].rigidbody = rb;
        self.physical_recalculate_mass_based_on_size(p);
        if self.physicals[p].IsWeightless {
            unimplemented!("PhysicalBehaviour.MakeWeightless");
        }
        if self.physicals[p].isDisintegrated {
            unimplemented!("PhysicalBehaviour.Disintegrate");
        }
        // SetupSpawnPrefab: Instantiate(Resources.Load("Prefabs/SpawnParticles")) under this transform, then
        // AddComponent<Optout>(). The clone is a particle effect that deletes itself after a second; it draws no
        // UnityEngine.Random (the grid jitter below matches the game without it).
        if self.physicals[p].SpawnSpawnParticles && !self.physicals[p].isStatic {
            for _ in 0..SPAWN_PARTICLES_OBJECTS + 1 {
                self.allocate_instance_id();
            }
        }
        // The context menu buttons are not modelled.
        for i in 0..self.physicals[p].LocalColliderGridPoints.as_ref().map_or(0, |g| g.len()) {
            let c = self.random.inside_unit_circle();
            let g = &mut self.physicals[p].LocalColliderGridPoints.as_mut().unwrap()[i];
            *g = [g[0] + c[0] * 0.02, g[1] + c[1] * 0.02];
        }
        if self.physicals[p].Circumference < 0.0 {
            self.physical_calculate_circumference(p);
        }
        // Instantiate(Global.main.MetalScrapePrefab, transform): a particle effect for OnCollisionStay to play.
        // Only its instance ids and its ActivateIfFancyEffects Awakes reach the simulation (load_main checks):
        // SetActive(Chance > Random.value && FancyEffects) on childless flash objects.
        if self.properties[self.physicals[p].Properties].SparksOnSlide {
            let (n, draws) = self.metal_scrape.unwrap_or_else(|| unimplemented!("PhysicalBehaviour.Start: MetalScrapePrefab outside a scene loaded from data"));
            for _ in 0..n {
                self.allocate_instance_id();
            }
            for _ in 0..draws {
                self.random.value();
            }
        }
    }

    /// PhysicalBehaviour.CalculateCircumference: the perimeter of LocalColliderGridPoints' convex hull
    /// (Utils.ComputeConvexHull sorts the points in place).
    fn physical_calculate_circumference(&mut self, p: usize) {
        let ph = &mut self.physicals[p];
        if ph.LocalColliderGridPoints.as_ref().unwrap().is_empty() {
            // The AutoSyncTransforms the first non-trigger collider's bounds starts with.
            if ph.collider_defs.iter().any(|c| !c.is_trigger) {
                self.sync_pending();
            }
            let baked = self.physical_bake_collider_grid_points(p);
            self.physicals[p].LocalColliderGridPoints = Some(baked);
        }
        let ph = &mut self.physicals[p];
        let points = ph.LocalColliderGridPoints.as_mut().unwrap();
        if points.is_empty() {
            ph.Circumference = 0.0;
            return;
        }
        let mut hull = Vec::new();
        super::shatter::compute_convex_hull(points, &mut hull);
        let mut num = 0.0f32;
        for i in 0..hull.len() {
            num += super::shatter::v2_distance(hull[i], hull[(i + 1) % hull.len()]);
        }
        ph.Circumference = num;
    }

    /// Collider2D.bounds (Collider2D::GetBounds, 180c11150): the union of the shapes' AABBs (ComputeAABB with the
    /// radius) as center and extents, z from the Transform. The caller runs the AutoSyncTransforms it starts with.
    fn collider_bounds(&self, p: usize, collider: i32) -> ([f32; 3], [f32; 3]) {
        let body = self.world.body(self.physicals[p].rigidbody.body);
        let mut aabb: Option<crate::b2::broadphase::Aabb> = None;
        for &f in &body.fixtures {
            let fx = self.world.fixture(f);
            if fx.collider.id != collider {
                continue;
            }
            for child in 0..fx.shape.child_count() {
                let a = fx.shape.compute_aabb(&body.xf, child);
                aabb = Some(match aabb {
                    None => a,
                    Some(b) => crate::b2::broadphase::Aabb {
                        lower: crate::b2::math::Vec2::new(b.lower.x.min(a.lower.x), b.lower.y.min(a.lower.y)),
                        upper: crate::b2::math::Vec2::new(b.upper.x.max(a.upper.x), b.upper.y.max(a.upper.y)),
                    },
                });
            }
        }
        let Some(a) = aabb else { unimplemented!("Collider2D.bounds of a collider without shapes") };
        let z = self.physicals[p].transform.position[2];
        ([(a.upper.x + a.lower.x) * 0.5, (a.upper.y + a.lower.y) * 0.5, z], [(a.upper.x - a.lower.x) * 0.5, (a.upper.y - a.lower.y) * 0.5, 0.0])
    }

    /// Collider2D.OverlapPoint (180c12440): any of the collider's shapes contains the point.
    fn collider_overlap_point(&self, p: usize, collider: i32, point: [f32; 2]) -> bool {
        let body = self.world.body(self.physicals[p].rigidbody.body);
        body.fixtures.iter().map(|&f| self.world.fixture(f)).filter(|fx| fx.collider.id == collider).any(|fx| super::tool::test_point(&fx.shape, &body.xf, crate::b2::math::Vec2::new(point[0], point[1])))
    }

    /// PhysicalBehaviour.BakeColliderGridPoints: a (GridRes + 1)^2 grid over the non-trigger colliders' bounds
    /// (shrunk by 0.999), keeping the points inside a collider and at least 0.025 from every earlier grid point,
    /// in local space; then each collider's outline points.
    fn physical_bake_collider_grid_points(&self, p: usize) -> Vec<[f32; 2]> {
        use crate::assets::components::ColliderShape;
        let ph = &self.physicals[p];
        if ph.collider_defs.len() != ph.colliders.len() {
            unimplemented!("{}: BakeColliderGridPoints without the serialized colliders", ph.name);
        }
        // Bounds(position, zero), Encapsulate(collider.bounds) each: Encapsulate(center - extents) and
        // Encapsulate(center + extents), each SetMinMax(Min(min, point), Max(max, point)).
        let (mut center, mut extents) = (ph.transform.position, [0.0f32; 3]);
        let mut encapsulate = |point: [f32; 3]| {
            let min: [f32; 3] = std::array::from_fn(|i| super::mathf::min(center[i] - extents[i], point[i]));
            let max: [f32; 3] = std::array::from_fn(|i| super::mathf::max(center[i] + extents[i], point[i]));
            extents = std::array::from_fn(|i| (max[i] - min[i]) * 0.5);
            center = std::array::from_fn(|i| min[i] + extents[i]);
        };
        // GetComponents<Collider2D>() where !isTrigger.
        let colls: Vec<(i32, &crate::assets::components::Collider2D)> = ph.colliders.iter().copied().zip(&ph.collider_defs).filter(|(_, c)| !c.is_trigger).collect();
        for &(c, _) in &colls {
            let (bc, be) = self.collider_bounds(p, c);
            encapsulate(std::array::from_fn(|i| bc[i] - be[i]));
            encapsulate(std::array::from_fn(|i| bc[i] + be[i]));
        }
        // bounds.size *= 0.999f
        extents = extents.map(|e| e * 2.0 * 0.999 * 0.5);
        let size = extents.map(|e| e * 2.0);
        let num2 = ph.GridRes.max(1);
        let step = [size[0] / num2 as f32, size[1] / num2 as f32];
        let min = [center[0] - extents[0], center[1] - extents[1]];
        let mut list = Vec::new();
        let mut seen: Vec<[f32; 2]> = Vec::new();
        for num3 in 0..num2 + 1 {
            for num4 in 0..num2 + 1 {
                let pos = [min[0] + num4 as f32 * step[0], min[1] + num3 as f32 * step[1]];
                if seen.iter().any(|&q| super::shatter::v2_distance(q, pos) < 0.025) {
                    continue;
                }
                seen.push(pos);
                if colls.iter().any(|&(c, _)| self.collider_overlap_point(p, c, pos)) {
                    let l = self.inverse_transform_point(p, [pos[0], pos[1], 0.0]);
                    list.push([l[0], l[1]]);
                }
            }
        }
        for &(_, c) in &colls {
            let o = c.offset;
            match &c.shape {
                ColliderShape::Polygon { paths, .. } => list.extend(paths.first().into_iter().flatten().map(|q| [q[0] + o[0], q[1] + o[1]])),
                ColliderShape::Box { size, .. } => {
                    let h = [size[0] / 2.0, size[1] / 2.0];
                    list.push([h[0] + o[0], h[1] + o[1]]);
                    list.push([(0.0 - h[0]) + o[0], h[1] + o[1]]);
                    list.push([(0.0 - h[0]) + o[0], (0.0 - h[1]) + o[1]]);
                    list.push([h[0] + o[0], (0.0 - h[1]) + o[1]]);
                }
                ColliderShape::Circle { radius } => {
                    let pi = std::f32::consts::PI;
                    let n = ((d(pi) * 2.0f32 * *radius / 0.25f32).f() as f64).ceil() as i32;
                    let n = n.clamp(6, 32);
                    for i in 0..n {
                        let f = (d(pi) * 2.0f32 * ((d(i as f32) / n as f32).f() - 1.0)).f();
                        let v = [((f as f64).cos() as f32) * radius, ((f as f64).sin() as f32) * radius];
                        list.push([v[0] + o[0], v[1] + o[1]]);
                    }
                }
                _ => {}
            }
        }
        list
    }

    /// PhysicalBehaviour.RecalculateMassBasedOnSize
    pub fn physical_recalculate_mass_based_on_size(&mut self, p: usize) {
        let s = self.lossy_scale(p);
        let ph = &mut self.physicals[p];
        ph.InitialMass = (d(ph.TrueInitialMass) * s[0].abs() * s[1].abs()).f();
        if !ph.IsWeightless {
            let (body, mass) = (ph.rigidbody.body, ph.InitialMass);
            self.body_set_mass(body, mass);
        }
    }

    /// PhysicalBehaviour.SetProperties: rigidbody.sharedMaterial = Properties.PhysicMaterial when it has none.
    /// Rigidbody2D::SetMaterial (180c23840) stores the material and runs Collider2D::UpdateColliderMaterialState
    /// (180c16380) on each attached collider: GetColliderMaterialState (180c11360) gives the collider's own material,
    /// else the body's (the prefab colliders have none, see spawn), the fixtures take its friction and bounciness and
    /// the collider refilters. The colliders are the body's fixtures' user data, sorted and made unique; the sort is
    /// by native address, modelled as fixture creation order (the allocation-order model of the heap order).
    fn physical_set_properties(&mut self, p: usize) {
        let props = &self.properties[self.physicals[p].Properties];
        let (Some(friction), false) = (props.friction, self.physicals[p].rigidbody_has_material) else { return };
        let bounciness = props.bounciness;
        self.physicals[p].rigidbody_has_material = true;
        let body = self.physicals[p].rigidbody.body;
        let fixtures = self.world.body(body).fixtures.clone();
        let mut colliders: Vec<i32> = Vec::new();
        for &f in &fixtures {
            let id = self.world.fixtures[f].as_ref().unwrap().collider.id;
            if !colliders.contains(&id) {
                colliders.push(id);
            }
        }
        for collider in colliders {
            for &f in &fixtures {
                let fixture = self.world.fixtures[f].as_mut().unwrap();
                if fixture.collider.id == collider {
                    (fixture.friction, fixture.restitution) = (friction, bounciness);
                }
            }
            self.world.refilter_collider(collider);
        }
    }
}

impl Physical {
    /// Charge getter
    pub fn Charge(&self) -> f32 {
        if !self.ForceNoCharge { self.charge } else { 0.0 }
    }
    pub fn GetChargeWithWireResistance(&self) -> f32 {
        self.Charge() * self.EnergyWireResistance
    }
}

impl Game {
    pub fn heat_capacity(&self, p: usize) -> f32 {
        let ph = &self.physicals[p];
        if !ph.SimulateTemperature {
            return 1.0;
        }
        ph.ObjectArea * mathf::max(0.1, 0.025 * ph.rigidbody.mass(&self.world))
    }

    fn buffer_entry_live(&self, pair: &Option<ColliderBoolPair>) -> Option<ColliderBoolPair> {
        pair.filter(|c| c.Active && self.collider_alive(c.Coll))
    }

    /// Collider2D != null: the object behind the instance id is not destroyed (a bare collider goes with its anchor
    /// physical's object).
    pub(super) fn collider_exists(&self, c: i32) -> bool {
        match self.physicals.iter().position(|p| p.all_colliders.contains(&c)) {
            Some(p) => !self.physicals[p].destroyed,
            None => !matches!(self.bare_colliders[&c].anchor, BareAnchor::Physical(p) if self.physicals[p].destroyed),
        }
    }

    /// `(bool)collider && collider.gameObject.activeSelf`.
    pub(super) fn collider_alive(&self, c: i32) -> bool {
        self.collider_exists(c)
            && match self.physical_of_collider(c) {
                Some(p) => self.physicals[p].active,
                None => self.bare_colliders[&c].active_self,
            }
    }

    /// SendMessage("OnCollisionExit2D", new Collision2D()) to a collider's GameObject.
    pub(super) fn send_empty_collision_exit(&mut self, c: i32) {
        match self.physical_of_collider(c) {
            Some(v) => self.physical_on_collision_exit(v, &ScriptCollision::default()),
            None if self.bare_colliders[&c].receives(&["TorchBehaviour"]) => unimplemented!("TorchBehaviour.OnCollisionExit2D"),
            None => {}
        }
    }

    /// PhysicalBehaviour.IsTouchingSomething
    pub fn is_touching_something(&self, p: usize) -> bool {
        self.physicals[p].onCollisionStayBuffer.iter().any(|c| self.buffer_entry_live(c).is_some())
    }

    /// Utils.TransferEnergyFixedRate
    pub fn transfer_energy_fixed_rate(&mut self, a: usize, b: usize, rate: f32) {
        let ca = self.physicals[a].GetChargeWithWireResistance();
        let cb = self.physicals[b].GetChargeWithWireResistance();
        if ca > cb {
            let v = mathf::lerp(self.physicals[b].Charge(), ca, rate);
            self.physicals[b].charge = v;
        } else {
            let v = mathf::lerp(self.physicals[a].Charge(), cb, rate);
            self.physicals[a].charge = v;
        }
    }

    /// Utils.AverageTemperature
    pub fn average_temperature(&mut self, a: usize, b: usize, transfer_factor: f32) {
        if !(self.physicals[a].SimulateTemperature && self.physicals[b].SimulateTemperature) {
            return;
        }
        let pa = &self.properties[self.physicals[a].Properties];
        let pb = &self.properties[self.physicals[b].Properties];
        let num = mathf::min(pa.HeatTransferSpeedMultiplier, pb.HeatTransferSpeedMultiplier) * transfer_factor;
        let (ta, tb) = (self.physicals[a].Temperature, self.physicals[b].Temperature);
        let ha = self.heat_capacity(a);
        self.physicals[a].Temperature = mathf::lerp(ta, tb, num / ha);
        let hb = self.heat_capacity(b);
        self.physicals[b].Temperature = mathf::lerp(tb, ta, num / hb);
    }

    /// PhysicalBehaviour.IgniteInternal
    fn ignite_internal(&mut self, p: usize, ignore_flammability: bool) {
        let ph = &self.physicals[p];
        let flammability = self.properties[ph.Properties].Flammability;
        if !ph.isStatic
            && (ignore_flammability || !(mathf::perlin(self.time.time * 100.0, ph.seed + 235.42) > flammability))
            && !(flammability <= mathf::EPSILON)
        {
            self.physicals[p].OnFire = true;
        }
    }

    /// PhysicalBehaviour.Ignite(bool, Vector2)
    pub fn ignite_at(&mut self, p: usize, ignore_flammability: bool, position: [f32; 2]) {
        self.ignite_internal(p, ignore_flammability);
        match self.physicals[p].fire_map {
            Some(f) if self.physicals[p].OnFire => self.fire_map_ignite_at(f, position),
            None if self.physicals[p].has_fire_map => unimplemented!("a recorded LocalFirePropagationMap (its Points are not recorded)"),
            _ => {}
        }
    }

    pub fn extinguish(&mut self, p: usize) {
        let threshold = self.properties[self.physicals[p].Properties].BurningTemperatureThreshold;
        let ph = &mut self.physicals[p];
        ph.Temperature = mathf::min(ph.Temperature, threshold);
        ph.OnFire = false;
    }

    /// PhysicalBehaviour.GetRandomGridPoint
    fn random_grid_point(&mut self, p: usize) -> [f32; 2] {
        match &self.physicals[p].LocalColliderGridPoints {
            None => [0.0, 0.0],
            Some(points) => {
                let n = points.len() as i32;
                // Utils.PickRandom
                if n == 0 {
                    return [0.0, 0.0];
                }
                let i = self.random.range_int(0, n) as usize;
                self.physicals[p].LocalColliderGridPoints.as_ref().unwrap()[i]
            }
        }
    }

    /// PhysicalBehaviour.ManagedFixedUpdate
    pub fn physical_fixed_update(&mut self, p: usize) {
        if self.physicals[p].isStatic {
            return;
        }
        self.physicals[p].charge *= 0.95;
        if self.prefs.CollisionQuality == super::COLLISION_DYNAMIC {
            let rb = &self.physicals[p].rigidbody;
            let fast = mathf::sqr_magnitude(rb.velocity(&self.world)) > self.DynamicSqrdVelocityThreshold;
            let mode = rb.collisionDetectionMode;
            if fast {
                if mode != 1 {
                    let mut rb = rb.clone();
                    rb.set_collision_detection_mode(&mut self.world, 1);
                    self.physicals[p].rigidbody = rb;
                }
            } else if mode != 0 {
                let mut rb = rb.clone();
                rb.set_collision_detection_mode(&mut self.world, 0);
                self.physicals[p].rigidbody = rb;
            }
        }
        let props = self.properties[self.physicals[p].Properties].clone();
        if self.physicals[p].SimulateTemperature {
            if self.physicals[p].AmbientTemperatureTransfer {
                if self.prefs.AmbientTemperatureTransfer {
                    self.TransferHeat(p);
                } else {
                    let h = self.heat_capacity(p);
                    let ambient = self.AmbientTemperature;
                    let ph = &mut self.physicals[p];
                    ph.Temperature = mathf::lerp(ph.Temperature, ambient, 0.0001 / h);
                }
            }
            let ph = &mut self.physicals[p];
            if ph.OnFire && ph.Temperature < props.BurningTemperatureThreshold * 3.0 {
                ph.Temperature = mathf::lerp(ph.Temperature, props.BurningTemperatureThreshold * 3.0, 0.01 * ph.TemperatureWhenBurningLerpMultiplier);
            }
            let flag = ph.BurnProgress <= 0.98;
            if flag && ph.Temperature > props.BurningTemperatureThreshold && props.Flammability > mathf::EPSILON {
                if ph.BurnProgress < 0.2 {
                    ph.BurnProgress += 0.001;
                }
                if self.random.value() as f64 > 0.9995 && props.Flammability > mathf::EPSILON {
                    let local = self.random_grid_point(p);
                    let point = self.transform_point(p, local);
                    self.ignite_at(p, true, point);
                }
            }
            let ph = &mut self.physicals[p];
            if ph.OnFire {
                let m = mathf::max(0.0, ph.Temperature - props.BurningTemperatureThreshold);
                ph.BurnProgress = (d(ph.BurnProgress) + d(m) * 6e-5f32 * (d(props.Burnrate) / 2.0f32) * ph.BurningProgressionMultiplier).f();
            }
            if ph.Temperature < props.BurningTemperatureThreshold {
                if flag && ph.OnFire {
                    ph.BurnProgress = (d(ph.BurnProgress) + d(2.5e-5) * (d(props.Burnrate) / 2.0f32) * ph.BurningProgressionMultiplier).f();
                }
                if ph.OnFire && self.random.value() > 0.995 {
                    self.extinguish(p);
                }
            }
            let ph = &mut self.physicals[p];
            ph.Temperature = (d(ph.Temperature) + d(ph.Charge()) * 0.0008f32 / props.HeatTransferSpeedMultiplier).f();
            // The original passes its arguments as Clamp(-273.15, Temperature, 5000000).
            ph.Temperature = mathf::clamp(-273.15, ph.Temperature, 5_000_000.0);
        }
        self.on_collision_stay_without_sleep(p);
        if self.physicals[p].IsUnderWater && self.random.value() > 0.95 && !self.physicals[p].decal_controllers.is_empty() {
            // ponytail: underwater decal fading is visual only; decals are not rewritten yet.
        }
        self.stab_fixed_update(p);
        if self.physicals[p].OnFire {
            if self.random.value() > 0.9993 {
                self.extinguish(p);
            }
            // FireLoopSoundControllerBehaviour.FuzzySetVolumeAt: sound only.
        }
    }

    /// PhysicalBehaviour.Ignite(bool)
    pub fn ignite(&mut self, p: usize, ignore_flammability: bool) {
        self.ignite_internal(p, ignore_flammability);
        match self.physicals[p].fire_map {
            Some(f) if self.physicals[p].OnFire => self.fire_map_ignite_random(f),
            None if self.physicals[p].has_fire_map => unimplemented!("a recorded LocalFirePropagationMap (its Points are not recorded)"),
            _ => {}
        }
    }

    /// PhysicalBehaviour.ManagedUpdate
    pub fn physical_update(&mut self, p: usize) {
        let dt = self.time.deltaTime;
        // audioSource / HandleSlidingSounds: sound only.
        let ph = &mut self.physicals[p];
        if ph.sizzleAudioHeat > 0.0 {
            ph.sizzleAudioHeat -= dt;
        }
        if ph.isStatic {
            return;
        }
        let burnrate = self.properties[ph.Properties].Burnrate;
        if ph.BurnProgress < 0.3 && ph.ChargeBurns {
            ph.BurnProgress = (d(ph.BurnProgress) + d(burnrate) * ph.Charge() * 0.01f32 * self.time.timeScale).f();
        }
        ph.Wetness = mathf::clamp01(ph.Wetness - dt);
        if ph.OnFire {
            ph.burnIntensity = (d(ph.burnIntensity) + d(dt) * 0.1f32).f();
            if ph.BurnProgress >= 0.995 {
                ph.OnFire = false;
            }
            ph.BurnProgress = mathf::clamp01(ph.BurnProgress);
            if ph.Wetness as f64 > 0.1 {
                self.extinguish(p);
                self.physicals[p].burnIntensity = 0.0;
            }
        } else {
            ph.burnIntensity = (d(ph.burnIntensity) - d(dt) * 0.1f32).f();
        }
        let ph = &mut self.physicals[p];
        ph.burnIntensity = mathf::clamp01(ph.burnIntensity);
        self.set_particle_emission(p);
        let ph = &mut self.physicals[p];
        ph.affectSurroundingTimer += dt;
        if ph.affectSurroundingTimer > 1.0 {
            ph.affectSurroundingTimer = 0.0;
            self.affect_surroundings(p);
            let ph = &mut self.physicals[p];
            if ph.Deletable && ph.rigidbody.bodyType == 0 {
                let position = ph.transform.position;
                if self.BoundingBox.contains_expanded([position[0], position[1]], 50.0) {
                    ph.outOfBoundsDuration = 0.0;
                } else {
                    ph.outOfBoundsDuration += 1.0;
                    // ponytail: Disintegrate / Destroy(root) are not rewritten yet.
                    assert!(ph.outOfBoundsDuration < 5.0, "out-of-bounds removal is not rewritten yet");
                }
            }
        }
        self.stab_update(p);
    }

    /// PhysicalBehaviour.SetParticleEmission: only the flags and the fire emission's Random draws matter.
    fn set_particle_emission(&mut self, p: usize) {
        let flammability = self.properties[self.physicals[p].Properties].Flammability;
        let ph = &mut self.physicals[p];
        if ph.isStatic {
            return;
        }
        if ph.dirtyFire {
            if ph.has_fire_map && ph.burnIntensity > 0.01 {
                let f = ph.fire_map.unwrap_or_else(|| unimplemented!("a recorded LocalFirePropagationMap (its Points are not recorded)"));
                self.fire_map_emission(p, f);
            }
        } else if ph.burnIntensity > 0.05 {
            ph.dirtyFire = true;
        }
        let ph = &mut self.physicals[p];
        if !ph.dirtyBubble && ph.Wetness as f64 > 0.01 {
            ph.dirtyBubble = true;
        }
        if !ph.ForceNoCharge && !ph.ForceNoChargeParticles {
            if !ph.dirtyCharged && ph.Charge() > 0.001 {
                ph.dirtyCharged = true;
            }
            if !ph.dirtySmoke && ph.Charge() * flammability > 0.001 {
                ph.dirtySmoke = true;
            }
        }
    }

    /// PhysicalBehaviour.AffectSurroundings
    fn affect_surroundings(&mut self, p: usize) {
        let ph = &self.physicals[p];
        if ph.isStatic || (!ph.OnFire && !(ph.Charge() > 0.001)) {
            return;
        }
        for item in self.physics_objects_near(p) {
            let (me, other) = (&self.physicals[p], &self.physicals[item]);
            if other.IsUnderWater && me.IsUnderWater && me.Charge() > other.Charge() {
                self.physicals[item].charge = me.Charge();
            }
            if self.physicals[p].OnFire && !self.physicals[item].OnFire && self.random.value() > 0.7 {
                self.ignite(item, false);
            }
        }
    }

    /// Global.GetChunkNear
    fn chunk_near(&mut self, position: [f32; 3], radius: f32) -> (i32, i32) {
        let num = radius / 2.0;
        let c = self.random.inside_unit_circle();
        let v = [(position[0] + c[0] * num) * (2.0f32 / 3.0), (position[1] + c[1] * num) * (2.0f32 / 3.0)];
        // Mathf.RoundToInt: Math.Round, ties to even.
        ((v[0] as f64).round_ties_even() as i32, (v[1] as f64).round_ties_even() as i32)
    }

    /// Global.GetChunkOf
    fn chunk_of(&mut self, p: usize) -> (i32, i32) {
        let ph = &self.physicals[p];
        self.chunk_near(ph.transform.position, ph.ObjectArea / 2.0)
    }

    /// Global.GetPhysicsObjectsNear. The game's iterator reads the chunk live; nothing in
    /// AffectSurroundings changes chunk membership, so a copy is the same.
    fn physics_objects_near(&mut self, me: usize) -> Vec<usize> {
        if !self.chunksUpToDate {
            for chunk in self.physical_chunks.values_mut() {
                chunk.clear();
            }
            for p in 0..self.physicals.len() {
                if !self.physicals[p].isDisintegrated {
                    let key = self.chunk_of(p);
                    let chunk = self.physical_chunks.entry(key).or_insert_with(|| Vec::with_capacity(128));
                    if chunk.len() < 128 {
                        chunk.push(p);
                    }
                }
            }
            self.chunksUpToDate = true;
        }
        let key = self.chunk_of(me);
        let Some(chunk) = self.physical_chunks.get(&key) else { return Vec::new() };
        chunk.iter().copied().filter(|&o| o != me && !self.physicals[o].isDisintegrated).collect()
    }

    /// PhysicalBehaviour.ManagedLateUpdate
    pub fn physical_late_update(&mut self, p: usize) {
        if !self.physicals[p].isStatic {
            let v = self.physicals[p].rigidbody.velocity(&self.world);
            self.physicals[p].previousVelocity = v;
        }
    }

    /// PhysicalBehaviour.OnCollisionStayWithoutSleep
    fn on_collision_stay_without_sleep(&mut self, p: usize) {
        if self.physicals[p].isStatic {
            return;
        }
        for i in 0..8 {
            let Some(pair) = self.physicals[p].onCollisionStayBuffer[i] else { continue };
            if !pair.Active {
                continue;
            }
            if !self.collider_alive(pair.Coll) {
                self.physicals[p].onCollisionStayBuffer[i].as_mut().unwrap().Active = false;
                continue;
            }
            // PhysicalObjectsInWorldByTransform.TryGetValue(coll.transform): the loop stops at the first collider
            // whose own object is no listed physical.
            let Some(v) = self.physical_of_collider(pair.Coll).filter(|&v| self.physicals[v].in_world) else { break };
            if self.physicals[p].SimulateTemperature && self.physicals[v].SimulateTemperature {
                self.average_temperature(p, v, 0.01);
            }
            let (me, other) = (&self.physicals[p], &self.physicals[v]);
            if !me.ForceNoCharge
                && me.Charge() > other.Charge()
                && me.ConductOverride
                && other.ConductOverride
                && self.properties[other.Properties].Conducting
                && self.properties[me.Properties].Conducting
            {
                let c = mathf::lerp(other.Charge(), me.Charge(), 0.8);
                self.physicals[v].charge = c;
            }
        }
    }

    /// SendMessage("Decal", new DecalInstruction(type, position, colour, size)) to a physical object:
    /// PhysicalBehaviour.Decal adds a DecalControllerBehaviour for a new descriptor and calls it, then every
    /// DecalControllerBehaviour on the object receives the message too (the new one finds its decal: IsNearOtherDecal).
    pub fn physical_decal(&mut self, p: usize, ty: usize, position: [f32; 2], size: f32, colour: [f32; 4]) {
        let size = mathf::max(0.05, size);
        let ph = &mut self.physicals[p];
        if ph.DisplayBloodDecals && self.prefs.Decals && ph.decal_controllers.iter().all(|c| c.descriptor != ty) {
            ph.decal_controllers.push(super::misc::DecalController { descriptor: ty, decals: Vec::new() });
            let c = self.physicals[p].decal_controllers.len() - 1;
            self.decal_controller_decal(p, c, ty, position, size, colour);
        }
        for c in 0..self.physicals[p].decal_controllers.len() {
            self.decal_controller_decal(p, c, ty, position, size, colour);
        }
    }

    /// DecalControllerBehaviour.Decal. CreateContainer and the sprite objects are presentation.
    fn decal_controller_decal(&mut self, p: usize, c: usize, ty: usize, position: [f32; 2], size: f32, colour: [f32; 4]) {
        if !self.prefs.Decals || self.physicals[p].decal_controllers[c].descriptor != ty {
            return;
        }
        let local = self.inverse_transform_point(p, [position[0], position[1], 0.0]);
        let local = [local[0], local[1]];
        // IsNearOtherDecal
        let ty = self.decal_descriptors[ty].clone();
        let radius = ty.IgnoreRadius * ty.IgnoreRadius;
        let at = self.transform_point3(p, [local[0], local[1], 0.0]);
        let near = self.physicals[p].decal_controllers[c].decals.iter().any(|decal| {
            let o = self.transform_point3(p, [decal.position[0], decal.position[1], 0.0]);
            let v = [at[0] - o[0], at[1] - o[1], at[2] - o[2]];
            (d(v[0]) * v[0] + d(v[1]) * v[1] + d(v[2]) * v[2]).f() < radius
        });
        if near {
            return;
        }
        let size = size * self.random.range(1.0, 1.2);
        // Sprites.PickRandom(); Color times the instruction's multiplier.
        let sprite = self.pick_random(ty.Sprites).unwrap();
        let mut colour = [ty.Color[0] * colour[0], ty.Color[1] * colour[1], ty.Color[2] * colour[2], ty.Color[3] * colour[3]];
        if self.prefs.GorelessMode {
            colour = super::circulation::change_red_to_orange(colour);
        }
        if ty.MaxDripCount > 0 {
            unimplemented!("DecalControllerBehaviour.Decal: {} drip streaks", ty.name);
        }
        self.physicals[p].decal_controllers[c].decals.push(super::misc::Decal { position: local, size, sprite, colour });
    }

    /// spriteRenderer.bounds: the sprite's local AABB through the object's transforms (center, extents).
    pub fn sprite_world_bounds(&self, p: usize) -> ([f32; 3], [f32; 3]) {
        let Some((c, e)) = self.physicals[p].sprite_bounds else {
            unimplemented!("SpriteRenderer.bounds of {}: the sprite's local bounds are not loaded", self.physicals[p].name)
        };
        let chain: Vec<_> = self.local_chain(p).into_iter().rev().collect();
        crate::unity::transform::transform_aabb(c, e, &chain)
    }

    /// PhysicalBehaviour.CreateImpactEffect
    pub fn create_impact_effect(&mut self, p: usize, position: [f32; 2], direction: [f32; 2], size: f32) {
        if mathf::sqr_magnitude(direction) <= f32::EPSILON || size.is_nan() {
            return;
        }
        let z = (d((direction[1] as f64).atan2(direction[0] as f64) as f32) * 57.29578f32 - 90.0f32).f();
        let props = &self.properties[self.physicals[p].Properties];
        let Some((source, blood_puff)) = props.ShotImpact else { unimplemented!("PhysicalBehaviour.CreateImpactEffect: a null ShotImpact on {} (NullReferenceException)", props.name) };
        if blood_puff {
            self.create_blood_puff(p, position, z, size);
            return;
        }
        // The rotation, the scale and OnImpactCreated (only a BloodImpactBehaviour on the clone has a receiver) are
        // presentation for these.
        let source = source.expect("CreateImpactEffect: a recording names the ShotImpact but holds no game data");
        let pool = self.effect_pool(source, "PhysicalProperties.ShotImpact");
        if self.splash_pools[pool].poolable {
            self.request_pooled_effect(source, "PhysicalBehaviour.CreateImpactEffect");
        } else {
            // Object.Instantiate(ShotImpact, position, identity)
            let colliding = self.splash_pools[pool].prefab.objects.iter().any(|ob| ob.components.iter().any(|(_, c)| matches!(c, crate::assets::db::Component::Collider2D(_) | crate::assets::db::Component::Rigidbody2D(_))));
            if colliding {
                unimplemented!("PhysicalBehaviour.CreateImpactEffect: a ShotImpact with physics (its scale and rotation)");
            }
            self.spawn_prefab(source, [position[0], position[1], 0.0]);
        }
    }

    /// The splash pool of an effect prefab, read from the data the first time.
    fn effect_pool(&mut self, source: crate::assets::db::ObjRef, what: &str) -> usize {
        let mut db = self.assets.take().unwrap_or_else(|| panic!("{what} without the game's data"));
        let pool = self.splash_pool_for(&mut db, source);
        self.assets = Some(db);
        pool.unwrap_or_else(|e| panic!("{what}: {e}"))
    }

    /// PoolGenerator.RequestPrefab(effect, position) of a particle effect: the position and rotation only place
    /// particles. `what` names the caller.
    pub(super) fn request_pooled_effect(&mut self, source: crate::assets::db::ObjRef, what: &str) {
        let pool = self.effect_pool(source, what);
        if !self.splash_pools[pool].poolable {
            unimplemented!("{what}: a pooled effect not tagged Poolable (its particle lifetime)");
        }
        for ob in &self.splash_pools[pool].prefab.objects {
            for (_, c) in &ob.components {
                if let crate::assets::db::Component::Script { class, .. } = c {
                    if !matches!(class.as_str(), "ExplosionSoundBehviour" | "PoolableParticleSystemHelper" | "PoolableSizeHelper" | "ActivateIfFancyEffects" | "Optout") {
                        unimplemented!("{what}: a pooled effect with a {class}");
                    }
                }
            }
        }
        self.splash_request(pool);
    }

    /// Transform.TransformPoint for a 2D point of a physical object's transform.
    /// ponytail: managed-style TRS; the engine's native hierarchy math (parent scale) is not rewritten yet.
    pub fn transform_point(&self, p: usize, local: [f32; 2]) -> [f32; 2] {
        let t = &self.physicals[p].transform;
        let s = [local[0] * t.localScale[0], local[1] * t.localScale[1], 0.0];
        let r = mathf::rotate(t.rotation, s);
        [r[0] + t.position[0], r[1] + t.position[1]]
    }
}

/// Collision2D.GetContacts(buffer): copies min(count, buffer length) contacts, stale entries stay.
pub fn get_contacts(c: &ScriptCollision, buffer: &mut [ContactPoint2D; 8]) -> usize {
    let n = c.contacts.len().min(buffer.len());
    buffer[..n].copy_from_slice(&c.contacts[..n]);
    n
}

/// Utils.GetFirstValidContact
pub fn get_first_valid_contact(buffer: &[ContactPoint2D; 8], count: usize) -> ContactPoint2D {
    for i in 0..count {
        let result = buffer[i];
        if !result.normal_impulse.is_nan() && !result.normal_impulse.is_infinite() {
            return result;
        }
    }
    buffer[0]
}

/// Utils.GetAverageImpulseRemoveOutliers (outlierThreshold = 1)
pub fn get_average_impulse_remove_outliers(buffer: &[ContactPoint2D; 8], count: usize) -> f32 {
    let outlier_threshold = 1.0f32;
    let mut num = 0.0f32;
    let mut num2 = 0i32;
    for i in 0..count {
        let normal_impulse = buffer[i].normal_impulse;
        if !normal_impulse.is_nan() && !normal_impulse.is_infinite() {
            num += normal_impulse;
            num2 += 1;
        }
    }
    let num3 = num / num2 as f32;
    if count >= 2 && outlier_threshold > mathf::EPSILON {
        for j in 0..count - 1 {
            let normal_impulse2 = buffer[j].normal_impulse;
            if (normal_impulse2 - num3).abs() > outlier_threshold {
                num -= normal_impulse2;
                num2 -= 1;
            }
        }
        return num / num2 as f32;
    }
    num3
}

/// Utils.GetMinImpulse
pub fn get_min_impulse(buffer: &[ContactPoint2D; 8], count: usize) -> f32 {
    let mut num = f32::MAX;
    for i in 0..count {
        let normal_impulse = buffer[i].normal_impulse;
        if !normal_impulse.is_nan() {
            num = mathf::min(num, normal_impulse);
        }
    }
    num
}

/// Vector2.Dot (rounded at its return).
pub fn dot2(a: [f32; 2], b: [f32; 2]) -> f32 {
    (d(a[0]) * b[0] + d(a[1]) * b[1]).f()
}

impl Game {
    /// Global.PhysicalObjectsInWorldByTransform.TryGetValue(collision.transform): None for a collider
    /// without a PhysicalBehaviour (and for the empty `new Collision2D()`).
    pub fn collision_physical(&self, c: &ScriptCollision) -> Option<usize> {
        if c.collider == 0 { None } else { self.physical_of_collider(c.collider) }
    }

    /// Rigidbody2D.velocity of the rigidbody with this instance id.
    pub fn rigidbody_velocity_by_id(&self, id: i32) -> [f32; 2] {
        for b in self.world.bodies.iter().flatten() {
            if b.user.map(|u| u.id) == Some(id) {
                return if b.kind == BodyType::Static { [0.0, 0.0] } else { [b.linear_velocity.x, b.linear_velocity.y] };
            }
        }
        [0.0, 0.0]
    }

    /// Transform.TransformVector for a 2D vector of a physical object's transform.
    pub(super) fn transform_vector(&self, p: usize, local: [f32; 2]) -> [f32; 3] {
        let t = &self.physicals[p].transform;
        mathf::rotate(t.rotation, [local[0] * t.localScale[0], local[1] * t.localScale[1], 0.0])
    }

    /// PhysicalBehaviour.TryGetFreeCollisionBufferIndex
    fn try_get_free_collision_buffer_index(&self, p: usize) -> Option<usize> {
        let buffer = &self.physicals[p].onCollisionStayBuffer;
        (0..8).find(|&i| buffer[i].is_none() || self.buffer_entry_live(&buffer[i]).is_none())
    }

    /// PhysicalBehaviour.CalculateSliding
    fn calculate_sliding(&mut self, p: usize, c: &ScriptCollision) {
        if self.physicals[p].isStatic {
            return;
        }
        self.physicals[p].isSliding = false;
        let rb = self.physicals[p].rigidbody.clone();
        if rb.bodyType != 0 {
            return;
        }
        let v = rb.velocity(&self.world);
        let num = match c.rigidbody {
            None => mathf::magnitude2(v),
            Some(id) => {
                let o = self.rigidbody_velocity_by_id(id);
                mathf::magnitude2([v[0] - o[0], v[1] - o[1]])
            }
        };
        let num2 = rb.angular_velocity(&self.world).abs() * 0.025;
        if !(mathf::max(num, num2) <= 0.05) {
            let a = if num2 > 0.05 && num < 2.0 { num2 } else { 0.0 };
            let b = if num2 < 2.0 && num > 0.05 { num } else { 0.0 };
            self.physicals[p].slideIntensity = mathf::max(a, b) / 40.0;
            if !((self.physicals[p].slideIntensity as f64) < 0.1) {
                self.physicals[p].isSliding = true;
            }
        }
    }

    /// PhysicalBehaviour.OnCollisionStay2D
    pub fn physical_on_collision_stay(&mut self, p: usize, c: &ScriptCollision) {
        if self.physicals[p].isStatic || self.collision_physical(c) == Some(p) {
            return;
        }
        self.calculate_sliding(p, c);
        if self.physicals[p].SimulateTemperature {
            let num = 0.0001f32 * self.properties[self.physicals[p].Properties].friction.map_or(0.5, |f| f * 2.0);
            let sq = mathf::sqr_magnitude([c.relative_velocity.x, c.relative_velocity.y]);
            let ph = &mut self.physicals[p];
            ph.Temperature = (d(ph.Temperature) + d(sq) * num).f();
        }
        // FancyEffects scrape sparks (scrapeParticles Play/Stop, contact position): particles only.
    }

    /// PhysicalBehaviour.OnCollisionExit2D
    pub fn physical_on_collision_exit(&mut self, p: usize, c: &ScriptCollision) {
        // scrapeParticles.Stop(): particles only.
        if self.physicals[p].isStatic {
            return;
        }
        if c.rigidbody.is_some() {
            for pair in self.physicals[p].onCollisionStayBuffer.iter_mut().flatten() {
                if pair.Coll == c.collider {
                    pair.Active = false;
                }
            }
        }
        self.physicals[p].isSliding = false;
        // slidingAudioSource.Stop(): sound only.
    }

    /// PhysicalBehaviour.OnCollisionEnter2D
    pub fn physical_on_collision_enter(&mut self, p: usize, c: &ScriptCollision) {
        if self.physicals[p].isStatic {
            return;
        }
        let value = self.collision_physical(c);
        if value != Some(p) {
            if let Some(index) = self.try_get_free_collision_buffer_index(p) {
                self.physicals[p].onCollisionStayBuffer[index] = Some(ColliderBoolPair { Active: true, Coll: c.collider });
            }
        }
        let mut buffer = self.physical_contact_buffer;
        let contacts = get_contacts(c, &mut buffer);
        self.physical_contact_buffer = buffer;
        if contacts == 0 {
            return;
        }
        let first_valid_contact = get_first_valid_contact(&buffer, contacts);
        let average_impulse = get_average_impulse_remove_outliers(&buffer, contacts);
        // CameraShakeBehaviour.main.Shake: camera only.
        self.handle_sounds(p, average_impulse);
        self.handle_stabbing(p, c, &first_valid_contact);
        if let Some(v) = value {
            let props = &self.properties[self.physicals[p].Properties];
            let (me, other) = (&self.physicals[p], &self.physicals[v]);
            if props.SizzleSounds.is_some()
                && other.SimulateTemperature
                && me.SimulateTemperature
                && (other.Temperature - me.Temperature).abs() > 10.0
                && other.Temperature > me.Temperature
                && other.Temperature >= props.BurningTemperatureThreshold
            {
                self.sizzle(p, true);
            }
            // The draw comes first in the original's condition.
            let roll = self.random.value();
            let props = &self.properties[self.physicals[p].Properties];
            let (me, other) = (&self.physicals[p], &self.physicals[v]);
            let conducting = (props.Conducting, self.properties[other.Properties].Conducting);
            if !(roll > 0.5)
                && me.Charge() > other.Charge()
                && me.ConductOverride
                && other.ConductOverride
                && conducting.1
                && conducting.0
                && (me.Charge() - other.Charge()).abs() > 15.0
            {
                let diff = (me.Charge() - other.Charge()).abs();
                let from = me.transform.position;
                let to = other.transform.position;
                let direction = super::person::normalized([to[0] - from[0], to[1] - from[1], to[2] - from[2]]);
                let k = 0.2f32 * diff;
                let impulse = [k * direction[0], k * direction[1]];
                let charge = me.Charge();
                let rb = other.rigidbody.clone();
                rb.add_force_impulse(&mut self.world, impulse);
                self.physicals[v].charge = charge;
                // Instantiate(Prefabs/BigZap): effect prefab only.
            }
        }
    }

    /// PhysicalBehaviour.Sizzle
    pub fn sizzle(&mut self, p: usize, with_sound: bool) {
        if !self.physicals[p].dirtySmoke {
            // SetupSmokePrefab: instantiates the smoke particle prefab (presentation).
            self.physicals[p].dirtySmoke = true;
        }
        // SmokeParticleSystem.Emit(5): particles only.
        if with_sound {
            if let Some(n) = self.properties[self.physicals[p].Properties].SizzleSounds {
                if n != 0 && self.physicals[p].sizzleAudioHeat <= mathf::EPSILON {
                    // PlayClipOnce(SizzleSounds.PickRandom(), 0.5f): only the draw matters.
                    self.pick_random(n);
                    self.physicals[p].sizzleAudioHeat = 1.0;
                }
            }
        }
    }

    /// PhysicalBehaviour.HandleSounds: the clip choice draws a random number, the playback is audio only.
    fn handle_sounds(&mut self, p: usize, impulse: f32) {
        if !self.physicals[p].audioSource {
            return;
        }
        let props = &self.properties[self.physicals[p].Properties];
        let num = impulse * props.ImpactIntensityMutliplier;
        if num < 0.002 {
            return;
        }
        let num2 = mathf::max(1.0, self.physicals[p].rigidbody.mass(&self.world));
        let flag = num > 0.5 / num2;
        let (hard, soft) = (props.HardImpact, props.SoftImpact);
        let over = self.physicals[p].OverrideImpactSounds;
        if over != 0 {
            self.pick_random(over);
        } else if flag {
            if hard != 0 {
                self.pick_random(hard);
            }
        } else if soft != 0 {
            self.pick_random(soft);
        }
    }

    /// PhysicalBehaviour.IsPointNearLooseSharpEnd
    fn is_point_near_loose_sharp_end(&self, p: usize, point: [f32; 2]) -> bool {
        let position = self.physicals[p].transform.position;
        let loose = self.physicals[p].LooseEndSize;
        let end = |axis: &SharpAxis, t: f32| {
            let v = self.transform_vector(p, axis.Axis);
            let b = [position[0] + v[0], position[1] + v[1]];
            // Vector2.LerpUnclamped
            let l = [(d(position[0]) + (d(b[0]) - position[0]) * t).f(), (d(position[1]) + (d(b[1]) - position[1]) * t).f()];
            mathf::sqr_magnitude([l[0] - point[0], l[1] - point[1]])
        };
        for axis in &self.properties[self.physicals[p].Properties].SharpAxes {
            if axis.LooseLowerLimit && end(axis, axis.LowerLimit) < loose * loose {
                return true;
            }
            if axis.LooseUpperLimit && end(axis, axis.UpperLimit) < loose * loose {
                return true;
            }
        }
        false
    }

    /// PhysicalBehaviour.HandleStabbing
    fn handle_stabbing(&mut self, p: usize, c: &ScriptCollision, contact: &ContactPoint2D) {
        let props = &self.properties[self.physicals[p].Properties];
        if !props.Sharp || self.physicals[p].penetrations.len() as i32 >= self.physicals[p].MaximumStabVictims {
            return;
        }
        let Some(v) = self.collision_physical(c) else { return };
        let victim = &self.properties[self.physicals[v].Properties];
        let relative_velocity = [c.relative_velocity.x, c.relative_velocity.y];
        if (victim.Softness <= 0.01 && !props.IgnoreStabResistance)
            || (victim.IgnoreStabResistance && props.IgnoreStabResistance)
            || mathf::magnitude2(relative_velocity) / props.SharpForceThresholdMultiplier
                < mathf::map_range(0.01, 1.0, 20.0, 1.25, if props.IgnoreStabResistance { 1.0 } else { mathf::sqrt(victim.Softness) })
            || !self.is_point_near_loose_sharp_end(p, [contact.point.x, contact.point.y])
        {
            return;
        }
        let normalized = mathf::normalized2(relative_velocity);
        let contact_normal = [contact.normal.x, contact.normal.y];
        if dot2(contact_normal, normalized) < -0.98 {
            return;
        }
        let hit = props.SharpAxes.iter().copied().find(|axis| {
            let vector = self.transform_vector(p, axis.Axis);
            let vector = [vector[0], vector[1]];
            dot2(normalized, vector) < -0.92 && dot2(contact_normal, vector) < -0.3
        });
        if let Some(axis) = hit {
            self.stab_begin(p, v, axis, c, contact);
        }
    }

    /// PhysicalBehaviour.Disintegrate
    pub fn physical_disintegrate(&mut self, p: usize) {
        let ph = &self.physicals[p];
        if ph.isStatic || !ph.Disintegratable || !ph.Deletable {
            return;
        }
        self.stab_disintegrate(p);
        // OnDisintegration: the limb's handler, then the person's DisintegrationCounterBehaviour (the other
        // subscriber, LimbStatusBehaviour, only hides the status UI).
        if let Some(l) = self.limbs.iter().position(|limb| limb.PhysicalBehaviour == p) {
            self.limb_on_disintegration(l);
            self.person_on_child_disintegrated(self.limbs[l].Person);
        }
        self.disintegration_counters_on_disintegration(p);
        // GetComponentsInChildren<Collider2D>().enabled = false: the object's own collider. Renderers are presentation.
        self.collider_disable(p);
        // Rigidbody2D.simulated = false (Rigidbody2D::SetSimulated -> b2Body::SetActive(false)). Limbs have no
        // BloodTankBehaviour.
        let body = self.physicals[p].rigidbody.body;
        self.world.deactivate_body(body);
        self.physicals[p].rigidbody.simulated = false;
        self.deactivate(p);
        self.physicals[p].isDisintegrated = true;
        // InvokeOnItemRemoved: the modding event only.
    }

    /// Collider2D::SetEnabled(false) (180c15670), for each collider in turn: Collider2D::Cleanup (180c0d940) with
    /// ColliderCleanupOperation 2 destroys the shapes last first, resets the body's mass and, with
    /// Physics2D.callbacksOnDisable off, drops the collider's contact records without callbacks.
    pub(super) fn collider_disable(&mut self, p: usize) {
        let body = self.physicals[p].rigidbody.body;
        for collider in self.physicals[p].all_colliders.clone() {
            self.collider_disable_one(collider, body);
        }
    }

    /// Collider2D::SetEnabled(false) of one collider whose fixtures sit on `body`.
    pub(super) fn collider_disable_one(&mut self, collider: i32, body: crate::b2::world::BodyId) {
        let shapes: Vec<_> = self.world.body(body).fixtures.iter().copied().filter(|&f| self.world.fixture(f).collider.id == collider).collect();
        for &f in shapes.iter().rev() {
            self.world.destroy_fixture(body, f);
        }
        if !shapes.is_empty() {
            self.world.reset_mass_data(body);
        }
        self.world.contacts2d.destroy_contacts(collider);
    }

    /// GameObject.SetActive(false) on a physical object, component by component.
    fn deactivate(&mut self, p: usize) {
        // Collider2D::Deactivate (180c0fc00): its Physics2D.IgnoreCollision pairs go; the cleanup finds no shapes.
        let w = &mut self.world;
        for &collider in &self.physicals[p].all_colliders {
            w.ignored.retain(|&(a, b)| a != collider && b != collider);
            w.contacts2d.destroy_contacts(collider);
        }
        self.rigidbody_cleanup(p);
        self.physicals[p].active = false;
        // GForceMeasureBehaviour.OnDisable on a limb.
        if let Some(l) = self.limbs.iter().position(|limb| limb.PhysicalBehaviour == p) {
            let g = &mut self.gforces[l];
            (g.acceleration, g.sustainedAcceleration, g.velocity, g.velocityLastFrame) = ([0.0; 2], [0.0; 2], [0.0; 2], [0.0; 2]);
        }
        self.physical_on_disable(p);
        // The HingeJoint2D/SpringJoint2D Deactivate cleanups find their b2 joints already gone.
    }

    /// PhysicalBehaviour.OnDisable: an empty Collision2D exit to each collider still in the stay buffer
    /// (SendMessage skips inactive objects; collision messages reach disabled behaviours).
    pub fn physical_on_disable(&mut self, p: usize) {
        for pair in self.physicals[p].onCollisionStayBuffer.into_iter().flatten() {
            if pair.Active && self.collider_alive(pair.Coll) {
                self.send_empty_collision_exit(pair.Coll);
            }
        }
    }

    /// Rigidbody2D::Cleanup (180c1dd20) with DeactivateOperation 0: no attached colliders are left, every joint on
    /// the body (body list from the head) loses its b2 joint, then the body goes and the scene's movement list is
    /// rebuilt after the next step.
    fn rigidbody_cleanup(&mut self, p: usize) {
        let body = self.physicals[p].rigidbody.body;
        for (_, j) in self.world.body(body).joints.clone().into_iter().rev() {
            // Joint2D::Cleanup (180c03500). HingeJoint2D::Cleanup (180bfc5e0) keeps the angles for a re-create
            // that a disintegrated limb never gets.
            if let Some(h) = self.hinges.iter().position(|h| h.joint == Some(j)) {
                self.hinges[h].joint = None;
            } else if let Some(l) = self.limbs.iter().position(|l| l.gore.joint == Some(j)) {
                self.limbs[l].gore.joint = None;
            } else if let Some(t) = self.tool.as_mut().filter(|t| t.dragJoint.iter().chain(&t.destroying).any(|d| d.joint == Some(j))) {
                for d in t.dragJoint.iter_mut().chain(&mut t.destroying) {
                    if d.joint == Some(j) {
                        d.joint = None;
                    }
                }
            } else if let Some(f) = self.fixed_joints.iter().position(|f| f.joint == Some(j)) {
                // FixedJoint2D / SliderJoint2D
                self.fixed_joints[f].joint = None;
            } else {
                unimplemented!("Rigidbody2D::Cleanup: a joint other than a hinge, gore string, drag joint or fixed/slider joint");
            }
            self.world.destroy_joint(j);
        }
        let b = self.world.body(body);
        let (mass, props) = (b.rigidbody, [b.linear_damping, b.angular_damping, b.gravity_scale]);
        self.world.destroy_body(body);
        let rb = &mut self.physicals[p].rigidbody;
        rb.detached = Some(mass);
        rb.detached_props = props;
        rb.movement.listed = false;
        self.rigidbody_hierarchy_changed = true;
    }
}

#[cfg(test)]
mod tests {
    use super::super::{dump, Game};
    use crate::assets::db::{AssetDb, Component};
    use crate::assets::mono::Value;
    use crate::b2::world::BodyId;
    use crate::game_assets::HumanRig;
    use std::path::Path;

    fn grid(v: &Value) -> Vec<[f32; 2]> {
        v.array().unwrap().iter().map(|p| [p.get("x").unwrap().f32().unwrap(), p.get("y").unwrap().f32().unwrap()]).collect()
    }

    /// PhysicalBehaviour.Awake and Start on dump30start's first snapshot (the Person's 14 limbs, after the
    /// game ran them): the fields reset to the Person prefab's values, then Awake and Start must rebuild what the
    /// recording holds, bit for bit. SpawnSpawnParticles is switched off: Instantiate of the Resources prefab is
    /// not rewritten, and it takes instance ids and nothing else in this check.
    #[test]
    fn spawn_particles_objects_match_data() {
        let Ok(game) = crate::game_assets::find_game(None) else { return };
        let mut db = AssetDb::open(&game).unwrap();
        let prefab = db.prefab(db.resource("Prefabs/SpawnParticles", 1).unwrap().unwrap()).unwrap();
        assert_eq!(prefab.objects.iter().map(|o| 1 + o.components.len()).sum::<usize>(), super::SPAWN_PARTICLES_OBJECTS);
    }

    #[test]
    fn start_matches_game() {
        let Ok(game) = crate::game_assets::find_game(None) else { return };
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let Ok(rig) = HumanRig::load(&root.join("local-cache"), &game) else { return };
        let Ok(mut snaps) = dump::snapshots(&root.join("reference-probe/game/dump30start.txt")) else { return };
        // The first snapshot (frame 428, right after the Starts) holds the fields; the first FS one with the b2 world builds the game.
        let first = snaps.next().unwrap();
        let fs = snaps.find(|s| s.pt == "FS" && s.of("b2World").next().is_some()).unwrap();
        let mut g = Game::from_snapshot(&fs, &rig).unwrap();

        let mut db = AssetDb::open(&game).unwrap();
        let catalog = db.find_scripts("CatalogData").unwrap()[0];
        let (_, _, cat) = db.mono(catalog).unwrap().unwrap();
        let mut person = None;
        for item in cat.get("Items").and_then(Value::array).unwrap().to_vec() {
            let asset = db.resolve(catalog.file, item.pptr().unwrap()).unwrap();
            let (_, _, sp) = db.mono(asset).unwrap().unwrap();
            if let Some(p) = sp.get("Prefab").and_then(Value::pptr).and_then(|p| db.resolve(asset.file, p)) {
                if db.game_object(p).unwrap().name == "Person" {
                    person = Some(p);
                }
            }
        }
        let prefab = db.prefab(person.unwrap()).unwrap();

        // The Start order is the recording's: the Person's limbs as the snapshot lists their PhysicalBehaviours.
        let records: Vec<_> = first.of("PhysicalBehaviour").collect();
        let order: Vec<usize> = records
            .iter()
            .filter(|r| prefab.objects.iter().any(|o| o.name == r.name && o.components.iter().any(|(_, c)| matches!(c, Component::Rigidbody2D(_)))))
            .map(|r| g.physicals.iter().position(|p| p.id == r.id).unwrap())
            .collect();
        assert_eq!(order.len(), 14);
        let mut out = Vec::new();
        for (n, &p) in order.iter().enumerate() {
            let name = g.physicals[p].name.clone();
            let r = records.iter().find(|r| r.id == g.physicals[p].id).unwrap();
            let obj = prefab.objects.iter().find(|o| o.name == name).unwrap();
            let rb = obj.components.iter().find_map(|(_, c)| match c { Component::Rigidbody2D(r) => Some(r.clone()), _ => None }).unwrap();
            let fields = obj.components.iter().find_map(|(_, c)| match c { Component::Script { class, fields, .. } if class == "PhysicalBehaviour" => Some(fields.clone()), _ => None }).unwrap();
            let body: BodyId = g.physicals[p].rigidbody.body;

            // The recording.
            let id_of = |k: &str| -> i32 { r.get(k).unwrap().rsplit_once('#').unwrap().1.parse().unwrap() };
            let b = g.world.body(body);
            let want_mass = [b.mass, b.inv_mass, b.i, b.inv_i].map(f32::to_bits);
            let want_rb_mass = b.rigidbody.mass;
            let ph = &g.physicals[p];
            let f = |k: &str| f32::from_bits(u32::from_str_radix(r.get(k).unwrap(), 16).unwrap());
            let want = (f("seed"), r.get("intSeed").unwrap().parse::<i32>().unwrap(), f("affectSurroundingTimer"), f("InitialGravityScale"), f("ObjectArea"), f("InitialMass"), f("TrueInitialMass"));
            let want_grid: Vec<[f32; 2]> = r.get("LocalColliderGridPoints").unwrap().trim_matches(|c| c == '[' || c == ']').split(';').map(|p| {
                let (x, y) = p.split_once(',').unwrap();
                [f32::from_bits(u32::from_str_radix(x, 16).unwrap()), f32::from_bits(u32::from_str_radix(y, 16).unwrap())]
            }).collect();
            let want_circ = f("Circumference");
            let want_ids = (id_of("ActivationNode"), id_of("ContextMenuOptions"), id_of("audioSource"), id_of("slidingAudioSource"), id_of("selectionOutlineObject"), id_of("outlineSpriteRenderer"));
            let want_fx: Vec<(u32, u32)> = g.world.body(body).fixtures.iter().map(|&f| (g.world.fixture(f).friction.to_bits(), g.world.fixture(f).restitution.to_bits())).collect();
            let want_mode = (ph.rigidbody.collisionDetectionMode, ph.rigidbody.interpolation);
            let want_bounds = r.get("InitialBounds").unwrap().to_string();

            // Back to the prefab.
            for &f in &g.world.body(body).fixtures.clone() {
                let fx = g.world.fixtures[f].as_mut().unwrap();
                (fx.friction, fx.restitution) = (0.4, 0.0);
            }
            {
                let b = g.world.body_mut(body);
                b.rigidbody.mass = rb.mass;
                b.rigidbody.use_auto_mass = rb.use_auto_mass;
            }
            g.world.reset_mass_data(body);
            {
                let ph = &mut g.physicals[p];
                ph.seed = 0.0;
                ph.intSeed = 0;
                ph.affectSurroundingTimer = 0.0;
                ph.InitialGravityScale = 0.0;
                ph.ObjectArea = 0.0;
                ph.InitialMass = 0.0;
                ph.TrueInitialMass = 0.0;
                ph.LocalColliderGridPoints = Some(grid(fields.get("LocalColliderGridPoints").unwrap()));
                ph.Circumference = fields.get("Circumference").unwrap().f32().unwrap();
                ph.ids = Default::default();
                ph.audioSource = false;
                ph.audio_time_scale = false;
                ph.rigidbody.collisionDetectionMode = rb.collision_detection;
                ph.rigidbody.interpolation = rb.interpolate;
                ph.SpawnSpawnParticles = false;
            }
            let prefab_circ = g.physicals[p].Circumference;
            g.next_instance_id = want_ids.0;
            g.physical_awake(p);
            // The first Start also adds the AudioSourceTimeScaleBehaviour to the root.
            g.next_instance_id = want_ids.2 + if n == 0 { 2 } else { 0 };
            g.physical_start(p);

            let b = g.world.body(body);
            let got_mass = [b.mass, b.inv_mass, b.i, b.inv_i].map(f32::to_bits);
            let ph = &g.physicals[p];
            let got = (ph.seed, ph.intSeed, ph.affectSurroundingTimer, ph.InitialGravityScale, ph.ObjectArea, ph.InitialMass, ph.TrueInitialMass);
            let got_ids = (ph.ids.ActivationNode, ph.ids.ContextMenuOptions, ph.ids.audioSource, ph.ids.slidingAudioSource, ph.ids.selectionOutlineObject, ph.ids.outlineSpriteRenderer);
            let got_fx: Vec<(u32, u32)> = g.world.body(body).fixtures.iter().map(|&f| (g.world.fixture(f).friction.to_bits(), g.world.fixture(f).restitution.to_bits())).collect();
            let eq = |a: f32, b: f32| a.to_bits() == b.to_bits();
            let mut bad = Vec::new();
            if got_mass != want_mass {
                bad.push(format!("mass words game {want_mass:08x?} ours {got_mass:08x?}"));
            }
            if !eq(b.rigidbody.mass, want_rb_mass) {
                bad.push(format!("rigidbody.mass game {want_rb_mass} ours {}", b.rigidbody.mass));
            }
            if !(eq(got.0, want.0) && got.1 == want.1 && eq(got.2, want.2) && eq(got.3, want.3) && eq(got.4, want.4) && eq(got.5, want.5) && eq(got.6, want.6)) {
                bad.push(format!("seed/intSeed/timer/gravity/area/mass/true game {want:?} ours {got:?}"));
            }
            if ph.LocalColliderGridPoints.as_ref().unwrap().iter().flatten().map(|v| v.to_bits()).ne(want_grid.iter().flatten().map(|v| v.to_bits())) {
                bad.push(format!("grid game {want_grid:?} ours {:?}", ph.LocalColliderGridPoints));
            }
            if !eq(prefab_circ, want_circ) {
                bad.push(format!("Circumference prefab {prefab_circ} game {want_circ}"));
            }
            if got_ids != want_ids {
                bad.push(format!("ids game {want_ids:?} ours {got_ids:?}"));
            }
            if g.next_instance_id != want_ids.5 - 2 {
                bad.push(format!("next id {} after the outline renderer {}", g.next_instance_id, want_ids.5));
            }
            if got_fx != want_fx {
                bad.push(format!("fixture friction/restitution game {want_fx:08x?} ours {got_fx:08x?}"));
            }
            if (ph.rigidbody.collisionDetectionMode, ph.rigidbody.interpolation) != want_mode {
                bad.push(format!("cdm/interpolation game {want_mode:?} ours {:?}", (ph.rigidbody.collisionDetectionMode, ph.rigidbody.interpolation)));
            }
            let (c, e) = ph.InitialBounds;
            let ours = format!("Bounds(m_Center={:08x},{:08x},{:08x} m_Extents={:08x},{:08x},{:08x})", c[0].to_bits(), c[1].to_bits(), c[2].to_bits(), e[0].to_bits(), e[1].to_bits(), e[2].to_bits());
            if ours != want_bounds {
                bad.push(format!("InitialBounds game {want_bounds} ours {ours}"));
            }
            out.extend(bad.into_iter().map(|m| format!("{name}: {m}")));
        }
        assert!(out.is_empty(), "{} mismatches:\n{}", out.len(), out.join("\n"));
    }
}
