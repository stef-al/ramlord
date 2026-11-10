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
fixtures/      buildings and snapshots as JSON; timelines/ for flags over time;
               golden/ holds the expected PNGs
rules/         windows.toml: how processes become tenants, built into the exe
prices.json    price per GB by memory type (used from M5)
scripts/       wine-run.sh, wine-demo.sh and wine-evict.sh: rung 4
docs/evidence/ screenshots and reports behind each milestone's boxes
```

## Build and test

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo check --workspace --all-targets --target x86_64-pc-windows-gnu
cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings
```

Render the sample building (the tower at 3x, the card at 4x), and a
snapshot grouped with the rules:

```bash
cargo run -p ramlord -- --building fixtures/sample-building.json --render tower.png --card rent-roll.png
```

```bash
cargo run -p ramlord -- --snapshot fixtures/snapshot-desk.json --render desk.png
```

When a rendering change is intended, regenerate the golden images with the
same commands into `fixtures/golden/` (`sample-tower.png`,
`sample-rent-roll.png`, `snapshot-desk-tower.png`, `snapshot-packed-tower.png`
from `fixtures/snapshot-packed.json`, `snapshot-gallery-tower.png` from
`fixtures/snapshot-gallery.json`), look at them, and say in the commit
what changed and why. `timeline-full-tower.png` is the last frame
of `fixtures/timelines/full.json`; write it, and a frame at every checkpoint
of every timeline, with:

```bash
RAMLORD_PNG_DIR=/some/dir cargo test -p ramlord-core --test flags
```

`details-card.png` is Discord's card on the sample building. The input
tests write it as `details-discord.png`, with Chrome's card, every
eviction dialog, and Chrome's card and the Staff card on the gallery
snapshot:

```bash
RAMLORD_PNG_DIR=/some/dir cargo test -p ramlord-core --test input
```

Clicks and keys go through `ramlord_core::ui`: `scene::window` returns
the click regions with the draw list, `Ui::input` turns a click or key into
`Command`s for the Windows layer, and eviction is the `Eviction` state
machine. `crates/app/src/win/evict.rs` only carries the commands out.

Beyond SPEC.md, at the owner's request: the boxes in the basement
(compressed memory, the Memory Compression process), the gallery (graphics
memory in RAM, per process from the GPU Process Memory counters), VM hosts
as tenants billed for their whole working set (`vm_hosts` in the rules,
HP Sure Click included), the Staff card with Staff's parts
(`StaffParts`, cut so they sum to Staff), and the details card's lease
breakdown (in RAM, gallery, boxes share, on paper). The building sums
Staff, rent, gallery, boxes and Vacant to capacity.
`crates/app/src/win/counters.rs` reads the performance counters through
pdh.dll by their English names.

Timelines (`fixtures/timelines/*.json`, format in
`crates/core/tests/timeline/mod.rs`) describe hours of samples, sleeps and
the flags expected after each step. Each flag's timeline has a sleep placed
so the test fails if a gap ever counts as watched time.

Check a live dump from Windows (CI does this on the Windows runner):

```bash
RAMLORD_DUMP=dump.json cargo test -p ramlord-core --test live_dump -- --nocapture
```

Build the Windows exe in the session (needs `x86_64-w64-mingw32-gcc`):

```bash
cargo build --release --target x86_64-pc-windows-gnu
```

The exe exits with 0 on success, 2 for a usage error, 3 when the input
cannot be read or is invalid, 4 when the output cannot be written, 5 for a
mode not available on this platform, 6 when the window fails and 7 when
`--self-test` finds a broken budget (the JSON report says which).

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
- Until M5, a live building's price line says `RAM` at the DDR5 price
  (`Lease::placeholder`), and F2 and F3 do nothing.
- The live app keeps `state.json` in `%APPDATA%\RAMlord\` (hues, watched
  time, flags' counters) and the Family list in `settings.json` beside it.
  It keeps the other keys in `settings.json`, so M5's settings can share
  the file. `--dump`, `--snapshot` and `--self-test` never read or write
  either file.
- Eviction never happens without the user: one confirmation, a polite
  `WM_CLOSE` first, and Esc in any dialog changes nothing. Keep it that way.

## The six verification rungs

| Rung | Runs | Check |
| --- | --- | --- |
| 1 | Session | `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` |
| 2 | Session | Render a fixture to PNG (command above), open it and compare with `docs/reference/` |
| 3 | Session | `cargo check --workspace --all-targets --target x86_64-pc-windows-gnu` |
| 4 | Session | Build the exe with MinGW and run `--demo` under `xvfb-run wine` with a screenshot (below) |
| 5 | GitHub Actions `windows-latest` | Release build, `cargo test`, render the fixture with the exe and compare with the golden PNGs, `ramlord --self-test`, `ramlord --dump` checked by `tests/live_dump.rs` |
| 6 | The owner, on a Windows PC | The checklist in `SPEC.md` |

Rung 4, adapted from `docs/reference/wine-probe/run.sh`. The live app with
Notepad running (drop `WINE_ALSO` and add `--demo` for the sample building):

```bash
cargo build --release --target x86_64-pc-windows-gnu
WINEPREFIX=$PWD/scripts/prefix WINEDEBUG=-all WINEDLLOVERRIDES="mscoree,mshtml=" WINE_ALSO=notepad \
  xvfb-run -a -s "-screen 0 1920x1080x24" \
  scripts/wine-run.sh target/x86_64-pc-windows-gnu/release/ramlord.exe shot.png
```

Evicting Notepad from the running app (M4). Add `force` to give Notepad
unsaved text first and force it out after it refuses. It needs `xdotool`:

```bash
WINEPREFIX=$PWD/scripts/prefix WINEDEBUG=-all WINEDLLOVERRIDES="mscoree,mshtml=" \
  xvfb-run -a -s "-screen 0 1920x1080x24" \
  scripts/wine-evict.sh target/x86_64-pc-windows-gnu/release/ramlord.exe shots/
```

`--dump` and `--self-test` run under Wine too (`wine ramlord.exe --dump
out.json` inside `xvfb-run`). Wine reports no private working set, so rent
there is estimated and amounts show a `~`.

Tools for rungs 3 and 4, if missing:

```bash
rustup target add x86_64-pc-windows-gnu
apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends gcc-mingw-w64-x86-64 wine wine64 xvfb imagemagick xdotool
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
