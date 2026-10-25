//! Transitions in and out of a scene, built on tachyonfx.
//!
//! Every effect starts (or, leaving, ends) with the art fully hidden, so the
//! next scene can swap in on the same frame.

use std::sync::Arc;

use ratatui::buffer::Cell;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use tachyonfx::fx::{self, EvolveSymbolSet};
use tachyonfx::pattern::{
    AnyPattern, DiagonalPattern, DiamondPattern, RadialPattern, SpiralPattern, SweepPattern,
};
use tachyonfx::{Effect, Interpolation, Motion};

use crate::stage::{InkMask, Stage};
use crate::theme::{Rgb, Theme};

pub fn enter(stage: &Stage, theme: &Theme) -> Effect {
    let bg = theme.background.color();
    let timer = (1600, Interpolation::QuadOut);
    let effect = match fastrand::u8(0..8) {
        0 => patterned(
            fx::coalesce_from(Style::new().fg(bg).bg(bg), timer),
            stage.area,
        ),
        1 => patterned(fx::fade_from(bg, bg, timer), stage.area),
        2 => fx::sweep_in(motion(), gradient(stage.area), 4, bg, timer),
        3 => fx::slide_in(motion(), gradient(stage.area), 3, bg, timer),
        4 => {
            let pattern = pattern(stage.area);
            fx::parallel(&[
                with(fx::evolve_from(symbols(), timer), pattern.clone()),
                with(fx::fade_from(bg, bg, timer), pattern),
            ])
        }
        5 => decrypt(stage, theme),
        6 => matrix(stage, theme),
        _ => beams(stage, theme),
    };
    effect.with_area(stage.area)
}

pub fn leave(stage: &Stage, theme: &Theme) -> Effect {
    let bg = theme.background.color();
    let timer = (1100, Interpolation::QuadIn);
    let effect = match fastrand::u8(0..8) {
        0 => patterned(
            fx::dissolve_to(Style::new().fg(bg).bg(bg), timer),
            stage.area,
        ),
        1 => patterned(fx::fade_to(bg, bg, timer), stage.area),
        2 => fx::sweep_out(motion(), gradient(stage.area), 4, bg, timer),
        3 => fx::slide_out(motion(), gradient(stage.area), 3, bg, timer),
        4 => {
            let pattern = pattern(stage.area);
            fx::parallel(&[
                with(fx::evolve_into(symbols(), timer), pattern.clone()),
                with(fx::fade_to(bg, bg, timer), pattern),
            ])
        }
        5 => decrypt(stage, theme).reversed(),
        6 => matrix(stage, theme).reversed(),
        _ => beams(stage, theme).reversed(),
    };
    effect.with_area(stage.area)
}

fn patterned(effect: Effect, area: Rect) -> Effect {
    with(effect, pattern(area))
}

fn with(effect: Effect, pattern: Option<AnyPattern>) -> Effect {
    match pattern {
        Some(pattern) => effect.with_pattern(pattern),
        None => effect,
    }
}

fn pattern(area: Rect) -> Option<AnyPattern> {
    let width = (area.width.max(area.height) / 5).max(4) as f32;
    match fastrand::u8(0..6) {
        0 => None,
        1 => Some(RadialPattern::center().with_transition_width(width).into()),
        2 => Some(DiamondPattern::center().with_transition_width(width).into()),
        3 => Some(SpiralPattern::center().with_transition_width(width).into()),
        4 => Some(
            DiagonalPattern::top_left_to_bottom_right()
                .with_transition_width(width)
                .into(),
        ),
        _ => Some(SweepPattern::left_to_right(width as u16).into()),
    }
}

fn motion() -> Motion {
    [
        Motion::LeftToRight,
        Motion::RightToLeft,
        Motion::UpToDown,
        Motion::DownToUp,
    ]
    .get(fastrand::usize(..4))
    .copied()
    .unwrap_or(Motion::LeftToRight)
}

fn gradient(area: Rect) -> u16 {
    (area.width / 6).clamp(4, 40)
}

fn symbols() -> EvolveSymbolSet {
    [
        EvolveSymbolSet::Shaded,
        EvolveSymbolSet::Quadrants,
        EvolveSymbolSet::Circles,
        EvolveSymbolSet::Squares,
    ]
    .get(fastrand::usize(..4))
    .copied()
    .unwrap_or_default()
}

#[derive(Clone)]
struct Glyphs {
    ink: Arc<InkMask>,
    background: Color,
    accent: Rgb,
}

const NOISE: &[char] = &[
    '░', '▒', '▓', '#', '%', '&', '@', '$', '*', '+', '=', ':', '/', '<', '>', '?',
];
const RAIN: &[char] = &[
    '0', '1', 'ｱ', 'ｲ', 'ｳ', 'ｴ', 'ｵ', 'ｶ', 'ｷ', 'ｸ', 'ｹ', 'ｺ', 'ｻ', 'ｼ', 'ｽ', 'ﾀ', 'ﾁ', 'ﾂ',
];

impl Glyphs {
    fn new(stage: &Stage, theme: &Theme) -> Self {
        Self {
            ink: stage.ink.clone(),
            background: theme.background.color(),
            accent: theme.accent,
        }
    }

    fn hide(&self, cell: &mut Cell) {
        cell.set_char(' ')
            .set_fg(self.background)
            .set_bg(self.background);
    }
}

