//! M4: clicks and keys map to the right tenant and action.
//!
//! With `RAMLORD_PNG_DIR` set, the details card and the dialogs are written
//! there as PNGs, to look at (rung 2).

use std::collections::BTreeSet;
use std::io::Cursor;
use std::path::Path;

use ramlord_core::flags::State;
use ramlord_core::layout::{Holder, Layout, Owner};
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
    // Staff's units open the Staff card; Vacant's select nothing.
    assert_eq!(at(unit(0, 0)), Some(Target::Staff));
    assert_eq!(at(unit(15, 0)), None);
    // Rent roll rows: Chrome, Discord, VS Code...
    assert_eq!(at((300, 54)), Some(Target::Tenant(0)));
    assert_eq!(at((300, 64)), Some(Target::Tenant(1)));
    assert_eq!(at((300, 144)), Some(Target::Tenant(9))); // OneDrive, the tenth
    // The STAFF line, below RENT OWED, opens the Staff card too.
    assert_eq!(at((300, 182)), Some(Target::Staff));
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
                    Target::Tenant(_)
                        | Target::Staff
                        | Target::Others
                        | Target::RentRollCard
                        | Target::Settings
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
                    Target::Tenant(_)
                        | Target::Staff
                        | Target::Others
                        | Target::RentRollCard
                        | Target::Settings
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
    let Command::Close { processes } = &commands[0] else {
        panic!("{commands:?}");
    };
    assert_eq!(processes.len(), 42);
    // Each is named with the start time the snapshot gave it, so the
    // Windows layer can tell it from a later process with the same id.
    for key in processes {
        let p = snapshot.processes.iter().find(|p| p.pid == key.pid);
        assert_eq!(p.map(|p| p.started_at_ms), Some(key.started_at_ms));
        assert!(key.started_at_ms > 0);
    }
    assert!(live.waiting());
    // Some stay past 5 seconds: it refuses.
    live.poll(1, now + 5_000);
    assert!(!live.waiting());
    save(live.frame().unwrap(), "dialog-refused.png");
    // Meanwhile all but one of its processes have left: Force names the
    // ones the newest sample shows, not the 42 it was asked with.
    let mut later = snapshot.clone();
    later.at_ms += 2_000;
    let mut kept = 0;
    later.processes.retain(|p| {
        let chrome = p
            .image_path
            .as_deref()
            .is_some_and(|i| i.contains("Chrome"));
        kept += u32::from(chrome);
        !chrome || kept == 1
    });
    live.update(&later).unwrap();
    let commands = live.input(Input::Key(Key::Delete), now + 6_000);
    let [Command::Terminate { processes }] = &commands[..] else {
        panic!("{commands:?}");
    };
    assert_eq!(processes.len(), 1);
    live.poll(0, now + 6_500);
    // The next sample, without Chrome, reports what was vacated.
    let mut after = snapshot.clone();
    after.at_ms += 4_000;
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
    assert!(matches!(&commands[..], [Command::ApplySettings(s)] if s.family.len() == 1));
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

fn gallery() -> Snapshot {
    let text = std::fs::read_to_string(repo().join("fixtures/snapshot-gallery.json")).unwrap();
    Snapshot::from_json(&text).unwrap()
}

/// Chrome on a PC with graphics memory: its card shows the lease broken
/// down, and its pictures in the gallery are its own.
#[test]
fn the_lease_is_broken_down_on_the_card() {
    let mut live = live();
    let snapshot = gallery();
    live.update(&snapshot).unwrap();
    let now = snapshot.at_ms;
    live.input(Input::Key(Key::Down), now);
    live.input(Input::Key(Key::Enter), now);
    save(live.frame().unwrap(), "details-gallery-chrome.png");
    let b = live.building().unwrap().clone();
    let index = b.tenants.iter().position(|t| t.name == "Chrome").unwrap();
    let chrome = &b.tenants[index];
    assert_eq!(chrome.gpu_bytes, 900 << 20);
    assert_eq!(b.gallery_bytes, 2050 << 20);
    // Its lease holds what is in RAM and its pictures; the rest is on
    // paper (in the page file, or never touched).
    assert!(chrome.lease_bytes > chrome.rent_bytes + chrome.gpu_bytes);
    // A click on one of its pictures selects it too.
    let (layout, _) = Layout::new(&b, &HueBook::new());
    let picture = layout
        .cells
        .iter()
        .position(|&o| o == Owner::Gallery(Holder::Tenant(index)))
        .expect("Chrome has pictures of its own");
    let (x, y) = unit((picture / 16) as i32, (picture % 16) as i32);
    live.input(Input::Key(Key::Escape), now);
    live.input(Input::Key(Key::Escape), now);
    live.input(Input::Click { x, y }, now);
    assert_eq!(live.ui().view, View::Details);
    assert_eq!(live.ui().selected.as_deref(), Some(chrome.id.as_str()));
}

