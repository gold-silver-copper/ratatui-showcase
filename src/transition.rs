//! Full-screen transitions from one scene to the next, drawn at sub-cell
//! resolution: masked wipes, warps of the whole frame, and particles that
//! carry the old art's pixels to the new art's.

use std::f32::consts::{PI, TAU};

use crate::backdrop::{Place, hash};
use crate::canvas::Canvas;
use crate::glyphs::{COLS, ROWS};
use crate::num;
use crate::stage::Stage;
use crate::theme::Rgb;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Radial,
    Diamond,
    Clock,
    Spiral,
    Diagonal,
    Blinds,
    Checker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The new scene spreads over the old behind a glowing edge.
    Wipe(Shape),
    /// The old scene burns away in noisy flames.
    Burn,
    /// Fly through the old scene; the new one springs up from afar.
    Zoom,
    /// Everything twists into a whirlpool and back out.
    Swirl,
    /// Rings of waves ripple out from the centre.
    Ripple,
    /// The frame breaks into ever bigger blocks and back.
    Mosaic,
    /// Bands slide off one way and the new ones slide in.
    Slices,
    /// A broken signal: torn rows, split colours, static.
    Glitch,
    /// An old television switching off and on.
    Crt,
    /// Every pixel of the old art flows to a place in the new.
    Morph,
    /// Pixels swarm around the screen before settling.
    Swarm,
    /// The old art explodes; the new flies in from all sides.
    Scatter,
    /// The old art falls away; the new pours in row by row.
    Pour,
    /// Into a black hole and out again.
    Vortex,
    /// The old art crumbles like sand; the new rises up.
    Crumble,
}

pub const KINDS: &[Kind] = &[
    Kind::Wipe(Shape::Radial),
    Kind::Wipe(Shape::Diamond),
    Kind::Wipe(Shape::Clock),
    Kind::Wipe(Shape::Spiral),
    Kind::Wipe(Shape::Diagonal),
    Kind::Wipe(Shape::Blinds),
    Kind::Wipe(Shape::Checker),
    Kind::Burn,
    Kind::Zoom,
    Kind::Swirl,
    Kind::Ripple,
    Kind::Mosaic,
    Kind::Slices,
    Kind::Glitch,
    Kind::Crt,
    Kind::Morph,
    Kind::Swarm,
    Kind::Scatter,
    Kind::Pour,
    Kind::Vortex,
    Kind::Crumble,
];

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Wipe(Shape::Radial) => "radial wipe",
            Kind::Wipe(Shape::Diamond) => "diamond wipe",
            Kind::Wipe(Shape::Clock) => "clock wipe",
            Kind::Wipe(Shape::Spiral) => "spiral wipe",
            Kind::Wipe(Shape::Diagonal) => "diagonal wipe",
            Kind::Wipe(Shape::Blinds) => "blinds",
            Kind::Wipe(Shape::Checker) => "checkerboard",
            Kind::Burn => "burn",
            Kind::Zoom => "zoom",
            Kind::Swirl => "swirl",
            Kind::Ripple => "ripple",
            Kind::Mosaic => "mosaic",
            Kind::Slices => "slices",
            Kind::Glitch => "glitch",
            Kind::Crt => "crt",
            Kind::Morph => "morph",
            Kind::Swarm => "swarm",
            Kind::Scatter => "scatter",
            Kind::Pour => "pour",
            Kind::Vortex => "vortex",
            Kind::Crumble => "crumble",
        }
    }

    fn duration(self) -> f32 {
        match self {
            Kind::Wipe(_) => 1.8,
            Kind::Glitch | Kind::Crt => 1.6,
            Kind::Zoom | Kind::Mosaic | Kind::Slices => 1.9,
            Kind::Burn | Kind::Swirl | Kind::Ripple => 2.2,
            Kind::Morph | Kind::Vortex | Kind::Scatter => 2.8,
            Kind::Swarm | Kind::Pour | Kind::Crumble => 3.2,
        }
    }

    fn particles(self) -> bool {
        matches!(
            self,
            Kind::Morph | Kind::Swarm | Kind::Scatter | Kind::Pour | Kind::Vortex | Kind::Crumble
        )
    }
}

