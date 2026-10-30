//! An instantiated prefab's MonoBehaviours as the Game's component arenas. Each script gets its arena entry with
//! its serialized fields, and the rewrite's links (arena indices) stand for its component references: the
//! serialized ones resolve through the prefab, the GetComponent ones through the object's components. Runtime
//! state stays at its field initializer for Awake/OnEnable/Start to set.

use super::circulation::Circulation;
use super::limb::{GoreString, Limb};
use super::misc::{AutofixJoint, AutofixJoints, GForce, Grip, Node, Skin};
use super::person::{LimbPose, Person, RagdollPose};
use super::physical::{Physical, PhysicalProperties, SharpAxis};
use super::water::Water;
use super::spawn::{anchored_initial_state, joint_connected_object, load_values, Instance};
use super::*;
use crate::assets::classes::PPtr;
use crate::assets::components::{JointKind, PhysicsMaterial2D, Sprite};
use crate::assets::db::{AssetDb, Component, ObjRef, Prefab, PrefabObject};
use crate::assets::mono::Value;
use crate::game_assets::{Curve, CurveCache};
use crate::unity::transform::Node as TransformNode;
use std::collections::HashMap;

/// AudioSource's class id.
const AUDIO_SOURCE: i32 = 82;

/// The arena index of each prefab object's scripts, in prefab (GetComponentsInChildren) order.
#[derive(Default)]
struct Arenas {
    physical: HashMap<usize, usize>,
    limb: HashMap<usize, usize>,
    grip: HashMap<usize, usize>,
    hinge: HashMap<(usize, usize), usize>,
    /// Transform nodes by object (the Rigidbody-less parents of bodies).
    node: HashMap<usize, usize>,
}

/// What `link_instance` built: the Person's index (a root PersonBehaviour), and the arena index of each limb,
/// PhysicalBehaviour and MapLightBehaviour object and of each Rigidbody-less Transform above a body.
pub struct Linked {
    pub person: Option<usize>,
    pub limbs: HashMap<usize, usize>,
    pub nodes: HashMap<usize, usize>,
    pub physicals: HashMap<usize, usize>,
    /// Game::hinges index by (object, component).
    pub hinges: HashMap<(usize, usize), usize>,
    pub map_lights: HashMap<usize, usize>,
    /// Game::waters index by object.
    pub waters: HashMap<usize, usize>,
    pub machinery: HashMap<usize, usize>,
    /// Game::lavas, spawners and heat_rects index by object.
    pub lavas: HashMap<usize, usize>,
    pub destroyables: HashMap<usize, usize>,
    pub fire_maps: HashMap<usize, usize>,
    pub firearms: HashMap<usize, usize>,
    pub launchers: HashMap<usize, usize>,
    pub bells: HashMap<usize, usize>,
    pub machine_guns: HashMap<usize, usize>,
    pub explosives: HashMap<usize, usize>,
    pub delete_afters: HashMap<usize, usize>,
    pub point_to_velocities: HashMap<usize, usize>,
    pub cars: HashMap<usize, usize>,
    pub tyres: HashMap<usize, usize>,
    pub flasks: HashMap<usize, usize>,
    pub liquid_containers: HashMap<usize, usize>,
    pub truck_doors: HashMap<usize, usize>,
    pub glowtubes: HashMap<usize, usize>,
    pub leds: HashMap<usize, usize>,
    pub detectors: HashMap<usize, usize>,
    pub sticky_grenades: HashMap<usize, usize>,
    pub generators: HashMap<usize, usize>,
    pub floodlights: HashMap<usize, usize>,
    pub spike_grenades: HashMap<usize, usize>,
    pub heating_elements: HashMap<usize, usize>,
    pub ray_guns: HashMap<usize, usize>,
    pub act_on_collides: HashMap<usize, usize>,
    pub spawners: HashMap<usize, usize>,
    pub heat_rects: HashMap<usize, usize>,
    pub random_sprites: HashMap<usize, usize>,
    /// Game::node_rigidbodies index by object.
    pub node_rigidbodies: HashMap<usize, usize>,
    /// The PlayAudioAtRandom on the instance's parent (`ambient_audio`), which MapConfig.Reinitialise configures.
    pub parent_audio: Option<usize>,
}

/// The clone's Transform as a physical object's.
fn pose(inst: &Instance, o: usize) -> Pose {
    let trs = inst.objects[o].trs;
    Pose { position: inst.position(o), rotation: inst.rotation(o), localScale: [trs.s[0], trs.s[1], trs.s[2]], local: Some(([trs.t[0], trs.t[1], trs.t[2]], trs.q)) }
}

fn script<'a>(prefab: &'a Prefab, o: usize, class: &str) -> Option<(usize, bool, &'a Value)> {
    prefab.objects[o].components.iter().enumerate().find_map(|(c, (_, comp))| match comp {
        Component::Script { class: k, enabled, fields, .. } if k == class => Some((c, *enabled, fields)),
        _ => None,
    })
}

fn field<'a>(v: &'a Value, name: &str) -> Result<&'a Value, String> {
    v.get(name).ok_or_else(|| format!("no serialized {name}"))
}

fn float(v: &Value, name: &str) -> Result<f32, String> {
    field(v, name)?.f32().ok_or_else(|| format!("{name}: not a float"))
}

fn array<'a>(v: &'a Value, name: &str) -> Result<&'a [Value], String> {
    field(v, name)?.array().ok_or_else(|| format!("{name}: not an array"))
}

fn string(v: &Value, name: &str) -> Result<String, String> {
    match field(v, name)? {
        Value::Str(s) => Ok(s.clone()),
        x => Err(format!("{name}: not a string: {x:?}")),
    }
}

fn floats<const N: usize>(v: &Value) -> Result<[f32; N], String> {
    let mut out = [0.0; N];
    out.set(v)?;
    Ok(out)
}

/// A serialized ExplosionCreator.ExplosionParameters.
fn explosion_parameters(v: &Value) -> Result<super::explosion::ExplosionParameters, String> {
    use super::explosion::EffectSize;
    let int = |name: &str| field(v, name)?.i64().ok_or_else(|| format!("{name}: not an integer"));
    Ok(super::explosion::ExplosionParameters {
        FragmentationRayCount: int("FragmentationRayCount")? as u32,
        Position: floats::<3>(field(v, "Position")?)?,
        Range: float(v, "Range")?,
        FragmentForce: float(v, "FragmentForce")?,
        CreateParticlesAndSound: field(v, "CreateParticlesAndSound")?.bool().ok_or("CreateParticlesAndSound")?,
        EffectSize: match int("EffectSize")? {
            0 => EffectSize::Medium,
            1 => EffectSize::Small,
            2 => EffectSize::Large,
            x => return Err(format!("EffectSize {x}")),
        },
        DismemberChance: float(v, "DismemberChance")?,
        BallisticShrapnelCount: int("BallisticShrapnelCount")? as i32,
    })
}

/// A serialized Bounds as center then extents.
fn bounds(v: &Value) -> Result<[f32; 6], String> {
    let [cx, cy, cz] = floats::<3>(field(v, "m_Center")?)?;
    let [ex, ey, ez] = floats::<3>(field(v, "m_Extent")?)?;
    Ok([cx, cy, cz, ex, ey, ez])
}

/// A LineSegment (A, B).
fn line(v: &Value) -> Result<[f32; 4], String> {
    let [ax, ay] = floats::<2>(field(v, "A")?)?;
    let [bx, by] = floats::<2>(field(v, "B")?)?;
    Ok([ax, ay, bx, by])
}

fn curve(v: &Value) -> Result<Curve, String> {
    let mut keys = Vec::new();
    for k in array(v, "m_Curve")? {
        if field(k, "weightedMode")?.i64() != Some(0) {
            unimplemented!("a weighted AnimationCurve key");
        }
        keys.push([float(k, "time")?, float(k, "value")?, float(k, "inSlope")?, float(k, "outSlope")?]);
    }
    let int = |n: &str| -> Result<i32, String> { Ok(field(v, n)?.i64().ok_or(n.to_string())? as i32) };
    Ok(Curve { keys, pre: int("m_PreInfinity")?, post: int("m_PostInfinity")? })
}

/// Sprite.bounds (Sprite::GetBounds): the sprite rect around its pivot in units, 0.1 deep either side.
/// A UnityEngine.Bounds field as the 2D box the map scripts use.
fn map_bounds(v: &Value) -> Result<Bounds, String> {
    let b = bounds(field(v, "BoundingBox")?)?;
    Ok(Bounds { center: [b[0], b[1]], extents: [b[3], b[4]] })
}

fn sprite_bounds(s: &Sprite) -> ([f32; 3], [f32; 3]) {
    let (w, h) = (s.rect[2] / s.pixels_to_units, s.rect[3] / s.pixels_to_units);
    let (ex, ey) = (w * 0.5, h * 0.5);
    ([ex - s.pivot[0] * w, ey - s.pivot[1] * h, 0.0], [ex, ey, 0.1])
}

/// SpriteRenderer::GetSpriteBounds (1806e6840): Simple takes the sprite's bounds, mirrored by the flips
/// (Sprite::GetBounds negates the corners); Sliced and Tiled the renderer's size around the pivot, the flips as
/// signs, encapsulated from an empty AABB with z -0.1 and 0.1.
fn renderer_bounds(s: &Sprite, r: &crate::assets::components::SpriteRenderer) -> ([f32; 3], [f32; 3]) {
    if r.draw_mode == 0 {
        let (mut c, e) = sprite_bounds(s);
        if r.flip_x {
            c[0] = -c[0];
        }
        if r.flip_y {
            c[1] = -c[1];
        }
        return (c, e);
    }
    let sign = |flip: bool| if flip { -1.0f32 } else { 1.0 };
    let corner = |size: f32, pivot: f32, f: f32| {
        let below = size * pivot * f;
        (size * f - below, -below)
    };
    let (hx, lx) = corner(r.size[0], s.pivot[0], sign(r.flip_x));
    let (hy, ly) = corner(r.size[1], s.pivot[1], sign(r.flip_y));
    let (min, max) = ([hx.min(lx), hy.min(ly), -0.1f32], [hx.max(lx), hy.max(ly), 0.1f32]);
    (std::array::from_fn(|i| (min[i] + max[i]) * 0.5), std::array::from_fn(|i| (max[i] - min[i]) * 0.5))
}

struct Linker<'a> {
    prefab: &'a Prefab,
    inst: &'a Instance,
    file: usize,
    arenas: Arenas,
}

impl Linker<'_> {
    /// The prefab object a local reference points at (its GameObject or one of its components).
    fn object(&self, v: &Value) -> Result<Option<usize>, String> {
        self.object_of(v.pptr().ok_or_else(|| format!("not a reference: {v:?}"))?)
    }

    fn object_of(&self, p: PPtr) -> Result<Option<usize>, String> {
        if p.is_null() {
            return Ok(None);
        }
        if p.file != 0 {
            return Err(format!("a reference outside the prefab: {p:?}"));
        }
        let (o, _) = self.prefab.index.get(&ObjRef { file: self.file, path_id: p.path_id }).ok_or_else(|| format!("dangling reference {p:?}"))?;
        Ok(Some(*o))
    }

    fn in_arena(&self, v: &Value, arena: &HashMap<usize, usize>, what: &str) -> Result<Option<usize>, String> {
        match self.object(v)? {
            None => Ok(None),
            Some(o) => arena.get(&o).copied().map(Some).ok_or_else(|| format!("{} has no {what}", self.prefab.objects[o].name)),
        }
    }

    fn limb(&self, v: &Value) -> Result<Option<usize>, String> {
        self.in_arena(v, &self.arenas.limb, "LimbBehaviour")
    }

    fn limbs(&self, v: &Value, name: &str) -> Result<Vec<usize>, String> {
        array(v, name)?.iter().map(|x| self.limb(x)?.ok_or_else(|| format!("{name}: a null entry"))).collect()
    }

    fn physical(&self, v: &Value) -> Result<Option<usize>, String> {
        self.in_arena(v, &self.arenas.physical, "PhysicalBehaviour")
    }
}

/// Whether every persistent call of a UnityEvent that runs on Invoke targets an AudioSource, an audio filter or
/// reverb zone, a ParticleSystem or a RectangleAudioSourceBehaviour.
pub(super) fn presentation_event(prefab: &Prefab, file: usize, event: &Value) -> bool {
    invoked_calls(event).iter().all(|call| presentation_call(prefab, file, call))
}

/// The persistent calls of an event that target a CarBehaviour (SetHealth(h), set_enabled(false)), as the car's component
/// id and the call, in order.
fn car_calls(prefab: &Prefab, file: usize, event: &Value, inst: &Instance) -> Vec<(i32, super::car::CarCall)> {
    use super::car::CarCall;
    let mut out = Vec::new();
    for call in invoked_calls(event) {
        let Some((to, tc)) = call_target(prefab, file, call) else { continue };
        if !matches!(&prefab.objects[to].components[tc].1, Component::Script { class, .. } if class == "CarBehaviour") {
            continue;
        }
        let args = call.get("m_Arguments").unwrap();
        let method = string(call, "m_MethodName").unwrap();
        let call = match method.as_str() {
            "SetHealth" => CarCall::SetHealth(float(args, "m_FloatArgument").unwrap()),
            "set_enabled" if args.get("m_BoolArgument").is_some_and(|b| b.bool() == Some(false) || b.i64() == Some(0)) => CarCall::Disable,
            m => unimplemented!("a UnityEvent calling CarBehaviour.{m}"),
        };
        out.push((inst.objects[to].components[tc], call));
    }
    out
}

/// A persistent call `event_calls` keeps: CarBehaviour's, or DetachFromParent.Detach by its component id.
#[derive(Clone, Copy, Debug)]
pub enum EventCall {
    Car(i32, super::car::CarCall),
    Detach(i32),
}

/// The calls of an event that are not presentation (`presentation_call`), in order: CarBehaviour (`car_calls`) and
/// DetachFromParent.Detach.
fn event_calls(prefab: &Prefab, file: usize, event: &Value, inst: &Instance, what: &str) -> Result<Vec<EventCall>, String> {
    let mut cars = car_calls(prefab, file, event, inst).into_iter();
    let mut out = Vec::new();
    for call in invoked_calls(event) {
        if presentation_call(prefab, file, call) {
            continue;
        }
        let class = call_target(prefab, file, call).and_then(|(o, c)| match &prefab.objects[o].components[c].1 {
            Component::Script { class, .. } => Some((class.as_str(), inst.objects[o].components[c])),
            _ => None,
        });
        match class {
            Some(("CarBehaviour", _)) => out.push(cars.next().map(|(id, c)| EventCall::Car(id, c)).unwrap()),
            Some(("DetachFromParent", id)) if string(call, "m_MethodName")? == "Detach" => out.push(EventCall::Detach(id)),
            _ => unimplemented!("{what}: a persistent call beyond audio, particles, CarBehaviour and DetachFromParent.Detach ({})", string(call, "m_MethodName")?),
        }
    }
    Ok(out)
}

/// A UnityEvent's persistent calls that run on Invoke (all but the Off ones).
fn invoked_calls(event: &Value) -> Vec<&Value> {
    let calls = event.get("m_PersistentCalls").and_then(|c| c.get("m_Calls")).and_then(Value::array).unwrap();
    calls.iter().filter(|c| c.get("m_CallState").and_then(Value::i64) != Some(0)).collect()
}

/// The prefab object and component a persistent call targets.
fn call_target(prefab: &Prefab, file: usize, call: &Value) -> Option<(usize, usize)> {
    let target = call.get("m_Target").and_then(Value::pptr).unwrap();
    (target.file == 0).then(|| prefab.index.get(&ObjRef { file, path_id: target.path_id }).copied()).flatten().filter(|&(_, c)| c != usize::MAX)
}

/// Checks a UnityEvent only plays audio, particles, or a Translation/Rotation/ColorAnimationBehaviour animating a
/// subtree without physics. `what` names the event.
fn check_presentation_event(prefab: &Prefab, file: usize, inst: &Instance, event: &Value, what: &str) -> Result<(), String> {
    for call in invoked_calls(event) {
        if presentation_call(prefab, file, call) {
            continue;
        }
        let animation = call_target(prefab, file, call).filter(|&(to, tc)| {
            matches!(&prefab.objects[to].components[tc].1, Component::Script { class, .. } if matches!(class.as_str(), "TranslationAnimationBehaviour" | "RotationAnimationBehaviour" | "ColorAnimationBehaviour"))
                && string(call, "m_MethodName").is_ok_and(|m| m == "Play")
                && std::iter::once(to).chain(inst.descendants(to)).all(|x| prefab.objects[x].components.iter().all(|(_, c)| !matches!(c, Component::Collider2D(_) | Component::Rigidbody2D(_))))
        });
        if animation.is_none() {
            unimplemented!("{what}: a persistent call beyond audio, particles and animations ({})", string(call, "m_MethodName")?);
        }
    }
    Ok(())
}

fn presentation_call(prefab: &Prefab, file: usize, call: &Value) -> bool {
    call_target(prefab, file, call).is_some_and(|(o, c)| match &prefab.objects[o].components[c].1 {
        Component::Other(82 | 164..=170 | 198) => true,
        // Its LateUpdate only drives an AudioSource (see check_rewritten).
        Component::Script { class, .. } => class == "RectangleAudioSourceBehaviour" || (class == "StickyGrenadeBehaviour" && string(call, "m_MethodName").is_ok_and(|m| m == "StartBlinking")),
        _ => false,
    })
}

/// `check_rewritten` for the script on prefab object `o`: a TrivialAnimationBehaviour only animates its own
/// Transform or SpriteRenderer (AnimationCurve/Gradient, no Random), which is presentation over a subtree without
/// colliders or bodies.
pub(super) fn check_rewritten_at(prefab: &Prefab, o: usize, class: &str) {
    if matches!(class, "TranslationAnimationBehaviour" | "RotationAnimationBehaviour" | "ColorAnimationBehaviour") {
        fn physics(prefab: &Prefab, o: usize) -> bool {
            prefab.objects[o].components.iter().any(|(_, c)| matches!(c, Component::Collider2D(_) | Component::Rigidbody2D(_))) || prefab.objects[o].children.iter().any(|&x| physics(prefab, x))
        }
        if physics(prefab, o) {
            unimplemented!("{class} moving colliders or bodies");
        }
        return;
    }
    check_rewritten(class);
}

/// GetComponentsInChildren<Collider2D>() from object `o` of a clone: the active objects depth first, each one's
/// colliders in component order, as (collider id, object).
pub(super) fn colliders_in_children(inst: &Instance, o: usize) -> Vec<(i32, usize)> {
    let prefab = inst.prefab.clone();
    std::iter::once(o)
        .chain(inst.descendants(o))
        .filter(|&x| inst.objects[x].active)
        .flat_map(|x| prefab.objects[x].components.iter().enumerate().filter(|(_, (_, c))| matches!(c, Component::Collider2D(_))).map(move |(k, _)| (x, k)).collect::<Vec<_>>())
        .map(|(x, k)| (inst.objects[x].components[k], x))
        .collect()
}

