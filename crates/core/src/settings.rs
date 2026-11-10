//! Settings (SPEC.md, Settings and state): what `%APPDATA%\RAMlord\
//! settings.json` holds, the rows of the settings screen (F3) and how
//! Enter changes each. Keys this version does not know are kept.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::prices::decimal_micros;
use crate::smbios::MemoryType;

/// A price per GB the user typed.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct CustomPrice {
    pub per_gb_micros: u64,
    /// One to three characters, shown before every amount.
    pub symbol: String,
    /// `YYYY-MM-DD`: the day it was typed, for the price line.
    pub as_of: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    /// None: automatic, the largest of 2 to 5 that fits. Else 2 to 5.
    pub scale: Option<u32>,
    pub always_on_top: bool,
    /// The close button quits instead of hiding to the tray.
    pub close_quits: bool,
    /// Mirrors the per-user Run key, which is the truth.
    pub start_with_windows: bool,
    /// None: as detected.
    pub memory_type: Option<MemoryType>,
    pub custom_price: Option<CustomPrice>,
    pub fetch_prices: bool,
    /// AWAY after this many watched hours, 1 to 168.
    pub away_hours: u32,
    /// Tenant ids marked Family.
    pub family: BTreeSet<String>,
    /// The rent roll card says TENANT 1 to n instead of names.
    pub anonymous: bool,
    /// When the price file was last fetched or tried (UTC ms); not shown.
    pub price_checked_ms: u64,
    /// Keys this version does not know, kept as they were.
    #[serde(flatten)]
    pub other: BTreeMap<String, serde_json::Value>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            scale: None,
            always_on_top: false,
            close_quits: false,
            start_with_windows: false,
            memory_type: None,
            custom_price: None,
            fetch_prices: true,
            away_hours: 4,
            family: BTreeSet::new(),
            anonymous: false,
            price_checked_ms: 0,
            other: BTreeMap::new(),
        }
    }
}

/// The settings screen's rows, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    Scale,
    OnTop,
    CloseButton,
    StartWithWindows,
    MemoryType,
    Price,
    FetchPrices,
    AwayAfter,
    Family,
    Anonymous,
}

pub const ROWS: [Row; 10] = [
    Row::Scale,
    Row::OnTop,
    Row::CloseButton,
    Row::StartWithWindows,
    Row::MemoryType,
    Row::Price,
    Row::FetchPrices,
    Row::AwayAfter,
    Row::Family,
    Row::Anonymous,
];

/// The away thresholds Enter steps through, in hours.
const AWAY_STEPS: [u32; 8] = [1, 2, 4, 8, 12, 24, 48, 168];

impl Row {
    pub fn label(self) -> &'static str {
        match self {
            Row::Scale => "SCALE",
            Row::OnTop => "ALWAYS ON TOP",
            Row::CloseButton => "CLOSE BUTTON",
            Row::StartWithWindows => "WITH WINDOWS",
            Row::MemoryType => "MEMORY TYPE",
            Row::Price => "PRICE/GB",
            Row::FetchPrices => "FETCH PRICES",
            Row::AwayAfter => "AWAY AFTER",
            Row::Family => "FAMILY",
            Row::Anonymous => "ANONYMOUS F2",
        }
    }

    /// Two lines under the list for the selected row, 24 characters each.
    pub fn help(self) -> [&'static str; 2] {
        match self {
            Row::Scale => ["AUTO FITS 70% OF THE", "SCREEN; OR 2X TO 5X."],
            Row::OnTop => ["KEEPS THIS WINDOW ABOVE", "ALL THE OTHERS."],
            Row::CloseButton => ["X HIDES TO THE TRAY,", "OR QUITS RAMLORD."],
            Row::StartWithWindows => ["STARTS HIDDEN AT SIGN-IN;", "SQUATTERS NEED IT."],
            Row::MemoryType => ["AUTO IS WHAT THE PC SAYS.", "LPDDR AND DDR3: EST."],
            Row::Price => ["ENTER, TYPE A PRICE AND", "SYMBOL. EMPTY: THE FILE."],
            Row::FetchPrices => ["ONCE A DAY FROM GITHUB;", "OFF: BUILT-IN PRICES."],
            Row::AwayAfter => ["UNSEEN THIS LONG, A", "TENANT IS TAGGED AWAY."],
            Row::Family => ["SET ON A TENANT'S CARD;", "NEVER FLAGGED, NO EVICT."],
            Row::Anonymous => ["THE F2 CARD SAYS TENANT", "1, 2, 3 INSTEAD OF NAMES."],
        }
    }
}

