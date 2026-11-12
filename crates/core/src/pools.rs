//! Kernel pool memory by tag (SPEC.md, After M4: what the owner added).
//! Drivers label each kernel pool allocation with four characters, and
//! Windows counts the bytes under each label: the answer to "which driver
//! holds Staff's kernel pools". `--dump` lists the largest tags and the
//! loaded driver files that contain each one, the way Microsoft suggests
//! finding a tag's owner (searching driver files for it).
//!
//! Pure: these functions read the bytes Windows returns and the bytes of
//! driver files; the Windows layer makes the calls and reads the files.

use crate::snapshot::{PoolDriver, PoolTag};

/// One SYSTEM_POOLTAG on x64: tag, paged allocs and frees, padding, paged
/// bytes, nonpaged allocs and frees, nonpaged bytes.
const TAG_SIZE: usize = 40;
/// SYSTEM_POOLTAG_INFORMATION: a count, then the entries 8-byte aligned.
const TAGS_AT: usize = 8;

/// One RTL_PROCESS_MODULE_INFORMATION on x64; its full path, a NUL
/// terminated ANSI string of 256 bytes, starts at byte 40.
const MODULE_SIZE: usize = 296;
const MODULE_PATH_AT: usize = 40;
const MODULE_PATH_LEN: usize = 256;
/// RTL_PROCESS_MODULES: a count, then the entries 8-byte aligned.
const MODULES_AT: usize = 8;

fn u32_at(raw: &[u8], at: usize) -> u32 {
    let mut b = [0u8; 4];
    b.copy_from_slice(&raw[at..at + 4]);
    u32::from_le_bytes(b)
}

fn u64_at(raw: &[u8], at: usize) -> u64 {
    let mut b = [0u8; 8];
    b.copy_from_slice(&raw[at..at + 8]);
    u64::from_le_bytes(b)
}

/// The count at the start of a list, limited to the entries that fit.
fn entries(raw: &[u8], first: usize, size: usize) -> usize {
    if raw.len() < first {
        return 0;
    }
    let count = u32_at(raw, 0) as usize;
    count.min((raw.len() - first) / size)
}

/// A tag as text: printable ASCII as it is, any other byte as `\xNN`.
pub fn tag_text(tag: [u8; 4]) -> String {
    tag.iter()
        .map(|&b| {
            if (0x20..0x7f).contains(&b) && b != b'\\' {
                char::from(b).to_string()
            } else {
                format!("\\x{b:02x}")
            }
        })
        .collect()
}

/// Every tag in a SYSTEM_POOLTAG_INFORMATION buffer (class 22), with its
/// tag bytes. A buffer cut short yields the entries that fit.
pub fn parse_tags(raw: &[u8]) -> Vec<([u8; 4], PoolTag)> {
    (0..entries(raw, TAGS_AT, TAG_SIZE))
        .map(|k| {
            let at = TAGS_AT + k * TAG_SIZE;
            let mut tag = [0u8; 4];
            tag.copy_from_slice(&raw[at..at + 4]);
            let entry = PoolTag {
                tag: tag_text(tag),
                paged_bytes: u64_at(raw, at + 16),
                nonpaged_bytes: u64_at(raw, at + 32),
                ..PoolTag::default()
            };
            (tag, entry)
        })
        .collect()
}

/// The `n` tags holding the most, paged and nonpaged together, largest
/// first; ties by tag.
pub fn largest(mut tags: Vec<([u8; 4], PoolTag)>, n: usize) -> Vec<([u8; 4], PoolTag)> {
    tags.sort_by(|(a, x), (b, y)| {
        let (x_total, y_total) = (
            x.paged_bytes + x.nonpaged_bytes,
            y.paged_bytes + y.nonpaged_bytes,
        );
        y_total.cmp(&x_total).then_with(|| a.cmp(b))
    });
    tags.truncate(n);
    tags
}

/// The full path of every loaded kernel module in an RTL_PROCESS_MODULES
/// buffer (class 11), in load order: the kernel first, then drivers.
pub fn parse_modules(raw: &[u8]) -> Vec<String> {
    (0..entries(raw, MODULES_AT, MODULE_SIZE))
        .map(|k| {
            let at = MODULES_AT + k * MODULE_SIZE + MODULE_PATH_AT;
            let name = &raw[at..at + MODULE_PATH_LEN];
            let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
            String::from_utf8_lossy(&name[..end]).into_owned()
        })
        .filter(|path| !path.is_empty())
        .collect()
}

