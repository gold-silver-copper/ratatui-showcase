//! What plays next: a fresh combination of everything, kept from repeating
//! what played recently, and now and then a hand-tuned preset.

use std::collections::VecDeque;

use crate::art::{STYLES, Style};
use crate::backdrop::{BACKDROPS, Backdrop};
use crate::fonts::SystemFonts;
use crate::stage::{
    self, Act, IDLES, Idle, LOOKS, Logo, Look, MODES, Mode, POSTS, Post, Subject, Word,
};
use crate::theme::{Palette, SHADERS, Shader, THEMES};

/// Scenes remembered, so the next few don't repeat them.
const MEMORY: usize = 4;

#[derive(Default)]
pub struct Director {
    recent: VecDeque<Act>,
}

/// A combination that is known to look good, for any subject.
struct Preset {
    theme: Option<&'static str>,
    shader: Shader,
    backdrop: Backdrop,
    look: Look,
    style: Style,
    mode: Mode,
    post: Post,
    idle: Idle,
}

const fn preset(
    theme: Option<&'static str>,
    shader: Shader,
    backdrop: Backdrop,
    (look, style, mode): (Look, Style, Mode),
    post: Post,
    idle: Idle,
) -> Preset {
    Preset {
        theme,
        shader,
        backdrop,
        look,
        style,
        mode,
        post,
        idle,
    }
}

const PRESETS: &[Preset] = &[
    preset(
        Some("Synthwave"),
        Shader::Chrome,
        Backdrop::Grid,
        (Look::Smooth, Style::Neon, Mode::Flat),
        Post::Crt,
        Idle::Float,
    ),
    preset(
        Some("Ember"),
        Shader::Gold,
        Backdrop::Mandelbulb,
        (Look::Smooth, Style::Solid, Mode::Turntable),
        Post::Rays,
        Idle::Still,
    ),
    preset(
        None,
        Shader::Holo,
        Backdrop::Warp,
        (Look::Smooth, Style::Bevel, Mode::Flat),
        Post::Bloom,
        Idle::Breathe,
    ),
    preset(
        Some("Phosphor"),
        Shader::Scanline,
        Backdrop::Stars,
        (Look::Code, Style::Solid, Mode::Flat),
        Post::Crt,
        Idle::Still,
    ),
    preset(
        None,
        Shader::Lava,
        Backdrop::Flame,
        (Look::Smooth, Style::Shadow, Mode::Fire),
        Post::Bloom,
        Idle::Still,
    ),
    preset(
        None,
        Shader::Plasma,
        Backdrop::Kaleidoscope,
        (Look::Smooth, Style::Solid, Mode::Fluid),
        Post::Trails,
        Idle::Still,
    ),
    preset(
        Some("Glacier"),
        Shader::Marble,
        Backdrop::Aurora,
        (Look::Smooth, Style::Reflect, Mode::Flat),
        Post::Rays,
        Idle::Sway,
    ),
    preset(
        None,
        Shader::Rainbow,
        Backdrop::Lorenz,
        (Look::Braille, Style::Outline, Mode::Flat),
        Post::Echo,
        Idle::Wave,
    ),
    preset(
        Some("Toxic"),
        Shader::Pulse,
        Backdrop::Julia,
        (Look::Smooth, Style::Solid, Mode::Reaction),
        Post::Bloom,
        Idle::Still,
    ),
    preset(
        None,
        Shader::Glitter,
        Backdrop::Menger,
        (Look::Smooth, Style::Extrude, Mode::Flat),
        Post::Chroma,
        Idle::Bounce,
    ),
    preset(
        None,
        Shader::Stripes,
        Backdrop::Tunnel,
        (Look::Led, Style::Solid, Mode::Flat),
        Post::Grain,
        Idle::Wave,
    ),
    preset(
        None,
        Shader::Sheen,
        Backdrop::Mandelbrot,
        (Look::Smooth, Style::Neon, Mode::Swarm),
        Post::Rays,
        Idle::Still,
    ),
];

/// A random item, avoiding `avoid` when anything else is left.
pub fn fresh<T: Copy + PartialEq>(items: &[T], avoid: &[T]) -> Option<T> {
    let choices: Vec<T> = items
        .iter()
        .copied()
        .filter(|i| !avoid.contains(i))
        .collect();
    let from = if choices.is_empty() {
        items.to_vec()
    } else {
        choices
    };
    from.get(fastrand::usize(..from.len().max(1))).copied()
}

