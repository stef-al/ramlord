//! Watched time and the four flags (SPEC.md, Flags), and the state they
//! keep between runs (SPEC.md, Settings and state: `state.json`).
//!
//! Timing rules count watched time only: time when RAMlord was sampling
//! and the tenant was running. A gap of more than 10 seconds between two
//! samples, such as sleep, does not count.
//!
//! | Flag | Rule |
//! | --- | --- |
//! | AWAY | Owns a visible window, holds at least 100 MB, and has gone 4 watched hours without focus |
//! | WALLS | Owned no visible window during the last 10 watched minutes |
//! | SQUAT | Started within 10 minutes of sign-in, never had focus, 30 watched minutes have passed; only when RAMlord itself started within 10 minutes of sign-in |
//! | LEAK | Holds at least 500 MB and grew at least 50% over the last 60 watched minutes, rising in at least 10 of the 12 five-minute buckets; clears when rent falls 20% below its peak |
//!
//! A tenant in the Family list is never flagged.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::building::Flag;
use crate::layout::HueBook;
use crate::snapshot::Snapshot;
use crate::tenants::Grouping;

const MIB: u64 = 1 << 20;
const MINUTE: u64 = 60_000;
const HOUR: u64 = 60 * MINUTE;
const DAY: u64 = 24 * HOUR;

/// A longer gap between samples is not watched time.
pub const GAP_MS: u64 = 10_000;
pub const WALLS_MS: u64 = 10 * MINUTE;
/// The default away threshold; settings can make it 1 hour to 7 days.
pub const AWAY_MS: u64 = 4 * HOUR;
pub const AWAY_MIN_BYTES: u64 = 100 * MIB;
/// "Within 10 minutes of sign-in", for tenants and for RAMlord itself.
pub const BOOT_MS: u64 = 10 * MINUTE;
pub const SQUAT_AFTER_MS: u64 = 30 * MINUTE;
pub const LEAK_MIN_BYTES: u64 = 500 * MIB;
pub const BUCKET_MS: u64 = 5 * MINUTE;
/// The last hour is 12 buckets; the value before the first is kept too.
pub const BUCKETS: usize = 12;
pub const LEAK_GROWTH_PERCENT: u64 = 50;
pub const LEAK_RISING: usize = 10;
pub const LEAK_CLEAR_PERCENT: u64 = 20;
/// The state keeps nothing older than this.
pub const KEEP_MS: u64 = 7 * DAY;
/// At most this many tenants are remembered, running ones first.
pub const MAX_RECORDS: usize = 1_000;

/// What `state.json` holds.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct State {
    pub version: u32,
    /// Wall-clock time of the last sample.
    pub last_at_ms: u64,
    /// RAMlord's own watched time: the clock remarks run on.
    pub watched_ms: u64,
    /// The sign-in that the squatter remark was made for (once per sign-in).
    pub squatters_told: Option<u64>,
    pub tenants: BTreeMap<String, Record>,
}

/// One tenant, running or gone.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct Record {
    /// A tenant keeps its hue for life.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hue: Option<u8>,
    pub last_seen_ms: u64,
    /// The running instance; none once the tenant has gone.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live: Option<Instance>,
}

/// Counters for one run of a tenant. A tenant that restarts starts over.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct Instance {
    pub started_at_ms: u64,
    /// Watched time since it moved in.
    pub watched_ms: u64,
    /// Watched time since it last had focus.
    pub unfocused_ms: u64,
    /// Watched time since it last owned a visible window.
    pub windowless_ms: u64,
    pub ever_focused: bool,
    pub peak_bytes: u64,
    /// Watched time into the current five-minute bucket.
    pub bucket_ms: u64,
    /// Rent at the end of each five-minute bucket, oldest first: up to
    /// `BUCKETS + 1` values, so the first is from an hour ago.
    pub buckets: Vec<u64>,
    /// While LEAK: the highest rent since it was flagged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leak_peak: Option<u64>,
}

