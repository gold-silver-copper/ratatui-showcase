//! Full-screen transitions from one scene to the next. The GPU draws them
//! (`render.wgsl`); this side keeps time and finds the particles: masked wipes,
//! warps of the whole frame, and particles that carry the old art's pixels to
//! the new art's.

use crate::backdrop::hash;
use crate::glyphs::{COLS, ROWS};
use crate::num;
use crate::stage::Stage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Radial,
    Diamond,
    Clock,
    Spiral,
    Diagonal,
    Blinds,
    Checker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The new scene spreads over the old behind a glowing edge.
    Wipe(Shape),
    /// The old scene burns away in noisy flames.
    Burn,
    /// Fly through the old scene; the new one springs up from afar.
    Zoom,
    /// Everything twists into a whirlpool and back out.
    Swirl,
    /// Rings of waves ripple out from the centre.
    Ripple,
    /// The frame breaks into ever bigger blocks and back.
    Mosaic,
    /// Bands slide off one way and the new ones slide in.
    Slices,
    /// A broken signal: torn rows, split colours, static.
    Glitch,
    /// An old television switching off and on.
    Crt,
    /// Every pixel of the old art flows to a place in the new.
    Morph,
    /// Pixels swarm around the screen before settling.
    Swarm,
    /// The old art explodes; the new flies in from all sides.
    Scatter,
    /// The old art falls away; the new pours in row by row.
    Pour,
    /// Into a black hole and out again.
    Vortex,
    /// The old art crumbles like sand; the new rises up.
    Crumble,
    /// The old shape melts into the new one.
    Melt,
}

pub const KINDS: &[Kind] = &[
    Kind::Wipe(Shape::Radial),
    Kind::Wipe(Shape::Diamond),
    Kind::Wipe(Shape::Clock),
    Kind::Wipe(Shape::Spiral),
    Kind::Wipe(Shape::Diagonal),
    Kind::Wipe(Shape::Blinds),
    Kind::Wipe(Shape::Checker),
    Kind::Burn,
    Kind::Zoom,
    Kind::Swirl,
    Kind::Ripple,
    Kind::Mosaic,
    Kind::Slices,
    Kind::Glitch,
    Kind::Crt,
    Kind::Morph,
    Kind::Swarm,
    Kind::Scatter,
    Kind::Pour,
    Kind::Vortex,
    Kind::Crumble,
    Kind::Melt,
    Kind::Melt,
];

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Wipe(Shape::Radial) => "radial wipe",
            Kind::Wipe(Shape::Diamond) => "diamond wipe",
            Kind::Wipe(Shape::Clock) => "clock wipe",
            Kind::Wipe(Shape::Spiral) => "spiral wipe",
            Kind::Wipe(Shape::Diagonal) => "diagonal wipe",
            Kind::Wipe(Shape::Blinds) => "blinds",
            Kind::Wipe(Shape::Checker) => "checkerboard",
            Kind::Burn => "burn",
            Kind::Zoom => "zoom",
            Kind::Swirl => "swirl",
            Kind::Ripple => "ripple",
            Kind::Mosaic => "mosaic",
            Kind::Slices => "slices",
            Kind::Glitch => "glitch",
            Kind::Crt => "crt",
            Kind::Morph => "morph",
            Kind::Swarm => "swarm",
            Kind::Scatter => "scatter",
            Kind::Pour => "pour",
            Kind::Vortex => "vortex",
            Kind::Crumble => "crumble",
            Kind::Melt => "melt",
        }
    }

    fn duration(self) -> f32 {
        match self {
            Kind::Wipe(_) => 1.8,
            Kind::Glitch | Kind::Crt => 1.6,
            Kind::Zoom | Kind::Mosaic | Kind::Slices => 1.9,
            Kind::Burn | Kind::Swirl | Kind::Ripple => 2.2,
            Kind::Morph | Kind::Vortex | Kind::Scatter => 2.8,
            Kind::Swarm | Kind::Pour | Kind::Crumble => 3.2,
            Kind::Melt => 2.6,
        }
    }

    /// The kind's number in the shader: 0 is a scene at rest.
    pub fn code(self) -> u32 {
        let index = KINDS.iter().position(|&k| k == self).unwrap_or(0);
        num::u32_of(index.saturating_add(1))
    }

    fn particles(self) -> bool {
        matches!(
            self,
            Kind::Morph | Kind::Swarm | Kind::Scatter | Kind::Pour | Kind::Vortex | Kind::Crumble
        )
    }
}

