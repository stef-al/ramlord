//! Grouping tests on hand-written snapshots (M2): a browser with 40
//! children, a Store app, a console app under Terminal, a game started by
//! Steam, WebView2 helpers and a process that cannot be opened.

use ramlord_core::building::Price;
use ramlord_core::rules::Rules;
use ramlord_core::snapshot::{Process, Snapshot};
use ramlord_core::tenants::{Lease, Role, building, group};

const MIB: u64 = 1 << 20;
const GIB: u64 = 1 << 30;
const T0: u64 = 1_794_268_800_000;

const SVCHOST: &str = r"C:\Windows\System32\svchost.exe";
const EXPLORER: &str = r"C:\Windows\explorer.exe";
const CHROME: &str = r"C:\Program Files\Google\Chrome\Application\chrome.exe";

/// A hand-written PC: 16 GiB, 6 GiB available, with the usual system
/// processes. Processes start in pid order unless a test says otherwise.
struct Pc(Snapshot);

impl Pc {
    fn new() -> Pc {
        let mut pc = Pc(Snapshot {
            at_ms: T0,
            capacity_bytes: 16 * GIB,
            available_bytes: 6 * GIB,
            commit_used_bytes: 12 * GIB,
            commit_limit_bytes: 24 * GIB,
            signed_in_at_ms: T0 - 3_600_000,
            foreground_pid: None,
            rent_is_estimate: false,
            processes: Vec::new(),
        });
        pc.add(4, 0, None, 0).session = 0; // System: cannot be opened
        pc.add(50, 4, Some(SVCHOST), 30).session = 0;
        pc.add(100, 90, Some(EXPLORER), 120).has_visible_window = true;
        pc
    }

    fn add(&mut self, pid: u32, parent: u32, path: Option<&str>, rent_mib: u64) -> &mut Process {
        self.0.processes.push(Process {
            pid,
            parent_pid: parent,
            started_at_ms: T0 - 3_000_000 + u64::from(pid) * 10,
            session: 1,
            image_path: path.map(String::from),
            description: None,
            rent_bytes: rent_mib * MIB,
            lease_bytes: rent_mib * MIB * 3 / 2,
            has_visible_window: false,
            is_self: false,
        });
        self.0.processes.last_mut().unwrap()
    }

    /// (tenant name, process count, rent in MiB), by name.
    fn tenants(&self) -> Vec<(String, u32, u64)> {
        let mut t: Vec<_> = group(&self.0, &Rules::builtin())
            .tenants
            .into_iter()
            .map(|t| (t.name, t.process_count, t.rent_bytes / MIB))
            .collect();
        t.sort();
        t
    }

    fn role(&self, pid: u32) -> Role {
        group(&self.0, &Rules::builtin()).roles[&pid]
    }

    fn tenant_name_of(&self, pid: u32) -> Option<String> {
        group(&self.0, &Rules::builtin())
            .tenant_of(pid)
            .map(|t| t.name.clone())
    }
}

fn row(name: &str, count: u32, mib: u64) -> (String, u32, u64) {
    (name.to_string(), count, mib)
}

#[test]
fn a_browser_with_40_children_is_one_tenant() {
    let mut pc = Pc::new();
    pc.add(200, 100, Some(CHROME), 300).has_visible_window = true;
    for pid in 201..=240 {
        pc.add(pid, 200, Some(CHROME), 100);
    }
    pc.add(
        250,
        200,
        Some(r"C:\Program Files\Google\Chrome\Application\crashpad_handler.exe"),
        5,
    );
    assert_eq!(pc.tenants(), [row("Chrome", 42, 300 + 40 * 100 + 5)]);
    let g = group(&pc.0, &Rules::builtin());
    assert_eq!(g.tenants[0].id, CHROME.to_lowercase());
    assert_eq!(pc.role(240), Role::Root(200));
}

