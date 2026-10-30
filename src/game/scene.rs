//! The Main scene (level1) as SceneManager loads it: the serialized state of the scene objects the simulation
//! reads, with the Awake/Start effects that touch it. A scene object's instance id is the scene's base id plus
//! twice its index in the file (traced under cdb), so the file order is the id order.
use super::{temperature::AmbientTemperatureGridBehaviour, tool::Tool, Game, PlayAudioAtRandom, Prefs};
use crate::b2::world::BodyId;
use crate::assets::classes::{MonoBehaviour, Transform};
use crate::assets::db::{AssetDb, ObjRef};
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

    /// SceneManager.LoadScene("Main"): the scene objects' serialized state and the Awake/Start effects the
    /// simulation reads. `game` is the game folder (UserPreferenceManager's working directory).
    pub fn load_main(db: &mut AssetDb, game: &Path) -> Result<Game, String> {
        let mut g = Game::default();
        // UserPreferenceLoader (level0) ran UserPreferenceManager.Load.
        g.prefs = Prefs::load(game);
        g.load_time_manager(db)?;
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
                }
                // VideoSettingsInitBehaviour.Start: Sync.
                "VideoSettingsInitBehaviour" if s.runs => {
                    found[1] = true;
                    (g.positionIterations, g.velocityIterations) = (g.prefs.PhysicsIterations, g.prefs.PhysicsIterations);
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
