//! The full-screen frame: art coverage at sub-cell resolution, colour at two
//! pixels per cell, then encoded into terminal cells in one of several looks.

use ratatui::buffer::Buffer;
use ratatui::layout::Position;

use crate::glyphs::{self, COLS, FULL, Glyphs, ROWS};
use crate::num;
use crate::stage::Stage;
use crate::theme::Rgb;

const SAMPLES: usize = COLS * ROWS;

/// How the frame is turned into characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    /// Block glyphs matched to each cell's shape, for crisp edges.
    Smooth,
    /// Chunky half-block pixels.
    Pixels,
    /// Braille dots.
    Braille,
    /// Characters by density.
    Ascii,
    /// An LED sign.
    Led,
}

pub const LOOKS: &[Look] = &[
    Look::Smooth,
    Look::Smooth,
    Look::Smooth,
    Look::Pixels,
    Look::Braille,
    Look::Braille,
    Look::Ascii,
    Look::Led,
];

impl Look {
    pub fn name(self) -> &'static str {
        match self {
            Look::Smooth => "smooth",
            Look::Pixels => "pixels",
            Look::Braille => "braille",
            Look::Ascii => "ascii",
            Look::Led => "led",
        }
    }
}

const RAMP: &[char] = &[' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'];

/// One terminal cell's worth of the frame.
#[derive(Clone, Copy, Default)]
struct Spot {
    /// Art coverage per sample, row by row.
    alpha: [f32; SAMPLES],
    /// Art colour of the top and bottom half, premultiplied by `weight`.
    ink: [Rgb; 2],
    weight: [f32; 2],
    /// Background and light of the top and bottom half.
    base: [Rgb; 2],
}

impl Spot {
    /// Background and art colour of each half.
    fn halves(&self) -> [(Rgb, Rgb); 2] {
        let [top, bottom] = [
            (self.base[0], self.ink[0], self.weight[0]),
            (self.base[1], self.ink[1], self.weight[1]),
        ];
        [top, bottom].map(|(base, ink, weight)| {
            (
                base,
                if weight > 0.0 {
                    ink.scale(1.0 / weight)
                } else {
                    base
                },
            )
        })
    }

    /// Mean coverage of the top and bottom half.
    fn half_means(&self) -> [f32; 2] {
        let (top, bottom) = self
            .alpha
            .split_at_checked(SAMPLES / 2)
            .unwrap_or((&[], &[]));
        let half = num::float(SAMPLES / 2);
        [
            top.iter().sum::<f32>() / half,
            bottom.iter().sum::<f32>() / half,
        ]
    }
}

/// Each sample's `(column, row)` within its cell, in sample order.
fn sample_offsets() -> impl Iterator<Item = (usize, usize)> {
    (0..ROWS).flat_map(|row| (0..COLS).map(move |col| (col, row)))
}

/// Each braille dot's bit, and the four samples (a 2x2 block of the 4x8
/// grid) it stands for. Dots 1-3 run down the left column, 4-6 down the
/// right, and 7 and 8 sit at the bottom.
const BRAILLE: [(usize, [usize; 4]); 8] = [
    (0, [0, 1, 4, 5]),
    (3, [2, 3, 6, 7]),
    (1, [8, 9, 12, 13]),
    (4, [10, 11, 14, 15]),
    (2, [16, 17, 20, 21]),
    (5, [18, 19, 22, 23]),
    (6, [24, 25, 28, 29]),
    (7, [26, 27, 30, 31]),
];

const COLS_F: f32 = 4.0;
const ROWS_F: f32 = 8.0;

#[derive(Default)]
pub struct Canvas {
    pub cols: usize,
    pub rows: usize,
    spots: Vec<Spot>,
}

impl Canvas {
    pub fn resize(&mut self, cols: usize, rows: usize) {
        if (cols, rows) != (self.cols, self.rows) {
            *self = Self {
                cols,
                rows,
                spots: vec![Spot::default(); cols.saturating_mul(rows)],
            };
        }
        for spot in &mut self.spots {
            spot.alpha = [0.0; SAMPLES];
            spot.ink = [Rgb::BLACK; 2];
            spot.weight = [0.0; 2];
        }
    }

    /// Width in samples.
    pub fn width(&self) -> f32 {
        num::float(self.cols) * COLS_F
    }

    /// Height in samples.
    pub fn height(&self) -> f32 {
        num::float(self.rows) * ROWS_F
    }

    /// Every spot with its cell's column and row.
    fn cells(&mut self) -> impl Iterator<Item = (usize, usize, &mut Spot)> {
        let cols = self.cols.max(1);
        self.spots.iter_mut().enumerate().map(move |(i, spot)| {
            (
                i.checked_rem(cols).unwrap_or(0),
                i.checked_div(cols).unwrap_or(0),
                spot,
            )
        })
    }

    fn spot(&mut self, cx: usize, cy: usize) -> Option<&mut Spot> {
        let index = cy.checked_mul(self.cols)?.checked_add(cx)?;
        if cx >= self.cols {
            return None;
        }
        self.spots.get_mut(index)
    }

