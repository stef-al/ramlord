//! Files in `%APPDATA%\RAMlord\` (SPEC.md, Settings and state):
//! `state.json` holds hues, watched-time counters, peaks and recent rent
//! history; `settings.json` holds the settings; `prices.json` is the last
//! price file fetched. A missing or unreadable file means a fresh start,
//! never a failure.

use std::path::{Path, PathBuf};

use ramlord_core::flags::State;
use ramlord_core::settings::Settings;

fn folder() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(appdata).join("RAMlord"))
}

/// `%APPDATA%\RAMlord\state.json`, when `APPDATA` is set.
pub fn state_path() -> Option<PathBuf> {
    Some(folder()?.join("state.json"))
}

/// `%APPDATA%\RAMlord\settings.json`, when `APPDATA` is set.
pub fn settings_path() -> Option<PathBuf> {
    Some(folder()?.join("settings.json"))
}

/// The settings, or the defaults if there are none or they cannot be
/// read. Keys this version does not know are kept for the next save.
pub fn load_settings(path: &Path) -> Settings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Saves the settings, replacing the file at once.
pub fn save_settings(path: &Path, settings: &Settings) -> Result<(), String> {
    let text = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    write(path, &text)
}

/// `%APPDATA%\RAMlord\prices.json`: the last price file fetched.
pub fn prices_path() -> Option<PathBuf> {
    Some(folder()?.join("prices.json"))
}

/// Writes `text` to `path` the same safe way.
pub fn write_text(path: &Path, text: &str) -> Result<(), String> {
    write(path, text)
}

/// The saved state, or a fresh one if there is none or it cannot be read.
pub fn load(path: &Path) -> State {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| State::from_json(&text).ok())
        .unwrap_or_default()
}

/// Writes the state next to its final place and renames it over the old
/// file, so a crash mid-write never leaves half a file.
pub fn save(path: &Path, state: &State) -> Result<(), String> {
    write(path, &state.to_json())
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let partial = path.with_extension("json.partial");
    std::fs::write(&partial, text).map_err(|e| format!("{}: {e}", partial.display()))?;
    std::fs::rename(&partial, path).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ramlord-store-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn saves_and_loads_through_a_new_folder() {
        let dir = scratch("roundtrip");
        let path = dir.join("RAMlord").join("state.json");
        let state = State {
            watched_ms: 1234,
            ..State::default()
        };
        save(&path, &state).unwrap();
        assert_eq!(load(&path), state);
        // Saving again replaces the file.
        let later = State {
            watched_ms: 5678,
            ..State::default()
        };
        save(&path, &later).unwrap();
        assert_eq!(load(&path).watched_ms, 5678);
        assert!(!path.with_extension("json.partial").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_missing_or_broken_file_is_a_fresh_start() {
        let dir = scratch("broken");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(load(&dir.join("absent.json")), State::default());
        let broken = dir.join("state.json");
        std::fs::write(&broken, "{ not json").unwrap();
        assert_eq!(load(&broken), State::default());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn settings_are_saved_and_keep_keys_they_do_not_know() {
        let dir = scratch("settings");
        let path = dir.join("RAMlord").join("settings.json");
        assert_eq!(load_settings(&path), Settings::default());
        // A file from M4, with only the Family list, and a key from later.
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"family": ["old.exe"], "later": true}"#).unwrap();
        let mut settings = load_settings(&path);
        assert_eq!(settings.family.len(), 1);
        settings.always_on_top = true;
        save_settings(&path, &settings).unwrap();
        let saved: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved["later"], serde_json::Value::Bool(true));
        assert_eq!(load_settings(&path), settings);
        // A broken file reads as the defaults.
        std::fs::write(&path, "[1, 2").unwrap();
        assert_eq!(load_settings(&path), Settings::default());
        std::fs::remove_dir_all(&dir).ok();
    }
}