#[derive(Clone, Copy)]
struct Point {
    x: f32,
    y: f32,
    seed: f32,
}

pub struct Crossing {
    pub kind: Kind,
    pub from: Stage,
    elapsed: f32,
    /// Particle size in samples.
    grain: f32,
    sources: Vec<Point>,
    targets: Vec<Point>,
}

/// Particles per scene; more cost time, fewer look coarse.
const MAX_PARTICLES: usize = 24_000;

impl Crossing {
    pub fn new(kind: Kind, from: Stage, to: &Stage) -> Self {
        let mut grain = 2.0;
        let (mut sources, mut targets) = (Vec::new(), Vec::new());
        if kind.particles() {
            loop {
                sources = points(&from, grain);
                targets = points(to, grain);
                if sources.len().max(targets.len()) <= MAX_PARTICLES {
                    break;
                }
                grain += 1.0;
            }
            // Morphs pair pixels in reading order, so shapes flow sideways
            // into each other; everything else pairs at random.
            let key = |p: &Point| (num::floor_u32(p.x / grain), num::floor_u32(p.y / grain));
            if kind == Kind::Morph {
                sources.sort_by_key(key);
                targets.sort_by_key(key);
            }
        }
        Self {
            kind,
            from,
            elapsed: 0.0,
            grain,
            sources,
            targets,
        }
    }

    pub fn advance(&mut self, dt: f32) {
        self.elapsed += dt;
    }

    pub fn done(&self) -> bool {
        self.elapsed >= self.kind.duration()
    }

