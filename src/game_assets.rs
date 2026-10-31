use bevy::prelude::*;
use serde::Deserialize;
use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
    time::UNIX_EPOCH,
};

const REQUIRED: [&str; 4] = [
    "People Playground.exe",
    "People Playground_Data/Managed/Assembly-CSharp.dll",
    "People Playground_Data/resources.assets",
    "People Playground_Data/sharedassets0.assets",
];

#[derive(Deserialize)]
pub struct SourceFile {
    pub path: String,
    pub bytes: u64,
    pub modified_ns: u64,
}

#[derive(Clone, Deserialize)]
pub struct BoxShape {
    pub offset: [f32; 2],
    pub size: [f32; 2],
}

#[derive(Deserialize)]
pub struct Part {
    pub name: String,
    pub position: [f32; 2],
    pub angle: f32,
    pub mass: f32,
    pub linear_drag: f32,
    pub angular_drag: f32,
    pub colliders: Vec<BoxShape>,
    pub texture: String,
    pub rect: [f32; 4],
    pub size: [f32; 2],
    pub pivot: [f32; 2],
    pub layer: i32,
    #[serde(default)]
    pub muscle: Option<MuscleSpec>,
    #[serde(default)]
    pub physics_material: Option<PhysicsMaterial>,
}

#[derive(Deserialize)]
pub struct PhysicsMaterial {
    pub friction: f32,
    pub restitution: f32,
}

#[derive(Deserialize)]
pub struct MuscleSpec {
    /// LimbBehaviour.BaseStrength; StrengthMultipliers apply after MotorStrength.
    pub strength: f32,
    #[serde(default)]
    pub multipliers: Vec<f32>,
    pub upright_force: f32,
    pub has_brain: bool,
    #[serde(default)]
    pub do_balance_jerk: bool,
    #[serde(default)]
    pub balance_muscle_movement: f32,
    #[serde(default)]
    pub do_stumble: bool,
    #[serde(default = "one")]
    pub blood_strength_ratio: f32,
}

fn one() -> f32 {
    1.0
}

#[derive(Deserialize)]
pub struct PoseAngle {
    pub part: String,
    pub angle: f32,
    #[serde(default)]
    pub angle_degrees: Option<f32>,
    pub rigidity_modifier: f32,
    #[serde(default)]
    pub animation: Option<PoseAnimation>,
}

/// RagdollPose.LimbPose animation fields (Animated limbs only).
#[derive(Deserialize)]
pub struct PoseAnimation {
    pub start: f32,
    pub end: f32,
    pub duration: f32,
    pub random_influence: f32,
    pub random_speed: f32,
    pub time_offset: f32,
    pub curve: Curve,
}

/// A Unity AnimationCurve with unweighted keys [time, value, inSlope, outSlope].
#[derive(Clone, Deserialize)]
pub struct Curve {
    pub keys: Vec<[f32; 4]>,
    /// Unity's internal wrap modes: 0 ping-pong, 1 repeat, 2 clamp.
    pub pre: i32,
    pub post: i32,
}

/// InternalWrapMode
pub const PING_PONG: i32 = 0;
pub const REPEAT: i32 = 1;
pub const CLAMP: i32 = 2;

/// AnimationCurveTpl<float>::Cache: the segment polynomial the last Evaluate built, reused while time stays inside it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CurveCache {
    pub index: i32,
    pub time: f32,
    pub time_end: f32,
    pub coeff: [f32; 4],
}

impl Default for CurveCache {
    fn default() -> Self {
        Self { index: 0, time: f32::INFINITY, time_end: 0.0, coeff: [0.0; 4] }
    }
}

impl CurveCache {
    fn polynomial(&self, t: f32) -> f32 {
        let x = t - self.time;
        ((x * self.coeff[0] + self.coeff[1]) * x + self.coeff[2]) * x + self.coeff[3]
    }
}

/// The engine's inline repeat wrap.
pub fn repeat(t: f32, first: f32, last: f32) -> f32 {
    let length = last - first;
    ((t - first) - ((t - first) / length).floor() * length) + first
}

