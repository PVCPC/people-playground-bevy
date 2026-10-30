//! Unity KeyCode names (what InputSystem's actions are bound to) and the Bevy inputs that stand for them.

use bevy::prelude::{ButtonInput, KeyCode, MouseButton};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Src {
    Key(KeyCode),
    Mouse(MouseButton),
}

/// Unity name -> physical key; the first entry for a key is the name it is shown and saved under.
const KEYS: &[(&str, KeyCode)] = &[
    ("Caret", KeyCode::Backquote),
    ("BackQuote", KeyCode::Backquote),
    ("Space", KeyCode::Space),
    ("Return", KeyCode::Enter),
    ("Escape", KeyCode::Escape),
    ("Tab", KeyCode::Tab),
    ("Backspace", KeyCode::Backspace),
    ("Delete", KeyCode::Delete),
    ("Insert", KeyCode::Insert),
    ("Home", KeyCode::Home),
    ("End", KeyCode::End),
    ("PageUp", KeyCode::PageUp),
    ("PageDown", KeyCode::PageDown),
    ("UpArrow", KeyCode::ArrowUp),
    ("DownArrow", KeyCode::ArrowDown),
    ("LeftArrow", KeyCode::ArrowLeft),
    ("RightArrow", KeyCode::ArrowRight),
    ("LeftShift", KeyCode::ShiftLeft),
    ("RightShift", KeyCode::ShiftRight),
    ("LeftControl", KeyCode::ControlLeft),
    ("RightControl", KeyCode::ControlRight),
    ("LeftAlt", KeyCode::AltLeft),
    ("RightAlt", KeyCode::AltRight),
    ("Plus", KeyCode::Equal),
    ("Equals", KeyCode::Equal),
    ("Minus", KeyCode::Minus),
    ("Comma", KeyCode::Comma),
    ("Period", KeyCode::Period),
    ("Slash", KeyCode::Slash),
    ("Backslash", KeyCode::Backslash),
    ("Semicolon", KeyCode::Semicolon),
    ("Quote", KeyCode::Quote),
    ("LeftBracket", KeyCode::BracketLeft),
    ("RightBracket", KeyCode::BracketRight),
    ("CapsLock", KeyCode::CapsLock),
];

const LETTERS: [KeyCode; 26] = {
    use KeyCode::*;
    [KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN, KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ]
};
const DIGITS: [KeyCode; 10] = {
    use KeyCode::*;
    [Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9]
};
const KEYPAD: [KeyCode; 10] = {
    use KeyCode::*;
    [Numpad0, Numpad1, Numpad2, Numpad3, Numpad4, Numpad5, Numpad6, Numpad7, Numpad8, Numpad9]
};
const FUNCTION: [KeyCode; 12] = {
    use KeyCode::*;
    [F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12]
};

/// The input a Unity KeyCode name stands for; None for "None" and names this keyboard table lacks.
pub fn src(name: &str) -> Option<Src> {
    if let Some(n) = name.strip_prefix("Mouse") {
        return Some(Src::Mouse(match n.parse::<u16>().ok()? {
            0 => MouseButton::Left,
            1 => MouseButton::Right,
            2 => MouseButton::Middle,
            3 => MouseButton::Back,
            4 => MouseButton::Forward,
            n => MouseButton::Other(n),
        }));
    }
    if let Some(n) = name.strip_prefix("Alpha") {
        return DIGITS.get(n.parse::<usize>().ok()?).map(|&k| Src::Key(k));
    }
    if let Some(n) = name.strip_prefix("Keypad") {
        return KEYPAD.get(n.parse::<usize>().ok()?).map(|&k| Src::Key(k));
    }
    if let Some(n) = name.strip_prefix('F').and_then(|n| n.parse::<usize>().ok()) {
        return FUNCTION.get(n.checked_sub(1)?).map(|&k| Src::Key(k));
    }
    if let [c @ b'A'..=b'Z'] = name.as_bytes() {
        return Some(Src::Key(LETTERS[(c - b'A') as usize]));
    }
    KEYS.iter().find(|(n, _)| *n == name).map(|&(_, k)| Src::Key(k))
}

/// The Unity name an input is saved and shown under.
pub fn name(s: Src) -> Option<String> {
    match s {
        Src::Mouse(b) => Some(format!("Mouse{}", match b {
            MouseButton::Left => 0,
            MouseButton::Right => 1,
            MouseButton::Middle => 2,
            MouseButton::Back => 3,
            MouseButton::Forward => 4,
            MouseButton::Other(n) => n,
        })),
        Src::Key(k) => {
            if let Some(i) = LETTERS.iter().position(|&l| l == k) {
                return Some(((b'A' + i as u8) as char).to_string());
            }
            if let Some(i) = DIGITS.iter().position(|&l| l == k) {
                return Some(format!("Alpha{i}"));
            }
            if let Some(i) = KEYPAD.iter().position(|&l| l == k) {
                return Some(format!("Keypad{i}"));
            }
            if let Some(i) = FUNCTION.iter().position(|&l| l == k) {
                return Some(format!("F{}", i + 1));
            }
            KEYS.iter().find(|&&(_, c)| c == k).map(|&(n, _)| n.to_string())
        }
    }
}

/// InputAction.GetDisplayText for one key.
pub fn display(name: &str) -> String {
    match name {
        "Mouse0" => "Left Mouse".into(),
        "Mouse1" => "Right Mouse".into(),
        "Mouse2" => "Middle Mouse".into(),
        n => n.to_string(),
    }
}

pub struct Inputs<'a> {
    pub keys: &'a ButtonInput<KeyCode>,
    pub mouse: &'a ButtonInput<MouseButton>,
}

impl Inputs<'_> {
    pub fn held(&self, s: Src) -> bool {
        match s {
            Src::Key(k) => self.keys.pressed(k),
            Src::Mouse(b) => self.mouse.pressed(b),
        }
    }
    pub fn down(&self, s: Src) -> bool {
        match s {
            Src::Key(k) => self.keys.just_pressed(k),
            Src::Mouse(b) => self.mouse.just_pressed(b),
        }
    }
    pub fn up(&self, s: Src) -> bool {
        match s {
            Src::Key(k) => self.keys.just_released(k),
            Src::Mouse(b) => self.mouse.just_released(b),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_default_binding_has_an_input_and_names_round_trip() {
        for a in super::super::tables::ACTIONS {
            for n in [a.key, a.secondary] {
                if n == "None" {
                    continue;
                }
                let s = src(n).unwrap_or_else(|| panic!("{} binds {n}", a.id));
                assert_eq!(src(&name(s).unwrap()), Some(s), "{n}");
            }
        }
        assert_eq!(name(src("Caret").unwrap()).unwrap(), "Caret");
        assert_eq!(display("Mouse1"), "Right Mouse");
    }
}
