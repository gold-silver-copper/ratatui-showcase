//! A fullscreen ratatui showcase for big monitors and projectors: the logo and
//! the wordmark take turns, scaled to fill any terminal.

mod art;
mod backdrop;
mod card;
mod diff;
mod director;
mod fonts;
mod fx;
mod gallery;
mod glyph_table;
mod glyphs;
mod gpu;
mod noise;
mod num;
mod params;
mod profile;
mod screen;
mod shader;
mod stage;
mod tempo;
mod theme;
mod transition;
mod zoom;

use std::io::Write;
use std::process::ExitCode;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use bevy::app::TaskPoolOptions;
use bevy::app::{AppExit, ScheduleRunnerPlugin};
use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on};
use bevy::window::ExitCondition;
use bevy_ratatui::event::{KeyMessage, ResizeMessage};
use bevy_ratatui::{RatatuiContext, RatatuiPlugins};
use ratatui::crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
use ratatui::crossterm::terminal;
use tachyonfx::EffectManager;

use crate::backdrop::{BACKDROPS, Backdrop};
use crate::card::Card;
use crate::director::{Director, Pin};
use crate::fonts::SystemFonts;
use crate::gallery::Gallery;
use crate::glyphs::{GlyphSet, Glyphs};
use crate::stage::{Act, IDLES, Idle, Look, Mode, Post, Stage};
use crate::tempo::Tempo;
use crate::theme::{Palette, SHADERS, Shader};
use crate::transition::{Crossing, KINDS};

const USAGE: &str = "\
usage: ratatui-showcase [--hold SECONDS] [--bpm N] [--seed N] [--fps N]
                        [--glyphs blocks|sextants|full] [--pin LAYER=CHOICE]...
                        [--watch] [--profile] [--dither]

  --hold       how long each scene stays on screen, 0.5 to 86400, rounded to
               whole bars (default 7: 4 bars at 120 bpm)
  --bpm        the show's tempo, 40 to 240 (default 120): cuts land on bar
               lines, transitions last whole beats, motions keep time
  --seed       the scenes to play; the caption shows it, and the same seed
               on the same terminal size plays the same scenes again
  --fps        frames per second, 1 to 1000 (default 60)
  --glyphs     block characters the terminal draws well (default full: octants
               and wedges, as in foot, kitty, ghostty and wezterm)
  --pin        keep a layer in every scene, such as post=paint, mode=slime,
               shader=plasma, backdrop=mandelbrot, look=ascii or theme=ember
  --watch      reload src/shaders when they change, to tune effects live
  --profile    show how long each phase takes on the GPU in the caption
  --dither     smooth colours by dithering over time (every cell changes every
               frame, so the terminal works about three times as hard)";

struct Config {
    hold: f32,
    bpm: f32,
    seed: Option<u64>,
    fps: f64,
    glyphs: GlyphSet,
    watch: bool,
    profile: bool,
    dither: bool,
    pins: Vec<Pin>,
}

fn config() -> std::result::Result<Config, String> {
    let mut config = Config {
        hold: 7.0,
        bpm: 120.0,
        seed: None,
        fps: 60.0,
        glyphs: GlyphSet::Full,
        watch: false,
        profile: false,
        dither: false,
        pins: Vec::new(),
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--hold" => {
                config.hold =
                    within(&value()?, 0.5, 86_400.0).ok_or("--hold takes 0.5 to 86400 seconds")?
            }
            "--bpm" => {
                config.bpm = within(&value()?, 40.0, 240.0).ok_or("--bpm takes 40 to 240")?
            }
            "--seed" => {
                config.seed = Some(
                    value()?
                        .parse()
                        .map_err(|_| "--seed takes a whole number")?,
                )
            }
            "--fps" => {
                config.fps =
                    f64::from(within(&value()?, 1.0, 1000.0).ok_or("--fps takes 1 to 1000")?)
            }
            "--glyphs" => config.glyphs = GlyphSet::parse(&value()?).ok_or("unknown glyph set")?,
            "--pin" => config.pins.push(Pin::parse(&value()?)?),
            "--profile" => config.profile = true,
            "--watch" => config.watch = true,
            "--dither" => config.dither = true,
            "-h" | "--help" => return Err(String::new()),
            _ => return Err(format!("unknown option {arg}")),
        }
    }
    Ok(config)
}

