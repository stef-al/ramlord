//! M5: the SMBIOS parser on byte fixtures (fixtures/smbios, made by
//! make.py there): DDR4, DDR5, LPDDR5, a damaged table and a table without
//! memory devices.

use std::path::Path;

use ramlord_core::smbios::{MemoryType, memory_type};

fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/smbios")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn a_desktop_with_two_ddr4_dimms_and_two_empty_slots() {
    assert_eq!(memory_type(&fixture("ddr4.bin")), Some(MemoryType::Ddr4));
}

#[test]
fn a_laptop_with_ddr5_sodimms() {
    assert_eq!(memory_type(&fixture("ddr5.bin")), Some(MemoryType::Ddr5));
}

#[test]
fn soldered_lpddr5() {
    assert_eq!(
        memory_type(&fixture("lpddr5.bin")),
        Some(MemoryType::Lpddr5)
    );
}

#[test]
fn a_damaged_table_gives_no_type_and_no_panic() {
    assert_eq!(memory_type(&fixture("damaged.bin")), None);
}

#[test]
fn a_table_without_memory_devices_gives_no_type() {
    assert_eq!(memory_type(&fixture("no-memory.bin")), None);
}

#[test]
fn every_cut_of_every_table_parses_without_panic() {
    for name in [
        "ddr4.bin",
        "ddr5.bin",
        "lpddr5.bin",
        "damaged.bin",
        "no-memory.bin",
    ] {
        let bytes = fixture(name);
        for end in 0..=bytes.len() {
            let _ = memory_type(&bytes[..end]);
        }
    }
}

#[test]
fn damage_stops_the_walk_where_it_starts() {
    // Corrupt the length byte of the third Memory Device of the DDR4
    // desktop (the second filled slot): the first device was read.
    let mut bytes = fixture("ddr4.bin");
    let devices: Vec<usize> = (8..bytes.len() - 1)
        .filter(|&k| bytes[k] == 17 && bytes[k + 1] == 0x28)
        .collect();
    assert_eq!(devices.len(), 4);
    bytes[devices[2] + 1] = 2;
    assert_eq!(memory_type(&bytes), Some(MemoryType::Ddr4));
    // A length running past the end is damage too.
    let mut bytes = fixture("ddr5.bin");
    let first = (8..bytes.len()).find(|&k| bytes[k] == 17).unwrap();
    bytes[first + 1] = 0xFF;
    assert_eq!(memory_type(&bytes), None);
}

#[test]
fn the_most_common_installed_type_wins() {
    // DDR4 twice, then one DDR5 device: DDR4.
    let ddr4 = fixture("ddr4.bin");
    let ddr5 = fixture("ddr5.bin");
    let devices = |b: &[u8]| -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        let mut at = 8;
        while at + 4 <= b.len() {
            let len = b[at + 1] as usize;
            let end = at + len + b[at + len..].windows(2).position(|w| w == [0, 0]).unwrap() + 2;
            if b[at] == 17 {
                out.push(b[at..end].to_vec());
            }
            if b[at] == 127 {
                break;
            }
            at = end;
        }
        out
    };
    let mut table: Vec<u8> = devices(&ddr4).concat();
    table.extend(devices(&ddr5)[0].clone());
    table.extend([127, 4, 0xFF, 0xFE, 0, 0]);
    let mut raw = vec![0, 3, 4, 0];
    raw.extend((table.len() as u32).to_le_bytes());
    raw.extend(table);
    assert_eq!(memory_type(&raw), Some(MemoryType::Ddr4));
}
