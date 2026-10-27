//! The terminal side of a frame. Each frame is drawn into a buffer; only
//! the cells that changed since the last one are sent, in as few bytes as
//! the escape codes allow, as one synchronized update in one write.
//!
//! A terminal can only take so many cells a second. Writing blocks while
//! it catches up, so a write that takes a good part of the frame's time
//! means it is falling behind, and frames would come unevenly, as each
//! write allows. The screen then backs off: it lets cells that barely
//! changed keep what they show for longer (`pressure`), and draws at a
//! steady lower rate that leaves the terminal time to keep up; once
//! writes are quick again it comes back.

use std::fmt::Write as _;
use std::io::Write;
use std::time::{Duration, Instant};

use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};

/// The share of the time between two frames a write may take.
const SHARE: f64 = 0.6;
/// Below this share of a frame's time, writes are quick.
const QUICK: f64 = 0.3;
/// The most frames' time between two drawn ones.
const SLOWEST: f64 = 4.0;
/// How far colours may drift before a cell is sent again, in steps of
/// backing off, and the frames drawn between two steps, so each shows its
/// effect first.
const MAX_PRESSURE: u32 = 3;
const SETTLE: u32 = 8;

/// One frame's cost: when it went out, its bytes, the cells the GPU
/// changed and changed back (to what they showed the frame before), and
/// how long drawing and encoding it, and writing it, took.
type Sent = (Instant, usize, usize, usize, Duration, Duration);

/// The pen: what the terminal draws the next character with.
#[derive(Clone, Copy, PartialEq)]
struct Pen {
    fg: Color,
    bg: Color,
    modifier: Modifier,
}

#[derive(bevy::prelude::Resource)]
pub struct Screen {
    /// What the terminal shows, and the frame being drawn.
    shown: Buffer,
    next: Buffer,
    /// Whether the terminal shows `shown`; not after a resize.
    valid: bool,
    out: Vec<u8>,
    /// The last second of frames.
    sent: Vec<Sent>,
    /// The frame's time, how far the screen has backed off and how many
    /// frames it has drawn since, and the time since the last drawn one.
    budget: Duration,
    pressure: u32,
    settled: u32,
    pending: Duration,
    /// How long writes take, smoothed.
    write: f64,
}

impl Screen {
    pub fn new(fps: f64) -> Self {
        Self {
            shown: Buffer::empty(Rect::ZERO),
            next: Buffer::empty(Rect::ZERO),
            valid: false,
            out: Vec::new(),
            sent: Vec::new(),
            budget: Duration::from_secs_f64(1.0 / fps),
            pressure: 0,
            settled: 0,
            pending: Duration::ZERO,
            write: 0.0,
        }
    }

    /// Whether this frame is drawn, and if so the time since the last
    /// drawn one; a frame that isn't only adds `dt` to it.
    pub fn due(&mut self, dt: Duration) -> Option<Duration> {
        self.pending = self.pending.saturating_add(dt);
        let budget = self.budget.as_secs_f64();
        let interval = (self.write / SHARE).clamp(budget, budget * SLOWEST);
        // Frames come on a fixed beat: within half of one counts.
        if self.pending.as_secs_f64() + budget * 0.5 < interval {
            return None;
        }
        Some(std::mem::take(&mut self.pending))
    }

    /// How far behind the terminal is: 0 when it keeps up, up to
    /// `MAX_PRESSURE`. The GPU lets cells change by more before sending
    /// them again as it rises.
    pub fn pressure(&self) -> u32 {
        self.pressure
    }

    /// Frames per second, kilobytes, cells changed and changed back per
    /// frame, and the average write, for the caption.
    pub fn summary(&self) -> String {
        let n = self.sent.len().max(1);
        let mean = |f: fn(&Sent) -> usize| {
            self.sent
                .iter()
                .map(f)
                .sum::<usize>()
                .checked_div(n)
                .unwrap_or(0)
        };
        let ms = |f: fn(&Sent) -> Duration| {
            self.sent.iter().map(f).sum::<Duration>().as_secs_f64() * 1000.0 / n as f64
        };
        // The longest wait between two frames: a stutter.
        let gap = self
            .sent
            .iter()
            .zip(self.sent.iter().skip(1))
            .map(|(a, b)| b.0.saturating_duration_since(a.0))
            .max()
            .unwrap_or_default();
        format!(
            "{} fps, {:.1} kB, {} cells, {} back, encode {:.2} ms, write {:.2} ms, worst gap {:.0} ms",
            self.sent.len(),
            mean(|f| f.1) as f64 / 1000.0,
            mean(|f| f.2),
            mean(|f| f.3),
            ms(|f| f.4),
            ms(|f| f.5),
            gap.as_secs_f64() * 1000.0
        )
    }

