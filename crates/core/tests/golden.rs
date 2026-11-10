//! Golden-image tests: the sample building drawn now must match the PNGs
//! in `fixtures/golden/` pixel for pixel, on every platform.
//!
//! When a rendering change is intended, regenerate the golden images with
//! `ramlord --building fixtures/sample-building.json --render
//! fixtures/golden/sample-tower.png --card fixtures/golden/sample-rent-roll.png`,
//! look at them, and say in the commit what changed.

mod timeline;

use std::collections::BTreeSet;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use ramlord_core::scene::{CARD_PNG_SCALE, TOWER_PNG_SCALE};
use ramlord_core::tenants::{Lease, building};
use ramlord_core::{Building, Frame, HueBook, Rules, Snapshot, render_card, render_tower};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn sample() -> Building {
    let text = std::fs::read_to_string(repo().join("fixtures/sample-building.json")).unwrap();
    Building::from_json(&text).unwrap()
}

/// Decodes a PNG into (width, height, RGB bytes).
fn decode(path: &Path) -> (u32, u32, Vec<u8>) {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let decoder = png::Decoder::new(Cursor::new(bytes));
    let mut reader = decoder.read_info().expect("PNG header");
    let mut buf = vec![0; reader.output_buffer_size().expect("buffer size")];
    let info = reader.next_frame(&mut buf).expect("PNG data");
    buf.truncate(info.buffer_size());
    let rgb = match info.color_type {
        png::ColorType::Rgb => buf,
        png::ColorType::Rgba => buf.chunks(4).flat_map(|p| [p[0], p[1], p[2]]).collect(),
        other => panic!("{}: unexpected colour type {other:?}", path.display()),
    };
    (info.width, info.height, rgb)
}

/// Asserts `frame` equals the PNG at `path`, naming the first differences.
fn assert_same_pixels(frame: &Frame, path: &Path) {
    let (w, h, expected) = decode(path);
    assert_eq!(
        (frame.width, frame.height),
        (w, h),
        "{}: size",
        path.display()
    );
    let actual = frame.to_rgb8();
    let diffs: Vec<String> = (0..(w * h) as usize)
        .filter(|&i| actual[i * 3..i * 3 + 3] != expected[i * 3..i * 3 + 3])
        .take(5)
        .map(|i| format!("({}, {})", i % w as usize, i / w as usize))
        .collect();
    assert!(
        diffs.is_empty(),
        "{}: pixels differ, first at {}",
        path.display(),
        diffs.join(", ")
    );
}

#[test]
fn sample_tower_matches_golden_pixel_for_pixel() {
    let (frame, _) = render_tower(&sample(), &HueBook::new());
    assert_eq!((frame.width, frame.height), (384, 240));
    assert_same_pixels(
        &frame.scaled(TOWER_PNG_SCALE),
        &repo().join("fixtures/golden/sample-tower.png"),
    );
}

#[test]
fn sample_card_matches_golden_pixel_for_pixel() {
    let frame = render_card(&sample(), &HueBook::new());
    assert_eq!((frame.width, frame.height), (240, 300));
    assert_same_pixels(
        &frame.scaled(CARD_PNG_SCALE),
        &repo().join("fixtures/golden/sample-rent-roll.png"),
    );
}

/// M1's acceptance: the renders match the reference images that
/// `docs/reference/prototype.py` drew. Drop this test when a later
/// milestone changes the look on purpose; the golden tests stay.
#[test]
fn sample_matches_the_reference_renders() {
    let b = sample();
    let (tower, _) = render_tower(&b, &HueBook::new());
    assert_same_pixels(
        &tower.scaled(TOWER_PNG_SCALE),
        &repo().join("docs/reference/tower.png"),
    );
    assert_same_pixels(
        &render_card(&b, &HueBook::new()).scaled(CARD_PNG_SCALE),
        &repo().join("docs/reference/rent-roll.png"),
    );
}

/// A hand-written snapshot of a 16 GB PC, grouped with the built-in rules:
/// the `--snapshot` path from processes to pixels.
#[test]
fn desk_snapshot_matches_golden_pixel_for_pixel() {
    let text = std::fs::read_to_string(repo().join("fixtures/snapshot-desk.json")).unwrap();
    let snapshot = Snapshot::from_json(&text).unwrap();
    let b = building(&snapshot, &Rules::builtin(), &Lease::placeholder());
    b.validate().unwrap();
    let (frame, _) = render_tower(&b, &HueBook::new());
    assert_same_pixels(
        &frame.scaled(TOWER_PNG_SCALE),
        &repo().join("fixtures/golden/snapshot-desk-tower.png"),
    );
}

/// A full building after an hour of watched time: the remark, a gold
/// outline around the largest tenant, LEAK and WALLS (fixtures/timelines/full.json).
#[test]
fn full_building_timeline_matches_golden_pixel_for_pixel() {
    use ramlord_core::flags::State;
    use ramlord_core::live::Live;
    let mut live = Live::new(
        Rules::builtin(),
        Lease::placeholder(),
        State::default(),
        BTreeSet::new(),
    );
    for event in timeline::expand(&timeline::load("full.json")) {
        if let timeline::Event::Sample(s) = event {
            live.update(&s).unwrap();
        }
    }
    assert_same_pixels(
        &live.frame().unwrap().scaled(TOWER_PNG_SCALE),
        &repo().join("fixtures/golden/timeline-full-tower.png"),
    );
}

/// The desk PC under memory pressure: compressed memory as boxes in the
/// basement in their tenants' hues, a VM host as a tenant, a BOXES line in
/// the rent roll (fixtures/snapshot-packed.json).
#[test]
fn packed_snapshot_matches_golden_pixel_for_pixel() {
    let text = std::fs::read_to_string(repo().join("fixtures/snapshot-packed.json")).unwrap();
    let snapshot = Snapshot::from_json(&text).unwrap();
    let b = building(&snapshot, &Rules::builtin(), &Lease::placeholder());
    b.validate().unwrap();
    assert!(b.boxes_bytes > 0);
    let (frame, _) = render_tower(&b, &HueBook::new());
    assert_same_pixels(
        &frame.scaled(TOWER_PNG_SCALE),
        &repo().join("fixtures/golden/snapshot-packed-tower.png"),
    );
}

#[test]
fn same_input_same_pixels() {
    let b = sample();
    let (a, _) = render_tower(&b, &HueBook::new());
    let (c, _) = render_tower(&b, &HueBook::new());
    assert_eq!(a, c);
}
