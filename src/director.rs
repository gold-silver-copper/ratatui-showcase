//! What plays next. Scenes come in short runs, each built from a pattern:
//! building up to a peak and calming down, one background under several
//! looks, a tour of backgrounds, and so on. A run starts from scratch; each
//! cut after that changes one or two layers. The hand-tuned presets are the
//! peaks; everything else is random within the pattern.
//!
//! Every choice of every layer has a busyness: how much it draws the eye.
//! A scene's busyness, with a penalty for pairs known to clash, stays under
//! the step's budget, and at most one layer leads, so the others support
//! it. Among the choices that fit, the ones not seen for longest are the
//! likeliest, so over hours every choice still comes up.
//!
//! All of it comes from one seeded generator: the same seed on the same
//! terminal plays the same scenes.

use std::collections::HashMap;

use fastrand::Rng;

use crate::art::Style;
use crate::backdrop::{BACKDROPS, Backdrop};
use crate::fonts::{Face, SystemFonts};
use crate::stage::{self, Act, IDLES, Idle, Logo, Look, Mode, Post, Subject, Word};
use crate::theme::{Palette, SHADERS, Shader, THEMES};

/// The most busyness any scene outside the presets may have.
pub const MAX_BUDGET: u32 = 7;

/// How busy a choice is: 0 plain, 1 a touch, 2 lively, 3 enough to lead
/// the scene.
pub trait Layer: Copy + PartialEq + 'static {
    /// Every choice once.
    const VALUES: &'static [Self];
    const SLOT: Slot;
    fn busyness(self) -> u32;
    /// How often it comes up compared with the others.
    fn share(self) -> u32 {
        1
    }
    fn of(act: &Act) -> Self;
    fn set(self, act: &mut Act);
}

/// The layers of a scene the director chooses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Slot {
    Palette,
    Shader,
    Backdrop,
    Look,
    Style,
    Mode,
    Post,
    Idle,
}

const SLOTS: &[Slot] = &[
    Slot::Palette,
    Slot::Shader,
    Slot::Backdrop,
    Slot::Look,
    Slot::Style,
    Slot::Mode,
    Slot::Post,
    Slot::Idle,
];

impl Layer for Shader {
    const VALUES: &'static [Self] = SHADERS;
    const SLOT: Slot = Slot::Shader;
    fn busyness(self) -> u32 {
        match self {
            Shader::Flow | Shader::Sheen | Shader::Gold => 0,
            Shader::Pulse | Shader::Scanline | Shader::Chrome | Shader::Holo | Shader::Marble => 1,
            Shader::Plasma
            | Shader::Rainbow
            | Shader::Fire
            | Shader::Stripes
            | Shader::Lava
            | Shader::Glitter => 2,
        }
    }
    fn of(act: &Act) -> Self {
        act.shader
    }
    fn set(self, act: &mut Act) {
        act.shader = self;
    }
}

impl Layer for Backdrop {
    const VALUES: &'static [Self] = BACKDROPS;
    const SLOT: Slot = Slot::Backdrop;
    fn busyness(self) -> u32 {
        match self {
            Backdrop::Vignette => 0,
            Backdrop::Stars | Backdrop::Aurora | Backdrop::Rays => 1,
            Backdrop::Grid
            | Backdrop::Warp
            | Backdrop::Terrain
            | Backdrop::Metaballs
            | Backdrop::Collage
            | Backdrop::Flame
            | Backdrop::Clifford
            | Backdrop::DeJong
            | Backdrop::Lorenz
            | Backdrop::ParticleLife => 2,
            Backdrop::Julia
            | Backdrop::Mandelbrot
            | Backdrop::Ship
            | Backdrop::Newton
            | Backdrop::Kaleidoscope
            | Backdrop::Tunnel
            | Backdrop::Mandelbulb
            | Backdrop::Menger
            | Backdrop::Voronoi
            | Backdrop::Marble
            | Backdrop::Apollonian
            | Backdrop::Kleinian
            | Backdrop::Hyperbolic
            | Backdrop::Complex
            | Backdrop::Mandelbox
            | Backdrop::Kifs => 3,
        }
    }
    fn of(act: &Act) -> Self {
        act.backdrop
    }
    fn set(self, act: &mut Act) {
        act.backdrop = self;
    }
}

