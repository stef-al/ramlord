//! Guards the rule that `unsafe` appears only under `crates/app/src/win/`.

use std::fs;
use std::path::{Path, PathBuf};

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

/// True when the `unsafe` keyword appears outside `//` comments, as a whole
/// token (so the lint name `unsafe_code` does not count).
fn uses_unsafe(text: &str) -> bool {
    let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    text.lines()
        .map(|line| line.split("//").next().unwrap_or(""))
        .any(|code| {
            code.match_indices("unsafe").any(|(at, word)| {
                let before = code[..at].chars().next_back();
                let after = code[at + word.len()..].chars().next();
                !before.is_some_and(is_ident) && !after.is_some_and(is_ident)
            })
        })
}

#[test]
fn unsafe_only_under_src_win() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let win = src.join("win");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    assert!(
        !files.is_empty(),
        "no sources found under {}",
        src.display()
    );
    let offenders: Vec<String> = files
        .iter()
        .filter(|f| !f.starts_with(&win))
        .filter(|f| uses_unsafe(&fs::read_to_string(f).expect("read source")))
        .map(|f| f.display().to_string())
        .collect();
    assert!(
        offenders.is_empty(),
        "unsafe outside src/win/: {offenders:?}"
    );
}

#[test]
fn keyword_check_ignores_the_lint_name() {
    assert!(uses_unsafe("let p = unsafe { f() };"));
    assert!(!uses_unsafe("#![deny(unsafe_code)]"));
    assert!(!uses_unsafe("// unsafe is only allowed in win/"));
}
