//! Windows' own memory counters and each process's graphics memory, read
//! through the Performance Data Helper (pdh.dll). These are documented
//! performance counters, the ones Task Manager and Performance Monitor
//! show. They are added by their English names, so a Czech or German
//! Windows answers too, and need no administrator rights. A counter this
//! Windows does not have (Wine, or no graphics driver that publishes them)
//! reads as 0.

use std::collections::HashMap;

use windows::Win32::System::Performance::{
    PDH_CSTATUS_NEW_DATA, PDH_CSTATUS_VALID_DATA, PDH_FMT_COUNTERVALUE,
    PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_LARGE, PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA,
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
    PdhGetFormattedCounterValue, PdhOpenQueryW,
};
use windows::core::PCWSTR;

/// The Memory object's counters, in the order of `Memory`'s fields.
const MEMORY: [&str; 9] = [
    r"\Memory\Pool Paged Resident Bytes",
    r"\Memory\System Cache Resident Bytes",
    r"\Memory\System Driver Resident Bytes",
    r"\Memory\System Code Resident Bytes",
    r"\Memory\Modified Page List Bytes",
    r"\Memory\Standby Cache Core Bytes",
    r"\Memory\Standby Cache Normal Priority Bytes",
    r"\Memory\Standby Cache Reserve Bytes",
    r"\Memory\Free & Zero Page List Bytes",
];

/// Per process and per adapter; instances look like
/// `pid_1234_luid_0x00000000_0x0000D1C2_phys_0`.
const GPU: [&str; 3] = [
    r"\GPU Process Memory(*)\Shared Usage",
    r"\GPU Process Memory(*)\Dedicated Usage",
    r"\GPU Adapter Memory(*)\Shared Usage",
];

/// Bytes from the Memory object; 0 where a counter is missing.
#[derive(Clone, Copy, Debug, Default)]
pub struct Memory {
    pub paged_resident: u64,
    pub file_cache: u64,
    pub drivers: u64,
    pub kernel_code: u64,
    pub modified: u64,
    pub standby: u64,
    pub free: u64,
}

/// Graphics memory by process, and the adapters' total, in bytes.
#[derive(Clone, Debug, Default)]
pub struct Gpu {
    pub shared: HashMap<u32, u64>,
    pub dedicated: HashMap<u32, u64>,
    pub adapters_shared: u64,
}

/// An open query with its counters; a counter Windows refused is None.
struct Query {
    handle: PDH_HQUERY,
    counters: Vec<Option<PDH_HCOUNTER>>,
}

impl Query {
    /// None when not one of `paths` can be added.
    fn open(paths: &[&str]) -> Option<Query> {
        let mut handle = PDH_HQUERY::default();
        // SAFETY: `handle` receives the new query, closed in `drop`.
        if unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &mut handle) } != 0 {
            return None;
        }
        let query = Query {
            handle,
            counters: paths
                .iter()
                .map(|path| {
                    let wide: Vec<u16> = path.encode_utf16().chain([0]).collect();
                    let mut counter = PDH_HCOUNTER::default();
                    // SAFETY: `wide` is a live, NUL-terminated path for the
                    // call; the counter belongs to the query.
                    let status = unsafe {
                        PdhAddEnglishCounterW(handle, PCWSTR(wide.as_ptr()), 0, &mut counter)
                    };
                    (status == 0).then_some(counter)
                })
                .collect(),
        };
        query.counters.iter().any(Option::is_some).then_some(query)
    }

    /// Reads every counter of the query now.
    fn collect(&self) -> bool {
        // SAFETY: a query this struct owns.
        unsafe { PdhCollectQueryData(self.handle) == 0 }
    }
}

impl Drop for Query {
    fn drop(&mut self) {
        // SAFETY: closes the query and its counters, once.
        unsafe {
            PdhCloseQuery(self.handle);
        }
    }
}

fn valid(status: u32) -> bool {
    status == PDH_CSTATUS_VALID_DATA || status == PDH_CSTATUS_NEW_DATA
}

/// One counter's value; 0 if it is missing or has no data.
fn value(counter: Option<PDH_HCOUNTER>) -> u64 {
    let Some(counter) = counter else { return 0 };
    let mut value = PDH_FMT_COUNTERVALUE::default();
    // SAFETY: `value` is a live local the call fills.
    let status = unsafe { PdhGetFormattedCounterValue(counter, PDH_FMT_LARGE, None, &mut value) };
    if status != 0 || !valid(value.CStatus) {
        return 0;
    }
    // SAFETY: PDH_FMT_LARGE fills `largeValue`.
    u64::try_from(unsafe { value.Anonymous.largeValue }).unwrap_or(0)
}

