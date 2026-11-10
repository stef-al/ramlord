//! `state.json` on disk (SPEC.md, Settings and state): hues, watched-time
//! counters, peaks and recent rent history, in `%APPDATA%\RAMlord\`.
//! A missing or unreadable file means a fresh start, never a failure.

use std::path::{Path, PathBuf};

use ramlord_core::flags::State;

/// `%APPDATA%\RAMlord\state.json`, when `APPDATA` is set.
pub fn state_path() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(appdata).join("RAMlord").join("state.json"))
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
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let partial = path.with_extension("json.partial");
    std::fs::write(&partial, state.to_json()).map_err(|e| format!("{}: {e}", partial.display()))?;
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
}
