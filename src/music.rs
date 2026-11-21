//! The show's music: an endless tune in the style of a demoscene tracker
//! module, synthesized sample by sample while it plays. It keeps the show's
//! tempo and starts on the show's clock, so its bars line up with the bar
//! lines the cuts land on.
//!
//! Every choice, from the key to each bar's melody, comes from the seed and
//! the step it is for, so a seed plays the same music again. Sections of
//! eight bars take turns building up, dropping and breaking down; each round
//! of eight sections picks a new key.

use std::f64::consts::TAU;
use std::num::NonZero;
use std::time::Duration;

use bevy::asset::Asset;
use bevy::audio::{ChannelCount, Decodable, Sample, SampleRate, Source};
use bevy::reflect::TypePath;

use crate::num;
use crate::tempo::Tempo;

const RATE: f64 = 48_000.0;
const SAMPLE_RATE: SampleRate = NonZero::<u32>::MIN.saturating_add(47_999);
const CHANNELS: ChannelCount = NonZero::<u16>::MIN.saturating_add(1);
/// Steps (sixteenth notes) per bar, bars per section, sections per round.
const STEPS: u64 = 16;
const BARS: u64 = 8;
const SECTIONS: u64 = 8;
/// The lead's echo, in steps: a dotted eighth.
const ECHO_STEPS: f64 = 3.0;

/// The music asset: what to play and where on the show's clock it starts.
#[derive(Asset, TypePath, Clone, Copy)]
pub struct Music {
    seed: u64,
    /// Seconds per step.
    step: f64,
    /// The show's clock, in seconds, when the first sample plays.
    start: f64,
}

impl Music {
    pub fn new(seed: u64, tempo: Tempo, start: f64) -> Self {
        Self {
            seed,
            step: tempo.beat() / 4.0,
            start,
        }
    }
}

impl Decodable for Music {
    type Decoder = Player;

    fn decoder(&self) -> Player {
        Player::new(*self)
    }
}

