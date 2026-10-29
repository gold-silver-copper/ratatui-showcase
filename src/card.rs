//! A card between the scenes, drawn with ratatui's own widgets on a calm
//! background: "ratatui" in the 8x8 font, a fact about it typed out, and
//! a dashboard of built-in widgets with live data. It takes the colours of
//! the scene before it and moves gently, so it stays easy to read.

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::canvas::{Canvas, Line as Stroke, Map, MapResolution, Points};
use ratatui::widgets::{
    Axis, Bar, BarChart, BarGroup, Block, BorderType, Chart, Dataset, Gauge, GraphType, Paragraph,
    Widget,
};

use crate::art;
use crate::num;
use crate::theme::{Palette, Rgb};

/// The shortest a card holds, in seconds, long enough to read it.
pub const HOLD: f64 = 10.0;

/// What ratatui is, one fact a card: a title and a sentence or two.
const FACTS: &[(&str, &str)] = &[
    (
        "What is ratatui?",
        "ratatui is a Rust library for building rich apps that run in the terminal: \
         dashboards, editors, file managers, games and demos like this one.",
    ),
    (
        "Immediate mode",
        "Every frame you describe the whole screen. ratatui compares it with the \
         last frame and sends only the cells that changed.",
    ),
    (
        "Widgets included",
        "Blocks, paragraphs, lists, tables, tabs, charts, sparklines, gauges, bar \
         charts and a canvas for drawing, and writing your own is one trait.",
    ),
    (
        "Layout",
        "Split the screen into rows and columns by length, percentage, ratio or \
         fill, and the same app fits any terminal size.",
    ),
    (
        "Made in Rust",
        "Memory safe without a garbage collector and fast enough to redraw every \
         frame. Add it to a project with one line: cargo add ratatui.",
    ),
    (
        "Any backend",
        "ratatui draws through crossterm, termion or termwiz, and into a plain \
         buffer to test what an app shows.",
    ),
    (
        "This show",
        "Every frame here is a ratatui buffer: GPU shaders paint the pixels, \
         tachyonfx animates the cells, and ratatui puts them on screen.",
    ),
    (
        "Open source",
        "MIT licensed and built by its community, ratatui carries on the tui-rs \
         crate. Apps like gitui, yazi, bottom and atuin are built with it.",
    ),
];

/// Cities on the map, `(longitude, latitude)`, and the routes between them.
const CITIES: &[(f64, f64)] = &[
    (-0.13, 51.5),  // London
    (-74.0, 40.7),  // New York
    (-122.4, 37.8), // San Francisco
    (139.7, 35.7),  // Tokyo
    (151.2, -33.9), // Sydney
    (-46.6, -23.5), // São Paulo
    (18.4, -33.9),  // Cape Town
    (72.9, 19.1),   // Mumbai
    (103.8, 1.35),  // Singapore
    (13.4, 52.5),   // Berlin
];
const ROUTES: &[(usize, usize)] = &[
    (0, 1),
    (1, 2),
    (1, 5),
    (0, 6),
    (9, 7),
    (7, 8),
    (8, 4),
    (8, 3),
];
/// Seconds the map takes to draw itself, and each route to fly.
const DRAW: f32 = 3.5;
const FLIGHT: f32 = 1.6;

pub struct Card {
    /// Which fact.
    index: usize,
    colors: Colors,
    started: f64,
    /// How long it holds, in seconds.
    pub hold: f64,
}

impl Card {
    /// The `n`th card of the show.
    pub fn new(n: usize, palette: &Palette, now: f64, hold: f64) -> Self {
        Self {
            index: n,
            colors: Colors::new(palette),
            started: now,
            hold,
        }
    }

    pub fn background(&self) -> Rgb {
        self.colors.bg
    }

