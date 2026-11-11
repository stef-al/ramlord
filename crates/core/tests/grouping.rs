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
            ..Default::default()
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
            ..Default::default()
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

#[test]
fn a_vm_host_is_a_tenant_even_in_session_0_and_unopened() {
    let mut pc = Pc::new();
    pc.add(1200, 50, Some(r"C:\Windows\System32\vmcompute.exe"), 20)
        .session = 0;
    let vm = pc.add(1210, 1200, None, 2048);
    vm.session = 0;
    vm.image_name = Some("vmmem".into());
    let zygote = pc.add(1220, 1200, None, 0);
    zygote.session = 0;
    zygote.image_name = Some("vmmemCmZygote".into());
    assert_eq!(
        pc.tenants(),
        [row("Sandbox", 1, 0), row("Virtual machine", 1, 2048)]
    );
    assert_eq!(pc.role(1200), Role::Staff);
    let g = group(&pc.0, &Rules::builtin());
    assert_eq!(g.tenant_of(1210).unwrap().id, "vmmem");
}

/// Graphics memory in RAM is the gallery: each tenant's from its
/// processes, the total from the adapters, apart from Staff. Staff's parts
/// come from the counters, and the rest is SHARED, OTHER.
#[test]
fn graphics_memory_hangs_in_the_gallery_and_staff_breaks_down() {
    let mut pc = Pc::new();
    pc.0.physical_total_bytes = 16 * GIB - 368 * MIB;
    pc.0.kernel_nonpaged_bytes = 400 * MIB;
    pc.0.kernel_paged_resident_bytes = 300 * MIB;
    pc.0.driver_bytes = 60 * MIB;
    pc.0.kernel_code_bytes = 20 * MIB;
    pc.0.file_cache_bytes = 250 * MIB;
    pc.0.modified_bytes = 100 * MIB;
    pc.0.gpu_shared_bytes = 1600 * MIB; // more than the processes' sum
    pc.add(200, 100, Some(CHROME), 900);
    // Its GPU process pays commit for its pictures.
    let gpu = pc.add(201, 200, Some(CHROME), 60);
    gpu.gpu_shared_bytes = 1000 * MIB;
    gpu.lease_bytes = 1100 * MIB;
    let dwm = pc.add(90, 4, Some(r"C:\Windows\System32\dwm.exe"), 150);
    dwm.gpu_shared_bytes = 300 * MIB;
    dwm.lease_bytes = 500 * MIB;
    let g = group(&pc.0, &Rules::builtin());
    assert_eq!(g.gallery_bytes, 1600 * MIB);
    let chrome = g.tenant_of(200).unwrap();
    assert_eq!(chrome.gpu_bytes, 1000 * MIB);
    let b = building(&pc.0, &Rules::builtin(), &Lease::placeholder());
    b.validate().expect("the gallery is part of the sum");
    assert_eq!(b.gallery_bytes, 1600 * MIB);
    // Used memory is 10 GiB: Chrome's 960 MiB rent and the gallery come
    // out of what used to be Staff.
    assert_eq!(b.staff_bytes, 10 * GIB - 960 * MIB - 1600 * MIB);
    let parts = &b.staff_parts;
    assert_eq!(parts.services_bytes, (30 + 120 + 150) * MIB); // svchost, explorer, dwm
    assert_eq!(parts.hardware_bytes, 368 * MIB);
    assert_eq!(parts.kernel_pool_bytes, 700 * MIB);
    assert_eq!(parts.drivers_bytes, 80 * MIB);
    assert_eq!(parts.file_cache_bytes, 250 * MIB);
    assert_eq!(parts.modified_bytes, 100 * MIB);
    assert_eq!(parts.sum(), b.staff_bytes);
}