/// Stops at a MonoBehaviour whose Unity messages (lifecycle.rs) the rewrite does not run: every class with any is
/// either rewritten or only touches audio, rendering or statistics the rewrite does not simulate.
pub(super) fn check_rewritten(class: &str) {
    match class {
        "PhysicalBehaviour" | "LimbBehaviour" | "CirculationBehaviour" | "PersonBehaviour" | "SkinMaterialHandler" | "ConnectedNodeBehaviour"
        | "GoreStringBehaviour" | "GripBehaviour" | "HingeJointLimitAutofixBehaviour" | "MapLightBehaviour" | "MapConfig" | "BoundingBoxBehaviour"
        | "ActivateIfTutorial" | "WaterBehaviour" | "PlayAudioAtRandom" | "ShatteredObjectGenerator"
        | "JointBreakSoundBehaviour" | "DamagableMachineryBehaviour" | "DestroyableBehaviour" | "ExplosionSoundBehviour" | "ActOnDestroy"
        | "DestroyWhenAllChildrenDestroyed" | "InstantiateObjectBehaviour" | "DetachFromParentOnStart" | "RandomForceAtStart" | "DoAtTimeOnce" | "RandomSpriteBehaviour" | "LavaBehaviour" | "RandomPrefabSpawnerBehaviour"
        | "TemperatureTargetRectangleBehaviour" | "ExistByChance" | "ExplosiveBehaviour" | "DeleteAfterTime" | "ActOnCollide" | "NoCollide" | "StickyGrenadeBehaviour" | "ActOnHingeJointLimit" | "PointToVelocityBehaviour" | "ActivateIfFancyEffects" | "ExistInDetailView" | "IgnoreParentSize" | "LocalFirePropagationMap" | "RandomSpawnedSize" | "NoChildrenCollide" | "FirearmBehaviour" | "CarBehaviour" | "TyreDeflationBehaviour" | "GasTankBehaviour" | "FlaskBehaviour" | "LiquidContainerController" | "PointLiquidTransferBehaviour" | "TruckDoorBehaviour" | "GlowtubeBehaviour" | "LEDBulbBehaviour" | "DetectorBehaviour" | "RandomCarTextureBehaviour" | "NoCollideGm" | "JointSerialisationHelper" | "BatchJointSerialisationHelper" | "ProjectileLauncherBehaviour" | "OnlyExplosionBehaviour" | "BellBehaviour" | "MachineGunBehaviour" | "GeneratorBehaviour" | "SingleFloodlightBehaviour" | "SpikeGrenadeBehaviour" | "HeatingElementBehaviour" | "TemperatureRayGunBehaviour" => {}
        // Renderer sorting from the parent's SpriteRenderer; SpriteRenderer.color from Temperature when visible.
        // SmoothLineControllerBehaviour only sets LineRenderer positions and width.
        "SortingLayerChild" | "VertexColourBasedOnTemperatureBehaviour" | "SmoothLineControllerBehaviour" => {}
        // OnEnable's two Random.value (`color_wave_on_enable`) feed only OnWillRenderObject's SpriteRenderer.color.
        "ColorWave" => {}
        // Start: see PendingStart::FirearmAttachmentPoint. OnTriggerEnter2D only takes a FirearmAttachmentBehaviour (whose
        // Awake stops the rewrite), and OnDestroy only destroys a connected one.
        "FirearmAttachmentPointBehaviour" => {}
        // Awake keeps the bodies' masses for OnMakeWeightful; nothing rewritten sends OnMakeWeightless/OnMakeWeightful.
        "WeightlessHelper" => {}
        // OnChildDisintegrated over a person's limbs (PersonBehaviour.DisintegrationCount) or `Game::disintegration_counters`.
        "DisintegrationCounterBehaviour" => {}
        // OnDestroy drops the object's undo entry.
        "DeregisterBehaviour" => {}
        // Global.AddAudioSource/RemoveAudioSource, AudioSource.volume, StatManager, SpriteRenderer.color; material
        // property blocks in OnWillRenderObject.
        // AudioSource.Play/Stop/volume from the camera position (RectangleAudioSourceBehaviour), the radio clip (its instance
        // id is taken in Awake), and the OnBecameVisible fade of a SpriteRenderer before SetActive(false) of a render-only
        // subtree (PoliceBoxBehaviour).
        "AudioSourceTimeScaleBehaviour" | "WaterSoundBehaviour" | "IncreaseStatOnStart" | "PanelsBehaviour" | "BurnableSpriteBehaviour"
        | "GlowingHotMetalBehaviour" | "FuseBoxBehaviour" | "RectangleAudioSourceBehaviour" | "PoliceBoxBehaviour"
        // Update stores ParticleSystem.isEmitting in IsPlaying (read when serialising); Start plays or stops that particle system.
        | "ParticleSystemSerialisationHelper" => {}
        _ => {
            let messages = super::lifecycle::LIFECYCLE.iter().find(|(k, _)| *k == class).map_or(&[][..], |(_, m)| *m);
            if !messages.is_empty() {
                unimplemented!("{class} ({})", messages.join(", "));
            }
        }
    }
}

