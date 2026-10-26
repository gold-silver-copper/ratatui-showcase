//! A fullscreen ratatui showcase for big monitors and projectors: the logo and
//! the wordmark take turns, scaled to fill any terminal.

mod art;
mod backdrop;
mod director;
mod fonts;
mod fx;
mod glyph_table;
mod glyphs;
mod gpu;
mod num;
mod params;
mod profile;
mod shader;
mod stage;
mod theme;
mod transition;

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
use ratatui::crossterm::terminal::{self, BeginSynchronizedUpdate, EndSynchronizedUpdate};
use tachyonfx::EffectManager;

use crate::art::Style;
use crate::backdrop::{BACKDROPS, Backdrop};
use crate::director::Director;
use crate::fonts::SystemFonts;
use crate::glyphs::{GlyphSet, Glyphs};
use crate::stage::{Act, IDLES, Idle, LOOKS, Logo, Look, MODES, Mode, POSTS, Post, Stage, Subject};
use crate::theme::{Palette, SHADERS, Shader};
use crate::transition::{Crossing, KINDS};

const USAGE: &str = "\
usage: ratatui-showcase [--hold SECONDS] [--fps N] [--glyphs blocks|sextants|full]
                        [--watch] [--profile] [--dither]

  --hold       how long each scene stays on screen, 0.5 to 86400 (default 7)
  --fps        frames per second, 1 to 1000 (default 60)
  --glyphs     block characters the terminal draws well (default full: octants
               and wedges, as in foot, kitty, ghostty and wezterm)
  --watch      reload src/shaders when they change, to tune effects live
  --profile    show how long each phase takes on the GPU in the caption
  --dither     smooth colours by dithering over time (every cell changes every
               frame, so the terminal works about three times as hard)";

struct Config {
    hold: f32,
    fps: f64,
    glyphs: GlyphSet,
    watch: bool,
    profile: bool,
    dither: bool,
}