/// `text` as a number from `low` to `high`.
fn within(text: &str, low: f32, high: f32) -> Option<f32> {
    text.parse().ok().filter(|v: &f32| (low..=high).contains(v))
}

fn main() -> ExitCode {
    let config = match config() {
        Ok(config) => config,
        Err(message) => {
            let (text, code) = if message.is_empty() {
                (format!("{USAGE}\n"), ExitCode::SUCCESS)
            } else {
                (format!("{message}\n\n{USAGE}\n"), ExitCode::from(2))
            };
            // With stderr gone there is no one left to tell.
            let _ = std::io::stderr().write_all(text.as_bytes());
            return code;
        }
    };
    diff::warm();
    gallery::warm();
    let exit = App::new()
        .add_plugins((
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: None,
                    exit_condition: ExitCondition::DontExit,
                    close_when_requested: false,
                    ..default()
                })
                // A handful of small systems: more threads only add overhead.
                .set(TaskPoolPlugin {
                    task_pool_options: TaskPoolOptions::with_num_threads(2),
                })
                .set(RenderPlugin {
                    synchronous_pipeline_compilation: true,
                    ..default()
                }),
            ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(1.0 / config.fps)),
            RatatuiPlugins::default(),
            gpu::GpuPlugin {
                watch: config.watch,
                profile: config.profile,
            },
        ))
        .insert_resource(SystemFonts::load())
        .insert_resource(Glyph(Glyphs::new(config.glyphs)))
        .insert_resource(Dither(config.dither))
        .insert_resource(screen::Screen::new(config.fps))
        .insert_resource(Show::new(
            f64::from(config.hold),
            Tempo::new(f64::from(config.bpm)),
            config.seed.unwrap_or_else(|| fastrand::u64(..1_000_000)),
            config.pins,
        ))
        .init_resource::<Effects>()
        .add_systems(PreUpdate, input)
        .add_systems(Update, frame)
        .run();
    if exit.is_success() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

#[derive(Resource)]
struct Glyph(Glyphs);

/// Whether colours are dithered over time.
#[derive(Resource)]
struct Dither(bool);

/// Effects are `Send` but not `Sync`; with `ResMut` the lock is never contended.
#[derive(Resource, Default)]
struct Effects(Mutex<Layers>);

#[derive(Default)]
struct Layers {
    transition: EffectManager<()>,
    accent: EffectManager<()>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Entering,
    Holding,
    Crossing,
    Leaving,
}

/// Scenes between two interludes.
const SCENES_PER_CARD: usize = 1;

/// What shows: GPU scenes, or one of the interludes between them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Scene,
    Card,
    Gallery,
}

impl Kind {
    const ALL: [Kind; 3] = [Kind::Scene, Kind::Card, Kind::Gallery];

    /// The kind `by` places on, wrapping round.
    fn step(self, by: isize) -> Kind {
        let len = Kind::ALL.len().cast_signed();
        let at = Kind::ALL.iter().position(|&k| k == self).unwrap_or(0);
        let next = at
            .cast_signed()
            .saturating_add(by)
            .rem_euclid(len.max(1))
            .cast_unsigned();
        Kind::ALL.get(next).copied().unwrap_or(Kind::Scene)
    }
}

/// What plays between scenes, in turns: the card about ratatui, and
/// libraries from its community.
enum Interlude {
    Card(Card),
    Gallery(Box<Gallery>),
}

impl Interlude {
    fn kind(&self) -> Kind {
        match self {
            Interlude::Card(_) => Kind::Card,
            Interlude::Gallery(_) => Kind::Gallery,
        }
    }

    fn hold(&self) -> f64 {
        match self {
            Interlude::Card(card) => card.hold,
            Interlude::Gallery(gallery) => gallery.hold,
        }
    }

    fn background(&self) -> theme::Rgb {
        match self {
            Interlude::Card(card) => card.background(),
            Interlude::Gallery(gallery) => gallery.background(),
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Interlude::Card(_) => "about ratatui",
            Interlude::Gallery(_) => "community widgets",
        }
    }

