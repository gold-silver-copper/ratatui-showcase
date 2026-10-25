//! Animated backgrounds behind the art. Kept dim so the art stays the star.

use std::f32::consts::TAU;

use crate::num;
use crate::theme::{Rgb, Theme};

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
}

pub const BACKDROPS: &[Backdrop] = &[
    Backdrop::Vignette,
    Backdrop::Stars,
    Backdrop::Grid,
    Backdrop::Aurora,
    Backdrop::Rays,
];

/// A screen position: `(x, y)` in `0.0..1.0`, and `(px, py)` the half-cell
/// pixel there.
#[derive(Clone, Copy)]
pub struct Place {
    pub x: f32,
    pub y: f32,
    pub px: usize,
    pub py: usize,
}

impl Backdrop {
    pub fn name(self) -> &'static str {
        match self {
            Backdrop::Vignette => "vignette",
            Backdrop::Stars => "stars",
            Backdrop::Grid => "grid",
            Backdrop::Aurora => "aurora",
            Backdrop::Rays => "rays",
        }
    }

    /// Colour at `place`; `aspect` is the screen's width over its height.
    pub fn color(self, theme: &Theme, place: Place, aspect: f32, t: f32) -> Rgb {
        let Place { x, y, px, py } = place;
        let (dx, dy) = (x - 0.5, y - 0.5);
        let vignette = theme.background.scale(1.6 - 1.8 * (dx * dx + dy * dy));
        // Noise dithering hides banding between the few dark levels.
        let dither = hash(num::u32_of(px), num::u32_of(py).wrapping_add(7)) * 2.5 / 255.0;
        let base = vignette.add(Rgb(dither, dither, dither));
        match self {
            Backdrop::Vignette => base,
            Backdrop::Stars => {
                let (cx, cy) = (num::u32_of(px / 2), num::u32_of(py / 4));
                let seed = hash(cx, cy);
                if seed > 0.006 {
                    return base;
                }
                let twinkle =
                    (((t * (0.5 + seed * 300.0)) + seed * 1000.0) * TAU).sin() * 0.5 + 0.5;
                let color = Rgb::WHITE.lerp(theme.sample(seed * 100.0), 0.4);
                base.add(color.scale(twinkle.powi(3) * 0.75))
            }
            Backdrop::Grid => {
                let horizon = 0.58;
                let glow = theme
                    .sample(0.15)
                    .scale(0.35 * (-((y - horizon) * 14.0).powi(2)).exp());
                if y < horizon {
                    let sky = theme.sample(0.75).scale(0.12 * (y / horizon).powi(2));
                    return base.add(sky).add(glow);
                }
                let depth = 1.0 / (y - horizon + 0.02);
                let z = depth * 0.6 + t * 1.2;
                let lane = dx * aspect * depth * 0.9;
                let line = |v: f32, width: f32| {
                    (1.0 - (v.rem_euclid(1.0) - 0.5).abs() * 2.0 / width).max(0.0)
                };
                let fade = ((y - horizon) / (1.0 - horizon)).powf(0.6);
                let grid = line(z, 0.12).max(line(lane, 0.08 * depth.min(10.0) / 4.0));
                base.add(theme.sample(0.0).scale(grid * 0.45 * fade))
                    .add(glow)
            }
            Backdrop::Aurora => {
                let wave = (x * 3.0 + t * 0.15).sin() * 0.08 + (x * 7.0 - t * 0.1).sin() * 0.04;
                let band = (-((y - 0.32 - wave) * 6.0).powi(2)).exp();
                let rays = (x * 40.0 + (x * 5.0 + t * 0.3).sin() * 6.0).sin() * 0.35 + 0.65;
                base.add(theme.sample(x * 0.6 + t * 0.02).scale(band * rays * 0.28))
            }
            Backdrop::Rays => {
                let angle = (dy / aspect.max(0.1)).atan2(dx) / TAU;
                let beam = ((angle * 14.0 + t * 0.04).rem_euclid(1.0) - 0.5).abs() * 2.0;
                let falloff = (1.0 - (dx * dx + dy * dy).sqrt() * 1.4).max(0.0);
                base.add(theme.sample(0.3).scale(beam.powi(2) * falloff * 0.12))
            }
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