#[test]
fn a_store_app_is_its_own_tenant_and_its_frame_host_is_staff() {
    let mut pc = Pc::new();
    let app = r"C:\Program Files\WindowsApps\Microsoft.WindowsCalculator_11.2405.2.0_x64__8wekyb3d8bbwe\CalculatorApp.exe";
    // Store apps start under svchost in session 0; the provider credits the
    // ApplicationFrameWindow to the app's own process.
    let calc = pc.add(300, 50, Some(app), 40);
    calc.description = Some("Calculator".into());
    calc.has_visible_window = true;
    pc.add(
        310,
        50,
        Some(r"C:\Windows\System32\ApplicationFrameHost.exe"),
        25,
    );
    pc.add(320, 50, Some(r"C:\Windows\System32\RuntimeBroker.exe"), 8);
    assert_eq!(pc.tenants(), [row("Calculator", 1, 40)]);
    assert_eq!(pc.role(310), Role::Staff);
    assert_eq!(pc.role(320), Role::Staff);
}

#[test]
fn a_console_app_under_terminal_is_its_own_tenant() {
    let mut pc = Pc::new();
    let dir =
        r"C:\Program Files\WindowsApps\Microsoft.WindowsTerminal_1.21.3231.0_x64__8wekyb3d8bbwe";
    pc.add(400, 100, Some(&format!(r"{dir}\WindowsTerminal.exe")), 90)
        .has_visible_window = true;
    pc.add(401, 400, Some(&format!(r"{dir}\OpenConsole.exe")), 6);
    pc.add(
        402,
        400,
        Some(r"C:\Program Files\PowerShell\7\pwsh.exe"),
        70,
    )
    .description = Some("pwsh".into());
    pc.add(
        403,
        402,
        Some(r"C:\Users\ada\AppData\Local\Programs\Python\Python313\python.exe"),
        45,
    )
    .description = Some("Python".into());
    // A classic console: conhost follows cmd, although it lives in the
    // Windows directory and owns no visible window.
    pc.add(410, 100, Some(r"C:\Windows\System32\cmd.exe"), 4)
        .description = Some("Windows Command Processor".into());
    pc.add(411, 410, Some(r"C:\Windows\System32\conhost.exe"), 7);
    assert_eq!(
        pc.tenants(),
        [
            row("Python", 1, 45),
            row("Terminal", 2, 96),
            row("Windows Command Processor", 2, 11),
            row("pwsh", 1, 70),
        ]
    );
    assert_eq!(pc.tenant_name_of(401).as_deref(), Some("Terminal"));
}

#[test]
fn a_game_started_by_steam_is_its_own_tenant_and_steam_keeps_its_helpers() {
    let mut pc = Pc::new();
    let steam = r"C:\Program Files (x86)\Steam";
    pc.add(500, 100, Some(&format!(r"{steam}\steam.exe")), 150)
        .has_visible_window = true;
    pc.add(
        501,
        500,
        Some(&format!(r"{steam}\bin\cef\cef.win7x64\steamwebhelper.exe")),
        120,
    );
    pc.add(
        502,
        501,
        Some(&format!(r"{steam}\bin\cef\cef.win7x64\steamwebhelper.exe")),
        80,
    );
    let game = pc.add(
        510,
        500,
        Some(&format!(r"{steam}\steamapps\common\Hades\x64\Hades.exe")),
        2_000,
    );
    game.description = Some("Hades".into());
    game.has_visible_window = true;
    pc.add(511, 510, Some(&format!(r"{steam}\gameoverlayui.exe")), 30);
    pc.add(520, 50, Some(&format!(r"{steam}\bin\steamservice.exe")), 10)
        .session = 0;
    assert_eq!(pc.tenants(), [row("Hades", 1, 2_000), row("Steam", 4, 380)]);
    assert_eq!(pc.role(520), Role::Staff);
}

#[test]
fn webview2_helpers_join_their_host() {
    let mut pc = Pc::new();
    let webview = r"C:\Program Files (x86)\Microsoft\EdgeWebView\Application\131.0.2903.70\msedgewebview2.exe";
    pc.add(600, 100, Some(r"C:\Program Files\WindowsApps\MSTeams_24295.605.3225.8804_x64__8wekyb3d8bbwe\ms-teams.exe"), 200)
        .has_visible_window = true;
    pc.add(601, 600, Some(webview), 250);
    for pid in 602..=604 {
        pc.add(pid, 601, Some(webview), 100);
    }
    let outlook = pc.add(700, 100, Some(r"C:\Program Files\WindowsApps\Microsoft.OutlookForWindows_1.0_x64__8wekyb3d8bbwe\olk.exe"), 150);
    outlook.description = Some("Microsoft Outlook".into());
    pc.add(701, 700, Some(webview), 180);
    assert_eq!(pc.tenants(), [row("Outlook", 2, 330), row("Teams", 5, 750)]);
}

