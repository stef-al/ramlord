//! The building itself: sky, street, shell, the 256 units with their
//! windows, boxes and pictures, the labels and the two status lines.

use crate::building::{Building, Flag};
use crate::font::{ADVANCE, UP_ARROW};
use crate::format::{Shown, cut, gb_text, micros};
use crate::layout::{COLS, FLOORS, Holder, Layout, Owner};
use crate::palette::*;
use crate::render::Draw;

use super::{HEIGHT, REMARK_MAX, WIDTH};

pub(super) const CELL_W: i32 = 12;
pub(super) const CELL_H: i32 = 11;
const GRID_X: i32 = 16;
const GRID_Y: i32 = 30;

/// Top-left pixel of a unit.
pub(super) fn cell_xy(floor: usize, col: usize) -> (i32, i32) {
    (
        GRID_X + col as i32 * CELL_W,
        GRID_Y + (FLOORS - 1 - floor) as i32 * CELL_H,
    )
}

/// The label strip on a tenant's widest floor segment.
pub(super) struct Strip {
    pub(super) floor: usize,
    pub(super) first_col: usize,
    pub(super) cells: usize,
    pub(super) label: String,
    pub(super) arrow: bool,
}

pub(super) fn label_strips(building: &Building, layout: &Layout) -> Vec<(usize, Strip)> {
    let symbol = &building.price.symbol;
    let mut strips = Vec::new();
    for named in &layout.named {
        let tenant = &building.tenants[named.tenant];
        // Widest segment; on a tie, the lowest floor.
        let Some(seg) = layout
            .segments(Owner::Tenant(named.tenant))
            .into_iter()
            .max_by(|a, b| a.width().cmp(&b.width()).then(b.floor.cmp(&a.floor)))
        else {
            continue;
        };
        let room = seg.width() as i32 * CELL_W - 4;
        let arrow = tenant.has(Flag::Leaker);
        let extra = if arrow { 2 + ADVANCE } else { 0 };
        let name = tenant.name.to_uppercase();
        let rent = Shown::rent(micros(tenant.rent_bytes, building.price.per_gb_micros));
        let full = format!(
            "{name} {}",
            rent.fit(symbol, building.rent_is_estimate, None)
        );
        let fits = |s: &String| s.chars().count() as i32 * ADVANCE + extra <= room;
        if let Some(label) = [full, name].into_iter().find(fits) {
            let needed = label.chars().count() as i32 * ADVANCE + extra + 3;
            strips.push((
                named.tenant,
                Strip {
                    floor: seg.floor,
                    first_col: seg.first_col,
                    cells: (needed + CELL_W - 1).div_euclid(CELL_W) as usize,
                    label,
                    arrow,
                },
            ));
        }
    }
    strips
}

