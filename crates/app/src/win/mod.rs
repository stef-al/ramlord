//! The Windows layer: the only place `unsafe` is allowed. It calls Win32
//! directly through Microsoft's `windows` crate and stays thin; all
//! behaviour lives in `ramlord-core`.
//!
//! M1 has the `shell` part only: a fixed-size window that shows one frame.

pub mod shell;
