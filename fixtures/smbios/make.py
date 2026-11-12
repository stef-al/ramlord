#!/usr/bin/env python3
"""Writes the SMBIOS byte fixtures for crates/core/tests/smbios.rs.

Each file is what GetSystemFirmwareTable('RSMB') returns: an 8-byte
RawSMBIOSData header (calling method, major, minor, DMI revision, a
little-endian u32 table length) and the structure table. The tables are
made by hand after real laptops and desktops: BIOS (type 0), System (1),
Physical Memory Array (16), Memory Devices (17, SMBIOS 2.8 layout,
0x28 bytes) and End-of-Table (127). Run it from anywhere:

    python3 fixtures/smbios/make.py
"""
import os
import struct

HERE = os.path.dirname(os.path.abspath(__file__))


def structure(kind, handle, body, strings=()):
    formatted = struct.pack("<BBH", kind, 4 + len(body), handle) + body
    tail = b"".join(s.encode() + b"\0" for s in strings) + b"\0"
    if not strings:
        tail += b"\0"
    return formatted + tail


def bios():
    body = struct.pack("<BBHBBQBB", 1, 2, 0xE800, 3, 0x0F, 0x0B, 0x0D, 0x0F)
    return structure(0, 0x0000, body, ["Firmware Vendor", "F.21", "06/17/2026"])


def system():
    body = bytes([1, 2, 3, 4]) + bytes(16) + bytes([6, 5, 6])
    return structure(1, 0x0001, body, ["Maker", "Laptop 840 G9", "1.0", "SN000"])


def memory_array(slots):
    body = struct.pack("<BBBIHHQ", 3, 3, 3, 0x02000000, 0xFFFE, slots, 0)[:0x13]
    return structure(16, 0x1000, body)


def memory_device(handle, size_mb, code, form, locator):
    # Offsets 0x04 to 0x27 of an SMBIOS 2.8 Memory Device.
    body = struct.pack(
        "<HHHHHBBBBBHHBBBBBIHHHH",
        0x1000,          # 0x04 physical memory array handle
        0xFFFE,          # 0x06 memory error information handle
        64,              # 0x08 total width
        64,              # 0x0A data width
        size_mb,         # 0x0C size in MB, 0 when the slot is empty
        form,            # 0x0E form factor
        0,               # 0x0F device set
        1,               # 0x10 device locator (string 1)
        2,               # 0x11 bank locator (string 2)
        code,            # 0x12 memory type
        0x0080,          # 0x13 type detail
        4800,            # 0x15 speed
        3,               # 0x17 manufacturer (string 3)
        0,               # 0x18 serial number
        0,               # 0x19 asset tag
        4,               # 0x1A part number (string 4)
        1,               # 0x1B attributes
        0,               # 0x1C extended size
        4800,            # 0x20 configured speed
        1100,            # 0x22 minimum voltage
        1100,            # 0x24 maximum voltage
        1100,            # 0x26 configured voltage
    )
    assert len(body) == 0x28 - 4, len(body)
    return structure(17, handle, body, [locator, "BANK 0", "Maker", "PART-1"])


def end():
    return structure(127, 0xFEFF, b"")


def raw(table, claimed=None):
    length = len(table) if claimed is None else claimed
    return struct.pack("<BBBBI", 0, 3, 4, 0, length) + table


def write(name, data):
    with open(os.path.join(HERE, name), "wb") as f:
        f.write(data)


DIMM, SODIMM, CHIPS = 0x09, 0x0D, 0x0B
DDR4, DDR5, LPDDR5, UNKNOWN = 0x1A, 0x22, 0x23, 0x02

# A desktop: four DIMM slots, two filled with DDR4.
ddr4 = bios() + system() + memory_array(4) + b"".join([
    memory_device(0x1100, 16384, DDR4, DIMM, "DIMM A1"),
    memory_device(0x1101, 0, UNKNOWN, DIMM, "DIMM A2"),
    memory_device(0x1102, 16384, DDR4, DIMM, "DIMM B1"),
    memory_device(0x1103, 0, UNKNOWN, DIMM, "DIMM B2"),
]) + end()
write("ddr4.bin", raw(ddr4))

# A laptop: two SODIMM slots with DDR5.
ddr5 = bios() + system() + memory_array(2) + b"".join([
    memory_device(0x1100, 8192, DDR5, SODIMM, "SODIMM 0"),
    memory_device(0x1101, 8192, DDR5, SODIMM, "SODIMM 1"),
]) + end()
write("ddr5.bin", raw(ddr5))

# A thin laptop: LPDDR5 soldered as rows of chips.
lpddr5 = bios() + system() + memory_array(4) + b"".join(
    memory_device(0x1100 + k, 4096, LPDDR5, CHIPS, "Controller0-ChannelA") for k in range(4)
) + end()
write("lpddr5.bin", raw(lpddr5))

# Damaged: the table is cut inside the first Memory Device, though the
# header claims the whole length.
cut = len(bios() + system() + memory_array(2)) + 20
write("damaged.bin", raw(ddr5[:cut], claimed=len(ddr5)))

# A table without Memory Devices, as some virtual machines give.
write("no-memory.bin", raw(bios() + system() + end()))
