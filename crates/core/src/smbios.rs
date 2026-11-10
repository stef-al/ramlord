//! The memory type, from the SMBIOS table (SPEC.md, Prices): the Memory
//! Device structures (type 17), byte 0x12, of the modules that are
//! installed. The bytes come from `GetSystemFirmwareTable('RSMB')`, which
//! starts with an 8-byte RawSMBIOSData header. A damaged table never
//! panics: parsing stops where the damage starts.

use serde::{Deserialize, Serialize};

/// The memory types RAMlord prices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
pub enum MemoryType {
    #[serde(rename = "DDR3")]
    Ddr3,
    #[serde(rename = "DDR4")]
    Ddr4,
    #[serde(rename = "LPDDR4")]
    Lpddr4,
    #[serde(rename = "DDR5")]
    Ddr5,
    #[serde(rename = "LPDDR5")]
    Lpddr5,
}

impl MemoryType {
    pub const ALL: [MemoryType; 5] = [
        MemoryType::Ddr3,
        MemoryType::Ddr4,
        MemoryType::Lpddr4,
        MemoryType::Ddr5,
        MemoryType::Lpddr5,
    ];

    /// The SMBIOS Memory Type code (SMBIOS 3.x, table 76).
    pub fn from_code(code: u8) -> Option<MemoryType> {
        Some(match code {
            0x18 => MemoryType::Ddr3,
            0x1A => MemoryType::Ddr4,
            0x1E => MemoryType::Lpddr4,
            0x22 => MemoryType::Ddr5,
            0x23 => MemoryType::Lpddr5,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            MemoryType::Ddr3 => "DDR3",
            MemoryType::Ddr4 => "DDR4",
            MemoryType::Lpddr4 => "LPDDR4",
            MemoryType::Ddr5 => "DDR5",
            MemoryType::Lpddr5 => "LPDDR5",
        }
    }

    /// The price file entry this type is billed at, and whether that is an
    /// estimate: LPDDR uses the matching DDR price, DDR3 the DDR4 price.
    pub fn priced_as(self) -> (&'static str, bool) {
        match self {
            MemoryType::Ddr5 => ("DDR5", false),
            MemoryType::Ddr4 => ("DDR4", false),
            MemoryType::Lpddr5 => ("DDR5", true),
            MemoryType::Lpddr4 => ("DDR4", true),
            MemoryType::Ddr3 => ("DDR4", true),
        }
    }
}

/// The SMBIOS structure type of a Memory Device.
const MEMORY_DEVICE: u8 = 17;
/// The end-of-table structure.
const END_OF_TABLE: u8 = 127;
/// Offsets inside a Memory Device's formatted area.
const SIZE_AT: usize = 0x0C;
const TYPE_AT: usize = 0x12;

/// The structure table inside a RawSMBIOSData buffer: an 8-byte header
/// (calling method, major, minor, DMI revision, then a little-endian u32
/// length) and the table. A length past the buffer is cut to the buffer.
fn table(raw: &[u8]) -> &[u8] {
    let Some(header) = raw.get(..8) else {
        return &[];
    };
    let length = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
    &raw[8..raw.len().min(8 + length)]
}

/// The memory type of the installed modules: the most common one among
/// the Memory Devices that hold a module, the first on a tie. None when no
/// installed device names a type RAMlord knows, or the table is unreadable.
pub fn memory_type(raw: &[u8]) -> Option<MemoryType> {
    let table = table(raw);
    let mut found: Vec<MemoryType> = Vec::new();
    let mut at = 0;
    while at + 4 <= table.len() {
        let kind = table[at];
        let length = usize::from(table[at + 1]);
        // A formatted area shorter than its header, or running past the
        // table, is damage: stop here.
        if length < 4 || at + length > table.len() {
            break;
        }
        let formatted = &table[at..at + length];
        if kind == MEMORY_DEVICE && length > TYPE_AT {
            let size = u16::from_le_bytes([formatted[SIZE_AT], formatted[SIZE_AT + 1]]);
            // 0: no module in this slot.
            if size != 0
                && let Some(memory) = MemoryType::from_code(formatted[TYPE_AT])
            {
                found.push(memory);
            }
        }
        if kind == END_OF_TABLE {
            break;
        }
        // The strings follow, ended by two zero bytes.
        let strings = &table[at + length..];
        match strings.windows(2).position(|w| w == [0, 0]) {
            Some(end) => at += length + end + 2,
            None => break,
        }
    }
    let count = |m: &MemoryType| found.iter().filter(|f| *f == m).count();
    found
        .iter()
        .copied()
        .fold(None, |best: Option<MemoryType>, m| match best {
            Some(b) if count(&b) >= count(&m) => Some(b),
            _ => Some(m),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_map_to_types_and_prices() {
        assert_eq!(MemoryType::from_code(0x22), Some(MemoryType::Ddr5));
        assert_eq!(MemoryType::from_code(0x1A), Some(MemoryType::Ddr4));
        assert_eq!(MemoryType::from_code(0x23), Some(MemoryType::Lpddr5));
        assert_eq!(MemoryType::from_code(0x02), None);
        assert_eq!(MemoryType::Lpddr5.priced_as(), ("DDR5", true));
        assert_eq!(MemoryType::Ddr3.priced_as(), ("DDR4", true));
        assert_eq!(MemoryType::Ddr4.priced_as(), ("DDR4", false));
    }

    #[test]
    fn short_and_empty_buffers_have_no_type() {
        assert_eq!(memory_type(&[]), None);
        assert_eq!(memory_type(&[0, 3, 4, 0]), None);
        assert_eq!(memory_type(&[0, 3, 4, 0, 0, 0, 0, 0]), None);
    }
}
