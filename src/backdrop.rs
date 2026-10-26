//! Animated backgrounds behind the art. Kept dim so the art stays the star.

use crate::num;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backdrop {
    /// A soft glow in the middle.
    Vignette,
    /// Twinkling stars.
    Stars,
    /// A synthwave grid racing toward a glowing horizon.
    Grid,
    /// Slow curtains of light.
    Aurora,
    /// Sunbeams turning slowly around the centre.
    Rays,
    /// A Julia set drifting through its family.
    Julia,
    /// A zoom into the Mandelbrot set's seahorse valley.
    Mandelbrot,
    /// The Burning Ship fractal.
    Ship,
    /// Newton's fractal for z^3 = 1, turning.
    Newton,
    /// Mirrored noise in eight segments.
    Kaleidoscope,
    /// A checkered tunnel rushing past.
    Tunnel,
    /// Stars streaking past at warp speed.
    Warp,
    /// A ray-marched Mandelbulb, slowly orbited.
    Mandelbulb,
    /// Flying through an endless Menger sponge.
    Menger,
    /// Flying low over ray-marched hills.
    Terrain,
    /// Glowing blobs that merge and split.
    Metaballs,
    /// Stained glass: drifting Voronoi cells with bright leading.
    Voronoi,
    /// Domain-warped noise, like marbled paper.
    Marble,
    /// Circles packed in circles, zooming in.
    Apollonian,
    /// The limit set of a Kleinian group in the Maskit slice, by Jos Leys'
    /// escape-time algorithm.
    Kleinian,
    /// A hyperbolic tiling of the Poincare disk, gliding.
    Hyperbolic,
    /// A complex function's domain colouring: phase as colour, contours of
    /// its size.
    Complex,
    /// A ray-marched Mandelbox.
    Mandelbox,
    /// A kaleidoscopic IFS: space folded through a polyhedron's mirrors.
    Kifs,
    /// A fractal flame of millions of points.
    Flame,
    /// The art as an iterated function system of itself (Barnsley's collage
    /// theorem): copies of the art make up the art, each turning slowly.
    Collage,
    /// The Clifford attractor, drifting.
    Clifford,
    /// The de Jong attractor, drifting.
    DeJong,
    /// The Lorenz butterfly, turning.
    Lorenz,
    /// Particle Life: kinds of particles that chase and flee each other.
    ParticleLife,
}

/// In the shader's order (`backdrop.wgsl`).
pub const BACKDROPS: &[Backdrop] = &[
    Backdrop::Vignette,
    Backdrop::Stars,
    Backdrop::Grid,
    Backdrop::Aurora,
    Backdrop::Rays,
    Backdrop::Julia,
    Backdrop::Mandelbrot,
    Backdrop::Ship,
    Backdrop::Newton,
    Backdrop::Kaleidoscope,
    Backdrop::Tunnel,
    Backdrop::Warp,
    Backdrop::Mandelbulb,
    Backdrop::Menger,
    Backdrop::Terrain,
    Backdrop::Metaballs,
    Backdrop::Voronoi,
    Backdrop::Marble,
    Backdrop::Apollonian,
    Backdrop::Kleinian,
    Backdrop::Hyperbolic,
    Backdrop::Complex,
    Backdrop::Mandelbox,
    Backdrop::Kifs,
    Backdrop::Flame,
    Backdrop::Collage,
    Backdrop::Clifford,
    Backdrop::DeJong,
    Backdrop::Lorenz,
    Backdrop::ParticleLife,
];

impl Backdrop {
    /// Drawn by splatting points: flames and attractors.
    pub fn splatted(self) -> bool {
        matches!(
            self,
            Backdrop::Flame
                | Backdrop::Collage
                | Backdrop::Clifford
                | Backdrop::DeJong
                | Backdrop::Lorenz
                | Backdrop::ParticleLife
        )
    }

    pub fn name(self) -> &'static str {
        match self {
            Backdrop::Vignette => "vignette",
            Backdrop::Stars => "stars",
            Backdrop::Grid => "grid",
            Backdrop::Aurora => "aurora",
            Backdrop::Rays => "rays",
            Backdrop::Julia => "julia",
            Backdrop::Mandelbrot => "mandelbrot",
            Backdrop::Ship => "burning ship",
            Backdrop::Newton => "newton",
            Backdrop::Kaleidoscope => "kaleidoscope",
            Backdrop::Tunnel => "tunnel",
            Backdrop::Warp => "warp",
            Backdrop::Mandelbulb => "mandelbulb",
            Backdrop::Menger => "menger",
            Backdrop::Terrain => "terrain",
            Backdrop::Metaballs => "metaballs",
            Backdrop::Voronoi => "stained glass",
            Backdrop::Marble => "marbling",
            Backdrop::Apollonian => "apollonian",
            Backdrop::Kleinian => "kleinian",
            Backdrop::Hyperbolic => "hyperbolic",
            Backdrop::Complex => "domain colouring",
            Backdrop::Mandelbox => "mandelbox",
            Backdrop::Kifs => "kaleidoscopic ifs",
            Backdrop::Collage => "collage",
            Backdrop::Flame => "flame",
            Backdrop::Clifford => "clifford",
            Backdrop::DeJong => "de jong",
            Backdrop::Lorenz => "lorenz",
            Backdrop::ParticleLife => "particle life",
        }
    }
}

/// Stable pseudo-random `0.0..1.0`.
pub fn hash(x: u32, y: u32) -> f32 {
    let mut h = x.wrapping_mul(0x9e37_79b1) ^ y.wrapping_mul(0x85eb_ca77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    (h & 0xff_ffff) as f32 / 0x100_0000 as f32
}

/// Where the Mandelbrot zoom heads: deep in seahorse valley.
const CENTRE: (f64, f64) = (-0.743_643_887_037_158_7, 0.131_825_904_205_311_97);

/// The orbit of the zoom's centre, in double precision, as `re, im` f32
/// pairs. Each pixel then only follows its tiny difference from it, which
/// f32 can hold far deeper than it could hold the pixel's own position.
pub fn reference_orbit() -> Vec<u32> {
    let (cr, ci) = CENTRE;
    let (mut zr, mut zi) = (0.0_f64, 0.0_f64);
    let mut orbit = Vec::with_capacity(12_000);
    for _ in 0..6000 {
        orbit.extend([num::narrow(zr).to_bits(), num::narrow(zi).to_bits()]);
        let next = (zr * zr - zi * zi + cr, 2.0 * zr * zi + ci);
        (zr, zi) = next;
        if zr * zr + zi * zi > 16.0 {
            break;
        }
    }
    orbit
}