pub(super) fn building_scene(
    d: &mut Vec<Draw>,
    building: &Building,
    layout: &Layout,
    highlight: Option<usize>,
) {
    let w = WIDTH as i32;
    let h = HEIGHT as i32;
    d.push(Draw::rect(0, 120, w - 1, h - 1, SKY_LOW));
    for (x, y) in [
        (6, 9),
        (228, 6),
        (300, 4),
        (340, 9),
        (372, 3),
        (216, 60),
        (218, 150),
        (4, 100),
        (7, 170),
        (220, 200),
    ] {
        d.push(Draw::dot(x, y, STAR));
    }
    // Street: a kerb line, then the road (0, 212, 220 by 4).
    d.push(Draw::rect(0, 212, 219, 212, BUILDING));
    d.push(Draw::rect(0, 213, 219, 215, STREET));

    // Shell: roof, walls, foundation, antenna and sign.
    let x0 = GRID_X - 4;
    let x1 = GRID_X + COLS as i32 * CELL_W + 3;
    let top = GRID_Y;
    let bottom = GRID_Y + FLOORS as i32 * CELL_H - 1;
    d.push(Draw::rect(x0, top - 12, x1, top - 1, ROOF));
    d.push(Draw::rect(x0, top - 12, x1, top - 12, ROOF_EDGE));
    d.push(Draw::rect(x0 - 2, top - 3, x1 + 2, top - 1, ROOF_EDGE));
    d.push(Draw::rect(x0, top, x1, bottom, BUILDING));
    d.push(Draw::rect(
        x0 - 1,
        bottom + 1,
        x1 + 1,
        bottom + 6,
        FOUNDATION,
    ));
    d.push(Draw::rect(x1 - 14, top - 24, x1 - 14, top - 13, ROOF_EDGE));
    d.push(Draw::dot(x1 - 14, top - 25, ALERT));
    d.push(Draw::shadowed(x0 + 6, top - 23, "RAMLORD", GOLD, OUTLINE));

    let strips = label_strips(building, layout);
    let strip_of = |owner: Owner| match owner {
        Owner::Tenant(t) => strips.iter().find(|(k, _)| *k == t).map(|(_, s)| s),
        _ => None,
    };

    for (index, &owner) in layout.cells.iter().enumerate() {
        let (floor, col) = (index / COLS, index % COLS);
        let (x, y) = cell_xy(floor, col);
        let tenant = match owner {
            Owner::Tenant(t) => Some(&building.tenants[t]),
            _ => None,
        };
        let (wall, window) = match owner {
            Owner::Boxes(_) => (BASEMENT_WALL, CARDBOARD_TAPE),
            Owner::Gallery(_) => (GALLERY_WALL, FRAME),
            Owner::Vacant => (VACANT_WALL, VACANT_WINDOW),
            Owner::Staff => (STAFF_WALL, STAFF_DETAIL),
            Owner::Others => (OTHERS_WALL, OTHERS_WINDOW),
            Owner::Tenant(t) => {
                let hue = HUES[usize::from(layout.hue_of(t).unwrap_or(0))];
                (hue.wall, hue.window)
            }
        };
        d.push(Draw::rect(x, y, x + CELL_W - 1, y + CELL_H - 1, wall));
        let in_strip = strip_of(owner)
            .is_some_and(|s| s.floor == floor && s.first_col <= col && col < s.first_col + s.cells);
        let has = |flag| tenant.is_some_and(|t| t.has(flag));
        if let Owner::Boxes(whose) = owner {
            // A box in its tenant's hue, taped down the middle; the shared
            // stack is plain cardboard.
            let (body, tape, edge) = match whose {
                Holder::Tenant(t) => {
                    let hue = HUES[usize::from(layout.hue_of(t).unwrap_or(0))];
                    (hue.wall, hue.window, shade(hue.wall, 70))
                }
                Holder::Others => (OTHERS_WALL, OTHERS_WINDOW, shade(OTHERS_WALL, 70)),
                Holder::Staff => (CARDBOARD, CARDBOARD_TAPE, CARDBOARD_DARK),
            };
            // Wider and lower than a window: it sits on the floor, with a
            // shaded lid and side.
            d.push(Draw::rect(x + 1, y + 4, x + 9, y + 9, body));
            d.push(Draw::rect(x + 1, y + 4, x + 9, y + 4, edge));
            d.push(Draw::rect(x + 9, y + 4, x + 9, y + 9, edge));
            d.push(Draw::rect(x + 4, y + 4, x + 5, y + 9, tape));
        } else if let Owner::Gallery(whose) = owner {
            // A framed picture: a hill under a sky, in its tenant's hue;
            // the shared pictures are plain canvas.
            let (sky, hill) = match whose {
                Holder::Tenant(t) => {
                    let hue = HUES[usize::from(layout.hue_of(t).unwrap_or(0))];
                    (hue.window, hue.wall)
                }
                Holder::Others => (OTHERS_WINDOW, OTHERS_WALL),
                Holder::Staff => (CANVAS, CANVAS_INK),
            };
            d.push(Draw::rect(x + 2, y + 1, x + 8, y + 7, FRAME));
            d.push(Draw::rect(x + 2, y + 7, x + 8, y + 7, FRAME_DARK));
            d.push(Draw::rect(x + 3, y + 2, x + 7, y + 6, sky));
            d.push(Draw::rect(x + 3, y + 6, x + 7, y + 6, hill));
            d.push(Draw::rect(x + 4, y + 5, x + 6, y + 5, hill));
            d.push(Draw::dot(x + 5, y + 4, hill));
        } else if !in_strip {
            if owner == Owner::Staff {
                // A service panel with a pipe.
                d.push(Draw::rect(x + 2, y + 2, x + 8, y + 7, STAFF_PANEL));
                d.push(Draw::rect(x + 2, y + 4, x + 8, y + 4, window));
                d.push(Draw::dot(x + 5, y + 6, window));
            } else if has(Flag::Walls) {
                // No window, six vent dots.
                for dx in [3, 5, 7] {
                    for dy in [4, 6] {
                        d.push(Draw::dot(x + dx, y + dy, shade(wall, 60)));
                    }
                }
            } else if has(Flag::Away) {
                d.push(Draw::rect(x + 2, y + 2, x + 8, y + 7, DARK_WINDOW));
            } else if owner == Owner::Vacant {
                d.push(Draw::rect(x + 2, y + 2, x + 8, y + 7, window));
            } else {
                // A lit two-pane window.
                d.push(Draw::rect(x + 2, y + 2, x + 8, y + 7, window));
                d.push(Draw::rect(x + 5, y + 2, x + 5, y + 7, shade(wall, 85)));
            }
        }
        // Mortar along the bottom and right edges.
        let mortar = shade(wall, 80);
        d.push(Draw::rect(
            x,
            y + CELL_H - 1,
            x + CELL_W - 1,
            y + CELL_H - 1,
            mortar,
        ));
        d.push(Draw::rect(
            x + CELL_W - 1,
            y,
            x + CELL_W - 1,
            y + CELL_H - 1,
            mortar,
        ));
    }

    // A 1 pixel dark outline separates different owners.
    for (index, &owner) in layout.cells.iter().enumerate() {
        let (floor, col) = (index / COLS, index % COLS);
        let (x, y) = cell_xy(floor, col);
        if col < COLS - 1 && layout.cells[index + 1] != owner {
            d.push(Draw::rect(
                x + CELL_W - 1,
                y,
                x + CELL_W - 1,
                y + CELL_H - 1,
                OUTLINE,
            ));
        }
        if floor < FLOORS - 1 && layout.cells[index + COLS] != owner {
            let left = if col > 0 { x - 1 } else { x };
            d.push(Draw::rect(left, y - 1, x + CELL_W - 1, y - 1, OUTLINE));
        }
    }
    let gx1 = GRID_X + COLS as i32 * CELL_W - 1;
    d.push(Draw::rect(GRID_X - 1, GRID_Y - 1, gx1, GRID_Y - 1, OUTLINE));
    d.push(Draw::rect(GRID_X - 1, bottom, gx1, bottom, OUTLINE));
    d.push(Draw::rect(
        GRID_X - 1,
        GRID_Y - 1,
        GRID_X - 1,
        bottom,
        OUTLINE,
    ));
    d.push(Draw::rect(gx1, GRID_Y - 1, gx1, bottom, OUTLINE));

    if let Some(tenant) = highlight {
        outline_holder(d, layout, Holder::Tenant(tenant), GOLD);
    }

    // Labels: NAME $RENT or NAME, a red up arrow after a leaker's.
    for (_, strip) in &strips {
        let (x, y) = cell_xy(strip.floor, strip.first_col);
        d.push(Draw::shadowed(
            x + 3,
            y + 1,
            strip.label.clone(),
            TEXT,
            OUTLINE,
        ));
        if strip.arrow {
            let ax = x + 3 + strip.label.chars().count() as i32 * ADVANCE + 2;
            d.push(Draw::shadowed(
                ax,
                y + 1,
                UP_ARROW.to_string(),
                ALERT,
                OUTLINE,
            ));
        }
    }
    // BOXES on the ground floor, when the basement's first floor has room.
    let box_units = layout.box_units();
    if box_units.min(COLS) >= 3 {
        let (x, y) = cell_xy(0, 0);
        d.push(Draw::rect(x + 1, y + 1, x + 34, y + 8, BASEMENT_WALL));
        d.push(Draw::shadowed(x + 3, y + 1, "BOXES", TEXT_DIM, OUTLINE));
    }
    // GALLERY on the gallery's lowest floor, when it has room there.
    if let Some(seg) = layout
        .cells
        .iter()
        .position(|o| matches!(o, Owner::Gallery(_)))
        .map(|first| (first / COLS, first % COLS))
        && layout.cells[seg.0 * COLS + seg.1..(seg.0 + 1) * COLS]
            .iter()
            .take_while(|o| matches!(o, Owner::Gallery(_)))
            .count()
            >= 4
    {
        let (x, y) = cell_xy(seg.0, seg.1);
        d.push(Draw::rect(x + 1, y + 1, x + 46, y + 8, GALLERY_WALL));
        d.push(Draw::shadowed(x + 3, y + 1, "GALLERY", TEXT_DIM, OUTLINE));
    }
    // STAFF on Staff's lowest floor, when it has room there.
    if let Some(seg) = layout.segments(Owner::Staff).first()
        && seg.width() >= 3
    {
        let (x, y) = cell_xy(seg.floor, seg.first_col);
        d.push(Draw::rect(x + 1, y + 1, x + 34, y + 8, STAFF_WALL));
        d.push(Draw::shadowed(x + 3, y + 1, "STAFF", TEXT_DIM, OUTLINE));
    }
    // FOR RENT on the widest vacant segment (the highest on a tie), when it
    // is at least 6 units wide.
    if let Some(seg) = layout
        .segments(Owner::Vacant)
        .into_iter()
        .max_by(|a, b| a.width().cmp(&b.width()).then(a.floor.cmp(&b.floor)))
        && seg.width() >= 6
    {
        let (x, y) = cell_xy(seg.floor, seg.first_col);
        let label_w = 8 * ADVANCE;
        let cx = x + (seg.width() as i32 * CELL_W - label_w).div_euclid(2);
        d.push(Draw::rect(
            cx - 3,
            y + 1,
            cx + label_w + 1,
            y + 9,
            VACANT_WALL,
        ));
        d.push(Draw::text(cx, y + 2, "FOR RENT", TEXT_DIM));
    }

    // Status lines: occupancy and the two key hints, then the remark.
    d.push(Draw::text(
        12,
        219,
        format!(
            "OCCUPANCY {}%  {} GB VACANT",
            building.occupancy_percent(),
            gb_text(building.vacant_bytes)
        ),
        TEXT_DIM,
    ));
    d.push(Draw::text_right(
        376,
        219,
        "F2 RENT ROLL  F3 SETTINGS",
        TEXT_DIM,
        1,
    ));
    d.push(Draw::text(12, 230, cut(&building.remark, REMARK_MAX), TEXT));
}

