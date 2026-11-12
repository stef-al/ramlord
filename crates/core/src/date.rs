//! Calendar dates as written in Building files (`YYYY-MM-DD`).

pub const MONTHS: [&str; 12] = [
    "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
];
pub const WEEKDAYS: [&str; 7] = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    pub year: u32,
    /// 1 to 12.
    pub month: u32,
    /// 1 to the month's length.
    pub day: u32,
}

impl Date {
    pub const EPOCH: Date = Date {
        year: 1970,
        month: 1,
        day: 1,
    };

    /// Parses `YYYY-MM-DD`, checking the day against the month's length.
    pub fn parse(text: &str) -> Option<Date> {
        let bytes = text.as_bytes();
        if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
            return None;
        }
        let number = |range: std::ops::Range<usize>| -> Option<u32> {
            let part = &text[range];
            part.bytes()
                .all(|b| b.is_ascii_digit())
                .then(|| part.parse().ok())
                .flatten()
        };
        let date = Date {
            year: number(0..4)?,
            month: number(5..7)?,
            day: number(8..10)?,
        };
        let valid = date.year >= 1
            && (1..=12).contains(&date.month)
            && date.day >= 1
            && date.day <= days_in_month(date.year, date.month);
        valid.then_some(date)
    }

    /// The UTC date of a time in milliseconds since 1970.
    pub fn from_unix_ms(ms: u64) -> Date {
        // Howard Hinnant's civil_from_days, in integers.
        let z = (ms / 86_400_000) as i64 + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        let year = (yoe + era * 400 + i64::from(month <= 2)) as u32;
        Date { year, month, day }
    }

    /// `YYYY-MM-DD`.
    pub fn iso(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    /// Three-letter month, such as `NOV`.
    pub fn month_name(self) -> &'static str {
        MONTHS[(self.month - 1) as usize]
    }

    /// Three-letter weekday, such as `MON` (Sakamoto's method).
    pub fn weekday_name(self) -> &'static str {
        const OFFSETS: [u32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
        let y = if self.month < 3 {
            self.year - 1
        } else {
            self.year
        };
        let index =
            (y + y / 4 - y / 100 + y / 400 + OFFSETS[(self.month - 1) as usize] + self.day) % 7;
        WEEKDAYS[index as usize]
    }
}

fn days_in_month(year: u32, month: u32) -> u32 {
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_dates_and_refuses_the_rest() {
        assert_eq!(
            Date::parse("2026-11-09"),
            Some(Date {
                year: 2026,
                month: 11,
                day: 9
            })
        );
        assert!(Date::parse("2028-02-29").is_some());
        for bad in [
            "2026-02-29",
            "2026-13-01",
            "2026-00-10",
            "2026-11-31",
            "2026-1-09",
            "+026-11-09",
            "0000-01-01",
            "",
        ] {
            assert_eq!(Date::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn weekdays_match_the_calendar() {
        assert_eq!(Date::parse("2026-11-09").unwrap().weekday_name(), "MON");
        assert_eq!(Date::parse("2026-11-03").unwrap().weekday_name(), "TUE");
        assert_eq!(Date::parse("2000-01-01").unwrap().weekday_name(), "SAT");
        assert_eq!(Date::parse("2024-02-29").unwrap().weekday_name(), "THU");
    }

    #[test]
    fn dates_from_unix_milliseconds() {
        assert_eq!(Date::from_unix_ms(0).iso(), "1970-01-01");
        assert_eq!(Date::from_unix_ms(1_794_268_800_000).iso(), "2026-11-10");
        assert_eq!(
            Date::from_unix_ms(1_794_268_800_000 - 1).iso(),
            "2026-11-09"
        );
        assert_eq!(Date::from_unix_ms(951_782_400_000).iso(), "2000-02-29");
        assert_eq!(
            Date::from_unix_ms(1_735_603_200_000 + 86_399_999).iso(),
            "2024-12-31"
        );
        let d = Date::from_unix_ms(1_794_268_800_000);
        assert_eq!(Date::parse(&d.iso()), Some(d));
    }

    #[test]
    fn month_names() {
        assert_eq!(Date::parse("2026-11-03").unwrap().month_name(), "NOV");
        assert_eq!(Date::parse("2026-01-03").unwrap().month_name(), "JAN");
    }
}
