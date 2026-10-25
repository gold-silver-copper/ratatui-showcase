//! Colour themes and the per-pixel shaders that paint the art.

use std::f32::consts::TAU;

use ratatui::style::Color;

use crate::num;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rgb(pub f32, pub f32, pub f32);

impl Rgb {
    pub const BLACK: Rgb = Rgb(0.0, 0.0, 0.0);
    pub const WHITE: Rgb = Rgb(1.0, 1.0, 1.0);

    pub const fn hex(hex: u32) -> Self {
        Self(
            ((hex >> 16) & 0xff) as f32 / 255.0,
            ((hex >> 8) & 0xff) as f32 / 255.0,
            (hex & 0xff) as f32 / 255.0,
        )
    }

    pub fn lerp(self, other: Rgb, t: f32) -> Rgb {
        let t = t.clamp(0.0, 1.0);
        Rgb(
            self.0 + (other.0 - self.0) * t,
            self.1 + (other.1 - self.1) * t,
            self.2 + (other.2 - self.2) * t,
        )
    }

    /// Blends in linear light, so anti-aliased edges keep their weight
    /// instead of turning dark and thin.
    pub fn mix(self, other: Rgb, t: f32) -> Rgb {
        let t = t.clamp(0.0, 1.0);
        let channel = |a: f32, b: f32| (a * a + (b * b - a * a) * t).max(0.0).sqrt();
        Rgb(
            channel(self.0, other.0),
            channel(self.1, other.1),
            channel(self.2, other.2),
        )
    }

    pub fn scale(self, k: f32) -> Rgb {
        Rgb(self.0 * k, self.1 * k, self.2 * k)
    }

    pub fn add(self, other: Rgb) -> Rgb {
        Rgb(self.0 + other.0, self.1 + other.1, self.2 + other.2)
    }

    pub fn mul(self, other: Rgb) -> Rgb {
        Rgb(self.0 * other.0, self.1 * other.1, self.2 * other.2)
    }

    /// Hue in turns, saturation and lightness in `0.0..=1.0`.
    pub fn hsl(hue: f32, saturation: f32, lightness: f32) -> Rgb {
        let channel = |n: f32| {
            let k = (n + hue.rem_euclid(1.0) * 12.0) % 12.0;
            let a = saturation * lightness.min(1.0 - lightness);
            lightness - a * (k - 3.0).min(9.0 - k).clamp(-1.0, 1.0)
        };
        Rgb(channel(0.0), channel(8.0), channel(4.0))
    }

    /// Quantized to 7 bits per channel so slowly drifting colours only touch
    /// the terminal when they visibly change.
    pub fn color(self) -> Color {
        let channel = |v: f32| num::round_u8(v.clamp(0.0, 1.0) * 255.0) & 0xfe;
        Color::Rgb(channel(self.0), channel(self.1), channel(self.2))
    }

    pub fn from_color(color: Color) -> Rgb {
        let channel = |c: u8| f32::from(c) / 255.0;
        if let Color::Rgb(r, g, b) = color {
            Rgb(channel(r), channel(g), channel(b))
        } else {
            Rgb::BLACK
        }
    }
}

pub struct Theme {
    pub name: &'static str,
    pub background: Rgb,
    /// A looping gradient; the shaders sample it.
    pub stops: &'static [u32],
    pub accent: Rgb,
}

impl Theme {
    /// Samples the looping gradient at `t` (wraps every 1.0).
    pub fn sample(&self, t: f32) -> Rgb {
        let n = self.stops.len();
        let p = t.rem_euclid(1.0) * num::float(n);
        let i = num::floor_usize(p);
        let a = self
            .stops
            .get(i.checked_rem(n).unwrap_or(0))
            .map_or(Rgb::BLACK, |&h| Rgb::hex(h));
        let b = self
            .stops
            .get(i.saturating_add(1).checked_rem(n).unwrap_or(0))
            .map_or(Rgb::BLACK, |&h| Rgb::hex(h));
        let f = p.fract();
        a.lerp(b, f * f * (3.0 - 2.0 * f))
    }
}

