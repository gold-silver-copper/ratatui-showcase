//! Widgets from ratatui's community, one at a time, under a header that
//! stays: "ratatui" in the 8x8 font, the rat, and a line about it. Each
//! visit shows a few of them, a while each, and the next visit carries on
//! where it left off.

use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::Duration;

use edtui::syntect::easy::HighlightLines;
use edtui::syntect::util::LinesWithEndings;
use edtui::{
    EditorMode, EditorState, EditorTheme, EditorView, Index2, LineNumbers, Lines, SYNTAX_SET,
    SyntaxHighlighter, THEME_SET,
};
use qrcode::QrCode;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, RatatuiMascot, Widget};
use ratatui_markdown::ThemeConfig;
use ratatui_markdown::markdown::{MarkdownRenderer, RenderHooks};
use ratatui_wireframe::WireframeWidget;
use ratatui_wireframe::model::Model;
use tachyonfx::fx;
use tachyonfx::{Effect, Interpolation, Motion};
use tui_equalizer::{Band, Equalizer};
use tui_globe::{CONTOUR_COLOR, Camera, Globe, LAND_COLOR, MapData};
use tui_qrcode::{Colors as QrColors, QrCodeWidget, QuietZone, Scaling};

use crate::art;
use crate::card::{self, Colors};
use crate::num;
use crate::theme::{Palette, Rgb};

/// Libraries a visit shows.
pub const PER_VISIT: usize = 3;
/// The shortest a library shows, in seconds.
pub const SHOW: f64 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Exhibit {
    Globe,
    Code,
    Qr,
    Markdown,
    Equalizer,
    Wireframe,
}

const EXHIBITS: [Exhibit; 6] = [
    Exhibit::Globe,
    Exhibit::Code,
    Exhibit::Qr,
    Exhibit::Markdown,
    Exhibit::Equalizer,
    Exhibit::Wireframe,
];

impl Exhibit {
    /// The crate and what it is.
    fn label(self) -> (&'static str, &'static str) {
        match self {
            Exhibit::Globe => ("tui-globe", "a 3D globe in braille"),
            Exhibit::Code => ("edtui", "a vim-like code editor"),
            Exhibit::Qr => ("tui-qrcode", "QR codes, scannable from the screen"),
            Exhibit::Markdown => ("ratatui-markdown", "markdown, tables and diagrams"),
            Exhibit::Equalizer => ("tui-equalizer", "an equalizer, here on the beat"),
            Exhibit::Wireframe => ("ratatui-wireframe", "3D wireframe models"),
        }
    }
}

/// The program the editor types out, and the window it makes.
const HELLO: &str = r#"use ratatui::widgets::{Block, Paragraph};

fn main() -> std::io::Result<()> {
    ratatui::run(|terminal| loop {
        terminal.draw(|frame| {
            let hello = Paragraph::new("Hello, world!")
                .centered()
                .block(Block::bordered().title("ratatui"));
            frame.render_widget(hello, frame.area());
        })?;
        if ratatui::crossterm::event::read()?.is_key_press() {
            break Ok(());
        }
    })
}"#;

const MARKDOWN: &str = r#"# ratatui

A Rust library for **terminal user interfaces**: you describe the screen,
ratatui draws it, one frame at a time.

## How a frame happens

```mermaid
graph LR
    A[App state] --> B[Widgets]
    B --> C[Buffer]
    C --> D[Terminal]
```

## Widgets built in

| Widget | Shows |
| ------ | ----- |
| Paragraph | styled, wrapped text |
| Table | rows and columns |
| Chart | lines on axes |
| Canvas | free drawing |
| Gauge | progress |

## Your own widget

```rust
struct Hello;

impl Widget for Hello {
    fn render(self, area: Rect, buf: &mut Buffer) {
        buf.set_string(area.x, area.y, "hi", Style::new());
    }
}
```

> Every widget, here or in the community, is this one trait.

## Add it

```sh
cargo add ratatui
```
"#;

/// The size of ratatui's `RatatuiMascot`, in cells.
const MASCOT_W: u16 = 32;
const MASCOT_H: u16 = 16;