fn on_off(on: bool) -> &'static str {
    if on { "ON" } else { "OFF" }
}

/// `4 H`, `1 D`, `7 D`.
pub fn hours_text(hours: u32) -> String {
    if hours >= 24 && hours % 24 == 0 {
        format!("{} D", hours / 24)
    } else {
        format!("{hours} H")
    }
}

impl Settings {
    /// The value shown for `row`. `detected` is the memory type the PC
    /// reports; `file_price` the price file's price as shown.
    pub fn value_text(&self, row: Row, detected: Option<MemoryType>, file_price: &str) -> String {
        match row {
            Row::Scale => self.scale.map_or("AUTO".into(), |s| format!("{s}X")),
            Row::OnTop => on_off(self.always_on_top).into(),
            Row::CloseButton => if self.close_quits { "QUITS" } else { "TO TRAY" }.into(),
            Row::StartWithWindows => on_off(self.start_with_windows).into(),
            Row::MemoryType => match self.memory_type {
                Some(m) => m.name().into(),
                None => format!("AUTO {}", detected.map_or("?", MemoryType::name)),
            },
            Row::Price => match &self.custom_price {
                Some(c) => format!(
                    "OWN {}{}.{:02}",
                    c.symbol,
                    c.per_gb_micros / 1_000_000,
                    c.per_gb_micros % 1_000_000 / 10_000
                ),
                None => format!("FILE {file_price}"),
            },
            Row::FetchPrices => on_off(self.fetch_prices).into(),
            Row::AwayAfter => hours_text(self.away_hours),
            Row::Family => match self.family.len() {
                0 => "NONE".into(),
                1 => "1 APP".into(),
                n => format!("{n} APPS"),
            },
            Row::Anonymous => on_off(self.anonymous).into(),
        }
    }

    /// The settings after Enter on `row`: the next value in its cycle.
    /// The price is typed instead, and Family is set on the cards, so
    /// those two rows do not change here.
    pub fn changed(&self, row: Row) -> Settings {
        let mut s = self.clone();
        match row {
            Row::Scale => {
                s.scale = match s.scale {
                    None => Some(2),
                    Some(5) => None,
                    Some(n) => Some(n + 1),
                }
            }
            Row::OnTop => s.always_on_top = !s.always_on_top,
            Row::CloseButton => s.close_quits = !s.close_quits,
            Row::StartWithWindows => s.start_with_windows = !s.start_with_windows,
            Row::MemoryType => {
                s.memory_type = match s.memory_type {
                    None => Some(MemoryType::ALL[0]),
                    Some(m) => {
                        let at = MemoryType::ALL.iter().position(|&k| k == m).unwrap_or(0);
                        MemoryType::ALL.get(at + 1).copied()
                    }
                }
            }
            Row::FetchPrices => s.fetch_prices = !s.fetch_prices,
            Row::AwayAfter => {
                let next = AWAY_STEPS.iter().find(|&&h| h > s.away_hours);
                s.away_hours = *next.unwrap_or(&AWAY_STEPS[0]);
            }
            Row::Anonymous => s.anonymous = !s.anonymous,
            Row::Price | Row::Family => {}
        }
        s
    }
}