/// PingPong (UnityPlayer 0x1800d1240)
pub fn ping_pong(t: f32, first: f32, last: f32) -> f32 {
    let length = last - first;
    let double = length + length;
    let r = (t - first) - ((t - first) / double).floor() * double;
    (length - (r - length).abs()) + first
}

impl Curve {
    /// AnimationCurveTpl<float>::IsValid
    fn is_valid(&self) -> bool {
        !self.keys.is_empty() && self.keys[0][0].is_finite() && self.keys[self.keys.len() - 1][0].is_finite()
    }

    /// AnimationCurveTpl<float>::FindIndexForSampling: up to three steps from the cached index, then a binary search.
    fn find_index(&self, cache: &CurveCache, t: f32) -> (usize, usize) {
        let n = self.keys.len() as i32;
        let time = |i: i32| self.keys[i as usize][0];
        // A wrapped time that rounds below the first key gives -1, and the engine then reads the bytes in front of
        // the key array; the rewrite reads key 0 there instead. Unreachable with the game's pose curves: all
        // ping-pong over [0, 1], whose wrap only divides and multiplies by 2 and so is exact.
        let found = |i: i32| (i.max(0) as usize, (i + 1).min(n - 1) as usize);
        // Branches as compiled: a NaN time steps backward from the hint, then takes the right half of every split.
        let mut i = cache.index;
        if i != -1 {
            if !(t > time(i)) {
                for _ in 0..3 {
                    if i >= 0 && t >= time(i) {
                        return found(i);
                    }
                    i -= 1;
                }
            } else {
                for _ in 0..3 {
                    if i + 1 < n && t < time(i + 1) {
                        return found(i);
                    }
                    i += 1;
                }
            }
        }
        let (mut first, mut count) = (0, n);
        while count > 0 {
            let step = count >> 1;
            if !(time(first + step) > t) {
                first += step + 1;
                count -= step + 1;
            } else {
                count = step;
            }
        }
        found(first - 1)
    }

    /// AnimationCurveTpl<float>::CalculateCacheData
    fn cache_segment(&self, cache: &mut CurveCache, lhs: usize, rhs: usize, offset: f32) {
        let (l, r) = (self.keys[lhs], self.keys[rhs]);
        cache.index = lhs as i32;
        cache.time = offset + l[0];
        cache.time_end = offset + r[0];
        let dv = r[1] - l[1];
        let d = r[0] - l[0];
        let dx = if 0.0001 <= d { d } else { 0.0001 };
        let m0 = l[3] * dx;
        let m1 = dx * r[2];
        let inverse = 1.0 / (dx * dx);
        cache.coeff = [((((m1 + m0) - dv) - dv) * inverse) / dx, ((((dv + dv) + dv) - m0) - m0 - m1) * inverse, l[3], l[1]];
        if l[3] == f32::INFINITY || r[2] == f32::INFINITY {
            cache.coeff = [0.0, 0.0, 0.0, l[1]];
        }
    }

    /// AnimationCurveTpl<float>::EvaluateWithoutCacheCoefficients (ping-pong and unknown wrap modes).
    fn evaluate_uncached(&self, cache: &CurveCache, t: f32) -> f32 {
        let n = self.keys.len();
        if n == 1 {
            return self.keys[0][1];
        }
        let (first, last) = (self.keys[0][0], self.keys[n - 1][0]);
        let wrap = |mode: i32, clamped: f32| match mode {
            CLAMP => clamped,
            PING_PONG => ping_pong(t, first, last),
            _ => repeat(t, first, last),
        };
        let x = if first <= t {
            if last < t { wrap(self.post, last) } else { t }
        } else {
            wrap(self.pre, first)
        };
        let (lhs, rhs) = self.find_index(cache, x);
        let (l, r) = (self.keys[lhs], self.keys[rhs]);
        let dx = r[0] - l[0];
        let (p, m0, m1) = if dx == 0.0 { (0.0, 0.0, 0.0) } else { ((x - l[0]) / dx, dx * l[3], dx * r[2]) };
        let p2 = p * p;
        let p3 = p2 * p;
        let value = ((p3 - (p2 + p2)) + p) * m0 + (((p3 + p3) - p2 * 3.0) + 1.0) * l[1] + (p3 - p2) * m1 + (p2 * 3.0 - (p3 + p3)) * r[1];
        if l[3] == f32::INFINITY || r[2] == f32::INFINITY { l[1] } else { value }
    }

