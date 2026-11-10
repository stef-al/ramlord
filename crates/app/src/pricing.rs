//! When to fetch the price file, and what a fetch changes (SPEC.md,
//! Prices). The fetch itself is the Windows layer's (WinHTTP); these rules
//! run and are tested everywhere.

use ramlord_core::prices::PriceTable;
use ramlord_core::settings::Settings;

/// The price file at most once a day.
pub const FETCH_EVERY_MS: u64 = 24 * 3_600_000;

/// Where the repository's price file lives.
pub const HOST: &str = "raw.githubusercontent.com";
pub const PATH: &str = "/stef-al/ramlord/main/prices.json";

/// Whether a fetch is due: fetching is on and there was no try yet, or the
/// last try (successful or not) was a day ago or more, or in the future (a
/// clock set back).
pub fn due(settings: &Settings, now_ms: u64) -> bool {
    settings.fetch_prices
        && (settings.price_checked_ms == 0
            || now_ms < settings.price_checked_ms
            || now_ms - settings.price_checked_ms >= FETCH_EVERY_MS)
}

/// What a fetch leaves: the table to bill at, and the text to keep in the
/// cache when it is newer. A failed or unreadable fetch, or an older file,
/// leaves `current` in place and writes nothing.
pub fn after_fetch(
    current: &PriceTable,
    fetched: Result<String, String>,
) -> (PriceTable, Option<String>) {
    let Ok(text) = fetched else {
        return (current.clone(), None);
    };
    let next = current.clone().refreshed(Some(&text));
    if next == *current {
        (next, None)
    } else {
        (next, Some(text))
    }
}

/// `rent-roll-YYYYMMDD-HHMMSS.png` (SPEC.md, The rent roll card).
pub fn card_file_name(
    year: u16,
    month: u16,
    day: u16,
    hour: u16,
    minute: u16,
    second: u16,
) -> String {
    format!("rent-roll-{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}.png")
}

#[cfg(test)]
mod tests {
    use super::*;

    const NEWER: &str =
        r#"{"as_of":"2026-12-01","currency":"USD","symbol":"$","per_gb":{"DDR4":7.0,"DDR5":15.0}}"#;

    #[test]
    fn a_fetch_is_due_once_a_day_when_on() {
        let mut s = Settings::default();
        assert!(due(&s, 1_000), "never checked");
        s.price_checked_ms = 1_000;
        assert!(!due(&s, 1_000 + FETCH_EVERY_MS - 1));
        assert!(due(&s, 1_000 + FETCH_EVERY_MS));
        assert!(due(&s, 500), "the clock went back");
        s.fetch_prices = false;
        assert!(!due(&s, 1_000 + 2 * FETCH_EVERY_MS));
    }

    #[test]
    fn a_failed_price_fetch_leaves_the_embedded_prices_in_place() {
        let embedded = PriceTable::embedded();
        for fetched in [
            Err("no network".to_string()),
            Err("HTTP 404".to_string()),
            Ok("<html>blocked by proxy</html>".to_string()),
            Ok(String::new()),
        ] {
            let (table, cache) = after_fetch(&embedded, fetched);
            assert_eq!(table, embedded);
            assert_eq!(cache, None);
        }
        let (table, cache) = after_fetch(&embedded, Ok(NEWER.into()));
        assert_eq!(table.as_of, "2026-12-01");
        assert_eq!(cache.as_deref(), Some(NEWER));
        // Fetched again, the same file changes nothing and writes nothing.
        let (again, cache) = after_fetch(&table, Ok(NEWER.into()));
        assert_eq!((again, cache), (table, None));
    }

    #[test]
    fn the_card_file_name_sorts_by_time() {
        assert_eq!(
            card_file_name(2026, 11, 10, 9, 5, 3),
            "rent-roll-20261110-090503.png"
        );
    }
}
