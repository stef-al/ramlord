//! The app icon and the exe's resources (SPEC.md, M6). The icon is drawn
//! like everything else, and its pixels are pinned by
//! `fixtures/golden/app-icon.png` (256 pixels). The resource bytes are
//! read back here the way Windows reads them: the .ico directory, the
//! `.res` entries, and the COFF object's resource tree and relocations.
//!
//! With `RAMLORD_PNG_DIR` set, writes `app-icon.png` (256) and
//! `app-icon-16.png` and `app-icon-32.png` there, enlarged to look at.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use ramlord_core::palette::TRAY_CLEAR;
use ramlord_core::render_app_icon;
use ramlord_core::resources::{
    self, ICON_ID, IconImage, LANGUAGE, RT_GROUP_ICON, RT_ICON, RT_VERSION, Resource, Version,
};
use ramlord_core::scene::APP_ICON_SIZES;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

fn png_rgba(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(rgba)
        .unwrap();
    out
}

/// The images as the build script makes them: DIBs up to 64, a PNG at 256.
fn images() -> Vec<IconImage> {
    APP_ICON_SIZES
        .iter()
        .map(|&size| {
            let frame = render_app_icon(size).unwrap();
            if size >= 256 {
                IconImage::Png {
                    size,
                    bytes: png_rgba(size, size, &resources::rgba(&frame)),
                }
            } else {
                IconImage::Pixels(frame)
            }
        })
        .collect()
}

fn save(name: &str, size: u32, scale: u32) {
    let Some(dir) = std::env::var_os("RAMLORD_PNG_DIR") else {
        return;
    };
    let frame = render_app_icon(size).unwrap().scaled(scale);
    let bytes = png_rgba(frame.width, frame.height, &resources::rgba(&frame));
    std::fs::write(Path::new(&dir).join(name), bytes).unwrap();
}

#[test]
fn the_icon_comes_in_its_sizes_with_square_pixels() {
    for size in APP_ICON_SIZES {
        let frame = render_app_icon(size).unwrap();
        assert_eq!((frame.width, frame.height), (size, size));
        // Transparent around the tower: the corners.
        for (x, y) in [(0, size - 1), (size - 1, size - 1)] {
            assert_eq!(frame.pixel(x, y), TRAY_CLEAR, "{size}: ({x}, {y})");
        }
        // Whole-number enlargements of the 16 or 32 pixel drawing.
        let art = if size % 32 == 0 { 32 } else { 16 };
        assert_eq!(
            frame,
            render_app_icon(art).unwrap().scaled(size / art),
            "{size}"
        );
    }
    assert_eq!(render_app_icon(24), None);
    save("app-icon.png", 256, 1);
    save("app-icon-16.png", 16, 8);
    save("app-icon-32.png", 32, 8);
}

#[test]
fn the_icon_matches_golden_pixel_for_pixel() {
    let path = repo().join("fixtures/golden/app-icon.png");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba);
    let frame = render_app_icon(256).unwrap();
    assert_eq!((info.width, info.height), (256, 256));
    assert!(
        buf[..info.buffer_size()] == resources::rgba(&frame)[..],
        "app-icon.png differs from the drawing"
    );
}

#[test]
fn the_ico_file_lists_each_image_with_its_bytes() {
    let images = images();
    let ico = resources::ico_file(&images);
    assert_eq!(
        (u16_at(&ico, 0), u16_at(&ico, 2), u16_at(&ico, 4)),
        (0, 1, 5)
    );
    for (k, size) in APP_ICON_SIZES.iter().enumerate() {
        let e = 6 + 16 * k;
        let byte = if *size == 256 { 0 } else { *size as u8 };
        assert_eq!((ico[e], ico[e + 1]), (byte, byte));
        assert_eq!((u16_at(&ico, e + 4), u16_at(&ico, e + 6)), (1, 32));
        let (len, at) = (u32_at(&ico, e + 8) as usize, u32_at(&ico, e + 12) as usize);
        let data = &ico[at..at + len];
        if *size == 256 {
            assert_eq!(&data[1..4], b"PNG");
        } else {
            // A 32-bit DIB twice as tall, for the mask.
            assert_eq!(u32_at(data, 0), 40);
            assert_eq!((u32_at(data, 4), u32_at(data, 8)), (*size, 2 * size));
            assert_eq!(u16_at(data, 14), 32);
            let row = (*size as usize).div_ceil(32) * 4;
            assert_eq!(len, 40 + (size * size * 4) as usize + row * *size as usize);
            // The bottom-left pixel is transparent: alpha 0 in the first
            // row of colour (bottom-up), and set in the mask.
            assert_eq!(data[40 + 3], 0);
            let mask = 40 + (size * size * 4) as usize;
            assert_eq!(data[mask] & 0x80, 0x80);
        }
    }
}

