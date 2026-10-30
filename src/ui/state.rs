//! The UI's state and its transitions, with no Bevy in them: menu navigation, the preferences store, the control scheme.

use super::tables::*;
use crate::game::{tool::Control, Prefs};
use bevy::prelude::Resource;
use std::collections::HashMap;

// ---------------------------------------------------------------- preferences

/// Every Preferences field (the settings menu's and the hidden ones), by `SETTINGS` index.
#[derive(Resource, Clone, Debug)]
pub struct Settings {
    pub values: Vec<f32>,
}

/// Utils.Snap
fn snap(v: f32, step: f32) -> f32 {
    if step.abs() < 1e-6 { v } else { (v / step).round() * step }
}

impl Default for Settings {
    fn default() -> Self {
        Self { values: SETTINGS.iter().map(|s| s.default).collect() }
    }
}

/// Fields the rewrite acts on: the simulation's `Prefs`, the window, and what this UI itself reads. The others are
/// stored and shown but have nothing to drive yet.
pub fn applies(field: &str) -> bool {
    matches!(
        field,
        "CollisionQuality" | "AmbientTemperatureTransfer" | "BrainDamage" | "AutoHealWounds" | "StopAnimationOnDamage"
            | "FragilityMultiplier" | "LimbCrushing" | "GorelessMode" | "DropOnDeath" | "DismembermentLooseTissue"
            | "CrushForceMultiplier" | "ProceduralFragments" | "FancyEffects" | "Decals" | "ToolSelectionMode" | "GridSnap"
            | "PhysicsIterations" | "ShowTutorial" | "DistantSoundEffects" | "TracerBullets" | "BulletCrackWhiz"
            | "ChunkyShotParticles" | "ShakeIntensity" | "ShowOutlines" | "SlowMotionSpeed" | "ZoomSensitivity"
            | "ShowFramerate" | "TemperatureUnit" | "VSync" | "WindowMode"
    )
}

/// An option the rewrite cannot run (the Depth selection mode panics in the simulation).
pub fn option_unavailable(field: &str, option: usize) -> bool {
    field == "ToolSelectionMode" && option == 2
}

impl Settings {
    pub fn index(field: &str) -> usize {
        SETTINGS.iter().position(|s| s.field == field).expect("a Preferences field")
    }

    pub fn get(&self, field: &str) -> f32 {
        self.values[Self::index(field)]
    }

    pub fn flag(&self, field: &str) -> bool {
        self.get(field) != 0.0
    }

    /// Stores a value the way the settings menu's widget would: whole for ints, snapped to the step, clamped, the
    /// nearest option for option lists.
    pub fn set(&mut self, i: usize, v: f32) {
        self.values[i] = match SETTINGS[i].kind {
            Kind::Bool => (v != 0.0) as i32 as f32,
            Kind::Enum(names) => v.round().clamp(0.0, names.len() as f32 - 1.0),
            Kind::Int { min, max, step } => snap(v.round(), step).round().clamp(min, max),
            Kind::Float { min, max, step } => snap(v, step).clamp(min, max),
            Kind::Options(options) => *options.iter().min_by(|a, b| (*a - v).abs().total_cmp(&(*b - v).abs())).unwrap(),
        };
    }

    /// 0..1 along a slider's track.
    pub fn fraction(&self, i: usize) -> f32 {
        match SETTINGS[i].kind {
            Kind::Int { min, max, .. } | Kind::Float { min, max, .. } => ((self.values[i] - min) / (max - min)).clamp(0.0, 1.0),
            _ => 0.0,
        }
    }

    pub fn set_fraction(&mut self, i: usize, f: f32) {
        if let Kind::Int { min, max, .. } | Kind::Float { min, max, .. } = SETTINGS[i].kind {
            self.set(i, min + f.clamp(0.0, 1.0) * (max - min));
        }
    }

