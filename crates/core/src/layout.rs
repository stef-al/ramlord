//! Dividing the building: 256 units shared by Staff, the named tenants,
//! Others and Vacant (SPEC.md, The tower). Integer bytes throughout.

use std::collections::BTreeMap;

use crate::building::Building;
use crate::palette::HUE_COUNT;

pub const COLS: usize = 16;
pub const FLOORS: usize = 16;
pub const UNITS: usize = COLS * FLOORS;
/// At most this many tenants get their own units; the rest merge into Others.
pub const MAX_NAMED: usize = 40;

/// Who holds a unit. `Tenant` holds an index into `Building::tenants`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Owner {
    Staff,
    Tenant(usize),
    Others,
    Vacant,
    /// Compressed memory in the basement: a named tenant's boxes, or
    /// everyone else's (None).
    Boxes(Option<usize>),
}

/// One entry of the split, in fill order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub owner: Owner,
    pub bytes: u64,
    pub units: u32,
}

/// Saved hues: tenant id to an index into `palette::HUES`. A tenant keeps
/// its hue for life; this map is part of the state file from M3.
pub type HueBook = BTreeMap<String, u8>;

/// A named tenant: one with its own units and hue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Named {
    /// Index into `Building::tenants`.
    pub tenant: usize,
    /// Index into `palette::HUES`.
    pub hue: u8,
}

/// The result of dividing a building.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    /// Staff, named tenants, Others (only when someone merged), Vacant.
    pub entries: Vec<Entry>,
    /// Owner of each unit; index 0 is the ground floor's leftmost unit,
    /// filled left to right, then floor by floor upwards.
    pub cells: Vec<Owner>,
    /// Named tenants in fill order: rent descending, ties by name.
    pub named: Vec<Named>,
    /// Tenants merged into Others, in the same order.
    pub merged: Vec<usize>,
}

/// One floor's run of an owner's units (columns inclusive).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    /// 0 is the ground floor.
    pub floor: usize,
    pub first_col: usize,
    pub last_col: usize,
}

impl Segment {
    pub fn width(self) -> usize {
        self.last_col - self.first_col + 1
    }
}

/// Tenant indices ordered by rent descending, ties by name, then by id.
pub fn ranked(building: &Building) -> Vec<usize> {
    let mut order: Vec<usize> = (0..building.tenants.len()).collect();
    order.sort_by(|&a, &b| {
        let (ta, tb) = (&building.tenants[a], &building.tenants[b]);
        tb.rent_bytes
            .cmp(&ta.rent_bytes)
            .then_with(|| ta.name.cmp(&tb.name))
            .then_with(|| ta.id.cmp(&tb.id))
    });
    order
}

/// Rule 2: named tenants are the 40 largest that hold at least half a unit.
/// Returns `(named, merged)`, both in rank order.
pub fn split_named(building: &Building) -> (Vec<usize>, Vec<usize>) {
    let capacity = u128::from(building.capacity_bytes);
    let (mut named, mut merged) = (Vec::new(), Vec::new());
    for index in ranked(building) {
        let rent = u128::from(building.tenants[index].rent_bytes);
        let half_unit_or_more = 2 * rent * UNITS as u128 >= capacity;
        if half_unit_or_more && named.len() < MAX_NAMED {
            named.push(index);
        } else {
            merged.push(index);
        }
    }
    (named, merged)
}

