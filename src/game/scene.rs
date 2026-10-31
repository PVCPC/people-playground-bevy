//! The Main scene (level1) as SceneManager loads it: the serialized state of the scene objects the simulation
//! reads, with the Awake/Start effects that touch it. A scene object's instance id is the scene's base id plus
//! twice its index in the file (traced under cdb), so the file order is the id order.
use super::{temperature::AmbientTemperatureGridBehaviour, tool::Tool, Game, PlayAudioAtRandom, Prefs};
use crate::b2::settings::Physics2DSettings;
use crate::b2::world::{BodyId, World};
use crate::assets::classes::{MonoBehaviour, Transform};
use crate::assets::db::{AssetDb, Component, ObjRef};
use crate::assets::mono::Value;
use crate::assets::serialized::Reader;
use std::path::Path;

/// A scene MonoBehaviour: its class, whether Unity runs its Awake/Start (enabled, on an object active in the
/// hierarchy), and its serialized fields.
struct SceneScript {
    class: String,
    runs: bool,
    fields: Value,
    game_object: ObjRef,
}

/// Every MonoBehaviour of a scene file, in file order.
fn scene_scripts(db: &mut AssetDb, scene: &str) -> Result<Vec<SceneScript>, String> {
    let file = db.file_index(scene).ok_or_else(|| format!("no {scene}"))?;
    let ids: Vec<i64> = db.files[file].objects.iter().filter(|o| o.class_id == 114).map(|o| o.path_id).collect();
    let mut out = Vec::new();
    for path_id in ids {
        let r = ObjRef { file, path_id };
        let Some((script, mb, fields)) = db.mono(r)? else { continue };
        let runs = mb.enabled && active_in_hierarchy(db, file, &mb)?;
        let game_object = db.resolve(file, mb.game_object).ok_or("a behaviour without its GameObject")?;
        out.push(SceneScript { class: script.class_name, runs, fields, game_object });
    }
    Ok(out)
}

/// GameObject.activeInHierarchy of a behaviour's object: it and every ancestor active. A RectTransform (224)
/// starts with Transform's fields.
fn active_in_hierarchy(db: &AssetDb, file: usize, mb: &MonoBehaviour) -> Result<bool, String> {
    let mut go = db.resolve(file, mb.game_object).ok_or("a behaviour without its GameObject")?;
    loop {
        let g = db.game_object(go)?;
        if !g.active {
            return Ok(false);
        }
        let t = g.components.iter().filter_map(|&c| db.resolve(go.file, c)).find(|&c| matches!(db.class(c), 4 | 224)).ok_or_else(|| format!("{}: no Transform", g.name))?;
        let t = Transform::read(db.data(t))?;
        if t.father.is_null() {
            return Ok(true);
        }
        let father = db.resolve(go.file, t.father).ok_or("a Transform parent outside the data")?;
        go = db.resolve(go.file, Transform::read(db.data(father))?.game_object).ok_or("a Transform without its GameObject")?;
    }
}

impl Game {
    /// globalgamemanagers' TimeManager (class 5), as its StreamedBinaryRead transfer (1805359f0) reads it:
    /// Fixed Timestep, Maximum Allowed Timestep (SetMaximumDeltaTime keeps it at least the fixed step), m_TimeScale,
    /// and Maximum Particle Timestep (particles only).
    pub fn load_time_manager(&mut self, db: &AssetDb) -> Result<(), String> {
        let file = db.file_index("globalgamemanagers").ok_or("no globalgamemanagers")?;
        let manager = db.files[file].objects.iter().find(|o| o.class_id == 5).ok_or("no TimeManager")?;
        let r = &mut Reader::new(db.data(ObjRef { file, path_id: manager.path_id }));
        self.time.fixedDeltaTime = r.f32()?;
        self.time.maximumDeltaTime = super::mathf::max(r.f32()?, self.time.fixedDeltaTime);
        self.time.timeScale = r.f32()?;
        Ok(())
    }

    /// globalgamemanagers' Physics2DSettings (class 19).
    pub fn load_physics2d_settings(db: &AssetDb) -> Result<Physics2DSettings, String> {
        let file = db.file_index("globalgamemanagers").ok_or("no globalgamemanagers")?;
        let settings = db.files[file].objects.iter().find(|o| o.class_id == 19).ok_or("no Physics2DSettings")?;
        let r = &mut Reader::new(db.data(ObjRef { file, path_id: settings.path_id }));
        Physics2DSettings::read(r)
    }

