//! The exe's resources (SPEC.md, M6): the app icon and a version block, as
//! bytes a linker takes. Pure: the app's build script calls it and hands
//! the result to the linker. MSVC's `link.exe` takes a `.res` file
//! (`res_file`); MinGW's `ld` takes a COFF object with a `.rsrc` section
//! (`coff_object`), the same thing `windres` writes.
//!
//! Every resource is in US English (0x0409), the language of the app.

use crate::palette::TRAY_CLEAR;
use crate::render::Frame;

/// RT_ICON, RT_GROUP_ICON and RT_VERSION.
pub const RT_ICON: u16 = 3;
pub const RT_GROUP_ICON: u16 = 14;
pub const RT_VERSION: u16 = 16;

/// US English.
pub const LANGUAGE: u16 = 0x0409;

/// The icon group's id: `LoadIconW(instance, MAKEINTRESOURCE(1))`, and the
/// one Explorer shows for the exe.
pub const ICON_ID: u16 = 1;

/// One image of the icon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IconImage {
    /// Stored as a 32-bit DIB with its mask; `TRAY_CLEAR` is transparent.
    Pixels(Frame),
    /// An already encoded PNG of a square `size`, for 256 pixels, where a
    /// DIB would take 260 KB.
    Png { size: u32, bytes: Vec<u8> },
}

impl IconImage {
    fn size(&self) -> u32 {
        match self {
            IconImage::Pixels(frame) => frame.width,
            IconImage::Png { size, .. } => *size,
        }
    }

    /// What an .ico file or an RT_ICON resource holds for this image.
    fn data(&self) -> Vec<u8> {
        match self {
            IconImage::Pixels(frame) => dib(frame),
            IconImage::Png { bytes, .. } => bytes.clone(),
        }
    }
}

/// RGBA bytes, top row first, with `TRAY_CLEAR` fully transparent: what a
/// PNG encoder takes.
pub fn rgba(frame: &Frame) -> Vec<u8> {
    frame
        .pixels
        .iter()
        .flat_map(|&p| {
            let alpha = if p == TRAY_CLEAR { 0 } else { 255 };
            [(p >> 16) as u8, (p >> 8) as u8, p as u8, alpha]
        })
        .collect()
}

/// A 32-bit icon DIB: the header (with the height doubled, for the mask),
/// BGRA rows bottom-up, then the 1-bit AND mask, rows padded to 4 bytes.
fn dib(frame: &Frame) -> Vec<u8> {
    let (w, h) = (frame.width, frame.height);
    let mask_row = (w as usize).div_ceil(32) * 4;
    let mut out = Vec::new();
    for value in [40, w, 2 * h] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.extend_from_slice(&1u16.to_le_bytes()); // planes
    out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
    out.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    let image_size = w * h * 4 + mask_row as u32 * h;
    out.extend_from_slice(&image_size.to_le_bytes());
    out.extend_from_slice(&[0; 16]); // resolution and palette: unused
    for y in (0..h).rev() {
        for x in 0..w {
            let p = frame.pixel(x, y);
            let alpha = if p == TRAY_CLEAR { 0 } else { 255 };
            let color = if alpha == 0 { 0 } else { p };
            out.extend_from_slice(&[color as u8, (color >> 8) as u8, (color >> 16) as u8, alpha]);
        }
    }
    for y in (0..h).rev() {
        let mut row = vec![0u8; mask_row];
        for x in 0..w {
            if frame.pixel(x, y) == TRAY_CLEAR {
                row[x as usize / 8] |= 0x80 >> (x % 8);
            }
        }
        out.extend_from_slice(&row);
    }
    out
}

/// The width or height byte of an icon directory entry: 0 means 256.
fn size_byte(size: u32) -> u8 {
    if size >= 256 { 0 } else { size as u8 }
}