    /// Draws the card at `now`.
    pub fn draw(&self, buf: &mut Buffer, now: f64) {
        let t = num::narrow(now - self.started);
        let c = &self.colors;
        self.backdrop(buf);
        // The bottom row is the caption's.
        let area = Rect {
            height: buf.area.height.saturating_sub(1),
            ..buf.area
        }
        .inner(Margin::new(2, 1));
        let (title, body) = FACTS
            .get(self.index.checked_rem(FACTS.len()).unwrap_or(0))
            .copied()
            .unwrap_or_default();
        let lines = wrap(body, usize::from(area.width.min(100)));
        let word = art::pixel_font("ratatui");
        // The heading takes at most a sixth of the height.
        let scale = (1..=6).rev().find(|&s: &usize| {
            word.width.saturating_mul(s) <= usize::from(area.width).saturating_mul(3) / 5
                && s.saturating_mul(4) <= usize::from(area.height) / 6
        });
        let head_h = scale.map_or(1, |s| num::u16_of(s.saturating_mul(4)));
        let [head, _, heading, text, footer, _, body] = Layout::vertical([
            Constraint::Length(head_h),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(num::u16_of(lines.len())),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .areas(area);

        match scale {
            Some(s) => {
                let left = head.x.saturating_add(
                    head.width
                        .saturating_sub(num::u16_of(word.width.saturating_mul(s)))
                        / 2,
                );
                let width = word.width.max(1);
                // Letters arrive one by one, then hold still.
                let shown = num::floor_usize((t - 0.3) * 10.0);
                let top = usize::from(head.y).saturating_mul(2);
                pixels(buf, left, top, (word.width, 8), s, |x, y| {
                    (x / 8 < shown && word.get(x, y).ink > 0.5).then(|| {
                        c.palette
                            .sample(0.55 * num::float(x) / num::float(width))
                            .lerp(Rgb::WHITE, 0.15)
                    })
                });
            }
            None => Line::styled("ratatui", Style::new().fg(c.accent.color()).bold())
                .centered()
                .render(head, buf),
        }

        // The fact, typed out.
        Line::styled(title, Style::new().fg(c.accent.color()).bold())
            .centered()
            .render(heading, buf);
        let mut left = num::floor_usize((t - 0.8) * 55.0);
        let typing = left < lines.iter().map(|l| l.chars().count()).sum();
        let mut typed: Vec<Line> = Vec::new();
        for line in &lines {
            let n = line.chars().count().min(left);
            left = left.saturating_sub(n);
            let mut spans = vec![Span::raw(line.chars().take(n).collect::<String>())];
            let cut = typing && n < line.chars().count();
            if cut {
                spans.push(Span::styled("▌", Style::new().fg(c.accent.color())));
            }
            typed.push(Line::from(spans));
            if cut {
                break;
            }
        }
        Paragraph::new(typed)
            .style(Style::new().fg(c.text.color()))
            .alignment(Alignment::Center)
            .render(text, buf);
        Line::from(vec![
            Span::styled("ratatui.rs", Style::new().fg(c.dim.color())),
            Span::styled("   ·   ", Style::new().fg(c.dim.color())),
            Span::styled("cargo add ratatui", Style::new().fg(c.accent.color())),
        ])
        .centered()
        .render(footer, buf);

        // Built-in widgets, each box titled with its name.
        let progress = (f64::from(t) / self.hold.max(1.0)).clamp(0.0, 1.0);
        let [map, side] = Layout::horizontal([Constraint::Percentage(58), Constraint::Fill(1)])
            .spacing(1)
            .areas(body);
        let [chart, bars, gauge] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Fill(1),
            Constraint::Length(3),
        ])
        .areas(side);
        self.world(buf, map, t);
        self.chart(buf, chart, t);
        self.bars(buf, bars, t);
        self.gauge(buf, gauge, progress);
    }

    /// A still, faint glow of the theme's first colour from the top.
    fn backdrop(&self, buf: &mut Buffer) {
        let area = buf.area;
        let tint = self.colors.palette.sample(0.0);
        for y in area.top()..area.bottom() {
            let fall = 1.0 - f32::from(y) / f32::from(area.height.max(1));
            let color = self.colors.bg.lerp(tint, 0.07 * fall * fall);
            buf.set_style(
                Rect::new(area.x, y, area.width, 1),
                Style::new().bg(color.color()),
            );
        }
    }