    /// PhysicsManager2D at startup: the b2 globals (Physics2DSettings::UpdateBox2D), a world at the settings' gravity
    /// with their layer collision matrix and CallbacksOnDisable. What the rewrite does not model is refused.
    pub fn apply_physics2d_settings(&mut self, s: &Physics2DSettings) {
        if s.job_options.use_multithreading {
            unimplemented!("b2World with PhysicsJobOptions2D.useMultithreading");
        }
        if !s.default_material.is_null() {
            unimplemented!("Physics2DSettings.m_DefaultMaterial");
        }
        if s.simulation_mode != 0 {
            unimplemented!("Physics2D.simulationMode other than FixedUpdate");
        }
        if s.queries_hit_triggers || s.queries_start_in_colliders {
            unimplemented!("Physics2D.queriesHitTriggers / queriesStartInColliders");
        }
        if !s.reuse_collision_callbacks || !s.auto_sync_transforms {
            unimplemented!("Physics2D.reuseCollisionCallbacks / autoSyncTransforms off");
        }
        s.update_box2d();
        self.world = World::new(s.gravity);
        self.world.layer_collision_matrix = s.layer_collision_matrix.clone();
        self.world.contacts2d.callbacks_on_disable = s.callbacks_on_disable;
    }

    /// The scene's ToolController after ToolControllerBehaviour.Start (`body` is its Rigidbody2D). SetTool(0)
    /// adds ToolLibrary.Tools[0]'s component and calls OnToolChosen: the DragTool starts at its field
    /// initializers (rotationSpeed 1) with isActive set, and its Awake only subscribes to
    /// ContinuousActivationBehaviour. The SelectionController's selection starts empty.
    pub fn scene_tool(db: &mut AssetDb, body: BodyId) -> Result<Tool, String> {
        let scripts = scene_scripts(db, "level1")?;
        let running = |class: &str| scripts.iter().find(|s| s.runs && s.class == class).ok_or_else(|| format!("level1 has no running {class}"));
        let library = running("ToolLibrary")?;
        let first = library.fields.get("Tools").and_then(Value::array).and_then(|t| t.first()).ok_or("ToolLibrary.Tools is empty")?;
        let Some(Value::Str(ty)) = first.get("Type") else { return Err("ToolLibrary.Tool.Type".into()) };
        if ty != "DragTool" {
            unimplemented!("ToolControllerBehaviour.SetTool(0) of a {ty}");
        }
        let controller = running("ToolControllerBehaviour")?;
        let mask = controller.fields.get("SelectionLayer").and_then(|m| m.get("m_Bits")).and_then(Value::i64).ok_or("SelectionLayer")?;
        let g = db.game_object(controller.game_object)?;
        let t = g.components.iter().filter_map(|&c| db.resolve(controller.game_object.file, c)).find(|&c| db.class(c) == 4).ok_or("ToolController without a Transform")?;
        let t = Transform::read(db.data(t))?;
        if !t.father.is_null() {
            return Err("ToolController is not a root object".into());
        }
        Ok(Tool {
            body,
            position: t.local_position,
            rotation: t.local_rotation,
            SelectionLayer: mask as u32,
            isCurrentlyHolding: false,
            currentlyHovering: None,
            isSelecting: false,
            selectionDragStart: [0.0; 2],
            rawSelection: Vec::new(),
            hover: None,
            offset: [0.0; 2],
            dragJoint: None,
            destroying: Vec::new(),
            controlRotation: false,
            selectedRigidbody: None,
            rotationSpeed: 1.0,
            currentRotation: 0.0,
            snappedMouseDelta: [0.0; 3],
            prevMousePos: [0.0; 3],
            initialSnappingFinished: false,
            isActive: true,
            shouldSendContinuousUseMessage: false,
            initialTranslationSnapOffset: [0.0; 3],
            ActiveSingleSelected: None,
        })
    }