/// From the owner's second dump: Windows' Shared Usage counts what a
/// process can see, so the desktop compositor reported 11,458 MiB on a
/// 16 GiB PC whose adapter held 2,759 MiB. What a process holds is capped
/// at its commit beyond its working set; the adapter's total is the
/// gallery, which never squeezes Vacant.
#[test]
fn graphics_memory_is_what_a_process_pays_commit_for() {
    let mut pc = Pc::new();
    pc.0.available_bytes = 743 * MIB;
    pc.0.gpu_shared_bytes = 2759 * MIB;
    let mut gpu = |pid: u32, parent: u32, path: &str, private: u64, commit: u64, shared: u64| {
        let p = pc.add(pid, parent, Some(path), private);
        p.lease_bytes = commit * MIB;
        p.gpu_shared_bytes = shared * MIB;
        p.has_visible_window = true;
    };
    gpu(1724, 1112, r"C:\Windows\System32\dwm.exe", 175, 510, 11458);
    gpu(44852, 100, r"C:\Windows\System32\mstsc.exe", 23, 568, 373);
    gpu(
        22292,
        100,
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        237,
        1207,
        302,
    );
    let g = group(&pc.0, &Rules::builtin());
    assert_eq!(g.gallery_bytes, 2759 * MIB, "the adapter's measure");
    // dwm sees 11,458 MiB but pays commit for 335.
    assert_eq!(g.gpu_owned_bytes, (335 + 373 + 302) * MIB);
    let edge = g.tenant_of(22292).unwrap();
    assert_eq!(edge.gpu_bytes, 302 * MIB);
    // Its packed memory leaves its pictures out: 1,207 - 237 - 302.
    assert_eq!(edge.packed_bytes, 668 * MIB);
    let b = building(&pc.0, &Rules::builtin(), &Lease::placeholder());
    b.validate().unwrap();
    assert_eq!(
        b.vacant_bytes,
        743 * MIB,
        "the gallery never squeezes Vacant"
    );
    assert_eq!(b.gallery_bytes, 2759 * MIB);
    assert_eq!(
        b.tenants.iter().map(|t| t.gpu_bytes).sum::<u64>(),
        (373 + 302) * MIB
    );
}

/// A gallery larger than what Staff leaves is cut to fit, and the
/// tenants' pictures with it; Vacant stays as measured.
#[test]
fn the_gallery_comes_out_of_staff_only() {
    let mut pc = Pc::new();
    pc.0.gpu_shared_bytes = 20 * GIB; // nonsense, as the per-process sum was
    let edge = pc.add(200, 100, Some(CHROME), 1000);
    edge.lease_bytes = 9 * GIB;
    edge.gpu_shared_bytes = 8 * GIB;
    let b = building(&pc.0, &Rules::builtin(), &Lease::placeholder());
    b.validate().unwrap();
    assert_eq!(b.vacant_bytes, 6 * GIB);
    assert_eq!(b.staff_bytes, 0);
    assert_eq!(b.gallery_bytes, 10 * GIB - 1000 * MIB);
    assert!(b.tenants[0].gpu_bytes <= b.gallery_bytes);
}

/// Processes Windows will not open have no image path; the process list
/// still names them (csrss.exe, winlogon.exe, an audio service). They are
/// Staff, as they were before RAMlord read the process list.
#[test]
fn a_process_that_cannot_be_opened_is_staff_though_named() {
    let mut pc = Pc::new();
    let csrss = pc.add(1048, 1028, None, 1);
    csrss.image_name = Some("csrss.exe".into());
    let audio = pc.add(11768, 100, None, 2);
    audio.image_name = Some("RtkAudUService64.exe".into());
    assert_eq!(pc.role(1048), Role::Staff);
    assert_eq!(pc.role(11768), Role::Staff);
    assert!(pc.tenants().is_empty());
}

