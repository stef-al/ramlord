//! RAMlord's pure core: a memory snapshot in, pixels out.
//!
//! Three rules keep it testable (SPEC.md, Architecture):
//! - no OS calls, clock reads, randomness, threads or floating point;
//! - bytes, units, pixels and money are integers (money in micro-units);
//! - the same input gives the same pixels on every platform.
//!
//! `tests/purity.rs` scans this crate's sources and fails on a breach.

#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]

pub mod building;
pub mod date;
pub mod format;
pub mod layout;
pub mod palette;

pub use building::{Building, BuildingError, Flag, Price, Tenant};
pub use layout::{HueBook, Layout};