impl Default for State {
    fn default() -> State {
        State {
            version: State::VERSION,
            last_at_ms: 0,
            watched_ms: 0,
            squatters_told: None,
            tenants: BTreeMap::new(),
        }
    }
}

impl State {
    pub const VERSION: u32 = 1;

    pub fn from_json(text: &str) -> Result<State, String> {
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|e| format!("not a valid state file: {e}"))?;
        if value.get("version").is_none() {
            return Err("the state file has no version".into());
        }
        let state: State =
            serde_json::from_value(value).map_err(|e| format!("not a valid state file: {e}"))?;
        if state.version != State::VERSION {
            return Err(format!(
                "state version {} is not {}",
                state.version,
                State::VERSION
            ));
        }
        Ok(state)
    }

    /// Compact JSON, to stay well under 1 MB.
    pub fn to_json(&self) -> String {
        let mut state = self.clone();
        state.version = State::VERSION;
        serde_json::to_string(&state).unwrap_or_default()
    }

    /// The saved hues.
    pub fn hues(&self) -> HueBook {
        self.tenants
            .iter()
            .filter_map(|(id, r)| r.hue.map(|h| (id.clone(), h)))
            .collect()
    }

    /// Stores hues given out by the layout.
    pub fn keep_hues(&mut self, hues: &HueBook) {
        for (id, &hue) in hues {
            if let Some(record) = self.tenants.get_mut(id) {
                record.hue = Some(hue);
            }
        }
    }

    /// Forgets tenants not seen for 7 days, then the longest-gone ones
    /// beyond `MAX_RECORDS`.
    pub fn prune(&mut self, now_ms: u64) {
        self.tenants
            .retain(|_, r| r.live.is_some() || r.last_seen_ms + KEEP_MS >= now_ms);
        if self.tenants.len() > MAX_RECORDS {
            let mut gone: Vec<(u64, String)> = self
                .tenants
                .iter()
                .filter(|(_, r)| r.live.is_none())
                .map(|(id, r)| (r.last_seen_ms, id.clone()))
                .collect();
            gone.sort();
            let excess = self.tenants.len() - MAX_RECORDS;
            for (_, id) in gone.into_iter().take(excess) {
                self.tenants.remove(&id);
            }
        }
    }
}

/// A tenant gained a flag in this sample.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gained {
    pub tenant: String,
    pub flag: Flag,
    /// LEAK: rent an hour of watched time ago, and now.
    pub from_bytes: u64,
    pub to_bytes: u64,
    /// AWAY: watched time without focus.
    pub unfocused_ms: u64,
}

/// Count of buckets that rose from the one before.
fn rising(buckets: &[u64]) -> usize {
    buckets.windows(2).filter(|w| w[1] > w[0]).count()
}

/// Keeps watched time and flags from one sample to the next.
pub struct Watch {
    state: State,
    family: BTreeSet<String>,
    away_ms: u64,
    /// Which tenant each pid belonged to in the last sample.
    owners: BTreeMap<u32, String>,
    /// Tenants that had focus since the last sample.
    focused: BTreeSet<String>,
    /// Tenants present in the last sample.
    present: BTreeSet<String>,
    flags: BTreeMap<String, Vec<Flag>>,
    /// When the state was last pruned; pruning runs once a minute.
    pruned_at_ms: u64,
}

impl Watch {
    pub fn new(state: State, family: BTreeSet<String>) -> Watch {
        Watch {
            state,
            family,
            away_ms: AWAY_MS,
            owners: BTreeMap::new(),
            focused: BTreeSet::new(),
            present: BTreeSet::new(),
            flags: BTreeMap::new(),
            pruned_at_ms: 0,
        }
    }

