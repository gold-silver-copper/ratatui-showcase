//! What a scene shows, and that scene laid out for the current terminal.
//!
//! Positions are in sub-cell samples: each cell is `COLS` x `ROWS` of them.

use std::f32::consts::TAU;
use std::sync::Arc;

use ratatui::layout::Position;

use crate::art::{self, Bitmap, Material, Style};
use crate::backdrop::Backdrop;
use crate::canvas::Look;
use crate::fonts::{self, Face};
use crate::glyphs::{COLS, ROWS};
use crate::num;
use crate::theme::{Rgb, Shader, THEMES, Theme};

pub const WORDMARK: &str = "ratatui";

#[derive(Clone)]
pub enum Logo {
    Mascot,
    Emblem,
}

#[derive(Clone)]
pub enum Word {
    Small,
    Tiny,
    Pixel,
    Face(Face),
}

#[derive(Clone)]
pub enum Subject {
    Logo(Logo),
    Word(Word),
    /// Logo and wordmark together.
    Lockup(Logo, Word),
}

/// Gentle motion while a scene holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Idle {
    Still,
    Float,
    Wave,
    Breathe,
    Sway,
}

pub const IDLES: &[Idle] = &[
    Idle::Still,
    Idle::Float,
    Idle::Wave,
    Idle::Breathe,
    Idle::Sway,
];

/// Everything that makes one scene look the way it does.
#[derive(Clone)]
pub struct Act {
    pub subject: Subject,
    pub theme: usize,
    pub shader: Shader,
    pub backdrop: Backdrop,
    pub look: Look,
    pub style: Style,
    pub idle: Idle,
}

impl Act {
    pub fn theme(&self) -> &'static Theme {
        THEMES
            .get(self.theme)
            .or(THEMES.first())
            .unwrap_or(&FALLBACK)
    }

    pub fn label(&self) -> String {
        let name = |word: &Word| match word {
            Word::Small => "RatatuiLogo::small()".to_owned(),
            Word::Tiny => "RatatuiLogo::tiny()".to_owned(),
            Word::Pixel => "font8x8".to_owned(),
            Word::Face(face) => face.name.clone(),
        };
        let logo = |logo: &Logo| match logo {
            Logo::Mascot => "RatatuiMascot",
            Logo::Emblem => "logo.svg",
        };
        match &self.subject {
            Subject::Logo(l) => logo(l).to_owned(),
            Subject::Word(w) => name(w),
            Subject::Lockup(l, w) => format!("{} + {}", logo(l), name(w)),
        }
    }
}

const FALLBACK: Theme = Theme {
    name: "",
    background: Rgb::BLACK,
    stops: &[],
    accent: Rgb::WHITE,
};

struct Piece {
    bitmap: Bitmap,
    glow: Bitmap,
    glow_radius: f32,
    x: f32,
    y: f32,
}

impl Piece {
    fn new(bitmap: Bitmap, x: f32, y: f32) -> Self {
        let glow_radius = (bitmap.height / 18).clamp(2, 24);
        let glow = bitmap.glow(glow_radius);
        Self {
            bitmap,
            glow,
            glow_radius: glow_radius as f32,
            x,
            y,
        }
    }

    fn width(&self) -> f32 {
        self.bitmap.width as f32
    }

    fn height(&self) -> f32 {
        self.bitmap.height as f32
    }

    /// Where screen position `(x, y)` lands on the bitmap at time `t`.
    fn local(&self, idle: Idle, amplitude: f32, x: f32, y: f32, t: f32) -> (f32, f32) {
        let (mut lx, mut ly) = (x - self.x, y - self.y);
        let (cx, cy) = (self.width() / 2.0, self.height() / 2.0);
        match idle {
            Idle::Still => {}
            Idle::Float => ly -= amplitude * (t * 0.9).sin(),
            Idle::Wave => {
                ly -= amplitude * (lx / self.width().max(1.0) * TAU * 0.9 - t * 2.2).sin()
            }
            Idle::Breathe => {
                let scale = 1.0 + 0.035 * (t * 1.2).sin();
                lx = cx + (lx - cx) / scale;
                ly = cy + (ly - cy) / scale;
            }
            Idle::Sway => {
                let (sin, cos) = (0.04 * (t * 0.7).sin()).sin_cos();
                let (dx, dy) = (lx - cx, ly - cy);
                lx = cx + dx * cos + dy * sin;
                ly = cy - dx * sin + dy * cos;
            }
        }
        (lx, ly)
    }
}

/// Which cells hold part of the art, for effects that treat it specially.
pub struct InkMask {
    width: u16,
    cells: Vec<bool>,
}

