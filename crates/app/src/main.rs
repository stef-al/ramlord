//! `ramlord`: the exe. On Windows it is a GUI-subsystem program, so every
//! mode reports through files and exit codes, not console text (SPEC.md,
//! Command line). On Linux it has no window; it only renders files.

#![cfg_attr(windows, windows_subsystem = "windows")]
#![deny(unsafe_code)]

mod cli;
mod output;
#[cfg(windows)]
#[allow(unsafe_code)]
mod win;

use std::path::Path;
use std::process::ExitCode;

use ramlord_core::{Building, HueBook};

/// Exit codes shared by every mode.
pub mod exit {
    pub const USAGE: u8 = 2;
    pub const INPUT: u8 = 3;
    pub const OUTPUT: u8 = 4;
    pub const NOT_AVAILABLE: u8 = 5;
    pub const WINDOW: u8 = 6;
}

/// The demo fixture and first test vector, built into the exe.
const SAMPLE_BUILDING: &str = include_str!("../../../fixtures/sample-building.json");

/// Reports a failure on stderr (seen on Linux and in CI; a GUI-subsystem
/// exe on Windows has no console) and returns its exit code.
fn fail(code: u8, message: impl std::fmt::Display) -> ExitCode {
    eprintln!("ramlord: {message}");
    ExitCode::from(code)
}

fn main() -> ExitCode {
    match cli::parse(std::env::args().skip(1)) {
        Ok(cli::Command::Building { file, render, card }) => {
            draw_building(&file, render.as_deref(), card.as_deref())
        }
        Ok(cli::Command::Demo) => demo(),
        Ok(cli::Command::Run) => fail(
            exit::NOT_AVAILABLE,
            "live mode comes with milestone M2; try --demo or --building",
        ),
        Ok(cli::Command::Later(flag)) => fail(
            exit::NOT_AVAILABLE,
            format!("{flag} comes with milestone M2"),
        ),
        Err(e) => fail(exit::USAGE, format!("{e}\n{}", cli::USAGE)),
    }
}

fn draw_building(file: &Path, render: Option<&Path>, card: Option<&Path>) -> ExitCode {
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(e) => return fail(exit::INPUT, format!("{}: {e}", file.display())),
    };
    let building = match Building::from_json(&text) {
        Ok(b) => b,
        Err(e) => return fail(exit::INPUT, format!("{}: {e}", file.display())),
    };
    let hues = HueBook::new();
    if let Some(path) = render {
        let (frame, _) = ramlord_core::render_tower(&building, &hues);
        if let Err(e) = output::write_png(path, &frame, ramlord_core::scene::TOWER_PNG_SCALE) {
            return fail(exit::OUTPUT, e);
        }
    }
    if let Some(path) = card {
        let frame = ramlord_core::render_card(&building, &hues);
        if let Err(e) = output::write_png(path, &frame, ramlord_core::scene::CARD_PNG_SCALE) {
            return fail(exit::OUTPUT, e);
        }
    }
    ExitCode::SUCCESS
}

#[cfg(windows)]
fn demo() -> ExitCode {
    let building = match Building::from_json(SAMPLE_BUILDING) {
        Ok(b) => b,
        Err(e) => return fail(exit::INPUT, e),
    };
    let (frame, _) = ramlord_core::render_tower(&building, &HueBook::new());
    match win::shell::show(frame) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(exit::WINDOW, e),
    }
}

#[cfg(not(windows))]
fn demo() -> ExitCode {
    // Keep the embedded fixture checked on every platform.
    if let Err(e) = Building::from_json(SAMPLE_BUILDING) {
        return fail(exit::INPUT, e);
    }
    fail(
        exit::NOT_AVAILABLE,
        "--demo needs Windows; on Linux use --building",
    )
}