    fn draw(&mut self, buf: &mut ratatui::buffer::Buffer, now: f64, dt: f32, beat: f32) {
        match self {
            Interlude::Card(card) => card.draw(buf, now),
            Interlude::Gallery(gallery) => gallery.draw(buf, now, dt, beat),
        }
    }
}

#[derive(Resource)]
struct Show {
    act: Act,
    stage: Option<Stage>,
    /// The scene after the current cell transition.
    pending: Option<Stage>,
    crossing: Option<Crossing>,
    phase: Phase,
    tempo: Tempo,
    /// How long a scene holds, in seconds: whole bars.
    hold: f64,
    /// When the next scene is laid out, the accent plays and the scene
    /// ends, in seconds since launch; the last two on bar lines.
    prepare_at: f64,
    accent_at: f64,
    hold_until: f64,
    caption: bool,
    skip: bool,
    /// A switch asked for with the arrow keys: places along `Kind::ALL`.
    jump: isize,
    /// The interlude on screen, and the one that comes once what shows
    /// has left.
    interlude: Option<Interlude>,
    due: Option<Kind>,
    /// Cards and gallery visits so far, and scenes since the last
    /// interlude.
    cards: usize,
    visits: usize,
    scenes: usize,
    /// The next scene, laid out in the background while this one holds.
    next: Option<(u16, u16, Task<Stage>)>,
    director: Director,
    /// When the shaders' clock started, in seconds since launch. It starts
    /// over now and then, at a scene change, so f32 keeps its precision.
    epoch: f64,
}

impl Show {
    fn new(hold: f64, tempo: Tempo, seed: u64, pins: Vec<Pin>) -> Self {
        let director = Director::new(seed, pins);
        Self {
            act: director.pinned(Act {
                shader: Shader::Sheen,
                idle: Idle::Float,
                post: Post::Bloom,
                ..Act::plain()
            }),
            stage: None,
            pending: None,
            crossing: None,
            phase: Phase::Entering,
            tempo,
            hold: tempo.bars(hold),
            prepare_at: f64::MAX,
            accent_at: f64::MAX,
            hold_until: f64::MAX,
            caption: true,
            skip: false,
            jump: 0,
            interlude: None,
            due: None,
            cards: 0,
            visits: 0,
            scenes: 0,
            next: None,
            director,
            epoch: 0.0,
        }
    }

    /// The scene starts holding at `now`, until the first bar line a hold
    /// later.
    fn hold_from(&mut self, now: f64) {
        let hold = self.interlude.as_ref().map_or(self.hold, Interlude::hold);
        self.phase = Phase::Holding;
        self.prepare_at = now + hold * 0.2;
        self.hold_until = self.tempo.next_bar(now + hold);
        let accent = self.tempo.next_bar(now + hold * 0.45);
        self.accent_at = if accent < self.hold_until {
            accent
        } else {
            f64::MAX
        };
    }

    /// The interlude that comes next on its own: the card and the gallery
    /// take turns.
    fn next_interlude(&self) -> Kind {
        if self.cards > self.visits {
            Kind::Gallery
        } else {
            Kind::Card
        }
    }

    /// A new interlude of `kind`; a card takes the colours of the scene
    /// before it.
    fn new_interlude(&mut self, kind: Kind, now: f64) -> Interlude {
        let act = self.stage.as_ref().map_or(&self.act, |stage| &stage.act);
        if kind == Kind::Gallery {
            let each = self.hold.max(gallery::SHOW);
            let hold = self
                .tempo
                .bars(each * f64::from(num::u32_of(gallery::PER_VISIT)));
            let gallery = Gallery::new(self.visits, act.theme(), now, hold);
            self.visits = self.visits.saturating_add(1);
            Interlude::Gallery(Box::new(gallery))
        } else {
            let hold = self.tempo.bars(self.hold.max(card::HOLD));
            let card = Card::new(self.cards, act.theme(), now, hold);
            self.cards = self.cards.saturating_add(1);
            Interlude::Card(card)
        }
    }