/// A module's path as a file a program can open. The kernel names them
/// `\SystemRoot\system32\...`, `\??\C:\...`, `\Windows\...` (on the system
/// drive) or relative to the Windows folder.
pub fn module_file(path: &str, windows_dir: &str) -> String {
    let windows_dir = windows_dir.trim_end_matches('\\');
    let starts = |prefix: &str| {
        path.get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
    };
    if starts("\\SystemRoot\\") {
        format!("{windows_dir}\\{}", &path["\\SystemRoot\\".len()..])
    } else if starts("\\??\\") {
        path["\\??\\".len()..].to_string()
    } else if path.starts_with('\\') {
        let drive = windows_dir.get(..2).unwrap_or("C:");
        format!("{drive}{path}")
    } else {
        format!("{windows_dir}\\{path}")
    }
}

/// A path's last part in lower case, such as `ntoskrnl.exe`.
pub fn file_name(path: &str) -> String {
    crate::snapshot::file_name(path)
}

/// Which of `tags` appear anywhere in `file`'s bytes. Fast for thousands
/// of tags: a table of every tag's first two bytes passes over most of the
/// file, and the rest is a binary search.
pub fn tags_in(file: &[u8], tags: &[[u8; 4]]) -> Vec<bool> {
    let mut sorted: Vec<([u8; 4], usize)> = tags.iter().copied().zip(0..).collect();
    sorted.sort();
    let mut pairs = vec![false; 1 << 16];
    for (tag, _) in &sorted {
        pairs[usize::from(tag[0]) << 8 | usize::from(tag[1])] = true;
    }
    let mut found = vec![false; tags.len()];
    for window in file.windows(4) {
        if !pairs[usize::from(window[0]) << 8 | usize::from(window[1])] {
            continue;
        }
        let key = [window[0], window[1], window[2], window[3]];
        let first = sorted.partition_point(|(tag, _)| *tag < key);
        for (tag, k) in &sorted[first..] {
            if *tag != key {
                break;
            }
            found[*k] = true;
        }
    }
    found
}

/// The most driver files kept per tag; `found_in` counts them all.
pub const DRIVERS_KEPT: usize = 5;

/// Notes in each tag which of `files` (name, bytes) contain it, in order.
pub fn attribute(tags: &mut [([u8; 4], PoolTag)], files: &[(String, Vec<u8>)]) {
    let wanted: Vec<[u8; 4]> = tags.iter().map(|(t, _)| *t).collect();
    for (name, bytes) in files {
        for (k, hit) in tags_in(bytes, &wanted).into_iter().enumerate() {
            if hit {
                let entry = &mut tags[k].1;
                entry.found_in += 1;
                if entry.drivers.len() < DRIVERS_KEPT {
                    entry.drivers.push(name.clone());
                }
            }
        }
    }
}

/// Where a tag found in several loaded files is counted in `by_driver`.
pub const SEVERAL: &str = "(several files)";
/// Where a tag found in no loaded file is counted in `by_driver`.
pub const NONE: &str = "(no loaded file)";

