//! Writing frames as PNG files.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use ramlord_core::Frame;

/// Encodes `frame` as an 8-bit RGB PNG.
pub fn encode_png(frame: &Frame, out: impl std::io::Write) -> Result<(), png::EncodingError> {
    let mut encoder = png::Encoder::new(out, frame.width, frame.height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&frame.to_rgb8())?;
    writer.finish()
}

/// Writes `frame` to `path`, enlarged by `scale`.
pub fn write_png(path: &Path, frame: &Frame, scale: u32) -> Result<(), String> {
    let file = File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut out = BufWriter::new(file);
    encode_png(&frame.scaled(scale), &mut out).map_err(|e| format!("{}: {e}", path.display()))?;
    std::io::Write::flush(&mut out).map_err(|e| format!("{}: {e}", path.display()))
}

/// `frame` as a device-independent bitmap for the clipboard (`CF_DIB`): a
/// 40-byte BITMAPINFOHEADER, then 24-bit rows in blue, green, red order,
/// the bottom row first, each padded to four bytes.
///
/// 24 bits on purpose. A 32-bit bitmap has a spare byte per pixel, which
/// some programs read as transparency; ours would be 0 everywhere and the
/// pasted card would be invisible there.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn clipboard_dib(frame: &Frame) -> Vec<u8> {
    let (width, height) = (frame.width as usize, frame.height as usize);
    let row = (width * 3).next_multiple_of(4);
    let mut out = Vec::with_capacity(40 + row * height);
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(frame.width as i32).to_le_bytes());
    // Positive: bottom-up, as a bitmap on the clipboard is expected to be.
    out.extend_from_slice(&(frame.height as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // planes
    out.extend_from_slice(&24u16.to_le_bytes()); // bits per pixel
    out.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    out.extend_from_slice(&((row * height) as u32).to_le_bytes());
    out.extend_from_slice(&[0; 16]); // resolution and palette: unused
    for y in (0..height).rev() {
        let start = out.len();
        for &pixel in &frame.pixels[y * width..(y + 1) * width] {
            out.extend_from_slice(&[pixel as u8, (pixel >> 8) as u8, (pixel >> 16) as u8]);
        }
        out.resize(start + row, 0);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clipboard_bitmap_is_24_bit_bottom_up_with_padded_rows() {
        // 3 by 2: the top row red, green, blue; the bottom row all white.
        let frame = Frame {
            width: 3,
            height: 2,
            pixels: vec![0xff0000, 0x00ff00, 0x0000ff, 0xffffff, 0xffffff, 0xffffff],
        };
        let dib = clipboard_dib(&frame);
        let u32_at = |at: usize| u32::from_le_bytes(dib[at..at + 4].try_into().unwrap());
        assert_eq!(u32_at(0), 40, "header size");
        assert_eq!((u32_at(4), u32_at(8)), (3, 2), "width, height (bottom-up)");
        assert_eq!(&dib[12..16], &[1, 0, 24, 0], "one plane, 24 bits");
        assert_eq!(u32_at(16), 0, "uncompressed");
        // Rows of 9 bytes are padded to 12.
        assert_eq!(u32_at(20), 24);
        assert_eq!(dib.len(), 40 + 24);
        // The bottom row comes first: white, then padding.
        assert_eq!(
            &dib[40..52],
            &[255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0]
        );
        // Then the top row, each pixel as blue, green, red.
        assert_eq!(&dib[52..64], &[0, 0, 255, 0, 255, 0, 255, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn the_card_at_its_saved_size_is_a_few_megabytes() {
        let frame = Frame::new(960, 1200, 0x123456);
        let dib = clipboard_dib(&frame);
        assert_eq!(dib.len(), 40 + 960 * 3 * 1200);
        assert_eq!(&dib[40..43], &[0x56, 0x34, 0x12]);
    }

    #[test]
    fn a_png_is_written_at_the_scale_asked() {
        let mut bytes = Vec::new();
        encode_png(&Frame::new(2, 3, 0xff8000).scaled(2), &mut bytes).unwrap();
        let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
            .read_info()
            .unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!((info.width, info.height), (4, 6));
        assert_eq!(&buf[..3], &[0xff, 0x80, 0x00]);
    }
}