/// Cities on the globe, `(latitude, longitude)`.
const CITIES: &[(f32, f32)] = &[
    (51.5, -0.13),
    (40.7, -74.0),
    (37.8, -122.4),
    (35.7, 139.7),
    (-33.9, 151.2),
    (-23.5, -46.6),
    (-33.9, 18.4),
    (19.1, 72.9),
    (1.35, 103.8),
];

/// The globe's map, decompressed on first use.
fn map() -> &'static MapData {
    static MAP: OnceLock<MapData> = OnceLock::new();
    MAP.get_or_init(MapData::embedded)
}

/// Decompresses the globe's map ahead of time, on a thread of its own, so
/// no frame waits for it.
pub fn warm() {
    std::thread::spawn(map);
}

pub struct Gallery {
    colors: Colors,
    /// The first exhibit of this visit.
    first: usize,
    started: f64,
    /// How long it holds, in seconds.
    pub hold: f64,
    /// The exhibit showing, and the sweep it came in with (`Send` but not
    /// `Sync`; with `&mut self` the lock is never contended).
    showing: Option<usize>,
    sweep: Mutex<Option<Effect>>,
    /// The markdown, rendered for one width.
    markdown: Option<(u16, Vec<Line<'static>>)>,
    /// The globe's last picture: where, and at which of its frames.
    globe: Option<(Rect, usize, Buffer)>,
}

impl Gallery {
    /// The `n`th visit.
    pub fn new(n: usize, palette: &Palette, now: f64, hold: f64) -> Self {
        Self {
            colors: Colors::new(palette),
            first: n.saturating_mul(PER_VISIT),
            started: now,
            hold,
            showing: None,
            sweep: Mutex::new(None),
            markdown: None,
            globe: None,
        }
    }

    pub fn background(&self) -> Rgb {
        self.colors.bg
    }