impl Layer for Look {
    const VALUES: &'static [Self] = Look::ALL;
    const SLOT: Slot = Slot::Look;
    fn busyness(self) -> u32 {
        match self {
            Look::Smooth => 0,
            Look::Pixels | Look::Braille | Look::Led => 1,
            Look::Ascii | Look::Shapes | Look::Halftone => 2,
            Look::Code | Look::Contour | Look::Text => 3,
        }
    }
    fn share(self) -> u32 {
        match self {
            Look::Smooth => 6,
            Look::Braille => 2,
            Look::Pixels
            | Look::Ascii
            | Look::Led
            | Look::Shapes
            | Look::Code
            | Look::Contour
            | Look::Halftone
            | Look::Text => 1,
        }
    }
    fn of(act: &Act) -> Self {
        act.look
    }
    fn set(self, act: &mut Act) {
        act.look = self;
    }
}

impl Layer for Style {
    const VALUES: &'static [Self] = Style::ALL;
    const SLOT: Slot = Slot::Style;
    fn busyness(self) -> u32 {
        match self {
            Style::Solid | Style::Bevel | Style::Shadow => 0,
            Style::Outline | Style::Neon | Style::Extrude | Style::Reflect => 1,
        }
    }
    fn of(act: &Act) -> Self {
        act.style
    }
    fn set(self, act: &mut Act) {
        act.style = self;
    }
}

impl Layer for Mode {
    const VALUES: &'static [Self] = Mode::ALL;
    const SLOT: Slot = Slot::Mode;
    fn busyness(self) -> u32 {
        match self {
            Mode::Flat => 0,
            Mode::Ripple => 1,
            Mode::Turntable | Mode::Swarm | Mode::Boids | Mode::Sand | Mode::Crystal => 2,
            Mode::Fluid
            | Mode::Reaction
            | Mode::Life
            | Mode::Fire
            | Mode::Slime
            | Mode::Ink
            | Mode::Lenia => 3,
        }
    }
    fn share(self) -> u32 {
        match self {
            Mode::Flat => 6,
            Mode::Turntable => 2,
            Mode::Fluid
            | Mode::Reaction
            | Mode::Life
            | Mode::Fire
            | Mode::Ripple
            | Mode::Swarm
            | Mode::Slime
            | Mode::Boids
            | Mode::Sand
            | Mode::Crystal
            | Mode::Ink
            | Mode::Lenia => 1,
        }
    }
    fn of(act: &Act) -> Self {
        act.mode
    }
    fn set(self, act: &mut Act) {
        act.mode = self;
    }
}

impl Layer for Post {
    const VALUES: &'static [Self] = Post::ALL;
    const SLOT: Slot = Slot::Post;
    fn busyness(self) -> u32 {
        match self {
            Post::Clean | Post::Bloom => 0,
            Post::Trails | Post::Chroma | Post::Crt | Post::Grain | Post::Rays => 1,
            Post::Echo | Post::Paint | Post::Silk => 2,
            Post::Droste => 3,
        }
    }
    fn of(act: &Act) -> Self {
        act.post
    }
    fn set(self, act: &mut Act) {
        act.post = self;
    }
}

impl Layer for Idle {
    const VALUES: &'static [Self] = IDLES;
    const SLOT: Slot = Slot::Idle;
    fn busyness(self) -> u32 {
        match self {
            Idle::Still | Idle::Float | Idle::Breathe | Idle::Sway => 0,
            Idle::Wave | Idle::Bounce | Idle::Arrive => 1,
            Idle::Marquee => 2,
        }
    }
    fn of(act: &Act) -> Self {
        act.idle
    }
    fn set(self, act: &mut Act) {
        act.idle = self;
    }
}