    /// AnimationCurveTpl<float>::Evaluate with the curve's own cache (AnimationCurve.Evaluate).
    pub fn evaluate(&self, cache: &mut CurveCache, t: f32) -> f32 {
        let n = self.keys.len();
        if n == 1 {
            return self.keys[0][1];
        }
        if cache.time <= t && t < cache.time_end {
            return cache.polynomial(t);
        }
        if !self.is_valid() {
            return 0.0;
        }
        let (first, last) = (self.keys[0][0], self.keys[n - 1][0]);
        let constant = |cache: &mut CurveCache, time: f32, time_end: f32, value: f32| {
            cache.time = time;
            cache.time_end = time_end;
            cache.coeff = [0.0, 0.0, 0.0, value];
            cache.polynomial(t)
        };
        // comiss branches: an unordered time takes the in-range path.
        let (sample, offset) = if t < last || t.is_nan() {
            if first <= t || t.is_nan() {
                (t, 0.0)
            } else {
                match self.pre {
                    CLAMP => return constant(cache, t - 1000.0, first, self.keys[0][1]),
                    REPEAT => {
                        let w = repeat(t, first, last);
                        (w, t - w)
                    }
                    _ => return self.evaluate_uncached(cache, t),
                }
            }
        } else {
            match self.post {
                CLAMP => return constant(cache, last, f32::INFINITY, self.keys[n - 1][1]),
                REPEAT => {
                    // maxss/minss: a NaN takes the bound.
                    let w = repeat(t, first, last);
                    let w = if w > first { w } else { first };
                    let w = if w < last { w } else { last };
                    (w, t - w)
                }
                _ => return self.evaluate_uncached(cache, t),
            }
        };
        let (lhs, rhs) = self.find_index(cache, sample);
        self.cache_segment(cache, lhs, rhs, offset);
        cache.polynomial(t)
    }
}

impl PoseAngle {
    /// LimbPose.EvaluateAngleAt: the pose angle in degrees at an explicit animation time.
    pub fn degrees_at(&self, cache: &mut CurveCache, time: f32) -> f32 {
        match &self.animation {
            None => self.degrees(),
            Some(a) => {
                let t = a.time_offset + time / a.duration;
                a.start + (a.end - a.start) * a.curve.evaluate(cache, t).clamp(0.0, 1.0)
            }
        }
    }

    pub fn degrees(&self) -> f32 {
        self.angle_degrees
            .unwrap_or_else(|| self.angle.to_degrees())
    }

    pub fn radians(&self) -> f32 {
        self.angle_degrees.map_or(self.angle, f32::to_radians)
    }
}

#[derive(Deserialize)]
pub struct HumanPose {
    pub name: String,
    pub rigidity: f32,
    pub force_multiplier: f32,
    pub stand_upright: bool,
    pub upright_multiplier: f32,
    pub drag_influence: f32,
    #[serde(default)]
    pub state: i32,
    #[serde(default)]
    pub should_stumble: bool,
    #[serde(default = "one")]
    #[allow(dead_code)]
    pub animation_speed: f32,
    pub angles: Vec<PoseAngle>,
}

/// PoseState.Stumbling
pub const STUMBLING: i32 = 3;

#[derive(Deserialize)]
pub struct Joint {
    pub a: String,
    pub b: String,
    pub anchor_a: [f32; 2],
    pub anchor_b: [f32; 2],
    pub limits: Option<[f32; 2]>,
    #[serde(default)]
    pub limits_degrees: Option<[f32; 2]>,
}

impl Joint {
    pub fn radians_limits(&self) -> Option<[f32; 2]> {
        self.limits_degrees
            .map(|v| v.map(f32::to_radians))
            .or(self.limits)
    }
}

#[derive(Resource, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct SimulationSettings {
    pub fixed_delta: f64,
    pub gravity: [f32; 2],
    pub velocity_iterations: u32,
    pub position_iterations: u32,
    pub max_angular_correction_degrees: f32,
    pub max_translation: f32,
    pub max_rotation_degrees: f32,
}