    /// A rounded box titled with a widget's name.
    fn block<'a>(&self, title: &'a str) -> Block<'a> {
        let c = &self.colors;
        Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(c.border.color()))
            .title(Line::styled(
                format!(" {title} "),
                Style::new()
                    .fg(c.accent.color())
                    .add_modifier(Modifier::BOLD),
            ))
            .style(Style::new().bg(c.panel.color()).fg(c.text.color()))
    }

    /// The world map draws itself from west to east, then routes fly
    /// between cities one after another.
    fn world(&self, buf: &mut Buffer, area: Rect, t: f32) {
        let c = &self.colors;
        let block = self.block("Canvas");
        let inner = block.inner(area);
        let reveal = smooth(((t - 0.4) / DRAW).clamp(0.0, 1.0));
        let flying = t - 0.4 - DRAW;
        let flown = num::floor_usize(flying / FLIGHT);
        let along = (flying / FLIGHT).fract();
        let land = c.palette.sample(0.3).lerp(c.panel, 0.25);
        Canvas::default()
            .block(block)
            .background_color(c.panel.color())
            .marker(Marker::Braille)
            .x_bounds([-180.0, 180.0])
            .y_bounds([-58.0, 84.0])
            .paint(|ctx| {
                ctx.draw(&Map {
                    color: land.color(),
                    resolution: MapResolution::High,
                });
                if flying < 0.0 {
                    return;
                }
                ctx.layer();
                for (i, &(from, to)) in ROUTES.iter().enumerate() {
                    let current = Some(i) == flown.checked_rem(ROUTES.len());
                    if i < flown {
                        route(ctx, from, to, 1.0, c.dim);
                    }
                    if current {
                        route(ctx, from, to, f64::from(along), c.accent);
                    }
                }
                ctx.draw(&Points {
                    coords: CITIES,
                    color: c.text.color(),
                });
            })
            .render(area, buf);
        // Hide what the sweep hasn't reached; its edge glows.
        let edge = inner
            .x
            .saturating_add(num::floor_u16(reveal * f32::from(inner.width)));
        for y in inner.top()..inner.bottom() {
            for x in edge.saturating_sub(2)..inner.right() {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    if x >= edge {
                        cell.set_char(' ');
                    } else if reveal < 1.0 {
                        cell.set_fg(c.accent.color());
                    }
                }
            }
        }
    }

    fn bars(&self, buf: &mut Buffer, area: Rect, t: f32) {
        let c = &self.colors;
        let block = self.block("BarChart");
        let inner = block.inner(area);
        let n: u16 = 8;
        let width = inner
            .width
            .saturating_sub(n.saturating_sub(1))
            .checked_div(n)
            .unwrap_or(1)
            .max(1);
        let bars: Vec<Bar> = (0..n)
            .map(|i| {
                let k = f32::from(i);
                let swing = (t * 0.9 + k * 0.75).sin() * (t * 0.31 + k * 1.7).cos();
                let value = num::floor_u32(55.0 + 40.0 * swing).min(100);
                Bar::default()
                    .value(u64::from(value))
                    .label(Line::from(format!("cpu{i}")))
                    .text_value(format!("{value}%"))
                    .style(Style::new().fg(c.palette.sample(k / 8.0).lerp(Rgb::WHITE, 0.1).color()))
                    .value_style(
                        Style::new().fg(c.panel.color()).bg(c
                            .palette
                            .sample(k / 8.0)
                            .lerp(Rgb::WHITE, 0.1)
                            .color()),
                    )
            })
            .collect();
        BarChart::default()
            .block(block)
            .data(BarGroup::default().bars(&bars))
            .bar_width(width)
            .bar_gap(1)
            .max(100)
            .label_style(Style::new().fg(c.dim.color()))
            .render(area, buf);
    }

    /// How far the card is to the next scene.
    fn gauge(&self, buf: &mut Buffer, area: Rect, progress: f64) {
        let c = &self.colors;
        Gauge::default()
            .block(self.block("Gauge"))
            .gauge_style(
                Style::new()
                    .fg(c.palette.sample(0.4).lerp(Rgb::WHITE, 0.1).color())
                    .bg(c.panel.lerp(Rgb::WHITE, 0.06).color()),
            )
            .use_unicode(true)
            .ratio(progress)
            .label(Span::styled(
                format!("next scene  {:.0}%", progress * 100.0),
                Style::new().fg(c.text.color()).add_modifier(Modifier::BOLD),
            ))
            .render(area, buf);
    }

    /// Two signals scrolling past.
    fn chart(&self, buf: &mut Buffer, area: Rect, t: f32) {
        let c = &self.colors;
        let now = f64::from(t);
        let points = |f: fn(f64) -> f64| -> Vec<(f64, f64)> {
            (0..=160)
                .map(|i| {
                    let x = now - 10.0 + f64::from(i) / 16.0;
                    (x, f(x))
                })
                .collect()
        };
        let wave = points(|x| 0.75 * (x * 1.3).sin() + 0.2 * (x * 0.37).sin());
        let drift = points(|x| 0.6 * (x * 0.7 + 1.0).cos() * (x * 0.23).sin() - 0.1);
        let datasets = vec![
            Dataset::default()
                .name("wave")
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::new().fg(c.palette.sample(0.2).lerp(Rgb::WHITE, 0.1).color()))
                .data(&wave),
            Dataset::default()
                .name("drift")
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::new().fg(c.palette.sample(0.6).lerp(Rgb::WHITE, 0.1).color()))
                .data(&drift),
        ];
        let axis = Style::new().fg(c.dim.color());
        Chart::new(datasets)
            .block(self.block("Chart"))
            .x_axis(
                Axis::default()
                    .bounds([now - 10.0, now])
                    .labels(["-10 s", "-5 s", "now"])
                    .style(axis),
            )
            .y_axis(
                Axis::default()
                    .bounds([-1.1, 1.1])
                    .labels(["-1", "0", "1"])
                    .style(axis),
            )
            .render(area, buf);
    }
}