impl Game {
    /// The instance's scripts as arena entries with their serialized fields and links, in prefab order (the
    /// order GetComponentsInChildren lists them, and the queue 0x14 order within one script class). Limb-owned
    /// components (CirculationBehaviour, ConnectedNodeBehaviour, SkinMaterialHandler, GForceMeasureBehaviour)
    /// share their limb's index. Rigidbody-less Transforms above bodies become `transform_nodes` in the order
    /// the bodies reach them.
    pub fn link_instance(&mut self, db: &mut AssetDb, prefab: &Prefab, inst: &Instance) -> Result<Linked, String> {
        let file = prefab.objects[0].game_object.file;
        let mut l = Linker { prefab, inst, file, arenas: Arenas::default() };
        let n = prefab.objects.len();
        let person = script(prefab, 0, "PersonBehaviour").map(|_| self.people.len());

        // TransformChain: each Rigidbody2D's parents up to the root (an inactive one's too: it has no body yet), the
        // first to reach one listing it.
        let mut body_parents = std::collections::BTreeSet::new();
        for o in (0..n).filter(|&o| prefab.objects[o].components.iter().any(|(_, x)| matches!(x, Component::Rigidbody2D(_)))) {
            let mut p = inst.objects[o].parent;
            while let Some(t) = p {
                // A static body never moves in the simulation, so its Transform serves as a plain node (the
                // Default map's walls under Root); a moving body's node follows its physical.
                let static_body = prefab.objects[t].components.iter().any(|(_, x)| matches!(x, Component::Rigidbody2D(rb) if rb.body_type == 2));
                if inst.bodies.contains_key(&t) && !static_body {
                    body_parents.insert(t);
                }
                if l.arenas.node.contains_key(&t) {
                    break;
                }
                l.arenas.node.insert(t, self.transform_nodes.len());
                let transform = prefab.objects[t].components.iter().position(|(_, c)| matches!(c, Component::Transform(_))).ok_or("an object without a Transform")?;
                self.transform_nodes.push(TransformNode { id: inst.objects[t].components[transform], trs: inst.objects[t].trs, parent: None });
                p = inst.objects[t].parent;
            }
        }
        // A Rigidbody2D without a PhysicalBehaviour: its own Transform is a node its body writes.
        let bare_bodies: Vec<usize> = (0..n).filter(|&o| script(prefab, o, "PhysicalBehaviour").is_none() && prefab.objects[o].components.iter().any(|(_, x)| matches!(x, Component::Rigidbody2D(_)))).collect();
        for &o in &bare_bodies {
            if !l.arenas.node.contains_key(&o) {
                l.arenas.node.insert(o, self.transform_nodes.len());
                let transform = prefab.objects[o].components.iter().position(|(_, c)| matches!(c, Component::Transform(_))).ok_or("an object without a Transform")?;
                self.transform_nodes.push(TransformNode { id: inst.objects[o].components[transform], trs: inst.objects[o].trs, parent: None });
            }
        }
        for (&o, &i) in &l.arenas.node {
            self.transform_nodes[i].parent = inst.objects[o].parent.map(|t| l.arenas.node[&t]);
        }

        // Arena slots first, so every reference resolves.
        let mut next = (self.physicals.len(), self.limbs.len(), self.grips.len(), self.hinges.len());
        for o in 0..n {
            if script(prefab, o, "PhysicalBehaviour").is_some() {
                l.arenas.physical.insert(o, next.0);
                next.0 += 1;
            }
            if script(prefab, o, "LimbBehaviour").is_some() {
                l.arenas.limb.insert(o, next.1);
                next.1 += 1;
            }
            if script(prefab, o, "GripBehaviour").is_some() {
                l.arenas.grip.insert(o, next.2);
                next.2 += 1;
            }
            for (c, (_, comp)) in prefab.objects[o].components.iter().enumerate() {
                match comp {
                    Component::Joint2D(j) if matches!(j.kind, JointKind::Hinge { .. }) => {
                        l.arenas.hinge.insert((o, c), next.3);
                        next.3 += 1;
                    }
                    _ => {}
                }
            }
        }
        for t in body_parents {
            let p = *l.arenas.physical.get(&t).ok_or_else(|| format!("{}: a body under a Rigidbody2D without a PhysicalBehaviour", prefab.objects[t].name))?;
            self.node_bodies.insert(l.arenas.node[&t], p);
        }
        let mut node_rigidbodies = HashMap::new();
        for &o in &bare_bodies {
            let Some(Component::Rigidbody2D(rb)) = prefab.objects[o].components.iter().map(|(_, x)| x).find(|x| matches!(x, Component::Rigidbody2D(_))) else { unreachable!() };
            // An inactive object's Rigidbody2D has no body (m_Body null) until it is activated.
            let (body, detached) = match inst.bodies.get(&o) {
                Some(&body) => (body, None),
                None if !inst.objects[o].active => (usize::MAX, Some(crate::b2::world::RigidbodyMass { use_auto_mass: rb.use_auto_mass, mass: rb.mass, inertia: None, center_of_mass: None })),
                None => unreachable!("an active Rigidbody2D without a body"),
            };
            let component = inst.objects[o].components[prefab.objects[o].components.iter().position(|(_, x)| matches!(x, Component::Rigidbody2D(_))).unwrap()];
            let rigidbody = Rigidbody { body, bodyType: rb.body_type, collisionDetectionMode: rb.collision_detection, interpolation: rb.interpolate, movement: MovementState::default(), detached, component, detached_props: [rb.linear_drag, rb.angular_drag, rb.gravity_scale] };
            self.rigidbody_hierarchy_changed |= detached.is_none() && rb.body_type != 2;
            let mut driven_by = None;
            if rb.body_type != 2 && detached.is_none() {
                let mut up = inst.objects[o].parent;
                while let Some(t) = up {
                    if prefab.objects[t].components.iter().any(|(_, x)| matches!(x, Component::Rigidbody2D(_))) {
                        driven_by = Some(*inst.bodies.get(&t).ok_or_else(|| format!("{}: driven by a Rigidbody2D without a body", prefab.objects[o].name))?);
                        break;
                    }
                    up = inst.objects[t].parent;
                }
            }
            node_rigidbodies.insert(o, self.node_rigidbodies.len());
            self.node_rigidbodies.push(NodeBody { node: l.arenas.node[&o], rigidbody, driven_by });
        }
        // Colliders and sprites without a PhysicalBehaviour on their GameObject: the Transforms down from the
        // nearest physical or node.
        let anchor_of = |o: usize| {
            let (mut chain, mut up) = (Vec::new(), Some(o));
            let anchor = loop {
                let Some(t) = up else { break BareAnchor::Root };
                if let Some(&p) = l.arenas.physical.get(&t) {
                    break BareAnchor::Physical(p);
                }
                if let Some(&i) = l.arenas.node.get(&t) {
                    break BareAnchor::Node(i);
                }
                chain.push(inst.objects[t].trs);
                up = inst.objects[t].parent;
            };
            (anchor, chain)
        };
        for o in (0..n).filter(|&o| !l.arenas.physical.contains_key(&o)) {
            for (c, (_, comp)) in prefab.objects[o].components.iter().enumerate() {
                match comp {
                    Component::Collider2D(_) => {
                        let (anchor, chain) = anchor_of(o);
                        let scripts = prefab.objects[o].components.iter().filter_map(|(_, x)| if let Component::Script { class, .. } = x { Some(class.clone()) } else { None }).collect();
                        self.bare_colliders.insert(inst.objects[o].components[c], BareCollider { anchor, chain, scripts, active_self: prefab.objects[o].active });
                    }
                    Component::SpriteRenderer(r) => {
                        let Some(sprite) = db.resolve(file, r.sprite) else { continue };
                        let (anchor, chain) = anchor_of(o);
                        let visible = inst.objects[o].active && r.enabled;
                        self.renderers.push(Renderer { anchor, chain, sprite: (sprite, r.color, r.sorting_order), id: inst.objects[o].components[c], draw: ([r.flip_x, r.flip_y], r.draw_mode, r.size), visible });
                    }
                    _ => {}
                }
            }
        }
        for class in ["CirculationBehaviour", "ConnectedNodeBehaviour", "SkinMaterialHandler"] {
            for o in 0..n {
                if (script(prefab, o, class).is_some()) != l.arenas.limb.contains_key(&o) {
                    return Err(format!("{}: {class} without its LimbBehaviour (or the reverse)", prefab.objects[o].name));
                }
            }
        }

        for o in 0..n {
            let Some((c, enabled, v)) = script(prefab, o, "PhysicalBehaviour") else { continue };
            let mut p = Physical { name: prefab.objects[o].name.clone(), id: inst.objects[o].components[c], ..Default::default() };
            load_values(p.fields(), v)?;
            p.enabled = enabled;
            p.active = inst.objects[o].active;
            // GameObject.isStatic: player data carries no static editor flags.
            p.isStatic = false;
            p.colliders = prefab.objects[o].components.iter().enumerate().filter(|(_, (_, x))| matches!(x, Component::Collider2D(_))).map(|(k, _)| inst.objects[o].components[k]).collect();
            p.collider_defs = prefab.objects[o].components.iter().filter_map(|(_, x)| match x {
                Component::Collider2D(c) => Some(c.clone()),
                _ => None,
            }).collect();
            p.all_colliders = p.colliders.clone();
            p.all_colliders.sort_by(|a, b| b.cmp(a));
            p.transform_id = inst.objects[o].components[0];
            p.BulletPenetration = field(v, "BulletPenetration")?.bool().ok_or("BulletPenetration")?;
            p.ReflectsLasers = field(v, "ReflectsLasers")?.bool().ok_or("ReflectsLasers")?;
            p.AbsorbsLasers = field(v, "AbsorbsLasers")?.bool().ok_or("AbsorbsLasers")?;
            p.BulletDeflectChanceOverride = float(v, "BulletDeflectChanceOverride")?;
            p.OverrideShotSounds = array(v, "OverrideShotSounds")?.len();
            // Start adds it to Global.PhysicalObjectsInWorld (an inactive object never starts).
            p.in_world = false;
            p.Properties = match db.resolve(file, field(v, "Properties")?.pptr().ok_or("Properties")?) {
                Some(asset) => self.physical_properties(db, asset)?,
                None => unimplemented!("{}: PhysicalBehaviour without Properties", p.name),
            };
            let Some(Component::Rigidbody2D(rb)) = prefab.objects[o].components.iter().map(|(_, x)| x).find(|x| matches!(x, Component::Rigidbody2D(_))) else {
                return Err(format!("{}: PhysicalBehaviour without a Rigidbody2D", p.name));
            };
            // An inactive object's Rigidbody2D has no body (m_Body null) until it is activated.
            let (body, detached) = match inst.bodies.get(&o) {
                Some(&body) => (body, None),
                None if !p.active => (usize::MAX, Some(crate::b2::world::RigidbodyMass { use_auto_mass: rb.use_auto_mass, mass: rb.mass, inertia: None, center_of_mass: None })),
                None => unreachable!("an active Rigidbody2D without a body"),
            };
            let component = inst.objects[o].components[prefab.objects[o].components.iter().position(|(_, x)| matches!(x, Component::Rigidbody2D(_))).unwrap()];
            p.rigidbody = Rigidbody { body, bodyType: rb.body_type, collisionDetectionMode: rb.collision_detection, interpolation: rb.interpolate, movement: MovementState::default(), detached, component, detached_props: [rb.linear_drag, rb.angular_drag, rb.gravity_scale] };
            // Rigidbody2D::Create set the scene's m_RigidbodyHierarchyChanged.
            self.rigidbody_hierarchy_changed |= detached.is_none() && rb.body_type != 2;
            p.transform = pose(inst, o);
            p.parent = inst.objects[o].parent.map(|t| l.arenas.node[&t]);
            // Rigidbody2D::Create's CheckForDrivenByParentRigidbody.
            if rb.body_type != 2 && detached.is_none() {
                let mut up = inst.objects[o].parent;
                while let Some(t) = up {
                    if prefab.objects[t].components.iter().any(|(_, x)| matches!(x, Component::Rigidbody2D(_))) {
                        p.driven_by = Some(*inst.bodies.get(&t).ok_or_else(|| format!("{}: driven by a Rigidbody2D without a body", p.name))?);
                        break;
                    }
                    up = inst.objects[t].parent;
                }
            }
            p.LocalColliderGridPoints = Some(array(v, "LocalColliderGridPoints")?.iter().map(floats::<2>).collect::<Result<_, _>>()?);
            // Awake's GetComponent<LocalFirePropagationMap>() and Start's GetComponents<DecalControllerBehaviour>().
            p.has_fire_map = script(prefab, o, "LocalFirePropagationMap").is_some();
            p.prefab_decal_controllers = prefab.objects[o].components.iter().filter(|(_, x)| matches!(x, Component::Script { class, .. } if class == "DecalControllerBehaviour")).count();
            p.audio_time_scale = script(prefab, o, "AudioSourceTimeScaleBehaviour").is_some();
            p.audioSource = prefab.objects[o].components.iter().any(|(_, x)| matches!(x, Component::Other(AUDIO_SOURCE)));
            p.MaximumStabVictims = field(v, "MaximumStabVictims")?.i64().ok_or("MaximumStabVictims")? as i32;
            p.LooseEndSize = float(v, "LooseEndSize")?;
            p.StabReleaseSpeedModifier = float(v, "StabReleaseSpeedModifier")?;
            p.StabWoundSizeMultiplier = float(v, "StabWoundSizeMultiplier")?;
            p.OverrideImpactSounds = array(v, "OverrideImpactSounds")?.len();
            let renderer = prefab.objects[o].components.iter().find_map(|(_, x)| match x {
                Component::SpriteRenderer(r) => Some(r),
                _ => None,
            });
            if let Some(r) = renderer {
                let sprite = db.resolve(file, r.sprite).ok_or("a SpriteRenderer without a sprite")?;
                p.sprite_bounds = Some(renderer_bounds(&Sprite::read(db.data(sprite))?, r));
                p.sprite = Some((sprite, r.color, r.sorting_order));
                p.sprite_draw = ([r.flip_x, r.flip_y], r.draw_mode, r.size);
            }
            self.physicals.push(p);
        }

        for o in 0..n {
            for (c, (_, comp)) in prefab.objects[o].components.iter().enumerate() {
                let Component::Joint2D(j) = comp else { continue };
                let JointKind::Hinge { use_motor, motor_speed, max_motor_torque, use_limits, lower, upper } = j.kind else { continue };
                let connected = l.object_of(j.connected_body)?;
                let h = Hinge {
                    joint: inst.joints.get(&(o, c)).copied(),
                    useMotor: use_motor,
                    motor: [motor_speed, max_motor_torque],
                    useLimits: use_limits,
                    limits: [lower, upper],
                    breakForce: j.break_force,
                    breakTorque: j.break_torque,
                    connected: connected.map(|b| inst.bodies.get(&b).copied().ok_or("a hinge to an object without a Rigidbody2D")).transpose()?,
                    anchor: j.anchor,
                    connectedAnchor: inst.hinges.get(&(o, c)).map_or(j.connected_anchor, |s| s.connected_anchor),
                    enabled: j.enabled,
                    destroyed: false,
                };
                // The create put the joint in m_JointBreakLimits as BreakJoint::Anchored(slot): it becomes Hinge(h), in place.
                if let Some(slot) = inst.hinges.get(&(o, c)).and_then(|s| s.slot) {
                    let anchored = BreakJoint::Anchored(slot);
                    for k in self.joint_break_limits.iter_mut().filter(|k| **k == anchored) {
                        *k = BreakJoint::Hinge(self.hinges.len());
                    }
                    self.anchored_joints[slot].joint = None;
                    self.anchored_joints[slot].hinge = Some(self.hinges.len());
                }
                self.hinges.push(h);
            }
        }

        for o in 0..n {
            let Some((c, enabled, v)) = script(prefab, o, "LimbBehaviour") else { continue };
            let mut b = Limb { name: prefab.objects[o].name.clone(), id: inst.objects[o].components[c], ..Default::default() };
            load_values(b.fields(), v)?;
            b.enabled = enabled;
            // Awake's GetComponent<PhysicalBehaviour/HingeJoint2D/GripBehaviour>() (the first of each on the object),
            // and PersonBehaviour.Awake's limb.Person = this.
            b.PhysicalBehaviour = *l.arenas.physical.get(&o).ok_or_else(|| format!("{}: LimbBehaviour without PhysicalBehaviour", b.name))?;
            b.Person = person.ok_or_else(|| format!("{}: a LimbBehaviour outside a Person", b.name))?;
            b.Joint = (0..prefab.objects[o].components.len()).find_map(|k| l.arenas.hinge.get(&(o, k)).copied());
            b.GripBehaviour = l.arenas.grip.get(&o).copied();
            b.ConnectedLimbs = l.limbs(v, "ConnectedLimbs")?;
            b.KillShotParticles = l.object(field(v, "KillShotParticles")?)?.is_some();
            b.NearestLimbToBrain = l.limb(field(v, "NearestLimbToBrain")?)?;
            b.StrengthMultipliers = array(v, "StrengthMultipliers")?.iter().map(|x| x.f32().ok_or("StrengthMultipliers")).collect::<Result<_, _>>()?;
            b.SpeciesIdentity = string(v, "SpeciesIdentity")?;
            b.BloodLiquidType = string(v, "BloodLiquidType")?;
            let decal = db.resolve(file, field(v, "BloodDecal")?.pptr().ok_or("BloodDecal")?).ok_or("LimbBehaviour without a BloodDecal")?;
            let decal = self.load_decal_descriptor(db, decal)?;
            b.BloodDecal = self.decal_descriptors[decal].name.clone();
            self.vital_parts.insert(self.limbs.len(), array(v, "VitalParts")?.iter().map(bounds).collect::<Result<_, _>>()?);
            // Awake's GetComponent<GoreStringBehaviour>().
            b.goreString = false;
            if let Some((_, gore_enabled, gv)) = script(prefab, o, "GoreStringBehaviour") {
                b.goreString = true;
                b.goreStringEnabled = gore_enabled;
                // GoreStringBehaviour.Joint is not serialized: Start makes it (CreateJointOnStart).
                b.goreStringJoint = false;
                b.gore = GoreString {
                    Other: l.limb(field(gv, "Other")?)?,
                    JointDampening: float(gv, "JointDampening")?,
                    JointFreq: float(gv, "JointFreq")?,
                    JointBreakThreshold: float(gv, "JointBreakThreshold")?,
                    InvincibilityTime: float(gv, "InvincibilityTime")?,
                    OriginLine: line(field(gv, "OriginLine")?)?,
                    OtherLine: line(field(gv, "OtherLine")?)?,
                    HadJointWhenSerialised: field(gv, "HadJointWhenSerialised")?.bool().ok_or("HadJointWhenSerialised")?,
                    TissueSnapClips: array(gv, "TissueSnapClips")?.len(),
                    AudioSource: l.object(field(gv, "AudioSource")?)?.is_some(),
                    ..Default::default()
                };
            }
            self.limbs.push(b);
        }

        for o in 0..n {
            let Some((_, _, v)) = script(prefab, o, "GripBehaviour") else { continue };
            let mut g = Grip::default();
            load_values(g.fields(), v)?;
            // Awake's GetComponent<PhysicalBehaviour>().
            g.PhysicalBehaviour = *l.arenas.physical.get(&o).ok_or("GripBehaviour without PhysicalBehaviour")?;
            g.CurrentlyHolding = l.physical(field(v, "CurrentlyHolding")?)?;
            self.grips.push(g);
        }

        for o in 0..n {
            let Some((_, enabled, v)) = script(prefab, o, "CirculationBehaviour") else { continue };
            let mut c = Circulation::default();
            load_values(c.fields(), v)?;
            c.enabled = enabled;
            c.Source = l.limb(field(v, "Source")?)?;
            c.PushesTo = l.limbs(v, "PushesTo")?;
            c.LocalArteryRects = Some(array(v, "LocalArteryRects")?.iter().map(bounds).collect::<Result<_, _>>()?);
            self.circulations.push(c);
        }
        for o in 0..n {
            let Some((_, _, v)) = script(prefab, o, "ConnectedNodeBehaviour") else { continue };
            let mut d = Node::default();
            load_values(d.fields(), v)?;
            d.Connections = array(v, "Connections")?.iter().map(|x| l.limb(x)).collect::<Result<_, _>>()?;
            self.nodes.push(d);
        }
        for o in 0..n {
            let Some((_, _, v)) = script(prefab, o, "SkinMaterialHandler") else { continue };
            let mut k = Skin::default();
            load_values(k.fields(), v)?;
            k.adjacentLimbs = l.limbs(v, "adjacentLimbs")?;
            self.skins.push(k);
        }
        // LimbBehaviour.Awake's GetOrAddComponent<GForceMeasureBehaviour>() adds the ones the prefab lacks; the
        // arena keeps the limb index only if the prefab has all or none.
        // Stored by ascending instance id, as a recording lists them.
        let mut map_lights = HashMap::new();
        let mut lights: Vec<(i32, usize)> = (0..n).filter_map(|o| script(prefab, o, "MapLightBehaviour").map(|(c, _, _)| (inst.objects[o].components[c], o))).collect();
        lights.sort();
        for (_, o) in lights {
            let (_, _, v) = script(prefab, o, "MapLightBehaviour").unwrap();
            let mut m = MapLight::default();
            load_values(m.fields(), v)?;
            // A serialized array is never null.
            m.switches = Some(array(v, "Switches")?.len());
            m.hasAudio = l.object(field(v, "AudioSource")?)?.is_some();
            map_lights.insert(o, self.map_lights.len());
            self.map_lights.push(m);
        }
        let mut waters = HashMap::new();
        for o in 0..n {
            let Some((c, enabled, v)) = script(prefab, o, "WaterBehaviour") else { continue };
            let component = |name: &str| -> Result<(usize, usize), String> {
                let pp = field(v, name)?.pptr().ok_or_else(|| name.to_string())?;
                prefab.index.get(&ObjRef { file, path_id: pp.path_id }).copied().filter(|_| !pp.is_null() && pp.file == 0).ok_or_else(|| format!("WaterBehaviour.{name} outside the prefab or null"))
            };
            let mut w = Water::default();
            load_values(w.fields(), v)?;
            (w.id, w.object, w.component, w.enabled, w.active) = (inst.objects[o].components[c], o, c, enabled, inst.objects[o].active);
            w.chain = inst.chain(o);
            let (to, tc) = component("Trigger")?;
            w.trigger = inst.objects[to].components[tc];
            w.trigger_fixtures = inst.fixtures.get(&(to, tc)).cloned().unwrap_or_default();
            w.ice_object = component("Ice")?.0;
            w.ice = *l.arenas.physical.get(&w.ice_object).ok_or("WaterBehaviour.Ice without a PhysicalBehaviour")?;
            w.boiling_sound = l.object(field(v, "BoilingWaterSound")?)?.ok_or("WaterBehaviour.BoilingWaterSound is null")?;
            for x in array(v, "ObjectsToBeActiveIfWater")? {
                w.objects_to_be_active.push(l.object(x)?.ok_or("a null ObjectsToBeActiveIfWater entry")?);
            }
            if let Some(r) = field(v, "Splash")?.pptr().and_then(|pp| db.resolve(file, pp)) {
                w.splash_pool = Some(self.splash_pool_for(db, r)?);
            }
            w.waterSizzles = db.resources_in("Audio/water_sizzle", 83)?.len();
            waters.insert(o, self.waters.len());
            self.waters.push(w);
        }
        // ActOnDestroy: Event's persistent calls may only reach audio and particles; DestroyWith is filled at run time.
        for o in 0..n {
            let Some((_, _, v)) = script(prefab, o, "ActOnDestroy") else { continue };
            if !presentation_event(prefab, file, field(v, "Event")?) {
                unimplemented!("ActOnDestroy.Event: a persistent call beyond audio and particles");
            }
            let mut a = super::destroyable::ActOnDestroy::default();
            for g in array(v, "DestroyWith")? {
                if let Some(x) = l.object(g)? {
                    a.destroy_with.push(inst.objects[x].components[0]);
                }
            }
            self.act_on_destroy.insert(inst.objects[o].components[0], a);
        }
        for o in 0..n {
            if let Some((c, _, v)) = script(prefab, o, "InstantiateObjectBehaviour") {
                let mut f = super::destroyable::InstantiateObject::default();
                load_values(f.fields(), v)?;
                let p = field(v, "Prefab")?.pptr().ok_or("Prefab")?;
                let source = if p.is_null() { None } else { Some(db.resolve(file, p).ok_or("InstantiateObjectBehaviour.Prefab outside the data")?) };
                self.instantiate_objects.insert(inst.objects[o].components[c], (source, f));
            }
            // DoAtTimeOnce: each Action's Event may call InstantiateObjectBehaviour.Instantiate, audio and particles.
            if let Some((c, _, v)) = script(prefab, o, "DoAtTimeOnce") {
                let mut actions = Vec::new();
                for a in array(v, "Actions")? {
                    let mut instantiates = Vec::new();
                    for call in invoked_calls(field(a, "Event")?) {
                        match call_target(prefab, file, call) {
                            Some((x, k))
                                if matches!(&prefab.objects[x].components[k].1, Component::Script { class, .. } if class == "InstantiateObjectBehaviour")
                                    && string(call, "m_MethodName")? == "Instantiate" =>
                            {
                                instantiates.push(inst.objects[x].components[k]);
                            }
                            _ if presentation_call(prefab, file, call) => {}
                            _ => unimplemented!("DoAtTimeOnce: a persistent call beyond InstantiateObjectBehaviour.Instantiate, audio and particles"),
                        }
                    }
                    let enabled = field(a, "Enabled")?.bool().ok_or("Enabled")?;
                    actions.push(super::destroyable::Action { Enabled: enabled, Delay: float(a, "Delay")?, Duration: float(a, "Duration")?, instantiates });
                }
                self.do_at_time_onces.push(super::destroyable::DoAtTimeOnce { id: inst.objects[o].components[c], actions });
            }
        }
        let mut destroyables = HashMap::new();
        for o in 0..n {
            let Some((c, _, v)) = script(prefab, o, "DestroyableBehaviour") else { continue };
            let mut x = super::destroyable::Destroyable::default();
            load_values(x.fields(), v)?;
            x.id = inst.objects[o].components[c];
            x.PhysicalBehaviour = *l.arenas.physical.get(&o).unwrap_or_else(|| unimplemented!("DestroyableBehaviour without a PhysicalBehaviour"));
            if l.arenas.limb.contains_key(&o) {
                unimplemented!("DestroyableBehaviour on a limb: SendMessage order with LimbBehaviour");
            }
            x.after_physical = script(prefab, o, "PhysicalBehaviour").is_some_and(|(pc, _, _)| pc < c);
            x.index = c;
            let debris = field(v, "DebrisPrefab")?.pptr().ok_or("DebrisPrefab")?;
            x.DebrisPrefab = if debris.is_null() { None } else { Some(db.resolve(file, debris).ok_or("DebrisPrefab outside the data")?) };
            let ancestor = field(v, "VelocityAncestor")?.pptr().ok_or("VelocityAncestor")?;
            if !ancestor.is_null() {
                let (ao, _) = *prefab.index.get(&ObjRef { file, path_id: ancestor.path_id }).filter(|_| ancestor.file == 0).ok_or("VelocityAncestor outside the prefab")?;
                x.VelocityAncestor = Some(*l.arenas.physical.get(&ao).unwrap_or_else(|| unimplemented!("DestroyableBehaviour.VelocityAncestor without a PhysicalBehaviour")));
            }
            x.OnBreakPresentation = presentation_event(prefab, file, field(v, "OnBreak")?);
            destroyables.insert(o, self.destroyables.len());
            self.destroyables.push(x);
        }
        let (mut explosives, mut delete_afters) = (HashMap::new(), HashMap::new());
        for o in 0..n {
            if let Some((c, _, v)) = script(prefab, o, "ExplosiveBehaviour") {
                let mut x = super::explosion::Explosive::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.physical = l.arenas.physical.get(&o).copied();
                if l.arenas.limb.contains_key(&o) {
                    unimplemented!("ExplosiveBehaviour on a limb");
                }
                if !field(v, "Killzone")?.pptr().ok_or("Killzone")?.is_null() {
                    unimplemented!("ExplosiveBehaviour with a serialized Killzone");
                }
                // Awake: BlockingLayers = 10752 when it is 0 (Objects, Debris, CollidingDebris + Bounds layers).
                let layers = field(field(v, "BlockingLayers")?, "m_Bits")?.i64().ok_or("BlockingLayers.m_Bits")? as u32;
                x.BlockingLayers = if layers == 0 { 10752 } else { layers };
                x.after_physical = script(prefab, o, "PhysicalBehaviour").is_some_and(|(pc, _, _)| pc < c);
                x.before_destroyable = script(prefab, o, "DestroyableBehaviour").is_some_and(|(dc, _, _)| c < dc);
                x.index = c;
                x.OnExplodeCalls = event_calls(prefab, file, field(v, "OnExplode")?, inst, "ExplosiveBehaviour.OnExplode")?;
                x.OnArmPresentation = presentation_event(prefab, file, field(v, "OnArm")?);
                let vfx = field(v, "VFXOverride")?.pptr().ok_or("VFXOverride")?;
                x.VFXOverride = if vfx.is_null() { None } else { Some(db.resolve(file, vfx).ok_or("VFXOverride outside the data")?) };
                explosives.insert(o, self.explosives.len());
                self.explosives.push(x);
            }
            if let Some((c, _, v)) = script(prefab, o, "DeleteAfterTime") {
                let mut x = super::explosion::DeleteAfter::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.OnEndOfLifePresentation = presentation_event(prefab, file, field(v, "OnEndOfLife")?);
                delete_afters.insert(o, self.delete_afters.len());
                self.delete_afters.push(x);
            }
        }
        let mut firearms = HashMap::new();
        for o in 0..n {
            let Some((c, enabled, v)) = script(prefab, o, "FirearmBehaviour") else { continue };
            let mut fa = super::firearm::Firearm::default();
            load_values(fa.fields(), v)?;
            fa.physical = l.arenas.physical.get(&o).copied();
            if fa.physical.is_none() {
                unimplemented!("FirearmBehaviour without a Rigidbody2D (NullReferenceException in Fire)");
            }
            (fa.id, fa.enabled) = (inst.objects[o].components[c], enabled);
            let cartridge = field(v, "Cartridge")?.pptr().ok_or("Cartridge")?;
            let Some(cartridge) = db.resolve(file, cartridge) else { unimplemented!("FirearmBehaviour.Awake with a null Cartridge (NullReferenceException)") };
            let (_, _, cv) = db.mono(cartridge)?.ok_or("Cartridge is not a MonoBehaviour")?;
            fa.Cartridge = super::ballistics::Cartridge::read(&cv)?;
            fa.ShotSounds = array(v, "ShotSounds")?.len();
            fa.InaccuracyMultipliers = array(v, "InaccuracyMultipliers")?.iter().map(|x| x.f32().ok_or("InaccuracyMultipliers")).collect::<Result<_, _>>()?;
            fa.LayersToHit = field(field(v, "LayersToHit")?, "m_Bits")?.i64().ok_or("LayersToHit.m_Bits")? as u32;
            let effect = field(v, "MuzzleFlashEffectOverride")?.pptr().ok_or("MuzzleFlashEffectOverride")?;
            if !effect.is_null() {
                let r = db.resolve(file, effect).ok_or("MuzzleFlashEffectOverride outside the data")?;
                let physics = db.prefab(r)?.objects.iter().any(|x| x.components.iter().any(|(_, c)| matches!(c, Component::Collider2D(_) | Component::Rigidbody2D(_))));
                if physics {
                    unimplemented!("FirearmBehaviour: a MuzzleFlashEffectOverride with physics (its pose)");
                }
                fa.MuzzleFlashEffectOverride = Some(r);
            }
            check_presentation_event(prefab, file, inst, field(v, "OnFire")?, "FirearmBehaviour.OnFire")?;
            // The ids an Instantiate of a Resources prefab takes: each GameObject and its components.
            let mut clone_ids = |path: &str| -> Result<usize, String> {
                let r = db.resource(path, 1)?.ok_or_else(|| format!("Resources.Load({path}) found no GameObject"))?;
                Ok(db.prefab(r)?.objects.iter().map(|x| 1 + x.components.len()).sum())
            };
            fa.casing_ids = clone_ids("Prefabs/BulletCasing")?;
            fa.smoke_ids = if fa.MuzzleFlashEffectOverride.is_some() { 0 } else { clone_ids("Prefabs/MuzzleSmoke")? };
            firearms.insert(o, self.firearms.len());
            self.firearms.push(fa);
        }
        let mut machine_guns = HashMap::new();
        for o in 0..n {
            let Some((c, _, v)) = script(prefab, o, "MachineGunBehaviour") else { continue };
            let mut x = super::machinegun::MachineGun::default();
            load_values(x.fields(), v)?;
            x.id = inst.objects[o].components[c];
            x.physical = l.arenas.physical.get(&o).copied().unwrap_or_else(|| unimplemented!("MachineGunBehaviour without a PhysicalBehaviour (NullReferenceException)"));
            let cartridge = field(v, "Cartridge")?.pptr().ok_or("Cartridge")?;
            let Some(cartridge) = db.resolve(file, cartridge) else { unimplemented!("MachineGunBehaviour with a null Cartridge (NullReferenceException)") };
            let (_, _, cv) = db.mono(cartridge)?.ok_or("Cartridge is not a MonoBehaviour")?;
            x.Cartridge = super::ballistics::Cartridge::read(&cv)?;
            x.ExplosionSettings = explosion_parameters(field(v, "ExplosionSettings")?)?;
            x.Sounds = array(v, "Sounds")?.len();
            for (name, slot) in [("ImpactEffect", &mut x.ImpactEffect), ("RicochetEffect", &mut x.RicochetEffect)] {
                let p = field(v, name)?.pptr().ok_or(name)?;
                if !p.is_null() {
                    let r = db.resolve(file, p).ok_or_else(|| format!("MachineGunBehaviour.{name} outside the data"))?;
                    if db.prefab(r)?.objects.iter().any(|x| x.components.iter().any(|(_, c)| matches!(c, Component::Collider2D(_) | Component::Rigidbody2D(_)))) {
                        unimplemented!("MachineGunBehaviour: an {name} with physics (its pose)");
                    }
                    *slot = Some(r);
                }
            }
            // Start's Effect pose for a flipped root: a particle system without physics.
            let effect = field(v, "Effect")?.pptr().ok_or("Effect")?;
            if !effect.is_null() {
                let (eo, _) = *prefab.index.get(&ObjRef { file, path_id: effect.path_id }).filter(|_| effect.file == 0).ok_or("MachineGunBehaviour.Effect outside the prefab")?;
                if std::iter::once(eo).chain(inst.descendants(eo)).any(|d| prefab.objects[d].components.iter().any(|(_, c)| matches!(c, Component::Collider2D(_) | Component::Rigidbody2D(_)))) {
                    unimplemented!("MachineGunBehaviour.Effect with physics (its pose in Start)");
                }
            }
            machine_guns.insert(o, self.machine_guns.len());
            self.machine_guns.push(x);
        }
        let mut bells = HashMap::new();
        for o in 0..n {
            let Some((_, _, v)) = script(prefab, o, "BellBehaviour") else { continue };
            let mut x = super::bell::Bell::default();
            load_values(x.fields(), v)?;
            let r = field(v, "PhysicalBehaviour")?.pptr().ok_or("PhysicalBehaviour")?;
            let own = script(prefab, o, "PhysicalBehaviour").map(|(pc, _, _)| prefab.objects[o].components[pc].0.path_id);
            if r.file != 0 || own != Some(r.path_id) {
                unimplemented!("BellBehaviour.PhysicalBehaviour not on its own GameObject");
            }
            x.physical = l.arenas.physical[&o];
            bells.insert(o, self.bells.len());
            self.bells.push(x);
        }
        let mut launchers = HashMap::new();
        for o in 0..n {
            let Some((c, enabled, v)) = script(prefab, o, "ProjectileLauncherBehaviour") else { continue };
            let mut x = super::launcher::Launcher::default();
            load_values(x.fields(), v)?;
            (x.id, x.enabled) = (inst.objects[o].components[c], enabled);
            x.physical = l.arenas.physical.get(&o).copied().unwrap_or_else(|| unimplemented!("ProjectileLauncherBehaviour without a PhysicalBehaviour (NullReferenceException)"));
            let asset = field(v, "projectileAsset")?.pptr().ok_or("projectileAsset")?;
            x.projectile = if !asset.is_null() {
                let r = db.resolve(file, asset).ok_or("projectileAsset outside the data")?;
                let (_, mb, av) = db.mono(r)?.ok_or("SpawnableAsset")?;
                let prefab = db.resolve(r.file, av.get("Prefab").and_then(Value::pptr).ok_or("SpawnableAsset.Prefab")?);
                Some((prefab.unwrap_or_else(|| unimplemented!("ProjectileLauncherBehaviour: a SpawnableAsset with a null Prefab")), Some(mb.name.clone())))
            } else {
                let p = field(v, "projectilePrefab")?.pptr().ok_or("projectilePrefab")?;
                if p.is_null() { None } else { Some((db.resolve(file, p).ok_or("projectilePrefab outside the data")?, None)) }
            };
            let flash = field(v, "muzzleFlashPrefab")?.pptr().ok_or("muzzleFlashPrefab")?;
            if !flash.is_null() {
                let r = db.resolve(file, flash).ok_or("muzzleFlashPrefab outside the data")?;
                let effect = db.prefab(r)?;
                let poolable = effect.objects[0].tag == super::water::POOLABLE_TAG;
                if !poolable && effect.objects.iter().any(|x| x.components.iter().any(|(_, c)| matches!(c, Component::Collider2D(_) | Component::Rigidbody2D(_)))) {
                    unimplemented!("ProjectileLauncherBehaviour: a muzzleFlashPrefab with physics (its pose)");
                }
                x.muzzle_flash = Some((r, poolable));
            }
            x.launch_sound = !field(v, "launchSound")?.pptr().ok_or("launchSound")?.is_null();
            x.LaunchClips = array(v, "LaunchClips")?.len();
            check_presentation_event(prefab, file, inst, field(v, "OnLaunchEvent")?, "ProjectileLauncherBehaviour.OnLaunchEvent")?;
            launchers.insert(o, self.launchers.len());
            self.launchers.push(x);
        }
        // UseEventTrigger.Use invokes Event (nothing sets its Action): presentation only, so Use skips it.
        for o in 0..n {
            if let Some((_, _, v)) = script(prefab, o, "UseEventTrigger") {
                check_presentation_event(prefab, file, inst, field(v, "Event")?, "UseEventTrigger.Event")?;
            }
        }
        let (mut act_on_collides, mut point_to_velocities, mut cars, mut tyres, mut flasks, mut truck_doors) = (HashMap::new(), HashMap::new(), HashMap::new(), HashMap::new(), HashMap::new(), HashMap::new());
        let (mut glowtubes, mut leds, mut detectors, mut sticky_grenades) = (HashMap::new(), HashMap::new(), HashMap::new(), HashMap::new());
        let mut liquid_containers = HashMap::new();
        for o in 0..n {
            if let Some((c, _, v)) = script(prefab, o, "LiquidContainerController") {
                let mut x = super::flask::LiquidContainer::default();
                (x.id, x.FillPercentage) = (inst.objects[o].components[c], float(v, "FillPercentage")?);
                let b = field(v, "Container")?;
                let (center, extent) = (field(b, "m_Center")?, field(b, "m_Extent")?);
                // Bounds.min and max: center -/+ extents.
                (x.min, x.max) = ([float(center, "x")? - float(extent, "x")?, float(center, "y")? - float(extent, "y")?], [float(center, "x")? + float(extent, "x")?, float(center, "y")? + float(extent, "y")?]);
                liquid_containers.insert(o, self.liquid_containers.len());
                self.liquid_containers.push(x);
            }
        }
        for o in 0..n {
            if let Some((c, enabled, v)) = script(prefab, o, "ActOnCollide") {
                let mut x = super::acts::ActOnCollide::default();
                load_values(x.fields(), v)?;
                (x.enabled, x.physical, x.index) = (enabled, l.arenas.physical.get(&o).copied(), c);
                x.after_physical = script(prefab, o, "PhysicalBehaviour").is_some_and(|(pc, _, _)| pc < c);
                if let Some(p) = field(v, "Collider")?.pptr().filter(|p| !p.is_null()) {
                    let (co, cc) = *prefab.index.get(&ObjRef { file, path_id: p.path_id }).filter(|_| p.file == 0).ok_or("ActOnCollide.Collider outside the prefab")?;
                    x.Collider = Some(inst.objects[co].components[cc]);
                }
                // Actions: ExplosiveBehaviour.Activate (the mine), audio and particles.
                for call in invoked_calls(field(v, "Actions")?) {
                    match call_target(prefab, file, call) {
                        Some((to, tc)) if matches!(&prefab.objects[to].components[tc].1, Component::Script { class, .. } if class == "ExplosiveBehaviour") && string(call, "m_MethodName")? == "Activate" => {
                            x.targets.push(super::acts::ActTarget::Activate(explosives[&to]));
                        }
                        Some((to, tc)) if matches!(&prefab.objects[to].components[tc].1, Component::Script { class, .. } if class == "FirearmBehaviour") && string(call, "m_MethodName")? == "Shoot" => {
                            x.targets.push(super::acts::ActTarget::Shoot(firearms[&to]));
                        }
                        Some((to, tc)) if matches!(&prefab.objects[to].components[tc].1, Component::Script { class, .. } if class == "ProjectileLauncherBehaviour") && string(call, "m_MethodName")? == "Launch" => {
                            x.targets.push(super::acts::ActTarget::Launch(launchers[&to]));
                        }
                        _ if presentation_call(prefab, file, call) => {}
                        _ => unimplemented!("ActOnCollide.Actions: a persistent call beyond ExplosiveBehaviour.Activate, audio and particles ({})", string(call, "m_MethodName")?),
                    }
                }
                act_on_collides.insert(o, self.act_on_collides.len());
                self.act_on_collides.push(x);
            }
            if let Some((c, _, v)) = script(prefab, o, "NoCollide") {
                let set = |name: &str| -> Result<Vec<Option<i32>>, String> {
                    array(v, name)?
                        .iter()
                        .map(|r| {
                            let p = r.pptr().ok_or("NoCollide: not a reference")?;
                            if p.is_null() {
                                return Ok(None);
                            }
                            let (co, cc) = *prefab.index.get(&ObjRef { file, path_id: p.path_id }).filter(|_| p.file == 0).ok_or("NoCollide: a collider outside the prefab")?;
                            Ok(Some(inst.objects[co].components[cc]))
                        })
                        .collect()
                };
                let bis_a = field(v, "BisA")?.bool().ok_or("BisA")?;
                self.no_collides.push(super::acts::NoCollide { id: inst.objects[o].components[c], a: set("NoCollideSetA")?, b: set("NoCollideSetB")?, bis_a });
            }
            if let Some((c, _, v)) = script(prefab, o, "StickyGrenadeBehaviour") {
                let mut x = super::acts::StickyGrenade::default();
                x.physical = l.arenas.physical.get(&o).copied();
                x.index = c;
                x.after_physical = script(prefab, o, "PhysicalBehaviour").is_some_and(|(pc, _, _)| pc < c);
                let r = field(v, "Explosive")?.pptr().ok_or("Explosive")?;
                let (eo, _) = *prefab.index.get(&ObjRef { file, path_id: r.path_id }).filter(|_| r.file == 0).ok_or("StickyGrenadeBehaviour.Explosive outside the prefab")?;
                x.explosive = *explosives.get(&eo).ok_or("StickyGrenadeBehaviour.Explosive is not an ExplosiveBehaviour")?;
                // Awake: unstickyPropertySet from StickyPropertiesToIgnore.
                for a in array(v, "StickyPropertiesToIgnore")? {
                    let asset = a.pptr().ok_or("StickyPropertiesToIgnore")?;
                    let asset = db.resolve(file, asset).ok_or("StickyPropertiesToIgnore outside the data")?;
                    x.unsticky.push(self.physical_properties(db, asset)?);
                }
                sticky_grenades.insert(o, self.sticky_grenades.len());
                self.sticky_grenades.push(x);
            }
            // FixedUpdate loops over Joints: with none, nothing happens.
            if let Some((_, _, v)) = script(prefab, o, "ActOnHingeJointLimit") {
                if !array(v, "Joints")?.is_empty() {
                    unimplemented!("ActOnHingeJointLimit with Joints: HingeJoint2D.limitState");
                }
            }
            if let Some((_, _, v)) = script(prefab, o, "PointToVelocityBehaviour") {
                let mut x = super::acts::PointToVelocity::default();
                load_values(x.fields(), v)?;
                x.physical = *l.arenas.physical.get(&o).unwrap_or_else(|| unimplemented!("PointToVelocityBehaviour without a PhysicalBehaviour: GetComponent<Rigidbody2D>() of a node"));
                point_to_velocities.insert(o, self.point_to_velocities.len());
                self.point_to_velocities.push(x);
            }
            if let Some((c, enabled, v)) = script(prefab, o, "CarBehaviour") {
                let mut x = super::car::CarBehaviour::default();
                load_values(x.fields(), v)?;
                x.enabled = enabled;
                x.id = inst.objects[o].components[c];
                // Awake: Phys = GetComponent<PhysicalBehaviour>() unless serialized.
                let phys = l.in_arena(field(v, "Phys")?, &l.arenas.physical, "PhysicalBehaviour")?;
                x.own_phys = phys.is_none();
                x.Phys = phys.or_else(|| l.arenas.physical.get(&o).copied()).unwrap_or_else(|| unimplemented!("CarBehaviour without a PhysicalBehaviour"));
                x.own_phys = x.own_phys || l.arenas.physical.get(&o) == Some(&x.Phys);
                x.AccelerationCurve = Some(curve(field(v, "AccelerationCurve")?)?);
                cars.insert(o, self.cars.len());
                self.cars.push(x);
            }
            if let Some((c, _, v)) = script(prefab, o, "TyreDeflationBehaviour") {
                let mut x = super::tyre::TyreDeflationBehaviour::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.Phys = l.in_arena(field(v, "Phys")?, &l.arenas.physical, "PhysicalBehaviour")?.ok_or("TyreDeflationBehaviour.Phys is null")?;
                let p = field(v, "Collider")?.pptr().filter(|p| !p.is_null() && p.file == 0).ok_or("TyreDeflationBehaviour.Collider")?;
                x.collider = *prefab.index.get(&ObjRef { file, path_id: p.path_id }).ok_or("TyreDeflationBehaviour.Collider outside the prefab")?;
                if !matches!(&prefab.objects[x.collider.0].components[x.collider.1].1, Component::Collider2D(c) if matches!(c.shape, crate::assets::components::ColliderShape::Circle { .. })) {
                    return Err("TyreDeflationBehaviour.Collider is not a CircleCollider2D".into());
                }
                let settings = field(v, "DeflatedColliderSettings")?;
                x.DeflatedRadius = float(settings, "Radius")?;
                x.DeflatedOffset.set(field(settings, "Offset")?)?;
                x.sounds = array(v, "TyreDeflationSound")?.len();
                tyres.insert(o, self.tyres.len());
                self.tyres.push(x);
            }
            if let Some((c, _, v)) = script(prefab, o, "GasTankBehaviour").or_else(|| script(prefab, o, "FlaskBehaviour")) {
                let mut x = super::flask::Flask::default();
                load_values(x.fields(), v)?;
                // GasTankBehaviour.Limits is (0, 20); FlaskBehaviour's (0, Capacity).
                x.limit = if script(prefab, o, "GasTankBehaviour").is_some() { 20.0 } else { x.Capacity };
                x.id = inst.objects[o].components[c];
                if !presentation_event(prefab, file, field(v, "OnDistributionChange")?) {
                    unimplemented!("BloodContainer.OnDistributionChange: a persistent call beyond audio and particles");
                }
                x.LiquidContainer = l.in_arena(field(v, "LiquidContainer")?, &liquid_containers, "LiquidContainerController")?;
                let start = field(v, "StartLiquid")?;
                x.StartLiquid = string(start, "LiquidID")?;
                x.StartAmount = float(start, "Amount")?;
                flasks.insert(o, self.flasks.len());
                self.flasks.push(x);
            }
            if let Some((c, _, v)) = script(prefab, o, "GlowtubeBehaviour") {
                let mut x = super::light::Glowtube::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.Phys = l.in_arena(field(v, "PhysicalBehaviour")?, &l.arenas.physical, "PhysicalBehaviour")?.ok_or("GlowtubeBehaviour.PhysicalBehaviour is null")?;
                let gas = field(v, "Gas")?.pptr().ok_or("Gas")?;
                x.Gas = if gas.is_null() { None } else { Some(db.resolve(file, gas).ok_or("GlowtubeBehaviour.Gas outside the data")?) };
                glowtubes.insert(o, self.glowtubes.len());
                self.glowtubes.push(x);
            }
            if let Some((c, enabled, v)) = script(prefab, o, "LEDBulbBehaviour") {
                let mut x = super::light::LedBulb::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                x.Phys = l.in_arena(field(v, "PhysicalBehaviour")?, &l.arenas.physical, "PhysicalBehaviour")?.ok_or("LEDBulbBehaviour.PhysicalBehaviour is null")?;
                leds.insert(o, self.leds.len());
                self.leds.push(x);
            }
            if let Some((c, enabled, v)) = script(prefab, o, "DetectorBehaviour") {
                let mut x = super::detector::Detector::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                x.Phys = l.in_arena(field(v, "PhysicalBehaviour")?, &l.arenas.physical, "PhysicalBehaviour")?.ok_or("DetectorBehaviour.PhysicalBehaviour is null")?;
                x.LayersToDetect = field(field(v, "LayersToDetect")?, "m_Bits")?.i64().ok_or("LayersToDetect.m_Bits")? as u32;
                // SendMessage("IsolatedActivation") and ("IsolatedContinuousActivation") reach the detector's own GameObject.
                if script(prefab, o, "UseWireBehaviour").is_some() {
                    unimplemented!("DetectorBehaviour.IsolatedActivation received by a UseWireBehaviour");
                }
                for f in array(v, "PropertyFilter")? {
                    let asset = db.resolve(file, f.pptr().ok_or("PropertyFilter")?).ok_or("PropertyFilter outside the data")?;
                    x.PropertyFilter.push(self.physical_properties(db, asset)?);
                }
                detectors.insert(o, self.detectors.len());
                self.detectors.push(x);
            }
            if let Some((c, _, v)) = script(prefab, o, "TruckDoorBehaviour") {
                let mut x = super::car::TruckDoor::default();
                load_values(x.fields(), v)?;
                x.joint = (0..prefab.objects[o].components.len()).find_map(|k| l.arenas.hinge.get(&(o, k)).copied());
                x.root_physical = l.arenas.physical.get(&0).copied();
                truck_doors.insert(o, self.truck_doors.len());
                self.truck_doors.push((inst.objects[o].components[c], x));
            }
        }
        let mut machinery = HashMap::new();
        for o in 0..n {
            let Some((_, _, v)) = script(prefab, o, "DamagableMachineryBehaviour") else { continue };
            let mut m = super::machinery::Machinery::default();
            load_values(m.fields(), v)?;
            m.PhysicalBehaviour = *l.arenas.physical.get(&o).ok_or("DamagableMachineryBehaviour without a PhysicalBehaviour")?;
            // SparkPrefab, else Resources.Load("Prefabs/BrokenElectronicsSpark").
            m.SparkPrefab = l.object(field(v, "SparkPrefab")?)?.is_some() || db.resource("Prefabs/BrokenElectronicsSpark", 1)?.is_some();
            m.ExplosionPrefab = !field(v, "ExplosionPrefab")?.pptr().ok_or("ExplosionPrefab")?.is_null();
            for b in array(v, "BehavioursToToggle")? {
                if let Some(t) = b.pptr().filter(|p| !p.is_null()) {
                    let (to, tc) = prefab.index[&ObjRef { file, path_id: t.path_id }];
                    let Component::Script { class, .. } = &prefab.objects[to].components[tc].1 else { unreachable!() };
                    m.BehavioursToToggle.push((class.clone(), inst.objects[to].components[tc]));
                }
            }
            for g in array(v, "GameObjectsToToggle")? {
                if let Some(x) = l.object(g)? {
                    m.GameObjectsToToggle.push(inst.objects[x].components[0]);
                }
            }
            m.OnDamagedPresentation = presentation_event(prefab, file, field(v, "OnDamaged")?);
            machinery.insert(o, self.machinery.len());
            self.machinery.push(m);
        }
        let (mut generators, mut floodlights, mut spike_grenades, mut heating_elements, mut ray_guns) = (HashMap::new(), HashMap::new(), HashMap::new(), HashMap::new(), HashMap::new());
        for o in 0..n {
            if let Some((c, enabled, v)) = script(prefab, o, "GeneratorBehaviour") {
                let mut x = super::devices::Generator::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                x.Phys = l.in_arena(field(v, "physicalBehaviour")?, &l.arenas.physical, "PhysicalBehaviour")?.ok_or("GeneratorBehaviour.physicalBehaviour is null")?;
                generators.insert(o, self.generators.len());
                self.generators.push(x);
            }
            if let Some((c, enabled, v)) = script(prefab, o, "SingleFloodlightBehaviour") {
                let mut x = super::devices::Floodlight::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                x.ToToggle = l.object(field(v, "ToToggle")?)?;
                x.SwitchSounds = array(v, "SwitchSounds")?.len();
                x.OnSwitchSound = array(v, "OnSwitchSound")?.len();
                floodlights.insert(o, self.floodlights.len());
                self.floodlights.push(x);
            }
            if let Some((c, enabled, v)) = script(prefab, o, "SpikeGrenadeBehaviour") {
                let mut x = super::devices::SpikeGrenade::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                x.Phys = *l.arenas.physical.get(&o).ok_or("SpikeGrenadeBehaviour without a PhysicalBehaviour")?;
                for (name, slot) in [("SpikePrefab", &mut x.SpikePrefab), ("ExplosionVFX", &mut x.ExplosionVFX)] {
                    let p = field(v, name)?.pptr().ok_or(name)?;
                    *slot = if p.is_null() { None } else { Some(db.resolve(file, p).ok_or_else(|| format!("SpikeGrenadeBehaviour.{name} outside the data"))?) };
                }
                x.CollisionLayers = field(field(v, "CollisionLayers")?, "m_Bits")?.i64().ok_or("CollisionLayers.m_Bits")? as u32;
                spike_grenades.insert(o, self.spike_grenades.len());
                self.spike_grenades.push(x);
            }
            if let Some((_, _, v)) = script(prefab, o, "HeatingElementBehaviour") {
                let mut x = super::devices::HeatingElement::default();
                load_values(x.fields(), v)?;
                x.Phys = l.in_arena(field(v, "PhysicalBehaviour")?, &l.arenas.physical, "PhysicalBehaviour")?.ok_or("HeatingElementBehaviour.PhysicalBehaviour is null")?;
                let m = l.object(field(v, "DamagableMachineryBehaviour")?)?.ok_or("HeatingElementBehaviour.DamagableMachineryBehaviour is null")?;
                x.Machinery = *machinery.get(&m).ok_or("HeatingElementBehaviour: no DamagableMachineryBehaviour on its object")?;
                heating_elements.insert(o, self.heating_elements.len());
                self.heating_elements.push(x);
            }
            if let Some((c, enabled, v)) = script(prefab, o, "TemperatureRayGunBehaviour") {
                let mut x = super::devices::RayGun::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                x.Phys = l.in_arena(field(v, "PhysicalBehaviour")?, &l.arenas.physical, "PhysicalBehaviour")?.ok_or("TemperatureRayGunBehaviour.PhysicalBehaviour is null")?;
                x.Layers = field(field(v, "Layers")?, "m_Bits")?.i64().ok_or("Layers.m_Bits")? as u32;
                // LineRenderer.transform: its object's local TRS and each parent's below the gun's own object.
                let mut at = l.object(field(v, "LineRenderer")?)?.ok_or("TemperatureRayGunBehaviour.LineRenderer is null")?;
                while at != o {
                    x.Barrel.push(inst.objects[at].trs);
                    at = inst.objects[at].parent.ok_or("TemperatureRayGunBehaviour.LineRenderer is outside the gun")?;
                }
                ray_guns.insert(o, self.ray_guns.len());
                self.ray_guns.push(x);
            }
        }
        let mut fire_maps = HashMap::new();
        for o in 0..n {
            let Some((_, _, v)) = script(prefab, o, "LocalFirePropagationMap") else { continue };
            let p = *l.arenas.physical.get(&o).unwrap_or_else(|| unimplemented!("LocalFirePropagationMap without a PhysicalBehaviour (NullReferenceException)"));
            let mut m = super::fire::FireMap::new(p);
            m.GridStep = float(v, "GridStep")?;
            m.FireIntensityDampening = float(v, "FireIntensityDampening")?;
            m.FireSpreadSpeed = float(v, "FireSpreadSpeed")?;
            let pp = field(v, "Collider")?.pptr().ok_or("Collider")?;
            if !pp.is_null() {
                let (co, cc) = prefab.index.get(&ObjRef { file, path_id: pp.path_id }).copied().filter(|_| pp.file == 0).ok_or("LocalFirePropagationMap.Collider outside the prefab")?;
                m.collider = Some(inst.objects[co].components[cc]);
            }
            for x in array(v, "Points")? {
                let neighbours = array(x, "NeighbourIndices")?.iter().map(|i| i.i64().map(|i| i as i32).ok_or("NeighbourIndices")).collect::<Result<_, _>>()?;
                m.Points.push(super::fire::BurnPoint { LocalPoint: floats(field(x, "LocalPoint")?)?, FireIntensity: float(x, "FireIntensity")?, Softness: float(x, "Softness")?, NeighbourIndices: neighbours });
            }
            self.physicals[p].fire_map = Some(self.fire_maps.len());
            fire_maps.insert(o, self.fire_maps.len());
            self.fire_maps.push(m);
        }
        let (mut lavas, mut spawners, mut heat_rects) = (HashMap::new(), HashMap::new(), HashMap::new());
        for o in 0..n {
            if let Some((_, _, v)) = script(prefab, o, "LavaBehaviour") {
                let mut lava = super::scenery::Lava::default();
                load_values(lava.fields(), v)?;
                let pp = field(v, "Trigger")?.pptr().ok_or("Trigger")?;
                let (to, tc) = prefab.index.get(&ObjRef { file, path_id: pp.path_id }).copied().filter(|_| !pp.is_null() && pp.file == 0).ok_or("LavaBehaviour.Trigger outside the prefab or null")?;
                lava.trigger_fixtures = inst.fixtures.get(&(to, tc)).cloned().unwrap_or_default();
                let filter = field(v, "ContactFilter")?;
                let flag = |name: &str| field(filter, name).and_then(|x| x.bool().ok_or_else(|| name.to_string()));
                if flag("useDepth")? || flag("useOutsideDepth")? || flag("useNormalAngle")? || flag("useOutsideNormalAngle")? {
                    unimplemented!("LavaBehaviour.ContactFilter depth or normal angle filtering");
                }
                if flag("useLayerMask")? {
                    lava.layer_mask = field(field(filter, "layerMask")?, "m_Bits")?.i64().ok_or("layerMask.m_Bits")? as u32;
                }
                if let Some(r) = field(v, "ImpactPrefab")?.pptr().and_then(|pp| db.resolve(file, pp)) {
                    lava.impact = Some(self.splash_pool_for(db, r)?);
                }
                lavas.insert(o, self.lavas.len());
                self.lavas.push(lava);
            }
            if let Some((_, _, v)) = script(prefab, o, "RandomPrefabSpawnerBehaviour") {
                let mut s = super::scenery::Spawner::default();
                load_values(s.fields(), v)?;
                for p in array(v, "Prefabs")? {
                    s.prefabs.push(db.resolve(file, p.pptr().ok_or("Prefabs")?));
                }
                s.chain = inst.chain(o);
                spawners.insert(o, self.spawners.len());
                self.spawners.push(s);
            }
            if let Some((_, _, v)) = script(prefab, o, "TemperatureTargetRectangleBehaviour") {
                let mut h = super::scenery::HeatRect::default();
                load_values(h.fields(), v)?;
                let ints = |name: &str| -> Result<[i32; 3], String> {
                    let x = field(v, name)?;
                    let c = |k: &str| field(x, k).and_then(|y| y.i64().map(|i| i as i32).ok_or_else(|| format!("{name}.{k}")));
                    Ok([c("x")?, c("y")?, c("z")?])
                };
                (h.Size, h.Offset, h.position) = (ints("Size")?, ints("Offset")?, inst.position(o));
                heat_rects.insert(o, self.heat_rects.len());
                self.heat_rects.push(h);
            }
        }
        let mut random_sprites = HashMap::new();
        for o in 0..n {
            let Some((_, _, v)) = script(prefab, o, "RandomSpriteBehaviour") else { continue };
            let physical = *l.arenas.physical.get(&o).unwrap_or_else(|| unimplemented!("RandomSpriteBehaviour without a PhysicalBehaviour"));
            let r = prefab.objects[o].components.iter().find_map(|(_, x)| match x {
                Component::SpriteRenderer(r) => Some(r),
                _ => None,
            });
            let r = r.ok_or("RandomSpriteBehaviour without a SpriteRenderer")?;
            let mut sprites = Vec::new();
            for s in array(v, "sprites")? {
                let s = db.resolve(file, s.pptr().ok_or("sprites")?).unwrap_or_else(|| unimplemented!("RandomSpriteBehaviour: a null sprite"));
                sprites.push((s, renderer_bounds(&Sprite::read(db.data(s))?, r)));
            }
            random_sprites.insert(o, self.random_sprites.len());
            self.random_sprites.push(super::misc::RandomSprite {
                PhysicalBehaviour: physical,
                PickRandomOnStart: field(v, "PickRandomOnStart")?.bool().ok_or("PickRandomOnStart")?,
                chosenIndex: field(v, "chosenIndex")?.i64().ok_or("chosenIndex")? as i32,
                sprites,
                OnAfterChangePresentation: presentation_event(prefab, file, field(v, "OnAfterChange")?),
            });
        }
        for o in 0..n {
            for (c, (_, comp)) in prefab.objects[o].components.iter().enumerate() {
                if let Component::Script { class, fields: v, .. } = comp {
                    if class == "NoCollideGm" {
                        self.no_collide_gms.push(super::car::NoCollideGm { id: inst.objects[o].components[c], a: l.object(field(v, "A")?)?, b: l.object(field(v, "B")?)? });
                    }
                }
            }
        }
        for o in 0..n {
            let Some((c, _, v)) = script(prefab, o, "RandomCarTextureBehaviour") else { continue };
            let mut slots = Vec::new();
            let mut renderers = Vec::new();
            for name in ["Body", "Background", "Bonnet", "Boot", "FrontDoor", "BackDoor"] {
                let r = field(v, name)?.pptr().ok_or(name)?;
                let (ro, rc) = *prefab.index.get(&ObjRef { file, path_id: r.path_id }).filter(|_| r.file == 0).ok_or("RandomCarTextureBehaviour: a renderer outside the prefab")?;
                let Component::SpriteRenderer(data) = &prefab.objects[ro].components[rc].1 else { return Err(format!("RandomCarTextureBehaviour.{name}: not a SpriteRenderer")) };
                slots.push(match l.arenas.physical.get(&ro) {
                    Some(&p) => super::car::CarSlot::Physical(p),
                    None => super::car::CarSlot::Renderer(inst.objects[ro].components[rc]),
                });
                renderers.push(data);
            }
            let mut textures = Vec::new();
            for t in array(v, "Textures")? {
                let mut sprites = Vec::new();
                for (name, r) in ["Body", "Background", "Bonnet", "Boot", "FrontDoor", "BackDoor"].into_iter().zip(&renderers) {
                    let s = db.resolve(file, field(t, name)?.pptr().ok_or(name)?).unwrap_or_else(|| unimplemented!("RandomCarTextureBehaviour: a null sprite"));
                    sprites.push((s, renderer_bounds(&Sprite::read(db.data(s))?, r)));
                }
                textures.push(sprites.try_into().unwrap());
            }
            self.random_car_textures.push(super::car::RandomCarTexture {
                id: inst.objects[o].components[c],
                chosenIndex: field(v, "chosenIndex")?.i64().ok_or("chosenIndex")? as i32,
                slots: slots.try_into().unwrap(),
                textures,
                OnAfterChangePresentation: presentation_event(prefab, file, field(v, "OnAfterChange")?),
            });
        }
        let gforces = (0..n).filter(|&o| script(prefab, o, "GForceMeasureBehaviour").is_some()).count();
        if gforces != 0 && gforces != l.arenas.limb.len() {
            unimplemented!("a prefab with GForceMeasureBehaviour on some limbs only");
        }
        for o in 0..n {
            let Some((_, _, v)) = script(prefab, o, "GForceMeasureBehaviour") else { continue };
            let mut gf = GForce::default();
            load_values(gf.fields(), v)?;
            self.gforces.push(gf);
        }

        if let Some((c, _, v)) = script(prefab, 0, "PersonBehaviour") {
            let mut p = Person { name: prefab.objects[0].name.clone(), id: inst.objects[0].components[c], ..Default::default() };
            load_values(p.fields(), v)?;
            p.Limbs = l.limbs(v, "Limbs")?;
            p.BoneBreakClips = array(v, "BoneBreakClips")?.len();
            p.DismembermentClips = array(v, "DismembermentClips")?.len();
            if let Some(effect) = field(v, "PoolableImpactEffect")?.pptr().filter(|p| !p.is_null()) {
                p.PoolableImpactEffect = Some(Some(db.resolve(file, effect).ok_or("PoolableImpactEffect outside the data")?));
            }
            // CirculationBehaviour copies the BleedingParticle prefab's DripSounds (a serialized array, never null).
            if let Some(bp) = field(v, "BleedingParticlePrefab")?.pptr().filter(|p| !p.is_null()) {
                let go = db.resolve(file, bp).ok_or("BleedingParticlePrefab outside the data")?;
                let mut drips = None;
                for c in db.game_object(go)?.components {
                    let c = db.resolve(go.file, c).ok_or("a BleedingParticlePrefab component outside the data")?;
                    if db.class(c) != 114 {
                        continue;
                    }
                    if let Some((script, _, bv)) = db.mono(c)? {
                        if script.class_name == "BleedingParticleBehaviour" {
                            drips = Some(array(&bv, "DripSounds")?.len());
                        }
                    }
                }
                p.BleedingDripSounds = Some(drips.ok_or("BleedingParticlePrefab without BleedingParticleBehaviour")?);
            }
            let s = inst.objects[0].trs.s;
            p.localScale = [s[0], s[1], s[2]];
            for pose in array(v, "Poses")? {
                let mut angles = Vec::new();
                for a in array(pose, "Angles")? {
                    let animated = field(a, "Animated")?.bool().ok_or("Animated")?;
                    angles.push(LimbPose {
                        Limb: l.limb(field(a, "Limb")?)?.ok_or("a LimbPose without its limb")?,
                        Animated: animated,
                        PoseRigidityModifier: float(a, "PoseRigidityModifier")?,
                        Angle: float(a, "Angle")?,
                        StartAngle: float(a, "StartAngle")?,
                        EndAngle: float(a, "EndAngle")?,
                        AnimationDuration: float(a, "AnimationDuration")?,
                        RandomInfluence: float(a, "RandomInfluence")?,
                        RandomSpeed: float(a, "RandomSpeed")?,
                        TimeOffset: float(a, "TimeOffset")?,
                        AnimationCurve: Some(curve(field(a, "AnimationCurve")?)?),
                        cache: CurveCache::default(),
                    });
                }
                p.Poses.push(RagdollPose {
                    Name: string(pose, "Name")?,
                    Rigidity: float(pose, "Rigidity")?,
                    ForceMultiplier: float(pose, "ForceMultiplier")?,
                    ShouldStandUpright: field(pose, "ShouldStandUpright")?.bool().ok_or("ShouldStandUpright")?,
                    DragInfluence: float(pose, "DragInfluence")?,
                    UprightForceMultiplier: float(pose, "UprightForceMultiplier")?,
                    ShouldStumble: field(pose, "ShouldStumble")?.bool().ok_or("ShouldStumble")?,
                    State: field(pose, "State")?.i64().ok_or("State")? as i32,
                    AnimationSpeedMultiplier: float(pose, "AnimationSpeedMultiplier")?,
                    Angles: angles,
                });
            }
            self.people.push(p);
        }
        Ok(Linked { person, limbs: l.arenas.limb, nodes: l.arenas.node, physicals: l.arenas.physical, hinges: l.arenas.hinge, map_lights, waters, machinery, destroyables, fire_maps, firearms, launchers, bells, machine_guns, explosives, delete_afters, point_to_velocities, cars, act_on_collides, tyres, flasks, truck_doors, liquid_containers, glowtubes, leds, detectors, sticky_grenades, generators, floodlights, spike_grenades, heating_elements, ray_guns, lavas, spawners, heat_rects, random_sprites, node_rigidbodies, parent_audio: None })
    }

