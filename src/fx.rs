//! Cell-level effects on the whole screen, built on tachyonfx: transitions
//! out of and into a scene (used when the look changes, since the frame's
//! characters change kind), and accents that liven up a scene while it holds.
//!
//! Every transition starts (or, leaving, ends) with the screen fully hidden,
//! so the next scene can swap in on the same frame.

use std::sync::Arc;

use ratatui::buffer::Cell;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use tachyonfx::fx::{self, EvolveSymbolSet};
use tachyonfx::pattern::{
    AnyPattern, DiagonalPattern, DiamondPattern, RadialPattern, SpiralPattern, SweepPattern,
};
use tachyonfx::{Effect, Interpolation, Motion};

use crate::backdrop::hash;
use crate::num;
use crate::stage::{InkMask, Stage};
use crate::theme::Rgb;

pub fn enter(stage: &Stage, area: Rect) -> Effect {
    let bg = stage.act.theme().background.color();
    let timer = (1800, Interpolation::QuadOut);
    match fastrand::u8(0..8) {
        0 => patterned(fx::coalesce_from(Style::new().fg(bg).bg(bg), timer), area),
        1 => patterned(fx::fade_from(bg, bg, timer), area),
        2 => fx::sweep_in(motion(), gradient(area), 6, bg, timer),
        3 => fx::slide_in(motion(), gradient(area), 4, bg, timer),
        4 => {
            let pattern = pattern(area);
            fx::parallel(&[
                with(fx::evolve_from(symbols(), timer), pattern.clone()),
                with(fx::fade_from(bg, bg, timer), pattern),
            ])
        }
        5 => decrypt(stage),
        6 => matrix(stage),
        _ => beams(stage),
    }
}

pub fn leave(stage: &Stage, area: Rect) -> Effect {
    let bg = stage.act.theme().background.color();
    let timer = (1300, Interpolation::QuadIn);
    match fastrand::u8(0..8) {
        0 => patterned(fx::dissolve_to(Style::new().fg(bg).bg(bg), timer), area),
        1 => patterned(fx::fade_to(bg, bg, timer), area),
        2 => fx::sweep_out(motion(), gradient(area), 6, bg, timer),
        3 => fx::slide_out(motion(), gradient(area), 4, bg, timer),
        4 => {
            let pattern = pattern(area);
            fx::parallel(&[
                with(fx::evolve_into(symbols(), timer), pattern.clone()),
                with(fx::fade_to(bg, bg, timer), pattern),
            ])
        }
        5 => decrypt(stage).reversed(),
        6 => matrix(stage).reversed(),
        _ => beams(stage).reversed(),
    }
}

const HUE_RIPPLE: [f32; 3] = [120.0, 10.0, 6.0];