    pub fn render(&self, canvas: &mut Canvas, to: &Stage, t: f32) {
        let p = (self.elapsed / self.kind.duration()).clamp(0.0, 1.0);
        let from = &self.from;
        let (w, h) = (canvas.width(), canvas.height());
        let (cx, cy) = (w / 2.0, h / 2.0);
        let accent = to.act.theme().accent;
        let identity = |x: f32, y: f32| Some((x, y));
        let none = |c: Rgb| c;

        match self.kind {
            Kind::Wipe(_) | Kind::Burn => {
                let edge = if self.kind == Kind::Burn { 0.16 } else { 0.1 };
                let field = |x: f32, y: f32| self.field(x / w, y / h, w / h);
                let k = |x: f32, y: f32| ((p * (1.0 + edge) - field(x, y)) / edge).clamp(0.0, 1.0);
                backdrops(canvas, from, to, t, |x, y| k(x * w, y * h));
                canvas.paint(from, t, identity, |x, y| 1.0 - k(x, y), none);
                canvas.paint(to, t, identity, k, none);
                let fire = self.kind == Kind::Burn;
                canvas.light(|x, y| {
                    let band = 4.0 * k(x, y) * (1.0 - k(x, y));
                    let color = if fire {
                        Rgb::hex(0xff5a00).lerp(Rgb::hex(0xffe08a), band)
                    } else {
                        accent
                    };
                    color.scale(band * band * 0.7)
                });
            }
            Kind::Zoom => {
                let out = 1.0 + 3.0 * ease_in(p / 0.6);
                let inn = 0.15 + 0.85 * ease_out_back(((p - 0.35) / 0.65).clamp(0.0, 1.0));
                let mix = smooth(0.35, 0.55, p);
                backdrops(canvas, from, to, t, |_, _| mix);
                let scaled =
                    |s: f32| move |x: f32, y: f32| Some((cx + (x - cx) / s, cy + (y - cy) / s));
                canvas.paint(from, t, scaled(out), |_, _| 1.0 - smooth(0.3, 0.6, p), none);
                canvas.paint(to, t, scaled(inn), |_, _| mix, none);
                let flash = (-((p - 0.45) * 9.0).powi(2)).exp() * 0.3;
                canvas.light(|x, y| accent.scale(flash * (1.0 - distance(x - cx, y - cy, w, h))));
            }
            Kind::Swirl | Kind::Ripple => {
                let strength = (p * PI).sin();
                let radius = w.max(h) * 0.6;
                let swirl = self.kind == Kind::Swirl;
                let warp = move |x: f32, y: f32| {
                    let (dx, dy) = (x - cx, y - cy);
                    let r = (dx * dx + dy * dy).sqrt();
                    if swirl {
                        let angle = strength * TAU * 1.25 * (1.0 - r / radius).max(0.0).powi(2);
                        let (sin, cos) = angle.sin_cos();
                        Some((cx + dx * cos - dy * sin, cy + dx * sin + dy * cos))
                    } else {
                        let wave = strength * w * 0.035 * (r / (w * 0.08) * TAU - p * 14.0).sin();
                        let scale = if r > 0.0 { (r + wave) / r } else { 1.0 };
                        Some((cx + dx * scale, cy + dy * scale))
                    }
                };
                let mix = smooth(0.4, 0.6, p);
                backdrops(canvas, from, to, t, |_, _| mix);
                canvas.paint(from, t, warp, |_, _| 1.0 - mix, none);
                canvas.paint(to, t, warp, |_, _| mix, none);
            }
            Kind::Mosaic => {
                let block = 1.0 + (p * PI).sin().powf(1.2) * w.min(h) * 0.09;
                let warp = move |x: f32, y: f32| {
                    Some((
                        ((x / block).floor() + 0.5) * block,
                        ((y / block).floor() + 0.5) * block,
                    ))
                };
                let mix = smooth(0.45, 0.55, p);
                backdrops(canvas, from, to, t, |_, _| mix);
                canvas.paint(from, t, warp, |_, _| 1.0 - mix, none);
                canvas.paint(to, t, warp, |_, _| mix, none);
            }
            Kind::Slices => {
                let bands = 10.0;
                let band = move |y: f32| (y / h * bands).floor().clamp(0.0, bands - 1.0);
                let dir = move |y: f32| {
                    if num::floor_u32(band(y)).is_multiple_of(2) {
                        1.0
                    } else {
                        -1.0
                    }
                };
                let delay = move |y: f32| band(y) / bands * 0.25;
                let out = move |y: f32| ease_in(((p - delay(y)) / 0.45).clamp(0.0, 1.0));
                let inn = move |y: f32| ease_out(((p - 0.3 - delay(y)) / 0.45).clamp(0.0, 1.0));
                backdrops(canvas, from, to, t, |_, y| inn(y * h));
                canvas.paint(
                    from,
                    t,
                    |x, y| Some((x - dir(y) * out(y) * w * 1.1, y)),
                    |_, _| 1.0,
                    none,
                );
                canvas.paint(
                    to,
                    t,
                    |x, y| Some((x + dir(y) * (1.0 - inn(y)) * w * 1.1, y)),
                    |_, _| 1.0,
                    none,
                );
            }
            Kind::Glitch => {
                let step = num::floor_u32(t * 24.0);
                let intensity = (p * PI).sin().powf(0.7);
                let band = move |y: f32| num::floor_u32(y / (num::float(ROWS) * 1.5));
                let torn = move |y: f32| hash(band(y), step) < intensity * 0.6;
                let shift = move |y: f32| {
                    if torn(y) {
                        (hash(band(y), step.wrapping_add(7)) - 0.5) * w * 0.3 * intensity
                    } else {
                        0.0
                    }
                };
                let shows_to = move |y: f32| {
                    p + (hash(band(y), step.wrapping_add(13)) - 0.5) * 0.5 * intensity >= 0.5
                };
                let warp = move |x: f32, y: f32| Some((x - shift(y), y));
                backdrops(
                    canvas,
                    from,
                    to,
                    t,
                    |_, y| if shows_to(y * h) { 1.0 } else { 0.0 },
                );
                let tint = move |y: f32| match num::floor_u32(
                    hash(band(y), step.wrapping_add(21)) * 3.0,
                ) {
                    0 if torn(y) => Rgb(1.0, 0.25, 0.35),
                    1 if torn(y) => Rgb(0.25, 1.0, 0.9),
                    _ => Rgb::WHITE,
                };
                canvas.paint(
                    from,
                    t,
                    warp,
                    |_, y| if shows_to(y) { 0.0 } else { 1.0 },
                    none,
                );
                canvas.paint(
                    to,
                    t,
                    warp,
                    |_, y| if shows_to(y) { 1.0 } else { 0.0 },
                    none,
                );
                canvas.light(|x, y| {
                    let noise = hash(
                        num::floor_u32(x / 2.0).wrapping_add(step.wrapping_mul(7919)),
                        num::floor_u32(y / 2.0),
                    );
                    let line = hash(band(y), step.wrapping_add(31)) < intensity * 0.08;
                    let static_ = if noise < intensity * 0.05 { 0.5 } else { 0.0 };
                    tint(y).scale(if line { 0.25 } else { 0.0 } + static_)
                });
            }
            Kind::Crt => {
                // Squash to a line, then to a dot; then the reverse.
                let (stage, q) = if p < 0.5 {
                    (from, (p / 0.45).min(1.0))
                } else {
                    (to, ((1.0 - p) / 0.45).min(1.0))
                };
                let sy = if q < 0.6 {
                    1.0 - ease_in(q / 0.6) * 0.985
                } else {
                    0.015
                };
                let sx = if q < 0.6 {
                    1.0
                } else {
                    1.0 - ease_in((q - 0.6) / 0.4) * 0.99
                };
                let inside = move |x: f32, y: f32| {
                    (x - cx).abs() < w / 2.0 * sx && (y - cy).abs() < h / 2.0 * sy
                };
                let fade = 1.0 - q;
                canvas.backdrop(|x, y, px, py| {
                    if inside(x * w, y * h) {
                        stage
                            .act
                            .backdrop
                            .color(stage.act.theme(), Place { x, y, px, py }, aspect(stage), t)
                            .scale(fade)
                    } else {
                        Rgb::BLACK
                    }
                });
                let warp = move |x: f32, y: f32| {
                    inside(x, y).then(|| (cx + (x - cx) / sx, cy + (y - cy) / sy))
                };
                canvas.paint(stage, t, warp, |_, _| 1.0, |c| c.lerp(Rgb::WHITE, q * 0.8));
                let thickness = (sy * h / 2.0).max(1.5);
                canvas.light(|x, y| {
                    let across = (1.0 - ((x - cx).abs() / (w / 2.0 * sx + 2.0)).powi(4)).max(0.0);
                    let line = (-((y - cy) / thickness).powi(2)).exp();
                    Rgb::WHITE.scale(line * across * q * 0.9)
                });
            }
            Kind::Morph
            | Kind::Swarm
            | Kind::Scatter
            | Kind::Pour
            | Kind::Vortex
            | Kind::Crumble => {
                self.render_particles(canvas, to, t, p);
            }
        }
    }

