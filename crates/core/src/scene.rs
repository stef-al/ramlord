//! Scenes: what to draw for the tower window and the rent roll card.
//!
//! The geometry follows `docs/reference/prototype.py`; the rules follow
//! SPEC.md (The tower; Behaviors, The rent roll card).

use crate::building::{Building, Flag, Tenant};
use crate::font::{ADVANCE, UP_ARROW};
use crate::format::{Shown, cut, gb_text, micros, price_text, whole_gb, whole_mb, wrap_balanced};
use crate::layout::{COLS, FLOORS, Holder, Layout, Owner, box_share};
use crate::palette::*;
use crate::render::Draw;
use crate::ui::{Eviction, OTHERS_PAGE, Region, Target};

/// The tower canvas.
pub const WIDTH: u32 = 384;
pub const HEIGHT: u32 = 240;
/// The rent roll card canvas.
pub const CARD_WIDTH: u32 = 240;
pub const CARD_HEIGHT: u32 = 300;
/// `--render` saves the tower at 3 times scale, like the reference render
/// and the window on a 1080p screen.
pub const TOWER_PNG_SCALE: u32 = 3;
/// The card is saved at 4 times scale: 960 by 1200 (SPEC.md).
pub const CARD_PNG_SCALE: u32 = 4;

const CELL_W: i32 = 12;
const CELL_H: i32 = 11;
const GRID_X: i32 = 16;
const GRID_Y: i32 = 30;
/// Rows listed by name in the rent roll.
pub const ROLL_ROWS: usize = 10;
/// Characters of a name in the rent roll.
pub const ROLL_NAME: usize = 8;
/// Characters of a remark.
pub const REMARK_MAX: usize = 60;
/// Characters per line of the remark on the card.
pub const CARD_REMARK_WIDTH: usize = 30;

/// Top-left pixel of a unit.
fn cell_xy(floor: usize, col: usize) -> (i32, i32) {
    (
        GRID_X + col as i32 * CELL_W,
        GRID_Y + (FLOORS - 1 - floor) as i32 * CELL_H,
    )
}

/// The one tag a tenant shows: LEAK, SQUAT, AWAY, WALLS in that priority.
pub fn tag_of(tenant: &Tenant) -> Option<&'static str> {
    Flag::TAG_PRIORITY
        .iter()
        .find(|&&f| tenant.has(f))
        .and_then(|f| f.tag())
}

/// One rent roll row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RollRow {
    /// Index into `Building::tenants`.
    pub tenant: usize,
    pub name: String,
    pub tag: Option<&'static str>,
    pub hue: u8,
    pub bytes: u64,
    pub rent: Shown,
}

/// The rent roll's numbers, shared by the panel and the card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Roll {
    /// The 10 largest named tenants, largest first.
    pub rows: Vec<RollRow>,
    /// Everyone not listed: `(count, bytes, rent)`.
    pub others: Option<(usize, u64, Shown)>,
    /// The sum of the amounts shown above it.
    pub owed: Shown,
    pub staff: Shown,
    pub vacant: Shown,
    /// RAMlord's own memory, in cents.
    pub office: Shown,
    /// The boxes in the basement, when there are any.
    pub boxes: Option<Shown>,
    /// The gallery, when there is one.
    pub gallery: Option<Shown>,
    /// The tenants in the OTHERS line, largest first: the Others card.
    pub rest: Vec<usize>,
}

impl Roll {
    pub fn new(building: &Building, layout: &Layout) -> Roll {
        let price = building.price.per_gb_micros;
        let rent = |bytes| Shown::rent(micros(bytes, price));
        // For each line below (BOXES, GALLERY), one tenant row less fits
        // the panel.
        let listed = ROLL_ROWS
            - usize::from(building.boxes_bytes > 0)
            - usize::from(building.gallery_bytes > 0);
        let rows: Vec<RollRow> = layout
            .named
            .iter()
            .take(listed)
            .map(|n| {
                let t = &building.tenants[n.tenant];
                RollRow {
                    tenant: n.tenant,
                    name: cut(&t.name, ROLL_NAME),
                    tag: tag_of(t),
                    hue: n.hue,
                    bytes: t.rent_bytes,
                    rent: rent(t.rent_bytes),
                }
            })
            .collect();
        let mut rest: Vec<usize> = layout
            .named
            .iter()
            .skip(listed)
            .map(|n| n.tenant)
            .chain(layout.merged.iter().copied())
            .collect();
        rest.sort_by(|&a, &b| {
            let (a, b) = (&building.tenants[a], &building.tenants[b]);
            b.rent_bytes
                .cmp(&a.rent_bytes)
                .then_with(|| a.name.cmp(&b.name))
        });
        let others = (!rest.is_empty()).then(|| {
            let bytes = rest.iter().map(|&t| building.tenants[t].rent_bytes).sum();
            (rest.len(), bytes, rent(bytes))
        });
        let owed = Shown::sum(rows.iter().map(|r| r.rent).chain(others.map(|o| o.2)));
        Roll {
            rows,
            others,
            owed,
            staff: rent(building.staff_bytes),
            vacant: rent(building.vacant_bytes),
            office: Shown::in_cents(micros(building.own_bytes, price)),
            boxes: (building.boxes_bytes > 0).then(|| rent(building.boxes_bytes)),
            gallery: (building.gallery_bytes > 0).then(|| rent(building.gallery_bytes)),
            rest,
        }
    }
}

/// The label strip on a tenant's widest floor segment.
struct Strip {
    floor: usize,
    first_col: usize,
    cells: usize,
    label: String,
    arrow: bool,
}

