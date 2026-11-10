//! From processes to tenants (SPEC.md, Tenants and rent): a Snapshot and
//! the rules in, tenant samples and a Building out.
//!
//! 1. Staff: a process that cannot be opened, runs in session 0, has an
//!    image name in `staff`, or is RAMlord itself. A process under the
//!    Windows directory is also Staff when it owns no visible window and
//!    is not in `launchers` (nor a `parent` helper, which follows its
//!    parent instead).
//! 2. Root: walk up the parent chain while the parent is alive, older than
//!    the child, not Staff and not in `launchers`.
//! 3. Tenant: processes whose roots share one image path; the id is that
//!    path in lower case.
//! 4. Helpers: `parent` follows the parent even if it is a launcher (Staff
//!    if the parent is Staff); an image such as `steam.exe` joins that
//!    tenant if it exists.
//!
//! Two additions the owner asked for: a virtual machine host (`vm_hosts`,
//! such as `vmmem`) is a tenant of its own even in session 0, since its
//! memory is a guest's, and its rent is its whole working set, since a
//! guest's memory is mapped rather than private (HP Sure Click's micro-VMs
//! hold over a gigabyte with no private memory at all); and the Memory
//! Compression process (`compression`)
//! holds tenants' pages compressed, which the tower shows as boxes in the
//! basement rather than as Staff.

use std::collections::BTreeMap;

use crate::building::{Building, Price, Tenant};
use crate::date::Date;
use crate::rules::{Helper, Rules};
use crate::snapshot::{Process, Snapshot};

/// Where a process ends up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Staff,
    /// Belongs to the tenant rooted at this pid.
    Root(u32),
}

/// One tenant at one moment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TenantSample {
    /// The root image path in lower case.
    pub id: String,
    pub name: String,
    pub rent_bytes: u64,
    pub lease_bytes: u64,
    pub process_count: u32,
    /// Member processes, lowest pid first.
    pub pids: Vec<u32>,
    /// Committed private memory that is not in RAM now: compressed in the
    /// basement, written to the page file, or never touched. Summed over
    /// the members of max(commit - private working set, 0). An estimate:
    /// Windows does not tell which pages of a process are compressed.
    pub packed_bytes: u64,
    /// When its oldest member started (0 when unknown).
    pub started_at_ms: u64,
    /// Any member owns a visible window.
    pub has_window: bool,
}

/// The result of grouping one snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grouping {
    /// Tenants by id.
    pub tenants: Vec<TenantSample>,
    /// The role of every process, by pid.
    pub roles: BTreeMap<u32, Role>,
    /// RAMlord's own rent: the `MY OFFICE` line. It is part of Staff.
    pub own_bytes: u64,
    /// The Memory Compression process's working set: the boxes.
    pub boxes_bytes: u64,
}

impl Grouping {
    pub fn tenant_of(&self, pid: u32) -> Option<&TenantSample> {
        self.tenants.iter().find(|t| t.pids.contains(&pid))
    }

    pub fn rent_bytes(&self) -> u64 {
        self.tenants.iter().map(|t| t.rent_bytes).sum()
    }
}

/// True for a path under a `?:\Windows\` folder or `%SystemRoot%\`.
fn under_windows_dir(path: &str) -> bool {
    let lower = path.to_lowercase().replace('/', "\\");
    let bytes = lower.as_bytes();
    let drive =
        bytes.len() > 11 && bytes[0].is_ascii_alphabetic() && &lower[1..11] == ":\\windows\\";
    drive || lower.starts_with("%systemroot%\\") || lower.starts_with("%windir%\\")
}

struct Grouper<'a> {
    rules: &'a Rules,
    by_pid: BTreeMap<u32, &'a Process>,
    roles: BTreeMap<u32, Role>,
    visiting: Vec<u32>,
}

impl<'a> Grouper<'a> {
    /// Staff no matter what its parent is.
    fn hard_staff(&self, p: &Process) -> bool {
        if self.is_vm_host(p) {
            return false;
        }
        p.is_self
            || p.session == 0
            || match p.exe() {
                None => true,
                Some(name) => self.rules.is_staff(&name),
            }
    }

