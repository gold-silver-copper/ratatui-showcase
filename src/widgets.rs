//! Real ratatui widgets as a scene's art: a bar chart, a line chart or a
//! small dashboard, rendered into a buffer with made-up data, then drawn
//! cell by cell into pixels so every effect can treat them like a logo.

use std::sync::OnceLock;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::symbols::Marker;
use ratatui::widgets::{
    Axis, BarChart, Block, Chart, Dataset, Gauge, GraphType, LineGauge, Sparkline, Widget,
};

use crate::art::{Bitmap, Material, Px};
use crate::glyphs::{COLS, GlyphSet, Glyphs, ROWS};
use crate::num;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Bars,
    Lines,
    Dashboard,
}

pub const PANELS: &[Panel] = &[Panel::Bars, Panel::Lines, Panel::Dashboard];

impl Panel {
    pub fn name(self) -> &'static str {
        match self {
            Panel::Bars => "BarChart",
            Panel::Lines => "Chart",
            Panel::Dashboard => "Gauge + Sparkline + LineGauge",
        }
    }
}

/// Cells the widgets are laid out in.
const WIDTH: u16 = 60;
const HEIGHT: u16 = 18;

/// The widgets of `panel`, with data from `seed`, as pixels: 4 x 8 per cell.
pub fn render(panel: Panel, seed: u64) -> Bitmap {
    let mut rng = fastrand::Rng::with_seed(seed);
    let area = Rect::new(0, 0, WIDTH, HEIGHT);
    let mut buf = Buffer::empty(area);
    match panel {
        Panel::Bars => {
            let names = [
                "rat", "a", "tui", "tty", "vt", "pty", "ansi", "crt", "tty2", "bash",
            ];
            let data: Vec<(&str, u64)> = names.iter().map(|&n| (n, rng.u64(2..=40))).collect();
            BarChart::default()
                .block(Block::bordered().title(" widgets rendered "))
                .bar_width(4)
                .bar_gap(1)
                .data(data.as_slice())
                .render(area, &mut buf);
        }
        Panel::Lines => {
            let (a, b) = (rng.f64() * 3.0 + 1.0, rng.f64() * 2.0);
            let wave: Vec<(f64, f64)> = (0..200)
                .map(|i| {
                    let x = f64::from(i) / 20.0;
                    (x, (x * a).sin() * 0.8 + (x * 0.7 + b).sin() * 0.2)
                })
                .collect();
            let other: Vec<(f64, f64)> = (0..200)
                .map(|i| {
                    let x = f64::from(i) / 20.0;
                    (x, (x * 1.3 + b).cos() * 0.5)
                })
                .collect();
            Chart::new(vec![
                Dataset::default()
                    .marker(Marker::Braille)
                    .graph_type(GraphType::Line)
                    .data(&wave),
                Dataset::default()
                    .marker(Marker::Braille)
                    .graph_type(GraphType::Line)
                    .data(&other),
            ])
            .block(Block::bordered().title(" Chart "))
            .x_axis(Axis::default().bounds([0.0, 10.0]))
            .y_axis(Axis::default().bounds([-1.1, 1.1]))
            .render(area, &mut buf);
        }
        Panel::Dashboard => {
            let rows = Layout::vertical([
                Constraint::Length(3),
                Constraint::Min(4),
                Constraint::Length(3),
                Constraint::Length(3),
            ])
            .split(area);
            let samples: Vec<u64> = (0..u64::from(WIDTH))
                .map(|i| {
                    let x =
                        num::narrow(f64::from(num::u32_of(usize::try_from(i).unwrap_or(0))) * 0.25);
                    num::floor_u32((x.sin() * 0.5 + 0.5) * 9.0 + rng.f32() * 6.0).into()
                })
                .collect();
            let ratio = 0.3 + rng.f64() * 0.6;
            if let (Some(&top), Some(&middle), Some(&lower), Some(&bottom)) =
                (rows.first(), rows.get(1), rows.get(2), rows.get(3))
            {
                Gauge::default()
                    .block(Block::bordered().title(" Gauge "))
                    .ratio(ratio)
                    .render(top, &mut buf);
                Sparkline::default()
                    .block(Block::bordered().title(" Sparkline "))
                    .data(&samples)
                    .render(middle, &mut buf);
                LineGauge::default()
                    .block(Block::bordered().title(" LineGauge "))
                    .ratio(1.0 - ratio * 0.5)
                    .render(lower, &mut buf);
                Gauge::default()
                    .block(Block::bordered())
                    .ratio(ratio * 0.6)
                    .label("ratatui")
                    .render(bottom, &mut buf);
            }
        }
    }
    pixels(&buf)
}

