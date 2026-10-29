//! The game's own scripts (Assembly-CSharp) rewritten line by line, on top of the b2 world.
//! Field names keep the original C# names so every value can be checked against the running game
//! (tools/recorder PPG_DUMP); methods keep the original statement order and float operation order.
#![allow(non_snake_case, dead_code)]

pub mod circulation;
pub mod dump;
pub mod limb;
pub mod misc;
pub mod person;
pub mod physical;

use crate::b2::{
    math::Vec2 as V,
    world::{BodyId, BodyType, JointId, World, BODY_AWAKE, BODY_BULLET, BODY_FIXED_ROTATION},
};
use crate::unity::Random;

pub use circulation::Circulation;
pub use limb::Limb;
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
    pub fn sqrt(f: f32) -> f32 {
        (f as f64).sqrt() as f32
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
        lower2 + (value - lower1) * (upper2 - lower2) / (upper1 - lower1)
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
    pub fn sqr_magnitude(v: [f32; 2]) -> f32 {
        v[0] * v[0] + v[1] * v[1]
    }
    /// Quaternion * Vector3 (UnityEngine.Quaternion operator*).
    pub fn rotate(q: [f32; 4], p: [f32; 3]) -> [f32; 3] {
        let [x, y, z, w] = q;
        let (n1, n2, n3) = (x * 2.0, y * 2.0, z * 2.0);
        let (n4, n5, n6) = (x * n1, y * n2, z * n3);
        let (n7, n8, n9) = (x * n2, x * n3, y * n3);
        let (n10, n11, n12) = (w * n1, w * n2, w * n3);
        [
            (1.0 - (n5 + n6)) * p[0] + (n7 - n12) * p[1] + (n8 + n11) * p[2],
            (n7 + n12) * p[0] + (1.0 - (n4 + n6)) * p[1] + (n9 - n10) * p[2],
            (n8 - n11) * p[0] + (n9 + n10) * p[1] + (1.0 - (n4 + n5)) * p[2],
        ]
    }
}

/// One value the recorder dumps: floats as hex bits, vectors as comma lists, booleans as 0/1.
pub trait Field {
    fn parse(&mut self, text: &str) -> Result<(), String>;
    fn show(&self) -> String;
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
}
impl Field for Option<f32> {
    fn parse(&mut self, text: &str) -> Result<(), String> {
        *self = if text == "null" { None } else { Some(bits(text)?) };
        Ok(())
    }
    fn show(&self) -> String {
        self.map_or("null".into(), |v| v.show())
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
    pub fixedTime: f32,
    pub deltaTime: f32,
    pub fixedDeltaTime: f32,
    pub maximumDeltaTime: f32,
    pub frameCount: i32,
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
        }
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
}

impl Rigidbody {
    pub fn velocity(&self, w: &World) -> [f32; 2] {
        let b = w.body(self.body);
        if self.bodyType == 2 { [0.0, 0.0] } else { [b.linear_velocity.x, b.linear_velocity.y] }
    }
    /// Rigidbody2D::SetVelocity
    pub fn set_velocity(&self, w: &mut World, v: [f32; 2]) {
        if self.bodyType == 2 {
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
        if self.bodyType == 2 { 0.0 } else { w.body(self.body).angular_velocity * mathf::RAD2DEG }
    }
    /// Rigidbody2D::SetAngularVelocity
    pub fn set_angular_velocity(&self, w: &mut World, degrees: f32) {
        if self.bodyType == 2 {
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
        if self.bodyType != 0 {
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
        w.body(self.body).rigidbody.mass
    }
    /// Rigidbody2D::SetCollisionDetectionMode (CollisionDetectionMode2D: 0 discrete, 1 continuous)
    pub fn set_collision_detection_mode(&mut self, w: &mut World, mode: i32) {
        self.collisionDetectionMode = mode;
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
    /// The limb whose body is the connected body.
    pub connected: usize,
    pub anchor: [f32; 2],
    pub connectedAnchor: [f32; 2],
    pub enabled: bool,
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
        (w.body(j.body_b).sweep.a - w.body(j.body_a).sweep.a - j.revolute.def.reference_angle) * mathf::RAD2DEG
    }
    /// HingeJoint2D::GetReferenceAngle, degrees.
    pub fn reference_angle(&self, w: &World) -> f32 {
        self.joint.map_or(0.0, |id| w.joints[id].as_ref().unwrap().revolute.def.reference_angle * mathf::RAD2DEG)
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
        let def = &j.revolute.def;
        let (enable, old_speed, old_torque) = (def.enable_motor, def.motor_speed, def.max_motor_torque);
        let speed = s * mathf::DEG2RAD;
        if !enable {
            wake_for_joint(w, a);
            wake_for_joint(w, b);
            w.joints[id].as_mut().unwrap().revolute.def.enable_motor = true;
        }
        if speed != old_speed {
            wake_for_joint(w, a);
            wake_for_joint(w, b);
            w.joints[id].as_mut().unwrap().revolute.def.motor_speed = speed;
        }
        if t != old_torque {
            wake_for_joint(w, a);
            wake_for_joint(w, b);
            w.joints[id].as_mut().unwrap().revolute.def.max_motor_torque = t;
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
        if !j.revolute.def.enable_limit {
            wake_for_joint(w, a);
            wake_for_joint(w, b);
            let r = &mut w.joints[id].as_mut().unwrap().revolute;
            r.def.enable_limit = true;
            r.impulse[2] = 0.0;
        }
        let (mut lower, mut upper) = (self.limits[0] * mathf::DEG2RAD, self.limits[1] * mathf::DEG2RAD);
        if upper < lower {
            std::mem::swap(&mut lower, &mut upper);
        }
        let def = &w.joints[id].as_ref().unwrap().revolute.def;
        if lower != def.lower_angle || upper != def.upper_angle {
            wake_for_joint(w, a);
            wake_for_joint(w, b);
            let r = &mut w.joints[id].as_mut().unwrap().revolute;
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

/// A Transform as the scripts read it: world position, rotation and lossy scale.
#[derive(Clone, Copy, Debug, Default)]
pub struct Pose {
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub localScale: [f32; 3],
}

impl Pose {
    pub fn up(&self) -> [f32; 3] {
        mathf::rotate(self.rotation, [0.0, 1.0, 0.0])
    }
    pub fn right(&self) -> [f32; 3] {
        mathf::rotate(self.rotation, [1.0, 0.0, 0.0])
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
    pub limbs: Vec<Limb>,
    pub circulations: Vec<Circulation>,
    pub nodes: Vec<Node>,
    pub skins: Vec<Skin>,
    pub gforces: Vec<GForce>,
    pub grips: Vec<Grip>,
    pub people: Vec<Person>,
    pub bleeding: Vec<BleedingParticle>,
    pub splats: Vec<FancyBloodSplat>,
    pub hinges: Vec<Hinge>,
    /// Global.main.DynamicSqrdVelocityThreshold
    pub DynamicSqrdVelocityThreshold: f32,
}

impl Default for Random {
    fn default() -> Self {
        Random::new(0)
    }
}
