//! The landlord's remark (SPEC.md, Behaviors): one line at a time, highest
//! priority first, each shown for at least 20 seconds, at most 60
//! characters. All times are watched time, so a sleeping PC does not move
//! the clock.
//!
//! | Priority | Situation | Trigger |
//! | --- | --- | --- |
//! | 1 | Building full | Vacant under 10% of capacity for 20 seconds, or commit over 90% of the limit |
//! | 2 | Leak | A tenant gains LEAK |
//! | 3 | Squatters | Once per sign-in, when the first tenant gains SQUAT |
//! | 4 | Not home | A tenant holding at least 500 MB gains AWAY |
//! | 5 | Packed | Boxes in the basement hold at least 5% of the building (added after M3) |
//! | 6 | All calm | Vacant over 50% |
//! | 7 | Small talk | Otherwise, a new line every 60 seconds |

use crate::building::{Building, Flag};
use crate::flags::Gained;
use crate::format::{Shown as Money, cut, gb_text, micros};

const MIB: u64 = 1 << 20;

pub const MIN_SHOW_MS: u64 = 20_000;
pub const FULL_AFTER_MS: u64 = 20_000;
pub const SMALL_TALK_MS: u64 = 60_000;
pub const MAX_CHARS: usize = 60;
/// Names in remarks are cut like names on the details card.
pub const NAME_CHARS: usize = 16;
pub const NOT_HOME_MIN_BYTES: u64 = 500 * MIB;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// The report of an eviction the user asked for. It answers the user,
    /// so it is shown at once, then held 20 seconds like any other line.
    Evicted = 0,
    Full = 1,
    Leak = 2,
    Squatters = 3,
    NotHome = 4,
    Packed = 5,
    Calm = 6,
    SmallTalk = 7,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Remark {
    pub kind: Kind,
    pub text: String,
    /// The tenant the remark is about. For Full, the tenant to highlight.
    pub tenant: Option<String>,
}

struct Showing {
    remark: Remark,
    since: u64,
}

/// What the picker needs to know about this sample besides the building.
pub struct Sample<'a> {
    /// RAMlord's watched time.
    pub watched_ms: u64,
    pub commit_used_bytes: u64,
    pub commit_limit_bytes: u64,
    pub signed_in_at_ms: u64,
    pub gained: &'a [Gained],
    /// An eviction finished in this sample: its report.
    pub evicted: Option<&'a str>,
}

#[derive(Default)]
pub struct Remarks {
    current: Option<Showing>,
    queue: Vec<Remark>,
    low_since: Option<u64>,
    turn: usize,
}

/// `make(name)` with the name cut until the line fits in 60 characters.
fn with_name(name: &str, make: impl Fn(&str) -> String) -> String {
    let name: String = cut(&name.to_uppercase(), NAME_CHARS);
    let mut chars = name.chars().count();
    loop {
        let line = make(&cut(&name, chars));
        if line.chars().count() <= MAX_CHARS || chars == 0 {
            return cut(&line, MAX_CHARS);
        }
        chars -= 1;
    }
}

/// "3 days", "1 day", "5 hours", "1 hour".
fn span(ms: u64) -> String {
    const HOUR: u64 = 3_600_000;
    let days = ms / (24 * HOUR);
    let hours = ms / HOUR;
    match (days, hours) {
        (0, 0 | 1) => "1 hour".into(),
        (0, h) => format!("{h} hours"),
        (1, _) => "1 day".into(),
        (d, _) => format!("{d} days"),
    }
}

/// "Evicted DISCORD. 2.9 GB vacated."
pub fn evicted_text(name: &str, vacated_bytes: u64) -> String {
    let gb = gb_text(vacated_bytes);
    with_name(name, |n| format!("Evicted {n}. {gb} GB vacated."))
}

pub fn full_text() -> String {
    "We're full. Somebody has to go.".into()
}

pub fn calm_text() -> String {
    "Half the building is empty. Nobody needs evicting.".into()
}