fn glyphs() -> &'static Glyphs {
    static GLYPHS: OnceLock<Glyphs> = OnceLock::new();
    GLYPHS.get_or_init(|| Glyphs::new(GlyphSet::Full))
}

/// Each cell of `buf` drawn as 4 x 8 pixels: borders by their arms,
/// braille by its dots, everything else by its block or ASCII shape.
fn pixels(buf: &Buffer) -> Bitmap {
    let (w, h) = (usize::from(buf.area.width), usize::from(buf.area.height));
    let mut bitmap = Bitmap::new(w.saturating_mul(COLS), h.saturating_mul(ROWS), 1.0);
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            let Some(cell) = buf.cell((x, y)) else {
                continue;
            };
            let Some(c) = cell.symbol().chars().next() else {
                continue;
            };
            let (mask, material) = if let Some(mask) = border(c) {
                (mask, Material::Border)
            } else if let Some(mask) = braille(c) {
                (mask, Material::Ink)
            } else if c.is_ascii_alphanumeric() {
                (glyphs().shape(c).unwrap_or(0), Material::Hat)
            } else {
                (glyphs().shape(c).unwrap_or(0), Material::Ink)
            };
            for bit in 0..COLS * ROWS {
                if mask >> bit & 1 == 1 {
                    bitmap.set(
                        usize::from(x)
                            .saturating_mul(COLS)
                            .saturating_add(bit % COLS),
                        usize::from(y)
                            .saturating_mul(ROWS)
                            .saturating_add(bit / COLS),
                        Px { ink: 1.0, material },
                    );
                }
            }
        }
    }
    bitmap
}

/// The pixels of 4 x 8 inside `x0..x1` by `y0..y1`.
fn rect(x0: usize, x1: usize, y0: usize, y1: usize) -> u32 {
    (y0..y1)
        .flat_map(|y| (x0..x1).map(move |x| y.saturating_mul(COLS).saturating_add(x)))
        .fold(0, |m, bit| m | 1 << bit)
}

/// A box-drawing character by the arms it has: up, down, left, right.
fn border(c: char) -> Option<u32> {
    let (up, down, left, right) = match c {
        '─' | '━' | '═' => (false, false, true, true),
        '│' | '┃' | '║' => (true, true, false, false),
        '┌' | '╭' | '┏' | '╔' => (false, true, false, true),
        '┐' | '╮' | '┓' | '╗' => (false, true, true, false),
        '└' | '╰' | '┗' | '╚' => (true, false, false, true),
        '┘' | '╯' | '┛' | '╝' => (true, false, true, false),
        '├' => (true, true, false, true),
        '┤' => (true, true, true, false),
        '┬' => (false, true, true, true),
        '┴' => (true, false, true, true),
        '┼' => (true, true, true, true),
        _ => return None,
    };
    let mut mask = rect(1, 3, 3, 5);
    if up {
        mask |= rect(1, 3, 0, 4);
    }
    if down {
        mask |= rect(1, 3, 4, 8);
    }
    if left {
        mask |= rect(0, 2, 3, 5);
    }
    if right {
        mask |= rect(2, 4, 3, 5);
    }
    Some(mask)
}

/// A braille character's dots, each 2 x 2 pixels.
fn braille(c: char) -> Option<u32> {
    let code = u32::from(c).checked_sub(0x2800).filter(|&d| d < 256)?;
    // Bit i of the code is the dot at (column, row).
    let dots: [(usize, usize); 8] = [
        (0, 0),
        (0, 1),
        (0, 2),
        (1, 0),
        (1, 1),
        (1, 2),
        (0, 3),
        (1, 3),
    ];
    Some(
        dots.iter()
            .enumerate()
            .filter(|&(i, _)| code >> i & 1 == 1)
            .fold(0, |m, (_, &(x, y))| {
                let (x, y) = (x.saturating_mul(2), y.saturating_mul(2));
                m | rect(x, x.saturating_add(2), y, y.saturating_add(2))
            }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_panel_draws_something() {
        for &panel in PANELS {
            let bitmap = render(panel, 7);
            let ink = (0..bitmap.height)
                .flat_map(|y| (0..bitmap.width).map(move |x| (x, y)))
                .filter(|&(x, y)| bitmap.get(x, y).ink > 0.5)
                .count();
            assert!(ink > 2000, "{panel:?} drew {ink} pixels");
        }
    }

    #[test]
    fn borders_join_up() {
        // A corner's arms reach the cell's edges, so lines run on unbroken.
        let corner = border('┌').unwrap_or(0);
        assert_eq!(corner >> (7 * COLS + 1) & 1, 1);
        assert_eq!(corner >> (3 * COLS + 3) & 1, 1);
    }
}