    fn render_particles(&self, canvas: &mut Canvas, to: &Stage, t: f32, p: f32) {
        let from = &self.from;
        let (w, h) = (canvas.width(), canvas.height());
        let (cx, cy) = (w / 2.0, h / 2.0);
        let grain = self.grain;
        backdrops(canvas, from, to, t, |_, _| smooth(0.2, 0.8, p));
        // Hand over smoothly between the art and its particles.
        let identity = |x: f32, y: f32| Some((x, y));
        canvas.paint(from, t, identity, |_, _| 1.0 - smooth(0.0, 0.06, p), |c| c);
        canvas.paint(to, t, identity, |_, _| smooth(0.94, 1.0, p), |c| c);
        let color = |stage: &Stage, point: &Point| {
            stage
                .shade(point.x, point.y, t, num::floor_usize(point.y / 4.0))
                .ink
        };

        match self.kind {
            Kind::Morph | Kind::Swarm => {
                let n = self.sources.len().max(self.targets.len());
                let swarm = self.kind == Kind::Swarm;
                for i in 0..n {
                    let a = self.sources.get(
                        i.saturating_mul(self.sources.len())
                            .checked_div(n)
                            .unwrap_or(0),
                    );
                    let b = self.targets.get(
                        i.saturating_mul(self.targets.len())
                            .checked_div(n)
                            .unwrap_or(0),
                    );
                    let (Some(a), Some(b)) = (a, b) else { continue };
                    let spread = if swarm { 0.4 } else { 0.25 };
                    let q = ease_in_out(((p - a.seed * spread) / (1.0 - spread)).clamp(0.0, 1.0));
                    let (x, y) = if swarm {
                        let (kx, ky) = (hash(num::u32_of(i), 1) * w, hash(num::u32_of(i), 2) * h);
                        let (u, v) = ((1.0 - q) * (1.0 - q), 2.0 * (1.0 - q) * q);
                        (
                            u * a.x + v * kx + q * q * b.x,
                            u * a.y + v * ky + q * q * b.y,
                        )
                    } else {
                        let (dx, dy) = (b.x - a.x, b.y - a.y);
                        let curve = (b.seed - 0.5) * 0.5 * (q * PI).sin();
                        (a.x + dx * q - dy * curve, a.y + dy * q + dx * curve)
                    };
                    let c = color(from, a).mix(color(to, b), q);
                    canvas.splat(x, y, grain, c, 1.0);
                }
            }
            Kind::Scatter => {
                let out = (p / 0.6).clamp(0.0, 1.0);
                for (i, a) in self.sources.iter().enumerate() {
                    let (dx, dy) = (a.x - cx, a.y - cy);
                    let len = (dx * dx + dy * dy).sqrt().max(1.0);
                    let speed = (0.5 + a.seed) * w * 0.7;
                    let jitter = (hash(num::u32_of(i), 3) - 0.5) * 0.8;
                    let (vx, vy) = (dx / len + jitter, dy / len - 0.6 + jitter);
                    let (x, y) = (
                        a.x + vx * speed * out,
                        a.y + vy * speed * out + h * 1.5 * out * out,
                    );
                    canvas.splat(x, y, grain, color(from, a), 1.0 - out);
                }
                for (i, b) in self.targets.iter().enumerate() {
                    let q = ease_out(((p - 0.3 - b.seed * 0.25) / 0.45).clamp(0.0, 1.0));
                    let angle = hash(num::u32_of(i), 4) * TAU;
                    let (sx, sy) = (cx + angle.cos() * w, cy + angle.sin() * h);
                    canvas.splat(
                        sx + (b.x - sx) * q,
                        sy + (b.y - sy) * q,
                        grain,
                        color(to, b),
                        q.min(1.0),
                    );
                }
            }
            Kind::Pour | Kind::Crumble => {
                let crumble = self.kind == Kind::Crumble;
                for a in &self.sources {
                    let column = hash(num::floor_u32(a.x / (grain * 3.0)), 5);
                    let delay = if crumble {
                        column * 0.35 + (1.0 - a.y / h) * 0.1
                    } else {
                        a.seed * 0.3
                    };
                    let fall = ((p - delay) / 0.45).clamp(0.0, 1.0);
                    let drift = if crumble {
                        (a.seed - 0.5) * fall * w * 0.06
                    } else {
                        0.0
                    };
                    canvas.splat(
                        a.x + drift,
                        a.y + fall * fall * h * 1.3,
                        grain,
                        color(from, a),
                        1.0,
                    );
                }
                for b in &self.targets {
                    let (delay, q) = if crumble {
                        let delay = 0.35 + b.y / h * 0.25 + b.seed * 0.1;
                        (delay, ease_out(((p - delay) / 0.4).clamp(0.0, 1.0)))
                    } else {
                        let delay = 0.3 + (1.0 - b.y / h) * 0.4 + b.seed * 0.05;
                        (delay, ease_out_bounce(((p - delay) / 0.28).clamp(0.0, 1.0)))
                    };
                    if p < delay {
                        continue;
                    }
                    let start = if crumble { h + grain } else { -grain - h * 0.1 };
                    canvas.splat(b.x, start + (b.y - start) * q, grain, color(to, b), 1.0);
                }
            }
            Kind::Vortex => {
                let spiral = |point: &Point, q: f32, inward: bool| {
                    let (dx, dy) = (point.x - cx, point.y - cy);
                    let r = (dx * dx + dy * dy).sqrt();
                    let turn = if inward {
                        q * TAU * 1.5
                    } else {
                        -(1.0 - q) * TAU * 1.5
                    };
                    let angle = dy.atan2(dx) + turn;
                    let radius = if inward { r * (1.0 - q) } else { r * q };
                    (cx + angle.cos() * radius, cy + angle.sin() * radius)
                };
                for a in &self.sources {
                    let q = ease_in(((p - a.seed * 0.2) / 0.5).clamp(0.0, 1.0));
                    let (x, y) = spiral(a, q, true);
                    canvas.splat(x, y, grain, color(from, a), 1.0 - q * q);
                }
                for b in &self.targets {
                    let q = ease_out(((p - 0.45 - b.seed * 0.15) / 0.45).clamp(0.0, 1.0));
                    if q > 0.0 {
                        let (x, y) = spiral(b, q, false);
                        canvas.splat(x, y, grain, color(to, b), q.sqrt());
                    }
                }
            }
            Kind::Wipe(_)
            | Kind::Burn
            | Kind::Zoom
            | Kind::Swirl
            | Kind::Ripple
            | Kind::Mosaic
            | Kind::Slices
            | Kind::Glitch
            | Kind::Crt => {}
        }
    }

