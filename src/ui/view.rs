//! Spawns the UI's entities for the current state; `live` keeps their texts and colours current in between.

use super::state::*;
use super::tables::*;
use bevy::{
    picking::hover::Hovered,
    prelude::*,
    text::FontSize,
    ui::{RelativeCursorPosition, ScrollPosition},
};

const PANEL: Color = Color::srgba(0.08, 0.09, 0.12, 0.94);
const BASE: Color = Color::srgb(0.17, 0.19, 0.24);
const HOVER: Color = Color::srgb(0.27, 0.31, 0.4);
const SELECTED: Color = Color::srgb(0.2, 0.42, 0.68);
const SELECTED_HOVER: Color = Color::srgb(0.27, 0.5, 0.78);
const DISABLED: Color = Color::srgb(0.12, 0.13, 0.16);
const CONFLICT: Color = Color::srgb(0.45, 0.16, 0.16);
const TEXT: Color = Color::srgb(0.88, 0.91, 0.95);
const DIM: Color = Color::srgb(0.48, 0.52, 0.58);
const WARN: Color = Color::srgb(0.95, 0.55, 0.45);

/// The "not rewritten yet" marker the UI uses wherever the rewrite has nothing behind a control.
pub const NOT_YET: &str = "not rewritten yet";

#[derive(Component)]
pub struct Root;

/// A widget a click activates.
#[derive(Component)]
pub struct Press(pub Act);

/// Shown in the tooltip line while the pointer is over it.
#[derive(Component)]
pub struct Tip(pub String);

#[derive(Component)]
pub enum Bound {
    Setting(usize),
    Bind(usize),
    Status,
    Tip,
    Toast,
    Filter,
}

#[derive(Component)]
pub struct SliderOf(pub usize);

#[derive(Component)]
pub struct FillOf(pub usize);

#[derive(Component)]
pub struct Scroll(pub u8);

/// What makes a widget look selected.
#[derive(Clone, PartialEq)]
pub enum Sel {
    None,
    Setting(usize, f32),
    Slowmo,
    Time,
    Category(usize),
    Item(String),
    Tab(usize),
    Powers(bool),
    Env,
    Map(usize),
    Bind(usize),
    Tool,
}

impl Sel {
    pub fn selected(&self, ui: &Ui, settings: &Settings, _: &Controls) -> bool {
        match self {
            Sel::None => false,
            Sel::Setting(i, v) => settings.values[*i] == *v,
            Sel::Slowmo => ui.slowmo,
            Sel::Time => ui.paused,
            Sel::Category(c) => ui.category == *c && ui.filter.is_empty(),
            Sel::Item(n) => ui.item.as_deref() == Some(n),
            Sel::Tab(t) => ui.tab == *t,
            Sel::Powers(p) => ui.powers == *p,
            Sel::Env => ui.env_open,
            Sel::Map(i) => ui.selected == Some(*i),
            Sel::Bind(i) => ui.rebinding == Some(*i),
            Sel::Tool => true,
        }
    }
}

#[derive(Component)]
pub struct Look {
    pub sel: Sel,
    pub on: bool,
}

impl Look {
    pub fn colour(&self, selected: bool, hovered: bool, _: &Ui, controls: &Controls) -> Color {
        if !self.on {
            return if selected { SELECTED } else { DISABLED };
        }
        let conflict = matches!(self.sel, Sel::Bind(i) if controls.conflicts(i));
        match (selected, hovered, conflict) {
            (true, true, _) => SELECTED_HOVER,
            (true, false, _) => SELECTED,
            (false, true, _) => HOVER,
            (false, false, true) => CONFLICT,
            _ => BASE,
        }
    }
}

type Kids<'a> = ChildSpawnerCommands<'a>;

fn font(size: f32) -> TextFont {
    TextFont { font_size: FontSize::Px(size), ..default() }
}

fn label(p: &mut Kids, s: impl Into<String>, size: f32, color: Color) {
    p.spawn((Text::new(s), font(size), TextColor(color)));
}

fn bound(p: &mut Kids, b: Bound, size: f32, color: Color) {
    p.spawn((Text::new(""), font(size), TextColor(color), b));
}