    /// Draws the gallery at `now`, `dt` seconds after the last frame, with
    /// `beat` the show's beats.
    pub fn draw(&mut self, buf: &mut Buffer, now: f64, dt: f32, beat: f32) {
        let t = num::narrow(now - self.started);
        self.colors.backdrop(buf);
        // The bottom row is the caption's.
        let area = Rect {
            height: buf.area.height.saturating_sub(1),
            ..buf.area
        }
        .inner(Margin::new(2, 1));
        // The rat needs its full height to look like itself.
        let header_h = if area.height >= 44 {
            MASCOT_H
        } else {
            (area.height / 4).clamp(4, 12)
        };
        let [header, _, body] = Layout::vertical([
            Constraint::Length(header_h),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .areas(area);
        self.header(buf, header);

        let each = num::narrow(self.hold) / num::float(PER_VISIT);
        let step = num::floor_usize(t / each.max(1.0)).min(PER_VISIT.saturating_sub(1));
        let index = self
            .first
            .saturating_add(step)
            .checked_rem(EXHIBITS.len())
            .unwrap_or(0);
        let Some(&exhibit) = EXHIBITS.get(index) else {
            return;
        };
        let local = t - each * num::float(step);

        let c = &self.colors;
        let (name, what) = exhibit.label();
        let dots: String = (0..PER_VISIT)
            .map(|i| if i == step { "●" } else { "○" })
            .collect::<Vec<_>>()
            .join(" ");
        let block = c
            .block(name)
            .title(
                Line::styled(format!(" {what} "), Style::new().fg(c.dim.color())).right_aligned(),
            )
            .title_bottom(
                Line::styled(format!(" {dots} "), Style::new().fg(c.dim.color())).centered(),
            );
        let inner = block.inner(body);
        block.render(body, buf);
        match exhibit {
            Exhibit::Globe => self.globe(buf, inner, local),
            Exhibit::Code => self.code(buf, inner, local, each),
            Exhibit::Qr => self.qr(buf, inner),
            Exhibit::Markdown => self.markdown(buf, inner, local, each),
            Exhibit::Equalizer => self.equalizer(buf, inner, local, beat),
            Exhibit::Wireframe => self.wireframe(buf, inner, local),
        }

        // Each exhibit sweeps in over the last.
        let sweep = self.sweep.get_mut().unwrap_or_else(PoisonError::into_inner);
        if self.showing != Some(step) {
            self.showing = Some(step);
            let panel = self.colors.panel.color();
            *sweep = Some(fx::sweep_in(
                Motion::LeftToRight,
                (inner.width / 4).clamp(4, 40),
                0,
                panel,
                (700, Interpolation::QuadOut),
            ));
        }
        if let Some(effect) = sweep {
            effect.process(Duration::from_secs_f32(dt.max(0.0)).into(), buf, inner);
            if effect.done() {
                *sweep = None;
            }
        }
    }

    /// The rat, "ratatui" in the 8x8 font and a line about it, centred,
    /// each left out when there's no room.
    fn header(&self, buf: &mut Buffer, area: Rect) {
        let c = &self.colors;
        let word = art::pixel_font("ratatui");
        let blurb = "Terminal user interfaces in Rust · widgets from its community";
        let blurb_w = num::u16_of(blurb.chars().count());
        // Pixels are half a row tall; the word wants a row for the blurb.
        let rows = usize::from(area.height);
        // The largest word that fits, keeping the rat if it can stay.
        let fits = |s: usize, rat: u16| {
            let text = num::u16_of(word.width.saturating_mul(s)).max(blurb_w);
            s.saturating_mul(4).saturating_add(2) <= rows && rat.saturating_add(text) <= area.width
        };
        let rat_room = MASCOT_W.saturating_add(4);
        let with_rat = area.height >= MASCOT_H && (0..=4).any(|s| fits(s, rat_room));
        let rat_w = if with_rat { rat_room } else { 0 };
        let scale = (0..=4).rev().find(|&s| fits(s, rat_w)).unwrap_or(0);
        let word_w = num::u16_of(word.width.saturating_mul(scale));
        let text_w = word_w.max(blurb_w);
        let total = rat_w.saturating_add(text_w).min(area.width);
        let left = area.x.saturating_add(area.width.saturating_sub(total) / 2);
        if with_rat {
            RatatuiMascot::new().render(Rect::new(left, area.y, MASCOT_W, MASCOT_H), buf);
        }
        let text_left = left.saturating_add(rat_w);
        let text = Rect::new(text_left, area.y, text_w, area.height).intersection(area);
        let word_rows = num::u16_of(scale.saturating_mul(4));
        let top = text
            .y
            .saturating_add(text.height.saturating_sub(word_rows.saturating_add(2)) / 2);
        if scale > 0 && word_w <= text.width {
            let width = word.width.max(1);
            let x = text.x.saturating_add(text.width.saturating_sub(word_w) / 2);
            card::pixels(
                buf,
                x,
                usize::from(top).saturating_mul(2),
                (word.width, 8),
                scale,
                |px, py| {
                    (word.get(px, py).ink > 0.5).then(|| {
                        c.palette
                            .sample(0.55 * num::float(px) / num::float(width))
                            .lerp(Rgb::WHITE, 0.15)
                    })
                },
            );
        } else {
            Line::styled("ratatui", Style::new().fg(c.accent.color()).bold())
                .centered()
                .render(Rect::new(text.x, top, text.width, 1), buf);
        }
        let blurb_y = top.saturating_add(word_rows.max(1)).saturating_add(1);
        Line::styled(blurb, Style::new().fg(c.dim.color()))
            .centered()
            .render(
                Rect::new(text.x, blurb_y, text.width, 1).intersection(area),
                buf,
            );
    }

    /// The globe turns, the cities marked as they come round. Drawing it
    /// takes milliseconds and it turns slowly: it's drawn anew 20 times a
    /// second, and copied in between.
    fn globe(&mut self, buf: &mut Buffer, area: Rect, t: f32) {
        let frame = num::floor_usize(t * 20.0);
        if self
            .globe
            .as_ref()
            .is_none_or(|(at, seen, _)| (*at, *seen) != (area, frame))
        {
            let mut picture = Buffer::empty(area);
            self.draw_globe(&mut picture, area, num::float(frame) / 20.0);
            self.globe = Some((area, frame, picture));
        }
        let Some((_, _, picture)) = &self.globe else {
            return;
        };
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                if let (Some(from), Some(to)) = (picture.cell((x, y)), buf.cell_mut((x, y)))
                    && from.symbol() != " "
                {
                    to.set_symbol(from.symbol()).set_fg(from.fg);
                }
            }
        }
    }