    /// The value as the settings menu prints it (the Format attribute, else ToString).
    pub fn text(&self, i: usize) -> String {
        let (s, v) = (&SETTINGS[i], self.values[i]);
        match s.kind {
            Kind::Bool => if v != 0.0 { "Enabled" } else { "Disabled" }.into(),
            Kind::Enum(names) => names[v as usize].into(),
            Kind::Int { .. } | Kind::Float { .. } | Kind::Options(_) => {
                let int = matches!(s.kind, Kind::Int { .. });
                match s.fmt {
                    None => if int { format!("{}", v as i32) } else { format!("{v}") },
                    Some(f) => {
                        let x = (v * f.mult) as f64;
                        let x = if int { x } else { let k = 10f64.powi(f.digits); (x * k).round() / k };
                        f.template.replace("{0}", &format!("{x}"))
                    }
                }
            }
        }
    }

    /// Members of a preferences object (config.json, or our own file) over the current values; other members and
    /// values of the wrong type are ignored.
    pub fn load(&mut self, text: &str) {
        let Ok(serde_json::Value::Object(obj)) = serde_json::from_str(text) else { return };
        for (i, s) in SETTINGS.iter().enumerate() {
            let Some(v) = obj.get(s.field) else { continue };
            let v = match (v, s.kind) {
                (serde_json::Value::Bool(b), Kind::Bool) => *b as i32 as f32,
                (serde_json::Value::String(n), Kind::Enum(names)) => match names.iter().position(|x| x.eq_ignore_ascii_case(n)) {
                    Some(p) => p as f32,
                    None => continue,
                },
                (serde_json::Value::Number(n), k) if !matches!(k, Kind::Bool) => n.as_f64().unwrap_or(s.default as f64) as f32,
                _ => continue,
            };
            self.set(i, v);
        }
    }

    pub fn json(&self) -> String {
        let mut obj = serde_json::Map::new();
        for (i, s) in SETTINGS.iter().enumerate() {
            let v = self.values[i];
            obj.insert(
                s.field.into(),
                match s.kind {
                    Kind::Bool => (v != 0.0).into(),
                    Kind::Enum(names) => names[v as usize].into(),
                    _ => ((v as f64 * 1e4).round() / 1e4).into(),
                },
            );
        }
        serde_json::to_string_pretty(&obj).unwrap()
    }

    /// The simulation's Preferences.
    pub fn prefs(&self) -> Prefs {
        let mut p = Prefs::default();
        let (b, f) = (|n| self.flag(n), |n| self.get(n));
        p.CollisionQuality = f("CollisionQuality") as i32;
        p.AmbientTemperatureTransfer = b("AmbientTemperatureTransfer");
        p.BrainDamage = b("BrainDamage");
        p.AutoHealWounds = b("AutoHealWounds");
        p.StopAnimationOnDamage = b("StopAnimationOnDamage");
        p.FragilityMultiplier = f("FragilityMultiplier");
        p.LimbCrushing = b("LimbCrushing");
        p.GorelessMode = b("GorelessMode");
        p.DropOnDeath = b("DropOnDeath");
        p.DismembermentLooseTissue = b("DismembermentLooseTissue");
        p.CrushForceMultiplier = f("CrushForceMultiplier");
        p.ProceduralFragments = b("ProceduralFragments");
        p.FancyEffects = b("FancyEffects");
        p.Decals = b("Decals");
        // Depth is not rewritten: the simulation keeps the default (Area).
        if !option_unavailable("ToolSelectionMode", f("ToolSelectionMode") as usize) {
            p.ToolSelectionMode = f("ToolSelectionMode") as i32;
        }
        p.GridSnap = f("GridSnap");
        p.PhysicsIterations = f("PhysicsIterations") as i32;
        p.ShowTutorial = b("ShowTutorial");
        p.DistantSoundEffects = b("DistantSoundEffects");
        p.TracerBullets = b("TracerBullets");
        p.BulletCrackWhiz = b("BulletCrackWhiz");
        p.ChunkyShotParticles = b("ChunkyShotParticles");
        p.ShakeIntensity = f("ShakeIntensity");
        p
    }
}

/// The settings table's rows for a category tab, in the menu's order.
pub fn tab_rows(category: usize) -> impl Iterator<Item = usize> {
    SETTINGS.iter().enumerate().filter(move |(_, s)| s.menu && s.category == category).map(|(i, _)| i)
}

// ---------------------------------------------------------------- control scheme

