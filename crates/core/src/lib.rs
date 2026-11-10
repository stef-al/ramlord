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