/// Rules 1, 4 and 5: gives each entry its units. `capacity` must equal the
/// sum of the entries' bytes, which `Building::validate` guarantees.
pub fn divide(capacity: u64, entries: &mut [Entry]) {
    let capacity = u128::from(capacity.max(1));
    let units = UNITS as u128;
    let mut remainders = Vec::with_capacity(entries.len());
    for entry in entries.iter_mut() {
        let scaled = u128::from(entry.bytes) * units;
        entry.units = (scaled / capacity) as u32;
        remainders.push(scaled % capacity);
    }

    // Rule 4: leftover units one each, largest remainder first; ties go to
    // more bytes, then to fill order.
    let given: u32 = entries.iter().map(|e| e.units).sum();
    let leftover = (UNITS as u32).saturating_sub(given) as usize;
    let mut order: Vec<usize> = (0..entries.len()).collect();
    order.sort_by(|&a, &b| {
        remainders[b]
            .cmp(&remainders[a])
            .then_with(|| entries[b].bytes.cmp(&entries[a].bytes))
            .then_with(|| a.cmp(&b))
    });
    for &k in order.iter().take(leftover) {
        entries[k].units += 1;
    }

    // Rule 5: an entry with memory but no unit takes one from the entry
    // holding the most units (the first in fill order on a tie). Vacant may
    // be zero.
    for k in 0..entries.len() {
        if entries[k].bytes > 0 && entries[k].units == 0 && entries[k].owner != Owner::Vacant {
            let donor = (0..entries.len())
                .max_by(|&a, &b| {
                    entries[a]
                        .units
                        .cmp(&entries[b].units)
                        .then_with(|| b.cmp(&a))
                })
                .expect("entries is not empty");
            if entries[donor].units > 1 {
                entries[donor].units -= 1;
                entries[k].units = 1;
            }
        }
    }
}

/// Hues: a tenant keeps its saved hue. New tenants are served in fill
/// order; each gets the hue least used among current named tenants, ties
/// in palette order. Returns the book with the new tenants added.
pub fn assign_hues(ids_in_fill_order: &[&str], saved: &HueBook) -> HueBook {
    let mut book = saved.clone();
    book.retain(|_, hue| usize::from(*hue) < HUE_COUNT);
    let mut used = [0u32; HUE_COUNT];
    for id in ids_in_fill_order {
        if let Some(&hue) = book.get(*id) {
            used[usize::from(hue)] += 1;
        }
    }
    for id in ids_in_fill_order {
        if !book.contains_key(*id) {
            let hue = (0..HUE_COUNT)
                .min_by_key(|&h| (used[h], h))
                .expect("palette is not empty");
            used[hue] += 1;
            book.insert((*id).to_string(), hue as u8);
        }
    }
    book
}

/// Splits the boxes among tenants by their packed memory (committed but
/// not in RAM), the best estimate Windows allows of whose pages are
/// compressed. A named tenant whose share is at least half a unit gets its
/// own boxes, largest first; the rest share one stack. With no packed
/// memory at all, the boxes are nobody's in particular.
pub fn box_shares(building: &Building, named: &[usize]) -> Vec<(Option<usize>, u64)> {
    let total = building.boxes_bytes;
    if total == 0 {
        return Vec::new();
    }
    let packed: u128 = building
        .tenants
        .iter()
        .map(|t| u128::from(t.packed_bytes))
        .sum();
    if packed == 0 {
        return vec![(None, total)];
    }
    let capacity = u128::from(building.split_bytes().max(1));
    let mut own: Vec<(usize, u64)> = named
        .iter()
        .map(|&t| {
            let share = u128::from(total) * u128::from(building.tenants[t].packed_bytes) / packed;
            (t, share as u64)
        })
        .filter(|&(_, share)| 2 * u128::from(share) * UNITS as u128 >= capacity)
        .collect();
    own.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| building.tenants[a.0].name.cmp(&building.tenants[b.0].name))
    });
    let rest = total - own.iter().map(|&(_, share)| share).sum::<u64>();
    let mut shares: Vec<(Option<usize>, u64)> =
        own.into_iter().map(|(t, b)| (Some(t), b)).collect();
    if rest > 0 {
        shares.push((None, rest));
    }
    shares
}

impl Layout {
    /// Divides `building` (rules 1 to 6) and gives every named tenant a hue.
    /// Returns the layout and the hue book with any new tenants added.
    pub fn new(building: &Building, saved_hues: &HueBook) -> (Layout, HueBook) {
        Layout::ordered(building, saved_hues, &[])
    }

