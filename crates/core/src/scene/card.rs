//! The rent roll card (SPEC.md, The rent roll card): a paper receipt on
//! the night sky, 240 by 300.

use crate::building::Building;
use crate::format::{Shown, cut, gb_text, price_text, whole_gb, wrap_balanced};
use crate::layout::Layout;
use crate::palette::*;
use crate::render::Draw;

use super::{CARD_HEIGHT, CARD_REMARK_WIDTH, REMARK_MAX, ROLL_ROWS, Roll};

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
    card_with(building, layout, false)
}

/// The rent roll card; `anonymous` names the tenants TENANT 1 to n
/// (SPEC.md, The rent roll card: privacy).
pub fn card_with(building: &Building, layout: &Layout, anonymous: bool) -> Vec<Draw> {
    let symbol = building.price.symbol.as_str();
    let estimate = building.rent_is_estimate;
    // The card's rent column holds five characters.
    let column = |amount: Shown, estimate: bool| amount.fit(symbol, estimate, Some(5));
    let (x0, x1) = (22, 217);
    let date = building.reading_date();
    let roll = Roll::listing(building, layout, ROLL_ROWS);
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
            "{} GB {} AT {}/GB{}",
            whole_gb(building.capacity_bytes),
            building.memory_type,
            price_text(building.price.per_gb_micros, symbol),
            if building.price.estimated {
                " EST."
            } else {
                ""
            }
        )),
        Line::Rule,
    ];
    for (k, row) in roll.rows.iter().enumerate() {
        lines.push(Line::Row {
            name: if anonymous {
                format!("TENANT {}", k + 1)
            } else {
                row.name.clone()
            },
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
