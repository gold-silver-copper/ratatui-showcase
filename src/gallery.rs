//! Widgets from ratatui's community, one at a time, under a bar that stays:
//! the rat, "ratatui" in the 8x8 font with a line about it, and the
//! libraries of the visit. Each library fills the screen beside a panel on
//! what it is and how it's used. Flat surfaces, no borders. Each visit
//! shows a few of them, a while each, and the next carries on where it
//! left off.

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
use ratatui::widgets::{Block, Padding, Paragraph, RatatuiMascot, Widget};
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
use crate::diff::DiffView;
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
    Diff,
    Qr,
    Markdown,
    Equalizer,
    Wireframe,
}

const EXHIBITS: [Exhibit; 7] = [
    Exhibit::Globe,
    Exhibit::Code,
    Exhibit::Diff,
    Exhibit::Qr,
    Exhibit::Markdown,
    Exhibit::Equalizer,
    Exhibit::Wireframe,
];

impl Exhibit {
    /// What it is and does, for the side panel.
    fn about(self) -> &'static str {
        match self {
            Exhibit::Globe => {
                "A 3D globe drawn in braille dots, eight to a cell. A camera turns it, \
                 and project_point pins anything to it: here, nine cities."
            }
            Exhibit::Code => {
                "A vim-like editor widget with syntax colours. Here it types out a whole \
                 ratatui app, then shows what that app draws."
            }
            Exhibit::Diff => {
                "Unified and side-by-side diffs with word highlights, syntax colours and \
                 navigation. Here: every change to ratatui's own source from 0.28 to 0.29."
            }
            Exhibit::Qr => {
                "QR codes in half blocks, two modules to a cell. Point a phone at it: \
                 it opens ratatui.rs."
            }
            Exhibit::Markdown => {
                "Markdown into ratatui text: headings, lists, emphasis and code. Hooks \
                 draw any part an app's own way; the code here is coloured by syntect."
            }
            Exhibit::Equalizer => {
                "Equalizer bars, one per band. Fed by the show's beat: every beat kicks \
                 the low bands."
            }
            Exhibit::Wireframe => {
                "Wireframe models turning on a braille canvas: a few solids, OBJ files, \
                 or any vertices and edges, like this icosahedron."
            }
        }
    }

    /// How an app draws it.
    fn usage(self) -> &'static str {
        match self {
            Exhibit::Globe => {
                "let camera = Camera {\n    yaw: t * 0.35,\n    pitch: 0.35,\n    zoom: 1.4,\n};\n\
                 let globe = Globe::new(&map, camera);\nframe.render_widget(globe, area);"
            }
            Exhibit::Code => {
                "let mut state = EditorState::default();\nlet view = EditorView::new(&mut state);\n\
                 frame.render_widget(view, area);"
            }
            Exhibit::Diff => {
                "let doc = DiffDocument::parse(patch)?;\nlet diff = Diff::new(&doc)\n    \
                 .mode(ViewMode::Split);\nframe.render_stateful_widget(\n    &diff, area, &mut state);"
            }
            Exhibit::Qr => {
                "let url = \"https://ratatui.rs\";\nlet code = QrCode::new(url)?;\n\
                 let qr = QrCodeWidget::new(code);\nframe.render_widget(qr, area);"
            }
            Exhibit::Markdown => {
                "let md = MarkdownRenderer::new(width);\nlet blocks = md.parse(doc);\n\
                 let lines = md.render(&blocks, &theme);\nlet page = Paragraph::new(lines);\n\
                 frame.render_widget(page, area);"
            }
            Exhibit::Equalizer => {
                "let bands = levels\n    .iter()\n    .map(|&level| Band::from(level))\n    .collect();\n\
                 let eq = Equalizer { bands, brightness: 1.0 };\nframe.render_widget(eq, area);"
            }
            Exhibit::Wireframe => {
                "let shape = Model::cube();\nlet wire = WireframeWidget::new(\n    pitch, yaw, roll,\n)\n\
                 .model(&shape);\nframe.render_widget(wire, area);"
            }
        }
    }

    /// The crate and what it is.
    fn label(self) -> (&'static str, &'static str) {
        match self {
            Exhibit::Globe => ("tui-globe", "a 3D globe in braille"),
            Exhibit::Code => ("edtui", "a vim-like code editor"),
            Exhibit::Diff => ("ratatui-diff", "diffs, side by side"),
            Exhibit::Qr => ("tui-qrcode", "QR codes, scannable from the screen"),
            Exhibit::Markdown => ("ratatui-markdown", "markdown as ratatui text"),
            Exhibit::Equalizer => ("tui-equalizer", "an equalizer, here on the beat"),
            Exhibit::Wireframe => ("ratatui-wireframe", "3D wireframe models"),
        }
    }
}