/// A text button; without an act it is disabled.
fn button(p: &mut Kids, text: &str, act: Option<Act>, sel: Sel, tip: &str, width: Val) {
    let on = act.is_some();
    let mut e = p.spawn((
        Node { width, padding: UiRect::axes(px(10), px(5)), justify_content: JustifyContent::Center, ..default() },
        BackgroundColor(if on { BASE } else { DISABLED }),
        Hovered::default(),
        Look { sel, on },
        Tip(tip.into()),
    ));
    if let Some(a) = act {
        e.insert(Press(a));
    }
    e.with_children(|c| label(c, text, 15.0, if on { TEXT } else { DIM }));
}

/// A button for something the rewrite has not got yet.
fn missing(p: &mut Kids, text: &str, tip: &str, width: Val) {
    let tip = if tip.is_empty() { NOT_YET.to_string() } else { format!("{tip} ({NOT_YET})") };
    button(p, text, None, Sel::None, &tip, width);
}

fn column(gap: f32) -> Node {
    Node { flex_direction: FlexDirection::Column, row_gap: px(gap), ..default() }
}

fn row(gap: f32) -> Node {
    Node { flex_direction: FlexDirection::Row, column_gap: px(gap), align_items: AlignItems::Center, ..default() }
}

fn scroll_area(p: &mut Kids, ui: &Ui, id: u8, width: Val, height: Val, fill: impl FnOnce(&mut Kids)) {
    p.spawn((
        Node { width, height, flex_direction: FlexDirection::Column, row_gap: px(6), padding: UiRect::all(px(6)), overflow: Overflow::scroll_y(), ..default() },
        ScrollPosition(Vec2::new(0.0, ui.scroll.get(&id).copied().unwrap_or(0.0))),
        Scroll(id),
        Hovered::default(),
    ))
    .with_children(fill);
}

/// ControlSchemeEditorBehaviour: the category name with a space before each capital.
fn spaced(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && c.is_uppercase() {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

fn category_index(key: &str) -> usize {
    CATEGORIES.iter().position(|c| c.0 == key).expect("a SettingCategory")
}

pub fn build(commands: &mut Commands, ui: &Ui, settings: &Settings, controls: &Controls, stopped: Option<&str>) {
    commands
        .spawn((
            Root,
            Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), flex_direction: FlexDirection::Column, ..default() },
            Pickable::IGNORE,
            GlobalZIndex(10),
        ))
        .with_children(|root| {
            match ui.mode {
                Mode::Game => {
                    if !ui.hide_ui {
                        hud(root, ui, settings, controls, stopped);
                    }
                }
                Mode::Loading => overlay(root, Color::srgb(0.05, 0.06, 0.08), |p| {
                    label(p, format!("Loading {}...", ui.map.as_deref().unwrap_or("")), 28.0, TEXT);
                }),
                Mode::Menu => overlay(root, Color::srgb(0.05, 0.06, 0.08), |p| page(p, ui, settings, controls)),
                Mode::Pause => {
                    hud(root, ui, settings, controls, stopped);
                    overlay(root, Color::srgba(0.0, 0.0, 0.0, 0.7), |p| page(p, ui, settings, controls));
                }
            }
            // The tooltip line and a message from a key press; always last, always on top.
            root.spawn((Node { position_type: PositionType::Absolute, bottom: px(4), left: px(8), right: px(8), justify_content: JustifyContent::SpaceBetween, ..default() }, Pickable::IGNORE))
                .with_children(|bar| {
                    bound(bar, Bound::Tip, 15.0, TEXT);
                    bound(bar, Bound::Toast, 15.0, WARN);
                });
        });
}

fn overlay(root: &mut Kids, color: Color, content: impl FnOnce(&mut Kids)) {
    root.spawn((
        Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
        BackgroundColor(color),
    ))
    .with_children(|o| {
        o.spawn((Node { padding: UiRect::all(px(18)), align_items: AlignItems::Center, ..column(8.0) }, BackgroundColor(PANEL))).with_children(content);
    });
}

// ---------------------------------------------------------------- menu pages