/// Looks that turn the picture into characters, losing fine detail.
fn coarse(look: Look) -> bool {
    matches!(
        look,
        Look::Ascii | Look::Shapes | Look::Code | Look::Contour | Look::Halftone | Look::Text
    )
}

/// Extra busyness for pairs that fight each other. Only busy choices
/// clash, so the plain ones always fit.
fn clash(act: &Act) -> u32 {
    let mut penalty = 0_u32;
    let mut add = |when: bool, by: u32| {
        if when {
            penalty = penalty.saturating_add(by);
        }
    };
    // Characters resample the picture: grain, brush strokes and colour
    // fringes turn into noise, and thin strokes vanish.
    add(
        coarse(act.look)
            && matches!(
                act.post,
                Post::Grain | Post::Paint | Post::Silk | Post::Chroma | Post::Droste
            ),
        2,
    );
    add(
        coarse(act.look) && matches!(act.style, Style::Outline | Style::Neon),
        2,
    );
    // A pattern painted over a pattern.
    add(
        act.shader.busyness() >= 2 && act.backdrop.busyness() >= 3,
        1,
    );
    // The spiral can't follow art that moves across the screen.
    add(
        act.post == Post::Droste && matches!(act.idle, Idle::Marquee | Idle::Arrive),
        2,
    );
    penalty
}

/// Each layer's busyness, in the order of `SLOTS`.
fn busyness(act: &Act) -> [u32; 8] {
    [
        0,
        act.shader.busyness(),
        act.backdrop.busyness(),
        act.look.busyness(),
        act.style.busyness(),
        act.mode.busyness(),
        act.post.busyness(),
        act.idle.busyness(),
    ]
}

/// How busy the scene is, clashes included.
pub fn cost(act: &Act) -> u32 {
    busyness(act)
        .iter()
        .fold(clash(act), |sum, b| sum.saturating_add(*b))
}

/// Whether the scene fits `budget` with at most one layer leading, and its
/// motion suits its mode.
pub fn fits(act: &Act, budget: u32) -> bool {
    let leads = busyness(act).iter().filter(|b| **b >= 3).count();
    leads <= 1 && cost(act) <= budget && act.idle.suits(act.mode)
}