    /// The rest of LoadScene("Main") after `load_main`: the scene's Physics2D natives (`load_scene_natives`),
    /// the ToolController after its Start (`scene_tool`) and the resize handles ResizeHandles.Update hides.
    pub fn load_scene(&mut self, db: &mut AssetDb) -> Result<(), String> {
        let mut scene = self.load_scene_natives(db)?;
        self.tool = Some(Game::scene_tool(db, scene.tool_body)?);
        let scripts = scene_scripts(db, "level1")?;
        let handles = scripts.iter().find(|s| s.runs && s.class == "ResizeHandles").ok_or("level1 has no running ResizeHandles")?;
        if !handles.fields.get("Targets").and_then(Value::array).ok_or("ResizeHandles.Targets")?.is_empty() {
            unimplemented!("ResizeHandles with serialized Targets");
        }
        let root = scene.roots.iter().position(|r| r.prefab.objects.iter().any(|o| o.game_object == handles.game_object)).ok_or("ResizeHandles outside the scene's Physics2D roots")?;
        let inst = scene.roots.swap_remove(root);
        let o = inst.prefab.objects.iter().position(|o| o.game_object == handles.game_object).unwrap();
        self.resize_handles = Some((inst, o));
        Ok(())
    }

    /// MapLoaderBehaviour.Awake -> Load with CurrentMap = the Map asset named `map` (the main menu's choice;
    /// MapLoadOverride is editor-only): WORLD has no children to destroy or deactivate, InstantiateMap
    /// (`load_map`; no InstantiateOverride without mods), and no mod loaders to tell.
    pub fn map_loader_awake(&mut self, db: &mut AssetDb, map: &str) -> Result<(), String> {
        let scripts = scene_scripts(db, "level1")?;
        let loader = scripts.iter().find(|s| s.runs && s.class == "MapLoaderBehaviour").ok_or("level1 has no running MapLoaderBehaviour")?;
        let go = db.game_object(loader.game_object)?;
        let t = go.components.iter().filter_map(|&c| db.resolve(loader.game_object.file, c)).find(|&c| db.class(c) == 4).ok_or("MapLoaderBehaviour without a Transform")?;
        if !Transform::read(db.data(t))?.children.is_empty() {
            unimplemented!("MapLoaderBehaviour.Load: Object.Destroy of WORLD's scene children");
        }
        let mut names = Vec::new();
        let mut prefab = None;
        for m in db.find_scripts("Map")? {
            let Some((_, mb, v)) = db.mono(m)? else { continue };
            if mb.name == map {
                prefab = Some(v.get("Prefab").and_then(Value::pptr).and_then(|p| db.resolve(m.file, p)).ok_or("Map.Prefab")?);
            }
            names.push(mb.name);
        }
        let prefab = prefab.ok_or_else(|| format!("no map {map:?}; the maps are {names:?}"))?;
        let prefab = db.prefab(prefab)?;
        let (inst, _) = self.load_map(db, &prefab)?;
        self.unsynced.push(inst);
        Ok(())
    }

    /// ResizeHandles.Update: with no valid Targets (none are ever set: the resize context menu is not rewritten)
    /// it hides itself with SetActive(false), which removes the handles' fixtures. Its other behaviours have no
    /// OnDisable.
    pub fn resize_handles_update(&mut self) {
        let Some((mut inst, o)) = self.resize_handles.take() else { return };
        for (d, k) in self.set_active(&mut inst, o, false) {
            let Component::Script { class, .. } = &inst.prefab.objects[d].components[k].1 else { unreachable!() };
            if class != "ResizeHandles" {
                unimplemented!("{class}.OnDisable under the resize handles");
            }
        }
    }