/// A card's colours, from the scene before it.
struct Colors {
    palette: Palette,
    bg: Rgb,
    panel: Rgb,
    text: Rgb,
    dim: Rgb,
    border: Rgb,
    accent: Rgb,
}

impl Colors {
    fn new(palette: &Palette) -> Self {
        let bg = palette.background;
        let text = Rgb::WHITE.lerp(palette.accent, 0.08);
        Self {
            bg,
            panel: bg.lerp(palette.sample(0.0), 0.07),
            text,
            dim: text.lerp(bg, 0.45),
            border: palette.sample(0.15).lerp(bg, 0.4),
            accent: palette.accent.lerp(Rgb::WHITE, 0.2),
            palette: palette.clone(),
        }
    }
}

/// A route from city `from` to city `to`, bowed towards the pole, drawn
/// as far as `along` (0 to 1), with a dot where it has got to.
fn route(
    ctx: &mut ratatui::widgets::canvas::Context,
    from: usize,
    to: usize,
    along: f64,
    color: Rgb,
) {
    let (Some(&(x0, y0)), Some(&(x1, y1))) = (CITIES.get(from), CITIES.get(to)) else {
        return;
    };
    let bow = (x1 - x0).hypot(y1 - y0) * 0.22;
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0 + bow.copysign(y0 + y1));
    let at = |s: f64| {
        let r = 1.0 - s;
        (
            r * r * x0 + 2.0 * r * s * cx + s * s * x1,
            r * r * y0 + 2.0 * r * s * cy + s * s * y1,
        )
    };
    let steps = 32;
    let mut last = at(0.0);
    for i in 1..=steps {
        let s = f64::from(i) / f64::from(steps);
        if s > along {
            break;
        }
        let next = at(s);
        ctx.draw(&Stroke::new(last.0, last.1, next.0, next.1, color.color()));
        last = next;
    }
    let head = at(along);
    ctx.draw(&Stroke::new(last.0, last.1, head.0, head.1, color.color()));
}

/// Draws a picture of `w` x `h` pixels in half blocks, two pixels a cell
/// one above the other, so with cells twice as tall as wide they come out
/// square. Each pixel is `scale` cells wide and `scale` half rows tall;
/// `top` is in half rows. Pixels `at` gives no colour show what is below.
fn pixels(
    buf: &mut Buffer,
    left: u16,
    top: usize,
    (w, h): (usize, usize),
    scale: usize,
    at: impl Fn(usize, usize) -> Option<Rgb>,
) {
    let scale = scale.max(1);
    let tall = h.saturating_mul(scale);
    let pick = |x: usize, half: usize| {
        let sub = half.checked_sub(top).filter(|&s| s < tall)?;
        at(x.checked_div(scale)?, sub.checked_div(scale)?)
    };
    let first = top / 2;
    let last = top.saturating_add(tall).div_ceil(2);
    for row in first..last {
        let Ok(y) = u16::try_from(row) else { return };
        let half = row.saturating_mul(2);
        for col in 0..w.saturating_mul(scale) {
            let Some(x) = u16::try_from(col).ok().map(|c| left.saturating_add(c)) else {
                break;
            };
            let Some(cell) = buf.cell_mut((x, y)) else {
                continue;
            };
            match (pick(col, half), pick(col, half.saturating_add(1))) {
                (None, None) => {}
                (Some(a), Some(b)) if a == b => {
                    cell.set_char('█').set_fg(a.color());
                }
                (Some(a), Some(b)) => {
                    cell.set_char('▀').set_fg(a.color()).set_bg(b.color());
                }
                (Some(a), None) => {
                    cell.set_char('▀').set_fg(a.color());
                }
                (None, Some(b)) => {
                    cell.set_char('▄').set_fg(b.color());
                }
            }
        }
    }
}

/// `text` broken into lines of at most `width` characters, at spaces.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(10);
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(line)
                if line
                    .chars()
                    .count()
                    .saturating_add(1)
                    .saturating_add(word.chars().count())
                    <= width =>
            {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_owned()),
        }
    }
    lines
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cards_draw_at_any_size() {
        for n in 0..2 {
            let card = Card::new(n, &Palette::first(), 0.0, 10.0);
            for (w, h) in [(1, 1), (12, 4), (40, 12), (80, 24), (400, 120)] {
                for t in [0.0, 2.0, 6.0, 30.0] {
                    let mut buf = Buffer::empty(Rect::new(0, 0, w, h));
                    card.draw(&mut buf, t);
                }
            }
        }
    }
}