/// Outlines every cell `holder` has (its units, its boxes and its
/// pictures, each group on its own), on the lines where the dark outline
/// between owners runs.
pub(super) fn outline_holder(d: &mut Vec<Draw>, layout: &Layout, holder: Holder, color: Rgb) {
    let ours = |k: usize| layout.cells[k].holder() == Some(holder);
    for index in (0..layout.cells.len()).filter(|&k| ours(k)) {
        let (floor, col) = (index / COLS, index % COLS);
        let (x, y) = cell_xy(floor, col);
        let other = |k: Option<usize>| k.is_none_or(|k| !ours(k));
        if other((col + 1 < COLS).then_some(index + 1)) {
            d.push(Draw::rect(
                x + CELL_W - 1,
                y - 1,
                x + CELL_W - 1,
                y + CELL_H - 1,
                color,
            ));
        }
        if other((col > 0).then(|| index - 1)) {
            d.push(Draw::rect(x - 1, y - 1, x - 1, y + CELL_H - 1, color));
        }
        if other((floor + 1 < FLOORS).then_some(index + COLS)) {
            d.push(Draw::rect(x - 1, y - 1, x + CELL_W - 1, y - 1, color));
        }
        if other((floor > 0).then(|| index - COLS)) {
            d.push(Draw::rect(
                x - 1,
                y + CELL_H - 1,
                x + CELL_W - 1,
                y + CELL_H - 1,
                color,
            ));
        }
    }
}