pub fn leak_text(name: &str, from_bytes: u64, to_bytes: u64) -> String {
    let (a, b) = (gb_text(from_bytes), gb_text(to_bytes));
    let long = with_name(name, |n| {
        format!("{n} keeps knocking down walls: {a} to {b} GB in 1 hour.")
    });
    if long.contains(" in 1 hour.") {
        return long;
    }
    with_name(name, |n| {
        format!("{n} keeps knocking down walls: {a} to {b} GB.")
    })
}

/// The boxes in the basement, and the tenant with the most packed memory.
pub fn packed_text(building: &Building) -> String {
    let gb = gb_text(building.boxes_bytes);
    let top = building
        .tenants
        .iter()
        .filter(|t| t.packed_bytes > 0 && !t.has(Flag::Family))
        .max_by_key(|t| t.packed_bytes);
    match top {
        Some(t) => with_name(&t.name, |n| {
            format!("{gb} GB in boxes in the basement, mostly {n}'s.")
        }),
        None => format!("{gb} GB of tenants' things sit in boxes in the basement."),
    }
}

pub fn squatters_text(count: usize) -> String {
    if count == 1 {
        "1 tenant moved in at boot and you never visited.".into()
    } else {
        format!("{count} tenants moved in at boot and you never visited.")
    }
}

pub fn not_home_text(name: &str, unfocused_ms: u64) -> String {
    let span = span(unfocused_ms);
    with_name(name, |n| format!("{n} hasn't been home in {span}."))
}

/// Small talk: the first line that applies, trying line `from` first.
/// Returns the line's number and its text.
pub fn small_talk(building: &Building, from: usize) -> (usize, String) {
    let largest = building.tenants.iter().max_by_key(|t| t.rent_bytes);
    let busiest = building.tenants.iter().max_by_key(|t| t.process_count);
    let lines: [Option<String>; 4] = [
        busiest.filter(|t| t.process_count >= 3).map(|t| {
            with_name(&t.name, |n| {
                format!("{n} is subletting to {} processes.", t.process_count)
            })
        }),
        largest
            .map(|t| {
                (
                    t,
                    u128::from(t.rent_bytes) * 100 / u128::from(building.capacity_bytes.max(1)),
                )
            })
            .filter(|(_, pct)| *pct >= 1)
            .map(|(t, pct)| {
                with_name(&t.name, |n| format!("{n} takes up {pct}% of the building."))
            }),
        (building.staff_bytes > 0).then(|| {
            format!(
                "Staff keeps {} GB for Windows itself.",
                gb_text(building.staff_bytes)
            )
        }),
        (building.vacant_bytes > 0).then(|| {
            let rent = Money::rent(micros(building.vacant_bytes, building.price.per_gb_micros));
            format!(
                "{} GB stands empty. Asking {}.",
                gb_text(building.vacant_bytes),
                rent.text(&building.price.symbol)
            )
        }),
    ];
    (0..lines.len())
        .map(|k| (from + k) % lines.len())
        .find_map(|k| lines[k].clone().map(|text| (k, text)))
        .unwrap_or((from, "Quiet day at the building.".into()))
}

impl Remarks {
    pub fn new() -> Remarks {
        Remarks::default()
    }

    pub fn current(&self) -> Option<&Remark> {
        self.current.as_ref().map(|s| &s.remark)
    }

