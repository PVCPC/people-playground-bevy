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

/// Where the modelled managed heap starts (see `Game::spawn`): a 2048-aligned address. Any value is as real as another
/// (the game's own differs per run); this one is a fixed pick.
pub(super) const MODELLED_HEAP_BASE: u64 = 0x0000_0000_014f_5000;
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
#[derive(Clone)]
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
    pub miniguns: HashMap<usize, usize>,
    pub accelerators: HashMap<usize, usize>,
    pub accelerator_bolts: HashMap<usize, usize>,
    pub archelix_casters: HashMap<usize, usize>,
    pub archelix_bolts: HashMap<usize, usize>,
    pub universal_bells: HashMap<usize, usize>,
    pub toggleable_mirrors: HashMap<usize, usize>,
    pub valves: HashMap<usize, usize>,
    pub timed_gates: HashMap<usize, usize>,
    pub defibrillators: HashMap<usize, usize>,
    pub cardiopulmonary_bypasses: HashMap<usize, usize>,
    pub heart_monitors: HashMap<usize, usize>,
    pub excaliburs: HashMap<usize, usize>,
    pub sharp_on_all_sides: HashMap<usize, usize>,
    pub blood_tanks: HashMap<usize, usize>,
    pub explosives: HashMap<usize, usize>,
    pub delete_afters: HashMap<usize, usize>,
    pub point_to_velocities: HashMap<usize, usize>,
    pub android_sparks: HashMap<usize, usize>,
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
    pub bulbs: HashMap<usize, usize>,
    pub thrusterbeds: HashMap<usize, usize>,
    pub pistons: HashMap<usize, usize>,
    pub flamethrowers: HashMap<usize, usize>,
    pub rockets: HashMap<usize, usize>,
    pub airfoils: HashMap<usize, usize>,
    pub stunners: HashMap<usize, usize>,
    pub soaps: HashMap<usize, usize>,
    pub power_hammers: HashMap<usize, usize>,
    pub balloons: HashMap<usize, usize>,
    pub metronomes: HashMap<usize, usize>,
    pub thermometers: HashMap<usize, usize>,
    pub infrared_thermometers: HashMap<usize, usize>,
    pub activation_toggles: HashMap<usize, usize>,
    pub power_gauges: HashMap<usize, usize>,
    pub torches: HashMap<usize, usize>,
    pub flares: HashMap<usize, usize>,
    pub buttons: HashMap<usize, usize>,
    pub lasers: HashMap<usize, usize>,
    pub firearm_attachments: HashMap<usize, usize>,
    pub televisions: HashMap<usize, usize>,
    pub displays: HashMap<usize, usize>,
    pub syringes: HashMap<usize, usize>,
    pub jet_engines: HashMap<usize, usize>,
    pub chainsaws: HashMap<usize, usize>,
    pub freezes: HashMap<usize, usize>,
    pub emps: HashMap<usize, usize>,
    pub decimators: HashMap<usize, usize>,
    pub immobility_fields: HashMap<usize, usize>,
    pub energy_swords: HashMap<usize, usize>,
    pub blasters: HashMap<usize, usize>,
    pub blasterbolts: HashMap<usize, usize>,
    pub beamformers: HashMap<usize, usize>,
    pub scifi40s: HashMap<usize, usize>,
    pub effect_guns: HashMap<usize, usize>,
    pub signal_converters: HashMap<usize, usize>,
    pub syringe_explosions: HashMap<usize, usize>,
    pub point_liquids: HashMap<usize, usize>,
    pub act_on_collides: HashMap<usize, usize>,
    pub spawners: HashMap<usize, usize>,
    pub heat_rects: HashMap<usize, usize>,
    pub random_sprites: HashMap<usize, usize>,
    /// Game::node_rigidbodies index by object.
    pub node_rigidbodies: HashMap<usize, usize>,
    /// The PlayAudioAtRandom on the instance's parent (`ambient_audio`), which MapConfig.Reinitialise configures.
    pub parent_audio: Option<usize>,
    /// The translated components (tools/translator) by (object, component index).
    pub translated: HashMap<(usize, usize), super::translated::TBehaviour>,
}

/// The clone's Transform as a physical object's.
pub(super) fn pose(inst: &Instance, o: usize) -> Pose {
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

/// A persistent call `event_calls` keeps: CarBehaviour's, DetachFromParent.Detach by its component id,
/// ActOnDestroy.Clear by its GameObject's Transform id, Behaviour.set_enabled(true) on a DeleteAfterTime (its component
/// id) or a RandomForceAtStart (its Rigidbody2D's component id, PositionalVelocityStrength, RotationalVelocityStrength),
/// or Rigidbody2D.set_simulated(true) (the Rigidbody2D's component id); InstantiateObjectBehaviour.Instantiate by its
/// component id; PhysicalBehaviour.HaltNextPropagation by its component id; Joint2D.set_breakForce(v) by the joint's
/// component id; Behaviour.set_enabled(v) on a UseEventTrigger by its component id; Rigidbody2D.WakeUp and
/// AddTorque(torque) by the Rigidbody2D's component id.
#[derive(Clone, Copy, Debug)]
pub enum EventCall {
    Car(i32, super::car::CarCall),
    Detach(i32),
    ClearDestroyWith(i32),
    EnableDeleteAfter(i32),
    EnableRandomForce(i32, f32, f32),
    Simulate(i32),
    Instantiate(i32),
    AddRelativeForce(i32, f32),
    HaltNextPropagation(i32),
    SetBreakForce(i32, f32),
    SetUseEventEnabled(i32, bool),
    /// Behaviour.set_enabled(false) on a DamagableMachineryBehaviour.
    DisableMachinery(i32),
    /// JetEngineBehaviour.ForceShutoff by its component id.
    JetForceShutoff(i32),
    WakeUp(i32),
    AddTorque(i32, f32),
    /// LiquidSpreadAoEBehaviour.Spread by its component id.
    LiquidSpread(i32),
}

/// UseEventTrigger: Use runs Event while enabled and (not OnlyOnce or never used). Nothing sets its Action.
#[derive(Clone, Debug)]
pub struct UseEvent {
    pub enabled: bool,
    pub only_once: bool,
    pub use_count: i32,
    pub calls: Vec<EventCall>,
}

/// The calls of an event that are not presentation (`presentation_call`; `animations` also lets the animation
/// behaviours of `animation_call` through), in order: CarBehaviour (`car_calls`), DetachFromParent.Detach,
/// ActOnDestroy.Clear, Behaviour.set_enabled(true) on a DeleteAfterTime or RandomForceAtStart and
/// Rigidbody2D.set_simulated(true).
fn event_calls(prefab: &Prefab, file: usize, event: &Value, inst: &Instance, what: &str, animations: bool) -> Result<Vec<EventCall>, String> {
    match try_event_calls(prefab, file, event, inst, animations)? {
        Ok(calls) => Ok(calls),
        Err(method) => unimplemented!("{what}: a persistent call beyond audio, particles, CarBehaviour, DetachFromParent.Detach, ActOnDestroy.Clear, InstantiateObjectBehaviour.Instantiate, AddForceBehaviour.AddRelativeForce, PhysicalBehaviour.HaltNextPropagation, set_enabled of a DeleteAfterTime, RandomForceAtStart, UseEventTrigger or DamagableMachineryBehaviour, Joint2D.set_breakForce and Rigidbody2D.set_simulated(true), WakeUp and AddTorque ({method})"),
    }
}

/// `event_calls` without the panic: the method name of the first call it cannot keep is the inner Err.
fn try_event_calls(prefab: &Prefab, file: usize, event: &Value, inst: &Instance, animations: bool) -> Result<Result<Vec<EventCall>, String>, String> {
    let mut cars = car_calls(prefab, file, event, inst).into_iter();
    let mut out = Vec::new();
    for call in invoked_calls(event) {
        if presentation_call(prefab, file, call) || animations && animation_call(prefab, file, inst, call) {
            continue;
        }
        let method = string(call, "m_MethodName")?;
        let on = call.get("m_Arguments").and_then(|a| a.get("m_BoolArgument")).and_then(Value::bool);
        let number = call.get("m_Arguments").and_then(|a| a.get("m_FloatArgument")).and_then(Value::f32);
        let target = call_target(prefab, file, call);
        let kept = target.and_then(|(o, c)| match &prefab.objects[o].components[c].1 {
            Component::Script { class, fields, .. } => match (class.as_str(), method.as_str()) {
                ("CarBehaviour", _) => cars.next().map(|(id, c)| EventCall::Car(id, c)),
                ("DetachFromParent", "Detach") => Some(EventCall::Detach(inst.objects[o].components[c])),
                ("ActOnDestroy", "Clear") => Some(EventCall::ClearDestroyWith(inst.objects[o].components[0])),
                ("AddForceBehaviour", "AddRelativeForce") if number.is_some() => Some(EventCall::AddRelativeForce(inst.objects[o].components[c], number.unwrap())),
                ("InstantiateObjectBehaviour", "Instantiate") => Some(EventCall::Instantiate(inst.objects[o].components[c])),
                ("PhysicalBehaviour", "HaltNextPropagation") => Some(EventCall::HaltNextPropagation(inst.objects[o].components[c])),
                ("DamagableMachineryBehaviour", "set_enabled") if on == Some(false) => Some(EventCall::DisableMachinery(inst.objects[o].components[c])),
                ("JetEngineBehaviour", "ForceShutoff") => Some(EventCall::JetForceShutoff(inst.objects[o].components[c])),
                ("LiquidSpreadAoEBehaviour", "Spread") => Some(EventCall::LiquidSpread(inst.objects[o].components[c])),
                ("UseEventTrigger", "set_enabled") if on.is_some() => Some(EventCall::SetUseEventEnabled(inst.objects[o].components[c], on.unwrap())),
                ("DeleteAfterTime", "set_enabled") if on == Some(true) => Some(EventCall::EnableDeleteAfter(inst.objects[o].components[c])),
                ("RandomForceAtStart", "set_enabled") if on == Some(true) => {
                    let k = prefab.objects[o].components.iter().position(|(_, x)| matches!(x, Component::Rigidbody2D(_)))?;
                    Some(EventCall::EnableRandomForce(inst.objects[o].components[k], float(fields, "PositionalVelocityStrength").ok()?, float(fields, "RotationalVelocityStrength").ok()?))
                }
                _ => None,
            },
            Component::Rigidbody2D(_) if method == "set_simulated" && on == Some(true) => Some(EventCall::Simulate(inst.objects[o].components[c])),
            Component::Rigidbody2D(_) if method == "WakeUp" => Some(EventCall::WakeUp(inst.objects[o].components[c])),
            Component::Rigidbody2D(_) if method == "AddTorque" && number.is_some() => Some(EventCall::AddTorque(inst.objects[o].components[c], number.unwrap())),
            Component::Joint2D(_) if method == "set_breakForce" && number.is_some() => Some(EventCall::SetBreakForce(inst.objects[o].components[c], number.unwrap())),
            _ => None,
        });
        match kept {
            Some(e) => out.push(e),
            None => return Ok(Err(method)),
        }
    }
    Ok(Ok(out))
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
        if !animation_call(prefab, file, inst, call) {
            unimplemented!("{what}: a persistent call beyond audio, particles and animations ({})", string(call, "m_MethodName")?);
        }
    }
    Ok(())
}

/// Play on a Translation/Rotation/ColorAnimationBehaviour animating a subtree without physics.
fn animation_call(prefab: &Prefab, file: usize, inst: &Instance, call: &Value) -> bool {
    call_target(prefab, file, call).is_some_and(|(to, tc)| {
        matches!(&prefab.objects[to].components[tc].1, Component::Script { class, .. } if matches!(class.as_str(), "TranslationAnimationBehaviour" | "RotationAnimationBehaviour" | "ColorAnimationBehaviour"))
            && string(call, "m_MethodName").is_ok_and(|m| m == "Play")
            && std::iter::once(to).chain(inst.descendants(to)).all(|x| prefab.objects[x].components.iter().all(|(_, c)| !matches!(c, Component::Collider2D(_) | Component::Rigidbody2D(_))))
    })
}

fn presentation_call(prefab: &Prefab, file: usize, call: &Value) -> bool {
    call_target(prefab, file, call).is_some_and(|(o, c)| match &prefab.objects[o].components[c].1 {
        Component::Other(82 | 164..=170 | 198) => true,
        // Its LateUpdate only drives an AudioSource (see check_rewritten).
        Component::Script { class, .. } => class == "RectangleAudioSourceBehaviour"
            || (class == "StickyGrenadeBehaviour" && string(call, "m_MethodName").is_ok_and(|m| m == "StartBlinking"))
            // Refresh: LightSprite.enabled = GlowingSprites.Contains(ObjectSprite.sprite).
            || (class == "JackOLaternBehaviour" && string(call, "m_MethodName").is_ok_and(|m| m == "Refresh")),
        _ => false,
    })
}

/// `check_rewritten` for the script on prefab object `o`: a TrivialAnimationBehaviour only animates its own
/// Transform or SpriteRenderer (AnimationCurve/Gradient, no Random), which is presentation over a subtree without
/// colliders or bodies.
pub(super) fn check_rewritten_at(prefab: &Prefab, o: usize, class: &str) {
    // Update reads IsBeingUsedContinuously into private counters and calls ForceSetProgress on its TrivialAnimationBehaviour:
    // presentation when that animation is.
    if class == "TrivialAnimationActivationBehaviour" {
        let Some((_, Component::Script { fields, .. })) = prefab.objects[o].components.iter().find(|(_, c)| matches!(c, Component::Script { class: k, .. } if k == class)) else { unreachable!() };
        let target = fields.get("AnimationBehaviour").and_then(Value::pptr).filter(|p| !p.is_null()).unwrap_or_else(|| unimplemented!("TrivialAnimationActivationBehaviour with a null AnimationBehaviour (NullReferenceException)"));
        let (to, tc) = prefab.index.iter().find(|(k, _)| k.path_id == target.path_id).map(|(_, v)| *v).expect("TrivialAnimationActivationBehaviour.AnimationBehaviour outside the prefab");
        let Component::Script { class: animation, .. } = &prefab.objects[to].components[tc].1 else { unimplemented!("TrivialAnimationActivationBehaviour.AnimationBehaviour is no script") };
        check_rewritten_at(prefab, to, animation);
        return;
    }
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
        | "JointBreakSoundBehaviour" | "DamagableMachineryBehaviour" | "DestroyableBehaviour" | "VaseBehaviour" | "ExplosionSoundBehviour" | "ActOnDestroy"
        | "DestroyWhenAllChildrenDestroyed" | "InstantiateObjectBehaviour" | "DetachFromParentOnStart" | "RandomForceAtStart" | "DoAtTimeOnce" | "RandomSpriteBehaviour" | "LavaBehaviour" | "RandomPrefabSpawnerBehaviour"
        | "TemperatureTargetRectangleBehaviour" | "ExistByChance" | "ExplosiveBehaviour" | "DeleteAfterTime" | "ActOnCollide" | "NoCollide" | "StickyGrenadeBehaviour" | "ActOnHingeJointLimit" | "PointToVelocityBehaviour" | "AndroidSparkCreator" | "ActivateIfFancyEffects" | "ExistInDetailView" | "IgnoreParentSize" | "LocalFirePropagationMap" | "RandomSpawnedSize" | "NoChildrenCollide" | "FirearmBehaviour" | "CarBehaviour" | "TyreDeflationBehaviour" | "GasTankBehaviour" | "FlaskBehaviour" | "LiquidContainerController" | "PointLiquidTransferBehaviour" | "TruckDoorBehaviour" | "GlowtubeBehaviour" | "LEDBulbBehaviour" | "DetectorBehaviour" | "RandomCarTextureBehaviour" | "NoCollideGm" | "JointSerialisationHelper" | "BatchJointSerialisationHelper" | "ProjectileLauncherBehaviour" | "OnlyExplosionBehaviour" | "BellBehaviour" | "MachineGunBehaviour" | "GeneratorBehaviour" | "SingleFloodlightBehaviour" | "SpikeGrenadeBehaviour" | "HeatingElementBehaviour" | "TemperatureRayGunBehaviour" | "BulbBehaviour" | "ThrusterbedBehaviour" | "PistonBehaviour" | "FlamethrowerBehaviour" | "LaunchedRocketBehaviour"
        // Airfoil's Awake, Start and FixedUpdate are rewritten (airfoil.rs; its context menu buttons are UI). Stunner's Update is (stunner.rs).
        | "AirfoilBehaviour" | "StunnerBehaviour" | "SoapBehaviour" | "PowerHammerBehaviour" | "BalloonBehaviour" | "MetronomeBehaviour" => {}
        // Start, OnEnable, FixedUpdate and OnDisable only show or hide readouts; Use and OnDisable's state is rewritten (thermometer.rs).
        // InfraredThermometer.Update is rewritten (thermometer.rs: infrared_update); its text and line are presentation.
        "ThermometerBehaviour" | "InfraredThermometer" => {}
        // Start flips a negative local scale.x back (thermometer.rs: maintain_scale_start).
        "MaintainScaleSignBehaviour" => {}
        // Awake only fetches components; Update only moves the arm (presentation). Start spawns the labels (gauge.rs).
        "PowerGaugeBehaviour" => {}
        // Start enables its TorchBehaviour while Activated; Use lights it (flare.rs).
        "FlareBehaviour" => {}
        // OnEnable registers contacts, OnCollisionEnter2D and Exit2D and FixedUpdate are rewritten (torch.rs).
        "TorchBehaviour" => {}
        // Awake draws the glow seed, Start is presentation and the context menu, and Update, Use, OnCollisionEnter2D and
        // Shot/Break are rewritten (television.rs).
        "TelevisionBehaviour" => {}
        // Awake subscribes to the continuous listener and FixedUpdate presses (button.rs); Start is the context menu, and
        // Update and OnDisable only drive the light.
        "ButtonBehaviour" => {}
        // Start and Use run UpdateLaser and LateUpdate fires it (laser.rs); OnDisable and OnDestroy only hide the beam.
        "LaserBehaviour" => {}
        // Awake adds a CheckOnDestroyAttachment; Use and Battery's FixedUpdate need a connection, which the point makes and
        // this file refuses (attachment.rs). The base class is the scope's and the others' Awake.
        "FirearmAttachmentBehaviour" | "ScopeAttachmentBehaviour" | "BatteryAttachmentBehaviour" | "ExplosiveAttachmentBehaviour" | "FlashlightAttachmentBehaviour" | "IncendiaryAttachmentBehaviour" => {}
        // Awake draws the start string and Use and OnDisable flip Activated (display.rs); Start writes the text and menu.
        "DisplayBehaviour" => {}
        // Awake draws its offset (the draw stays, in awake_behaviours); OnWillRenderObject's colour and the text object's
        // localPosition from the offset's Perlin noise are rendering.
        "HologramJitter" => {}
        // Awake subscribes to the physical's OnSingleUse (activate_direct), Start and OnDisable are rewritten
        // (activation_toggle.rs), OnDestroy only removes the listener (destroyable.rs).
        "ActivationToggleBehaviour" => {}
        // jet_engine.rs: Awake caches components and clears the plasma colour (rendering).
        "JetEngineBehaviour" => {}
        // chainsaw.rs: Start draws the seed, FixedUpdate saws, Update adds speed, OnDestroy destroys the joints; the
        // audio, particles and the saw sprite are presentation.
        "ChainsawBehaviour" => {}
        // freeze.rs: Start keeps the velocities and requests the freeze, OnDestroy releases it; the outline is rendering.
        "FreezeBehaviour" => {}
        // emp.rs: Use runs Activate; Awake's Killzone clone is presentation (no collider, sized for rendering).
        "EMPBehaviour" => {}
        // decimator.rs
        "DecimatorBehaviour" => {}
        // immobility_field.rs
        "ImmobilityFieldBehaviour" => {}
        // energy_sword.rs: Awake hides the blade, Start, Use, FixedUpdate, Update, OnDisable and OnDestroy are
        // rewritten; the audio and particles are presentation.
        "EnergySwordBehaviour" => {}
        // OnCollisionEnter2D instantiates CollideWithOtherSwordVFX (simulate.rs).
        "EnergySwordBladeClashBehaviour" => {}
        // The blade's trail mesh: Awake, OnEnable, Update and OnDestroy are rendering (it moves only its own
        // Transform, which nothing simulated reads).
        "EnergySwordMotionBlur" => {}
        // blaster.rs, scifi.rs: Awake caches components (the Disassembler's takes an AudioSource id), Start flips the
        // muzzle flash (BlasterBehaviour), shows or hides lines (BeamformerBehaviour); the bolt only has Update.
        "BlasterBehaviour" | "BlasterboltBehaviour" | "BeamformerBehaviour" | "GenericScifiWeapon40Behaviour" => {}
        // effect_gun.rs: Awake adds an AudioSource (its id), Use and Affect are rewritten.
        "HealthGunBehaviour" | "FreezeGunBehaviour" => {}
        // syringe.rs: Awake and OnWillRenderObject are rendering, Start's AddButtons is UI.
        c if super::syringe::is_syringe(c) || c == "SyringeExplosionBehaviour" => {}
        // Renderer sorting from the parent's SpriteRenderer; SpriteRenderer.color from Temperature when visible.
        // SmoothLineControllerBehaviour only sets LineRenderer positions and width.
        "SortingLayerChild" | "VertexColourBasedOnTemperatureBehaviour" | "SmoothLineControllerBehaviour" => {}
        // OnEnable's two Random.value (`color_wave_on_enable`) feed only OnWillRenderObject's SpriteRenderer.color.
        "ColorWave" => {}
        // Start: see PendingStart::FirearmAttachmentPoint; OnTriggerEnter2D: `attachment_point_trigger_enter`; OnDestroy only
        // destroys the FirearmAttachment component, which goes with its object.
        "FirearmAttachmentPointBehaviour" => {}
        // Awake keeps the bodies' masses for OnMakeWeightful; nothing rewritten sends OnMakeWeightless/OnMakeWeightful.
        "WeightlessHelper" => {}
        // OnChildDisintegrated over a person's limbs (PersonBehaviour.DisintegrationCount) or `Game::disintegration_counters`.
        "DisintegrationCounterBehaviour" => {}
        // OnDestroy drops the object's undo entry.
        "DeregisterBehaviour" => {}
        // Awake and OnDestroy keep GearManagerBehaviour.Gears; OnTriggerEnter2D's GetComponentInParent is unused and
        // ManagedUpdate/ManagedLateUpdate are empty.
        "GearBehaviour" => {}
        // Global.AddAudioSource/RemoveAudioSource, AudioSource.volume, StatManager, SpriteRenderer.color; material
        // property blocks in OnWillRenderObject.
        // AudioSource.Play/Stop/volume from the camera position (RectangleAudioSourceBehaviour), the radio clip (its instance
        // id is taken in Awake), and the OnBecameVisible fade of a SpriteRenderer before SetActive(false) of a render-only
        // subtree (PoliceBoxBehaviour).
        "AudioSourceTimeScaleBehaviour" | "WaterSoundBehaviour" | "IncreaseStatOnStart" | "PanelsBehaviour" | "BurnableSpriteBehaviour"
        | "GlowingHotMetalBehaviour" | "FuseBoxBehaviour" | "RectangleAudioSourceBehaviour" | "PoliceBoxBehaviour"
        // Update stores ParticleSystem.isEmitting in IsPlaying (read when serialising); Start plays or stops that particle system.
        | "ParticleSystemSerialisationHelper" => {}
        // Start adds a context menu button and sets AudioSource.enabled on its Sources.
        "MuteBehaviour" => {}
        // Use and Update are rewritten (minigun.rs). Awake caches components, FixedUpdate only decays the barrel's spin
        // and OnWillRenderObject sets its property block: presentation. The sound coroutine is audio.
        "MinigunBehaviour" => {}
        // Update (the charge and the shots) is rewritten (accelerator.rs).
        "AcceleratorGunBehaviour" => {}
        // Update and FixedUpdate are rewritten (accelerator.rs, bolt.rs for the flight); the bolt has no Awake or Start.
        "AcceleratorBoltBehaviour" => {}
        // Start and Use (the BurstShot coroutine) are rewritten (archelix.rs); the muzzle flash and sounds are presentation.
        "ArchelixCasterBehaviour" => {}
        // Start, Update, FixedUpdate, OnHit, DespawnRoutine and OnPoolableInitialised are rewritten (archelix.rs, pool.rs).
        "ArchelixCasterBoltBehaviour" => {}
        // Start, FixedUpdate and Update are rewritten (universal_bell.rs); Awake, colours and labels are presentation.
        "UniversalBellBehaviour" => {}
        // FixedUpdate is rewritten (mirror.rs); Awake and OnWillRenderObject set the shader's progress only.
        "ToggleableMirrorBehaviour" => {}
        // Awake (linked), Start and OnWillRenderObject are rewritten (foliage.rs); the property block is presentation.
        "FoliageBehaviour" => {}
        // Awake, Update, OnJointBreak2D, Shot, Slice and OnEnable's GetComponent are rewritten (breakjoint.rs).
        "BreakJointOnBurnBehaviour" | "BreakJointOnShotBehaviour" => {}
        // Awake (linked) and FixedUpdate are rewritten (shake.rs); the one-shot itself is audio.
        "SoundOnShakeBehaviour" => {}
        // FixedUpdate and OnCollisionEnter2D are rewritten (gorse.rs); Start caches the emission module only.
        "GorseDroolingBehaviour" | "GorseProjectileBehaviour" => {}
        // Start and FixedUpdate are rewritten (misc.rs); OnCollide has no subscriber outside the rewrite's checks.
        "FancyBloodSplatController" => {}
        // FixedUpdate only feeds ParticleSystem.Emit (the native particle RNG): presentation, and a null rb or ps only
        // throws inside it.
        "EmitParticlesOnShakeBehaviour" => {}
        // Start and Spread are rewritten (liquid_spread.rs).
        "LiquidSpreadAoEBehaviour" => {}
        // OnCollisionEnter2D is rewritten (wrench.rs); its android limb heal and Repair receivers stop there.
        "WrenchBehaviour" => {}
        // OnCollisionEnter2D is rewritten (no_collide.rs).
        "NoCollideWithBodyPart" => {}
        // Start and Update are rewritten (blood_tank.rs); its splat's OnCollide listener is set at link time.
        "BloodTankBehaviour" => {}
        // Update and Use are rewritten (valve.rs); Awake, Start, the display and OnWillRenderObject are presentation.
        "ValveBehaviour" => {}
        // Awake sets the physical's delay, Start and OnEnable resume a busy DoLag, Use and OnDisable are rewritten (timed_gate.rs).
        "TimedGateBehaviour" => {}
        // Use, DefibRoutine and OnDisable are rewritten (resuscitation.rs); Start and OnEnable only set the text screen.
        "DefibrillatorBehaviour" => {}
        // Start, Use, OnEnable (UpdateActivation) and OnDisable are rewritten (resuscitation.rs); the pumping is BloodWireBehaviour's.
        "CardiopulmonaryBypassMachineBehaviour" => {}
        // Use and Update are rewritten (heart_monitor.rs); Awake, Start, OnEnable and the display are presentation.
        "HeartMonitorBehaviour" => {}
        // Use and Update are rewritten (excalibur.rs); Awake, the glow and the sounds are presentation. CastSpell and
        // the fireball stop there.
        "ExcaliburBehaviour" => {}
        // FixedUpdate, OnDisable and OnDestroy are rewritten (sharp_on_all_sides.rs, soft_connect.rs).
        "SharpOnAllSidesBehaviour" => {}
        // Start sets the static Instance that the bolt's lightning calls Emit on (lightning.rs); it has no other state.
        "SimpleLightningEmitterBehaviour" => {}
        // Awake only subscribes SyncSprite to the parent's OnAfterChange (the sprite is presentation). It draws nothing.
        "RandomSpriteChildBehaviour" => {}
        name if super::translated::CLASSES.contains(&name) => {}
        _ => {
            let messages = super::lifecycle::LIFECYCLE.iter().find(|(k, _)| *k == class).map_or(&[][..], |(_, m)| *m);
            if !messages.is_empty() {
                unimplemented!("{class} ({})", messages.join(", "));
            }
        }
    }
}