fn config() -> std::result::Result<Config, String> {
    let mut config = Config {
        hold: 7.0,
        fps: 60.0,
        glyphs: GlyphSet::Full,
        watch: false,
        profile: false,
        dither: false,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--hold" => {
                config.hold =
                    within(&value()?, 0.5, 86_400.0).ok_or("--hold takes 0.5 to 86400 seconds")?
            }
            "--fps" => {
                config.fps =
                    f64::from(within(&value()?, 1.0, 1000.0).ok_or("--fps takes 1 to 1000")?)
            }
            "--glyphs" => config.glyphs = GlyphSet::parse(&value()?).ok_or("unknown glyph set")?,
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
        .insert_resource(Show::new(config.hold))
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

#[derive(Resource)]
struct Show {
    act: Act,
    stage: Option<Stage>,
    /// The scene after the current cell transition.
    pending: Option<Stage>,
    crossing: Option<Crossing>,
    phase: Phase,
    hold: Timer,
    accented: bool,
    caption: bool,
    skip: bool,
    /// The next scene, laid out in the background while this one holds.
    next: Option<(u16, u16, Task<Stage>)>,
    director: Director,
    /// When the shaders' clock started, in seconds since launch. It starts
    /// over now and then, at a scene change, so f32 keeps its precision.
    epoch: f64,
}

impl Show {
    fn new(hold: f32) -> Self {
        Self {
            act: Act {
                subject: Subject::Logo(Logo::Mascot),
                palette: Palette::first(),
                shader: Shader::Sheen,
                backdrop: Backdrop::Vignette,
                look: Look::Smooth,
                style: Style::Solid,
                idle: Idle::Float,
                mode: Mode::Flat,
                post: Post::Bloom,
                variant: 0,
            },
            stage: None,
            pending: None,
            crossing: None,
            phase: Phase::Entering,
            hold: Timer::from_seconds(hold, TimerMode::Once),
            accented: false,
            caption: true,
            skip: false,
            next: None,
            director: Director::default(),
            epoch: 0.0,
        }
    }

    /// Starts laying out the scene after `current` on another thread.
    fn prepare(&mut self, current: &Act, fonts: &SystemFonts, cols: u16, rows: u16) {
        let act = self.director.next(current, fonts);
        let ratio = cell_ratio();
        let task =
            AsyncComputeTaskPool::get().spawn(async move { Stage::new(act, cols, rows, ratio) });
        self.next = Some((cols, rows, task));
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
fn pick<T: Copy + PartialEq>(items: &[T], avoid: Option<T>) -> Option<T> {
    let choices: Vec<T> = items
        .iter()
        .copied()
        .filter(|&i| Some(i) != avoid)
        .collect();
    choices
        .get(fastrand::usize(..choices.len().max(1)))
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
        } else if matches!(
            key.code,
            KeyCode::Char(' ') | KeyCode::Right | KeyCode::Enter
        ) {
            show.skip = true;
        } else if let KeyCode::Char(c) = key.code {
            match c {
                't' => act.palette = Palette::random(),
                's' => act.shader = pick(SHADERS, Some(act.shader)).unwrap_or(act.shader),
                'b' => act.backdrop = pick(BACKDROPS, Some(act.backdrop)).unwrap_or(act.backdrop),
                'l' => act.look = pick(LOOKS, Some(act.look)).unwrap_or(act.look),
                'p' => act.post = pick(POSTS, Some(act.post)).unwrap_or(act.post),
                'w' => act.idle = pick(IDLES, Some(act.idle)).unwrap_or(act.idle),
                'm' => {
                    act.mode = pick(MODES, Some(act.mode)).unwrap_or(act.mode);
                    if act.mode == Mode::Turntable {
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
    mut context: ResMut<RatatuiContext>,
    mut show: ResMut<Show>,
    mut effects: ResMut<Effects>,
    (glyphs, fonts, time): (Res<Glyph>, Res<SystemFonts>, Res<Time>),
    mut gpu: gpu::Gpu,
    (gpu_frame, dither): (Res<gpu::GpuFrame>, Res<Dither>),
) -> Result {
    let show = &mut *show;
    let effects = effects.0.get_mut().unwrap_or_else(PoisonError::into_inner);
    let (now, dt) = (time.elapsed_secs_f64(), time.delta());
    // Frames go out whole: the terminal shows none of it until the end.
    ratatui::crossterm::queue!(context.backend_mut(), BeginSynchronizedUpdate)?;
    context.draw(|frame| {
        let area = frame.area();
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
            } else {
                show.act = act;
            }
            effects.transition.cancel_unique_effect(());
            effects.accent.cancel_unique_effect(());
            show.crossing = None;
            show.phase = Phase::Holding;
            let stage = Stage::new(show.act.clone(), area.width, area.height, cell_ratio());
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
            Phase::Entering if skip || !effects.transition.is_running() => {
                show.phase = Phase::Holding;
                show.hold.reset();
                show.accented = false;
            }
            Phase::Holding => {
                show.hold.tick(dt);
                if show.next.is_none()
                    && show.hold.fraction() > 0.2
                    && let Some(stage) = &show.stage
                {
                    let act = stage.act.clone();
                    show.prepare(&act, &fonts, area.width, area.height);
                }
                if !show.accented && show.hold.fraction() > 0.45 {
                    show.accented = true;
                    if let Some(stage) = &show.stage
                        && let Some(accent) = fx::accent(stage, area)
                    {
                        effects.accent.add_unique_effect((), accent);
                    }
                }
                if skip || show.hold.is_finished() {
                    effects.accent.cancel_unique_effect(());
                    change(show, effects, &fonts, area, now);
                }
            }
            Phase::Crossing => {
                if let Some(crossing) = &mut show.crossing {
                    crossing.advance(dt.as_secs_f32());
                }
                if show.crossing.as_ref().is_none_or(Crossing::done) {
                    show.crossing = None;
                    show.phase = Phase::Holding;
                    show.hold.reset();
                    show.accented = false;
                }
            }
            Phase::Leaving if !effects.transition.is_running() => {
                if let Some(stage) = show.pending.take() {
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
        let frame_info = gpu::Frame {
            t: num::narrow(now - show.epoch),
            now,
            dt: dt.as_secs_f32(),
            dither: dither.0,
        };
        if let Some(reason) = gpu.render(&glyphs.0, stage, show.crossing.as_ref(), frame_info) {
            notice(frame.buffer_mut(), &reason);
            return;
        }
        if let Some(cells) = gpu_frame.cells(area.width, area.height) {
            gpu::draw(cells, frame.buffer_mut());
        }
        effects
            .transition
            .process_effects(dt.into(), frame.buffer_mut(), area);
        effects
            .accent
            .process_effects(dt.into(), frame.buffer_mut(), area);

        if show.caption {
            let act = &stage.act;
            let mut caption = format!(
                "{}  ·  {}  ·  {}",
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
            if let Some(profile) = &gpu_frame.profile {
                caption = format!("{profile}  ·  {caption}");
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
            caption_line(frame.buffer_mut(), &caption, act);
        }
    })?;
    ratatui::crossterm::execute!(context.backend_mut(), EndSynchronizedUpdate)?;
    Ok(())
}

/// `text` wrapped and centred on an otherwise blank screen.
fn notice(buf: &mut ratatui::buffer::Buffer, text: &str) {
    let width = usize::from(buf.area.width.saturating_sub(4)).max(10);
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

/// Starts the change to the next scene: usually a pixel-level crossing in
/// the same look, sometimes a cell-level transition that can change the look.
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
    let mut next = show.take_next(&from.act, fonts, area.width, area.height);
    if fastrand::f32() < 0.65 {
        next.act.look = from.act.look;
        // Rarely needed: most clocks start over during a cell transition.
        show.rebase(now, 1800.0);
        let to = next;
        let kind = pick(KINDS, None).unwrap_or(transition::Kind::Morph);
        show.crossing = Some(Crossing::new(kind, from, &to));
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