pub const THEMES: &[Theme] = &[
    Theme {
        name: "Classic",
        background: Rgb::hex(0x0a0a0c),
        stops: &[0xd0d0d0, 0xffffff, 0xa8a8a8, 0xe4e4e4],
        accent: Rgb::hex(0xff3030),
    },
    Theme {
        name: "Aurora",
        background: Rgb::hex(0x050a10),
        stops: &[0x00f5a0, 0x00d9f5, 0x7b5cff, 0xff4ecd],
        accent: Rgb::hex(0xffe066),
    },
    Theme {
        name: "Synthwave",
        background: Rgb::hex(0x0d0518),
        stops: &[0xff2a6d, 0xd300c5, 0x7a04eb, 0x05d9e8],
        accent: Rgb::hex(0xf9f871),
    },
    Theme {
        name: "Ember",
        background: Rgb::hex(0x100604),
        stops: &[0xff3d00, 0xff9100, 0xffd740, 0xff6e40],
        accent: Rgb::hex(0xfff3c4),
    },
    Theme {
        name: "Ocean",
        background: Rgb::hex(0x030b16),
        stops: &[0x0077b6, 0x00b4d8, 0x90e0ef, 0x48cae4],
        accent: Rgb::hex(0xffd166),
    },
    Theme {
        name: "Candy",
        background: Rgb::hex(0x0e0812),
        stops: &[0xff99c8, 0xfcf6bd, 0xd0f4de, 0xa9def9, 0xe4c1f9],
        accent: Rgb::hex(0xff595e),
    },
    Theme {
        name: "Phosphor",
        background: Rgb::hex(0x020a04),
        stops: &[0x00ff41, 0x14a830, 0x39ff14, 0x0b6e1f],
        accent: Rgb::hex(0xd8ffd8),
    },
    Theme {
        name: "Prism",
        background: Rgb::hex(0x080808),
        stops: &[0xff595e, 0xffca3a, 0x8ac926, 0x1982c4, 0x6a4c93],
        accent: Rgb::hex(0xffffff),
    },
    Theme {
        name: "Sunset",
        background: Rgb::hex(0x120714),
        stops: &[0xff7e5f, 0xfeb47b, 0xff6f91, 0x845ec2],
        accent: Rgb::hex(0xffe8a8),
    },
    Theme {
        name: "Glacier",
        background: Rgb::hex(0x040a12),
        stops: &[0xe0fbfc, 0x98c1d9, 0x5c8fd6, 0xc2dfe3],
        accent: Rgb::hex(0x7df9ff),
    },
    Theme {
        name: "Toxic",
        background: Rgb::hex(0x060a02),
        stops: &[0xccff00, 0x39ff14, 0x00ffcc, 0xfaff00],
        accent: Rgb::hex(0xff00aa),
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shader {
    /// The gradient drifts diagonally across the art.
    Flow,
    /// Interfering sine waves.
    Plasma,
    /// Rings expanding from the centre.
    Pulse,
    /// A bright highlight sweeps across a slow gradient.
    Sheen,
    /// The full colour wheel, whatever the theme.
    Rainbow,
    /// A CRT: scanlines and a rolling bright bar.
    Scanline,
    /// Eighties chrome: sky above the horizon, ground below.
    Chrome,
    /// Iridescent foil that shifts with position and time.
    Holo,
    /// Flickering flames rising through the art.
    Fire,
    /// Candy stripes marching diagonally.
    Stripes,
}

pub const SHADERS: &[Shader] = &[
    Shader::Flow,
    Shader::Plasma,
    Shader::Pulse,
    Shader::Sheen,
    Shader::Rainbow,
    Shader::Scanline,
    Shader::Chrome,
    Shader::Holo,
    Shader::Fire,
    Shader::Stripes,
];

impl Shader {
    pub fn name(self) -> &'static str {
        match self {
            Shader::Flow => "flow",
            Shader::Plasma => "plasma",
            Shader::Pulse => "pulse",
            Shader::Sheen => "sheen",
            Shader::Rainbow => "rainbow",
            Shader::Scanline => "scanline",
            Shader::Chrome => "chrome",
            Shader::Holo => "holo",
            Shader::Fire => "fire",
            Shader::Stripes => "stripes",
        }
    }

    /// Ink colour at `(u, v)` within the art (`0.0..=1.0` each way), at time
    /// `t` seconds. `row` is the screen pixel row, for effects tied to it.
    pub fn color(self, theme: &Theme, u: f32, v: f32, t: f32, row: usize) -> Rgb {
        let d = ((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt();
        match self {
            Shader::Flow => theme.sample(0.6 * u + 0.4 * v - 0.08 * t),
            Shader::Plasma => {
                let s = (u * 7.0 + t * 0.9).sin()
                    + (v * 9.0 - t * 1.1).sin()
                    + ((u + v) * 5.0 + t * 0.6).sin()
                    + (d * 12.0 - t * 1.4).sin();
                theme.sample(s * 0.125 + 0.5 + t * 0.02)
            }
            Shader::Pulse => theme.sample(d * 1.6 - t * 0.15),
            Shader::Sheen => {
                let base = theme.sample(u * 0.5 + t * 0.03);
                base.lerp(Rgb::WHITE, sweep(u + v * 0.6, t) * 0.8)
            }
            Shader::Rainbow => Rgb::hsl(u * 0.9 + v * 0.3 - t * 0.1, 0.85, 0.62),
            Shader::Scanline => {
                let base = theme.sample(v * 0.8 + t * 0.05);
                let line = if row.is_multiple_of(2) { 1.0 } else { 0.72 };
                let roll = (-((v - (t * 0.25).rem_euclid(1.4) + 0.2) * 9.0).powi(2)).exp();
                let flicker = 0.97 + 0.03 * (t * 13.0 * TAU).sin();
                base.scale(line * flicker).lerp(Rgb::WHITE, roll * 0.35)
            }
            Shader::Chrome => {
                let horizon = 0.55 + 0.03 * (u * 6.0 + t * 0.7).sin();
                let chrome = if v < horizon {
                    let sky = theme.sample(0.55).lerp(Rgb::WHITE, 0.55);
                    Rgb::hex(0x1a2a6c).mix(sky, v / horizon)
                } else {
                    let ground = theme.sample(0.1).lerp(Rgb::hex(0xffcf70), 0.4);
                    Rgb::hex(0x2b1200).mix(ground, ((v - horizon) / (1.0 - horizon)).powf(0.7))
                };
                chrome.lerp(Rgb::WHITE, sweep(u + v * 0.3, t) * 0.9)
            }
            Shader::Holo => {
                let angle = (u - 0.5).atan2(v - 0.5) / TAU;
                let foil = Rgb::hsl(angle * 2.0 + d * 1.5 - t * 0.12, 0.7, 0.72);
                foil.mix(theme.sample(u + t * 0.05), 0.35)
                    .lerp(Rgb::WHITE, sparkle(u, v, t) * 0.9)
            }
            Shader::Fire => {
                let flicker = (u * 23.0 + t * 7.0).sin() * 0.5 + (u * 41.0 - t * 11.0).sin() * 0.25;
                let heat =
                    (1.15 - v + flicker * 0.12 + (t * 3.0 + u * 9.0).sin() * 0.05).clamp(0.0, 1.0);
                let flame = Rgb::hex(0x400000)
                    .mix(Rgb::hex(0xff3000), heat * 1.6)
                    .mix(Rgb::hex(0xffd23f), (heat - 0.5) * 2.0)
                    .mix(Rgb::WHITE, (heat - 0.85) * 4.0);
                flame.mix(theme.sample(heat * 0.5), 0.25)
            }
            Shader::Stripes => {
                let s = ((u * 2.0 + v * 1.2) * 4.0 - t * 0.6).rem_euclid(1.0);
                let band = (s * 2.0).floor() / 2.0;
                theme
                    .sample(band * 0.5 + t * 0.03)
                    .lerp(Rgb::WHITE, if s < 0.06 { 0.4 } else { 0.0 })
            }
        }
    }
}

/// A highlight band sweeping across every few seconds.
fn sweep(position: f32, t: f32) -> f32 {
    let phase = (t * 0.35).rem_euclid(2.0) * 1.6 - 0.3;
    (-(position * 0.8 - phase).powi(2) * 60.0).exp()
}

/// Rare bright glints.
fn sparkle(u: f32, v: f32, t: f32) -> f32 {
    let cell = num::floor_u32(u * 40.0).wrapping_mul(7919)
        ^ num::floor_u32(v * 20.0).wrapping_mul(104_729);
    let phase = (cell % 997) as f32 / 997.0;
    let s = ((t * 0.8 + phase * 13.0) * TAU).sin();
    if s > 0.985 { (s - 0.985) / 0.015 } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradient_loops_seamlessly() {
        let Some(theme) = THEMES.get(1) else { return };
        let (a, b) = (theme.sample(0.0), theme.sample(0.999_99));
        assert!((a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01 && (a.2 - b.2).abs() < 0.01);
    }
}
