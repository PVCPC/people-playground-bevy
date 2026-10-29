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
}

#[derive(Deserialize)]
pub struct Joint {
    pub a: String,
    pub b: String,
    pub anchor_a: [f32; 2],
    pub anchor_b: [f32; 2],
    pub limits: Option<[f32; 2]>,
}

#[derive(Resource, Deserialize)]
pub struct HumanRig {
    pub source_files: Vec<SourceFile>,
    pub parts: Vec<Part>,
    pub joints: Vec<Joint>,
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
        }
        for j in &self.joints {
            if j.a == j.b
                || !names.contains(j.a.as_str())
                || !names.contains(j.b.as_str())
                || !j.anchor_a.iter().chain(&j.anchor_b).all(|v| v.is_finite())
                || j.limits
                    .is_some_and(|v| !v[0].is_finite() || !v[1].is_finite() || v[0] > v[1])
            {
                return Err("Invalid human joint".into());
            }
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