    /// Sets every pixel's background from `f(x, y, px, py)`, with `x` and `y`
    /// in `0.0..1.0` and `(px, py)` the half-cell pixel.
    pub fn backdrop(&mut self, f: impl Fn(f32, f32, usize, usize) -> Rgb) {
        let (cols, rows) = (
            num::float(self.cols.max(1)),
            num::float(self.rows.max(1)) * 2.0,
        );
        for (cx, cy, spot) in self.cells() {
            for (half, base) in spot.base.iter_mut().enumerate() {
                let py = cy.saturating_mul(2).saturating_add(half);
                *base = f(
                    (num::float(cx) + 0.5) / cols,
                    (num::float(py) + 0.5) / rows,
                    cx,
                    py,
                );
            }
        }
    }

    /// Adds light from `f(x, y)` (in samples) to every pixel's background.
    pub fn light(&mut self, f: impl Fn(f32, f32) -> Rgb) {
        for (cx, cy, spot) in self.cells() {
            for (half, base) in spot.base.iter_mut().enumerate() {
                let (x, y) = half_centre(cx, cy, half);
                *base = base.add(f(x, y));
            }
        }
    }

    /// Draws `stage` with each screen position looked up at `warp(x, y)`
    /// (`None` leaves it empty), faded by `opacity(x, y)` and coloured
    /// through `tint`. Opacity is taken once per half cell.
    pub fn paint(
        &mut self,
        stage: &Stage,
        t: f32,
        warp: impl Fn(f32, f32) -> Option<(f32, f32)>,
        opacity: impl Fn(f32, f32) -> f32,
        tint: impl Fn(Rgb) -> Rgb,
    ) {
        let (left, top, right, bottom) = stage.bounds();
        // A cell whose centre maps this far from the art can't touch it.
        let (mx, my) = (COLS_F * 4.0, ROWS_F * 4.0);
        for (cx, cy, spot) in self.cells() {
            let (x0, y0) = (num::float(cx) * COLS_F, num::float(cy) * ROWS_F);
            let near = warp(x0 + COLS_F / 2.0, y0 + ROWS_F / 2.0).is_some_and(|(x, y)| {
                x > left - mx && x < right + mx && y > top - my && y < bottom + my
            });
            if !near {
                continue;
            }
            let ops = [0, 1].map(|half| {
                let (x, y) = half_centre(cx, cy, half);
                opacity(x, y)
            });
            if ops.iter().all(|&op| op <= 0.0) {
                continue;
            }
            for ((col, row), alpha) in sample_offsets().zip(spot.alpha.iter_mut()) {
                let op = if row < ROWS / 2 { ops[0] } else { ops[1] };
                if op <= 0.0 {
                    continue;
                }
                let (x, y) = (x0 + num::float(col) + 0.5, y0 + num::float(row) + 0.5);
                if let Some((wx, wy)) = warp(x, y) {
                    *alpha = (*alpha + stage.coverage(wx, wy, t) * op).min(1.0);
                }
            }
            let rows = [cy.saturating_mul(2), cy.saturating_mul(2).saturating_add(1)];
            for (half, ((op, row), (ink, (weight, base)))) in ops
                .into_iter()
                .zip(rows)
                .zip(
                    spot.ink
                        .iter_mut()
                        .zip(spot.weight.iter_mut().zip(spot.base.iter_mut())),
                )
                .enumerate()
            {
                let (x, y) = half_centre(cx, cy, half);
                let Some((wx, wy)) = warp(x, y) else { continue };
                let shade = stage.shade(wx, wy, t, row);
                let color = tint(shade.ink);
                let w = op * (0.02 + stage.coverage(wx, wy, t));
                *ink = ink.add(color.scale(w));
                *weight += w;
                *base = base.add(color.scale(shade.glow * 0.3 * op));
            }
        }
    }

    /// Draws a square particle of `size` samples centred on `(x, y)`.
    pub fn splat(&mut self, x: f32, y: f32, size: f32, color: Rgb, alpha: f32) {
        let half = size / 2.0;
        let (x0, y0) = (num::floor_usize(x - half), num::floor_usize(y - half));
        let (x1, y1) = (num::floor_usize(x + half), num::floor_usize(y + half));
        let (width, height) = (
            self.cols.saturating_mul(COLS),
            self.rows.saturating_mul(ROWS),
        );
        for sy in y0..y1.min(height) {
            for sx in x0..x1.min(width) {
                let sample = (sy % ROWS).saturating_mul(COLS).saturating_add(sx % COLS);
                if let Some(a) = self
                    .spot(sx / COLS, sy / ROWS)
                    .and_then(|spot| spot.alpha.get_mut(sample))
                {
                    *a = (*a + alpha).min(1.0);
                }
            }
        }
        if x < 0.0 || y < 0.0 || x >= self.width() || y >= self.height() {
            return;
        }
        let (sx, sy) = (num::floor_usize(x), num::floor_usize(y));
        let half = usize::from(sy % ROWS >= ROWS / 2);
        if let Some(spot) = self.spot(sx / COLS, sy / ROWS)
            && let (Some(ink), Some(weight)) = (spot.ink.get_mut(half), spot.weight.get_mut(half))
        {
            *ink = ink.add(color.scale(alpha));
            *weight += alpha;
        }
    }