    /// When each place gets wiped, in `0.0..1.0`. `(x, y)` are in `0.0..1.0`;
    /// `aspect` is width over height in samples.
    fn field(&self, x: f32, y: f32, aspect: f32) -> f32 {
        let (dx, dy) = ((x - 0.5) * aspect, y - 0.5);
        let corner = (0.25 * aspect * aspect + 0.25).sqrt();
        let radius = (dx * dx + dy * dy).sqrt() / corner;
        let angle = (dy.atan2(dx) / TAU + 0.25).rem_euclid(1.0);
        match self.kind {
            Kind::Wipe(Shape::Radial) => radius,
            Kind::Wipe(Shape::Diamond) => (dx.abs() + dy.abs()) / (0.5 * aspect + 0.5),
            Kind::Wipe(Shape::Clock) => angle,
            Kind::Wipe(Shape::Spiral) => {
                0.55 * radius + 0.45 * (angle + radius * 1.5).rem_euclid(1.0)
            }
            Kind::Wipe(Shape::Diagonal) => (x + y) / 2.0,
            Kind::Wipe(Shape::Blinds) => (y * 8.0).fract() * 0.7 + x * 0.3,
            Kind::Wipe(Shape::Checker) => {
                hash(num::floor_u32(x * 16.0), num::floor_u32(y * 9.0)) * 0.8 + x * 0.2
            }
            // Burn; the other kinds don't wipe.
            Kind::Burn
            | Kind::Zoom
            | Kind::Swirl
            | Kind::Ripple
            | Kind::Mosaic
            | Kind::Slices
            | Kind::Glitch
            | Kind::Crt
            | Kind::Morph
            | Kind::Swarm
            | Kind::Scatter
            | Kind::Pour
            | Kind::Vortex
            | Kind::Crumble => {
                let noise = value_noise(x * 5.0 * aspect, y * 5.0) * 0.65
                    + value_noise(x * 13.0 * aspect, y * 13.0) * 0.35;
                (noise * 1.4 - 0.2).clamp(0.0, 1.0)
            }
        }
    }
}

