//! The two icons: the tray's, which fills with occupancy, and the app's.
//! Both are drawn over `palette::TRAY_CLEAR`, which becomes transparent.

use crate::palette::*;
use crate::render::Draw;

/// The tray icon's size in pixels.
pub const TRAY_SIZE: u32 = 16;

/// The tray icon: a small tower whose windows light up from the ground
/// floor with occupancy, red when the building is full (SPEC.md, Window
/// and tray). Drawn over `palette::TRAY_CLEAR`, which the Windows layer
/// makes transparent.
pub fn tray_icon(occupancy_percent: u64, full: bool) -> Vec<Draw> {
    let wall = if full { TRAY_FULL } else { TRAY_WALL };
    let mut d = vec![
        // Antenna with its light, roof, walls with a dark outline.
        Draw::rect(11, 0, 11, 2, OUTLINE),
        Draw::dot(11, 0, ALERT),
        Draw::rect(2, 2, 13, 15, OUTLINE),
        Draw::rect(3, 3, 12, 14, wall),
        Draw::rect(3, 3, 12, 3, ROOF_EDGE),
    ];
    // Twelve windows, three across and four floors up, lit from the
    // ground floor in reading order.
    let lit = ((occupancy_percent.min(100) * 12 + 50) / 100) as usize;
    for k in 0..12 {
        let (floor, col) = (k / 3, k % 3);
        let x = 4 + 3 * col as i32;
        let y = 12 - 3 * floor as i32;
        let color = if k < lit { GOLD } else { DARK_WINDOW };
        d.push(Draw::rect(x, y, x + 1, y + 1, color));
    }
    d
}

/// The app icon's sizes: the 16 pixel drawing for 16 and 48, the 32 pixel
/// one for 32, 64 and 256, each enlarged by a whole number so the pixels
/// stay square.
pub const APP_ICON_SIZES: [u32; 5] = [16, 32, 48, 64, 256];

/// The app icon (SPEC.md, M6), `size` 16 or 32: the tower with tenants'
/// floors, the antenna's light, and at 32 a lit door between a box and a
/// picture. Drawn over `palette::TRAY_CLEAR`, which becomes transparent.
pub fn app_icon(size: u32) -> Vec<Draw> {
    let [red, orange, blue, green, ..] = HUES;
    if size < 32 {
        let mut d = vec![
            Draw::rect(11, 0, 11, 2, OUTLINE),
            Draw::dot(11, 0, ALERT),
            Draw::rect(2, 2, 13, 15, OUTLINE),
            Draw::rect(3, 3, 12, 14, BUILDING),
            Draw::rect(3, 3, 12, 3, ROOF_EDGE),
        ];
        // Three windows across, four floors: vacant on top, then tenants.
        let floors = [
            [VACANT_WINDOW, VACANT_WINDOW, VACANT_WINDOW],
            [blue.wall, blue.wall, VACANT_WINDOW],
            [orange.wall, orange.wall, orange.wall],
            [red.wall, red.wall, red.wall],
        ];
        for (floor, colors) in floors.iter().enumerate() {
            for (col, &color) in colors.iter().enumerate() {
                let (x, y) = (4 + 3 * col as i32, 4 + 3 * floor as i32);
                d.push(Draw::rect(x, y, x + 1, y + 1, color));
            }
        }
        return d;
    }
    let mut d = vec![
        // Antenna, roof and walls.
        Draw::rect(21, 0, 21, 4, OUTLINE),
        Draw::dot(21, 0, ALERT),
        Draw::rect(4, 4, 27, 8, OUTLINE),
        Draw::rect(5, 5, 26, 7, ROOF),
        Draw::rect(5, 5, 26, 5, ROOF_EDGE),
        Draw::rect(5, 8, 26, 31, OUTLINE),
        Draw::rect(6, 9, 25, 30, BUILDING),
    ];
    // Four floors of five units, filled from the ground up as the tower
    // fills them, with the top floor vacant.
    let vacant = Hue {
        wall: VACANT_WALL,
        window: VACANT_WINDOW,
    };
    let floors = [
        [vacant, vacant, vacant, vacant, vacant],
        [blue, blue, green, vacant, vacant],
        [orange, orange, orange, orange, blue],
        [red, red, red, red, orange],
    ];
    for (floor, hues) in floors.iter().enumerate() {
        let y = 9 + 4 * floor as i32;
        for (col, hue) in hues.iter().enumerate() {
            let x = 6 + 4 * col as i32;
            d.push(Draw::rect(x, y, x + 3, y + 2, hue.wall));
            d.push(Draw::rect(x + 1, y, x + 2, y + 1, hue.window));
        }
        d.push(Draw::rect(6, y + 3, 25, y + 3, OUTLINE));
    }
    // The ground floor: a box, the lit door and a picture.
    d.extend([
        Draw::rect(6, 25, 25, 30, STAFF_WALL),
        Draw::rect(7, 27, 10, 30, CARDBOARD_DARK),
        Draw::rect(7, 27, 10, 29, CARDBOARD),
        Draw::rect(8, 27, 9, 27, CARDBOARD_TAPE),
        Draw::rect(13, 26, 18, 30, OUTLINE),
        Draw::rect(14, 27, 17, 30, GOLD),
        Draw::rect(21, 26, 24, 29, FRAME),
        Draw::rect(22, 27, 23, 28, CANVAS),
    ]);
    d
}