    /// Like `new`, but named tenants fill in the order of `order` (tenant
    /// ids, as the smoother keeps it); any not listed follow by rank. Which
    /// tenants are named still depends on rank alone.
    pub fn ordered(
        building: &Building,
        saved_hues: &HueBook,
        order: &[String],
    ) -> (Layout, HueBook) {
        let (mut named, merged) = split_named(building);
        if !order.is_empty() {
            let place = |t: &usize| {
                let id = &building.tenants[*t].id;
                order.iter().position(|o| o == id).unwrap_or(usize::MAX)
            };
            named.sort_by_key(place);
        }

        // Rule 3: fill order is Staff, named tenants, Others, Vacant. The
        // boxes go first, in the basement: they are tenants' memory, kept
        // compressed by Windows.
        let mut entries: Vec<Entry> = box_shares(building, &named)
            .into_iter()
            .map(|(tenant, bytes)| Entry {
                owner: Owner::Boxes(tenant),
                bytes,
                units: 0,
            })
            .collect();
        entries.push(Entry {
            owner: Owner::Staff,
            bytes: building.staff_bytes,
            units: 0,
        });
        entries.extend(named.iter().map(|&t| Entry {
            owner: Owner::Tenant(t),
            bytes: building.tenants[t].rent_bytes,
            units: 0,
        }));
        if !merged.is_empty() {
            entries.push(Entry {
                owner: Owner::Others,
                bytes: merged.iter().map(|&t| building.tenants[t].rent_bytes).sum(),
                units: 0,
            });
        }
        entries.push(Entry {
            owner: Owner::Vacant,
            bytes: building.vacant_bytes,
            units: 0,
        });
        divide(building.split_bytes(), &mut entries);

        // Rule 6: fill from the ground floor up, left to right.
        let mut cells = Vec::with_capacity(UNITS);
        for entry in &entries {
            cells.extend(std::iter::repeat_n(entry.owner, entry.units as usize));
        }
        cells.resize(UNITS, Owner::Vacant);

        let ids: Vec<&str> = named
            .iter()
            .map(|&t| building.tenants[t].id.as_str())
            .collect();
        let book = assign_hues(&ids, saved_hues);
        let named = named
            .iter()
            .zip(&ids)
            .map(|(&tenant, id)| Named {
                tenant,
                hue: book[*id],
            })
            .collect();
        (
            Layout {
                entries,
                cells,
                named,
                merged,
            },
            book,
        )
    }

    /// Units held by `owner`.
    pub fn units_of(&self, owner: Owner) -> u32 {
        self.entries
            .iter()
            .filter(|e| e.owner == owner)
            .map(|e| e.units)
            .sum()
    }

    /// Units in the basement, everyone's boxes together.
    pub fn box_units(&self) -> usize {
        self.cells
            .iter()
            .filter(|o| matches!(o, Owner::Boxes(_)))
            .count()
    }

    /// The hue of a named tenant.
    pub fn hue_of(&self, tenant: usize) -> Option<u8> {
        self.named
            .iter()
            .find(|n| n.tenant == tenant)
            .map(|n| n.hue)
    }

