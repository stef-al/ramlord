//! M4: clicks and keys map to the right tenant and action.
//!
//! With `RAMLORD_PNG_DIR` set, the details card and the dialogs are written
//! there as PNGs, to look at (rung 2).

use std::collections::BTreeSet;
use std::io::Cursor;
use std::path::Path;

use ramlord_core::flags::State;
use ramlord_core::layout::Layout;
use ramlord_core::live::Live;
use ramlord_core::scene::{self, Details, Panel, TOWER_PNG_SCALE};
use ramlord_core::tenants::Lease;
use ramlord_core::ui::{Command, Eviction, Input, Key, Target, View, hit};
use ramlord_core::{Building, Frame, HueBook, Rules, Snapshot};

fn repo() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn sample() -> (Building, Layout) {
    let text = std::fs::read_to_string(repo().join("fixtures/sample-building.json")).unwrap();
    let b = Building::from_json(&text).unwrap();
    let (layout, _) = Layout::new(&b, &HueBook::new());
    (b, layout)
}

fn desk() -> Snapshot {
    let text = std::fs::read_to_string(repo().join("fixtures/snapshot-desk.json")).unwrap();
    Snapshot::from_json(&text).unwrap()
}

/// The middle of a unit: floor 0 is the ground floor.
fn unit(floor: i32, col: i32) -> (i32, i32) {
    (16 + col * 12 + 5, 30 + (15 - floor) * 11 + 5)
}

fn save(frame: &Frame, name: &str) {
    let Some(dir) = std::env::var_os("RAMLORD_PNG_DIR") else {
        return;
    };
    let frame = frame.scaled(TOWER_PNG_SCALE);
    let file = std::fs::File::create(Path::new(&dir).join(name)).unwrap();
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), frame.width, frame.height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&frame.to_rgb8())
        .unwrap();
}

#[test]
fn clicks_on_units_rows_and_hints() {
    let (b, layout) = sample();
    let (_, regions) = scene::window(&b, &layout, None, None, &Panel::Roll);
    let at = |(x, y): (i32, i32)| hit(&regions, x, y);
    // Chrome starts at floor 2, column 12; Discord's label is on floor 6.
    assert_eq!(at(unit(2, 12)), Some(Target::Tenant(0)));
    assert_eq!(at(unit(6, 0)), Some(Target::Tenant(1)));
    // Staff and Vacant units select nothing.
    assert_eq!(at(unit(0, 0)), None);
    assert_eq!(at(unit(15, 0)), None);
    // Rent roll rows: Chrome, Discord, VS Code...
    assert_eq!(at((300, 54)), Some(Target::Tenant(0)));
    assert_eq!(at((300, 64)), Some(Target::Tenant(1)));
    assert_eq!(at((300, 144)), Some(Target::Tenant(9))); // OneDrive, the tenth
    // The two hints.
    assert_eq!(at((240, 222)), Some(Target::RentRollCard));
    assert_eq!(at((350, 222)), Some(Target::Settings));
}

#[test]
fn the_details_card_offers_the_right_actions() {
    let (b, layout) = sample();
    let card = |tenant: usize, family: bool| {
        let details = Details {
            tenant,
            family,
            ..Details::default()
        };
        let (draws, regions) =
            scene::window(&b, &layout, None, Some(tenant), &Panel::Details(&details));
        let targets: Vec<Target> = regions
            .iter()
            .map(|r| r.target)
            .filter(|t| {
                !matches!(
                    t,
                    Target::Tenant(_) | Target::RentRollCard | Target::Settings
                )
            })
            .collect();
        (draws, targets)
    };
    let (_, chrome) = card(0, false);
    assert_eq!(chrome, [Target::Evict, Target::Family, Target::Back]);
    // Copilot squats: it also gets Open Startup settings.
    let (_, copilot) = card(7, false);
    assert_eq!(
        copilot,
        [Target::Evict, Target::Family, Target::Startup, Target::Back]
    );
    // Family cannot be evicted.
    let (_, family) = card(0, true);
    assert_eq!(family, [Target::Family, Target::Back]);
}

