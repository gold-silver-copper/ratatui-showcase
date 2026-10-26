//! What a scene shows, and that scene laid out for the current terminal.
//!
//! Positions are in sub-cell samples: each cell is `COLS` x `ROWS` of them.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ratatui::layout::Position;

use crate::art::{self, Bitmap, Material, Style};
use crate::backdrop::{BACKDROPS, Backdrop};
use crate::fonts::{self, Face};
use crate::glyphs::{COLS, ROWS};
use crate::num;
use crate::params::{BLOCK, PIECE_LEN, S};
use crate::theme::{Palette, SHADERS, Shader};
use crate::widgets::{self, Panel};

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
    /// Each letter cut from a different font, like a ransom note.
    Ransom(Vec<Face>),
}

#[derive(Clone)]
pub enum Subject {
    Logo(Logo),
    Word(Word),
    /// Logo and wordmark together.
    Lockup(Logo, Word),
    /// A phrase about ratatui, in a font.
    Phrase(&'static str, Word),
    /// Real ratatui widgets, with data from a seed.
    Widget(Panel, u64),
}

/// Phrases a scene can spell out instead of the wordmark.
pub const PHRASES: &[&str] = &[
    "terminal UIs in Rust",
    "cargo add ratatui",
    "ratatui.rs",
    "cooking up TUIs",
    "widgets  layouts  style",
    "immediate mode",
    "tachyonfx",
    "bevy_ratatui",
    "made with ratatui",
];

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
    /// ASCII characters matched to each cell's shape.
    Shapes,
    /// Falling code, lit by the picture.
    Code,
    /// Contour lines, like a map's, in box-drawing characters.
    Contour,
    /// Printed halftone dots, bigger where it is brighter.
    Halftone,
    /// The picture spelled out in this project's README.
    Text,
}

pub const LOOKS: &[Look] = &[
    Look::Smooth,
    Look::Smooth,
    Look::Smooth,
    Look::Smooth,
    Look::Pixels,
    Look::Braille,
    Look::Braille,
    Look::Ascii,
    Look::Led,
    Look::Shapes,
    Look::Code,
    Look::Contour,
    Look::Halftone,
    Look::Text,
];

impl Look {
    /// Every look once, in the shader's order (`L_*`).
    pub const ALL: &[Look] = &[
        Look::Smooth,
        Look::Pixels,
        Look::Braille,
        Look::Ascii,
        Look::Led,
        Look::Shapes,
        Look::Code,
        Look::Contour,
        Look::Halftone,
        Look::Text,
    ];

    pub fn code(self) -> u32 {
        code(Self::ALL, self)
    }

    pub fn name(self) -> &'static str {
        match self {
            Look::Smooth => "smooth",
            Look::Pixels => "pixels",
            Look::Braille => "braille",
            Look::Ascii => "ascii",
            Look::Led => "led",
            Look::Shapes => "shapes",
            Look::Code => "code",
            Look::Contour => "contour",
            Look::Halftone => "halftone",
            Look::Text => "text",
        }
    }
}

/// How a scene draws its art.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Flat, in its style.
    Flat,
    /// A solid, turning in 3D.
    Turntable,
    /// Dye in a swirling fluid.
    Fluid,
    /// Coral growing by reaction-diffusion.
    Reaction,
    /// The Game of Life, seeded by the letters.
    Life,
    /// Burning.
    Fire,
    /// Under rippling water.
    Ripple,
    /// A swarm of particles.
    Swarm,
    /// Slime mold: agents follow each other's trails into the letters.
    Slime,
    /// A flock that gathers into the letters.
    Boids,
    /// The letters crumble into falling sand.
    Sand,
    /// Crystals branch out until they fill the letters.
    Crystal,
    /// Ink rising from the letters and curling through water.
    Ink,
}

pub const MODES: &[Mode] = &[
    Mode::Flat,
    Mode::Flat,
    Mode::Flat,
    Mode::Flat,
    Mode::Turntable,
    Mode::Turntable,
    Mode::Fluid,
    Mode::Reaction,
    Mode::Life,
    Mode::Fire,
    Mode::Ripple,
    Mode::Swarm,
    Mode::Slime,
    Mode::Boids,
    Mode::Sand,
    Mode::Crystal,
    Mode::Ink,
];

