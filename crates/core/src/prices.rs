//! The price file (SPEC.md, Prices): price per GB by memory type, read
//! without floating point, and the lease a building is billed at. The exe
//! embeds `prices.json`; the Windows layer may hand a newer copy fetched
//! from the repository. Anything that does not parse is ignored and the
//! embedded prices stay.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::building::Price;
use crate::date::Date;
use crate::settings::Settings;
use crate::smbios::MemoryType;
use crate::tenants::Lease;

/// The repository's `prices.json`, built into the exe.
pub const EMBEDDED: &str = include_str!("../../../prices.json");

/// A parsed price file: price per GB in micro-units, by memory type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PriceTable {
    /// `YYYY-MM-DD`.
    pub as_of: String,
    pub currency: String,
    pub symbol: String,
    pub per_gb_micros: BTreeMap<String, u64>,
}

#[derive(Deserialize)]
struct RawTable {
    as_of: String,
    currency: String,
    symbol: String,
    #[serde(default)]
    #[allow(dead_code)]
    basis: String,
    per_gb: BTreeMap<String, serde_json::Number>,
}

/// A decimal amount such as `14.34` or `6` in micro-units (six decimals
/// at most). None for anything else: signs, exponents, empty text.
pub fn decimal_micros(text: &str) -> Option<u64> {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if whole.is_empty() || !digits(whole) || !digits(fraction) || fraction.len() > 6 {
        return None;
    }
    let whole: u64 = whole.parse().ok()?;
    let fraction: u64 = format!("{fraction:0<6}").parse().ok()?;
    whole.checked_mul(1_000_000)?.checked_add(fraction)
}

impl PriceTable {
    /// Parses and checks a price file: a valid date, a symbol, and a
    /// positive DDR4 and DDR5 price.
    pub fn parse(text: &str) -> Result<PriceTable, String> {
        let raw: RawTable = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if Date::parse(&raw.as_of).is_none() {
            return Err(format!("as_of {:?} is not a date", raw.as_of));
        }
        if raw.symbol.is_empty() || raw.symbol.chars().count() > 3 {
            return Err(format!("symbol {:?} must be 1 to 3 characters", raw.symbol));
        }
        let mut per_gb_micros = BTreeMap::new();
        for (kind, number) in raw.per_gb {
            let text = number.to_string();
            let micros = decimal_micros(&text)
                .filter(|&m| m > 0)
                .ok_or_else(|| format!("{kind}: {text} is not a price"))?;
            per_gb_micros.insert(kind, micros);
        }
        for needed in ["DDR4", "DDR5"] {
            if !per_gb_micros.contains_key(needed) {
                return Err(format!("no {needed} price"));
            }
        }
        Ok(PriceTable {
            as_of: raw.as_of,
            currency: raw.currency,
            symbol: raw.symbol,
            per_gb_micros,
        })
    }

    /// The prices built into the exe.
    pub fn embedded() -> PriceTable {
        PriceTable::parse(EMBEDDED).expect("the embedded prices.json is valid")
    }

    /// The newer of two tables by date; on a tie, `self`.
    pub fn newest(self, other: PriceTable) -> PriceTable {
        if other.as_of > self.as_of {
            other
        } else {
            self
        }
    }

    /// The newer of `self` and whatever `text` parses to; text that does
    /// not parse changes nothing.
    pub fn refreshed(self, text: Option<&str>) -> PriceTable {
        match text.map(PriceTable::parse) {
            Some(Ok(fetched)) => self.newest(fetched),
            _ => self,
        }
    }
}

/// The memory type a building is billed for: the one chosen in settings,
/// else the detected one.
pub fn billed_type(settings: &Settings, detected: Option<MemoryType>) -> Option<MemoryType> {
    settings.memory_type.or(detected)
}

