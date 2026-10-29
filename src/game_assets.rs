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
    pub strength: f32,
    pub upright_force: f32,
    pub has_brain: bool,
}

#[derive(Deserialize)]
pub struct PoseAngle {
    pub part: String,
    pub angle: f32,
    #[serde(default)]
    pub angle_degrees: Option<f32>,
    pub rigidity_modifier: f32,
}

impl PoseAngle {
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
    pub angles: Vec<PoseAngle>,
}

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
        let rig: Self =
            serde_json::from_str(&text).map_err(|e| format!("Invalid human cache: {e}"))?;
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
                    || muscle.upright_force < 0.0)
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
    fn invalid_muscle_values_do_not_reach_the_physics_engine() {
        let mut rig = test_rig();
        rig.parts[0].muscle = Some(MuscleSpec {
            strength: f32::INFINITY,
            upright_force: 0.0,
            has_brain: true,
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