    /// SceneManager.LoadScene("Main"): the scene objects' serialized state and the Awake/Start effects the
    /// simulation reads. `game` is the game folder (UserPreferenceManager's working directory).
    pub fn load_main(db: &mut AssetDb, game: &Path) -> Result<Game, String> {
        let mut g = Game::default();
        // UserPreferenceLoader (level0) ran UserPreferenceManager.Load.
        g.prefs = Prefs::load(game);
        g.load_time_manager(db)?;
        g.apply_physics2d_settings(&Self::load_physics2d_settings(db)?);
        let mut found = [false; 4];
        for s in scene_scripts(db, "level1")? {
            let v = &s.fields;
            match s.class.as_str() {
                "Global" => {
                    found[0] = true;
                    let f = v.get("DynamicSqrdVelocityThreshold").and_then(Value::f32).ok_or("Global.DynamicSqrdVelocityThreshold")?;
                    g.DynamicSqrdVelocityThreshold = f;
                    let scrape = v.get("MetalScrapePrefab").and_then(Value::pptr).and_then(|p| db.resolve(s.game_object.file, p)).ok_or("Global.MetalScrapePrefab")?;
                    let prefab = db.prefab(scrape)?;
                    // Only ActivateIfFancyEffects has a message (Optout is a marker): its Awake draws Random.value
                    // on each active object, and a childless object's SetActive skips no later Awake.
                    let mut draws = 0;
                    for (o, obj) in prefab.objects.iter().enumerate() {
                        let mut active = obj.active;
                        let mut up = obj.parent;
                        while let Some(q) = up {
                            active &= prefab.objects[q].active;
                            up = prefab.objects[q].parent;
                        }
                        for (_, c) in &obj.components {
                            let crate::assets::db::Component::Script { class, .. } = c else { continue };
                            match class.as_str() {
                                "Optout" => {}
                                "ActivateIfFancyEffects" if !active => {}
                                "ActivateIfFancyEffects" if obj.children.is_empty() => draws += 1,
                                _ => unimplemented!("a {class} on Global.MetalScrapePrefab ({o}: {})", obj.name),
                            }
                        }
                    }
                    g.metal_scrape = Some((prefab.objects.iter().map(|o| 1 + o.components.len()).sum(), draws));
                    g.holle_man_prefab = v.get("HolleManPrefab").and_then(Value::pptr).and_then(|p| db.resolve(s.game_object.file, p));
                    g.bullet_crack_clips = v.get("BulletCrackClips").and_then(Value::array).ok_or("Global.BulletCrackClips")?.len();
                }
                // VideoSettingsInitBehaviour.Start: Sync.
                "VideoSettingsInitBehaviour" if s.runs => {
                    found[1] = true;
                    (g.positionIterations, g.velocityIterations) = (g.prefs.PhysicsIterations, g.prefs.PhysicsIterations);
                }
                // Global.Awake: camera = Camera.main, the MainCamera-tagged camera this one shakes.
                "CameraShakeBehaviour" => {
                    let go = db.game_object(s.game_object)?;
                    if go.tag != 5 {
                        unimplemented!("CameraShakeBehaviour off the MainCamera");
                    }
                    let camera = go.components.iter().filter_map(|&p| db.resolve(s.game_object.file, p)).find(|&r| db.class(r) == 20).ok_or("the main camera's Camera")?;
                    let camera = crate::assets::components::Camera::read(db.data(camera)).map_err(|e| format!("Camera: {e}"))?;
                    if !camera.orthographic {
                        unimplemented!("a perspective main camera");
                    }
                    g.camera.orthographic_size = camera.orthographic_size;
                }
                // Awake only sets Instance.
                "BulletTracerPool" => {
                    let prefab = v.get("Prefab").and_then(Value::pptr).and_then(|p| db.resolve(s.game_object.file, p)).ok_or("BulletTracerPool.Prefab")?;
                    let prefab = db.prefab(prefab)?;
                    for o in &prefab.objects {
                        for (_, c) in &o.components {
                            if let crate::assets::db::Component::Script { class, .. } = c {
                                if class != "PoolableSizeHelper" {
                                    unimplemented!("a {class} on the BulletTracerPool prefab");
                                }
                            }
                        }
                    }
                    g.tracer_pool.clone_ids = prefab.objects.iter().map(|o| 1 + o.components.len()).sum();
                    g.tracer_pool.MaxPoolSize = v.get("MaxPoolSize").and_then(Value::i64).ok_or("BulletTracerPool.MaxPoolSize")? as usize;
                    g.tracer_pool.InactivityThresholdInSeconds = v.get("InactivityThresholdInSeconds").and_then(Value::f32).ok_or("BulletTracerPool.InactivityThresholdInSeconds")?;
                }
                // Awake makes its BallisticsEmitter and sets Instance.
                "GlobalShrapnelEmitter" if s.runs => {
                    let cartridge = v.get("Cartridge").and_then(Value::pptr).and_then(|p| db.resolve(s.game_object.file, p)).ok_or("GlobalShrapnelEmitter.Cartridge")?;
                    let (_, _, cv) = db.mono(cartridge)?.ok_or("GlobalShrapnelEmitter.Cartridge is not a MonoBehaviour")?;
                    let mask = |name: &str| v.get(name).and_then(|m| m.get("m_Bits")).and_then(Value::i64).map(|b| b as u32).ok_or_else(|| format!("GlobalShrapnelEmitter.{name}"));
                    let mut e = super::ballistics::Emitter::new(super::ballistics::Cartridge::read(&cv)?, mask("LayersToHit")?, mask("WaterLayer")?);
                    e.RenderTracersIfTraversal = false;
                    e.DoTraversal = true;
                    e.MaxBallisticsIterations = 16;
                    e.RicochetChanceMultiplier = 4.0;
                    e.MaxTotalDistance = 30.0;
                    g.shrapnel = Some(super::ballistics::ShrapnelEmitter { emitter: e });
                }
                // Awake only sets Instance; Play draws from these.
                "DistantSoundBehaviour" => {
                    for (i, name) in ["SmallDistantExplosions", "LargeDistantExplosions", "LargerDistantExplosions", "SmallDistantFirearms", "MediumDistantFirearms", "LargeDistantFirearms"].iter().enumerate() {
                        g.distant_sounds[i] = v.get(name).and_then(Value::array).ok_or_else(|| format!("DistantSoundBehaviour.{name}"))?.len();
                    }
                }
                // Awake sets Instance; Get draws from ShatterMasks (the Textures' red channel is all GetPolygon reads).
                "ProceduralShatterMaterialPool" => {
                    for t in v.get("ShatterMasks").and_then(Value::array).ok_or("ProceduralShatterMaterialPool.ShatterMasks")? {
                        let r = t.pptr().and_then(|p| db.resolve(s.game_object.file, p)).ok_or("ProceduralShatterMaterialPool.ShatterMasks: a null texture")?;
                        let tex = crate::assets::components::Texture2D::read(db.data(r))?;
                        let rgba = tex.rgba(&db.data_dir)?;
                        // GetPixels(): byte / 255f, rows from the bottom.
                        let red = rgba.chunks(4).map(|c| c[0] as f32 / 255.0).collect();
                        g.shatter.shatter_masks.push(super::shatter::Texture { instance_id: r.path_id as i32, width: tex.width as usize, height: tex.height as usize, red });
                    }
                }
                // Awake only sets Instance.
                "AmbientTemperatureGridBehaviour" => {
                    found[2] = true;
                    let mut grid = AmbientTemperatureGridBehaviour::default();
                    for (name, slot) in [("ObjectToAirTransferRate", &mut grid.ObjectToAirTransferRate), ("AirToObjectTransferRate", &mut grid.AirToObjectTransferRate), ("AmbientTemperatureTransferRate", &mut grid.AmbientTemperatureTransferRate)] {
                        *slot = v.get(name).and_then(Value::f32).ok_or_else(|| format!("AmbientTemperatureGridBehaviour.{name}"))?;
                    }
                    grid.GridSize = v.get("GridSize").and_then(Value::i64).and_then(|x| i32::try_from(x).ok()).ok_or("AmbientTemperatureGridBehaviour.GridSize")?;
                    g.ambient_grid = grid;
                }
                // WORLD's ambience emitter; MapConfig.Reinitialise sets PositionCenter and Radius when a map loads.
                "GearManagerBehaviour" => g.gear_manager_awake(),
                "PlayAudioAtRandom" => {
                    found[3] = true;
                    let mut a = PlayAudioAtRandom::default();
                    super::spawn::load_values(a.fields(), v)?;
                    a.clips = v.get("Clips").and_then(Value::array).map(<[Value]>::len);
                    g.ambient_audio.push(a);
                }
                _ => {}
            }
        }
        if found != [true; 4] {
            return Err(format!("level1 lacks a scene script the simulation reads ({found:?})"));
        }
        Ok(g)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The b2 globals the code starts with (before load_main) are what the data and UpdateBox2D give, and the
    /// recorder's `physics` header (the game's own Physics2D readings) says the same.
    #[test]
    fn physics2d_settings_match_data_and_recording() {
        let Ok(game) = crate::game_assets::find_game(None) else { return };
        let db = AssetDb::open(&game).unwrap();
        let s = Game::load_physics2d_settings(&db).unwrap();
        assert_eq!(crate::b2::settings::globals(), s.globals());
        let dump = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("reference-probe/game/dump22drag.txt"));
        let Ok(dump) = dump else { return };
        let header = dump.lines().find(|l| l.contains("velThreshold=")).unwrap();
        // Each reading is `value|bits`; gravity's two components are separated by a space.
        let get = |k: &str| -> f32 { f32::from_bits(u32::from_str_radix(header.split(' ').find_map(|kv| kv.strip_prefix(&format!("{k}="))).unwrap().split('|').nth(1).unwrap(), 16).unwrap()) };
        // The recorded gravity is MapConfig's (Gravity * Vector2.up: x is -0), the data's default is +0.
        assert_eq!((s.gravity.x, s.gravity.y.to_bits()), (get("gravity"), 0xc11cf5c3));
        let got = [s.velocity_threshold, s.max_linear_correction, s.max_angular_correction, s.max_translation_speed, s.max_rotation_speed, s.baumgarte_scale, s.baumgarte_toi_scale, s.time_to_sleep, s.linear_sleep_tolerance, s.angular_sleep_tolerance, s.default_contact_offset];
        let want = ["velThreshold", "maxLin", "maxAng", "maxTrans", "maxRot", "baum", "baumToi", "sleepTime", "linSleep", "angSleep", "contactOffset"].map(get);
        assert_eq!(got, want);
    }