    /// Takes one sample and returns the remark to show. `squatters_told`
    /// is the sign-in the squatter remark was last made for; it lives in
    /// the state file so the remark comes once per sign-in.
    pub fn update(
        &mut self,
        building: &Building,
        sample: &Sample,
        squatters_told: &mut Option<u64>,
    ) -> &Remark {
        let now = sample.watched_ms;
        let tenant = |id: &str| building.tenants.iter().find(|t| t.id == id);

        for g in sample.gained {
            let Some(t) = tenant(&g.tenant) else { continue };
            let remark = match g.flag {
                Flag::Leaker => Some((Kind::Leak, leak_text(&t.name, g.from_bytes, g.to_bytes))),
                Flag::Squatter if *squatters_told != Some(sample.signed_in_at_ms) => {
                    *squatters_told = Some(sample.signed_in_at_ms);
                    let count = building
                        .tenants
                        .iter()
                        .filter(|t| t.has(Flag::Squatter))
                        .count();
                    Some((Kind::Squatters, squatters_text(count)))
                }
                Flag::Away if g.to_bytes >= NOT_HOME_MIN_BYTES => {
                    Some((Kind::NotHome, not_home_text(&t.name, g.unfocused_ms)))
                }
                _ => None,
            };
            if let Some((kind, text)) = remark {
                self.queue.push(Remark {
                    kind,
                    text,
                    tenant: Some(g.tenant.clone()),
                });
            }
        }
        // A remark about a flag that has cleared, or a tenant that left,
        // is no longer true.
        self.queue.retain(|r| {
            let flag = match r.kind {
                Kind::Leak => Flag::Leaker,
                Kind::NotHome => Flag::Away,
                _ => return true,
            };
            r.tenant
                .as_deref()
                .and_then(tenant)
                .is_some_and(|t| t.has(flag))
        });

        let capacity = u128::from(building.capacity_bytes.max(1));
        let vacant = u128::from(building.vacant_bytes);
        let low = vacant * 10 < capacity;
        self.low_since = if low {
            Some(self.low_since.unwrap_or(now))
        } else {
            None
        };
        let commit_full = sample.commit_limit_bytes > 0
            && u128::from(sample.commit_used_bytes) * 10
                > u128::from(sample.commit_limit_bytes) * 9;
        let full = commit_full
            || self
                .low_since
                .is_some_and(|since| now - since >= FULL_AFTER_MS);
        let calm = vacant * 2 > capacity;
        let packed = u128::from(building.boxes_bytes) * 20 >= capacity && building.boxes_bytes > 0;
        let highlight = building
            .tenants
            .iter()
            .filter(|t| !t.has(Flag::Family))
            .max_by_key(|t| t.rent_bytes)
            .map(|t| t.id.clone());

        // A line stays at least 20 seconds; after that, the most urgent
        // one wins, and a condition that still holds keeps its line.
        let hold = self
            .current
            .as_ref()
            .is_some_and(|s| now.saturating_sub(s.since) < MIN_SHOW_MS);
        let next = if full {
            Kind::Full
        } else if let Some(kind) = self.queue.iter().map(|r| r.kind).min() {
            kind
        } else if packed {
            Kind::Packed
        } else if calm {
            Kind::Calm
        } else {
            Kind::SmallTalk
        };
        let keep = hold
            || self.current.as_ref().is_some_and(|s| match next {
                Kind::Full | Kind::Packed | Kind::Calm => s.remark.kind == next,
                Kind::SmallTalk => {
                    s.remark.kind == next && now.saturating_sub(s.since) < SMALL_TALK_MS
                }
                _ => false,
            });

        if let Some(text) = sample.evicted {
            self.current = Some(Showing {
                remark: Remark {
                    kind: Kind::Evicted,
                    text: text.to_owned(),
                    tenant: None,
                },
                since: now,
            });
        } else if keep {
            if let Some(showing) = self.current.as_mut()
                && showing.remark.kind == Kind::Full
            {
                showing.remark.tenant = highlight;
            }
        } else {
            let remark = match next {
                Kind::Full => Remark {
                    kind: Kind::Full,
                    text: full_text(),
                    tenant: highlight,
                },
                Kind::Calm => Remark {
                    kind: Kind::Calm,
                    text: calm_text(),
                    tenant: None,
                },
                Kind::Packed => Remark {
                    kind: Kind::Packed,
                    text: packed_text(building),
                    tenant: None,
                },
                Kind::SmallTalk => {
                    let (line, text) = small_talk(building, self.turn);
                    self.turn = line + 1;
                    Remark {
                        kind: Kind::SmallTalk,
                        text,
                        tenant: None,
                    }
                }
                kind => {
                    let at = self
                        .queue
                        .iter()
                        .position(|r| r.kind == kind)
                        .expect("queued");
                    self.queue.remove(at)
                }
            };
            self.current = Some(Showing { remark, since: now });
        }
        &self
            .current
            .as_ref()
            .expect("a remark is always set")
            .remark
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::test_building;

    const GIB: u64 = 1 << 30;

    /// 16 GiB: A 4 GiB, B 600 MiB; Staff 2 GiB; vacant as given in MiB.
    fn building(vacant_mib: u64) -> Building {
        let rest = 16 * 1024 - 4096 - 600 - 2048 - vacant_mib;
        test_building(
            (2048 + rest) * MIB,
            &[("A", 4 * GIB), ("B", 600 * MIB)],
            vacant_mib * MIB,
        )
    }

    fn sample(watched_ms: u64, gained: &[Gained]) -> Sample<'_> {
        Sample {
            watched_ms,
            commit_used_bytes: 10 * GIB,
            commit_limit_bytes: 32 * GIB,
            signed_in_at_ms: 1_000,
            gained,
            evicted: None,
        }
    }