/// Which instruments play in a section.
#[derive(Clone, Copy, Default)]
struct Part {
    kick: bool,
    snare: bool,
    /// Hi-hats on every second step, or on every step.
    hats: Hats,
    bass: bool,
    /// The arpeggio plucks each step instead of holding.
    pluck: bool,
    lead: bool,
    pad: bool,
    /// A snare roll over the last two bars, into the next section.
    roll: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Hats {
    #[default]
    None,
    Eighths,
    Sixteenths,
}

const fn part(drums: bool, hats: Hats, bass: bool, lead: bool, pad: bool, roll: bool) -> Part {
    Part {
        kick: drums,
        snare: drums,
        hats,
        bass,
        pluck: bass,
        lead,
        pad,
        roll,
    }
}

/// One round: intro, build, two drops, a break, a build and two drops.
const PARTS: [Part; 8] = [
    part(false, Hats::None, false, false, true, false),
    part(false, Hats::Eighths, true, false, true, true),
    part(true, Hats::Sixteenths, true, true, false, false),
    part(true, Hats::Sixteenths, true, true, false, false),
    part(false, Hats::None, false, true, true, false),
    part(false, Hats::Eighths, true, false, false, true),
    part(true, Hats::Sixteenths, true, true, false, false),
    part(true, Hats::Sixteenths, true, true, true, false),
];

/// Semitones of each scale degree: natural minor, dorian, harmonic minor
/// and phrygian.
const SCALES: [[i32; 7]; 4] = [
    [0, 2, 3, 5, 7, 8, 10],
    [0, 2, 3, 5, 7, 9, 10],
    [0, 2, 3, 5, 7, 8, 11],
    [0, 1, 3, 5, 7, 8, 10],
];

/// Chord roots, as scale degrees, for four bars.
const PROGRESSIONS: [[i32; 4]; 6] = [
    [0, 5, 2, 6],
    [0, 6, 5, 6],
    [0, 3, 4, 0],
    [5, 6, 0, 0],
    [0, 4, 5, 3],
    [0, 2, 5, 4],
];

/// Bass lines: scale degrees above the chord's root, or a rest.
const BASS: [[Option<i32>; 16]; 4] = {
    const R: Option<i32> = Some(0);
    const O: Option<i32> = Some(7);
    const F: Option<i32> = Some(4);
    const N: Option<i32> = None;
    [
        [R, N, O, N, R, N, O, N, R, N, O, N, R, N, O, N],
        [R, R, O, R, R, R, O, R, R, R, O, R, R, O, F, O],
        [R, N, N, R, N, N, R, N, R, N, N, R, N, N, O, N],
        [R, N, R, O, R, N, R, O, R, N, R, O, R, F, O, F],
    ]
};

/// The lead's rhythms: bit `p` starts a note on step `p`.
const RHYTHMS: [u16; 7] = [0x5549, 0x5555, 0x1449, 0x5951, 0xd5d7, 0x4141, 0x194d];

/// Extra kicks on top of four on the floor.
const KICKS: [u16; 4] = [0x0000, 0x4000, 0x0400, 0x4080];

/// A key: the bass's tonic as a MIDI note and its scale.
#[derive(Clone, Copy)]
struct Key {
    root: i32,
    scale: [i32; 7],
}

impl Key {
    /// The frequency of scale `degree` (any integer), `octaves` above the
    /// tonic.
    fn freq(self, degree: i32, octaves: i32) -> f64 {
        let index = usize::try_from(degree.rem_euclid(7)).unwrap_or(0);
        let semitone = self.scale.get(index).copied().unwrap_or(0);
        let midi = f64::from(self.root)
            + f64::from(semitone)
            + 12.0 * (f64::from(octaves) + f64::from(degree.div_euclid(7)));
        440.0 * ((midi - 69.0) / 12.0).exp2()
    }
}

/// Where a step falls in the song.
#[derive(Clone, Copy)]
struct Spot {
    /// Step within the bar, bar within the section.
    pos: u32,
    bar: u64,
    section: u64,
    part: Part,
    key: Key,
    /// The bar's chord root, as a scale degree.
    chord: i32,
}

impl Spot {
    fn new(seed: u64, step: u64) -> Self {
        let bar = step / STEPS;
        let section = bar / BARS;
        let round = section / SECTIONS;
        let part = PARTS
            .get(usize::try_from(section % SECTIONS).unwrap_or(0))
            .copied()
            .unwrap_or_default();
        let key = Key {
            root: i32::try_from(pick(hash(seed, round, 1), 8))
                .unwrap_or(0)
                .saturating_add(36),
            scale: choose(&SCALES, hash(seed, round, 2)).unwrap_or_default(),
        };
        let progression = choose(&PROGRESSIONS, hash(seed, round, 3)).unwrap_or_default();
        let chord = progression
            .get(usize::try_from(bar % 4).unwrap_or(0))
            .copied()
            .unwrap_or(0);
        Self {
            pos: u32::try_from(step % STEPS).unwrap_or(0),
            bar: bar % BARS,
            section,
            part,
            key,
            chord,
        }
    }