#[test]
fn dialogs_offer_their_buttons() {
    let (b, layout) = sample();
    let buttons = |eviction: &Eviction| -> Vec<Target> {
        let (_, regions) = scene::window(&b, &layout, None, Some(0), &Panel::Dialog(eviction));
        regions
            .iter()
            .map(|r| r.target)
            .filter(|t| {
                !matches!(
                    t,
                    Target::Tenant(_) | Target::RentRollCard | Target::Settings
                )
            })
            .collect()
    };
    let confirm = Eviction::Confirm {
        tenant: "chrome.exe".into(),
        name: "CHROME".into(),
        before: 1,
    };
    assert_eq!(buttons(&confirm), [Target::CloseIt, Target::Cancel]);
    let refused = Eviction::Refused {
        tenant: "chrome.exe".into(),
        name: "CHROME".into(),
        pids: vec![1],
        before: 1,
        no_window: false,
    };
    assert_eq!(buttons(&refused), [Target::Leave, Target::Force]);
    assert_eq!(
        buttons(&Eviction::Lawyer {
            name: "CHROME".into()
        }),
        [Target::Ok]
    );
}

fn live() -> Live {
    Live::new(
        Rules::builtin(),
        Lease::placeholder(),
        State::default(),
        BTreeSet::new(),
    )
}

#[test]
fn keys_select_open_and_evict_through_the_whole_pipeline() {
    let mut live = live();
    let snapshot = desk();
    live.update(&snapshot).unwrap();
    let now = snapshot.at_ms;
    // Down selects the first row (Chrome), Enter opens its card.
    live.input(Input::Key(Key::Down), now);
    let chrome = live
        .building()
        .unwrap()
        .tenants
        .iter()
        .position(|t| t.name == "Chrome")
        .unwrap();
    assert_eq!(
        live.ui().selected.as_deref(),
        Some(live.building().unwrap().tenants[chrome].id.as_str())
    );
    live.input(Input::Key(Key::Enter), now);
    assert_eq!(live.ui().view, View::Details);
    save(live.frame().unwrap(), "details-chrome.png");
    // Delete asks first; Enter closes it politely with all 42 processes.
    assert!(live.input(Input::Key(Key::Delete), now).is_empty());
    save(live.frame().unwrap(), "dialog-confirm.png");
    let commands = live.input(Input::Key(Key::Enter), now);
    let Command::Close { pids } = &commands[0] else {
        panic!("{commands:?}");
    };
    assert_eq!(pids.len(), 42);
    assert!(live.waiting());
    // Some stay past 5 seconds: it refuses.
    live.poll(&[3000], now + 5_000);
    assert!(!live.waiting());
    save(live.frame().unwrap(), "dialog-refused.png");
    assert_eq!(
        live.input(Input::Key(Key::Delete), now + 6_000),
        [Command::Terminate { pids: vec![3000] }]
    );
    live.poll(&[], now + 6_500);
    // The next sample, without Chrome, reports what was vacated.
    let mut after = snapshot.clone();
    after.at_ms += 2_000;
    let chrome = |p: &ramlord_core::snapshot::Process| {
        p.image_path
            .as_deref()
            .is_some_and(|i| i.contains("Chrome"))
    };
    // Its memory comes free, as when a real process exits.
    let freed: u64 = after
        .processes
        .iter()
        .filter(|p| chrome(p))
        .map(|p| p.rent_bytes)
        .sum();
    after.available_bytes += freed;
    after.commit_used_bytes -= after
        .processes
        .iter()
        .filter(|p| chrome(p))
        .map(|p| p.lease_bytes)
        .sum::<u64>();
    after.processes.retain(|p| !chrome(p));
    live.update(&after).unwrap();
    // The dialog closes, the rent roll is back, and the landlord says it.
    assert_eq!(live.ui().eviction, Eviction::Idle);
    assert_eq!(live.ui().view, View::Roll);
    let remark = &live.building().unwrap().remark;
    assert!(
        remark.starts_with("Evicted CHROME. ") && remark.ends_with(" GB vacated."),
        "{remark}"
    );
    save(live.frame().unwrap(), "evicted.png");
}