    fn draw_globe(&self, buf: &mut Buffer, area: Rect, t: f32) {
        let c = &self.colors;
        let map = map();
        let camera = Camera {
            yaw: t * 0.35,
            pitch: 0.35,
            zoom: 1.0,
        };
        Globe::new(map, camera).render(area, buf);
        // Its colours are fixed; the palette's go over them.
        let coast = c.palette.sample(0.2).lerp(Rgb::WHITE, 0.15).color();
        let land = c.palette.sample(0.55).lerp(c.panel, 0.35).color();
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    if cell.fg == CONTOUR_COLOR {
                        cell.set_fg(coast);
                    } else if cell.fg == LAND_COLOR {
                        cell.set_fg(land);
                    }
                }
            }
        }
        for &(lat, lon) in CITIES {
            if let Some(at) = tui_globe::project_point(lat, lon, camera, area)
                && let Some(cell) = buf.cell_mut(at)
            {
                cell.set_char('●').set_fg(c.accent.color());
            }
        }
    }

    /// A ratatui hello world types itself out; once it's done, the window
    /// it makes appears beside it.
    fn code(&self, buf: &mut Buffer, area: Rect, t: f32, each: f32) {
        let c = &self.colors;
        let total = HELLO.chars().count();
        let speed = num::float(total) / (each * 0.7).max(1.0);
        let typed = num::floor_usize((t - 0.4) * speed).min(total);
        let text: String = HELLO.chars().take(typed).collect();
        let row = text.matches('\n').count();
        let col = text.rsplit('\n').next().map_or(0, |l| l.chars().count());
        let done = typed >= total;
        let [editor, window] =
            Layout::horizontal([Constraint::Percentage(62), Constraint::Fill(1)])
                .spacing(2)
                .areas(area);
        let mut state = EditorState::new(Lines::from(text.as_str()));
        state.mode = EditorMode::Insert;
        state.cursor = Index2::new(row, col);
        let theme = EditorTheme::default()
            .base(Style::new().bg(c.panel.color()).fg(c.text.color()))
            .cursor_style(Style::new().bg(c.accent.color()).fg(c.panel.color()))
            .line_numbers_style(Style::new().bg(c.panel.color()).fg(c.dim.color()))
            .hide_status_line();
        EditorView::new(&mut state)
            .theme(theme)
            .syntax_highlighter(SyntaxHighlighter::new("one-dark", "rs").ok())
            .line_numbers(LineNumbers::Absolute)
            .wrap(false)
            .render(editor, buf);
        if done {
            let window = window.centered(Constraint::Max(40), Constraint::Max(9));
            let height = window.height;
            Paragraph::new(vec![
                Line::default();
                usize::from(height.saturating_sub(2) / 2)
            ])
            .style(Style::new().bg(Rgb::BLACK.color()))
            .render(window, buf);
            let inner = window.inner(Margin::new(0, height.saturating_sub(3) / 2));
            Paragraph::new("Hello, world!")
                .centered()
                .style(Style::new().fg(Rgb::WHITE.color()))
                .render(inner, buf);
            Block::bordered()
                .title("ratatui")
                .style(Style::new().fg(Rgb::WHITE.color()))
                .render(window, buf);
        }
    }

    /// A QR code for ratatui.rs, black on white, as large as fits.
    fn qr(&self, buf: &mut Buffer, area: Rect) {
        let c = &self.colors;
        let Ok(code) = QrCode::new("https://ratatui.rs") else {
            return;
        };
        // The modules and a quiet zone of four a side.
        let side = num::u16_of(code.width().saturating_add(8));
        let room = Rect {
            height: area.height.saturating_sub(2),
            ..area
        };
        let n = (room.width.checked_div(side).unwrap_or(0))
            .min(room.height.saturating_mul(2).checked_div(side).unwrap_or(0));
        let label = Line::styled(
            "scan for ratatui.rs",
            Style::new().fg(c.text.color()).add_modifier(Modifier::BOLD),
        );
        if n == 0 {
            label.centered().render(area, buf);
            return;
        }
        let widget = QrCodeWidget::new(code)
            .quiet_zone(QuietZone::Enabled)
            .scaling(Scaling::Exact(n, n))
            .colors(QrColors::Inverted)
            .style(Style::new().fg(Rgb::WHITE.color()).bg(Rgb::BLACK.color()));
        let size = widget.size(room);
        let at = room.centered(
            Constraint::Length(size.width),
            Constraint::Length(size.height),
        );
        if at.width < size.width || at.height < size.height {
            label.centered().render(area, buf);
            return;
        }
        widget.render(at, buf);
        label.centered().render(
            Rect::new(area.x, at.bottom().saturating_add(1), area.width, 1).intersection(area),
            buf,
        );
    }

    /// A short document about ratatui, scrolling from top to bottom.
    fn markdown(&mut self, buf: &mut Buffer, area: Rect, t: f32, each: f32) {
        let c = &self.colors;
        let area = area.centered_horizontally(Constraint::Max(100));
        let width = area.width.saturating_sub(4);
        if self.markdown.as_ref().is_none_or(|(w, _)| *w != width) {
            let theme = ThemeConfig {
                text_color: c.text.color(),
                muted_text_color: c.dim.color(),
                primary_color: c.accent.color(),
                border_color: c.border.color(),
                ..ThemeConfig::default()
            };
            let renderer =
                MarkdownRenderer::new(usize::from(width)).with_render_hooks(Box::new(Hooks {
                    heading: Style::new()
                        .fg(c.accent.color())
                        .add_modifier(Modifier::BOLD),
                    border: Style::new().fg(c.border.color()),
                }));
            let lines = renderer.render(&renderer.parse(MARKDOWN), &theme);
            self.markdown = Some((width, lines));
        }
        let Some((_, lines)) = &self.markdown else {
            return;
        };
        let overflow = num::u16_of(lines.len()).saturating_sub(area.height);
        let along = ((t - 1.0) / (each * 0.8 - 1.0).max(1.0)).clamp(0.0, 1.0);
        let scroll = num::floor_u16(along * f32::from(overflow));
        Paragraph::new(lines.clone())
            .scroll((scroll, 0))
            .render(area.inner(Margin::new(2, 0)), buf);
    }

    /// Bars that jump on every beat and fall back, low bands most.
    fn equalizer(&self, buf: &mut Buffer, area: Rect, t: f32, beat: f32) {
        let c = &self.colors;
        let area = area.centered(Constraint::Max(120), Constraint::Max(24));
        // More than one band to two columns would draw past the area.
        let n = usize::from(area.width / 2);
        let kick = (1.0 - beat.fract()).powi(3);
        let bands: Vec<Band> = (0..n)
            .map(|i| {
                let f = num::float(i) / num::float(n.max(1));
                let wobble = 0.5 + 0.5 * (t * (2.0 + 3.0 * f) + num::float(i) * 0.7).sin();
                Band::from(f64::from((1.0 - f) * kick * 0.7 + wobble * 0.3))
            })
            .collect();
        Equalizer {
            bands,
            brightness: 1.0,
        }
        .render(area, buf);
        // Coloured by height, from the palette.
        let rows = f32::from(area.height.max(1));
        for y in area.top()..area.bottom() {
            let up = f32::from(area.bottom().saturating_sub(y)) / rows;
            let color = c.palette.sample(up * 0.6).lerp(Rgb::WHITE, 0.1).color();
            for x in area.left()..area.right() {
                if let Some(cell) = buf.cell_mut((x, y))
                    && cell.symbol() == "▄"
                {
                    cell.set_fg(color);
                }
            }
        }
    }

    /// An icosahedron turning on all three axes.
    fn wireframe(&self, buf: &mut Buffer, area: Rect, t: f32) {
        static SHAPE: OnceLock<Model> = OnceLock::new();
        let model = SHAPE.get_or_init(icosahedron);
        WireframeWidget::new(f64::from(t * 0.5), f64::from(t * 0.8), f64::from(t * 0.2))
            .title(" icosahedron ")
            .color(self.colors.accent.color())
            .model(model)
            .render(area, buf);
    }
}

