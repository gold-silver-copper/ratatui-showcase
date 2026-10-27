//! Aiming the Mandelbrot deep zoom. The zoom's orbit trap is the art itself
//! laid over the plane (`backdrop.wgsl`): where an escaping orbit passes
//! through the art, the pixel lights. Deep in the set, the orbits of a small
//! patch of the plane follow each other until, at some step `n`, the patch
//! is stretched over the whole trap at once; if the map from the patch to
//! the trap is nearly linear there, the screen shows a clear copy of the art.
//!
//! So the search works backwards from the trap. It takes a point on the
//! set's boundary, finds the step at which its orbit has been stretched by
//! the factor a copy of the right size needs, and solves, by Newton's
//! method, for the nearby point whose orbit lands on the art's centre at
//! that step. The view is laid out so that point sits in the largest space
//! the art leaves free, turned so the copy stands upright, and then checked:
//! a small render on the CPU, done the way the GPU does it, must light a
//! copy that matches the art. All of it runs off the frame path, while the
//! scene before holds, within a budget of time and of tries; when nothing
//! good turns up, the scene keeps the endless zoom.

use std::time::{Duration, Instant};

use crate::num;

/// Where the trap lies in the plane: the screen laid over it, `SPAN` high
/// around `TRAP` (as in `backdrop.wgsl`'s `mandelbrot`). An aimed dive
/// shrinks it on the way, so the art spans at most `REACH` of the plane
/// either way from its centre: wider, the last steps bend the copy.
const TRAP: C = C { re: 0.0, im: 1.2 };
const SPAN: f64 = 0.6;
const REACH: f64 = 0.22;
/// Escape radius, squared, and how many times a pixel looks for the trap.
const ESCAPE: f64 = 16.0;
const CHECKS: u32 = 96;
/// The most iterations the GPU spends on a pixel.
pub const MAX_LIMIT: u32 = 3000;
/// The deepest and the shallowest end of a dive, as `ln(3 / height)`.
const DEPTHS: (f64, f64) = (9.0, 16.0);
/// The check's resolution, in pixels across the screen's height.
const CHECK_ROWS: usize = 24;
/// The copy's resolution in the check, in pixels across its height.
const COPY_ROWS: usize = 36;

#[derive(Debug, Clone, Copy, PartialEq)]
struct C {
    re: f64,
    im: f64,
}

impl C {
    const ZERO: C = C { re: 0.0, im: 0.0 };

    fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }
    fn add(self, o: C) -> C {
        C::new(self.re + o.re, self.im + o.im)
    }
    fn sub(self, o: C) -> C {
        C::new(self.re - o.re, self.im - o.im)
    }
    fn mul(self, o: C) -> C {
        C::new(
            self.re * o.re - self.im * o.im,
            self.re * o.im + self.im * o.re,
        )
    }
    fn div(self, o: C) -> C {
        let d = o.norm().max(1e-300);
        C::new(
            (self.re * o.re + self.im * o.im) / d,
            (self.im * o.re - self.re * o.im) / d,
        )
    }
    fn scale(self, k: f64) -> C {
        C::new(self.re * k, self.im * k)
    }
    /// The square of the length.
    fn norm(self) -> f64 {
        self.re * self.re + self.im * self.im
    }
    fn abs(self) -> f64 {
        self.norm().sqrt()
    }
    fn arg(self) -> f64 {
        self.im.atan2(self.re)
    }
    /// Turned by `a` radians.
    fn turn(self, a: f64) -> C {
        let (s, c) = a.sin_cos();
        C::new(self.re * c - self.im * s, self.re * s + self.im * c)
    }
}

/// Where the endless zoom heads: deep in seahorse valley.
const SEAHORSE: C = C {
    re: -0.743_643_887_037_158_7,
    im: 0.131_825_904_205_311_97,
};