    fn last_bars(self) -> bool {
        self.bar >= BARS - 2
    }
}

/// A note of the lead: its scale degree, and where it starts and ends in
/// its bar.
#[derive(Clone, Copy)]
struct Tone {
    degree: i32,
    from: u32,
    to: u32,
}

/// The lead's notes for the bar at `spot`: the same few motifs come back
/// within a section, with a new one in its last bar.
fn melody(seed: u64, spot: Spot) -> [Option<Tone>; 16] {
    let motif = if spot.bar == BARS - 1 {
        4
    } else {
        spot.bar % 4
    };
    let mut state = hash(seed, spot.section, motif.wrapping_add(16));
    let mut next = |n: u64| {
        state = mix(state.wrapping_add(0x9e37_79b9_7f4a_7c15));
        pick(state, n)
    };
    let rhythm = choose(&RHYTHMS, next(u64::MAX)).unwrap_or(0x5555);
    let onsets: Vec<u32> = (0..16)
        .filter(|&p| rhythm.checked_shr(p).is_some_and(|bits| bits & 1 == 1))
        .collect();
    let mut notes = [None; 16];
    let mut degree = spot.chord;
    for (i, &from) in onsets.iter().enumerate() {
        degree = if from % 4 == 0 {
            let chord_tone = choose(&[0, 2, 4, 7], next(4)).unwrap_or(0);
            spot.chord.saturating_add(chord_tone)
        } else {
            let walk = choose(&[-2, -1, 1, 2, -1, 1], next(6)).unwrap_or(1);
            degree
                .saturating_add(walk)
                .clamp(spot.chord.saturating_sub(3), spot.chord.saturating_add(9))
        };
        let end = onsets.get(i.saturating_add(1)).copied().unwrap_or(16);
        // Now and then a note is cut short, so the line breathes.
        let to = if next(3) == 0 {
            end.min(from.saturating_add(2))
        } else {
            end
        };
        for p in from..to {
            if let Some(note) = notes.get_mut(usize::try_from(p).unwrap_or(16)) {
                *note = Some(Tone { degree, from, to });
            }
        }
    }
    notes
}

/// Plays the music: an endless stereo stream.
pub struct Player {
    music: Music,
    /// Samples played so far, and the right channel of the current frame
    /// when the left has gone out.
    played: u64,
    right: Option<f32>,
    /// The current bar's melody, by the step it starts on.
    melody: Option<(u64, [Option<Tone>; 16])>,
    lead_phase: f64,
    arp_phase: f64,
    pad_phases: [f64; 6],
    bass_low: [f64; 2],
    lead_low: f64,
    pad_low: [f64; 2],
    /// Ping-pong echo of the lead: left and right delay lines.
    echo: Vec<[f64; 2]>,
    echo_at: usize,
}

impl Player {
    fn new(music: Music) -> Self {
        let length = num::floor_u64(music.step * ECHO_STEPS * RATE).max(1);
        Self {
            music,
            played: 0,
            right: None,
            melody: None,
            lead_phase: 0.0,
            arp_phase: 0.0,
            pad_phases: [0.0; 6],
            bass_low: [0.0; 2],
            lead_low: 0.0,
            pad_low: [0.0; 2],
            echo: vec![[0.0; 2]; usize::try_from(length).unwrap_or(1)],
            echo_at: 0,
        }
    }