impl InkMask {
    pub fn contains(&self, pos: Position) -> bool {
        let index = usize::from(pos.y)
            .checked_mul(usize::from(self.width))
            .and_then(|i| i.checked_add(usize::from(pos.x)));
        pos.x < self.width && index.and_then(|i| self.cells.get(i)) == Some(&true)
    }
}

pub struct Shade {
    pub ink: Rgb,
    pub glow: f32,
}

pub struct Stage {
    pub act: Act,
    pub cols: u16,
    pub rows: u16,
    /// A cell's height over its width.
    pub cell_ratio: f32,
    pieces: Vec<Piece>,
    amplitude: f32,
    pub ink: Arc<InkMask>,
}

impl Stage {
    /// `cell_ratio` is a cell's height over its width.
    pub fn new(act: Act, cols: u16, rows: u16, cell_ratio: f32) -> Self {
        let w = f32::from(cols) * num::float(COLS);
        let h = f32::from(rows.saturating_sub(1)) * num::float(ROWS);
        let aspect = cell_ratio * COLS as f32 / ROWS as f32;
        let make = |kind: Kind, box_w: f32, box_h: f32| -> Bitmap {
            let (rw, rh) = act.style.reserve();
            let (bw, bh) = (num::floor_usize(box_w * rw), num::floor_usize(box_h * rh));
            let bitmap = match kind {
                Kind::Logo(Logo::Mascot) => art::mascot().fit(bw, bh, aspect),
                Kind::Logo(Logo::Emblem) => art::emblem().fit(bw, bh, aspect),
                Kind::Word(Word::Small) => art::quadrants(art::LOGO_SMALL).fit(bw, bh, aspect),
                Kind::Word(Word::Tiny) => art::quadrants(art::LOGO_TINY).fit(bw, bh, aspect),
                Kind::Word(Word::Pixel) => art::pixel_font(WORDMARK).fit(bw, bh, aspect),
                Kind::Word(Word::Face(face)) => {
                    fonts::rasterize(&face.font, WORDMARK, bw, bh, aspect)
                }
            };
            act.style.apply(bitmap)
        };
        let centered = |bitmap: Bitmap| {
            let (x, y) = (
                (w - bitmap.width as f32) / 2.0,
                (h - bitmap.height as f32) / 2.0,
            );
            Piece::new(bitmap, x.floor(), y.floor())
        };

        let pieces = match &act.subject {
            Subject::Logo(logo) => {
                vec![centered(make(Kind::Logo(logo.clone()), w * 0.78, h * 0.8))]
            }
            Subject::Word(word) => {
                vec![centered(make(Kind::Word(word.clone()), w * 0.88, h * 0.56))]
            }
            // Side by side on a wide screen, stacked on a tall one.
            Subject::Lockup(logo, word) if w > h * aspect * 1.3 => {
                let logo = make(Kind::Logo(logo.clone()), w * 0.3, h * 0.7);
                let word = make(Kind::Word(word.clone()), w * 0.56, h * 0.42);
                let gap = w * 0.04;
                let total = logo.width as f32 + gap + word.width as f32;
                let left = ((w - total) / 2.0).floor();
                let logo_y = ((h - logo.height as f32) / 2.0).floor();
                let word_y = ((h - word.height as f32) / 2.0).floor();
                let word_x = left + logo.width as f32 + gap;
                vec![
                    Piece::new(logo, left, logo_y),
                    Piece::new(word, word_x.floor(), word_y),
                ]
            }
            Subject::Lockup(logo, word) => {
                let logo = make(Kind::Logo(logo.clone()), w * 0.7, h * 0.5);
                let word = make(Kind::Word(word.clone()), w * 0.85, h * 0.26);
                let gap = h * 0.05;
                let total = logo.height as f32 + gap + word.height as f32;
                let top = ((h - total) / 2.0).floor();
                let logo_x = ((w - logo.width as f32) / 2.0).floor();
                let word_x = ((w - word.width as f32) / 2.0).floor();
                let word_y = top + logo.height as f32 + gap;
                vec![
                    Piece::new(logo, logo_x, top),
                    Piece::new(word, word_x, word_y.floor()),
                ]
            }
        };

        let amplitude = (h * 0.015).max(1.0);
        let mut stage = Self {
            act,
            cols,
            rows,
            cell_ratio,
            pieces,
            amplitude,
            ink: Arc::new(InkMask {
                width: 0,
                cells: vec![],
            }),
        };
        stage.ink = Arc::new(stage.ink_mask());
        stage
    }

    fn ink_mask(&self) -> InkMask {
        let mut cells =
            Vec::with_capacity(usize::from(self.cols).saturating_mul(usize::from(self.rows)));
        for cy in 0..self.rows {
            for cx in 0..self.cols {
                let (x0, y0) = (
                    f32::from(cx) * num::float(COLS),
                    f32::from(cy) * num::float(ROWS),
                );
                cells.push((0..ROWS).any(|row| {
                    (0..COLS).any(|col| {
                        let (x, y) = (x0 + num::float(col) + 0.5, y0 + num::float(row) + 0.5);
                        self.coverage(x, y, 0.0) > 0.5
                    })
                }));
            }
        }
        InkMask {
            width: self.cols,
            cells,
        }
    }