/// The endless zoom's reference orbit, as `re, im` f32 pairs. Each pixel
/// then only follows its tiny difference from it, which f32 can hold far
/// deeper than it could hold the pixel's own position.
pub fn endless() -> Vec<u32> {
    reference(SEAHORSE, 6000)
}

/// A dive the GPU can follow: the reference orbit at the view's centre,
/// and how the view ends.
#[derive(Debug, Clone)]
pub struct Aim {
    /// `re, im` f32 pairs.
    pub orbit: Vec<u32>,
    /// The view's height in the plane at the end.
    pub scale: f32,
    /// How far the view turns on the way, so the copy ends upright.
    pub turn: f32,
    /// The trap's height in the plane at the end.
    pub span: f32,
    /// Iterations per pixel at the end.
    pub limit: u32,
    /// Seconds until the view arrives.
    pub dive: f32,
}

/// The art as the search sees it, in samples of the screen.
pub struct Picture<'a> {
    /// The screen's size in samples, and its width over its height.
    pub width: f64,
    pub height: f64,
    pub aspect: f64,
    /// The art's box: `(left, top, right, bottom)`.
    pub art: (f64, f64, f64, f64),
    /// Whether the art covers a sample.
    pub ink: &'a dyn Fn(f64, f64) -> bool,
}

/// Where on the screen the copy goes and how big, in screen heights from
/// the centre; and the art's centre in the plane, and its height.
struct Layout {
    at: C,
    size: f64,
    centre: C,
    art: f64,
    /// The art's width over its height.
    wide: f64,
    /// The trap's height in the plane at the end.
    span: f64,
}

impl Picture<'_> {
    /// The largest space the art leaves free, beside it or above or below,
    /// and how big a copy of the same proportions fits there.
    fn layout(&self) -> Option<Layout> {
        let (l, t, r, b) = self.art;
        let (aw, ah) = ((r - l).max(1.0), (b - t).max(1.0));
        let (w, h) = (self.width, self.height);
        // A little room for the art's glow.
        let gap = h * 0.04;
        let (rl, rt, rr, rb) = (
            l - gap / (self.aspect * h / w),
            t - gap,
            r + gap / (self.aspect * h / w),
            b + gap,
        );
        // Bands: left, right, above, below; each its box in samples.
        let bands = [
            (0.0, 0.0, rl.max(0.0), h),
            (rr.min(w), 0.0, w, h),
            (0.0, 0.0, w, rt.max(0.0)),
            (0.0, rb.min(h), w, h),
        ];
        // Samples aren't square: the aspect turns widths into heights.
        let across = self.aspect * h / w;
        let fit = |(x0, y0, x1, y1): (f64, f64, f64, f64)| {
            let (bw, bh) = ((x1 - x0) * across, y1 - y0);
            let size = (bh.min(bw * ah / (aw * across)) * 0.85).min(h * 0.55);
            (size, ((x0 + x1) / 2.0, (y0 + y1) / 2.0))
        };
        let (size, (x, y)) = bands
            .into_iter()
            .map(fit)
            .max_by(|a, b| a.0.total_cmp(&b.0))?;
        // Too small to read from across a room.
        if size < h * 0.16 {
            return None;
        }
        let art_centre = ((l + r) / 2.0, (t + b) / 2.0);
        let (art, wide) = (ah / h, aw * across / ah);
        let span = SPAN.min(REACH * 2.0 / (art * wide.max(1.0)));
        Some(Layout {
            at: self.offset(x, y),
            size: size / h,
            centre: TRAP.add(self.offset(art_centre.0, art_centre.1).scale(span)),
            art,
            wide,
            span,
        })
    }

    /// Sample `(x, y)` as an offset from the screen's centre in screen
    /// heights, as the shaders lay the screen over the plane.
    fn offset(&self, x: f64, y: f64) -> C {
        C::new((x / self.width - 0.5) * self.aspect, y / self.height - 0.5)
    }

    /// Whether the orbit point `z` lands on the art in a trap `span` high.
    fn trapped(&self, z: C, span: f64) -> bool {
        let q = z.sub(TRAP).scale(1.0 / span);
        let (x, y) = (
            (q.re / self.aspect + 0.5) * self.width,
            (q.im + 0.5) * self.height,
        );
        x >= 0.0 && y >= 0.0 && x < self.width && y < self.height && (self.ink)(x, y)
    }
}