/// The Staff card opens from Staff's units or its line, lists its parts,
/// and Esc or Back closes it.
#[test]
fn staff_opens_its_card_with_its_parts() {
    const MIB: u64 = 1 << 20;
    let mut live = live();
    let snapshot = gallery();
    live.update(&snapshot).unwrap();
    let now = snapshot.at_ms;
    let staff_line = live
        .regions()
        .iter()
        .find(|r| r.target == Target::Staff && r.x0 > 224)
        .copied()
        .expect("the STAFF line can be clicked");
    let click_line = Input::Click {
        x: staff_line.x0 + 2,
        y: staff_line.y0 + 2,
    };
    live.input(click_line, now);
    assert_eq!(live.ui().view, View::Staff);
    save(live.frame().unwrap(), "staff-card.png");
    let b = live.building().unwrap();
    let parts = &b.staff_parts;
    assert_eq!(parts.sum(), b.staff_bytes, "the parts make up Staff");
    assert_eq!(parts.hardware_bytes, 368 * MIB);
    assert_eq!(parts.kernel_pool_bytes, (380 + 420) * MIB);
    assert_eq!(parts.drivers_bytes, (58 + 17) * MIB);
    assert_eq!(parts.file_cache_bytes, 310 * MIB);
    assert_eq!(parts.modified_bytes, 140 * MIB);
    assert!(parts.services_bytes > 0);
    // The next sample keeps the card open, though no tenant is selected.
    let mut later = snapshot.clone();
    later.at_ms += 2_000;
    live.update(&later).unwrap();
    assert_eq!(live.ui().view, View::Staff);
    // Esc closes it; a click on a Staff unit opens it again; Back closes it.
    live.input(Input::Key(Key::Escape), now);
    assert_eq!(live.ui().view, View::Roll);
    let staff_unit = live
        .regions()
        .iter()
        .find(|r| r.target == Target::Staff && r.x0 < 224)
        .copied()
        .expect("Staff's units can be clicked");
    live.input(
        Input::Click {
            x: staff_unit.x0 + 2,
            y: staff_unit.y0 + 2,
        },
        now,
    );
    assert_eq!(live.ui().view, View::Staff);
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
    // Down from the Staff card selects the first tenant in the rent roll.
    live.input(click_line, now);
    live.input(Input::Key(Key::Down), now);
    assert_eq!(live.ui().view, View::Roll);
    assert!(live.ui().selected.is_some());
}

/// The top-left pixel of a unit; floor 0 is the ground floor.
fn cell_origin(index: usize) -> (u32, u32) {
    let (floor, col) = ((index / 16) as u32, (index % 16) as u32);
    (16 + col * 12, 30 + (15 - floor) * 11)
}

/// Every unit, box and picture can be clicked, and leads to its holder: a
/// tenant, Staff or Others. Selecting a tenant outlines all it has.
#[test]
fn every_part_of_the_house_leads_to_its_holder() {
    let mut live = live();
    let snapshot = gallery();
    live.update(&snapshot).unwrap();
    let now = snapshot.at_ms;
    let b = live.building().unwrap().clone();
    let (layout, _) = Layout::new(&b, &HueBook::new());
    for (index, owner) in layout.cells.iter().enumerate() {
        let (x, y) = cell_origin(index);
        let want = match owner.holder() {
            Some(Holder::Tenant(t)) => Some(Target::Tenant(t)),
            Some(Holder::Staff) => Some(Target::Staff),
            Some(Holder::Others) => Some(Target::Others),
            None => None,
        };
        assert_eq!(
            hit(live.regions(), x as i32 + 5, y as i32 + 5),
            want,
            "cell {index}"
        );
    }
    // A click on one of Chrome's boxes selects Chrome and outlines its
    // units, boxes and pictures alike.
    let chrome = b.tenants.iter().position(|t| t.name == "Chrome").unwrap();
    let mine = Holder::Tenant(chrome);
    let first = |part: fn(Holder) -> Owner| {
        layout
            .cells
            .iter()
            .position(|&o| o == part(mine))
            .expect("Chrome has one")
    };
    let (bx, by) = cell_origin(first(Owner::Boxes));
    live.input(
        Input::Click {
            x: bx as i32 + 5,
            y: by as i32 + 5,
        },
        now,
    );
    assert_eq!(live.ui().view, View::Details);
    save(live.frame().unwrap(), "holder-chrome.png");
    let frame = live.frame().unwrap();
    let text = ramlord_core::palette::TEXT;
    for part in [Owner::Boxes, Owner::Gallery] {
        // Its first cell of each kind has an outline on its left or below.
        let index = first(part);
        let (x, y) = cell_origin(index);
        let left = index % 16 > 0 && layout.cells[index - 1].holder() != Some(mine);
        let lit = if left {
            frame.pixel(x - 1, y + 4)
        } else {
            frame.pixel(x + 4, y + 10)
        };
        assert_eq!(lit, text, "{:?} is outlined", part(mine));
    }
    // A click on Staff's pictures opens the Staff card.
    let staff_picture = layout
        .cells
        .iter()
        .position(|&o| o == Owner::Gallery(Holder::Staff))
        .expect("Windows hangs pictures too");
    let (sx, sy) = cell_origin(staff_picture);
    live.input(Input::Key(Key::Escape), now);
    live.input(
        Input::Click {
            x: sx as i32 + 5,
            y: sy as i32 + 5,
        },
        now,
    );
    assert_eq!(live.ui().view, View::Staff);
    save(live.frame().unwrap(), "holder-staff.png");
}