impl Mode {
    /// Every mode once, in the shader's order (`M_*`).
    pub const ALL: &[Mode] = &[
        Mode::Flat,
        Mode::Turntable,
        Mode::Fluid,
        Mode::Reaction,
        Mode::Life,
        Mode::Fire,
        Mode::Ripple,
        Mode::Swarm,
        Mode::Slime,
        Mode::Boids,
        Mode::Sand,
        Mode::Crystal,
        Mode::Ink,
    ];

    pub fn code(self) -> u32 {
        code(Self::ALL, self)
    }

    /// Whether the mode flies particles around the art (in `Region::Particles`).
    pub fn flocks(self) -> bool {
        matches!(self, Mode::Swarm | Mode::Boids)
    }

    /// Whether the mode runs a simulation on the grid.
    pub fn simulates(self) -> bool {
        match self {
            Mode::Fluid
            | Mode::Reaction
            | Mode::Life
            | Mode::Fire
            | Mode::Ripple
            | Mode::Slime
            | Mode::Sand
            | Mode::Crystal
            | Mode::Ink => true,
            Mode::Flat | Mode::Turntable | Mode::Swarm | Mode::Boids => false,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Mode::Flat => "flat",
            Mode::Turntable => "3d",
            Mode::Fluid => "fluid",
            Mode::Reaction => "coral",
            Mode::Life => "life",
            Mode::Fire => "fire",
            Mode::Ripple => "water",
            Mode::Swarm => "swarm",
            Mode::Slime => "slime mold",
            Mode::Boids => "boids",
            Mode::Sand => "sand",
            Mode::Crystal => "crystal",
            Mode::Ink => "ink",
        }
    }
}

/// Post-processing over the whole frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Post {
    Clean,
    Bloom,
    /// Moving things leave trails.
    Trails,
    /// Each frame echoes inward, like a tunnel of itself.
    Echo,
    /// Colour fringes toward the edges, like a cheap lens.
    Chroma,
    /// A curved, glowing CRT.
    Crt,
    /// Film grain.
    Grain,
    /// Shafts of light streaming out from the art.
    Rays,
}

pub const POSTS: &[Post] = &[
    Post::Clean,
    Post::Clean,
    Post::Bloom,
    Post::Bloom,
    Post::Trails,
    Post::Echo,
    Post::Chroma,
    Post::Crt,
    Post::Grain,
    Post::Rays,
    Post::Rays,
];

impl Post {
    /// Every effect once, in the shader's order (`P_*`).
    pub const ALL: &[Post] = &[
        Post::Clean,
        Post::Bloom,
        Post::Trails,
        Post::Echo,
        Post::Chroma,
        Post::Crt,
        Post::Grain,
        Post::Rays,
    ];

    pub fn code(self) -> u32 {
        code(Self::ALL, self)
    }

    /// Whether the effect adds bloom, so the bloom passes must run.
    pub fn blooms(self) -> bool {
        self != Post::Clean
    }

    pub fn name(self) -> &'static str {
        match self {
            Post::Clean => "clean",
            Post::Bloom => "bloom",
            Post::Trails => "trails",
            Post::Echo => "echo",
            Post::Chroma => "chroma",
            Post::Crt => "crt",
            Post::Grain => "grain",
            Post::Rays => "rays",
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
    /// Letters hop one after another.
    Bounce,
    /// Letters drop into place one after another as the scene starts.
    Arrive,
    /// The art scrolls across the screen like a ticker.
    Marquee,
}

impl Idle {
    pub fn name(self) -> &'static str {
        match self {
            Idle::Still => "still",
            Idle::Float => "float",
            Idle::Wave => "wave",
            Idle::Breathe => "breathe",
            Idle::Sway => "sway",
            Idle::Bounce => "bounce",
            Idle::Arrive => "arrive",
            Idle::Marquee => "marquee",
        }
    }
}

