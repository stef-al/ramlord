//! Turns a draw list into pixels.

use crate::font::{ADVANCE, GLYPH_WIDTH, glyph, text_width};
use crate::palette::Rgb;

/// One drawing instruction. Coordinates may fall outside the canvas; the
/// parts outside are clipped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Draw {
    /// A filled rectangle; both corners are inclusive.
    Rect {
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        color: Rgb,
    },
    /// Text in the 5 by 7 font, upper-cased. With a shadow, each glyph's
    /// shadow is drawn first, one pixel down and to the right.
    Text {
        x: i32,
        y: i32,
        text: String,
        color: Rgb,
        shadow: Option<Rgb>,
        scale: i32,
    },
}

impl Draw {
    pub fn rect(x0: i32, y0: i32, x1: i32, y1: i32, color: Rgb) -> Draw {
        Draw::Rect {
            x0,
            y0,
            x1,
            y1,
            color,
        }
    }

    pub fn dot(x: i32, y: i32, color: Rgb) -> Draw {
        Draw::rect(x, y, x, y, color)
    }

    pub fn text(x: i32, y: i32, text: impl Into<String>, color: Rgb) -> Draw {
        Draw::Text {
            x,
            y,
            text: text.into(),
            color,
            shadow: None,
            scale: 1,
        }
    }

    pub fn shadowed(x: i32, y: i32, text: impl Into<String>, color: Rgb, shadow: Rgb) -> Draw {
        Draw::Text {
            x,
            y,
            text: text.into(),
            color,
            shadow: Some(shadow),
            scale: 1,
        }
    }

    /// Text whose last glyph's right edge lands on `x_right`.
    pub fn text_right(
        x_right: i32,
        y: i32,
        text: impl Into<String>,
        color: Rgb,
        scale: i32,
    ) -> Draw {
        let text = text.into();
        let x = x_right - text_width(text.chars().count(), scale) + 1;
        Draw::Text {
            x,
            y,
            text,
            color,
            shadow: None,
            scale,
        }
    }

    /// Text centred between `x0` and `x1` (inclusive), counting a full
    /// advance per character.
    pub fn text_center(
        x0: i32,
        x1: i32,
        y: i32,
        text: impl Into<String>,
        color: Rgb,
        scale: i32,
    ) -> Draw {
        let text = text.into();
        let room = x1 - x0 + 1 - text.chars().count() as i32 * ADVANCE * scale;
        Draw::Text {
            x: x0 + room.div_euclid(2),
            y,
            text,
            color,
            shadow: None,
            scale,
        }
    }
}

/// A CPU framebuffer of 0x00RRGGBB pixels, top row first. In memory each
/// pixel is the little-endian 32-bit BGRX a top-down Windows DIB expects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u32>,
}

impl Frame {
    pub fn new(width: u32, height: u32, color: Rgb) -> Frame {
        Frame {
            width,
            height,
            pixels: vec![color; width as usize * height as usize],
        }
    }

    pub fn pixel(&self, x: u32, y: u32) -> Rgb {
        self.pixels[(y * self.width + x) as usize]
    }

    /// Fills the rectangle with inclusive corners, clipped to the frame.
    pub fn fill(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: Rgb) {
        let (w, h) = (self.width as i32, self.height as i32);
        let (x0, x1) = (x0.max(0), x1.min(w - 1));
        let (y0, y1) = (y0.max(0), y1.min(h - 1));
        if x0 > x1 || y0 > y1 {
            return;
        }
        for y in y0..=y1 {
            let row = (y * w) as usize;
            self.pixels[row + x0 as usize..=row + x1 as usize].fill(color);
        }
    }

    pub fn text(
        &mut self,
        x: i32,
        y: i32,
        text: &str,
        color: Rgb,
        shadow: Option<Rgb>,
        scale: i32,
    ) {
        let mut gx = x;
        for c in text.chars() {
            let rows = glyph(c);
            for (offset, ink) in [(1, shadow), (0, Some(color))] {
                let Some(ink) = ink else { continue };
                for (r, bits) in rows.iter().enumerate() {
                    for k in 0..GLYPH_WIDTH {
                        if bits >> (GLYPH_WIDTH - 1 - k) & 1 == 1 {
                            let px = gx + k * scale + offset;
                            let py = y + r as i32 * scale + offset;
                            self.fill(px, py, px + scale - 1, py + scale - 1, ink);
                        }
                    }
                }
            }
            gx += ADVANCE * scale;
        }
    }

    pub fn draw(&mut self, item: &Draw) {
        match item {
            Draw::Rect {
                x0,
                y0,
                x1,
                y1,
                color,
            } => self.fill(*x0, *y0, *x1, *y1, *color),
            Draw::Text {
                x,
                y,
                text,
                color,
                shadow,
                scale,
            } => self.text(*x, *y, text, *color, *shadow, *scale),
        }
    }

