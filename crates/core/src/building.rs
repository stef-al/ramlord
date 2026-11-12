//! The Building: tenants with flags, Staff and Vacant at one moment.
//!
//! This is the input of `layout` and the `--building` file format. All
//! sizes are bytes; Staff, the tenants' rent, the gallery, the boxes and
//! Vacant sum to capacity.

use serde::Deserialize;

use crate::date::Date;

/// One app: a group of related processes.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Tenant {
    /// Stable identity: the root image path in lower case.
    pub id: String,
    /// Display name, cut to 8 characters in the rent roll.
    pub name: String,
    /// Private working set, summed over the tenant's processes.
    pub rent_bytes: u64,
    /// Commit charge; shown on the details card only.
    pub lease_bytes: u64,
    pub process_count: u32,
    /// Committed memory not in RAM now (see `tenants::TenantSample`).
    #[serde(default)]
    pub packed_bytes: u64,
    /// Graphics memory its processes hold in RAM, estimated and fitted to
    /// the gallery: its pictures. Not part of its rent; part of its lease.
    #[serde(default)]
    pub gpu_bytes: u64,
    #[serde(default)]
    pub flags: Vec<Flag>,
}

impl Tenant {
    pub fn has(&self, flag: Flag) -> bool {
        self.flags.contains(&flag)
    }
}

/// What Staff is made of: the rows of the Staff card. The parts sum to
/// Staff. Each measured part is cut to what the parts before it leave,
/// and `other_bytes` is the rest, which Windows does not break down
/// without administrator rights.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, default)]
pub struct StaffParts {
    /// Windows' own processes and services: their private working sets,
    /// RAMlord's included.
    pub services_bytes: u64,
    /// Reserved by the hardware (firmware, built-in graphics): installed
    /// but never Windows' to use.
    pub hardware_bytes: u64,
    /// The kernel's pools in RAM: nonpaged, and the paged pool's resident
    /// part.
    pub kernel_pool_bytes: u64,
    /// Drivers' and the kernel's pageable code and data in RAM.
    pub drivers_bytes: u64,
    /// The file cache in RAM.
    pub file_cache_bytes: u64,
    /// Changed pages waiting to be written to disk.
    pub modified_bytes: u64,
    /// The rest: pages shared between processes (program code, mapped
    /// files), page tables, kernel stacks, memory drivers keep locked.
    pub other_bytes: u64,
}

impl StaffParts {
    /// Cuts the measured parts of `measured` to `staff`, in field order;
    /// `other_bytes` becomes the rest.
    pub fn cut(staff: u64, measured: &StaffParts) -> StaffParts {
        let mut left = staff;
        let mut take = |bytes: u64| {
            let taken = bytes.min(left);
            left -= taken;
            taken
        };
        let mut parts = StaffParts {
            services_bytes: take(measured.services_bytes),
            hardware_bytes: take(measured.hardware_bytes),
            kernel_pool_bytes: take(measured.kernel_pool_bytes),
            drivers_bytes: take(measured.drivers_bytes),
            file_cache_bytes: take(measured.file_cache_bytes),
            modified_bytes: take(measured.modified_bytes),
            other_bytes: 0,
        };
        parts.other_bytes = left;
        parts
    }

    /// The Staff card's rows: label and bytes, in order.
    pub fn rows(&self) -> [(&'static str, u64); 7] {
        [
            ("SERVICES", self.services_bytes),
            ("KERNEL POOLS", self.kernel_pool_bytes),
            ("DRIVERS", self.drivers_bytes),
            ("FILE CACHE", self.file_cache_bytes),
            ("TO DISK", self.modified_bytes),
            ("HARDWARE", self.hardware_bytes),
            ("SHARED, OTHER", self.other_bytes),
        ]
    }

    pub fn sum(&self) -> u64 {
        self.rows().iter().map(|r| r.1).sum()
    }
}

/// The flags a tenant can carry (SPEC.md, Flags).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Flag {
    /// LEAK: memory keeps growing.
    Leaker,
    /// SQUAT: started with Windows and never opened.
    Squatter,
    /// AWAY: has a window that has gone 4 watched hours without focus.
    Away,
    /// WALLS: owned no visible window during the last 10 watched minutes.
    Walls,
    /// Marked by the user; never flagged, never suggested for eviction.
    Family,
}

impl Flag {
    /// The rent roll shows one tag, in this priority: LEAK, SQUAT, AWAY, WALLS.
    pub const TAG_PRIORITY: [Flag; 4] = [Flag::Leaker, Flag::Squatter, Flag::Away, Flag::Walls];