pub const IDLES: &[Idle] = &[
    Idle::Still,
    Idle::Float,
    Idle::Wave,
    Idle::Breathe,
    Idle::Sway,
    Idle::Bounce,
    Idle::Arrive,
    Idle::Marquee,
];

/// Everything that makes one scene look the way it does.
#[derive(Clone)]
pub struct Act {
    pub subject: Subject,
    pub palette: Palette,
    pub shader: Shader,
    pub backdrop: Backdrop,
    pub look: Look,
    pub style: Style,
    pub idle: Idle,
    pub mode: Mode,
    pub post: Post,
    /// Picks among a mode's or background's presets.
    pub variant: u32,
}

impl Act {
    pub fn theme(&self) -> &Palette {
        &self.palette
    }

    pub fn label(&self) -> String {
        let name = |word: &Word| match word {
            Word::Small => "RatatuiLogo::small()".to_owned(),
            Word::Tiny => "RatatuiLogo::tiny()".to_owned(),
            Word::Pixel => "font8x8".to_owned(),
            Word::Face(face) => face.name.clone(),
            Word::Ransom(_) => "ransom note".to_owned(),
        };
        let logo = |logo: &Logo| match logo {
            Logo::Mascot => "RatatuiMascot",
            Logo::Emblem => "logo.svg",
        };
        match &self.subject {
            Subject::Logo(l) => logo(l).to_owned(),
            Subject::Word(w) => name(w),
            Subject::Lockup(l, w) => format!("{} + {}", logo(l), name(w)),
            Subject::Phrase(text, w) => format!("\"{text}\" in {}", name(w)),
            Subject::Widget(panel, _) => panel.name().to_owned(),
        }
    }
}

struct Piece {
    bitmap: Bitmap,
    /// How far the glow reaches, in samples.
    glow_radius: f32,
    x: f32,
    y: f32,
}