/// Code blocks in syntax colours, and headings in the accent.
struct Hooks {
    heading: Style,
    border: Style,
}

impl Hooks {
    fn heading(&self, text: &str) -> Option<Line<'static>> {
        Some(Line::styled(text.to_owned(), self.heading))
    }
}

impl RenderHooks for Hooks {
    fn heading1(&self, text: &str) -> Option<Line<'static>> {
        self.heading(text)
    }

    fn heading2(&self, text: &str) -> Option<Line<'static>> {
        self.heading(text)
    }

    fn heading3(&self, text: &str) -> Option<Line<'static>> {
        self.heading(text)
    }

    /// `None` leaves a block to the crate's own renderer: mermaid diagrams.
    fn render_code_block(&self, lang: &str, code: &str) -> Option<Vec<Line<'static>>> {
        let syntax = SYNTAX_SET.find_syntax_by_token(lang)?;
        let mut highlight = HighlightLines::new(syntax, THEME_SET.themes.get("base16-ocean.dark")?);
        let mut out = vec![Line::styled(format!("╭─ {lang}"), self.border)];
        for line in LinesWithEndings::from(code) {
            let mut spans = vec![Span::styled("│ ", self.border)];
            for (style, text) in highlight.highlight_line(line, &SYNTAX_SET).ok()? {
                let f = style.foreground;
                spans.push(Span::styled(
                    text.trim_end_matches('\n').to_owned(),
                    Style::new().fg(Color::Rgb(f.r, f.g, f.b)),
                ));
            }
            out.push(Line::from(spans));
        }
        out.push(Line::styled("╰─", self.border));
        Some(out)
    }
}

