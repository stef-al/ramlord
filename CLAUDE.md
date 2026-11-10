# RAMlord: notes for Claude

RAMlord is a small Windows desktop app in Rust that draws the PC's memory as a
pixel-art apartment tower. `SPEC.md` is the source of truth; read it before
changing behavior. `docs/reference/prototype.py` is a worked example of the
look; where it disagrees with `SPEC.md`, `SPEC.md` wins. Never build from
`docs/reference/drafts/`: those files are superseded.

## Layout

```text
crates/core/   ramlord-core: logic and drawing, pure (no OS calls)
crates/app/    ramlord: the exe; all Windows code under src/win/
fixtures/      buildings as JSON; fixtures/golden/ holds the expected PNGs
rules/         windows.toml: how processes become tenants (used from M2)
prices.json    price per GB by memory type (used from M5)
scripts/       wine-demo.sh: rung 4
```

## Build and test

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo check --workspace --all-targets --target x86_64-pc-windows-gnu
cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings
```

Render the sample building (the tower at 3x, the card at 4x):

```bash
cargo run -p ramlord -- --building fixtures/sample-building.json --render tower.png --card rent-roll.png
```

When a rendering change is intended, regenerate the golden images with the
same command into `fixtures/golden/sample-tower.png` and
`fixtures/golden/sample-rent-roll.png`, look at both, and say in the commit
what changed and why.

Build the Windows exe in the session (needs `x86_64-w64-mingw32-gcc`):

```bash
cargo build --release --target x86_64-pc-windows-gnu
```

The exe exits with 0 on success, 2 for a usage error, 3 when the input
cannot be read or is invalid, 4 when the output cannot be written, 5 for a
mode not available on this platform or not built yet, and 6 when the window
fails.

## Ground rules

- The core crate is pure: no OS calls, clock, randomness, threads or floating
  point. Bytes, units, pixels and money are integers; money is micro-units
  (price per GB times 1,000,000). `crates/core/tests/purity.rs` enforces it.
- `unsafe` appears only under `crates/app/src/win/`.
  `crates/app/tests/unsafe_scope.rs` enforces it.
- Dependencies stay within `windows` (Windows only), `serde`, `serde_json`,
  `toml` and `png`. Anything more needs a reason in the pull request and a
  license that GPL-3.0 allows. No GUI toolkit, web view, async runtime,
  bundled TLS library or exe packer.
- Keep `LICENSE` (GPL-3.0) as it is; `Cargo.toml` says `GPL-3.0-only`.
- Tests come with the code. The "Dividing the building" rule and the sample
  building table in `SPEC.md` are test vectors.
- Same input, same pixels on every platform: golden-image tests depend on it.
- Commit in small steps with clear messages.
- `gh pr` and `gh issue` do not work in cloud sessions; the owner creates pull
  requests. Do not start a milestone that was not asked for.

## The six verification rungs

| Rung | Runs | Check |
| --- | --- | --- |
| 1 | Session | `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` |
| 2 | Session | Render a fixture to PNG (command above), open it and compare with `docs/reference/` |
| 3 | Session | `cargo check --workspace --all-targets --target x86_64-pc-windows-gnu` |
| 4 | Session | Build the exe with MinGW and run `--demo` under `xvfb-run wine` with a screenshot (below) |
| 5 | GitHub Actions `windows-latest` | Release build, `cargo test`, render the fixture with the exe and compare with the golden PNGs (`--self-test` and `--dump` join in M2) |
| 6 | The owner, on a Windows PC | The checklist in `SPEC.md` |

Rung 4, adapted from `docs/reference/wine-probe/run.sh`:

```bash
cargo build --release --target x86_64-pc-windows-gnu
WINEPREFIX=$PWD/scripts/prefix WINEDEBUG=-all WINEDLLOVERRIDES="mscoree,mshtml=" \
  xvfb-run -a -s "-screen 0 1920x1080x24" \
  scripts/wine-demo.sh target/x86_64-pc-windows-gnu/release/ramlord.exe demo-shot.png
```

Tools for rungs 3 and 4, if missing:

```bash
rustup target add x86_64-pc-windows-gnu
apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends gcc-mingw-w64-x86-64 wine wine64 xvfb imagemagick
```

## CI

`.github/workflows/ci.yml` runs on every push. The `linux` job runs rungs 1
and 3; the `windows` job runs rung 5 and uploads the exe and the rendered PNGs.
A failing step repeats the end of its output as an `::error::` annotation
(`.github/scripts/step.sh`). Read the results with:

```bash
gh api repos/stef-al/ramlord/commits/SHA/check-runs
gh api repos/stef-al/ramlord/check-runs/CHECK_RUN_ID/annotations
```