/// Mixes the two scenes' backgrounds: `to`'s share at each `(x, y)` in
/// `0.0..1.0` is `mix(x, y)`.
fn backdrops(canvas: &mut Canvas, from: &Stage, to: &Stage, t: f32, mix: impl Fn(f32, f32) -> f32) {
    let (a, b) = (from.act.backdrop, to.act.backdrop);
    let (theme_a, theme_b) = (from.act.theme(), to.act.theme());
    let aspect = aspect(to);
    canvas.backdrop(|x, y, px, py| {
        let k = mix(x, y);
        if k <= 0.0 {
            return a.color(theme_a, Place { x, y, px, py }, aspect, t);
        }
        if k >= 1.0 {
            return b.color(theme_b, Place { x, y, px, py }, aspect, t);
        }
        a.color(theme_a, Place { x, y, px, py }, aspect, t)
            .mix(b.color(theme_b, Place { x, y, px, py }, aspect, t), k)
    });
}

/// The screen's width over its height.
pub fn aspect(stage: &Stage) -> f32 {
    f32::from(stage.cols) / (f32::from(stage.rows.max(1)) * stage.cell_ratio)
}

/// The art's pixels, `grain` samples apart.
fn points(stage: &Stage, grain: f32) -> Vec<Point> {
    let (left, top, right, bottom) = stage.bounds();
    let mut points = Vec::new();
    let mut y = top.max(0.0);
    while y < bottom.min((stage.rows as usize * ROWS) as f32) {
        let mut x = left.max(0.0);
        while x < right.min((stage.cols as usize * COLS) as f32) {
            if stage.coverage(x + grain / 2.0, y + grain / 2.0, 0.0) > 0.5 {
                let seed = hash(num::floor_u32(x), num::floor_u32(y).wrapping_add(17));
                points.push(Point {
                    x: x + grain / 2.0,
                    y: y + grain / 2.0,
                    seed,
                });
            }
            x += grain;
        }
        y += grain;
    }
    points
}

