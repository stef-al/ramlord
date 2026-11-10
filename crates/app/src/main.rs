//! `ramlord`: the exe. On Windows it is a GUI-subsystem program, so every
//! mode reports through files and exit codes, not console text (SPEC.md,
//! Command line). On Linux it has no window; it only renders files.

#![cfg_attr(windows, windows_subsystem = "windows")]
#![deny(unsafe_code)]

use std::process::ExitCode;

/// Exit code for a mode that is not built yet.
const NOT_AVAILABLE: u8 = 5;

fn main() -> ExitCode {
    // M0 skeleton: no mode is built yet.
    ExitCode::from(NOT_AVAILABLE)
}
