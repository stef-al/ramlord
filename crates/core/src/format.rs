//! Money, sizes and text in integer arithmetic only.
//!
//! Money is integer micro-units: price per GB times 1,000,000. Sizes are
//! binary, as in Windows: 1 MB is 2^20 bytes and 1 GB is 2^30 bytes.

pub const MIB: u64 = 1 << 20;
pub const GIB: u64 = 1 << 30;

/// The money `bytes` cost at `per_gb_micros`, in micro-units (rounded down).
pub fn micros(bytes: u64, per_gb_micros: u64) -> u64 {
    let exact = u128::from(bytes) * u128::from(per_gb_micros) / u128::from(GIB);
    u64::try_from(exact).unwrap_or(u64::MAX)
}

/// An amount of money as it is shown, so the rent roll can add up exactly
/// what it shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shown {
    /// The shown amount in hundredths of the currency unit.
    pub cents: u64,
    /// Shown with two decimals, such as `$0.43`, rather than `$92`.
    pub with_cents: bool,
}

impl Shown {
    /// Rent as SPEC.md shows it: whole units with halves rounded up, and
    /// cents when the amount is under one unit (after rounding to cents).
    pub fn rent(micros: u64) -> Shown {
        let cents = micros.saturating_add(5_000) / 10_000;
        if cents < 100 {
            Shown {
                cents,
                with_cents: true,
            }
        } else {
            Shown {
                cents: micros.saturating_add(500_000) / 1_000_000 * 100,
                with_cents: false,
            }
        }
    }

    /// Always in cents, halves rounded up: the `MY OFFICE` line.
    pub fn in_cents(micros: u64) -> Shown {
        Shown {
            cents: micros.saturating_add(5_000) / 10_000,
            with_cents: true,
        }
    }

    /// The sum of shown amounts, so a total always adds up. It carries
    /// cents only when the cents do not cancel out.
    pub fn sum(items: impl IntoIterator<Item = Shown>) -> Shown {
        let cents = items
            .into_iter()
            .fold(0u64, |sum, s| sum.saturating_add(s.cents));
        Shown {
            cents,
            with_cents: cents % 100 != 0,
        }
    }

    /// `$92`, `$0.43` or `$241.40`.
    pub fn text(self, symbol: &str) -> String {
        if self.with_cents {
            format!("{symbol}{}.{:02}", self.cents / 100, self.cents % 100)
        } else {
            format!("{symbol}{}", self.cents / 100)
        }
    }

    /// Like `text`, but an amount under one unit drops its leading zero
    /// (`$.43`), so it fits the rent roll panel's four-character column.
    pub fn text_narrow(self, symbol: &str) -> String {
        if self.with_cents && self.cents < 100 {
            format!("{symbol}.{:02}", self.cents)
        } else {
            self.text(symbol)
        }
    }
}

/// Whole currency units, halves rounded up.
pub fn whole_units(micros: u64) -> u64 {
    micros.saturating_add(500_000) / 1_000_000
}

/// The price per GB with two decimals, truncated: `$14.34`.
pub fn price_text(per_gb_micros: u64, symbol: &str) -> String {
    format!(
        "{symbol}{}.{:02}",
        per_gb_micros / 1_000_000,
        per_gb_micros % 1_000_000 / 10_000
    )
}

/// GB to one decimal, halves rounded up: `6.4`.
pub fn gb_text(bytes: u64) -> String {
    let tenths = (u128::from(bytes) * 10 + u128::from(GIB / 2)) / u128::from(GIB);
    format!("{}.{}", tenths / 10, tenths % 10)
}

/// Whole GB, halves rounded up: the building size on the card.
pub fn whole_gb(bytes: u64) -> u64 {
    ((u128::from(bytes) + u128::from(GIB / 2)) / u128::from(GIB)) as u64
}

/// Whole MB, rounded down: the `MY OFFICE` line.
pub fn whole_mb(bytes: u64) -> u64 {
    bytes / MIB
}

/// The first `max` characters of `text`.
pub fn cut(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// Wraps `text` into as few lines of at most `width` characters as possible,
/// with lengths as even as possible (smallest spread between the longest and
/// the shortest line). Among equally even splits the first one found wins,
/// breaking as early as possible. Words longer than `width` are cut.
pub fn wrap_balanced(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut words: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        let chars: Vec<char> = word.chars().collect();
        words.extend(chars.chunks(width).map(|c| c.iter().collect::<String>()));
    }
    if words.is_empty() {
        return Vec::new();
    }
    let total = words.iter().map(|w| w.chars().count()).sum::<usize>() + words.len() - 1;
    let mut count = total.div_ceil(width);
    loop {
        let mut best: Option<(usize, Vec<String>)> = None;
        search(&words, width, count, 0, &mut Vec::new(), &mut best);
        if let Some((_, lines)) = best {
            return lines;
        }
        count += 1;
    }
}