fn distance(dx: f32, dy: f32, w: f32, h: f32) -> f32 {
    ((dx / w).powi(2) + (dy / h).powi(2)).sqrt().min(1.0)
}

fn value_noise(x: f32, y: f32) -> f32 {
    let (ix, iy) = (x.floor(), y.floor());
    let (fx, fy) = (x - ix, y - iy);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let corner = |dx: f32, dy: f32| hash(num::floor_u32(ix + dx), num::floor_u32(iy + dy));
    let top = corner(0.0, 0.0) + (corner(1.0, 0.0) - corner(0.0, 0.0)) * sx;
    let bottom = corner(0.0, 1.0) + (corner(1.0, 1.0) - corner(0.0, 1.0)) * sx;
    top + (bottom - top) * sy
}

fn smooth(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn ease_in(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * t
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

fn ease_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

fn ease_out_back(t: f32) -> f32 {
    let (c1, c3) = (1.70158, 2.70158);
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

fn ease_out_bounce(t: f32) -> f32 {
    let (n, d) = (7.5625, 2.75);
    if t < 1.0 / d {
        n * t * t
    } else if t < 2.0 / d {
        let t = t - 1.5 / d;
        n * t * t + 0.75
    } else if t < 2.5 / d {
        let t = t - 2.25 / d;
        n * t * t + 0.9375
    } else {
        let t = t - 2.625 / d;
        n * t * t + 0.984_375
    }
}

/// A scene at rest.
pub fn still(canvas: &mut Canvas, stage: &Stage, t: f32) {
    let (backdrop, theme, aspect) = (stage.act.backdrop, stage.act.theme(), aspect(stage));
    canvas.backdrop(|x, y, px, py| backdrop.color(theme, Place { x, y, px, py }, aspect, t));
    canvas.paint(stage, t, |x, y| Some((x, y)), |_, _| 1.0, |c| c);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easings_start_at_zero_and_end_at_one() {
        for ease in [
            ease_in,
            ease_out,
            ease_in_out,
            ease_out_back,
            ease_out_bounce,
        ] {
            assert!(ease(0.0).abs() < 1e-3);
            assert!((ease(1.0) - 1.0).abs() < 1e-3);
        }
    }
}
