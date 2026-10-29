//! ratatui's own widgets, the showcase's first library: a dashboard of
//! built-in widgets with live data, and facts about ratatui for the panel
//! beside it. Also the showcase's colours and drawing helpers.

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::canvas::{Canvas, Line as Stroke, Map, MapResolution, Points};
use ratatui::widgets::{
    Axis, Bar, BarChart, BarGroup, Block, Chart, Dataset, Gauge, GraphType, LineGauge, Padding,
    Row, Sparkline, StatefulWidget, Table, TableState, Widget,
};

use crate::num;
use crate::theme::{Palette, Rgb};

/// What ratatui is, one fact a visit.
const FACTS: &[&str] = &[
    "ratatui is a Rust library for building rich apps that run in the terminal: \
     dashboards, editors, file managers, games and demos like this one.",
    "Every frame you describe the whole screen. ratatui compares it with the \
     last frame and sends only the cells that changed.",
    "Blocks, paragraphs, lists, tables, tabs, charts, sparklines, gauges, bar \
     charts and a canvas for drawing, and writing your own is one trait.",
    "Split the screen into rows and columns by length, percentage, ratio or \
     fill, and the same app fits any terminal size.",
    "Memory safe without a garbage collector and fast enough to redraw every \
     frame. Add it to a project with one line: cargo add ratatui.",
    "ratatui draws through crossterm, termion or termwiz, and into a plain \
     buffer to test what an app shows.",
    "Every frame here is a ratatui buffer: GPU shaders paint the pixels, \
     tachyonfx animates the cells, and ratatui puts them on screen.",
    "MIT licensed and built by its community, ratatui carries on the tui-rs \
     crate. Apps like codex, herdr, gitui, yazi, bottom and atuin are built with it.",
];

/// The `n`th fact about ratatui.
pub fn fact(n: usize) -> &'static str {
    FACTS
        .get(n.checked_rem(FACTS.len()).unwrap_or(0))
        .copied()
        .unwrap_or_default()
}