impl Default for SimulationSettings {
    fn default() -> Self {
        Self {
            fixed_delta: 0.02,
            gravity: [0.0, -9.81],
            velocity_iterations: 16,
            position_iterations: 16,
            max_angular_correction_degrees: 8.0,
            max_translation: 100.0,
            max_rotation_degrees: 360.0,
        }
    }
}

impl SimulationSettings {
    /// VideoSettingsInitBehaviour.Sync overrides both Physics2D iteration counts with the user's
    /// The PhysicsIterations preference (UserPreferenceManager.Current, loaded by Prefs::load) for both iteration counts.
    fn apply_preferences(&mut self, game: &Path) {
        let iterations = crate::game::Prefs::load(game).PhysicsIterations as u32;
        self.velocity_iterations = iterations;
        self.position_iterations = iterations;
    }
}

#[derive(Resource, Deserialize)]
pub struct HumanRig {
    pub source_files: Vec<SourceFile>,
    pub parts: Vec<Part>,
    pub joints: Vec<Joint>,
    #[serde(default)]
    pub poses: Vec<HumanPose>,
    #[serde(default)]
    pub simulation: SimulationSettings,
}

fn is_relative_file(path: &Path) -> bool {
    !path.as_os_str().is_empty() && path.components().all(|c| matches!(c, Component::Normal(_)))
}

impl HumanRig {
    pub fn load(cache: &Path, game: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(cache.join("human.json")).map_err(|e| {
            format!("Missing human asset cache ({e}). Run scripts/prepare-assets.py first.")
        })?;
        let mut rig: Self =
            serde_json::from_str(&text).map_err(|e| format!("Invalid human cache: {e}"))?;
        rig.simulation.apply_preferences(game);
        rig.validate()?;
        if rig.source_files.len() != REQUIRED.len() {
            return Err("Incomplete installation fingerprint".into());
        }
        for required in REQUIRED {
            let source = rig
                .source_files
                .iter()
                .find(|s| s.path == required)
                .ok_or_else(|| format!("Missing source fingerprint: {required}"))?;
            let metadata = game.join(required).metadata().map_err(|_| {
                format!(
                    "People Playground must be installed. Missing: {}",
                    game.join(required).display()
                )
            })?;
            let modified = metadata
                .modified()
                .map_err(|e| e.to_string())?
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos();
            if !metadata.is_file()
                || metadata.len() != source.bytes
                || modified != source.modified_ns as u128
            {
                return Err(format!(
                    "Installed game differs from the asset cache: {required}. Export and prepare a fresh cache."
                ));
            }
        }
        for part in &rig.parts {
            if !cache.join(&part.texture).is_file() {
                return Err(format!("Missing cached texture: {}", part.texture));
            }
        }
        Ok(rig)
    }