/// `c`'s orbit and its derivative by `c`, at step `n`; `None` if it escapes
/// first.
fn orbit_at(c: C, n: u32) -> Option<(C, C)> {
    let (mut z, mut d) = (C::ZERO, C::ZERO);
    for _ in 0..n {
        d = z.mul(d).scale(2.0).add(C::new(1.0, 0.0));
        z = z.mul(z).add(c);
        if z.norm() > 1e10 {
            return None;
        }
    }
    Some((z, d))
}

/// How far step `n` bends a patch around `c` that it stretches over
/// `reach` of the plane: the second derivative's share of the change across
/// the patch, 0 for a perfectly straight map. A copy drawn through a bent
/// map is bent too.
fn bend(c: C, n: u32, reach: f64) -> f64 {
    let (mut z, mut d, mut dd) = (C::ZERO, C::ZERO, C::ZERO);
    for _ in 0..n {
        dd = d.mul(d).add(z.mul(dd)).scale(2.0);
        d = z.mul(d).scale(2.0).add(C::new(1.0, 0.0));
        z = z.mul(z).add(c);
    }
    // Across the patch, c moves by reach / |d|.
    dd.abs() * reach / d.norm().max(1e-300)
}

/// The step at which `c`'s orbit, stretched by about `stretch` (within a
/// factor of four), comes closest to `target`, within `max` steps.
fn closest(c: C, stretch: f64, target: C, max: u32) -> Option<u32> {
    let (mut z, mut d) = (C::ZERO, C::ZERO);
    let mut best: Option<(f64, u32)> = None;
    for n in 0..max {
        if d.abs() > stretch * 4.0 {
            break;
        }
        if d.abs() >= stretch / 4.0 {
            let miss = z.sub(target).abs();
            if best.is_none_or(|(m, _)| miss < m) {
                best = Some((miss, n));
            }
        }
        d = z.mul(d).scale(2.0).add(C::new(1.0, 0.0));
        z = z.mul(z).add(c);
        if z.norm() > ESCAPE {
            break;
        }
    }
    best.map(|(_, n)| n)
}

/// The point near `c` whose orbit is at `target` at step `n`: Newton's
/// method, each step moving the orbit's point by at most a little, so it
/// follows the patch around `c` as it stretches (a continuation).
fn newton(mut c: C, n: u32, target: C) -> Option<C> {
    let (mut z, mut d) = orbit_at(c, n)?;
    let mut reach = 0.1;
    for _ in 0..200 {
        let miss = z.sub(target);
        if miss.abs() < 1e-7 {
            return Some(c);
        }
        let step = if miss.abs() > reach {
            miss.scale(reach / miss.abs())
        } else {
            miss
        };
        let next = c.sub(step.div(d));
        match orbit_at(next, n) {
            Some((nz, nd)) if nz.sub(target).abs() < miss.abs() => {
                (c, z, d) = (next, nz, nd);
                reach = (reach * 1.5).min(0.3);
            }
            _ => {
                reach *= 0.5;
                if reach < 1e-4 {
                    return None;
                }
            }
        }
    }
    None
}

/// Whether `c` stays bounded for `steps` steps.
fn bounded(c: C, steps: u32) -> bool {
    let mut z = C::ZERO;
    for _ in 0..steps {
        z = z.mul(z).add(c);
        if z.norm() > ESCAPE {
            return false;
        }
    }
    true
}