    /// The away threshold, 1 hour to 7 days (a setting from M5).
    pub fn set_away_ms(&mut self, away_ms: u64) {
        self.away_ms = away_ms.clamp(HOUR, 7 * DAY);
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut State {
        &mut self.state
    }

    pub fn flags_of(&self, tenant: &str) -> &[Flag] {
        self.flags.get(tenant).map_or(&[], Vec::as_slice)
    }

    pub fn record(&self, tenant: &str) -> Option<&Record> {
        self.state.tenants.get(tenant)
    }

    /// Credits focus to the tenant that owns `pid`, as of the last sample.
    /// The Windows layer calls this once a second.
    pub fn focus(&mut self, pid: u32) {
        if let Some(tenant) = self.owners.get(&pid) {
            self.focused.insert(tenant.clone());
        }
    }

    /// Takes one sample: advances watched time, updates every tenant's
    /// counters and flags, and returns the flags gained.
    pub fn observe(&mut self, snapshot: &Snapshot, grouping: &Grouping) -> Vec<Gained> {
        let at = snapshot.at_ms;
        let last = self.state.last_at_ms;
        let dt = if last > 0 && at > last && at - last <= GAP_MS {
            at - last
        } else {
            0
        };
        self.state.watched_ms += dt;
        self.state.last_at_ms = at;

        self.owners = grouping
            .tenants
            .iter()
            .flat_map(|t| t.pids.iter().map(move |&pid| (pid, t.id.clone())))
            .collect();
        if let Some(pid) = snapshot.foreground_pid {
            self.focus(pid);
        }

        let sign_in = snapshot.signed_in_at_ms;
        let self_started = snapshot
            .processes
            .iter()
            .find(|p| p.is_self)
            .map_or(0, |p| p.started_at_ms);
        let squatting_applies =
            sign_in > 0 && self_started > 0 && self_started <= sign_in + BOOT_MS;

        let mut gained = Vec::new();
        let mut flags = BTreeMap::new();
        for t in &grouping.tenants {
            let record = self.state.tenants.entry(t.id.clone()).or_default();
            record.last_seen_ms = at;
            let restarted = record
                .live
                .as_ref()
                .is_none_or(|i| i.started_at_ms != t.started_at_ms);
            if restarted {
                record.live = Some(Instance {
                    started_at_ms: t.started_at_ms,
                    ..Instance::default()
                });
            }
            let i = record.live.as_mut().expect("set above");
            // Only time it was running for, seen at both ends, counts.
            let tdt = if !restarted && self.present.contains(&t.id) {
                dt
            } else {
                0
            };
            i.watched_ms += tdt;
            if self.focused.contains(&t.id) {
                i.ever_focused = true;
                i.unfocused_ms = 0;
            } else {
                i.unfocused_ms += tdt;
            }
            if t.has_window {
                i.windowless_ms = 0;
            } else {
                i.windowless_ms += tdt;
            }
            let rent = t.rent_bytes;
            i.peak_bytes = i.peak_bytes.max(rent);
            if i.buckets.is_empty() {
                i.buckets.push(rent);
            }
            i.bucket_ms += tdt;
            while i.bucket_ms >= BUCKET_MS {
                i.bucket_ms -= BUCKET_MS;
                i.buckets.push(rent);
                if i.buckets.len() > BUCKETS + 1 {
                    i.buckets.remove(0);
                }
            }

            if self.family.contains(&t.id) {
                i.leak_peak = None;
                flags.insert(t.id.clone(), vec![Flag::Family]);
                continue;
            }

            let mut leak_from = 0;
            match i.leak_peak {
                Some(peak) => {
                    let peak = peak.max(rent);
                    if u128::from(rent) * 100
                        < u128::from(peak) * u128::from(100 - LEAK_CLEAR_PERCENT)
                    {
                        // Cleared; a new hour of growth is needed to flag it again.
                        i.leak_peak = None;
                        i.buckets = vec![rent];
                        i.bucket_ms = 0;
                    } else {
                        i.leak_peak = Some(peak);
                    }
                }
                None => {
                    let hour = i.buckets.len() == BUCKETS + 1;
                    let start = i.buckets.first().copied().unwrap_or(0);
                    let grew = u128::from(rent) * 100
                        >= u128::from(start) * u128::from(100 + LEAK_GROWTH_PERCENT);
                    if hour && rent >= LEAK_MIN_BYTES && grew && rising(&i.buckets) >= LEAK_RISING {
                        i.leak_peak = Some(rent);
                        leak_from = start;
                    }
                }
            }

            let mut now = Vec::new();
            if i.leak_peak.is_some() {
                now.push(Flag::Leaker);
            }
            let started_at_boot = t.started_at_ms > 0 && t.started_at_ms <= sign_in + BOOT_MS;
            if squatting_applies
                && started_at_boot
                && !i.ever_focused
                && i.watched_ms >= SQUAT_AFTER_MS
            {
                now.push(Flag::Squatter);
            }
            if t.has_window && rent >= AWAY_MIN_BYTES && i.unfocused_ms >= self.away_ms {
                now.push(Flag::Away);
            }
            if i.windowless_ms >= WALLS_MS {
                now.push(Flag::Walls);
            }

            let before = self.flags.get(&t.id).map_or(&[][..], Vec::as_slice);
            for &flag in &now {
                if !before.contains(&flag) {
                    gained.push(Gained {
                        tenant: t.id.clone(),
                        flag,
                        from_bytes: leak_from,
                        to_bytes: rent,
                        unfocused_ms: i.unfocused_ms,
                    });
                }
            }
            flags.insert(t.id.clone(), now);
        }

        let present: BTreeSet<String> = grouping.tenants.iter().map(|t| t.id.clone()).collect();
        for gone in self.present.difference(&present) {
            if let Some(record) = self.state.tenants.get_mut(gone) {
                record.live = None;
            }
        }
        self.present = present;
        self.flags = flags;
        self.focused.clear();
        if at.saturating_sub(self.pruned_at_ms) >= MINUTE || at < self.pruned_at_ms {
            self.state.prune(at);
            self.pruned_at_ms = at;
        }
        gained
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rising_counts_steps_up() {
        assert_eq!(rising(&[1, 2, 2, 3, 1, 4]), 3);
        assert_eq!(rising(&[]), 0);
    }

    #[test]
    fn prune_forgets_after_seven_days_and_caps_gone_tenants() {
        let mut s = State::default();
        let gone = |seen| Record {
            hue: Some(1),
            last_seen_ms: seen,
            live: None,
        };
        s.tenants.insert("old".into(), gone(0));
        s.tenants.insert("recent".into(), gone(KEEP_MS));
        s.tenants.insert(
            "running".into(),
            Record {
                live: Some(Instance::default()),
                ..gone(0)
            },
        );
        s.prune(KEEP_MS + 1);
        let ids: Vec<&str> = s.tenants.keys().map(String::as_str).collect();
        assert_eq!(ids, ["recent", "running"]);

        for k in 0..MAX_RECORDS as u64 + 50 {
            s.tenants.insert(format!("t{k}"), gone(KEEP_MS + k));
        }
        s.prune(KEEP_MS + 1);
        assert_eq!(s.tenants.len(), MAX_RECORDS);
        assert!(s.tenants.contains_key("running"));
        assert!(!s.tenants.contains_key("t0"));
    }

    #[test]
    fn state_round_trips_and_refuses_other_versions() {
        let mut s = State {
            watched_ms: 42,
            ..State::default()
        };
        s.tenants.insert(
            "a".into(),
            Record {
                hue: Some(3),
                last_seen_ms: 7,
                live: Some(Instance {
                    buckets: vec![1, 2],
                    ..Instance::default()
                }),
            },
        );
        let text = s.to_json();
        let back = State::from_json(&text).unwrap();
        assert_eq!(back.watched_ms, 42);
        assert_eq!(back.tenants, s.tenants);
        assert!(State::from_json("{\"version\": 99}").is_err());
        assert!(State::from_json("{}").is_err());
        assert!(State::from_json("not json").is_err());
        assert_eq!(back.hues()["a"], 3);
    }
}