    pub fn encode(&self, look: Look, glyphs: &Glyphs, buf: &mut Buffer) {
        let cols = self.cols.max(1);
        for (i, spot) in self.spots.iter().enumerate() {
            let (cx, cy) = (
                i.checked_rem(cols).unwrap_or(0),
                i.checked_div(cols).unwrap_or(0),
            );
            let position = Position::new(num::u16_of(cx), num::u16_of(cy));
            let Some(cell) = buf.cell_mut(position) else {
                continue;
            };
            let [top, bottom] = spot.halves();
            let [top_mean, bottom_mean] = spot.half_means();
            let mean = (top_mean + bottom_mean) / 2.0;
            let blend = |(base, ink): (Rgb, Rgb), a: f32| base.mix(ink, a);
            let pixels = || ('▀', blend(top, top_mean), blend(bottom, bottom_mean));

            let (symbol, fg, bg) = match look {
                Look::Pixels => pixels(),
                Look::Smooth => {
                    let mask = spot.alpha.iter().enumerate().fold(0_u32, |m, (i, &a)| {
                        if a >= 0.5 { glyphs::set(m, i) } else { m }
                    });
                    let (glyph, shape) = glyphs.best(mask);
                    if shape == 0 || shape == FULL {
                        pixels()
                    } else {
                        // Each side of the glyph gets the average of the
                        // samples it covers, in linear light.
                        let side = |inside: bool| {
                            average(
                                spot.alpha
                                    .iter()
                                    .enumerate()
                                    .filter(|&(i, _)| glyphs::bit(shape, i) == inside)
                                    .map(|(i, &a)| {
                                        blend(if i < SAMPLES / 2 { top } else { bottom }, a)
                                    }),
                            )
                        };
                        (glyph, side(true), side(false))
                    }
                }
                Look::Braille => {
                    let bits = BRAILLE.iter().fold(0_u32, |bits, &(bit, samples)| {
                        let covered = samples
                            .iter()
                            .map(|&s| spot.alpha.get(s).copied().unwrap_or(0.0))
                            .sum::<f32>();
                        if covered >= 2.0 {
                            glyphs::set(bits, bit)
                        } else {
                            bits
                        }
                    });
                    let symbol = if bits == 0 {
                        ' '
                    } else {
                        char::from_u32(0x2800_u32.saturating_add(bits)).unwrap_or(' ')
                    };
                    let top_dots =
                        num::float(usize::try_from((bits & 0b0001_1011).count_ones()).unwrap_or(0));
                    let dots = num::float(usize::try_from(bits.count_ones().max(1)).unwrap_or(1));
                    let ink = top.1.mix(bottom.1, 1.0 - top_dots / dots);
                    (symbol, ink.lerp(Rgb::WHITE, 0.1), top.0.mix(bottom.0, 0.5))
                }
                Look::Ascii => {
                    let level =
                        num::floor_usize((mean * num::float(RAMP.len().saturating_sub(1))).round());
                    let symbol = RAMP.get(level).copied().unwrap_or(' ');
                    (symbol, top.1.mix(bottom.1, 0.5), top.0.mix(bottom.0, 0.5))
                }
                Look::Led => {
                    let base = top.0.mix(bottom.0, 0.5);
                    if mean >= 0.35 {
                        ('●', top.1.mix(bottom.1, 0.5).lerp(Rgb::WHITE, 0.08), base)
                    } else {
                        ('·', base.scale(1.8).add(Rgb(0.04, 0.04, 0.04)), base)
                    }
                }
            };
            cell.set_char(symbol).set_fg(fg.color()).set_bg(bg.color());
        }
    }
}

/// The centre of half `half` (0 top, 1 bottom) of cell `(cx, cy)`, in samples.
fn half_centre(cx: usize, cy: usize, half: usize) -> (f32, f32) {
    (
        num::float(cx) * COLS_F + COLS_F / 2.0,
        num::float(cy) * ROWS_F + num::float(half) * ROWS_F / 2.0 + ROWS_F / 4.0,
    )
}

/// Average colour in linear light.
fn average(colors: impl Iterator<Item = Rgb>) -> Rgb {
    let (sum, n) = colors.fold((Rgb::BLACK, 0.0), |(sum, n), c| {
        (sum.add(c.mul(c)), n + 1.0)
    });
    if n == 0.0 {
        return Rgb::BLACK;
    }
    let mean = sum.scale(1.0 / n);
    Rgb(mean.0.sqrt(), mean.1.sqrt(), mean.2.sqrt())
}