/// A point just outside the set, near its boundary: bisected between a
/// point inside the main cardioid or the period-2 bulb and one far outside,
/// as close as escaping within `steps` steps can tell.
fn boundary(rng: &mut fastrand::Rng, steps: u32) -> Option<C> {
    let a = rng.f64() * std::f64::consts::TAU;
    let inside = if rng.u8(0..4) == 0 {
        C::new(-1.0, 0.0).add(C::new(a.cos(), a.sin()).scale(0.24 * rng.f64().sqrt()))
    } else {
        // w - w^2 for |w| < 1/2 fills the main cardioid.
        let w = C::new(a.cos(), a.sin()).scale(0.49 * rng.f64().sqrt());
        w.sub(w.mul(w))
    };
    let b = rng.f64() * std::f64::consts::TAU;
    let mut outside = inside.add(C::new(b.cos(), b.sin()).scale(3.0));
    let mut inner = inside;
    for _ in 0..56 {
        let mid = inner.add(outside).scale(0.5);
        if bounded(mid, steps) {
            inner = mid;
        } else {
            outside = mid;
        }
    }
    Some(outside)
}

/// `c`'s orbit as f32 pairs, as far as it goes before escaping.
fn reference(c: C, steps: u32) -> Vec<u32> {
    let mut z = C::ZERO;
    let mut orbit = Vec::with_capacity(usize::try_from(steps).unwrap_or(0).saturating_mul(2));
    for _ in 0..steps {
        orbit.extend([num::narrow(z.re).to_bits(), num::narrow(z.im).to_bits()]);
        z = z.mul(z).add(c);
        if z.norm() > ESCAPE {
            break;
        }
    }
    orbit
}

/// A view of the plane around the reference orbit's point: its height,
/// turn and iterations.
struct View {
    scale: f64,
    span: f64,
    turn: f64,
    limit: u32,
}

/// A part of the screen the check renders: its centre and half its size
/// in screen heights from the screen's centre, and its pixels.
struct Window {
    centre: C,
    half: C,
    cols: usize,
    rows: usize,
}

impl Window {
    /// Each pixel's centre, row by row.
    fn pixels(&self) -> impl Iterator<Item = C> + '_ {
        (0..self.rows).flat_map(move |y| {
            (0..self.cols).map(move |x| {
                let u = (x as f64 + 0.5) / self.cols as f64 * 2.0 - 1.0;
                let v = (y as f64 + 0.5) / self.rows as f64 * 2.0 - 1.0;
                self.centre.add(C::new(u * self.half.re, v * self.half.im))
            })
        })
    }
}

/// What the check found: which pixels light, as the GPU decides it, and
/// how many lie near the set's boundary, where its detail is.
struct Check {
    lit: Vec<bool>,
    edge: usize,
}

/// Renders `window` of the view: each pixel follows its difference from
/// the reference orbit, starting over from the reference when that
/// difference grows past the orbit itself, and looks for the trap once it
/// has left its neighbours; its distance from the set comes from the
/// derivative.
fn render(picture: &Picture, view: &View, orbit: &[C], window: &Window) -> Check {
    let last = orbit.len().saturating_sub(1);
    let at = |m: usize| orbit.get(m.min(last)).copied().unwrap_or(C::ZERO);
    let pixel = view.scale * window.half.im * 2.0 / window.rows as f64;
    let mut lit = Vec::with_capacity(window.cols.saturating_mul(window.rows));
    let mut edge = 0_usize;
    for p in window.pixels() {
        let dc = p.turn(view.turn).scale(view.scale);
        let (mut dz, mut m, mut checks) = (C::ZERO, 0_usize, 0);
        let (mut z, mut d) = (C::ZERO, C::ZERO);
        let mut hit = false;
        let mut escaped = false;
        for _ in 0..view.limit {
            d = z.mul(d).scale(2.0).add(C::new(1.0, 0.0));
            dz = at(m).scale(2.0).add(dz).mul(dz).add(dc);
            m = m.saturating_add(1);
            z = at(m).add(dz);
            if z.norm() > ESCAPE {
                escaped = true;
                break;
            }
            if checks < CHECKS && dz.norm() > 1e-4 {
                hit |= picture.trapped(z, view.span);
                checks = checks.saturating_add(1);
            }
            if z.norm() < dz.norm() || m.saturating_add(1) >= orbit.len() {
                dz = z;
                m = 0;
            }
        }
        let distance = z.abs() * z.abs().ln() / d.abs().max(1e-300);
        if !escaped || distance < pixel * 2.0 {
            edge = edge.saturating_add(1);
        }
        lit.push(hit && escaped);
    }
    Check { lit, edge }
}

