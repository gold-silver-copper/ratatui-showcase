//! Cards between the scenes, drawn with ratatui's own widgets on a calm
//! background: one about ratatui, with Ferris the crab, and one showing off
//! the built-in widgets with live data. They take the colours of the scene
//! before them and move gently, in time with the beat, so they stay easy
//! to read.

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::canvas::{Canvas, Line as Stroke, Map, MapResolution, Points};
use ratatui::widgets::{
    Axis, Bar, BarChart, BarGroup, Block, BorderType, Chart, Dataset, Gauge, GraphType, LineGauge,
    Padding, Paragraph, Row, Sparkline, StatefulWidget, Table, TableState, Tabs, Widget,
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

/// The widgets the showcase names: its table and tabs.
const WIDGETS: &[(&str, &str)] = &[
    ("Block", "borders, titles and padding"),
    ("Paragraph", "styled, wrapped text"),
    ("List", "items to scroll and select"),
    ("Table", "rows and columns, like this one"),
    ("Tabs", "a row of titles, one selected"),
    ("Chart", "lines and scatter plots on axes"),
    ("Canvas", "free drawing: shapes, maps"),
    ("BarChart", "labelled bars, side by side"),
    ("Sparkline", "a small line of recent values"),
    ("Gauge", "progress, as a bar or a line"),
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

/// Ferris, in half-block pixels: `o` shell, `d` shade, `w` and `e` the
/// parts of an eye a blink closes, `k` the rest of it, `m` the mouth.
const FERRIS: &[&str] = &[
    ".oo..oo....................oo..oo.",
    "oo....oo..................oo....oo",
    "oo....oo..................oo....oo",
    "ooo..ooo..................ooo..ooo",
    ".oooooo....................oooooo.",
    "..oooo.......o......o.......oooo..",
    "...dd.....o.ooo.oo.ooo.o.....dd...",
    "....dd...oooooooooooooooo...dd....",
    ".....dd.oooooweooooewooooo.dd.....",
    "......oooooooeeooooeeooooooo......",
    "......oooooookkooookkooooooo......",
    ".....oooooooooooooooooooooooo.....",
    "....oooooooooooooooooooooooooo....",
    "...d.oooooooooooooooooooooooo.d...",
    "..d..oooooooooommmmoooooooooo..d..",
    "..d...oooooooooommoooooooooo...d..",
    "...d...oooooooooooooooooooo...d...",
    "....d...oddoooddooddoooddo...d....",
    "......d..d...d..dd..d...d..d......",
];
/// The claws' first rows when they snap shut.
const SNAP: &[&str] = &[
    "..ooo........................ooo..",
    ".ooooo......................ooooo.",
    "ooooooo....................ooooooo",
    ".oooooo....................oooooo.",
];
const FERRIS_W: usize = 34;
const FERRIS_H: usize = 19;

/// Pixels between Ferris and the word.
const GAP: usize = 4;

/// How the about card's heading is laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Head {
    Side,
    Stacked,
    Alone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    About,
    Widgets,
}

pub struct Card {
    kind: Kind,
    /// Which fact, or which set of widgets.
    index: usize,
    colors: Colors,
    started: f64,
    /// How long it holds, in seconds.
    pub hold: f64,
}

impl Card {
    /// The `n`th card of the show: about and widgets take turns.
    pub fn new(n: usize, palette: &Palette, now: f64, hold: f64) -> Self {
        Self {
            kind: if n.is_multiple_of(2) {
                Kind::About
            } else {
                Kind::Widgets
            },
            index: n.checked_div(2).unwrap_or(0),
            colors: Colors::new(palette),
            started: now,
            hold,
        }
    }

    pub fn name(&self) -> &'static str {
        match self.kind {
            Kind::About => "about ratatui",
            Kind::Widgets => "widgets",
        }
    }

    pub fn background(&self) -> Rgb {
        self.colors.bg
    }

    /// Draws the card at `now`, with `beat` the show's beats.
    pub fn draw(&self, buf: &mut Buffer, now: f64, beat: f32) {
        let t = num::narrow(now - self.started);
        let area = buf.area;
        self.backdrop(buf);
        // The bottom row is the caption's.
        let area = Rect {
            height: area.height.saturating_sub(1),
            ..area
        };
        match self.kind {
            Kind::About => self.about(buf, area, t, beat),
            Kind::Widgets => self.widgets(buf, area, t, beat),
        }
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

    fn about(&self, buf: &mut Buffer, area: Rect, t: f32, beat: f32) {
        let c = &self.colors;
        let (title, body) = FACTS
            .get(self.index.checked_rem(FACTS.len()).unwrap_or(0))
            .copied()
            .unwrap_or_default();
        let panel_w = area.width.saturating_sub(4).min(80);
        let lines = wrap(body, usize::from(panel_w.saturating_sub(8)));
        let panel_h = num::u16_of(lines.len()).saturating_add(4);

        let word = art::pixel_font("ratatui");
        // Rows below the heading: a gap, the tagline, a gap, the panel, a
        // gap and the footer.
        let below = panel_h.saturating_add(6);
        let room_h = usize::from(area.height.saturating_sub(below)).saturating_mul(2);
        let fit = |(w, h): (usize, usize)| {
            (1..=12).rev().find(|&s: &usize| {
                w.saturating_mul(s) <= usize::from(area.width).saturating_mul(9) / 10
                    && h.saturating_mul(s) <= room_h
            })
        };
        // Ferris beside the word or above it, whichever shows them larger,
        // else the word alone; sizes in pixels.
        let side = (
            FERRIS_W.saturating_add(GAP).saturating_add(word.width),
            FERRIS_H,
        );
        let stacked = (
            FERRIS_W.max(word.width),
            FERRIS_H.saturating_add(GAP).saturating_add(8),
        );
        let alone = (word.width, 8);
        // Ties go to the later one: Ferris, beside the word if he can.
        let head = [
            (Head::Alone, alone),
            (Head::Stacked, stacked),
            (Head::Side, side),
        ]
        .into_iter()
        .filter_map(|(head, size)| Some((head, size, fit(size)?)))
        .max_by_key(|&(_, _, s)| s);
        let head_h = head.map_or(1, |(_, (_, h), s)| {
            num::u16_of(h.saturating_mul(s).div_ceil(2))
        });
        let total = head_h.saturating_add(below);
        let top = area.y.saturating_add(area.height.saturating_sub(total) / 2);

        // The heading: Ferris and the word in the 8x8 font.
        match head {
            Some((head, (w, _), s)) => {
                let centre = |w: usize| {
                    area.x.saturating_add(
                        area.width.saturating_sub(num::u16_of(w.saturating_mul(s))) / 2,
                    )
                };
                let half_top = usize::from(top).saturating_mul(2);
                let left = centre(w);
                let below_ferris = FERRIS_H.saturating_add(GAP).saturating_mul(s);
                let (word_left, word_top) = match head {
                    Head::Side => (
                        left.saturating_add(num::u16_of(
                            FERRIS_W.saturating_add(GAP).saturating_mul(s),
                        )),
                        half_top.saturating_add(FERRIS_H.saturating_sub(8).saturating_mul(s) / 2),
                    ),
                    Head::Stacked => (centre(word.width), half_top.saturating_add(below_ferris)),
                    Head::Alone => (left, half_top),
                };
                match head {
                    Head::Side => self.ferris(buf, left, half_top, s, t, beat),
                    Head::Stacked => self.ferris(buf, centre(FERRIS_W), half_top, s, t, beat),
                    Head::Alone => {}
                }
                let width = word.width.max(1);
                // Letters arrive one by one, then hold still.
                let shown = num::floor_usize((t - 0.3) * 10.0);
                pixels(buf, word_left, word_top, (word.width, 8), s, |x, y| {
                    (x / 8 < shown && word.get(x, y).ink > 0.5).then(|| {
                        c.palette
                            .sample(0.55 * num::float(x) / num::float(width))
                            .lerp(Rgb::WHITE, 0.15)
                    })
                });
            }
            None => {
                let line = Line::styled("ratatui", Style::new().fg(c.accent.color()).bold());
                line.centered()
                    .render(Rect::new(area.x, top, area.width, 1), buf);
            }
        }

        let mut y = top.saturating_add(head_h).saturating_add(1);
        Line::styled(
            "Terminal user interfaces in Rust",
            Style::new().fg(c.dim.color()),
        )
        .centered()
        .render(Rect::new(area.x, y, area.width, 1), buf);
        y = y.saturating_add(2);

        // The fact, typed out.
        let panel = Rect::new(
            area.x
                .saturating_add(area.width.saturating_sub(panel_w) / 2),
            y,
            panel_w,
            panel_h,
        )
        .intersection(area);
        let mut left = num::floor_usize((t - 0.8) * 55.0);
        let typing = left < lines.iter().map(|l| l.chars().count()).sum();
        let mut text: Vec<Line> = Vec::new();
        for line in &lines {
            let n = line.chars().count().min(left);
            left = left.saturating_sub(n);
            let mut spans = vec![Span::raw(line.chars().take(n).collect::<String>())];
            if typing && n < line.chars().count() {
                spans.push(Span::styled("▌", Style::new().fg(c.accent.color())));
                text.push(Line::from(spans));
                break;
            }
            text.push(Line::from(spans));
        }
        Paragraph::new(text)
            .style(Style::new().fg(c.text.color()))
            .alignment(Alignment::Center)
            .block(
                self.block(title)
                    .title_alignment(Alignment::Center)
                    .padding(Padding::new(3, 3, 1, 1)),
            )
            .render(panel, buf);
        y = y.saturating_add(panel_h).saturating_add(1);

        Line::from(vec![
            Span::styled("ratatui.rs", Style::new().fg(c.text.color())),
            Span::styled("   ·   ", Style::new().fg(c.dim.color())),
            Span::styled("cargo add ratatui", Style::new().fg(c.accent.color())),
        ])
        .centered()
        .render(Rect::new(area.x, y, area.width, 1).intersection(area), buf);
    }

    /// Ferris at `left`, `top` (in half rows): the claws snap on every
    /// other beat, and now and then a blink.
    fn ferris(&self, buf: &mut Buffer, left: u16, top: usize, scale: usize, t: f32, beat: f32) {
        let snap = num::floor_u32(beat).is_multiple_of(2) && beat.fract() < 0.22;
        let blink = t.rem_euclid(4.3) > 4.15;
        let orange = Rgb::hex(0xf74c00);
        let shade = Rgb::hex(0xa52b00);
        pixels(buf, left, top, (FERRIS_W, FERRIS_H), scale, |x, y| {
            let row = if snap {
                SNAP.get(y).or_else(|| FERRIS.get(y))
            } else {
                FERRIS.get(y)
            };
            match row?.chars().nth(x)? {
                'o' => Some(orange),
                'd' => Some(shade),
                'w' if blink => Some(orange),
                'e' if blink => Some(orange),
                'w' => Some(Rgb::WHITE),
                'e' | 'k' | 'm' => Some(Rgb::hex(0x1a0a04)),
                _ => None,
            }
        });
    }

    fn widgets(&self, buf: &mut Buffer, area: Rect, t: f32, beat: f32) {
        let c = &self.colors;
        let word = art::pixel_font("widgets");
        let scale = (1..=3).rev().find(|&s: &usize| {
            word.width.saturating_mul(s) <= usize::from(area.width) / 2
                && usize::from(area.height) >= s.saturating_mul(4).saturating_mul(6)
        });
        let head_h = scale.map_or(1, |s| num::u16_of(s.saturating_mul(4)));
        let [head, tagline, body] = Layout::vertical([
            Constraint::Length(head_h),
            Constraint::Length(2),
            Constraint::Fill(1),
        ])
        .areas(area.inner(Margin::new(2, 1)));
        match scale {
            Some(s) => {
                let left = head.x.saturating_add(
                    head.width
                        .saturating_sub(num::u16_of(word.width.saturating_mul(s)))
                        / 2,
                );
                let width = word.width.max(1);
                pixels(
                    buf,
                    left,
                    usize::from(head.y).saturating_mul(2),
                    (word.width, 8),
                    s,
                    |x, y| {
                        (word.get(x, y).ink > 0.5).then(|| {
                            c.palette
                                .sample(0.5 + 0.55 * num::float(x) / num::float(width))
                                .lerp(Rgb::WHITE, 0.15)
                        })
                    },
                );
            }
            None => Line::styled("widgets", Style::new().fg(c.accent.color()).bold())
                .centered()
                .render(head, buf),
        }
        Line::styled(
            "Every box is a widget that comes with ratatui",
            Style::new().fg(c.dim.color()),
        )
        .centered()
        .render(tagline, buf);

        let progress = (f64::from(t) / self.hold.max(1.0)).clamp(0.0, 1.0);
        if self.index.is_multiple_of(2) {
            let [map, side] = Layout::horizontal([Constraint::Percentage(62), Constraint::Fill(1)])
                .spacing(1)
                .areas(body);
            self.world(buf, map, t);
            let [spark, bars, gauge] = Layout::vertical([
                Constraint::Percentage(30),
                Constraint::Fill(1),
                Constraint::Length(3),
            ])
            .areas(side);
            self.sparkline(buf, spark, t);
            self.bars(buf, bars, t);
            self.gauge(buf, gauge, progress);
        } else {
            let [tabs, middle, gauges] = Layout::vertical([
                Constraint::Length(3),
                Constraint::Fill(1),
                Constraint::Length(3),
            ])
            .areas(body);
            let [chart, table] =
                Layout::horizontal([Constraint::Percentage(55), Constraint::Fill(1)])
                    .spacing(1)
                    .areas(middle);
            let [line, gauge] = Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)])
                .spacing(1)
                .areas(gauges);
            // The table's highlight and the selected tab move together.
            let selected = num::floor_usize(beat / 2.0)
                .checked_rem(WIDGETS.len())
                .unwrap_or(0);
            self.tabs(buf, tabs, selected);
            self.chart(buf, chart, t);
            self.table(buf, table, selected);
            self.line_gauge(buf, line, t);
            self.gauge(buf, gauge, progress);
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

    fn sparkline(&self, buf: &mut Buffer, area: Rect, t: f32) {
        let c = &self.colors;
        let block = self.block("Sparkline");
        let n = usize::from(block.inner(area).width);
        // A new value four times a second; the line scrolls left.
        let start = num::floor_usize(t * 4.0);
        let data: Vec<u64> = (start..start.saturating_add(n))
            .map(|k| {
                let k = num::float(k);
                let v = 45.0
                    + 22.0 * (k * 0.21).sin()
                    + 16.0 * (k * 0.067 + 1.3).sin()
                    + 12.0 * jitter(k);
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

    fn line_gauge(&self, buf: &mut Buffer, area: Rect, t: f32) {
        let c = &self.colors;
        let ratio = 0.55 + 0.3 * (t * 0.45).sin() * (t * 0.17 + 0.6).cos();
        LineGauge::default()
            .block(self.block("LineGauge"))
            .filled_style(Style::new().fg(c.palette.sample(0.7).lerp(Rgb::WHITE, 0.1).color()))
            .unfilled_style(Style::new().fg(c.border.color()))
            .ratio(f64::from(ratio).clamp(0.0, 1.0))
            .label(Span::styled(
                format!("memory {:>3.0}%", ratio * 100.0),
                Style::new().fg(c.text.color()),
            ))
            .render(area, buf);
    }

    fn tabs(&self, buf: &mut Buffer, area: Rect, selected: usize) {
        let c = &self.colors;
        Tabs::new(WIDGETS.iter().map(|&(name, _)| name))
            .block(self.block("Tabs"))
            .style(Style::new().fg(c.dim.color()))
            .highlight_style(
                Style::new()
                    .fg(c.accent.color())
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )
            .divider(Span::styled("·", Style::new().fg(c.border.color())))
            .select(selected)
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

    fn table(&self, buf: &mut Buffer, area: Rect, selected: usize) {
        let c = &self.colors;
        let rows = WIDGETS.iter().map(|&(name, what)| Row::new([name, what]));
        let table = Table::new(rows, [Constraint::Length(10), Constraint::Fill(1)])
            .block(self.block("Table"))
            .header(
                Row::new(["Widget", "What it shows"])
                    .style(
                        Style::new()
                            .fg(c.accent.color())
                            .add_modifier(Modifier::BOLD),
                    )
                    .bottom_margin(1),
            )
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

/// A steady pseudo-random value in `-0.5..0.5` for `k`.
fn jitter(k: f32) -> f32 {
    ((k * 12.9898).sin() * 43_758.547).fract() - 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cards_draw_at_any_size() {
        for n in 0..4 {
            let card = Card::new(n, &Palette::first(), 0.0, 10.0);
            for (w, h) in [(1, 1), (12, 4), (40, 12), (80, 24), (400, 120)] {
                for t in [0.0, 2.0, 6.0, 30.0] {
                    let mut buf = Buffer::empty(Rect::new(0, 0, w, h));
                    card.draw(&mut buf, t, 3.5);
                }
            }
        }
    }
}