#[test]
fn a_process_that_cannot_be_opened_is_staff() {
    let mut pc = Pc::new();
    pc.add(800, 100, None, 0);
    // Its child cannot climb into Staff, so it roots its own tenant.
    pc.add(801, 800, Some(r"C:\Tools\agent.exe"), 12);
    assert_eq!(pc.role(800), Role::Staff);
    assert_eq!(pc.role(4), Role::Staff);
    assert_eq!(pc.tenants(), [row("agent", 1, 12)]);
}

#[test]
fn a_reused_parent_pid_does_not_adopt_older_children() {
    let mut pc = Pc::new();
    pc.add(900, 100, Some(r"C:\Apps\new-parent.exe"), 10);
    let orphan = pc.add(901, 900, Some(r"C:\Apps\orphan.exe"), 20);
    orphan.started_at_ms = T0 - 4_000_000; // older than pid 900
    assert_eq!(pc.role(901), Role::Root(901));
}

#[test]
fn the_windows_directory_needs_a_visible_window_to_be_a_tenant() {
    let mut pc = Pc::new();
    pc.add(950, 100, Some(r"C:\Windows\System32\notepad.exe"), 20)
        .has_visible_window = true;
    pc.add(
        951,
        100,
        Some(r"C:\Windows\System32\backgroundhelper.exe"),
        15,
    );
    assert_eq!(pc.role(951), Role::Staff);
    assert_eq!(pc.tenants(), [row("notepad", 1, 20)]);
}

#[test]
fn ramlord_is_never_a_tenant_and_its_memory_is_staff() {
    let mut pc = Pc::new();
    pc.add(990, 100, Some(r"C:\Tools\ramlord.exe"), 31).is_self = true;
    pc.add(200, 100, Some(CHROME), 500);
    let g = group(&pc.0, &Rules::builtin());
    assert_eq!(g.roles[&990], Role::Staff);
    assert_eq!(g.own_bytes, 31 * MIB);
    assert_eq!(pc.tenants(), [row("Chrome", 1, 500)]);
}

#[test]
fn a_helper_whose_owner_is_absent_follows_the_usual_rules() {
    let mut pc = Pc::new();
    pc.add(530, 100, Some(r"C:\Steam\bin\steamwebhelper.exe"), 50);
    assert_eq!(pc.role(530), Role::Root(530));
}

#[test]
fn staff_tenants_and_vacant_sum_to_capacity() {
    let mut pc = Pc::new();
    pc.add(200, 100, Some(CHROME), 3_000);
    pc.add(201, 200, Some(CHROME), 1_000);
    pc.add(990, 100, Some(r"C:\Tools\ramlord.exe"), 31).is_self = true;
    let lease = Lease {
        memory_type: "RAM".into(),
        price: Price::placeholder(),
        remark: String::new(),
    };
    let b = building(&pc.0, &Rules::builtin(), &lease);
    b.validate().expect("the building balances");
    assert_eq!(
        b.staff_bytes + b.rent_bytes() + b.vacant_bytes,
        b.capacity_bytes
    );
    assert_eq!(b.vacant_bytes, 6 * GIB);
    assert_eq!(b.rent_bytes(), 4_000 * MIB);
    assert_eq!(b.own_bytes, 31 * MIB);
    assert_eq!(b.date, "2026-11-10");
}

#[test]
fn estimated_rent_can_overfill_the_building() {
    let mut pc = Pc::new();
    pc.0.rent_is_estimate = true;
    pc.add(200, 100, Some(CHROME), 17 * 1024);
    let lease = Lease {
        memory_type: "RAM".into(),
        price: Price::placeholder(),
        remark: String::new(),
    };
    let b = building(&pc.0, &Rules::builtin(), &lease);
    assert_eq!((b.staff_bytes, b.vacant_bytes), (0, 0));
    assert!(b.rent_is_estimate);
    b.validate().expect("an overfull building is valid");
}
