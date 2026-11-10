//! The Windows layer: the only place `unsafe` is allowed. It calls Win32
//! directly through Microsoft's `windows` crate and stays thin; all
//! behaviour lives in `ramlord-core`.
//!
//! - `provider` builds a Snapshot.
//! - `shell` is the window: whole-number scaling, the 2 second sampling
//!   loop, redraws only when the scene changes, clicks and keys.
//! - `evict` closes and, when asked, terminates a tenant's processes.

pub mod evict;
pub mod provider;
pub mod shell;

use std::time::Duration;

use ramlord_core::Snapshot;

use crate::selftest::Machine;

/// The PC itself, for `--self-test`.
pub struct Pc(pub provider::Provider);

impl Machine for Pc {
    fn snapshot(&mut self) -> Result<Snapshot, String> {
        self.0.sample()
    }

    fn cpu_time(&self) -> Duration {
        provider::own_cpu_time()
    }

    fn own_bytes(&self) -> u64 {
        provider::own_private_bytes()
    }

    fn foreground(&mut self) -> Option<u32> {
        provider::foreground_pid()
    }
}