/// The twelve corners of an icosahedron, every pair of them two apart
/// joined by an edge.
fn icosahedron() -> Model {
    let phi = (1.0 + 5.0_f64.sqrt()) / 2.0;
    let mut vertices = Vec::new();
    for a in [-1.0, 1.0] {
        for b in [-phi, phi] {
            vertices.push((0.0, a, b));
            vertices.push((a, b, 0.0));
            vertices.push((b, 0.0, a));
        }
    }
    let scale = 1.5 / (1.0 + phi * phi).sqrt();
    let vertices: Vec<(f64, f64, f64)> = vertices
        .into_iter()
        .map(|(x, y, z)| (x * scale, y * scale, z * scale))
        .collect();
    let edge = 2.0 * scale;
    let mut edges = Vec::new();
    for (i, a) in vertices.iter().enumerate() {
        for (j, b) in vertices.iter().enumerate().skip(i.saturating_add(1)) {
            let d = ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2) + (a.2 - b.2).powi(2)).sqrt();
            if (d - edge).abs() < 1e-6 {
                edges.push((i, j));
            }
        }
    }
    Model { vertices, edges }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_exhibit_draws_at_any_size() {
        for n in 0..2 {
            let mut gallery = Gallery::new(n, &Palette::first(), 0.0, 24.0);
            for (w, h) in [(1, 1), (12, 4), (40, 12), (80, 24), (300, 90)] {
                for t in [0.0, 3.0, 7.9, 8.5, 12.0, 16.5, 20.0, 23.9] {
                    let mut buf = Buffer::empty(Rect::new(0, 0, w, h));
                    gallery.draw(&mut buf, t, 0.016, num::narrow(t * 2.0));
                }
            }
        }
    }

    #[test]
    fn the_icosahedron_has_thirty_edges() {
        assert_eq!(icosahedron().edges.len(), 30);
    }
}