impl Piece {
    fn new(bitmap: Bitmap, x: f32, y: f32) -> Self {
        let glow_radius = num::float((bitmap.height / 10).clamp(3, 48));
        Self {
            bitmap,
            glow_radius,
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
}

/// Which letter each column of `bitmap` belongs to: letters are runs of
/// columns with ink, split by empty ones.
fn letters(bitmap: &Bitmap) -> Vec<u8> {
    let mut letter = 0_u8;
    let mut inside = false;
    (0..bitmap.width)
        .map(|x| {
            let ink = (0..bitmap.height).any(|y| bitmap.get(x, y).ink > 0.5);
            if ink && !inside {
                letter = letter.wrapping_add(1);
            }
            inside = ink;
            letter
        })
        .collect()
}

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
                Kind::Word(Word::Ransom(faces)) => fonts::ransom(&faces, WORDMARK, bw, bh, aspect),
                Kind::Phrase(text, Word::Face(face)) => {
                    fonts::rasterize(&face.font, text, bw, bh, aspect)
                }
                Kind::Phrase(text, Word::Ransom(faces)) => {
                    fonts::ransom(&faces, text, bw, bh, aspect)
                }
                Kind::Phrase(text, Word::Small | Word::Tiny | Word::Pixel) => {
                    art::pixel_font(text).fit(bw, bh, aspect)
                }
                Kind::Widget(panel, seed) => widgets::render(panel, seed).fit(bw, bh, aspect),
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
            Subject::Phrase(text, word) => {
                vec![centered(make(
                    Kind::Phrase(text, word.clone()),
                    w * 0.92,
                    h * 0.42,
                ))]
            }
            Subject::Widget(panel, seed) => {
                vec![centered(make(
                    Kind::Widget(*panel, *seed),
                    w * 0.82,
                    h * 0.84,
                ))]
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
                        self.coverage(x, y) > 0.5
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

    /// How much of the art at rest covers screen position `(x, y)`. The GPU
    /// moves it while it holds (`art.wgsl`).
    pub fn coverage(&self, x: f32, y: f32) -> f32 {
        self.pieces
            .iter()
            .map(|piece| {
                let (lx, ly) = (x - piece.x, y - piece.y);
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

    /// The art for the GPU: each piece's pixels in turn, with ink in bits
    /// 0-7, material in 8-15, its parameter in 16-23 and which letter the
    /// column belongs to in 24-31.
    pub fn texels(&self) -> Vec<u32> {
        let mut texels = Vec::with_capacity(self.texels_len());
        for piece in &self.pieces {
            let letters = letters(&piece.bitmap);
            for y in 0..piece.bitmap.height {
                for (x, &letter) in (0..piece.bitmap.width).zip(&letters) {
                    let px = piece.bitmap.get(x, y);
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
                    texels.push(
                        ink | material << 8 | u32::from(param) << 16 | u32::from(letter) << 24,
                    );
                }
            }
        }
        texels
    }

    pub fn texels_len(&self) -> usize {
        self.pieces
            .iter()
            .map(|p| p.bitmap.width.saturating_mul(p.bitmap.height))
            .sum()
    }

    /// The screen's width over its height.
    pub fn screen_aspect(&self) -> f32 {
        f32::from(self.cols) / (f32::from(self.rows.max(1)) * self.cell_ratio)
    }

    /// This scene's settings for the GPU (`params::S`), with its art
    /// starting at `art` in the art section and `age` seconds since it began.
    pub fn block(&self, art: u32, age: f32) -> [u32; BLOCK] {
        let mut block = [0; BLOCK];
        let theme = self.act.theme();
        let mut put = |at: usize, value: u32| {
            if let Some(slot) = block.get_mut(at) {
                *slot = value;
            }
        };
        put(S::Shader.at(), code(SHADERS, self.act.shader));
        put(S::Backdrop.at(), code(BACKDROPS, self.act.backdrop));
        put(S::Idle.at(), code(IDLES, self.act.idle));
        put(S::Amplitude.at(), self.amplitude.to_bits());
        put(
            S::Pieces.at(),
            num::u32_of(self.pieces.len().min(MAX_PIECES)),
        );
        put(S::Stops.at(), num::u32_of(theme.stops.len().min(MAX_STOPS)));
        put(S::Mode.at(), self.act.mode.code());
        put(S::Style.at(), self.act.style.code());
        let colors = [theme.background, theme.accent]
            .into_iter()
            .chain(theme.stops.iter().take(MAX_STOPS).copied());
        let channels = colors.flat_map(|c| [c.0, c.1, c.2]);
        for (i, v) in channels.enumerate() {
            put(S::Background.at().saturating_add(i), v.to_bits());
        }
        let tallest = self
            .pieces
            .iter()
            .map(|p| p.bitmap.height)
            .max()
            .unwrap_or(0);
        put(S::Age.at(), age.to_bits());
        put(S::Variant.at(), self.act.variant);
        put(
            S::Width.at(),
            num::float((tallest / 14).clamp(2, 24)).to_bits(),
        );
        put(
            S::Glow.at(),
            self.pieces
                .iter()
                .map(|p| p.glow_radius)
                .fold(1.0, f32::max)
                .to_bits(),
        );
        let mut offset = art;
        for (i, piece) in self.pieces.iter().take(MAX_PIECES).enumerate() {
            let at = S::Piece0.at().saturating_add(i.saturating_mul(PIECE_LEN));
            let (w, h) = (
                num::u32_of(piece.bitmap.width),
                num::u32_of(piece.bitmap.height),
            );
            let fields = [
                piece.x.to_bits(),
                piece.y.to_bits(),
                w,
                h,
                piece.glow_radius.to_bits(),
                offset,
            ];
            for (j, value) in fields.into_iter().enumerate() {
                put(at.saturating_add(j), value);
            }
            offset = offset.saturating_add(w.saturating_mul(h));
        }
        block
    }
}

enum Kind {
    Logo(Logo),
    Word(Word),
    Phrase(&'static str, Word),
    Widget(Panel, u64),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn act(subject: Subject) -> Act {
        Act {
            subject,
            palette: Palette::generated(),
            shader: Shader::Flow,
            backdrop: Backdrop::Vignette,
            look: Look::Smooth,
            style: Style::Solid,
            idle: Idle::Still,
            mode: Mode::Flat,
            post: Post::Clean,
            variant: 0,
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