impl Game {
    /// The ShatteredObjectGenerator component `reference` points at, as a `shatter_generators` entry (None for a null reference).
    fn load_shatter_generator(&mut self, db: &mut AssetDb, l: &Linker, reference: &Value) -> Result<Option<usize>, String> {
        let p = reference.pptr().ok_or("ShatteredObjectSpriteInitialiser: not a reference")?;
        if p.is_null() {
            return Ok(None);
        }
        let &(o, c) = l.prefab.index.get(&ObjRef { file: l.file, path_id: p.path_id }).ok_or("ShatteredObjectSpriteInitialiser: a dangling generator")?;
        let Component::Script { class, fields: v, .. } = &l.prefab.objects[o].components[c].1 else { return Err("ShatteredObjectSpriteInitialiser: not a script".into()) };
        if class != "ShatteredObjectGenerator" {
            return Err(format!("ShatteredObjectSpriteInitialiser: a {class} generator"));
        }
        let sprite_ref = field(v, "Sprite")?.pptr().ok_or("ShatteredObjectGenerator.Sprite")?;
        let sprite_asset = db.resolve(l.file, sprite_ref).ok_or("ShatteredObjectGenerator.Sprite does not resolve")?;
        let sprite = Sprite::read(db.data(sprite_asset))?;
        // Sprite.uv and texture size only feed the shard material's _AtlasTransform (rendering).
        let int = |name: &str| field(v, name)?.i64().ok_or_else(|| format!("ShatteredObjectGenerator.{name}"));
        let range = field(v, "SoftJointStrengthRange")?;
        let g = shatter::ShatteredObjectGenerator {
            name: string(v, "Name")?,
            PartMass: float(v, "PartMass")?,
            Sprite: shatter::Sprite { id: sprite_asset.path_id as i32, rect_width: sprite.rect[2], rect_height: sprite.rect[3], pixels_per_unit: sprite.pixels_to_units, ..Default::default() },
            PartCount: int("PartCount")? as i32,
            PartSizeRatio: float(v, "PartSizeRatio")?,
            PartsLayer: int("PartsLayer")? as i32,
            PartsPhysicalProperties: 0,
            RandomForceIntensity: float(v, "RandomForceIntensity")?,
            RandomTorqueIntensity: float(v, "RandomTorqueIntensity")?,
            GenerateSoftJointsChance: float(v, "GenerateSoftJointsChance")?,
            GenerateOnStart: field(v, "GenerateOnStart")?.bool().ok_or("ShatteredObjectGenerator.GenerateOnStart")?,
            ConnectTo: l.physical(field(v, "ConnectTo")?)?,
            ConnectToRange: float(v, "ConnectToRange")?,
            SoftJointStrengthRange: [float(range, "x")?, float(range, "y")?],
            Brightness: 1.0,
        };
        let p = field(v, "PartsPhysicalProperties")?.pptr().ok_or("ShatteredObjectGenerator.PartsPhysicalProperties")?;
        let asset = db.resolve(l.file, p).ok_or("ShatteredObjectGenerator.PartsPhysicalProperties does not resolve")?;
        // Shards add it to `properties` when the first one is made (the recorded arena holds only what a spawned component used).
        let bounds = sprite_bounds(&sprite);
        self.shatter_generators.push(shatter::GeneratorSource { g, properties: read_physical_properties(db, asset)?, sprite: Some(sprite_asset), sprite_bounds: bounds });
        Ok(Some(self.shatter_generators.len() - 1))
    }

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
            let rigidbody = Rigidbody { body, bodyType: rb.body_type, collisionDetectionMode: rb.collision_detection, interpolation: rb.interpolate, movement: MovementState::default(), detached, component, simulated: rb.simulated, detached_props: [rb.linear_drag, rb.angular_drag, rb.gravity_scale], constraints: rb.constraints };
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
        // Soap's Clean walks a hit collider's ancestors for DecalControllers; the non-physical ones are not rewritten.
        for o in 0..n {
            for (c, (_, comp)) in prefab.objects[o].components.iter().enumerate() {
                if !matches!(comp, Component::Collider2D(_)) {
                    continue;
                }
                let (mut up, mut decal) = (Some(o), false);
                while let Some(t) = up {
                    if !l.arenas.physical.contains_key(&t) && prefab.objects[t].components.iter().any(|(_, x)| matches!(x, Component::Script { class, .. } if class == "DecalControllerBehaviour")) {
                        decal = true;
                        break;
                    }
                    up = inst.objects[t].parent;
                }
                if decal {
                    self.decal_chain_colliders.insert(inst.objects[o].components[c]);
                }
            }
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
                        let bounds = renderer_bounds(&Sprite::read(db.data(sprite))?, r);
                        self.renderers.push(Renderer { anchor, chain, sprite: (sprite, r.color, r.sorting_order), id: inst.objects[o].components[c], draw: ([r.flip_x, r.flip_y], r.draw_mode, r.size), visible, bounds });
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
            p.rigidbody = Rigidbody { body, bodyType: rb.body_type, collisionDetectionMode: rb.collision_detection, interpolation: rb.interpolate, movement: MovementState::default(), detached, component, simulated: rb.simulated, detached_props: [rb.linear_drag, rb.angular_drag, rb.gravity_scale], constraints: rb.constraints };
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
            p.HoldingPositions = array(v, "HoldingPositions")?.iter().map(floats::<3>).collect::<Result<_, _>>()?;
            // Awake's GetComponent<LocalFirePropagationMap>() and Start's GetComponents<DecalControllerBehaviour>().
            p.has_fire_map = script(prefab, o, "LocalFirePropagationMap").is_some();
            p.prefab_decal_controllers = prefab.objects[o].components.iter().filter(|(_, x)| matches!(x, Component::Script { class, .. } if class == "DecalControllerBehaviour")).count();
            p.audio_time_scale = script(prefab, o, "AudioSourceTimeScaleBehaviour").is_some();
            p.audioSource = prefab.objects[o].components.iter().any(|(_, x)| matches!(x, Component::Other(AUDIO_SOURCE)));
            p.MaximumStabVictims = field(v, "MaximumStabVictims")?.i64().ok_or("MaximumStabVictims")? as i32;
            p.LooseEndSize = float(v, "LooseEndSize")?;
            p.StabReleaseSpeedModifier = float(v, "StabReleaseSpeedModifier")?;
            p.StabWoundSizeMultiplier = float(v, "StabWoundSizeMultiplier")?;
            p.ActivationPropagationDelay = float(v, "ActivationPropagationDelay")?;
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
                    destroy_at: None,
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
            // A null BloodDecal (the Android's limbs) stays the empty name: see Game::limb_blood_decal.
            let decal = field(v, "BloodDecal")?.pptr().ok_or("BloodDecal")?;
            b.BloodDecal = match db.resolve(file, decal) {
                Some(decal) => {
                    let decal = self.load_decal_descriptor(db, decal)?;
                    self.decal_descriptors[decal].name.clone()
                }
                None if decal.is_null() => String::new(),
                None => return Err("LimbBehaviour: a BloodDecal that does not resolve".into()),
            };
            self.vital_parts.insert(self.limbs.len(), array(v, "VitalParts")?.iter().map(bounds).collect::<Result<_, _>>()?);
            // Awake's GetComponent<ShatteredObjectSpriteInitialiser>() and its three generators.
            b.ShatteredObjectGenerators = match script(prefab, o, "ShatteredObjectSpriteInitialiser") {
                None => None,
                Some((_, _, iv)) => {
                    let mut gens = [None; 3];
                    for (k, name) in ["BoneGenerator", "FleshGenerator", "SkinGenerator"].into_iter().enumerate() {
                        gens[k] = self.load_shatter_generator(db, &l, field(iv, name)?)?;
                    }
                    Some(gens)
                }
            };
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
            let Some((c, _, v)) = script(prefab, o, "GripBehaviour") else { continue };
            let mut g = Grip::default();
            load_values(g.fields(), v)?;
            g.id = inst.objects[o].components[c];
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
            // VaseBehaviour extends DestroyableBehaviour (its own methods are presentation: debris sprite).
            let Some((c, _, v)) = script(prefab, o, "DestroyableBehaviour").or_else(|| script(prefab, o, "VaseBehaviour")) else { continue };
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
            x.OnBreak = event_calls(prefab, file, field(v, "OnBreak")?, inst, "DestroyableBehaviour.OnBreak", false)?;
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
                x.Tag = string(v, "Tag")?;
                x.OnExplodeCalls = event_calls(prefab, file, field(v, "OnExplode")?, inst, "ExplosiveBehaviour.OnExplode", false)?;
                x.OnArmPresentation = presentation_event(prefab, file, field(v, "OnArm")?);
                let vfx = field(v, "VFXOverride")?.pptr().ok_or("VFXOverride")?;
                x.VFXOverride = if vfx.is_null() { None } else { Some(db.resolve(file, vfx).ok_or("VFXOverride outside the data")?) };
                explosives.insert(o, self.explosives.len());
                self.explosives.push(x);
            }
            if let Some((c, _, v)) = script(prefab, o, "AddForceBehaviour") {
                let physical = *l.arenas.physical.get(&o).unwrap_or_else(|| unimplemented!("AddForceBehaviour on a GameObject without a PhysicalBehaviour"));
                self.add_forces.push(super::misc::AddForce {
                    id: inst.objects[o].components[c],
                    physical,
                    ForceMode: field(v, "ForceMode")?.i64().ok_or("ForceMode")? as i32,
                    LocalAxis: floats::<2>(field(v, "LocalAxis")?)?,
                    ScaleWithSize: field(v, "ScaleWithSize")?.bool().ok_or("ScaleWithSize")?,
                });
            }
            if let Some((c, enabled, v)) = script(prefab, o, "DeleteAfterTime") {
                let mut x = super::explosion::DeleteAfter::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
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
        // MinigunBehaviour: fires and resets the FirearmBehaviour field (a null one is a NullReferenceException on Update).
        let mut miniguns = HashMap::new();
        for o in 0..n {
            let Some((c, enabled, v)) = script(prefab, o, "MinigunBehaviour") else { continue };
            let mut x = super::minigun::Minigun::default();
            load_values(x.fields(), v)?;
            x.id = inst.objects[o].components[c];
            x.enabled = enabled;
            x.physical = l.arenas.physical.get(&o).copied().unwrap_or_else(|| unimplemented!("MinigunBehaviour without a PhysicalBehaviour (NullReferenceException)"));
            let fr = field(v, "FirearmBehaviour")?.pptr().ok_or("MinigunBehaviour.FirearmBehaviour")?;
            if fr.is_null() || fr.file != 0 {
                unimplemented!("MinigunBehaviour.FirearmBehaviour null or outside the prefab (NullReferenceException in Update)");
            }
            let (fo, _) = *prefab.index.get(&ObjRef { file, path_id: fr.path_id }).ok_or("MinigunBehaviour.FirearmBehaviour: dangling reference")?;
            x.firearm = *firearms.get(&fo).unwrap_or_else(|| unimplemented!("MinigunBehaviour.FirearmBehaviour on an object without a FirearmBehaviour in this clone"));
            miniguns.insert(o, self.miniguns.len());
            self.miniguns.push(x);
        }
        // AcceleratorGunBehaviour: the charge is PhysicalBehaviour's; a release spawns the bolt (accelerator.rs).
        let mut accelerators = HashMap::new();
        for o in 0..n {
            let Some((c, enabled, v)) = script(prefab, o, "AcceleratorGunBehaviour") else { continue };
            let mut x = super::accelerator::Accelerator::default();
            load_values(x.fields(), v)?;
            x.id = inst.objects[o].components[c];
            x.enabled = enabled;
            let ph = field(v, "PhysicalBehaviour")?.pptr().ok_or("AcceleratorGunBehaviour.PhysicalBehaviour")?;
            if ph.is_null() || ph.file != 0 {
                unimplemented!("AcceleratorGunBehaviour.PhysicalBehaviour null or outside the prefab (NullReferenceException in Update)");
            }
            let (po, _) = *prefab.index.get(&ObjRef { file, path_id: ph.path_id }).ok_or("AcceleratorGunBehaviour.PhysicalBehaviour: dangling reference")?;
            x.physical = l.arenas.physical.get(&po).copied().unwrap_or_else(|| unimplemented!("AcceleratorGunBehaviour.PhysicalBehaviour on an object without a physical behaviour in this clone"));
            x.SingleShotSound = array(v, "SingleShotSound")?.len();
            // The barrel is the Rigidbody2D's of this GameObject (BarrelPosition and the recoil use the physical's body).
            let rbp = field(v, "Rigidbody")?.pptr().ok_or("AcceleratorGunBehaviour.Rigidbody")?;
            if rbp.is_null() || rbp.file != 0 || prefab.index.get(&ObjRef { file, path_id: rbp.path_id }).map(|&(ro, _)| ro) != Some(o) {
                unimplemented!("AcceleratorGunBehaviour.Rigidbody not on its own GameObject");
            }
            let proj = field(v, "ProjectilePrefab")?.pptr().ok_or("AcceleratorGunBehaviour.ProjectilePrefab")?;
            if !proj.is_null() {
                x.ProjectilePrefab = Some(db.resolve(file, proj).ok_or("AcceleratorGunBehaviour.ProjectilePrefab outside the data")?);
            }
            accelerators.insert(o, self.accelerators.len());
            self.accelerators.push(x);
        }
        // AcceleratorBoltBehaviour: a bare Transform the gun's shots instantiate (its prefab has no physics).
        let mut accelerator_bolts = HashMap::new();
        for o in 0..n {
            let Some((c, enabled, v)) = script(prefab, o, "AcceleratorBoltBehaviour") else { continue };
            if prefab.objects.iter().any(|x| x.components.iter().any(|(_, c)| matches!(c, Component::Collider2D(_) | Component::Rigidbody2D(_)))) {
                unimplemented!("AcceleratorBoltBehaviour on a prefab with physics (its pose)");
            }
            let mut x = super::accelerator::AcceleratorBolt::default();
            load_values(x.fields(), v)?;
            x.id = inst.objects[o].components[c];
            x.enabled = enabled;
            x.layers = field(field(v, "Layers")?, "m_Bits")?.i64().ok_or("AcceleratorBoltBehaviour.Layers.m_Bits")? as u32;
            x.ImmobiltyFieldLayer = field(field(v, "ImmobiltyFieldLayer")?, "m_Bits")?.i64().ok_or("AcceleratorBoltBehaviour.ImmobiltyFieldLayer.m_Bits")? as u32;
            for (name, slot) in [("ImpactEffect", &mut x.ImpactEffect), ("LargeImpactEffect", &mut x.LargeImpactEffect)] {
                let p = field(v, name)?.pptr().ok_or(name)?;
                if !p.is_null() {
                    let r = db.resolve(file, p).ok_or_else(|| format!("AcceleratorBoltBehaviour.{name} outside the data"))?;
                    if db.prefab(r)?.objects.iter().any(|x| x.components.iter().any(|(_, c)| matches!(c, Component::Collider2D(_) | Component::Rigidbody2D(_)))) {
                        unimplemented!("AcceleratorBoltBehaviour: an {name} with physics (its pose)");
                    }
                    *slot = Some(r);
                }
            }
            accelerator_bolts.insert(o, self.accelerator_bolts.len());
            self.accelerator_bolts.push(x);
        }
        // ArchelixCasterBehaviour: Use starts BurstShot, which requests pooled bolts (archelix.rs).
        let mut archelix_casters = HashMap::new();
        for o in 0..n {
            let Some((c, enabled, v)) = script(prefab, o, "ArchelixCasterBehaviour") else { continue };
            let mut x = super::archelix::ArchelixCaster::default();
            load_values(x.fields(), v)?;
            x.id = inst.objects[o].components[c];
            x.enabled = enabled;
            let ph = field(v, "PhysicalBehaviour")?.pptr().ok_or("ArchelixCasterBehaviour.PhysicalBehaviour")?;
            if ph.is_null() || ph.file != 0 {
                unimplemented!("ArchelixCasterBehaviour.PhysicalBehaviour null or outside the prefab (NullReferenceException)");
            }
            let (po, _) = *prefab.index.get(&ObjRef { file, path_id: ph.path_id }).ok_or("ArchelixCasterBehaviour.PhysicalBehaviour: dangling reference")?;
            x.physical = l.arenas.physical.get(&po).copied().unwrap_or_else(|| unimplemented!("ArchelixCasterBehaviour.PhysicalBehaviour on an object without a physical behaviour in this clone"));
            x.ShootSounds = array(v, "ShootSounds")?.len();
            x.MuzzleFlash = !field(v, "MuzzleFlash")?.pptr().ok_or("ArchelixCasterBehaviour.MuzzleFlash")?.is_null();
            // BarrelPosition uses the Rigidbody2D of this GameObject (the physical's body).
            let rbp = field(v, "Rigidbody")?.pptr().ok_or("ArchelixCasterBehaviour.Rigidbody")?;
            if rbp.is_null() || rbp.file != 0 || prefab.index.get(&ObjRef { file, path_id: rbp.path_id }).map(|&(ro, _)| ro) != Some(o) {
                unimplemented!("ArchelixCasterBehaviour.Rigidbody not on its own GameObject");
            }
            let proj = field(v, "projectilePrefab")?.pptr().ok_or("ArchelixCasterBehaviour.projectilePrefab")?;
            if !proj.is_null() {
                x.projectilePrefab = Some(db.resolve(file, proj).ok_or("ArchelixCasterBehaviour.projectilePrefab outside the data")?);
            }
            archelix_casters.insert(o, self.archelix_casters.len());
            self.archelix_casters.push(x);
        }
        // ArchelixCasterBoltBehaviour: a pooled bolt without physics; its trail is a TrailRenderer on a child at the bolt's origin.
        let mut archelix_bolts = HashMap::new();
        for o in 0..n {
            let Some((c, enabled, v)) = script(prefab, o, "ArchelixCasterBoltBehaviour") else { continue };
            if prefab.objects.iter().any(|x| x.components.iter().any(|(_, c)| matches!(c, Component::Collider2D(_) | Component::Rigidbody2D(_)))) {
                unimplemented!("ArchelixCasterBoltBehaviour on a prefab with physics (its pose)");
            }
            if !enabled || o != 0 {
                unimplemented!("ArchelixCasterBoltBehaviour disabled in the prefab, or not on the root");
            }
            let mut x = super::archelix::ArchelixBolt::default();
            load_values(x.fields(), v)?;
            x.id = inst.objects[o].components[c];
            x.layers = field(field(v, "Layers")?, "m_Bits")?.i64().ok_or("ArchelixCasterBoltBehaviour.Layers.m_Bits")? as u32;
            let imp = field(v, "ImpactPoolable")?.pptr().ok_or("ArchelixCasterBoltBehaviour.ImpactPoolable")?;
            if !imp.is_null() {
                x.ImpactPoolable = Some(db.resolve(file, imp).ok_or("ArchelixCasterBoltBehaviour.ImpactPoolable outside the data")?);
            }
            let tp = field(v, "TrailRendererToDetach")?.pptr().ok_or("ArchelixCasterBoltBehaviour.TrailRendererToDetach")?;
            if tp.is_null() || tp.file != 0 {
                unimplemented!("ArchelixCasterBoltBehaviour.TrailRendererToDetach null or outside the prefab (NullReferenceException)");
            }
            let (to, tc) = *prefab.index.get(&ObjRef { file, path_id: tp.path_id }).ok_or("ArchelixCasterBoltBehaviour.TrailRendererToDetach: dangling reference")?;
            let (tref, tcomp) = &prefab.objects[to].components[tc];
            if !matches!(tcomp, Component::Other(96)) {
                unimplemented!("ArchelixCasterBoltBehaviour.TrailRendererToDetach is not a TrailRenderer");
            }
            let head = crate::assets::components::TrailHead::read(db.data(*tref))?;
            if head.autodestruct || !head.emitting || !head.enabled {
                unimplemented!("ArchelixCasterBoltBehaviour: a TrailRenderer that is autodestructing, not emitting or disabled");
            }
            // The trail's points are the bolt's position: a child with an identity Transform.
            let identity = matches!(&prefab.objects[to].components[0].1, Component::Transform(t) if t.local_position == [0.0; 3] && t.local_rotation == [0.0, 0.0, 0.0, 1.0] && t.local_scale == [1.0; 3]);
            if prefab.objects[to].parent != Some(o) || !identity {
                unimplemented!("ArchelixCasterBoltBehaviour: a TrailRenderer that is not a child at the bolt's origin");
            }
            x.trail_time = head.time;
            x.trail_min_distance = head.min_vertex_distance;
            archelix_bolts.insert(o, self.archelix_bolts.len());
            self.archelix_bolts.push(x);
        }
        // UniversalBellBehaviour: its Start, FixedUpdate and Update touch the physical's body (universal_bell.rs). Its
        // clapper and clip sprites and labels are presentation, but Start dereferences them (NullReferenceException).
        let mut universal_bells = HashMap::new();
        for o in 0..n {
            let Some((c, enabled, v)) = script(prefab, o, "UniversalBellBehaviour") else { continue };
            let mut x = super::universal_bell::UniversalBell::default();
            load_values(x.fields(), v)?;
            x.id = inst.objects[o].components[c];
            x.enabled = enabled;
            x.physical = l.arenas.physical.get(&o).copied().unwrap_or_else(|| unimplemented!("UniversalBellBehaviour without a PhysicalBehaviour (NullReferenceException)"));
            for name in ["clapperSpriteRenderer", "DetailViewLabel"] {
                if field(v, name)?.pptr().ok_or(name)?.is_null() {
                    unimplemented!("UniversalBellBehaviour.{name} is null (NullReferenceException in Start)");
                }
            }
            universal_bells.insert(o, self.universal_bells.len());
            self.universal_bells.push(x);
        }
        // ToggleableMirrorBehaviour: FixedUpdate writes its physical's laser flags (mirror.rs).
        let mut toggleable_mirrors = HashMap::new();
        for o in 0..n {
            if script(prefab, o, "ToggleableMirrorBehaviour").is_none() {
                continue;
            }
            let mut x = super::mirror::ToggleableMirror::default();
            x.physical = l.arenas.physical.get(&o).copied().unwrap_or_else(|| unimplemented!("ToggleableMirrorBehaviour without a PhysicalBehaviour (NullReferenceException)"));
            toggleable_mirrors.insert(o, self.toggleable_mirrors.len());
            self.toggleable_mirrors.push(x);
        }
        // BreakJointOnBurnBehaviour / BreakJointOnShotBehaviour: their Joint, Awake's GetComponent<PhysicalBehaviour> and
        // breakForce read, OnEnable's GetComponent<Joint2D> for an unset one (breakjoint.rs).
        for o in 0..n {
            let file = prefab.objects[o].game_object.file;
            for (c, (_, comp)) in prefab.objects[o].components.iter().enumerate() {
                let Component::Script { class, fields: v, .. } = comp else { continue };
                if class != "BreakJointOnBurnBehaviour" && class != "BreakJointOnShotBehaviour" {
                    continue;
                }
                let p = field(v, "Joint")?.pptr().ok_or("Joint")?;
                let joint = if p.is_null() {
                    None
                } else {
                    assert_eq!(p.file, 0, "{class}: a joint outside its prefab");
                    let k = prefab.index[&ObjRef { file, path_id: p.path_id }];
                    let Component::Joint2D(j) = &prefab.objects[k.0].components[k.1].1 else { unimplemented!("{class}.Joint is not a Joint2D") };
                    Some((inst.objects[k.0].components[k.1], j.break_force))
                };
                let id = inst.objects[o].components[c];
                if class == "BreakJointOnBurnBehaviour" {
                    let (Joint, initialThreshold) = joint.unwrap_or_else(|| unimplemented!("BreakJointOnBurnBehaviour.Awake with a null Joint (NullReferenceException)"));
                    let phys = l.arenas.physical.get(&o).copied();
                    self.break_joint_on_burns.push(super::breakjoint::BreakJointOnBurn { id, Joint, phys, initialThreshold, randomOffset: 1.0 });
                } else {
                    let own = || prefab.objects[o].components.iter().position(|(_, x)| matches!(x, Component::Joint2D(_))).map(|k| inst.objects[o].components[k]);
                    let Joint = joint.map(|j| j.0).or_else(own);
                    let (Chance, ShotIntensityInfluence) = (float(v, "Chance")?, float(v, "ShotIntensityInfluence")?);
                    self.break_joint_on_shots.push(super::breakjoint::BreakJointOnShot { id, Joint, Chance, ShotIntensityInfluence });
                }
            }
        }
        // LiquidSpreadAoEBehaviour: its LiquidContainer and the PhysicalBehaviour whose position it spreads from.
        for o in 0..n {
            let Some((c, _, v)) = script(prefab, o, "LiquidSpreadAoEBehaviour") else { continue };
            let file = prefab.objects[o].game_object.file;
            let lc = field(v, "LiquidContainer")?.pptr().ok_or("LiquidContainer")?;
            let LiquidContainer = (!lc.is_null()).then(|| {
                assert_eq!(lc.file, 0, "LiquidSpreadAoEBehaviour.LiquidContainer outside its prefab");
                let k = prefab.index[&ObjRef { file, path_id: lc.path_id }];
                inst.objects[k.0].components[k.1]
            });
            let phys = *l.arenas.physical.get(&o).unwrap_or_else(|| unimplemented!("LiquidSpreadAoEBehaviour on an object without a PhysicalBehaviour"));
            self.liquid_spreads.push(super::liquid_spread::LiquidSpreadAoE {
                id: inst.objects[o].components[c],
                LiquidContainer,
                AmountMultiplier: float(v, "AmountMultiplier")?,
                LimbsOnly: field(v, "LimbsOnly")?.bool().ok_or("LimbsOnly")?,
                SpreadOnStart: field(v, "SpreadOnStart")?.bool().ok_or("SpreadOnStart")?,
                Range: float(v, "Range")?,
                Mask: field(field(v, "Mask")?, "m_Bits")?.i64().ok_or("Mask.m_Bits")? as u32,
                phys,
            });
        }
        // FancyBloodSplatController in an instantiated prefab: its fields, decal, and initialParent's position and
        // its own rotation (neither moves before Start). Under a physical it moves with it when the initialParent is
        // that physical's object or one of its direct children (the bleeding-particle pose; update_splat_pose).
        let mut splat_by_object = HashMap::new();
        for o in 0..n {
            let Some((c, _, v)) = script(prefab, o, "FancyBloodSplatController") else { continue };
            let file = prefab.objects[o].game_object.file;
            let mut x = super::misc::FancyBloodSplat::default();
            load_values(x.fields(), v)?;
            let dp = field(v, "Decal")?.pptr().ok_or("Decal")?;
            let decal = db.resolve(file, dp).unwrap_or_else(|| unimplemented!("FancyBloodSplatController.Decal null or outside the data"));
            let k = self.load_decal_descriptor(db, decal)?;
            x.Decal = self.decal_descriptors[k].name.clone();
            x.CollisionLayers = field(field(v, "CollisionLayers")?, "m_Bits")?.i64().ok_or("CollisionLayers.m_Bits")? as u32;
            let parent = inst.objects[o].parent.unwrap_or_else(|| unimplemented!("FancyBloodSplatController without a parent (initialParent.position: NullReferenceException)"));
            // The nearest physical above the splat, and no transform node or body above it.
            let mut nearest = None;
            let mut up = Some(o);
            while let Some(t) = up {
                if l.arenas.node.contains_key(&t) {
                    unimplemented!("FancyBloodSplatController under a Transform node or body");
                }
                if let Some(&p) = l.arenas.physical.get(&t) {
                    if t == o {
                        unimplemented!("FancyBloodSplatController on a physical object");
                    }
                    nearest.get_or_insert((t, p));
                }
                up = inst.objects[t].parent;
            }
            // Attached when the initialParent is the physical's object (identity pose) or its direct child (its local
            // TRS). Deeper, the initialParent's pose is not modelled.
            x.attached = match nearest {
                None => None,
                Some((t, p)) if t == parent => Some((p, crate::unity::transform::Trs { t: [0.0; 4], q: [0.0, 0.0, 0.0, 1.0], s: [1.0, 1.0, 1.0, 0.0] }, inst.objects[o].trs.q)),
                Some((t, p)) if inst.objects[parent].parent == Some(t) => Some((p, inst.objects[parent].trs, inst.objects[o].trs.q)),
                Some(_) => unimplemented!("FancyBloodSplatController more than one Transform below its physical (its initialParent's pose is not modelled)"),
            };
            x.parent_position = inst.position(parent);
            x.rotation = inst.rotation(o);
            x.started = false;
            x.id = inst.objects[o].components[c];
            splat_by_object.insert(o, self.splats.len());
            self.splats.push(x);
        }
        // BloodTankBehaviour: its PhysicalParticles splat calls its OnCollide listener (blood_tank.rs).
        let mut blood_tanks = HashMap::new();
        for o in 0..n {
            let Some((c, enabled, v)) = script(prefab, o, "BloodTankBehaviour") else { continue };
            let file = prefab.objects[o].game_object.file;
            let mut x = super::blood_tank::BloodTank::default();
            load_values(x.fields(), v)?;
            x.id = inst.objects[o].components[c];
            x.enabled = enabled;
            x.physical = *l.arenas.physical.get(&o).unwrap_or_else(|| unimplemented!("BloodTankBehaviour without a PhysicalBehaviour (NullReferenceException in Awake)"));
            // PhysicalParticles: the FancyBloodSplatController of this prefab (Awake's OnCollide subscription on it).
            let pp = field(v, "PhysicalParticles")?.pptr().filter(|p| !p.is_null() && p.file == 0).unwrap_or_else(|| unimplemented!("BloodTankBehaviour.PhysicalParticles null or outside its prefab (NullReferenceException in Awake)"));
            let (po, _) = *prefab.index.get(&ObjRef { file, path_id: pp.path_id }).ok_or("BloodTankBehaviour.PhysicalParticles: dangling reference")?;
            x.splat = *splat_by_object.get(&po).unwrap_or_else(|| unimplemented!("BloodTankBehaviour.PhysicalParticles is not a FancyBloodSplatController of this prefab"));
            // WaterPuffParticles: its Transform (the first component of its object) decides whether the puff is submerged.
            let wp = field(v, "WaterPuffParticles")?.pptr().filter(|p| !p.is_null() && p.file == 0).unwrap_or_else(|| unimplemented!("BloodTankBehaviour.WaterPuffParticles null or outside its prefab (NullReferenceException in UpdateActivation)"));
            let (wo, _) = *prefab.index.get(&ObjRef { file, path_id: wp.path_id }).ok_or("BloodTankBehaviour.WaterPuffParticles: dangling reference")?;
            x.water_puff = inst.objects[wo].components[0];
            self.splats[x.splat].on_collide_tank = Some(self.blood_tanks.len());
            blood_tanks.insert(o, self.blood_tanks.len());
            self.blood_tanks.push(x);
        }
        // GorseDroolingBehaviour: its LimbBehaviour, Projectile and where its Transform hangs.
        for o in 0..n {
            let Some((c, _, v)) = script(prefab, o, "GorseDroolingBehaviour") else { continue };
            let file = prefab.objects[o].game_object.file;
            let lp = field(v, "LimbBehaviour")?.pptr().ok_or("LimbBehaviour")?;
            if lp.is_null() || lp.file != 0 {
                unimplemented!("GorseDroolingBehaviour.LimbBehaviour null or outside its prefab");
            }
            let lo = prefab.index[&ObjRef { file, path_id: lp.path_id }].0;
            let limb = *l.arenas.limb.get(&lo).unwrap_or_else(|| unimplemented!("GorseDroolingBehaviour.LimbBehaviour not a linked limb"));
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
            let proj = field(v, "Projectile")?.pptr().ok_or("Projectile")?;
            let Projectile = if proj.is_null() { None } else { Some(db.resolve(file, proj).ok_or("GorseDroolingBehaviour.Projectile outside the data")?) };
            self.gorse_droolings.push(super::gorse::GorseDrooling {
                id: inst.objects[o].components[c],
                limb,
                anchor,
                chain,
                CanSee: field(field(v, "CanSee")?, "m_Bits")?.i64().ok_or("CanSee.m_Bits")? as u32,
                Projectile,
                FireClips: array(v, "FireClip")?.len(),
                Range: float(v, "Range")?,
                Force: float(v, "Force")?,
                step: 0,
                isAlive: false,
            });
        }
        // GorseProjectileBehaviour: its own colliders, body and DisperseEffect.
        for o in 0..n {
            let Some((c, _, v)) = script(prefab, o, "GorseProjectileBehaviour") else { continue };
            let file = prefab.objects[o].game_object.file;
            let node_body = *node_rigidbodies.get(&o).unwrap_or_else(|| unimplemented!("GorseProjectileBehaviour without a bare Rigidbody2D on its GameObject"));
            let effect = field(v, "DisperseEffect")?.pptr().ok_or("DisperseEffect")?;
            let DisperseEffect = if effect.is_null() { None } else { Some(db.resolve(file, effect).ok_or("GorseProjectileBehaviour.DisperseEffect outside the data")?) };
            let colliders = prefab.objects[o].components.iter().enumerate().filter(|(_, (_, comp))| matches!(comp, Component::Collider2D(_))).map(|(k, _)| inst.objects[o].components[k]).collect();
            self.gorse_projectiles.push(super::gorse::GorseProjectile {
                id: inst.objects[o].components[c],
                Damage: float(v, "Damage")?,
                DisperseEffect,
                AcidChance: float(v, "AcidChance")?,
                colliders,
                node_body,
            });
        }
        // SoundOnShakeBehaviour: Awake's GetComponent<Rigidbody2D>, its AudioSource and the lengths of Clips (shake.rs).
        for o in 0..n {
            let Some((c, _, v)) = script(prefab, o, "SoundOnShakeBehaviour") else { continue };
            let file = prefab.objects[o].game_object.file;
            let rb = prefab.objects[o].components.iter().position(|(_, x)| matches!(x, Component::Rigidbody2D(_))).map(|k| inst.objects[o].components[k]);
            let rb = rb.unwrap_or_else(|| unimplemented!("SoundOnShakeBehaviour without a Rigidbody2D (NullReferenceException in FixedUpdate)"));
            let a = field(v, "AudioSource")?.pptr().ok_or("AudioSource")?;
            if a.is_null() || a.file != 0 {
                unimplemented!("SoundOnShakeBehaviour.AudioSource null or outside its prefab");
            }
            let source = ObjRef { file, path_id: a.path_id };
            // AudioSource.m_Enabled, after m_GameObject: a disabled one never plays.
            if db.data(source)[12] == 0 {
                unimplemented!("SoundOnShakeBehaviour with a disabled AudioSource");
            }
            let k = prefab.index[&source];
            let mut up = Some(k.0);
            let mut time_scaled = false;
            while let Some(t) = up {
                if script(prefab, t, "AudioSourceTimeScaleBehaviour").is_some() {
                    time_scaled = true;
                    break;
                }
                up = inst.objects[t].parent;
            }
            let mut clips = Vec::new();
            for x in array(v, "Clips")? {
                let r = db.resolve(file, x.pptr().ok_or("Clips")?).unwrap_or_else(|| unimplemented!("SoundOnShakeBehaviour: a null AudioClip"));
                clips.push(crate::assets::classes::AudioClip::read(db.data(r))?.length);
            }
            self.sound_on_shakes.push(super::shake::SoundOnShake {
                id: inst.objects[o].components[c],
                rb,
                AudioSource: inst.objects[k.0].components[k.1],
                Clips: clips,
                ShakeThreshold: float(v, "ShakeThreshold")?,
                previousVelocity: [0.0, 0.0],
                root: inst.objects[0].components[0],
                time_scaled,
            });
        }
        // FoliageBehaviour: Awake's GetComponentInParent<PhysicalBehaviour>, and its own SpriteRenderer (foliage.rs).
        for o in 0..n {
            let Some((c, enabled, v)) = script(prefab, o, "FoliageBehaviour") else { continue };
            if !enabled {
                unimplemented!("a disabled FoliageBehaviour (OnWillRenderObject still reaches it)");
            }
            let mut x = super::foliage::Foliage::default();
            load_values(x.fields(), v)?;
            x.id = inst.objects[o].components[c];
            if x.SyncWithParentPhysicalBehaviour {
                let mut up = Some(o);
                while let Some(t) = up {
                    if let Some(&p) = l.arenas.physical.get(&t) {
                        x.phys = Some(p);
                        break;
                    }
                    up = inst.objects[t].parent;
                }
            }
            let k = prefab.objects[o].components.iter().position(|(_, x)| matches!(x, Component::SpriteRenderer(_)));
            let k = k.unwrap_or_else(|| unimplemented!("FoliageBehaviour without a SpriteRenderer (NullReferenceException in Awake)"));
            let rid = inst.objects[o].components[k];
            x.renderer = self.renderers.iter().position(|r| r.id == rid).unwrap_or_else(|| unimplemented!("FoliageBehaviour: a SpriteRenderer with a null sprite"));
            self.foliages.push(x);
        }
        // WrenchBehaviour: OnCollisionEnter2D reaches it through its physical (wrench.rs); no Awake, Update or Start.
        for o in 0..n {
            let Some((c, enabled, v)) = script(prefab, o, "WrenchBehaviour") else { continue };
            let mut x = super::wrench::Wrench::default();
            load_values(x.fields(), v)?;
            x.id = inst.objects[o].components[c];
            x.enabled = enabled;
            x.physical = l.arenas.physical.get(&o).copied().unwrap_or_else(|| unimplemented!("WrenchBehaviour without a PhysicalBehaviour (its collisions are not delivered)"));
            x.index = c;
            x.after_physical = script(prefab, o, "PhysicalBehaviour").is_some_and(|(pc, _, _)| pc < c);
            self.wrenches.push(x);
        }
        // NoCollideWithBodyPart: its collider's collisions with a body part's limb are ignored (no_collide.rs).
        for o in 0..n {
            let Some((c, enabled, v)) = script(prefab, o, "NoCollideWithBodyPart") else { continue };
            let mut x = super::no_collide::NoCollideWithBodyPart::default();
            load_values(x.fields(), v)?;
            x.enabled = enabled;
            x.physical = l.arenas.physical.get(&o).copied().unwrap_or_else(|| unimplemented!("NoCollideWithBodyPart without a PhysicalBehaviour (its collisions are not delivered)"));
            x.index = c;
            x.after_physical = script(prefab, o, "PhysicalBehaviour").is_some_and(|(pc, _, _)| pc < c);
            // A null Collider never equals the receiver's own collider, so its callback does nothing (Collider stays 0).
            if let Some(p) = field(v, "Collider")?.pptr().filter(|p| !p.is_null()) {
                let (co, cc) = *prefab.index.get(&ObjRef { file, path_id: p.path_id }).filter(|_| p.file == 0).ok_or("NoCollideWithBodyPart.Collider outside the prefab")?;
                x.Collider = inst.objects[co].components[cc];
            }
            self.no_collide_body_parts.push(x);
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
        // UseEventTrigger.Use invokes Event (nothing sets its Action).
        for o in 0..n {
            if let Some((c, enabled, v)) = script(prefab, o, "UseEventTrigger") {
                let calls = event_calls(prefab, file, field(v, "Event")?, inst, "UseEventTrigger.Event", true)?;
                let only_once = field(v, "OnlyOnce")?.bool().ok_or("OnlyOnce")?;
                self.use_events.insert(inst.objects[o].components[c], UseEvent { enabled, only_once, use_count: 0, calls });
            }
        }
        let mut android_sparks = HashMap::new();
        let (mut act_on_collides, mut point_to_velocities, mut cars, mut tyres, mut flasks, mut truck_doors) = (HashMap::new(), HashMap::new(), HashMap::new(), HashMap::new(), HashMap::new(), HashMap::new());
        let (mut glowtubes, mut leds, mut detectors, mut sticky_grenades) = (HashMap::new(), HashMap::new(), HashMap::new(), HashMap::new());
        let mut liquid_containers = HashMap::new();
        let (mut syringes, mut syringe_explosions) = (HashMap::new(), HashMap::new());
        let mut valves = HashMap::new();
        let mut timed_gates = HashMap::new();
        let (mut defibrillators, mut cardiopulmonary_bypasses) = (HashMap::new(), HashMap::new());
        let mut heart_monitors = HashMap::new();
        let mut excaliburs = HashMap::new();
        let mut sharp_on_all_sides = HashMap::new();
        let mut point_liquids = HashMap::new();
        for o in 0..n {
            if let Some((c, _, v)) = script(prefab, o, "LiquidContainerController") {
                let mut x = super::flask::LiquidContainer::default();
                (x.id, x.FillPercentage) = (inst.objects[o].components[c], float(v, "FillPercentage")?);
                let b = field(v, "Container")?;
                let (center, extent) = (field(b, "m_Center")?, field(b, "m_Extent")?);
                // Bounds.min and max: center -/+ extents.
                (x.min, x.max) = ([float(center, "x")? - float(extent, "x")?, float(center, "y")? - float(extent, "y")?], [float(center, "x")? + float(extent, "x")?, float(center, "y")? + float(extent, "y")?]);
                x.center = [float(center, "x")?, float(center, "y")?, float(center, "z")?];
                x.extents = [float(extent, "x")?, float(extent, "y")?, float(extent, "z")?];
                // base.transform: the Transforms down from the nearest physical or node.
                (x.anchor, x.chain) = anchor_of(o);
                // Rigidbody (a Rigidbody2D of this prefab; null stops SimulateLiquid).
                if let Some(p) = field(v, "Rigidbody")?.pptr().filter(|p| !p.is_null()) {
                    let (ro, rc) = *prefab.index.get(&ObjRef { file, path_id: p.path_id }).filter(|_| p.file == 0).ok_or("LiquidContainerController.Rigidbody outside the prefab")?;
                    x.Rigidbody = Some(inst.objects[ro].components[rc]);
                }
                liquid_containers.insert(o, self.liquid_containers.len());
                self.liquid_containers.push(x);
            }
        }
        // The sci-fi weapons the ActOnCollide and ActOnShot Actions call.
        let (mut blasters, mut blasterbolts, mut beamformers, mut scifi40s) = (HashMap::new(), HashMap::new(), HashMap::new(), HashMap::new());
        let mut effect_guns = HashMap::new();
        for o in 0..n {
            if let Some((c, enabled, v)) = script(prefab, o, "BlasterBehaviour") {
                let mut x = super::blaster::Blaster::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                x.physical = l.arenas.physical.get(&o).copied().unwrap_or_else(|| unimplemented!("BlasterBehaviour without a PhysicalBehaviour (NullReferenceException)"));
                let bolt = field(v, "Bolt")?.pptr().ok_or("BlasterBehaviour.Bolt")?;
                if !bolt.is_null() {
                    let r = db.resolve(file, bolt).ok_or("BlasterBehaviour.Bolt outside the data")?;
                    // The bolt's pose is the script's: Transform.right and position on a bare Transform.
                    if db.prefab(r)?.objects.iter().any(|x| x.components.iter().any(|(_, c)| matches!(c, Component::Collider2D(_) | Component::Rigidbody2D(_)))) {
                        unimplemented!("BlasterBehaviour: a Bolt with physics (its pose)");
                    }
                    x.Bolt = Some(r);
                }
                x.Clips = array(v, "Clips")?.len();
                x.ScreenShakeMultiplier = float(v, "ScreenShakeMultiplier")?;
                blasters.insert(o, self.blasters.len());
                self.blasters.push(x);
            }
            if let Some((c, _, v)) = script(prefab, o, "BlasterboltBehaviour") {
                let mut x = super::blaster::Blasterbolt::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.layers = field(field(v, "layers")?, "m_Bits")?.i64().ok_or("BlasterboltBehaviour.layers.m_Bits")? as u32;
                for (name, slot) in [("ImpactEffect", &mut x.ImpactEffect), ("HitDecal", &mut x.HitDecal)] {
                    let p = field(v, name)?.pptr().ok_or(name)?;
                    if !p.is_null() {
                        let r = db.resolve(file, p).ok_or_else(|| format!("BlasterboltBehaviour.{name} outside the data"))?;
                        if db.prefab(r)?.objects.iter().any(|x| x.components.iter().any(|(_, c)| matches!(c, Component::Collider2D(_) | Component::Rigidbody2D(_)))) {
                            unimplemented!("BlasterboltBehaviour: an {name} with physics (its pose)");
                        }
                        *slot = Some(r);
                    }
                }
                let decal = field(v, "ImpactDecal")?.pptr().ok_or("BlasterboltBehaviour.ImpactDecal")?;
                if !decal.is_null() {
                    let r = db.resolve(file, decal).ok_or("BlasterboltBehaviour.ImpactDecal outside the data")?;
                    x.ImpactDecal = Some(self.load_decal_descriptor(db, r)?);
                }
                if prefab.objects.iter().any(|x| x.components.iter().any(|(_, c)| matches!(c, Component::Collider2D(_) | Component::Rigidbody2D(_)))) {
                    unimplemented!("BlasterboltBehaviour on a prefab with physics (its pose)");
                }
                blasterbolts.insert(o, self.blasterbolts.len());
                self.blasterbolts.push(x);
            }
            if let Some((c, enabled, v)) = script(prefab, o, "BeamformerBehaviour") {
                let mut x = super::scifi::Beamformer::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                x.phys = l.arenas.physical.get(&o).copied().unwrap_or_else(|| unimplemented!("BeamformerBehaviour without a PhysicalBehaviour (NullReferenceException)"));
                x.LayersToHit = field(field(v, "LayersToHit")?, "m_Bits")?.i64().ok_or("BeamformerBehaviour.LayersToHit.m_Bits")? as i32;
                // Start: BeamLineRenderer.gameObject.SetActive(false) must only hide a line.
                let line = field(v, "BeamLineRenderer")?.pptr().ok_or("BeamformerBehaviour.BeamLineRenderer")?;
                let (lo, _) = *prefab.index.get(&ObjRef { file, path_id: line.path_id }).filter(|_| line.file == 0 && !line.is_null()).ok_or("BeamformerBehaviour.BeamLineRenderer outside the prefab")?;
                if std::iter::once(lo).chain(inst.descendants(lo)).any(|d| prefab.objects[d].components.iter().any(|(_, c)| matches!(c, Component::Collider2D(_) | Component::Rigidbody2D(_)) || matches!(c, Component::Script { class, .. } if super::declares(class, "OnEnable") || super::declares(class, "OnDisable")))) {
                    unimplemented!("BeamformerBehaviour.BeamLineRenderer on a GameObject with physics or an OnEnable/OnDisable script");
                }
                beamformers.insert(o, self.beamformers.len());
                self.beamformers.push(x);
            }
            if let Some((c, enabled, v)) = script(prefab, o, "GenericScifiWeapon40Behaviour") {
                let mut x = super::scifi::Scifi40::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                x.physical = l.arenas.physical.get(&o).copied().unwrap_or_else(|| unimplemented!("GenericScifiWeapon40Behaviour without a PhysicalBehaviour (NullReferenceException)"));
                x.LayersToHit = field(field(v, "LayersToHit")?, "m_Bits")?.i64().ok_or("GenericScifiWeapon40Behaviour.LayersToHit.m_Bits")? as u32;
                // Rigidbody is the physical's own Rigidbody2D.
                let rb = field(v, "Rigidbody")?.pptr().ok_or("GenericScifiWeapon40Behaviour.Rigidbody")?;
                let own = prefab.index.get(&ObjRef { file, path_id: rb.path_id }).filter(|_| rb.file == 0 && !rb.is_null()).is_some_and(|&(ro, rc)| ro == o && matches!(prefab.objects[ro].components[rc].1, Component::Rigidbody2D(_)));
                if !own {
                    unimplemented!("GenericScifiWeapon40Behaviour.Rigidbody that is not the Rigidbody2D of its own GameObject");
                }
                let impact = field(v, "Impact")?.pptr().ok_or("GenericScifiWeapon40Behaviour.Impact")?;
                if !impact.is_null() {
                    let r = db.resolve(file, impact).ok_or("GenericScifiWeapon40Behaviour.Impact outside the data")?;
                    if db.prefab(r)?.objects.iter().any(|x| x.components.iter().any(|(_, c)| matches!(c, Component::Collider2D(_) | Component::Rigidbody2D(_)))) {
                        unimplemented!("GenericScifiWeapon40Behaviour: an Impact with physics (its pose)");
                    }
                    x.Impact = Some(r);
                }
                scifi40s.insert(o, self.scifi40s.len());
                self.scifi40s.push(x);
            }
            for class in ["HealthGunBehaviour", "FreezeGunBehaviour"] {
                let Some((c, enabled, v)) = script(prefab, o, class) else { continue };
                let mut x = super::effect_gun::EffectGun::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                x.freeze = class == "FreezeGunBehaviour";
                // PhysicalBehaviour is [SkipSerialisation]: only read by the base class through transform here.
                x.physical = l.arenas.physical.get(&o).copied().unwrap_or_else(|| unimplemented!("{class} without a PhysicalBehaviour on its GameObject"));
                let flash = field(v, "Muzzleflash")?.pptr().ok_or("EffectGunBehaviour.Muzzleflash")?;
                if !flash.is_null() {
                    let r = db.resolve(file, flash).ok_or("EffectGunBehaviour.Muzzleflash outside the data")?;
                    if db.prefab(r)?.objects.iter().any(|x| x.components.iter().any(|(_, c)| matches!(c, Component::Collider2D(_) | Component::Rigidbody2D(_)))) {
                        unimplemented!("EffectGunBehaviour: a Muzzleflash with physics (its pose)");
                    }
                    x.Muzzleflash = Some(r);
                }
                x.ShotSounds = array(v, "ShotSounds")?.len();
                effect_guns.insert(o, self.effect_guns.len());
                self.effect_guns.push(x);
            }
        }
        // A UnityEvent of persistent calls: ExplosiveBehaviour.Activate (the mine), FirearmBehaviour.Shoot,
        // ProjectileLauncherBehaviour.Launch, DamagableMachineryBehaviour.ForceBreak, BlasterBehaviour.Shoot,
        // BeamformerBehaviour.Shoot, GenericScifiWeapon40Behaviour.Fire and PhysicalBehaviour.ForceSendUse, audio and particles.
        // `machinery` is `Game::machinery.len()` before this clone's DamagableMachineryBehaviours.
        let act_targets = |actions: &Value, machinery: usize, what: &str| -> Result<Vec<super::acts::ActTarget>, String> {
            use super::acts::ActTarget;
            let mut targets = Vec::new();
            for call in invoked_calls(actions) {
                let method = string(call, "m_MethodName")?;
                let class_of = |to: usize, tc: usize| match &prefab.objects[to].components[tc].1 {
                    Component::Script { class, .. } => class.as_str(),
                    _ => "",
                };
                match call_target(prefab, file, call) {
                    Some((to, tc)) if class_of(to, tc) == "ExplosiveBehaviour" && method == "Activate" => targets.push(ActTarget::Activate(explosives[&to])),
                    Some((to, tc)) if class_of(to, tc) == "FirearmBehaviour" && method == "Shoot" => targets.push(ActTarget::Shoot(firearms[&to])),
                    Some((to, tc)) if class_of(to, tc) == "ProjectileLauncherBehaviour" && method == "Launch" => targets.push(ActTarget::Launch(launchers[&to])),
                    Some((to, tc)) if class_of(to, tc) == "DamagableMachineryBehaviour" && method == "ForceBreak" => {
                        // The machinery loop below pushes this clone's DamagableMachineryBehaviours in object order.
                        let before = (0..to).filter(|&x| script(prefab, x, "DamagableMachineryBehaviour").is_some()).count();
                        targets.push(ActTarget::ForceBreak(machinery + before));
                    }
                    Some((to, tc)) if class_of(to, tc) == "BlasterBehaviour" && method == "Shoot" => targets.push(ActTarget::Blast(blasters[&to])),
                    Some((to, tc)) if class_of(to, tc) == "BeamformerBehaviour" && method == "Shoot" => targets.push(ActTarget::Beam(beamformers[&to])),
                    Some((to, tc)) if class_of(to, tc) == "GenericScifiWeapon40Behaviour" && method == "Fire" => targets.push(ActTarget::Fire(scifi40s[&to])),
                    Some((to, tc)) if class_of(to, tc) == "PhysicalBehaviour" && method == "ForceSendUse" => targets.push(ActTarget::ForceSendUse(l.arenas.physical[&to])),
                    // Ignite(bool): the call's bool argument is ignoreFlammability (a UnityEvent's BoolArgument is 0 or 1).
                    Some((to, tc)) if class_of(to, tc) == "PhysicalBehaviour" && method == "Ignite" => {
                        let ignore = call.get("m_Arguments").and_then(|a| a.get("m_BoolArgument")).is_some_and(|b| b.bool() == Some(true) || b.i64() == Some(1));
                        targets.push(ActTarget::Ignite(l.arenas.physical[&to], ignore));
                    }
                    // The CarBehaviour calls car_calls keeps (the same argument reads).
                    Some((to, tc)) if class_of(to, tc) == "CarBehaviour" && method == "SetHealth" => {
                        let h = call.get("m_Arguments").and_then(|a| a.get("m_FloatArgument")).and_then(Value::f32).ok_or("CarBehaviour.SetHealth without its float argument")?;
                        targets.push(ActTarget::Car(inst.objects[to].components[tc], super::car::CarCall::SetHealth(h)));
                    }
                    Some((to, tc)) if class_of(to, tc) == "CarBehaviour" && method == "set_enabled" && call.get("m_Arguments").and_then(|a| a.get("m_BoolArgument")).is_some_and(|b| b.bool() == Some(false) || b.i64() == Some(0)) => {
                        targets.push(ActTarget::Car(inst.objects[to].components[tc], super::car::CarCall::Disable));
                    }
                    _ if presentation_call(prefab, file, call) => {}
                    _ => unimplemented!("{what}.Actions: a persistent call beyond ExplosiveBehaviour.Activate, FirearmBehaviour.Shoot, ProjectileLauncherBehaviour.Launch, DamagableMachineryBehaviour.ForceBreak, BlasterBehaviour.Shoot, BeamformerBehaviour.Shoot, GenericScifiWeapon40Behaviour.Fire, PhysicalBehaviour.ForceSendUse, audio and particles ({method})"),
                }
            }
            Ok(targets)
        };
        let (mut buttons, mut displays, mut lasers) = (HashMap::new(), HashMap::new(), HashMap::new());
        let mut firearm_attachments = HashMap::new();
        let mut attachment_points: HashMap<usize, usize> = HashMap::new();
        let mut televisions = HashMap::new();
        for o in 0..n {
            if let Some((c, _, v)) = script(prefab, o, "ButtonBehaviour") {
                let mut x = super::button::Button::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                // FixedUpdate's press sends and the continuous listener's sends reach this object's receivers only.
                if prefab.objects[o].components.iter().any(|(_, comp)| matches!(comp, Component::Script { class, .. } if class == "UseWireBehaviour" || super::translated::receives(class, "IsolatedActivation") || super::translated::receives(class, "IsolatedContinuousActivation"))) {
                    unimplemented!("ButtonBehaviour: IsolatedActivation or IsolatedContinuousActivation to a receiver on its own object");
                }
                let file = prefab.objects[o].game_object.file;
                let slider = field(v, "slider")?.pptr().filter(|p| !p.is_null()).unwrap_or_else(|| unimplemented!("ButtonBehaviour.slider is null (NullReferenceException)"));
                assert_eq!(slider.file, 0, "ButtonBehaviour.slider outside its prefab");
                let (so, sc) = prefab.index[&ObjRef { file, path_id: slider.path_id }];
                if !matches!(&prefab.objects[so].components[sc].1, Component::Joint2D(j) if matches!(j.kind, JointKind::Slider { .. })) {
                    unimplemented!("ButtonBehaviour.slider is not a SliderJoint2D");
                }
                (x.slider_object, x.slider_component) = (so, sc);
                buttons.insert(o, self.buttons.len());
                self.buttons.push(x);
            }
            if let Some((c, enabled, v)) = script(prefab, o, "DisplayBehaviour") {
                let mut x = super::display::Display::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                x.Value = string(v, "Value")?;
                x.randomStartStrings = array(v, "randomStartStrings")?
                    .iter()
                    .map(|s| match s {
                        Value::Str(s) => Ok(s.clone()),
                        _ => Err("DisplayBehaviour.randomStartStrings: not a string".to_string()),
                    })
                    .collect::<Result<_, String>>()?;
                // The Edit text dialog's DontMindMe events: audio is presentation, and a ForceBreak runs through act_targets.
                x.DontMindMe = array(v, "DontMindMe")?
                    .iter()
                    .map(|e| Ok(super::display::EasterEgg { Match: string(e, "Match")?, Event: act_targets(field(e, "Event")?, self.machinery.len(), "DisplayBehaviour.DontMindMe")? }))
                    .collect::<Result<_, String>>()?;
                displays.insert(o, self.displays.len());
                self.displays.push(x);
            }
            if let Some((c, enabled, v)) = script(prefab, o, "LaserBehaviour") {
                let mut x = super::laser::Laser::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                x.physical = l.in_arena(field(v, "PhysicalBehaviour")?, &l.arenas.physical, "PhysicalBehaviour")?.ok_or("LaserBehaviour.PhysicalBehaviour is null")?;
                x.LayersToHit = field(field(v, "LayersToHit")?, "m_Bits")?.i64().ok_or("LaserBehaviour.LayersToHit.m_Bits")? as u32;
                lasers.insert(o, self.lasers.len());
                self.lasers.push(x);
            }
            if let Some((c, enabled, v)) = script(prefab, o, "TelevisionBehaviour") {
                let mut x = super::television::Television::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                x.physical = *l.arenas.physical.get(&o).ok_or("TelevisionBehaviour without a PhysicalBehaviour (GetComponent is null)")?;
                x.index = c;
                x.after_physical = script(prefab, o, "PhysicalBehaviour").is_some_and(|(pc, _, _)| pc < c);
                x.break_sounds = array(v, "BreakSounds")?.len();
                televisions.insert(o, self.televisions.len());
                self.televisions.push(x);
            }
            for class in super::attachment::CLASSES {
                if let Some((c, enabled, v)) = script(prefab, o, class) {
                    let mut x = super::attachment::FirearmAttachment::default();
                    load_values(x.fields(), v)?;
                    x.id = inst.objects[o].components[c];
                    x.enabled = enabled;
                    x.physical = l.arenas.physical.get(&o).copied();
                    x.laser = lasers.get(&o).copied();
                    let int = |name: &str| field(v, name)?.i64().map(|x| x as i32).ok_or_else(|| format!("{name}: not an integer"));
                    (x.kind, ()) = match class {
                        "ScopeAttachmentBehaviour" => {
                            x.AccuracyPercent = int("AccuracyPercent")?;
                            (super::attachment::Kind::Scope, ())
                        }
                        "BatteryAttachmentBehaviour" => {
                            x.BaseCharge = float(v, "BaseCharge")?;
                            (super::attachment::Kind::Battery, ())
                        }
                        "ExplosiveAttachmentBehaviour" => {
                            x.ExplosionParams = explosion_parameters(field(v, "ExplosionParams")?)?;
                            (super::attachment::Kind::Explosive, ())
                        }
                        "FlashlightAttachmentBehaviour" => (super::attachment::Kind::Flashlight, ()),
                        _ => {
                            x.FireChance = int("FireChance")?;
                            (super::attachment::Kind::Incendiary, ())
                        }
                    };
                    // GetLocalJointAnchor reads the SpriteRenderer's sprite rect height.
                    let renderer = prefab.objects[o].components.iter().find_map(|(_, x)| match x {
                        Component::SpriteRenderer(r) => Some(r),
                        _ => None,
                    });
                    if let Some(r) = renderer {
                        let sprite = db.resolve(file, r.sprite).ok_or("a SpriteRenderer without a sprite")?;
                        x.sprite_height = Sprite::read(db.data(sprite))?.rect[3];
                    }
                    firearm_attachments.insert(o, self.firearm_attachments.len());
                    self.firearm_attachments.push(x);
                }
            }
            if let Some((c, _, v)) = script(prefab, o, "FirearmAttachmentPointBehaviour") {
                // Start: firearmBehaviour = transform.parent.GetComponent<FirearmBehaviour>().
                let firearm = prefab.objects[o].parent.and_then(|t| firearms.get(&t).copied());
                let colliders = prefab.objects[o].components.iter().enumerate().filter(|(_, (_, x))| matches!(x, Component::Collider2D(_))).map(|(k, _)| inst.objects[o].components[k]).collect();
                attachment_points.insert(o, self.attachment_points.len());
                self.attachment_points.push(super::attachment::AttachmentPoint { id: inst.objects[o].components[c], colliders, AttachmentType: field(v, "AttachmentType")?.i64().ok_or("AttachmentType")? as i32, firearm, ..Default::default() });
            }
            if let Some((c, enabled, v)) = script(prefab, o, "ActOnShot") {
                let mut x = super::acts::ActOnShot::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                x.targets = act_targets(field(v, "Actions")?, self.machinery.len(), "ActOnShot")?;
                self.act_on_shots.push(x);
            }
            if let Some((c, enabled, v)) = script(prefab, o, "ActOnCollide") {
                let mut x = super::acts::ActOnCollide::default();
                load_values(x.fields(), v)?;
                (x.enabled, x.physical, x.index) = (enabled, l.arenas.physical.get(&o).copied(), c);
                x.after_physical = script(prefab, o, "PhysicalBehaviour").is_some_and(|(pc, _, _)| pc < c);
                if let Some(p) = field(v, "Collider")?.pptr().filter(|p| !p.is_null()) {
                    let (co, cc) = *prefab.index.get(&ObjRef { file, path_id: p.path_id }).filter(|_| p.file == 0).ok_or("ActOnCollide.Collider outside the prefab")?;
                    x.Collider = Some(inst.objects[co].components[cc]);
                }
                x.targets = act_targets(field(v, "Actions")?, self.machinery.len(), "ActOnCollide")?;
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
            if let Some((_, _, sv)) = script(prefab, o, "AndroidSparkCreator") {
                let mut x = super::acts::AndroidSparkCreator::default();
                x.limb = *l.arenas.limb.get(&o).unwrap_or_else(|| unimplemented!("AndroidSparkCreator without a LimbBehaviour (NullReferenceException in FixedUpdate)"));
                // spark: the GameObject Instantiate copies in FixedUpdate (a null one is an ArgumentException there).
                let spark = field(sv, "spark")?.pptr().ok_or("AndroidSparkCreator.spark")?;
                if !spark.is_null() {
                    x.spark = Some(db.resolve(file, spark).ok_or("AndroidSparkCreator.spark outside the data")?);
                }
                android_sparks.insert(o, self.android_sparks.len());
                self.android_sparks.push(x);
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
                x.Phys = l.arenas.physical.get(&o).copied();
                flasks.insert(o, self.flasks.len());
                self.flasks.push(x);
            }
            if let Some((class, (c, _, v))) = super::syringe::CLASSES.iter().find_map(|k| script(prefab, o, k).map(|x| (*k, x))) {
                let mut x = super::syringe::Syringe::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                if !presentation_event(prefab, file, field(v, "OnDistributionChange")?) {
                    unimplemented!("BloodContainer.OnDistributionChange: a persistent call beyond audio and particles");
                }
                x.Phys = *l.arenas.physical.get(&o).ok_or("SyringeBehaviour without a PhysicalBehaviour")?;
                x.liquid = super::syringe::liquid_id(class).map(super::circulation::liquid);
                x.pink = class == "PinkSyringe";
                // SendMessage("Lodged"/"Dislodged") from a stab (stab.rs) finds the receiver by this class.
                self.physicals[x.Phys].lodge_receiver = class.to_string();
                if x.pink {
                    super::Field::set(&mut x.Fingerprint, field(v, "Fingerprint")?)?;
                }
                syringes.insert(o, self.syringes.len());
                self.syringes.push(x);
            }
            if let Some((c, enabled, v)) = script(prefab, o, "ValveBehaviour") {
                let mut x = super::valve::Valve::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                x.physical = l.arenas.physical.get(&o).copied().unwrap_or_else(|| unimplemented!("ValveBehaviour without a PhysicalBehaviour (NullReferenceException in TurnRoutine's PlayClipOnce)"));
                if !presentation_event(prefab, file, field(v, "OnDistributionChange")?) {
                    unimplemented!("BloodContainer.OnDistributionChange: a persistent call beyond audio and particles");
                }
                x.ValveTurnCurve = Some(curve(field(v, "ValveTurnCurve")?)?);
                valves.insert(o, self.valves.len());
                self.valves.push(x);
            }
            if let Some((c, enabled, v)) = script(prefab, o, "HeartMonitorBehaviour") {
                let mut x = super::heart_monitor::HeartMonitor::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                x.physical = l.arenas.physical.get(&o).copied().unwrap_or_else(|| unimplemented!("HeartMonitorBehaviour without a PhysicalBehaviour (a wire's GetComponent finds it through one)"));
                heart_monitors.insert(o, self.heart_monitors.len());
                self.heart_monitors.push(x);
            }
            if let Some((c, enabled, v)) = script(prefab, o, "ExcaliburBehaviour") {
                let mut x = super::excalibur::Excalibur::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                // Lodged reaches it through its physical's receiver name (stab.rs send_lodged).
                let p = *l.arenas.physical.get(&o).unwrap_or_else(|| unimplemented!("ExcaliburBehaviour without a PhysicalBehaviour (its Lodged receiver is the physical's)"));
                self.physicals[p].lodge_receiver = "ExcaliburBehaviour".to_string();
                excaliburs.insert(o, self.excaliburs.len());
                self.excaliburs.push(x);
            }
            if let Some((c, enabled, v)) = script(prefab, o, "SharpOnAllSidesBehaviour") {
                let file = prefab.objects[o].game_object.file;
                let mut x = super::sharp_on_all_sides::SharpOnAllSides::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                x.physical = *l.arenas.physical.get(&o).unwrap_or_else(|| unimplemented!("SharpOnAllSidesBehaviour without a PhysicalBehaviour (NullReferenceException in FixedUpdate)"));
                // SharpCollider's fixtures (the query shape of OverlapCollider).
                let sc = field(v, "SharpCollider")?.pptr().filter(|p| !p.is_null() && p.file == 0).unwrap_or_else(|| unimplemented!("SharpOnAllSidesBehaviour.SharpCollider null or outside its prefab (NullReferenceException in FixedUpdate)"));
                let (so, scc) = *prefab.index.get(&ObjRef { file, path_id: sc.path_id }).ok_or("SharpOnAllSidesBehaviour.SharpCollider: dangling reference")?;
                x.SharpCollider = inst.fixtures.get(&(so, scc)).cloned().unwrap_or_default();
                x.sharp_collider_id = inst.objects[so].components[scc];
                x.LayerMask = field(field(v, "LayerMask")?, "m_Bits")?.i64().ok_or("LayerMask.m_Bits")? as u32;
                // OnAfterDeserialise reconnects SerialisableVictims; a field the prefab omits is an empty array.
                if v.get("SerialisableVictims").and_then(Value::array).is_some_and(|a| !a.is_empty()) {
                    unimplemented!("SharpOnAllSidesBehaviour.SerialisableVictims: OnAfterDeserialise (GUID lookup of the victims)");
                }
                sharp_on_all_sides.insert(o, self.sharp_on_all_sides.len());
                self.sharp_on_all_sides.push(x);
            }
            if let Some((c, enabled, v)) = script(prefab, o, "TimedGateBehaviour") {
                let mut x = super::timed_gate::TimedGate::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                x.physical = l.arenas.physical.get(&o).copied().unwrap_or_else(|| unimplemented!("TimedGateBehaviour without a PhysicalBehaviour (NullReferenceException)"));
                // Knob is dereferenced by DoLag and Update (NullReferenceException when it is null).
                if field(v, "Knob")?.pptr().ok_or("Knob")?.is_null() {
                    unimplemented!("TimedGateBehaviour.Knob is null (NullReferenceException)");
                }
                timed_gates.insert(o, self.timed_gates.len());
                self.timed_gates.push(x);
            }
            if let Some((c, enabled, v)) = script(prefab, o, "DefibrillatorBehaviour") {
                let mut x = super::resuscitation::Defibrillator::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                // PlayClipOnce on the PhysicalBehaviour, TextScreen (Start, OnEnable) and both pads are used without a null check.
                if l.physical(field(v, "PhysicalBehaviour")?)?.is_none() || field(v, "TextScreen")?.pptr().ok_or("TextScreen")?.is_null() {
                    unimplemented!("DefibrillatorBehaviour.PhysicalBehaviour or TextScreen is null (NullReferenceException)");
                }
                x.Pad1 = l.physical(field(v, "Pad1")?)?.unwrap_or_else(|| unimplemented!("DefibrillatorBehaviour.Pad1 is null (NullReferenceException)"));
                x.Pad2 = l.physical(field(v, "Pad2")?)?.unwrap_or_else(|| unimplemented!("DefibrillatorBehaviour.Pad2 is null (NullReferenceException)"));
                x.PadLayer = field(field(v, "PadLayer")?, "m_Bits")?.i64().ok_or("PadLayer.m_Bits")? as u32;
                let clip = field(v, "AudioIndicator")?.pptr().ok_or("AudioIndicator")?;
                x.AudioIndicator = if clip.is_null() { None } else { Some(crate::assets::classes::AudioClip::read(db.data(db.resolve(file, clip).ok_or("AudioIndicator outside the data")?))?.length) };
                defibrillators.insert(o, self.defibrillators.len());
                self.defibrillators.push(x);
            }
            if let Some((c, enabled, v)) = script(prefab, o, "CardiopulmonaryBypassMachineBehaviour") {
                let mut x = super::resuscitation::CardiopulmonaryBypass::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                x.physical = l.arenas.physical.get(&o).copied().unwrap_or_else(|| unimplemented!("CardiopulmonaryBypassMachineBehaviour without a PhysicalBehaviour (a wire's GetComponent finds it through one)"));
                // UpdateActivation (Start, OnEnable) uses Cosmetics and StatusText, and OnDisable the LoopSource, without a null check.
                let cosmetics = field(v, "Cosmetics")?.pptr().ok_or("Cosmetics")?;
                if cosmetics.is_null() || cosmetics.file != 0 || field(v, "StatusText")?.pptr().ok_or("StatusText")?.is_null() || field(v, "LoopSource")?.pptr().ok_or("LoopSource")?.is_null() {
                    unimplemented!("CardiopulmonaryBypassMachineBehaviour.Cosmetics, StatusText or LoopSource is null (NullReferenceException)");
                }
                x.Cosmetics = prefab.index[&ObjRef { file, path_id: cosmetics.path_id }].0;
                cardiopulmonary_bypasses.insert(o, self.cardiopulmonary_bypasses.len());
                self.cardiopulmonary_bypasses.push(x);
            }
            if let Some((c, _, v)) = script(prefab, o, "SyringeExplosionBehaviour") {
                let mut x = super::syringe::SyringeExplosion::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                syringe_explosions.insert(o, self.syringe_explosions.len());
                self.syringe_explosions.push(x);
            }
            if let Some((_, _, v)) = script(prefab, o, "PointLiquidTransferBehaviour") {
                let mut x = super::flask::PointLiquidTransfer::default();
                load_values(x.fields(), v)?;
                x.Phys = *l.arenas.physical.get(&o).ok_or("PointLiquidTransferBehaviour without a PhysicalBehaviour")?;
                x.Flask = *flasks.get(&o).ok_or("PointLiquidTransferBehaviour without a flask BloodContainer")?;
                x.Layers = field(field(v, "Layers")?, "m_Bits")?.i64().ok_or("Layers.m_Bits")? as u32;
                point_liquids.insert(o, self.point_liquids.len());
                self.point_liquids.push(x);
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
            let Some((c, _, v)) = script(prefab, o, "DamagableMachineryBehaviour") else { continue };
            let mut m = super::machinery::Machinery::default();
            load_values(m.fields(), v)?;
            m.PhysicalBehaviour = *l.arenas.physical.get(&o).ok_or("DamagableMachineryBehaviour without a PhysicalBehaviour")?;
            // SparkPrefab, else Resources.Load("Prefabs/BrokenElectronicsSpark").
            let spark = field(v, "SparkPrefab")?.pptr().ok_or("SparkPrefab")?;
            if spark.is_null() {
                m.SparkPrefab = db.resource("Prefabs/BrokenElectronicsSpark", 1)?.is_some();
            } else {
                if spark.file == 0 {
                    unimplemented!("DamagableMachineryBehaviour.SparkPrefab: a scene object inside the prefab");
                }
                m.SparkPrefab = true;
                m.SparkPrefabSource = Some(db.resolve(file, spark).ok_or("DamagableMachineryBehaviour.SparkPrefab outside the data")?);
            }
            let explosion_prefab = field(v, "ExplosionPrefab")?.pptr().ok_or("ExplosionPrefab")?;
            m.ExplosionPrefab = !explosion_prefab.is_null();
            if m.ExplosionPrefab {
                m.ExplosionPrefabSource = Some(db.resolve(file, explosion_prefab).ok_or("DamagableMachineryBehaviour.ExplosionPrefab outside the data")?);
            }
            m.Explosion = explosion_parameters(field(v, "Explosion")?)?;
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
            m.id = inst.objects[o].components[c];
            m.OnDamagedPresentation = presentation_event(prefab, file, field(v, "OnDamaged")?);
            m.OnRepairedPresentation = presentation_event(prefab, file, field(v, "OnRepaired")?);
            if !m.OnDamagedPresentation {
                // Kept only when every call is one EventCall; otherwise SetAsBroken stays unimplemented.
                if let Ok(calls) = try_event_calls(prefab, file, field(v, "OnDamaged")?, inst, false)? {
                    (m.OnDamagedCalls, m.OnDamagedPresentation) = (calls, true);
                }
            }
            machinery.insert(o, self.machinery.len());
            self.machinery.push(m);
        }
        let mut airfoils = HashMap::new();
        let mut stunners = HashMap::new();
        let mut soaps = HashMap::new();
        let mut power_hammers = HashMap::new();
        let mut balloons = HashMap::new();
        let (mut metronomes, mut thermometers, mut infrared_thermometers) = (HashMap::new(), HashMap::new(), HashMap::new());
        let (mut activation_toggles, mut power_gauges) = (HashMap::new(), HashMap::new());
        let (mut torches, mut flares) = (HashMap::new(), HashMap::new());
        let (mut bulbs, mut thrusterbeds, mut pistons, mut flamethrowers, mut rockets) = (HashMap::new(), HashMap::new(), HashMap::new(), HashMap::new(), HashMap::new());
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
            if let Some((c, _, v)) = script(prefab, o, "BulbBehaviour") {
                let mut x = super::devices::Bulb::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.Phys = l.in_arena(field(v, "PhysicalBehaviour")?, &l.arenas.physical, "PhysicalBehaviour")?.ok_or("BulbBehaviour.PhysicalBehaviour is null")?;
                x.OtherPart = l.object(field(v, "OtherPart")?)?;
                // Explosion.GetComponentInChildren<ExplosionSoundBehviour>(): the object, then its children depth first.
                let explosion = l.object(field(v, "Explosion")?)?.ok_or("BulbBehaviour.Explosion is null")?;
                let mut stack = vec![explosion];
                while let Some(e) = stack.pop() {
                    if let Some((sc, senabled, sv)) = script(prefab, e, "ExplosionSoundBehviour") {
                        let set = field(sv, "audioSource")?.pptr().is_some_and(|p| !p.is_null());
                        let has = set || prefab.objects[e].components.iter().any(|(_, x)| matches!(x, Component::Other(82)));
                        let near = (senabled && has).then(|| array(sv, "Near").map(<[_]>::len)).transpose()?;
                        x.sound = Some((inst.objects[e].components[sc], near, field(sv, "DistantSound")?.i64().ok_or("DistantSound")?));
                        break;
                    }
                    stack.extend(prefab.objects[e].children.iter().rev());
                }
                if let Some(p) = field(v, "ColliderToDisable")?.pptr().filter(|p| !p.is_null()) {
                    let (co, cc) = prefab.index[&ObjRef { file: l.file, path_id: p.path_id }];
                    if co != o {
                        unimplemented!("BulbBehaviour.ColliderToDisable on another object");
                    }
                    x.collider = Some(inst.objects[co].components[cc]);
                }
                x.index = c;
                x.after_physical = script(prefab, o, "PhysicalBehaviour").is_some_and(|(pc, _, _)| pc < c);
                bulbs.insert(o, self.bulbs.len());
                self.bulbs.push(x);
            }
            if let Some((c, enabled, v)) = script(prefab, o, "ThrusterbedBehaviour") {
                let mut x = super::thruster::Thrusterbed::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                x.Phys = *l.arenas.physical.get(&o).ok_or("ThrusterbedBehaviour without a PhysicalBehaviour")?;
                x.LayersToBurn = field(field(v, "LayersToBurn")?, "m_Bits")?.i64().ok_or("LayersToBurn.m_Bits")? as u32;
                x.StartupStrengthCurve = Some(curve(field(v, "StartupStrengthCurve")?)?);
                thrusterbeds.insert(o, self.thrusterbeds.len());
                self.thrusterbeds.push(x);
            }
            if let Some((c, enabled, v)) = script(prefab, o, "PistonBehaviour") {
                let mut x = super::piston::Piston::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                x.Phys = l.in_arena(field(v, "PhysicalBehaviour")?, &l.arenas.physical, "PhysicalBehaviour")?.ok_or("PistonBehaviour.PhysicalBehaviour is null")?;
                x.Head = l.in_arena(field(v, "PistonHead")?, &l.arenas.physical, "PhysicalBehaviour")?.ok_or("PistonBehaviour.PistonHead is null")?;
                let m = l.object(field(v, "DamagableMachinery")?)?.ok_or("PistonBehaviour.DamagableMachinery is null")?;
                x.Machinery = *machinery.get(&m).ok_or("PistonBehaviour: no DamagableMachineryBehaviour on its object")?;
                if l.object(field(v, "Slider")?)? != Some(o) {
                    unimplemented!("PistonBehaviour.Slider on another object");
                }
                pistons.insert(o, self.pistons.len());
                self.pistons.push(x);
            }
            if let Some((c, enabled, v)) = script(prefab, o, "FlamethrowerBehaviour") {
                let mut x = super::flamethrower::Flamethrower::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                x.phys = *l.arenas.physical.get(&o).ok_or("FlamethrowerBehaviour without a PhysicalBehaviour")?;
                x.LayerMask = field(field(v, "LayerMask")?, "m_Bits")?.i64().ok_or("LayerMask.m_Bits")? as u32;
                let particle = field(v, "particlePrefab")?.pptr().ok_or("particlePrefab")?;
                if !particle.is_null() {
                    let r = db.resolve(file, particle).ok_or("FlamethrowerBehaviour.particlePrefab outside the data")?;
                    let effect = db.prefab(r)?;
                    if effect.objects[0].tag != super::water::POOLABLE_TAG && effect.objects.iter().any(|x| x.components.iter().any(|(_, c)| matches!(c, Component::Collider2D(_) | Component::Rigidbody2D(_)))) {
                        unimplemented!("FlamethrowerBehaviour: a particlePrefab with physics (its pose)");
                    }
                    x.particlePrefab = Some(r);
                }
                let mut at = l.object(field(v, "Muzzle")?)?.ok_or("FlamethrowerBehaviour.Muzzle is null")?;
                while at != o {
                    x.Muzzle.push(inst.objects[at].trs);
                    at = inst.objects[at].parent.ok_or("FlamethrowerBehaviour.Muzzle is outside the flamethrower")?;
                }
                if x.Muzzle.is_empty() {
                    unimplemented!("FlamethrowerBehaviour.Muzzle on the flamethrower's own object");
                }
                flamethrowers.insert(o, self.flamethrowers.len());
                self.flamethrowers.push(x);
            }
            if let Some((c, _, v)) = script(prefab, o, "LaunchedRocketBehaviour") {
                let mut x = super::rocket::LaunchedRocket::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.ImmobiltyFieldLayer = field(field(v, "ImmobiltyFieldLayer")?, "m_Bits")?.i64().ok_or("ImmobiltyFieldLayer.m_Bits")? as u32;
                let trail = l.object(field(v, "relevantSystem")?)?.unwrap_or_else(|| unimplemented!("LaunchedRocketBehaviour.relevantSystem is null (NullReferenceException on a hit)"));
                x.relevantSystem = inst.objects[trail].components[0];
                x.Explosion = Some(explosion_parameters(field(v, "Explosion")?)?);
                rockets.insert(o, self.rockets.len());
                self.rockets.push(x);
            }
            if let Some((c, _, v)) = script(prefab, o, "AirfoilBehaviour") {
                let mut x = super::airfoil::Airfoil::default();
                x.id = inst.objects[o].components[c];
                x.PhysicalBehaviour = l.in_arena(field(v, "PhysicalBehaviour")?, &l.arenas.physical, "PhysicalBehaviour")?.ok_or("AirfoilBehaviour.PhysicalBehaviour is null")?;
                // SpriteRenderer: the renderer on the same object, which the physical's sprite is (SetWingType swaps it).
                let file = prefab.objects[o].game_object.file;
                let sr = field(v, "SpriteRenderer")?.pptr().filter(|p| !p.is_null()).ok_or("AirfoilBehaviour.SpriteRenderer is null")?;
                let (so, sc) = prefab.index[&ObjRef { file, path_id: sr.path_id }];
                if so != o {
                    unimplemented!("AirfoilBehaviour.SpriteRenderer on another object");
                }
                let r = match &prefab.objects[o].components[sc].1 {
                    Component::SpriteRenderer(r) => r,
                    _ => return Err("AirfoilBehaviour.SpriteRenderer is not a SpriteRenderer".into()),
                };
                for w in array(v, "WingTypes")? {
                    let sprite = match field(w, "Sprite")?.pptr().ok_or("AirfoilBehaviour.WingType.Sprite")? {
                        p if p.is_null() => None,
                        p => {
                            let s = db.resolve(file, p).unwrap_or_else(|| unimplemented!("AirfoilBehaviour.WingType: a Sprite outside the data"));
                            Some((s, renderer_bounds(&Sprite::read(db.data(s))?, r)))
                        }
                    };
                    x.WingTypes.push(super::airfoil::WingType {
                        Name: string(w, "Name")?,
                        Sprite: sprite,
                        Lifts: field(w, "Lifts")?.bool().ok_or("AirfoilBehaviour.WingType.Lifts")?,
                        LiftMultiplier: float(w, "LiftMultiplier")?,
                        DragMultiplier: float(w, "DragMultiplier")?,
                    });
                }
                // A serialised CurrentWingType that lifts or drags would act in FixedUpdate before Start replaces it.
                let current = field(v, "CurrentWingType")?;
                if field(current, "Lifts")?.bool() != Some(false) || float(current, "DragMultiplier")? != 0.0 {
                    unimplemented!("AirfoilBehaviour: a serialised CurrentWingType that lifts or drags");
                }
                x.CurrentWingTypeName = string(v, "CurrentWingTypeName")?;
                airfoils.insert(o, self.airfoils.len());
                self.airfoils.push(x);
            }
            if let Some((c, _, v)) = script(prefab, o, "StunnerBehaviour") {
                let mut x = super::stunner::Stunner::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.mask = field(field(v, "mask")?, "m_Bits")?.i64().ok_or("StunnerBehaviour.mask.m_Bits")? as u32;
                let file = prefab.objects[o].game_object.file;
                let impact = field(v, "ImpactEffect")?.pptr().ok_or("StunnerBehaviour.ImpactEffect")?;
                if impact.is_null() {
                    unimplemented!("StunnerBehaviour: Instantiate(null ImpactEffect) (ArgumentException)");
                }
                x.ImpactEffect = Some(db.resolve(file, impact).ok_or("StunnerBehaviour.ImpactEffect outside the data")?);
                stunners.insert(o, self.stunners.len());
                self.stunners.push(x);
            }
            if let Some((c, _, v)) = script(prefab, o, "SoapBehaviour") {
                let mut x = super::soap::Soap::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.physical = l.arenas.physical.get(&o).copied();
                x.index = c;
                x.after_physical = script(prefab, o, "PhysicalBehaviour").is_some_and(|(pc, _, _)| pc < c);
                soaps.insert(o, self.soaps.len());
                self.soaps.push(x);
            }
            if let Some((c, _, v)) = script(prefab, o, "PowerHammerBehaviour") {
                let mut x = super::power_hammer::PowerHammer::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.physical = l.arenas.physical.get(&o).copied();
                x.index = c;
                x.after_physical = script(prefab, o, "PhysicalBehaviour").is_some_and(|(pc, _, _)| pc < c);
                // powerImpactCollider: a Collider2D of the prefab (possibly on another object), its instance id.
                let file = prefab.objects[o].game_object.file;
                x.powerImpactCollider = match field(v, "powerImpactCollider")?.pptr().filter(|p| !p.is_null()) {
                    None => None,
                    Some(p) => {
                        let (co, cc) = prefab.index[&ObjRef { file, path_id: p.path_id }];
                        Some(inst.objects[co].components[cc])
                    }
                };
                x.PhysicalBehaviour = l.in_arena(field(v, "physicalBehaviour")?, &l.arenas.physical, "PhysicalBehaviour")?.ok_or("PowerHammerBehaviour.physicalBehaviour is null")?;
                x.ImpactClipCount = array(v, "ImpactClips")?.len();
                let effect = field(v, "HammerEffectPrefab")?.pptr().ok_or("HammerEffectPrefab")?;
                x.HammerEffectPrefab = if effect.is_null() { None } else { Some(db.resolve(file, effect).ok_or("PowerHammerBehaviour.HammerEffectPrefab outside the data")?) };
                x.LayersToConsider = field(field(v, "LayersToConsider")?, "m_Bits")?.i64().ok_or("LayersToConsider.m_Bits")? as u32;
                power_hammers.insert(o, self.power_hammers.len());
                self.power_hammers.push(x);
            }
            if let Some((c, _, v)) = script(prefab, o, "BalloonBehaviour") {
                let mut x = super::balloon::Balloon::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.PhysicalBehaviour = l.in_arena(field(v, "PhysicalBehaviour")?, &l.arenas.physical, "PhysicalBehaviour")?.ok_or("BalloonBehaviour.PhysicalBehaviour is null")?;
                x.physical = l.arenas.physical.get(&o).copied();
                x.index = c;
                x.after_physical = script(prefab, o, "PhysicalBehaviour").is_some_and(|(pc, _, _)| pc < c);
                let effect = l.object(field(v, "PoppedEffect")?)?.ok_or("BalloonBehaviour.PoppedEffect is null")?;
                x.PoppedEffect = inst.objects[effect].components[0];
                balloons.insert(o, self.balloons.len());
                self.balloons.push(x);
            }
            if let Some((c, enabled, v)) = script(prefab, o, "MetronomeBehaviour") {
                let mut x = super::metronome::Metronome::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                x.phys = l.in_arena(field(v, "PhysicalBehaviour")?, &l.arenas.physical, "PhysicalBehaviour")?.ok_or("MetronomeBehaviour.PhysicalBehaviour is null")?;
                // Its ticks SendMessage IsolatedActivation to this object: a receiver on it is not rewritten.
                if prefab.objects[o].components.iter().any(|(_, comp)| matches!(comp, Component::Script { class, .. } if class == "UseWireBehaviour" || super::translated::receives(class, "IsolatedActivation"))) {
                    unimplemented!("MetronomeBehaviour: IsolatedActivation to a receiver on its own object");
                }
                metronomes.insert(o, self.metronomes.len());
                self.metronomes.push(x);
            }
            if let Some((c, enabled, v)) = script(prefab, o, "ThermometerBehaviour") {
                let mut x = super::thermometer::Thermometer::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                thermometers.insert(o, self.thermometers.len());
                self.thermometers.push(x);
            }
            if let Some((c, enabled, v)) = script(prefab, o, "InfraredThermometer") {
                let mut x = super::thermometer::InfraredThermometer::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                x.physical = *l.arenas.physical.get(&o).ok_or("InfraredThermometer without a PhysicalBehaviour (NullReferenceException)")?;
                let mut at = l.object(field(v, "RayPoint")?)?.ok_or("InfraredThermometer.RayPoint is null")?;
                while at != o {
                    x.RayPoint.push(inst.objects[at].trs);
                    at = inst.objects[at].parent.ok_or("InfraredThermometer.RayPoint is outside the thermometer")?;
                }
                if x.RayPoint.is_empty() {
                    unimplemented!("InfraredThermometer.RayPoint on the thermometer's own object");
                }
                x.LayersToHit = field(field(v, "LayersToHit")?, "m_Bits")?.i64().ok_or("InfraredThermometer.LayersToHit.m_Bits")? as u32;
                infrared_thermometers.insert(o, self.infrared_thermometers.len());
                self.infrared_thermometers.push(x);
            }
            if let Some((c, enabled, v)) = script(prefab, o, "ActivationToggleBehaviour") {
                let mut x = super::activation_toggle::ActivationToggle::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                x.physical = *l.arenas.physical.get(&o).ok_or("ActivationToggleBehaviour without a PhysicalBehaviour (GetComponent is null)")?;
                x.light = l.object(field(v, "LightObject")?)?.ok_or("ActivationToggleBehaviour.LightObject is null")?;
                // Its ContinuousActivation listener sends IsolatedContinuousActivation to this object: a receiver there is not rewritten.
                if prefab.objects[o].components.iter().any(|(_, comp)| matches!(comp, Component::Script { class, .. } if class == "UseWireBehaviour" || super::translated::receives(class, "IsolatedContinuousActivation"))) {
                    unimplemented!("ActivationToggleBehaviour: IsolatedContinuousActivation to a receiver on its own object");
                }
                activation_toggles.insert(o, self.activation_toggles.len());
                self.activation_toggles.push(x);
            }
            if let Some((c, _, v)) = script(prefab, o, "PowerGaugeBehaviour") {
                let mut x = super::gauge::PowerGauge::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                let file = prefab.objects[o].game_object.file;
                let label = field(v, "GaugeLabelPrefab")?.pptr().ok_or("GaugeLabelPrefab")?;
                x.label = if label.is_null() { None } else { Some(db.resolve(file, label).ok_or("PowerGaugeBehaviour.GaugeLabelPrefab outside the data")?) };
                x.center = l.object(field(v, "GaugeCenter")?)?.ok_or("PowerGaugeBehaviour.GaugeCenter is null")?;
                power_gauges.insert(o, self.power_gauges.len());
                self.power_gauges.push(x);
            }
            if let Some((c, enabled, v)) = script(prefab, o, "TorchBehaviour") {
                let mut x = super::torch::Torch::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                x.physical = l.arenas.physical.get(&o).copied();
                x.index = c;
                x.after_physical = script(prefab, o, "PhysicalBehaviour").is_some_and(|(pc, _, _)| pc < c);
                x.colliders = prefab.objects[o].components.iter().enumerate().filter(|(_, (_, comp))| matches!(comp, Component::Collider2D(_))).map(|(k, _)| inst.objects[o].components[k]).collect();
                torches.insert(o, self.torches.len());
                self.torches.push(x);
            }
            if let Some((c, enabled, v)) = script(prefab, o, "FlareBehaviour") {
                let mut x = super::flare::Flare::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                let file = prefab.objects[o].game_object.file;
                let torch = field(v, "TorchBehaviour")?.pptr().filter(|p| !p.is_null()).ok_or("FlareBehaviour.TorchBehaviour is null")?;
                let (to, tc) = prefab.index[&ObjRef { file, path_id: torch.path_id }];
                x.torch = inst.objects[to].components[tc];
                flares.insert(o, self.flares.len());
                self.flares.push(x);
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
        // The translated classes (tools/translator): each component with its plain fields, then its links.
        let mut translated = HashMap::new();
        let physics = |o: usize| std::iter::once(o).chain(inst.descendants(o)).any(|d| prefab.objects[d].components.iter().any(|(_, x)| matches!(x, Component::Collider2D(_) | Component::Rigidbody2D(_))));
        for o in 0..n {
            for (c, (_, comp)) in prefab.objects[o].components.iter().enumerate() {
                let Component::Script { class, enabled, fields: v, .. } = comp else { continue };
                if !super::translated::CLASSES.contains(&class.as_str()) {
                    continue;
                }
                let b = self.tr_new(class, v, inst.objects[o].components[c], *enabled, l.arenas.physical.get(&o).copied(), physics(o))?;
                translated.insert((o, c), b);
                if b.has_collision() {
                    // SendMessage of a collision goes to the GameObject of the PhysicalBehaviour that took it.
                    let physical = *l.arenas.physical.get(&o).unwrap_or_else(|| unimplemented!("{class}: collision messages on a GameObject without a PhysicalBehaviour"));
                    if l.arenas.limb.contains_key(&o) {
                        unimplemented!("{class} on a limb: SendMessage order with LimbBehaviour");
                    }
                    let after_physical = script(prefab, o, "PhysicalBehaviour").is_some_and(|(pc, _, _)| pc < c);
                    self.tr.collision_scripts.push(super::translated::rt::CollisionScript { physical, index: c, after_physical, b });
                }
                let trigger = ["OnTriggerEnter2D", "OnTriggerStay2D", "OnTriggerExit2D"].iter().any(|m| super::translated::receives(class, m));
                let joint_break = super::translated::receives(class, "OnJointBreak2D");
                if trigger || joint_break {
                    // SendMessage goes to the receiving GameObject's scripts: select it by that object's components.
                    let ids = |f: fn(&Component) -> bool| -> Vec<i32> { (0..prefab.objects[o].components.len()).filter(|&k| f(&prefab.objects[o].components[k].1)).map(|k| inst.objects[o].components[k]).collect() };
                    let after_physical = script(prefab, o, "PhysicalBehaviour").is_some_and(|(pc, _, _)| pc < c);
                    if trigger && script(prefab, o, "WaterBehaviour").is_some() {
                        unimplemented!("{class} next to a WaterBehaviour: OnTrigger*2D component order");
                    }
                    self.message_scripts.push(super::messages::MessageScript {
                        colliders: ids(|x| matches!(x, Component::Collider2D(_))),
                        rigidbodies: ids(|x| matches!(x, Component::Rigidbody2D(_))),
                        joints: ids(|x| matches!(x, Component::Joint2D(_))),
                        index: c,
                        after_physical,
                        b,
                        trigger,
                        joint_break,
                    });
                }
            }
        }
        for (&(o, c), &b) in &translated {
            use super::translated::{LinkKind as K, LinkVal as V};
            let Component::Script { class, fields: v, .. } = &prefab.objects[o].components[c].1 else { unreachable!() };
            for &(name, kind) in super::translated::link_fields(class) {
                let x = field(v, name)?;
                // A HingeJoint2D of this prefab: its `Game::hinges` index.
                let hinge = |x: &Value| -> Result<Option<usize>, String> {
                    match x.pptr().ok_or(name)? {
                        p if p.is_null() => Ok(None),
                        p => {
                            let at = *prefab.index.get(&ObjRef { file, path_id: p.path_id }).filter(|_| p.file == 0).ok_or_else(|| format!("{class}.{name} outside the prefab"))?;
                            Ok(Some(*l.arenas.hinge.get(&at).ok_or_else(|| format!("{class}.{name}: not a HingeJoint2D"))?))
                        }
                    }
                };
                let val = match kind {
                    K::Physical => V::Physical(l.in_arena(x, &l.arenas.physical, "PhysicalBehaviour")?),
                    K::Rigidbody => V::Physical(match l.object(x)? {
                        None => None,
                        Some(t) => Some(*l.arenas.physical.get(&t).unwrap_or_else(|| unimplemented!("{class}.{name}: a Rigidbody2D without a PhysicalBehaviour"))),
                    }),
                    K::Script(want) => V::Script(match x.pptr().ok_or(name)? {
                        p if p.is_null() => None,
                        p => {
                            let at = *prefab.index.get(&ObjRef { file, path_id: p.path_id }).filter(|_| p.file == 0).ok_or_else(|| format!("{class}.{name} outside the prefab"))?;
                            let t = *translated.get(&at).ok_or_else(|| format!("{class}.{name}: not a translated component"))?;
                            if t.class() != want {
                                return Err(format!("{class}.{name}: a {} for a {want}", t.class()));
                            }
                            Some(t)
                        }
                    }),
                    // An inline value (a Gradient, an AnimationCurve) is never null.
                    K::Present => V::Present(x.pptr().is_none_or(|p| !p.is_null())),
                    K::Len => V::Len(x.array().ok_or(name)?.len()),
                    K::Prefab => V::Prefab(match x.pptr().ok_or(name)? {
                        p if p.is_null() => None,
                        // An object of this prefab: by its Transform's instance id. Another prefab of the same file is an asset.
                        p if p.file == 0 && prefab.index.contains_key(&ObjRef { file, path_id: p.path_id }) => Some(super::translated::rt::Go::Local(inst.objects[l.object_of(p)?.unwrap()].components[0])),
                        p => Some(super::translated::rt::Go::Asset(db.resolve(file, p).ok_or_else(|| format!("{class}.{name} outside the data"))?)),
                    }),
                    K::Mask => V::Mask(field(x, "m_Bits")?.i64().ok_or("m_Bits")? as u32),
                    K::Hinge => V::Hinge(hinge(x)?),
                    K::Collider => V::Collider(match x.pptr().ok_or(name)? {
                        p if p.is_null() => 0,
                        p => {
                            let (o, c) = *prefab.index.get(&ObjRef { file, path_id: p.path_id }).filter(|_| p.file == 0).ok_or_else(|| format!("{class}.{name} outside the prefab"))?;
                            if !matches!(prefab.objects[o].components[c].1, Component::Collider2D(_)) {
                                return Err(format!("{class}.{name}: not a Collider2D"));
                            }
                            inst.objects[o].components[c]
                        }
                    }),
                    K::Hinges => V::Hinges(x.array().ok_or(name)?.iter().map(&hinge).collect::<Result<_, _>>()?),
                    K::Event => V::Event(event_calls(prefab, file, x, inst, &format!("{class}.{name}"), true)?),
                    K::Handwritten(arena) => V::Handwritten(match l.object(x)? {
                        None => None,
                        Some(t) => Some(*match arena {
                            "DamagableMachineryBehaviour" => &machinery,
                            "LimbBehaviour" => &l.arenas.limb,
                            _ => unimplemented!("{class}.{name}: a {arena}"),
                        }.get(&t).ok_or_else(|| format!("{class}.{name}: no {arena} there"))?),
                    }),
                    K::Transform => V::Transform(l.object(x)?.map(|t| super::translated::rt::Xf::this(l.arenas.physical.get(&t).copied(), physics(t)))),
                };
                self.tr_link(b, name, val);
            }
        }
        // The classes that read the translated behaviours: JetEngineBehaviour toggles its GuideTemperatureBehaviour.
        let mut jet_engines = HashMap::new();
        let mut chainsaws = HashMap::new();
        let mut freezes = HashMap::new();
        let mut emps = HashMap::new();
        let mut decimators = HashMap::new();
        let mut immobility_fields = HashMap::new();
        let mut energy_swords = HashMap::new();
        let mut signal_converters = HashMap::new();
        for o in 0..n {
            if let Some((c, _, v)) = script(prefab, o, "JetEngineBehaviour") {
                let mut x = super::jet_engine::JetEngine::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.PhysicalBehaviour = l.in_arena(field(v, "PhysicalBehaviour")?, &l.arenas.physical, "PhysicalBehaviour")?.ok_or("JetEngineBehaviour.PhysicalBehaviour is null")?;
                let m = l.object(field(v, "DamagableMachinery")?)?.ok_or("JetEngineBehaviour.DamagableMachinery is null")?;
                x.DamagableMachinery = *machinery.get(&m).ok_or("JetEngineBehaviour: no DamagableMachineryBehaviour on its object")?;
                let effector = |name: &str| -> Result<(usize, usize), String> {
                    let pp = field(v, name)?.pptr().ok_or_else(|| name.to_string())?;
                    prefab.index.get(&ObjRef { file, path_id: pp.path_id }).copied().filter(|_| !pp.is_null() && pp.file == 0).ok_or_else(|| format!("JetEngineBehaviour.{name} outside the prefab or null"))
                };
                let (io, ic) = effector("IntakeEffector")?;
                let (eo, ec) = effector("ExhaustEffector")?;
                for (name, k) in [("IntakeEffector", (io, ic)), ("ExhaustEffector", (eo, ec))] {
                    if !matches!(prefab.objects[k.0].components[k.1].1, Component::PointEffector2D(_)) {
                        return Err(format!("JetEngineBehaviour.{name} is not a PointEffector2D"));
                    }
                }
                x.IntakeEffector = inst.objects[io].components[ic];
                x.ExhaustEffector = inst.objects[eo].components[ec];
                // exhaustEffectorCollider = ExhaustEffector.GetComponent<Collider2D>(): the first one.
                let collider = prefab.objects[eo].components.iter().position(|(_, k)| matches!(k, Component::Collider2D(_))).ok_or("JetEngineBehaviour: the ExhaustEffector has no Collider2D")?;
                x.exhaustEffectorCollider = inst.fixtures.get(&(eo, collider)).cloned().unwrap_or_default();
                x.guideTemperatureBehaviour = translated.iter().find(|((to, tc), _)| *to == o && matches!(&prefab.objects[*to].components[*tc].1, Component::Script { class, .. } if class == "GuideTemperatureBehaviour")).map(|(_, &b)| b);
                jet_engines.insert(o, self.jet_engines.len());
                self.jet_engines.push(x);
            }
            if let Some((c, enabled, v)) = script(prefab, o, "ImmobilityFieldBehaviour") {
                let mut x = super::immobility_field::ImmobilityField::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                x.physical = l.in_arena(field(v, "PhysicalBehaviour")?, &l.arenas.physical, "PhysicalBehaviour")?.unwrap_or_else(|| unimplemented!("ImmobilityFieldBehaviour.PhysicalBehaviour null (NullReferenceException)"));
                // AddComponent<FrictionJoint2D> and GetComponents<FrictionJoint2D> act on the script's GameObject.
                if l.arenas.physical.get(&o) != Some(&x.physical) {
                    unimplemented!("ImmobilityFieldBehaviour on another GameObject than its PhysicalBehaviour (its FrictionJoint2Ds)");
                }
                if prefab.objects[o].components.iter().any(|(_, k)| matches!(k, Component::Joint2D(_))) {
                    unimplemented!("ImmobilityFieldBehaviour on a GameObject with a serialized Joint2D (GetComponents<FrictionJoint2D>)");
                }
                x.CaptureObject = l.object(field(v, "CaptureObject")?)?.unwrap_or_else(|| unimplemented!("ImmobilityFieldBehaviour.CaptureObject null (NullReferenceException in UpdateActivation)"));
                let p = field(v, "Trigger")?.pptr().filter(|p| !p.is_null() && p.file == 0).unwrap_or_else(|| unimplemented!("ImmobilityFieldBehaviour.Trigger null or outside its prefab (NullReferenceException)"));
                let k = *prefab.index.get(&ObjRef { file, path_id: p.path_id }).ok_or("ImmobilityFieldBehaviour.Trigger: dangling reference")?;
                if !matches!(prefab.objects[k.0].components[k.1].1, Component::Collider2D(_)) {
                    return Err("ImmobilityFieldBehaviour.Trigger is not a Collider2D".into());
                }
                x.Trigger = k;
                x.colliders = prefab.objects[o].components.iter().enumerate().filter(|(_, (_, k))| matches!(k, Component::Collider2D(_))).map(|(k, _)| inst.objects[o].components[k]).collect();
                x.rigidbody = prefab.objects[o].components.iter().position(|(_, k)| matches!(k, Component::Rigidbody2D(_))).map_or(0, |k| inst.objects[o].components[k]);
                x.CaptureClips = array(v, "CaptureClips")?.len();
                x.Layers = field(field(v, "Layers")?, "m_Bits")?.i64().ok_or("Layers.m_Bits")? as u32;
                immobility_fields.insert(o, self.immobility_fields.len());
                self.immobility_fields.push(x);
            }
            if let Some((c, enabled, v)) = script(prefab, o, "DecimatorBehaviour") {
                let mut x = super::decimator::Decimator::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                x.physical = l.in_arena(field(v, "PhysicalBehaviour")?, &l.arenas.physical, "PhysicalBehaviour")?.unwrap_or_else(|| unimplemented!("DecimatorBehaviour.PhysicalBehaviour null (NullReferenceException in FixedUpdate)"));
                let p = field(v, "DecimateTrigger")?.pptr().filter(|p| !p.is_null() && p.file == 0).unwrap_or_else(|| unimplemented!("DecimatorBehaviour.DecimateTrigger null or outside its prefab (NullReferenceException)"));
                let k = *prefab.index.get(&ObjRef { file, path_id: p.path_id }).ok_or("DecimatorBehaviour.DecimateTrigger: dangling reference")?;
                if !matches!(prefab.objects[k.0].components[k.1].1, Component::Collider2D(_)) {
                    return Err("DecimatorBehaviour.DecimateTrigger is not a Collider2D".into());
                }
                x.DecimateTrigger = k;
                x.colliders = prefab.objects[o].components.iter().enumerate().filter(|(_, (_, k))| matches!(k, Component::Collider2D(_))).map(|(k, _)| inst.objects[o].components[k]).collect();
                x.rigidbody = prefab.objects[o].components.iter().position(|(_, k)| matches!(k, Component::Rigidbody2D(_))).map_or(0, |k| inst.objects[o].components[k]);
                // UpdateActivation plays and stops these unconditionally.
                for name in ["ParticleSystem", "AudioSource"] {
                    if l.object(field(v, name)?)?.is_none() {
                        unimplemented!("DecimatorBehaviour.{name} null (NullReferenceException in UpdateActivation)");
                    }
                }
                for (name, slot) in [("DeleteEffect", &mut x.DeleteEffect), ("BlackHole", &mut x.BlackHole)] {
                    let p = field(v, name)?.pptr().ok_or(name)?;
                    if p.is_null() {
                        unimplemented!("DecimatorBehaviour: Instantiate(null {name}) (ArgumentException)");
                    }
                    *slot = db.resolve(file, p).ok_or_else(|| format!("DecimatorBehaviour.{name} outside the data"))?;
                }
                decimators.insert(o, self.decimators.len());
                self.decimators.push(x);
            }
            if let Some((c, enabled, v)) = script(prefab, o, "ChainsawBehaviour") {
                let mut x = super::chainsaw::Chainsaw::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                x.physical = l.in_arena(field(v, "PhysicalBehaviour")?, &l.arenas.physical, "PhysicalBehaviour")?.unwrap_or_else(|| unimplemented!("ChainsawBehaviour.PhysicalBehaviour null (NullReferenceException in FixedUpdate)"));
                let m = l.object(field(v, "DamagableMachinery")?)?.unwrap_or_else(|| unimplemented!("ChainsawBehaviour.DamagableMachinery null (NullReferenceException in Start)"));
                x.DamagableMachinery = *machinery.get(&m).ok_or("ChainsawBehaviour: no DamagableMachineryBehaviour on its object")?;
                let sc = field(v, "SawCollider")?.pptr().filter(|p| !p.is_null() && p.file == 0).unwrap_or_else(|| unimplemented!("ChainsawBehaviour.SawCollider null or outside its prefab (NullReferenceException in FixedUpdate)"));
                let key = *prefab.index.get(&ObjRef { file, path_id: sc.path_id }).ok_or("ChainsawBehaviour.SawCollider: dangling reference")?;
                if !matches!(prefab.objects[key.0].components[key.1].1, Component::Collider2D(_)) {
                    return Err("ChainsawBehaviour.SawCollider is not a Collider2D".into());
                }
                x.SawCollider = key;
                x.saw_collider_id = inst.objects[key.0].components[key.1];
                x.LayerMask = field(field(v, "LayerMask")?, "m_Bits")?.i64().ok_or("LayerMask.m_Bits")? as u32;
                // OnAfterDeserialise reconnects SerialisableVictims; a field the prefab omits is an empty array.
                if v.get("SerialisableVictims").and_then(Value::array).is_some_and(|a| !a.is_empty()) {
                    unimplemented!("ChainsawBehaviour.SerialisableVictims: OnAfterDeserialise (GUID lookup of the victims)");
                }
                chainsaws.insert(o, self.chainsaws.len());
                self.chainsaws.push(x);
            }
            if let Some((c, enabled, v)) = script(prefab, o, "EMPBehaviour") {
                let mut x = super::emp::Emp::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                x.physical = l.arenas.physical.get(&o).copied().unwrap_or_else(|| unimplemented!("EMPBehaviour on an object without a PhysicalBehaviour"));
                // Activate's Instantiate(Effect, ...) throws on a null Effect (ArgumentException).
                if field(v, "Effect")?.pptr().filter(|p| !p.is_null()).is_none() {
                    unimplemented!("EMPBehaviour.Effect null (ArgumentException in Activate)");
                }
                emps.insert(o, self.emps.len());
                self.emps.push(x);
            }
            if let Some((c, enabled, _)) = script(prefab, o, "FreezeBehaviour") {
                let mut x = super::freeze::Freeze::default();
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                x.physical = l.arenas.physical.get(&o).copied().unwrap_or_else(|| unimplemented!("FreezeBehaviour without a Rigidbody2D (NullReferenceException in Start)"));
                freezes.insert(o, self.freezes.len());
                self.freezes.push(x);
            }
            if let Some((c, enabled, v)) = script(prefab, o, "EnergySwordBehaviour") {
                let mut x = super::energy_sword::EnergySword::default();
                load_values(x.fields(), v)?;
                x.id = inst.objects[o].components[c];
                x.enabled = enabled;
                x.physical = l.in_arena(field(v, "PhysicalBehaviour")?, &l.arenas.physical, "PhysicalBehaviour")?.unwrap_or_else(|| unimplemented!("EnergySwordBehaviour.PhysicalBehaviour null (NullReferenceException)"));
                let collider = |name: &str| -> Result<(usize, usize), String> {
                    let p = field(v, name)?.pptr().filter(|p| !p.is_null() && p.file == 0).unwrap_or_else(|| unimplemented!("EnergySwordBehaviour.{name} null or outside its prefab (NullReferenceException)"));
                    let k = *prefab.index.get(&ObjRef { file, path_id: p.path_id }).ok_or_else(|| format!("EnergySwordBehaviour.{name}: dangling reference"))?;
                    match prefab.objects[k.0].components[k.1].1 {
                        Component::Collider2D(_) => Ok(k),
                        _ => Err(format!("EnergySwordBehaviour.{name} is not a Collider2D")),
                    }
                };
                let main = collider("MainCollider")?;
                x.MainCollider = inst.objects[main.0].components[main.1];
                x.BladeCollider = collider("BladeCollider")?;
                x.blade_collider_id = inst.objects[x.BladeCollider.0].components[x.BladeCollider.1];
                let line = field(v, "LocalBladeLine")?;
                for (k, name) in ["A", "B"].into_iter().enumerate() {
                    super::Field::set(&mut x.LocalBladeLine[k], field(line, name)?).map_err(|e| format!("EnergySwordBehaviour.LocalBladeLine.{name}: {e}"))?;
                }
                x.BladeObject = l.object(field(v, "BladeObject")?)?.unwrap_or_else(|| unimplemented!("EnergySwordBehaviour.BladeObject null (NullReferenceException in Awake)"));
                x.MotionBlur = l.object(field(v, "MotionBlur")?)?.unwrap_or_else(|| unimplemented!("EnergySwordBehaviour.MotionBlur null (NullReferenceException in Awake)"));
                x.LayerMask = field(field(v, "LayerMask")?, "m_Bits")?.i64().ok_or("LayerMask.m_Bits")? as u32;
                x.ImpactSound = array(v, "ImpactSound")?.len();
                // SetActivation plays and stops these and the particles unconditionally.
                for name in ["HitParticle", "BoilingVFX", "RainParticles", "LoopSource"] {
                    if l.object(field(v, name)?)?.is_none() {
                        unimplemented!("EnergySwordBehaviour.{name} null (NullReferenceException in SetActivation)");
                    }
                }
                // ImpactSource: its isPlaying draws Random.value on Play. A looping, enabled source with a clip plays
                // until Stop.
                let a = field(v, "ImpactSource")?.pptr().filter(|p| !p.is_null() && p.file == 0).unwrap_or_else(|| unimplemented!("EnergySwordBehaviour.ImpactSource null or outside its prefab"));
                let source = crate::assets::classes::AudioSource::read(db.data(ObjRef { file, path_id: a.path_id }))?;
                let so = prefab.index[&ObjRef { file, path_id: a.path_id }].0;
                if !source.enabled || source.clip.is_null() || !source.looping || so == x.BladeObject || inst.descendants(x.BladeObject).contains(&so) {
                    unimplemented!("EnergySwordBehaviour.ImpactSource: disabled, without a clip, not looping or under BladeObject (its isPlaying)");
                }
                if v.get("SerialisableVictims").and_then(Value::array).is_some_and(|a| !a.is_empty()) {
                    unimplemented!("EnergySwordBehaviour.SerialisableVictims: OnAfterDeserialise (GUID lookup of the victims)");
                }
                energy_swords.insert(o, self.energy_swords.len());
                self.energy_swords.push(x);
            }
            if let Some((c, _, v)) = script(prefab, o, "EnergySwordBladeClashBehaviour") {
                let p = field(v, "CollideWithOtherSwordVFX")?.pptr().ok_or("CollideWithOtherSwordVFX")?;
                if p.is_null() {
                    unimplemented!("EnergySwordBladeClashBehaviour: Instantiate(null CollideWithOtherSwordVFX) (ArgumentException)");
                }
                let r = db.resolve(file, p).ok_or("EnergySwordBladeClashBehaviour.CollideWithOtherSwordVFX outside the data")?;
                if db.prefab(r)?.objects.iter().any(|x| x.components.iter().any(|(_, c)| matches!(c, Component::Collider2D(_) | Component::Rigidbody2D(_)))) {
                    unimplemented!("EnergySwordBladeClashBehaviour: a CollideWithOtherSwordVFX with physics (its pose)");
                }
                let colliders = prefab.objects[o].components.iter().enumerate().filter(|(_, (_, k))| matches!(k, Component::Collider2D(_))).map(|(k, _)| inst.objects[o].components[k]).collect();
                self.blade_clashes.push((inst.objects[o].components[c], colliders, r));
            }
            if let Some((_, _, v)) = script(prefab, o, "SignalConverterBehaviour") {
                let mut x = super::signal_converter::SignalConverter::default();
                load_values(x.fields(), v)?;
                x.wired = script(prefab, o, "UseWireBehaviour").is_some();
                signal_converters.insert(o, self.signal_converters.len());
                self.signal_converters.push(x);
            }
        }
        Ok(Linked { person, limbs: l.arenas.limb, nodes: l.arenas.node, physicals: l.arenas.physical, hinges: l.arenas.hinge, map_lights, waters, machinery, destroyables, fire_maps, firearms, launchers, bells, machine_guns, miniguns, accelerators, accelerator_bolts, archelix_casters, archelix_bolts, universal_bells, toggleable_mirrors, valves, timed_gates, defibrillators, cardiopulmonary_bypasses, heart_monitors, blood_tanks, excaliburs, sharp_on_all_sides, explosives, delete_afters, point_to_velocities, android_sparks, cars, act_on_collides, tyres, flasks, truck_doors, liquid_containers, glowtubes, leds, detectors, sticky_grenades, generators, floodlights, spike_grenades, heating_elements, ray_guns, bulbs, thrusterbeds, pistons, flamethrowers, rockets, airfoils, stunners, soaps, power_hammers, balloons, metronomes, thermometers, infrared_thermometers, activation_toggles, power_gauges, torches, flares, buttons, displays, lasers, firearm_attachments, televisions, syringes, jet_engines, chainsaws, freezes, emps,decimators, immobility_fields, energy_swords, blasters, blasterbolts, beamformers, scifi40s, effect_guns, signal_converters, syringe_explosions, point_liquids, lavas, spawners, heat_rects, random_sprites, node_rigidbodies, parent_audio: None, translated })
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
        // 0x9e3779b1 (mono_object_hash, 180116520), of the managed heap address. That address is not defined by the
        // game: it varies from run to run with the OS's heap placement. The rewrite must not (the same input gives the
        // same frames), so the address is modelled: Boehm hands out large objects upward from the heap base, so the
        // n-th PhysicalBehaviour sits `n` objects above it. A PhysicalBehaviour is a large object, 2048-aligned (every
        // recorded hash has a zero low byte). Recorded runs (dump.rs) carry the real hash instead.
        for p in first..self.physicals.len() {
            let address = self.heap_base.unwrap_or(MODELLED_HEAP_BASE) + ((p as u64) << 11);
            self.physicals[p].hash_code = ((address >> 3) as u32).wrapping_mul(0x9e37_79b1) as i32;
        }
        self.awake_behaviours(prefab, &mut inst, &linked, &behaviours);
        self.queue_starts(Some(&mut *db), prefab, &inst, &linked, &behaviours)?;
        if inst.objects.iter().any(|x| !x.activated) {
            self.first_activation.insert(inst.objects[0].components[0], linked.clone());
        }
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
            EventCall::LiquidSpread(id) if self.destroyed_components.contains(&id) => {}
            EventCall::LiquidSpread(id) => self.liquid_spread(id),
            // DetachFromParent.Detach: transform.parent = null.
            EventCall::Detach(id) => self.set_parent_null_of(id, |_, _, _| {}),
            // ActOnDestroy.Clear: DestroyWith = null. A destroyed ActOnDestroy (its entry is gone) is a destroyed target: the call is skipped.
            EventCall::ClearDestroyWith(key) => {
                if let Some(a) = self.act_on_destroy.get_mut(&key) {
                    a.destroy_with.clear();
                }
            }
            // A destroyed target's call is skipped (BaseInvokableCall.AllowInvoke).
            EventCall::EnableDeleteAfter(id) if self.destroyed_components.contains(&id) => {}
            // Behaviour.enabled = true: nothing when it already is; else, on an active object, Behaviour::AddToManager.
            EventCall::EnableDeleteAfter(id) => {
                let i = self.delete_afters.iter().position(|d| d.id == id).expect("a DeleteAfterTime in the arena");
                if !std::mem::replace(&mut self.delete_afters[i].enabled, true) && self.with_component_instance(id, |_, inst, o| inst.objects[o].active) {
                    let (order, b) = self.behaviour_of[&id];
                    self.behaviours.add(order, b);
                }
            }
            // Behaviour.enabled = true on a RandomForceAtStart that never ran Awake: Start follows before its first
            // Update (it destroys itself, so it only enables once).
            EventCall::EnableRandomForce(rb, ..) if self.destroyed_components.contains(&rb) => {}
            EventCall::EnableRandomForce(rb, positional, rotational) => self.pending_starts.push(super::misc::PendingStart::RandomForce(rb, positional, rotational)),
            EventCall::Simulate(id) if self.destroyed_components.contains(&id) => {}
            EventCall::Simulate(id) => self.rigidbody_set_simulated(id),
            EventCall::Instantiate(id) if self.destroyed_components.contains(&id) => {}
            EventCall::Instantiate(id) => self.instantiate_object(id, false),
            EventCall::AddRelativeForce(id, _) if self.destroyed_components.contains(&id) => {}
            EventCall::AddRelativeForce(id, intensity) => self.add_force_relative(id, intensity),
            EventCall::HaltNextPropagation(id) if self.destroyed_components.contains(&id) => {}
            EventCall::HaltNextPropagation(id) => self.physical_halt_next_propagation(id),
            EventCall::SetBreakForce(id, _) | EventCall::WakeUp(id) | EventCall::AddTorque(id, _) if self.destroyed_components.contains(&id) => {}
            EventCall::SetBreakForce(id, v) => self.joint_set_break_force(id, v),
            EventCall::WakeUp(id) => self.rigidbody_wake_up(id),
            EventCall::AddTorque(id, torque) => self.rigidbody_add_torque(id, torque),
            // Behaviour.enabled of a UseEventTrigger: Use reads it; it has no OnEnable or OnDisable.
            EventCall::DisableMachinery(id) => {
                let m = self.machinery.iter().position(|m| m.id == id).expect("a DamagableMachineryBehaviour in the arena");
                self.behaviours.remove(Behaviour::Machinery(m));
            }
            EventCall::JetForceShutoff(id) if self.destroyed_components.contains(&id) => {}
            EventCall::JetForceShutoff(id) => self.jet_engine_force_shutoff(id),
            EventCall::SetUseEventEnabled(id, _) if self.destroyed_components.contains(&id) => {}
            EventCall::SetUseEventEnabled(id, on) => self.use_events.get_mut(&id).expect("a UseEventTrigger in the arena").enabled = on,
        }
    }

    /// UseEventTrigger.Use (a Messages.IUse receiver): the Event's persistent calls in order, once with OnlyOnce.
    pub(super) fn use_event_trigger_use(&mut self, id: i32) {
        let e = self.use_events.get_mut(&id).expect("a UseEventTrigger in the arena");
        if (e.only_once && e.use_count > 0) || !e.enabled {
            return;
        }
        e.use_count += 1;
        for call in e.calls.clone() {
            self.event_call(call);
        }
    }

    /// Transform.SetParent(null) of the clone object holding component `id`, when it has a parent; `before` runs
    /// first with the parent.
    pub(super) fn set_parent_null_of(&mut self, id: i32, before: impl FnOnce(&mut Game, &mut Instance, usize)) {
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
        // A PhysicalBehaviour's Transform is kept live (write-back, refresh_nested), so it gives the world pose then.
        let mut live = None;
        if let Some(&b) = inst.bodies.get(&o) {
            let (pos, xf) = (inst.position(o), self.world.body(b).xf.p);
            if pos[0] != xf.x || pos[1] != xf.y {
                let p = self.physicals.iter().position(|p| inst.objects[o].components.contains(&p.id)).filter(|&p| self.physicals[p].transform.local.is_some());
                // A Rigidbody2D without a PhysicalBehaviour: its Transform node is kept live.
                let k = self.node_rigidbodies.iter().position(|n| self.transform_nodes[n.node].id == inst.objects[o].components[0]);
                match (p, k) {
                    (Some(p), _) => {
                        let t = &self.physicals[p].transform;
                        live = Some((t.position, t.rotation, self.lossy_scale(p)));
                    }
                    (None, Some(k)) => {
                        let (position, rotation) = self.node_world(self.node_rigidbodies[k].node);
                        live = Some((position, rotation, self.owner_lossy_scale(BodyOwner::Node(k))));
                    }
                    (None, None) => unimplemented!("{}: Transform.SetParent after its body moved (the clone's Transforms are stale)", prefab.objects[o].name),
                }
            }
        }
        let bodies: Vec<_> = subtree.iter().filter_map(|x| inst.bodies.get(x).copied()).collect();
        for (&(x, _), fixtures) in &inst.fixtures {
            if subtree.contains(&x) && fixtures.first().is_some_and(|&f| !bodies.contains(&self.world.fixture(f).body)) {
                unimplemented!("{}: SetParent(null) moves a Collider2D off its Rigidbody2D (Collider2D::ParentChanged)", prefab.objects[x].name);
            }
        }
        match live {
            Some(world) => inst.set_parent_null_at(o, world),
            None => inst.set_parent_null(o),
        }
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
            let owner = self.rigidbody_owner(id).expect("a Rigidbody2D outside the arenas");
            // CheckForDrivenByParentRigidbody(null): the nearest ancestor Rigidbody2D drives a non-static one.
            let driven_by = self.rigidbody2d_driven_by(inst, &prefab, x, self.owner_rigidbody(owner).bodyType);
            self.set_driven_by(owner, driven_by);
            self.owner_rigidbody_mut(owner).movement.interpolating = false;
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
        // SpawnedGameObjects.Add (not simulation state); UndoControllerBehaviour.RegisterAction(new ObjectCreationAction(gameObject)),
        // whose DeregisterBehaviour (added above) drops it again when the object is destroyed.
        let root = inst.objects[0].components[0];
        self.deregister_behaviours.insert(root);
        self.undo_register_action(super::undo::ObjectCreationAction { displayName: asset.to_string(), RelevantObjects: vec![root] });
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
        let root = match (linked.physicals.get(&0), linked.nodes.get(&0)) {
            (Some(&p), _) => self.physicals[p].transform_id,
            (_, Some(&n)) => self.transform_nodes[n].id,
            _ => unreachable!(),
        };
        // Its Start's Global.AddAudioSource of every AudioSource below: their pitch follows Time.timeScale.
        for s in &mut self.sound_on_shakes {
            if s.root == root {
                s.time_scaled = true;
            }
        }
        self.object_names.insert(root, asset.to_string());
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

    /// GameObject.SetActive(false) on object `o` while its clone is being awoken: the OnDisable of the scripts below it
    /// whose OnEnable already ran.
    fn awake_set_inactive(&mut self, prefab: &Prefab, inst: &mut Instance, linked: &Linked, enabled_behaviours: &mut std::collections::HashSet<(usize, usize)>, o: usize) {
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
                "ThrusterbedBehaviour" => self.thrusterbed_on_disable(linked.thrusterbeds[&d]),
                "MetronomeBehaviour" => self.metronome_on_disable(linked.metronomes[&d]),
                "TimedGateBehaviour" => self.timed_gate_on_disable(linked.timed_gates[&d]),
                "DefibrillatorBehaviour" => self.defibrillator_on_disable(linked.defibrillators[&d]),
                // OnDisable only stops the loop sound.
                "CardiopulmonaryBypassMachineBehaviour" => {}
                "HeartMonitorBehaviour" => self.heart_monitor_on_disable(linked.heart_monitors[&d]),
                "ImmobilityFieldBehaviour" => self.immobility_field_on_disable(linked.immobility_fields[&d], inst),
                // OnDisable -> OnDestroy: clears softConnections, empty before the first FixedUpdate.
                "SharpOnAllSidesBehaviour" => {}
                "InfraredThermometer" => self.infrared_on_disable(linked.infrared_thermometers[&d]),
                "ActivationToggleBehaviour" => self.activation_toggle_on_disable(linked.activation_toggles[&d]),
                "DisplayBehaviour" => self.display_on_disable(linked.displays[&d]),
                // OnDisable only hides the light (button.rs).
                "ButtonBehaviour" => {}
                "LaserBehaviour" => {}
                // OnDisable only hides the readout (TextMesh and Light).
                "ThermometerBehaviour" => {}
                // No OnDisable; TextMeshPro's only touches its mesh and the font atlas.
                "ActivateIfTutorial" | "ActivateIfFancyEffects" | "AudioSourceTimeScaleBehaviour" | "DetachFromParentOnStart" | "DebrisComponent" | "Optout" | "MaintainScaleSignBehaviour" | "TextMeshProControlReplacer" | "TextMeshPro" => {}
                name if super::translated::CLASSES.contains(&name) => self.tr_on_disable(linked.translated[&(d, k)]),
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
                "ActivateIfTutorial" | "ActivateIfFancyEffects" => self.awake_set_inactive(prefab, inst, linked, &mut enabled_behaviours, o),
                name if super::translated::CLASSES.contains(&name) => {
                    let b = linked.translated[&(o, c)];
                    // The translated code reaches its clone through the instance lists.
                    let root = inst.objects[0].components[0];
                    self.unsynced.push(std::mem::take(inst));
                    self.awaking = true;
                    self.tr_awake(b);
                    if *enabled {
                        self.tr_on_enable(b);
                    }
                    self.awaking = false;
                    let at = self.unsynced.iter().rposition(|i| i.objects[0].components[0] == root).expect("the instance being awoken");
                    *inst = self.unsynced.remove(at);
                }
                "LiquidContainerController" => self.liquid_container_awake(linked.liquid_containers[&o]),
                // No Awake; its OnEnable follows here.
                "ColorWave" if *enabled => self.color_wave_on_enable(),
                // OnEnable while the instance is still being built: its colliders come from the instance in hand.
                "TorchBehaviour" if *enabled => {
                    let colliders = Game::torch_child_colliders(inst, o);
                    self.torch_enable_contacts(linked.torches[&o], colliders);
                }
                "DetectorBehaviour" => self.detector_awake(linked.detectors[&o]),
                "TelevisionBehaviour" => self.television_awake(linked.televisions[&o]),
                "EnergySwordBehaviour" => {
                    // Awake: BladeObject.SetActive(false), MotionBlur.SetActive(false).
                    let x = &self.energy_swords[linked.energy_swords[&o]];
                    let (blade, blur) = (x.BladeObject, x.MotionBlur);
                    self.awake_set_inactive(prefab, inst, linked, &mut enabled_behaviours, blade);
                    self.awake_set_inactive(prefab, inst, linked, &mut enabled_behaviours, blur);
                }
                "ImmobilityFieldBehaviour" if *enabled => {
                    // OnEnable: UpdateActivation. Activated shows CaptureObject, which must be active already.
                    let i = linked.immobility_fields[&o];
                    let x = &self.immobility_fields[i];
                    let (on, capture, trigger) = (x.Activated, x.CaptureObject, x.Trigger);
                    match on {
                        true if inst.objects[capture].active => {}
                        true => unimplemented!("ImmobilityFieldBehaviour.OnEnable: CaptureObject.SetActive(true) of an inactive object while the clone is awoken"),
                        false => self.awake_set_inactive(prefab, inst, linked, &mut enabled_behaviours, capture),
                    }
                    self.collider_set_enabled(inst, trigger, on);
                    self.immobility_field_destroy_all_joints(i);
                }
                "CardiopulmonaryBypassMachineBehaviour" if *enabled => {
                    // OnEnable: UpdateActivation. Activated shows the Cosmetics object, which the prefab has active already.
                    let x = &self.cardiopulmonary_bypasses[linked.cardiopulmonary_bypasses[&o]];
                    let cosmetics = x.Cosmetics;
                    match x.Activated {
                        true if inst.objects[cosmetics].active => {}
                        true => unimplemented!("CardiopulmonaryBypassMachineBehaviour.OnEnable: Cosmetics.SetActive(true) of an inactive object while the clone is awoken"),
                        false => self.awake_set_inactive(prefab, inst, linked, &mut enabled_behaviours, cosmetics),
                    }
                }
                "TimedGateBehaviour" => {
                    self.timed_gate_awake(linked.timed_gates[&o]);
                    if *enabled {
                        self.timed_gate_on_enable(linked.timed_gates[&o]);
                    }
                }
                "FirearmAttachmentBehaviour" | "ScopeAttachmentBehaviour" | "BatteryAttachmentBehaviour" | "ExplosiveAttachmentBehaviour" | "FlashlightAttachmentBehaviour" | "IncendiaryAttachmentBehaviour" => self.firearm_attachment_awake(),
                "DisplayBehaviour" => self.display_awake(linked.displays[&o]),
                // offset = Random.Range(-1f, 1f) * 1500f: only OnWillRenderObject reads it.
                "HologramJitter" => {
                    self.random.range(-1.0, 1.0);
                }
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
                "GearBehaviour" => self.gear_awake(inst.objects[o].components[c]),
                "LaunchedRocketBehaviour" => self.rocket_awake(linked.rockets[&o]),
                "BreakJointOnBurnBehaviour" => {
                    let b = self.break_joint_on_burns.iter().position(|x| x.id == inst.objects[o].components[c]).unwrap();
                    self.break_joint_on_burn_awake(b);
                }
                "GenericScifiWeapon40Behaviour" => self.scifi40_awake(),
                "HealthGunBehaviour" | "FreezeGunBehaviour" => self.effect_gun_awake(),
                "AirfoilBehaviour" => self.airfoil_awake(linked.airfoils[&o]),
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
                    let mut calls = Vec::new();
                    for call in invoked_calls(v.get("OnBreak").unwrap()) {
                        match call_target(prefab, file, call) {
                            Some((to, tc)) if matches!(&prefab.objects[to].components[tc].1, Component::Script { class, .. } if class == "ExplosiveBehaviour") && string(call, "m_MethodName").unwrap() == "ForceImmediateExplosion" => {
                                explosives.push(linked.explosives[&to]);
                            }
                            _ if presentation_call(prefab, file, call) => {}
                            Some((to, tc)) if matches!(&prefab.objects[to].components[tc].1, Component::Script { class, .. } if class == "CarBehaviour") => {}
                            Some((to, tc)) if matches!(&prefab.objects[to].components[tc].1, Component::Script { class, .. } if class == "AddForceBehaviour") && string(call, "m_MethodName").unwrap() == "AddRelativeForce" => {
                                let intensity = call.get("m_Arguments").and_then(|a| a.get("m_FloatArgument")).and_then(Value::f32).unwrap();
                                calls.push(EventCall::AddRelativeForce(inst.objects[to].components[tc], intensity));
                            }
                            _ => unimplemented!("JointBreakSoundBehaviour.OnBreak: a persistent call beyond ExplosiveBehaviour.ForceImmediateExplosion, CarBehaviour, AddForceBehaviour.AddRelativeForce, audio and particles"),
                        }
                    }
                    let cars = car_calls(prefab, file, v.get("OnBreak").unwrap(), inst);
                    self.joint_break_sounds.push(super::misc::JointBreakSound {
                        joints,
                        explosives,
                        cars,
                        calls,
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
                "DestroyableBehaviour" | "VaseBehaviour" => Some(Behaviour::Destroyable(linked.destroyables[&o])),
                "LocalFirePropagationMap" => Some(Behaviour::FireMap(linked.fire_maps[&o])),
                "FirearmBehaviour" => Some(Behaviour::Firearm(linked.firearms[&o])),
                "ProjectileLauncherBehaviour" => Some(Behaviour::Launcher(linked.launchers[&o])),
                "BellBehaviour" => Some(Behaviour::Bell(linked.bells[&o])),
                "MachineGunBehaviour" => Some(Behaviour::MachineGun(linked.machine_guns[&o])),
                "MinigunBehaviour" => Some(Behaviour::Minigun(linked.miniguns[&o])),
                "AcceleratorGunBehaviour" => Some(Behaviour::Accelerator(linked.accelerators[&o])),
                "AcceleratorBoltBehaviour" => Some(Behaviour::AcceleratorBolt(linked.accelerator_bolts[&o])),
                "ArchelixCasterBehaviour" => Some(Behaviour::ArchelixCaster(linked.archelix_casters[&o])),
                "ArchelixCasterBoltBehaviour" => Some(Behaviour::ArchelixBolt(linked.archelix_bolts[&o])),
                "UniversalBellBehaviour" => Some(Behaviour::UniversalBell(linked.universal_bells[&o])),
                "ToggleableMirrorBehaviour" => Some(Behaviour::ToggleableMirror(linked.toggleable_mirrors[&o])),
                "ValveBehaviour" => Some(Behaviour::Valve(linked.valves[&o])),
                "TimedGateBehaviour" => Some(Behaviour::TimedGate(linked.timed_gates[&o])),
                "DefibrillatorBehaviour" => Some(Behaviour::Defibrillator(linked.defibrillators[&o])),
                "CardiopulmonaryBypassMachineBehaviour" => Some(Behaviour::CardiopulmonaryBypass(linked.cardiopulmonary_bypasses[&o])),
                "HeartMonitorBehaviour" => Some(Behaviour::HeartMonitor(linked.heart_monitors[&o])),
                "ExcaliburBehaviour" => Some(Behaviour::Excalibur(linked.excaliburs[&o])),
                "SharpOnAllSidesBehaviour" => Some(Behaviour::SharpOnAllSides(linked.sharp_on_all_sides[&o])),
                "BloodTankBehaviour" => Some(Behaviour::BloodTank(linked.blood_tanks[&o])),
                "ExplosiveBehaviour" => Some(Behaviour::Explosive(linked.explosives[&o])),
                "DeleteAfterTime" => Some(Behaviour::DeleteAfter(linked.delete_afters[&o])),
                "PointToVelocityBehaviour" => Some(Behaviour::PointToVelocity(linked.point_to_velocities[&o])),
                "AndroidSparkCreator" => Some(Behaviour::AndroidSpark(linked.android_sparks[&o])),
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
                "BulbBehaviour" => Some(Behaviour::Bulb(linked.bulbs[&o])),
                "ThrusterbedBehaviour" => Some(Behaviour::Thrusterbed(linked.thrusterbeds[&o])),
                "PistonBehaviour" => Some(Behaviour::Piston(linked.pistons[&o])),
                "FlamethrowerBehaviour" => Some(Behaviour::Flamethrower(linked.flamethrowers[&o])),
                "LaunchedRocketBehaviour" => Some(Behaviour::Rocket(linked.rockets[&o])),
                "FancyBloodSplatController" => Some(Behaviour::Splat(self.splats.iter().position(|x| x.id == inst.objects[o].components[c]).unwrap())),
                "GorseDroolingBehaviour" => Some(Behaviour::GorseDrooling(self.gorse_droolings.iter().position(|x| x.id == inst.objects[o].components[c]).unwrap())),
                "SoundOnShakeBehaviour" => Some(Behaviour::SoundOnShake(self.sound_on_shakes.iter().position(|x| x.id == inst.objects[o].components[c]).unwrap())),
                "BreakJointOnBurnBehaviour" => Some(Behaviour::BreakJointOnBurn(self.break_joint_on_burns.iter().position(|x| x.id == inst.objects[o].components[c]).unwrap())),
                "AirfoilBehaviour" => Some(Behaviour::Airfoil(linked.airfoils[&o])),
                "StunnerBehaviour" => Some(Behaviour::Stunner(linked.stunners[&o])),
                "BalloonBehaviour" => Some(Behaviour::Balloon(linked.balloons[&o])),
                "TorchBehaviour" => Some(Behaviour::Torch(linked.torches[&o])),
                "FlareBehaviour" => Some(Behaviour::Flare(linked.flares[&o])),
                "ButtonBehaviour" => Some(Behaviour::Button(linked.buttons[&o])),
                "DisplayBehaviour" => Some(Behaviour::Display(linked.displays[&o])),
                "BatteryAttachmentBehaviour" => Some(Behaviour::BatteryAttachment(linked.firearm_attachments[&o])),
                "TelevisionBehaviour" => Some(Behaviour::Television(linked.televisions[&o])),
                "ScopeAttachmentBehaviour" | "ExplosiveAttachmentBehaviour" | "FlashlightAttachmentBehaviour" | "IncendiaryAttachmentBehaviour" => Some(Behaviour::FirearmAttachment(linked.firearm_attachments[&o])),
                "LaserBehaviour" => Some(Behaviour::Laser(linked.lasers[&o])),
                "MetronomeBehaviour" => Some(Behaviour::Metronome(linked.metronomes[&o])),
                "ThermometerBehaviour" => Some(Behaviour::Thermometer(linked.thermometers[&o])),
                "InfraredThermometer" => Some(Behaviour::InfraredThermometer(linked.infrared_thermometers[&o])),
                "JetEngineBehaviour" => Some(Behaviour::JetEngine(linked.jet_engines[&o])),
                "ChainsawBehaviour" => Some(Behaviour::Chainsaw(linked.chainsaws[&o])),
                "EMPBehaviour" => Some(Behaviour::Emp(linked.emps[&o])),
                "DecimatorBehaviour" => Some(Behaviour::Decimator(linked.decimators[&o])),
                "ImmobilityFieldBehaviour" => Some(Behaviour::ImmobilityField(linked.immobility_fields[&o])),
                "EnergySwordBehaviour" => Some(Behaviour::EnergySword(linked.energy_swords[&o])),
                "BlasterBehaviour" => Some(Behaviour::Blaster(linked.blasters[&o])),
                "BlasterboltBehaviour" => Some(Behaviour::Blasterbolt(linked.blasterbolts[&o])),
                "BeamformerBehaviour" => Some(Behaviour::Beamformer(linked.beamformers[&o])),
                "GenericScifiWeapon40Behaviour" => Some(Behaviour::Scifi40(linked.scifi40s[&o])),
                "HealthGunBehaviour" => Some(Behaviour::HealthGun(linked.effect_guns[&o])),
                "FreezeGunBehaviour" => Some(Behaviour::FreezeGun(linked.effect_guns[&o])),
                "SignalConverterBehaviour" => Some(Behaviour::SignalConverter(linked.signal_converters[&o])),
                c if super::syringe::is_syringe(c) => Some(Behaviour::Syringe(linked.syringes[&o])),
                "PointLiquidTransferBehaviour" => Some(Behaviour::PointLiquid(linked.point_liquids[&o])),
                name if super::translated::CLASSES.contains(&name) => Some(Behaviour::Translated(linked.translated[&(o, c)])),
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
    pub fn queue_starts(&mut self, mut db: Option<&mut AssetDb>, prefab: &Prefab, inst: &Instance, linked: &Linked, order: &[(usize, usize)]) -> Result<(), String> {
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
                "DestroyableBehaviour" | "VaseBehaviour" => PendingStart::Destroyable(linked.destroyables[&o]),
                "UniversalBellBehaviour" => PendingStart::UniversalBell(linked.universal_bells[&o]),
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
                        unimplemented!("FirearmAttachmentPointBehaviour.Start with a FirearmAttachment (a saved, connected contraption: every item prefab holds null)");
                    }
                    PendingStart::FirearmAttachmentPoint
                }
                "GasTankBehaviour" | "FlaskBehaviour" => PendingStart::Flask(linked.flasks[&o]),
                c if super::syringe::is_syringe(c) => PendingStart::Syringe(linked.syringes[&o]),
                "SyringeExplosionBehaviour" => PendingStart::SyringeExplosion(linked.syringe_explosions[&o]),
                "LiquidContainerController" => PendingStart::LiquidContainer(linked.liquid_containers[&o]),
                "DetectorBehaviour" => PendingStart::Detector(linked.detectors[&o]),
                "LaserBehaviour" => PendingStart::Laser(linked.lasers[&o]),
                "SingleFloodlightBehaviour" => PendingStart::Floodlight(linked.floodlights[&o]),
                "SpikeGrenadeBehaviour" => PendingStart::SpikeGrenade(linked.spike_grenades[&o]),
                "BulbBehaviour" => PendingStart::Bulb(linked.bulbs[&o]),
                "ChainsawBehaviour" => PendingStart::Chainsaw(linked.chainsaws[&o]),
                "FreezeBehaviour" => PendingStart::Freeze(linked.freezes[&o]),
                "DecimatorBehaviour" => PendingStart::Decimator(linked.decimators[&o]),
                "ImmobilityFieldBehaviour" => PendingStart::ImmobilityField(linked.immobility_fields[&o]),
                "EnergySwordBehaviour" => PendingStart::EnergySword(linked.energy_swords[&o]),
                "ThrusterbedBehaviour" => PendingStart::Thrusterbed(linked.thrusterbeds[&o]),
                // Start reads Slider.limits: the component's state, taken from the spawn when the Start runs.
                "PistonBehaviour" => {
                    let mut joints = self.autofix_components(prefab, inst, linked, o);
                    joints.sliders.retain(|a| a.object == o);
                    if joints.sliders.len() != 1 {
                        unimplemented!("PistonBehaviour without exactly one SliderJoint2D on its object");
                    }
                    joints.hinges.clear();
                    joints.fixeds.clear();
                    self.autofix_joints.push(joints);
                    PendingStart::Piston(linked.pistons[&o], self.autofix_joints.len() - 1)
                }
                "RandomCarTextureBehaviour" => PendingStart::CarTexture(self.random_car_textures.iter().position(|x| x.id == inst.objects[o].components[c]).unwrap()),
                "DisintegrationCounterBehaviour" if linked.person.is_some() && o == 0 => continue,
                "DisintegrationCounterBehaviour" => PendingStart::DisintegrationCounter(inst.objects[o].components[c]),
                "ExistInDetailView" => PendingStart::ExistInDetailView(inst.objects[o].components[c]),
                "IgnoreParentSize" => PendingStart::IgnoreParentSize(inst.objects[o].components[c]),
                "RandomSpriteBehaviour" => PendingStart::RandomSprite(linked.random_sprites[&o]),
                "AirfoilBehaviour" => PendingStart::Airfoil(linked.airfoils[&o]),
                "MetronomeBehaviour" => PendingStart::Metronome(linked.metronomes[&o]),
                "TimedGateBehaviour" => PendingStart::TimedGate(linked.timed_gates[&o]),
                "CardiopulmonaryBypassMachineBehaviour" => PendingStart::CardiopulmonaryBypass(linked.cardiopulmonary_bypasses[&o]),
                "BloodTankBehaviour" => PendingStart::BloodTank(linked.blood_tanks[&o]),
                "ArchelixCasterBehaviour" => PendingStart::ArchelixCaster(linked.archelix_casters[&o]),
                "ArchelixCasterBoltBehaviour" => PendingStart::ArchelixBolt(linked.archelix_bolts[&o]),
                "MaintainScaleSignBehaviour" => PendingStart::MaintainScale(inst.objects[o].components[c]),
                "FancyBloodSplatController" => PendingStart::Splat(self.splats.iter().position(|x| x.id == inst.objects[o].components[c]).unwrap()),
                "LiquidSpreadAoEBehaviour" => PendingStart::LiquidSpread(self.liquid_spreads.iter().position(|x| x.id == inst.objects[o].components[c]).unwrap()),
                "FoliageBehaviour" => PendingStart::Foliage(self.foliages.iter().position(|x| x.id == inst.objects[o].components[c]).unwrap()),
                "ActivationToggleBehaviour" => PendingStart::ActivationToggle(linked.activation_toggles[&o]),
                "PowerGaugeBehaviour" => PendingStart::PowerGauge(linked.power_gauges[&o]),
                // A Signal Flare that starts Activated enables its TorchBehaviour (not rewritten); otherwise Start does nothing.
                "FlareBehaviour" => PendingStart::Flare(linked.flares[&o]),
                // CameraControlBehaviour takes the box.
                "BoundingBoxBehaviour" => PendingStart::BoundingBox(map_bounds(v)?),
                "MapConfig" => {
                    self.map_config_present = true;
                    let db = db.as_deref_mut().unwrap_or_else(|| unimplemented!("MapConfig.Start on a first GameObject.SetActive(true)"));
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
                name if super::translated::CLASSES.contains(&name) => match linked.translated[&(o, c)] {
                    b if b.has_start() => PendingStart::Translated(b),
                    _ => continue,
                },
                _ => continue,
            };
            self.pending_starts.push(start);
        }
        Ok(())
    }

    /// The PhysicalProperties asset's arena index, loading it on first use (assets are shared by name).
    fn physical_properties(&mut self, db: &mut AssetDb, asset: ObjRef) -> Result<usize, String> {
        let (_, mb, _) = db.mono(asset)?.ok_or("PhysicalProperties is not a MonoBehaviour")?;
        if let Some(i) = self.properties.iter().position(|p| p.name == mb.name) {
            return Ok(i);
        }
        let p = read_physical_properties(db, asset)?;
        self.properties.push(p);
        Ok(self.properties.len() - 1)
    }
}

/// A PhysicalProperties asset as the game reads it.
pub(super) fn read_physical_properties(db: &mut AssetDb, asset: ObjRef) -> Result<PhysicalProperties, String> {
    {
        let (_, mb, v) = db.mono(asset)?.ok_or("PhysicalProperties is not a MonoBehaviour")?;
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
        Ok(p)
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
        g.queue_starts(Some(&mut db), &prefab, &inst, &linked, &order).unwrap();
        // The clone stays with the game like a real spawn's: Starts that look at the hierarchy find it.
        g.unsynced.push(inst);
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

/// The persistent UnityEvent calls of `EventCall` that act on components.
impl Game {
    /// PhysicalBehaviour.HaltNextPropagation: `StartCoroutine(waitOneFrame())`, which yields WaitForSeconds(0.016f)
    /// and then sets ActivationPropagationDelay to -1f. Nothing starts on an inactive GameObject.
    pub(super) fn physical_halt_next_propagation(&mut self, id: i32) {
        let mut waited = false;
        self.start_coroutine(
            id,
            id,
            Box::new(move |g: &mut Game| {
                if !std::mem::replace(&mut waited, true) {
                    return super::coroutine::Yield::WaitForSeconds(0.016);
                }
                if let Some(p) = g.physicals.iter().position(|p| p.id == id && !p.destroyed) {
                    g.physicals[p].ActivationPropagationDelay = -1.0;
                }
                super::coroutine::Yield::Done
            }),
        );
    }

    /// Joint2D.set_breakForce on a prefab joint: the live setter of whichever arena entry owns the component.
    pub(super) fn joint_set_break_force(&mut self, id: i32, v: f32) {
        let slot = self.anchored_joints.iter().position(|a| a.component == id).unwrap_or_else(|| unimplemented!("Joint2D.set_breakForce on a joint outside the anchored joints"));
        if let Some(&f) = self.adopted_slots.get(&slot) {
            self.fixed_joint_set_break_force(f, v);
        } else if let Some(h) = self.anchored_joints[slot].hinge {
            self.set_hinge_break_force(h, v);
        } else {
            let v = crate::b2::weld::clamp_break(v);
            self.anchored_joints[slot].break_force = v;
            let torque = self.anchored_joints[slot].break_torque;
            self.add_break_limits_update(BreakJoint::Anchored(slot), v, torque);
        }
    }

    /// Rigidbody2D::SetSleeping(false) (Rigidbody2D_CUSTOM_WakeUp 180c55140 -> 180c241d0): a live non-static body is
    /// flagged awake with its sleep time zeroed, whether it slept or not; a static one has awake cleared and its
    /// velocities, force and sleep time zeroed.
    pub(super) fn rigidbody_wake_up(&mut self, id: i32) {
        let owner = self.rigidbody_owner(id).expect("Rigidbody2D.WakeUp: a Rigidbody2D outside the arenas");
        let rb = self.owner_rigidbody(owner);
        if rb.detached.is_some() {
            return;
        }
        self.world.body_mut(rb.body).set_sleeping_false();
    }

    /// Rigidbody2D.AddTorque(float), ForceMode2D.Force.
    pub(super) fn rigidbody_add_torque(&mut self, id: i32, torque: f32) {
        let owner = self.rigidbody_owner(id).expect("Rigidbody2D.AddTorque: a Rigidbody2D outside the arenas");
        let rb = self.owner_rigidbody(owner).clone();
        rb.add_torque(&mut self.world, torque);
    }
}

impl Game {
    /// AddForceBehaviour.AddRelativeForce: `intensity` times |lossyScale.x * lossyScale.y| when ScaleWithSize, then
    /// GetComponent<Rigidbody2D>().AddForce(transform.TransformDirection(LocalAxis) * intensity, ForceMode).
    pub(super) fn add_force_relative(&mut self, id: i32, mut intensity: f32) {
        let a = self.add_forces.iter().find(|a| a.id == id).expect("an AddForceBehaviour in the arena").clone();
        let p = a.physical;
        if self.physicals[p].destroyed {
            return;
        }
        if a.ScaleWithSize {
            let s = self.lossy_scale(p);
            intensity *= (s[0] * s[1]).abs();
        }
        let dir = mathf::rotate(self.physicals[p].transform.rotation, [a.LocalAxis[0], a.LocalAxis[1], 0.0]);
        let force = [dir[0] * intensity, dir[1] * intensity];
        let rb = self.physicals[p].rigidbody.clone();
        match a.ForceMode {
            0 => rb.add_force(&mut self.world, force),
            1 => rb.add_force_impulse(&mut self.world, force),
            m => unimplemented!("AddForceBehaviour with ForceMode2D {m}"),
        }
    }
}