impl Director {
    /// What `recent` scenes used, by `field`, leaving out `common`: the
    /// plain choices that are meant to come up often.
    fn used<T: Copy + PartialEq>(&self, field: fn(&Act) -> T, common: &[T], last: usize) -> Vec<T> {
        self.recent
            .iter()
            .rev()
            .take(last)
            .map(field)
            .filter(|v| !common.contains(v))
            .collect()
    }

    /// The scene after `current`.
    pub fn next(&mut self, current: &Act, fonts: &SystemFonts) -> Act {
        if self.recent.is_empty() {
            self.recent.push_back(current.clone());
        }
        let subject = subject(current, fonts);
        let act = if fastrand::f32() < 0.22 {
            self.preset(subject)
        } else {
            self.random(subject)
        };
        self.recent.push_back(act.clone());
        if self.recent.len() > MEMORY {
            self.recent.pop_front();
        }
        act
    }

    fn palette(&self) -> Palette {
        let names: Vec<&str> = self
            .recent
            .iter()
            .map(|a| a.palette.name.as_str())
            .collect();
        (0..8)
            .map(|_| Palette::random())
            .find(|p| !names.contains(&p.name.as_str()))
            .unwrap_or_else(Palette::generated)
    }

    fn random(&self, subject: Subject) -> Act {
        let mode = fresh(MODES, &self.used(|a| a.mode, &[Mode::Flat], 3)).unwrap_or(Mode::Flat);
        Act {
            subject,
            palette: self.palette(),
            shader: fresh(SHADERS, &self.used(|a| a.shader, &[], 3)).unwrap_or(Shader::Flow),
            backdrop: fresh(BACKDROPS, &self.used(|a| a.backdrop, &[], MEMORY))
                .unwrap_or(Backdrop::Vignette),
            look: fresh(LOOKS, &self.used(|a| a.look, &[Look::Smooth], 3)).unwrap_or(Look::Smooth),
            style: fresh(STYLES, &self.used(|a| a.style, &[Style::Solid], 2))
                .unwrap_or(Style::Solid),
            idle: if mode == Mode::Turntable {
                Idle::Still
            } else {
                fresh(IDLES, &self.used(|a| a.idle, &[], 2)).unwrap_or(Idle::Still)
            },
            mode,
            post: fresh(POSTS, &self.used(|a| a.post, &[Post::Clean], 2)).unwrap_or(Post::Clean),
            variant: fastrand::u32(..),
        }
    }

    /// A preset that didn't play recently.
    fn preset(&self, subject: Subject) -> Act {
        let recent: Vec<(Backdrop, Shader)> =
            self.recent.iter().map(|a| (a.backdrop, a.shader)).collect();
        let fresh_presets: Vec<&Preset> = PRESETS
            .iter()
            .filter(|p| !recent.contains(&(p.backdrop, p.shader)))
            .collect();
        let Some(p) = fresh_presets.get(fastrand::usize(..fresh_presets.len().max(1))) else {
            return self.random(subject);
        };
        let palette = p
            .theme
            .and_then(|name| THEMES.iter().find(|t| t.name == name))
            .map_or_else(|| self.palette(), Palette::theme);
        Act {
            subject,
            palette,
            shader: p.shader,
            backdrop: p.backdrop,
            look: p.look,
            style: p.style,
            idle: p.idle,
            mode: p.mode,
            post: p.post,
            variant: fastrand::u32(..),
        }
    }
}

/// Alternates logo and wordmark, sometimes both together.
fn subject(current: &Act, fonts: &SystemFonts) -> Subject {
    let word = || match fastrand::u8(0..6) {
        0 => Word::Small,
        1 => Word::Tiny,
        2 => Word::Pixel,
        _ => fonts.pick(stage::WORDMARK).map_or(Word::Pixel, Word::Face),
    };
    let logo = || {
        if fastrand::bool() {
            Logo::Mascot
        } else {
            Logo::Emblem
        }
    };
    match (&current.subject, fastrand::u8(0..4)) {
        (Subject::Lockup(..), n) if n < 2 => Subject::Logo(logo()),
        (Subject::Lockup(..), _) => Subject::Word(word()),
        (_, 0) => Subject::Lockup(logo(), word()),
        (Subject::Logo(_), _) => Subject::Word(word()),
        (Subject::Word(_), _) => Subject::Logo(logo()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_avoids_what_it_can() {
        for _ in 0..50 {
            assert_eq!(fresh(&[1, 2, 3], &[1, 3]), Some(2));
            assert!(fresh(&[1, 2], &[1, 2]).is_some());
        }
    }

    #[test]
    fn presets_name_real_themes() {
        for p in PRESETS {
            if let Some(name) = p.theme {
                assert!(THEMES.iter().any(|t| t.name == name), "{name}");
            }
        }
    }
}