fn label_strips(building: &Building, layout: &Layout) -> Vec<(usize, Strip)> {
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

/// The tower window: tower, rent roll and two status lines on a 384 by 240
/// canvas, painted over a `SKY` background.
pub fn tower(building: &Building, layout: &Layout) -> Vec<Draw> {
    tower_highlighting(building, layout, None)
}

/// The tower window with one tenant outlined in gold: the largest tenant
/// outside Family while the building is full (SPEC.md, The landlord's
/// remark). Gold, because the largest tenant usually wears the first hue,
/// red, where a red outline would vanish. `highlight` is an index into
/// `Building::tenants`.
pub fn tower_highlighting(
    building: &Building,
    layout: &Layout,
    highlight: Option<usize>,
) -> Vec<Draw> {
    window(building, layout, highlight, None, &Panel::Roll).0
}

/// What the right-hand panel shows.
pub enum Panel<'a> {
    Roll,
    Details(&'a Details),
    Dialog(&'a Eviction),
    /// What Staff is made of.
    Staff,
    /// Everyone the rent roll does not list.
    Others,
}

/// The whole window: the building, the selected tenant outlined in white,
/// and the panel. Returns the draw list and the clickable regions.
pub fn window(
    building: &Building,
    layout: &Layout,
    highlight: Option<usize>,
    selected: Option<usize>,
    panel: &Panel,
) -> (Vec<Draw>, Vec<Region>) {
    let mut d = Vec::new();
    let mut regions = Vec::new();
    building_scene(&mut d, building, layout, highlight);
    // Every unit, box and picture can be clicked: a tenant's select it,
    // Staff's and Others' open their cards.
    for (index, &owner) in layout.cells.iter().enumerate() {
        let target = match owner.holder() {
            Some(Holder::Tenant(t)) => Target::Tenant(t),
            Some(Holder::Staff) => Target::Staff,
            Some(Holder::Others) => Target::Others,
            None => continue,
        };
        let (x, y) = cell_xy(index / COLS, index % COLS);
        regions.push(Region {
            x0: x,
            y0: y,
            x1: x + CELL_W - 1,
            y1: y + CELL_H - 1,
            target,
        });
    }
    // What is selected is outlined everywhere it lives: its units, its
    // boxes and its pictures. A tenant merged into Others lives there.
    let chosen = match panel {
        Panel::Staff => Some(Holder::Staff),
        Panel::Others => Some(Holder::Others),
        _ => selected.map(|t| match layout.hue_of(t) {
            Some(_) => Holder::Tenant(t),
            None => Holder::Others,
        }),
    };
    if let Some(holder) = chosen
        && highlight.map(Holder::Tenant) != Some(holder)
    {
        outline_holder(&mut d, layout, holder, TEXT);
    }
    // The two key hints can be clicked.
    let hints_x = 376 - crate::font::text_width(25, 1) + 1;
    regions.push(Region {
        x0: hints_x - 2,
        y0: 217,
        x1: hints_x + 12 * ADVANCE,
        y1: 227,
        target: Target::RentRollCard,
    });
    regions.push(Region {
        x0: hints_x + 14 * ADVANCE - 2,
        y0: 217,
        x1: 377,
        y1: 227,
        target: Target::Settings,
    });
    match panel {
        Panel::Roll => rent_roll_panel(&mut d, &mut regions, building, layout, selected),
        Panel::Details(details) => details_panel(&mut d, &mut regions, building, layout, details),
        Panel::Dialog(eviction) => dialog_panel(&mut d, &mut regions, eviction),
        Panel::Staff => staff_panel(&mut d, &mut regions, building, layout),
        Panel::Others => others_panel(&mut d, &mut regions, building, layout, selected),
    }
    (d, regions)
}

fn building_scene(
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

/// Outlines every edge between `owner`'s units and anyone else's, on the
/// lines where the dark owner outline runs.
/// Outlines every cell `holder` has: its units, its boxes and its
/// pictures, each group on its own.
fn outline_holder(d: &mut Vec<Draw>, layout: &Layout, holder: Holder, color: Rgb) {
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

/// The rent roll panel: 224, 10, 154 by 204.
fn rent_roll_panel(
    d: &mut Vec<Draw>,
    regions: &mut Vec<Region>,
    building: &Building,
    layout: &Layout,
    selected: Option<usize>,
) {
    let symbol = building.price.symbol.as_str();
    let estimate = building.rent_is_estimate;
    // The rent column holds four characters.
    let column = |amount: Shown, estimate: bool| amount.fit(symbol, estimate, Some(4));
    let (lx0, lx1) = (224, 377);
    let gb_right = lx0 + 121;
    let rent_right = lx1 - 5;
    d.push(Draw::rect(lx0, 10, lx1, 213, PANEL_LINE));
    d.push(Draw::rect(lx0 + 1, 11, lx1 - 1, 212, PANEL));
    d.push(Draw::text(lx0 + 5, 15, "RENT ROLL", GOLD));
    let as_of = building.price_date();
    d.push(Draw::text(
        lx0 + 5,
        26,
        format!(
            "{} {}/GB  {} {}",
            building.memory_type,
            price_text(building.price.per_gb_micros, symbol),
            as_of.month_name(),
            as_of.day
        ),
        TEXT_DIM,
    ));
    d.push(Draw::rect(lx0 + 4, 36, lx1 - 4, 36, PANEL_LINE));
    d.push(Draw::text(lx0 + 13, 40, "TENANT", TEXT_DIM));
    d.push(Draw::text_right(gb_right, 40, "GB", TEXT_DIM, 1));
    d.push(Draw::text_right(rent_right, 40, "RENT", TEXT_DIM, 1));

    let roll = Roll::new(building, layout);
    let mut y = 51;
    for row in &roll.rows {
        if selected == Some(row.tenant) {
            d.push(Draw::rect(lx0 + 2, y - 2, lx1 - 2, y + 7, PANEL_LINE));
        }
        regions.push(Region {
            x0: lx0 + 1,
            y0: y - 2,
            x1: lx1 - 1,
            y1: y + 7,
            target: Target::Tenant(row.tenant),
        });
        d.push(Draw::rect(
            lx0 + 5,
            y,
            lx0 + 9,
            y + 6,
            HUES[usize::from(row.hue)].wall,
        ));
        d.push(Draw::text(lx0 + 13, y, row.name.clone(), TEXT));
        if let Some(tag) = row.tag {
            let color = match tag {
                "LEAK" => ALERT,
                "SQUAT" => GOLD,
                _ => TEXT_DIM,
            };
            d.push(Draw::text(lx0 + 66, y, tag, color));
        }
        d.push(Draw::text_right(
            gb_right,
            y,
            gb_text(row.bytes),
            TEXT_DIM,
            1,
        ));
        d.push(Draw::text_right(
            rent_right,
            y,
            column(row.rent, estimate),
            TEXT,
            1,
        ));
        y += 10;
    }
    if let Some((count, bytes, rent)) = roll.others {
        regions.push(Region {
            x0: lx0 + 1,
            y0: y - 2,
            x1: lx1 - 1,
            y1: y + 7,
            target: Target::Others,
        });
        d.push(Draw::rect(lx0 + 5, y, lx0 + 9, y + 6, OTHERS_WALL));
        d.push(Draw::text(
            lx0 + 13,
            y,
            format!("OTHERS ({count})"),
            TEXT_DIM,
        ));
        d.push(Draw::text_right(gb_right, y, gb_text(bytes), TEXT_DIM, 1));
        d.push(Draw::text_right(
            rent_right,
            y,
            column(rent, estimate),
            TEXT_DIM,
            1,
        ));
        y += 10;
    }
    y += 2;
    d.push(Draw::rect(lx0 + 4, y, lx1 - 4, y, PANEL_LINE));
    y += 5;
    d.push(Draw::text(lx0 + 5, y, "RENT OWED", GOLD));
    d.push(Draw::text_right(
        rent_right,
        y,
        roll.owed.fit(symbol, estimate, None),
        GOLD,
        1,
    ));
    y += 12;
    // Vacant is measured, not estimated; Staff is what rent leaves. The
    // boxes and the gallery, when there are any, are measured too. The
    // STAFF line opens the Staff card.
    regions.push(Region {
        x0: lx0 + 1,
        y0: y - 2,
        x1: lx1 - 1,
        y1: y + 7,
        target: Target::Staff,
    });
    let mut lines = vec![(
        STAFF_WALL,
        "STAFF",
        building.staff_bytes,
        roll.staff,
        TEXT_DIM,
        estimate,
    )];
    if let Some(gallery) = roll.gallery {
        lines.push((
            FRAME,
            "GALLERY",
            building.gallery_bytes,
            gallery,
            TEXT_DIM,
            false,
        ));
    }
    if let Some(boxes) = roll.boxes {
        lines.push((
            CARDBOARD,
            "BOXES",
            building.boxes_bytes,
            boxes,
            TEXT_DIM,
            false,
        ));
    }
    lines.push((
        VACANT_WINDOW,
        "VACANT",
        building.vacant_bytes,
        roll.vacant,
        GOOD,
        false,
    ));
    for (swatch, label, bytes, rent, color, estimated) in lines {
        d.push(Draw::rect(lx0 + 5, y, lx0 + 9, y + 6, swatch));
        d.push(Draw::text(lx0 + 13, y, label, TEXT_DIM));
        d.push(Draw::text_right(gb_right, y, gb_text(bytes), TEXT_DIM, 1));
        d.push(Draw::text_right(
            rent_right,
            y,
            column(rent, estimated),
            color,
            1,
        ));
        y += 10;
    }
    y += 4;
    d.push(Draw::text(
        lx0 + 5,
        y,
        format!("MY OFFICE {} MB", whole_mb(building.own_bytes)),
        TEXT_DIM,
    ));
    d.push(Draw::text_right(
        rent_right,
        y,
        roll.office.fit(symbol, estimate, None),
        TEXT_DIM,
        1,
    ));
}

/// Everything the details card shows besides the tenant itself.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Details {
    /// Index into `Building::tenants`.
    pub tenant: usize,
    pub peak_bytes: u64,
    /// Rent an hour of watched time ago, once there is an hour of history.
    pub hour_ago_bytes: Option<u64>,
    /// Wall time since its oldest process started.
    pub moved_in_ms: Option<u64>,
    /// Watched time since it last had focus; None if it never had.
    pub last_visit_ms: Option<u64>,
    /// Rent at the end of each five-minute bucket of the last watched
    /// hour, oldest first: at most 12.
    pub history: Vec<u64>,
    /// On the user's Family list.
    pub family: bool,
}

/// "JUST NOW", "5 MIN", "3 H", "2 D".
pub fn span_text(ms: u64) -> String {
    const MIN: u64 = 60_000;
    match ms {
        m if m < MIN => "JUST NOW".into(),
        m if m < 60 * MIN => format!("{} MIN", m / MIN),
        m if m < 48 * 60 * MIN => format!("{} H", m / (60 * MIN)),
        m => format!("{} D", m / (24 * 60 * MIN)),
    }
}

/// "JUST NOW", "5 MIN AGO", "3 H AGO".
pub fn ago_text(ms: u64) -> String {
    match span_text(ms) {
        now if ms < 60_000 => now,
        span => format!("{span} AGO"),
    }
}

/// A tenant's flags in words, such as "LEAKING, NOT HOME".
pub fn flags_in_words(tenant: &Tenant, family: bool) -> String {
    let mut words = Vec::new();
    if family || tenant.has(Flag::Family) {
        words.push("FAMILY");
    }
    for (flag, word) in [
        (Flag::Leaker, "LEAKING"),
        (Flag::Squatter, "SQUATTING"),
        (Flag::Away, "NOT HOME"),
        (Flag::Walls, "IN THE WALLS"),
    ] {
        if tenant.has(flag) {
            words.push(word);
        }
    }
    if words.is_empty() {
        "NO COMPLAINTS".into()
    } else {
        cut(&words.join(", "), 24)
    }
}

const BUTTON_H: i32 = 12;

fn button(
    d: &mut Vec<Draw>,
    regions: &mut Vec<Region>,
    (x0, x1): (i32, i32),
    y0: i32,
    label: &str,
    color: Rgb,
    target: Option<Target>,
) {
    d.push(Draw::rect(x0, y0, x1, y0 + BUTTON_H - 1, PANEL_LINE));
    d.push(Draw::rect(x0 + 1, y0 + 1, x1 - 1, y0 + BUTTON_H - 2, PANEL));
    d.push(Draw::text_center(x0, x1, y0 + 3, label, color, 1));
    if let Some(target) = target {
        regions.push(Region {
            x0,
            y0,
            x1,
            y1: y0 + BUTTON_H - 1,
            target,
        });
    }
}

/// The panel frame shared by the card, the dialog and the rent roll.
fn panel_frame(d: &mut Vec<Draw>) -> (i32, i32) {
    let (lx0, lx1) = (224, 377);
    d.push(Draw::rect(lx0, 10, lx1, 213, PANEL_LINE));
    d.push(Draw::rect(lx0 + 1, 11, lx1 - 1, 212, PANEL));
    (lx0, lx1)
}

/// A bar `x0..=x1` wide of stacked parts, each `(bytes, color)`; parts
/// with `None` stay empty. The whole bar is `total` bytes.
fn stacked_bar(
    d: &mut Vec<Draw>,
    (x0, x1): (i32, i32),
    y: i32,
    total: u64,
    parts: &[(u64, Option<Rgb>)],
) {
    d.push(Draw::rect(x0, y, x1, y + 5, PANEL_LINE));
    d.push(Draw::rect(x0 + 1, y + 1, x1 - 1, y + 4, PANEL));
    let total = u128::from(total.max(1));
    let width = u128::from((x1 - x0 + 1) as u32);
    let mut sum = 0u128;
    for &(bytes, color) in parts {
        let from = x0 + (width * sum / total) as i32;
        sum += u128::from(bytes);
        let to = x0 + (width * sum.min(total) / total) as i32 - 1;
        if let Some(color) = color
            && to >= from
        {
            d.push(Draw::rect(from, y, to, y + 5, color));
        }
    }
}

/// A row of a card: a swatch (filled, or an outline for `None`), a label
/// and a value on the right.
fn swatch_row(
    d: &mut Vec<Draw>,
    (lx0, lx1): (i32, i32),
    y: i32,
    swatch: Option<Rgb>,
    label: &str,
    value: String,
) {
    match swatch {
        Some(color) => d.push(Draw::rect(lx0 + 5, y, lx0 + 9, y + 6, color)),
        None => {
            d.push(Draw::rect(lx0 + 5, y, lx0 + 9, y + 6, PANEL_LINE));
            d.push(Draw::rect(lx0 + 6, y + 1, lx0 + 8, y + 5, PANEL));
        }
    }
    d.push(Draw::text(lx0 + 13, y, label, TEXT_DIM));
    d.push(Draw::text_right(lx1 - 5, y, value, TEXT, 1));
}

/// The details card (SPEC.md, The details card), in the rent roll's panel.
/// The lease is broken down: what is in RAM, its pictures in the gallery,
/// its share of the boxes, and the rest, leased on paper only.
fn details_panel(
    d: &mut Vec<Draw>,
    regions: &mut Vec<Region>,
    building: &Building,
    layout: &Layout,
    details: &Details,
) {
    let (lx0, lx1) = panel_frame(d);
    let tenant = &building.tenants[details.tenant];
    let symbol = building.price.symbol.as_str();
    let estimate = building.rent_is_estimate;
    let money = |bytes| {
        Shown::rent(micros(bytes, building.price.per_gb_micros)).fit(symbol, estimate, None)
    };
    let hue = layout.hue_of(details.tenant).map(|h| HUES[usize::from(h)]);
    let gb = |bytes: u64| format!("{} GB", gb_text(bytes));

    // Top: name and rent now, then its flags in words. A tenant merged
    // into Others wears Others' gray.
    d.push(Draw::rect(
        lx0 + 5,
        15,
        lx0 + 9,
        21,
        hue.map_or(OTHERS_WALL, |h| h.wall),
    ));
    d.push(Draw::text(
        lx0 + 13,
        15,
        cut(&tenant.name.to_uppercase(), crate::ui::NAME_CHARS),
        GOLD,
    ));
    d.push(Draw::text_right(
        lx1 - 5,
        15,
        money(tenant.rent_bytes),
        TEXT,
        1,
    ));
    let flagged = tenant.flags.iter().any(|f| *f != Flag::Family);
    d.push(Draw::text(
        lx0 + 5,
        26,
        flags_in_words(tenant, details.family),
        if flagged { ALERT } else { TEXT_DIM },
    ));
    d.push(Draw::rect(lx0 + 4, 36, lx1 - 4, 36, PANEL_LINE));

    // The lease, what it reserved, against what it holds.
    let boxes = box_share(building, details.tenant);
    let held = tenant
        .rent_bytes
        .saturating_add(tenant.gpu_bytes)
        .saturating_add(boxes);
    let on_paper = tenant.lease_bytes.saturating_sub(held);
    d.push(Draw::text(lx0 + 5, 40, "LEASE", TEXT_DIM));
    d.push(Draw::text_right(
        lx1 - 5,
        40,
        gb(tenant.lease_bytes),
        TEXT,
        1,
    ));
    let in_ram = hue.map_or(OTHERS_WALL, |h| h.wall);
    stacked_bar(
        d,
        (lx0 + 5, lx1 - 5),
        50,
        tenant.lease_bytes.max(held),
        &[
            (tenant.rent_bytes, Some(in_ram)),
            (tenant.gpu_bytes, Some(FRAME)),
            (boxes, Some(CARDBOARD)),
            (on_paper, None),
        ],
    );
    let est = if estimate { "~" } else { "" };
    // A building without a gallery or boxes does not know: a dash.
    let part = |known: bool, text: String| if known { text } else { "-".into() };
    let rows = [
        (
            Some(in_ram),
            "IN RAM",
            format!("{est}{}", gb(tenant.rent_bytes)),
        ),
        (
            Some(FRAME),
            "GALLERY",
            part(building.gallery_bytes > 0, gb(tenant.gpu_bytes)),
        ),
        (
            Some(CARDBOARD),
            "BOXES",
            part(building.boxes_bytes > 0, format!("~{}", gb(boxes))),
        ),
        (None, "ON PAPER", gb(on_paper)),
    ];
    let mut y = 59;
    for (swatch, label, value) in rows {
        swatch_row(d, (lx0, lx1), y, swatch, label, value);
        y += 9;
    }
    d.push(Draw::rect(lx0 + 4, 96, lx1 - 4, 96, PANEL_LINE));

    // Numbers.
    let rows = [
        ("PEAK", gb(details.peak_bytes.max(tenant.rent_bytes))),
        ("AN HOUR AGO", details.hour_ago_bytes.map_or("-".into(), gb)),
        ("PROCESSES", tenant.process_count.to_string()),
        ("MOVED IN", details.moved_in_ms.map_or("-".into(), ago_text)),
        (
            "LAST VISIT",
            details.last_visit_ms.map_or("NEVER".into(), ago_text),
        ),
    ];
    let mut y = 100;
    for (label, value) in rows {
        d.push(Draw::text(lx0 + 5, y, label, TEXT_DIM));
        d.push(Draw::text_right(lx1 - 5, y, value, TEXT, 1));
        y += 9;
    }
    d.push(Draw::rect(lx0 + 4, 146, lx1 - 4, 146, PANEL_LINE));

    // History: the last watched hour in five-minute bars, newest right.
    d.push(Draw::text(lx0 + 5, 149, "LAST HOUR", TEXT_DIM));
    let top = details
        .history
        .iter()
        .copied()
        .chain([tenant.rent_bytes])
        .max()
        .unwrap_or(0)
        .max(1);
    let base = 177;
    let slots = 12usize;
    let first = slots.saturating_sub(details.history.len());
    for (k, &value) in details.history.iter().rev().take(slots).rev().enumerate() {
        let x = lx0 + 5 + (first + k) as i32 * 12;
        let height = (u128::from(value) * 18 / u128::from(top)) as i32;
        d.push(Draw::rect(x, base - height, x + 9, base, in_ram));
    }
    d.push(Draw::rect(lx0 + 4, base + 1, lx1 - 4, base + 1, PANEL_LINE));

    // Actions. Family cannot be evicted.
    let family = details.family || tenant.has(Flag::Family);
    let (left, right) = ((lx0 + 5, lx0 + 74), (lx0 + 79, lx1 - 5));
    button(
        d,
        regions,
        left,
        183,
        "EVICT",
        if family { TEXT_DIM } else { ALERT },
        (!family).then_some(Target::Evict),
    );
    let label = if details.family {
        "FAMILY ON"
    } else {
        "FAMILY OFF"
    };
    button(d, regions, right, 183, label, TEXT, Some(Target::Family));
    if tenant.has(Flag::Squatter) {
        button(
            d,
            regions,
            left,
            198,
            "STARTUP",
            GOLD,
            Some(Target::Startup),
        );
    }
    button(d, regions, right, 198, "BACK", TEXT, Some(Target::Back));
}

/// The Staff card: what Staff is made of, in the rent roll's panel. The
/// parts come from Windows' own counters; SHARED, OTHER is what they
/// leave, which Windows does not break down without administrator rights.
fn staff_panel(d: &mut Vec<Draw>, regions: &mut Vec<Region>, building: &Building, layout: &Layout) {
    let (lx0, lx1) = panel_frame(d);
    let symbol = building.price.symbol.as_str();
    let staff = building.staff_bytes;
    let gb = |bytes: u64| format!("{} GB", gb_text(bytes));
    d.push(Draw::rect(lx0 + 5, 15, lx0 + 9, 21, STAFF_WALL));
    d.push(Draw::text(lx0 + 13, 15, "STAFF", GOLD));
    d.push(Draw::text_right(
        lx1 - 5,
        15,
        Shown::rent(micros(staff, building.price.per_gb_micros)).fit(
            symbol,
            building.rent_is_estimate,
            None,
        ),
        TEXT,
        1,
    ));
    d.push(Draw::text(
        lx0 + 5,
        26,
        "WINDOWS KEEPS IT RUNNING",
        TEXT_DIM,
    ));
    d.push(Draw::rect(lx0 + 4, 36, lx1 - 4, 36, PANEL_LINE));
    d.push(Draw::text(lx0 + 5, 40, "TOTAL", TEXT_DIM));
    d.push(Draw::text_right(lx1 - 5, 40, gb(staff), TEXT, 1));

    let parts = &building.staff_parts;
    if parts.sum() == 0 {
        // A building file that does not say.
        d.push(Draw::text(lx0 + 5, 59, "NOT BROKEN DOWN HERE", TEXT_DIM));
    } else {
        let rows = parts.rows();
        stacked_bar(
            d,
            (lx0 + 5, lx1 - 5),
            50,
            parts.sum(),
            &rows
                .iter()
                .zip(STAFF_PARTS)
                .map(|(&(_, bytes), color)| (bytes, Some(color)))
                .collect::<Vec<_>>(),
        );
        let mut y = 59;
        for (&(label, bytes), color) in rows.iter().zip(STAFF_PARTS) {
            swatch_row(d, (lx0, lx1), y, Some(color), label, gb(bytes));
            y += 10;
        }
        // Besides, like a tenant: its own pictures and boxes.
        d.push(Draw::rect(lx0 + 4, 130, lx1 - 4, 130, PANEL_LINE));
        let own = |part: Owner| {
            layout
                .entries
                .iter()
                .filter(|e| e.owner == part)
                .map(|e| e.bytes)
                .sum::<u64>()
        };
        let part = |known: bool, text: String| if known { text } else { "-".into() };
        swatch_row(
            d,
            (lx0, lx1),
            134,
            Some(FRAME),
            "GALLERY",
            part(
                building.gallery_bytes > 0,
                gb(own(Owner::Gallery(Holder::Staff))),
            ),
        );
        swatch_row(
            d,
            (lx0, lx1),
            143,
            Some(CARDBOARD),
            "BOXES",
            part(
                building.boxes_bytes > 0,
                format!("~{}", gb(own(Owner::Boxes(Holder::Staff)))),
            ),
        );
        d.push(Draw::rect(lx0 + 4, 153, lx1 - 4, 153, PANEL_LINE));
        let mut y = 157;
        for line in [
            "SHARED, OTHER: SHARED",
            "CODE AND FILES, PAGE",
            "TABLES, DRIVERS' LOCKS.",
        ] {
            d.push(Draw::text(lx0 + 5, y, line, TEXT_DIM));
            y += 9;
        }
    }
    button(
        d,
        regions,
        (lx0 + 79, lx1 - 5),
        198,
        "BACK",
        TEXT,
        Some(Target::Back),
    );
}

/// The Others card: everyone the rent roll does not list, largest first,
/// a page at a time. A row opens that tenant's details card, whose Back
/// returns here.
fn others_panel(
    d: &mut Vec<Draw>,
    regions: &mut Vec<Region>,
    building: &Building,
    layout: &Layout,
    selected: Option<usize>,
) {
    let (lx0, lx1) = panel_frame(d);
    let symbol = building.price.symbol.as_str();
    let estimate = building.rent_is_estimate;
    let column = |amount: Shown| amount.fit(symbol, estimate, Some(4));
    let roll = Roll::new(building, layout);
    let gb_right = lx0 + 121;
    let rent_right = lx1 - 5;
    d.push(Draw::rect(lx0 + 5, 15, lx0 + 9, 21, OTHERS_WALL));
    d.push(Draw::text(
        lx0 + 13,
        15,
        format!("OTHERS ({})", roll.rest.len()),
        GOLD,
    ));
    if let Some((_, _, rent)) = roll.others {
        d.push(Draw::text_right(
            lx1 - 5,
            15,
            rent.fit(symbol, estimate, None),
            TEXT,
            1,
        ));
    }
    d.push(Draw::text(lx0 + 5, 26, "EVERYONE NOT LISTED", TEXT_DIM));
    d.push(Draw::rect(lx0 + 4, 36, lx1 - 4, 36, PANEL_LINE));
    d.push(Draw::text(lx0 + 13, 40, "TENANT", TEXT_DIM));
    d.push(Draw::text_right(gb_right, 40, "GB", TEXT_DIM, 1));
    d.push(Draw::text_right(rent_right, 40, "RENT", TEXT_DIM, 1));

    let at = selected
        .and_then(|s| roll.rest.iter().position(|&t| t == s))
        .unwrap_or(0);
    let pages = roll.rest.len().div_ceil(OTHERS_PAGE).max(1);
    let page = at / OTHERS_PAGE;
    let mut y = 51;
    for &t in roll.rest.iter().skip(page * OTHERS_PAGE).take(OTHERS_PAGE) {
        let tenant = &building.tenants[t];
        if selected == Some(t) {
            d.push(Draw::rect(lx0 + 2, y - 2, lx1 - 2, y + 7, PANEL_LINE));
        }
        regions.push(Region {
            x0: lx0 + 1,
            y0: y - 2,
            x1: lx1 - 1,
            y1: y + 7,
            target: Target::Tenant(t),
        });
        let swatch = layout
            .hue_of(t)
            .map_or(OTHERS_WALL, |h| HUES[usize::from(h)].wall);
        d.push(Draw::rect(lx0 + 5, y, lx0 + 9, y + 6, swatch));
        d.push(Draw::text(
            lx0 + 13,
            y,
            cut(&tenant.name.to_uppercase(), 14),
            TEXT,
        ));
        d.push(Draw::text_right(
            gb_right,
            y,
            gb_text(tenant.rent_bytes),
            TEXT_DIM,
            1,
        ));
        d.push(Draw::text_right(
            rent_right,
            y,
            column(Shown::rent(micros(
                tenant.rent_bytes,
                building.price.per_gb_micros,
            ))),
            TEXT,
            1,
        ));
        y += 10;
    }
    if pages > 1 {
        button(
            d,
            regions,
            (lx0 + 5, lx0 + 74),
            198,
            &format!("MORE {}/{pages}", page + 1),
            TEXT,
            Some(Target::More),
        );
    }
    button(
        d,
        regions,
        (lx0 + 79, lx1 - 5),
        198,
        "BACK",
        TEXT,
        Some(Target::Back),
    );
}

/// The eviction dialog, in the rent roll's panel.
fn dialog_panel(d: &mut Vec<Draw>, regions: &mut Vec<Region>, eviction: &Eviction) {
    let (lx0, lx1) = panel_frame(d);
    d.push(Draw::text(lx0 + 5, 15, "EVICTION", GOLD));
    d.push(Draw::rect(lx0 + 4, 25, lx1 - 4, 25, PANEL_LINE));
    let mut y = 32;
    for line in eviction.lines(24) {
        d.push(Draw::text(lx0 + 5, y, line, TEXT));
        y += 10;
    }
    let label = |t: Target| match t {
        Target::CloseIt => ("CLOSE IT", ALERT),
        Target::Cancel => ("CANCEL", TEXT),
        Target::Force => ("FORCE EVICT", ALERT),
        Target::Leave => ("LEAVE IT", TEXT),
        _ => ("OK", TEXT),
    };
    let buttons = eviction.buttons();
    let spots = [(lx0 + 5, lx0 + 74), (lx0 + 79, lx1 - 5)];
    // One button sits on the right; two fill the row, primary first.
    let start = spots.len() - buttons.len().min(spots.len());
    for (k, &target) in buttons.iter().enumerate() {
        let (x0, x1) = spots[start + k];
        let (text, color) = label(target);
        button(d, regions, (x0, x1), 186, text, color, Some(target));
    }
    if !eviction.keys().is_empty() {
        d.push(Draw::text(lx0 + 5, 172, eviction.keys(), TEXT_DIM));
    }
}

/// One line of the card, top to bottom.
enum Line {
    Big(String),
    Center(String),
    Rule,
    Row {
        name: String,
        tag: Option<&'static str>,
        size: String,
        rent: String,
    },
    Total(Shown),
    Gap,
}

impl Line {
    fn height(&self) -> i32 {
        match self {
            Line::Big(_) => 20,
            Line::Center(_) | Line::Row { .. } => 10,
            Line::Rule => 8,
            Line::Total(_) => 19,
            Line::Gap => 5,
        }
    }
}

/// The rent roll card: a paper receipt on the night sky, 240 by 300, painted
/// over a `SKY` background.
pub fn card(building: &Building, layout: &Layout) -> Vec<Draw> {
    let symbol = building.price.symbol.as_str();
    let estimate = building.rent_is_estimate;
    // The card's rent column holds five characters.
    let column = |amount: Shown, estimate: bool| amount.fit(symbol, estimate, Some(5));
    let (x0, x1) = (22, 217);
    let date = building.reading_date();
    let roll = Roll::new(building, layout);
    let size = |bytes| format!("{} GB", gb_text(bytes));

    let mut lines = vec![
        Line::Big("RAMLORD".into()),
        Line::Center("RENT ROLL".into()),
        Line::Center(format!(
            "{} {} {} {}",
            date.weekday_name(),
            date.day,
            date.month_name(),
            date.year
        )),
        Line::Center(format!(
            "{} GB {} AT {}/GB",
            whole_gb(building.capacity_bytes),
            building.memory_type,
            price_text(building.price.per_gb_micros, symbol)
        )),
        Line::Rule,
    ];
    for row in &roll.rows {
        lines.push(Line::Row {
            name: row.name.clone(),
            tag: row.tag,
            size: size(row.bytes),
            rent: column(row.rent, estimate),
        });
    }
    if let Some((count, bytes, rent)) = roll.others {
        lines.push(Line::Row {
            name: format!("OTHERS ({count})"),
            tag: None,
            size: size(bytes),
            rent: column(rent, estimate),
        });
    }
    lines.push(Line::Rule);
    lines.push(Line::Total(roll.owed));
    lines.push(Line::Row {
        name: "VACANT".into(),
        tag: None,
        size: size(building.vacant_bytes),
        rent: column(roll.vacant, false),
    });
    lines.push(Line::Rule);
    let remark = cut(&building.remark, REMARK_MAX).to_uppercase();
    lines.extend(
        wrap_balanced(&remark, CARD_REMARK_WIDTH)
            .into_iter()
            .map(Line::Center),
    );
    lines.push(Line::Gap);
    lines.push(Line::Center("THEY LIVED RENT-FREE.".into()));
    lines.push(Line::Center("NOT ANYMORE.".into()));

    let body: i32 = lines.iter().map(Line::height).sum();
    let top = (CARD_HEIGHT as i32 - body - 22).div_euclid(2);
    let bottom = top + body + 16;

    let mut d = Vec::new();
    for (x, y) in [
        (8, 12),
        (12, 280),
        (230, 20),
        (228, 120),
        (10, 150),
        (232, 268),
        (12, 66),
        (229, 200),
        (6, 230),
    ] {
        d.push(Draw::dot(x, y, STAR));
    }
    d.push(Draw::rect(
        x0 + 2,
        top + 2,
        x1 + 2,
        bottom + 2,
        PAPER_SHADOW,
    ));
    d.push(Draw::rect(x0, top, x1, bottom, PAPER));
    for x in x0..=x1 {
        // A torn lower edge.
        if ((x - x0) / 3) % 2 == 0 {
            d.push(Draw::rect(x, bottom - 1, x, bottom + 2, SKY));
        }
    }
    let mut y = top + 8;
    for line in &lines {
        match line {
            Line::Big(text) => d.push(Draw::text_center(x0, x1, y, text.clone(), INK, 2)),
            Line::Center(text) => d.push(Draw::text_center(x0, x1, y, text.clone(), INK, 1)),
            Line::Rule => {
                for x in (x0 + 6..x1 - 5).step_by(4) {
                    d.push(Draw::rect(x, y + 2, x + 1, y + 2, PAPER_FAINT));
                }
            }
            Line::Row {
                name,
                tag,
                size,
                rent,
            } => {
                d.push(Draw::text(x0 + 6, y, name.clone(), INK));
                if let Some(tag) = tag {
                    let color = if *tag == "LEAK" {
                        PAPER_ALERT
                    } else {
                        PAPER_FAINT
                    };
                    d.push(Draw::text(x0 + 68, y, *tag, color));
                }
                d.push(Draw::text_right(x1 - 40, y, size.clone(), PAPER_FAINT, 1));
                d.push(Draw::text_right(x1 - 6, y, rent.clone(), INK, 1));
            }
            Line::Total(owed) => {
                d.push(Draw::Text {
                    x: x0 + 6,
                    y: y + 1,
                    text: "RENT OWED".into(),
                    color: INK,
                    shadow: None,
                    scale: 2,
                });
                total_amount(&mut d, x1 - 6, y + 1, *owed, symbol, estimate);
            }
            Line::Gap => {}
        }
        y += line.height();
    }
    d
}

/// RENT OWED on the card in double-size type. Cents, when the total has
/// any, follow in single size like a price tag, so the amount never runs
/// into the label.
fn total_amount(
    d: &mut Vec<Draw>,
    x_right: i32,
    y: i32,
    owed: Shown,
    symbol: &str,
    estimate: bool,
) {
    let tilde = if estimate { "~" } else { "" };
    let whole = format!("{tilde}{symbol}{}", owed.cents / 100);
    if !owed.with_cents {
        d.push(Draw::text_right(x_right, y, whole, INK, 2));
        return;
    }
    let cents = format!(".{:02}", owed.cents % 100);
    let cents_w = crate::font::text_width(cents.chars().count(), 1);
    d.push(Draw::text_right(x_right - cents_w - 1, y, whole, INK, 2));
    d.push(Draw::text_right(x_right, y, cents, INK, 1));
}

/// Rent of `bytes` in whole units: the sample table's Rent column.
pub fn whole_rent(building: &Building, bytes: u64) -> u64 {
    crate::format::whole_units(micros(bytes, building.price.per_gb_micros))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::test_building;
    use crate::format::MIB;
    use crate::layout::HueBook;

    const SAMPLE: &str = include_str!("../../../fixtures/sample-building.json");

    fn layout(b: &Building) -> Layout {
        Layout::new(b, &HueBook::new()).0
    }

    /// Every text drawn: (x, y, text, scale).
    fn texts(draws: &[Draw]) -> Vec<(i32, i32, String, i32)> {
        draws
            .iter()
            .filter_map(|d| match d {
                Draw::Text {
                    x, y, text, scale, ..
                } => Some((*x, *y, text.clone(), *scale)),
                _ => None,
            })
            .collect()
    }

    fn has_text(draws: &[Draw], wanted: &str) -> bool {
        texts(draws).iter().any(|t| t.2 == wanted)
    }

    #[test]
    fn labels_fall_back_from_name_and_rent_to_name_to_nothing() {
        let b = Building::from_json(SAMPLE).unwrap();
        let strips = label_strips(&b, &layout(&b));
        let labels: Vec<(&str, bool)> = strips
            .iter()
            .map(|(_, s)| (s.label.as_str(), s.arrow))
            .collect();
        assert_eq!(
            labels,
            [
                ("CHROME $92", false),
                ("DISCORD $42", true),
                ("VS CODE $32", false),
                ("TEAMS $23", false),
                ("STEAM $13", false),
                ("SLACK", false),
                ("SPOTIFY", false),
                ("COPILOT", false),
            ]
        );
        // WhatsApp (4 units, 44 pixels of room) and OneDrive (one unit per
        // floor) have no room for a name.
    }

    #[test]
    fn others_counts_named_tenants_beyond_the_tenth() {
        let names: Vec<String> = (0..12).map(|k| format!("APP{k:02}")).collect();
        let mut tenants: Vec<(&str, u64)> = names.iter().map(|n| (n.as_str(), 200 * MIB)).collect();
        tenants.push(("TINY", MIB));
        let b = test_building(1000 * MIB, &tenants, 4791 * MIB);
        let roll = Roll::new(&b, &layout(&b));
        assert_eq!(roll.rows.len(), ROLL_ROWS);
        let (count, bytes, _) = roll.others.unwrap();
        assert_eq!((count, bytes), (3, 401 * MIB));
        let shown = Shown::sum(
            roll.rows
                .iter()
                .map(|r| r.rent)
                .chain([roll.others.unwrap().2]),
        );
        assert_eq!(roll.owed, shown);
        assert!(has_text(&tower(&b, &layout(&b)), "OTHERS (3)"));
    }

    #[test]
    fn amounts_under_one_unit_show_cents_and_the_total_still_adds_up() {
        // 8 GiB, half a unit is 16 MiB: SMALL and SMALLER merge into
        // Others, 25 MiB, which is $0.35 at $14.34 per GB.
        let b = test_building(
            4000 * MIB,
            &[
                ("BIG", 3000 * MIB),
                ("SMALL", 15 * MIB),
                ("SMALLER", 10 * MIB),
            ],
            1167 * MIB,
        );
        let l = layout(&b);
        let roll = Roll::new(&b, &l);
        assert_eq!(roll.rows.len(), 1);
        assert_eq!(roll.rows[0].rent.text("$"), "$42");
        assert_eq!(roll.others.unwrap().2.text("$"), "$0.35");
        assert_eq!(roll.owed.text("$"), "$42.35");

        // The panel's four-character column drops the leading zero.
        let panel = tower(&b, &l);
        assert!(has_text(&panel, "$.35"));
        assert!(has_text(&panel, "$42.35"));

        // The card writes cents in full and puts the total's cents in
        // single size after double-size dollars, clear of the label.
        let receipt = card(&b, &l);
        assert!(has_text(&receipt, "$0.35"));
        let all = texts(&receipt);
        let dollars = all.iter().find(|t| t.2 == "$42" && t.3 == 2).unwrap();
        let cents = all.iter().find(|t| t.2 == ".35" && t.3 == 1).unwrap();
        let label = all.iter().find(|t| t.2 == "RENT OWED").unwrap();
        let label_right = label.0 + crate::font::text_width(9, 2) - 1;
        assert!(dollars.0 > label_right);
        assert!(cents.0 > dollars.0 + crate::font::text_width(3, 2) - 1);
    }

    #[test]
    fn a_large_total_with_cents_fits_on_the_card() {
        // RENT OWED $1234.56 would run into its label in double size.
        let b = test_building(
            GIB_100,
            &[("HUGE", 86 * (1 << 30)), ("SPECK", 39 * MIB)],
            GIB_100,
        );
        let receipt = card(&b, &layout(&b));
        let all = texts(&receipt);
        let label = all.iter().find(|t| t.2 == "RENT OWED").unwrap();
        let amount = all
            .iter()
            .find(|t| t.3 == 2 && t.2.starts_with('$'))
            .unwrap();
        assert!(
            amount.0 > label.0 + crate::font::text_width(9, 2),
            "{all:?}"
        );
    }
    const GIB_100: u64 = 100 << 30;

    #[test]
    fn for_rent_needs_six_vacant_units_in_a_row() {
        // 8 GiB: a unit is 32 MiB. Five vacant units: no sign.
        let b = test_building(1000 * MIB, &[("A", 7032 * MIB)], 160 * MIB);
        assert!(!has_text(&tower(&b, &layout(&b)), "FOR RENT"));
        let b = test_building(1000 * MIB, &[("A", 6992 * MIB)], 200 * MIB);
        assert!(has_text(&tower(&b, &layout(&b)), "FOR RENT"));
    }

    #[test]
    fn the_staff_sign_needs_three_units() {
        // The rent roll always has a STAFF row; the sign sits on the tower.
        let sign = |b: &Building| {
            texts(&tower(b, &layout(b)))
                .iter()
                .any(|t| t.2 == "STAFF" && t.0 < 224)
        };
        // 8 GiB: a unit is 32 MiB.
        assert!(!sign(&test_building(
            64 * MIB,
            &[("A", 4000 * MIB)],
            4128 * MIB
        )));
        assert!(sign(&test_building(
            96 * MIB,
            &[("A", 4000 * MIB)],
            4096 * MIB
        )));
    }

    #[test]
    fn remarks_are_cut_to_sixty_characters() {
        let mut b = Building::from_json(SAMPLE).unwrap();
        b.remark = "X".repeat(70);
        let l = layout(&b);
        assert!(has_text(&tower(&b, &l), &"X".repeat(60)));
        let lines: Vec<String> = texts(&card(&b, &l))
            .into_iter()
            .map(|t| t.2)
            .filter(|t| t.starts_with('X'))
            .collect();
        assert_eq!(lines, ["X".repeat(30), "X".repeat(30)]);
    }

    #[test]
    fn estimated_amounts_start_with_a_tilde_except_vacant() {
        let mut b = Building::from_json(SAMPLE).unwrap();
        b.rent_is_estimate = true;
        let l = layout(&b);
        let panel = tower(&b, &l);
        for text in [
            "CHROME ~$92",
            "~$92",
            "~$241",
            "~$78",
            "$141",
            "~$0.43",
            "~$1",
        ] {
            assert!(has_text(&panel, text), "panel lacks {text}");
        }
        assert!(!has_text(&panel, "~$141"));
        let receipt = card(&b, &l);
        for text in ["~$92", "~$241", "$141", "~$1"] {
            assert!(has_text(&receipt, text), "card lacks {text}");
        }
    }

    #[test]
    fn a_highlight_outlines_the_tenant_in_gold() {
        let b = Building::from_json(SAMPLE).unwrap();
        let l = layout(&b);
        let plain = tower(&b, &l);
        let lit = tower_highlighting(&b, &l, Some(0)); // Chrome
        let gold = |draws: &[Draw]| {
            draws
                .iter()
                .filter(|d| matches!(d, Draw::Rect { color, .. } if *color == GOLD))
                .count()
        };
        assert_eq!(gold(&plain), 0);
        assert!(gold(&lit) > 10);
        let frame = crate::render::rasterize(WIDTH, HEIGHT, SKY, &lit);
        // Chrome's lowest units sit on floor 2 from column 12; the edge
        // between Staff and Chrome above column 0 of floor 3 is gold.
        let (x, y) = cell_xy(3, 0);
        assert_eq!(frame.pixel(x as u32 + 5, (y + CELL_H - 1) as u32), GOLD);
    }

    #[test]
    fn boxes_take_a_line_of_the_rent_roll_and_the_basement() {
        let mut b = Building::from_json(SAMPLE).unwrap();
        b.boxes_bytes = 2048 << 20;
        b.staff_bytes -= 1024 << 20;
        b.vacant_bytes -= 1024 << 20;
        b.tenants[0].packed_bytes = 3000 << 20; // Chrome
        b.validate().unwrap();
        let l = layout(&b);
        let roll = Roll::new(&b, &l);
        // Nine tenants, the tenth joins Others, and BOXES has a line.
        assert_eq!(roll.rows.len(), ROLL_ROWS - 1);
        assert_eq!(roll.others.unwrap().0, 3);
        assert!(roll.boxes.is_some());
        let panel = tower(&b, &l);
        assert!(has_text(&panel, "BOXES"));
        assert!(has_text(&panel, "OTHERS (3)"));
        // Chrome's boxes are first in the basement, in Chrome's hue.
        assert_eq!(l.cells[0], Owner::Boxes(Holder::Tenant(0)));
        let frame = crate::render::rasterize(WIDTH, HEIGHT, SKY, &panel);
        let (x, y) = cell_xy(0, 5);
        assert_eq!(frame.pixel(x as u32 + 2, y as u32 + 6), HUES[0].wall);
        // The rent owed still adds up the rows shown.
        let shown = Shown::sum(
            roll.rows
                .iter()
                .map(|r| r.rent)
                .chain([roll.others.unwrap().2]),
        );
        assert_eq!(roll.owed, shown);
    }

    #[test]
    fn tags_follow_their_priority() {
        let mut b = Building::from_json(SAMPLE).unwrap();
        b.tenants[0].flags = vec![Flag::Walls, Flag::Away, Flag::Squatter, Flag::Leaker];
        assert_eq!(tag_of(&b.tenants[0]), Some("LEAK"));
        b.tenants[0].flags = vec![Flag::Walls, Flag::Away];
        assert_eq!(tag_of(&b.tenants[0]), Some("AWAY"));
        b.tenants[0].flags = vec![Flag::Family];
        assert_eq!(tag_of(&b.tenants[0]), None);
    }

    #[test]
    fn rent_roll_names_are_cut_to_eight_characters() {
        let mut b = Building::from_json(SAMPLE).unwrap();
        b.tenants[0].name = "Google Chrome".into();
        let l = layout(&b);
        assert_eq!(Roll::new(&b, &l).rows[0].name, "Google C");
        assert!(has_text(&card(&b, &l), "Google C"));
    }

    #[test]
    fn spans_read_as_words() {
        assert_eq!(ago_text(0), "JUST NOW");
        assert_eq!(ago_text(59_999), "JUST NOW");
        assert_eq!(ago_text(60_000), "1 MIN AGO");
        assert_eq!(ago_text(3 * 3_600_000), "3 H AGO");
        assert_eq!(ago_text(50 * 3_600_000), "2 D AGO");
    }
}
