//! One beat for the whole show: cuts land on the first beat of a bar,
//! accents on a later bar line, transitions last whole beats, and the
//! motions while holding move in time with it.

use crate::num;

const BEATS_PER_BAR: f64 = 4.0;
/// The beat the shaders see wraps after this many, a whole number of bars
/// for every motion's period, so f32 keeps its precision.
const WRAP: f64 = 64.0;

#[derive(Debug, Clone, Copy)]
pub struct Tempo {
    /// Seconds per beat.
    beat: f64,
}

impl Tempo {
    pub fn new(bpm: f64) -> Self {
        Self {
            beat: 60.0 / bpm.max(1.0),
        }
    }

    pub fn bar(self) -> f64 {
        self.beat * BEATS_PER_BAR
    }

    /// `seconds` rounded to whole bars, at least one.
    pub fn bars(self, seconds: f64) -> f64 {
        (seconds / self.bar()).round().max(1.0) * self.bar()
    }

    /// `seconds` rounded to whole beats, at least one.
    pub fn beats(self, seconds: f64) -> f64 {
        (seconds / self.beat).round().max(1.0) * self.beat
    }

    /// The first bar line at or after `t` seconds (a frame late still
    /// counts as on it).
    pub fn next_bar(self, t: f64) -> f64 {
        ((t - 0.02) / self.bar()).ceil() * self.bar()
    }

    /// Beats since `t` = 0, wrapped, for the shaders.
    pub fn phase(self, t: f64) -> f32 {
        num::narrow((t / self.beat).rem_euclid(WRAP))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holds_round_to_whole_bars() {
        let tempo = Tempo::new(120.0);
        assert!((tempo.bar() - 2.0).abs() < 1e-9);
        assert!((tempo.bars(7.0) - 8.0).abs() < 1e-9);
        assert!((tempo.bars(0.5) - 2.0).abs() < 1e-9);
        assert!((tempo.beats(1.8) - 2.0).abs() < 1e-9);
        assert!((tempo.next_bar(3.1) - 4.0).abs() < 1e-9);
        assert!((tempo.next_bar(4.01) - 4.0).abs() < 1e-9);
    }
}