    fn validate(&self) -> Result<(), String> {
        let simulation = &self.simulation;
        if !simulation.fixed_delta.is_finite()
            || !(0.0001..=1.0).contains(&simulation.fixed_delta)
            || !simulation.gravity.iter().all(|v| v.is_finite())
            || !(1..=256).contains(&simulation.velocity_iterations)
            || !(1..=256).contains(&simulation.position_iterations)
            || !simulation.max_angular_correction_degrees.is_finite()
            || !(0.0..=360.0).contains(&simulation.max_angular_correction_degrees)
            || !simulation.max_translation.is_finite()
            || simulation.max_translation < 0.0
            || !simulation.max_rotation_degrees.is_finite()
            || simulation.max_rotation_degrees < 0.0
        {
            return Err("Invalid simulation settings".into());
        }
        let names: HashSet<_> = self.parts.iter().map(|p| p.name.as_str()).collect();
        if self.parts.is_empty() || self.parts.len() > 128 || names.len() != self.parts.len() {
            return Err("Invalid or duplicate human bodies".into());
        }
        for p in &self.parts {
            let finite = p
                .position
                .iter()
                .chain(&p.size)
                .chain(&p.rect)
                .chain(&p.pivot)
                .chain([&p.angle, &p.mass, &p.linear_drag, &p.angular_drag])
                .all(|v| v.is_finite());
            if !finite
                || p.mass <= 0.0
                || p.size.iter().any(|v| *v <= 0.0)
                || p.rect[2..].iter().any(|v| *v <= 0.0)
                || p.colliders.is_empty()
                || !is_relative_file(Path::new(&p.texture))
                || p.linear_drag < 0.0
                || p.angular_drag < 0.0
            {
                return Err(format!("Invalid body data: {}", p.name));
            }
            for shape in &p.colliders {
                if !shape
                    .offset
                    .iter()
                    .chain(&shape.size)
                    .all(|v| v.is_finite())
                    || shape.size.iter().any(|v| *v <= 0.0)
                {
                    return Err(format!("Invalid collider: {}", p.name));
                }
            }
            if let Some(material) = &p.physics_material
                && (!material.friction.is_finite()
                    || material.friction < 0.0
                    || !material.restitution.is_finite()
                    || !(0.0..=1.0).contains(&material.restitution))
            {
                return Err(format!("Invalid physics material: {}", p.name));
            }
            if let Some(muscle) = &p.muscle
                && (!muscle.strength.is_finite()
                    || muscle.strength < 0.0
                    || !muscle.upright_force.is_finite()
                    || muscle.upright_force < 0.0
                    || !muscle.balance_muscle_movement.is_finite()
                    || !muscle.blood_strength_ratio.is_finite()
                    || !muscle.multipliers.iter().all(|m| m.is_finite()))
            {
                return Err(format!("Invalid muscle settings: {}", p.name));
            }
        }
        for pose in &self.poses {
            if ![
                pose.rigidity,
                pose.force_multiplier,
                pose.upright_multiplier,
                pose.drag_influence,
            ]
            .iter()
            .all(|v| v.is_finite() && *v >= 0.0)
                || pose.drag_influence > 1.0
            {
                return Err(format!("Invalid pose settings: {}", pose.name));
            }
            let mut posed = HashSet::new();
            for angle in &pose.angles {
                if !names.contains(angle.part.as_str())
                    || !posed.insert(angle.part.as_str())
                    || !angle.angle.is_finite()
                    || !angle.degrees().is_finite()
                    || !angle.radians().is_finite()
                    || !angle.rigidity_modifier.is_finite()
                {
                    return Err(format!("Invalid limb target in pose: {}", pose.name));
                }
            }
        }
        if !self.poses.is_empty() && self.parts.iter().any(|p| p.muscle.is_none()) {
            return Err("Pose cache is missing muscle settings; prepare a fresh cache".into());
        }
        for j in &self.joints {
            if j.a == j.b
                || !names.contains(j.a.as_str())
                || !names.contains(j.b.as_str())
                || !j.anchor_a.iter().chain(&j.anchor_b).all(|v| v.is_finite())
                || j.limits
                    .is_some_and(|v| !v[0].is_finite() || !v[1].is_finite() || v[0] > v[1])
                || j.radians_limits()
                    .is_some_and(|v| !v[0].is_finite() || !v[1].is_finite() || v[0] > v[1])
            {
                return Err("Invalid human joint".into());
            }
        }
        let child_names: HashSet<_> = self.joints.iter().map(|j| j.a.as_str()).collect();
        if self.joints.len() + 1 != self.parts.len() || child_names.len() != self.joints.len() {
            return Err("Human joints must form a single body tree".into());
        }
        let root = self
            .parts
            .iter()
            .find(|p| !child_names.contains(p.name.as_str()))
            .ok_or("Human has no root body")?;
        let mut reached = HashSet::from([root.name.as_str()]);
        loop {
            let previous = reached.len();
            for j in &self.joints {
                if reached.contains(j.b.as_str()) {
                    reached.insert(j.a.as_str());
                }
            }
            if reached.len() == previous {
                break;
            }
        }
        if reached.len() != self.parts.len() {
            return Err("Human joint graph is disconnected or cyclic".into());
        }
        Ok(())
    }
}