/// Which pixels of `window` a perfect copy of the art, as `layout` places
/// it, would cover.
fn wanted(picture: &Picture, layout: &Layout, window: &Window) -> Vec<bool> {
    let (l, t, r, b) = picture.art;
    let (cx, cy) = ((l + r) / 2.0, (t + b) / 2.0);
    // A pixel's offset from the copy's centre, back in the art's samples.
    let back = layout.art / layout.size;
    window
        .pixels()
        .map(|p| {
            let p = p.sub(layout.at);
            let sx = cx + p.re * back * picture.width / picture.aspect;
            let sy = cy + p.im * back * picture.height;
            sx >= 0.0
                && sy >= 0.0
                && sx < picture.width
                && sy < picture.height
                && (picture.ink)(sx, sy)
        })
        .collect()
}

/// How well `lit` matches `want` on a grid `cols` wide, forgiving a pixel
/// either way: the harmonic mean of the share of lit pixels near a wanted
/// one and of wanted pixels near a lit one.
fn agreement(lit: &[bool], want: &[bool], cols: usize) -> f64 {
    let near = |grid: &[bool], i: usize| {
        let (x, y) = (
            i.checked_rem(cols).unwrap_or(0),
            i.checked_div(cols).unwrap_or(0),
        );
        (y.saturating_sub(1)..=y.saturating_add(1)).any(|yy| {
            (x.saturating_sub(1)..=x.saturating_add(1).min(cols.saturating_sub(1))).any(|xx| {
                yy.checked_mul(cols)
                    .and_then(|r| r.checked_add(xx))
                    .and_then(|j| grid.get(j))
                    .copied()
                    .unwrap_or(false)
            })
        })
    };
    let count = |a: &[bool], b: &[bool]| {
        let all = a.iter().filter(|v| **v).count();
        let hit = (0..a.len())
            .filter(|&i| a.get(i).copied().unwrap_or(false) && near(b, i))
            .count();
        hit as f64 / all.max(1) as f64
    };
    let (precision, recall) = (count(lit, want), count(want, lit));
    2.0 * precision * recall / (precision + recall).max(1e-9)
}