    fn gained(tenant: &str, flag: Flag) -> Gained {
        Gained {
            tenant: tenant.into(),
            flag,
            from_bytes: 900 * MIB,
            to_bytes: 2_970 * MIB,
            unfocused_ms: 3 * 24 * 3_600_000,
        }
    }

    fn flagged(mut b: Building, id: &str, flag: Flag) -> Building {
        b.tenants
            .iter_mut()
            .find(|t| t.id == id)
            .unwrap()
            .flags
            .push(flag);
        b
    }

    #[test]
    fn texts_match_the_spec_examples() {
        assert_eq!(
            leak_text("Discord", 900 * MIB, 2_970 * MIB),
            "DISCORD keeps knocking down walls: 0.9 to 2.9 GB in 1 hour."
        );
        assert_eq!(
            not_home_text("Teams", 3 * 24 * 3_600_000),
            "TEAMS hasn't been home in 3 days."
        );
        assert_eq!(
            squatters_text(3),
            "3 tenants moved in at boot and you never visited."
        );
        assert_eq!(full_text(), "We're full. Somebody has to go.");
        assert_eq!(
            evicted_text("DISCORD", 2_970 * MIB),
            "Evicted DISCORD. 2.9 GB vacated."
        );
        assert_eq!(
            calm_text(),
            "Half the building is empty. Nobody needs evicting."
        );
    }

    #[test]
    fn every_remark_fits_sixty_characters() {
        let long = "Microsoft Visual Studio Code Insiders Edition";
        for text in [
            leak_text(long, 12_345 * MIB, 19_999 * MIB),
            not_home_text(long, 400 * 24 * 3_600_000),
            {
                let mut b = building(1000);
                b.boxes_bytes = 12_345 * MIB;
                b.tenants[0].name = long.into();
                b.tenants[0].packed_bytes = 1;
                packed_text(&b)
            },
            squatters_text(12_345),
            evicted_text(long, 123_456 * MIB),
            full_text(),
            calm_text(),
        ] {
            assert!(
                text.chars().count() <= MAX_CHARS,
                "{} chars: {text}",
                text.chars().count()
            );
        }
        // A long name is cut, not the sentence.
        assert!(leak_text(long, 900 * MIB, 2_970 * MIB).ends_with("0.9 to 2.9 GB in 1 hour."));
        let mut b = building(1000);
        b.tenants[0].name = long.into();
        b.tenants[0].process_count = 123_456;
        for from in 0..8 {
            let (_, text) = small_talk(&b, from);
            assert!(text.chars().count() <= MAX_CHARS, "{text}");
        }
    }

    #[test]
    fn spans() {
        assert_eq!(span(4 * 3_600_000), "4 hours");
        assert_eq!(span(3_600_000), "1 hour");
        assert_eq!(span(30 * 3_600_000), "1 day");
        assert_eq!(span(3 * 24 * 3_600_000 + 5), "3 days");
    }