    /// Starts laying out the scene after `current` on another thread.
    fn prepare(&mut self, current: &Act, fonts: &SystemFonts, cols: u16, rows: u16) {
        let act = self.director.next(current, fonts);
        let ratio = cell_ratio();
        let aim = self.aim(&act);
        let task = AsyncComputeTaskPool::get().spawn(async move {
            let mut stage = Stage::new(act, cols, rows, ratio);
            if let Some((dive, seed)) = aim {
                stage.aim(dive, seed);
            }
            stage
        });
        self.next = Some((cols, rows, task));
    }

    /// For a Mandelbrot zoom that should dive somewhere (two in three; the
    /// rest keep the endless zoom): how long the dive takes, three quarters
    /// of the hold, and the seed for the search.
    fn aim(&mut self, act: &Act) -> Option<(f32, u64)> {
        let seed = self.director.rng().u64(..);
        (act.backdrop == Backdrop::Mandelbrot && !seed.is_multiple_of(3))
            .then(|| (num::narrow(self.hold * 0.75), seed))
    }

    /// The prepared next scene if it fits `cols` x `rows`, else a new one.
    fn take_next(&mut self, current: &Act, fonts: &SystemFonts, cols: u16, rows: u16) -> Stage {
        match self.next.take() {
            Some((c, r, task)) if (c, r) == (cols, rows) => block_on(task),
            _ => Stage::new(self.director.next(current, fonts), cols, rows, cell_ratio()),
        }
    }

    /// Restarts the shaders' clock once it has run for `after` seconds.
    fn rebase(&mut self, now: f64, after: f64) {
        if now - self.epoch > after {
            self.epoch = now;
        }
    }
}

/// A random item, avoiding `avoid` when possible.
fn pick<T: Copy + PartialEq>(items: &[T], avoid: Option<T>, rng: &mut fastrand::Rng) -> Option<T> {
    let choices: Vec<T> = items
        .iter()
        .copied()
        .filter(|&i| Some(i) != avoid)
        .collect();
    choices
        .get(rng.usize(..choices.len().max(1)))
        .copied()
        .or(items.first().copied())
}

fn input(
    mut keys: MessageReader<KeyMessage>,
    mut resizes: MessageReader<ResizeMessage>,
    mut show: ResMut<Show>,
    mut exit: MessageWriter<AppExit>,
) {
    // A new mode needs a fresh stage, so its simulation starts from scratch.
    let mut rebuild = resizes.read().count() > 0;
    for key in keys.read().filter(|key| key.kind != KeyEventKind::Release) {
        let show = &mut *show;
        let act = show
            .stage
            .as_mut()
            .map_or(&mut show.act, |stage| &mut stage.act);
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
            || (ctrl && key.code == KeyCode::Char('c'))
        {
            exit.write_default();
        } else if matches!(key.code, KeyCode::Char(' ') | KeyCode::Enter) {
            show.skip = true;
        } else if key.code == KeyCode::Right {
            show.jump = show.jump.saturating_add(1);
        } else if key.code == KeyCode::Left {
            show.jump = show.jump.saturating_sub(1);
        } else if let KeyCode::Char(c) = key.code {
            // Keys are the visitor's choice, not the seed's.
            let rng = &mut fastrand::Rng::new();
            match c {
                't' => act.palette = Palette::random(rng),
                's' => act.shader = pick(SHADERS, Some(act.shader), rng).unwrap_or(act.shader),
                'b' => {
                    act.backdrop = pick(BACKDROPS, Some(act.backdrop), rng).unwrap_or(act.backdrop)
                }
                'l' => act.look = pick(Look::ALL, Some(act.look), rng).unwrap_or(act.look),
                'p' => act.post = pick(Post::ALL, Some(act.post), rng).unwrap_or(act.post),
                'w' => {
                    let suited: Vec<Idle> = IDLES
                        .iter()
                        .copied()
                        .filter(|i| i.suits(act.mode))
                        .collect();
                    act.idle = pick(&suited, Some(act.idle), rng).unwrap_or(act.idle);
                }
                'm' => {
                    act.mode = pick(Mode::ALL, Some(act.mode), rng).unwrap_or(act.mode);
                    if !act.idle.suits(act.mode) {
                        act.idle = Idle::Still;
                    }
                    rebuild = true;
                }
                'i' => show.caption = !show.caption,
                _ => {}
            }
        }
    }
    if rebuild && let Some(stage) = show.stage.take() {
        show.act = stage.act;
    }
}

