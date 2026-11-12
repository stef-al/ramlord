//! M3: timeline fixtures switch each flag on and off, including a sleep gap
//! that must not count (`fixtures/timelines/`).
//!
//! With `RAMLORD_PNG_DIR` set, the last frame of every timeline is written
//! there as a PNG, to look at (rung 2).

mod timeline;

use std::collections::BTreeSet;

use ramlord_core::flags::{Gained, State, Watch};
use ramlord_core::live::Live;
use ramlord_core::scene::TOWER_PNG_SCALE;
use ramlord_core::tenants::{Lease, group};
use ramlord_core::{Flag, Rules};
use timeline::{Event, expand, load, tenant_id};

fn sorted(mut flags: Vec<Flag>) -> Vec<Flag> {
    flags.sort();
    flags
}

/// Runs a timeline through `Watch`, checking every expectation. Returns
/// every flag gained, in order.
fn run(name: &str, family: &[&str]) -> Vec<Gained> {
    let t = load(name);
    let rules = Rules::builtin();
    let family: BTreeSet<String> = family.iter().map(|n| tenant_id(n)).collect();
    let mut watch = Watch::new(State::default(), family);
    let mut gained = Vec::new();
    let mut checks = 0;
    for event in expand(&t) {
        match event {
            Event::Sample(snapshot) => {
                gained.extend(watch.observe(&snapshot, &group(&snapshot, &rules)));
            }
            Event::Expect { step, flags } => {
                for (tenant, want) in flags {
                    let got = sorted(watch.flags_of(&tenant_id(&tenant)).to_vec());
                    assert_eq!(
                        got,
                        sorted(want),
                        "{name}, step {step}, {tenant}: {}",
                        t.about
                    );
                    checks += 1;
                }
            }
        }
    }
    assert!(checks > 0, "{name} checks nothing");
    gained
}

fn count(gained: &[Gained], tenant: &str, flag: Flag) -> usize {
    gained
        .iter()
        .filter(|g| g.tenant == tenant_id(tenant) && g.flag == flag)
        .count()
}

#[test]
fn walls_switches_on_and_off_and_sleep_does_not_count() {
    let gained = run("walls.json", &[]);
    assert_eq!(count(&gained, "agent", Flag::Walls), 1);
}

#[test]
fn away_switches_on_and_off_and_sleep_does_not_count() {
    let gained = run("away.json", &[]);
    assert_eq!(count(&gained, "viewer", Flag::Away), 1);
    let away = gained.iter().find(|g| g.flag == Flag::Away).unwrap();
    assert!(away.unfocused_ms >= 4 * 3_600_000);
}

#[test]
fn squatter_switches_on_and_off_and_sleep_does_not_count() {
    let gained = run("squatter.json", &[]);
    assert_eq!(count(&gained, "helper", Flag::Squatter), 1);
    assert_eq!(count(&gained, "late", Flag::Squatter), 0);
}

#[test]
fn no_squatters_when_ramlord_started_late() {
    let gained = run("squatter-late-start.json", &[]);
    assert_eq!(count(&gained, "helper", Flag::Squatter), 0);
}

#[test]
fn leaker_switches_on_and_off_and_sleep_does_not_count() {
    let gained = run("leaker.json", &[]);
    assert_eq!(count(&gained, "grower", Flag::Leaker), 1);
    let leak = gained.iter().find(|g| g.flag == Flag::Leaker).unwrap();
    // The remark's numbers: rent an hour of watched time ago, and now.
    assert_eq!(leak.from_bytes, 600 << 20);
    assert!(leak.to_bytes >= 990 << 20);
}

#[test]
fn family_is_never_flagged() {
    let t = load("walls.json");
    let rules = Rules::builtin();
    let family: BTreeSet<String> = [tenant_id("agent")].into();
    let mut watch = Watch::new(State::default(), family);
    let mut gained = Vec::new();
    for event in expand(&t) {
        if let Event::Sample(s) = event {
            gained.extend(watch.observe(&s, &group(&s, &rules)));
            assert_eq!(watch.flags_of(&tenant_id("agent")), [Flag::Family]);
        }
    }
    assert!(gained.iter().all(|g| g.tenant != tenant_id("agent")));
}

/// Writes `frame` as a PNG at the tower's usual scale.
fn write_png(frame: &ramlord_core::Frame, path: &std::path::Path) {
    let frame = frame.scaled(TOWER_PNG_SCALE);
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), frame.width, frame.height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&frame.to_rgb8())
        .unwrap();
}

/// The whole pipeline on a timeline: flags, remark and pixels. With
/// `RAMLORD_PNG_DIR` set, the frame at every checkpoint is written there.
fn live_run(name: &str) -> Live {
    let t = load(name);
    let dir = std::env::var_os("RAMLORD_PNG_DIR");
    let mut live = Live::new(
        Rules::builtin(),
        Lease::placeholder(),
        State::default(),
        BTreeSet::new(),
    );
    for event in expand(&t) {
        match event {
            Event::Sample(s) => {
                live.update(&s).expect("every sample draws");
            }
            Event::Expect { step, .. } => {
                if let Some(dir) = &dir {
                    let file = format!("{}-step{step}.png", name.trim_end_matches(".json"));
                    write_png(live.frame().unwrap(), &std::path::Path::new(dir).join(file));
                }
            }
        }
    }
    live
}

#[test]
fn a_full_building_outranks_the_leak_remark() {
    let live = live_run("full.json");
    let b = live.building().unwrap();
    assert_eq!(b.remark, "We're full. Somebody has to go.");
    let chat = b
        .tenants
        .iter()
        .find(|t| t.id == tenant_id("chat"))
        .unwrap();
    assert_eq!(chat.flags, [Flag::Leaker]);
}

#[test]
fn every_timeline_draws_and_its_remark_fits() {
    for name in [
        "walls.json",
        "away.json",
        "squatter.json",
        "squatter-late-start.json",
        "leaker.json",
        "full.json",
    ] {
        let live = live_run(name);
        let b = live.building().unwrap();
        assert!(!b.remark.is_empty(), "{name} has no remark");
        assert!(b.remark.chars().count() <= 60, "{name}: {}", b.remark);
    }
}

/// Every remark the timeline shows, in order, each once.
fn remarks_seen(name: &str) -> Vec<String> {
    let mut live = Live::new(
        Rules::builtin(),
        Lease::placeholder(),
        State::default(),
        BTreeSet::new(),
    );
    let mut seen: Vec<String> = Vec::new();
    for event in expand(&load(name)) {
        if let Event::Sample(s) = event {
            live.update(&s).unwrap();
            let remark = &live.building().unwrap().remark;
            if seen.last() != Some(remark) {
                seen.push(remark.clone());
            }
        }
    }
    seen
}

#[test]
fn the_squatter_remark_is_made_once() {
    let seen = remarks_seen("squatter.json");
    let told = "1 tenant moved in at boot and you never visited.";
    assert_eq!(seen.iter().filter(|r| *r == told).count(), 1, "{seen:?}");
}

#[test]
fn the_leak_remark_names_the_numbers() {
    let seen = remarks_seen("leaker.json");
    assert!(
        seen.iter()
            .any(|r| r == "GROWER keeps knocking down walls: 0.6 to 1.0 GB in 1 hour."),
        "{seen:?}"
    );
}