/// Each action's Key and SecondaryKey (Unity names), by `ACTIONS` index.
#[derive(Resource, Clone, Debug)]
pub struct Controls {
    pub binds: Vec<(String, String)>,
}

impl Default for Controls {
    fn default() -> Self {
        Self { binds: ACTIONS.iter().map(|a| (a.key.to_string(), a.secondary.to_string())).collect() }
    }
}

fn universes_match(a: &str, b: &str) -> bool {
    a != "None" && b != "None" && (a == "All" || b == "All" || a == b)
}

impl Controls {
    pub fn index(id: &str) -> usize {
        ACTIONS.iter().position(|a| a.id == id).expect("an action")
    }

    pub fn bind(&self, id: &str) -> &(String, String) {
        &self.binds[Self::index(id)]
    }

    /// TriggerEditorBehaviour.SetText
    pub fn text(&self, i: usize) -> String {
        let (k, s) = &self.binds[i];
        let name = |n: &str| super::keys::display(n);
        match s.as_str() {
            "None" => name(k),
            s => format!("{} + {}", name(s), name(k)),
        }
    }

    /// TriggerEditorBehaviour.AreConflicting against any other action.
    pub fn conflicts(&self, i: usize) -> bool {
        let (a, b) = (&self.binds[i], ACTIONS[i]);
        (0..ACTIONS.len()).any(|j| {
            if j == i || !universes_match(b.universe, ACTIONS[j].universe) {
                return false;
            }
            let o = &self.binds[j];
            let (k, s) = (a.0 != "None", a.1 != "None");
            (k && a.0 == o.0) || (s && a.1 == o.1) || (s && a.1 == o.0 && o.0 != "None") || (o.1 != "None" && o.1 == a.0 && k)
        })
    }

    /// `rebind`: the key released and the key held with it.
    pub fn set(&mut self, i: usize, key: &str, secondary: &str) {
        self.binds[i] = (key.into(), secondary.into());
    }

    pub fn json(&self) -> String {
        let mut obj = serde_json::Map::new();
        for (a, (k, s)) in ACTIONS.iter().zip(&self.binds) {
            obj.insert(a.id.into(), serde_json::json!({ "Key": k, "SecondaryKey": s }));
        }
        serde_json::to_string_pretty(&obj).unwrap()
    }

    pub fn load(&mut self, text: &str) {
        let Ok(serde_json::Value::Object(obj)) = serde_json::from_str(text) else { return };
        for (i, a) in ACTIONS.iter().enumerate() {
            let name = |f: &str| obj.get(a.id).and_then(|v| v.get(f)).and_then(|v| v.as_str()).filter(|n| *n == "None" || super::keys::src(n).is_some());
            if let (Some(k), Some(s)) = (name("Key"), name("SecondaryKey")) {
                self.binds[i] = (k.into(), s.into());
            }
        }
    }
}

/// What the game sees of a control: only presses that began where the world may take input go through, until
/// they end.
pub fn gate(owned: &mut bool, raw: Control, allowed: bool) -> Control {
    if raw.down && allowed {
        *owned = true;
    }
    if !*owned {
        return Control::default();
    }
    // Over once it is released, or the world no longer takes input.
    let up = raw.up || !allowed || (!raw.held && !raw.down);
    *owned = raw.held && !up;
    Control { down: raw.down, held: raw.held && allowed, up }
}

/// Utils.SmoothApproach
pub fn smooth_approach(past: f32, target: f32, speed: f32, dt: f32) -> f32 {
    target + (past - target) * (-dt * speed).exp()
}

/// Global.Update's time scale for this frame: 0 paused, SlowMotionSpeed % slowed, else 1.
pub fn time_scale_target(stopped: bool, slowmo: bool, speed_percent: f32) -> f32 {
    if stopped { 0.0 } else if slowmo { speed_percent / 100.0 } else { 1.0 }
}

/// A temperature in degrees Celsius, in the chosen TemperatureUnit (0 Celsius, 1 Fahrenheit, 2 Kelvin).
pub fn temperature_text(celsius: f32, unit: usize) -> String {
    match unit {
        1 => format!("{} F", ((celsius * 9.0 / 5.0 + 32.0) * 100.0).round() / 100.0),
        2 => format!("{} K", ((celsius + 273.15) * 100.0).round() / 100.0),
        _ => format!("{celsius} C"),
    }
}

