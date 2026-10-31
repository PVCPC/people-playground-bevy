//! PlayerPrefs (UnityPlayer's Windows storage) and UserPreferenceManager.LoadFromLegacyStorage, which reads it.
//!
//! A value is a registry value under HKEY_CURRENT_USER\Software\<companyName>\<productName>, named `<key>_h<hash>`, where
//! hash is djb2 with each byte XORed in (the native loop sign-extends the byte; keys here are ASCII). Verified against the
//! values stored on this machine. Encodings, from PlayerPrefs::SetString / SetInt / SetFloat in UnityPlayer.dll: a string
//! is its UTF-8 bytes and a NUL (REG_BINARY); an int is REG_DWORD; a float is the double of the float (SetFloat widens it
//! with cvtss2sd and writes 8 bytes).
#![allow(non_snake_case)]

use super::Prefs;

/// ProjectSettings.asset: companyName and productName.
const COMPANY_NAME: &str = "Studio Minus";
const PRODUCT_NAME: &str = "People Playground";

/// The registry name of a PlayerPrefs key: djb2 with each byte XORed in, as Unity hashes it.
pub fn value_name(key: &str) -> String {
    let mut h: u32 = 5381;
    for &b in key.as_bytes() {
        h = h.wrapping_mul(33) ^ (b as i8 as i32 as u32);
    }
    format!("{key}_h{h}")
}

/// A stored value: its registry type (REG_DWORD 4, REG_BINARY 3) and its bytes.
pub trait PlayerPrefsStore {
    fn value(&self, key: &str) -> Option<(u32, Vec<u8>)>;
}

const REG_BINARY: u32 = 3;
const REG_DWORD: u32 = 4;

/// PlayerPrefs.GetString(key, default): REG_BINARY bytes up to the NUL.
pub fn get_string(store: &dyn PlayerPrefsStore, key: &str, default: &str) -> String {
    match store.value(key) {
        None => default.to_owned(),
        Some((REG_BINARY, bytes)) => {
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
            String::from_utf8(bytes[..end].to_vec()).unwrap_or_else(|_| unimplemented!("PlayerPrefs.GetString({key}): not UTF-8"))
        }
        Some((kind, _)) => unimplemented!("PlayerPrefs.GetString({key}): registry type {kind}"),
    }
}

/// PlayerPrefs.GetFloat(key, default): the stored double, as a float.
pub fn get_float(store: &dyn PlayerPrefsStore, key: &str, default: f32) -> f32 {
    match store.value(key) {
        None => default,
        Some((kind, bytes)) if (kind == REG_DWORD || kind == REG_BINARY) && bytes.len() == 8 => {
            f64::from_le_bytes(bytes.try_into().unwrap()) as f32
        }
        Some((kind, bytes)) => unimplemented!("PlayerPrefs.GetFloat({key}): registry type {kind}, {} bytes", bytes.len()),
    }
}

/// UserPreferenceManager.LoadFromLegacyStorage: the preference values the simulation reads, from PlayerPrefs. The fields
/// the simulation does not read (window, bloom, resolution-independent rendering and volume settings) are left out.
pub fn legacy_preferences(store: &dyn PlayerPrefsStore) -> Prefs {
    let mut p = Prefs::default();
    let on = |key: &str, default: &str| get_string(store, key, default) == "on";
    p.DropOnDeath = on("dropOnDeath", "on");
    p.LimbCrushing = on("crushing", "on");
    p.TracerBullets = on("tracers", "off");
    p.FancyEffects = on("extraDecals", "on");
    p.Decals = on("decals", "on");
    p.CrushForceMultiplier = get_float(store, "crushForce", 1.0);
    p.ShakeIntensity = get_float(store, "shakeIntensity", 1.0);
    // CollisionQuality: "continuous" or else Discrete (0).
    p.CollisionQuality = if get_string(store, "collision", "discrete") == "continuous" { 1 } else { 0 };
    // Mathf.RoundToInt(float) rounds half to even.
    p.PhysicsIterations = get_float(store, "physicsIterations", 16.0).round_ties_even() as i32;
    // The resolution is only parsed: a malformed "WxH" makes int.Parse throw in the game.
    let resolution = get_string(store, "resolution", "automatic");
    if resolution != "automatic" {
        let parts: Vec<&str> = resolution.split('x').collect();
        let parse = |s: Option<&&str>| s.and_then(|s| s.trim().parse::<i32>().ok());
        if parse(parts.first()).is_none() || parse(parts.get(1)).is_none() {
            unimplemented!("LoadFromLegacyStorage: resolution {resolution:?} (int.Parse throws)");
        }
    }
    p
}

/// PlayerPrefs' Windows store: HKEY_CURRENT_USER\Software\<company>\<product>, opened read-only. A missing key holds no values.
#[cfg(windows)]
pub struct Registry(Option<windows_sys::Win32::System::Registry::HKEY>);

#[cfg(windows)]
impl Registry {
    pub fn open() -> Registry {
        use windows_sys::Win32::Foundation::ERROR_SUCCESS;
        use windows_sys::Win32::System::Registry::{HKEY, HKEY_CURRENT_USER, KEY_READ, RegOpenKeyExW};
        let path: Vec<u16> = format!("Software\\{COMPANY_NAME}\\{PRODUCT_NAME}").encode_utf16().chain(std::iter::once(0)).collect();
        let mut key: HKEY = std::ptr::null_mut();
        // SAFETY: `path` is a NUL-terminated UTF-16 string and `key` is a valid out pointer.
        let status = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, KEY_READ, &mut key) };
        if status == ERROR_SUCCESS { Registry(Some(key)) } else { Registry(None) }
    }
}