    /// Object.Instantiate of a Person prefab up to the end of the Instantiate call: the native clone and its
    /// AwakeFromLoad queues, the scripts' deserialization (`link_instance`), then queue 0x14's Awake calls.
    /// The Starts are queued: Unity runs them in the spawn frame, PersonBehaviour's before its LateUpdate and the
    /// rest after the LateUpdate phase (`update_scripts`). The b2 bodies see PersonBehaviour.Awake's rescale at the
    /// next SyncTransforms.
    /// `parent_audio` is the PlayAudioAtRandom of the parent the clone goes under (`ambient_audio`).
    pub fn spawn(&mut self, db: &mut AssetDb, prefab: &Prefab, position: [f32; 3], rotation: [f32; 4], parent_audio: Option<usize>) -> Result<(Instance, Linked), String> {
        let original_ids = db.resource_load_order()?;
        let (mut inst, behaviours) = self.instantiate_native(prefab, position, rotation, &original_ids);
        let first = self.physicals.len();
        let mut linked = self.link_instance(db, prefab, &inst)?;
        linked.parent_audio = parent_audio;
        // GetHashCode (Random.InitState in PhysicalBehaviour.Start) is Mono's object hash, (address >> 3) *
        // 0x9e3779b1 (mono_object_hash, 180116520), of a managed heap address that differs per run in the game too.
        // A PhysicalBehaviour is a Boehm large object, 2048-aligned (every recorded hash has a zero low byte).
        for p in first..self.physicals.len() {
            let address = std::hash::BuildHasher::hash_one(&std::collections::hash_map::RandomState::new(), p) << 11;
            self.physicals[p].hash_code = ((address >> 3) as u32).wrapping_mul(0x9e37_79b1) as i32;
        }
        self.awake_behaviours(prefab, &mut inst, &linked, &behaviours);
        self.queue_starts(db, prefab, &inst, &linked, &behaviours)?;
        Ok((inst, linked))
    }

