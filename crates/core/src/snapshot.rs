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
    /// Physical memory Windows manages: capacity minus what the hardware
    /// reserves. 0 when unknown. For diagnosis; the layout does not use it.
    #[serde(default)]
    pub physical_total_bytes: u64,
    /// The kernel's paged pool, all of it: some may be in the page file.
    /// For diagnosis; Staff uses `kernel_paged_resident_bytes`.
    #[serde(default)]
    pub kernel_paged_bytes: u64,
    /// The kernel's nonpaged pool: always in RAM, part of Staff.
    #[serde(default)]
    pub kernel_nonpaged_bytes: u64,
    /// GetPerformanceInfo's SystemCache: the standby list plus the system
    /// working set, so partly Vacant. For diagnosis only.
    #[serde(default)]
    pub system_cache_bytes: u64,

    // From Windows' performance counters (Memory object). 0 when unknown,
    // as under Wine.
    /// The part of the paged pool in RAM (Pool Paged Resident Bytes).
    #[serde(default)]
    pub kernel_paged_resident_bytes: u64,
    /// The file cache in RAM (System Cache Resident Bytes).
    #[serde(default)]
    pub file_cache_bytes: u64,
    /// Drivers' pageable code and data in RAM (System Driver Resident Bytes).
    #[serde(default)]
    pub driver_bytes: u64,
    /// The kernel's pageable code in RAM (System Code Resident Bytes).
    #[serde(default)]
    pub kernel_code_bytes: u64,
    /// Changed pages waiting to be written to disk (Modified Page List
    /// Bytes): in use by nobody, not yet available.
    #[serde(default)]
    pub modified_bytes: u64,
    /// The standby list (the three Standby Cache counters): part of
    /// Available.
    #[serde(default)]
    pub standby_bytes: u64,
    /// Free and zeroed pages (Free & Zero Page List Bytes): the rest of
    /// Available.
    #[serde(default)]
    pub free_bytes: u64,
    /// Memory the graphics adapters use in RAM (GPU Adapter Memory, Shared
    /// Usage, all adapters): the gallery. Not in any process's working
    /// set. 0 when unknown.
    #[serde(default)]
    pub gpu_shared_bytes: u64,
    /// The kernel pool tags holding the most, largest first (`pools`).
    /// Only `--dump` reads them; empty otherwise.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pool_tags: Vec<PoolTag>,
    /// Every pool tag's bytes summed by the one loaded driver whose file
    /// contains it (`pools::by_driver`), largest first. `--dump` only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pool_drivers: Vec<PoolDriver>,
}

/// Kernel pool memory summed over the tags only one loaded driver's file
/// contains, or over tags in several files or none (`pools::SEVERAL`,
/// `pools::NONE`).
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PoolDriver {
    /// The driver's file name, such as `fltmgr.sys`.
    pub driver: String,
    pub paged_bytes: u64,
    pub nonpaged_bytes: u64,
    /// How many tags were summed.
    pub tags: u32,
}

/// Kernel pool memory under one tag: the four characters a driver labels
/// its allocations with. For diagnosis: which driver holds Staff's pools.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PoolTag {
    /// The tag, with bytes that are not printable written as `\xNN`.
    pub tag: String,
    /// Paged pool under this tag, in RAM or in the page file.
    pub paged_bytes: u64,
    /// Nonpaged pool under this tag: always in RAM.
    pub nonpaged_bytes: u64,
    /// Loaded kernel modules whose files contain the tag, in load order,
    /// at most `pools::DRIVERS_KEPT`: the likely owners.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drivers: Vec<String>,
    /// How many loaded modules' files contain the tag.
    #[serde(default)]
    pub found_in: u32,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Process {
    pub pid: u32,
    pub parent_pid: u32,
    /// 0 when unknown.
    pub started_at_ms: u64,
    pub session: u32,
    /// None when the process cannot be opened, and for session 0
    /// processes, which are not opened.
    pub image_path: Option<String>,
    /// The name in the process list, such as `chrome.exe`, `vmmem` or
    /// `MemCompression`: known even when the process cannot be opened.
    #[serde(default)]
    pub image_name: Option<String>,
    /// FileDescription of the image. None when unknown, and for session 0
    /// processes, which are always Staff and never named.
    pub description: Option<String>,
    /// Private working set.
    pub rent_bytes: u64,
    /// Commit charge.
    pub lease_bytes: u64,
    /// The whole working set, shared pages included. 0 when unknown.
    #[serde(default)]
    pub working_set_bytes: u64,
    /// Graphics memory this process holds in RAM (GPU Process Memory,
    /// Shared Usage): its pictures in the gallery. Not part of its working
    /// set; charged to its commit. 0 when unknown.
    #[serde(default)]
    pub gpu_shared_bytes: u64,
    /// Graphics memory on a graphics card's own memory (Dedicated Usage).
    /// Not RAM: for diagnosis only.
    #[serde(default)]
    pub gpu_dedicated_bytes: u64,
    /// Bytes of its working set that other processes map too (a share
    /// count of 2 or more in `QueryWorkingSet`). Only `--dump` measures it,
    /// and only for VM hosts it can read: it shows whether a micro-VM's
    /// shared memory is its template's. None when not measured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ws_shared_with_others_bytes: Option<u64>,
    pub has_visible_window: bool,
    pub is_self: bool,
}

impl Process {
    /// The image's file name in lower case, such as `chrome.exe`: from the
    /// path when the process could be opened, else from the process list.
    pub fn exe(&self) -> Option<String> {
        self.image_path
            .as_deref()
            .map(file_name)
            .filter(|n| !n.is_empty())
            .or_else(|| self.image_name.as_deref().map(str::to_lowercase))
            .filter(|n| !n.is_empty())
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
    fn the_exe_name_comes_from_the_path_or_else_the_list() {
        let mut p = Process {
            image_name: Some("vmmem".into()),
            ..Process::default()
        };
        assert_eq!(p.exe().as_deref(), Some("vmmem"));
        p.image_path = Some(r"C:\X\Chrome.exe".into());
        assert_eq!(p.exe().as_deref(), Some("chrome.exe"));
        assert_eq!(Process::default().exe(), None);
    }

    #[test]
    fn older_dumps_without_the_new_fields_still_read() {
        let old = r#"{"at_ms":1,"capacity_bytes":2,"available_bytes":1,"commit_used_bytes":0,
            "commit_limit_bytes":0,"signed_in_at_ms":0,"foreground_pid":null,"rent_is_estimate":false,
            "processes":[{"pid":4,"parent_pid":0,"started_at_ms":0,"session":0,"image_path":null,
            "description":null,"rent_bytes":0,"lease_bytes":0,"has_visible_window":false,"is_self":false}]}"#;
        let s = Snapshot::from_json(old).unwrap();
        assert_eq!(s.processes[0].image_name, None);
        assert_eq!(s.physical_total_bytes, 0);
    }

    #[test]
    fn file_names_are_lower_case() {
        assert_eq!(file_name(r"C:\Windows\System32\ConHost.exe"), "conhost.exe");
        assert_eq!(file_name("steam.exe"), "steam.exe");
    }
}
