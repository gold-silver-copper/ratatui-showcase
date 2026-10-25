//! A scene laid out for the current terminal size, and drawn each frame.

use std::sync::Arc;

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;

use crate::art::{self, Bitmap, Material};
use crate::fonts::{self, Face};
use crate::theme::{Rgb, Shader, Theme};

pub const WORDMARK: &str = "ratatui";

pub enum Scene {
    Mascot,
    Emblem,
    Logo { small: bool },
    PixelFont,
    Face(Face),
}

impl Scene {
    pub fn is_logo(&self) -> bool {
        matches!(self, Scene::Mascot | Scene::Emblem)
    }

    pub fn label(&self) -> String {
        match self {
            Scene::Mascot => "RatatuiMascot".to_owned(),
            Scene::Emblem => "logo.svg".to_owned(),
            Scene::Logo { small: true } => "RatatuiLogo::small()".to_owned(),
            Scene::Logo { small: false } => "RatatuiLogo::tiny()".to_owned(),
            Scene::PixelFont => "font8x8".to_owned(),
            Scene::Face(face) => face.name.clone(),
        }
    }

    fn bitmap(&self, box_w: usize, box_h: usize, aspect: f32) -> Bitmap {
        match self {
            Scene::Mascot => art::mascot().fit(box_w, box_h, aspect),
            Scene::Emblem => art::emblem().fit(box_w, box_h, aspect),
            Scene::Logo { small } => art::quadrants(if *small {
                art::LOGO_SMALL
            } else {
                art::LOGO_TINY
            })
            .fit(box_w, box_h, aspect),
            Scene::PixelFont => art::pixel_font(WORDMARK).fit(box_w, box_h, aspect),
            Scene::Face(face) => fonts::rasterize(&face.font, WORDMARK, box_w, box_h, aspect),
        }
    }
}

/// Which cells hold part of the art, for effects that only touch the art.
pub struct InkMask {
    width: u16,
    cells: Vec<bool>,
}

impl InkMask {
    pub fn contains(&self, pos: Position) -> bool {
        pos.x < self.width
            && self
                .cells
                .get(pos.y as usize * self.width as usize + pos.x as usize)
                == Some(&true)
    }
}

/// Screen pixels are half a cell: each cell draws two of them with `▀`.
pub struct Stage {
    pub size: Rect,
    bitmap: Bitmap,
    glow: Bitmap,
    glow_radius: usize,
    /// Top-left screen pixel of the bitmap.
    origin: (usize, usize),
    /// Cells covered by the art and its glow.
    pub area: Rect,
    pub ink: Arc<InkMask>,
}

impl Stage {
    pub fn new(scene: &Scene, size: Rect, aspect: f32) -> Self {
        let (px_w, px_h) = (
            size.width as usize,
            (size.height as usize * 2).saturating_sub(4),
        );
        let (fill_w, fill_h) = if scene.is_logo() {
            (0.8, 0.8)
        } else {
            (0.88, 0.6)
        };
        let bitmap = scene.bitmap(
            (px_w as f32 * fill_w) as usize,
            (px_h as f32 * fill_h) as usize,
            aspect,
        );
        let glow_radius = (bitmap.height.max(bitmap.width / 4) / 14).clamp(1, 12);
        let glow = bitmap.glow(glow_radius);
        let origin = (
            px_w.saturating_sub(bitmap.width) / 2,
            px_h.saturating_sub(bitmap.height) / 2,
        );

        let mut cells = vec![false; size.width as usize * size.height as usize];
        for y in 0..bitmap.height {
            for x in 0..bitmap.width {
                let index = (origin.1 + y) / 2 * size.width as usize + origin.0 + x;
                if bitmap.get(x, y).ink > 0.5
                    && let Some(cell) = cells.get_mut(index)
                {
                    *cell = true;
                }
            }
        }

        let left = origin.0.saturating_sub(glow_radius);
        let top = origin.1.saturating_sub(glow_radius) / 2;
        let right = (origin.0 + bitmap.width + glow_radius).min(size.width as usize);
        let bottom = (origin.1 + bitmap.height + glow_radius)
            .div_ceil(2)
            .min(size.height as usize);
        let area = Rect::new(
            left as u16,
            top as u16,
            right.saturating_sub(left) as u16,
            bottom.saturating_sub(top) as u16,
        );

        Self {
            size,
            bitmap,
            glow,
            glow_radius,
            origin,
            area,
            ink: Arc::new(InkMask {
                width: size.width,
                cells,
            }),
        }
    }

