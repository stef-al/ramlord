//! The exe end to end: `--building` with `--render` and `--card`, and the
//! exit codes. On the Windows CI runner this runs the Windows exe, so the
//! golden comparison there shows Windows draws the same pixels as Linux.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn ramlord() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ramlord"))
}

/// A fresh directory for this test's output files.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ramlord-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn pixels(path: &Path) -> (u32, u32, Vec<u8>) {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    buf.truncate(info.buffer_size());
    (info.width, info.height, buf)
}

fn exit_code(args: &[&str]) -> i32 {
    ramlord().args(args).status().unwrap().code().unwrap()
}

#[test]
fn render_and_card_match_the_golden_images() {
    let dir = scratch("golden");
    let (tower, card) = (dir.join("tower.png"), dir.join("card.png"));
    let status = ramlord()
        .arg("--building")
        .arg(repo().join("fixtures/sample-building.json"))
        .arg("--render")
        .arg(&tower)
        .arg("--card")
        .arg(&card)
        .status()
        .unwrap();
    assert!(status.success(), "{status:?}");
    let golden_tower = pixels(&repo().join("fixtures/golden/sample-tower.png"));
    let golden_card = pixels(&repo().join("fixtures/golden/sample-rent-roll.png"));
    let (tw, th, tp) = pixels(&tower);
    let (cw, ch, cp) = pixels(&card);
    assert_eq!((tw, th), (1152, 720));
    assert_eq!((cw, ch), (960, 1200));
    assert!(
        tp == golden_tower.2,
        "the tower differs from the golden image"
    );
    assert!(
        cp == golden_card.2,
        "the card differs from the golden image"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn missing_or_invalid_input_exits_3() {
    let dir = scratch("input");
    let out = dir.join("out.png");
    let out = out.to_str().unwrap();
    assert_eq!(
        exit_code(&["--building", "no-such-file.json", "--render", out]),
        3
    );
    let bad = dir.join("bad.json");
    std::fs::write(&bad, "{\"kind\": \"building\"}").unwrap();
    assert_eq!(
        exit_code(&["--building", bad.to_str().unwrap(), "--render", out]),
        3
    );
    assert!(!Path::new(out).exists());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn unwritable_output_exits_4() {
    let fixture = repo().join("fixtures/sample-building.json");
    let out = repo().join("no-such-dir/out.png");
    assert_eq!(
        exit_code(&[
            "--building",
            fixture.to_str().unwrap(),
            "--card",
            out.to_str().unwrap()
        ]),
        4
    );
}

#[test]
fn usage_errors_exit_2() {
    assert_eq!(exit_code(&["--frobnicate"]), 2);
    assert_eq!(exit_code(&["--building", "x.json"]), 2);
}

#[test]
fn snapshot_render_matches_the_golden_image() {
    let dir = scratch("snapshot");
    let tower = dir.join("tower.png");
    let status = ramlord()
        .arg("--snapshot")
        .arg(repo().join("fixtures/snapshot-desk.json"))
        .arg("--render")
        .arg(&tower)
        .status()
        .unwrap();
    assert!(status.success(), "{status:?}");
    let golden = pixels(&repo().join("fixtures/golden/snapshot-desk-tower.png"));
    assert!(
        pixels(&tower) == golden,
        "the tower differs from the golden image"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_invalid_snapshot_exits_3() {
    let fixture = repo().join("fixtures/sample-building.json");
    let out = std::env::temp_dir().join("ramlord-test-never-written.png");
    assert_eq!(
        exit_code(&[
            "--snapshot",
            fixture.to_str().unwrap(),
            "--render",
            out.to_str().unwrap()
        ]),
        3
    );
}

/// On Windows these open a window or sample the PC; CI runs them there.
#[cfg(not(windows))]
#[test]
fn the_windows_modes_need_windows() {
    assert_eq!(exit_code(&[]), 5);
    assert_eq!(exit_code(&["--minimized"]), 5);
    assert_eq!(exit_code(&["--demo"]), 5);
    assert_eq!(exit_code(&["--dump", "out.json"]), 5);
    assert_eq!(exit_code(&["--self-test", "out.json"]), 5);
}