/// Every instance of a wildcard counter: `(instance name, value)`.
fn values(counter: Option<PDH_HCOUNTER>) -> Vec<(String, u64)> {
    let Some(counter) = counter else {
        return Vec::new();
    };
    // Instances can come and go between the two calls: try a few times.
    for _ in 0..3 {
        let (mut bytes, mut count) = (0u32, 0u32);
        // SAFETY: asks for the buffer size only.
        let status = unsafe {
            PdhGetFormattedCounterArrayW(counter, PDH_FMT_LARGE, &mut bytes, &mut count, None)
        };
        if status != PDH_MORE_DATA {
            return Vec::new();
        }
        // The items come first, then the instance names they point to.
        let item = size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>();
        let mut buffer =
            vec![PDH_FMT_COUNTERVALUE_ITEM_W::default(); (bytes as usize).div_ceil(item)];
        // SAFETY: `buffer` holds at least `bytes` bytes, suitably aligned.
        let status = unsafe {
            PdhGetFormattedCounterArrayW(
                counter,
                PDH_FMT_LARGE,
                &mut bytes,
                &mut count,
                Some(buffer.as_mut_ptr()),
            )
        };
        if status == PDH_MORE_DATA {
            continue;
        }
        if status != 0 {
            return Vec::new();
        }
        return buffer
            .iter()
            .take(count as usize)
            .filter(|i| valid(i.FmtValue.CStatus))
            .map(|i| {
                // SAFETY: the name points into `buffer`, alive here; the
                // value was formatted as PDH_FMT_LARGE.
                let name = unsafe { i.szName.to_string() }.unwrap_or_default();
                let value = unsafe { i.FmtValue.Anonymous.largeValue };
                (name, u64::try_from(value).unwrap_or(0))
            })
            .collect();
    }
    Vec::new()
}

/// The pid in a GPU Process Memory instance name.
fn pid_of(instance: &str) -> Option<u32> {
    instance
        .strip_prefix("pid_")?
        .split('_')
        .next()?
        .parse()
        .ok()
}

/// Sums a per-process counter by pid.
fn by_pid(values: Vec<(String, u64)>) -> HashMap<u32, u64> {
    let mut sums = HashMap::new();
    for (instance, bytes) in values {
        if let Some(pid) = pid_of(&instance) {
            *sums.entry(pid).or_insert(0) += bytes;
        }
    }
    sums
}

/// The two queries, opened once: adding counters loads Windows' counter
/// names, which is slow the first time.
pub struct Counters {
    memory: Option<Query>,
    gpu: Option<Query>,
}

impl Counters {
    pub fn new() -> Counters {
        let counters = Counters {
            memory: Query::open(&MEMORY),
            gpu: Query::open(&GPU),
        };
        // The first collection loads the counters' providers; do it here,
        // not in the first sample.
        let _ = counters.memory();
        let _ = counters.gpu();
        counters
    }

    pub fn memory(&self) -> Memory {
        let Some(query) = &self.memory else {
            return Memory::default();
        };
        if !query.collect() {
            return Memory::default();
        }
        let v: Vec<u64> = query.counters.iter().map(|&c| value(c)).collect();
        Memory {
            paged_resident: v[0],
            file_cache: v[1],
            drivers: v[2],
            kernel_code: v[3],
            modified: v[4],
            standby: v[5] + v[6] + v[7],
            free: v[8],
        }
    }

    pub fn gpu(&self) -> Gpu {
        let Some(query) = &self.gpu else {
            return Gpu::default();
        };
        if !query.collect() {
            return Gpu::default();
        }
        Gpu {
            shared: by_pid(values(query.counters[0])),
            dedicated: by_pid(values(query.counters[1])),
            adapters_shared: values(query.counters[2]).iter().map(|(_, b)| b).sum(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pids_come_from_instance_names() {
        assert_eq!(
            pid_of("pid_1234_luid_0x00000000_0x0000D1C2_phys_0"),
            Some(1234)
        );
        assert_eq!(pid_of("luid_0x00000000_0x0000D1C2_phys_0"), None);
        assert_eq!(pid_of("pid_x_luid"), None);
        let sums = by_pid(vec![
            ("pid_7_luid_0x0_0x1_phys_0".into(), 5),
            ("pid_7_luid_0x0_0x2_phys_0".into(), 6),
            ("pid_8_luid_0x0_0x1_phys_0".into(), 1),
        ]);
        assert_eq!(sums[&7], 11);
        assert_eq!(sums[&8], 1);
    }
}
