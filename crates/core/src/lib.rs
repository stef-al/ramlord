//! RAMlord's pure core: a memory snapshot in, pixels out.
//!
//! Three rules keep it testable (SPEC.md, Architecture):
//! - no OS calls, clock reads, randomness, threads or floating point;
//! - bytes, units, pixels and money are integers (money in micro-units);
//! - the same input gives the same pixels on every platform.
//!
//! `tests/purity.rs` scans this crate's sources and fails on a breach.
//!
//! The pipeline: [`tenants`] groups a [`Snapshot`] into a [`Building`],
//! [`flags`] adds the four flags over watched time, [`remarks`] picks the
//! landlord's line, [`layout`] divides the building into 256 units,
//! [`scene`] turns it into a draw list, and [`render`] turns the draw list
//! into a [`Frame`]. [`live`] runs the whole pipeline sample by sample.

#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]

pub mod building;
pub mod date;
pub mod flags;
pub mod font;
pub mod format;
pub mod layout;
pub mod live;
pub mod palette;
pub mod remarks;
pub mod render;
pub mod rules;
pub mod scene;
pub mod smooth;
pub mod snapshot;
pub mod tenants;
pub mod ui;

pub use building::{Building, BuildingError, Flag, Price, Tenant};
pub use layout::{HueBook, Layout};
pub use render::Frame;
pub use rules::Rules;
pub use snapshot::Snapshot;

/// Draws the tower window, 384 by 240. Returns the frame and `saved_hues`
/// with any new tenants' hues added.
pub fn render_tower(building: &Building, saved_hues: &HueBook) -> (Frame, HueBook) {
    let (layout, hues) = Layout::new(building, saved_hues);
    let draws = scene::tower(building, &layout);
    let frame = render::rasterize(scene::WIDTH, scene::HEIGHT, palette::SKY, &draws);
    (frame, hues)
}

/// Draws the rent roll card, 240 by 300.
pub fn render_card(building: &Building, saved_hues: &HueBook) -> Frame {
    let (layout, _) = Layout::new(building, saved_hues);
    let draws = scene::card(building, &layout);
    render::rasterize(scene::CARD_WIDTH, scene::CARD_HEIGHT, palette::SKY, &draws)
}
