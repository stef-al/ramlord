# Evidence

Screenshots and outputs that back a milestone's "done when" boxes.

| File | What it shows |
| --- | --- |
| `m1-wine-demo.png` | Rung 4 for M1: `ramlord --demo`, built with MinGW, running under Wine 9.0 and Xvfb (1920 by 1080, no window manager, so no title bar). The window's client area at (3, 29) is `fixtures/golden/sample-tower.png` pixel for pixel. Made with `scripts/wine-demo.sh`; the demo exited with code 0 on `WM_CLOSE`. |
| `m2-wine-live.png` | Rung 4 for M2: the live app (`ramlord` with no arguments) under Wine 9.0 with Notepad and Minesweeper running. Wine reports no private working set, so rent is estimated and marked `~`; Wine's own processes are Staff, and the two small apps merge into Others. Made with `WINE_ALSO="notepad winemine" scripts/wine-run.sh`; the app exited with code 0 on `WM_CLOSE`. |
| `m2-wine-dump.json` | `ramlord --dump` under Wine: 11 processes, `rent_is_estimate: true`, RAMlord found as `is_self`. |
| `m2-wine-self-test.json` | `ramlord --self-test` under Wine: every budget passes (exe 765 KB, own working set 19 MB, CPU 0.04% of the machine, slowest snapshot 32 ms, slowest frame 0.7 ms). |