#[cfg(windows)]
impl Drop for Registry {
    fn drop(&mut self) {
        if let Some(key) = self.0 {
            // SAFETY: `key` was opened by RegOpenKeyExW and is closed once.
            unsafe { windows_sys::Win32::System::Registry::RegCloseKey(key) };
        }
    }
}

#[cfg(windows)]
impl PlayerPrefsStore for Registry {
    fn value(&self, key: &str) -> Option<(u32, Vec<u8>)> {
        use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
        use windows_sys::Win32::System::Registry::RegQueryValueExW;
        let hkey = self.0?;
        let name: Vec<u16> = value_name(key).encode_utf16().chain(std::iter::once(0)).collect();
        let mut kind = 0u32;
        let mut size = 0u32;
        // SAFETY: the name is NUL-terminated; a null data pointer asks for the size only.
        let status = unsafe { RegQueryValueExW(hkey, name.as_ptr(), std::ptr::null(), &mut kind, std::ptr::null_mut(), &mut size) };
        if status == ERROR_FILE_NOT_FOUND {
            return None;
        }
        assert_eq!(status, ERROR_SUCCESS, "RegQueryValueExW({key}) size");
        let mut bytes = vec![0u8; size as usize];
        // SAFETY: `bytes` has the size the first query reported.
        let status = unsafe { RegQueryValueExW(hkey, name.as_ptr(), std::ptr::null(), &mut kind, bytes.as_mut_ptr(), &mut size) };
        assert_eq!(status, ERROR_SUCCESS, "RegQueryValueExW({key}) data");
        bytes.truncate(size as usize);
        Some((kind, bytes))
    }
}

/// The game's PlayerPrefs store.
#[cfg(windows)]
pub fn store() -> Registry {
    Registry::open()
}

#[cfg(not(windows))]
pub fn store() -> &'static dyn PlayerPrefsStore {
    unimplemented!("PlayerPrefs is the Windows registry; this port runs on Windows only")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Values set by name, as SetString / SetFloat would store them.
    #[derive(Default)]
    struct Fake(HashMap<String, (u32, Vec<u8>)>);
    impl Fake {
        fn set_string(&mut self, key: &str, s: &str) {
            let mut bytes = s.as_bytes().to_vec();
            bytes.push(0);
            self.0.insert(value_name(key), (REG_BINARY, bytes));
        }
        fn set_float(&mut self, key: &str, f: f32) {
            self.0.insert(value_name(key), (REG_DWORD, (f as f64).to_le_bytes().to_vec()));
        }
    }
    impl PlayerPrefsStore for Fake {
        fn value(&self, key: &str) -> Option<(u32, Vec<u8>)> {
            self.0.get(&value_name(key)).cloned()
        }
    }

    /// The names and hashes read from the game's own registry key on this machine (the 13 stored values).
    #[test]
    fn value_names_hash_like_unity() {
        for (key, hash) in [
            ("Screenmanager Stereo 3D", 1665754519u32),
            ("Screenmanager Resolution Width Default", 680557497),
            ("Screenmanager Resolution Height Default", 1380706816),
            ("Screenmanager Resolution Use Native Default", 1405981789),
            ("Screenmanager Fullscreen mode Default", 401710285),
            ("Screenmanager Resolution Width", 182942802),
            ("Screenmanager Resolution Height", 2627697771),
            ("Screenmanager Fullscreen mode", 3630240806),
            ("Screenmanager Resolution Use Native", 1405027254),
            ("unity.player_session_count", 922449978),
            ("unity.player_sessionid", 1351336811),
            ("unity.cloud_userid", 2665564582),
            ("UnitySelectMonitor", 17969598),
        ] {
            assert_eq!(value_name(key), format!("{key}_h{hash}"));
        }
    }

    #[test]
    fn strings_stop_at_the_nul_and_floats_are_doubles() {
        let mut s = Fake::default();
        s.set_string("collision", "continuous");
        s.set_float("crushForce", 2.5);
        assert_eq!(get_string(&s, "collision", "discrete"), "continuous");
        assert_eq!(get_float(&s, "crushForce", 1.0), 2.5);
        assert_eq!(get_string(&s, "missing", "d"), "d");
        assert_eq!(get_float(&s, "missing", 7.0), 7.0);
    }

    #[test]
    fn legacy_preferences_read_the_stored_values() {
        let mut s = Fake::default();
        s.set_string("collision", "continuous");
        s.set_string("tracers", "on");
        s.set_string("dropOnDeath", "off");
        s.set_float("physicsIterations", 9.5);
        s.set_string("resolution", "1920x1080");
        let p = legacy_preferences(&s);
        assert_eq!((p.CollisionQuality, p.TracerBullets, p.DropOnDeath), (1, true, false));
        // Mathf.RoundToInt(9.5) is 10 (half to even: 9.5 -> 10, 8.5 -> 8).
        assert_eq!(p.PhysicsIterations, 10);
        // Nothing stored: the legacy defaults (physicsIterations 16, collision discrete, tracers off).
        let empty = Fake::default();
        let d = legacy_preferences(&empty);
        assert_eq!((d.CollisionQuality, d.TracerBullets, d.PhysicsIterations, d.LimbCrushing), (0, false, 16, true));
    }
}