    /// One segment per floor the owner occupies, from the ground floor up.
    pub fn segments(&self, owner: Owner) -> Vec<Segment> {
        (0..FLOORS)
            .filter_map(|floor| {
                let row = &self.cells[floor * COLS..(floor + 1) * COLS];
                let first = row.iter().position(|&o| o == owner)?;
                let last = row.iter().rposition(|&o| o == owner)?;
                Some(Segment {
                    floor,
                    first_col: first,
                    last_col: last,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::test_building as building;
    use crate::format::MIB;

    const SAMPLE: &str = include_str!("../../../fixtures/sample-building.json");

    fn sample() -> Building {
        Building::from_json(SAMPLE).unwrap()
    }

    /// (label, units) per entry, in fill order.
    fn units(b: &Building) -> Vec<(String, u32)> {
        let (layout, _) = Layout::new(b, &HueBook::new());
        assert_eq!(layout.cells.len(), UNITS);
        assert_eq!(
            layout.entries.iter().map(|e| e.units).sum::<u32>(),
            UNITS as u32
        );
        layout
            .entries
            .iter()
            .map(|e| {
                let label = match e.owner {
                    Owner::Staff => "Staff".to_string(),
                    Owner::Tenant(t) => b.tenants[t].name.clone(),
                    Owner::Others => "Others".to_string(),
                    Owner::Vacant => "Vacant".to_string(),
                    Owner::Boxes(_) => "Boxes".to_string(),
                };
                (label, e.units)
            })
            .collect()
    }

    #[test]
    fn sample_others_holds_terminal_and_notepad() {
        let b = sample();
        let (layout, _) = Layout::new(&b, &HueBook::new());
        let merged: Vec<&str> = layout
            .merged
            .iter()
            .map(|&t| b.tenants[t].name.as_str())
            .collect();
        assert_eq!(merged, ["Terminal", "Notepad"]);
        assert_eq!(layout.units_of(Owner::Others), 1);
        let others = layout
            .entries
            .iter()
            .find(|e| e.owner == Owner::Others)
            .unwrap();
        assert_eq!(others.bytes, 72 * MIB);
    }

    #[test]
    fn sample_cells_fill_from_the_ground_floor_up() {
        let b = sample();
        let (layout, _) = Layout::new(&b, &HueBook::new());
        assert!(layout.cells[..44].iter().all(|&o| o == Owner::Staff));
        assert_eq!(layout.cells[44], Owner::Tenant(0)); // Chrome
        assert!(layout.cells[256 - 78..].iter().all(|&o| o == Owner::Vacant));
        let staff = layout.segments(Owner::Staff);
        assert_eq!(staff.len(), 3);
        assert_eq!(
            staff[2],
            Segment {
                floor: 2,
                first_col: 0,
                last_col: 11
            }
        );
    }

    #[test]
    fn sample_hues_follow_fill_order() {
        let b = sample();
        let (layout, book) = Layout::new(&b, &HueBook::new());
        let hues: Vec<u8> = layout.named.iter().map(|n| n.hue).collect();
        assert_eq!(hues, (0..10).collect::<Vec<u8>>());
        assert_eq!(book.len(), 10);
        assert_eq!(book["onedrive.exe"], 9);
    }

    #[test]
    fn no_vacant_memory() {
        let gib = 1u64 << 30;
        let b = building(2 * gib, &[("A", 4 * gib), ("B", 2 * gib)], 0);
        let got = units(&b);
        assert_eq!(
            got,
            [
                ("Staff".into(), 64),
                ("A".into(), 128),
                ("B".into(), 64),
                ("Vacant".into(), 0)
            ]
        );
    }

    #[test]
    fn one_tenant_holding_nearly_everything() {
        // 8 GiB: a unit is 32 MiB. The tenant gets every unit by rules 1
        // and 4, then rule 5 hands one back to Staff. Vacant stays at zero.
        let b = building(2 * MIB, &[("Hog", 8189 * MIB)], MIB);
        let got = units(&b);
        assert_eq!(
            got,
            [
                ("Staff".into(), 1),
                ("Hog".into(), 255),
                ("Vacant".into(), 0)
            ]
        );
    }

    #[test]
    fn sixty_equal_tenants() {
        let names: Vec<String> = (0..60).map(|k| format!("T{k:02}")).collect();
        let tenants: Vec<(&str, u64)> = names.iter().map(|n| (n.as_str(), 100 * MIB)).collect();
        let b = building(1000 * MIB, &tenants, 1192 * MIB); // 8 GiB
        let (layout, book) = Layout::new(&b, &HueBook::new());
        assert_eq!(layout.named.len(), MAX_NAMED);
        // Equal rent: ties go by name, so T00 to T39 are named.
        let named: Vec<&str> = layout
            .named
            .iter()
            .map(|n| b.tenants[n.tenant].name.as_str())
            .collect();
        assert_eq!(named.first(), Some(&"T00"));
        assert_eq!(named.last(), Some(&"T39"));
        assert_eq!(layout.merged.len(), 20);
        let others = layout
            .entries
            .iter()
            .find(|e| e.owner == Owner::Others)
            .unwrap();
        assert_eq!(others.bytes, 2000 * MIB);
        // A unit is 32 MiB. Whole units: Staff 31 (rem 8 MiB), each tenant
        // 3 (rem 4 MiB), Others 62 (rem 16 MiB), Vacant 37 (rem 8 MiB): 250.
        // Six leftovers: Others, then Vacant and Staff (equal remainders,
        // Vacant holds more bytes), then T00 to T02 in fill order.
        let got = units(&b);
        assert_eq!(got[0], ("Staff".into(), 32));
        assert!(got[1..4].iter().all(|(_, u)| *u == 4), "{got:?}");
        assert!(got[4..41].iter().all(|(_, u)| *u == 3), "{got:?}");
        assert_eq!(got[41], ("Others".into(), 63));
        assert_eq!(got[42], ("Vacant".into(), 38));
        // All 40 hues cycle through the 12-colour palette.
        let mut counts = [0; HUE_COUNT];
        for hue in book.values() {
            counts[usize::from(*hue)] += 1;
        }
        assert_eq!(counts, [4, 4, 4, 4, 3, 3, 3, 3, 3, 3, 3, 3]);
    }

    #[test]
    fn tenants_under_half_a_unit_merge_into_others() {
        // 8 GiB: a unit is 32 MiB, half a unit 16 MiB.
        let b = building(
            4000 * MIB,
            &[("Half", 16 * MIB), ("Under", 16 * MIB - 1), ("Tiny", MIB)],
            4159 * MIB + 1,
        );
        assert_eq!(b.capacity_bytes, 8 << 30);
        let (layout, _) = Layout::new(&b, &HueBook::new());
        let named: Vec<&str> = layout
            .named
            .iter()
            .map(|n| b.tenants[n.tenant].name.as_str())
            .collect();
        assert_eq!(named, ["Half"]);
        let merged: Vec<&str> = layout
            .merged
            .iter()
            .map(|&t| b.tenants[t].name.as_str())
            .collect();
        assert_eq!(merged, ["Under", "Tiny"]);
        // Whole units: Staff 125, Half 0, Others 0, Vacant 129. The two
        // leftovers go to Vacant (0.97) and Others (0.53) before Half (0.5);
        // then Half takes one from Vacant by rule 5.
        let got = units(&b);
        assert_eq!(
            got,
            [
                ("Staff".into(), 125),
                ("Half".into(), 1),
                ("Others".into(), 1),
                ("Vacant".into(), 129)
            ]
        );
    }

    #[test]
    fn capacity_that_is_not_a_power_of_two() {
        // 6,000,000,007 bytes: a unit is 23,437,500.03 bytes.
        let b = building(
            1_000_000_000,
            &[("A", 2_500_000_000), ("B", 499_999_999), ("C", 12_000_000)],
            1_988_000_008,
        );
        assert_eq!(b.capacity_bytes, 6_000_000_007);
        // Whole units 42, 106, 21, 0, 84 (253). The three leftovers go to
        // Vacant (0.82), Staff (0.66667) and A (0.66666); C (0.51 units)
        // then takes one from A, which holds the most, by rule 5.
        let got = units(&b);
        assert_eq!(
            got,
            [
                ("Staff".into(), 43),
                ("A".into(), 106),
                ("B".into(), 21),
                ("C".into(), 1),
                ("Vacant".into(), 85),
            ]
        );
    }

    #[test]
    fn remainder_ties_go_to_more_bytes_then_fill_order() {
        // 512 bytes: a unit is 2 bytes. Every entry has the same remainder
        // (half a unit) and two leftover units are handed out.
        let mut entries = [101, 51, 31, 329].map(|bytes| Entry {
            owner: Owner::Staff,
            bytes,
            units: 0,
        });
        entries[3].owner = Owner::Vacant;
        divide(512, &mut entries);
        assert_eq!(entries.map(|e| e.units), [51, 25, 15, 165]);

        // Equal bytes and remainders: fill order decides.
        let mut entries = [100, 51, 51, 310].map(|bytes| Entry {
            owner: Owner::Staff,
            bytes,
            units: 0,
        });
        divide(512, &mut entries);
        assert_eq!(entries.map(|e| e.units), [50, 26, 25, 155]);
    }

    #[test]
    fn vacant_may_be_zero_even_with_memory() {
        // Vacant holds 0.1 units and loses the one leftover to Staff.
        let mut entries = [
            Entry {
                owner: Owner::Staff,
                bytes: 1022 * 100 + 60,
                units: 0,
            },
            Entry {
                owner: Owner::Vacant,
                bytes: 40,
                units: 0,
            },
        ];
        divide(1022 * 100 + 100, &mut entries);
        assert_eq!(entries.map(|e| e.units), [256, 0]);
    }

    #[test]
    fn saved_hues_are_kept_and_new_tenants_take_the_least_used() {
        let mut saved = HueBook::new();
        saved.insert("a".into(), 0);
        saved.insert("b".into(), 0);
        saved.insert("gone".into(), 3);
        saved.insert("broken".into(), 200);
        let book = assign_hues(&["a", "b", "c", "d"], &saved);
        assert_eq!(book["a"], 0);
        assert_eq!(book["b"], 0);
        assert_eq!(book["c"], 1);
        assert_eq!(book["d"], 2);
        // A tenant that is not named now keeps its hue but does not count.
        assert_eq!(book["gone"], 3);
        assert!(!book.contains_key("broken"));
    }

    #[test]
    fn an_explicit_order_rearranges_named_tenants_only() {
        let b = building(
            1000 * MIB,
            &[("A", 3000 * MIB), ("B", 2000 * MIB), ("C", 10 * MIB)],
            2182 * MIB,
        );
        let order = vec!["b".to_string(), "a".to_string()];
        let (layout, _) = Layout::ordered(&b, &HueBook::new(), &order);
        let fill: Vec<&str> = layout
            .named
            .iter()
            .map(|n| b.tenants[n.tenant].name.as_str())
            .collect();
        assert_eq!(fill, ["B", "A"]);
        assert_eq!(layout.merged, [2]);
        assert_eq!(
            layout.cells[layout.units_of(Owner::Staff) as usize],
            Owner::Tenant(1)
        );
    }

    #[test]
    fn boxes_go_to_the_basement_split_by_packed_memory() {
        // 8 GiB: a unit is 32 MiB. 1 GiB of boxes; A packed 3 GiB, B 1 GiB,
        // C 10 MiB (too little for boxes of its own).
        let mut b = building(
            1000 * MIB,
            &[("A", 2000 * MIB), ("B", 1000 * MIB), ("C", 100 * MIB)],
            3068 * MIB,
        );
        b.boxes_bytes = 1024 * MIB;
        b.capacity_bytes += 1024 * MIB;
        b.tenants[0].packed_bytes = 3072 * MIB;
        b.tenants[1].packed_bytes = 1024 * MIB;
        b.tenants[2].packed_bytes = 10 * MIB;
        b.validate().unwrap();
        let (named, _) = split_named(&b);
        let shares = box_shares(&b, &named);
        let total_packed = 4106u128;
        let a = (1024u128 * MIB as u128 * 3072 / total_packed) as u64;
        let bb = (1024u128 * MIB as u128 * 1024 / total_packed) as u64;
        assert_eq!(
            shares,
            [(Some(0), a), (Some(1), bb), (None, 1024 * MIB - a - bb)]
        );

        let (layout, _) = Layout::new(&b, &HueBook::new());
        assert_eq!(layout.entries[0].owner, Owner::Boxes(Some(0)));
        assert_eq!(layout.cells[0], Owner::Boxes(Some(0)));
        // 8 GiB: a unit is 32 MiB. A's boxes (23.9 units) and B's (7.98)
        // round up to 24 and 8; the shared stack (2.5 MiB) gets one unit
        // by rule 5.
        assert_eq!(layout.box_units(), 33);
        assert_eq!(layout.units_of(Owner::Boxes(None)), 1);
        assert_eq!(layout.entries.iter().map(|e| e.units).sum::<u32>(), 256);
    }

    #[test]
    fn boxes_with_nothing_packed_are_nobodys() {
        let mut b = building(1000 * MIB, &[("A", 2000 * MIB)], 4168 * MIB);
        b.boxes_bytes = 1024 * MIB;
        b.capacity_bytes += 1024 * MIB;
        let (named, _) = split_named(&b);
        assert_eq!(box_shares(&b, &named), [(None, 1024 * MIB)]);
        b.boxes_bytes = 0;
        assert!(box_shares(&b, &named).is_empty());
    }

    #[test]
    fn ties_in_rent_are_ranked_by_name() {
        let b = sample();
        let order: Vec<&str> = ranked(&b)
            .iter()
            .map(|&t| b.tenants[t].name.as_str())
            .collect();
        let copilot = order.iter().position(|&n| n == "Copilot").unwrap();
        assert_eq!(order[copilot + 1], "WhatsApp");
    }
}