fn page(p: &mut Kids, ui: &Ui, settings: &Settings, controls: &Controls) {
    match *ui.pages.last().unwrap_or(&Page::Main) {
        Page::Main if ui.mode == Mode::Pause => pause_menu(p, ui),
        Page::Main => main_menu(p, ui),
        Page::Maps => maps(p, ui),
        Page::Settings => settings_page(p, ui, settings),
        Page::Controls => controls_page(p, ui, controls),
        Page::Info => {
            label(p, "People Playground rewrite", 26.0, TEXT);
            for t in INFO {
                label(p, *t, 15.0, TEXT);
            }
            label(p, "A rewrite of People Playground, built from the installed game's own data.", 14.0, DIM);
            button(p, "back", Some(Act::Back), Sel::None, "", px(260));
        }
        Page::Mods => {
            label(p, "mods", 26.0, TEXT);
            label(p, "No mods found", 15.0, TEXT);
            label(p, format!("Loading mods is {NOT_YET}."), 14.0, DIM);
            button(p, "back", Some(Act::Back), Sel::None, "", px(260));
        }
        Page::Stats => {
            label(p, "player statistics", 26.0, TEXT);
            label(p, format!("Player statistics are {NOT_YET}."), 15.0, DIM);
            button(p, "back", Some(Act::Back), Sel::None, "", px(260));
        }
    }
}

fn main_menu(p: &mut Kids, ui: &Ui) {
    label(p, "People Playground", 44.0, TEXT);
    label(p, "rewrite", 18.0, DIM);
    entries(p, MAIN_MENU, ui);
}

fn pause_menu(p: &mut Kids, ui: &Ui) {
    label(p, "paused", 30.0, TEXT);
    entries(p, PAUSE_MENU, ui);
}

/// A MenuController list: the buttons the scene shows, in order, the last one set apart.
fn entries(p: &mut Kids, table: &[Entry], ui: &Ui) {
    let shown: Vec<_> = table.iter().filter(|e| e.1).collect();
    for (n, &&(name, _, page, kind, tip)) in shown.iter().enumerate() {
        if n + 1 == shown.len() && n > 0 {
            p.spawn(Node { height: px(14), ..default() });
        }
        let in_game = ui.mode == Mode::Pause;
        let (act, tip): (Option<Act>, String) = match (name, page, kind) {
            ("resume", ..) => (Some(Act::Resume), tip.into()),
            ("restart", ..) => (ui.map.as_ref().map(|_| Act::Restart), tip.into()),
            (_, "Map Selection", _) => (Some(Act::Nav(Page::Maps)), tip.into()),
            (_, "Settings", _) => (Some(Act::Nav(Page::Settings)), tip.into()),
            (_, "Mods", _) => (Some(Act::Nav(Page::Mods)), tip.into()),
            (_, "Information", _) => (Some(Act::Nav(Page::Info)), tip.into()),
            (_, "Stats", _) => (Some(Act::Nav(Page::Stats)), tip.into()),
            (_, _, "quit") => (Some(Act::Quit), tip.into()),
            ("quit", ..) if in_game => (Some(Act::ToMenu), tip.into()),
            _ => (None, tip.into()),
        };
        match act {
            Some(a) => button(p, name, Some(a), Sel::None, &tip, px(280)),
            None => missing(p, name, &tip, px(280)),
        }
    }
}

fn maps(p: &mut Kids, ui: &Ui) {
    label(p, "map selection", 26.0, TEXT);
    if let Some(e) = &ui.error {
        label(p, format!("Could not load: {e}"), 14.0, WARN);
    }
    if ui.maps.is_empty() {
        label(p, "No maps found", 15.0, DIM);
    }
    scroll_area(p, ui, 0, px(520), px(380), |s| {
        for (i, m) in ui.maps.iter().enumerate() {
            button(s, &m.name, Some(Act::SelectMap(i)), Sel::Map(i), &m.description, percent(100));
        }
    });
    p.spawn(row(10.0)).with_children(|r| {
        match ui.selected.and_then(|i| ui.maps.get(i)) {
            Some(m) => button(r, &format!("Enter {}", m.name), Some(Act::Enter), Sel::None, "", px(300)),
            None => missing(r, "Select a chamber", "", px(300)),
        }
        button(r, "back", Some(Act::Back), Sel::None, "", px(160));
    });
}

