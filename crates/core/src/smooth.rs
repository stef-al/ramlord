//! Keeping the tower calm (SPEC.md, Dividing the building): the layout is
//! fed rent smoothed over 10 seconds, and two neighbours swap places only
//! after the lower one has led by 5% for 30 seconds. The rent roll always
//! shows current numbers, so only the layout goes through here.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::building::Building;
use crate::layout::split_named;
use crate::tenants::balance;

/// Samples older than this no longer count.
pub const WINDOW_MS: u64 = 10_000;
/// How far the lower neighbour must lead, in percent.
pub const LEAD_PERCENT: u64 = 5;
/// How long it must lead before the two swap.
pub const LEAD_MS: u64 = 30_000;

struct Sample {
    at_ms: u64,
    vacant: u64,
    boxes: u64,
    rents: BTreeMap<String, u64>,
}

/// The layout's memory of recent samples and of the fill order.
#[derive(Default)]
pub struct Smoother {
    samples: VecDeque<Sample>,
    /// Named tenant ids in fill order.
    order: Vec<String>,
    /// When the lower of two neighbours, `(upper, lower)`, began to lead.
    leads: BTreeMap<(String, String), u64>,
}

impl Smoother {
    pub fn new() -> Smoother {
        Smoother::default()
    }

    /// Records `current` as the sample at `at_ms` and returns the building
    /// for the layout: each tenant's rent and Vacant averaged over the last
    /// 10 seconds (a sample without the tenant counts as zero), and Staff
    /// what is left. Tenants, their order in the list and everything else
    /// are those of `current`.
    pub fn smooth(&mut self, at_ms: u64, current: &Building) -> Building {
        if self.samples.back().is_some_and(|s| s.at_ms > at_ms) {
            // The clock went backwards: start over.
            self.samples.clear();
            self.leads.clear();
        }
        self.samples.push_back(Sample {
            at_ms,
            vacant: current.vacant_bytes,
            boxes: current.boxes_bytes,
            rents: current
                .tenants
                .iter()
                .map(|t| (t.id.clone(), t.rent_bytes))
                .collect(),
        });
        while self
            .samples
            .front()
            .is_some_and(|s| s.at_ms + WINDOW_MS <= at_ms)
        {
            self.samples.pop_front();
        }

        let n = self.samples.len() as u128;
        let average = |value: &dyn Fn(&Sample) -> u64| -> u64 {
            (self
                .samples
                .iter()
                .map(|s| u128::from(value(s)))
                .sum::<u128>()
                / n) as u64
        };
        let mut smoothed = current.clone();
        for tenant in &mut smoothed.tenants {
            tenant.rent_bytes = average(&|s| s.rents.get(&tenant.id).copied().unwrap_or(0));
        }
        let vacant = average(&|s| s.vacant);
        let boxes = average(&|s| s.boxes);
        let (staff, vacant) = balance(
            smoothed.capacity_bytes,
            vacant,
            smoothed.rent_bytes().saturating_add(boxes),
        );
        smoothed.staff_bytes = staff;
        smoothed.vacant_bytes = vacant;
        smoothed.boxes_bytes = boxes;
        smoothed
    }