#[test]
fn clicks_select_through_the_whole_pipeline() {
    let mut live = live();
    let snapshot = desk();
    live.update(&snapshot).unwrap();
    let now = snapshot.at_ms;
    // A click on the first rent roll row opens that tenant's card.
    live.input(Input::Click { x: 300, y: 54 }, now);
    assert_eq!(live.ui().view, View::Details);
    let evict = live
        .regions()
        .iter()
        .find(|r| r.target == Target::Evict)
        .copied()
        .unwrap();
    live.input(
        Input::Click {
            x: evict.x0 + 2,
            y: evict.y0 + 2,
        },
        now,
    );
    assert!(matches!(live.ui().eviction, Eviction::Confirm { .. }));
    let cancel = live
        .regions()
        .iter()
        .find(|r| r.target == Target::Cancel)
        .copied()
        .unwrap();
    live.input(
        Input::Click {
            x: cancel.x0 + 2,
            y: cancel.y0 + 2,
        },
        now,
    );
    assert_eq!(live.ui().eviction, Eviction::Idle);
    // Back, then F2 on the hint.
    let back = live
        .regions()
        .iter()
        .find(|r| r.target == Target::Back)
        .copied()
        .unwrap();
    live.input(
        Input::Click {
            x: back.x0 + 2,
            y: back.y0 + 2,
        },
        now,
    );
    assert_eq!(live.ui().view, View::Roll);
    assert_eq!(
        live.input(Input::Click { x: 240, y: 222 }, now),
        [Command::RentRollCard]
    );
}

#[test]
fn family_is_saved_and_cannot_be_evicted() {
    let mut live = live();
    let snapshot = desk();
    live.update(&snapshot).unwrap();
    let now = snapshot.at_ms;
    live.input(Input::Key(Key::Down), now);
    live.input(Input::Key(Key::Enter), now);
    let family = live
        .regions()
        .iter()
        .find(|r| r.target == Target::Family)
        .copied()
        .unwrap();
    let commands = live.input(
        Input::Click {
            x: family.x0 + 2,
            y: family.y0 + 2,
        },
        now,
    );
    assert!(matches!(&commands[..], [Command::SaveFamily(f)] if f.len() == 1));
    assert!(live.input(Input::Key(Key::Delete), now).is_empty());
    assert_eq!(live.ui().eviction, Eviction::Idle);
    // From the next sample on, the tenant carries the Family flag.
    let mut later = snapshot.clone();
    later.at_ms += 2_000;
    live.update(&later).unwrap();
    let id = live.ui().selected.clone().unwrap();
    let tenant = live
        .building()
        .unwrap()
        .tenants
        .iter()
        .find(|t| t.id == id)
        .unwrap();
    assert_eq!(tenant.flags, [ramlord_core::Flag::Family]);
}

/// The details card of Discord in the sample building, pixel for pixel.
#[test]
fn details_card_matches_golden() {
    let (b, layout) = sample();
    let details = Details {
        tenant: 1,
        peak_bytes: 3_100 << 20,
        hour_ago_bytes: Some(900 << 20),
        moved_in_ms: Some(3 * 3_600_000),
        last_visit_ms: Some(40 * 60_000),
        history: (0..12).map(|k| (900 + k * 190) << 20).collect(),
        family: false,
    };
    let (draws, _) = scene::window(&b, &layout, None, Some(1), &Panel::Details(&details));
    let frame = ramlord_core::render::rasterize(
        scene::WIDTH,
        scene::HEIGHT,
        ramlord_core::palette::SKY,
        &draws,
    );
    save(&frame, "details-discord.png");
    let golden = repo().join("fixtures/golden/details-card.png");
    let bytes = std::fs::read(&golden).unwrap_or_else(|e| panic!("{}: {e}", golden.display()));
    let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    buf.truncate(info.buffer_size());
    assert!(
        buf == frame.scaled(TOWER_PNG_SCALE).to_rgb8(),
        "the details card differs from the golden image"
    );
}