/// Apps built with ratatui, for the table.
const APPS: &[(&str, &str)] = &[
    ("codex", "OpenAI's coding agent"),
    ("herdr", "an AI agent multiplexer"),
    ("gitui", "git, fast"),
    ("yazi", "a file manager"),
    ("bottom", "a system monitor"),
    ("atuin", "shell history, searched"),
    ("oha", "HTTP load testing"),
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

/// ratatui's own widgets with live data, each on a surface titled with its
/// name, at `t` seconds; `progress` fills the gauge.
pub fn dashboard(c: &Colors, buf: &mut Buffer, area: Rect, t: f32, progress: f64) {
    let [top, middle, bottom] = Layout::vertical([
        Constraint::Fill(3),
        Constraint::Fill(2),
        Constraint::Length(3),
    ])
    .spacing(1)
    .areas(area);
    let [map, chart] = Layout::horizontal([Constraint::Percentage(58), Constraint::Fill(1)])
        .spacing(2)
        .areas(top);
    let [bars, spark, table] = Layout::horizontal([Constraint::Fill(1); 3])
        .spacing(2)
        .areas(middle);
    let [line, gauge] = Layout::horizontal([Constraint::Fill(1); 2])
        .spacing(2)
        .areas(bottom);
    world(c, buf, map, t);
    self::chart(c, buf, chart, t);
    self::bars(c, buf, bars, t);
    sparkline(c, buf, spark, t);
    self::table(c, buf, table, t);
    line_gauge(c, buf, line, t);
    self::gauge(c, buf, gauge, progress);
}

/// The world map draws itself from west to east, then routes fly
/// between cities one after another.
fn world(c: &Colors, buf: &mut Buffer, area: Rect, t: f32) {
    let block = c.block("Canvas");
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

fn sparkline(c: &Colors, buf: &mut Buffer, area: Rect, t: f32) {
    let block = c.block("Sparkline");
    let n = usize::from(block.inner(area).width);
    // A new value four times a second; the line scrolls left.
    let start = num::floor_usize(t * 4.0);
    let data: Vec<u64> = (start..start.saturating_add(n))
        .map(|k| {
            let k = num::float(k);
            let v =
                45.0 + 22.0 * (k * 0.21).sin() + 16.0 * (k * 0.067 + 1.3).sin() + 12.0 * jitter(k);
            u64::from(num::floor_u32(v))
        })
        .collect();
    Sparkline::default()
        .block(block)
        .data(&data)
        .max(100)
        .style(Style::new().fg(c.palette.sample(0.15).lerp(Rgb::WHITE, 0.1).color()))
        .render(area, buf);
}

fn bars(c: &Colors, buf: &mut Buffer, area: Rect, t: f32) {
    let block = c.block("BarChart");
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
                .label(Line::from(if width >= 4 {
                    format!("cpu{i}")
                } else {
                    i.to_string()
                }))
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

/// How far it is to the next library.
fn gauge(c: &Colors, buf: &mut Buffer, area: Rect, progress: f64) {
    Gauge::default()
        .block(c.block("Gauge"))
        .gauge_style(
            Style::new()
                .fg(c.palette.sample(0.4).lerp(Rgb::WHITE, 0.1).color())
                .bg(c.panel.lerp(Rgb::WHITE, 0.06).color()),
        )
        .use_unicode(true)
        .ratio(progress)
        .label(Span::styled(
            format!("next library  {:.0}%", progress * 100.0),
            Style::new().fg(c.text.color()).add_modifier(Modifier::BOLD),
        ))
        .render(area, buf);
}

fn line_gauge(c: &Colors, buf: &mut Buffer, area: Rect, t: f32) {
    let ratio = 0.55 + 0.3 * (t * 0.45).sin() * (t * 0.17 + 0.6).cos();
    LineGauge::default()
        .block(c.block("LineGauge"))
        .filled_style(Style::new().fg(c.palette.sample(0.7).lerp(Rgb::WHITE, 0.1).color()))
        .unfilled_style(Style::new().fg(c.border.color()))
        .ratio(f64::from(ratio).clamp(0.0, 1.0))
        .label(Span::styled(
            format!("memory {:>3.0}%", ratio * 100.0),
            Style::new().fg(c.text.color()),
        ))
        .render(area, buf);
}

/// Apps built with ratatui, the highlight moving down them.
fn table(c: &Colors, buf: &mut Buffer, area: Rect, t: f32) {
    let selected = num::floor_usize(t / 1.5)
        .checked_rem(APPS.len())
        .unwrap_or(0);
    let rows = APPS.iter().map(|&(name, what)| Row::new([name, what]));
    let table = Table::new(rows, [Constraint::Length(7), Constraint::Fill(1)])
        .block(c.block("Table · built with ratatui"))
        .column_spacing(2)
        .row_highlight_style(
            Style::new()
                .bg(c.palette.sample(0.4).lerp(c.panel, 0.55).color())
                .fg(Rgb::WHITE.color()),
        )
        .highlight_symbol("▶ ");
    let mut state = TableState::default().with_selected(Some(selected));
    StatefulWidget::render(table, area, buf, &mut state);
}

/// Two signals scrolling past.
fn chart(c: &Colors, buf: &mut Buffer, area: Rect, t: f32) {
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
        .legend_position(None)
        .block(c.block("Chart"))
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

/// The showcase's colours, from the scene before it.
pub struct Colors {
    pub palette: Palette,
    pub bg: Rgb,
    /// Surfaces on the background, and surfaces on those.
    pub panel: Rgb,
    pub raised: Rgb,
    pub text: Rgb,
    pub dim: Rgb,
    pub border: Rgb,
    pub accent: Rgb,
}

impl Colors {
    pub fn new(palette: &Palette) -> Self {
        let bg = palette.background;
        let text = Rgb::WHITE.lerp(palette.accent, 0.08);
        Self {
            bg,
            panel: bg.lerp(palette.sample(0.0), 0.07),
            raised: bg.lerp(palette.sample(0.0), 0.14),
            text,
            dim: text.lerp(bg, 0.45),
            border: palette.sample(0.15).lerp(bg, 0.4),
            accent: palette.accent.lerp(Rgb::WHITE, 0.2),
            palette: palette.clone(),
        }
    }

    /// A still, faint glow of the theme's first colour from the top.
    pub fn backdrop(&self, buf: &mut Buffer) {
        let area = buf.area;
        let tint = self.palette.sample(0.0);
        for y in area.top()..area.bottom() {
            let fall = 1.0 - f32::from(y) / f32::from(area.height.max(1));
            let color = self.bg.lerp(tint, 0.035 * fall * fall);
            buf.set_style(
                Rect::new(area.x, y, area.width, 1),
                Style::new().bg(color.color()),
            );
        }
    }

    /// A flat surface titled with a widget's name: no borders, the title
    /// on its own row.
    pub fn block<'a>(&self, title: &'a str) -> Block<'a> {
        Block::new()
            .title(Line::styled(
                format!("  {title}"),
                Style::new()
                    .fg(self.accent.color())
                    .add_modifier(Modifier::BOLD),
            ))
            .padding(Padding::new(2, 2, 0, 1))
            .style(self.surface())
    }

    /// A flat surface's colours.
    pub fn surface(&self) -> Style {
        Style::new().bg(self.panel.color()).fg(self.text.color())
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
pub fn pixels(
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

/// A steady pseudo-random value in `-0.5..0.5` for `k`.
fn jitter(k: f32) -> f32 {
    ((k * 12.9898).sin() * 43_758.547).fract() - 0.5
}