    /// The fill order of the named tenants of `smoothed` at `at_ms`. Kept
    /// from the last call; a new tenant goes in at its rank; neighbours
    /// swap once the lower one has led the upper by 5% for 30 seconds.
    pub fn order(&mut self, at_ms: u64, smoothed: &Building) -> Vec<String> {
        let (named, _) = split_named(smoothed);
        let rank: Vec<&str> = named
            .iter()
            .map(|&t| smoothed.tenants[t].id.as_str())
            .collect();
        let rent: BTreeMap<&str, u64> = named
            .iter()
            .map(|&t| {
                (
                    smoothed.tenants[t].id.as_str(),
                    smoothed.tenants[t].rent_bytes,
                )
            })
            .collect();
        let rank_of = |id: &str| rank.iter().position(|r| *r == id).unwrap_or(usize::MAX);

        let mut order: Vec<String> = self
            .order
            .iter()
            .filter(|id| rent.contains_key(id.as_str()))
            .cloned()
            .collect();
        for (k, id) in rank.iter().enumerate() {
            if !order.iter().any(|o| o == id) {
                let at = order
                    .iter()
                    .position(|o| rank_of(o) > k)
                    .unwrap_or(order.len());
                order.insert(at, (*id).to_string());
            }
        }

        let mut i = 0;
        while i + 1 < order.len() {
            let key = (order[i].clone(), order[i + 1].clone());
            let upper = u128::from(rent[key.0.as_str()]);
            let lower = u128::from(rent[key.1.as_str()]);
            if lower * 100 >= upper * u128::from(100 + LEAD_PERCENT) {
                let since = *self.leads.entry(key.clone()).or_insert(at_ms);
                if at_ms.saturating_sub(since) >= LEAD_MS {
                    order.swap(i, i + 1);
                    self.leads.remove(&key);
                    i += 2;
                    continue;
                }
            } else {
                self.leads.remove(&key);
            }
            i += 1;
        }
        let neighbours: BTreeSet<(String, String)> = order
            .windows(2)
            .map(|w| (w[0].clone(), w[1].clone()))
            .collect();
        self.leads.retain(|pair, _| neighbours.contains(pair));
        self.order = order.clone();
        order
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::test_building;
    use crate::format::MIB;

    /// 8 GiB with tenants A and B at the given MiB; Staff 1 GiB.
    fn pc(a: u64, b: u64) -> Building {
        test_building(
            1024 * MIB,
            &[("A", a * MIB), ("B", b * MIB)],
            (7168 - a - b) * MIB,
        )
    }

    #[test]
    fn rent_is_averaged_over_ten_seconds() {
        let mut s = Smoother::new();
        let first = s.smooth(0, &pc(1000, 500));
        assert_eq!(first.tenants[0].rent_bytes, 1000 * MIB);
        s.smooth(2_000, &pc(1000, 500));
        let third = s.smooth(4_000, &pc(1600, 500));
        assert_eq!(third.tenants[0].rent_bytes, 1200 * MIB);
        // Samples at 0 and 2 s have left the window at 12 s.
        let later = s.smooth(12_000, &pc(1600, 500));
        assert_eq!(later.tenants[0].rent_bytes, 1600 * MIB);
        later.validate().expect("smoothed buildings balance");
    }

    #[test]
    fn a_new_tenant_ramps_in_and_the_building_still_balances() {
        let mut s = Smoother::new();
        s.smooth(
            0,
            &test_building(1024 * MIB, &[("A", 1000 * MIB)], 6168 * MIB),
        );
        let b = s.smooth(2_000, &pc(1000, 600));
        assert_eq!(b.tenants[1].rent_bytes, 300 * MIB);
        assert_eq!(
            b.staff_bytes + b.rent_bytes() + b.vacant_bytes,
            b.capacity_bytes
        );
    }

    #[test]
    fn neighbours_swap_only_after_leading_by_five_percent_for_thirty_seconds() {
        let mut s = Smoother::new();
        let order = |s: &mut Smoother, at, a, b| s.order(at, &pc(a, b));
        assert_eq!(order(&mut s, 0, 1000, 900), ["a", "b"]);
        // B leads by 4%: never enough.
        assert_eq!(order(&mut s, 2_000, 1000, 1040), ["a", "b"]);
        assert_eq!(order(&mut s, 60_000, 1000, 1040), ["a", "b"]);
        // B leads by 5% from 62 s on: the swap comes at 92 s, not before.
        assert_eq!(order(&mut s, 62_000, 1000, 1050), ["a", "b"]);
        assert_eq!(order(&mut s, 91_998, 1000, 1050), ["a", "b"]);
        assert_eq!(order(&mut s, 92_000, 1000, 1050), ["b", "a"]);
        assert_eq!(order(&mut s, 94_000, 1000, 1050), ["b", "a"]);
    }

    #[test]
    fn a_lead_that_lapses_starts_over() {
        let mut s = Smoother::new();
        s.order(0, &pc(1000, 900));
        s.order(2_000, &pc(1000, 1100));
        s.order(20_000, &pc(1000, 1000)); // lead lost
        assert_eq!(s.order(31_000, &pc(1000, 1100)), ["a", "b"]);
        assert_eq!(s.order(60_000, &pc(1000, 1100)), ["a", "b"]);
        assert_eq!(s.order(61_000, &pc(1000, 1100)), ["b", "a"]);
    }

    #[test]
    fn new_tenants_go_in_at_their_rank() {
        let mut s = Smoother::new();
        s.order(0, &pc(1000, 900));
        let b = test_building(
            1024 * MIB,
            &[("A", 1000 * MIB), ("B", 900 * MIB), ("C", 950 * MIB)],
            4294 * MIB,
        );
        assert_eq!(s.order(2_000, &b), ["a", "c", "b"]);
    }
}