    pub fn draw(&self, buf: &mut Buffer, theme: &Theme, shader: Shader, t: f32) {
        for y in 0..self.size.height {
            for x in 0..self.size.width {
                let top = self.shade(theme, shader, t, x as usize, y as usize * 2);
                let bottom = self.shade(theme, shader, t, x as usize, y as usize * 2 + 1);
                if let Some(cell) = buf.cell_mut(Position::new(x, y)) {
                    cell.set_char('▀')
                        .set_fg(top.color())
                        .set_bg(bottom.color());
                }
            }
        }
    }

    pub fn caption(&self, buf: &mut Buffer, text: &str, theme: &Theme) {
        let width = text.chars().count() as u16;
        let x = self.size.width.saturating_sub(width) / 2;
        let y = self.size.height.saturating_sub(1);
        let color = theme.sample(0.0).lerp(theme.background, 0.45).color();
        buf.set_string(x, y, text, Style::new().fg(color));
    }

    fn shade(&self, theme: &Theme, shader: Shader, t: f32, px: usize, py: usize) -> Rgb {
        let background = self.background(theme, px, py);
        let (gx, gy) = (px + self.glow_radius, py + self.glow_radius);
        let (Some(gx), Some(gy)) = (gx.checked_sub(self.origin.0), gy.checked_sub(self.origin.1))
        else {
            return background;
        };
        let glow = self.glow.get(gx, gy).ink;
        let pixel = match (
            gx.checked_sub(self.glow_radius),
            gy.checked_sub(self.glow_radius),
        ) {
            (Some(x), Some(y)) => self.bitmap.get(x, y),
            _ => Default::default(),
        };
        if pixel.ink <= 0.0 && glow < 0.01 {
            return background;
        }

        let u = (gx as f32 - self.glow_radius as f32) / self.bitmap.width.max(1) as f32;
        let v = (gy as f32 - self.glow_radius as f32) / self.bitmap.height.max(1) as f32;
        let ink = shader.color(theme, u, v, t, py);
        let surface = match pixel.material {
            Material::Ink => ink,
            Material::Hat => ink.lerp(Rgb::WHITE, 0.75),
            Material::Eye if t.rem_euclid(5.0) < 0.15 => theme.accent,
            Material::Eye => theme.background.scale(2.5),
            Material::Screen => theme.background.lerp(ink, 0.1),
            Material::Border => ink.scale(0.55),
            Material::Cursor if (t * 1.2).fract() < 0.5 => theme.accent,
            Material::Cursor => theme.background.lerp(ink, 0.1),
        };
        background
            .lerp(surface, pixel.ink)
            .add(ink.scale(glow * 0.5 * (1.0 - pixel.ink)))
    }

    /// The theme's background with a soft vignette.
    fn background(&self, theme: &Theme, px: usize, py: usize) -> Rgb {
        let dx = px as f32 / self.size.width.max(1) as f32 - 0.5;
        let dy = py as f32 / (self.size.height.max(1) as f32 * 2.0) - 0.5;
        // Ordered dithering hides banding between the few dark levels.
        let dither = [0.0, 0.5, 0.75, 0.25]
            .get(px % 2 + 2 * (py % 2))
            .copied()
            .unwrap_or(0.0);
        let shade = theme.background.scale(1.6 - 1.8 * (dx * dx + dy * dy));
        shade.add(Rgb(dither, dither, dither).scale(2.0 / 255.0))
    }
}