fn frame(
    // The context set the terminal up; frames go out through the screen.
    _context: Res<RatatuiContext>,
    (mut screen, mut last): (ResMut<screen::Screen>, Local<Frames>),
    mut show: ResMut<Show>,
    mut effects: ResMut<Effects>,
    (glyphs, fonts, time): (Res<Glyph>, Res<SystemFonts>, Res<Time>),
    mut gpu: gpu::Gpu,
    (gpu_frame, dither): (Res<gpu::GpuFrame>, Res<Dither>),
) -> Result {
    let show = &mut *show;
    let effects = effects.0.get_mut().unwrap_or_else(PoisonError::into_inner);
    let now = time.elapsed_secs_f64();
    // While the terminal falls behind, some frames aren't drawn at all.
    let Some(dt) = screen.due(time.delta()) else {
        gpu.idle();
        return Ok(());
    };
    let (cols, rows) = terminal::size()?;
    let area = ratatui::layout::Rect::new(0, 0, cols, rows);
    let (changed, flips) = if gpu_frame.profile.is_some() {
        flicker(&gpu_frame.data, &mut last)
    } else {
        (0, 0)
    };
    let wire = gpu_frame.profile.as_ref().map(|_| screen.summary());
    let calm = screen.pressure();
    screen.frame(area, changed, flips, |buffer| {
        if show
            .stage
            .as_ref()
            .is_none_or(|stage| (stage.cols, stage.rows) != (area.width, area.height))
        {
            let first = show.stage.is_none()
                && show.phase == Phase::Entering
                && !effects.transition.is_running();
            let act = show
                .stage
                .take()
                .map_or_else(|| show.act.clone(), |stage| stage.act);
            // A half-finished transition would play on the old layout: settle it.
            if let Some(pending) = show.pending.take() {
                show.act = pending.act;
                show.interlude = None;
            } else {
                show.act = act;
            }
            if let Some(kind) = show.due.take() {
                show.interlude = Some(show.new_interlude(kind, now));
            }
            effects.transition.cancel_unique_effect(());
            effects.accent.cancel_unique_effect(());
            show.crossing = None;
            show.hold_from(now);
            let mut stage = Stage::new(show.act.clone(), area.width, area.height, cell_ratio());
            // Nothing shows yet: the first scene can wait for its search.
            if first && let Some((dive, seed)) = show.aim(&stage.act) {
                stage.aim(dive, seed);
            }
            if first {
                effects
                    .transition
                    .add_unique_effect((), fx::enter(&stage, area));
                show.phase = Phase::Entering;
            }
            show.stage = Some(stage);
        }

        let skip = std::mem::take(&mut show.skip);
        match show.phase {
            Phase::Entering if skip || !effects.transition.is_running() => show.hold_from(now),
            Phase::Holding => {
                if show.next.is_none()
                    && now >= show.prepare_at
                    && let Some(stage) = &show.stage
                {
                    let act = stage.act.clone();
                    show.prepare(&act, &fonts, area.width, area.height);
                }
                if now >= show.accent_at {
                    show.accent_at = f64::MAX;
                    if show.interlude.is_none()
                        && let Some(stage) = &show.stage
                        && let Some(accent) = fx::accent(stage, area)
                    {
                        effects.accent.add_unique_effect((), accent);
                    }
                }
                let jump = std::mem::take(&mut show.jump);
                let showing = show.interlude.as_ref().map_or(Kind::Scene, Interlude::kind);
                // In the gallery, skipping goes to its next library first.
                let skip = skip
                    && match &mut show.interlude {
                        Some(Interlude::Gallery(gallery)) => match gallery.next(now) {
                            Some(skipped) => {
                                show.hold_until -= skipped;
                                false
                            }
                            None => true,
                        },
                        _ => true,
                    };
                let to = if jump != 0 {
                    Some(showing.step(jump))
                } else if skip || now >= show.hold_until {
                    Some(if showing == Kind::Scene {
                        show.scenes = show.scenes.saturating_add(1);
                        if show.scenes >= SCENES_PER_CARD {
                            show.next_interlude()
                        } else {
                            Kind::Scene
                        }
                    } else {
                        Kind::Scene
                    })
                } else {
                    None
                };
                match to {
                    None => {}
                    Some(Kind::Scene) if showing == Kind::Scene => {
                        effects.accent.cancel_unique_effect(());
                        change(show, effects, &fonts, area, now);
                    }
                    Some(Kind::Scene) => {
                        if let Some(interlude) = &show.interlude {
                            effects
                                .transition
                                .add_unique_effect((), fx::card_leave(interlude.background()));
                        }
                        if let Some(stage) = &show.stage {
                            let act = stage.act.clone();
                            show.pending =
                                Some(show.take_next(&act, &fonts, area.width, area.height));
                        }
                        show.phase = Phase::Leaving;
                    }
                    Some(kind) => {
                        effects.accent.cancel_unique_effect(());
                        match &show.interlude {
                            Some(interlude) => effects
                                .transition
                                .add_unique_effect((), fx::card_leave(interlude.background())),
                            None => {
                                if let Some(stage) = &show.stage {
                                    effects
                                        .transition
                                        .add_unique_effect((), fx::leave(stage, area));
                                }
                            }
                        }
                        show.scenes = 0;
                        show.due = Some(kind);
                        show.phase = Phase::Leaving;
                    }
                }
            }
            Phase::Crossing => {
                if let Some(crossing) = &mut show.crossing {
                    crossing.advance(dt.as_secs_f32());
                }
                if show.crossing.as_ref().is_none_or(Crossing::done) {
                    show.crossing = None;
                    show.hold_from(now);
                }
            }
            Phase::Leaving if !effects.transition.is_running() => {
                if let Some(kind) = show.due.take() {
                    let interlude = show.new_interlude(kind, now);
                    effects
                        .transition
                        .add_unique_effect((), fx::card_enter(interlude.background(), area));
                    show.interlude = Some(interlude);
                } else if let Some(stage) = show.pending.take() {
                    show.interlude = None;
                    // The old scene is gone: a good moment for the clock to
                    // start over unseen.
                    show.rebase(now, 600.0);
                    effects
                        .transition
                        .add_unique_effect((), fx::enter(&stage, area));
                    show.stage = Some(stage);
                }
                show.phase = Phase::Entering;
            }
            Phase::Entering | Phase::Leaving => {}
        }

        let Some(stage) = &show.stage else { return };
        if let Some(interlude) = &mut show.interlude {
            gpu.idle();
            interlude.draw(buffer, now, dt.as_secs_f32(), show.tempo.phase(now));
        } else {
            let frame_info = gpu::Frame {
                t: num::narrow(now - show.epoch),
                beat: show.tempo.phase(now),
                now,
                dt: dt.as_secs_f32(),
                dither: dither.0,
                calm,
            };
            if let Some(reason) = gpu.render(&glyphs.0, stage, show.crossing.as_ref(), frame_info) {
                notice(buffer, &reason);
                return;
            }
            if let Some(cells) = gpu_frame.cells(area.width, area.height) {
                gpu::draw(cells, buffer);
            }
        }
        effects.transition.process_effects(dt.into(), buffer, area);
        effects.accent.process_effects(dt.into(), buffer, area);

        if show.caption {
            let act = &stage.act;
            let mut caption = format!(
                "seed {}  ·  {}×{}",
                show.director.seed(),
                area.width,
                area.height
            );
            if let Some(interlude) = &show.interlude {
                caption += &format!("  ·  {}", interlude.name());
            } else {
                caption += &format!(
                    "  ·  {}  ·  {}  ·  {}",
                    act.label(),
                    act.theme().name,
                    act.shader.name()
                );
                caption += &format!("  ·  {}  ·  {}", act.backdrop.name(), act.look.name());
                caption += &format!(
                    "  ·  {}  ·  {}  ·  {}",
                    act.mode.name(),
                    act.post.name(),
                    act.idle.name()
                );
            }
            if let Some(profile) = &gpu_frame.profile {
                caption = format!("{}  ·  {profile}  ·  {caption}", wire.unwrap_or_default());
            }
            if let Some(error) = &gpu_frame.shader_error {
                caption = format!(
                    "shader: {}",
                    error.lines().find(|l| !l.trim().is_empty()).unwrap_or("")
                );
            }
            if let Some(crossing) = &show.crossing {
                caption += &format!("  ·  ↦ {}", crossing.kind.name());
            }
            caption_line(buffer, &caption, act);
        }
    })?;
    Ok(())
}