    fn is_vm_host(&self, p: &Process) -> bool {
        p.exe().is_some_and(|n| self.rules.is_vm_host(&n))
    }

    /// The parent, if it is alive and older than `p` (otherwise the pid was
    /// reused, or the parent has exited).
    fn parent(&self, p: &Process) -> Option<&'a Process> {
        let parent = *self.by_pid.get(&p.parent_pid)?;
        (parent.pid != p.pid && parent.started_at_ms <= p.started_at_ms).then_some(parent)
    }

    fn role(&mut self, pid: u32) -> Role {
        if let Some(&role) = self.roles.get(&pid) {
            return role;
        }
        if self.visiting.contains(&pid) {
            // A parent cycle: stop here.
            return Role::Root(pid);
        }
        self.visiting.push(pid);
        let role = self.resolve(pid);
        self.visiting.retain(|&v| v != pid);
        self.roles.insert(pid, role);
        role
    }

    fn resolve(&mut self, pid: u32) -> Role {
        let p = self.by_pid[&pid];
        if self.hard_staff(p) {
            return Role::Staff;
        }
        if self.is_vm_host(p) {
            return Role::Root(pid);
        }
        let name = p.exe().unwrap_or_default();
        match self.rules.helper(&name).cloned() {
            Some(Helper::Parent) => {
                if let Some(parent) = self.parent(p) {
                    return self.role(parent.pid);
                }
            }
            Some(Helper::Join(owner)) => {
                if let Some(root) = self.owner_root(&owner) {
                    return Role::Root(root);
                }
            }
            None => {}
        }
        let windows_dir = p.image_path.as_deref().is_some_and(under_windows_dir);
        if windows_dir
            && !p.has_visible_window
            && !self.rules.is_launcher(&name)
            && self.rules.helper(&name) != Some(&Helper::Parent)
        {
            return Role::Staff;
        }
        if let Some(parent) = self.parent(p) {
            let launcher = parent.exe().is_some_and(|n| self.rules.is_launcher(&n));
            if !launcher && let Role::Root(root) = self.role(parent.pid) {
                return Role::Root(root);
            }
        }
        Role::Root(pid)
    }

    /// The root of the tenant whose root image is `owner`, if one exists:
    /// the first process with that image name, by start time then pid,
    /// that roots its own tenant.
    fn owner_root(&mut self, owner: &str) -> Option<u32> {
        let mut candidates: Vec<&Process> = self
            .by_pid
            .values()
            .copied()
            .filter(|p| p.exe().as_deref() == Some(owner))
            .collect();
        candidates.sort_by_key(|p| (p.started_at_ms, p.pid));
        for candidate in candidates {
            if let Role::Root(root) = self.role(candidate.pid)
                && self.by_pid[&root].exe().as_deref() == Some(owner)
            {
                return Some(root);
            }
        }
        None
    }
}

/// The display name of a tenant rooted at `root` (SPEC.md): from `names`,
/// else from the FileDescription without a leading company word, else
/// from the file name.
pub fn tenant_name(root: &Process, rules: &Rules) -> String {
    let image = root.exe().unwrap_or_default();
    if let Some(name) = rules.names.get(&image) {
        return name.clone();
    }
    // Trademark signs have no glyph and do not belong in a name.
    let description = root
        .description
        .as_deref()
        .map(|d| d.replace(['\u{ae}', '\u{2122}', '\u{a9}'], ""));
    let words: Vec<&str> = description
        .as_deref()
        .unwrap_or("")
        .split_whitespace()
        .collect();
    if let Some((first, rest)) = words.split_first() {
        if !rest.is_empty() && rules.companies.contains(&first.to_lowercase()) {
            return rest.join(" ");
        }
        return words.join(" ");
    }
    let stem = root
        .image_path
        .as_deref()
        .map(|p| p.rsplit(['\\', '/']).next().unwrap_or(p))
        .unwrap_or("?");
    match stem.len().checked_sub(4) {
        Some(cut) if stem[cut..].eq_ignore_ascii_case(".exe") => stem[..cut].to_string(),
        _ => stem.to_string(),
    }
}