/// Noise glyphs scramble in, then settle into the art with a flash
/// (terminaltexteffects' "decrypt").
fn decrypt(stage: &Stage, theme: &Theme) -> Effect {
    fx::effect_fn(
        Glyphs::new(stage, theme),
        (2600, Interpolation::Linear),
        |glyphs, ctx, cells| {
            let a = ctx.alpha();
            for (pos, cell) in cells {
                if !glyphs.ink.contains(pos) {
                    continue;
                }
                let (x, y) = (pos.x as u32, pos.y as u32);
                let appear = hash(x, y, 1) * 0.45;
                let settle = appear + 0.2 + hash(x, y, 2) * 0.3;
                if a < appear {
                    glyphs.hide(cell);
                } else if a < settle {
                    let glyph = pick(NOISE, hash(x, y, (a * 50.0) as u32));
                    let color = rgb(cell.fg).lerp(glyphs.accent, 0.5).color();
                    cell.set_char(glyph).set_fg(color).set_bg(glyphs.background);
                } else {
                    brighten(cell, 1.0 - (a - settle) / 0.12);
                }
            }
        },
    )
}

/// Columns of falling glyphs uncover the art from top to bottom.
fn matrix(stage: &Stage, theme: &Theme) -> Effect {
    fx::effect_fn(
        Glyphs::new(stage, theme),
        (2800, Interpolation::Linear),
        |glyphs, ctx, cells| {
            let a = ctx.alpha();
            let area = ctx.area;
            let tail = area.height as f32 * 0.35 + 4.0;
            for (pos, cell) in cells {
                let column = pos.x as u32;
                let delay = hash(column, 0, 3) * 0.35;
                let speed = 1.0 + hash(column, 0, 4) * 0.6;
                let head = ((a - delay) / 0.65).max(0.0) * speed * (area.height as f32 + tail);
                let d = head - pos.y.saturating_sub(area.y) as f32;
                let glyph = pick(RAIN, hash(column, pos.y as u32, (a * 30.0) as u32));
                let ink = glyphs.ink.contains(pos);
                if ink && d < 0.0 {
                    glyphs.hide(cell);
                } else if (0.0..1.0).contains(&d) {
                    cell.set_char(glyph).set_fg(Rgb::WHITE.color());
                } else if d < tail {
                    let fade = 1.0 - d / tail;
                    if ink {
                        let color = rgb(cell.fg).lerp(glyphs.accent, fade).color();
                        cell.set_char(glyph).set_fg(color).set_bg(glyphs.background);
                    } else if d > 0.0 {
                        let color = rgb(cell.bg).lerp(glyphs.accent, fade * 0.6).color();
                        cell.set_char(glyph).set_fg(color);
                    }
                }
            }
        },
    )
}

/// Light beams race along each row and leave the art lit behind them.
fn beams(stage: &Stage, theme: &Theme) -> Effect {
    fx::effect_fn(
        Glyphs::new(stage, theme),
        (2200, Interpolation::Linear),
        |glyphs, ctx, cells| {
            let a = ctx.alpha();
            let area = ctx.area;
            let trail = area.width as f32 * 0.25 + 4.0;
            for (pos, cell) in cells {
                let row = pos.y as u32;
                let progress = ((a - hash(0, row, 5) * 0.5) / 0.5).clamp(0.0, 1.0);
                let head = progress * (area.width as f32 + trail);
                let column = if hash(0, row, 6) < 0.5 {
                    pos.x.saturating_sub(area.x)
                } else {
                    area.right().saturating_sub(pos.x + 1)
                };
                let d = head - column as f32;
                if glyphs.ink.contains(pos) {
                    if d < 0.0 {
                        glyphs.hide(cell);
                    } else if d < trail {
                        brighten(cell, 1.0 - d / trail);
                    }
                } else if (0.0..trail * 0.5).contains(&d) {
                    let color = rgb(cell.bg)
                        .lerp(glyphs.accent, 1.0 - 2.0 * d / trail)
                        .color();
                    cell.set_char('━').set_fg(color);
                }
            }
        },
    )
}

fn brighten(cell: &mut Cell, amount: f32) {
    if amount > 0.0 {
        let (fg, bg) = (
            rgb(cell.fg).lerp(Rgb::WHITE, amount * 0.7),
            rgb(cell.bg).lerp(Rgb::WHITE, amount * 0.7),
        );
        cell.set_fg(fg.color()).set_bg(bg.color());
    }
}

fn rgb(color: Color) -> Rgb {
    match color {
        Color::Rgb(r, g, b) => Rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0),
        _ => Rgb::default(),
    }
}

fn pick(glyphs: &[char], t: f32) -> char {
    glyphs
        .get((t * glyphs.len() as f32) as usize)
        .copied()
        .unwrap_or(' ')
}

/// Stable pseudo-random `0.0..1.0` per cell, so effects need no stored state.
fn hash(x: u32, y: u32, salt: u32) -> f32 {
    let mut h =
        x.wrapping_mul(0x9e37_79b1) ^ y.wrapping_mul(0x85eb_ca77) ^ salt.wrapping_mul(0xc2b2_ae3d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    (h & 0xff_ffff) as f32 / 0x100_0000 as f32
}
