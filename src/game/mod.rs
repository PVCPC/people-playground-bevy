//! The game's own scripts (Assembly-CSharp) rewritten line by line, on top of the b2 world.
//! Field names keep the original C# names so every value can be checked against the running game
//! (tools/recorder PPG_DUMP); methods keep the original statement order and float operation order.
#![allow(non_snake_case, dead_code)]

use std::collections::HashMap;
pub mod ballistics;
pub mod circulation;
pub mod acts;
pub mod car;
pub mod tyre;
pub mod flask;
pub mod light;
pub mod continuous;
pub mod detector;
pub mod sort;
pub mod flamethrower;
pub mod piston;
pub mod thruster;
pub mod devices;
pub mod dump;
pub mod explosion;
pub mod lifecycle;
pub mod destroyable;
pub mod fire;
pub mod firearm;
pub mod launcher;
pub mod bell;
pub mod machinegun;
pub mod translated;
pub mod limb;
pub mod machinery;
pub mod link;
pub mod map;
pub mod misc;
pub mod person;
pub mod physical;
pub mod shatter;
pub mod spawn;
pub mod scene;
pub mod scenery;
pub mod simulate;
pub mod stab;
pub mod temperature;
pub mod tool;
pub mod water;

use crate::b2::{
    math::Vec2 as V,
    world::{BodyId, BodyType, JointId, World, BODY_AWAKE, BODY_BULLET, BODY_FIXED_ROTATION},
};
use crate::unity::Random;

pub use circulation::Circulation;
pub use limb::Limb;
pub use map::{MapLight, PlayAudioAtRandom};
pub use misc::{BleedingParticle, FancyBloodSplat, GForce, Grip, Node, Skin};
pub use person::Person;
pub use physical::{Physical, PhysicalProperties};

/// A C# float expression as Mono evaluates it: in double precision, rounded to float by `.f()`
/// wherever the value is stored (local, field, argument, return). Single operations round the same
/// either way; chains do not.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct D(pub f64);

pub fn d(v: f32) -> D {
    D(v as f64)
}

impl D {
    pub fn f(self) -> f32 {
        self.0 as f32
    }
}

macro_rules! d_ops {
    ($($tr:ident $m:ident $op:tt),*) => {$(
        impl std::ops::$tr<D> for D { type Output = D; fn $m(self, o: D) -> D { D(self.0 $op o.0) } }
        impl std::ops::$tr<f32> for D { type Output = D; fn $m(self, o: f32) -> D { D(self.0 $op o as f64) } }
        impl std::ops::$tr<D> for f32 { type Output = D; fn $m(self, o: D) -> D { D(self as f64 $op o.0) } }
    )*};
}
d_ops!(Add add +, Sub sub -, Mul mul *, Div div /);

impl std::ops::Neg for D {
    type Output = D;
    fn neg(self) -> D {
        D(-self.0)
    }
}

/// UnityEngine.Mathf and the Vector2/Quaternion operators, in the original float operation order.
pub mod mathf {
    use super::d;
    pub const EPSILON: f32 = f32::from_bits(1);
    pub const RAD2DEG: f32 = f32::from_bits(0x42652ee1);
    pub const DEG2RAD: f32 = f32::from_bits(0x3c8efa35);

    pub fn clamp01(v: f32) -> f32 {
        if v < 0.0 {
            0.0
        } else if v > 1.0 {
            1.0
        } else {
            v
        }
    }
    pub fn clamp(mut v: f32, min: f32, max: f32) -> f32 {
        if v < min {
            v = min;
        } else if v > max {
            v = max;
        }
        v
    }
    /// Mono evaluates each expression in double and rounds at every store (locals included);
    /// CoreModule is a Debug build, so even inlined Mathf calls round through their result local.
    pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
        (d(a) + (d(b) - a) * clamp01(t)).f()
    }
    pub fn max(a: f32, b: f32) -> f32 {
        if a > b { a } else { b }
    }
    pub fn min(a: f32, b: f32) -> f32 {
        if a < b { a } else { b }
    }
    /// Mathf.Pow/Sin/Sqrt go through System.Math in double precision.
    pub fn pow(f: f32, p: f32) -> f32 {
        (f as f64).powf(p as f64) as f32
    }
    pub fn sin(f: f32) -> f32 {
        (f as f64).sin() as f32
    }
    pub fn cos(f: f32) -> f32 {
        (f as f64).cos() as f32
    }
    pub fn sqrt(f: f32) -> f32 {
        (f as f64).sqrt() as f32
    }
    /// Vector2.magnitude: (float)Math.Sqrt(x * x + y * y), in double.
    pub fn magnitude2(v: [f32; 2]) -> f32 {
        (super::d(v[0]) * v[0] + super::d(v[1]) * v[1]).0.sqrt() as f32
    }
    /// Vector2.normalized (magnitude is (float)Math.Sqrt(x * x + y * y), in double).
    pub fn normalized2(v: [f32; 2]) -> [f32; 2] {
        let num = (super::d(v[0]) * v[0] + super::d(v[1]) * v[1]).0.sqrt() as f32;
        if num > 1e-5 {
            [v[0] / num, v[1] / num]
        } else {
            [0.0, 0.0]
        }
    }
    pub fn floor(f: f32) -> f32 {
        (f as f64).floor() as f32
    }
    pub fn repeat(t: f32, length: f32) -> f32 {
        clamp((d(t) - d(floor(t / length)) * length).f(), 0.0, length)
    }
    pub fn delta_angle(current: f32, target: f32) -> f32 {
        let mut num = repeat(target - current, 360.0);
        if num > 180.0 {
            num -= 360.0;
        }
        num
    }
    pub fn approximately(a: f32, b: f32) -> bool {
        (b - a).abs() < max(1e-6 * max(a.abs(), b.abs()), EPSILON * 8.0)
    }
    /// Utils.MapRange
    pub fn map_range(lower1: f32, upper1: f32, lower2: f32, upper2: f32, value: f32) -> f32 {
        (d(lower2) + (d(value) - lower1) * (d(upper2) - lower2) / (d(upper1) - lower1)).f()
    }
    /// Utils.GetLerpFactorDeltaTime
    pub fn lerp_factor_dt(lerp_factor: f32, dt: f32) -> f32 {
        1.0 - pow(1.0 - lerp_factor, dt)
    }
    pub fn perlin(x: f32, y: f32) -> f32 {
        crate::unity::perlin_noise(x, y)
    }
    /// Utils.GetPerlin2Mapped
    pub fn perlin2_mapped(x: f32, y: f32) -> [f32; 2] {
        [perlin(x, y) * 2.0 - 1.0, perlin(y, x) * 2.0 - 1.0]
    }
    pub fn v2_lerp(a: [f32; 2], b: [f32; 2], t: f32) -> [f32; 2] {
        let t = clamp01(t) as f64;
        let l = |a: f32, b: f32| (a as f64 + (b as f64 - a as f64) * t) as f32;
        [l(a[0], b[0]), l(a[1], b[1])]
    }
    /// Vector2.sqrMagnitude (not inlined: rounded once at its return).
    pub fn sqr_magnitude(v: [f32; 2]) -> f32 {
        (d(v[0]) * v[0] + d(v[1]) * v[1]).f()
    }
    /// Quaternion * Vector3 (UnityEngine.Quaternion operator*).
    pub fn rotate(q: [f32; 4], p: [f32; 3]) -> [f32; 3] {
        let [x, y, z, w] = q;
        let (n1, n2, n3) = (x * 2.0, y * 2.0, z * 2.0);
        let (n4, n5, n6) = (x * n1, y * n2, z * n3);
        let (n7, n8, n9) = (x * n2, x * n3, y * n3);
        let (n10, n11, n12) = (w * n1, w * n2, w * n3);
        // Each component is one double-evaluated chain (Mono), rounded at the field store.
        [
            ((d(1.0) - (d(n5) + n6)) * p[0] + (d(n7) - n12) * p[1] + (d(n8) + n11) * p[2]).f(),
            ((d(n7) + n12) * p[0] + (d(1.0) - (d(n4) + n6)) * p[1] + (d(n9) - n10) * p[2]).f(),
            ((d(n8) - n11) * p[0] + (d(n9) + n10) * p[1] + (d(1.0) - (d(n4) + n5)) * p[2]).f(),
        ]
    }
}

