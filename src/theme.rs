//! Colour themes and the per-pixel shaders that paint the art.

use std::f32::consts::TAU;

use ratatui::style::Color;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rgb(pub f32, pub f32, pub f32);

impl Rgb {
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

    pub fn scale(self, k: f32) -> Rgb {
        Rgb(self.0 * k, self.1 * k, self.2 * k)
    }

    pub fn add(self, other: Rgb) -> Rgb {
        Rgb(self.0 + other.0, self.1 + other.1, self.2 + other.2)
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
        let channel = |v: f32| ((v.clamp(0.0, 1.0) * 255.0).round() as u8) & 0xfe;
        Color::Rgb(channel(self.0), channel(self.1), channel(self.2))
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
        let p = t.rem_euclid(1.0) * n as f32;
        let i = p as usize;
        let a = self
            .stops
            .get(i % n.max(1))
            .map_or(Rgb::default(), |&h| Rgb::hex(h));
        let b = self
            .stops
            .get((i + 1) % n.max(1))
            .map_or(Rgb::default(), |&h| Rgb::hex(h));
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
}

pub const SHADERS: &[Shader] = &[
    Shader::Flow,
    Shader::Plasma,
    Shader::Pulse,
    Shader::Sheen,
    Shader::Rainbow,
    Shader::Scanline,
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
        }
    }

    /// Ink colour at `(u, v)` within the art (`0.0..=1.0` each way), at time
    /// `t` seconds. `row` is the screen pixel row, for effects tied to it.
    pub fn color(self, theme: &Theme, u: f32, v: f32, t: f32, row: usize) -> Rgb {
        match self {
            Shader::Flow => theme.sample(0.6 * u + 0.4 * v - 0.08 * t),
            Shader::Plasma => {
                let d = ((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt();
                let s = (u * 7.0 + t * 0.9).sin()
                    + (v * 9.0 - t * 1.1).sin()
                    + ((u + v) * 5.0 + t * 0.6).sin()
                    + (d * 12.0 - t * 1.4).sin();
                theme.sample(s * 0.125 + 0.5 + t * 0.02)
            }
            Shader::Pulse => {
                let d = ((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt();
                theme.sample(d * 1.6 - t * 0.15)
            }
            Shader::Sheen => {
                let base = theme.sample(u * 0.5 + t * 0.03);
                let phase = (t * 0.35).rem_euclid(2.0) * 1.6 - 0.3;
                let band = (-((u + v * 0.6) * 0.8 - phase).powi(2) * 60.0).exp();
                base.lerp(Rgb::WHITE, band * 0.8)
            }
            Shader::Rainbow => Rgb::hsl(u * 0.9 + v * 0.3 - t * 0.1, 0.85, 0.62),
            Shader::Scanline => {
                let base = theme.sample(v * 0.8 + t * 0.05);
                let line = if row.is_multiple_of(2) { 1.0 } else { 0.72 };
                let roll = (-((v - (t * 0.25).rem_euclid(1.4) + 0.2) * 9.0).powi(2)).exp();
                let flicker = 0.97 + 0.03 * (t * 13.0 * TAU).sin();
                base.scale(line * flicker).lerp(Rgb::WHITE, roll * 0.35)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradient_loops_seamlessly() {
        let theme = &THEMES[1];
        let (a, b) = (theme.sample(0.0), theme.sample(0.999_99));
        assert!((a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01 && (a.2 - b.2).abs() < 0.01);
    }
}