/// Every attributed tag's bytes summed by driver: a tag found in exactly
/// one loaded file counts for that file, the rest for `SEVERAL` or `NONE`.
/// Largest first, ties by name.
pub fn by_driver(tags: &[([u8; 4], PoolTag)]) -> Vec<PoolDriver> {
    let mut drivers: Vec<PoolDriver> = Vec::new();
    for (_, tag) in tags {
        let name = match tag.found_in {
            0 => NONE,
            1 => tag.drivers[0].as_str(),
            _ => SEVERAL,
        };
        let at = match drivers.iter().position(|d| d.driver == name) {
            Some(at) => at,
            None => {
                drivers.push(PoolDriver {
                    driver: name.to_string(),
                    ..PoolDriver::default()
                });
                drivers.len() - 1
            }
        };
        let d = &mut drivers[at];
        d.paged_bytes += tag.paged_bytes;
        d.nonpaged_bytes += tag.nonpaged_bytes;
        d.tags += 1;
    }
    drivers.sort_by(|a, b| {
        (b.paged_bytes + b.nonpaged_bytes)
            .cmp(&(a.paged_bytes + a.nonpaged_bytes))
            .then_with(|| a.driver.cmp(&b.driver))
    });
    drivers
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A SYSTEM_POOLTAG_INFORMATION buffer, as Windows lays it out on x64.
    fn tags_buffer(tags: &[(&[u8; 4], u64, u64)]) -> Vec<u8> {
        let mut raw = (tags.len() as u32).to_le_bytes().to_vec();
        raw.extend_from_slice(&[0; 4]);
        for (tag, paged, nonpaged) in tags {
            raw.extend_from_slice(&tag[..]);
            raw.extend_from_slice(&[1, 0, 0, 0, 0, 0, 0, 0]); // allocs, frees
            raw.extend_from_slice(&[0; 4]); // padding
            raw.extend_from_slice(&paged.to_le_bytes());
            raw.extend_from_slice(&[2, 0, 0, 0, 0, 0, 0, 0]);
            raw.extend_from_slice(&nonpaged.to_le_bytes());
        }
        raw
    }

    fn modules_buffer(paths: &[&str]) -> Vec<u8> {
        let mut raw = (paths.len() as u32).to_le_bytes().to_vec();
        raw.extend_from_slice(&[0; 4]);
        for path in paths {
            let mut entry = vec![0xAAu8; MODULE_PATH_AT];
            let mut name = path.as_bytes().to_vec();
            name.resize(MODULE_PATH_LEN, 0);
            entry.extend_from_slice(&name);
            raw.extend_from_slice(&entry);
        }
        raw
    }

    #[test]
    fn tags_are_read_at_their_x64_offsets() {
        let raw = tags_buffer(&[(b"MmSt", 300, 0), (b"Uxn ", 5, 900), (b"Ntf0", 40, 7)]);
        assert_eq!(raw.len(), 8 + 3 * 40);
        let tags = parse_tags(&raw);
        assert_eq!(tags.len(), 3);
        assert_eq!(tags[1].0, *b"Uxn ");
        assert_eq!(
            (
                tags[1].1.tag.as_str(),
                tags[1].1.paged_bytes,
                tags[1].1.nonpaged_bytes
            ),
            ("Uxn ", 5, 900)
        );
        let top = largest(tags, 2);
        let names: Vec<&str> = top.iter().map(|(_, t)| t.tag.as_str()).collect();
        assert_eq!(names, ["Uxn ", "MmSt"]);
    }

    #[test]
    fn a_short_or_lying_buffer_yields_what_fits() {
        let raw = tags_buffer(&[(b"AAAA", 1, 1), (b"BBBB", 2, 2)]);
        assert_eq!(parse_tags(&raw[..8 + 40 + 39]).len(), 1);
        assert!(parse_tags(&raw[..5]).is_empty());
        let mut lying = raw.clone();
        lying[0] = 200;
        assert_eq!(parse_tags(&lying).len(), 2);
        assert!(parse_tags(&[]).is_empty());
    }

    #[test]
    fn odd_tag_bytes_are_written_out() {
        assert_eq!(tag_text(*b"Ab c"), "Ab c");
        assert_eq!(tag_text([b'I', b'r', b'p', 0]), "Irp\\x00");
        assert_eq!(tag_text([b'a', b'\\', 0xE9, b'z']), "a\\x5c\\xe9z");
    }

    #[test]
    fn modules_become_files() {
        let raw = modules_buffer(&[
            "\\SystemRoot\\system32\\ntoskrnl.exe",
            "\\SystemRoot\\System32\\drivers\\ACPI.sys",
            "\\??\\C:\\Program Files\\Vendor\\guard.sys",
            "",
            "\\Windows\\System32\\drivers\\uxen.sys",
            "System32\\Drivers\\Wof.sys",
        ]);
        let paths = parse_modules(&raw);
        assert_eq!(paths.len(), 5, "the empty one is dropped");
        let files: Vec<String> = paths
            .iter()
            .map(|p| module_file(p, "C:\\WINDOWS"))
            .collect();
        assert_eq!(
            files,
            [
                "C:\\WINDOWS\\system32\\ntoskrnl.exe",
                "C:\\WINDOWS\\System32\\drivers\\ACPI.sys",
                "C:\\Program Files\\Vendor\\guard.sys",
                "C:\\Windows\\System32\\drivers\\uxen.sys",
                "C:\\WINDOWS\\System32\\Drivers\\Wof.sys",
            ]
        );
        assert_eq!(file_name(&files[1]), "acpi.sys");
        assert_eq!(
            parse_modules(&raw[..8 + 296 + 295]).len(),
            1,
            "a cut entry is dropped"
        );
    }

    #[test]
    fn drivers_are_named_by_the_tags_their_files_contain() {
        let raw = tags_buffer(&[(b"Uxn ", 0, 900), (b"MmSt", 300, 0), (b"Zzzz", 1, 0)]);
        let mut tags = largest(parse_tags(&raw), 3);
        let files = vec![
            ("ntoskrnl.exe".to_string(), b"..MmSt..Uxn".to_vec()),
            ("uxen.sys".to_string(), b"\x00Uxn \x00MmSt".to_vec()),
            ("other.sys".to_string(), b"Uxn".to_vec()),
        ];
        attribute(&mut tags, &files);
        fn by(tags: &[([u8; 4], PoolTag)], name: &str) -> PoolTag {
            tags.iter().find(|(_, t)| t.tag == name).unwrap().1.clone()
        }
        assert_eq!(by(&tags, "Uxn ").drivers, ["uxen.sys"]);
        assert_eq!(by(&tags, "MmSt").drivers, ["ntoskrnl.exe", "uxen.sys"]);
        assert_eq!(by(&tags, "MmSt").found_in, 2);
        assert!(by(&tags, "Zzzz").drivers.is_empty());
        // A tag in many files keeps the first few and counts them all.
        let many: Vec<(String, Vec<u8>)> = (0..8)
            .map(|k| (format!("d{k}.sys"), b"Zzzz".to_vec()))
            .collect();
        attribute(&mut tags, &many);
        let z = by(&tags, "Zzzz");
        assert_eq!((z.drivers.len(), z.found_in), (DRIVERS_KEPT, 8));
    }

    #[test]
    fn every_tag_is_summed_by_the_one_driver_that_has_it() {
        let raw = tags_buffer(&[
            (b"#SAC", 0, 87),
            (b"#PAE", 0, 79),
            (b"FMfn", 174, 0),
            (b"File", 0, 66),
            (b"Gone", 3, 4),
        ]);
        let mut tags = parse_tags(&raw);
        let files = vec![
            ("ntoskrnl.exe".to_string(), b"File".to_vec()),
            ("atc.sys".to_string(), b"#SAC #PAE File".to_vec()),
            ("fltmgr.sys".to_string(), b"FMfn".to_vec()),
        ];
        attribute(&mut tags, &files);
        let drivers = by_driver(&tags);
        let rows: Vec<(&str, u64, u64, u32)> = drivers
            .iter()
            .map(|d| (d.driver.as_str(), d.paged_bytes, d.nonpaged_bytes, d.tags))
            .collect();
        assert_eq!(
            rows,
            [
                ("fltmgr.sys", 174, 0, 1),
                ("atc.sys", 0, 166, 2),
                (SEVERAL, 0, 66, 1),
                (NONE, 3, 4, 1),
            ]
        );
    }

    #[test]
    fn thousands_of_tags_are_searched_in_one_pass() {
        let tags: Vec<[u8; 4]> = (0..3000u32)
            .map(|k| {
                [
                    b'A' + (k % 26) as u8,
                    b'a' + (k / 26 % 26) as u8,
                    b'0' + (k / 676) as u8,
                    b' ',
                ]
            })
            .collect();
        let mut file = vec![0u8; 100_000];
        file[500..504].copy_from_slice(&tags[1234]);
        file[99_996..].copy_from_slice(&tags[2999]);
        let found = tags_in(&file, &tags);
        let hits: Vec<usize> = (0..tags.len()).filter(|&k| found[k]).collect();
        assert_eq!(hits, [1234, 2999]);
        // The same tag twice in the list is found twice.
        assert_eq!(tags_in(b"xMmSt", &[*b"MmSt", *b"MmSt"]), [true, true]);
    }

    #[test]
    fn a_tag_at_the_very_end_of_a_file_is_found() {
        assert_eq!(
            tags_in(b"xxxxTagA", &[*b"TagA", *b"xxxx", *b"none"]),
            [true, true, false]
        );
        assert_eq!(tags_in(b"Tag", &[*b"TagA"]), [false]);
    }
}