fn settings_page(p: &mut Kids, ui: &Ui, settings: &Settings) {
    p.spawn(row(18.0)).with_children(|r| {
        r.spawn(column(6.0)).with_children(|tabs| {
            label(tabs, "settings", 26.0, TEXT);
            for &(name, key) in SETTINGS_MENU {
                match key {
                    "Controls" => button(tabs, name, Some(Act::Nav(Page::Controls)), Sel::None, "", px(180)),
                    "Back" => {
                        tabs.spawn(Node { height: px(14), ..default() });
                        button(tabs, name, Some(Act::Back), Sel::None, "", px(180));
                    }
                    k => {
                        let c = category_index(k);
                        button(tabs, name, Some(Act::Tab(c)), Sel::Tab(c), "", px(180));
                    }
                }
            }
        });
        scroll_area(r, ui, 1, px(720), px(520), |s| {
            for i in tab_rows(ui.tab) {
                setting_row(s, i, settings);
            }
        });
    });
}

fn setting_row(s: &mut Kids, i: usize, settings: &Settings) {
    let def = &SETTINGS[i];
    let live = applies(def.field);
    let mut tip = def.description.to_string();
    if !live {
        tip = format!("{tip}{}({NOT_YET}: nothing in the rewrite reads this setting)", if tip.is_empty() { "" } else { "  " });
    }
    s.spawn((Node { align_items: AlignItems::Center, column_gap: px(10), min_height: px(32), ..row(10.0) }, Hovered::default(), Tip(tip))).with_children(|r| {
        r.spawn(Node { width: px(230), flex_direction: FlexDirection::Column, ..default() }).with_children(|t| {
            label(t, def.title, 15.0, TEXT);
            if !live {
                label(t, format!("({NOT_YET})"), 12.0, DIM);
            }
        });
        r.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: px(4), row_gap: px(4), width: px(440), align_items: AlignItems::Center, ..default() }).with_children(|w| {
            let choice = |w: &mut Kids, text: String, v: f32, off: bool| {
                if off {
                    missing(w, &text, "", px(130));
                } else {
                    button(w, &text, Some(Act::Set(i, v)), Sel::Setting(i, v), "", px(130));
                }
            };
            match def.kind {
                Kind::Bool => {
                    choice(w, "Enabled".into(), 1.0, false);
                    choice(w, "Disabled".into(), 0.0, false);
                }
                Kind::Enum(names) => {
                    for (n, name) in names.iter().enumerate() {
                        choice(w, name.to_string(), n as f32, option_unavailable(def.field, n));
                    }
                }
                Kind::Options(options) => {
                    for &o in options {
                        let mut t = settings.clone();
                        t.values[i] = o;
                        choice(w, t.text(i), o, false);
                    }
                }
                Kind::Int { .. } | Kind::Float { .. } => {
                    w.spawn((
                        Node { width: px(300), height: px(20), ..default() },
                        BackgroundColor(DISABLED),
                        SliderOf(i),
                        Hovered::default(),
                        RelativeCursorPosition::default(),
                    ))
                    .with_children(|track| {
                        track.spawn((
                            Node { width: percent(settings.fraction(i) * 100.0), height: percent(100), ..default() },
                            BackgroundColor(SELECTED),
                            FillOf(i),
                            Pickable::IGNORE,
                        ));
                    });
                    bound(w, Bound::Setting(i), 15.0, TEXT);
                }
            }
        });
    });
}