    /// Nearest-neighbour enlargement by a whole number.
    pub fn scaled(&self, scale: u32) -> Frame {
        let scale = scale.max(1);
        let width = self.width * scale;
        let mut pixels = Vec::with_capacity(self.pixels.len() * (scale * scale) as usize);
        for row in self.pixels.chunks(self.width as usize) {
            let wide: Vec<u32> = row
                .iter()
                .flat_map(|&p| std::iter::repeat_n(p, scale as usize))
                .collect();
            for _ in 0..scale {
                pixels.extend_from_slice(&wide);
            }
        }
        Frame {
            width,
            height: self.height * scale,
            pixels,
        }
    }

    /// Pixels as RGB bytes, top row first: what a PNG encoder takes.
    pub fn to_rgb8(&self) -> Vec<u8> {
        self.pixels
            .iter()
            .flat_map(|&p| [(p >> 16) as u8, (p >> 8) as u8, p as u8])
            .collect()
    }
}

/// Draws `items` in order on a `width` by `height` canvas.
pub fn rasterize(width: u32, height: u32, background: Rgb, items: &[Draw]) -> Frame {
    let mut frame = Frame::new(width, height, background);
    for item in items {
        frame.draw(item);
    }
    frame
}

/// The largest whole scale from 2 to 5 at which a `width` by `height`
/// canvas fits 70% of the work area; 2 when even that does not fit.
pub fn pick_scale(width: u32, height: u32, work_width: u32, work_height: u32) -> u32 {
    (2..=5)
        .rev()
        .find(|&s| {
            u64::from(width * s) * 10 <= u64::from(work_width) * 7
                && u64::from(height * s) * 10 <= u64::from(work_height) * 7
        })
        .unwrap_or(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_clips_to_the_frame() {
        let mut f = Frame::new(4, 3, 0);
        f.fill(-5, 1, 10, 1, 7);
        assert_eq!(f.pixels, [0, 0, 0, 0, 7, 7, 7, 7, 0, 0, 0, 0]);
        f.fill(3, 3, 9, 9, 9);
        f.fill(2, 0, 1, 0, 9);
        assert!(!f.pixels.contains(&9));
    }

    #[test]
    fn text_draws_shadow_then_glyph() {
        let mut f = Frame::new(8, 9, 0);
        f.text(0, 0, "I", 1, Some(2), 1);
        // "I" row 0 is .###. ; its shadow sits one pixel down and right.
        assert_eq!(&f.pixels[0..6], &[0, 1, 1, 1, 0, 0]);
        assert_eq!(f.pixel(4, 1), 2);
        assert_eq!(f.pixel(2, 1), 1);
    }

    #[test]
    fn double_size_text_doubles_each_pixel() {
        let mut f = Frame::new(12, 14, 0);
        f.text(0, 0, ".", 5, None, 2);
        // "." has pixels in columns 1 and 2 of rows 5 and 6.
        assert_eq!(f.pixel(2, 10), 5);
        assert_eq!(f.pixel(5, 13), 5);
        assert_eq!(f.pixel(1, 10), 0);
        assert_eq!(f.pixels.iter().filter(|&&p| p == 5).count(), 16);
    }

    #[test]
    fn text_right_ends_on_the_given_pixel() {
        let mut f = Frame::new(40, 8, 0);
        f.draw(&Draw::text_right(30, 0, "11", 1, 1));
        // "1" row 6 is .###. : its last lit column is 3.
        let last_lit = (0..40).rev().find(|&x| f.pixel(x, 6) == 1).unwrap();
        assert_eq!(last_lit, 30 - 1);
    }

    #[test]
    fn scaled_repeats_pixels() {
        let f = Frame {
            width: 2,
            height: 1,
            pixels: vec![1, 2],
        };
        let s = f.scaled(2);
        assert_eq!((s.width, s.height), (4, 2));
        assert_eq!(s.pixels, [1, 1, 2, 2, 1, 1, 2, 2]);
    }

    #[test]
    fn rgb8_is_red_green_blue() {
        let f = Frame::new(1, 1, 0x123456);
        assert_eq!(f.to_rgb8(), [0x12, 0x34, 0x56]);
    }

    #[test]
    fn scale_fits_seventy_percent_of_the_work_area() {
        assert_eq!(pick_scale(384, 240, 1920, 1040), 3);
        assert_eq!(pick_scale(384, 240, 2560, 1400), 4);
        assert_eq!(pick_scale(384, 240, 3840, 2100), 5);
        assert_eq!(pick_scale(384, 240, 1366, 728), 2);
        assert_eq!(pick_scale(384, 240, 800, 600), 2);
        // 1152 x 720 needs 1646 x 1029 of work area.
        assert_eq!(pick_scale(384, 240, 1646, 1029), 3);
        assert_eq!(pick_scale(384, 240, 1645, 1029), 2);
    }
}
