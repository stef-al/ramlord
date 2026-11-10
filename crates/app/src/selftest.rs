//! `--self-test`: sample and draw off screen for 10 seconds, write timings
//! and own memory, and fail if a budget is broken (SPEC.md, Budgets).
//!
//! The harness runs against a `Machine`, so its arithmetic is tested on
//! Linux with a fake one; the Windows layer supplies the real one.

use std::time::{Duration, Instant};

use ramlord_core::Snapshot;
use ramlord_core::live::Live;

const MIB: u64 = 1 << 20;

/// What the self-test needs from the PC.
pub trait Machine {
    fn snapshot(&mut self) -> Result<Snapshot, String>;
    /// CPU time this process has used so far, user and kernel.
    fn cpu_time(&self) -> Duration;
    /// This process's private working set now.
    fn own_bytes(&self) -> u64;
}

/// The budget limits (SPEC.md, Budgets).
pub struct Limits {
    pub exe_bytes: u64,
    pub own_bytes: u64,
    /// Average CPU as a share of the whole machine, in basis points.
    pub cpu_basis_points: u64,
    pub snapshot: Duration,
    pub frame: Duration,
}

impl Limits {
    pub const SPEC: Limits = Limits {
        exe_bytes: 5 * MIB,
        own_bytes: 40 * MIB,
        cpu_basis_points: 100,
        snapshot: Duration::from_millis(50),
        frame: Duration::from_millis(5),
    };
}

/// One measured budget.
#[derive(Debug, PartialEq, Eq)]
pub struct Budget {
    pub name: &'static str,
    pub value: u64,
    pub limit: u64,
    pub unit: &'static str,
}

impl Budget {
    pub fn pass(&self) -> bool {
        self.value <= self.limit
    }
}

#[derive(Debug)]
pub struct Report {
    pub budgets: Vec<Budget>,
    pub wall: Duration,
    pub snapshot_us: Vec<u64>,
    pub frame_us: Vec<u64>,
    pub processes: usize,
    pub tenants: usize,
    pub rent_is_estimate: bool,
    pub errors: Vec<String>,
}

impl Report {
    pub fn pass(&self) -> bool {
        self.errors.is_empty() && self.budgets.iter().all(Budget::pass)
    }

    pub fn to_json(&self) -> String {
        let budgets: Vec<serde_json::Value> = self
            .budgets
            .iter()
            .map(|b| {
                serde_json::json!({
                    "name": b.name,
                    "value": b.value,
                    "limit": b.limit,
                    "unit": b.unit,
                    "pass": b.pass(),
                })
            })
            .collect();
        let report = serde_json::json!({
            "pass": self.pass(),
            "budgets": budgets,
            "wall_ms": self.wall.as_millis() as u64,
            "samples": self.snapshot_us.len(),
            "snapshot_us": self.snapshot_us,
            "frame_us": self.frame_us,
            "processes": self.processes,
            "tenants": self.tenants,
            "rent_is_estimate": self.rent_is_estimate,
            "errors": self.errors,
        });
        let mut text = serde_json::to_string_pretty(&report).unwrap_or_default();
        text.push('\n');
        text
    }
}

fn micros(d: Duration) -> u64 {
    u64::try_from(d.as_micros()).unwrap_or(u64::MAX)
}