fn controls_page(p: &mut Kids, ui: &Ui, controls: &Controls) {
    label(p, "controls", 26.0, TEXT);
    scroll_area(p, ui, 2, px(720), px(520), |s| {
        for &cat in ACTION_CATEGORIES {
            let rows: Vec<usize> = ACTIONS.iter().enumerate().filter(|(_, a)| a.category == cat && !a.invisible).map(|(i, _)| i).collect();
            if rows.is_empty() {
                continue;
            }
            label(s, spaced(cat), 20.0, DIM);
            for i in rows {
                let a = ACTIONS[i];
                s.spawn(row(8.0)).with_children(|r| {
                    r.spawn(Node { width: px(360), ..default() }).with_children(|c| label(c, a.name, 15.0, TEXT));
                    r.spawn((
                        Node { width: px(220), padding: UiRect::axes(px(10), px(5)), justify_content: JustifyContent::Center, ..default() },
                        BackgroundColor(BASE),
                        Hovered::default(),
                        Look { sel: Sel::Bind(i), on: true },
                        Press(Act::Rebind(i)),
                        Tip("Click, then press the key (hold another key first for a modifier). Moving the mouse far cancels.".into()),
                    ))
                    .with_children(|b| bound(b, Bound::Bind(i), 15.0, TEXT));
                    if controls.binds[i].0 != "None" {
                        button(r, "X", Some(Act::Unbind(i)), Sel::None, "Unbind", px(34));
                    }
                });
            }
        }
    });
    p.spawn(row(10.0)).with_children(|r| {
        button(r, "reset to defaults", Some(Act::ResetControls), Sel::None, "", px(220));
        button(r, "back", Some(Act::Back), Sel::None, "", px(160));
    });
}

// ---------------------------------------------------------------- the game's HUD

fn hud(root: &mut Kids, ui: &Ui, settings: &Settings, controls: &Controls, stopped: Option<&str>) {
    // Top bar: the toolbar's tabs, then the on screen options.
    root.spawn((Node { width: percent(100), padding: UiRect::all(px(6)), column_gap: px(6), align_items: AlignItems::Center, ..default() }, BackgroundColor(PANEL))).with_children(|bar| {
        if !ui.hide_panels {
            button(bar, "Tools", Some(Act::Powers(false)), Sel::Powers(false), "", px(90));
            button(bar, "Powers", Some(Act::Powers(true)), Sel::Powers(true), "", px(90));
        }
        bar.spawn(Node { flex_grow: 1.0, ..default() });
        bound(bar, Bound::Status, 15.0, TEXT);
        bar.spawn(Node { flex_grow: 1.0, ..default() });
        button(bar, "Slow motion", Some(Act::Slowmo), Sel::Slowmo, &format!("Toggle slow motion ({})", controls.text(Controls::index("slowmo"))), px(120));
        button(bar, "Time", Some(Act::TimeToggle), Sel::Time, &format!("Toggle time ({})", controls.text(Controls::index("time"))), px(80));
        missing(bar, "Limb status", "Toggle detail view", px(110));
        button(bar, "Menu", Some(Act::Escape), Sel::None, "Pause menu", px(80));
    });
    if let Some(m) = stopped {
        root.spawn((Node { align_self: AlignSelf::Center, margin: UiRect::top(px(10)), padding: UiRect::all(px(14)), align_items: AlignItems::Center, ..column(8.0) }, BackgroundColor(Color::srgb(0.3, 0.1, 0.1)))).with_children(|b| {
            label(b, "The game stopped", 20.0, WARN);
            label(b, m, 14.0, TEXT);
            b.spawn(row(8.0)).with_children(|r| {
                match ui.map {
                    Some(_) => button(r, "Reset", Some(Act::Restart), Sel::None, "", px(120)),
                    None => missing(r, "Reset", "", px(120)),
                }
                button(r, "Menu", Some(Act::ToMenu), Sel::None, "", px(120));
            });
        });
    }
    // The middle: tools on the left, map options on the right.
    root.spawn((Node { flex_grow: 1.0, width: percent(100), justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::FlexStart, ..default() }, Pickable::IGNORE)).with_children(|mid| {
        if ui.hide_panels {
            mid.spawn(Node::default());
        } else {
            mid.spawn((Node { padding: UiRect::all(px(6)), width: px(230), ..column(4.0) }, BackgroundColor(PANEL))).with_children(|t| tools(t, ui, controls));
        }
        mid.spawn((Node { padding: UiRect::all(px(6)), width: px(260), ..column(4.0) }, BackgroundColor(PANEL))).with_children(|m| map_options(m, ui, settings));
    });
    if !ui.hide_panels {
        catalog(root, ui);
    }
    if let Some(at) = ui.context {
        root.spawn((Node { position_type: PositionType::Absolute, left: px(at[0]), top: px(at[1]), width: px(200), padding: UiRect::all(px(4)), ..column(2.0) }, BackgroundColor(PANEL)))
            .with_children(|m| {
                for &(name, text, tip, _) in CONTEXT_MENU {
                    if name == "-" {
                        m.spawn((Node { height: px(2), width: percent(100), ..default() }, BackgroundColor(DISABLED)));
                    } else {
                        missing(m, text, tip, percent(100));
                    }
                }
            });
    }
}

