//! Colour themes and the per-pixel shaders that paint the art.

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

/// A scene's colours: one of the themes, or a harmony generated for it.
#[derive(Debug, Clone, PartialEq)]
pub struct Palette {
    pub name: String,
    pub background: Rgb,
    /// A looping gradient; the shaders sample it.
    pub stops: Vec<Rgb>,
    pub accent: Rgb,
}

impl Palette {
    pub fn theme(theme: &Theme) -> Self {
        Self {
            name: theme.name.to_owned(),
            background: theme.background,
            stops: theme.stops.iter().map(|&h| Rgb::hex(h)).collect(),
            accent: theme.accent,
        }
    }

    /// The first theme, the show's opening colours.
    pub fn first() -> Self {
        THEMES.first().map_or_else(Self::generated, Self::theme)
    }

    /// One of the themes, or now and then a palette made up on the spot.
    pub fn random() -> Self {
        if fastrand::f32() < 0.45 {
            return Self::generated();
        }
        let theme = THEMES
            .get(fastrand::usize(..THEMES.len()))
            .unwrap_or(&FALLBACK);
        Self::theme(theme)
    }

    /// A colour harmony around a random hue, in OKLCH so its colours look
    /// evenly bright: hues spread by one of the classic schemes, a dark
    /// background tinted with the base hue, an accent opposite it.
    pub fn generated() -> Self {
        let hue = fastrand::f32() * 360.0;
        let (scheme, offsets): (&str, [f32; 5]) = match fastrand::u8(0..6) {
            0 => ("analogous", [-40.0, -20.0, 0.0, 20.0, 40.0]),
            1 => ("complementary", [0.0, 20.0, 180.0, 200.0, -15.0]),
            2 => ("triadic", [0.0, 120.0, 240.0, 30.0, 150.0]),
            3 => ("split", [0.0, 150.0, 210.0, 15.0, 165.0]),
            4 => ("tetradic", [0.0, 90.0, 180.0, 270.0, 45.0]),
            _ => ("tonal", [0.0, 8.0, -8.0, 16.0, -16.0]),
        };
        let stops = offsets
            .iter()
            .enumerate()
            .map(|(i, offset)| {
                let light = if scheme == "tonal" {
                    0.45 + 0.1 * num::float(i)
                } else {
                    0.62 + fastrand::f32() * 0.22
                };
                oklch(light, 0.09 + fastrand::f32() * 0.1, hue + offset)
            })
            .collect();
        Self {
            name: format!("{scheme} {}°", num::floor_u32(hue)),
            background: oklch(0.14, 0.03, hue),
            stops,
            accent: oklch(0.9, 0.12, hue + 180.0),
        }
    }

    /// Samples the looping gradient at `t` (wraps every 1.0).
    pub fn sample(&self, t: f32) -> Rgb {
        let n = self.stops.len();
        let p = t.rem_euclid(1.0) * num::float(n);
        let i = num::floor_usize(p);
        let at = |k: usize| {
            self.stops
                .get(k.checked_rem(n).unwrap_or(0))
                .copied()
                .unwrap_or(Rgb::BLACK)
        };
        let f = p.fract();
        at(i).lerp(at(i.saturating_add(1)), f * f * (3.0 - 2.0 * f))
    }
}

const FALLBACK: Theme = Theme {
    name: "",
    background: Rgb::BLACK,
    stops: &[0xffffff],
    accent: Rgb::WHITE,
};

/// A colour from OKLCH (lightness 0..1, chroma, hue in degrees), with its
/// chroma lowered until it fits in sRGB.
pub fn oklch(l: f32, c: f32, hue: f32) -> Rgb {
    let (sin, cos) = hue.to_radians().sin_cos();
    let mut chroma = c;
    loop {
        let (a, b) = (chroma * cos, chroma * sin);
        let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
        let m_ = l - 0.105_561_346 * a - 0.063_854_17 * b;
        let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;
        let (l3, m3, s3) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
        let linear = [
            4.076_741_7 * l3 - 3.307_711_6 * m3 + 0.230_969_94 * s3,
            -1.268_438 * l3 + 2.609_757_4 * m3 - 0.341_319_38 * s3,
            -0.004_196_086_3 * l3 - 0.703_418_6 * m3 + 1.707_614_7 * s3,
        ];
        if linear.iter().all(|v| (-0.001..=1.001).contains(v)) || chroma <= 0.0 {
            let encode = |v: f32| {
                let v = v.clamp(0.0, 1.0);
                if v <= 0.003_130_8 {
                    v * 12.92
                } else {
                    1.055 * v.powf(1.0 / 2.4) - 0.055
                }
            };
            let [r, g, b] = linear.map(encode);
            return Rgb(r, g, b);
        }
        chroma -= 0.01;
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
    /// Veined marble.
    Marble,
    /// Flowing lava with glowing cracks.
    Lava,
    /// Gold leaf with moving highlights.
    Gold,
    /// Sparkling glitter.
    Glitter,
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
    Shader::Marble,
    Shader::Lava,
    Shader::Gold,
    Shader::Glitter,
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
            Shader::Marble => "marble",
            Shader::Lava => "lava",
            Shader::Gold => "gold",
            Shader::Glitter => "glitter",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradient_loops_seamlessly() {
        let Some(theme) = THEMES.get(1) else { return };
        let palette = Palette::theme(theme);
        let (a, b) = (palette.sample(0.0), palette.sample(0.999_99));
        assert!((a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01 && (a.2 - b.2).abs() < 0.01);
    }

    #[test]
    fn generated_palettes_stay_in_gamut() {
        for _ in 0..200 {
            let p = Palette::generated();
            for c in p.stops.iter().chain([&p.background, &p.accent]) {
                assert!(
                    [c.0, c.1, c.2].iter().all(|v| (0.0..=1.0).contains(v)),
                    "{p:?}"
                );
            }
            // Backgrounds dark, stops bright enough to read against them.
            assert!(p.background.0.max(p.background.1).max(p.background.2) < 0.2);
        }
        let white = oklch(1.0, 0.0, 0.0);
        assert!(white.0 > 0.99 && white.1 > 0.99 && white.2 > 0.99);
    }
}