// ---------------------------------------------------------------- navigation

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Main,
    Maps,
    Settings,
    Controls,
    Info,
    Mods,
    Stats,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Menu,
    Loading,
    Game,
    Pause,
}

/// What the buttons and keys ask for.
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    Nav(Page),
    Back,
    /// Esc / the "pause" action.
    Escape,
    SelectMap(usize),
    Enter,
    Quit,
    Resume,
    Restart,
    ToMenu,
    /// Click into the catalog's filter box.
    Filter,
    /// Settings tab: a `CATEGORIES` index.
    Tab(usize),
    Slowmo,
    TimeToggle,
    Category(usize),
    Item(String),
    Powers(bool),
    Environment,
    /// The map options' "Clear Mess" and "Clear Living" (the game runs them).
    ClearMess,
    ClearLiving,
    Set(usize, f32),
    Rebind(usize),
    Unbind(usize),
    ResetControls,
}

/// What a transition needs the world for.
#[derive(Debug, PartialEq)]
pub enum Effect {
    None,
    Load(String),
    Quit,
    ToMenu,
}

pub struct MapRow {
    pub name: String,
    pub description: String,
}

#[derive(Resource)]
pub struct Ui {
    pub mode: Mode,
    pub pages: Vec<Page>,
    pub tab: usize,
    pub maps: Vec<MapRow>,
    pub selected: Option<usize>,
    last_click: (usize, f64),
    pub map: Option<String>,
    /// The installed game's folder.
    pub game: std::path::PathBuf,
    pub catalog: super::data::Catalog,
    pub paused: bool,
    pub slowmo: bool,
    pub powers: bool,
    pub category: usize,
    pub filter: String,
    /// Keys go to the filter box.
    pub typing: bool,
    pub item: Option<String>,
    pub env_open: bool,
    pub hide_panels: bool,
    pub hide_ui: bool,
    pub context: Option<[f32; 2]>,
    pub rebinding: Option<usize>,
    pub toast: Option<(String, f64)>,
    pub error: Option<String>,
    pub dirty: bool,
    pub scroll: HashMap<u8, f32>,
}

impl Ui {
    pub fn new(maps: Vec<MapRow>, mode: Mode, map: Option<String>, item: Option<String>) -> Self {
        Self {
            mode,
            pages: vec![Page::Main],
            tab: 0,
            maps,
            selected: None,
            last_click: (usize::MAX, f64::MIN),
            map,
            paused: false,
            slowmo: false,
            powers: false,
            category: 0,
            filter: String::new(),
            typing: false,
            item,
            game: Default::default(),
            catalog: Default::default(),
            env_open: false,
            hide_panels: false,
            hide_ui: false,
            context: None,
            rebinding: None,
            toast: None,
            error: None,
            dirty: true,
            scroll: HashMap::new(),
        }
    }

    /// A map has been loaded and the game starts.
    pub fn started(&mut self) {
        self.mode = Mode::Game;
        self.pages = vec![Page::Main];
        self.paused = false;
        self.slowmo = false;
        self.context = None;
        self.dirty = true;
    }

    pub fn failed(&mut self, error: String) {
        self.mode = Mode::Menu;
        self.pages = vec![Page::Main, Page::Maps];
        self.error = Some(error);
        self.dirty = true;
    }

    fn pop(&mut self) {
        if self.pages.len() > 1 {
            self.pages.pop();
        }
    }

