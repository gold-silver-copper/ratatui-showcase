//! A fullscreen ratatui showcase for big monitors and projectors: the logo and
//! the wordmark take turns, scaled to fill any terminal.

mod art;
mod fonts;
mod fx;
mod stage;
mod theme;

use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use bevy::app::{AppExit, ScheduleRunnerPlugin};
use bevy::prelude::*;
use bevy_ratatui::event::{KeyMessage, ResizeMessage};
use bevy_ratatui::{RatatuiContext, RatatuiPlugins};
use ratatui::crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
use ratatui::crossterm::terminal;
use tachyonfx::EffectManager;

use crate::fonts::SystemFonts;
use crate::stage::{Scene, Stage};
use crate::theme::{SHADERS, Shader, THEMES, Theme};

const HOLD: Duration = Duration::from_secs(7);

fn main() -> Result {
    App::new()
        .add_plugins((
            MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(
                1.0 / 60.0,
            ))),
            RatatuiPlugins::default(),
        ))
        .insert_resource(SystemFonts::load())
        .insert_resource(Show::default())
        .init_resource::<Transitions>()
        .add_systems(PreUpdate, input)
        .add_systems(Update, frame)
        .run();
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Entering,
    Holding,
    Leaving,
}

#[derive(Resource)]
struct Show {
    scene: Scene,
    stage: Option<Stage>,
    phase: Phase,
    hold: Timer,
    theme: usize,
    shader: Shader,
    caption: bool,
    skip: bool,
}

impl Default for Show {
    fn default() -> Self {
        Self {
            scene: Scene::Mascot,
            stage: None,
            phase: Phase::Entering,
            hold: Timer::new(HOLD, TimerMode::Once),
            theme: 0,
            shader: Shader::Sheen,
            caption: true,
            skip: false,
        }
    }
}

impl Show {
    fn theme(&self) -> &'static Theme {
        THEMES
            .get(self.theme)
            .or(THEMES.first())
            .unwrap_or(&FALLBACK_THEME)
    }

    /// Alternates logo and wordmark, each in a fresh theme and shader.
    fn next_scene(&mut self, fonts: &SystemFonts) {
        self.scene = if self.scene.is_logo() {
            match fastrand::u8(0..6) {
                0 => Scene::Logo { small: true },
                1 => Scene::Logo { small: false },
                2 => Scene::PixelFont,
                _ => fonts
                    .pick(stage::WORDMARK)
                    .map_or(Scene::PixelFont, Scene::Face),
            }
        } else if matches!(self.scene, Scene::Mascot) == fastrand::bool() {
            Scene::Emblem
        } else {
            Scene::Mascot
        };
        self.next_theme();
        self.next_shader();
    }

    fn next_theme(&mut self) {
        self.theme = (self.theme + 1 + fastrand::usize(..THEMES.len() - 1)) % THEMES.len();
    }

    fn next_shader(&mut self) {
        let others: Vec<_> = SHADERS.iter().filter(|&&s| s != self.shader).collect();
        self.shader = others
            .get(fastrand::usize(..others.len()))
            .map_or(self.shader, |&&s| s);
    }
}

const FALLBACK_THEME: Theme = Theme {
    name: "",
    background: theme::Rgb(0.0, 0.0, 0.0),
    stops: &[],
    accent: theme::Rgb::WHITE,
};

/// Effects are `Send` but not `Sync`; with `ResMut` the lock is never contended.
#[derive(Resource, Default)]
struct Transitions(Mutex<EffectManager<()>>);

fn input(
    mut keys: MessageReader<KeyMessage>,
    mut resizes: MessageReader<ResizeMessage>,
    mut show: ResMut<Show>,
    mut exit: MessageWriter<AppExit>,
) {
    for key in keys.read().filter(|key| key.kind != KeyEventKind::Release) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                exit.write_default();
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                exit.write_default();
            }
            KeyCode::Char(' ') | KeyCode::Right | KeyCode::Enter => show.skip = true,
            KeyCode::Char('t') => show.next_theme(),
            KeyCode::Char('s') => show.next_shader(),
            KeyCode::Char('i') => show.caption = !show.caption,
            _ => {}
        }
    }
    if resizes.read().count() > 0 {
        show.stage = None;
    }
}

fn frame(
    mut context: ResMut<RatatuiContext>,
    mut show: ResMut<Show>,
    mut transitions: ResMut<Transitions>,
    fonts: Res<SystemFonts>,
    time: Res<Time>,
) -> Result {
    let show = &mut *show;
    let transitions = transitions
        .0
        .get_mut()
        .unwrap_or_else(PoisonError::into_inner);
    context.draw(|frame| {
        let size = frame.area();
        if show.stage.as_ref().is_none_or(|stage| stage.size != size) {
            // A half-finished transition would play on the old layout: settle it.
            if show.stage.is_some() && show.phase != Phase::Holding {
                transitions.cancel_unique_effect(());
                show.phase = Phase::Holding;
            }
            show.stage = Some(Stage::new(&show.scene, size, screen_aspect()));
            if show.phase == Phase::Entering
                && let Some(stage) = &show.stage
            {
                transitions.add_unique_effect((), fx::enter(stage, show.theme()));
            }
        }

        let skip = std::mem::take(&mut show.skip);
        match show.phase {
            Phase::Entering if skip || !transitions.is_running() => {
                show.phase = Phase::Holding;
                show.hold.reset();
            }
            Phase::Holding if skip || show.hold.tick(time.delta()).is_finished() => {
                show.phase = Phase::Leaving;
                if let Some(stage) = &show.stage {
                    transitions.add_unique_effect((), fx::leave(stage, show.theme()));
                }
            }
            Phase::Leaving if !transitions.is_running() => {
                show.next_scene(&fonts);
                let stage = Stage::new(&show.scene, size, screen_aspect());
                transitions.add_unique_effect((), fx::enter(&stage, show.theme()));
                show.stage = Some(stage);
                show.phase = Phase::Entering;
            }
            _ => {}
        }

        let Some(stage) = &show.stage else { return };
        let theme = show.theme();
        stage.draw(frame.buffer_mut(), theme, show.shader, time.elapsed_secs());
        transitions.process_effects(time.delta().into(), frame.buffer_mut(), size);
        if show.caption {
            let caption = format!(
                "{}  ·  {}  ·  {}",
                show.scene.label(),
                theme.name,
                show.shader.name()
            );
            stage.caption(frame.buffer_mut(), &caption, theme);
        }
    })?;
    Ok(())
}

/// How many times taller than wide a half-cell pixel is. Terminals that
/// don't report their pixel size are assumed to have 1:2 cells.
fn screen_aspect() -> f32 {
    match terminal::window_size() {
        Ok(size) if size.width > 0 && size.height > 0 && size.columns > 0 && size.rows > 0 => {
            let cell_w = size.width as f32 / size.columns as f32;
            let cell_h = size.height as f32 / size.rows as f32;
            (cell_h / 2.0 / cell_w).clamp(0.5, 2.0)
        }
        _ => 1.0,
    }
}
