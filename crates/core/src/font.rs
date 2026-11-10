//! The 5 by 7 bitmap font with a 6 pixel advance, uppercase only.
//!
//! The glyph table is ported from `docs/reference/prototype.py`. Each row is
//! five bits, the leftmost pixel in the highest bit.

/// Pixels from one glyph's left edge to the next.
pub const ADVANCE: i32 = 6;
pub const GLYPH_WIDTH: i32 = 5;
pub const GLYPH_HEIGHT: i32 = 7;

/// Drawn after a leaker's label.
pub const UP_ARROW: char = '^';
/// A middle dot.
pub const MIDDLE_DOT: char = '|';

pub type Glyph = [u8; 7];

const GLYPHS: [(char, Glyph); 57] = [
    (
        'A',
        [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
    ),
    (
        'B',
        [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
    ),
    (
        'C',
        [
            0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110,
        ],
    ),
    (
        'D',
        [
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
    ),
    (
        'E',
        [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
    ),
    (
        'F',
        [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
    ),
    (
        'G',
        [
            0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110,
        ],
    ),
    (
        'H',
        [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
    ),
    (
        'I',
        [
            0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
    ),
    (
        'J',
        [
            0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100,
        ],
    ),
    (
        'K',
        [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
    ),
    (
        'L',
        [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
    ),
    (
        'M',
        [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
    ),
    (
        'N',
        [
            0b10001, 0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001,
        ],
    ),
    (
        'O',
        [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
    ),
    (
        'P',
        [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
    ),
    (
        'Q',
        [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ],
    ),
    (
        'R',
        [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
    ),
    (
        'S',
        [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
    ),
    (
        'T',
        [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
    ),
    (
        'U',
        [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
    ),
    (
        'V',
        [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
    ),
    (
        'W',
        [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010,
        ],
    ),
    (
        'X',
        [
            0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
        ],
    ),
    (
        'Y',
        [
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
    ),
    (
        'Z',
        [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
    ),
    (
        '0',
        [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
    ),
    (
        '1',
        [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
    ),
    (
        '2',
        [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
    ),
    (
        '3',
        [
            0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110,
        ],
    ),
    (
        '4',
        [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
    ),
    (
        '5',
        [
            0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
        ],
    ),
    (
        '6',
        [
            0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
    ),
    (
        '7',
        [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
    ),
    (
        '8',
        [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
    ),
    (
        '9',
        [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100,
        ],
    ),
    (
        ' ',
        [
            0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000,
        ],
    ),
    (
        '.',
        [
            0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b01100, 0b01100,
        ],
    ),
    (
        ',',
        [
            0b00000, 0b00000, 0b00000, 0b00000, 0b01100, 0b00100, 0b01000,
        ],
    ),
    (
        ':',
        [
            0b00000, 0b01100, 0b01100, 0b00000, 0b01100, 0b01100, 0b00000,
        ],
    ),
    (
        '-',
        [
            0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000,
        ],
    ),
    (
        '+',
        [
            0b00000, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0b00000,
        ],
    ),
    (
        '/',
        [
            0b00000, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b00000,
        ],
    ),
    (
        '$',
        [
            0b00100, 0b01111, 0b10100, 0b01110, 0b00101, 0b11110, 0b00100,
        ],
    ),
    (
        '%',
        [
            0b11000, 0b11001, 0b00010, 0b00100, 0b01000, 0b10011, 0b00011,
        ],
    ),
    (
        '(',
        [
            0b00010, 0b00100, 0b01000, 0b01000, 0b01000, 0b00100, 0b00010,
        ],
    ),
    (
        ')',
        [
            0b01000, 0b00100, 0b00010, 0b00010, 0b00010, 0b00100, 0b01000,
        ],
    ),
    (
        '!',
        [
            0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00000, 0b00100,
        ],
    ),
    (
        '?',
        [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b00000, 0b00100,
        ],
    ),
    (
        '\'',
        [
            0b01100, 0b00100, 0b01000, 0b00000, 0b00000, 0b00000, 0b00000,
        ],
    ),
    (
        '|',
        [
            0b00000, 0b00000, 0b00000, 0b01100, 0b01100, 0b00000, 0b00000,
        ],
    ), // middle dot
    (
        '=',
        [
            0b00000, 0b00000, 0b11111, 0b00000, 0b11111, 0b00000, 0b00000,
        ],
    ),
    (
        '&',
        [
            0b01100, 0b10010, 0b10100, 0b01000, 0b10101, 0b10010, 0b01101,
        ],
    ),
    (
        '_',
        [
            0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b11111,
        ],
    ),
    (
        '>',
        [
            0b10000, 0b01000, 0b00100, 0b00010, 0b00100, 0b01000, 0b10000,
        ],
    ),
    (
        '^',
        [
            0b00100, 0b01110, 0b10101, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
    ), // up arrow
    (
        '~',
        [
            0b00000, 0b00000, 0b01000, 0b10101, 0b00010, 0b00000, 0b00000,
        ],
    ), // marks an estimate
];

/// The glyph for `c`. Lower case is drawn as upper case; a typographic
/// apostrophe as `'`, `·` as the middle dot and `↑` as the up arrow. Any
/// other character without a glyph is drawn as `?`.
pub fn glyph(c: char) -> &'static Glyph {
    let c = match c {
        '\u{2019}' | '\u{2018}' => '\'',
        '\u{b7}' => MIDDLE_DOT,
        '\u{2191}' => UP_ARROW,
        other => fold(other).to_ascii_uppercase(),
    };
    GLYPHS
        .iter()
        .find(|(k, _)| *k == c)
        .or_else(|| GLYPHS.iter().find(|(k, _)| *k == '?'))
        .map(|(_, g)| g)
        .expect("the font has a ? glyph")
}

/// Latin letters with accents, and the base letter each is drawn as.
/// Covers Latin-1 and Latin Extended-A, so names such as "Správce úloh"
/// draw as SPRAVCE ULOH rather than with question marks.
const FOLDS: [(&str, char); 24] = [
    ("ÀÁÂÃÄÅàáâãäåĀāĂăĄą", 'A'),
    ("ÇçĆćĈĉĊċČč", 'C'),
    ("ĎďĐđÐð", 'D'),
    ("ÈÉÊËèéêëĒēĔĕĖėĘęĚě", 'E'),
    ("ĜĝĞğĠġĢģ", 'G'),
    ("ĤĥĦħ", 'H'),
    ("ÌÍÎÏìíîïĨĩĪīĬĭĮįİı", 'I'),
    ("Ĵĵ", 'J'),
    ("Ķķĸ", 'K'),
    ("ĹĺĻļĽľĿŀŁł", 'L'),
    ("ÑñŃńŅņŇňŉŊŋ", 'N'),
    ("ÒÓÔÕÖØòóôõöøŌōŎŏŐő", 'O'),
    ("ŔŕŖŗŘř", 'R'),
    ("ŚśŜŝŞşŠšß", 'S'),
    ("ŢţŤťŦŧ", 'T'),
    ("ÙÚÛÜùúûüŨũŪūŬŭŮůŰűŲų", 'U'),
    ("Ŵŵ", 'W'),
    ("ÝýÿŶŷŸ", 'Y'),
    ("ŹźŻżŽž", 'Z'),
    ("Ææ", 'A'),
    ("Œœ", 'O'),
    ("Þþ", 'P'),
    ("×", 'X'),
    ("÷", '/'),
];

/// The letter an accented Latin letter is drawn as; anything else as is.
pub fn fold(c: char) -> char {
    if c.is_ascii() {
        return c;
    }
    FOLDS
        .iter()
        .find(|(set, _)| set.contains(c))
        .map_or(c, |&(_, base)| base)
}

/// Width in pixels of `chars` characters at `scale`, from the first glyph's
/// left edge to the last glyph's right edge.
pub fn text_width(chars: usize, scale: i32) -> i32 {
    if chars == 0 {
        0
    } else {
        ((chars as i32 - 1) * ADVANCE + GLYPH_WIDTH) * scale
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_glyph_fits_five_columns() {
        for (c, g) in GLYPHS {
            assert!(g.iter().all(|row| *row < 0b100000), "{c}");
        }
    }

    #[test]
    fn the_table_has_letters_digits_and_the_punctuation_the_app_uses() {
        for c in "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 .,:-+/$%()!?'|=&_>^~".chars() {
            assert!(GLYPHS.iter().any(|(k, _)| *k == c), "missing {c:?}");
        }
        let mut keys: Vec<char> = GLYPHS.iter().map(|(k, _)| *k).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), GLYPHS.len(), "duplicate glyph");
    }

    #[test]
    fn lower_case_and_unknown_characters() {
        assert_eq!(glyph('a'), glyph('A'));
        assert_eq!(glyph('\u{2019}'), glyph('\''));
        assert_eq!(glyph('€'), glyph('?'));
        assert_eq!(glyph('#'), glyph('?'));
    }

    #[test]
    fn accented_letters_draw_as_their_base_letter() {
        // Seen on a Czech Windows: Task Manager is "Správce úloh".
        let drawn: String = "Správce úloh"
            .chars()
            .map(|c| fold(c).to_ascii_uppercase())
            .collect();
        assert_eq!(drawn, "SPRAVCE ULOH");
        assert_eq!(glyph('Á'), glyph('A'));
        assert_eq!(glyph('ů'), glyph('U'));
        assert_eq!(glyph('ž'), glyph('Z'));
        assert_eq!(glyph('ß'), glyph('S'));
        assert_eq!(glyph('ł'), glyph('L'));
        // Outside Latin it is still a question mark.
        assert_eq!(glyph('Ж'), glyph('?'));
        assert_eq!(glyph('中'), glyph('?'));
    }

    #[test]
    fn every_folded_letter_lands_on_a_glyph() {
        for (set, base) in FOLDS {
            assert!(GLYPHS.iter().any(|(k, _)| *k == base), "{base}");
            assert!(set.chars().all(|c| !c.is_ascii()), "{set}");
        }
    }

    #[test]
    fn a_is_ported_exactly() {
        assert_eq!(
            glyph('A'),
            &[
                0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001
            ]
        );
    }

    #[test]
    fn text_width_spans_first_to_last_glyph_edge() {
        assert_eq!(text_width(0, 1), 0);
        assert_eq!(text_width(1, 1), 5);
        assert_eq!(text_width(8, 1), 47);
        assert_eq!(text_width(1, 2), 10);
    }
}