    /// The transitions that need no world. `now` is in seconds (double clicks).
    pub fn press(&mut self, act: &Act, now: f64) -> Effect {
        self.dirty = true;
        match act {
            Act::Nav(p) => {
                // Opened from the toolbar, a page belongs to the pause menu.
                if self.mode == Mode::Game {
                    self.mode = Mode::Pause;
                    self.pages = vec![Page::Main];
                }
                self.pages.push(*p);
                self.error = None;
            }
            Act::Back => self.pop(),
            Act::Escape => match self.mode {
                Mode::Game if self.context.take().is_some() => {}
                Mode::Game => {
                    self.mode = Mode::Pause;
                    self.pages = vec![Page::Main];
                }
                Mode::Pause if self.pages.len() > 1 => self.pop(),
                Mode::Pause => self.mode = Mode::Game,
                Mode::Menu => self.pop(),
                Mode::Loading => {}
            },
            Act::SelectMap(i) => {
                self.selected = Some(*i);
                let double = self.last_click.0 == *i && now - self.last_click.1 < 0.2;
                self.last_click = (*i, now);
                if double {
                    return self.press(&Act::Enter, now);
                }
            }
            Act::Enter => {
                if let Some(m) = self.selected.and_then(|i| self.maps.get(i)) {
                    self.map = Some(m.name.clone());
                    self.mode = Mode::Loading;
                    return Effect::Load(m.name.clone());
                }
            }
            Act::Restart => {
                if let Some(m) = self.map.clone() {
                    self.mode = Mode::Loading;
                    return Effect::Load(m);
                }
            }
            Act::Resume => self.mode = Mode::Game,
            Act::ToMenu => {
                self.mode = Mode::Menu;
                self.pages = vec![Page::Main];
                return Effect::ToMenu;
            }
            Act::Quit => return Effect::Quit,
            Act::Tab(t) => self.tab = *t,
            Act::Slowmo => self.slowmo = !self.slowmo,
            Act::TimeToggle => self.paused = !self.paused,
            Act::Category(c) => {
                self.category = *c;
                self.filter.clear();
                self.typing = false;
            }
            Act::Item(n) => self.item = Some(n.clone()),
            Act::Filter => self.typing = !self.typing,
            Act::Powers(p) => self.powers = *p,
            Act::Environment => self.env_open = !self.env_open,
            // Settings and control edits are applied where the stores live.
            Act::Set(..) | Act::Rebind(_) | Act::Unbind(_) | Act::ResetControls | Act::ClearMess | Act::ClearLiving => {}
        }
        Effect::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu() -> Ui {
        let rows = ["Default", "Arena"].map(|n| MapRow { name: n.into(), description: String::new() });
        Ui::new(rows.into(), Mode::Menu, None, None)
    }

    #[test]
    fn play_select_enter_loads_the_map() {
        let mut ui = menu();
        assert_eq!(ui.press(&Act::Enter, 0.0), Effect::None, "nothing selected");
        ui.press(&Act::Nav(Page::Maps), 0.0);
        ui.press(&Act::SelectMap(1), 1.0);
        assert_eq!(ui.press(&Act::Enter, 2.0), Effect::Load("Arena".into()));
        assert_eq!(ui.mode, Mode::Loading);
        ui.started();
        assert_eq!(ui.mode, Mode::Game);
        // A failed load goes back to the map list with the message.
        ui.failed("boom".into());
        assert_eq!((ui.mode, ui.pages.last().copied(), ui.error.as_deref()), (Mode::Menu, Some(Page::Maps), Some("boom")));
    }

    #[test]
    fn double_click_enters() {
        let mut ui = menu();
        ui.press(&Act::Nav(Page::Maps), 0.0);
        assert_eq!(ui.press(&Act::SelectMap(0), 1.0), Effect::None);
        assert_eq!(ui.press(&Act::SelectMap(0), 1.1), Effect::Load("Default".into()));
        let mut ui = menu();
        ui.press(&Act::SelectMap(0), 1.0);
        assert_eq!(ui.press(&Act::SelectMap(0), 1.5), Effect::None, "too slow");
    }

    #[test]
    fn escape_walks_back_and_pauses() {
        let mut ui = menu();
        ui.press(&Act::Nav(Page::Settings), 0.0);
        ui.press(&Act::Nav(Page::Controls), 0.0);
        ui.press(&Act::Escape, 0.0);
        assert_eq!(ui.pages, [Page::Main, Page::Settings]);
        ui.press(&Act::Back, 0.0);
        ui.press(&Act::Escape, 0.0);
        assert_eq!((ui.mode, ui.pages.len()), (Mode::Menu, 1), "Esc at the main menu stays");
        ui.started();
        ui.press(&Act::Escape, 0.0);
        assert_eq!(ui.mode, Mode::Pause);
        ui.press(&Act::Nav(Page::Settings), 0.0);
        ui.press(&Act::Escape, 0.0);
        assert_eq!((ui.mode, ui.pages.len()), (Mode::Pause, 1));
        ui.press(&Act::Resume, 0.0);
        ui.press(&Act::Nav(Page::Settings), 0.0);
        assert_eq!((ui.mode, ui.pages.clone()), (Mode::Pause, vec![Page::Main, Page::Settings]), "the toolbar's Settings button");
        ui.press(&Act::Back, 0.0);
        ui.press(&Act::Escape, 0.0);
        assert_eq!(ui.mode, Mode::Game);
        // Esc closes the context menu first.
        ui.context = Some([1.0, 2.0]);
        ui.press(&Act::Escape, 0.0);
        assert_eq!((ui.mode, ui.context), (Mode::Game, None));
    }

    #[test]
    fn pause_menu_actions() {
        let mut ui = menu();
        ui.map = Some("Default".into());
        ui.started();
        ui.press(&Act::TimeToggle, 0.0);
        ui.press(&Act::Slowmo, 0.0);
        assert!(ui.paused && ui.slowmo);
        assert_eq!(ui.press(&Act::Restart, 0.0), Effect::Load("Default".into()));
        ui.started();
        assert!(!ui.paused && !ui.slowmo, "a new game starts at full speed");
        assert_eq!(ui.press(&Act::ToMenu, 0.0), Effect::ToMenu);
        assert_eq!(ui.press(&Act::Quit, 0.0), Effect::Quit);
    }

    #[test]
    fn catalog_selection_is_what_the_game_spawns() {
        let mut ui = menu();
        ui.press(&Act::Item("Pistol".into()), 0.0);
        assert_eq!(ui.item.as_deref(), Some("Pistol"));
        ui.filter = "x".into();
        ui.press(&Act::Category(2), 0.0);
        assert_eq!((ui.category, ui.filter.as_str()), (2, ""));
        ui.press(&Act::Filter, 0.0);
        assert!(ui.typing);
        ui.press(&Act::Category(0), 0.0);
        assert!(!ui.typing);
    }

    #[test]
    fn settings_defaults_match_the_game_and_the_simulation() {
        let s = Settings::default();
        let d = Prefs::default();
        let p = s.prefs();
        assert_eq!(format!("{p:?}"), format!("{d:?}"));
        assert_eq!(s.get("SlowMotionSpeed"), 20.0);
        assert_eq!(s.get("WindowMode"), 1.0);
        assert!(SETTINGS.iter().all(|s| s.default.is_finite()));
    }

    #[test]
    fn settings_text_follows_the_format_attributes() {
        let mut s = Settings::default();
        let t = |s: &Settings, f: &str| s.text(Settings::index(f));
        assert_eq!(t(&s, "PhysicsIterations"), "32 iterations");
        assert_eq!(t(&s, "ShakeIntensity"), "1x");
        assert_eq!(t(&s, "GridSnap"), "0.25 m");
        assert_eq!(t(&s, "CollisionQuality"), "Dynamic");
        assert_eq!(t(&s, "ShowOutlines"), "Enabled");
        assert_eq!(t(&s, "MasterVolume"), "100%");
        assert_eq!(t(&s, "RenderScale"), "100%");
        s.set(Settings::index("CrushForceMultiplier"), 0.57);
        assert_eq!(t(&s, "CrushForceMultiplier"), "55%");
        s.set(Settings::index("RenderScale"), 0.6);
        assert_eq!(t(&s, "RenderScale"), "50%");
        s.set(Settings::index("AngleSnap"), 17.4);
        assert_eq!(t(&s, "AngleSnap"), "17 degrees");
    }

    #[test]
    fn settings_clamp_snap_and_round_trip() {
        let mut s = Settings::default();
        let i = Settings::index("PhysicsIterations");
        s.set(i, 1000.0);
        assert_eq!(s.get("PhysicsIterations"), 256.0);
        s.set(Settings::index("ThermalVisionUpdateRate"), 33.0);
        assert_eq!(s.get("ThermalVisionUpdateRate"), 35.0);
        s.set_fraction(Settings::index("Brightness"), 0.5);
        assert_eq!(s.get("Brightness"), 205.0);
        s.set(Settings::index("TemperatureUnit"), 2.0);
        s.set(Settings::index("GorelessMode"), 1.0);
        s.set(Settings::index("FragilityMultiplier"), 0.5);
        let mut t = Settings::default();
        t.load(&s.json());
        assert_eq!(t.values, s.values);
        assert!(t.prefs().GorelessMode);
        assert_eq!(t.prefs().FragilityMultiplier, 0.5);
        // The game's config.json: names or numbers for enums, unknown members ignored.
        let mut g = Settings::default();
        g.load(r#"{"CollisionQuality":"Continuous","ToolSelectionMode":1,"Resolution":{"x":1},"Brightness":"bad","GridSnap":0.5}"#);
        assert_eq!((g.get("CollisionQuality"), g.get("ToolSelectionMode"), g.get("GridSnap"), g.get("Brightness")), (1.0, 1.0, 0.5, 100.0));
    }

    #[test]
    fn menu_tabs_come_from_the_table() {
        let general: Vec<_> = tab_rows(0).map(|i| SETTINGS[i].field).collect();
        assert_eq!(general.first(), Some(&"CollisionQuality"));
        assert!(!general.contains(&"ShowTutorial"), "hidden fields stay out of the menu");
        assert_eq!(SETTINGS_MENU.iter().map(|e| e.1).collect::<Vec<_>>(), ["General", "Controls", "Gore", "VisualEffects", "UserInterface", "Audio", "Video", "Back"]);
        for c in 0..CATEGORIES.len() {
            assert!(tab_rows(c).count() > 0, "{c}");
        }
    }

    #[test]
    fn controls_defaults_conflicts_and_files() {
        let mut c = Controls::default();
        assert_eq!(ACTIONS.len(), 57);
        assert_eq!(c.text(Controls::index("drag")), "Left Mouse");
        assert_eq!(c.text(Controls::index("slowmo")), "G");
        let slowmo = Controls::index("slowmo");
        assert!(!c.conflicts(slowmo), "the map editor's G is another universe");
        c.set(slowmo, "Space", "None");
        assert!(c.conflicts(slowmo) && c.conflicts(Controls::index("time")));
        c.set(slowmo, "Space", "LeftShift");
        assert_eq!(c.text(slowmo), "LeftShift + Space");
        let mut d = Controls::default();
        d.load(&c.json());
        assert_eq!(d.binds, c.binds);
    }

    #[test]
    fn gate_passes_only_presses_that_began_in_the_world() {
        let held = |down, held, up| Control { down, held, up };
        let mut o = false;
        // Pressed over the UI: nothing reaches the game, not even later in the world.
        assert_eq!(gate(&mut o, held(true, true, false), false), Control::default());
        assert_eq!(gate(&mut o, held(false, true, false), true), Control::default());
        // Pressed in the world and then dragged over the UI: released.
        assert_eq!(gate(&mut o, held(true, true, false), true), held(true, true, false));
        assert_eq!(gate(&mut o, held(false, true, false), true), held(false, true, false));
        assert_eq!(gate(&mut o, held(false, true, false), false), held(false, false, true));
        assert_eq!(gate(&mut o, held(false, true, false), true), Control::default());
        // A normal release.
        gate(&mut o, held(true, true, false), true);
        assert_eq!(gate(&mut o, held(false, false, true), true), held(false, false, true));
    }

    #[test]
    fn time_scale_follows_the_games_smoothing() {
        assert_eq!(smooth_approach(1.0, 1.0, 15.0, 0.016), 1.0, "full speed stays exactly 1");
        assert_eq!(time_scale_target(false, true, 20.0), 0.2);
        assert_eq!(time_scale_target(true, true, 20.0), 0.0);
        let mut ts = 1.0;
        for _ in 0..60 {
            ts = smooth_approach(ts, 0.2, 15.0, 1.0 / 60.0);
        }
        assert!((ts - 0.2).abs() < 1e-3);
        assert_eq!(temperature_text(18.0, 1), "64.4 F");
        assert_eq!(temperature_text(18.0, 2), "291.15 K");
    }
}