/// A typed price such as `€5.20`, `5,20 €`, `130 Kč` or `12`: an amount
/// with up to two decimals (point or comma) and a symbol of one to three
/// characters before or after it; without one, `default_symbol`. Empty
/// text means no custom price. None when it cannot be read.
pub fn parse_custom_price(
    text: &str,
    default_symbol: &str,
    today: &str,
) -> Option<Option<CustomPrice>> {
    let text = text.trim();
    if text.is_empty() {
        return Some(None);
    }
    let is_number = |c: char| c.is_ascii_digit() || c == '.' || c == ',';
    let start = text.find(|c: char| c.is_ascii_digit())?;
    let end = text[start..]
        .find(|c: char| !is_number(c))
        .map_or(text.len(), |k| start + k);
    let amount = text[start..end].replace(',', ".");
    let (whole, cents) = amount.split_once('.').unwrap_or((&amount, ""));
    if cents.len() > 2 {
        return None;
    }
    let micros = decimal_micros(&format!("{whole}.{cents}0"))?;
    let symbol: String = format!("{}{}", &text[..start], &text[end..])
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let symbol = if symbol.is_empty() {
        default_symbol.to_string()
    } else {
        symbol
    };
    if micros == 0 || symbol.chars().count() > 3 {
        return None;
    }
    Some(Some(CustomPrice {
        per_gb_micros: micros,
        symbol,
        as_of: today.to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_with_only_family_still_reads_and_unknown_keys_stay() {
        let s: Settings =
            serde_json::from_str(r#"{"family": ["a.exe"], "future_key": [1, 2]}"#).unwrap();
        assert_eq!(s.family.len(), 1);
        assert!(s.fetch_prices, "defaults fill the rest");
        assert_eq!(s.away_hours, 4);
        let back = serde_json::to_value(&s).unwrap();
        assert_eq!(back["future_key"], serde_json::json!([1, 2]));
        assert_eq!(back["memory_type"], serde_json::Value::Null);
        let typed: Settings =
            serde_json::from_str(r#"{"memory_type": "LPDDR5", "scale": 3}"#).unwrap();
        assert_eq!(typed.memory_type, Some(MemoryType::Lpddr5));
        assert_eq!(typed.scale, Some(3));
    }

    #[test]
    fn enter_cycles_every_row() {
        let s = Settings::default();
        assert_eq!(s.changed(Row::Scale).scale, Some(2));
        assert_eq!(
            Settings {
                scale: Some(5),
                ..Settings::default()
            }
            .changed(Row::Scale)
            .scale,
            None
        );
        assert!(s.changed(Row::OnTop).always_on_top);
        assert!(s.changed(Row::CloseButton).close_quits);
        assert!(s.changed(Row::StartWithWindows).start_with_windows);
        assert!(!s.changed(Row::FetchPrices).fetch_prices);
        assert!(s.changed(Row::Anonymous).anonymous);
        assert_eq!(s.changed(Row::AwayAfter).away_hours, 8);
        let mut types = vec![];
        let mut t = s.clone();
        for _ in 0..6 {
            t = t.changed(Row::MemoryType);
            types.push(t.memory_type);
        }
        assert_eq!(types[0], Some(MemoryType::Ddr3));
        assert_eq!(types[5], None, "after LPDDR5 comes AUTO again");
        assert_eq!(s.changed(Row::Price), s);
        assert_eq!(s.changed(Row::Family), s);
    }

    #[test]
    fn values_read_as_the_screen_shows_them() {
        let s = Settings::default();
        assert_eq!(s.value_text(Row::Scale, None, "$6.53"), "AUTO");
        assert_eq!(
            s.value_text(Row::MemoryType, Some(MemoryType::Ddr4), "$6.53"),
            "AUTO DDR4"
        );
        assert_eq!(s.value_text(Row::MemoryType, None, "$6.53"), "AUTO ?");
        assert_eq!(s.value_text(Row::Price, None, "$6.53"), "FILE $6.53");
        assert_eq!(s.value_text(Row::AwayAfter, None, ""), "4 H");
        assert_eq!(hours_text(168), "7 D");
        for row in ROWS {
            let (label, value) = (
                row.label(),
                s.value_text(row, Some(MemoryType::Lpddr5), "$14.34"),
            );
            assert!(
                label.len() + 1 + value.chars().count() <= 24,
                "{label} {value}"
            );
            for line in row.help() {
                assert!(line.chars().count() <= 25, "{line}");
            }
        }
    }

    #[test]
    fn typed_prices_with_symbols_before_or_after() {
        let p = |t: &str| parse_custom_price(t, "$", "2026-11-10");
        let price = |t: &str| p(t).unwrap().unwrap();
        assert_eq!(price("€5.20").per_gb_micros, 5_200_000);
        assert_eq!(price("€5.20").symbol, "€");
        assert_eq!(price("5,2 €").per_gb_micros, 5_200_000);
        assert_eq!(price("130 Kč").symbol, "Kč");
        assert_eq!(price("130 Kč").per_gb_micros, 130_000_000);
        assert_eq!(price("12").symbol, "$");
        assert_eq!(price("12").as_of, "2026-11-10");
        assert_eq!(p("  "), Some(None), "empty: back to the file");
        for bad in ["abc", "€", "1.234", "0", "5 EURO", "1.2.3"] {
            assert_eq!(p(bad), None, "{bad}");
        }
    }
}
