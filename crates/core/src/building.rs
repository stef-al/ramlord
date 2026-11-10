//! The Building: tenants with flags, Staff and Vacant at one moment.
//!
//! This is the input of `layout` and the `--building` file format. All
//! sizes are bytes; Staff, the tenants' rent and Vacant sum to capacity.

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
    #[serde(default)]
    pub flags: Vec<Flag>,
}

impl Tenant {
    pub fn has(&self, flag: Flag) -> bool {
        self.flags.contains(&flag)
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
    /// Staff, rent and Vacant must sum to capacity.
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
                "staff + tenants' rent + vacant is {sum} bytes, capacity is {capacity}"
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
            + self
                .tenants
                .iter()
                .map(|t| u128::from(t.rent_bytes))
                .sum::<u128>();
        if sum != u128::from(self.capacity_bytes) {
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

    #[test]
    fn sample_occupancy_is_69_percent() {
        let b = Building::from_json(SAMPLE).unwrap();
        assert_eq!(b.occupancy_percent(), 69);
    }

    fn edit(f: impl FnOnce(&mut serde_json::Value)) -> String {
        let mut v: serde_json::Value = serde_json::from_str(SAMPLE).unwrap();
        f(&mut v);
        v.to_string()
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
    fn tag_priority_is_leak_squat_away_walls() {
        let tags: Vec<_> = Flag::TAG_PRIORITY.iter().filter_map(|f| f.tag()).collect();
        assert_eq!(tags, ["LEAK", "SQUAT", "AWAY", "WALLS"]);
        assert_eq!(Flag::Family.tag(), None);
    }
}
