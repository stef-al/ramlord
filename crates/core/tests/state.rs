//! M3: `state.json` survives a restart and stays under 1 MB after a
//! simulated 7 days, keeping nothing older than 7 days.

mod timeline;

use std::collections::BTreeSet;

use ramlord_core::flags::{KEEP_MS, MAX_RECORDS, State, Watch};
use ramlord_core::snapshot::{Process, Snapshot};
use ramlord_core::tenants::group;
use ramlord_core::{Flag, Rules};
use timeline::{Event, MIB, SIGN_IN_MS, expand, load, tenant_id};

const ONE_MB: usize = 1 << 20;

/// Runs a timeline; after the steps listed in `restart_after`, saves the
/// state as JSON, drops everything and starts again from the file.
fn run(name: &str, restart_after: &[usize]) -> (Watch, Vec<Vec<Flag>>) {
    let rules = Rules::builtin();
    let mut watch = Watch::new(State::default(), BTreeSet::new());
    let mut seen = Vec::new();
    for event in expand(&load(name)) {
        match event {
            Event::Sample(s) => {
                watch.observe(&s, &group(&s, &rules));
            }
            Event::Expect { step, flags } => {
                for tenant in flags.keys() {
                    seen.push(watch.flags_of(&tenant_id(tenant)).to_vec());
                }
                if restart_after.contains(&step) {
                    let text = watch.state().to_json();
                    watch = Watch::new(
                        State::from_json(&text).expect("the file reads back"),
                        BTreeSet::new(),
                    );
                }
            }
        }
    }
    (watch, seen)
}

#[test]
fn state_survives_a_restart() {
    // Restart during each sleep: the flags and counters come out the same
    // as in a run without restarts.
    for (name, steps) in [
        ("away.json", &[1][..]),
        ("leaker.json", &[1][..]),
        ("walls.json", &[1][..]),
        ("squatter.json", &[1][..]),
    ] {
        let (straight, flags_straight) = run(name, &[]);
        let (restarted, flags_restarted) = run(name, steps);
        assert_eq!(flags_restarted, flags_straight, "{name}");
        assert_eq!(restarted.state(), straight.state(), "{name}");
    }
}

#[test]
fn hues_survive_a_restart() {
    let mut state = State::default();
    let rules = Rules::builtin();
    let t = load("away.json");
    let mut live = ramlord_core::live::Live::new(
        rules,
        ramlord_core::tenants::Lease::placeholder(),
        state.clone(),
        BTreeSet::new(),
    );
    for event in expand(&t).into_iter().take(10) {
        if let Event::Sample(s) = event {
            live.update(&s).unwrap();
        }
    }
    state = State::from_json(&live.state().to_json()).unwrap();
    let hues = state.hues();
    assert_eq!(hues.len(), 3, "{hues:?}");
    assert_eq!(hues[&tenant_id("editor")], 0);
}

/// One process per tenant, under explorer.
fn process(pid: u32, name: &str, started_at_ms: u64, mib: u64, window: bool) -> Process {
    Process {
        pid,
        parent_pid: 100,
        started_at_ms,
        session: 1,
        image_path: Some(format!(r"C:\Apps\{name}.exe")),
        description: Some(name.into()),
        rent_bytes: mib * MIB,
        lease_bytes: mib * MIB,
        has_visible_window: window,
        is_self: false,
        ..Default::default()
    }
}

#[test]
fn state_stays_under_1_mb_and_forgets_after_7_days() {
    // Eight days, a sample every 10 seconds: 30 apps that stay, and a new
    // app every 5 minutes that stays 20 minutes (2,304 of them).
    const STEP_MS: u64 = 10_000;
    const SAMPLES: u64 = 8 * 24 * 360;
    let rules = Rules::builtin();
    let mut watch = Watch::new(State::default(), BTreeSet::new());
    let mut largest = 0;
    let explorer = Process {
        pid: 100,
        parent_pid: 4,
        started_at_ms: SIGN_IN_MS,
        session: 1,
        image_path: Some(r"C:\Windows\explorer.exe".into()),
        ..Process::default()
    };
    let mut at = SIGN_IN_MS;
    for k in 0..SAMPLES {
        at = SIGN_IN_MS + k * STEP_MS;
        let mut processes = vec![explorer.clone()];
        for s in 0..30u32 {
            let mib = 100 + u64::from(s) * 10 + k % 7;
            processes.push(process(
                1000 + s,
                &format!("steady{s}"),
                SIGN_IN_MS,
                mib,
                s % 3 != 0,
            ));
        }
        let wave = k / 30; // a new one every 5 minutes
        for c in wave.saturating_sub(3)..=wave {
            let started = SIGN_IN_MS + c * 30 * STEP_MS;
            processes.push(process(
                5000 + c as u32,
                &format!("visitor{c}"),
                started,
                50 + c % 400,
                true,
            ));
        }
        let snapshot = Snapshot {
            at_ms: at,
            capacity_bytes: 16_384 * MIB,
            available_bytes: 8_000 * MIB,
            signed_in_at_ms: SIGN_IN_MS,
            foreground_pid: Some(1000 + (k / 90 % 30) as u32),
            processes,
            ..Snapshot::default()
        };
        watch.observe(&snapshot, &group(&snapshot, &rules));
        if k % 2_160 == 0 {
            largest = largest.max(watch.state().to_json().len());
        }
    }
    let text = watch.state().to_json();
    largest = largest.max(text.len());
    assert!(largest < ONE_MB, "state.json reached {largest} bytes");

    let state = State::from_json(&text).unwrap();
    assert!(state.tenants.len() <= MAX_RECORDS);
    assert!(
        state
            .tenants
            .values()
            .all(|r| r.last_seen_ms + KEEP_MS >= at),
        "a tenant older than 7 days is still there"
    );
    // The first visitors are long gone; the steady apps remain, with hues.
    assert!(!state.tenants.contains_key(&tenant_id("visitor0")));
    assert!(state.tenants[&tenant_id("steady0")].live.is_some());
    eprintln!(
        "after 8 simulated days: {} tenants, {} bytes (largest {largest})",
        state.tenants.len(),
        text.len()
    );
}