    /// ColorWave.OnEnable: oV = Random.value, sV = Random.value.
    pub(super) fn color_wave_on_enable(&mut self) {
        self.random.value();
        self.random.value();
    }

    /// The linked objects' Transforms after a script changed the clone's.
    /// DetachFromParentOnStart.Start on the clone object holding component `id`: with a parent,
    /// DestroyIfParentDestroyed adds DestroyThis to the parent's ActOnDestroy (GetOrAddComponent), then
    /// Transform.SetParent(null).
    pub(super) fn detach_from_parent_on_start(&mut self, id: i32, destroy_with_parent: bool) {
        self.set_parent_null_of(id, |g, inst, parent| {
            if destroy_with_parent {
                let key = inst.objects[parent].components[0];
                if !g.act_on_destroy.contains_key(&key) {
                    g.allocate_instance_id();
                }
                g.act_on_destroy.entry(key).or_default().listeners.push(super::destroyable::Listener::DestroyThis(id));
            }
        });
    }

    /// A persistent call `event_calls` kept.
    pub(super) fn event_call(&mut self, call: EventCall) {
        match call {
            EventCall::Car(car, c) => self.car_invoke(car, c),
            // DetachFromParent.Detach: transform.parent = null.
            EventCall::Detach(id) => self.set_parent_null_of(id, |_, _, _| {}),
        }
    }

