# RAMlord

<img src="fixtures/golden/app-icon.png" alt="" width="64" align="right">

Your apps have been living rent-free in your RAM.

RAMlord is a small Windows desktop app that draws your PC's memory as a
pixel-art apartment tower. Every running app is a tenant, sized by the memory
it holds and billed at the current street price of RAM. It is free, needs no
account, sends no telemetry and asks for no admin rights.

![The sample building](fixtures/golden/sample-tower.png)

## Download

Get `ramlord.exe` from the [Releases page](https://github.com/stef-al/ramlord/releases).
It is one file for 64-bit Windows 10 and 11, with no installer. Put it in any
folder and run it.

**Check the download first.** Each release lists the SHA-256 of its
`ramlord.exe` and has it in `ramlord.exe.sha256`. In PowerShell, in the folder
with the exe, this must print `True` (paste the hash from the release page):

```powershell
(Get-FileHash .\ramlord.exe -Algorithm SHA256).Hash -eq "PASTE-THE-HASH-HERE"
```

From Command Prompt, `certutil -hashfile ramlord.exe SHA256` prints the hash to
compare by eye. On Linux, macOS or Git Bash, `sha256sum -c ramlord.exe.sha256`
next to both files prints `ramlord.exe: OK`.

**Windows will warn you.** The exe is not signed yet, so SmartScreen says
"Windows protected your PC". Choose More info, then Run anyway, once you have
checked the hash. Every release is built by GitHub Actions from the tagged
source in this repository; the release notes link to the run.

## Using it

- Click a tenant, or use Up, Down and Enter, to open its details card. From
  there you can mark it Family (never flagged) or evict it with Delete.
- Eviction asks first, then asks the app to close the way its own close button
  would. It forces the app out only if the app refuses and you say so. Esc
  changes nothing.
- F2 saves the rent roll card to `Pictures\RAMlord` and copies it, ready to
  paste into a chat.
- F3 opens the settings: scale, always on top, what the close button does,
  start with Windows, your memory type and price (or your own price and
  currency symbol), fetching prices, the away threshold, Family and an
  anonymous rent roll.
- The close button hides RAMlord in the tray. Quit is in the tray menu. Only
  one copy runs; starting it again brings the window back.

The tags: AWAY is a window you have not looked at for 4 hours, WALLS an app
with no window, SQUAT an app that started with Windows and was never opened,
and LEAK an app whose memory keeps growing.

## What RAMlord reads

- **The process list.** For every process: its name, memory numbers (private
  memory in RAM, working set, commit), parent, session and start time, as
  Task Manager reads them.
- **Your apps.** For processes in your session: the path of the program, the
  description inside its exe file, and whether it runs as you. Each process
  is looked at once, when it first appears.
- **Windows.** Which window has focus, once a second, and which windows are
  visible, every 5 seconds. Never their titles or contents.
- **Memory totals.** Installed and available memory, commit, and Windows'
  memory and graphics memory performance counters.
- **Your memory type.** DDR4, DDR5 and so on, from the firmware's SMBIOS
  table, to pick the price.

## What it writes and sends

- `%APPDATA%\RAMlord\settings.json` and `state.json`: your settings, the
  tenants' colours and the counters behind the tags, nothing older than 7 days.
  `prices.json` there is the day's price file.
- `Pictures\RAMlord\rent-roll-*.png` and the clipboard, only when you press F2.
- The value `RAMlord` under `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run`,
  only while Start with Windows is on in the settings.
- **One network request, at most once a day:** it downloads
  [`prices.json`](prices.json) from `raw.githubusercontent.com`, as the agent
  `RAMlord`, sending nothing about you or your PC. Turn off Fetch prices
  online in the settings and it never connects; the prices built into the exe
  stay.

## What it never does

- It never asks for administrator rights and never changes Windows settings.
- It never closes an app without your confirmation, never closes several at
  once and never closes anything on its own. Before it closes a process it
  checks that it is still the one you chose, not a newer one that got the
  same process number.
- It never reads window titles, command lines, your files or what is inside
  another program's memory.
- It never switches other apps' startup entries; for a squatter it opens
  Windows' own Startup settings.
- It has no account, telemetry, ads or auto-update, and no button that
  pretends to "free" or "boost" memory: closing an app is the only thing that
  frees it, and RAMlord says so.

## Help with a wrong bill

`ramlord --dump my-pc.json` writes what RAMlord sees: process names, memory
numbers and program paths, with your profile folder replaced by
`%USERPROFILE%`, and no window titles, command lines or user names. It also
lists the kernel memory pool tags holding the most and which loaded drivers
contain each tag, to find a driver that hoards memory. Read it before you
share it. `ramlord --self-test report.json` checks RAMlord's own
budgets: under 40 MB of memory, under 1% CPU.

## Build

```bash
cargo test --workspace
cargo run -p ramlord -- --building fixtures/sample-building.json --render tower.png --card rent-roll.png
```

On Windows, `cargo build --release` produces `target/release/ramlord.exe`,
with its icon. [SPEC.md](SPEC.md) is the full design, and
[CLAUDE.md](CLAUDE.md) lists every check.

## License

GPL-3.0-only. See [LICENSE](LICENSE).
