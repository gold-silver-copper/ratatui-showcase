//! Animated backgrounds behind the art. Kept dim so the art stays the star.

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
];

impl Backdrop {
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