/// An .ico file of `images`.
pub fn ico_file(images: &[IconImage]) -> Vec<u8> {
    let data: Vec<Vec<u8>> = images.iter().map(IconImage::data).collect();
    let mut out = Vec::new();
    for value in [0u16, 1, images.len() as u16] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    let mut offset = 6 + 16 * images.len() as u32;
    for (image, bytes) in images.iter().zip(&data) {
        let size = size_byte(image.size());
        out.extend_from_slice(&[size, size, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += bytes.len() as u32;
    }
    for bytes in &data {
        out.extend_from_slice(bytes);
    }
    out
}

/// The RT_GROUP_ICON data: an .ico directory whose entries name RT_ICON
/// ids, 1 up, instead of file offsets.
fn icon_group(images: &[IconImage]) -> Vec<u8> {
    let mut out = Vec::new();
    for value in [0u16, 1, images.len() as u16] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    for (k, image) in images.iter().enumerate() {
        let size = size_byte(image.size());
        out.extend_from_slice(&[size, size, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(image.data().len() as u32).to_le_bytes());
        out.extend_from_slice(&(k as u16 + 1).to_le_bytes());
    }
    out
}

/// The program's version and the strings Explorer's Details tab shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
    /// Name, value: FileDescription, ProductName and the like.
    pub strings: Vec<(String, String)>,
}

impl Version {
    /// RAMlord's block for `major.minor.patch`.
    pub fn ramlord(major: u16, minor: u16, patch: u16) -> Version {
        let text = format!("{major}.{minor}.{patch}");
        let strings = [
            ("CompanyName", "RAMlord"),
            ("FileDescription", "RAMlord"),
            ("FileVersion", &text),
            ("InternalName", "ramlord"),
            ("LegalCopyright", "GPL-3.0-only"),
            ("OriginalFilename", "ramlord.exe"),
            ("ProductName", "RAMlord"),
            ("ProductVersion", &text),
            ("Comments", "https://github.com/stef-al/ramlord"),
        ];
        Version {
            major,
            minor,
            patch,
            strings: strings
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }
}

fn utf16z(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect()
}

fn pad4(out: &mut Vec<u8>) {
    while out.len() % 4 != 0 {
        out.push(0);
    }
}

/// One block of a VS_VERSIONINFO tree: length, value length, type, key,
/// value and children, each 4-byte aligned. A text value's length counts
/// UTF-16 units, a binary one's bytes.
fn version_block(key: &str, value: VersionValue, children: &[Vec<u8>]) -> Vec<u8> {
    let (bytes, value_length, kind) = match value {
        VersionValue::None => (Vec::new(), 0, 1),
        VersionValue::Binary(bytes) => {
            let n = bytes.len() as u16;
            (bytes, n, 0)
        }
        VersionValue::Text(text) => {
            let bytes = utf16z(text);
            let n = (bytes.len() / 2) as u16;
            (bytes, n, 1)
        }
    };
    let mut out = vec![0, 0];
    out.extend_from_slice(&value_length.to_le_bytes());
    out.extend_from_slice(&(kind as u16).to_le_bytes());
    out.extend_from_slice(&utf16z(key));
    pad4(&mut out);
    out.extend_from_slice(&bytes);
    for child in children {
        pad4(&mut out);
        out.extend_from_slice(child);
    }
    let length = out.len() as u16;
    out[..2].copy_from_slice(&length.to_le_bytes());
    out
}

enum VersionValue<'a> {
    None,
    Binary(Vec<u8>),
    Text(&'a str),
}

/// The RT_VERSION data: VS_VERSIONINFO with a VS_FIXEDFILEINFO, the
/// strings in US English (code page 1200, UTF-16) and the translation.
pub fn version_info(version: &Version) -> Vec<u8> {
    let ms = (u32::from(version.major) << 16) | u32::from(version.minor);
    let ls = u32::from(version.patch) << 16;
    let mut fixed = Vec::new();
    for value in [
        0xFEEF_04BD, // signature
        0x0001_0000, // structure version
        ms,
        ls, // file version
        ms,
        ls,          // product version
        0x3F,        // flags mask
        0,           // flags
        0x0004_0004, // VOS_NT_WINDOWS32
        1,           // VFT_APP
        0,           // subtype
        0,
        0, // date
    ] {
        fixed.extend_from_slice(&u32::to_le_bytes(value));
    }
    let strings: Vec<Vec<u8>> = version
        .strings
        .iter()
        .map(|(k, v)| version_block(k, VersionValue::Text(v), &[]))
        .collect();
    let table = version_block("040904B0", VersionValue::None, &strings);
    let string_info = version_block("StringFileInfo", VersionValue::None, &[table]);
    let mut translation = LANGUAGE.to_le_bytes().to_vec();
    translation.extend_from_slice(&1200u16.to_le_bytes());
    let var = version_block("Translation", VersionValue::Binary(translation), &[]);
    let var_info = version_block("VarFileInfo", VersionValue::None, &[var]);
    version_block(
        "VS_VERSION_INFO",
        VersionValue::Binary(fixed),
        &[string_info, var_info],
    )
}

/// One resource: type, id and data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resource {
    pub kind: u16,
    pub id: u16,
    pub data: Vec<u8>,
}

/// The icon's images as RT_ICON resources, its group and the version
/// block, sorted by type and id as the resource directory needs them.
pub fn resources(images: &[IconImage], version: &Version) -> Vec<Resource> {
    let mut out: Vec<Resource> = images
        .iter()
        .enumerate()
        .map(|(k, image)| Resource {
            kind: RT_ICON,
            id: k as u16 + 1,
            data: image.data(),
        })
        .collect();
    out.push(Resource {
        kind: RT_GROUP_ICON,
        id: ICON_ID,
        data: icon_group(images),
    });
    out.push(Resource {
        kind: RT_VERSION,
        id: 1,
        data: version_info(version),
    });
    out.sort_by_key(|r| (r.kind, r.id));
    out
}

/// A `.res` file: an empty first entry that marks a 32-bit file, then each
/// resource's header (sizes, type and id as ordinals, flags, language) and
/// its data, 4-byte aligned.
pub fn res_file(resources: &[Resource]) -> Vec<u8> {
    let mut out = Vec::new();
    let entry = |out: &mut Vec<u8>, kind: u16, id: u16, flags: u16, language: u16, data: &[u8]| {
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&32u32.to_le_bytes());
        for value in [0xFFFF, kind, 0xFFFF, id] {
            out.extend_from_slice(&u16::to_le_bytes(value));
        }
        out.extend_from_slice(&0u32.to_le_bytes()); // data version
        out.extend_from_slice(&flags.to_le_bytes());
        out.extend_from_slice(&language.to_le_bytes());
        out.extend_from_slice(&[0; 8]); // version, characteristics
        out.extend_from_slice(data);
        pad4(out);
    };
    entry(&mut out, 0, 0, 0, 0, &[]);
    for r in resources {
        // MOVEABLE | PURE, and DISCARDABLE for icons: what rc.exe writes.
        let flags = if r.kind == RT_VERSION { 0x0030 } else { 0x1030 };
        entry(&mut out, r.kind, r.id, flags, LANGUAGE, &r.data);
    }
    out
}

/// The resource directory, as it sits at the start of a `.rsrc` section:
/// types, then ids, then languages, then data entries, then the data,
/// 8-byte aligned. Returns the bytes and where each data entry's address
/// lies, which the linker must turn into an RVA.
pub fn rsrc_section(resources: &[Resource]) -> (Vec<u8>, Vec<u32>) {
    let mut kinds: Vec<u16> = resources.iter().map(|r| r.kind).collect();
    kinds.dedup();
    let dir = |entries: usize| 16 + 8 * entries as u32;
    let mut at = dir(kinds.len());
    let ids_at: Vec<u32> = kinds
        .iter()
        .map(|&k| {
            let here = at;
            at += dir(resources.iter().filter(|r| r.kind == k).count());
            here
        })
        .collect();
    let languages_at: Vec<u32> = resources
        .iter()
        .map(|_| {
            let here = at;
            at += dir(1);
            here
        })
        .collect();
    let entries_at: Vec<u32> = resources
        .iter()
        .map(|_| {
            let here = at;
            at += 16;
            here
        })
        .collect();
    let data_at: Vec<u32> = resources
        .iter()
        .map(|r| {
            at = at.next_multiple_of(8);
            let here = at;
            at += r.data.len() as u32;
            here
        })
        .collect();

    let mut out = Vec::new();
    let directory = |out: &mut Vec<u8>, entries: &[(u32, u32)]| {
        out.extend_from_slice(&[0; 12]); // characteristics, time, version
        out.extend_from_slice(&0u16.to_le_bytes()); // named entries
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        for &(name, offset) in entries {
            out.extend_from_slice(&name.to_le_bytes());
            out.extend_from_slice(&offset.to_le_bytes());
        }
    };
    const SUBDIRECTORY: u32 = 0x8000_0000;
    let types: Vec<(u32, u32)> = kinds
        .iter()
        .zip(&ids_at)
        .map(|(&k, &a)| (u32::from(k), a | SUBDIRECTORY))
        .collect();
    directory(&mut out, &types);
    for &k in &kinds {
        let ids: Vec<(u32, u32)> = resources
            .iter()
            .zip(&languages_at)
            .filter(|(r, _)| r.kind == k)
            .map(|(r, &a)| (u32::from(r.id), a | SUBDIRECTORY))
            .collect();
        directory(&mut out, &ids);
    }
    for &entry in &entries_at {
        directory(&mut out, &[(u32::from(LANGUAGE), entry)]);
    }
    for (r, &data) in resources.iter().zip(&data_at) {
        out.extend_from_slice(&data.to_le_bytes()); // an RVA once linked
        out.extend_from_slice(&(r.data.len() as u32).to_le_bytes());
        out.extend_from_slice(&[0; 8]); // code page, reserved
    }
    for (r, &data) in resources.iter().zip(&data_at) {
        out.resize(data as usize, 0);
        out.extend_from_slice(&r.data);
    }
    pad4(&mut out);
    (out, entries_at)
}

/// An x86-64 COFF object with the resources in one `.rsrc` section, as
/// `windres` writes it: each data entry's address is relocated to an RVA
/// (IMAGE_REL_AMD64_ADDR32NB) against the section's symbol.
pub fn coff_object(resources: &[Resource]) -> Vec<u8> {
    const AMD64: u16 = 0x8664;
    const ADDR32NB: u16 = 3;
    const HEADERS: u32 = 20 + 40;
    let (section, relocations) = rsrc_section(resources);
    let relocations_at = HEADERS + section.len() as u32;
    let symbols_at = relocations_at + 10 * relocations.len() as u32;

    let mut out = Vec::new();
    out.extend_from_slice(&AMD64.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // sections
    out.extend_from_slice(&0u32.to_le_bytes()); // time: reproducible
    out.extend_from_slice(&symbols_at.to_le_bytes());
    out.extend_from_slice(&2u32.to_le_bytes()); // the symbol and its aux
    out.extend_from_slice(&[0; 4]); // optional header size, flags

    out.extend_from_slice(b".rsrc\0\0\0");
    out.extend_from_slice(&[0; 8]); // virtual size and address
    out.extend_from_slice(&(section.len() as u32).to_le_bytes());
    out.extend_from_slice(&HEADERS.to_le_bytes());
    out.extend_from_slice(&relocations_at.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // line numbers
    out.extend_from_slice(&(relocations.len() as u16).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    // Initialized data, readable.
    out.extend_from_slice(&0x4000_0040u32.to_le_bytes());

    out.extend_from_slice(&section);
    for &at in &relocations {
        out.extend_from_slice(&at.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // symbol 0: .rsrc
        out.extend_from_slice(&ADDR32NB.to_le_bytes());
    }

    // The section's symbol, static, with an aux record of its size.
    out.extend_from_slice(b".rsrc\0\0\0");
    out.extend_from_slice(&0u32.to_le_bytes()); // value
    out.extend_from_slice(&1u16.to_le_bytes()); // section number
    out.extend_from_slice(&0u16.to_le_bytes()); // type
    out.extend_from_slice(&[3, 1]); // IMAGE_SYM_CLASS_STATIC, one aux
    out.extend_from_slice(&(section.len() as u32).to_le_bytes());
    out.extend_from_slice(&(relocations.len() as u16).to_le_bytes());
    out.extend_from_slice(&[0; 12]); // line numbers, checksum, number, selection
    // An empty string table: its size field only.
    out.extend_from_slice(&4u32.to_le_bytes());
    out
}
