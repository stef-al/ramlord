//! Guards the core's ground rules: no OS calls, clock, randomness, threads,
//! floating point or `unsafe` in `crates/core/src`.

use std::fs;
use std::path::{Path, PathBuf};

/// Identifiers and paths the core must never use.
const FORBIDDEN: &[&str] = &[
    "f32",
    "f64",
    "unsafe",
    "std::time",
    "std::fs",
    "std::thread",
    "std::process",
    "std::env",
    "std::net",
    "std::io",
    "std::os",
    "Instant",
    "SystemTime",
    "rand",
];

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read source dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Source text with `//` comments removed, so docs may name what is forbidden.
fn code_only(text: &str) -> String {
    text.lines()
        .map(|line| match line.find("//") {
            Some(at) => &line[..at],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// True when `word` appears in `code` as a whole token (not inside a longer identifier).
fn has_token(code: &str, word: &str) -> bool {
    let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    code.match_indices(word).any(|(at, _)| {
        let before = code[..at].chars().next_back();
        let after = code[at + word.len()..].chars().next();
        !before.is_some_and(is_ident) && !after.is_some_and(is_ident)
    })
}

#[test]
fn core_sources_stay_pure() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    assert!(
        !files.is_empty(),
        "no sources found under {}",
        src.display()
    );
    let mut breaches = Vec::new();
    for file in &files {
        let code = code_only(&fs::read_to_string(file).expect("read source"));
        for word in FORBIDDEN {
            if has_token(&code, word) {
                breaches.push(format!("{}: {word}", file.display()));
            }
        }
    }
    assert!(
        breaches.is_empty(),
        "the core must stay pure:\n{}",
        breaches.join("\n")
    );
}

#[test]
fn token_check_ignores_longer_identifiers() {
    assert!(has_token("let x: f64 = 1;", "f64"));
    assert!(!has_token("let half64 = 1;", "f64"));
    assert!(!has_token("operand", "rand"));
    assert!(has_token("use std::time::Instant;", "std::time"));
}