/// A dive for `picture` that ends on a clear copy of its art, arriving
/// after `dive` seconds, if one turns up within `budget` and `tries`.
pub fn aim(picture: &Picture, dive: f32, seed: u64, budget: Duration, tries: u32) -> Option<Aim> {
    let start = Instant::now();
    let layout = picture.layout()?;
    let mut rng = fastrand::Rng::with_seed(seed);
    let cols = |rows: usize, wide: f64| num::floor_usize(num::narrow(rows as f64 * wide)).max(8);
    // The whole screen, coarsely, for its detail; the copy, finely.
    let screen = Window {
        centre: C::ZERO,
        half: C::new(picture.aspect / 2.0, 0.5),
        cols: cols(CHECK_ROWS, picture.aspect),
        rows: CHECK_ROWS,
    };
    let copy = Window {
        centre: layout.at,
        half: C::new(layout.size * layout.wide, layout.size).scale(0.6),
        cols: cols(COPY_ROWS, layout.wide),
        rows: COPY_ROWS,
    };
    let want = wanted(picture, &layout, &copy);
    for _ in 0..tries {
        if start.elapsed() > budget {
            return None;
        }
        let steps = rng.u32(150..800);
        let Some(edge) = boundary(&mut rng, steps) else {
            continue;
        };
        // The copy is `size` high where the art is `art` high: the patch
        // the screen shows must be stretched that much more than the trap
        // is wide.
        let depth = DEPTHS.0 + rng.f64() * (DEPTHS.1 - DEPTHS.0);
        let stretch = layout.art * layout.span / (layout.size * 3.0 * (-depth).exp());
        let Some(n) = closest(edge, stretch, layout.centre, MAX_LIMIT - 400) else {
            continue;
        };
        let Some(target) = newton(edge, n, layout.centre) else {
            continue;
        };
        let Some((_, d)) = orbit_at(target, n) else {
            continue;
        };
        // The art reaches about its width and height from its centre.
        let reach = layout.art * layout.span * layout.wide.max(1.0) * 0.5;
        if bend(target, n, reach) > 0.25 {
            continue;
        }
        let scale = layout.art * layout.span / (layout.size * d.abs());
        if !(1e-10..1e-2).contains(&scale) {
            continue;
        }
        let turn = -d.arg();
        let view = View {
            scale,
            span: layout.span,
            turn,
            limit: (n.saturating_mul(3) / 2).saturating_add(300).min(MAX_LIMIT),
        };
        let centre = target.sub(layout.at.turn(turn).scale(scale));
        let orbit = reference(centre, MAX_LIMIT.saturating_add(200));
        // The orbit as the GPU sees it, rounded to f32.
        let points: Vec<C> = orbit
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&[re, im]| C::new(f64::from(f32::from_bits(re)), f64::from(f32::from_bits(im))))
            .collect();
        let near = render(picture, &view, &points, &copy);
        let score = agreement(&near.lit, &want, copy.cols);
        if score < 0.8 {
            continue;
        }
        let whole = render(picture, &view, &points, &screen);
        let edge = whole.edge as f64 / whole.lit.len().max(1) as f64;
        if edge >= 0.04 {
            return Some(Aim {
                orbit,
                scale: num::narrow(view.scale),
                turn: num::narrow(view.turn),
                span: num::narrow(view.span),
                limit: view.limit,
                dive,
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stage::{Act, Logo, Stage, Subject, Word};

    fn stage(subject: Subject, cols: u16, rows: u16) -> Stage {
        Stage::new(
            Act {
                subject,
                ..Act::plain()
            },
            cols,
            rows,
            2.0,
        )
    }

    #[test]
    fn a_logo_gets_a_dive_that_ends_on_it() {
        let found = (0..6)
            .filter(|&seed| {
                let mut s = stage(Subject::Logo(Logo::Mascot), 160, 48);
                s.aim(10.0, seed);
                s.zoom
                    .as_ref()
                    .is_some_and(|a| !a.orbit.is_empty() && a.limit <= MAX_LIMIT)
            })
            .count();
        assert!(found >= 5, "{found} of 6");
    }

    #[test]
    #[ignore = "a survey: prints how often the search succeeds"]
    fn survey_success() {
        for (name, subject) in [
            ("mascot", Subject::Logo(Logo::Mascot)),
            ("emblem", Subject::Logo(Logo::Emblem)),
            ("pixel", Subject::Word(Word::Pixel)),
            ("lockup", Subject::Lockup(Logo::Mascot, Word::Pixel)),
        ] {
            for (cols, rows) in [(160, 48), (300, 90)] {
                let mut found = 0;
                let mut total = Duration::ZERO;
                let mut worst = Duration::ZERO;
                let mut scores = Vec::new();
                for seed in 0..20 {
                    let mut s = stage(subject.clone(), cols, rows);
                    let t = Instant::now();
                    s.aim(10.0, seed);
                    let took = t.elapsed();
                    total += took;
                    worst = worst.max(took);
                    if let Some(a) = &s.zoom {
                        found += 1;
                        scores.push(format!("{:.0e}/{}", a.scale, a.limit));
                    }
                }
                println!(
                    "{name} {cols}x{rows}: {found}/20, mean {:?}, worst {:?}: {}",
                    total / 20,
                    worst,
                    scores.join(" ")
                );
            }
        }
    }
}