pub fn find_game(explicit: Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(path) = explicit.or_else(|| std::env::var_os("PPG_GAME_DIR").map(PathBuf::from)) {
        if REQUIRED.iter().all(|name| path.join(name).is_file()) {
            return Ok(path);
        }
        return Err(format!(
            "Not a complete People Playground installation: {}",
            path.display()
        ));
    }
    let mut steam_roots = vec![PathBuf::from(r"C:\Program Files (x86)\Steam")];
    for drive in b'C'..=b'Z' {
        steam_roots.push(PathBuf::from(format!("{}:/SteamLibrary", drive as char)));
        steam_roots.push(PathBuf::from(format!("{}:/Steam", drive as char)));
    }
    if let Ok(steam) = std::env::var("STEAM_DIR") {
        steam_roots.insert(0, steam.into());
    }
    let mut roots = steam_roots.clone();
    for root in steam_roots {
        let game = root.join("steamapps/common/People Playground");
        if REQUIRED.iter().all(|name| game.join(name).is_file()) {
            return Ok(game);
        }
        if let Ok(vdf) = std::fs::read_to_string(root.join("steamapps/libraryfolders.vdf")) {
            for line in vdf.lines().filter(|l| l.trim().starts_with("\"path\"")) {
                if let Some(path) = line.split('"').nth(3) {
                    roots.push(path.replace("\\\\", "\\").into());
                }
            }
        }
    }
    roots
        .into_iter()
        .map(|r| r.join("steamapps/common/People Playground"))
        .find(|p| REQUIRED.iter().all(|name| p.join(name).is_file()))
        .ok_or_else(|| {
            "Install People Playground, or pass --game-dir <installation> / set PPG_GAME_DIR."
                .into()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A temporary game folder: `config` is written as config.json when given.
    fn game_folder(name: &str, config: Option<&str>) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ppg-prefs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        if let Some(text) = config {
            std::fs::write(dir.join("config.json"), text).unwrap();
        }
        dir
    }

    #[test]
    fn missing_config_keeps_the_preference_default_iterations() {
        let dir = game_folder("missing", None);
        let mut s = SimulationSettings { velocity_iterations: 16, position_iterations: 16, ..Default::default() };
        s.apply_preferences(&dir);
        assert_eq!((s.velocity_iterations, s.position_iterations), (32, 32));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn config_sets_the_physics_iterations() {
        let dir = game_folder("set", Some(r#"{"PhysicsIterations": 8}"#));
        let mut s = SimulationSettings::default();
        s.apply_preferences(&dir);
        assert_eq!((s.velocity_iterations, s.position_iterations), (8, 8));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn test_rig() -> HumanRig {
        let part = serde_json::json!({"name":"A","position":[0,0],"angle":0,"mass":1,
            "linear_drag":0,"angular_drag":0,"colliders":[{"offset":[0,0],"size":[1,1]}],
            "texture":"test.png","rect":[0,0,1,1],"size":[1,1],"pivot":[0.5,0.5],"layer":0});
        let mut second = part.clone();
        second["name"] = "B".into();
        serde_json::from_value(serde_json::json!({"source_files":[],"parts":[part, second],
            "joints":[{"a":"B","b":"A","anchor_a":[0,0],"anchor_b":[0,0],"limits":null}]}))
        .unwrap()
    }

    #[test]
    fn legacy_caches_remain_valid_and_unsafe_joint_graphs_are_rejected() {
        let mut rig = test_rig();
        assert!(rig.validate().is_ok());
        rig.joints[0].a = "A".into();
        rig.joints[0].b = "A".into();
        assert!(rig.validate().is_err());
        let mut rig = test_rig();
        rig.joints.clear();
        assert!(rig.validate().is_err());
        let mut rig = test_rig();
        let mut third = test_rig().parts.pop().unwrap();
        third.name = "C".into();
        rig.parts.push(third);
        rig.joints[0].a = "A".into();
        rig.joints[0].b = "B".into();
        rig.joints.push(Joint {
            a: "B".into(),
            b: "A".into(),
            anchor_a: [0.0; 2],
            anchor_b: [0.0; 2],
            limits: None,
            limits_degrees: None,
        });
        assert!(rig.validate().is_err());
    }

    #[test]
    fn original_degrees_take_priority_without_breaking_legacy_caches() {
        let mut target: PoseAngle = serde_json::from_value(serde_json::json!({
            "part": "A", "angle": 0.001745329238474369f32,
            "angle_degrees": 0.1, "rigidity_modifier": 0
        }))
        .unwrap();
        assert_eq!(target.degrees().to_bits(), 0.1f32.to_bits());
        assert_eq!(target.radians().to_bits(), 0.1f32.to_radians().to_bits());
        target.angle_degrees = None;
        assert_eq!(target.radians(), target.angle);
        assert_eq!(target.degrees(), target.angle.to_degrees());

        let mut rig = test_rig();
        rig.joints[0].limits_degrees = Some([-0.1, 0.1]);
        assert_eq!(
            rig.joints[0].radians_limits(),
            Some([-0.1f32.to_radians(), 0.1f32.to_radians()])
        );
        rig.joints[0].limits_degrees = Some([1.0, -1.0]);
        assert!(rig.validate().is_err());
    }

    #[test]
    fn simulation_settings_have_legacy_defaults_and_reject_invalid_inputs() {
        let rig = test_rig();
        assert_eq!(rig.simulation.fixed_delta, 0.02);
        assert_eq!(rig.simulation.position_iterations, 16);
        for settings in [
            SimulationSettings {
                fixed_delta: f64::NAN,
                ..default()
            },
            SimulationSettings {
                fixed_delta: 0.0,
                ..default()
            },
            SimulationSettings {
                gravity: [0.0, f32::INFINITY],
                ..default()
            },
            SimulationSettings {
                velocity_iterations: 0,
                ..default()
            },
            SimulationSettings {
                position_iterations: 257,
                ..default()
            },
            SimulationSettings {
                max_angular_correction_degrees: -1.0,
                ..default()
            },
        ] {
            let mut rig = test_rig();
            rig.simulation = settings;
            assert!(rig.validate().is_err());
        }
    }

    #[test]
    fn physics_material_rejects_invalid_contact_coefficients() {
        for (friction, restitution) in [
            (-1.0, 0.2),
            (f32::NAN, 0.2),
            (1.0, 1.1),
            (1.0, f32::INFINITY),
        ] {
            let mut rig = test_rig();
            rig.parts[0].physics_material = Some(PhysicsMaterial {
                friction,
                restitution,
            });
            assert!(rig.validate().is_err());
        }
        let mut rig = test_rig();
        rig.parts[0].physics_material = Some(PhysicsMaterial {
            friction: 1.0,
            restitution: 0.2,
        });
        assert!(rig.validate().is_ok());
    }

    #[test]
    fn pose_curve_ping_pongs_outside_its_keys() {
        // Person.prefab Stumbling UpperLegFront.
        let curve = Curve {
            keys: vec![[0.0, 0.0, 2.0, 2.0], [0.5, 1.0, 2.0, -2.0], [1.0, 0.0, -2.0, -2.0]],
            pre: 0,
            post: 0,
        };
        let mut cache = CurveCache::default();
        let mut at = |t| curve.evaluate(&mut cache, t);
        assert_eq!(at(0.25), 0.5);
        assert_eq!(at(0.5), 1.0);
        assert_eq!(at(1.25), at(0.75));
        assert_eq!(at(-0.25), at(0.25));
    }

    #[test]
    fn invalid_muscle_values_do_not_reach_the_physics_engine() {
        let mut rig = test_rig();
        rig.parts[0].muscle = Some(MuscleSpec {
            strength: f32::INFINITY,
            multipliers: vec![],
            upright_force: 0.0,
            has_brain: true,
            do_balance_jerk: false,
            balance_muscle_movement: 0.0,
            do_stumble: false,
            blood_strength_ratio: 1.0,
        });
        assert!(rig.validate().is_err());
    }

    #[test]
    fn cache_paths_cannot_escape_the_cache() {
        assert!(is_relative_file(Path::new("skin.png")));
        assert!(!is_relative_file(Path::new("../game.dll")));
        assert!(!is_relative_file(Path::new(r"C:\game.dll")));
    }
    #[test]
    fn missing_game_is_rejected_before_startup() {
        assert!(
            find_game(Some(
                std::env::temp_dir().join("nonexistent-ppg-installation-50d177")
            ))
            .is_err()
        );
    }
}