fn tools(t: &mut Kids, ui: &Ui, controls: &Controls) {
    let list = if ui.powers { POWERS } else { TOOLS };
    let mut n = 0;
    for &(name, description, _, parent) in list {
        let child = !parent.is_empty();
        if !child {
            n += 1;
        }
        // tool1..tool9 are the parentless tools in order.
        let key = if !child && n <= 9 { format!("[{}] ", controls.text(Controls::index(&format!("tool{n}")))) } else { String::new() };
        let text = format!("{}{key}{name}", if child { "    " } else { "" });
        if name == "Drag" {
            button(t, &text, Some(Act::Powers(ui.powers)), Sel::Tool, description, percent(100));
        } else {
            missing(t, &text, description, percent(100));
        }
    }
}

fn map_options(m: &mut Kids, ui: &Ui, settings: &Settings) {
    label(m, "Map settings", 15.0, DIM);
    button(m, "Settings", Some(Act::Nav(Page::Settings)), Sel::None, "Open the settings", percent(100));
    // Settings opened from here belong to the pause menu.
    button(m, "Environment", Some(Act::Environment), Sel::Env, "", percent(100));
    if ui.env_open {
        label(m, format!("(applying these is {NOT_YET})"), 12.0, WARN);
        for &(name, is_bool, tip, _, default, temperature) in ENVIRONMENT {
            let value = match (is_bool, temperature) {
                (true, _) => if default != 0.0 { "on".to_string() } else { "off".to_string() },
                (false, true) => temperature_text(default, settings.get("TemperatureUnit") as usize),
                (false, false) => format!("{default}"),
            };
            missing(m, &format!("{name}: {value}"), tip, percent(100));
        }
    }
    label(m, "Clear", 15.0, DIM);
    missing(m, "Clear Everything", "Remove everything from the chamber", percent(100));
    missing(m, "Clear Mess", "Remove debris and decals", percent(100));
    missing(m, "Clear Living", "Remove all living things", percent(100));
}

fn catalog(root: &mut Kids, ui: &Ui) {
    root.spawn((Node { width: percent(100), height: px(190), padding: UiRect::all(px(6)), ..column(6.0) }, BackgroundColor(PANEL))).with_children(|c| {
        c.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: px(4), row_gap: px(4), align_items: AlignItems::Center, ..default() }).with_children(|cats| {
            for (i, cat) in ui.catalog.categories.iter().enumerate() {
                button(cats, &cat.name, Some(Act::Category(i)), Sel::Category(i), &cat.description, Val::Auto);
            }
            // The local contraptions category.
            missing(cats, "Contraptions", "Saved contraptions", Val::Auto);
            cats.spawn((
                Node { padding: UiRect::axes(px(10), px(5)), min_width: px(200), ..default() },
                BackgroundColor(BASE),
                Hovered::default(),
                Look { sel: Sel::None, on: true },
                Press(Act::Filter),
                Tip("Click, then type to filter by name or category".into()),
            ))
            .with_children(|f| bound(f, Bound::Filter, 15.0, TEXT));
        });
        scroll_area(c, ui, 3, percent(100), px(120), |items| {
            items
                .spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: px(4), row_gap: px(4), ..default() })
                .with_children(|w| {
                    for item in ui.catalog.shown(ui.category, &ui.filter) {
                        let tip = format!("{}{}{}", item.name, if item.description.is_empty() { "" } else { ": " }, item.description);
                        button(w, &item.name, Some(Act::Item(item.name.clone())), Sel::Item(item.name.clone()), &tip, Val::Auto);
                    }
                });
        });
    });
}
