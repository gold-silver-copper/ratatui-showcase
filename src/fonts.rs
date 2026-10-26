//! Rasterizing the wordmark from the system's installed fonts.

use std::sync::Arc;

use ab_glyph::{Font, FontRef, FontVec, PxScale, Rect, ScaleFont, point};
use bevy::prelude::*;

use crate::art::{Bitmap, Material, Px};
use crate::num;

/// Families that map letters to pictures, or don't draw Latin at all.
const SKIP: &[&str] = &[
    "emoji", "awesome", "symbol", "dingbat", "d050000l", "music", "math", "braille", "fallback",
];

#[derive(Resource)]
pub struct SystemFonts {
    db: fontdb::Database,
    /// Faces by family, so a family with many files (Noto has one per
    /// script) is picked no more often than one with a single file.
    families: Vec<Vec<fontdb::ID>>,
}

#[derive(Clone)]
pub struct Face {
    pub name: String,
    pub font: Arc<FontVec>,
}

impl SystemFonts {
    pub fn load() -> Self {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        let mut families: Vec<(String, Vec<fontdb::ID>)> = Vec::new();
        let faces = db
            .faces()
            // Hairline weights vanish on a projector.
            .filter(|face| face.weight.0 >= 300)
            .filter(|face| {
                face.families.iter().all(|(family, _)| {
                    let family = family.to_lowercase();
                    !SKIP.iter().any(|skip| family.contains(skip))
                })
            })
            .map(|face| (family_of(face), face.id));
        for (name, id) in faces {
            match families.iter_mut().find(|(n, _)| *n == name) {
                Some((_, ids)) => ids.push(id),
                None => families.push((name, vec![id])),
            }
        }
        Self {
            db,
            families: families.into_iter().map(|(_, ids)| ids).collect(),
        }
    }

    /// A random installed face that can draw `text`.
    pub fn pick(&self, text: &str) -> Option<Face> {
        (0..32).find_map(|_| {
            let family = self
                .families
                .get(fastrand::usize(..self.families.len().max(1)))?;
            let id = *family.get(fastrand::usize(..family.len().max(1)))?;
            // Check the glyphs on the borrowed file; copy only the one used.
            let font = self.db.with_face_data(id, |data, index| {
                let borrowed = FontRef::try_from_slice_and_index(data, index).ok()?;
                if text.chars().any(|c| borrowed.glyph_id(c).0 == 0) {
                    return None;
                }
                FontVec::try_from_vec_and_index(data.to_vec(), index).ok()
            })??;
            let info = self.db.face(id)?;
            let family = info.families.first().map_or("", |(name, _)| name.as_str());
            Some(Face {
                name: format!("{family} {}", style_name(info)),
                font: Arc::new(font),
            })
        })
    }
}

/// The face's family as it looks in Latin: script variants such as
/// "Noto Sans Adlam" or "Noto Serif CJK JP" draw their Latin letters from the
/// same design as "Noto Sans" or "Noto Serif".
fn family_of(face: &fontdb::FaceInfo) -> String {
    let name = face.families.first().map_or("", |(name, _)| name.as_str());
    let words: Vec<&str> = name.split_whitespace().collect();
    match words.as_slice() {
        ["Noto", kind, "Mono", ..] => format!("Noto {kind} Mono"),
        ["Noto", kind, ..] => format!("Noto {kind}"),
        _ => name.to_owned(),
    }
}

fn style_name(info: &fontdb::FaceInfo) -> String {
    let weight = match info.weight.0 {
        0..=249 => "Thin",
        250..=349 => "Light",
        350..=449 => "Regular",
        450..=549 => "Medium",
        550..=649 => "SemiBold",
        650..=749 => "Bold",
        _ => "Black",
    };
    match info.style {
        fontdb::Style::Normal => weight.to_owned(),
        fontdb::Style::Italic | fontdb::Style::Oblique => format!("{weight} Italic"),
    }
}

/// Renders `text` as large as fits in a `box_w` x `box_h` pixel box on a
/// screen whose pixels are `screen_aspect` times taller than wide.
pub fn rasterize(
    font: &FontVec,
    text: &str,
    box_w: usize,
    box_h: usize,
    screen_aspect: f32,
) -> Bitmap {
    // Lay out at a reference size, then again at the size that fits.
    let Some(bounds) = layout_bounds(font, text, 100.0, screen_aspect) else {
        return Bitmap::default();
    };
    let fit = (box_w as f32 / bounds.width()).min(box_h as f32 / bounds.height());
    let size = 100.0 * fit;
    let Some(bounds) = layout_bounds(font, text, size, screen_aspect) else {
        return Bitmap::default();
    };
    let mut bitmap = Bitmap::new(
        num::floor_usize(bounds.width().ceil()),
        num::floor_usize(bounds.height().ceil()),
        screen_aspect,
    );
    for glyph in layout(font, text, size, screen_aspect) {
        let Some(outline) = font.outline_glyph(glyph) else {
            continue;
        };
        let corner = outline.px_bounds().min;
        let min = point(corner.x - bounds.min.x, corner.y - bounds.min.y);
        outline.draw(|x, y, coverage| {
            let (x, y) = (
                num::floor_usize(min.x + x as f32),
                num::floor_usize(min.y + y as f32),
            );
            let ink = bitmap.get(x, y).ink.max(coverage.min(1.0));
            bitmap.set(
                x,
                y,
                Px {
                    ink,
                    material: Material::Ink,
                },
            );
        });
    }
    bitmap
}

fn layout(font: &FontVec, text: &str, size: f32, screen_aspect: f32) -> Vec<ab_glyph::Glyph> {
    // A pixel `screen_aspect` times taller than wide needs proportionally more
    // pixels across for the same physical width.
    let scale = PxScale {
        x: size * screen_aspect,
        y: size,
    };
    let scaled = font.as_scaled(scale);
    let mut caret = 0.0;
    let mut previous = None;
    text.chars()
        .map(|c| {
            let id = font.glyph_id(c);
            if let Some(previous) = previous {
                caret += scaled.kern(previous, id);
            }
            previous = Some(id);
            let glyph = id.with_scale_and_position(scale, point(caret, scaled.ascent()));
            caret += scaled.h_advance(id);
            glyph
        })
        .collect()
}

fn layout_bounds(font: &FontVec, text: &str, size: f32, screen_aspect: f32) -> Option<Rect> {
    layout(font, text, size, screen_aspect)
        .into_iter()
        .filter_map(|glyph| font.outline_glyph(glyph))
        .map(|outline| outline.px_bounds())
        .reduce(|a, b| Rect {
            min: point(a.min.x.min(b.min.x), a.min.y.min(b.min.y)),
            max: point(a.max.x.max(b.max.x), a.max.y.max(b.max.y)),
        })
        .filter(|bounds| bounds.width() > 0.0 && bounds.height() > 0.0)
}