/// From the owner's third dump: Sure Click's manager in session 0 (not
/// opened, 1,187 MiB in RAM, 7 MiB committed) and a micro-VM in the user's
/// session (opened, under BrHostSvr) are one tenant, which counts the
/// template's memory once.
#[test]
fn sure_click_is_one_tenant_and_counts_its_template_once() {
    let mut pc = Pc::new();
    pc.add(6760, 50, None, 20).image_name = Some("BrService.exe".into());
    pc.0.processes.last_mut().unwrap().session = 0;
    let manager = pc.add(22576, 6760, None, 0);
    manager.session = 0;
    manager.image_name = Some("Br-uxendm.exe".into());
    manager.working_set_bytes = 1187 * MIB;
    manager.lease_bytes = 7 * MIB;
    let host = pc.add(
        44324,
        6760,
        Some(r"C:\Program Files\HP\Sure Click\servers\BrHostSvr.exe"),
        54,
    );
    host.lease_bytes = 65 * MIB;
    let vm = pc.add(
        26572,
        44324,
        Some(r"C:\Program Files\HP\Sure Click\servers\Br-uxendm.exe"),
        355,
    );
    vm.working_set_bytes = 1398 * MIB;
    vm.lease_bytes = 1656 * MIB;
    let g = group(&pc.0, &Rules::builtin());
    let sure_click: Vec<_> = g
        .tenants
        .iter()
        .filter(|t| t.name == "Sure Click")
        .collect();
    assert_eq!(sure_click.len(), 1, "one Sure Click");
    let t = sure_click[0];
    assert_eq!(t.process_count, 3);
    // The micro-VM maps the template's memory copy-on-write: its 1,043 MiB
    // shared working set are pages the manager's 1,187 MiB already hold.
    // Private memory, plus the largest shared working set once.
    assert_eq!(t.rent_bytes, (54 + 355 + 1187) * MIB);
    // The commits summed, which here exceed what it holds.
    assert_eq!(t.lease_bytes, (7 + 65 + 1656) * MIB);
    assert!(t.lease_bytes >= t.rent_bytes);
    // Commit beyond what each process holds: the micro-VM's.
    assert_eq!(t.packed_bytes, (65 - 54 + 1656 - 1398) * MIB);
}

/// A VM host's lease is at least what it holds, even when Windows charges
/// none of the guest's memory to its commit.
#[test]
fn a_vm_host_leases_at_least_what_it_holds() {
    let mut pc = Pc::new();
    pc.add(1400, 50, None, 17).image_name = Some("BrService.exe".into());
    pc.0.processes.last_mut().unwrap().session = 0;
    for pid in [1410, 1420] {
        let vm = pc.add(pid, 1400, None, 100);
        vm.session = 0;
        vm.image_name = Some("Br-uxendm.exe".into());
        vm.working_set_bytes = if pid == 1410 { 900 } else { 600 } * MIB;
        vm.lease_bytes = 7 * MIB;
    }
    let g = group(&pc.0, &Rules::builtin());
    let t = g.tenant_of(1410).unwrap();
    assert_eq!(t.process_count, 2);
    assert_eq!(t.rent_bytes, (100 + 100 + 800) * MIB);
    assert_eq!(t.lease_bytes, t.rent_bytes);
    assert_eq!(t.packed_bytes, 0);
}

/// From the owner's PC: HP Sure Click's micro-VM manager runs in session 0
/// under its service and holds 1,186 MiB in RAM, none of it private, with
/// 7 MiB committed. Its whole working set is the guests' rent.
#[test]
fn sure_click_micro_vms_are_a_tenant_billed_for_their_working_set() {
    let mut pc = Pc::new();
    pc.add(1400, 50, None, 17).image_name = Some("BrService.exe".into());
    pc.0.processes.last_mut().unwrap().session = 0;
    let vms = pc.add(1410, 1400, None, 0);
    vms.session = 0;
    vms.image_name = Some("Br-uxendm.exe".into());
    vms.working_set_bytes = 1186 * MIB;
    vms.lease_bytes = 7 * MIB;
    assert_eq!(pc.tenants(), [row("Sure Click", 1, 1186)]);
    assert_eq!(pc.role(1400), Role::Staff);
    let g = group(&pc.0, &Rules::builtin());
    let sure_click = g.tenant_of(1410).unwrap();
    assert_eq!(
        sure_click.packed_bytes, 0,
        "its lease is less than its rent"
    );
    let b = building(&pc.0, &Rules::builtin(), &Lease::placeholder());
    b.validate().expect("the guests' memory comes out of Staff");
    assert_eq!(b.staff_bytes, 16 * GIB - 6 * GIB - 1186 * MIB);
}

