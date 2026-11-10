//! The sample building table in SPEC.md as a test vector: 32 GB of DDR5 at
//! $14.34 per GB, 10,035 MB vacant, rent owed $241, occupancy 69%.

use ramlord_core::format::{MIB, gb_text};
use ramlord_core::layout::{Layout, Owner};
use ramlord_core::scene::{Roll, whole_rent};
use ramlord_core::{Building, HueBook};

const SAMPLE: &str = include_str!("../../../fixtures/sample-building.json");

/// Tenant, MB, units, rent in whole dollars: the SPEC.md table.
const TABLE: [(&str, u64, u32, u64); 13] = [
    ("Staff", 5_560, 44, 78),
    ("Chrome", 6_554, 51, 92),
    ("Discord", 2_970, 23, 42),
    ("VS Code", 2_253, 18, 32),
    ("Teams", 1_638, 13, 23),
    ("Steam", 922, 7, 13),
    ("Slack", 819, 6, 11),
    ("Spotify", 614, 5, 9),
    ("Copilot", 512, 4, 7),
    ("WhatsApp", 512, 4, 7),
    ("OneDrive", 307, 2, 4),
    ("Others", 72, 1, 1),
    ("Vacant", 10_035, 78, 141),
];

fn sample() -> (Building, Layout) {
    let building = Building::from_json(SAMPLE).expect("the sample parses");
    let (layout, _) = Layout::new(&building, &HueBook::new());
    (building, layout)
}

fn label(building: &Building, owner: Owner) -> &str {
    match owner {
        Owner::Staff => "Staff",
        Owner::Tenant(t) => &building.tenants[t].name,
        Owner::Others => "Others",
        Owner::Vacant => "Vacant",
        Owner::Boxes(_) => "Boxes",
    }
}

#[test]
fn sizes_match_the_spec_table() {
    let (b, layout) = sample();
    let got: Vec<(&str, u64)> = layout
        .entries
        .iter()
        .map(|e| (label(&b, e.owner), e.bytes))
        .collect();
    let expected: Vec<(&str, u64)> = TABLE.iter().map(|r| (r.0, r.1 * MIB)).collect();
    assert_eq!(got, expected);
    assert_eq!(b.capacity_bytes, 32_768 * MIB);
}

#[test]
fn units_match_the_spec_table() {
    let (b, layout) = sample();
    let got: Vec<(&str, u32)> = layout
        .entries
        .iter()
        .map(|e| (label(&b, e.owner), e.units))
        .collect();
    let expected: Vec<(&str, u32)> = TABLE.iter().map(|r| (r.0, r.2)).collect();
    assert_eq!(got, expected);
}

#[test]
fn rents_match_the_spec_table() {
    let (b, layout) = sample();
    let got: Vec<(&str, u64)> = layout
        .entries
        .iter()
        .map(|e| (label(&b, e.owner), whole_rent(&b, e.bytes)))
        .collect();
    let expected: Vec<(&str, u64)> = TABLE.iter().map(|r| (r.0, r.3)).collect();
    assert_eq!(got, expected);
}

#[test]
fn rent_owed_is_241_and_adds_up_the_rows() {
    let (b, layout) = sample();
    let roll = Roll::new(&b, &layout);
    assert_eq!(roll.owed.text("$"), "$241");
    let rows: u64 = roll.rows.iter().map(|r| r.rent.cents).sum();
    assert_eq!(rows + roll.others.unwrap().2.cents, roll.owed.cents);
}

#[test]
fn occupancy_is_69_percent() {
    let (b, _) = sample();
    assert_eq!(b.occupancy_percent(), 69);
    assert_eq!(gb_text(b.vacant_bytes), "9.8");
}

#[test]
fn rent_roll_rows_tags_and_others() {
    let (b, layout) = sample();
    let roll = Roll::new(&b, &layout);
    let rows: Vec<(&str, Option<&str>, String, String)> = roll
        .rows
        .iter()
        .map(|r| (r.name.as_str(), r.tag, gb_text(r.bytes), r.rent.text("$")))
        .collect();
    let expected = [
        ("Chrome", None, "6.4", "$92"),
        ("Discord", Some("LEAK"), "2.9", "$42"),
        ("VS Code", None, "2.2", "$32"),
        ("Teams", Some("AWAY"), "1.6", "$23"),
        ("Steam", Some("WALLS"), "0.9", "$13"),
        ("Slack", Some("AWAY"), "0.8", "$11"),
        ("Spotify", None, "0.6", "$9"),
        ("Copilot", Some("SQUAT"), "0.5", "$7"),
        ("WhatsApp", Some("SQUAT"), "0.5", "$7"),
        ("OneDrive", Some("WALLS"), "0.3", "$4"),
    ];
    let expected: Vec<(&str, Option<&str>, String, String)> = expected
        .iter()
        .map(|&(n, t, g, r)| (n, t, g.to_string(), r.to_string()))
        .collect();
    assert_eq!(rows, expected);
    let (count, bytes, rent) = roll.others.unwrap();
    assert_eq!(
        (count, gb_text(bytes), rent.text("$")),
        (2, "0.1".into(), "$1".into())
    );
    assert_eq!(roll.staff.text("$"), "$78");
    assert_eq!(roll.vacant.text("$"), "$141");
    assert_eq!(roll.office.text("$"), "$0.43");
}