    /// The scene state against a recording made with the same config.json: the fields the map leaves alone.
    #[test]
    fn main_scene_matches_recording() {
        let Ok(game) = crate::game_assets::find_game(None) else { return };
        let mut db = AssetDb::open(&game).unwrap();
        let g = Game::load_main(&mut db, &game).unwrap();
        let dump = Path::new(env!("CARGO_MANIFEST_DIR")).join("reference-probe/game/dump31spawnrand.txt");
        let Ok(mut snaps) = super::super::dump::snapshots(&dump) else { return };
        let mut first = snaps.next().unwrap();
        // The map's and pool's splats are not loaded from snapshots.
        first.records.retain(|r| r.ty != "FancyBloodSplatController" && r.ty != "BleedingParticleBehaviour");
        let Ok(rig) = crate::game_assets::HumanRig::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("local-cache"), &game) else { return };
        let want = Game::from_snapshot(&first, &rig).unwrap();
        assert_eq!(g.DynamicSqrdVelocityThreshold, want.DynamicSqrdVelocityThreshold);
        assert_eq!((g.time.fixedDeltaTime, g.time.maximumDeltaTime, g.time.timeScale), (want.time.fixedDeltaTime, want.time.maximumDeltaTime, want.time.timeScale));
        assert_eq!((g.velocityIterations, g.positionIterations), (want.velocityIterations, want.positionIterations));
        let (a, b) = (&g.ambient_audio[0], &want.ambient_audio[0]);
        assert_eq!((a.ChooseRandomClip, a.PlaceInRandomPositionInCircle, a.OnlyOnEdge, a.clips), (b.ChooseRandomClip, b.PlaceInRandomPositionInCircle, b.OnlyOnEdge, b.clips));
        let d = AmbientTemperatureGridBehaviour::default();
        let x = &g.ambient_grid;
        assert_eq!((x.ObjectToAirTransferRate, x.AirToObjectTransferRate, x.AmbientTemperatureTransferRate, x.GridSize), (d.ObjectToAirTransferRate, d.AirToObjectTransferRate, d.AmbientTemperatureTransferRate, d.GridSize));
    }

    /// ToolControllerBehaviour.Start's state against the first snapshot of a recording that dumps the tool.
    #[test]
    fn scene_tool_matches_recording() {
        let Ok(game) = crate::game_assets::find_game(None) else { return };
        let mut db = AssetDb::open(&game).unwrap();
        let dump = Path::new(env!("CARGO_MANIFEST_DIR")).join("reference-probe/game/dump22drag.txt");
        let Ok(mut snaps) = super::super::dump::snapshots(&dump) else { return };
        let first = snaps.next().unwrap();
        let t = Game::scene_tool(&mut db, 0).unwrap();
        let (r, d) = (first.of("ToolControllerBehaviour").next().unwrap(), first.of("DragTool").next().unwrap());
        assert_eq!(format!("LayerMask(m_Mask={})", t.SelectionLayer), r.req("SelectionLayer").unwrap());
        assert_eq!((t.isCurrentlyHolding, t.isSelecting), (r.req("isCurrentlyHolding").unwrap() == "1", r.req("isSelecting").unwrap() == "1"));
        assert_eq!((t.isActive, format!("{:08x}", t.rotationSpeed.to_bits())), (d.req("isActive").unwrap() == "1", d.req("rotationSpeed").unwrap().to_string()));
        assert_eq!(d.req("dragJoint").unwrap(), "null");
    }
}