/// One value the recorder dumps: floats as hex bits, vectors as comma lists, booleans as 0/1.
pub trait Field {
    fn parse(&mut self, text: &str) -> Result<(), String>;
    fn show(&self) -> String;
    /// Loads the field's serialized value from game data (a prefab's MonoBehaviour).
    fn set(&mut self, v: &crate::assets::mono::Value) -> Result<(), String>;
}

fn bits(text: &str) -> Result<f32, String> {
    u32::from_str_radix(text, 16).map(f32::from_bits).map_err(|_| format!("bad float bits {text}"))
}

impl Field for f32 {
    fn parse(&mut self, text: &str) -> Result<(), String> {
        *self = bits(text)?;
        Ok(())
    }
    fn show(&self) -> String {
        format!("{:08x}", self.to_bits())
    }
    fn set(&mut self, v: &crate::assets::mono::Value) -> Result<(), String> {
        *self = v.f32().ok_or(format!("not a float: {v:?}"))?;
        Ok(())
    }
}
impl Field for bool {
    fn parse(&mut self, text: &str) -> Result<(), String> {
        *self = match text {
            "1" => true,
            "0" => false,
            _ => return Err(format!("bad bool {text}")),
        };
        Ok(())
    }
    fn show(&self) -> String {
        (if *self { "1" } else { "0" }).into()
    }
    fn set(&mut self, v: &crate::assets::mono::Value) -> Result<(), String> {
        *self = v.bool().ok_or(format!("not a bool: {v:?}"))?;
        Ok(())
    }
}
macro_rules! int_field {
    ($($t:ty),*) => {$(
        impl Field for $t {
            fn parse(&mut self, text: &str) -> Result<(), String> {
                *self = text.parse().map_err(|_| format!("bad integer {text}"))?;
                Ok(())
            }
            fn show(&self) -> String {
                self.to_string()
            }
            fn set(&mut self, v: &crate::assets::mono::Value) -> Result<(), String> {
                *self = v.i64().and_then(|i| <$t>::try_from(i).ok()).ok_or(format!("not a {}: {v:?}", stringify!($t)))?;
                Ok(())
            }
        }
    )*};
}
int_field!(i32, u16, u8);
impl<const N: usize> Field for [f32; N] {
    fn parse(&mut self, text: &str) -> Result<(), String> {
        let parts: Vec<&str> = text.split(',').collect();
        if parts.len() != N {
            return Err(format!("expected {N} values: {text}"));
        }
        for (v, p) in self.iter_mut().zip(parts) {
            *v = bits(p)?;
        }
        Ok(())
    }
    fn show(&self) -> String {
        self.iter().map(Field::show).collect::<Vec<_>>().join(",")
    }
    /// Vector2/3/4, Quaternion and Color: the struct's float members in order.
    fn set(&mut self, v: &crate::assets::mono::Value) -> Result<(), String> {
        let crate::assets::mono::Value::Struct(members) = v else { return Err(format!("not a struct: {v:?}")) };
        if members.len() != N {
            return Err(format!("expected {N} members: {v:?}"));
        }
        for (x, (_, m)) in self.iter_mut().zip(members) {
            x.set(m)?;
        }
        Ok(())
    }
}
impl Field for Vec<f32> {
    fn parse(&mut self, text: &str) -> Result<(), String> {
        *self = if text.is_empty() { Vec::new() } else { text.split(',').map(bits).collect::<Result<_, _>>()? };
        Ok(())
    }
    fn show(&self) -> String {
        self.iter().map(Field::show).collect::<Vec<_>>().join(",")
    }
    /// float[]: the array's elements.
    fn set(&mut self, v: &crate::assets::mono::Value) -> Result<(), String> {
        let crate::assets::mono::Value::Array(items) = v else { return Err(format!("not an array: {v:?}")) };
        *self = items.iter().map(|x| x.f32().ok_or(format!("not a float: {x:?}"))).collect::<Result<_, _>>()?;
        Ok(())
    }
}
impl Field for Option<f32> {
    fn parse(&mut self, text: &str) -> Result<(), String> {
        *self = if text == "null" { None } else { Some(bits(text)?) };
        Ok(())
    }
    fn show(&self) -> String {
        self.map_or("null".into(), |v| v.show())
    }
    fn set(&mut self, v: &crate::assets::mono::Value) -> Result<(), String> {
        Err(format!("Option<f32> has no serialized form: {v:?}"))
    }
}

