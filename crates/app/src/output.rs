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
