//! Picking the block glyph whose shape best matches a cell's coverage.
//!
//! The shader does the matching (`render.wgsl`); this builds the table.
//! A cell is sampled on a 4x8 grid; bit `row * 4 + col` is set where the art
//! covers that sample. Each candidate glyph gets the same kind of mask, and
//! the nearest one by Hamming distance wins. Edges then land on sub-cell
//! boundaries and diagonals become real slopes, instead of being blurred
//! across half-cell pixels.

use crate::glyph_table::{Fill, OCTANTS, WEDGES};
use crate::num;

pub const COLS: usize = 4;
pub const ROWS: usize = 8;

/// How much of Unicode's block repertoire the terminal draws well.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlyphSet {
    /// Halves, quadrants and eighths: every terminal.
    Blocks,
    /// Adds sextants (Unicode 13).
    Sextants,
    /// Adds octants (Unicode 16) and diagonal wedges.
    Full,
}

impl GlyphSet {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "blocks" => Some(Self::Blocks),
            "sextants" => Some(Self::Sextants),
            "full" => Some(Self::Full),
            _ => None,
        }
    }
}

pub struct Glyphs {
    table: Vec<(u32, char)>,
    /// Printable ASCII, for the look that draws with letters.
    ascii: Vec<(u32, char)>,
}

impl Glyphs {
    pub fn new(set: GlyphSet) -> Self {
        let mut table: Vec<(u32, char)> = Vec::new();
        let mut add = |c: char, inside: &dyn Fn(f32, f32) -> bool| {
            let mask = mask(inside);
            // Simpler glyphs come first and win ties.
            if !table.iter().any(|&(m, _)| m == mask) {
                table.push((mask, c));
            }
        };

        add(' ', &|_, _| false);
        add('█', &|_, _| true);
        add('▀', &|_, y| y < 0.5);
        add('▄', &|_, y| y >= 0.5);
        add('▌', &|x, _| x < 0.5);
        add('▐', &|x, _| x >= 0.5);
        for (c, bits) in [
            ('▘', 0b1000),
            ('▝', 0b0100),
            ('▖', 0b0010),
            ('▗', 0b0001),
            ('▚', 0b1001),
            ('▞', 0b0110),
            ('▛', 0b1110),
            ('▜', 0b1101),
            ('▙', 0b1011),
            ('▟', 0b0111),
        ] {
            add(c, &move |x, y| {
                let quadrant =
                    if y < 0.5 { 3_usize } else { 1 }.saturating_sub(usize::from(x >= 0.5));
                bit(bits, quadrant)
            });
        }
        for (i, c) in ['▁', '▂', '▃', '▅', '▆', '▇'].into_iter().enumerate() {
            let eighths = [1, 2, 3, 5, 6, 7].get(i).copied().unwrap_or(4) as f32;
            add(c, &move |_, y| y >= 1.0 - eighths / 8.0);
        }
        add('▔', &|_, y| y < 1.0 / 8.0);
        add('▎', &|x, _| x < 0.25);
        add('▊', &|x, _| x < 0.75);

        if set != GlyphSet::Blocks {
            // U+1FB00 onwards: every 2x3 pattern except blank, full and the
            // two halves, which exist already. Bit 0 is the top left, then
            // left to right, top to bottom.
            let mut code = 0x1fb00;
            for pattern in 1..63_u32 {
                if pattern == 21 || pattern == 42 {
                    continue;
                }
                if let Some(c) = char::from_u32(code) {
                    add(c, &move |x, y| {
                        let row: usize = if y < 1.0 / 3.0 {
                            0
                        } else if y < 2.0 / 3.0 {
                            1
                        } else {
                            2
                        };
                        bit(pattern, row.saturating_mul(2) | usize::from(x >= 0.5))
                    });
                }
                code = code.saturating_add(1);
            }
        }

        if set == GlyphSet::Full {
            for (i, &bits) in OCTANTS.iter().enumerate() {
                if let Some(c) = char::from_u32(0x1cd00_u32.saturating_add(num::u32_of(i))) {
                    add(c, &move |x, y| {
                        let index =
                            num::floor_usize(y * 4.0).saturating_add(if x >= 0.5 { 4 } else { 0 });
                        bit(u32::from(bits), index)
                    });
                }
            }
            for &(c, triangle, fill) in WEDGES {
                add(c, &move |x, y| {
                    let inside = in_triangle(x, y, triangle);
                    match fill {
                        Fill::Triangle => inside,
                        Fill::Outside => !inside,
                        Fill::TriangleAndBottomThird => inside || y >= 2.0 / 3.0,
                        Fill::TriangleAndTopThird => inside || y < 1.0 / 3.0,
                    }
                });
            }
        }

        Self {
            table,
            ascii: ascii(),
        }
    }

