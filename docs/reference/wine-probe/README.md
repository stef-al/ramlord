# Wine probe

`probe.c` tries the Win32 calls RAMlord needs and writes what happened to `report.txt`. It was built with MinGW and run under Wine 9.0 and Xvfb on Ubuntu 24.04. The report and screenshot here are from a run on 10 November 2026.

| File | What it is |
| --- | --- |
| `probe.c` | The program. It reads memory totals, lists processes, starts Notepad twice, closes one politely and one by force, then opens a window and paints `frame.bin`. |
| `run.sh` | Starts the probe under Wine and takes a screenshot while its window is open. |
| `frame.bin` | `../tower.png` shrunk to 384 by 240 as raw 32-bit BGR pixels, top row first. |
| `report.txt` | The probe's output from that run. |
| `shot.png` | The screenshot from that run: the tower painted by `StretchDIBits` at 3 times scale. |
| `apt.log` | The apt output from installing MinGW and Wine. |

## Run it again

As root on Ubuntu 24.04, in this folder:

```bash
apt-get install -y --no-install-recommends gcc-mingw-w64-x86-64 wine wine64 xvfb imagemagick
x86_64-w64-mingw32-gcc -O2 -municode -mwindows probe.c -o probe.exe -lpsapi -lgdi32 -luser32 -lkernel32
export WINEPREFIX=$PWD/prefix WINEDEBUG=-all WINEDLLOVERRIDES="mscoree,mshtml="
xvfb-run -a -s "-screen 0 1400x900x24" ./run.sh
```

The first run takes under a minute because Wine sets up `prefix/`. `probe.exe` and `prefix/` are not committed.

To rebuild `frame.bin` from the reference tower (needs Pillow):

```python
from PIL import Image
im = Image.open("../tower.png").convert("RGB").resize((384, 240), Image.NEAREST)
rgb = im.tobytes()
data = b"".join(bytes((rgb[i + 2], rgb[i + 1], rgb[i], 0)) for i in range(0, len(rgb), 3))
open("frame.bin", "wb").write(data)
```

## What the run showed

- These worked: `GetPerformanceInfo`, `GetPhysicallyInstalledSystemMemory`, the process list with parents, image paths, start times and session ids, `EnumWindows`, the foreground window, `WM_CLOSE`, `TerminateProcess`, `CreateWindowExW`, `StretchDIBits`, a named mutex and the `CF_DIB` clipboard.
- `GetSystemFirmwareTable` reported an SMBIOS table of 131 bytes. The probe does not parse it.
- `PrivateWorkingSetSize` is 0 for every process under Wine, while `WorkingSetSize` and `PagefileUsage` are filled.
- `SetProcessDpiAwarenessContext` fails under Wine.
- The last report line counts 0 `WM_PAINT` messages. That counter sees only queued messages; the screenshot shows the window did paint.