/// Samples `machine` every `interval` for `duration`, drawing each sample
/// with `live`, then measures the budgets.
pub fn run(
    machine: &mut dyn Machine,
    live: &mut Live,
    duration: Duration,
    interval: Duration,
    exe_bytes: u64,
    cpus: u64,
    limits: &Limits,
) -> Report {
    let start = Instant::now();
    let cpu_start = machine.cpu_time();
    let mut report = Report {
        budgets: Vec::new(),
        wall: Duration::ZERO,
        snapshot_us: Vec::new(),
        frame_us: Vec::new(),
        processes: 0,
        tenants: 0,
        rent_is_estimate: false,
        errors: Vec::new(),
    };
    let mut tick = 0u32;
    loop {
        let due = interval * tick;
        if due >= duration {
            break;
        }
        if let Some(wait) = due.checked_sub(start.elapsed()) {
            std::thread::sleep(wait);
        }
        tick += 1;

        let t = Instant::now();
        let snapshot = match machine.snapshot() {
            Ok(s) => s,
            Err(e) => {
                report.errors.push(e);
                continue;
            }
        };
        report.snapshot_us.push(micros(t.elapsed()));
        let t = Instant::now();
        if let Err(e) = live.update(&snapshot) {
            report.errors.push(e.to_string());
            continue;
        }
        report.frame_us.push(micros(t.elapsed()));
        report.processes = snapshot.processes.len();
        report.rent_is_estimate = snapshot.rent_is_estimate;
        report.tenants = live.building().map_or(0, |b| b.tenants.len());
    }
    if let Some(rest) = duration.checked_sub(start.elapsed()) {
        std::thread::sleep(rest);
    }
    report.wall = start.elapsed();

    let cpu = micros(machine.cpu_time().saturating_sub(cpu_start));
    let wall = micros(report.wall).max(1);
    let slowest = |v: &[u64]| v.iter().copied().max().unwrap_or(u64::MAX);
    report.budgets = vec![
        Budget {
            name: "exe size",
            value: exe_bytes,
            limit: limits.exe_bytes,
            unit: "bytes",
        },
        Budget {
            name: "own private working set",
            value: machine.own_bytes(),
            limit: limits.own_bytes,
            unit: "bytes",
        },
        Budget {
            name: "average CPU",
            value: (u128::from(cpu) * 10_000 / (u128::from(wall) * u128::from(cpus.max(1)))) as u64,
            limit: limits.cpu_basis_points,
            unit: "basis points of the machine",
        },
        Budget {
            name: "one snapshot",
            value: slowest(&report.snapshot_us),
            limit: micros(limits.snapshot),
            unit: "microseconds, slowest",
        },
        Budget {
            name: "one frame",
            value: slowest(&report.frame_us),
            limit: micros(limits.frame),
            unit: "microseconds, slowest",
        },
    ];
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use ramlord_core::Rules;
    use ramlord_core::building::Price;
    use ramlord_core::snapshot::Process;
    use ramlord_core::tenants::Lease;

    struct Fake {
        at_ms: u64,
        cpu: Duration,
        cpu_per_sample: Duration,
        own: u64,
        fail: bool,
    }

    impl Machine for Fake {
        fn snapshot(&mut self) -> Result<Snapshot, String> {
            if self.fail {
                return Err("no snapshot".into());
            }
            self.at_ms += 10;
            self.cpu += self.cpu_per_sample;
            Ok(Snapshot {
                at_ms: self.at_ms,
                capacity_bytes: 8192 * MIB,
                available_bytes: 4096 * MIB,
                processes: vec![Process {
                    pid: 10,
                    parent_pid: 1,
                    session: 1,
                    image_path: Some(r"C:\App\app.exe".into()),
                    rent_bytes: 500 * MIB,
                    ..Process::default()
                }],
                ..Snapshot::default()
            })
        }
        fn cpu_time(&self) -> Duration {
            self.cpu
        }
        fn own_bytes(&self) -> u64 {
            self.own
        }
    }

    fn live() -> Live {
        Live::new(
            Rules::builtin(),
            Lease {
                memory_type: "RAM".into(),
                price: Price::placeholder(),
                remark: String::new(),
            },
        )
    }

    fn fake(cpu_per_sample: Duration) -> Fake {
        Fake {
            at_ms: 0,
            cpu: Duration::ZERO,
            cpu_per_sample,
            own: 12 * MIB,
            fail: false,
        }
    }

    fn budget<'a>(report: &'a Report, name: &str) -> &'a Budget {
        report.budgets.iter().find(|b| b.name == name).unwrap()
    }

    #[test]
    fn a_light_run_passes_every_budget() {
        let mut m = fake(Duration::ZERO);
        let generous = Limits {
            snapshot: Duration::from_secs(1),
            frame: Duration::from_secs(1),
            ..Limits::SPEC
        };
        let r = run(
            &mut m,
            &mut live(),
            Duration::from_millis(60),
            Duration::from_millis(20),
            MIB,
            4,
            &generous,
        );
        assert_eq!(r.snapshot_us.len(), 3);
        assert_eq!(r.frame_us.len(), 3);
        assert!(r.wall >= Duration::from_millis(60));
        assert_eq!(r.tenants, 1);
        assert!(r.pass(), "{r:?}");
        let json: serde_json::Value = serde_json::from_str(&r.to_json()).unwrap();
        assert_eq!(json["pass"], true);
        assert_eq!(json["budgets"].as_array().unwrap().len(), 5);
    }

    #[test]
    fn busy_cpu_and_a_big_exe_fail() {
        // 30 ms of CPU per sample, 3 samples in about 60 ms on 4 CPUs: far
        // above 1% of the machine.
        let mut m = fake(Duration::from_millis(30));
        let r = run(
            &mut m,
            &mut live(),
            Duration::from_millis(60),
            Duration::from_millis(20),
            6 * MIB,
            4,
            &Limits::SPEC,
        );
        assert!(!budget(&r, "average CPU").pass());
        assert!(!budget(&r, "exe size").pass());
        assert!(budget(&r, "own private working set").pass());
        assert!(!r.pass());
    }

    #[test]
    fn a_failing_snapshot_fails_the_test() {
        let mut m = fake(Duration::ZERO);
        m.fail = true;
        let r = run(
            &mut m,
            &mut live(),
            Duration::from_millis(20),
            Duration::from_millis(10),
            MIB,
            4,
            &Limits::SPEC,
        );
        assert_eq!(r.errors.len(), 2);
        assert!(!r.pass());
        assert!(!budget(&r, "one snapshot").pass(), "no sample is no pass");
    }
}
