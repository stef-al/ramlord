//! Scenes: what to draw for the tower window and the rent roll card.
//!
//! The geometry follows `docs/reference/prototype.py`; the rules follow
//! SPEC.md (The tower; Behaviors, The rent roll card).

use crate::building::{Building, Flag, Tenant};
use crate::font::{ADVANCE, UP_ARROW};
use crate::format::{Shown, cut, gb_text, micros, price_text, whole_gb, whole_mb, wrap_balanced};
use crate::layout::{COLS, FLOORS, Layout, Owner};
use crate::palette::*;
use crate::render::Draw;

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
}

impl Roll {
    pub fn new(building: &Building, layout: &Layout) -> Roll {
        let price = building.price.per_gb_micros;
        let rent = |bytes| Shown::rent(micros(bytes, price));
        let rows: Vec<RollRow> = layout
            .named
            .iter()
            .take(ROLL_ROWS)
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
        let rest: Vec<usize> = layout
            .named
            .iter()
            .skip(ROLL_ROWS)
            .map(|n| n.tenant)
            .chain(layout.merged.iter().copied())
            .collect();
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
        let full = format!("{name} {}", rent.text(symbol));
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
    let mut d = Vec::new();
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
        if !in_strip {
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
    // STAFF on the ground floor, when Staff's ground segment has room.
    if let Some(seg) = layout.segments(Owner::Staff).first()
        && seg.floor == 0
        && seg.first_col == 0
        && seg.width() >= 3
    {
        let (x, y) = cell_xy(0, 0);
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

    rent_roll_panel(&mut d, building, layout);
    d
}

/// The rent roll panel: 224, 10, 154 by 204.
fn rent_roll_panel(d: &mut Vec<Draw>, building: &Building, layout: &Layout) {
    let symbol = building.price.symbol.as_str();
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
            row.rent.text_narrow(symbol),
            TEXT,
            1,
        ));
        y += 10;
    }
    if let Some((count, bytes, rent)) = roll.others {
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
            rent.text_narrow(symbol),
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
        roll.owed.text(symbol),
        GOLD,
        1,
    ));
    y += 12;
    for (swatch, label, bytes, rent, color) in [
        (
            STAFF_WALL,
            "STAFF",
            building.staff_bytes,
            roll.staff,
            TEXT_DIM,
        ),
        (
            VACANT_WINDOW,
            "VACANT",
            building.vacant_bytes,
            roll.vacant,
            GOOD,
        ),
    ] {
        d.push(Draw::rect(lx0 + 5, y, lx0 + 9, y + 6, swatch));
        d.push(Draw::text(lx0 + 13, y, label, TEXT_DIM));
        d.push(Draw::text_right(gb_right, y, gb_text(bytes), TEXT_DIM, 1));
        d.push(Draw::text_right(
            rent_right,
            y,
            rent.text_narrow(symbol),
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
        roll.office.text(symbol),
        TEXT_DIM,
        1,
    ));
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
            rent: row.rent.text(symbol),
        });
    }
    if let Some((count, bytes, rent)) = roll.others {
        lines.push(Line::Row {
            name: format!("OTHERS ({count})"),
            tag: None,
            size: size(bytes),
            rent: rent.text(symbol),
        });
    }
    lines.push(Line::Rule);
    lines.push(Line::Total(roll.owed));
    lines.push(Line::Row {
        name: "VACANT".into(),
        tag: None,
        size: size(building.vacant_bytes),
        rent: roll.vacant.text(symbol),
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
                total_amount(&mut d, x1 - 6, y + 1, *owed, symbol);
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
fn total_amount(d: &mut Vec<Draw>, x_right: i32, y: i32, owed: Shown, symbol: &str) {
    let whole = format!("{symbol}{}", owed.cents / 100);
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
}