/// The program the editor types out, and the window it makes.
const HELLO: &str = r#"use ratatui::style::Stylize;
use ratatui::widgets::Paragraph;

fn main() -> std::io::Result<()> {
    ratatui::run(|terminal| loop {
        terminal.draw(|frame| {
            let hello = Paragraph::new("Hello, world!")
                .centered()
                .white()
                .on_blue();
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

1. Your app keeps its **state**.
2. Each frame, **widgets** draw that state into a buffer.
3. ratatui sends the **cells that changed** to the terminal.

## Widgets built in

- **Paragraph**: styled, wrapped text
- **Table**: rows and columns
- **Chart**: lines on axes
- **Canvas**: free drawing
- **Gauge**: progress

## Your own widget

```rust
struct Hello;

impl Widget for Hello {
    fn render(self, area: Rect, buf: &mut Buffer) {
        buf.set_string(area.x, area.y, "hi", Style::new());
    }
}
```

Every widget, here or in the community, is this one trait.

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
    /// The diff, from when it came on.
    diff: Option<DiffView>,
    /// Seconds skipped ahead.
    skipped: f32,
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
            diff: None,
            skipped: 0.0,
        }
    }

    pub fn background(&self) -> Rgb {
        self.colors.bg
    }

    /// Draws the gallery at `now`, `dt` seconds after the last frame, with
    /// `beat` the show's beats.
    pub fn draw(&mut self, buf: &mut Buffer, now: f64, dt: f32, beat: f32) {
        let t = num::narrow(now - self.started) + self.skipped;
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
        let [header, body] = Layout::vertical([Constraint::Length(header_h), Constraint::Fill(1)])
            .spacing(1)
            .areas(area);

        let each = num::narrow(self.hold) / num::float(PER_VISIT);
        let step = num::floor_usize(t / each.max(1.0)).min(PER_VISIT.saturating_sub(1));
        let Some(exhibit) = self.exhibit(step) else {
            return;
        };
        let local = t - each * num::float(step);
        self.header(buf, header, step);

        // The library fills the screen beside a panel about it.
        let side_w = (body.width.saturating_mul(3) / 10).clamp(36, 56);
        let [main, side] = if body.width >= side_w.saturating_mul(2) {
            Layout::horizontal([Constraint::Fill(1), Constraint::Length(side_w)])
                .spacing(2)
                .areas(body)
        } else {
            [body, Rect::ZERO]
        };
        Block::new().style(self.colors.surface()).render(main, buf);
        let rest = self.side(buf, side, exhibit);
        let view = main.inner(Margin::new(2, 1));
        match exhibit {
            Exhibit::Globe => self.globe(buf, view, local),
            Exhibit::Code => self.code(buf, view, rest, local, each),
            Exhibit::Diff => {
                let first = self.first;
                self.diff
                    .get_or_insert_with(|| DiffView::new(first, now))
                    .draw(buf, main, now, dt);
            }
            Exhibit::Qr => self.qr(buf, view),
            Exhibit::Markdown => self.markdown(buf, view, local, each),
            Exhibit::Equalizer => self.equalizer(buf, view, local, beat),
            Exhibit::Wireframe => self.wireframe(buf, view, local),
        }

        // Each library sweeps in over the last.
        let sweep = self.sweep.get_mut().unwrap_or_else(PoisonError::into_inner);
        if self.showing != Some(step) {
            self.showing = Some(step);
            let bg = self.colors.bg.color();
            *sweep = Some(fx::sweep_in(
                Motion::LeftToRight,
                (body.width / 4).clamp(4, 40),
                0,
                bg,
                (700, Interpolation::QuadOut),
            ));
        }
        if let Some(effect) = sweep {
            effect.process(Duration::from_secs_f32(dt.max(0.0)).into(), buf, body);
            if effect.done() {
                *sweep = None;
            }
        }
    }

    /// Skips to the next library of this visit, if there is one: how many
    /// seconds it skipped.
    pub fn next(&mut self, now: f64) -> Option<f64> {
        let t = num::narrow(now - self.started) + self.skipped;
        let each = num::narrow(self.hold) / num::float(PER_VISIT);
        let step = num::floor_usize(t / each.max(1.0));
        let next = step.saturating_add(1);
        if next >= PER_VISIT {
            return None;
        }
        let jump = each * num::float(next) - t;
        self.skipped += jump;
        Some(f64::from(jump))
    }

    /// The `step`th library of this visit.
    fn exhibit(&self, step: usize) -> Option<Exhibit> {
        let index = self
            .first
            .saturating_add(step)
            .checked_rem(EXHIBITS.len())?;
        EXHIBITS.get(index).copied()
    }

    /// A bar across the top: the rat, "ratatui" in the 8x8 font over a line
    /// about it, and the libraries of this visit, the one showing lit. Each
    /// is left out when there's no room.
    fn header(&self, buf: &mut Buffer, area: Rect, step: usize) {
        let c = &self.colors;
        Block::new().style(c.surface()).render(area, buf);
        let rail_w: u16 = if area.width >= 120 { 34 } else { 0 };
        let rat_w = if area.height >= MASCOT_H && area.width >= 100 {
            MASCOT_W.saturating_add(4)
        } else {
            0
        };
        let [rat, middle, rail] = Layout::horizontal([
            Constraint::Length(rat_w),
            Constraint::Fill(1),
            Constraint::Length(rail_w),
        ])
        .areas(area);
        if rat_w > 0 {
            RatatuiMascot::new().render(
                Rect::new(rat.x.saturating_add(2), rat.y, MASCOT_W, MASCOT_H),
                buf,
            );
        }

        // The largest word that fits over the blurb.
        let word = art::pixel_font("ratatui");
        let blurb = "Terminal user interfaces in Rust · widgets from its community";
        let rows = usize::from(middle.height);
        let scale = (1..=4).rev().find(|&s: &usize| {
            s.saturating_mul(4).saturating_add(2) <= rows
                && word.width.saturating_mul(s) <= usize::from(middle.width)
        });
        let word_rows = scale.map_or(1, |s| num::u16_of(s.saturating_mul(4)));
        let top = middle
            .y
            .saturating_add(middle.height.saturating_sub(word_rows.saturating_add(2)) / 2);
        match scale {
            Some(s) => {
                let word_w = num::u16_of(word.width.saturating_mul(s));
                let x = middle
                    .x
                    .saturating_add(middle.width.saturating_sub(word_w) / 2);
                let width = word.width.max(1);
                card::pixels(
                    buf,
                    x,
                    usize::from(top).saturating_mul(2),
                    (word.width, 8),
                    s,
                    |px, py| {
                        (word.get(px, py).ink > 0.5).then(|| {
                            c.palette
                                .sample(0.55 * num::float(px) / num::float(width))
                                .lerp(Rgb::WHITE, 0.15)
                        })
                    },
                );
            }
            None => Line::styled("ratatui", Style::new().fg(c.accent.color()).bold())
                .centered()
                .render(Rect::new(middle.x, top, middle.width, 1), buf),
        }
        Line::styled(blurb, Style::new().fg(c.dim.color()))
            .centered()
            .render(
                Rect::new(
                    middle.x,
                    top.saturating_add(word_rows).saturating_add(1),
                    middle.width,
                    1,
                )
                .intersection(middle),
                buf,
            );

        // The visit's libraries, the one showing on the accent.
        if rail_w > 0 {
            let slots = Layout::vertical([Constraint::Fill(1); PER_VISIT])
                .spacing(1)
                .split(rail.inner(Margin::new(2, 1)));
            for (i, slot) in slots.iter().enumerate() {
                let Some(exhibit) = self.exhibit(i) else {
                    continue;
                };
                let (name, what) = exhibit.label();
                let lit = i == step;
                let (fill, fg, sub) = if lit {
                    (c.accent, c.bg, c.bg.lerp(c.accent, 0.3))
                } else {
                    (c.raised, c.text, c.dim)
                };
                let lines = vec![
                    Line::styled(
                        name,
                        Style::new().fg(fg.color()).add_modifier(Modifier::BOLD),
                    ),
                    Line::styled(what, Style::new().fg(sub.color())),
                ];
                let pad = slot.height.saturating_sub(2) / 2;
                Paragraph::new(lines)
                    .style(Style::new().bg(fill.color()))
                    .block(Block::new().padding(Padding::new(2, 1, pad, 0)))
                    .render(*slot, buf);
            }
        }
    }

    /// The side panel: the crate, what it is, how it's added and drawn.
    /// Returns the room left at its foot.
    fn side(&self, buf: &mut Buffer, area: Rect, exhibit: Exhibit) -> Rect {
        let c = &self.colors;
        if area.is_empty() {
            return area;
        }
        Block::new().style(c.surface()).render(area, buf);
        let inner = area.inner(Margin::new(2, 1));
        let (name, what) = exhibit.label();
        let about = card::wrap(exhibit.about(), usize::from(inner.width));
        let mut lines = vec![
            Line::styled(
                name,
                Style::new()
                    .fg(c.accent.color())
                    .add_modifier(Modifier::BOLD),
            ),
            Line::styled(what, Style::new().fg(c.dim.color())),
            Line::default(),
        ];
        lines.extend(about.into_iter().map(Line::from));
        let text_h = num::u16_of(lines.len());
        Paragraph::new(lines).render(inner, buf);

        // `cargo add`, then how it's drawn, on raised surfaces.
        let [_, add, _, usage] = Layout::vertical([
            Constraint::Length(text_h.saturating_add(1)),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .areas(inner);
        Paragraph::new(Line::from(vec![
            Span::styled("cargo add ", Style::new().fg(c.dim.color())),
            Span::styled(
                name,
                Style::new().fg(c.text.color()).add_modifier(Modifier::BOLD),
            ),
        ]))
        .style(Style::new().bg(c.raised.color()))
        .block(Block::new().padding(Padding::new(2, 2, 1, 0)))
        .render(add, buf);
        if exhibit == Exhibit::Code {
            return usage;
        }
        Paragraph::new(
            highlight(exhibit.usage(), "rs", usize::from(usage.width)).unwrap_or_default(),
        )
        .style(Style::new().bg(c.raised.color()))
        .block(Block::new().padding(Padding::uniform(1)))
        .render(usage, buf);
        Rect::ZERO
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
        // Zoomed in to fill the width, its poles cut off when it's wide.
        let zoom = f32::from(area.width) / f32::from(area.height.saturating_mul(2).max(1));
        let camera = Camera {
            yaw: t * 0.35,
            pitch: 0.35,
            zoom: zoom.clamp(1.0, 1.8),
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

    /// A ratatui hello world types itself out; once it's done, what it
    /// draws appears in `output`, at the side panel's foot.
    fn code(&self, buf: &mut Buffer, area: Rect, output: Rect, t: f32, each: f32) {
        let c = &self.colors;
        let total = HELLO.chars().count();
        let speed = num::float(total) / (each * 0.7).max(1.0);
        let typed = num::floor_usize((t - 0.4) * speed).min(total);
        let text: String = HELLO.chars().take(typed).collect();
        let row = text.matches('\n').count();
        let col = text.rsplit('\n').next().map_or(0, |l| l.chars().count());
        let mut state = EditorState::new(Lines::from(text.as_str()));
        state.mode = EditorMode::Insert;
        state.cursor = Index2::new(row, col);
        let theme = EditorTheme::default()
            .base(c.surface())
            .cursor_style(Style::new().bg(c.accent.color()).fg(c.panel.color()))
            .line_numbers_style(Style::new().bg(c.panel.color()).fg(c.dim.color()))
            .hide_status_line();
        EditorView::new(&mut state)
            .theme(theme)
            .syntax_highlighter(SyntaxHighlighter::new("one-dark", "rs").ok())
            .line_numbers(LineNumbers::Absolute)
            .wrap(false)
            .render(area, buf);

        if output.is_empty() {
            return;
        }
        let [label, window] =
            Layout::vertical([Constraint::Length(2), Constraint::Fill(1)]).areas(output);
        Line::styled("What it draws", Style::new().fg(c.dim.color())).render(label, buf);
        if typed >= total {
            Block::new()
                .style(Style::new().bg(Color::Blue))
                .render(window, buf);
            let middle = window.centered_vertically(Constraint::Length(1));
            Line::styled("Hello, world!", Style::new().fg(Color::White))
                .centered()
                .render(middle, buf);
        } else {
            Block::new()
                .style(Style::new().bg(c.raised.color()))
                .render(window, buf);
            Line::styled("runs once it's typed", Style::new().fg(c.dim.color()))
                .centered()
                .render(window.centered_vertically(Constraint::Length(1)), buf);
        }
    }

    /// A QR code for ratatui.rs, black on white, as large as fits, beside
    /// the address in the 8x8 font.
    fn qr(&self, buf: &mut Buffer, area: Rect) {
        let c = &self.colors;
        let Ok(code) = QrCode::new("https://ratatui.rs") else {
            return;
        };
        // The modules and a quiet zone of four a side.
        let side = num::u16_of(code.width().saturating_add(8));
        let n = (area.width.checked_div(side).unwrap_or(0))
            .min(area.height.saturating_mul(2).checked_div(side).unwrap_or(0));
        if n == 0 {
            Line::styled("ratatui.rs", Style::new().fg(c.text.color()).bold())
                .centered()
                .render(area.centered_vertically(Constraint::Length(1)), buf);
            return;
        }
        let widget = QrCodeWidget::new(code)
            .quiet_zone(QuietZone::Enabled)
            .scaling(Scaling::Exact(n, n))
            .colors(QrColors::Inverted)
            .style(Style::new().fg(Color::White).bg(Color::Black));
        let size = widget.size(area);
        let word = art::pixel_font("ratatui.rs");
        let gap = 4;
        let text_room = area.width.saturating_sub(size.width).saturating_sub(gap);
        let scale = (1..=4)
            .rev()
            .find(|&s: &usize| word.width.saturating_mul(s) <= usize::from(text_room));
        let text_w = scale.map_or(0, |s| num::u16_of(word.width.saturating_mul(s)));
        let total = if scale.is_some() {
            size.width.saturating_add(gap).saturating_add(text_w)
        } else {
            size.width
        };
        let left = area.x.saturating_add(area.width.saturating_sub(total) / 2);
        let top = area
            .y
            .saturating_add(area.height.saturating_sub(size.height) / 2);
        widget.render(
            Rect::new(left, top, size.width, size.height).intersection(area),
            buf,
        );
        let Some(s) = scale else { return };
        let x = left.saturating_add(size.width).saturating_add(gap);
        let word_rows = num::u16_of(s.saturating_mul(4));
        let y = area
            .y
            .saturating_add(area.height.saturating_sub(word_rows.saturating_add(3)) / 2);
        let width = word.width.max(1);
        card::pixels(
            buf,
            x,
            usize::from(y).saturating_mul(2),
            (word.width, 8),
            s,
            |px, py| {
                (word.get(px, py).ink > 0.5).then(|| {
                    c.palette
                        .sample(0.55 * num::float(px) / num::float(width))
                        .lerp(Rgb::WHITE, 0.15)
                })
            },
        );
        let below = Rect::new(x, y.saturating_add(word_rows).saturating_add(1), text_w, 2)
            .intersection(area);
        Paragraph::new(vec![
            Line::styled(
                "Scan it, or type it.",
                Style::new().fg(c.text.color()).bold(),
            ),
            Line::styled(
                "Docs, tutorials, examples and templates.",
                Style::new().fg(c.dim.color()),
            ),
        ])
        .centered()
        .render(below, buf);
    }

    /// A short document about ratatui, scrolling from top to bottom.
    fn markdown(&mut self, buf: &mut Buffer, area: Rect, t: f32, each: f32) {
        let c = &self.colors;
        let width = area.width;
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
                    code: Style::new().bg(c.raised.color()),
                    width: usize::from(width),
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
            .render(area, buf);
    }

    /// Bars that jump on every beat and fall back, low bands most.
    fn equalizer(&self, buf: &mut Buffer, area: Rect, t: f32, beat: f32) {
        let c = &self.colors;
        // More than one band to two columns would draw past the area.
        let n = usize::from(area.width / 2);
        let kick = (1.0 - beat.fract()).powi(3);
        let bands: Vec<Band> = (0..n)
            .map(|i| {
                let f = num::float(i) / num::float(n.max(1));
                let wobble = 0.5 + 0.5 * (t * (2.0 + 3.0 * f) + num::float(i) * 0.7).sin();
                Band::from(f64::from(0.25 + (1.0 - f) * kick * 0.45 + wobble * 0.3))
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

    /// An icosahedron turning on all three axes. The widget always frames
    /// itself; it's drawn a cell larger all round and the frame left off.
    fn wireframe(&self, buf: &mut Buffer, area: Rect, t: f32) {
        static SHAPE: OnceLock<Model> = OnceLock::new();
        let model = SHAPE.get_or_init(icosahedron);
        let framed = Rect::new(
            0,
            0,
            area.width.saturating_add(2),
            area.height.saturating_add(2),
        );
        let mut picture = Buffer::empty(framed);
        WireframeWidget::new(f64::from(t * 0.5), f64::from(t * 0.8), f64::from(t * 0.2))
            .title("")
            .color(self.colors.accent.color())
            .model(model)
            .render(framed, &mut picture);
        for y in 0..area.height {
            for x in 0..area.width {
                if let (Some(from), Some(to)) = (
                    picture.cell((x.saturating_add(1), y.saturating_add(1))),
                    buf.cell_mut((area.x.saturating_add(x), area.y.saturating_add(y))),
                ) && from.symbol() != " "
                {
                    to.set_symbol(from.symbol()).set_fg(from.fg);
                }
            }
        }
    }
}

/// Code blocks in syntax colours on a raised surface, and headings in the
/// accent.
struct Hooks {
    heading: Style,
    code: Style,
    width: usize,
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

    /// `None` leaves a block in a language syntect doesn't know to the
    /// crate's own renderer.
    fn render_code_block(&self, lang: &str, code: &str) -> Option<Vec<Line<'static>>> {
        let pad = Line::styled(
            std::iter::repeat_n(' ', self.width).collect::<String>(),
            self.code,
        );
        let mut lines = vec![pad.clone()];
        lines.extend(highlight(code, lang, self.width)?.into_iter().map(|line| {
            // Each line runs the surface's width.
            let used = line.width();
            let mut line = line.patch_style(self.code);
            line.push_span(Span::styled(
                std::iter::repeat_n(' ', self.width.saturating_sub(used)).collect::<String>(),
                self.code,
            ));
            line
        }));
        lines.push(pad);
        Some(lines)
    }
}

/// `code` in `lang`'s syntax colours, indented by two, each line cut at
/// `width`.
fn highlight(code: &str, lang: &str, width: usize) -> Option<Vec<Line<'static>>> {
    let syntax = SYNTAX_SET.find_syntax_by_token(lang)?;
    let mut highlight = HighlightLines::new(syntax, THEME_SET.themes.get("one-dark")?);
    let mut out = Vec::new();
    for line in LinesWithEndings::from(code) {
        let mut spans = vec![Span::raw("  ")];
        for (style, text) in highlight.highlight_line(line, &SYNTAX_SET).ok()? {
            let f = style.foreground;
            spans.push(Span::styled(
                text.trim_end_matches('\n').to_owned(),
                Style::new().fg(Color::Rgb(f.r, f.g, f.b)),
            ));
        }
        let line = Line::from(spans);
        out.push(if line.width() > width {
            Line::from(
                line.spans
                    .into_iter()
                    .scan(0, |used: &mut usize, span| {
                        let room = width.saturating_sub(*used);
                        let text: String = span.content.chars().take(room).collect();
                        *used = used.saturating_add(text.chars().count());
                        Some(Span::styled(text, span.style))
                    })
                    .collect::<Vec<_>>(),
            )
        } else {
            line
        });
    }
    Some(out)
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
        for n in 0..3 {
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
