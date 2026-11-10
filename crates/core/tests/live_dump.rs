//! Checks a live snapshot written by `ramlord --dump` (M2). Set
//! `RAMLORD_DUMP` to its path (relative paths start at the repository
//! root); without it this test does nothing. CI sets it on the Windows
//! runner:
//!
//! ```text
//! ramlord --dump out/dump.json
//! RAMLORD_DUMP=out/dump.json cargo test -p ramlord-core --test live_dump -- --nocapture
//! ```

use ramlord_core::tenants::{Lease, building, group};
use ramlord_core::{Rules, Snapshot};

const MIB: u64 = 1 << 20;

#[test]
fn a_live_dump_groups_into_staff_tenants_and_vacant_that_sum_to_capacity() {
    let Some(path) = std::env::var_os("RAMLORD_DUMP") else {
        eprintln!("RAMLORD_DUMP is not set; nothing to check");
        return;
    };
    // A relative path is taken from the repository root: cargo runs tests
    // from the crate's folder.
    let path = std::path::PathBuf::from(path);
    let path = if path.is_relative() {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(path)
    } else {
        path
    };
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read the dump {}: {e}", path.display()));
    let snapshot = Snapshot::from_json(&text).expect("the dump is a snapshot");

    assert!(
        snapshot.processes.len() > 20,
        "only {} processes",
        snapshot.processes.len()
    );
    let process_rent: u64 = snapshot.processes.iter().map(|p| p.rent_bytes).sum();
    assert!(process_rent > 0, "no process has rent");
    assert_eq!(
        snapshot.processes.iter().filter(|p| p.is_self).count(),
        1,
        "RAMlord must find itself exactly once"
    );
    assert!(snapshot.capacity_bytes > 0);
    assert!(snapshot.available_bytes <= snapshot.capacity_bytes);
    assert!(snapshot.at_ms > 1_700_000_000_000, "the clock looks wrong");

    // The profile folder must not appear in a dump.
    if let Some(profile) = std::env::var_os("USERPROFILE") {
        let profile = profile.to_string_lossy().to_lowercase();
        for p in &snapshot.processes {
            if let Some(path) = &p.image_path {
                assert!(
                    !path.to_lowercase().starts_with(&profile),
                    "{path} shows the profile folder"
                );
            }
        }
    }

    let grouping = group(&snapshot, &Rules::builtin());
    assert!(!grouping.tenants.is_empty(), "no tenants");
    assert!(grouping.rent_bytes() > 0, "the tenants have no rent");

    let b = building(&snapshot, &Rules::builtin(), &Lease::placeholder());
    b.validate().expect("the building balances");
    // Boxes (the Memory Compression process) and the gallery (graphics
    // memory in RAM) are parts of their own.
    let sum = b.staff_bytes + b.rent_bytes() + b.boxes_bytes + b.gallery_bytes + b.vacant_bytes;
    if b.staff_bytes == 0 && b.vacant_bytes == 0 && sum > b.capacity_bytes {
        assert!(
            snapshot.rent_is_estimate,
            "only estimated rent may overfill"
        );
    } else {
        assert_eq!(sum, b.capacity_bytes);
    }

    assert_eq!(
        b.staff_parts.sum(),
        b.staff_bytes,
        "Staff's parts make up Staff"
    );

    // The rows cross-checked, where Windows reports them. Available memory
    // is the standby list plus free and zeroed pages; read a moment apart,
    // they may differ a little.
    let slack = (64 * MIB).max(snapshot.capacity_bytes / 50);
    let lists = snapshot.standby_bytes + snapshot.free_bytes;
    if lists > 0 {
        assert!(
            lists.abs_diff(snapshot.available_bytes) <= slack,
            "standby {} MiB + free {} MiB should be the available {} MiB",
            snapshot.standby_bytes / MIB,
            snapshot.free_bytes / MIB,
            snapshot.available_bytes / MIB
        );
    }
    if snapshot.kernel_paged_resident_bytes > 0 {
        assert!(
            snapshot.kernel_paged_resident_bytes <= snapshot.kernel_paged_bytes + slack,
            "more of the paged pool in RAM than there is paged pool"
        );
    }
    if snapshot.physical_total_bytes > 0 {
        assert!(snapshot.physical_total_bytes <= snapshot.capacity_bytes);
    }
    if !snapshot.rent_is_estimate {
        for p in &snapshot.processes {
            assert!(
                p.working_set_bytes == 0 || p.rent_bytes <= p.working_set_bytes,
                "pid {}: more private than its whole working set",
                p.pid
            );
        }
    }
    let gpu_processes: u64 = snapshot.processes.iter().map(|p| p.gpu_shared_bytes).sum();
    let mut gpu: Vec<_> = snapshot
        .processes
        .iter()
        .filter(|p| p.gpu_shared_bytes > 0)
        .collect();
    gpu.sort_by_key(|p| std::cmp::Reverse(p.gpu_shared_bytes));
    let gpu_top: Vec<String> = gpu
        .iter()
        .take(8)
        .map(|p| {
            let name = p.exe().unwrap_or_else(|| "?".into());
            let tenant = grouping
                .tenant_of(p.pid)
                .map_or("Staff", |t| t.name.as_str());
            format!(
                "{name} ({tenant}) {} MiB in RAM, commit {} MiB, private {} MiB",
                p.gpu_shared_bytes / MIB,
                p.lease_bytes / MIB,
                p.rent_bytes / MIB
            )
        })
        .collect();
    let parts = &b.staff_parts;
    println!(
        "::notice title=Staff and gallery::Staff {} MiB = services {} + kernel pools {} + drivers {} + \
         file cache {} + to disk {} + hardware {} + shared, other {}. Gallery {} MiB (adapters {}, \
         processes {}). Standby {} + free {} vs available {} MiB; paged pool {} of which in RAM {}; \
         modified {} MiB. Graphics memory: {}",
        b.staff_bytes / MIB,
        parts.services_bytes / MIB,
        parts.kernel_pool_bytes / MIB,
        parts.drivers_bytes / MIB,
        parts.file_cache_bytes / MIB,
        parts.modified_bytes / MIB,
        parts.hardware_bytes / MIB,
        parts.other_bytes / MIB,
        b.gallery_bytes / MIB,
        snapshot.gpu_shared_bytes / MIB,
        gpu_processes / MIB,
        snapshot.standby_bytes / MIB,
        snapshot.free_bytes / MIB,
        snapshot.available_bytes / MIB,
        snapshot.kernel_paged_bytes / MIB,
        snapshot.kernel_paged_resident_bytes / MIB,
        snapshot.modified_bytes / MIB,
        if gpu_top.is_empty() {
            "none reported".to_string()
        } else {
            gpu_top.join("; ")
        }
    );

    // The gallery by holder, as the tower hangs it.
    let (named, _) = ramlord_core::layout::split_named(&b);
    let holders: Vec<String> = ramlord_core::layout::gallery_shares(&b, &named)
        .iter()
        .map(|&(holder, bytes)| {
            let who = match holder {
                ramlord_core::layout::Holder::Tenant(t) => b.tenants[t].name.clone(),
                other => format!("{other:?}"),
            };
            format!("{who} {} MiB", bytes / MIB)
        })
        .collect();
    println!(
        "::notice title=Gallery by holder::{}",
        if holders.is_empty() {
            "no gallery".to_string()
        } else {
            holders.join(", ")
        }
    );

    let mut largest: Vec<_> = grouping.tenants.iter().collect();
    largest.sort_by_key(|t| std::cmp::Reverse(t.rent_bytes));
    let top: Vec<String> = largest
        .iter()
        .take(5)
        .map(|t| format!("{} {} MiB", t.name, t.rent_bytes / MIB))
        .collect();
    println!(
        "::notice title=Live dump::{} processes, {} tenants. Staff {} MiB + tenants {} MiB + \
         gallery {} MiB + boxes {} MiB + vacant {} MiB = capacity {} MiB{}. Largest: {}",
        snapshot.processes.len(),
        b.tenants.len(),
        b.staff_bytes / MIB,
        b.rent_bytes() / MIB,
        b.gallery_bytes / MIB,
        b.boxes_bytes / MIB,
        b.vacant_bytes / MIB,
        b.capacity_bytes / MIB,
        if snapshot.rent_is_estimate {
            " (rent estimated from the working set)"
        } else {
            ""
        },
        top.join(", ")
    );
}
