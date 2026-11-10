//! The palette from SPEC.md, as 0xRRGGBB.

pub type Rgb = u32;

pub const SKY: Rgb = 0x1b1f3b;
pub const SKY_LOW: Rgb = 0x232848;
pub const STAR: Rgb = 0x8f97c9;
pub const OUTLINE: Rgb = 0x0d0e1c;
pub const BUILDING: Rgb = 0x3b3f5c;
pub const ROOF: Rgb = 0x55506e;
pub const ROOF_EDGE: Rgb = 0x6b6585;
pub const FOUNDATION: Rgb = 0x2b2d42;
pub const STREET: Rgb = 0x15172b;
pub const VACANT_WALL: Rgb = 0x20233f;
pub const VACANT_WINDOW: Rgb = 0x2b2f55;
pub const DARK_WINDOW: Rgb = 0x191b33;
pub const STAFF_WALL: Rgb = 0x4a4e69;
pub const STAFF_DETAIL: Rgb = 0x8a8fa8;
pub const STAFF_PANEL: Rgb = 0x3a3d55;
pub const OTHERS_WALL: Rgb = 0x6b6f8f;
pub const OTHERS_WINDOW: Rgb = 0xd5d8ea;
pub const TEXT: Rgb = 0xf4f1de;
pub const TEXT_DIM: Rgb = 0x9aa0c3;
pub const GOLD: Rgb = 0xffd166;
pub const ALERT: Rgb = 0xff5d5d;
pub const GOOD: Rgb = 0x6ee7a0;
pub const PANEL: Rgb = 0x14162b;
pub const PANEL_LINE: Rgb = 0x3b3f5c;

// The basement, where compressed memory sits in boxes (not in SPEC.md's
// palette: the owner asked for the boxes after M3).
pub const BASEMENT_WALL: Rgb = 0x2a2433;
pub const CARDBOARD: Rgb = 0xa47148;
pub const CARDBOARD_DARK: Rgb = 0x6e4a2c;
pub const CARDBOARD_TAPE: Rgb = 0xe3c48f;

// The gallery, where graphics memory hangs as framed pictures (not in
// SPEC.md's palette either: the owner asked for it after M4).
pub const GALLERY_WALL: Rgb = 0x3a2e45;
pub const FRAME: Rgb = 0xc9a14a;
pub const FRAME_DARK: Rgb = 0x7d5f28;
/// A picture nobody in particular hung: Windows' own and the small fry's.
pub const CANVAS: Rgb = 0xd9d3c3;
pub const CANVAS_INK: Rgb = 0x7c7f99;

/// The Staff card's parts, in `StaffParts::rows` order: services, kernel
/// pools, drivers, file cache, to disk, hardware, shared and other.
pub const STAFF_PARTS: [Rgb; 7] = [
    0x8a8fa8, 0x6b7cb8, 0x5d6390, 0x7fa7b8, 0xb59a6a, 0x4a4e69, 0x3a3d55,
];

// The rent roll card.
pub const PAPER: Rgb = 0xf4f1de;
pub const INK: Rgb = 0x1b1f3b;
pub const PAPER_FAINT: Rgb = 0x8a8576;
pub const PAPER_ALERT: Rgb = 0xb23a3a;
pub const PAPER_SHADOW: Rgb = 0x11132a;

/// A tenant's colours.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hue {
    pub wall: Rgb,
    pub window: Rgb,
}

pub const HUE_COUNT: usize = 12;

/// Tenant hues, wall then lit window; SPEC.md numbers them from 1.
#[rustfmt::skip]
pub const HUES: [Hue; HUE_COUNT] = [
    Hue { wall: 0xc94a4a, window: 0xffc2b8 }, Hue { wall: 0xd9822b, window: 0xffe0a8 },
    Hue { wall: 0x3f8fd9, window: 0xc4e2ff }, Hue { wall: 0x3fae6b, window: 0xbff2d2 },
    Hue { wall: 0x8f5ae0, window: 0xdcc6ff }, Hue { wall: 0x2fa3a3, window: 0xb4f0ec },
    Hue { wall: 0xc94fb5, window: 0xf7c0ee }, Hue { wall: 0xb89a2a, window: 0xfff1a8 },
    Hue { wall: 0x5a6ee0, window: 0xc6ceff }, Hue { wall: 0xe0608f, window: 0xffc6d8 },
    Hue { wall: 0x7fae3a, window: 0xdff2b0 }, Hue { wall: 0xb08a5a, window: 0xf0d8b6 },
];

/// Each channel scaled to `percent` of its value, rounded down.
pub fn shade(color: Rgb, percent: u32) -> Rgb {
    let channel = |shift: u32| ((color >> shift) & 0xff) * percent / 100;
    (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shade_scales_each_channel_down() {
        assert_eq!(shade(0xc94a4a, 80), 0xa03b3b);
        assert_eq!(shade(0xffffff, 100), 0xffffff);
        assert_eq!(shade(0x123456, 0), 0);
    }
}