/// One scene of a pattern.
#[derive(Debug, Clone, Copy)]
enum Step {
    /// Everything new under the budget, the lead (if any) first and busy.
    Reset(u32, Option<Slot>),
    /// So many layers change, from the list if it names any, under the
    /// budget, the lead (if any) busy.
    Cut(usize, u32, &'static [Slot], Option<Slot>),
    /// A hand-tuned preset.
    Peak,
}

use Step::{Cut, Peak, Reset};

const ANY: &[Slot] = &[];
const BACKDROP: Option<Slot> = Some(Slot::Backdrop);
/// The surface over a background that stays.
const DRESS: &[Slot] = &[Slot::Palette, Slot::Shader, Slot::Style, Slot::Idle];

/// Short runs of scenes. Each starts afresh or at a peak.
const PATTERNS: &[&[Step]] = &[
    // Build up, peak, calm down.
    &[
        Reset(2, None),
        Cut(1, 4, ANY, None),
        Cut(2, 6, ANY, None),
        Peak,
        Cut(2, 3, ANY, None),
    ],
    // A slow burn.
    &[
        Reset(1, None),
        Cut(1, 3, ANY, None),
        Cut(1, 5, ANY, None),
        Cut(2, MAX_BUDGET, ANY, None),
    ],
    // One background, three looks.
    &[
        Reset(5, BACKDROP),
        Cut(1, 5, &[Slot::Look], None),
        Cut(1, 5, &[Slot::Look], None),
    ],
    // One background in several moods.
    &[
        Reset(5, BACKDROP),
        Cut(2, 5, DRESS, None),
        Cut(2, 5, DRESS, None),
        Cut(2, 5, DRESS, None),
    ],
    // A tour of backgrounds under the same art.
    &[
        Reset(4, BACKDROP),
        Cut(1, 4, &[Slot::Backdrop], BACKDROP),
        Cut(1, 4, &[Slot::Backdrop], BACKDROP),
        Cut(2, 4, &[Slot::Backdrop, Slot::Palette], None),
    ],
    // Living letters: the art itself leads.
    &[
        Reset(5, Some(Slot::Mode)),
        Cut(1, 5, &[Slot::Backdrop, Slot::Palette], None),
        Cut(1, 5, &[Slot::Mode], Some(Slot::Mode)),
        Cut(2, 5, ANY, None),
    ],
    // A peak and its echoes.
    &[
        Peak,
        Cut(1, MAX_BUDGET, ANY, None),
        Cut(1, MAX_BUDGET, ANY, None),
        Cut(2, 4, ANY, None),
    ],
];

/// A combination that is known to look good, for any subject.
struct Preset {
    theme: Option<&'static str>,
    shader: Shader,
    backdrop: Backdrop,
    look: Look,
    style: Style,
    mode: Mode,
    post: Post,
    idle: Idle,
}

const fn preset(
    theme: Option<&'static str>,
    shader: Shader,
    backdrop: Backdrop,
    (look, style, mode): (Look, Style, Mode),
    post: Post,
    idle: Idle,
) -> Preset {
    Preset {
        theme,
        shader,
        backdrop,
        look,
        style,
        mode,
        post,
        idle,
    }
}

const PRESETS: &[Preset] = &[
    preset(
        Some("Synthwave"),
        Shader::Chrome,
        Backdrop::Grid,
        (Look::Smooth, Style::Neon, Mode::Flat),
        Post::Crt,
        Idle::Float,
    ),
    preset(
        Some("Ember"),
        Shader::Gold,
        Backdrop::Mandelbulb,
        (Look::Smooth, Style::Solid, Mode::Turntable),
        Post::Rays,
        Idle::Still,
    ),
    preset(
        None,
        Shader::Holo,
        Backdrop::Warp,
        (Look::Smooth, Style::Bevel, Mode::Flat),
        Post::Bloom,
        Idle::Breathe,
    ),
    preset(
        Some("Phosphor"),
        Shader::Scanline,
        Backdrop::Stars,
        (Look::Code, Style::Solid, Mode::Flat),
        Post::Crt,
        Idle::Still,
    ),
    preset(
        None,
        Shader::Lava,
        Backdrop::Flame,
        (Look::Smooth, Style::Shadow, Mode::Fire),
        Post::Bloom,
        Idle::Still,
    ),
    preset(
        None,
        Shader::Plasma,
        Backdrop::Kaleidoscope,
        (Look::Smooth, Style::Solid, Mode::Fluid),
        Post::Trails,
        Idle::Still,
    ),
    preset(
        Some("Glacier"),
        Shader::Marble,
        Backdrop::Aurora,
        (Look::Smooth, Style::Reflect, Mode::Flat),
        Post::Rays,
        Idle::Sway,
    ),
    preset(
        None,
        Shader::Rainbow,
        Backdrop::Lorenz,
        (Look::Braille, Style::Outline, Mode::Flat),
        Post::Echo,
        Idle::Wave,
    ),
    preset(
        Some("Toxic"),
        Shader::Pulse,
        Backdrop::Julia,
        (Look::Smooth, Style::Solid, Mode::Reaction),
        Post::Bloom,
        Idle::Still,
    ),
    preset(
        None,
        Shader::Glitter,
        Backdrop::Menger,
        (Look::Smooth, Style::Extrude, Mode::Flat),
        Post::Chroma,
        Idle::Bounce,
    ),
    preset(
        None,
        Shader::Stripes,
        Backdrop::Tunnel,
        (Look::Led, Style::Solid, Mode::Flat),
        Post::Grain,
        Idle::Wave,
    ),
    preset(
        None,
        Shader::Sheen,
        Backdrop::Mandelbrot,
        (Look::Smooth, Style::Neon, Mode::Swarm),
        Post::Rays,
        Idle::Still,
    ),
    preset(
        Some("Prism"),
        Shader::Sheen,
        Backdrop::Collage,
        (Look::Smooth, Style::Bevel, Mode::Flat),
        Post::Bloom,
        Idle::Still,
    ),
];

/// Scenes since a choice last played, for one never played.
const UNSEEN: u64 = 64;
/// Scenes a choice sits out after it played, when anything else fits.
const REST: u64 = 2;

pub struct Director {
    rng: Rng,
    seed: u64,
    /// Scenes chosen so far.
    scene: u64,
    /// When each choice of each layer last played, by its place in the
    /// layer's list; the presets under `None`.
    seen: HashMap<(Option<Slot>, usize), u64>,
    /// Which pattern plays, and its next step.
    pattern: usize,
    step: usize,
    /// The layer the run started around, which its cuts keep.
    theme: Option<Slot>,
}

impl Director {
    pub fn new(seed: u64) -> Self {
        Self {
            rng: Rng::with_seed(seed),
            seed,
            scene: 0,
            seen: HashMap::new(),
            pattern: usize::MAX,
            step: 0,
            theme: None,
        }
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// The generator, for other choices that should replay with the seed.
    pub fn rng(&mut self) -> &mut Rng {
        &mut self.rng
    }

    /// The scene after `current`.
    pub fn next(&mut self, current: &Act, fonts: &SystemFonts) -> Act {
        let step = self.step();
        let mut act = current.clone();
        act.subject = subject(&current.subject, fonts, &mut self.rng);
        match step {
            Reset(budget, lead) => {
                self.theme = lead;
                self.reset(&mut act, budget, lead);
            }
            Cut(changes, budget, only, lead) => self.cut(&mut act, changes, budget, only, lead),
            Peak => {
                self.theme = None;
                self.peak(&mut act);
            }
        }
        if act.backdrop != current.backdrop || act.mode != current.mode {
            act.variant = self.rng.u32(..);
        }
        self.scene = self.scene.saturating_add(1);
        self.played(&act);
        act
    }

    /// The next step of the run, starting a new run when one ends.
    fn step(&mut self) -> Step {
        if let Some(step) = PATTERNS
            .get(self.pattern)
            .and_then(|steps| steps.get(self.step))
        {
            self.step = self.step.saturating_add(1);
            return *step;
        }
        let last = self.pattern;
        let others: Vec<usize> = (0..PATTERNS.len()).filter(|&p| p != last).collect();
        self.pattern = others
            .get(self.rng.usize(..others.len().max(1)))
            .copied()
            .unwrap_or(0);
        self.step = 1;
        PATTERNS
            .get(self.pattern)
            .and_then(|steps| steps.first())
            .copied()
            .unwrap_or(Reset(MAX_BUDGET, None))
    }

    fn played(&mut self, act: &Act) {
        self.mark::<Shader>(act);
        self.mark::<Backdrop>(act);
        self.mark::<Look>(act);
        self.mark::<Style>(act);
        self.mark::<Mode>(act);
        self.mark::<Post>(act);
        self.mark::<Idle>(act);
    }

    fn mark<T: Layer>(&mut self, act: &Act) {
        if let Some(i) = index(T::of(act)) {
            self.seen.insert((Some(T::SLOT), i), self.scene);
        }
    }

    /// Scenes since `key` last played.
    fn age(&self, key: (Option<Slot>, usize)) -> u64 {
        self.seen
            .get(&key)
            .map_or(self.scene.saturating_add(UNSEEN), |&at| {
                self.scene.saturating_sub(at)
            })
    }

    /// An index into `weights`, picked in proportion to them.
    fn weighted(&mut self, weights: &[u64]) -> Option<usize> {
        let total: u64 = weights.iter().fold(0, |s, w| s.saturating_add(*w));
        if total == 0 {
            return None;
        }
        let mut r = self.rng.u64(..total);
        weights.iter().position(|&w| {
            if r < w {
                true
            } else {
                r = r.saturating_sub(w);
                false
            }
        })
    }

    /// Sets layer `T` of `act` to a choice that keeps it within `budget`,
    /// a new one if `fresh`. With `lead`, a busy one if any fits. Returns
    /// whether anything fit.
    fn choose<T: Layer>(&mut self, act: &mut Act, budget: u32, lead: bool, fresh: bool) -> bool {
        let not = fresh.then(|| T::of(act));
        let fitting: Vec<T> = T::VALUES
            .iter()
            .copied()
            .filter(|&v| Some(v) != not)
            .filter(|&v| {
                let mut trial = act.clone();
                v.set(&mut trial);
                fits(&trial, budget)
            })
            .collect();
        let busy: Vec<T> = fitting
            .iter()
            .copied()
            .filter(|v| v.busyness() >= 2)
            .collect();
        let pool = if lead && !busy.is_empty() {
            busy
        } else {
            fitting
        };
        let ages: Vec<u64> = pool
            .iter()
            .map(|&v| self.age((Some(T::SLOT), index(v).unwrap_or(0))))
            .collect();
        let rested = |a: u64| a.saturating_sub(REST);
        let mut weights: Vec<u64> = pool
            .iter()
            .zip(&ages)
            .map(|(v, &a)| u64::from(v.share()).saturating_mul(rested(a)))
            .collect();
        if weights.iter().all(|&w| w == 0) {
            weights = pool
                .iter()
                .zip(&ages)
                .map(|(v, &a)| u64::from(v.share()).saturating_mul(a.max(1)))
                .collect();
        }
        match self.weighted(&weights).and_then(|i| pool.get(i)) {
            Some(&v) => {
                v.set(act);
                true
            }
            None => false,
        }
    }

    /// Picks `slot` of `act` again, keeping it within `budget`: a new
    /// choice if `fresh`.
    fn change(&mut self, act: &mut Act, slot: Slot, budget: u32, lead: bool, fresh: bool) -> bool {
        match slot {
            Slot::Palette => {
                act.palette = self.palette(&act.palette.name);
                true
            }
            Slot::Shader => self.choose::<Shader>(act, budget, lead, fresh),
            Slot::Backdrop => self.choose::<Backdrop>(act, budget, lead, fresh),
            Slot::Look => self.choose::<Look>(act, budget, lead, fresh),
            Slot::Style => self.choose::<Style>(act, budget, lead, fresh),
            Slot::Mode => self.choose::<Mode>(act, budget, lead, fresh),
            Slot::Post => self.choose::<Post>(act, budget, lead, fresh),
            Slot::Idle => self.choose::<Idle>(act, budget, lead, fresh),
        }
    }

    /// A palette other than the one called `not`.
    fn palette(&mut self, not: &str) -> Palette {
        (0..8)
            .map(|_| Palette::random(&mut self.rng))
            .find(|p| p.name != not)
            .unwrap_or_else(|| Palette::generated(&mut self.rng))
    }

    fn shuffled(&mut self, slots: &[Slot]) -> Vec<Slot> {
        let mut slots = slots.to_vec();
        self.rng.shuffle(&mut slots);
        slots
    }

    /// Everything new: the plain scene, then each layer in a random order,
    /// the lead first.
    fn reset(&mut self, act: &mut Act, budget: u32, lead: Option<Slot>) {
        act.shader = Shader::Flow;
        act.backdrop = Backdrop::Vignette;
        act.look = Look::Smooth;
        act.style = Style::Solid;
        act.mode = Mode::Flat;
        act.post = Post::Clean;
        act.idle = Idle::Still;
        let mut order = self.shuffled(SLOTS);
        if let Some(lead) = lead {
            order.retain(|&s| s != lead);
            order = std::iter::once(lead).chain(order).collect();
        }
        for slot in order {
            // The plain choice may stay.
            let _ = self.change(act, slot, budget, Some(slot) == lead, false);
        }
    }

    /// `changes` layers change; if the scene is then over `budget`, its
    /// busiest other layers change too until it fits.
    fn cut(
        &mut self,
        act: &mut Act,
        changes: usize,
        budget: u32,
        only: &'static [Slot],
        lead: Option<Slot>,
    ) {
        let from = if only.is_empty() { SLOTS } else { only };
        let mut changed = Vec::new();
        // Coming down from a peak, the busiest layers go first.
        let mut order = self.shuffled(from);
        if !fits(act, budget) {
            order.sort_by_key(|&s| std::cmp::Reverse(slot_busyness(act, s)));
        }
        for slot in order {
            if changed.len() >= changes {
                break;
            }
            // Over budget, a change may only help. If nothing fits, the
            // other layers make room below.
            let room = budget.max(cost(act));
            let lead = Some(slot) == lead;
            if self.change(act, slot, room, lead, true)
                || self.change(act, slot, MAX_BUDGET.max(room), lead, true)
            {
                changed.push(slot);
            }
        }
        // What the run is about stays.
        let mut rest: Vec<Slot> = SLOTS
            .iter()
            .copied()
            .filter(|s| !changed.contains(s) && Some(*s) != self.theme)
            .collect();
        rest.sort_by_key(|&s| std::cmp::Reverse(slot_busyness(act, s)));
        if !act.idle.suits(act.mode) {
            act.idle = Idle::Still;
        }
        for slot in rest {
            if fits(act, budget) {
                break;
            }
            let room = budget.max(cost(act).saturating_sub(1));
            let _ = self.change(act, slot, room, false, true);
        }
    }

    /// A preset that hasn't played for longest, in the current palette
    /// unless it names a theme.
    fn peak(&mut self, act: &mut Act) {
        let weights: Vec<u64> = (0..PRESETS.len())
            .map(|i| self.age((None, i)).saturating_sub(REST))
            .collect();
        let pick = self
            .weighted(&weights)
            .unwrap_or_else(|| self.rng.usize(..PRESETS.len()));
        let Some(p) = PRESETS.get(pick) else {
            return;
        };
        self.seen.insert((None, pick), self.scene);
        if let Some(theme) = p
            .theme
            .and_then(|name| THEMES.iter().find(|t| t.name == name))
        {
            act.palette = Palette::theme(theme);
        }
        act.shader = p.shader;
        act.backdrop = p.backdrop;
        act.look = p.look;
        act.style = p.style;
        act.mode = p.mode;
        act.post = p.post;
        act.idle = p.idle;
    }
}

fn index<T: Layer>(value: T) -> Option<usize> {
    T::VALUES.iter().position(|&v| v == value)
}

fn slot_busyness(act: &Act, slot: Slot) -> u32 {
    let at = SLOTS.iter().position(|&s| s == slot);
    at.and_then(|i| busyness(act).get(i).copied()).unwrap_or(0)
}

/// Alternates logo and wordmark, sometimes both together.
fn subject(current: &Subject, fonts: &SystemFonts, rng: &mut Rng) -> Subject {
    let word = |rng: &mut Rng| match rng.u8(0..8) {
        0 => Word::Small,
        1 => Word::Tiny,
        2 => Word::Pixel,
        3 => {
            let faces: Vec<Face> = (0..7)
                .filter_map(|_| fonts.pick(stage::WORDMARK, rng))
                .collect();
            if faces.is_empty() {
                Word::Pixel
            } else {
                Word::Ransom(faces)
            }
        }
        _ => fonts
            .pick(stage::WORDMARK, rng)
            .map_or(Word::Pixel, Word::Face),
    };
    let logo = |rng: &mut Rng| {
        if rng.bool() {
            Logo::Mascot
        } else {
            Logo::Emblem
        }
    };
    match (current, rng.u8(0..4)) {
        (Subject::Lockup(..), n) if n < 2 => Subject::Logo(logo(rng)),
        (Subject::Lockup(..), _) => Subject::Word(word(rng)),
        (_, 0) => {
            let l = logo(rng);
            Subject::Lockup(l, word(rng))
        }
        (Subject::Logo(_), _) => Subject::Word(word(rng)),
        (Subject::Word(_), _) => Subject::Logo(logo(rng)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How many scenes pass `keep`, counting each layer's choices (not the
    /// palettes or subjects).
    fn combinations(keep: fn(&Act) -> bool) -> u64 {
        let mut act = Act::plain();
        let mut count = 0_u64;
        for &shader in SHADERS {
            act.shader = shader;
            for &backdrop in BACKDROPS {
                act.backdrop = backdrop;
                for &look in Look::ALL {
                    act.look = look;
                    for &style in Style::ALL {
                        act.style = style;
                        for &mode in Mode::ALL {
                            act.mode = mode;
                            for &post in Post::ALL {
                                act.post = post;
                                for &idle in IDLES {
                                    act.idle = idle;
                                    if keep(&act) {
                                        count = count.saturating_add(1);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        count
    }

    fn run(seed: u64, scenes: usize) -> Vec<Act> {
        let fonts = SystemFonts::empty();
        let mut director = Director::new(seed);
        let mut act = Act::plain();
        (0..scenes)
            .map(|_| {
                act = director.next(&act, &fonts);
                act.clone()
            })
            .collect()
    }

    fn key(act: &Act) -> String {
        format!(
            "{} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {} {}",
            act.palette.name,
            act.shader,
            act.backdrop,
            act.look,
            act.style,
            act.mode,
            act.post,
            act.idle,
            act.variant,
            act.label()
        )
    }

    #[test]
    fn the_budget_leaves_millions_of_scenes() {
        let all = combinations(|a| a.idle.suits(a.mode));
        let fitting = combinations(|a| fits(a, MAX_BUDGET));

        println!("scenes: {all} before the budget, {fitting} within it");
        assert!(fitting >= 1_000_000, "{fitting}");
        assert!(fitting < all);
    }

    #[test]
    fn a_seed_replays_its_scenes() {
        let (a, b) = (run(7, 60), run(7, 60));
        assert!(a.iter().zip(&b).all(|(x, y)| key(x) == key(y)));
        let c = run(8, 60);
        assert!(a.iter().zip(&c).any(|(x, y)| key(x) != key(y)));
    }

    #[test]
    fn scenes_stay_within_budget_and_cuts_stay_small() {
        let acts = run(3, 400);
        let presets = acts.iter().filter(|a| !fits(a, MAX_BUDGET)).count();
        // Only presets may go over.
        assert!(presets < acts.len() / 5, "{presets}");
        let small = acts
            .iter()
            .zip(acts.iter().skip(1))
            .filter(|(a, b)| changed_layers(a, b) <= 2)
            .count();
        assert!(small * 2 > acts.len(), "{small} of {}", acts.len());
    }

    fn changed_layers(a: &Act, b: &Act) -> usize {
        [
            a.palette.name != b.palette.name,
            a.shader != b.shader,
            a.backdrop != b.backdrop,
            a.look != b.look,
            a.style != b.style,
            a.mode != b.mode,
            a.post != b.post,
            a.idle != b.idle,
        ]
        .iter()
        .filter(|c| **c)
        .count()
    }

    #[test]
    fn long_runs_reach_every_choice() {
        let acts = run(11, 1500);
        fn all_seen<T: Layer + std::fmt::Debug>(acts: &[Act]) {
            for &v in T::VALUES {
                assert!(acts.iter().any(|a| T::of(a) == v), "{v:?} never played");
            }
        }
        all_seen::<Shader>(&acts);
        all_seen::<Backdrop>(&acts);
        all_seen::<Look>(&acts);
        all_seen::<Style>(&acts);
        all_seen::<Mode>(&acts);
        all_seen::<Post>(&acts);
        all_seen::<Idle>(&acts);
    }

    #[test]
    fn plain_choices_never_clash() {
        let act = Act::plain();
        assert_eq!(cost(&act), 0);
        assert!(fits(&act, 0));
    }

    #[test]
    fn presets_name_real_themes() {
        for p in PRESETS {
            if let Some(name) = p.theme {
                assert!(THEMES.iter().any(|t| t.name == name), "{name}");
            }
        }
    }
}