    /// The next stereo frame, each side from -1 to 1.
    fn frame(&mut self) -> [f64; 2] {
        let n = self.played;
        self.played = n.wrapping_add(1);
        let seed = self.music.seed;
        let len = self.music.step;
        let t = self.music.start + n as f64 / RATE;
        let step = num::floor_u64(t / len);
        let into = t - step as f64 * len;
        let spot = Spot::new(seed, step);
        let part = spot.part;
        // Seconds since the latest step at most `reach` back that `hit`
        // gives a velocity for, and that velocity.
        let since = |reach: u64, hit: &dyn Fn(u64, Spot) -> Option<f64>| {
            (0..=reach).find_map(|back| {
                let at = step.checked_sub(back)?;
                let velocity = hit(at, Spot::new(seed, at))?;
                Some((into + back as f64 * len, velocity, at))
            })
        };
        let noise = noise(n);
        let bright = 0.5 * (noise - self::noise(n.wrapping_sub(1)));
        let mut left = 0.0;
        let mut right = 0.0;
        let mut both = |v: f64, pan: f64| {
            left += v * (1.0 - pan);
            right += v * pan;
        };

        if let Some((tau, v, _)) = since(4, &|_, s| kick(seed, s)) {
            let phase = 48.0 * tau + 120.0 * (1.0 - (-28.0 * tau).exp()) / 28.0;
            both(0.9 * v * (TAU * phase).sin() * (-6.0 * tau).exp(), 0.5);
        }
        if let Some((tau, v, _)) = since(4, &|_, s| snare(s)) {
            let tone = (TAU * 190.0 * tau).sin() * (-20.0 * tau).exp();
            both(0.45 * v * (0.5 * tone + noise * (-14.0 * tau).exp()), 0.45);
        }
        if let Some((tau, v, _)) = since(2, &|_, s| hat(s)) {
            both(0.35 * v * bright * (-v * 55.0 * tau).exp(), 0.65);
        }
        if let Some((tau, _, _)) = since(24, &|_, s| crash(s)) {
            both(0.3 * bright * (-2.2 * tau).exp(), 0.35);
        }

        if part.bass
            && let Some((tau, _, at)) = since(3, &|_, s| bass(seed, s).map(|_| 1.0))
        {
            let onset = Spot::new(seed, at);
            let degree = bass(seed, onset).unwrap_or(0);
            let freq = onset.key.freq(onset.chord.saturating_add(degree), 0);
            let saw = 2.0 * (freq * tau).fract() - 1.0;
            let cutoff = 180.0 + 2_200.0 * (-12.0 * tau).exp();
            let k = 1.0 - (-TAU * cutoff / RATE).exp();
            let [a, b] = &mut self.bass_low;
            *a += (saw * (-3.5 * tau).exp() - *a) * k;
            *b += (*a - *b) * k;
            both(0.5 * *b, 0.5);
        }

        let chord = [0, 2, 4].map(|d| spot.key.freq(spot.chord.saturating_add(d), 2));
        if part.pad || part.pluck {
            let arp_step = len / 3.0;
            let tone = num::floor_u64(t / arp_step) % 3;
            let freq = chord
                .get(usize::try_from(tone).unwrap_or(0))
                .copied()
                .unwrap_or(0.0);
            self.arp_phase = (self.arp_phase + freq / RATE).fract();
            let duty = 0.25 + 0.15 * (TAU * t / (len * 64.0)).sin();
            let pulse = if self.arp_phase < duty { 1.0 } else { -1.0 };
            let level = if part.pluck {
                (-9.0 * into).exp()
            } else {
                0.6 + 0.4 * (TAU * t / (len * STEPS as f64)).sin()
            };
            both(0.06 * level * pulse, 0.75);
        }

        if part.pad {
            let bar_start = len * STEPS as f64;
            let in_bar = t % bar_start;
            let swell = (in_bar / 0.6).min(1.0) * (((bar_start - in_bar) / 0.15).min(1.0));
            let mut sum = 0.0;
            for (phase, (freq, detune)) in self.pad_phases.iter_mut().zip(
                chord
                    .iter()
                    .flat_map(|f| [(f * 0.5, 0.997), (f * 0.5, 1.003)]),
            ) {
                *phase = (*phase + freq * detune / RATE).fract();
                sum += 2.0 * *phase - 1.0;
            }
            let k = 1.0 - (-TAU * 1_400.0 / RATE).exp();
            let [a, b] = &mut self.pad_low;
            *a += (sum - *a) * k;
            *b += (*a - *b) * k;
            both(0.05 * swell * *b, 0.5);
        }

        let mut lead = 0.0;
        if part.lead {
            let bar = step / STEPS;
            if self.melody.is_none_or(|(at, _)| at != bar) {
                self.melody = Some((bar, melody(seed, spot)));
            }
            let tone = self
                .melody
                .and_then(|(_, notes)| notes.get(usize::try_from(spot.pos).unwrap_or(16)).copied())
                .flatten();
            if let Some(tone) = tone {
                let age = f64::from(spot.pos.saturating_sub(tone.from)) * len + into;
                let left_in = f64::from(tone.to.saturating_sub(spot.pos)) * len - into;
                let vibrato = 0.3 * (TAU * 5.5 * age).sin() * ((age - 0.15) / 0.2).clamp(0.0, 1.0);
                let freq = spot.key.freq(tone.degree, 3) * (vibrato / 12.0).exp2();
                self.lead_phase = (self.lead_phase + freq / RATE).fract();
                let square = if self.lead_phase < 0.5 { 1.0 } else { -1.0 };
                let envelope = (age / 0.005).min(1.0)
                    * (0.7 + 0.3 * (-10.0 * age).exp())
                    * (left_in / 0.02).clamp(0.0, 1.0);
                lead = square * envelope;
            }
        }
        let k = 1.0 - (-TAU * 3_500.0 / RATE).exp();
        self.lead_low += (lead - self.lead_low) * k;
        let dry = 0.13 * self.lead_low;
        both(dry, 0.4);
        let at = self.echo_at;
        let [echo_left, echo_right] = self.echo.get(at).copied().unwrap_or([0.0; 2]);
        if let Some(slot) = self.echo.get_mut(at) {
            *slot = [dry + 0.45 * echo_right, 0.45 * echo_left];
        }
        self.echo_at = at
            .saturating_add(1)
            .checked_rem(self.echo.len())
            .unwrap_or(0);
        left += 0.6 * echo_left;
        right += 0.6 * echo_right;

        [soft_clip(left), soft_clip(right)]
    }
}

fn soft_clip(v: f64) -> f64 {
    (1.1 * v).tanh() * 0.9
}

fn kick(seed: u64, s: Spot) -> Option<f64> {
    let extra = choose(&KICKS, hash(seed, s.section, 4)).unwrap_or(0);
    let on = s.pos.is_multiple_of(4) || extra.checked_shr(s.pos).is_some_and(|b| b & 1 == 1);
    // The last half bar before a section without drums leaves room.
    let next = PARTS
        .get(usize::try_from(s.section.wrapping_add(1) % SECTIONS).unwrap_or(0))
        .copied()
        .unwrap_or_default();
    let gap = !next.kick && s.bar == BARS - 1 && s.pos >= 8;
    (s.part.kick && on && !gap).then_some(1.0)
}

fn snare(s: Spot) -> Option<f64> {
    if s.part.snare && (s.pos == 4 || s.pos == 12) {
        return Some(1.0);
    }
    if s.part.roll && s.last_bars() {
        let dense = s.bar == BARS - 1;
        let on = dense || s.pos.is_multiple_of(2);
        let rise = (f64::from(s.pos) + if dense { 16.0 } else { 0.0 }) / 32.0;
        return on.then_some(0.3 + 0.7 * rise);
    }
    None
}

fn hat(s: Spot) -> Option<f64> {
    match s.part.hats {
        Hats::None => None,
        Hats::Eighths => (s.pos % 4 == 2).then_some(0.8),
        Hats::Sixteenths => Some(if s.pos % 4 == 2 { 0.5 } else { 0.9 }),
    }
}

/// A crash on the first step of a section with drums.
fn crash(s: Spot) -> Option<f64> {
    (s.part.kick && s.bar == 0 && s.pos == 0).then_some(1.0)
}

/// The bass note starting at `s`: scale degrees above the chord's root.
fn bass(seed: u64, s: Spot) -> Option<i32> {
    let line = choose(&BASS, hash(seed, s.section, 5))?;
    line.get(usize::try_from(s.pos).ok()?).copied().flatten()
}

impl Iterator for Player {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        if let Some(right) = self.right.take() {
            return Some(right);
        }
        let [left, right] = self.frame();
        self.right = Some(num::narrow(right));
        Some(num::narrow(left))
    }
}

