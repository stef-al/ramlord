//! Files in `%APPDATA%\RAMlord\` (SPEC.md, Settings and state):
//! `state.json` holds hues, watched-time counters, peaks and recent rent
//! history; `settings.json` holds the settings, of which M4 uses only the
//! Family list. A missing or unreadable file means a fresh start, never a
//! failure.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use ramlord_core::flags::State;
use serde_json::Value;

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

fn settings(path: &Path) -> serde_json::Map<String, Value> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|v| match v {
            Value::Object(map) => Some(map),
            _ => None,
        })
        .unwrap_or_default()
}

/// The Family list: tenant ids the user marked. Empty if there is none.
pub fn load_family(path: &Path) -> BTreeSet<String> {
    match settings(path).get("family") {
        Some(Value::Array(ids)) => ids
            .iter()
            .filter_map(|id| id.as_str().map(str::to_owned))
            .collect(),
        _ => BTreeSet::new(),
    }
}

/// Saves the Family list, keeping every other setting in the file.
pub fn save_family(path: &Path, family: &BTreeSet<String>) -> Result<(), String> {
    let mut map = settings(path);
    map.insert(
        "family".into(),
        Value::Array(family.iter().cloned().map(Value::String).collect()),
    );
    let text = serde_json::to_string_pretty(&Value::Object(map)).map_err(|e| e.to_string())?;
    write(path, &text)
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
    fn family_is_saved_beside_other_settings() {
        let dir = scratch("family");
        let path = dir.join("RAMlord").join("settings.json");
        assert!(load_family(&path).is_empty());
        // A setting written by a later version stays put.
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"always_on_top": true, "family": ["old.exe"]}"#).unwrap();
        assert_eq!(load_family(&path), BTreeSet::from(["old.exe".to_owned()]));
        let family = BTreeSet::from([r"C:\Apps\Notes\notes.exe".to_owned(), "zoom.exe".to_owned()]);
        save_family(&path, &family).unwrap();
        assert_eq!(load_family(&path), family);
        let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved["always_on_top"], Value::Bool(true));
        // A broken file reads as no Family.
        std::fs::write(&path, "[1, 2").unwrap();
        assert!(load_family(&path).is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }
}
