//! What a scene shows, and that scene laid out for the current terminal.
//!
//! Positions are in sub-cell samples: each cell is `COLS` x `ROWS` of them.

use std::f32::consts::TAU;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ratatui::layout::Position;

use crate::art::{self, Bitmap, Material, Px, Style};
use crate::backdrop::{BACKDROPS, Backdrop};
use crate::fonts::{self, Face};
use crate::glyphs::{COLS, ROWS};
use crate::num;
use crate::theme::{Rgb, SHADERS, Shader, THEMES, Theme};

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
    /// The look's number in the shader.
    pub fn code(self) -> u32 {
        match self {
            Look::Smooth => 0,
            Look::Pixels => 1,
            Look::Braille => 2,
            Look::Ascii => 3,
            Look::Led => 4,
        }
    }

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
/// Words in a scene's settings block for the GPU.
pub const BLOCK: usize = 64;
const MAX_PIECES: usize = 2;
const MAX_STOPS: usize = 5;

/// An option's position in its list, which the shader switches on.
fn code<T: PartialEq>(list: &[T], item: T) -> u32 {
    num::u32_of(list.iter().position(|i| *i == item).unwrap_or(0))
}

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

pub struct Stage {
    /// Unique per stage, to tell when the GPU needs new art.
    pub id: u64,
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
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let mut stage = Self {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
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

    /// The art for the GPU: per pixel of each piece's glow-padded grid, its
    /// ink (bits 0-7), material (8-15), material parameter (16-23) and glow
    /// (24-31). Pieces follow each other.
    pub fn texels(&self) -> Vec<u32> {
        let mut texels = Vec::new();
        for piece in &self.pieces {
            let r = num::floor_usize(piece.glow_radius);
            for gy in 0..piece.glow.height {
                for gx in 0..piece.glow.width {
                    let px = match (gx.checked_sub(r), gy.checked_sub(r)) {
                        (Some(x), Some(y)) => piece.bitmap.get(x, y),
                        _ => Px::default(),
                    };
                    let (material, param) = match px.material {
                        Material::Ink => (0, 0),
                        Material::Hat => (1, 0),
                        Material::Eye => (2, 0),
                        Material::Screen => (3, 0),
                        Material::Border => (4, 0),
                        Material::Cursor => (5, 0),
                        Material::Depth(d) => (6, d),
                        Material::Reflection(f) => (7, f),
                    };
                    let ink = u32::from(num::round_u8(px.ink * 255.0));
                    let glow = u32::from(num::round_u8(piece.glow.get(gx, gy).ink * 255.0));
                    texels.push(ink | material << 8 | u32::from(param) << 16 | glow << 24);
                }
            }
        }
        texels
    }

    pub fn texels_len(&self) -> usize {
        self.pieces
            .iter()
            .map(|p| p.glow.width.saturating_mul(p.glow.height))
            .sum()
    }

    /// The screen's width over its height.
    pub fn screen_aspect(&self) -> f32 {
        f32::from(self.cols) / (f32::from(self.rows.max(1)) * self.cell_ratio)
    }

    /// This scene's settings for the GPU (see `render.wgsl`), with its art
    /// starting at `art` in the art buffer.
    pub fn block(&self, art: u32) -> [u32; BLOCK] {
        let mut block = [0; BLOCK];
        let theme = self.act.theme();
        let put = |block: &mut [u32; BLOCK], at: usize, value: u32| {
            if let Some(slot) = block.get_mut(at) {
                *slot = value;
            }
        };
        let rgb = |block: &mut [u32; BLOCK], at: usize, c: Rgb| {
            for (i, v) in [c.0, c.1, c.2].into_iter().enumerate() {
                put(block, at.saturating_add(i), v.to_bits());
            }
        };
        put(&mut block, 0, code(SHADERS, self.act.shader));
        put(&mut block, 1, code(BACKDROPS, self.act.backdrop));
        put(&mut block, 2, code(IDLES, self.act.idle));
        put(&mut block, 3, self.amplitude.to_bits());
        put(
            &mut block,
            4,
            num::u32_of(self.pieces.len().min(MAX_PIECES)),
        );
        put(&mut block, 5, num::u32_of(theme.stops.len().min(MAX_STOPS)));
        rgb(&mut block, 8, theme.background);
        rgb(&mut block, 11, theme.accent);
        for (i, &stop) in theme.stops.iter().take(MAX_STOPS).enumerate() {
            rgb(
                &mut block,
                14_usize.saturating_add(i.saturating_mul(3)),
                Rgb::hex(stop),
            );
        }
        let mut offset = art;
        for (i, piece) in self.pieces.iter().take(MAX_PIECES).enumerate() {
            let at = 32_usize.saturating_add(i.saturating_mul(8));
            let fields = [
                piece.x.to_bits(),
                piece.y.to_bits(),
                num::u32_of(piece.bitmap.width),
                num::u32_of(piece.bitmap.height),
                num::floor_u32(piece.glow_radius),
                offset,
                num::u32_of(piece.glow.width),
                num::u32_of(piece.glow.height),
            ];
            for (j, value) in fields.into_iter().enumerate() {
                put(&mut block, at.saturating_add(j), value);
            }
            offset = offset.saturating_add(num::u32_of(
                piece.glow.width.saturating_mul(piece.glow.height),
            ));
        }
        block
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