    #[test]
    fn priorities_from_full_down_to_small_talk() {
        // Everything at once: each remark waits its turn, highest first.
        let b = flagged(
            flagged(
                flagged(building(500), "a", Flag::Leaker),
                "b",
                Flag::Squatter,
            ),
            "a",
            Flag::Away,
        );
        let events = [
            gained("b", Flag::Squatter),
            gained("a", Flag::Away),
            gained("a", Flag::Leaker),
        ];
        let mut r = Remarks::new();
        let mut told = None;
        // Vacant is under 10% but not yet for 20 seconds: the leak goes first.
        assert_eq!(
            r.update(&b, &sample(0, &events), &mut told).kind,
            Kind::Leak
        );
        assert_eq!(
            r.update(&b, &sample(19_999, &[]), &mut told).kind,
            Kind::Leak
        );
        // From 20 s on the building is full, and it stays so.
        assert_eq!(
            r.update(&b, &sample(20_000, &[]), &mut told).kind,
            Kind::Full
        );
        assert_eq!(
            r.update(&b, &sample(90_000, &[]), &mut told).kind,
            Kind::Full
        );
        // Room again: the queued remarks follow in priority order.
        let roomy = flagged(
            flagged(
                flagged(building(9_000), "a", Flag::Leaker),
                "b",
                Flag::Squatter,
            ),
            "a",
            Flag::Away,
        );
        assert_eq!(
            r.update(&roomy, &sample(92_000, &[]), &mut told).kind,
            Kind::Squatters
        );
        assert_eq!(
            r.update(&roomy, &sample(112_000, &[]), &mut told).kind,
            Kind::NotHome
        );
        assert_eq!(
            r.update(&roomy, &sample(132_000, &[]), &mut told).kind,
            Kind::Calm
        );
        assert_eq!(
            r.update(&roomy, &sample(500_000, &[]), &mut told).kind,
            Kind::Calm
        );
        // Neither full nor calm: small talk.
        let middling = building(4_000);
        assert_eq!(
            r.update(&middling, &sample(520_000, &[]), &mut told).kind,
            Kind::SmallTalk
        );
    }

    #[test]
    fn a_remark_stays_at_least_twenty_seconds() {
        let mut r = Remarks::new();
        let mut told = None;
        let b = building(4_000);
        assert_eq!(
            r.update(&b, &sample(0, &[]), &mut told).kind,
            Kind::SmallTalk
        );
        // A leak arrives at 5 s; it waits until the small talk has had 20 s.
        let leaking = flagged(building(4_000), "a", Flag::Leaker);
        let events = [gained("a", Flag::Leaker)];
        assert_eq!(
            r.update(&leaking, &sample(5_000, &events), &mut told).kind,
            Kind::SmallTalk
        );
        assert_eq!(
            r.update(&leaking, &sample(19_999, &[]), &mut told).kind,
            Kind::SmallTalk
        );
        assert_eq!(
            r.update(&leaking, &sample(20_000, &[]), &mut told).kind,
            Kind::Leak
        );
        assert_eq!(
            r.update(&leaking, &sample(39_999, &[]), &mut told).kind,
            Kind::Leak
        );
        assert_eq!(
            r.update(&leaking, &sample(40_000, &[]), &mut told).kind,
            Kind::SmallTalk
        );
    }

    #[test]
    fn an_eviction_report_comes_at_once_and_stays_twenty_seconds() {
        let mut r = Remarks::new();
        let mut told = None;
        let b = building(4_000);
        r.update(&b, &sample(0, &[]), &mut told);
        // The user evicted someone 5 seconds into the small talk: the
        // report answers at once.
        let report = sample(5_000, &[]);
        let report = Sample {
            evicted: Some("Evicted A. 4.0 GB vacated."),
            ..report
        };
        let remark = r.update(&b, &report, &mut told);
        assert_eq!(remark.kind, Kind::Evicted);
        assert_eq!(remark.text, "Evicted A. 4.0 GB vacated.");
        // Even a full building waits until the report has had 20 seconds.
        let full = building(100);
        for at in [6_000, 24_999] {
            assert_eq!(
                r.update(&full, &sample(at, &[]), &mut told).kind,
                Kind::Evicted
            );
        }
        // From then on, the lines go on as before. A full building takes
        // 20 seconds of its own before it is called full.
        assert_ne!(
            r.update(&full, &sample(25_000, &[]), &mut told).kind,
            Kind::Evicted
        );
    }

