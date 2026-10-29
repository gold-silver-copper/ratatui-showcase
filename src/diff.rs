//! Ratatui's own source scrolling past as a diff, one of the showcase's
//! libraries: every change to its source from 0.28.1 to 0.29.0, in
//! ratatui-diff's widget with Rust syntax colours, side by side when
//! there's room.

use std::sync::OnceLock;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::StatefulWidget;
use ratatui_diff::{
    Diff, DiffDocument, DiffState, DiffTheme, FileSyntax, SyntaxHighlighter, SyntaxSource,
    SyntaxStyles, ViewMode,
};

use crate::num;

/// `git diff -M v0.28.1 v0.29.0 -- 'src/*.rs'` in the ratatui repository.
const PATCH: &str = include_str!("../assets/ratatui-0.28-0.29.diff");

/// Rows scrolled a second, once the diff has come in.
const SPEED: f32 = 12.0;
/// The narrowest area that shows old and new side by side.
const SPLIT: u16 = 110;

struct Prepared {
    document: DiffDocument,
    styles: Option<SyntaxStyles>,
}

static PREPARED: OnceLock<Option<Prepared>> = OnceLock::new();

/// Parses and colours the patch ahead of time, on a thread of its own, so
/// no frame waits for it.
pub fn warm() {
    std::thread::spawn(prepare);
}

/// The patch, parsed and coloured on first use.
fn prepare() -> Option<&'static Prepared> {
    PREPARED
        .get_or_init(|| {
            let document = DiffDocument::parse(PATCH).ok()?;
            let files: Vec<FileSyntax> = (0..document.files().len())
                .map(|file| FileSyntax {
                    file,
                    language: "rs",
                    source: SyntaxSource::Retained,
                })
                .collect();
            let styles = SyntaxHighlighter::bundled("Coldark-Dark")
                .and_then(|h| h.contrast_with(theme()))
                .and_then(|h| h.prepare(&document, &files))
                .ok();
            Some(Prepared { document, styles })
        })
        .as_ref()
}

fn theme() -> DiffTheme {
    DiffTheme::aardvark_ink()
}

pub struct DiffView {
    state: DiffState,
    /// The file it opens at, a different one each time.
    file: usize,
    started: f64,
    /// Rows still to scroll, short of a whole one.
    pending: f32,
}

impl DiffView {
    /// The `n`th time the diff plays, from `now`.
    pub fn new(n: usize, now: f64) -> Self {
        Self {
            state: DiffState::new(),
            file: n.saturating_mul(7),
            started: now,
            pending: 0.0,
        }
    }

    /// Draws the diff in `area` at `now`, scrolled on by `dt` seconds'
    /// worth.
    pub fn draw(&mut self, buf: &mut Buffer, area: Rect, now: f64, dt: f32) {
        let Some(prepared) = prepare() else { return };
        let mode = if area.width >= SPLIT {
            ViewMode::Split
        } else {
            ViewMode::Unified
        };
        let plain = || {
            Diff::new(&prepared.document)
                .theme(theme())
                .mode(mode)
                .sticky_file_headers(true)
                .show_stats(true)
        };
        let diff = prepared
            .styles
            .as_ref()
            .and_then(|styles| plain().syntax_styles(styles).ok())
            .unwrap_or_else(plain);
        // Navigation needs a first render to know the rows.
        if self.file > 0 {
            diff.render(area, buf, &mut self.state);
            let files = prepared.document.files().len().max(1);
            for _ in 0..self.file.checked_rem(files).unwrap_or(0) {
                self.state.next_file();
            }
            self.file = 0;
        }
        if num::narrow(now - self.started) > 1.0 {
            self.pending += dt * SPEED;
            let whole = num::floor_u32(self.pending);
            self.pending -= num::float(usize::try_from(whole).unwrap_or(0));
            self.state.scroll_lines(isize::try_from(whole).unwrap_or(0));
        }
        diff.render(area, buf, &mut self.state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_patch_parses_and_colours() {
        let prepared = prepare();
        assert!(prepared.is_some_and(|p| p.document.files().len() > 60 && p.styles.is_some()));
    }
}
