# Evidence

Screenshots and outputs that back a milestone's "done when" boxes.

| File | What it shows |
| --- | --- |
| `m1-wine-demo.png` | Rung 4 for M1: `ramlord --demo`, built with MinGW, running under Wine 9.0 and Xvfb (1920 by 1080, no window manager, so no title bar). The window's client area at (3, 29) is `fixtures/golden/sample-tower.png` pixel for pixel. Made with `scripts/wine-demo.sh`; the demo exited with code 0 on `WM_CLOSE`. |