/// A PC with twenty small apps: each under half a unit, they all live in
/// Others.
fn many_small() -> Snapshot {
    let mut s = desk();
    let parent = s
        .processes
        .iter()
        .find(|p| {
            p.image_path
                .as_deref()
                .is_some_and(|i| i.ends_with("explorer.exe"))
        })
        .unwrap()
        .pid;
    for k in 0..20u32 {
        let mut p = s
            .processes
            .iter()
            .find(|p| p.pid == parent)
            .unwrap()
            .clone();
        p.pid = 9000 + k;
        p.parent_pid = parent;
        p.started_at_ms += 1000 + u64::from(k);
        p.image_path = Some(format!(r"C:\Apps\Small{k:02}\small{k:02}.exe"));
        p.image_name = Some(format!("small{k:02}.exe"));
        p.description = Some(format!("Small App {k:02}"));
        p.rent_bytes = (10 + u64::from(k)) << 20;
        p.lease_bytes = p.rent_bytes * 2;
        p.working_set_bytes = p.rent_bytes;
        p.has_visible_window = true;
        p.is_self = false;
        s.available_bytes -= p.rent_bytes;
        s.processes.push(p);
    }
    s
}

/// The Others card lists everyone the rent roll does not, a page at a
/// time; a row opens its details card, whose Back returns to the list.
#[test]
fn the_others_card_lists_its_apps_and_opens_their_cards() {
    let mut live = live();
    let snapshot = many_small();
    live.update(&snapshot).unwrap();
    let now = snapshot.at_ms;
    let find = |live: &Live, target: Target, right: bool| {
        live.regions()
            .iter()
            .find(|r| r.target == target && (r.x0 > 224) == right)
            .copied()
            .unwrap_or_else(|| panic!("{target:?} can be clicked"))
    };
    let click = |live: &mut Live, r: ramlord_core::ui::Region| {
        live.input(
            Input::Click {
                x: r.x0 + 2,
                y: r.y0 + 2,
            },
            now,
        );
    };
    // The OTHERS line opens it; so do Others' units.
    let line = find(&live, Target::Others, true);
    click(&mut live, line);
    assert_eq!(live.ui().view, View::Others);
    save(live.frame().unwrap(), "others-card.png");
    let rows = |live: &Live| {
        live.regions()
            .iter()
            .filter(|r| r.x0 > 224 && matches!(r.target, Target::Tenant(_)))
            .count()
    };
    assert_eq!(rows(&live), 13, "a page holds 13");
    // MORE turns the page and selects its first row.
    let more = find(&live, Target::More, true);
    click(&mut live, more);
    assert_eq!(live.ui().view, View::Others);
    let second_page = rows(&live);
    assert_eq!(second_page, 22 - 13, "the second page holds the rest");
    // A row opens that app's card; Back returns to the list.
    let row = live
        .regions()
        .iter()
        .find(|r| r.x0 > 224 && matches!(r.target, Target::Tenant(_)))
        .copied()
        .unwrap();
    let Target::Tenant(t) = row.target else {
        unreachable!()
    };
    click(&mut live, row);
    assert_eq!(live.ui().view, View::Details);
    assert_eq!(
        live.ui().selected.as_deref(),
        Some(live.building().unwrap().tenants[t].id.as_str())
    );
    save(live.frame().unwrap(), "others-details.png");
    assert!(
        live.regions().iter().any(|r| r.target == Target::Evict),
        "a small app can be evicted from its card"
    );
    let back = find(&live, Target::Back, true);
    click(&mut live, back);
    assert_eq!(live.ui().view, View::Others);
    // Keys: Down moves along the list, Enter opens, Esc goes back to the
    // list, Delete asks to evict, Esc twice leaves.
    live.input(Input::Key(Key::Down), now);
    live.input(Input::Key(Key::Enter), now);
    assert_eq!(live.ui().view, View::Details);
    live.input(Input::Key(Key::Escape), now);
    assert_eq!(live.ui().view, View::Others);
    live.input(Input::Key(Key::Delete), now);
    assert!(matches!(live.ui().eviction, Eviction::Confirm { .. }));
    live.input(Input::Key(Key::Escape), now);
    assert_eq!(live.ui().eviction, Eviction::Idle);
    live.input(Input::Key(Key::Escape), now);
    assert_eq!(live.ui().view, View::Roll);
    // Others' units open it too.
    let unit = find(&live, Target::Others, false);
    click(&mut live, unit);
    assert_eq!(live.ui().view, View::Others);
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