    /// Transform.SetParent(null) of the clone object holding component `id`, when it has a parent; `before` runs
    /// first with the parent.
    fn set_parent_null_of(&mut self, id: i32, before: impl FnOnce(&mut Game, &mut Instance, usize)) {
        let moved = self.with_component_instance(id, |g, inst, o| {
            let Some(parent) = inst.objects[o].parent else { return false };
            before(g, inst, parent);
            g.set_parent_null(inst, o);
            true
        });
        // Its changed Transforms reach the bodies at the next SyncTransforms.
        if moved && let Some(i) = self.retained.iter().position(|i| i.objects.iter().any(|x| x.components.contains(&id))) {
            let inst = self.retained.remove(i);
            self.unsynced.push(inst);
        }
    }

    /// NoChildrenCollide.Start on the clone object holding component `id`: RequestIgnoreCollision for every pair of
    /// GetComponentsInChildren<Collider2D>() (active objects, depth first, component order) on different Transforms.
    pub(super) fn no_children_collide_start(&mut self, id: i32) {
        let colliders: Vec<(i32, usize)> = self.with_component_instance(id, |_, inst, o| colliders_in_children(inst, o));
        for &(a, ta) in &colliders {
            for &(b, tb) in &colliders {
                if a != b && ta != tb {
                    self.ignore_collision_substitute(a, b, true);
                }
            }
        }
    }

    /// Transform.SetParent(null) on clone object `o` (Instance::set_parent_null), and the TransformHierarchyChange
    /// systems for it and its children: PhysicsManager2D::HandleRigidbodyParentHierarchyChanges (180bf0f20) runs
    /// CheckForDrivenByParentRigidbody, ResetRigidbodyUpdates (back into the scene's depth lists, which sets the
    /// rigidbody-hierarchy-changed flag) and clears the movement state's interpolating byte (+0x114) of each
    /// Rigidbody2D; Collider2D::ParentChanged (180c124e0) re-attaches a collider whose FindRigidbody changed.
    fn set_parent_null(&mut self, inst: &mut Instance, o: usize) {
        let prefab = inst.prefab.clone();
        let subtree: Vec<usize> = std::iter::once(o).chain(inst.descendants(o)).collect();
        // The clone's Transforms are the pose only until the bodies first move.
        if let Some(&b) = inst.bodies.get(&o) {
            let (pos, xf) = (inst.position(o), self.world.body(b).xf.p);
            if pos[0] != xf.x || pos[1] != xf.y {
                unimplemented!("{}: Transform.SetParent after its body moved (the clone's Transforms are stale)", prefab.objects[o].name);
            }
        }
        let bodies: Vec<_> = subtree.iter().filter_map(|x| inst.bodies.get(x).copied()).collect();
        for (&(x, _), fixtures) in &inst.fixtures {
            if subtree.contains(&x) && fixtures.first().is_some_and(|&f| !bodies.contains(&self.world.fixture(f).body)) {
                unimplemented!("{}: SetParent(null) moves a Collider2D off its Rigidbody2D (Collider2D::ParentChanged)", prefab.objects[x].name);
            }
        }
        inst.set_parent_null(o);
        let transform = inst.objects[o].components[0];
        let physical = self.physicals.iter().position(|p| inst.objects[o].components.contains(&p.id));
        let node = self.transform_nodes.iter().position(|n| n.id == transform);
        if let Some(p) = physical {
            self.physicals[p].parent = None;
            self.physicals[p].transform = pose(inst, o);
        }
        if let Some(n) = node {
            self.transform_nodes[n].parent = None;
            self.transform_nodes[n].trs = inst.objects[o].trs;
        }
        if physical.is_none() && node.is_none() && subtree.iter().any(|&x| prefab.objects[x].components.iter().any(|(_, c)| matches!(c, Component::Collider2D(_) | Component::SpriteRenderer(_)))) {
            unimplemented!("{}: SetParent(null) of a bare Transform above colliders or sprites", prefab.objects[o].name);
        }
        for &x in &subtree {
            let Some(k) = prefab.objects[x].components.iter().position(|(_, c)| matches!(c, Component::Rigidbody2D(_))) else { continue };
            if !inst.bodies.contains_key(&x) {
                continue;
            }
            let id = inst.objects[x].components[k];
            let (rb, driven_by) = match self.physicals.iter().position(|p| p.rigidbody.component == id) {
                Some(p) => {
                    let ph = &mut self.physicals[p];
                    (&mut ph.rigidbody, ph.driven_by)
                }
                None => {
                    let n = self.node_rigidbodies.iter().position(|n| n.rigidbody.component == id).expect("a Rigidbody2D outside the arenas");
                    let nb = &mut self.node_rigidbodies[n];
                    (&mut nb.rigidbody, nb.driven_by)
                }
            };
            if driven_by.is_some_and(|b| !bodies.contains(&b)) {
                unimplemented!("{}: SetParent(null) of a Rigidbody2D driven by a body above it", prefab.objects[x].name);
            }
            rb.movement.interpolating = false;
            self.rigidbody_hierarchy_changed = true;
        }
    }

    pub(super) fn refresh_transforms(&mut self, inst: &Instance, linked: &Linked) {
        for (&o, &n) in &linked.nodes {
            self.transform_nodes[n].trs = inst.objects[o].trs;
        }
        for (&o, &p) in &linked.physicals {
            self.physicals[p].transform = pose(inst, o);
        }
        if let Some(person) = linked.person {
            let s = inst.objects[0].trs.s;
            self.people[person].localScale = [s[0], s[1], s[2]];
        }
    }

    /// CatalogBehaviour.Update (no pause menu, dialog or UI block; no contraption is ever selected): releasing
    /// "spawnRight" spawns SelectedItem facing right, else releasing "spawnLeft" spawns it flipped.
    pub fn catalog_update(&mut self) {
        let Some(item) = self.catalog_selected.clone() else { return };
        let up = |k: &str| self.input.controls.get(k).is_some_and(|c| c.up);
        let flipped = match (up("spawnRight"), up("spawnLeft")) {
            (true, _) => false,
            (false, true) => true,
            _ => return,
        };
        let mut db = self.assets.take().expect("CatalogBehaviour.Update without the game's data");
        let spawned = self.catalog_spawn(&mut db, &item, self.MousePosition, flipped);
        self.assets = Some(db);
        match spawned {
            Ok((inst, _)) => self.unsynced.push(inst),
            Err(e) => panic!("CatalogBehaviour.Spawn({item}): {e}"),
        }
    }

    /// CatalogBehaviour.Spawn(SelectedItem, flipped) at Global.MousePosition: PerformInstantiation (no
    /// InstantiateOverride without mods), then AddComponent DeregisterBehaviour, TexturePackApplier,
    /// AudioSourceTimeScaleBehaviour and SerialiseInstructions, and the asset's name. SpawnedGameObjects, the undo
    /// action and the stats are not simulation state; DeregisterBehaviour and TexturePackApplier have no messages
    /// and AudioSourceTimeScaleBehaviour.Start only registers audio.
    pub fn catalog_spawn(&mut self, db: &mut AssetDb, asset: &str, position: [f32; 3], flipped: bool) -> Result<(Instance, Linked), String> {
        let catalog = *db.find_scripts("CatalogData")?.first().ok_or("no CatalogData")?;
        let (_, _, items) = db.mono(catalog)?.ok_or("CatalogData")?;
        let mut prefab = None;
        for item in items.get("Items").and_then(Value::array).ok_or("CatalogData.Items")?.to_vec() {
            let r = db.resolve(catalog.file, item.pptr().ok_or("CatalogData.Items")?).ok_or("a SpawnableAsset outside the data")?;
            let (_, mb, v) = db.mono(r)?.ok_or("SpawnableAsset")?;
            if mb.name == asset {
                prefab = Some(db.resolve(r.file, v.get("Prefab").and_then(Value::pptr).ok_or("SpawnableAsset.Prefab")?).ok_or("SpawnableAsset.Prefab")?);
                break;
            }
        }
        let prefab = db.prefab(prefab.ok_or_else(|| format!("no catalog item {asset:?}"))?)?;
        let (mut inst, linked) = self.spawn(db, &prefab, position, [0.0, 0.0, 0.0, 1.0], None)?;
        if flipped {
            let s = inst.objects[0].trs.s;
            inst.set_local_scale(0, [s[0] * -1.0, s[1], s[2]]);
            self.refresh_transforms(&inst, &linked);
        }
        self.spawnable_asset_components(&prefab, &linked, asset, 4);
        Ok((inst, linked))
    }

    /// The `added` components a SpawnableAsset clone gets (AudioSourceTimeScaleBehaviour, TexturePackApplier and
    /// SerialiseInstructions among them) and its gameObject.name.
    pub(super) fn spawnable_asset_components(&mut self, prefab: &Prefab, linked: &Linked, asset: &str, added: usize) {
        for _ in 0..added {
            self.allocate_instance_id();
        }
        // The root's AudioSourceTimeScaleBehaviour, which PhysicalBehaviour.Start's GetComponentInParent finds.
        match (linked.physicals.get(&0), linked.nodes.get(&0)) {
            (Some(&p), _) => self.physicals[p].audio_time_scale = true,
            (None, Some(&n)) => self.audio_time_scale_roots.push(n),
            (None, None) => unimplemented!("a catalog item whose root is neither physical nor a parent of physicals"),
        }
        // gameObject.name = e.name
        if let Some(&p) = linked.physicals.get(&0) {
            self.physicals[p].name = asset.to_string();
        }
        if let Some(p) = linked.person {
            self.people[p].name = asset.to_string();
        }
        // SerialiseInstructions.Start: a SerialisableIdentity on the root and on every Transform below without an
        // Optout, unless one is there (what Starts add first, the outlines and particles, all carry Optout).
        let has = |o: &PrefabObject, class: &str| o.components.iter().any(|(_, c)| matches!(c, Component::Script { class: k, .. } if k == class));
        let identities = prefab.objects.iter().enumerate().filter(|&(o, ob)| !has(ob, "SerialisableIdentity") && (o == 0 || !has(ob, "Optout"))).count();
        self.pending_starts.push(super::misc::PendingStart::SerialiseInstructions(identities));
    }