/// The GPU's last two frames of cells, for counting what changed.
type Frames = (Vec<u32>, Vec<u32>);

/// Cells the GPU changed since the last frame, and those of them that went
/// back to what they showed the frame before (flicker), for `--profile`.
fn flicker(data: &[u32], (last, before): &mut Frames) -> (usize, usize) {
    let cells = |d: &[u32]| d.as_chunks::<3>().0.to_vec();
    let (now, old, older) = (cells(data), cells(last), cells(before));
    let changed = now.iter().zip(&old).filter(|(a, b)| a != b).count();
    let flips = now
        .iter()
        .zip(&old)
        .zip(&older)
        .filter(|((a, b), c)| a != b && a == c)
        .count();
    std::mem::swap(last, before);
    last.clear();
    last.extend_from_slice(data);
    (changed, flips)
}

/// `text` wrapped and centred on an otherwise blank screen.
fn notice(buf: &mut ratatui::buffer::Buffer, text: &str) {
    let lines = card::wrap(text, usize::from(buf.area.width.saturating_sub(4)));
    let top = buf.area.height.saturating_sub(num::u16_of(lines.len())) / 2;
    for (y, line) in (top..).zip(&lines) {
        let x = buf
            .area
            .width
            .saturating_sub(num::u16_of(line.chars().count()))
            / 2;
        buf.set_string(x, y, line, ratatui::style::Style::new());
    }
}