    /// Draws a frame `area` large with `draw` and sends what changed;
    /// `cells` and `flips` are what the GPU changed, for the statistics.
    pub fn frame(
        &mut self,
        area: Rect,
        cells: usize,
        flips: usize,
        draw: impl FnOnce(&mut Buffer),
    ) -> std::io::Result<()> {
        if self.next.area != area {
            self.next = Buffer::empty(area);
            self.shown = Buffer::empty(area);
            self.valid = false;
        }
        let begun = Instant::now();
        self.next.reset();
        draw(&mut self.next);
        self.encode();
        let start = Instant::now();
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(&self.out)?;
        stdout.flush()?;
        let took = start.elapsed();
        self.sent
            .retain(|(at, ..)| start.saturating_duration_since(*at) < Duration::from_secs(1));
        let encoding = start.saturating_duration_since(begun);
        self.sent
            .push((start, self.out.len(), cells, flips, encoding, took));
        std::mem::swap(&mut self.shown, &mut self.next);
        self.valid = true;
        self.pace(took);
        Ok(())
    }

    /// Lets colours drift further while writes take long, less once they
    /// are quick.
    fn pace(&mut self, took: Duration) {
        self.write = self.write * 0.8 + took.as_secs_f64() * 0.2;
        self.settled = self.settled.saturating_add(1);
        if self.settled < SETTLE {
            return;
        }
        let budget = self.budget.as_secs_f64();
        let p = self.pressure;
        if self.write > budget * SHARE && p < MAX_PRESSURE {
            self.pressure = p.saturating_add(1);
            self.settled = 0;
        } else if self.write < budget * QUICK && p > 0 {
            self.pressure = p.saturating_sub(1);
            self.settled = 0;
        }
    }

    /// The bytes that turn what the terminal shows into the next frame.
    fn encode(&mut self) {
        let out = &mut self.out;
        out.clear();
        // Synchronized update: the terminal shows none of it until the end.
        out.extend_from_slice(b"\x1b[?2026h");
        let blank;
        let updates: Vec<(u16, u16, &Cell)> = if self.valid {
            self.shown.diff(&self.next)
        } else {
            // After a resize the screen is unknown: hide the cursor, clear
            // it, and send everything that isn't blank.
            out.extend_from_slice(b"\x1b[?25l\x1b[0m\x1b[2J");
            blank = Buffer::empty(self.next.area);
            blank.diff(&self.next)
        };
        let mut pen: Option<Pen> = None;
        let mut at: Option<(u16, u16)> = None;
        let mut text = String::new();
        for (x, y, cell) in updates {
            text.clear();
            match at {
                Some(a) if a == (x, y) => {}
                Some((ax, ay)) if ay == y && x > ax => {
                    let _ = write!(text, "\x1b[{}C", x.saturating_sub(ax));
                }
                _ => {
                    let _ = write!(
                        text,
                        "\x1b[{};{}H",
                        u32::from(y).saturating_add(1),
                        u32::from(x).saturating_add(1)
                    );
                }
            }
            let symbol = cell.symbol();
            let want = Pen {
                fg: cell.fg,
                bg: cell.bg,
                modifier: cell.modifier,
            };
            pen = Some(set_pen(&mut text, pen, want, symbol));
            text.push_str(symbol);
            out.extend_from_slice(text.as_bytes());
            let width = u16::try_from(ratatui::text::Span::raw(symbol).width().max(1)).unwrap_or(1);
            at = Some((x.saturating_add(width), y));
        }
        out.extend_from_slice(b"\x1b[0m\x1b[?2026l");
    }
}

/// Escape codes that change `pen` to draw `want`, leaving out what the
/// symbol doesn't show: a space's foreground, a full block's background.
fn set_pen(text: &mut String, pen: Option<Pen>, want: Pen, symbol: &str) -> Pen {
    let start = text.len();
    // Each parameter after the first is separated by a semicolon.
    let next_param = |text: &mut String| {
        if text.len() == start {
            text.push_str("\x1b[");
        } else {
            text.push(';');
        }
    };
    let mut next = pen.unwrap_or(Pen {
        fg: Color::Reset,
        bg: Color::Reset,
        modifier: Modifier::empty(),
    });
    if pen.is_none() || next.modifier != want.modifier {
        // Attributes only come off by resetting them all.
        next_param(text);
        text.push('0');
        for (m, code) in [
            (Modifier::BOLD, ";1"),
            (Modifier::DIM, ";2"),
            (Modifier::ITALIC, ";3"),
            (Modifier::UNDERLINED, ";4"),
            (Modifier::SLOW_BLINK, ";5"),
            (Modifier::RAPID_BLINK, ";6"),
            (Modifier::REVERSED, ";7"),
            (Modifier::HIDDEN, ";8"),
            (Modifier::CROSSED_OUT, ";9"),
        ] {
            if want.modifier.contains(m) {
                text.push_str(code);
            }
        }
        next = Pen {
            fg: Color::Reset,
            bg: Color::Reset,
            modifier: want.modifier,
        };
    }
    let plain = want.modifier.is_empty();
    if !(plain && symbol == " ") && next.fg != want.fg {
        next_param(text);
        sgr(text, want.fg, false);
        next.fg = want.fg;
    }
    if !(plain && symbol == "█") && next.bg != want.bg {
        next_param(text);
        sgr(text, want.bg, true);
        next.bg = want.bg;
    }
    if text.len() != start {
        text.push('m');
    }
    next
}

