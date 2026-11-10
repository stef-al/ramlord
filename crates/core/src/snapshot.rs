//! The snapshot contract (SPEC.md, Architecture): the boundary between
//! Windows and the core, the `--dump` file format and the fixture format.
//! All times are UTC milliseconds. A snapshot never holds window titles,
//! command lines or user names.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub at_ms: u64,
    pub capacity_bytes: u64,
    pub available_bytes: u64,
    pub commit_used_bytes: u64,
    pub commit_limit_bytes: u64,
    pub signed_in_at_ms: u64,
    pub foreground_pid: Option<u32>,
    /// True when the private working set is unavailable and rent is the
    /// whole working set instead.
    pub rent_is_estimate: bool,
    pub processes: Vec<Process>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Process {
    pub pid: u32,
    pub parent_pid: u32,
    /// 0 when unknown.
    pub started_at_ms: u64,
    pub session: u32,
    /// None when the process cannot be opened.
    pub image_path: Option<String>,
    /// FileDescription of the image.
    pub description: Option<String>,
    /// Private working set.
    pub rent_bytes: u64,
    /// Commit charge.
    pub lease_bytes: u64,
    pub has_visible_window: bool,
    pub is_self: bool,
}

impl Process {
    /// The image's file name in lower case, such as `chrome.exe`.
    pub fn image_name(&self) -> Option<String> {
        self.image_path.as_deref().map(file_name)
    }
}

/// The last component of a Windows or Unix path, in lower case.
pub fn file_name(path: &str) -> String {
    path.rsplit(['\\', '/'])
        .next()
        .unwrap_or(path)
        .to_lowercase()
}

impl Snapshot {
    pub fn from_json(text: &str) -> Result<Snapshot, String> {
        serde_json::from_str(text).map_err(|e| format!("not a valid snapshot file: {e}"))
    }

    pub fn to_json(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).unwrap_or_default();
        text.push('\n');
        text
    }

    /// Replaces the user's profile folder at the start of every image path
    /// with `%USERPROFILE%`, ignoring case, so a dump from a real PC is
    /// safe to commit as a fixture.
    pub fn hide_profile(&mut self, profile: &str) {
        let profile = profile.trim_end_matches(['\\', '/']);
        if profile.is_empty() {
            return;
        }
        for process in &mut self.processes {
            if let Some(path) = &mut process.image_path {
                let head = path.get(..profile.len());
                let rest = path.get(profile.len()..).unwrap_or("");
                let at_boundary = rest.is_empty() || rest.starts_with(['\\', '/']);
                if head.is_some_and(|h| h.eq_ignore_ascii_case(profile)) && at_boundary {
                    *path = format!("%USERPROFILE%{rest}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_path(path: &str) -> Snapshot {
        Snapshot {
            processes: vec![Process {
                pid: 1,
                image_path: Some(path.into()),
                ..Process::default()
            }],
            ..Snapshot::default()
        }
    }

    #[test]
    fn json_round_trip() {
        let s = with_path(r"C:\Program Files\App\app.exe");
        assert_eq!(Snapshot::from_json(&s.to_json()), Ok(s));
        assert!(Snapshot::from_json("{\"at_ms\": 1}").is_err());
    }

    #[test]
    fn the_profile_folder_is_hidden() {
        let mut s = with_path(r"c:\users\ada\AppData\Local\Discord\app-1.0\Discord.exe");
        s.hide_profile(r"C:\Users\Ada\");
        assert_eq!(
            s.processes[0].image_path.as_deref(),
            Some(r"%USERPROFILE%\AppData\Local\Discord\app-1.0\Discord.exe")
        );
    }

    #[test]
    fn only_whole_folder_names_are_hidden() {
        let mut s = with_path(r"C:\Users\Adam\app.exe");
        s.hide_profile(r"C:\Users\Ada");
        assert_eq!(
            s.processes[0].image_path.as_deref(),
            Some(r"C:\Users\Adam\app.exe")
        );
        s.hide_profile("");
        assert_eq!(
            s.processes[0].image_path.as_deref(),
            Some(r"C:\Users\Adam\app.exe")
        );
    }

    #[test]
    fn file_names_are_lower_case() {
        assert_eq!(file_name(r"C:\Windows\System32\ConHost.exe"), "conhost.exe");
        assert_eq!(file_name("steam.exe"), "steam.exe");
    }
}