    /// Screen-space box around everything the stage can draw, glow and
    /// motion included: `(left, top, right, bottom)`.
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        self.pieces.iter().fold(
            (f32::MAX, f32::MAX, f32::MIN, f32::MIN),
            |(l, t, r, b), p| {
                let margin = p.glow_radius + self.amplitude * 2.0 + p.height() * 0.05;
                (
                    l.min(p.x - margin),
                    t.min(p.y - margin),
                    r.max(p.x + p.width() + margin),
                    b.max(p.y + p.height() + margin),
                )
            },
        )
    }

    /// How much of the art covers screen position `(x, y)`.
    pub fn coverage(&self, x: f32, y: f32, t: f32) -> f32 {
        self.pieces
            .iter()
            .map(|piece| {
                let (lx, ly) = piece.local(self.act.idle, self.amplitude, x, y, t);
                if lx < 0.0 || ly < 0.0 {
                    return 0.0;
                }
                piece
                    .bitmap
                    .get(num::floor_usize(lx), num::floor_usize(ly))
                    .ink
            })
            .fold(0.0, f32::max)
    }

    /// The art's colour and glow around screen position `(x, y)`.
    pub fn shade(&self, x: f32, y: f32, t: f32, row: usize) -> Shade {
        let theme = self.act.theme();
        let mut best = Shade {
            ink: Rgb::BLACK,
            glow: 0.0,
        };
        let mut best_weight = -1.0;
        for piece in &self.pieces {
            let (lx, ly) = piece.local(self.act.idle, self.amplitude, x, y, t);
            let (gx, gy) = (lx + piece.glow_radius, ly + piece.glow_radius);
            if gx < 0.0 || gy < 0.0 {
                continue;
            }
            let glow = piece
                .glow
                .get(num::floor_usize(gx), num::floor_usize(gy))
                .ink;
            let px = if lx < 0.0 || ly < 0.0 {
                Default::default()
            } else {
                piece.bitmap.get(num::floor_usize(lx), num::floor_usize(ly))
            };
            let weight = px.ink + glow;
            if weight <= best_weight {
                continue;
            }
            best_weight = weight;
            let (u, v) = (lx / piece.width().max(1.0), ly / piece.height().max(1.0));
            let ink = self.act.shader.color(theme, u, v.min(1.0), t, row);
            let color = match px.material {
                Material::Ink => ink,
                Material::Hat => ink.lerp(Rgb::WHITE, 0.75),
                Material::Eye if t.rem_euclid(5.0) < 0.15 => theme.accent,
                Material::Eye => theme.background.scale(2.5),
                Material::Screen => theme.background.lerp(ink, 0.1),
                Material::Border => ink.scale(0.55),
                Material::Cursor if (t * 1.2).fract() < 0.5 => theme.accent,
                Material::Cursor => theme.background.lerp(ink, 0.1),
                Material::Depth(d) => ink.mix(theme.background, 0.45 + 0.35 * f32::from(d) / 255.0),
                Material::Reflection(f) => theme.background.mix(ink, f32::from(f) / 255.0),
            };
            best = Shade { ink: color, glow };
        }
        best
    }
}

enum Kind {
    Logo(Logo),
    Word(Word),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn act(subject: Subject) -> Act {
        Act {
            subject,
            theme: 0,
            shader: Shader::Flow,
            backdrop: Backdrop::Vignette,
            look: Look::Smooth,
            style: Style::Solid,
            idle: Idle::Still,
        }
    }

    #[test]
    fn art_fits_on_screen_for_every_subject_and_size() {
        for (cols, rows) in [(40, 12), (80, 24), (160, 48), (300, 90), (60, 70)] {
            for subject in [
                Subject::Logo(Logo::Mascot),
                Subject::Logo(Logo::Emblem),
                Subject::Word(Word::Small),
                Subject::Lockup(Logo::Mascot, Word::Pixel),
            ] {
                let stage = Stage::new(act(subject), cols, rows, 2.0);
                for piece in &stage.pieces {
                    assert!(piece.x >= 0.0 && piece.y >= 0.0, "{cols}x{rows}");
                    assert!(
                        piece.x + piece.width() <= f32::from(cols) * num::float(COLS),
                        "{cols}x{rows}"
                    );
                    assert!(
                        piece.y + piece.height() <= f32::from(rows) * num::float(ROWS),
                        "{cols}x{rows}"
                    );
                }
            }
        }
    }
}
