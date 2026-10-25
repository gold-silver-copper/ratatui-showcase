//! Source artwork as pixel bitmaps, and scaling them to fit any terminal.

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
            pixels: vec![Px::default(); width * height],
        }
    }

    pub fn get(&self, x: usize, y: usize) -> Px {
        if x >= self.width {
            return Px::default();
        }
        self.pixels
            .get(y * self.width + x)
            .copied()
            .unwrap_or_default()
    }

    pub fn set(&mut self, x: usize, y: usize, px: Px) {
        if x < self.width
            && let Some(p) = self.pixels.get_mut(y * self.width + x)
        {
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
        let width = (self.width as f32 * fx).round() as usize;
        let height = (self.height as f32 * fy).round() as usize;
        let mut out = Self::new(width, height, screen_aspect);
        for y in 0..height {
            for x in 0..width {
                out.set(
                    x,
                    y,
                    self.get((x as f32 / fx) as usize, (y as f32 / fy) as usize),
                );
            }
        }
        out
    }

    /// Ink coverage blurred by `radius`, padded by `radius` on every side.
    /// Used for the neon glow around the art.
    pub fn glow(&self, radius: usize) -> Bitmap {
        let mut padded = Self::new(
            self.width + 2 * radius,
            self.height + 2 * radius,
            self.aspect,
        );
        for y in 0..self.height {
            for x in 0..self.width {
                padded.set(x + radius, y + radius, self.get(x, y));
            }
        }
        // Two separable box blurs approximate a gaussian.
        for _ in 0..2 {
            padded = padded.box_blur(radius, true).box_blur(radius, false);
        }
        padded
    }

    fn box_blur(&self, radius: usize, horizontal: bool) -> Self {
        let mut out = Self::new(self.width, self.height, self.aspect);
        let span = (2 * radius + 1) as f32;
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
                if let Some(add) = (i + radius + 1 < inner).then(|| at(o, i + radius + 1)) {
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
    (1..=fit.floor() as usize)
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
    let mut bitmap = Bitmap::new(glyphs.len() * 8, 8, 1.0);
    for (i, glyph) in glyphs.iter().enumerate() {
        for (y, row) in glyph.iter().enumerate() {
            for x in 0..8 {
                if row & (1 << x) != 0 {
                    bitmap.set(
                        i * 8 + x,
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