fn search(
    words: &[String],
    width: usize,
    count: usize,
    start: usize,
    lines: &mut Vec<String>,
    best: &mut Option<(usize, Vec<String>)>,
) {
    if lines.len() + 1 == count {
        if start < words.len() {
            let last = words[start..].join(" ");
            if last.chars().count() <= width {
                lines.push(last);
                let lengths = lines.iter().map(|l| l.chars().count());
                let spread = lengths.clone().max().unwrap_or(0) - lengths.min().unwrap_or(0);
                if best.as_ref().is_none_or(|(b, _)| spread < *b) {
                    *best = Some((spread, lines.clone()));
                }
                lines.pop();
            }
        }
        return;
    }
    for end in start + 1..words.len() {
        let line = words[start..end].join(" ");
        if line.chars().count() > width {
            break;
        }
        lines.push(line);
        search(words, width, count, end, lines, best);
        lines.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DDR5: u64 = 14_340_000;

    #[test]
    fn whole_units_round_halves_up() {
        assert_eq!(whole_units(1_499_999), 1);
        assert_eq!(whole_units(1_500_000), 2);
        // Exactly half a unit: half a GB at one unit per GB.
        assert_eq!(whole_units(micros(GIB / 2, 1_000_000)), 1);
        assert_eq!(whole_units(micros(GIB / 2 - 1, 1_000_000)), 0);
    }

    #[test]
    fn rent_shows_whole_units_and_cents_under_one() {
        assert_eq!(Shown::rent(91_780_000).text("$"), "$92");
        assert_eq!(Shown::rent(434_000).text("$"), "$0.43");
        assert_eq!(Shown::rent(434_000).text_narrow("$"), "$.43");
        assert_eq!(Shown::rent(4_999).text("$"), "$0.00");
        assert_eq!(Shown::rent(5_000).text("$"), "$0.01");
        // Rounds to a full unit in cents, so it is shown as whole units.
        assert_eq!(Shown::rent(995_000).text("$"), "$1");
        assert_eq!(Shown::rent(994_999).text("$"), "$0.99");
        assert_eq!(Shown::rent(1_499_999).text_narrow("$"), "$1");
    }

    #[test]
    fn totals_add_up_what_is_shown() {
        let rows = [Shown::rent(91_780_000), Shown::rent(434_000)];
        let total = Shown::sum(rows);
        assert_eq!(total.text("$"), "$92.43");
        let whole = Shown::sum([Shown::rent(91_780_000), Shown::rent(1_000_000)]);
        assert_eq!(whole.text("$"), "$93");
        let cancel = Shown::sum([Shown::rent(500_000), Shown::rent(500_000)]);
        assert_eq!(cancel.text("$"), "$1");
    }

    #[test]
    fn my_office_is_in_cents() {
        assert_eq!(Shown::in_cents(micros(31 * MIB, DDR5)).text("$"), "$0.43");
        assert_eq!(Shown::in_cents(1_120_000).text("$"), "$1.12");
    }

    #[test]
    fn money_does_not_overflow_on_huge_inputs() {
        assert_eq!(micros(u64::MAX, u64::MAX), u64::MAX);
        assert!(!Shown::rent(u64::MAX).with_cents);
        assert_eq!(micros(1 << 40, DDR5), 1024 * DDR5);
    }

    #[test]
    fn sizes() {
        assert_eq!(gb_text(6_554 * MIB), "6.4");
        assert_eq!(gb_text(72 * MIB), "0.1");
        // 0.05 GB is 53,687,091.2 bytes: from the next whole byte up, it rounds up.
        assert_eq!(gb_text(53_687_092), "0.1");
        assert_eq!(gb_text(53_687_091), "0.0");
        assert_eq!(gb_text(0), "0.0");
        assert_eq!(whole_gb(32 * GIB), 32);
        assert_eq!(whole_gb(15 * GIB + GIB / 2), 16);
        assert_eq!(whole_mb(31 * MIB + MIB - 1), 31);
    }

    #[test]
    fn price_line() {
        assert_eq!(price_text(DDR5, "$"), "$14.34");
        assert_eq!(price_text(6_530_000, "$"), "$6.53");
        assert_eq!(price_text(14_349_999, "$"), "$14.34");
    }

    #[test]
    fn cut_counts_characters() {
        assert_eq!(cut("WhatsApp Desktop", 8), "WhatsApp");
        assert_eq!(cut("Zoë", 8), "Zoë");
    }

    #[test]
    fn wrap_balances_the_sample_remark() {
        assert_eq!(
            wrap_balanced("TEAMS HASN'T BEEN HOME IN 3 DAYS.", 30),
            ["TEAMS HASN'T BEEN", "HOME IN 3 DAYS."]
        );
    }

    #[test]
    fn wrap_keeps_lines_within_width() {
        let text = "WE'RE FULL. SOMEBODY HAS TO GO. CHROME IS SUBLETTING TO 47";
        let lines = wrap_balanced(text, 30);
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().all(|l| l.len() <= 30), "{lines:?}");
        assert_eq!(lines.join(" "), text);
    }

    #[test]
    fn wrap_cuts_words_longer_than_the_width() {
        let lines = wrap_balanced("SUPERCALIFRAGILISTICEXPIALIDOCIOUS-AND-MORE OK", 30);
        assert!(lines.iter().all(|l| l.chars().count() <= 30), "{lines:?}");
        assert_eq!(
            lines.concat().replace(' ', ""),
            "SUPERCALIFRAGILISTICEXPIALIDOCIOUS-AND-MOREOK"
        );
    }

    #[test]
    fn wrap_of_nothing_is_no_lines() {
        assert!(wrap_balanced("   ", 30).is_empty());
        assert_eq!(wrap_balanced("SHORT", 30), ["SHORT"]);
    }
}