    pub fn tag(self) -> Option<&'static str> {
        match self {
            Flag::Leaker => Some("LEAK"),
            Flag::Squatter => Some("SQUAT"),
            Flag::Away => Some("AWAY"),
            Flag::Walls => Some("WALLS"),
            Flag::Family => None,
        }
    }
}

/// The price of memory, in micro-units of the currency per GB (2^30 bytes).
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Price {
    pub per_gb_micros: u64,
    pub symbol: String,
    /// `YYYY-MM-DD` date of the price.
    pub as_of: String,
    /// Priced as another type (LPDDR as DDR, DDR3 as DDR4, or an unknown
    /// type): the price line marks it.
    #[serde(default)]
    pub estimated: bool,
}

impl Price {
    /// The DDR5 price from `prices.json`: what `--snapshot` and
    /// `--self-test` bill at. The live app bills by `prices::lease`.
    pub fn placeholder() -> Price {
        Price {
            per_gb_micros: 14_340_000,
            symbol: "$".into(),
            as_of: "2026-11-03".into(),
            estimated: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Building {
    /// Always `"building"`; tells a Building file from a Snapshot file.
    pub kind: String,
    /// `YYYY-MM-DD` local date of the reading.
    pub date: String,
    /// Memory type as shown on the price line, such as `DDR5`.
    pub memory_type: String,
    pub price: Price,
    pub capacity_bytes: u64,
    pub vacant_bytes: u64,
    pub staff_bytes: u64,
    /// RAMlord's own memory: the `MY OFFICE` line. Already part of Staff.
    pub own_bytes: u64,
    /// The landlord's remark, at most 60 characters.
    pub remark: String,
    /// Rent is the whole working set, not the private one: amounts derived
    /// from rent are shown with a `~` in front.
    #[serde(default)]
    pub rent_is_estimate: bool,
    /// Tenants' pages kept compressed by Windows: the boxes in the
    /// basement. Apart from Staff; part of the sum to capacity.
    #[serde(default)]
    pub boxes_bytes: u64,
    /// Graphics memory in RAM, everyone's: the gallery, where tenants hang
    /// the pictures they draw. Apart from Staff; part of the sum to
    /// capacity.
    #[serde(default)]
    pub gallery_bytes: u64,
    /// What Staff is made of. All zero in a building file that does not
    /// say; the Staff card then shows Staff as one row.
    #[serde(default)]
    pub staff_parts: StaffParts,
    /// Staff processes' committed memory not in RAM, for Staff's share of
    /// the boxes (see `Tenant::packed_bytes`).
    #[serde(default)]
    pub staff_packed_bytes: u64,
    pub tenants: Vec<Tenant>,
}

/// Why a Building file was refused.
#[derive(Debug, PartialEq, Eq)]
pub enum BuildingError {
    Json(String),
    Kind(String),
    Date {
        field: &'static str,
        value: String,
    },
    ZeroCapacity,
    ZeroPrice,
    /// Staff, rent, the gallery, the boxes and Vacant must sum to
    /// capacity.
    Unbalanced {
        capacity: u64,
        sum: u128,
    },
    EmptyId,
    DuplicateId(String),
}

impl std::fmt::Display for BuildingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildingError::Json(e) => write!(f, "not a valid building file: {e}"),
            BuildingError::Kind(k) => write!(f, "kind is {k:?}, expected \"building\""),
            BuildingError::Date { field, value } => {
                write!(f, "{field} is {value:?}, expected YYYY-MM-DD")
            }
            BuildingError::ZeroCapacity => write!(f, "capacity_bytes is 0"),
            BuildingError::ZeroPrice => write!(f, "price.per_gb_micros is 0"),
            BuildingError::Unbalanced { capacity, sum } => write!(
                f,
                "staff + tenants' rent + gallery + boxes + vacant is {sum} bytes, capacity is {capacity} \
                 (only an overfull building, with no staff and no vacancy, may exceed it)"
            ),
            BuildingError::EmptyId => write!(f, "a tenant has an empty id"),
            BuildingError::DuplicateId(id) => write!(f, "tenant id {id:?} appears twice"),
        }
    }
}

impl std::error::Error for BuildingError {}

impl Building {
    /// Parses and validates a Building file.
    pub fn from_json(text: &str) -> Result<Building, BuildingError> {
        let building: Building =
            serde_json::from_str(text).map_err(|e| BuildingError::Json(e.to_string()))?;
        building.validate()?;
        Ok(building)
    }