/// What a live building is billed at. A custom price wins. Otherwise the
/// price file's price for the type: LPDDR at the matching DDR price and
/// DDR3 at the DDR4 price, both marked estimated. An unknown type is
/// billed as DDR4, estimated, under the label `RAM`, until the user picks
/// one.
pub fn lease(settings: &Settings, detected: Option<MemoryType>, table: &PriceTable) -> Lease {
    let memory = billed_type(settings, detected);
    let memory_type = memory.map_or("RAM", MemoryType::name).to_string();
    let price = match &settings.custom_price {
        Some(custom) => Price {
            per_gb_micros: custom.per_gb_micros,
            symbol: custom.symbol.clone(),
            as_of: custom.as_of.clone(),
            estimated: false,
        },
        None => {
            let (key, estimated) = memory.map_or(("DDR4", true), MemoryType::priced_as);
            Price {
                per_gb_micros: table.per_gb_micros[key],
                symbol: table.symbol.clone(),
                as_of: table.as_of.clone(),
                estimated,
            }
        }
    };
    Lease {
        memory_type,
        price,
        remark: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::CustomPrice;

    #[test]
    fn decimals_become_micros_without_floating_point() {
        assert_eq!(decimal_micros("14.34"), Some(14_340_000));
        assert_eq!(decimal_micros("6.53"), Some(6_530_000));
        assert_eq!(decimal_micros("12"), Some(12_000_000));
        assert_eq!(decimal_micros("0.000001"), Some(1));
        for bad in ["", ".5", "-1", "1e3", "1.2.3", "1.0000001", "abc"] {
            assert_eq!(decimal_micros(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_embedded_file_parses() {
        let t = PriceTable::embedded();
        assert_eq!(t.as_of, "2026-11-03");
        assert_eq!(t.symbol, "$");
        assert_eq!(t.per_gb_micros["DDR5"], 14_340_000);
        assert_eq!(t.per_gb_micros["DDR4"], 6_530_000);
    }

    #[test]
    fn a_failed_fetch_leaves_the_embedded_prices_in_place() {
        let embedded = PriceTable::embedded();
        // Nothing fetched, an error page, broken JSON, a file without DDR5,
        // a negative price, an older file: the embedded prices stay.
        for text in [
            None,
            Some("<html>404: Not Found</html>"),
            Some(r#"{"as_of": "#),
            Some(r#"{"as_of":"2026-12-01","currency":"USD","symbol":"$","per_gb":{"DDR4":6.0}}"#),
            Some(
                r#"{"as_of":"2026-12-01","currency":"USD","symbol":"$","per_gb":{"DDR4":-6.0,"DDR5":14.0}}"#,
            ),
            Some(
                r#"{"as_of":"2026-01-01","currency":"USD","symbol":"$","per_gb":{"DDR4":1.0,"DDR5":2.0}}"#,
            ),
        ] {
            assert_eq!(embedded.clone().refreshed(text), embedded, "{text:?}");
        }
        // A newer, valid file wins.
        let newer = r#"{"as_of":"2026-12-01","currency":"USD","symbol":"$","basis":"x","per_gb":{"DDR4":7.1,"DDR5":15.25}}"#;
        let t = embedded.refreshed(Some(newer));
        assert_eq!(t.as_of, "2026-12-01");
        assert_eq!(t.per_gb_micros["DDR5"], 15_250_000);
    }

    #[test]
    fn leases_by_type_estimates_and_custom_prices() {
        let table = PriceTable::embedded();
        let auto = Settings::default();
        let ddr5 = lease(&auto, Some(MemoryType::Ddr5), &table);
        assert_eq!(
            (
                ddr5.memory_type.as_str(),
                ddr5.price.per_gb_micros,
                ddr5.price.estimated
            ),
            ("DDR5", 14_340_000, false)
        );
        let lp = lease(&auto, Some(MemoryType::Lpddr5), &table);
        assert_eq!(
            (
                lp.memory_type.as_str(),
                lp.price.per_gb_micros,
                lp.price.estimated
            ),
            ("LPDDR5", 14_340_000, true)
        );
        let ddr3 = lease(&auto, Some(MemoryType::Ddr3), &table);
        assert_eq!(
            (ddr3.price.per_gb_micros, ddr3.price.estimated),
            (6_530_000, true)
        );
        let unknown = lease(&auto, None, &table);
        assert_eq!(
            (unknown.memory_type.as_str(), unknown.price.estimated),
            ("RAM", true)
        );
        // A chosen type beats the detected one; a custom price beats both.
        let chosen = Settings {
            memory_type: Some(MemoryType::Ddr4),
            ..Settings::default()
        };
        assert_eq!(
            lease(&chosen, Some(MemoryType::Ddr5), &table).memory_type,
            "DDR4"
        );
        let custom = Settings {
            custom_price: Some(CustomPrice {
                per_gb_micros: 5_200_000,
                symbol: "€".into(),
                as_of: "2026-11-10".into(),
            }),
            ..chosen
        };
        let l = lease(&custom, Some(MemoryType::Ddr5), &table);
        assert_eq!(
            (
                l.price.symbol.as_str(),
                l.price.per_gb_micros,
                l.price.as_of.as_str()
            ),
            ("€", 5_200_000, "2026-11-10")
        );
    }
}
