//! Timeline fixtures (`fixtures/timelines/*.json`): a compact way to write
//! hours of snapshots by hand. A timeline is a list of steps; each step
//! samples every few seconds for a while with the tenants it lists, or
//! sleeps (no samples at all, as when the PC sleeps). After a step, the
//! flags each tenant should carry can be checked.
//!
//! ```json
//! { "about": "...", "capacity_mib": 16384, "available_mib": 8192,
//!   "ramlord_started_s": 60, "start_s": 120,
//!   "steps": [
//!     { "for_s": 600, "every_s": 2, "focus": "editor",
//!       "tenants": { "agent": { "mib": [300, 400], "window": false, "started_s": 30 } },
//!       "expect": { "agent": ["walls"] } },
//!     { "sleep_s": 7200 } ] }
//! ```
//!
//! Times are seconds after sign-in. `mib` is a number or `[from, to]`,
//! moving in a straight line over the step. Each tenant is one process,
//! `C:\Apps\NAME.exe`, started under explorer.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::Path;

use ramlord_core::Flag;
use ramlord_core::snapshot::{Process, Snapshot};
use serde::Deserialize;

pub const MIB: u64 = 1 << 20;
/// Sign-in: 10 November 2026, 08:00 UTC.
pub const SIGN_IN_MS: u64 = 1_794_297_600_000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Timeline {
    pub about: String,
    pub capacity_mib: u64,
    pub available_mib: u64,
    #[serde(default)]
    pub commit_percent: u64,
    pub ramlord_started_s: u64,
    pub start_s: u64,
    pub steps: Vec<Step>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    #[serde(default)]
    pub for_s: u64,
    #[serde(default = "two")]
    pub every_s: u64,
    #[serde(default)]
    pub sleep_s: u64,
    #[serde(default)]
    pub focus: Option<String>,
    #[serde(default)]
    pub tenants: BTreeMap<String, TenantSpec>,
    #[serde(default)]
    pub expect: BTreeMap<String, Vec<Flag>>,
}

fn two() -> u64 {
    2
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TenantSpec {
    pub mib: Mib,
    #[serde(default = "yes")]
    pub window: bool,
    #[serde(default)]
    pub started_s: u64,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Deserialize)]
#[serde(untagged)]
pub enum Mib {
    Flat(u64),
    Line([u64; 2]),
}

impl Mib {
    /// Rent at sample `k` of `n` in the step.
    fn at(&self, k: u64, n: u64) -> u64 {
        match *self {
            Mib::Flat(m) => m * MIB,
            Mib::Line([from, to]) => {
                let from = from * MIB;
                let to = to * MIB;
                if n == 0 {
                    return to;
                }
                if to >= from {
                    from + (to - from) * k / n
                } else {
                    from - (from - to) * k / n
                }
            }
        }
    }
}

/// What a timeline produces, in order.
pub enum Event {
    Sample(Snapshot),
    /// After step `step` (counting from 1): these tenants carry exactly
    /// these flags.
    Expect {
        step: usize,
        flags: BTreeMap<String, Vec<Flag>>,
    },
}

pub fn tenant_id(name: &str) -> String {
    format!(r"c:\apps\{}.exe", name.to_lowercase())
}

pub fn load(name: &str) -> Timeline {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/timelines")
        .join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Expands a timeline into snapshots and checks.
pub fn expand(t: &Timeline) -> Vec<Event> {
    let mut pids: BTreeMap<String, u32> = BTreeMap::new();
    let mut events = Vec::new();
    let mut now_s = t.start_s;
    let mut first = true;
    let snapshot = |at_s: u64, step: &Step, k: u64, n: u64, pids: &BTreeMap<String, u32>| {
        let mut processes = vec![
            Process {
                pid: 100,
                parent_pid: 4,
                started_at_ms: SIGN_IN_MS,
                session: 1,
                image_path: Some(r"C:\Windows\explorer.exe".into()),
                description: None,
                rent_bytes: 100 * MIB,
                lease_bytes: 100 * MIB,
                has_visible_window: true,
                is_self: false,
            },
            Process {
                pid: 110,
                parent_pid: 100,
                started_at_ms: SIGN_IN_MS + t.ramlord_started_s * 1000,
                session: 1,
                image_path: Some(r"C:\Tools\ramlord.exe".into()),
                description: None,
                rent_bytes: 20 * MIB,
                lease_bytes: 20 * MIB,
                has_visible_window: true,
                is_self: true,
            },
        ];
        for (name, spec) in &step.tenants {
            let pid = pids[name];
            let rent = spec.mib.at(k, n);
            processes.push(Process {
                pid,
                parent_pid: 100,
                started_at_ms: SIGN_IN_MS + spec.started_s * 1000,
                session: 1,
                image_path: Some(format!(r"C:\Apps\{name}.exe")),
                description: Some(name.clone()),
                rent_bytes: rent,
                lease_bytes: rent,
                has_visible_window: spec.window,
                is_self: false,
            });
        }
        let capacity = t.capacity_mib * MIB;
        Snapshot {
            at_ms: SIGN_IN_MS + at_s * 1000,
            capacity_bytes: capacity,
            available_bytes: t.available_mib * MIB,
            commit_used_bytes: capacity / 100 * t.commit_percent,
            commit_limit_bytes: capacity,
            signed_in_at_ms: SIGN_IN_MS,
            foreground_pid: step.focus.as_ref().and_then(|f| pids.get(f).copied()),
            rent_is_estimate: false,
            processes,
        }
    };
    for (index, step) in t.steps.iter().enumerate() {
        if step.sleep_s > 0 {
            now_s += step.sleep_s;
        } else {
            // A tenant keeps its pid from its first step on.
            for name in step.tenants.keys() {
                let next = 1000 + pids.len() as u32;
                pids.entry(name.clone()).or_insert(next);
            }
            let n = step.for_s / step.every_s;
            if first {
                events.push(Event::Sample(snapshot(now_s, step, 0, n, &pids)));
                first = false;
            }
            for k in 1..=n {
                events.push(Event::Sample(snapshot(
                    now_s + k * step.every_s,
                    step,
                    k,
                    n,
                    &pids,
                )));
            }
            now_s += n * step.every_s;
        }
        if !step.expect.is_empty() {
            events.push(Event::Expect {
                step: index + 1,
                flags: step.expect.clone(),
            });
        }
    }
    events
}