#[derive(Clone, Copy)]
struct Point {
    x: f32,
    y: f32,
    seed: f32,
}

pub struct Crossing {
    pub kind: Kind,
    pub from: Stage,
    elapsed: f32,
    /// Particle size in samples.
    pub grain: f32,
    sources: Vec<Point>,
    targets: Vec<Point>,
}

/// Particles per scene; more cost time, fewer look coarse.
const MAX_PARTICLES: usize = 24_000;

impl Crossing {
    pub fn new(kind: Kind, from: Stage, to: &Stage) -> Self {
        let mut grain = 2.0;
        let (mut sources, mut targets) = (Vec::new(), Vec::new());
        if kind.particles() {
            loop {
                sources = points(&from, grain);
                targets = points(to, grain);
                if sources.len().max(targets.len()) <= MAX_PARTICLES {
                    break;
                }
                grain += 1.0;
            }
            // Morphs pair pixels in reading order, so shapes flow sideways
            // into each other; everything else pairs at random.
            let key = |p: &Point| (num::floor_u32(p.x / grain), num::floor_u32(p.y / grain));
            if kind == Kind::Morph {
                sources.sort_by_key(key);
                targets.sort_by_key(key);
            }
        }
        Self {
            kind,
            from,
            elapsed: 0.0,
            grain,
            sources,
            targets,
        }
    }

    pub fn advance(&mut self, dt: f32) {
        self.elapsed += dt;
    }

    pub fn done(&self) -> bool {
        self.elapsed >= self.kind.duration()
    }

    pub fn progress(&self) -> f32 {
        (self.elapsed / self.kind.duration()).clamp(0.0, 1.0)
    }

    pub fn counts(&self) -> (u32, u32) {
        (
            num::u32_of(self.sources.len()),
            num::u32_of(self.targets.len()),
        )
    }

    /// The particles for the GPU, sources then targets, as `x, y, seed`; and
    /// how many threads move them.
    pub fn particles(&self) -> (Vec<u32>, u32) {
        let data = self
            .sources
            .iter()
            .chain(&self.targets)
            .flat_map(|p| [p.x, p.y, p.seed].map(f32::to_bits))
            .collect();
        let (sources, targets) = self.counts();
        let threads = match self.kind {
            Kind::Morph | Kind::Swarm => sources.max(targets),
            Kind::Wipe(_)
            | Kind::Burn
            | Kind::Zoom
            | Kind::Swirl
            | Kind::Ripple
            | Kind::Mosaic
            | Kind::Slices
            | Kind::Glitch
            | Kind::Crt
            | Kind::Scatter
            | Kind::Pour
            | Kind::Vortex
            | Kind::Crumble
            | Kind::Melt => sources.saturating_add(targets),
        };
        (data, threads)
    }
}

/// Places in `stage`'s art for up to `max` swarm particles to settle, as
/// `x, y, seed` floats.
pub fn homes(stage: &Stage, max: usize) -> Vec<u32> {
    let mut grain = 2.0;
    loop {
        let found = points(stage, grain);
        if found.len() <= max || grain > 64.0 {
            return found
                .iter()
                .flat_map(|p| [p.x, p.y, p.seed].map(f32::to_bits))
                .collect();
        }
        grain += 1.0;
    }
}

/// The art's pixels, `grain` samples apart.
fn points(stage: &Stage, grain: f32) -> Vec<Point> {
    let (left, top, right, bottom) = stage.bounds();
    let width = num::float(usize::from(stage.cols).saturating_mul(COLS));
    let height = num::float(usize::from(stage.rows).saturating_mul(ROWS));
    let mut points = Vec::new();
    let mut y = top.max(0.0);
    while y < bottom.min(height) {
        let mut x = left.max(0.0);
        while x < right.min(width) {
            if stage.coverage(x + grain / 2.0, y + grain / 2.0, 0.0) > 0.5 {
                let seed = hash(num::floor_u32(x), num::floor_u32(y).wrapping_add(17));
                points.push(Point {
                    x: x + grain / 2.0,
                    y: y + grain / 2.0,
                    seed,
                });
            }
            x += grain;
        }
        y += grain;
    }
    points
}
