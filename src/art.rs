//! Source artwork as pixel bitmaps, and scaling them to fit any terminal.

use crate::num;

/// What a pixel is made of; the theme colours each material differently.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Material {
    #[default]
    Ink,
    Hat,
    Eye,
    Screen,
    Border,
    Cursor,
    /// The side of extruded 3D lettering: 0 at the face, 255 at the back.
    Depth(u8),
    /// A mirror image below the art, faded to `n / 255` brightness.
    Reflection(u8),
}

/// How the art's shape is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Solid,
    Outline,
    Extrude,
    Reflect,
}

pub const STYLES: &[Style] = &[
    Style::Solid,
    Style::Solid,
    Style::Outline,
    Style::Extrude,
    Style::Reflect,
];

impl Style {
    /// The share of the layout box the plain art may use, leaving room for
    /// what the style adds.
    pub fn reserve(self) -> (f32, f32) {
        match self {
            Style::Solid | Style::Outline => (1.0, 1.0),
            Style::Extrude => (0.93, 0.86),
            Style::Reflect => (1.0, 0.64),
        }
    }

    pub fn apply(self, bitmap: Bitmap) -> Bitmap {
        match self {
            Style::Solid => bitmap,
            Style::Outline => bitmap.outline((bitmap.height / 14).max(2)),
            Style::Extrude => {
                let depth = (bitmap.height / 9).max(2);
                bitmap.extrude(depth)
            }
            Style::Reflect => bitmap.reflect(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Px {
    /// Coverage in `0.0..=1.0`; anti-aliased fonts produce fractional values.
    pub ink: f32,
    pub material: Material,
}

/// A grid of pixels. `aspect` is a pixel's height divided by its width, as the
/// artist intended it (quadrant art has tall pixels, half-block art square ones).
#[derive(Debug, Clone, Default)]
pub struct Bitmap {
    pub width: usize,
    pub height: usize,
    pub aspect: f32,
    pixels: Vec<Px>,
}

impl Bitmap {
    pub fn new(width: usize, height: usize, aspect: f32) -> Self {
        Self {
            width,
            height,
            aspect,
            pixels: vec![Px::default(); width.saturating_mul(height)],
        }
    }

    fn index(&self, x: usize, y: usize) -> Option<usize> {
        (x < self.width)
            .then(|| y.checked_mul(self.width)?.checked_add(x))
            .flatten()
    }

    pub fn get(&self, x: usize, y: usize) -> Px {
        self.index(x, y)
            .and_then(|i| self.pixels.get(i))
            .copied()
            .unwrap_or_default()
    }

    pub fn set(&mut self, x: usize, y: usize, px: Px) {
        if let Some(p) = self.index(x, y).and_then(|i| self.pixels.get_mut(i)) {
            *p = px;
        }
    }

    fn from_grid(grid: &str, aspect: f32, material: impl Fn(char) -> Option<Material>) -> Self {
        let width = grid.lines().map(|l| l.chars().count()).max().unwrap_or(0);
        let mut bitmap = Self::new(width, grid.lines().count(), aspect);
        for (y, line) in grid.lines().enumerate() {
            for (x, c) in line.chars().enumerate() {
                if let Some(material) = material(c) {
                    bitmap.set(x, y, Px { ink: 1.0, material });
                }
            }
        }
        bitmap
    }

    /// Nearest-neighbour scale into a `box_w` x `box_h` pixel box on a screen
    /// whose pixels are `screen_aspect` times taller than wide, keeping the
    /// art's physical proportions. Upscaling uses whole-number factors so every
    /// source pixel stays the same size.
    pub fn fit(&self, box_w: usize, box_h: usize, screen_aspect: f32) -> Self {
        let (fx, fy) = fit_scale(
            self.width,
            self.height,
            self.aspect,
            box_w,
            box_h,
            screen_aspect,
        );
        let width = num::floor_usize((num::float(self.width) * fx).round());
        let height = num::floor_usize((num::float(self.height) * fy).round());
        let mut out = Self::new(width, height, screen_aspect);
        for y in 0..height {
            for x in 0..width {
                out.set(
                    x,
                    y,
                    self.get(
                        num::floor_usize(num::float(x) / fx),
                        num::floor_usize(num::float(y) / fy),
                    ),
                );
            }
        }
        out
    }

    /// Ink coverage blurred by `radius`, padded by `radius` on every side.
    /// Used for the neon glow around the art.
    pub fn glow(&self, radius: usize) -> Bitmap {
        let mut padded = Self::new(
            self.width.saturating_add(radius.saturating_mul(2)),
            self.height.saturating_add(radius.saturating_mul(2)),
            self.aspect,
        );
        for y in 0..self.height {
            for x in 0..self.width {
                padded.set(
                    x.saturating_add(radius),
                    y.saturating_add(radius),
                    self.get(x, y),
                );
            }
        }
        // Two separable box blurs approximate a gaussian.
        for _ in 0..2 {
            padded = padded.box_blur(radius, true).box_blur(radius, false);
        }
        padded
    }

    /// Only the rim of the shape, `width` pixels thick.
    fn outline(&self, width: usize) -> Bitmap {
        let mut out = Self::new(self.width, self.height, self.aspect);
        for y in 0..self.height {
            for x in 0..self.width {
                let px = self.get(x, y);
                if px.ink <= 0.0 {
                    continue;
                }
                let inside = [(width, 0), (0, width)]
                    .iter()
                    .flat_map(|&(dx, dy)| {
                        [
                            (x.checked_sub(dx), y.checked_sub(dy)),
                            (x.checked_add(dx), y.checked_add(dy)),
                        ]
                    })
                    .map(|(nx, ny)| match (nx, ny) {
                        (Some(nx), Some(ny)) => self.get(nx, ny).ink,
                        _ => 0.0,
                    })
                    .fold(1.0_f32, f32::min);
                out.set(
                    x,
                    y,
                    Px {
                        ink: px.ink * (1.0 - inside),
                        ..px
                    },
                );
            }
        }
        out
    }

    /// 3D lettering: the face over `depth` receding layers.
    fn extrude(&self, depth: usize) -> Bitmap {
        let mut out = Self::new(
            self.width.saturating_add(depth),
            self.height.saturating_add(depth),
            self.aspect,
        );
        for layer in (1..=depth).rev() {
            let material =
                Material::Depth(num::round_u8(num::float(layer) * 255.0 / num::float(depth)));
            for y in 0..self.height {
                for x in 0..self.width {
                    let ink = self.get(x, y).ink;
                    let (ox, oy) = (x.saturating_add(layer), y.saturating_add(layer));
                    if ink > out.get(ox, oy).ink {
                        out.set(ox, oy, Px { ink, material });
                    }
                }
            }
        }
        for y in 0..self.height {
            for x in 0..self.width {
                let px = self.get(x, y);
                if px.ink > 0.0 {
                    let behind = out.get(x, y).ink;
                    out.set(
                        x,
                        y,
                        Px {
                            ink: px.ink.max(behind),
                            material: px.material,
                        },
                    );
                }
            }
        }
        out
    }

    /// The art over a fading mirror image of itself.
    fn reflect(&self) -> Bitmap {
        let gap = (self.height / 16).max(1);
        let mirror = self.height / 2;
        let below = self.height.saturating_add(gap);
        let mut out = Self::new(self.width, below.saturating_add(mirror), self.aspect);
        for y in 0..self.height {
            for x in 0..self.width {
                out.set(x, y, self.get(x, y));
            }
        }
        for my in 0..mirror {
            let fade = 0.45 * (1.0 - my as f32 / mirror as f32);
            let material = Material::Reflection(num::round_u8(fade * 255.0));
            for x in 0..self.width {
                let px = self.get(x, self.height.saturating_sub(my.saturating_add(1)));
                out.set(
                    x,
                    below.saturating_add(my),
                    Px {
                        ink: px.ink,
                        material,
                    },
                );
            }
        }
        out
    }

    fn box_blur(&self, radius: usize, horizontal: bool) -> Self {
        let mut out = Self::new(self.width, self.height, self.aspect);
        let span = num::float(radius) * 2.0 + 1.0;
        let (outer, inner) = if horizontal {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        };
        let at = |o: usize, i: usize| if horizontal { (i, o) } else { (o, i) };
        for o in 0..outer {
            let mut sum = 0.0;
            for i in 0..=radius.min(inner.saturating_sub(1)) {
                let (x, y) = at(o, i);
                sum += self.get(x, y).ink;
            }
            for i in 0..inner {
                let (x, y) = at(o, i);
                out.set(
                    x,
                    y,
                    Px {
                        ink: sum / span,
                        material: Material::Ink,
                    },
                );
                let ahead = i.saturating_add(radius).saturating_add(1);
                if let Some(add) = (ahead < inner).then(|| at(o, ahead)) {
                    sum += self.get(add.0, add.1).ink;
                }
                if let Some(sub) = i.checked_sub(radius).map(|j| at(o, j)) {
                    sum -= self.get(sub.0, sub.1).ink;
                }
            }
        }
        out
    }
}

/// Horizontal and vertical scale factors that fit `w` x `h` source pixels of
/// `aspect` into the box without distorting them.
fn fit_scale(
    w: usize,
    h: usize,
    aspect: f32,
    box_w: usize,
    box_h: usize,
    screen_aspect: f32,
) -> (f32, f32) {
    if w == 0 || h == 0 {
        return (1.0, 1.0);
    }
    // Screen pixels per source pixel: vertically fy = fx * aspect / screen_aspect.
    let ratio = aspect / screen_aspect;
    let fit = (box_w as f32 / w as f32).min(box_h as f32 / (h as f32 * ratio));
    if fit < 1.0 {
        return (fit, fit * ratio);
    }
    // Whole-number factors keep every source pixel the same size.
    (1..=num::floor_usize(fit))
        .rev()
        .map(|fx| (fx as f32, (fx as f32 * ratio).round().max(1.0)))
        .find(|&(_, fy)| h as f32 * fy <= box_h as f32)
        .unwrap_or((fit, fit * ratio))
}

/// The ratatui mascot, from `ratatui-widgets` (MIT). One character per pixel,
/// drawn with half blocks, so pixels are square.
const MASCOT: &str = "
                   hhh
                 hhhhhh
                hhhhhhh
               hhhhhhhh
              hhhhhhhhh
             hhhhhhhhhh
            hhhhhhhhhhhh
            hhhhhhhhhhhhh
            hhhhhhhhhhhhh     ██████
             hhhhhhhhhhh    ████████
                  hhhhh ███████████
                   hhh ██ee████████
                    h █████████████
                ████ █████████████
               █████████████████
               ████████████████
               ████████████████
                ███ ██████████
              ▒▒    █████████
             ▒░░▒   █████████
            ▒░░░░▒ ██████████
           ▒░░▓░░░▒ █████████
          ▒░░▓▓░░░░▒ ████████
         ▒░░░░░░░░░░▒ ██████████
        ▒░░░░░░░░░░░░▒ ██████████
       ▒░░░░░░░▓▓░░░░░▒ █████████
      ▒░░░░░░░░░▓▓░░░░░▒ ████  ███
     ▒░░░░░░░░░░░░░░░░░░▒ ██   ███
    ▒░░░░░░░░░░░░░░░░░░░░▒ █   ███
    ▒░░░░░░░░░░░░░░░░░░░░░▒   ███
     ▒░░░░░░░░░░░░░░░░░░░░░▒ ███
      ▒░░░░░░░░░░░░░░░░░░░░░▒ █";

/// `assets/logo.svg` from the ratatui repo, rasterized at its native 50x50.
const EMBLEM: &str = "
                       ######
                     #########
                    ##########
                   ###########
                  ############
                 #############
                ##############
               ###############
              ################
             #################
             ##################
             ###################
             ###################
             ####################       ##########
              ###################    #############
                    ############ #################
                     ########## #################
                      ######## ##  ##############
                        ##### ##    #############
                         ### ###    #############
                            #####  #############
                     ##########################
                   ###########################
                  ##########################
                 ##########################
                 #########################
                  #######################
                ## ####################
               #### ####  #############
              ##  ##     ##############
             ##    ##    ##############
            ##      ##  ###############
           ##    #   ## ###############
          ##     ##   ## ##############
         ##      ##    ## #############
        ##    #####     ## ############
       ##      ####      ## ################
      ##                  ## ################
     ##                    ## ###############
    ##           #          ## ##############
   ##            ##          ## ######### ###
  ##              ##          ## #####    ###
 ##                #           ## ###     ###
##                              ## ##     ###
##                               ## #     ###
 ##                               ##     ####
  ##                               ##   ####
   ##                               ## ####
    ##                               ## ###
     ##                               ## #";

/// `RatatuiLogo` wordmarks from `ratatui-widgets` (MIT), in quadrant blocks.
pub const LOGO_TINY: &str = "▛▚▗▀▖▜▘▞▚▝▛▐ ▌▌\n▛▚▐▀▌▐ ▛▜ ▌▝▄▘▌";
pub const LOGO_SMALL: &str = "█▀▀▄ ▄▀▀▄▝▜▛▘▄▀▀▄▝▜▛▘█  █ █\n█▀▀▄ █▀▀█ ▐▌ █▀▀█ ▐▌ ▀▄▄▀ █";

pub fn mascot() -> Bitmap {
    Bitmap::from_grid(MASCOT.trim_start_matches('\n'), 1.0, |c| match c {
        '█' => Some(Material::Ink),
        'h' => Some(Material::Hat),
        'e' => Some(Material::Eye),
        '░' => Some(Material::Screen),
        '▒' => Some(Material::Border),
        '▓' => Some(Material::Cursor),
        _ => None,
    })
}

pub fn emblem() -> Bitmap {
    Bitmap::from_grid(EMBLEM.trim_start_matches('\n'), 1.0, |c| {
        (c == '#').then_some(Material::Ink)
    })
}

/// Decodes quadrant-block art into pixels, two per cell each way. A quadrant
/// is half a cell wide and half a cell tall, so its pixels are about twice as
/// tall as wide.
pub fn quadrants(art: &str) -> Bitmap {
    let grid: String = art
        .lines()
        .flat_map(|line| {
            let [top, bottom] = [0, 1].map(|half| {
                line.chars()
                    .flat_map(|c| {
                        let [tl, tr, bl, br] = quadrant_bits(c);
                        if half == 0 { [tl, tr] } else { [bl, br] }
                    })
                    .map(|on| if on { '#' } else { ' ' })
                    .collect::<String>()
            });
            [top, bottom]
        })
        .collect::<Vec<_>>()
        .join("\n");
    Bitmap::from_grid(&grid, 2.0, |c| (c == '#').then_some(Material::Ink))
}

/// Which quadrants of a block character are filled: top-left, top-right,
/// bottom-left, bottom-right.
fn quadrant_bits(c: char) -> [bool; 4] {
    let bits: u8 = match c {
        '▘' => 0b1000,
        '▝' => 0b0100,
        '▖' => 0b0010,
        '▗' => 0b0001,
        '▀' => 0b1100,
        '▄' => 0b0011,
        '▌' => 0b1010,
        '▐' => 0b0101,
        '▚' => 0b1001,
        '▞' => 0b0110,
        '▛' => 0b1110,
        '▜' => 0b1101,
        '▙' => 0b1011,
        '▟' => 0b0111,
        '█' => 0b1111,
        _ => 0,
    };
    [bits & 8 != 0, bits & 4 != 0, bits & 2 != 0, bits & 1 != 0]
}

/// "ratatui" in the classic 8x8 bitmap font, with square pixels.
pub fn pixel_font(text: &str) -> Bitmap {
    use font8x8::UnicodeFonts;
    let glyphs: Vec<[u8; 8]> = text
        .chars()
        .filter_map(|c| font8x8::BASIC_FONTS.get(c))
        .collect();
    let mut bitmap = Bitmap::new(glyphs.len().saturating_mul(8), 8, 1.0);
    for (i, glyph) in glyphs.iter().enumerate() {
        for (y, row) in glyph.iter().enumerate() {
            for x in 0..8 {
                if crate::glyphs::bit(u32::from(*row), x) {
                    bitmap.set(
                        i.saturating_mul(8).saturating_add(x),
                        y,
                        Px {
                            ink: 1.0,
                            material: Material::Ink,
                        },
                    );
                }
            }
        }
    }
    bitmap
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upscaling_keeps_square_pixels_square() {
        // Screen pixels 1.25x taller than wide: 4 columns per 3 rows is closest.
        let (fx, fy) = fit_scale(50, 50, 1.0, 400, 400, 1.25);
        assert_eq!(fx.fract(), 0.0);
        assert!(((fy * 1.25) / fx - 1.0).abs() < 0.1, "{fx} x {fy}");
    }

    #[test]
    fn fitted_art_stays_inside_the_box() {
        for (box_w, box_h) in [(10, 4), (80, 48), (300, 160), (1000, 90)] {
            for art in [
                mascot(),
                emblem(),
                quadrants(LOGO_SMALL),
                pixel_font("ratatui"),
            ] {
                let fitted = art.fit(box_w, box_h, 1.1);
                assert!(
                    fitted.width <= box_w && fitted.height <= box_h,
                    "{box_w}x{box_h}"
                );
            }
        }
    }

    #[test]
    fn first_rows_keep_their_indentation() {
        assert_eq!(mascot().get(0, 0).ink, 0.0);
        assert_eq!(mascot().get(19, 0).material, Material::Hat);
        assert_eq!(emblem().get(0, 0).ink, 0.0);
        assert_eq!((emblem().width, emblem().height), (50, 50));
    }

    #[test]
    fn quadrant_wordmark_decodes_to_four_rows() {
        let logo = quadrants(LOGO_SMALL);
        assert_eq!((logo.width, logo.height), (54, 4));
        assert_eq!(logo.get(0, 0).ink, 1.0);
    }
}
