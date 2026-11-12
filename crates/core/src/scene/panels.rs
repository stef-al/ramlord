//! The right-hand panel: the rent roll, the details card, the Staff and
//! Others cards, the settings screen and the eviction dialog. All share
//! one 154 by 204 pixel frame.

use crate::building::{Building, Flag, Tenant};
use crate::format::{Shown, cut, gb_text, micros, price_text, whole_mb};
use crate::layout::{Holder, Layout, Owner, box_share};
use crate::palette::*;
use crate::render::Draw;
use crate::ui::{Eviction, OTHERS_PAGE, Region, Target};

use super::Roll;

/// The rent roll panel: 224, 10, 154 by 204.
pub(super) fn rent_roll_panel(
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
            "{} {}{}/GB  {} {}",
            building.memory_type,
            if building.price.estimated { "~" } else { "" },
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
pub(super) fn details_panel(
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
    // The lease never reads less than what is held: with estimated rent
    // (the whole working set) it could.
    d.push(Draw::text(lx0 + 5, 40, "LEASE", TEXT_DIM));
    d.push(Draw::text_right(
        lx1 - 5,
        40,
        gb(tenant.lease_bytes.max(held)),
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
            part(
                building.gallery_bytes > 0,
                format!("~{}", gb(tenant.gpu_bytes)),
            ),
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
pub(super) fn staff_panel(
    d: &mut Vec<Draw>,
    regions: &mut Vec<Region>,
    building: &Building,
    layout: &Layout,
) {
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
                format!("~{}", gb(own(Owner::Gallery(Holder::Staff)))),
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
pub(super) fn others_panel(
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

/// What the settings screen shows: each row's label and value, the
/// selected row, the price being typed and the selected row's help.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SettingsView {
    pub rows: Vec<(String, String)>,
    pub at: usize,
    pub editing: Option<String>,
    pub help: [String; 2],
}

/// The settings screen (SPEC.md, Settings and state), in the rent roll's
/// panel: Up and Down move, Enter or a click changes the selected row, and
/// the price is typed in.
pub(super) fn settings_panel(d: &mut Vec<Draw>, regions: &mut Vec<Region>, view: &SettingsView) {
    let (lx0, lx1) = panel_frame(d);
    d.push(Draw::text(lx0 + 5, 15, "SETTINGS", GOLD));
    d.push(Draw::rect(lx0 + 4, 25, lx1 - 4, 25, PANEL_LINE));
    let mut y = 30;
    for (k, (label, value)) in view.rows.iter().enumerate() {
        if k == view.at {
            d.push(Draw::rect(lx0 + 2, y - 2, lx1 - 2, y + 7, PANEL_LINE));
        }
        regions.push(Region {
            x0: lx0 + 1,
            y0: y - 2,
            x1: lx1 - 1,
            y1: y + 7,
            target: Target::Setting(k as u8),
        });
        d.push(Draw::text(lx0 + 5, y, label.clone(), TEXT_DIM));
        match (&view.editing, k == view.at) {
            (Some(text), true) => {
                d.push(Draw::text_right(lx1 - 5, y, format!("{text}_"), GOLD, 1));
            }
            _ => d.push(Draw::text_right(lx1 - 5, y, value.clone(), TEXT, 1)),
        }
        y += 10;
    }
    d.push(Draw::rect(lx0 + 4, 132, lx1 - 4, 132, PANEL_LINE));
    for (k, line) in view.help.iter().enumerate() {
        d.push(Draw::text(
            lx0 + 5,
            136 + 9 * k as i32,
            line.clone(),
            TEXT_DIM,
        ));
    }
    let keys = if view.editing.is_some() {
        "TYPE, ENTER OK, ESC UNDO"
    } else {
        "ENTER CHANGES, ESC BACK"
    };
    d.push(Draw::text(lx0 + 5, 172, keys, TEXT_DIM));
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
pub(super) fn dialog_panel(d: &mut Vec<Draw>, regions: &mut Vec<Region>, eviction: &Eviction) {
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