    /// Awake of each queue 0x14 behaviour, in `instantiate_native`'s order, skipping objects inactive by then.
    /// The classes left out have no Awake that touches simulation state (ConnectedNodeBehaviour, GripBehaviour
    /// and GoreStringBehaviour only read components; MapConfig, MapLightBehaviour and WindControllerBehaviour
    /// only fetch components), and none of these scripts has OnEnable.
    pub fn awake_behaviours(&mut self, prefab: &Prefab, inst: &mut Instance, linked: &Linked, order: &[(usize, usize)]) {
        // The behaviours whose OnEnable has run (awake and enabled), for OnDisable.
        let mut enabled_behaviours = std::collections::HashSet::new();
        for &(o, c) in order {
            if !inst.objects[o].active {
                continue;
            }
            let Component::Script { class, enabled, execution_order, fields: v, .. } = &prefab.objects[o].components[c].1 else { unreachable!() };
            check_rewritten_at(prefab, o, class);
            if *enabled {
                enabled_behaviours.insert((o, c));
            }
            let limb = || linked.limbs[&o];
            match class.as_str() {
                // SetActive(ShowTutorial); a tutorial left active clears the preference.
                "ActivateIfTutorial" if self.prefs.ShowTutorial => self.prefs.ShowTutorial = false,
                // SetActive(Chance > Random.value && FancyEffects): the draw happens whatever the preference.
                "ActivateIfFancyEffects" if { let chance = float(v, "Chance").unwrap(); chance > self.random.value() && self.prefs.FancyEffects } => {}
                "ActivateIfTutorial" | "ActivateIfFancyEffects" => {
                    for (d, k) in self.set_active(inst, o, false) {
                        if !enabled_behaviours.remove(&(d, k)) {
                            continue;
                        }
                        let Component::Script { class, .. } = &prefab.objects[d].components[k].1 else { unreachable!() };
                        match class.as_str() {
                            "PhysicalBehaviour" => self.physical_on_disable(linked.physicals[&d]),
                            "LEDBulbBehaviour" => self.led_on_disable(linked.leds[&d]),
                            "DetectorBehaviour" => self.detector_on_disable(linked.detectors[&d]),
                            "GeneratorBehaviour" => self.generator_on_disable(linked.generators[&d]),
                            "SingleFloodlightBehaviour" => self.floodlight_on_disable(linked.floodlights[&d], inst),
                            "SpikeGrenadeBehaviour" => self.spike_grenade_on_disable(linked.spike_grenades[&d]),
                            "TemperatureRayGunBehaviour" => self.ray_gun_on_disable(linked.ray_guns[&d]),
                            // No OnDisable; TextMeshPro's only touches its mesh and the font atlas.
                            "ActivateIfTutorial" | "ActivateIfFancyEffects" | "AudioSourceTimeScaleBehaviour" | "DetachFromParentOnStart" | "DebrisComponent" | "Optout" | "MaintainScaleSignBehaviour" | "TextMeshProControlReplacer" | "TextMeshPro" => {}
                            c if super::lifecycle::LIFECYCLE.iter().any(|(k, m)| *k == c && !m.contains(&"OnDisable")) => {}
                            _ => unimplemented!("{class}.OnDisable"),
                        }
                    }
                    for (&d, &p) in &linked.physicals {
                        self.physicals[p].active = inst.objects[d].active;
                    }
                    for (&d, &w) in &linked.waters {
                        self.waters[w].active = inst.objects[d].active;
                    }
                }
                "LiquidContainerController" => self.liquid_container_awake(linked.liquid_containers[&o]),
                // No Awake; its OnEnable follows here.
                "ColorWave" if *enabled => self.color_wave_on_enable(),
                "DetectorBehaviour" => self.detector_awake(linked.detectors[&o]),
                "PersonBehaviour" => {
                    if let Some(s) = self.person_awake(linked.person.unwrap()) {
                        inst.set_local_scale(0, s);
                        self.refresh_transforms(&inst, linked);
                    }
                }
                "SkinMaterialHandler" => self.skin_awake(limb()),
                "PhysicalBehaviour" => self.physical_awake(linked.physicals[&o]),
                "WaterBehaviour" => self.water_awake(linked.waters[&o]),
                "LimbBehaviour" => self.limb_awake(limb()),
                "CirculationBehaviour" => self.circulation_awake(limb()),
                "DamagableMachineryBehaviour" => self.machinery_awake(linked.machinery[&o]),
                "FirearmBehaviour" => self.firearm_awake(linked.firearms[&o]),
                // transform.localScale = Vector2.one * Random.Range(Range.x, Range.y) (a Vector2: z becomes 0).
                "RandomSpawnedSize" => {
                    let range: [f32; 2] = floats(field(v, "Range").unwrap()).unwrap();
                    let s = self.random.range(range[0], range[1]);
                    inst.set_local_scale(o, [s, s, 0.0]);
                    self.refresh_transforms(inst, linked);
                }
                "LocalFirePropagationMap" if self.fire_maps[linked.fire_maps[&o]].Points.is_empty() => {
                    unimplemented!("LocalFirePropagationMap.CalculatePoints (Awake without serialized Points)")
                }
                "DestroyWhenAllChildrenDestroyed" => {
                    let Component::Script { fields: v, .. } = &prefab.objects[o].components[c].1 else { unreachable!() };
                    let file = prefab.objects[o].game_object.file;
                    let exclude: Vec<usize> = v.get("Exclude").and_then(Value::array).expect("Exclude").iter().filter_map(|g| {
                        let p = g.pptr().filter(|p| !p.is_null() && p.file == 0)?;
                        prefab.index.get(&ObjRef { file, path_id: p.path_id }).map(|&(x, _)| x)
                    }).collect();
                    let children: Vec<(i32, bool)> = std::iter::once(o).chain(inst.descendants(o)).filter(|&d| inst.objects[d].active).map(|d| (inst.objects[d].components[0], exclude.contains(&d))).collect();
                    self.dwacd_awake(inst.objects[o].components[c], &children);
                }
                // AudioClip.Create("radioClip", ...) takes an instance id; the stream callback and Source.clip are audio.
                "FuseBoxBehaviour" => {
                    self.allocate_instance_id();
                }
                "RandomSpriteBehaviour" => self.random_sprite_awake(linked.random_sprites[&o], *enabled),
                "CarBehaviour" => self.car_awake(linked.cars[&o], inst, o),
                "TyreDeflationBehaviour" => self.tyre_awake(linked.tyres[&o], inst),
                // Start: Object.Destroy(Joint) for a Broken one, which only a loaded save sets.
                "JointSerialisationHelper" | "BatchJointSerialisationHelper" => {
                    let Component::Script { fields: v, .. } = &prefab.objects[o].components[c].1 else { unreachable!() };
                    let file = prefab.objects[o].game_object.file;
                    let key = |p: PPtr| {
                        if p.is_null() {
                            return None;
                        }
                        assert_eq!(p.file, 0, "{class}: a joint outside its prefab");
                        let k = prefab.index[&ObjRef { file, path_id: p.path_id }];
                        (k.0 == o).then(|| self.break_joint_key(inst, k)).flatten()
                    };
                    let joints: Vec<Option<BreakJoint>> = match class.as_str() {
                        "JointSerialisationHelper" => {
                            assert!(!v.get("Broken").and_then(Value::bool).unwrap(), "JointSerialisationHelper.Start: Object.Destroy(Joint) of a broken joint");
                            vec![key(v.get("Joint").and_then(Value::pptr).unwrap())]
                        }
                        _ => v.get("Joints").and_then(Value::array).unwrap().iter().map(|j| key(j.pptr().unwrap())).collect(),
                    };
                    self.joint_helpers.push(super::car::JointHelper { Broken: vec![false; joints.len()], joints });
                }
                "NoCollideGm" => self.no_collide_gm_awake(inst.objects[o].components[c], inst),
                "RandomCarTextureBehaviour" => {
                    let t = self.random_car_textures.iter().position(|x| x.id == inst.objects[o].components[c]).unwrap();
                    self.random_car_texture_awake(t, *enabled);
                }
                // Registered with its object: OnJointBreak2D for the joints on its own GameObject (SendMessage reaches no
                // other) and the break limit key each sits under.
                "JointBreakSoundBehaviour" => {
                    let Component::Script { fields: v, .. } = &prefab.objects[o].components[c].1 else { unreachable!() };
                    let file = prefab.objects[o].game_object.file;
                    let local = |p: PPtr| {
                        assert_eq!(p.file, 0, "JointBreakSoundBehaviour: a reference outside its prefab");
                        (!p.is_null()).then(|| prefab.index[&ObjRef { file, path_id: p.path_id }])
                    };
                    let joints = v.get("Joints").and_then(Value::array).unwrap().iter().filter_map(|j| local(j.pptr().unwrap())).filter(|&(jo, _)| jo == o);
                    let joints = joints.filter_map(|k| self.break_joint_key(inst, k)).collect();
                    // OnBreak: ExplosiveBehaviour.ForceImmediateExplosion, audio and particles.
                    let mut explosives = Vec::new();
                    for call in invoked_calls(v.get("OnBreak").unwrap()) {
                        match call_target(prefab, file, call) {
                            Some((to, tc)) if matches!(&prefab.objects[to].components[tc].1, Component::Script { class, .. } if class == "ExplosiveBehaviour") && string(call, "m_MethodName").unwrap() == "ForceImmediateExplosion" => {
                                explosives.push(linked.explosives[&to]);
                            }
                            _ if presentation_call(prefab, file, call) => {}
                            Some((to, tc)) if matches!(&prefab.objects[to].components[tc].1, Component::Script { class, .. } if class == "CarBehaviour") => {}
                            _ => unimplemented!("JointBreakSoundBehaviour.OnBreak: a persistent call beyond ExplosiveBehaviour.ForceImmediateExplosion, CarBehaviour, audio and particles"),
                        }
                    }
                    let cars = car_calls(prefab, file, v.get("OnBreak").unwrap(), inst);
                    self.joint_break_sounds.push(super::misc::JointBreakSound {
                        joints,
                        explosives,
                        cars,
                        enabled: *enabled,
                        clips: v.get("Clips").and_then(Value::array).map_or(0, <[_]>::len),
                    });
                }
                _ => {}
            }
            // Behaviour::AddToManager after Awake, for an enabled one.
            let behaviour = match class.as_str() {
                "PersonBehaviour" => Some(Behaviour::Person(linked.person.unwrap())),
                "CirculationBehaviour" => Some(Behaviour::Circulation(limb())),
                "SkinMaterialHandler" => Some(Behaviour::Skin(limb())),
                "GripBehaviour" => Some(Behaviour::Grip(self.limbs[limb()].GripBehaviour.unwrap())),
                "GoreStringBehaviour" => Some(Behaviour::GoreString(limb())),
                "GForceMeasureBehaviour" => Some(Behaviour::GForce(limb())),
                "DamagableMachineryBehaviour" => Some(Behaviour::Machinery(linked.machinery[&o])),
                "LavaBehaviour" => Some(Behaviour::Lava(linked.lavas[&o])),
                "RandomPrefabSpawnerBehaviour" => Some(Behaviour::Spawner(linked.spawners[&o])),
                "TemperatureTargetRectangleBehaviour" => Some(Behaviour::HeatRect(linked.heat_rects[&o])),
                "DestroyableBehaviour" => Some(Behaviour::Destroyable(linked.destroyables[&o])),
                "LocalFirePropagationMap" => Some(Behaviour::FireMap(linked.fire_maps[&o])),
                "FirearmBehaviour" => Some(Behaviour::Firearm(linked.firearms[&o])),
                "ProjectileLauncherBehaviour" => Some(Behaviour::Launcher(linked.launchers[&o])),
                "BellBehaviour" => Some(Behaviour::Bell(linked.bells[&o])),
                "MachineGunBehaviour" => Some(Behaviour::MachineGun(linked.machine_guns[&o])),
                "ExplosiveBehaviour" => Some(Behaviour::Explosive(linked.explosives[&o])),
                "DeleteAfterTime" => Some(Behaviour::DeleteAfter(linked.delete_afters[&o])),
                "PointToVelocityBehaviour" => Some(Behaviour::PointToVelocity(linked.point_to_velocities[&o])),
                "CarBehaviour" => Some(Behaviour::Car(linked.cars[&o])),
                "TyreDeflationBehaviour" => Some(Behaviour::Tyre(linked.tyres[&o])),
                "GasTankBehaviour" | "FlaskBehaviour" => Some(Behaviour::Flask(linked.flasks[&o])),
                "LiquidContainerController" => Some(Behaviour::LiquidContainer(linked.liquid_containers[&o])),
                "TruckDoorBehaviour" => Some(Behaviour::TruckDoor(linked.truck_doors[&o])),
                "GlowtubeBehaviour" => Some(Behaviour::Glowtube(linked.glowtubes[&o])),
                "LEDBulbBehaviour" => Some(Behaviour::Led(linked.leds[&o])),
                "DetectorBehaviour" => Some(Behaviour::Detector(linked.detectors[&o])),
                "StickyGrenadeBehaviour" => Some(Behaviour::Sticky(linked.sticky_grenades[&o])),
                "GeneratorBehaviour" => Some(Behaviour::Generator(linked.generators[&o])),
                "SingleFloodlightBehaviour" => Some(Behaviour::Floodlight(linked.floodlights[&o])),
                "SpikeGrenadeBehaviour" => Some(Behaviour::SpikeGrenade(linked.spike_grenades[&o])),
                "HeatingElementBehaviour" => Some(Behaviour::HeatingElement(linked.heating_elements[&o])),
                "TemperatureRayGunBehaviour" => Some(Behaviour::RayGun(linked.ray_guns[&o])),
                _ => None,
            };
            if let Some(b) = behaviour {
                self.behaviour_of.insert(inst.objects[o].components[c], (*execution_order, b));
                if *enabled {
                    self.behaviours.add(*execution_order, b);
                }
            }
        }
    }

    /// The m_JointBreakLimits key of a created prefab joint (`Game::hinges` took a linked hinge's slot over).
    fn break_joint_key(&self, inst: &Instance, k: (usize, usize)) -> Option<BreakJoint> {
        let slot = inst.hinges.get(&k).and_then(|s| s.slot).or_else(|| inst.anchored.get(&k).and_then(|s| s.slot))?;
        Some(match self.anchored_joints[slot].hinge {
            Some(h) => BreakJoint::Hinge(h),
            None => BreakJoint::Anchored(slot),
        })
    }

    /// `host.GetComponentsInChildren<HingeJoint2D>()`, `<SliderJoint2D>()` and `<FixedJoint2D>()`: the host and its active
    /// descendants, depth first, each object's components in order (disabled components included).
    fn autofix_components(&self, prefab: &Prefab, inst: &Instance, linked: &Linked, host: usize) -> AutofixJoints {
        let mut out = AutofixJoints::default();
        let physical = |o: usize| match linked.physicals.get(&o) {
            Some(&p) => BodyOwner::Physical(p),
            None => BodyOwner::Node(linked.node_rigidbodies[&o]),
        };
        let mut stack = vec![host];
        while let Some(o) = stack.pop() {
            if !inst.objects[o].active {
                continue;
            }
            stack.extend(prefab.objects[o].children.iter().rev());
            for (c, (_, comp)) in prefab.objects[o].components.iter().enumerate() {
                let Component::Joint2D(j) = comp else { continue };
                let list = match j.kind {
                    JointKind::Hinge { .. } => {
                        out.hinges.push(linked.hinges[&(o, c)]);
                        continue;
                    }
                    JointKind::Slider { .. } => &mut out.sliders,
                    JointKind::Fixed { .. } => &mut out.fixeds,
                    _ => continue,
                };
                let state = inst.anchored.get(&(o, c)).cloned().unwrap_or_else(|| anchored_initial_state(j));
                list.push(AutofixJoint {
                    object: o,
                    component: c,
                    state: state.base,
                    slider: state.slider,
                    joint: inst.joints.get(&(o, c)).copied(),
                    slot: state.slot,
                    attached: physical(o),
                    connected: joint_connected_object(prefab, o, c, j).map(physical),
                    enabled: j.enabled,
                });
            }
        }
        out
    }

    /// Queues the Start of each behaviour in `order` (`run_pending_starts`, or PersonBehaviour's before its first
    /// LateUpdate). Each PhysicalBehaviour's `hash_code` must be set by then: Random.InitState(GetHashCode()) takes
    /// Mono's object hash. SkinMaterialHandler, DisintegrationCounterBehaviour and IncreaseStatOnStart's Starts only
    /// touch rendering, event subscriptions and the stat manager.
    pub fn queue_starts(&mut self, db: &mut AssetDb, prefab: &Prefab, inst: &Instance, linked: &Linked, order: &[(usize, usize)]) -> Result<(), String> {
        use super::misc::PendingStart;
        for &(o, c) in order {
            let Component::Script { class, fields: v, enabled, .. } = &prefab.objects[o].components[c].1 else { unreachable!() };
            if !inst.objects[o].active || !enabled {
                continue;
            }
            let limb = || linked.limbs[&o];
            let flag = |name: &str| field(v, name).and_then(|x| x.bool().ok_or_else(|| name.to_string())).unwrap();
            let start = match class.as_str() {
                "PersonBehaviour" => PendingStart::Person(linked.person.unwrap()),
                "HingeJointLimitAutofixBehaviour" => match linked.person {
                    Some(p) => PendingStart::HingeJointLimitAutofix(p),
                    None => {
                        let joints = self.autofix_components(prefab, inst, linked, o);
                        // Without a limited hinge or slider, or a FixedJoint2D, below it neither a flip nor Start touches anything.
                        if !joints.hinges.iter().any(|&h| self.hinges[h].useLimits) && !joints.sliders.iter().any(|j| j.slider.as_ref().unwrap().use_limits) && joints.fixeds.is_empty() {
                            continue;
                        }
                        let host = linked.physicals.get(&o).copied().ok_or_else(|| {
                            *linked.nodes.get(&o).unwrap_or_else(|| unimplemented!("HingeJointLimitAutofixBehaviour on an object that is neither physical nor a Transform node"))
                        });
                        self.autofix_joints.push(joints);
                        PendingStart::HingeJointLimitAutofixItem(host, self.autofix_joints.len() - 1)
                    }
                },
                "CirculationBehaviour" => PendingStart::Circulation(limb()),
                "PhysicalBehaviour" => PendingStart::Physical(linked.physicals[&o]),
                "LimbBehaviour" => PendingStart::Limb(limb()),
                "ConnectedNodeBehaviour" => PendingStart::Node(limb(), v.get("SerialisableConnectedNodes").and_then(Value::array).map(|a| a.iter().map(|x| x.bool().unwrap()).collect())),
                "GripBehaviour" => PendingStart::Grip(self.limbs[limb()].GripBehaviour.unwrap()),
                "GoreStringBehaviour" => PendingStart::GoreString(limb(), flag("CreateJointOnStart")),
                "ShatteredObjectGenerator" if flag("GenerateOnStart") => unimplemented!("ShatteredObjectGenerator.Start with GenerateOnStart"),
                "MapLightBehaviour" => PendingStart::MapLight(linked.map_lights[&o]),
                "WaterBehaviour" => PendingStart::Water(linked.waters[&o]),
                "DamagableMachineryBehaviour" => PendingStart::Machinery(linked.machinery[&o]),
                "DestroyableBehaviour" => PendingStart::Destroyable(linked.destroyables[&o]),
                "LocalFirePropagationMap" => PendingStart::FireMap(linked.fire_maps[&o]),
                "NoChildrenCollide" => PendingStart::NoChildrenCollide(inst.objects[o].components[c]),
                "DetachFromParentOnStart" => PendingStart::DetachFromParent(inst.objects[o].components[c], flag("DestroyIfParentDestroyed")),
                "InstantiateObjectBehaviour" if !flag("InstantiateOnStart") => continue,
                "InstantiateObjectBehaviour" => PendingStart::InstantiateObject(inst.objects[o].components[c]),
                "RandomForceAtStart" => {
                    let k = prefab.objects[o].components.iter().position(|(_, x)| matches!(x, Component::Rigidbody2D(_))).ok_or("RandomForceAtStart without a Rigidbody2D")?;
                    PendingStart::RandomForce(inst.objects[o].components[k], float(v, "PositionalVelocityStrength")?, float(v, "RotationalVelocityStrength")?)
                }
                "DoAtTimeOnce" if !flag("StartSequenceOnStart") => continue,
                "DoAtTimeOnce" => {
                    let id = inst.objects[o].components[c];
                    PendingStart::DoAtTimeOnce(self.do_at_time_onces.iter().position(|d| d.id == id).unwrap())
                }
                // Awake: GetComponent<AudioSource>() when audioSource is null, Global.AddAudioSource (audio).
                "ExplosionSoundBehviour" if !flag("StartWhenAwake") => continue,
                "ExplosionSoundBehviour" => {
                    let set = field(v, "audioSource")?.pptr().is_some_and(|p| !p.is_null());
                    let has = set || prefab.objects[o].components.iter().any(|(_, x)| matches!(x, Component::Other(82)));
                    let near = array(v, "Near")?.len();
                    let distant = field(v, "DistantSound")?.i64().ok_or("DistantSound")?;
                    PendingStart::ExplosionSoundPlay(inst.objects[o].components[c], has.then_some(near), distant, float(v, "Delay")?)
                }
                "ExistByChance" => PendingStart::ExistByChance(inst.objects[o].components[c], float(v, "Chance")?),
                "OnlyExplosionBehaviour" if !flag("ExplodeOnStart") => continue,
                "OnlyExplosionBehaviour" => PendingStart::OnlyExplosion(inst.objects[o].components[c], explosion_parameters(field(v, "ExplosionParameters")?)?),
                "ExplosiveBehaviour" => PendingStart::Explosive(linked.explosives[&o]),
                "NoCollide" => PendingStart::NoCollide(inst.objects[o].components[c]),
                "CarBehaviour" => PendingStart::Car(linked.cars[&o]),
                "TyreDeflationBehaviour" => PendingStart::Tyre(linked.tyres[&o]),
                "FirearmBehaviour" => PendingStart::Firearm(linked.firearms[&o]),
                "MachineGunBehaviour" => PendingStart::MachineGun(linked.machine_guns[&o]),
                "FirearmAttachmentPointBehaviour" => {
                    if field(v, "FirearmAttachment")?.pptr().is_some_and(|p| !p.is_null()) {
                        unimplemented!("FirearmAttachmentPointBehaviour.Start with a FirearmAttachment");
                    }
                    PendingStart::FirearmAttachmentPoint
                }
                "GasTankBehaviour" | "FlaskBehaviour" => PendingStart::Flask(linked.flasks[&o]),
                "DetectorBehaviour" => PendingStart::Detector(linked.detectors[&o]),
                "SingleFloodlightBehaviour" => PendingStart::Floodlight(linked.floodlights[&o]),
                "SpikeGrenadeBehaviour" => PendingStart::SpikeGrenade(linked.spike_grenades[&o]),
                "RandomCarTextureBehaviour" => PendingStart::CarTexture(self.random_car_textures.iter().position(|x| x.id == inst.objects[o].components[c]).unwrap()),
                "DisintegrationCounterBehaviour" if linked.person.is_some() && o == 0 => continue,
                "DisintegrationCounterBehaviour" => PendingStart::DisintegrationCounter(inst.objects[o].components[c]),
                "ExistInDetailView" => PendingStart::ExistInDetailView(inst.objects[o].components[c]),
                "IgnoreParentSize" => PendingStart::IgnoreParentSize(inst.objects[o].components[c]),
                "RandomSpriteBehaviour" => PendingStart::RandomSprite(linked.random_sprites[&o]),
                // CameraControlBehaviour takes the box.
                "BoundingBoxBehaviour" => PendingStart::BoundingBox(map_bounds(v)?),
                "MapConfig" => {
                    let file = prefab.objects[o].game_object.file;
                    let mut weather = Vec::new();
                    for prefab_field in ["SnowPrefab", "RainPrefab", "FogPrefab", "LightningCreatorPrefab"] {
                        let p = field(v, prefab_field)?.pptr().ok_or(prefab_field)?;
                        weather.push(match p.is_null() {
                            true => None,
                            false => Some(std::sync::Arc::new(db.prefab(db.resolve(file, p).ok_or_else(|| format!("{prefab_field} outside the data"))?)?)),
                        });
                    }
                    let lightning = weather.pop().unwrap();
                    // Awake's GetComponent<BoundingBoxBehaviour>().
                    let (_, _, b) = script(prefab, o, "BoundingBoxBehaviour").ok_or("MapConfig without a BoundingBoxBehaviour")?;
                    let ambience = field(v, "Ambience")?;
                    let settings = field(v, "Settings")?;
                    PendingStart::MapConfig(super::map::MapConfigStart {
                        bounds: map_bounds(b)?,
                        audio: linked.parent_audio,
                        ambience: field(v, "OverrideAmbientSounds")?.bool().ok_or("OverrideAmbientSounds")?.then(|| -> Result<_, String> { Ok((float(ambience, "Chance")?, array(ambience, "Clips")?.len())) }).transpose()?,
                        gravity: float(settings, "Gravity")?,
                        ambient_temperature: float(settings, "Ambient_temperature")?,
                        lightning_chance: float(settings, "Lightning_chance")?,
                        weather,
                        lightning,
                        original_ids: std::sync::Arc::new(db.resource_load_order()?),
                    })
                }
                _ => continue,
            };
            self.pending_starts.push(start);
        }
        Ok(())
    }