    pub fn pack_ascii(&self) -> Vec<u32> {
        self.ascii
            .iter()
            .flat_map(|&(mask, c)| [mask, u32::from(c)])
            .collect()
    }

    pub fn ascii_len(&self) -> u32 {
        num::u32_of(self.ascii.len())
    }

    /// The table for the GPU: each glyph's mask, then its code point.
    pub fn pack(&self) -> Vec<u32> {
        self.table
            .iter()
            .flat_map(|&(mask, c)| [mask, u32::from(c)])
            .collect()
    }

    pub fn len(&self) -> u32 {
        num::u32_of(self.table.len())
    }

    /// The glyph nearest to `mask`, and that glyph's own mask, as the shader
    /// picks it.
    #[cfg(test)]
    fn best(&self, mask: u32) -> (char, u32) {
        self.table
            .iter()
            .min_by_key(|&&(m, _)| (m ^ mask).count_ones())
            .map_or((' ', 0), |&(m, c)| (c, m))
    }
}

/// Printable ASCII shapes from the 8x8 font: each pair of font columns
/// becomes one of the grid's four. Shapes that look the same keep the first.
fn ascii() -> Vec<(u32, char)> {
    use font8x8::UnicodeFonts;
    let mut table: Vec<(u32, char)> = vec![(0, ' ')];
    for c in (0x21_u8..0x7f).map(char::from) {
        let Some(glyph) = font8x8::BASIC_FONTS.get(c) else {
            continue;
        };
        let mask = glyph.iter().enumerate().fold(0, |mask, (row, &bits)| {
            (0..COLS).fold(mask, |mask, col| {
                let pair = (u32::from(bits) >> col.saturating_mul(2)) & 0b11;
                if pair != 0 {
                    set(mask, row.saturating_mul(COLS).saturating_add(col))
                } else {
                    mask
                }
            })
        });
        if !table.iter().any(|&(m, _)| m == mask) {
            table.push((mask, c));
        }
    }
    table
}

/// Whether bit `index` of `mask` is set.
pub fn bit(mask: u32, index: usize) -> bool {
    u32::try_from(index)
        .ok()
        .and_then(|i| mask.checked_shr(i))
        .is_some_and(|m| m & 1 == 1)
}

/// `mask` with bit `index` set.
pub fn set(mask: u32, index: usize) -> u32 {
    mask | u32::try_from(index)
        .ok()
        .and_then(|i| 1_u32.checked_shl(i))
        .unwrap_or(0)
}

/// Samples `inside` at the centre of each of the 4x8 grid's cells.
fn mask(inside: &dyn Fn(f32, f32) -> bool) -> u32 {
    (0..ROWS * COLS).fold(0, |mask, i| {
        let (x, y) = (
            ((i % COLS) as f32 + 0.5) / COLS as f32,
            ((i / COLS) as f32 + 0.5) / ROWS as f32,
        );
        if inside(x, y) { set(mask, i) } else { mask }
    })
}

fn in_triangle(x: f32, y: f32, [a, b, c]: [(f32, f32); 3]) -> bool {
    let side = |p: (f32, f32), q: (f32, f32)| (q.0 - p.0) * (y - p.1) - (q.1 - p.1) * (x - p.0);
    let (d1, d2, d3) = (side(a, b), side(b, c), side(c, a));
    let negative = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let positive = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(negative && positive)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: u32 = u32::MAX;

    #[test]
    fn exact_shapes_match_their_glyph() {
        let glyphs = Glyphs::new(GlyphSet::Full);
        assert_eq!(glyphs.best(0).0, ' ');
        assert_eq!(glyphs.best(FULL).0, '█');
        assert_eq!(glyphs.best(0x0000_ffff).0, '▀');
        assert_eq!(glyphs.best(0x3333_3333).0, '▌');
    }

    #[test]
    fn diagonals_fit_better_with_the_full_set() {
        let glyphs = Glyphs::new(GlyphSet::Full);
        // Lower-left triangle below the main diagonal.
        let diagonal = mask(&|x, y| y > x);
        let (c, m) = glyphs.best(diagonal);
        let blocks = Glyphs::new(GlyphSet::Blocks).best(diagonal).1;
        assert!(
            (m ^ diagonal).count_ones() < (blocks ^ diagonal).count_ones(),
            "{c:?}"
        );
    }

    #[test]
    fn ascii_shapes_are_distinct_and_printable() {
        let glyphs = Glyphs::new(GlyphSet::Blocks);
        assert!(glyphs.ascii.len() > 60);
        assert!(
            glyphs
                .ascii
                .iter()
                .all(|&(_, c)| c == ' ' || c.is_ascii_graphic())
        );
    }

    #[test]
    fn smaller_sets_still_cover_every_mask() {
        let glyphs = Glyphs::new(GlyphSet::Blocks);
        assert_eq!(glyphs.best(0x0000_ffff).0, '▀');
        assert!(glyphs.table.len() < 40);
    }
}