/// Reads one VS_VERSIONINFO block: (key, value bytes, children).
fn version_block(b: &[u8]) -> (String, Vec<u8>, Vec<&[u8]>) {
    let length = u16_at(b, 0) as usize;
    let (value_length, kind) = (u16_at(b, 2) as usize, u16_at(b, 4));
    let mut at = 6;
    let mut key = Vec::new();
    while u16_at(b, at) != 0 {
        key.push(u16_at(b, at));
        at += 2;
    }
    at = (at + 2).next_multiple_of(4);
    let value_bytes = if kind == 1 {
        2 * value_length
    } else {
        value_length
    };
    let value = b[at..at + value_bytes].to_vec();
    at += value_bytes;
    let mut children = Vec::new();
    loop {
        at = at.next_multiple_of(4);
        if at >= length {
            break;
        }
        let child = u16_at(b, at) as usize;
        children.push(&b[at..at + child]);
        at += child;
    }
    (String::from_utf16(&key).unwrap(), value, children)
}

fn text(value: &[u8]) -> String {
    let units: Vec<u16> = value
        .chunks(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&u| u != 0)
        .collect();
    String::from_utf16(&units).unwrap()
}

#[test]
fn the_version_block_says_what_explorer_shows() {
    let info = resources::version_info(&Version::ramlord(0, 1, 0));
    assert_eq!(u16_at(&info, 0) as usize, info.len());
    let (key, fixed, children) = version_block(&info);
    assert_eq!(key, "VS_VERSION_INFO");
    assert_eq!(fixed.len(), 52);
    assert_eq!(u32_at(&fixed, 0), 0xFEEF_04BD);
    // File and product version 0.1.0.0.
    assert_eq!(
        (u32_at(&fixed, 8), u32_at(&fixed, 12)),
        (0x0000_0001, 0x0000_0000)
    );
    assert_eq!(
        (u32_at(&fixed, 16), u32_at(&fixed, 20)),
        (0x0000_0001, 0x0000_0000)
    );
    assert_eq!(u32_at(&fixed, 36), 1, "an application");
    assert_eq!(children.len(), 2);
    let (key, _, tables) = version_block(children[0]);
    assert_eq!((key.as_str(), tables.len()), ("StringFileInfo", 1));
    let (key, _, strings) = version_block(tables[0]);
    assert_eq!(key, "040904B0");
    let strings: Vec<(String, String)> = strings
        .iter()
        .map(|s| {
            let (k, v, _) = version_block(s);
            (k, text(&v))
        })
        .collect();
    for wanted in [
        ("ProductName", "RAMlord"),
        ("FileDescription", "RAMlord"),
        ("ProductVersion", "0.1.0"),
        ("FileVersion", "0.1.0"),
        ("OriginalFilename", "ramlord.exe"),
        ("LegalCopyright", "GPL-3.0-only"),
    ] {
        assert!(
            strings.contains(&(wanted.0.to_string(), wanted.1.to_string())),
            "{wanted:?} in {strings:?}"
        );
    }
    let (key, _, vars) = version_block(children[1]);
    assert_eq!(key, "VarFileInfo");
    let (key, translation, _) = version_block(vars[0]);
    assert_eq!(key, "Translation");
    assert_eq!(translation, [0x09, 0x04, 0xB0, 0x04]);
}

fn all() -> Vec<Resource> {
    resources::resources(&images(), &Version::ramlord(0, 1, 0))
}

#[test]
fn the_res_file_holds_every_resource_after_an_empty_first_entry() {
    let all = all();
    let res = resources::res_file(&all);
    assert_eq!(&res[..8], &[0, 0, 0, 0, 32, 0, 0, 0]);
    let mut at = 32;
    let mut found = Vec::new();
    while at < res.len() {
        let (size, header) = (u32_at(&res, at) as usize, u32_at(&res, at + 4) as usize);
        assert_eq!(header, 32);
        assert_eq!(
            (u16_at(&res, at + 8), u16_at(&res, at + 12)),
            (0xFFFF, 0xFFFF)
        );
        let (kind, id) = (u16_at(&res, at + 10), u16_at(&res, at + 14));
        assert_eq!(u16_at(&res, at + 22), LANGUAGE);
        found.push((kind, id, res[at + header..at + header + size].to_vec()));
        at = (at + header + size).next_multiple_of(4);
    }
    assert_eq!(at, res.len());
    let expected: Vec<_> = all.iter().map(|r| (r.kind, r.id, r.data.clone())).collect();
    assert_eq!(found, expected);
}