impl Source for Player {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        CHANNELS
    }

    fn sample_rate(&self) -> SampleRate {
        SAMPLE_RATE
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

/// SplitMix64's finalizer.
fn mix(x: u64) -> u64 {
    let x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    let x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

fn hash(seed: u64, a: u64, b: u64) -> u64 {
    mix(seed ^ mix(a.wrapping_add(mix(b))))
}

/// A number below `n` from `hash`.
fn pick(hash: u64, n: u64) -> u64 {
    hash.checked_rem(n).unwrap_or(0)
}

fn choose<T: Copy>(items: &[T], hash: u64) -> Option<T> {
    let n = u64::try_from(items.len()).ok()?;
    items.get(usize::try_from(pick(hash, n)).ok()?).copied()
}

/// White noise from -1 to 1 for sample `n`.
fn noise(n: u64) -> f64 {
    (mix(n) >> 11) as f64 / (1_u64 << 53) as f64 * 2.0 - 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn play(seed: u64, seconds: f64) -> Vec<[f64; 2]> {
        let mut player = Music::new(seed, Tempo::new(120.0), 0.0).decoder();
        (0..num::floor_u64(seconds * RATE))
            .map(|_| player.frame())
            .collect()
    }

    #[test]
    fn a_seed_plays_the_same_music_again() {
        assert_eq!(play(7, 1.0), play(7, 1.0));
        assert_ne!(play(7, 1.0), play(8, 1.0));
    }

    #[test]
    fn every_section_is_heard_and_none_clips() {
        // A round is 8 sections of 16 s at 120 bpm.
        let frames = play(3, 128.0);
        let per_section = frames.len() / 8;
        for section in 0..8 {
            let samples: Vec<f64> = frames
                .iter()
                .skip(section * per_section)
                .take(per_section)
                .flatten()
                .copied()
                .collect();
            let peak = samples.iter().fold(0.0_f64, |peak, v| peak.max(v.abs()));
            let rms =
                (samples.iter().map(|v| v * v).sum::<f64>() / samples.len().max(1) as f64).sqrt();
            assert!(peak <= 1.0, "section {section} peaks at {peak}");
            assert!(rms > 0.02, "section {section} is nearly silent: {rms}");
        }
    }
}