    /// Checks the invariants every later stage relies on.
    pub fn validate(&self) -> Result<(), BuildingError> {
        if self.kind != "building" {
            return Err(BuildingError::Kind(self.kind.clone()));
        }
        for (field, value) in [("date", &self.date), ("price.as_of", &self.price.as_of)] {
            if Date::parse(value).is_none() {
                return Err(BuildingError::Date {
                    field,
                    value: value.clone(),
                });
            }
        }
        if self.capacity_bytes == 0 {
            return Err(BuildingError::ZeroCapacity);
        }
        if self.price.per_gb_micros == 0 {
            return Err(BuildingError::ZeroPrice);
        }
        let sum = u128::from(self.staff_bytes)
            + u128::from(self.vacant_bytes)
            + u128::from(self.boxes_bytes)
            + u128::from(self.gallery_bytes)
            + self
                .tenants
                .iter()
                .map(|t| u128::from(t.rent_bytes))
                .sum::<u128>();
        // Estimated rent alone can exceed capacity; Staff and Vacant are
        // then zero and the building is overfull.
        let overfull = self.staff_bytes == 0
            && self.vacant_bytes == 0
            && sum > u128::from(self.capacity_bytes);
        if sum != u128::from(self.capacity_bytes) && !overfull {
            return Err(BuildingError::Unbalanced {
                capacity: self.capacity_bytes,
                sum,
            });
        }
        let mut ids: Vec<&str> = self.tenants.iter().map(|t| t.id.as_str()).collect();
        if ids.iter().any(|id| id.is_empty()) {
            return Err(BuildingError::EmptyId);
        }
        ids.sort_unstable();
        if let Some(pair) = ids.windows(2).find(|w| w[0] == w[1]) {
            return Err(BuildingError::DuplicateId(pair[0].to_string()));
        }
        Ok(())
    }

    /// Total rent of all tenants.
    pub fn rent_bytes(&self) -> u64 {
        self.tenants.iter().map(|t| t.rent_bytes).sum()
    }

    /// What the 256 units divide: capacity, or more in an overfull building.
    pub fn split_bytes(&self) -> u64 {
        let sum = self
            .staff_bytes
            .saturating_add(self.vacant_bytes)
            .saturating_add(self.boxes_bytes)
            .saturating_add(self.gallery_bytes)
            .saturating_add(self.rent_bytes());
        self.capacity_bytes.max(sum)
    }

    /// Occupied share of the building in whole percent, halves rounded up.
    pub fn occupancy_percent(&self) -> u64 {
        let capacity = u128::from(self.capacity_bytes);
        let occupied = capacity - u128::from(self.vacant_bytes.min(self.capacity_bytes));
        ((occupied * 100 + capacity / 2) / capacity) as u64
    }

    /// The date of the reading. Valid after `validate`.
    pub fn reading_date(&self) -> Date {
        Date::parse(&self.date).unwrap_or(Date::EPOCH)
    }

    /// The date of the price. Valid after `validate`.
    pub fn price_date(&self) -> Date {
        Date::parse(&self.price.as_of).unwrap_or(Date::EPOCH)
    }
}

