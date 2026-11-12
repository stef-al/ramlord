//! Scenes: what to draw for the tower window and the rent roll card.
//!
//! The geometry follows `docs/reference/prototype.py`; the rules follow
//! SPEC.md (The tower; Behaviors, The rent roll card).
//!
//! - `tower` draws the building and its status lines;
//! - `panels` draws what the right-hand panel shows;
//! - `card` draws the rent roll card;
//! - `icons` draws the tray icon and the app icon.
//!
//! This module holds what they share: the canvas sizes, the rent roll's
//! numbers (`Roll`) and the whole window with its click regions.

mod card;
mod icons;
mod panels;
mod tower;

pub use card::{card, card_with};
pub use icons::{APP_ICON_SIZES, TRAY_SIZE, app_icon, tray_icon};
pub use panels::{Details, SettingsView, ago_text, flags_in_words, span_text};

use crate::building::{Building, Flag, Tenant};
use crate::font::ADVANCE;
use crate::format::{Shown, cut, micros};
use crate::layout::{COLS, Holder, Layout};
use crate::palette::TEXT;
use crate::render::Draw;
use crate::ui::{Eviction, Region, Target};
use panels::{
    details_panel, dialog_panel, others_panel, rent_roll_panel, settings_panel, staff_panel,
};
use tower::{CELL_H, CELL_W, building_scene, cell_xy, outline_holder};

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

/// Rows listed by name in the rent roll.
pub const ROLL_ROWS: usize = 10;
/// Characters of a name in the rent roll.
pub const ROLL_NAME: usize = 8;
/// Characters of a remark.
pub const REMARK_MAX: usize = 60;
/// Characters per line of the remark on the card.
pub const CARD_REMARK_WIDTH: usize = 30;

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
    /// The rent roll as the panel shows it: for each line below the total
    /// (BOXES, GALLERY), one tenant row less fits.
    pub fn new(building: &Building, layout: &Layout) -> Roll {
        let listed = ROLL_ROWS
            - usize::from(building.boxes_bytes > 0)
            - usize::from(building.gallery_bytes > 0);
        Roll::listing(building, layout, listed)
    }

    /// The rent roll listing at most `listed` tenants by name: the card
    /// always has room for ten.
    pub fn listing(building: &Building, layout: &Layout, listed: usize) -> Roll {
        let price = building.price.per_gb_micros;
        let rent = |bytes| Shown::rent(micros(bytes, price));
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
    /// The settings screen.
    Settings(&'a SettingsView),
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
        Panel::Settings(view) => settings_panel(&mut d, &mut regions, view),
    }
    (d, regions)
}

/// Rent of `bytes` in whole units: the sample table's Rent column.
pub fn whole_rent(building: &Building, bytes: u64) -> u64 {
    crate::format::whole_units(micros(bytes, building.price.per_gb_micros))
}

#[cfg(test)]
mod tests {
    use super::tower::label_strips;
    use super::*;
    use crate::building::test_building;
    use crate::format::MIB;
    use crate::layout::{HueBook, Owner};
    use crate::palette::*;

    const SAMPLE: &str = include_str!("../../../../fixtures/sample-building.json");

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
