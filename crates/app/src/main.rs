//! `ramlord`: the exe. On Windows it is a GUI-subsystem program, so every
//! mode reports through files and exit codes, not console text (SPEC.md,
//! Command line). On Linux it has no window; it only renders files.

#![cfg_attr(windows, windows_subsystem = "windows")]
#![deny(unsafe_code)]

mod cli;
mod output;
// The price-fetch rules; the fetch itself is the Windows layer's.
#[cfg_attr(not(windows), allow(dead_code))]
mod pricing;
// Runs only on Windows; on Linux its tests drive it with a fake machine.
#[cfg_attr(not(windows), allow(dead_code))]
mod selftest;
// state.json is read and written by the live app on Windows; its tests run everywhere.
#[cfg_attr(not(windows), allow(dead_code))]
mod store;
#[cfg(windows)]
#[allow(unsafe_code)]
mod win;

use std::path::Path;
use std::process::ExitCode;

#[cfg(windows)]
use ramlord_core::prices::PriceTable;
use ramlord_core::scene::{CARD_PNG_SCALE, TOWER_PNG_SCALE};
use ramlord_core::tenants::Lease;
use ramlord_core::{Building, HueBook, Rules, Snapshot};

/// Exit codes shared by every mode.
pub mod exit {
    pub const USAGE: u8 = 2;
    pub const INPUT: u8 = 3;
    pub const OUTPUT: u8 = 4;
    pub const NOT_AVAILABLE: u8 = 5;
    pub const WINDOW: u8 = 6;
    pub const BUDGET: u8 = 7;
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
            let building = read(&file).and_then(|t| {
                Building::from_json(&t).map_err(|e| format!("{}: {e}", file.display()))
            });
            match building {
                Ok(b) => draw(&b, render.as_deref(), card.as_deref()),
                Err(e) => fail(exit::INPUT, e),
            }
        }
        Ok(cli::Command::Snapshot { file, render, card }) => {
            let building = read(&file)
                .and_then(|t| {
                    Snapshot::from_json(&t).map_err(|e| format!("{}: {e}", file.display()))
                })
                .and_then(|s| {
                    let b = ramlord_core::tenants::building(
                        &s,
                        &Rules::builtin(),
                        &Lease::placeholder(),
                    );
                    b.validate()
                        .map(|()| b)
                        .map_err(|e| format!("{}: {e}", file.display()))
                });
            match building {
                Ok(b) => draw(&b, render.as_deref(), card.as_deref()),
                Err(e) => fail(exit::INPUT, e),
            }
        }
        Ok(cli::Command::Demo) => demo(),
        Ok(cli::Command::Run { minimized }) => run(minimized),
        Ok(cli::Command::Dump(out)) => dump(&out),
        Ok(cli::Command::SelfTest(out)) => self_test(&out),
        Err(e) => fail(exit::USAGE, format!("{e}\n{}", cli::USAGE)),
    }
}

fn read(file: &Path) -> Result<String, String> {
    std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))
}

fn draw(building: &Building, render: Option<&Path>, card: Option<&Path>) -> ExitCode {
    let hues = HueBook::new();
    if let Some(path) = render {
        let (frame, _) = ramlord_core::render_tower(building, &hues);
        if let Err(e) = output::write_png(path, &frame, TOWER_PNG_SCALE) {
            return fail(exit::OUTPUT, e);
        }
    }
    if let Some(path) = card {
        let frame = ramlord_core::render_card(building, &hues);
        if let Err(e) = output::write_png(path, &frame, CARD_PNG_SCALE) {
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
    match win::shell::show(win::shell::Content::Still(frame), false) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(exit::WINDOW, e),
    }
}

#[cfg(windows)]
fn run(minimized: bool) -> ExitCode {
    // Only one copy runs: a second start brings the first forward.
    if !win::instance::first() {
        return ExitCode::SUCCESS;
    }
    // Watched time, flags and hues carry over from the last run; settings
    // live beside them. The Run key is the truth for start with Windows.
    let state_path = store::state_path();
    let state = state_path.as_deref().map(store::load).unwrap_or_default();
    let settings_path = store::settings_path();
    let mut settings = settings_path
        .as_deref()
        .map(store::load_settings)
        .unwrap_or_default();
    settings.start_with_windows = win::extras::autostart();
    // The embedded prices, or the last fetched file if it is newer.
    let cache_path = store::prices_path();
    let cached = cache_path
        .as_deref()
        .and_then(|p| std::fs::read_to_string(p).ok());
    let prices = PriceTable::embedded().refreshed(cached.as_deref());
    let mut live = ramlord_core::live::Live::new(
        Rules::builtin(),
        Lease::placeholder(),
        state,
        settings.family.clone(),
    );
    live.configure(settings, prices, win::extras::memory_type());
    let content = win::shell::Content::Live {
        provider: Box::new(win::provider::Provider::new()),
        live: Box::new(live),
        state_path,
        settings_path,
        cache_path,
    };
    match win::shell::show(content, minimized) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(exit::WINDOW, e),
    }
}

/// `--dump`: one live snapshot, with the profile folder hidden.
#[cfg(windows)]
fn dump(out: &Path) -> ExitCode {
    let mut snapshot = match win::provider::Provider::new().sample() {
        Ok(s) => s,
        Err(e) => return fail(exit::INPUT, e),
    };
    if let Some(profile) = std::env::var_os("USERPROFILE") {
        snapshot.hide_profile(&profile.to_string_lossy());
    }
    match std::fs::write(out, snapshot.to_json()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(exit::OUTPUT, format!("{}: {e}", out.display())),
    }
}

/// `--self-test`: 10 seconds of sampling and drawing off screen.
#[cfg(windows)]
fn self_test(out: &Path) -> ExitCode {
    let exe_bytes = std::env::current_exe()
        .and_then(std::fs::metadata)
        .map(|m| m.len())
        .unwrap_or(u64::MAX);
    let cpus = std::thread::available_parallelism().map_or(1, |n| n.get() as u64);
    let mut machine = win::Pc(win::provider::Provider::new());
    // Off screen and with a fresh state: the self-test never touches the
    // user's state file.
    let mut live = ramlord_core::live::Live::new(
        Rules::builtin(),
        Lease::placeholder(),
        ramlord_core::flags::State::default(),
        Default::default(),
    );
    let report = selftest::run(
        &mut machine,
        &mut live,
        &selftest::Schedule::SPEC,
        exe_bytes,
        cpus,
        &selftest::Limits::SPEC,
    );
    if let Err(e) = std::fs::write(out, report.to_json()) {
        return fail(exit::OUTPUT, format!("{}: {e}", out.display()));
    }
    if report.pass() {
        ExitCode::SUCCESS
    } else {
        fail(exit::BUDGET, "a budget is broken; see the report")
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

#[cfg(not(windows))]
fn run(_minimized: bool) -> ExitCode {
    fail(
        exit::NOT_AVAILABLE,
        "the app needs Windows; on Linux use --building or --snapshot",
    )
}

#[cfg(not(windows))]
fn dump(_out: &Path) -> ExitCode {
    fail(exit::NOT_AVAILABLE, "--dump needs Windows")
}

#[cfg(not(windows))]
fn self_test(_out: &Path) -> ExitCode {
    fail(exit::NOT_AVAILABLE, "--self-test needs Windows")
}