/// Follows type, id and language from the start of `section` to a data
/// entry; returns where the entry is.
fn find(section: &[u8], kind: u16, id: u16) -> Option<usize> {
    let pick = |dir: usize, name: u32| -> Option<u32> {
        assert_eq!(u16_at(section, dir + 12), 0, "no named entries");
        let n = u16_at(section, dir + 14) as usize;
        let names: Vec<u32> = (0..n).map(|k| u32_at(section, dir + 16 + 8 * k)).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted, "entries sorted by id");
        (0..n)
            .find(|&k| names[k] == name)
            .map(|k| u32_at(section, dir + 20 + 8 * k))
    };
    let ids = pick(0, u32::from(kind))?;
    assert!(ids & 0x8000_0000 != 0);
    let languages = pick((ids & 0x7FFF_FFFF) as usize, u32::from(id))?;
    assert!(languages & 0x8000_0000 != 0);
    let entry = pick((languages & 0x7FFF_FFFF) as usize, u32::from(LANGUAGE))?;
    assert!(entry & 0x8000_0000 == 0, "a data entry, not a directory");
    Some(entry as usize)
}

#[test]
fn the_coff_object_has_one_rsrc_section_with_relocated_data_entries() {
    let all = all();
    let object = resources::coff_object(&all);
    assert_eq!(u16_at(&object, 0), 0x8664, "x86-64");
    assert_eq!(u16_at(&object, 2), 1, "one section");
    assert_eq!(u16_at(&object, 16), 0, "no optional header");
    let header = 20;
    assert_eq!(&object[header..header + 8], b".rsrc\0\0\0");
    let size = u32_at(&object, header + 16) as usize;
    let raw = u32_at(&object, header + 20) as usize;
    let relocations_at = u32_at(&object, header + 24) as usize;
    let relocations = u16_at(&object, header + 32) as usize;
    assert_eq!(relocations, all.len());
    let section = &object[raw..raw + size];

    // Every resource is found by type, id and language, and its data entry
    // points at its bytes and is relocated against the section symbol.
    let relocated: Vec<u32> = (0..relocations)
        .map(|k| {
            let r = relocations_at + 10 * k;
            assert_eq!(u32_at(&object, r + 4), 0, "the .rsrc symbol");
            assert_eq!(u16_at(&object, r + 8), 3, "IMAGE_REL_AMD64_ADDR32NB");
            u32_at(&object, r)
        })
        .collect();
    for r in &all {
        let entry = find(section, r.kind, r.id).unwrap_or_else(|| panic!("{} {}", r.kind, r.id));
        assert!(relocated.contains(&(entry as u32)));
        let (at, len) = (
            u32_at(section, entry) as usize,
            u32_at(section, entry + 4) as usize,
        );
        assert_eq!(at % 8, 0);
        assert_eq!(&section[at..at + len], &r.data[..]);
    }
    assert!(find(section, RT_GROUP_ICON, ICON_ID).is_some());
    assert!(find(section, RT_VERSION, 1).is_some());
    assert!(find(section, RT_ICON, 6).is_none());

    // The symbol table: the section's static symbol and its aux record,
    // then an empty string table.
    let symbols = u32_at(&object, 8) as usize;
    assert_eq!(u32_at(&object, 12), 2);
    assert_eq!(&object[symbols..symbols + 8], b".rsrc\0\0\0");
    assert_eq!(u16_at(&object, symbols + 12), 1, "section 1");
    assert_eq!(object[symbols + 16], 3, "static");
    assert_eq!(u32_at(&object, symbols + 18) as usize, size);
    assert_eq!(object.len(), symbols + 36 + 4);
    assert_eq!(u32_at(&object, symbols + 36), 4);
}

#[test]
fn the_icon_group_names_each_image_by_its_id() {
    let all = all();
    let group = all
        .iter()
        .find(|r| r.kind == RT_GROUP_ICON)
        .expect("a group");
    assert_eq!(u16_at(&group.data, 4), APP_ICON_SIZES.len() as u16);
    for (k, size) in APP_ICON_SIZES.iter().enumerate() {
        let e = 6 + 14 * k;
        let icon = all
            .iter()
            .find(|r| r.kind == RT_ICON && r.id == u16_at(&group.data, e + 12))
            .expect("its image");
        assert_eq!(u32_at(&group.data, e + 8) as usize, icon.data.len());
        let byte = if *size == 256 { 0 } else { *size as u8 };
        assert_eq!(group.data[e], byte);
    }
    assert_eq!(group.data.len(), 6 + 14 * APP_ICON_SIZES.len());
}