/// Groups a snapshot's processes into tenants.
pub fn group(snapshot: &Snapshot, rules: &Rules) -> Grouping {
    let mut grouper = Grouper {
        rules,
        by_pid: snapshot.processes.iter().map(|p| (p.pid, p)).collect(),
        roles: BTreeMap::new(),
        visiting: Vec::new(),
    };
    let pids: Vec<u32> = grouper.by_pid.keys().copied().collect();
    for &pid in &pids {
        grouper.role(pid);
    }

    let mut tenants: BTreeMap<String, TenantSample> = BTreeMap::new();
    for &pid in &pids {
        let Role::Root(root) = grouper.roles[&pid] else {
            continue;
        };
        let root = grouper.by_pid[&root];
        // The root's image path in lower case; a process that was not
        // opened (a VM host) goes by its name.
        let id = root
            .image_path
            .as_deref()
            .map(str::to_lowercase)
            .or_else(|| root.exe())
            .unwrap_or_else(|| "?".into());
        let p = grouper.by_pid[&pid];
        let tenant = tenants.entry(id.clone()).or_insert_with(|| TenantSample {
            id,
            name: tenant_name(root, rules),
            rent_bytes: 0,
            lease_bytes: 0,
            process_count: 0,
            pids: Vec::new(),
            packed_bytes: 0,
            started_at_ms: 0,
            has_window: false,
        });
        let rent = if grouper.is_vm_host(p) {
            p.rent_bytes.max(p.working_set_bytes)
        } else {
            p.rent_bytes
        };
        tenant.rent_bytes += rent;
        tenant.lease_bytes += p.lease_bytes;
        tenant.process_count += 1;
        tenant.pids.push(pid);
        tenant.packed_bytes += p.lease_bytes.saturating_sub(rent);
        tenant.has_window |= p.has_visible_window;
        if p.started_at_ms > 0
            && (tenant.started_at_ms == 0 || p.started_at_ms < tenant.started_at_ms)
        {
            tenant.started_at_ms = p.started_at_ms;
        }
    }
    let own_bytes = snapshot
        .processes
        .iter()
        .filter(|p| p.is_self)
        .map(|p| p.rent_bytes)
        .sum();
    let boxes_bytes = snapshot
        .processes
        .iter()
        .filter(|p| p.exe().is_some_and(|n| rules.is_compression(&n)))
        .map(|p| p.rent_bytes.max(p.working_set_bytes))
        .sum();
    Grouping {
        tenants: tenants.into_values().collect(),
        roles: grouper.roles,
        own_bytes,
        boxes_bytes,
    }
}

/// Staff and Vacant from capacity, available memory and the tenants' rent
/// (SPEC.md, What is measured): Staff is what is left. If rent plus vacant
/// exceeds capacity, Staff is zero and Vacant shrinks until the three sum
/// to capacity. If rent alone exceeds capacity (possible only with
/// estimated rent), both are zero. Returns `(staff_bytes, vacant_bytes)`.
pub fn balance(capacity: u64, available: u64, rent: u64) -> (u64, u64) {
    let vacant = available.min(capacity.saturating_sub(rent));
    let staff = capacity.saturating_sub(rent).saturating_sub(vacant);
    (staff, vacant)
}

/// What a Building needs besides the snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lease {
    pub memory_type: String,
    pub price: Price,
    pub remark: String,
}

impl Lease {
    /// Until M5 detects the memory type and reads the price file, a live
    /// building says RAM at the DDR5 price; remarks come with M3.
    pub fn placeholder() -> Lease {
        Lease {
            memory_type: "RAM".into(),
            price: Price::placeholder(),
            remark: String::new(),
        }
    }
}