    /// The PhysicalProperties asset's arena index, loading it on first use (assets are shared by name).
    fn physical_properties(&mut self, db: &mut AssetDb, asset: ObjRef) -> Result<usize, String> {
        let (_, mb, v) = db.mono(asset)?.ok_or("PhysicalProperties is not a MonoBehaviour")?;
        if let Some(i) = self.properties.iter().position(|p| p.name == mb.name) {
            return Ok(i);
        }
        let b = |n: &str| -> Result<bool, String> { field(&v, n)?.bool().ok_or(n.to_string()) };
        let mut p = PhysicalProperties { name: mb.name.clone(), ..Default::default() };
        p.Softness = float(&v, "Softness")?;
        p.Flammability = float(&v, "Flammability")?;
        p.Conducting = b("Conducting")?;
        p.Burnrate = float(&v, "Burnrate")?;
        p.BurningTemperatureThreshold = float(&v, "BurningTemperatureThreshold")?;
        p.HeatTransferSpeedMultiplier = float(&v, "HeatTransferSpeedMultiplier")?;
        p.Buoyancy = float(&v, "Buoyancy")?;
        p.ImpactIntensityMutliplier = float(&v, "ImpactIntensityMutliplier")?;
        p.HitVolumeMultiplier = float(&v, "HitVolumeMultiplier")?;
        p.SparksOnSlide = b("SparksOnSlide")?;
        p.SoftImpact = array(&v, "SoftImpact")?.len();
        p.HardImpact = array(&v, "HardImpact")?.len();
        // Serialized arrays load as arrays, never null.
        p.SizzleSounds = Some(array(&v, "SizzleSounds")?.len());
        p.Sharp = b("Sharp")?;
        p.IgnoreStabResistance = b("IgnoreStabResistance")?;
        p.SharpForceThresholdMultiplier = float(&v, "SharpForceThresholdMultiplier")?;
        for a in array(&v, "SharpAxes")? {
            p.SharpAxes.push(SharpAxis {
                Axis: floats(field(a, "Axis")?)?,
                LowerLimit: float(a, "LowerLimit")?,
                UpperLimit: float(a, "UpperLimit")?,
                LooseUpperLimit: field(a, "LooseUpperLimit")?.bool().ok_or("LooseUpperLimit")?,
                LooseLowerLimit: field(a, "LooseLowerLimit")?.bool().ok_or("LooseLowerLimit")?,
            });
        }
        p.LodgeStrengthMultiplier = float(&v, "LodgeStrengthMultiplier")?;
        p.StabSound = Some(array(&v, "StabSound")?.len());
        p.Brittleness = float(&v, "Brittleness")?;
        p.BulletSpeedAbsorptionPower = float(&v, "BulletSpeedAbsorptionPower")?;
        p.ShotImpact = match db.resolve(asset.file, field(&v, "ShotImpact")?.pptr().ok_or("ShotImpact")?) {
            Some(r) => Some((Some(r), db.game_object(r)?.name == "BloodPuff poolable")),
            None => None,
        };
        match db.resolve(asset.file, field(&v, "PhysicMaterial")?.pptr().ok_or("PhysicMaterial")?) {
            Some(m) => {
                let m = PhysicsMaterial2D::read(db.data(m))?;
                (p.friction, p.bounciness) = (Some(m.friction), m.bounciness);
            }
            None => (p.friction, p.bounciness) = (None, 0.0),
        }
        self.properties.push(p);
        Ok(self.properties.len() - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_assets::HumanRig;

    /// Every dumped field that differs, as "<label> <field>: game <x> ours <y>".
    fn diff(label: &str, mut game: Vec<(&'static str, &mut dyn Field)>, mut ours: Vec<(&'static str, &mut dyn Field)>, out: &mut Vec<String>) {
        for ((name, a), (_, b)) in game.iter_mut().zip(ours.iter_mut()) {
            let (a, b) = (a.show(), b.show());
            if a != b {
                out.push(format!("{label} {name}: game {a} ours {b}"));
            }
        }
    }

    /// The spawned Person's arenas against dump31spawnrand's first snapshot (after Awake and Start). The links
    /// must match, except those Awake/Start set (LimbBehaviour.NearestLimbToBrain, CirculationBehaviour.Source,
    /// PersonBehaviour.Limbs, the hinges' motors, PhysicalBehaviour's collision mode and grid jitter); dumped
    /// fields are printed where they differ. The recording's poses come from the asset cache, which normalizes
    /// unanimated angles, so only what an unanimated angle uses is compared there.
    #[test]
    fn person_links_match_game() {
        let Some((mut g, inst, prefab, mut db, first)) = super::super::spawn::tests::spawn_recorded("dump31spawnrand") else { return };
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let Ok(rig) = HumanRig::load(&root.join("local-cache"), &crate::game_assets::find_game(None).unwrap()) else { return };
        // The map's and pool's splats are not the Person's.
        let mut first = first;
        first.records.retain(|r| r.ty != "FancyBloodSplatController" && r.ty != "BleedingParticleBehaviour");
        let want = Game::from_snapshot(&first, &rig).unwrap();
        let linked = g.link_instance(&mut db, &prefab, &inst).unwrap();
        let person = linked.person.unwrap();
        assert_eq!(g.people[person].BleedingDripSounds, Some(11), "BleedingParticle DripSounds");
        // The queue 0x14 order, as instantiate_native returns it (script execution order, then the higher clone id).
        let mut order: Vec<(usize, usize)> = (0..prefab.objects.len())
            .flat_map(|o| (0..prefab.objects[o].components.len()).map(move |c| (o, c)))
            .filter(|&(o, c)| matches!(prefab.objects[o].components[c].1, Component::Script { .. }))
            .collect();
        order.sort_by_key(|&(o, c)| {
            let Component::Script { execution_order, .. } = &prefab.objects[o].components[c].1 else { unreachable!() };
            (*execution_order, std::cmp::Reverse(inst.objects[o].components[c]))
        });
        let header = super::super::dump::header(&root.join("reference-probe/game/dump31spawnrand.txt")).unwrap();
        g.random = crate::unity::Random { state: super::super::dump::spawn_random_state(&header).unwrap() };
        // spawn_recorded applied the recorded scale; PersonBehaviour.Awake must draw the same one.
        let recorded_scale = inst.objects[0].trs.s;
        let mut inst = inst;
        g.awake_behaviours(&prefab, &mut inst, &linked, &order);
        assert_eq!(g.people[person].localScale.map(f32::to_bits), [recorded_scale[0].to_bits(), recorded_scale[1].to_bits(), recorded_scale[2].to_bits()], "PersonBehaviour.Awake scale");
        // Mono's GetHashCode is a heap address: from the recording.
        for p in 0..g.physicals.len() {
            let id = g.physicals[p].id;
            g.physicals[p].hash_code = want.physicals.iter().find(|a| a.id == id).map_or(0, |a| a.hash_code);
        }
        g.queue_starts(&mut db, &prefab, &inst, &linked, &order).unwrap();
        g.run_pending_starts();
        assert!(g.physicals.iter().all(|p| p.in_world));
        // The first frame's render: OnWillRenderObject fills the blood colour caches.
        for l in 0..g.limbs.len() {
            g.skin_will_render(l);
        }
        let mut out = Vec::new();
        let mut links = Vec::new();
        let mut check = |label: String, a: String, b: String| {
            if a != b {
                // Long values: from just before the first difference.
                let at = a.bytes().zip(b.bytes()).position(|(x, y)| x != y).unwrap_or(a.len().min(b.len())).saturating_sub(80);
                let cut = |s: &str| s.get(at.min(s.len())..).unwrap_or("").chars().take(240).collect::<String>();
                links.push(format!("{label} @{at}: game {} ours {}", cut(&a), cut(&b)));
            }
        };

        let names = |g: &Game, v: &[usize]| v.iter().map(|&l| g.limbs[l].name.clone()).collect::<Vec<_>>().join(",");
        check("limb names".into(), names(&want, &(0..want.limbs.len()).collect::<Vec<_>>()), names(&g, &(0..g.limbs.len()).collect::<Vec<_>>()));
        for l in 0..g.limbs.len().min(want.limbs.len()) {
            let (a, b) = (&want.limbs[l], &g.limbs[l]);
            let n = b.name.clone();
            check(format!("{n} id"), a.id.to_string(), b.id.to_string());
            check(format!("{n} PhysicalBehaviour"), want.physicals[a.PhysicalBehaviour].id.to_string(), g.physicals[b.PhysicalBehaviour].id.to_string());
            check(format!("{n} Joint"), format!("{:?}", a.Joint), format!("{:?}", b.Joint));
            check(format!("{n} NearestLimbToBrain"), format!("{:?}", a.NearestLimbToBrain), format!("{:?}", b.NearestLimbToBrain));
            check(format!("{n} GripBehaviour"), format!("{:?}", a.GripBehaviour), format!("{:?}", b.GripBehaviour));
            check(format!("{n} ConnectedLimbs"), names(&want, &a.ConnectedLimbs), names(&g, &b.ConnectedLimbs));
            check(format!("{n} StrengthMultipliers"), format!("{:?}", a.StrengthMultipliers), format!("{:?}", b.StrengthMultipliers));
            check(format!("{n} species/blood/decal"), format!("{} {} {}", a.SpeciesIdentity, a.BloodLiquidType, a.BloodDecal), format!("{} {} {}", b.SpeciesIdentity, b.BloodLiquidType, b.BloodDecal.replace(' ', "_")));
            check(format!("{n} gore"), format!("{} {} {:?}", a.goreString, a.goreStringJoint, a.gore), format!("{} {} {:?}", b.goreString, b.goreStringJoint, b.gore));
            check(format!("{n} vital"), format!("{:?}", want.vital_parts.get(&l)), format!("{:?}", g.vital_parts.get(&l)));
            let (mut a, mut b) = (a.clone(), b.clone());
            diff(&n, a.fields(), b.fields(), &mut out);
            let (mut a, mut b) = (want.circulations[l].clone(), g.circulations[l].clone());
            check(format!("{n} circulation links"), format!("{:?} {:?} {:?}", a.PushesTo, a.LocalArteryRects, a.Source), format!("{:?} {:?} {:?}", b.PushesTo, b.LocalArteryRects, b.Source));
            diff(&format!("{n} circulation"), a.fields(), b.fields(), &mut out);
            let (mut a, mut b) = (want.nodes[l].clone(), g.nodes[l].clone());
            check(format!("{n} node links"), format!("{:?}", a.Connections), format!("{:?}", b.Connections));
            diff(&format!("{n} node"), a.fields(), b.fields(), &mut out);
            let (mut a, mut b) = (want.skins[l].clone(), g.skins[l].clone());
            check(format!("{n} skin links"), format!("{:?}", a.adjacentLimbs), format!("{:?}", b.adjacentLimbs));
            diff(&format!("{n} skin"), a.fields(), b.fields(), &mut out);
            if let (Some(a), Some(b)) = (want.gforces.get(l), g.gforces.get(l)) {
                let (mut a, mut b) = (a.clone(), b.clone());
                diff(&format!("{n} gforce"), a.fields(), b.fields(), &mut out);
            }
        }
        for (i, b) in g.physicals.iter().enumerate() {
            let Some(a) = want.physicals.iter().find(|a| a.id == b.id) else {
                check(format!("physical {}", b.name), "recorded".into(), b.id.to_string());
                continue;
            };
            let n = format!("{} physical", b.name);
            check(format!("{n} colliders"), format!("{:?} {:?}", a.colliders, a.all_colliders), format!("{:?} {:?}", b.colliders, b.all_colliders));
            check(format!("{n} Properties"), want.properties[a.Properties].name.clone(), g.properties[b.Properties].name.clone());
            check(format!("{n} rigidbody"), a.rigidbody.bodyType.to_string(), b.rigidbody.bodyType.to_string());
            check(format!("{n} transform"), format!("{:?}", a.transform), format!("{:?}", b.transform));
            check(format!("{n} parent"), format!("{:?}", a.parent.map(|t| want.transform_nodes[t].id)), format!("{:?}", b.parent.map(|t| g.transform_nodes[t].id)));
            check(format!("{n} grid count"), format!("{:?}", a.LocalColliderGridPoints.as_ref().map(Vec::len)), format!("{:?}", b.LocalColliderGridPoints.as_ref().map(Vec::len)));
            check(
                format!("{n} misc"),
                format!("{} {} {} {} {} {} {:?}", a.has_fire_map, a.MaximumStabVictims, a.LooseEndSize, a.StabReleaseSpeedModifier, a.StabWoundSizeMultiplier, a.OverrideImpactSounds, a.sprite_bounds),
                format!("{} {} {} {} {} {} {:?}", b.has_fire_map, b.MaximumStabVictims, b.LooseEndSize, b.StabReleaseSpeedModifier, b.StabWoundSizeMultiplier, b.OverrideImpactSounds, b.sprite_bounds),
            );
            let (mut a, mut b) = (a.clone(), g.physicals[i].clone());
            diff(&n, a.fields(), b.fields(), &mut out);
        }
        for b in &g.properties {
            match want.properties.iter().find(|a| a.name == b.name) {
                // Recordings name the ShotImpact asset without a reference to it.
                Some(a) => check(format!("properties {}", a.name), format!("{a:?}"), format!("{:?}", super::physical::PhysicalProperties { ShotImpact: b.ShotImpact.map(|(_, puff)| (None, puff)), ..b.clone() })),
                None => check(format!("properties {}", b.name), "recorded".into(), "missing".into()),
            }
        }
        check("hinge count".into(), want.hinges.len().to_string(), g.hinges.len().to_string());
        for (h, (a, b)) in want.hinges.iter().zip(&g.hinges).enumerate() {
            let show = |gm: &Game, x: &Hinge| format!("{} {:?} {} {:?} {} {} {:?} {}", x.useMotor, x.motor, x.useLimits, x.limits, x.breakForce, x.breakTorque, x.connected.map(|c| gm.limb_of_body(c)), x.enabled);
            check(format!("hinge {h}"), show(&want, a), show(&g, b));
        }
        check("grip count".into(), want.grips.len().to_string(), g.grips.len().to_string());
        for (k, (a, b)) in want.grips.iter().zip(&g.grips).enumerate() {
            check(format!("grip {k}"), format!("{} {:?}", want.physicals[a.PhysicalBehaviour].id, a.CurrentlyHolding), format!("{} {:?}", g.physicals[b.PhysicalBehaviour].id, b.CurrentlyHolding));
            let (mut a, mut b) = (a.clone(), b.clone());
            diff(&format!("grip {k}"), a.fields(), b.fields(), &mut out);
        }
        let node = |g: &Game, t: usize| format!("{} {:?} {:?}", g.transform_nodes[t].id, g.transform_nodes[t].trs, g.transform_nodes[t].parent.map(|p| g.transform_nodes[p].id));
        // The recording lists the map's nodes after the Person's.
        let k = g.transform_nodes.len().min(want.transform_nodes.len());
        check("transform nodes".into(), (0..k).map(|t| node(&want, t)).collect::<Vec<_>>().join(" | "), (0..g.transform_nodes.len()).map(|t| node(&g, t)).collect::<Vec<_>>().join(" | "));
        let (a, b) = (&want.people[0], &g.people[person]);
        check("person".into(), format!("{} {} {} {:?}", a.id, a.BoneBreakClips, a.DismembermentClips, a.localScale), format!("{} {} {} {:?}", b.id, b.BoneBreakClips, b.DismembermentClips, b.localScale));
        check("pose count".into(), a.Poses.len().to_string(), b.Poses.len().to_string());
        check("person runtime links".into(), format!("{} {:?} {:?} {:?}", names(&want, &a.Limbs), a.Head, a.LinkedPoses, a.ActivePose), format!("{} {:?} {:?} {:?}", names(&g, &b.Limbs), b.Head, b.LinkedPoses, b.ActivePose));
        for (pa, pb) in a.Poses.iter().zip(&b.Poses) {
            let show = |p: &RagdollPose| {
                let angles: Vec<String> = p.Angles.iter().map(|x| match x.Animated {
                    true => format!("{} anim {} {} {} {} {} {} {} {} {:?}", x.Limb, x.PoseRigidityModifier, x.StartAngle, x.EndAngle, x.AnimationDuration, x.RandomInfluence, x.RandomSpeed, x.TimeOffset, x.Angle, x.AnimationCurve.as_ref().map(|c| (&c.keys, c.pre, c.post))),
                    false => format!("{} {} {}", x.Limb, x.PoseRigidityModifier, x.Angle),
                }).collect();
                format!("{} {} {} {} {} {} {} {} {} {angles:?}", p.Name, p.Rigidity, p.ForceMultiplier, p.ShouldStandUpright, p.DragInfluence, p.UprightForceMultiplier, p.ShouldStumble, p.State, p.AnimationSpeedMultiplier)
            };
            check(format!("pose {}", pa.Name), show(pa), show(pb));
        }
        let (mut a, mut b) = (a.clone(), b.clone());
        diff("person", a.fields(), b.fields(), &mut out);
        for l in &links {
            eprintln!("LINK {l}");
        }
        for l in &out {
            eprintln!("FIELD {l}");
        }
        assert!(links.is_empty(), "{} link mismatches", links.len());
    }
}