/// Writes the SGR parameters that set the foreground (or `background`) to
/// `color`.
fn sgr(text: &mut String, color: Color, background: bool) {
    let base = if background { 40_u8 } else { 30 };
    let named = |n: u8| base.saturating_add(n);
    let bright = |n: u8| base.saturating_add(60).saturating_add(n);
    let code = match color {
        Color::Reset => named(9),
        Color::Black => named(0),
        Color::Red => named(1),
        Color::Green => named(2),
        Color::Yellow => named(3),
        Color::Blue => named(4),
        Color::Magenta => named(5),
        Color::Cyan => named(6),
        Color::Gray => named(7),
        Color::DarkGray => bright(0),
        Color::LightRed => bright(1),
        Color::LightGreen => bright(2),
        Color::LightYellow => bright(3),
        Color::LightBlue => bright(4),
        Color::LightMagenta => bright(5),
        Color::LightCyan => bright(6),
        Color::White => bright(7),
        Color::Indexed(i) => {
            let _ = write!(text, "{};5;{i}", base.saturating_add(8));
            return;
        }
        Color::Rgb(r, g, b) => {
            let _ = write!(text, "{};2;{r};{g};{b}", base.saturating_add(8));
            return;
        }
    };
    let _ = write!(text, "{code}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paint(buf: &mut Buffer) {
        for x in 0..4_u16 {
            if let Some(c) = buf.cell_mut((x, 0)) {
                c.set_char('▀')
                    .set_fg(Color::Rgb(10, 20, 30))
                    .set_bg(Color::Rgb(1, 2, 3));
            }
        }
    }

    #[test]
    fn only_what_changed_is_sent() {
        let area = Rect::new(0, 0, 4, 2);
        let mut screen = Screen::new(60.0);
        screen.next = Buffer::empty(area);
        screen.shown = Buffer::empty(area);
        paint(&mut screen.next);
        screen.encode();
        let first = String::from_utf8_lossy(&screen.out).into_owned();
        // One pen for the whole run, set once.
        assert!(
            first.contains("\x1b[0;38;2;10;20;30;48;2;1;2;3m▀▀▀▀"),
            "{first:?}"
        );
        std::mem::swap(&mut screen.shown, &mut screen.next);
        screen.valid = true;
        screen.next.reset();
        paint(&mut screen.next);
        screen.encode();
        assert_eq!(screen.out, b"\x1b[?2026h\x1b[0m\x1b[?2026l");
    }

    #[test]
    fn a_space_keeps_the_foreground() {
        let mut text = String::new();
        let pen = Pen {
            fg: Color::Rgb(1, 1, 1),
            bg: Color::Rgb(2, 2, 2),
            modifier: Modifier::empty(),
        };
        let want = Pen {
            fg: Color::Rgb(9, 9, 9),
            ..pen
        };
        let after = set_pen(&mut text, Some(pen), want, " ");
        assert!(text.is_empty());
        assert!(after == pen);
    }

    #[test]
    fn slow_writes_slow_the_frames_until_they_are_quick() {
        let mut screen = Screen::new(60.0);
        let frame = Duration::from_micros(16_667);
        let drawn = |screen: &mut Screen| (0..60).filter(|_| screen.due(frame).is_some()).count();
        assert_eq!(drawn(&mut screen), 60);
        // Writes of 20 ms: a frame every 33 ms or so, and colours drift.
        for _ in 0..100 {
            screen.pace(Duration::from_millis(20));
        }
        assert_eq!(screen.pressure(), MAX_PRESSURE);
        assert!((25..=35).contains(&drawn(&mut screen)));
        // Never slower than a quarter of the frames.
        for _ in 0..100 {
            screen.pace(Duration::from_millis(200));
        }
        assert_eq!(drawn(&mut screen), 15);
        for _ in 0..100 {
            screen.pace(Duration::from_micros(200));
        }
        assert_eq!(screen.pressure(), 0);
        assert_eq!(drawn(&mut screen), 60);
    }
}