/// A brief flourish over a scene that is holding still. A cell's foreground
/// and background are both picture (its top and bottom pixel), so accents
/// change them alike.
pub fn accent(stage: &Stage, area: Rect) -> Option<Effect> {
    // Most holds stay still; a flourish now and then keeps it alive.
    if fastrand::f32() >= 0.25 {
        return None;
    }
    let width = (area.width / 4).max(8);
    Some(match fastrand::u8(0..3) {
        // A faint sheen of light sweeps across and back.
        0 => fx::ping_pong(
            fx::lighten(Some(0.12), Some(0.12), (1100, Interpolation::SineInOut))
                .with_pattern(SweepPattern::new(motion(), width)),
        ),
        // Colours ripple out from the centre and settle.
        1 => fx::ping_pong(
            fx::hsl_shift(
                Some(HUE_RIPPLE),
                Some(HUE_RIPPLE),
                (1300, Interpolation::SineInOut),
            )
            .with_pattern(RadialPattern::center().with_transition_width(f32::from(width))),
        ),
        _ => static_burst(stage),
    })
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
    let width = f32::from((area.width.max(area.height) / 5).max(4));
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
        _ => Some(SweepPattern::left_to_right(num::floor_u16(width)).into()),
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
    (area.width / 5).clamp(4, 60)
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
    fn new(stage: &Stage) -> Self {
        let theme = stage.act.theme();
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

/// Noise glyphs scramble in all over the screen, then the art settles out of
/// them with a flash (terminaltexteffects' "decrypt").
fn decrypt(stage: &Stage) -> Effect {
    fx::effect_fn(
        Glyphs::new(stage),
        (2600, Interpolation::Linear),
        |glyphs, ctx, cells| {
            let a = ctx.alpha();
            for (pos, cell) in cells {
                let (x, y) = (u32::from(pos.x), u32::from(pos.y));
                let appear = hash(x, y.wrapping_add(1000)) * 0.45;
                let settle = appear + 0.2 + hash(x.wrapping_add(1000), y) * 0.3;
                let ink = glyphs.ink.contains(pos);
                if a < appear {
                    glyphs.hide(cell);
                } else if a < settle {
                    // Away from the art only some cells flicker, and dimmer.
                    if !ink && hash(x.wrapping_add(7), y.wrapping_add(7)) > 0.18 {
                        continue;
                    }
                    let glyph = pick(
                        NOISE,
                        hash(
                            x,
                            y.wrapping_add(num::floor_u32(a * 50.0).wrapping_mul(977)),
                        ),
                    );
                    let original = Rgb::from_color(cell.fg);
                    let color = if ink {
                        original.lerp(glyphs.accent, 0.5)
                    } else {
                        glyphs.accent.scale(0.45)
                    };
                    cell.set_char(glyph)
                        .set_fg(color.color())
                        .set_bg(glyphs.background);
                } else {
                    brighten(cell, 1.0 - (a - settle) / 0.12);
                }
            }
        },
    )
}

/// Columns of falling glyphs uncover the screen from top to bottom.
fn matrix(stage: &Stage) -> Effect {
    fx::effect_fn(
        Glyphs::new(stage),
        (2800, Interpolation::Linear),
        |glyphs, ctx, cells| {
            let a = ctx.alpha();
            let area = ctx.area;
            let tail = f32::from(area.height) * 0.35 + 4.0;
            for (pos, cell) in cells {
                let column = u32::from(pos.x);
                let delay = hash(column, 3) * 0.35;
                let speed = 1.0 + hash(column, 4) * 0.6;
                let head = ((a - delay) / 0.65).max(0.0) * speed * (f32::from(area.height) + tail);
                let d = head - f32::from(pos.y.saturating_sub(area.y));
                let glyph = pick(
                    RAIN,
                    hash(
                        column,
                        u32::from(pos.y).wrapping_add(num::floor_u32(a * 30.0).wrapping_mul(101)),
                    ),
                );
                if d < 0.0 {
                    glyphs.hide(cell);
                } else if d < 1.0 {
                    cell.set_char(glyph)
                        .set_fg(Rgb::WHITE.color())
                        .set_bg(glyphs.background);
                } else if d < tail {
                    let fade = 1.0 - d / tail;
                    let behind =
                        Rgb::from_color(cell.bg).mix(Rgb::from_color(glyphs.background), fade);
                    let color = Rgb::from_color(cell.fg).lerp(glyphs.accent, fade);
                    cell.set_char(glyph)
                        .set_fg(color.color())
                        .set_bg(behind.color());
                }
            }
        },
    )
}

/// Light beams race along every row and leave the screen lit behind them.
fn beams(stage: &Stage) -> Effect {
    fx::effect_fn(
        Glyphs::new(stage),
        (2200, Interpolation::Linear),
        |glyphs, ctx, cells| {
            let a = ctx.alpha();
            let area = ctx.area;
            let trail = f32::from(area.width) * 0.25 + 4.0;
            for (pos, cell) in cells {
                let row = u32::from(pos.y);
                let progress = ((a - hash(row, 5) * 0.5) / 0.5).clamp(0.0, 1.0);
                let head = progress * (f32::from(area.width) + trail);
                let column = if hash(row, 6) < 0.5 {
                    pos.x.saturating_sub(area.x)
                } else {
                    area.right().saturating_sub(pos.x.saturating_add(1))
                };
                let d = head - f32::from(column);
                if d < 0.0 {
                    glyphs.hide(cell);
                } else if d < 1.0 {
                    cell.set_char('━')
                        .set_fg(Rgb::WHITE.color())
                        .set_bg(glyphs.background);
                } else if d < trail {
                    brighten(
                        cell,
                        (1.0 - d / trail) * if glyphs.ink.contains(pos) { 1.0 } else { 0.6 },
                    );
                }
            }
        },
    )
}

/// A short burst of static across the screen.
fn static_burst(stage: &Stage) -> Effect {
    fx::effect_fn(
        Glyphs::new(stage),
        (500, Interpolation::Linear),
        |glyphs, ctx, cells| {
            let a = ctx.alpha();
            let density = (a * std::f32::consts::PI).sin() * 0.25;
            let step = num::floor_u32(a * 20.0);
            for (pos, cell) in cells {
                let (x, y) = (u32::from(pos.x), u32::from(pos.y));
                if hash(x.wrapping_add(step.wrapping_mul(131)), y) < density {
                    let glyph = pick(NOISE, hash(x, y.wrapping_add(step)));
                    let color = Rgb::from_color(cell.fg).lerp(glyphs.accent, 0.6);
                    cell.set_char(glyph).set_fg(color.color());
                }
            }
        },
    )
}

fn brighten(cell: &mut Cell, amount: f32) {
    if amount > 0.0 {
        let fg = Rgb::from_color(cell.fg).lerp(Rgb::WHITE, amount * 0.7);
        let bg = Rgb::from_color(cell.bg).lerp(Rgb::WHITE, amount * 0.7);
        cell.set_fg(fg.color()).set_bg(bg.color());
    }
}

fn pick(glyphs: &[char], t: f32) -> char {
    glyphs
        .get(num::floor_usize(t * num::float(glyphs.len())))
        .copied()
        .unwrap_or(' ')
}