/// A rewritten MonoBehaviour: `dumped` fields are the C# fields the oracle loads and compares,
/// `links` are this rewrite's references to other components and engine objects.
macro_rules! script {
    ($(#[$m:meta])* $name:ident { $($(#[$fm:meta])* $field:ident : $ty:ty = $default:expr),* $(,)? }
     links { $($(#[$lm:meta])* $lfield:ident : $lty:ty = $ldefault:expr),* $(,)? }) => {
        $(#[$m])*
        #[derive(Clone, Debug)]
        pub struct $name {
            $($(#[$fm])* pub $field: $ty,)*
            $($(#[$lm])* pub $lfield: $lty,)*
        }
        impl Default for $name {
            fn default() -> Self {
                Self { $($field: $default,)* $($lfield: $ldefault,)* }
            }
        }
        impl $name {
            pub fn fields(&mut self) -> Vec<(&'static str, &mut dyn $crate::game::Field)> {
                vec![$((stringify!($field), &mut self.$field as &mut dyn $crate::game::Field)),*]
            }
        }
    };
}
pub(crate) use script;

/// UnityEngine.Time as the scripts see it.
#[derive(Clone, Debug, Default)]
pub struct Time {
    pub time: f32,
    pub timeAsDouble: f64,
    pub fixedTime: f32,
    pub fixedTimeAsDouble: f64,
    pub deltaTime: f32,
    pub fixedDeltaTime: f32,
    pub maximumDeltaTime: f32,
    pub timeScale: f32,
    pub frameCount: i32,
    pub unscaledDeltaTime: f32,
    /// Time.smoothDeltaTime and TimeManager's weight counter (+0x80 and +0x84): the recordings lack them, so a loaded
    /// one starts at the steady state (the counter at its fixed point 1, the average at the frame's dt).
    pub smoothDeltaTime: f32,
    pub smoothCount: f32,
}

/// UnityEngine.Bounds in the xy plane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub center: [f32; 2],
    pub extents: [f32; 2],
}

impl Default for Bounds {
    /// The Default map prefab's BoundingBoxBehaviour.
    fn default() -> Self {
        Bounds { center: [572.5, 496.4], extents: [578.4, 496.6] }
    }
}

impl Bounds {
    /// Utils.ContainsExpanded
    pub fn contains_expanded(&self, p: [f32; 2], expansion: f32) -> bool {
        let min = [self.center[0] - self.extents[0], self.center[1] - self.extents[1]];
        let max = [self.center[0] + self.extents[0], self.center[1] + self.extents[1]];
        p[0] > min[0] - expansion && p[0] < max[0] + expansion && p[1] > min[1] - expansion && p[1] < max[1] + expansion
    }
}

/// UserPreferenceManager.Current: the values the simulation reads (config.json in the game folder).
#[derive(Clone, Debug)]
pub struct Prefs {
    pub CollisionQuality: i32,
    pub AmbientTemperatureTransfer: bool,
    pub BrainDamage: bool,
    pub AutoHealWounds: bool,
    pub StopAnimationOnDamage: bool,
    pub FragilityMultiplier: f32,
    pub LimbCrushing: bool,
    pub GorelessMode: bool,
    pub DropOnDeath: bool,
    pub DismembermentLooseTissue: bool,
    pub CrushForceMultiplier: f32,
    pub ProceduralFragments: bool,
    pub FancyEffects: bool,
    pub Decals: bool,
    pub ToolSelectionMode: i32,
    pub GridSnap: f32,
    pub PhysicsIterations: i32,
    pub ShowTutorial: bool,
    pub DistantSoundEffects: bool,
    pub TracerBullets: bool,
    pub BulletCrackWhiz: bool,
    pub ChunkyShotParticles: bool,
    pub ShakeIntensity: f32,
}

/// CollisionQuality.Dynamic
pub const COLLISION_DYNAMIC: i32 = 2;

impl Default for Prefs {
    fn default() -> Self {
        Self {
            CollisionQuality: COLLISION_DYNAMIC,
            AmbientTemperatureTransfer: false,
            BrainDamage: true,
            AutoHealWounds: true,
            StopAnimationOnDamage: false,
            FragilityMultiplier: 1.0,
            LimbCrushing: true,
            GorelessMode: false,
            DropOnDeath: true,
            DismembermentLooseTissue: true,
            CrushForceMultiplier: 1.0,
            ProceduralFragments: false,
            FancyEffects: true,
            Decals: true,
            ToolSelectionMode: tool::SELECTION_AREA,
            GridSnap: 0.25,
            PhysicsIterations: 32,
            ShowTutorial: true,
            DistantSoundEffects: true,
            TracerBullets: true,
            BulletCrackWhiz: true,
            ChunkyShotParticles: true,
            ShakeIntensity: 1.0,
        }
    }
}

impl Prefs {
    /// UserPreferenceManager.Load: config.json in the game folder (the working directory), deserialized by
    /// Newtonsoft over the defaults with unknown members ignored; without the file, the defaults.
    pub fn load(game: &std::path::Path) -> Prefs {
        match std::fs::read_to_string(game.join("config.json")) {
            Ok(text) => Prefs::from_json(&text).unwrap_or_else(|e| unimplemented!("UserPreferenceManager.LoadFromLegacyStorage (config.json: {e})")),
            Err(_) => Prefs::default(),
        }
    }

    /// The members the simulation reads. An enum takes its name or its number (Newtonsoft's StringEnumConverter
    /// writes names); a value of the wrong type fails the whole file.
    pub fn from_json(text: &str) -> Result<Prefs, String> {
        let json: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let obj = json.as_object().ok_or("not an object")?;
        let mut p = Prefs::default();
        for (k, v) in obj {
            let bool = || v.as_bool().ok_or_else(|| format!("{k}: not a bool"));
            let float = || v.as_f64().map(|x| x as f32).ok_or_else(|| format!("{k}: not a number"));
            let int = || v.as_i64().and_then(|x| i32::try_from(x).ok()).ok_or_else(|| format!("{k}: not an int"));
            let enumeration = |names: &[&str]| match v {
                serde_json::Value::String(s) => names.iter().position(|n| n.eq_ignore_ascii_case(s)).map(|i| i as i32).ok_or_else(|| format!("{k}: no member {s}")),
                _ => int(),
            };
            match k.as_str() {
                "CollisionQuality" => p.CollisionQuality = enumeration(&["Discrete", "Continuous", "Dynamic"])?,
                "AmbientTemperatureTransfer" => p.AmbientTemperatureTransfer = bool()?,
                "BrainDamage" => p.BrainDamage = bool()?,
                "AutoHealWounds" => p.AutoHealWounds = bool()?,
                "StopAnimationOnDamage" => p.StopAnimationOnDamage = bool()?,
                "FragilityMultiplier" => p.FragilityMultiplier = float()?,
                "LimbCrushing" => p.LimbCrushing = bool()?,
                "GorelessMode" => p.GorelessMode = bool()?,
                "DropOnDeath" => p.DropOnDeath = bool()?,
                "DismembermentLooseTissue" => p.DismembermentLooseTissue = bool()?,
                "CrushForceMultiplier" => p.CrushForceMultiplier = float()?,
                "ProceduralFragments" => p.ProceduralFragments = bool()?,
                "FancyEffects" => p.FancyEffects = bool()?,
                "Decals" => p.Decals = bool()?,
                "ToolSelectionMode" => p.ToolSelectionMode = enumeration(&["Area", "Mass"])?,
                "GridSnap" => p.GridSnap = float()?,
                "PhysicsIterations" => p.PhysicsIterations = int()?,
                "ShowTutorial" => p.ShowTutorial = bool()?,
                "DistantSoundEffects" => p.DistantSoundEffects = bool()?,
                "TracerBullets" => p.TracerBullets = bool()?,
                "BulletCrackWhiz" => p.BulletCrackWhiz = bool()?,
                "ChunkyShotParticles" => p.ChunkyShotParticles = bool()?,
                "ShakeIntensity" => p.ShakeIntensity = float()?,
                _ => {}
            }
        }
        Ok(p)
    }
}

/// System.Collections.Generic.Dictionary enumeration order: insertion order, with removed slots
/// reused by later insertions (most recently freed first).
#[derive(Clone, Debug)]
pub struct CsDict<K, V> {
    entries: Vec<Option<(K, V)>>,
    free: Vec<usize>,
}

impl<K, V> Default for CsDict<K, V> {
    fn default() -> Self {
        Self { entries: Vec::new(), free: Vec::new() }
    }
}

impl<K: PartialEq + Copy, V> CsDict<K, V> {
    pub fn get(&self, key: K) -> Option<&V> {
        self.entries.iter().flatten().find(|(k, _)| *k == key).map(|(_, v)| v)
    }
    pub fn get_mut(&mut self, key: K) -> Option<&mut V> {
        self.entries.iter_mut().flatten().find(|(k, _)| *k == key).map(|(_, v)| v)
    }
    pub fn insert(&mut self, key: K, value: V) {
        match self.free.pop() {
            Some(slot) => self.entries[slot] = Some((key, value)),
            None => self.entries.push(Some((key, value))),
        }
    }
    pub fn remove(&mut self, key: K) -> bool {
        match self.entries.iter().position(|e| matches!(e, Some((k, _)) if *k == key)) {
            Some(slot) => {
                self.entries[slot] = None;
                self.free.push(slot);
                true
            }
            None => false,
        }
    }
    pub fn len(&self) -> usize {
        self.entries.len() - self.free.len()
    }
    pub fn keys(&self) -> Vec<K> {
        self.entries.iter().flatten().map(|(k, _)| *k).collect()
    }
    pub fn iter(&self) -> impl Iterator<Item = &(K, V)> {
        self.entries.iter().flatten()
    }
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut (K, V)> {
        self.entries.iter_mut().flatten()
    }
}

/// UnityEngine.Rigidbody2D over a b2 body (RigidbodyType2D: 0 dynamic, 1 kinematic, 2 static).
#[derive(Clone, Debug)]
pub struct Rigidbody {
    pub body: BodyId,
    pub bodyType: i32,
    pub collisionDetectionMode: i32,
    pub interpolation: i32,
    pub movement: MovementState,
    /// Set once Rigidbody2D::Cleanup has destroyed the b2 body (m_Body null): the component's own mass fields.
    pub detached: Option<crate::b2::world::RigidbodyMass>,
    /// The Rigidbody2D component's instance id.
    pub component: i32,
    /// Rigidbody2D.simulated (+0x48): PhysicsManager2D::Simulate's write-back and Update's interpolation skip a body
    /// without it.
    pub simulated: bool,
    /// While detached, the component's linear drag, angular drag and gravity scale (the next Create reads them).
    pub detached_props: [f32; 3],
}

/// RigidbodyMovementState2D (Rigidbody2D+0x98): the body pose before and after the last physics step, and the
/// pose PhysicsManager2D::Update interpolates between them for the Transform.
#[derive(Clone, Copy, Debug, Default)]
pub struct MovementState {
    pub previous: ([f32; 3], [f32; 4]),
    pub current: ([f32; 3], [f32; 4]),
    pub body: ([f32; 3], [f32; 4]),
    pub interpolating: bool,
    /// In PhysicsScene2D.m_RigidbodyMovementStates (m_MovementIndex != -1).
    // ponytail: membership only; the index is the scripts' Start order and never changes a result.
    pub listed: bool,
}

impl Rigidbody {
    /// The movement state's interpolation flag with a non-None mode (both checked by the interpolation pass).
    pub fn interpolating(&self) -> bool {
        self.movement.listed && self.movement.interpolating && self.interpolation != 0
    }

    pub fn velocity(&self, w: &World) -> [f32; 2] {
        if self.detached.is_some() || self.bodyType == 2 {
            return [0.0, 0.0];
        }
        let b = w.body(self.body);
        [b.linear_velocity.x, b.linear_velocity.y]
    }
    /// Rigidbody2D::SetVelocity
    pub fn set_velocity(&self, w: &mut World, v: [f32; 2]) {
        if self.bodyType == 2 || self.detached.is_some() {
            return;
        }
        let b = w.body_mut(self.body);
        if b.kind != BodyType::Static {
            if 0.0 < v[1] * v[1] + v[0] * v[0] {
                b.flags |= BODY_AWAKE;
                b.sleep_time = 0.0;
            }
            b.linear_velocity = V::new(v[0], v[1]);
        }
    }
    /// Rigidbody2D::GetAngularVelocity, degrees per second.
    pub fn angular_velocity(&self, w: &World) -> f32 {
        if self.detached.is_some() || self.bodyType == 2 { 0.0 } else { w.body(self.body).angular_velocity * mathf::RAD2DEG }
    }
    /// Rigidbody2D::SetAngularVelocity
    pub fn set_angular_velocity(&self, w: &mut World, degrees: f32) {
        if self.bodyType == 2 || self.detached.is_some() {
            return;
        }
        let b = w.body_mut(self.body);
        let v = degrees * mathf::DEG2RAD;
        if b.kind != BodyType::Static && b.flags & BODY_FIXED_ROTATION == 0 {
            if 0.0 < v * v {
                b.flags |= BODY_AWAKE;
                b.sleep_time = 0.0;
            }
            b.angular_velocity = v;
        }
    }
    /// Rigidbody2D::AddTorque, ForceMode2D.Force
    pub fn add_torque(&self, w: &mut World, torque: f32) {
        if self.detached.is_some() || self.bodyType != 0 {
            return;
        }
        let b = w.body_mut(self.body);
        if b.kind == BodyType::Dynamic {
            if b.flags & BODY_AWAKE == 0 {
                b.sleep_time = 0.0;
                b.flags |= BODY_AWAKE;
            }
            b.torque += torque;
        }
    }
    pub fn mass(&self, w: &World) -> f32 {
        self.detached.unwrap_or_else(|| w.body(self.body).rigidbody).mass
    }
    /// Rigidbody2D::AddForce (180c1bf80) with ForceMode2D.Impulse: a Dynamic Rigidbody2D's body wakes and its
    /// velocity gains impulse times the inverse mass.
    pub fn add_force_impulse(&self, w: &mut World, impulse: [f32; 2]) {
        if self.detached.is_some() || self.bodyType != 0 {
            return;
        }
        let b = w.body_mut(self.body);
        if b.kind != BodyType::Dynamic {
            return;
        }
        if b.flags & BODY_AWAKE == 0 {
            b.flags |= BODY_AWAKE;
            b.sleep_time = 0.0;
        }
        b.linear_velocity = V::new(impulse[0] * b.inv_mass + b.linear_velocity.x, impulse[1] * b.inv_mass + b.linear_velocity.y);
    }

    /// Rigidbody2D::AddTorque (180c1c320) with ForceMode2D.Impulse: a Dynamic Rigidbody2D's body wakes and its
    /// angular velocity gains the impulse times the inverse inertia.
    pub fn add_torque_impulse(&self, w: &mut World, impulse: f32) {
        if self.detached.is_some() || self.bodyType != 0 {
            return;
        }
        let b = w.body_mut(self.body);
        if b.kind != BodyType::Dynamic {
            return;
        }
        if b.flags & BODY_AWAKE == 0 {
            b.flags |= BODY_AWAKE;
            b.sleep_time = 0.0;
        }
        b.angular_velocity = impulse * b.inv_i + b.angular_velocity;
    }
    /// Rigidbody2D::AddForce (180c1bf80) with ForceMode2D.Force: a dynamic Rigidbody2D that has a body.
    pub fn add_force(&self, w: &mut World, force: [f32; 2]) {
        if self.detached.is_some() || self.bodyType != 0 {
            return;
        }
        w.body_mut(self.body).apply_force_to_center(V::new(force[0], force[1]));
    }
    /// Rigidbody2D::AddRelativeForce (180c1c1b0) with ForceMode2D.Force: the force turned by the body's rotation.
    pub fn add_relative_force(&self, w: &mut World, force: [f32; 2]) {
        if self.detached.is_some() || self.bodyType != 0 {
            return;
        }
        let q = w.body(self.body).xf.q;
        w.body_mut(self.body).apply_force_to_center(V::new(q.c * force[0] - q.s * force[1], q.s * force[0] + q.c * force[1]));
    }
    /// Rigidbody2D::AddForceAtPosition (180c1c030) with ForceMode2D.Impulse.
    pub fn add_impulse_at_position(&self, w: &mut World, impulse: [f32; 2], point: [f32; 2]) {
        if self.detached.is_some() || self.bodyType != 0 {
            return;
        }
        w.body_mut(self.body).apply_impulse_at(V::new(impulse[0], impulse[1]), V::new(point[0], point[1]));
    }
    /// Rigidbody2D::AddForceAtPosition (180c1c030) with ForceMode2D.Force: a dynamic Rigidbody2D that has a body.
    pub fn add_force_at_position(&self, w: &mut World, force: [f32; 2], point: [f32; 2]) {
        if self.detached.is_some() || self.bodyType != 0 {
            return;
        }
        w.body_mut(self.body).apply_force(V::new(force[0], force[1]), V::new(point[0], point[1]));
    }
    /// Rigidbody2D::GetPointVelocity (180c1fa10), after the AutoSyncTransforms the caller runs: zero without a body
    /// or for a static Rigidbody2D.
    pub fn point_velocity(&self, w: &World, point: [f32; 2]) -> [f32; 2] {
        if self.detached.is_some() || self.bodyType == 2 {
            return [0.0, 0.0];
        }
        let v = w.body(self.body).velocity_at(V::new(point[0], point[1]));
        [v.x, v.y]
    }
    /// Rigidbody2D::SetCollisionDetectionMode (CollisionDetectionMode2D: 0 discrete, 1 continuous)
    pub fn set_collision_detection_mode(&mut self, w: &mut World, mode: i32) {
        self.collisionDetectionMode = mode;
        if self.detached.is_some() {
            return;
        }
        let b = w.body_mut(self.body);
        if mode == 1 {
            b.flags |= BODY_BULLET;
        } else {
            b.flags &= !BODY_BULLET;
        }
    }
    /// Rigidbody2D::GetRotation, degrees.
    pub fn rotation(&self, w: &World) -> f32 {
        w.body(self.body).sweep.a * mathf::RAD2DEG
    }
}

/// UnityEngine.HingeJoint2D: the managed-side motor and limits plus its b2 revolute joint.
#[derive(Clone, Debug)]
pub struct Hinge {
    pub joint: Option<JointId>,
    pub useMotor: bool,
    /// JointMotor2D (motorSpeed degrees/s, maxMotorTorque)
    pub motor: [f32; 2],
    pub useLimits: bool,
    /// JointAngleLimits2D (min, max) degrees
    pub limits: [f32; 2],
    pub breakForce: f32,
    pub breakTorque: f32,
    /// connectedBody: the Rigidbody2D's b2 body (None: the scene's ground body).
    pub connected: Option<BodyId>,
    pub anchor: [f32; 2],
    pub connectedAnchor: [f32; 2],
    pub enabled: bool,
    /// The component was destroyed (a broken joint).
    pub destroyed: bool,
}

/// b2Body::SetAwake(true) as the joint setters inline it: the sleep timer always restarts.
fn wake_for_joint(w: &mut World, id: BodyId) {
    let b = w.body_mut(id);
    b.sleep_time = 0.0;
    if b.kind == BodyType::Static {
        b.flags &= !BODY_AWAKE;
        b.linear_velocity = V::new(0.0, 0.0);
        b.angular_velocity = 0.0;
        b.force = V::new(0.0, 0.0);
        b.torque = 0.0;
    } else {
        b.flags |= BODY_AWAKE;
    }
}

impl Hinge {
    /// HingeJoint2D::GetJointAngle, degrees.
    pub fn joint_angle(&self, w: &World) -> f32 {
        let Some(id) = self.joint else { return 0.0 };
        let j = w.joints[id].as_ref().unwrap();
        (w.body(j.body_b).sweep.a - w.body(j.body_a).sweep.a - j.revolute().def.reference_angle) * mathf::RAD2DEG
    }
    /// HingeJoint2D::GetReferenceAngle, degrees.
    pub fn reference_angle(&self, w: &World) -> f32 {
        self.joint.map_or(0.0, |id| w.joints[id].as_ref().unwrap().revolute().def.reference_angle * mathf::RAD2DEG)
    }
    /// HingeJoint2D::SetMotor (the `Joint.motor = motor` setter).
    pub fn set_motor(&mut self, w: &mut World, speed: f32, torque: f32) {
        let mut s = -1_000_000.0;
        if -1_000_000.0 <= speed && { s = 1_000_000.0; speed <= 1_000_000.0 } {
            s = speed;
        }
        let mut t = 0.0;
        if 0.0 <= torque && { t = 1_000_000.0; torque <= 1_000_000.0 } {
            t = torque;
        }
        self.motor = [s, t];
        self.useMotor = true;
        let Some(id) = self.joint else { return };
        let j = w.joints[id].as_ref().unwrap();
        let (a, b) = (j.body_a, j.body_b);
        let def = &j.revolute().def;
        let (enable, old_speed, old_torque) = (def.enable_motor, def.motor_speed, def.max_motor_torque);
        let speed = s * mathf::DEG2RAD;
        if !enable {
            wake_for_joint(w, a);
            wake_for_joint(w, b);
            w.joints[id].as_mut().unwrap().revolute_mut().def.enable_motor = true;
        }
        if speed != old_speed {
            wake_for_joint(w, a);
            wake_for_joint(w, b);
            w.joints[id].as_mut().unwrap().revolute_mut().def.motor_speed = speed;
        }
        if t != old_torque {
            wake_for_joint(w, a);
            wake_for_joint(w, b);
            w.joints[id].as_mut().unwrap().revolute_mut().def.max_motor_torque = t;
        }
    }
    /// HingeJoint2D::SetLimits (the `Joint.limits = limits` setter).
    pub fn set_limits(&mut self, w: &mut World, min: f32, max: f32) {
        let clamp = |v: f32| {
            let mut r = -1_000_000.0;
            if -1_000_000.0 <= v && { r = 1_000_000.0; v <= 1_000_000.0 } {
                r = v;
            }
            r
        };
        self.limits = [clamp(min), clamp(max)];
        self.useLimits = true;
        let Some(id) = self.joint else { return };
        let j = w.joints[id].as_ref().unwrap();
        let (a, b) = (j.body_a, j.body_b);
        if !j.revolute().def.enable_limit {
            wake_for_joint(w, a);
            wake_for_joint(w, b);
            let r = w.joints[id].as_mut().unwrap().revolute_mut();
            r.def.enable_limit = true;
            r.impulse[2] = 0.0;
        }
        let (mut lower, mut upper) = (self.limits[0] * mathf::DEG2RAD, self.limits[1] * mathf::DEG2RAD);
        if upper < lower {
            std::mem::swap(&mut lower, &mut upper);
        }
        let def = &w.joints[id].as_ref().unwrap().revolute().def;
        if lower != def.lower_angle || upper != def.upper_angle {
            wake_for_joint(w, a);
            wake_for_joint(w, b);
            let r = w.joints[id].as_mut().unwrap().revolute_mut();
            r.def.lower_angle = lower;
            r.def.upper_angle = upper;
            r.impulse[2] = 0.0;
        }
    }
    /// Joint2D::SetBreakForce / SetBreakTorque clamp to [0, inf].
    pub fn set_break_force(&mut self, v: f32) {
        self.breakForce = if 0.0 <= v && v <= f32::INFINITY { v } else if 0.0 <= v { f32::INFINITY } else { 0.0 };
    }
    pub fn set_break_torque(&mut self, v: f32) {
        self.breakTorque = if 0.0 <= v && v <= f32::INFINITY { v } else if 0.0 <= v { f32::INFINITY } else { 0.0 };
    }
}

/// What a bare collider's Transform hangs from.
#[derive(Clone, Copy, Debug, Default)]
pub enum BareAnchor {
    Physical(usize),
    Node(usize),
    #[default]
    Root,
}

/// A Collider2D's GameObject without a PhysicalBehaviour: its local TRS and those of its ancestors up to the nearest
/// physical or transform node (object first). Every Rigidbody2D's object is one of those, so nothing in the chain
/// moves.
#[derive(Clone, Debug)]
pub struct BareCollider {
    pub anchor: BareAnchor,
    pub chain: Vec<crate::unity::transform::Trs>,
    /// The GameObject's MonoBehaviour classes (SendMessage receivers).
    pub scripts: Vec<String>,
    /// GameObject.activeSelf.
    pub active_self: bool,
}

impl BareCollider {
    /// Whether SendMessage(message) finds a receiver among its scripts (the classes declaring the method).
    pub fn receives(&self, receivers: &[&str]) -> bool {
        self.scripts.iter().any(|s| receivers.contains(&s.as_str()))
    }
}

/// A Rigidbody2D on a GameObject without a PhysicalBehaviour (FoliageBehaviour leaves, grenade pins, the Tank's
/// hinge): its Transform is `transform_nodes[node]`, whose local pose the body writes.
#[derive(Clone, Debug)]
pub struct NodeBody {
    pub node: usize,
    pub rigidbody: Rigidbody,
    /// CheckForDrivenByParentRigidbody: the nearest ancestor Rigidbody2D's body.
    pub driven_by: Option<usize>,
}

/// A SpriteRenderer on a GameObject without a PhysicalBehaviour: where its Transform hangs, and what it draws.
#[derive(Clone, Debug)]
pub struct Renderer {
    pub anchor: BareAnchor,
    pub chain: Vec<crate::unity::transform::Trs>,
    pub sprite: (crate::assets::db::ObjRef, [f32; 4], i16),
    /// The SpriteRenderer component's instance id.
    pub id: i32,
    /// flipX/flipY, drawMode, size.
    pub draw: ([bool; 2], i32, [f32; 2]),
    /// GameObject.activeInHierarchy and Renderer.enabled at instantiation.
    pub visible: bool,
}

/// The object owning a Rigidbody2D: a physical, or a `node_rigidbodies` entry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BodyOwner {
    Physical(usize),
    Node(usize),
}

/// A Joint2D in PhysicsScene2D.m_JointBreakLimits: a hinge, a limb's gore-string SpringJoint2D, a FixedJoint2D, or
/// a Fixed/Slider/SpringJoint2D cloned from a prefab (`Anchored`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BreakJoint {
    Hinge(usize),
    Spring(usize),
    Fixed(usize),
    Anchored(usize),
}

/// What PhysicsScene2D's break-limit walk reads from a prefab-cloned Fixed/Slider/SpringJoint2D.
#[derive(Clone, Copy, Debug)]
pub struct AnchoredJoint {
    pub break_force: f32,
    pub break_torque: f32,
    /// m_Joint (+0x58)
    pub joint: Option<crate::b2::world::JointId>,
    /// The `Game::hinges` entry that took a spawned hinge's slot over: its break limit is `BreakJoint::Hinge` from then on.
    pub hinge: Option<usize>,
    /// The joint component's instance id.
    pub component: i32,
}

/// A Transform as the scripts read it: world position and rotation, local scale, and the hierarchy's stored
/// local position and rotation (None: derive them from the world pose).
#[derive(Clone, Copy, Debug, Default)]
pub struct Pose {
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub localScale: [f32; 3],
    pub local: Option<([f32; 3], [f32; 4])>,
}

impl Pose {
    pub fn up(&self) -> [f32; 3] {
        mathf::rotate(self.rotation, [0.0, 1.0, 0.0])
    }
    pub fn right(&self) -> [f32; 3] {
        mathf::rotate(self.rotation, [1.0, 0.0, 0.0])
    }
}

/// A spawnable MonoBehaviour with Unity Update, FixedUpdate or LateUpdate messages, by its arena index.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Behaviour {
    Person(usize),
    /// CirculationBehaviour (its BloodContainer.Update) by limb.
    Circulation(usize),
    Skin(usize),
    Grip(usize),
    GoreString(usize),
    GForce(usize),
    Splat(usize),
    Bleeding(usize),
    Machinery(usize),
    Lava(usize),
    Destroyable(usize),
    Spawner(usize),
    HeatRect(usize),
    Explosive(usize),
    DeleteAfter(usize),
    FireMap(usize),
    Firearm(usize),
    Launcher(usize),
    Bell(usize),
    MachineGun(usize),
    PointToVelocity(usize),
    Car(usize),
    Tyre(usize),
    Flask(usize),
    LiquidContainer(usize),
    TruckDoor(usize),
    Glowtube(usize),
    Led(usize),
    Detector(usize),
    Sticky(usize),
    Generator(usize),
    Floodlight(usize),
    SpikeGrenade(usize),
    HeatingElement(usize),
    RayGun(usize),
    Bulb(usize),
    PointLiquid(usize),
    Thrusterbed(usize),
    Piston(usize),
    Flamethrower(usize),
    Translated(translated::TBehaviour),
}

impl Behaviour {
    fn class(self) -> &'static str {
        match self {
            Behaviour::Person(_) => "PersonBehaviour",
            Behaviour::Circulation(_) => "CirculationBehaviour",
            Behaviour::Skin(_) => "SkinMaterialHandler",
            Behaviour::Grip(_) => "GripBehaviour",
            Behaviour::GoreString(_) => "GoreStringBehaviour",
            Behaviour::GForce(_) => "GForceMeasureBehaviour",
            Behaviour::Splat(_) => "FancyBloodSplatController",
            Behaviour::Bleeding(_) => "BleedingParticleBehaviour",
            Behaviour::Machinery(_) => "DamagableMachineryBehaviour",
            Behaviour::Lava(_) => "LavaBehaviour",
            Behaviour::Destroyable(_) => "DestroyableBehaviour",
            Behaviour::Spawner(_) => "RandomPrefabSpawnerBehaviour",
            Behaviour::HeatRect(_) => "TemperatureTargetRectangleBehaviour",
            Behaviour::Explosive(_) => "ExplosiveBehaviour",
            Behaviour::DeleteAfter(_) => "DeleteAfterTime",
            Behaviour::PointToVelocity(_) => "PointToVelocityBehaviour",
            Behaviour::Car(_) => "CarBehaviour",
            Behaviour::Tyre(_) => "TyreDeflationBehaviour",
            Behaviour::Flask(_) => "GasTankBehaviour",
            Behaviour::LiquidContainer(_) => "LiquidContainerController",
            Behaviour::TruckDoor(_) => "TruckDoorBehaviour",
            Behaviour::Glowtube(_) => "GlowtubeBehaviour",
            Behaviour::Led(_) => "LEDBulbBehaviour",
            Behaviour::Detector(_) => "DetectorBehaviour",
            Behaviour::Generator(_) => "GeneratorBehaviour",
            Behaviour::Floodlight(_) => "SingleFloodlightBehaviour",
            Behaviour::SpikeGrenade(_) => "SpikeGrenadeBehaviour",
            Behaviour::HeatingElement(_) => "HeatingElementBehaviour",
            Behaviour::RayGun(_) => "TemperatureRayGunBehaviour",
            Behaviour::Bulb(_) => "BulbBehaviour",
            Behaviour::PointLiquid(_) => "PointLiquidTransferBehaviour",
            Behaviour::Thrusterbed(_) => "ThrusterbedBehaviour",
            Behaviour::Piston(_) => "PistonBehaviour",
            Behaviour::Flamethrower(_) => "FlamethrowerBehaviour",
            Behaviour::Sticky(_) => "StickyGrenadeBehaviour",
            Behaviour::FireMap(_) => "LocalFirePropagationMap",
            Behaviour::Firearm(_) => "FirearmBehaviour",
            Behaviour::Launcher(_) => "ProjectileLauncherBehaviour",
            Behaviour::Bell(_) => "BellBehaviour",
            Behaviour::MachineGun(_) => "MachineGunBehaviour",
            Behaviour::Translated(t) => t.class(),
        }
    }
}

/// Whether a MonoBehaviour class declares or inherits the Unity message (lifecycle.rs).
pub fn declares(class: &str, message: &str) -> bool {
    lifecycle::LIFECYCLE.iter().find(|(k, _)| *k == class).is_some_and(|(_, m)| m.contains(&message))
}

/// The passes of Unity's FixedBehaviourManager, BehaviourManager and LateBehaviourManager.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    Fixed,
    Update,
    Late,
}

/// The spawnable behaviours in Unity's three behaviour managers: by execution order, then AddBehaviour order
/// (BaseBehaviourManager::AddBehaviour 180632040 appends to the order's pending list, IntegrateLists 180634580
/// appends that to the list at the next pass). A behaviour is added once Awake ran on an active, enabled one.
#[derive(Default)]
pub struct Behaviours {
    pub fixed: Vec<(i32, Behaviour)>,
    pub update: Vec<(i32, Behaviour)>,
    pub late: Vec<(i32, Behaviour)>,
}

impl Behaviours {
    pub fn list(&self, phase: Phase) -> &Vec<(i32, Behaviour)> {
        match phase {
            Phase::Fixed => &self.fixed,
            Phase::Update => &self.update,
            Phase::Late => &self.late,
        }
    }

    /// MonoBehaviour::AddToManager: into each manager whose message the class declares.
    pub fn add(&mut self, order: i32, b: Behaviour) {
        for (list, m) in [(&mut self.fixed, "FixedUpdate"), (&mut self.update, "Update"), (&mut self.late, "LateUpdate")] {
            if declares(b.class(), m) {
                let at = list.partition_point(|&(o, _)| o <= order);
                list.insert(at, (order, b));
            }
        }
    }

    /// MonoBehaviour::RemoveFromManager
    pub fn remove(&mut self, b: Behaviour) {
        for list in [&mut self.fixed, &mut self.update, &mut self.late] {
            list.retain(|&(_, k)| k != b);
        }
    }
}

/// Everything the rewritten scripts touch: the b2 world, engine state and the component arenas.
/// Limb-owned components (circulation, node, skin, gforce) share the limb's index.
#[derive(Default)]
pub struct Game {
    pub time: Time,
    pub random: Random,
    pub prefs: Prefs,
    /// PhysicalBehaviour.AmbientTemperature (the map's EnvironmentalSettings.Ambient_temperature).
    pub AmbientTemperature: f32,
    pub world: World,
    pub properties: Vec<PhysicalProperties>,
    /// Global.PhysicalObjectsInWorld order.
    pub physicals: Vec<Physical>,
    /// AmbientTemperatureGridBehaviour.Instance (the Main scene's grid; nothing in the dumps records it).
    pub ambient_grid: temperature::AmbientTemperatureGridBehaviour,
    pub limbs: Vec<Limb>,
    pub circulations: Vec<Circulation>,
    pub nodes: Vec<Node>,
    pub skins: Vec<Skin>,
    pub gforces: Vec<GForce>,
    pub grips: Vec<Grip>,
    pub people: Vec<Person>,
    pub bleeding: Vec<BleedingParticle>,
    pub splats: Vec<FancyBloodSplat>,
    /// Behaviours instantiated since the last delayed-Start pass, in Awake order.
    pub pending_starts: Vec<misc::PendingStart>,
    /// The joints below each pending HingeJointLimitAutofixBehaviour host (`PendingStart::HingeJointLimitAutofixItem`'s
    /// index), kept current by `refresh_autofix_joints` until its Start.
    pub autofix_joints: Vec<misc::AutofixJoints>,
    pub joint_break_sounds: Vec<misc::JointBreakSound>,
    pub behaviours: Behaviours,
    pub machinery: Vec<machinery::Machinery>,
    pub lavas: Vec<scenery::Lava>,
    pub spawners: Vec<scenery::Spawner>,
    pub heat_rects: Vec<scenery::HeatRect>,
    pub random_sprites: Vec<misc::RandomSprite>,
    pub destroyables: Vec<destroyable::Destroyable>,
    pub fire_maps: Vec<fire::FireMap>,
    pub firearms: Vec<firearm::Firearm>,
    pub launchers: Vec<launcher::Launcher>,
    pub bells: Vec<bell::Bell>,
    pub machine_guns: Vec<machinegun::MachineGun>,
    /// The translated classes' components (tools/translator).
    pub tr: translated::Translated,
    pub explosives: Vec<explosion::Explosive>,
    pub act_on_collides: Vec<acts::ActOnCollide>,
    pub no_collides: Vec<acts::NoCollide>,
    pub disintegration_counters: Vec<acts::DisintegrationCounter>,
    pub sticky_grenades: Vec<acts::StickyGrenade>,
    pub point_to_velocities: Vec<acts::PointToVelocity>,
    pub cars: Vec<car::CarBehaviour>,
    pub tyres: Vec<tyre::TyreDeflationBehaviour>,
    pub flasks: Vec<flask::Flask>,
    pub point_liquids: Vec<flask::PointLiquidTransfer>,
    pub liquid_containers: Vec<flask::LiquidContainer>,
    pub glowtubes: Vec<light::Glowtube>,
    pub leds: Vec<light::LedBulb>,
    pub detectors: Vec<detector::Detector>,
    pub generators: Vec<devices::Generator>,
    pub floodlights: Vec<devices::Floodlight>,
    pub spike_grenades: Vec<devices::SpikeGrenade>,
    pub heating_elements: Vec<devices::HeatingElement>,
    pub ray_guns: Vec<devices::RayGun>,
    pub bulbs: Vec<devices::Bulb>,
    pub thrusterbeds: Vec<thruster::Thrusterbed>,
    pub pistons: Vec<piston::Piston>,
    pub flamethrowers: Vec<flamethrower::Flamethrower>,
    /// `anchored_joints` slot -> `fixed_joints` entry, for the prefab joints adopted (a script that reads the component,
    /// or HingeJointLimitAutofixBehaviour.Start of a mirrored host): the first adoption owns the component's state.
    pub adopted_slots: HashMap<usize, usize>,
    pub continuous: continuous::ContinuousActivation,
    /// Each TruckDoorBehaviour with its component's instance id.
    pub truck_doors: Vec<(i32, car::TruckDoor)>,
    pub joint_helpers: Vec<car::JointHelper>,
    pub no_collide_gms: Vec<car::NoCollideGm>,
    pub random_car_textures: Vec<car::RandomCarTexture>,
    pub delete_afters: Vec<explosion::DeleteAfter>,
    /// Coroutines in WaitForEndOfFrame, in the order they got there.
    pub end_of_frame: Vec<destroyable::Coroutine>,
    /// GlobalShrapnelEmitter.Instance
    pub shrapnel: Option<ballistics::ShrapnelEmitter>,
    /// Coroutines in WaitForFixedUpdate, in the order they got there.
    pub fixed_update_waits: Vec<destroyable::Coroutine>,
    /// BallisticsEmitter.BallisticIteration coroutines in a wait (`Step::Bullet`); None once resumed or stopped.
    pub bullets: Vec<Option<(ballistics::Bullet, Option<ballistics::BulletHit>)>>,
    pub tracer_pool: ballistics::TracerPool,
    pub camera: ballistics::CameraState,
    /// Global.BulletCrackClips length.
    pub bullet_crack_clips: usize,
    pub coroutine_seq: u64,
    /// The components of destroyed GameObjects (they read null).
    pub destroyed_components: std::collections::HashSet<i32>,
    pub act_on_destroy: HashMap<i32, destroyable::ActOnDestroy>,
    /// UseEventTrigger components by instance id.
    pub use_events: HashMap<i32, link::UseEvent>,
    /// What `spawn` linked of each clone with an object that was inactive when it spawned (by the id of the root
    /// Transform): the first GameObject.SetActive(true) of such an object runs its scripts' Awake with it.
    pub first_activation: HashMap<i32, link::Linked>,
    /// The GameObjects (by the id of their Transform) that GetOrAddComponent<DebrisComponent> added one to.
    pub debris_added: std::collections::HashSet<i32>,
    pub dwacds: Vec<destroyable::DestroyWhenAllChildrenDestroyed>,
    /// level1's DistantSoundBehaviour clip counts, by SoundType - 1 (SmallExplosion .. LargeFirearm).
    pub distant_sounds: [usize; 6],
    /// Coroutines waiting in WaitForSeconds, resumed by the dynamic delayed-call pass.
    pub coroutines: Vec<destroyable::Coroutine>,
    pub do_at_time_onces: Vec<destroyable::DoAtTimeOnce>,
    /// InstantiateObjectBehaviour by component id: Prefab and fields.
    pub instantiate_objects: HashMap<i32, (Option<crate::assets::db::ObjRef>, destroyable::InstantiateObject)>,
    /// Object.Destroy calls queued for the delayed-call pass: (time, a component id on the object).
    pub object_destroys: Vec<(f64, i32)>,
    /// Object.Destroy(FixedJoint2D) calls queued for the delayed-call pass: (time, `fixed_joints` index).
    pub joint_destroys: Vec<(f64, usize)>,
    /// The manager entry of each registered script component (by instance id), for SetActive.
    pub behaviour_of: HashMap<i32, (i32, Behaviour)>,
    /// Every BloodPuff the pool created and its ObjectPoolBehaviour (None until PoolGenerator.Ensure).
    pub blood_puffs: Vec<misc::BloodPuff>,
    pub blood_puff_pool: Option<misc::ObjectPool>,
    pub hinges: Vec<Hinge>,
    /// The map's scripts that draw UnityEngine.Random (scene order).
    pub ambient_audio: Vec<PlayAudioAtRandom>,
    pub map_lights: Vec<MapLight>,
    /// WaterBehaviour.waters (in Awake order, which is the arena order) with their state.
    pub waters: Vec<water::Water>,
    /// WaterBehaviour.waters: indices into `waters`, in Awake order.
    pub water_list: Vec<usize>,
    /// FreezeStackController.stack: the physical objects' Rigidbody2Ds with their request counts.
    pub freeze_stack: std::collections::HashMap<usize, i32>,
    pub frozen_in_ice: Vec<water::FrozenInIce>,
    /// The pools of the Splash prefabs the waters name.
    pub splash_pools: Vec<water::SplashPool>,
    /// Global.main.DynamicSqrdVelocityThreshold
    pub DynamicSqrdVelocityThreshold: f32,
    /// Global.main.CameraControlBehaviour.BoundingBox
    pub BoundingBox: Bounds,
    /// Global.physicalBehaviourChunks (each chunk holds at most 128) and chunksUpToDate.
    pub physical_chunks: std::collections::HashMap<(i32, i32), Vec<usize>>,
    pub chunksUpToDate: bool,
    /// PhysicalBehaviour.contactBuffer / LimbBehaviour.contactBuffer: static ContactPoint2D[8], stale between calls.
    pub physical_contact_buffer: [crate::b2::contacts2d::ContactPoint2D; 8],
    pub limb_contact_buffer: [crate::b2::contacts2d::ContactPoint2D; 8],
    /// Physics2D.velocityIterations / positionIterations (UserPreferences.PhysicsIterations).
    pub velocityIterations: i32,
    pub positionIterations: i32,
    /// PhysicsScene2D.m_JointBreakLimits: hinges with a finite break force or torque, in list order.
    pub joint_break_limits: Vec<BreakJoint>,
    /// The Fixed/Slider/SpringJoint2D components cloned from prefabs, by creation order (BreakJoint::Anchored).
    pub anchored_joints: Vec<AnchoredJoint>,
    /// FixedJoint2D components (the shattered pieces' welds), by creation order.
    pub fixed_joints: Vec<simulate::FixedJoint>,
    /// PhysicalBehaviour.Penetration objects, by creation order (Physical.penetrations/victimPenetrations index it).
    pub penetration_arena: Vec<stab::Penetration>,
    /// IgnoreCollisionStackController.stack: the request count of each collider pair (lower instance id first).
    pub ignore_stack: std::collections::HashMap<(i32, i32), i32>,
    /// LimbBehaviour.VitalParts by limb: Bounds as center then extents. The dump loader fills it.
    pub vital_parts: std::collections::HashMap<usize, Vec<[f32; 6]>>,
    /// The Transforms that parent physical objects: Rigidbody-less ones and static bodies' (fixed TRS), and
    /// non-static bodies' (`node_bodies`).
    pub transform_nodes: Vec<crate::unity::transform::Node>,
    /// The `transform_nodes` that are a physical's own Transform, by node: their TRS is the physical's pose.
    pub node_bodies: std::collections::HashMap<usize, usize>,
    /// Colliders on GameObjects without a PhysicalBehaviour, by Collider2D id: their Transforms for the queries.
    pub bare_colliders: std::collections::HashMap<i32, BareCollider>,
    /// Rigidbody2Ds on GameObjects without a PhysicalBehaviour.
    pub node_rigidbodies: Vec<NodeBody>,
    /// SpriteRenderers on GameObjects without a PhysicalBehaviour (drawing only).
    pub renderers: Vec<Renderer>,
    /// The DecalDescriptor assets in use (DecalController.descriptor indexes it).
    pub decal_descriptors: Vec<misc::DecalDescriptor>,
    /// Instances whose Transform changes (an Awake's or the catalog's scale) wait for Physics2D's next
    /// SyncTransforms (`sync_pending`); after it nothing moves their Transforms but the bodies.
    pub unsynced: Vec<spawn::Instance>,
    /// Synced instances that hold a WaterBehaviour (its SetActive calls change their objects).
    pub retained: Vec<spawn::Instance>,
    /// The game's data, for what scripts instantiate at run time (the catalog); None when loaded from a recording.
    pub assets: Option<Box<crate::assets::db::AssetDb>>,
    /// CatalogBehaviour.SelectedItem: the SpawnableAsset's name (the catalog UI is not rewritten).
    pub catalog_selected: Option<String>,
    /// The `transform_nodes` roots carrying an AudioSourceTimeScaleBehaviour (the catalog adds one).
    pub audio_time_scale_roots: Vec<usize>,
    /// PhysicsScene2D m_LastSimulationTime / m_LastSimulationDelta.
    pub last_simulation: (f64, f32),
    /// PhysicsScene2D m_RigidbodyHierarchyChanged: the movement-state list is rebuilt after the next step.
    pub rigidbody_hierarchy_changed: bool,
    /// The ToolController (recordings before the drag tool lack it).
    pub tool: Option<tool::Tool>,
    /// The scene's "Resizing handles" clone and the object its ResizeHandles sits on, while it is active.
    pub resize_handles: Option<(spawn::Instance, usize)>,
    pub input: tool::Input,
    /// Global.main.MousePosition
    pub MousePosition: [f32; 3],
    /// The restore's GameObject ids by name.
    pub objects: HashMap<String, i32>,
    /// The id Object.Instantiate gives its next clone (ids go down by 2).
    pub next_instance_id: i32,
    /// A clone of Global.MetalScrapePrefab: the instance ids it takes (objects and components) and the
    /// Random.value draws its ActivateIfFancyEffects Awakes make; None when the scene was not loaded from data.
    pub metal_scrape: Option<(usize, usize)>,
    /// Physics2DSettings.m_DefaultMaterial's (friction, bounciness); None without one.
    pub default_material2d: Option<(f32, f32)>,
}

impl Default for Random {
    fn default() -> Self {
        Random::new(0)
    }
}

#[cfg(test)]
mod prefs_tests {
    use super::*;

    #[test]
    fn prefs_from_json() {
        let p = Prefs::from_json(r#"{"CollisionQuality": "Continuous", "ToolSelectionMode": 1, "PhysicsIterations": 16, "ShowTutorial": false, "GridSnap": 0.5, "Resolution": {"x": 1}}"#).unwrap();
        assert_eq!((p.CollisionQuality, p.ToolSelectionMode, p.PhysicsIterations, p.ShowTutorial, p.GridSnap), (1, tool::SELECTION_MASS, 16, false, 0.5));
        assert_eq!(p.FragilityMultiplier, 1.0);
        assert!(Prefs::from_json(r#"{"LimbCrushing": 1}"#).is_err());
    }
}