    #[test]
    fn small_talk_changes_every_sixty_seconds() {
        let mut r = Remarks::new();
        let mut told = None;
        let b = building(4_000);
        let first = r.update(&b, &sample(0, &[]), &mut told).text.clone();
        assert_eq!(r.update(&b, &sample(59_999, &[]), &mut told).text, first);
        let second = r.update(&b, &sample(60_000, &[]), &mut told).text.clone();
        assert_ne!(second, first);
    }

    #[test]
    fn the_squatter_remark_comes_once_per_sign_in() {
        let mut r = Remarks::new();
        let mut told = None;
        let b = flagged(building(4_000), "b", Flag::Squatter);
        let events = [gained("b", Flag::Squatter)];
        assert_eq!(
            r.update(&b, &sample(0, &events), &mut told).kind,
            Kind::Squatters
        );
        assert_eq!(told, Some(1_000));
        assert_eq!(
            r.update(&b, &sample(20_000, &events), &mut told).kind,
            Kind::SmallTalk
        );
        // A new sign-in may hear it again.
        let mut s = sample(40_000, &events);
        s.signed_in_at_ms = 9_000;
        assert_eq!(r.update(&b, &s, &mut told).kind, Kind::Squatters);
        assert_eq!(told, Some(9_000));
    }

    #[test]
    fn not_home_needs_500_mb_and_stale_remarks_are_dropped() {
        let mut r = Remarks::new();
        let mut told = None;
        let mut small = gained("b", Flag::Away);
        small.to_bytes = 499 * MIB;
        let b = flagged(building(4_000), "b", Flag::Away);
        assert_eq!(
            r.update(&b, &sample(0, &[small]), &mut told).kind,
            Kind::SmallTalk
        );
        // A leak that clears before its turn is never mentioned.
        let leaking = flagged(building(4_000), "a", Flag::Leaker);
        r.update(
            &leaking,
            &sample(1_000, &[gained("a", Flag::Leaker)]),
            &mut told,
        );
        let cleared = building(4_000);
        assert_eq!(
            r.update(&cleared, &sample(30_000, &[]), &mut told).kind,
            Kind::SmallTalk
        );
    }

    #[test]
    fn packed_boxes_speak_after_events_and_before_calm() {
        let mut r = Remarks::new();
        let mut told = None;
        // 9,000 MiB vacant of 16 GiB is calm; 1 GiB of boxes is 6%.
        let mut b = building(9_000);
        b.boxes_bytes = 1024 * MIB;
        b.staff_bytes -= 1024 * MIB;
        b.tenants[1].packed_bytes = 700 * MIB;
        b.tenants[0].packed_bytes = 300 * MIB;
        b.validate().unwrap();
        let remark = r.update(&b, &sample(0, &[]), &mut told);
        assert_eq!(remark.kind, Kind::Packed);
        assert_eq!(remark.text, "1.0 GB in boxes in the basement, mostly B's.");
        // Under 5%: calm again.
        b.boxes_bytes = 800 * MIB;
        b.staff_bytes += 224 * MIB;
        assert_eq!(
            r.update(&b, &sample(30_000, &[]), &mut told).kind,
            Kind::Calm
        );
        // Nothing packed anywhere: the boxes are nobody's in particular.
        let mut none = building(9_000);
        none.boxes_bytes = 1024 * MIB;
        assert_eq!(
            packed_text(&none),
            "1.0 GB of tenants' things sit in boxes in the basement."
        );
    }

    #[test]
    fn commit_over_ninety_percent_is_full_at_once_and_highlights_the_largest_non_family_tenant() {
        let mut r = Remarks::new();
        let mut told = None;
        let b = flagged(building(8_000), "a", Flag::Family);
        let mut s = sample(0, &[]);
        s.commit_used_bytes = 29 * GIB;
        let remark = r.update(&b, &s, &mut told);
        assert_eq!(remark.kind, Kind::Full);
        assert_eq!(remark.tenant.as_deref(), Some("b"));
    }
}