#[test]
fn memory_compression_becomes_boxes_and_tenants_show_what_they_packed() {
    let mut pc = Pc::new();
    let store = pc.add(1300, 4, None, 0);
    store.session = 0;
    store.image_name = Some("MemCompression".into());
    store.working_set_bytes = 1536 * MIB;
    let browser = pc.add(200, 100, Some(CHROME), 900);
    browser.lease_bytes = 2900 * MIB; // 2,000 MiB committed but not in RAM
    pc.add(201, 200, Some(CHROME), 100).lease_bytes = 50 * MIB; // more in RAM than committed
    let g = group(&pc.0, &Rules::builtin());
    assert_eq!(g.boxes_bytes, 1536 * MIB);
    assert_eq!(g.tenants[0].packed_bytes, 2000 * MIB);
    let lease = Lease {
        memory_type: "RAM".into(),
        price: Price::placeholder(),
        remark: String::new(),
    };
    let b = building(&pc.0, &Rules::builtin(), &lease);
    b.validate().expect("boxes are part of the sum");
    assert_eq!(b.boxes_bytes, 1536 * MIB);
    assert_eq!(
        b.staff_bytes + b.rent_bytes() + b.boxes_bytes + b.vacant_bytes,
        b.capacity_bytes
    );
    // The boxes come out of what used to be Staff.
    assert_eq!(b.staff_bytes, 16 * GIB - 6 * GIB - 1000 * MIB - 1536 * MIB);
}

/// Apps installed in folders named in any script are grouped, named and
/// drawn like any other: nothing reads a path by bytes.
#[test]
fn apps_in_folders_of_any_script_become_tenants() {
    let mut pc = Pc::new();
    let game = pc.add(700, 100, Some(r"D:\游戏\原神\YuanShen.exe"), 900);
    game.has_visible_window = true;
    pc.add(710, 100, Some(r"D:\ab\Программа\Блокнот.exe"), 200)
        .has_visible_window = true;
    pc.add(720, 100, Some(r"C:\Users\Zoë\Ünïcödé\日本語"), 100)
        .has_visible_window = true;
    // Under the Windows folder without a window: Staff, whatever its name.
    pc.add(730, 100, Some(r"C:\Windows\System32\字体.exe"), 10);
    assert_eq!(
        pc.tenants(),
        [
            row("YuanShen", 1, 900),
            row("Блокнот", 1, 200),
            row("日本語", 1, 100),
        ]
    );
    assert_eq!(pc.role(730), Role::Staff);
    let b = building(&pc.0, &Rules::builtin(), &Lease::placeholder());
    b.validate().expect("the building balances");
    // Drawn without a panic; names without glyphs show question marks.
    let (frame, _) = ramlord_core::render_tower(&b, &ramlord_core::HueBook::new());
    assert_eq!((frame.width, frame.height), (384, 240));
}

/// A tenant's id is never empty, whatever the snapshot holds: an empty id
/// would make the building invalid and freeze the window.
#[test]
fn an_empty_image_path_still_gives_a_tenant_an_id() {
    let mut pc = Pc::new();
    pc.add(800, 100, Some(""), 300).has_visible_window = true;
    let odd = pc.add(810, 100, Some(""), 200);
    odd.has_visible_window = true;
    odd.image_name = Some("odd.exe".into());
    let g = group(&pc.0, &Rules::builtin());
    let ids: Vec<&str> = g.tenants.iter().map(|t| t.id.as_str()).collect();
    // Without a path or a name it cannot be told apart: Staff. With a
    // name from the process list, the name is its id.
    assert_eq!(ids, ["odd.exe"]);
    assert_eq!(pc.role(800), Role::Staff);
    building(&pc.0, &Rules::builtin(), &Lease::placeholder())
        .validate()
        .expect("valid");
}