/// A valid building for tests: the given tenants (name, rent), Staff
/// and Vacant as given, capacity their sum.
#[cfg(test)]
pub(crate) fn test_building(staff: u64, tenants: &[(&str, u64)], vacant: u64) -> Building {
    let rent: u64 = tenants.iter().map(|t| t.1).sum();
    let b = Building {
        kind: "building".into(),
        date: "2026-11-09".into(),
        memory_type: "DDR5".into(),
        price: Price {
            per_gb_micros: 14_340_000,
            symbol: "$".into(),
            as_of: "2026-11-03".into(),
            estimated: false,
        },
        capacity_bytes: staff + rent + vacant,
        vacant_bytes: vacant,
        staff_bytes: staff,
        own_bytes: 0,
        remark: String::new(),
        rent_is_estimate: false,
        boxes_bytes: 0,
        gallery_bytes: 0,
        staff_parts: StaffParts::default(),
        staff_packed_bytes: 0,
        tenants: tenants
            .iter()
            .map(|&(name, rent_bytes)| Tenant {
                id: name.to_lowercase(),
                name: name.into(),
                rent_bytes,
                lease_bytes: rent_bytes,
                process_count: 1,
                packed_bytes: 0,
                gpu_bytes: 0,
                flags: vec![],
            })
            .collect(),
    };
    b.validate().expect("test building is valid");
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("../../../fixtures/sample-building.json");

    #[test]
    fn sample_fixture_parses_and_validates() {
        let b = Building::from_json(SAMPLE).expect("sample parses");
        assert_eq!(b.tenants.len(), 12);
        assert_eq!(b.capacity_bytes, 32 << 30);
        assert_eq!(b.vacant_bytes, 10_035 << 20);
        assert_eq!(b.staff_bytes, 5_560 << 20);
        assert_eq!(b.price.per_gb_micros, 14_340_000);
        assert!(b.tenants[1].has(Flag::Leaker));
    }

    fn edit(f: impl FnOnce(&mut serde_json::Value)) -> String {
        let mut v: serde_json::Value = serde_json::from_str(SAMPLE).unwrap();
        f(&mut v);
        v.to_string()
    }

    #[test]
    fn staff_parts_are_cut_to_staff_and_the_rest_is_other() {
        let measured = StaffParts {
            services_bytes: 500,
            hardware_bytes: 100,
            kernel_pool_bytes: 300,
            drivers_bytes: 50,
            file_cache_bytes: 200,
            modified_bytes: 80,
            other_bytes: 999, // ignored: always the rest
        };
        let roomy = StaffParts::cut(2_000, &measured);
        assert_eq!(roomy.other_bytes, 2_000 - 1_230);
        assert_eq!(roomy.sum(), 2_000);
        // Measured parts that add up to more than Staff are cut in order:
        // services first, the modified list last.
        let tight = StaffParts::cut(1_000, &measured);
        assert_eq!(
            (
                tight.kernel_pool_bytes,
                tight.drivers_bytes,
                tight.file_cache_bytes
            ),
            (300, 50, 50)
        );
        assert_eq!((tight.modified_bytes, tight.other_bytes), (0, 0));
        assert_eq!(tight.sum(), 1_000);
        assert_eq!(StaffParts::cut(0, &measured).sum(), 0);
    }

    #[test]
    fn refuses_a_building_that_does_not_sum_to_capacity() {
        let text = edit(|v| v["staff_bytes"] = (1u64 << 30).into());
        assert!(matches!(
            Building::from_json(&text),
            Err(BuildingError::Unbalanced { .. })
        ));
    }

    #[test]
    fn refuses_wrong_kind_bad_dates_zero_capacity_and_duplicates() {
        let wrong_kind = edit(|v| v["kind"] = "snapshot".into());
        assert!(matches!(
            Building::from_json(&wrong_kind),
            Err(BuildingError::Kind(_))
        ));
        let bad_date = edit(|v| v["date"] = "9 Nov 2026".into());
        assert!(matches!(
            Building::from_json(&bad_date),
            Err(BuildingError::Date { .. })
        ));
        let bad_price_date = edit(|v| v["price"]["as_of"] = "2026-13-01".into());
        assert!(matches!(
            Building::from_json(&bad_price_date),
            Err(BuildingError::Date {
                field: "price.as_of",
                ..
            })
        ));
        let duplicate = edit(|v| v["tenants"][1]["id"] = "chrome.exe".into());
        assert_eq!(
            Building::from_json(&duplicate),
            Err(BuildingError::DuplicateId("chrome.exe".into()))
        );
        let unknown_flag = edit(|v| v["tenants"][0]["flags"] = serde_json::json!(["vampire"]));
        assert!(matches!(
            Building::from_json(&unknown_flag),
            Err(BuildingError::Json(_))
        ));
    }

    #[test]
    fn an_overfull_building_needs_no_staff_and_no_vacancy() {
        let mut b = test_building(0, &[("A", 300), ("B", 200)], 0);
        b.capacity_bytes = 400;
        assert_eq!(b.validate(), Ok(()));
        assert_eq!(b.split_bytes(), 500);
        b.vacant_bytes = 1;
        assert!(matches!(
            b.validate(),
            Err(BuildingError::Unbalanced { .. })
        ));
    }

    #[test]
    fn tag_priority_is_leak_squat_away_walls() {
        let tags: Vec<_> = Flag::TAG_PRIORITY.iter().filter_map(|f| f.tag()).collect();
        assert_eq!(tags, ["LEAK", "SQUAT", "AWAY", "WALLS"]);
        assert_eq!(Flag::Family.tag(), None);
    }
}