/// Groups `snapshot` and builds the Building the layout draws, without
/// flags: those need watched time, which `flags::Watch` keeps.
pub fn building(snapshot: &Snapshot, rules: &Rules, lease: &Lease) -> Building {
    building_of(snapshot, group(snapshot, rules), lease)
}

/// The Building for a snapshot already grouped.
pub fn building_of(snapshot: &Snapshot, grouping: Grouping, lease: &Lease) -> Building {
    let rent = grouping.rent_bytes();
    let boxes = grouping.boxes_bytes;
    let (staff, vacant) = balance(
        snapshot.capacity_bytes,
        snapshot.available_bytes,
        rent.saturating_add(boxes),
    );
    Building {
        kind: "building".into(),
        date: Date::from_unix_ms(snapshot.at_ms).iso(),
        memory_type: lease.memory_type.clone(),
        price: lease.price.clone(),
        capacity_bytes: snapshot.capacity_bytes,
        vacant_bytes: vacant,
        staff_bytes: staff,
        own_bytes: grouping.own_bytes,
        remark: lease.remark.clone(),
        rent_is_estimate: snapshot.rent_is_estimate,
        boxes_bytes: boxes,
        tenants: grouping
            .tenants
            .into_iter()
            .map(|t| Tenant {
                id: t.id,
                name: t.name,
                rent_bytes: t.rent_bytes,
                lease_bytes: t.lease_bytes,
                process_count: t.process_count,
                packed_bytes: t.packed_bytes,
                flags: Vec::new(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_windows_directory_on_any_drive() {
        assert!(under_windows_dir(r"C:\Windows\System32\svchost.exe"));
        assert!(under_windows_dir(r"d:\WINDOWS\explorer.exe"));
        assert!(under_windows_dir(r"%SystemRoot%\System32\conhost.exe"));
        assert!(!under_windows_dir(r"C:\WindowsApps\x.exe"));
        assert!(!under_windows_dir(
            r"C:\Program Files\WindowsApps\Calc\calc.exe"
        ));
        assert!(!under_windows_dir("windows"));
    }

    #[test]
    fn balance_leaves_staff_as_the_remainder() {
        assert_eq!(balance(100, 30, 50), (20, 30));
    }

    #[test]
    fn balance_shrinks_vacant_when_rent_plus_vacant_exceeds_capacity() {
        assert_eq!(balance(100, 30, 80), (0, 20));
        assert_eq!(balance(100, 30, 120), (0, 0));
        assert_eq!(balance(100, 130, 0), (0, 100));
    }

    fn named(path: Option<&str>, description: Option<&str>) -> Process {
        Process {
            image_path: path.map(String::from),
            description: description.map(String::from),
            ..Process::default()
        }
    }

    #[test]
    fn names_come_from_the_rules_then_the_description_then_the_file() {
        let rules = Rules::builtin();
        let name = |p, d| tenant_name(&named(p, d), &rules);
        assert_eq!(
            name(Some(r"C:\x\Code.exe"), Some("Visual Studio Code")),
            "VS Code"
        );
        assert_eq!(
            name(Some(r"C:\x\olk.exe"), Some("Microsoft Outlook")),
            "Outlook"
        );
        assert_eq!(
            name(Some(r"C:\x\a.exe"), Some("  Visual   Studio ")),
            "Visual Studio"
        );
        assert_eq!(name(Some(r"C:\x\a.exe"), Some("Microsoft")), "Microsoft");
        assert_eq!(name(Some(r"C:\x\paint.net.EXE"), None), "paint.net");
        // Seen on the Windows runner: the sign hid the company word.
        assert_eq!(
            name(
                Some(r"C:\x\vctip.exe"),
                Some("Microsoft\u{ae} VC compiler and tools")
            ),
            "VC compiler and tools"
        );
        assert_eq!(
            name(
                Some(r"C:\x\igcc.exe"),
                Some("Intel\u{ae} Graphics\u{2122} Command Center")
            ),
            "Graphics Command Center"
        );
        assert_eq!(name(Some(r"C:\x\tool"), Some("")), "tool");
    }
}
