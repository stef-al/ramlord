# RAMlord

Your apps have been living rent-free in your RAM.

RAMlord is a small Windows desktop app that draws your PC's memory as a
pixel-art apartment tower. Every running app is a tenant, sized by the memory
it holds and billed at the current street price of RAM. It is free, needs no
account, sends no telemetry and asks for no admin rights.

![The sample building](fixtures/golden/sample-tower.png)

**Status:** early development. The tower and the rent roll card can be drawn
from a fixture file, and on Windows `ramlord --demo` shows the sample building
in a window. Live memory readings come next. See [SPEC.md](SPEC.md) for the
full design and the milestones.

## Build

```bash
cargo test --workspace
cargo run -p ramlord -- --building fixtures/sample-building.json --render tower.png --card rent-roll.png
```

On Windows, `cargo build --release` produces `target/release/ramlord.exe`;
`ramlord --demo` opens the sample building.

## License

GPL-3.0-only. See [LICENSE](LICENSE).