/// Starts the change to the next scene: a pixel-level crossing in the same
/// look, or a cell-level transition when the look changes.
fn change(
    show: &mut Show,
    effects: &mut Layers,
    fonts: &SystemFonts,
    area: ratatui::layout::Rect,
    now: f64,
) {
    let Some(from) = show.stage.take() else {
        return;
    };
    let next = show.take_next(&from.act, fonts, area.width, area.height);
    if next.act.look == from.act.look {
        // Rarely needed: most clocks start over during a cell transition.
        show.rebase(now, 1800.0);
        let to = next;
        // Transitions that run a simulation need it to themselves.
        let busy = from.act.mode.simulates() || to.act.mode.simulates();
        let kinds: Vec<transition::Kind> = KINDS
            .iter()
            .copied()
            .filter(|k| !busy || k.simulation().is_none())
            .collect();
        let kind = pick(&kinds, None, show.director.rng()).unwrap_or(transition::Kind::Morph);
        let duration = num::narrow(show.tempo.beats(f64::from(kind.duration())));
        show.crossing = Some(Crossing::new(kind, from, &to, duration));
        show.stage = Some(to);
        show.phase = Phase::Crossing;
    } else {
        effects
            .transition
            .add_unique_effect((), fx::leave(&from, area));
        show.stage = Some(from);
        show.pending = Some(next);
        show.phase = Phase::Leaving;
    }
}

fn caption_line(buf: &mut ratatui::buffer::Buffer, text: &str, act: &Act) {
    let theme = act.theme();
    let width = num::u16_of(text.chars().count());
    let x = buf.area.width.saturating_sub(width) / 2;
    let y = buf.area.height.saturating_sub(1);
    let color = theme.sample(0.0).lerp(theme.background, 0.45).color();
    buf.set_string(x, y, text, ratatui::style::Style::new().fg(color));
}

/// A cell's height over its width. Terminals that don't report their pixel
/// size are assumed to have 1:2 cells.
fn cell_ratio() -> f32 {
    match terminal::window_size() {
        Ok(size) if size.width > 0 && size.height > 0 && size.columns > 0 && size.rows > 0 => {
            let cell_w = f32::from(size.width) / f32::from(size.columns);
            let cell_h = f32::from(size.height) / f32::from(size.rows);
            (cell_h / cell_w).clamp(1.0, 4.0)
        }
        _ => 2.0,
    }
}
