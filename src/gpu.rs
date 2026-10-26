//! Rendering on the GPU. The CPU lays scenes out and packs them into flat
//! buffers; compute passes (src/shaders) draw the frame, run simulations and
//! post-processing, and turn it into characters; the result is read back
//! each frame for the terminal.
//!
//! Buffers:
//! - params: the frame, both scenes and the transition (every frame; laid
//!   out in `params.rs`)
//! - inputs: glyph tables, the Mandelbrot reference orbit, both scenes' art,
//!   particle and swarm positions (when they change)
//! - state: the scene image, backgrounds, distance fields, bloom,
//!   simulations, swarm particles and flame points; it lives on the GPU
//!   between frames
//! - output: `[cols, rows]`, then a glyph, foreground and background per
//!   cell, then marks the GPU keeps for itself between frames (see
//!   `common.wgsl`)
//! - cover, color: scratch where particles are splatted
//! - hist: hit counts of flames and attractors, fading frame to frame
//! - frames: this frame's final image and the last one, packed to 8 bits
//! - bins: particles counted per 16 x 16 samples, so particle life and boids
//!   can feel their neighbours without visiting every other particle
//!
//! What has to stay in step with the GPU's state (whether a simulation has
//! started, which ping-pong buffer is current) is decided on the GPU, from
//! the marks: a frame the GPU never ran can't put it out of step.
//!
//! With `--profile`, each phase is timed on the GPU and shown in the caption.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use bevy::asset::{RenderAssetUsages, uuid_handle};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};
use bevy::render::gpu_readback::{Readback, ReadbackComplete};
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::binding_types::{
    storage_buffer_read_only_sized, storage_buffer_sized,
};
use bevy::render::render_resource::{
    BindGroup, BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries, Buffer,
    BufferDescriptor, BufferUsages, CachedComputePipelineId, CachedPipelineState,
    ComputePassDescriptor, ComputePipelineDescriptor, PipelineCache, ShaderStages,
};
use bevy::render::renderer::{
    PendingCommandBuffers, RenderContext, RenderDevice, RenderGraph, RenderGraphSystems,
    RenderQueue,
};
use bevy::render::storage::{GpuShaderBuffer, ShaderBuffer};
use bevy::render::{Render, RenderApp, RenderStartup, RenderSystems};
use bevy::shader::Shader;

use crate::backdrop::{self, BACKDROPS, Backdrop};
use crate::glyphs::Glyphs;
use crate::num;
use crate::params::{FROM, H, HEADER, Region, TO, Words};
use crate::profile::{self, Queries, Timings};
use crate::shader::{self, Entry, Watch};
use crate::stage::{Idle, Mode, Stage};
use crate::transition::{self, Crossing};

const SHADER: Handle<Shader> = uuid_handle!("6f0c3b8e-2d4a-4c1e-9b7f-5a8e3d2c1b0a");
const WORKGROUP: u32 = 64;
/// The most workgroups a dispatch may have along one axis.
const MAX_GROUPS: u32 = 65_535;
/// Swarm particles.
const SWARM: u32 = 24_000;
/// Jump-flooding passes for the largest grid: jumps of 2048 down to 1.
const MAX_JUMPS: u32 = 12;
/// Pressure solver iterations for the fluid, per step.
const PRESSURE: u32 = 20;
/// Reaction-diffusion steps per tick.
const REACTION: u32 = 12;
/// Simulation ticks per second, whatever the frame rate, and the most one
/// frame catches up on.
const TICKS: f64 = 60.0;
const MAX_TICKS: u32 = 4;
/// Life steps every this many ticks, so it can be followed.
const LIFE_EVERY: u64 = 3;
/// Flame and attractor points, and the steps each takes per frame.
const FLAME_THREADS: u32 = 65_536;
const FLAME_STEPS: u32 = 24;
/// Particle life's particles: each feels every other, so few.
const LIFE: u32 = 3072;
/// Slime mold agents and crystal walkers.
pub const AGENTS: u32 = 65_536;
/// How much of the flame histogram survives a sixtieth of a second.
const FLAME_FADE: f32 = 0.88;
/// Bloom levels, each half the size of the one before.
const BLOOM_LEVELS: u32 = 3;
/// Words the GPU keeps for itself after the cells in `output`.
const MARKS: usize = 8;

pub struct GpuPlugin {
    pub watch: bool,
    pub profile: bool,
}

impl Plugin for GpuPlugin {
    fn build(&self, app: &mut App) {
        let status = Status::default();
        app.add_plugins(ExtractResourcePlugin::<Job>::default())
            .init_resource::<GpuFrame>()
            .insert_resource(status.clone())
            .add_systems(Startup, setup)
            .add_systems(Update, show_status);
        if self.watch {
            app.insert_resource(Watch::new()).add_systems(Update, watch);
        }
        let timings = Timings::default();
        if self.profile {
            app.insert_resource(timings.clone())
                .add_systems(Update, show_profile);
        }
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        if self.profile {
            render_app
                .insert_resource(timings)
                .add_systems(RenderStartup, init_queries);
        }
        render_app
            .insert_resource(status)
            .add_systems(RenderStartup, init_pipelines)
            .add_systems(
                Render,
                (update_pipelines, prepare_bind_group)
                    .chain()
                    .in_set(RenderSystems::PrepareBindGroups),
            )
            .add_systems(
                RenderGraph,
                (
                    dispatch.in_set(RenderGraphSystems::Render),
                    submit.in_set(RenderGraphSystems::Submit),
                    collect_timings.in_set(RenderGraphSystems::Finish),
                ),
            );
    }
}

/// The latest frame read back from the GPU; the shader's last error when
/// watching for edits; GPU time per phase when profiling.
#[derive(Resource, Default)]
pub struct GpuFrame {
    pub data: Vec<u32>,
    pub shader_error: Option<String>,
    pub profile: Option<String>,
}

impl GpuFrame {
    /// The glyph and colours of each cell, if the frame is `cols` x `rows`.
    pub fn cells(&self, cols: u16, rows: u16) -> Option<&[u32]> {
        let (&c, rest) = self.data.split_first()?;
        let (&r, cells) = rest.split_first()?;
        (c == u32::from(cols) && r == u32::from(rows)).then_some(cells)
    }
}

/// What the render world tells the main world: which shader generation is
/// running, and why the newest one isn't.
#[derive(Resource, Clone, Default)]
struct Status(Arc<Mutex<Report>>);

#[derive(Default)]
struct Report {
    active: u32,
    error: Option<String>,
}

impl Status {
    fn with<T>(&self, f: impl FnOnce(&mut Report) -> T) -> T {
        f(&mut self.0.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

/// A compute pass: an entry point and its pipeline constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Pass {
    entry: &'static str,
    step: u32,
    flip: u32,
}

const fn pass(entry: &'static str) -> Pass {
    Pass {
        entry,
        step: 1,
        flip: 0,
    }
}

const fn flipped(entry: &'static str, flip: u32) -> Pass {
    Pass {
        entry,
        step: 1,
        flip,
    }
}

const fn leveled(entry: &'static str, level: u32, flip: u32) -> Pass {
    Pass {
        entry,
        step: level,
        flip,
    }
}

/// Step `k` of a frame's simulation steps, which alternate buffers.
const fn numbered(entry: &'static str, k: u32) -> Pass {
    Pass {
        entry,
        step: k,
        flip: k % 2,
    }
}

/// A jump of 2^k in the distance field, reading field 0 (flip 0) or 1.
fn jump(k: u32, flip: u32) -> Pass {
    Pass {
        entry: "field_jump",
        step: 1_u32.checked_shl(k).unwrap_or(1),
        flip,
    }
}

/// The jumps for a grid `extent` points across at most: from the largest
/// that still lands inside down to 1, then one more of 1 if that makes the
/// count even, so the last writes field 0 where the seeds began.
fn jumps(extent: u32) -> Vec<Pass> {
    let largest = extent.saturating_sub(1).max(1).ilog2().min(MAX_JUMPS - 1);
    let mut sizes: Vec<u32> = (0..=largest).rev().collect();
    if sizes.len() % 2 == 1 {
        sizes.push(0);
    }
    sizes
        .into_iter()
        .zip([0, 1].into_iter().cycle())
        .map(|(k, flip)| jump(k, flip))
        .collect()
}

/// What a pass is part of, for timing.
fn phase(p: &Pass) -> &'static str {
    match p.entry {
        "clear" | "particles" | "swarm" | "boid_bin" | "boids" => "particles",
        "splat_fade" | "flame" | "life_forces" | "life_move" => "flames",
        "bins_clear" => "particles",
        "field_cover" | "field_seed" | "field_jump" | "field_finish" => "fields",
        "backdrops" => "backdrops",
        "scene" => "scene",
        "bloom_down" | "bloom_blur" => "bloom",
        "post" => "post",
        "encode" | "finish" => "encode",
        _ => "sims",
    }
}

fn all_passes() -> Vec<Pass> {
    let mut passes: Vec<Pass> = [
        "clear",
        "particles",
        "swarm",
        "boid_bin",
        "boids",
        "splat_fade",
        "flame",
        "bins_clear",
        "life_forces",
        "life_move",
        "field_cover",
        "field_seed",
        "field_finish",
        "backdrops",
        "snapshot",
        "fluid_advect",
        "fluid_curl",
        "fluid_vorticity",
        "fluid_divergence",
        "fluid_project",
        "fluid_copy",
        "scene",
        "post",
        "encode",
        "finish",
    ]
    .into_iter()
    .map(pass)
    .collect();
    for k in 0..MAX_JUMPS {
        passes.extend([jump(k, 0), jump(k, 1)]);
    }
    for entry in ["fluid_pressure", "reaction"] {
        passes.extend([flipped(entry, 0), flipped(entry, 1)]);
    }
    for entry in [
        "life",
        "fire",
        "ripple",
        "slime_agents",
        "slime_diffuse",
        "crystal",
    ] {
        passes.extend((0..MAX_TICKS).map(|k| numbered(entry, k)));
    }
    passes.extend((0..MAX_TICKS * 2).map(|k| numbered("sand", k)));
    for level in 0..BLOOM_LEVELS {
        passes.extend([
            leveled("bloom_down", level, 0),
            leveled("bloom_blur", level, 0),
            leveled("bloom_blur", level, 1),
        ]);
    }
    passes
}

/// Every pipeline the program must build, for checking a shader edit.
fn entries() -> Vec<Entry> {
    all_passes()
        .into_iter()
        .map(|p| Entry {
            name: p.entry,
            step: p.step,
            flip: p.flip,
        })
        .collect()
}

/// Handles to the buffers, the shader, and this frame's work.
#[derive(Resource, ExtractResource, Clone)]
#[extract_app(RenderApp)]
struct Job {
    params: Vec<u32>,
    inputs: Handle<ShaderBuffer>,
    output: Handle<ShaderBuffer>,
    cover: Handle<ShaderBuffer>,
    color: Handle<ShaderBuffer>,
    hist: Handle<ShaderBuffer>,
    frames: Handle<ShaderBuffer>,
    bins: Handle<ShaderBuffer>,
    /// Size of the state buffer in vec4s.
    state: u64,
    passes: Vec<(Pass, u32)>,
    /// The newest shader, and its generation.
    shader: Handle<Shader>,
    generation: u32,
}

/// What the GPU already holds, and what the CPU tracks between frames for it.
#[derive(Default)]
pub struct Tracker {
    readback: Option<Entity>,
    size: (u16, u16),
    inputs: Option<(u64, u64, bool, u64)>,
    offsets: Inputs,
    homes: (u64, Vec<u32>),
    orbit: Vec<u32>,
    born: Vec<(u64, f64)>,
    frame: u32,
    /// Seconds of simulation not yet stepped, ticks stepped so far, and
    /// the scene they were for.
    clock: f64,
    ticks: u64,
    sim: u64,
}

/// Where each part of the inputs buffer starts, in words.
#[derive(Default, Clone, Copy)]
struct Inputs {
    ascii: u32,
    orbit: u32,
    orbit_len: u32,
    art: u32,
    points: u32,
    homes: u32,
    homes_count: u32,
    text: u32,
    text_len: u32,
}

/// The CPU's side of the GPU renderer.
#[derive(SystemParam)]
pub struct Gpu<'w, 's> {
    job: ResMut<'w, Job>,
    buffers: ResMut<'w, Assets<ShaderBuffer>>,
    device: Option<Res<'w, RenderDevice>>,
    tracker: Local<'s, Tracker>,
    commands: Commands<'w, 's>,
}

/// What applies to the whole frame.
#[derive(Clone, Copy)]
pub struct Frame {
    /// Seconds for the shaders' clock, which the caller keeps small enough
    /// for f32 to tell frames apart.
    pub t: f32,
    /// Seconds since launch, for scene ages.
    pub now: f64,
    pub dt: f32,
    pub dither: bool,
}

fn buffer(data: Vec<u32>) -> ShaderBuffer {
    // Empty buffers can't be bound.
    let data = if data.is_empty() { vec![0] } else { data };
    ShaderBuffer::new(data, RenderAssetUsages::RENDER_WORLD)
}

fn setup(
    mut commands: Commands,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut shaders: ResMut<Assets<Shader>>,
) {
    let _ = shaders.insert(
        &SHADER,
        Shader::from_wgsl(shader::built_in(), "showcase.wgsl"),
    );
    let mut add = || buffers.add(buffer(vec![]));
    let job = Job {
        params: vec![0],
        inputs: add(),
        output: add(),
        cover: add(),
        color: add(),
        hist: add(),
        frames: add(),
        bins: add(),
        state: 1,
        passes: Vec::new(),
        shader: SHADER,
        generation: 0,
    };
    commands.insert_resource(job);
}

/// Shaders made from edits, by generation, kept until a newer one runs.
#[derive(Default)]
struct Edits(Vec<(u32, Handle<Shader>)>);

/// Reloads the shader when its files change. An edit that doesn't compile
/// is reported and never replaces the running shader; one that does runs
/// once all of its pipelines are built (`update_pipelines`).
fn watch(
    mut watch: ResMut<Watch>,
    mut shaders: ResMut<Assets<Shader>>,
    mut job: ResMut<Job>,
    mut edits: Local<Edits>,
    status: Res<Status>,
) {
    let active = status.with(|r| r.active);
    edits.0.retain(|(generation, _)| *generation >= active);
    let Some(changed) = watch.changed() else {
        return;
    };
    match changed.and_then(|source| shader::validate(&source, &entries()).map(|()| source)) {
        Ok(source) => {
            let handle = shaders.add(Shader::from_wgsl(source, "showcase.wgsl"));
            job.generation = job.generation.wrapping_add(1);
            job.shader = handle.clone();
            edits.0.push((job.generation, handle));
        }
        Err(error) => status.with(|r| r.error = Some(error)),
    }
}

fn show_status(status: Res<Status>, mut frame: ResMut<GpuFrame>) {
    frame.shader_error = status.with(|r| r.error.clone());
}

/// GPU milliseconds per phase, for the caption.
fn show_profile(timings: Res<Timings>, mut frame: ResMut<GpuFrame>) {
    let mut phases = timings
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let total: f64 = phases.iter().map(|(_, ms)| ms).sum();
    phases.sort_by(|a, b| b.1.total_cmp(&a.1));
    let parts: Vec<String> = phases
        .iter()
        .take(7)
        .map(|(name, ms)| format!("{name} {ms:.2}"))
        .collect();
    frame.profile = Some(if phases.is_empty() {
        "gpu timing: waiting (or no timestamp support)".to_owned()
    } else {
        format!("gpu {total:.2} ms: {}", parts.join(", "))
    });
}

fn init_queries(
    mut commands: Commands,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    timings: Res<Timings>,
) {
    if let Some(queries) = Queries::new(&device, &queue, timings.clone()) {
        commands.insert_resource(queries);
    }
}

fn collect_timings(queries: Option<ResMut<Queries>>) {
    if let Some(mut queries) = queries {
        queries.collect();
    }
}

/// Where each region of the state buffer starts, in vec4s.
struct Layout {
    regions: Vec<u32>,
    total: u64,
}

impl Layout {
    fn new(cols: u16, rows: u16) -> Self {
        let (c, r) = (u32::from(cols), u32::from(rows));
        let cells = c.saturating_mul(r);
        let image = cells.saturating_mul(32);
        let grid = cells.saturating_mul(8);
        let level = |l: u32| {
            (c >> l)
                .max(1)
                .saturating_mul((r.saturating_mul(2) >> l).max(1))
        };
        let size = |region: Region| match region {
            Region::Scene => image,
            Region::Bloom0 | Region::Bloom1 => level(0),
            Region::Bloom2 | Region::Bloom3 => level(1),
            Region::Bloom4 | Region::Bloom5 => level(2),
            Region::Field0
            | Region::Field1
            | Region::Sim0
            | Region::Sim1
            | Region::Sim2
            | Region::Sim3
            | Region::Snapshot
            | Region::BgOld
            | Region::BgNew => grid,
            Region::Particles => SWARM,
            Region::Agents => AGENTS,
            Region::Flame => FLAME_THREADS,
        };
        let mut regions = Vec::with_capacity(Region::ALL.len());
        let mut at = 0_u64;
        for &(region, _, _) in Region::ALL {
            regions.push(u32::try_from(at).unwrap_or(u32::MAX));
            at = at.saturating_add(u64::from(size(region)));
        }
        Self { regions, total: at }
    }
}

/// Words in the bins buffer: 17 (`BIN_WORDS`) for each 16 x 16 samples,
/// that is 4 x 2 cells.
fn bin_words(cols: u16, rows: u16) -> usize {
    usize::from(cols.div_ceil(4))
        .saturating_mul(usize::from(rows.div_ceil(2)))
        .saturating_mul(17)
}

/// Sand works on 2 x 2 blocks of the grid, shifted by one every other
/// step: a spare row and column of blocks for the shifted ones.
fn sand_blocks(cols: u16, rows: u16) -> u32 {
    u32::from(cols)
        .saturating_add(1)
        .saturating_mul(u32::from(rows).saturating_mul(2).saturating_add(1))
}

/// Bytes each buffer needs for `cells` cells: state, hist, frames, cover.
fn sizes(state: u64, cells: usize) -> [u64; 4] {
    let cells = u64::try_from(cells).unwrap_or(u64::MAX);
    [
        state.saturating_mul(16),
        cells.saturating_mul(64 * 4),
        cells.saturating_mul(64 * 4),
        cells.saturating_mul(32 * 4),
    ]
}

impl Gpu<'_, '_> {
    fn replace(&mut self, handle: &Handle<ShaderBuffer>, data: Vec<u32>) {
        // Uploaded buffers leave the main world, so each change replaces them.
        let _ = self.buffers.insert(handle, buffer(data));
    }

    /// When the stage `id` first appeared.
    fn born(&mut self, id: u64, now: f64) -> f64 {
        if let Some(&(_, born)) = self.tracker.born.iter().find(|(i, _)| *i == id) {
            return born;
        }
        self.tracker.born.push((id, now));
        now
    }

    /// Why a `cols` x `rows` frame can't be drawn on this GPU, if it can't.
    fn too_big(&self, layout: &Layout, cells: usize) -> Option<String> {
        let limits = self.device.as_ref()?.limits();
        let binding = limits
            .max_storage_buffer_binding_size
            .min(limits.max_buffer_size);
        let needed = sizes(layout.total, cells).into_iter().max().unwrap_or(0);
        let mib = |bytes: u64| bytes / (1 << 20);
        (needed > binding).then(|| {
            format!(
                "This terminal is too large for the GPU: it needs a {} MiB buffer and the GPU allows {} MiB. Try a larger font.",
                mib(needed),
                mib(binding)
            )
        })
    }

    /// Queues a frame: `to` at rest, or `crossing` from its old scene to
    /// `to`. Returns why it can't, if the GPU can't draw a frame this size.
    pub fn render(
        &mut self,
        glyphs: &Glyphs,
        to: &Stage,
        crossing: Option<&Crossing>,
        frame: Frame,
    ) -> Option<String> {
        let from = crossing.map_or(to, |c| &c.from);
        let (cols, rows) = (to.cols, to.rows);
        let cells = usize::from(cols).saturating_mul(usize::from(rows));
        let layout = Layout::new(cols, rows);
        if let Some(reason) = self.too_big(&layout, cells) {
            self.job.passes.clear();
            self.job.state = 1;
            return Some(reason);
        }
        // The scene a simulation, the swarm or the flames belong to: the new
        // one's, else the old one's while it still shows.
        let owner = |wants: fn(&Stage) -> bool| {
            if wants(to) {
                Some((to, TO))
            } else if crossing.is_some() && wants(from) {
                Some((from, FROM))
            } else {
                None
            }
        };
        // The simulation: a transition's own, else the scene's that owns it,
        // with its mode and an id that changes when it must start afresh.
        let transit = crossing.and_then(|c| c.kind.simulation());
        let sim: Option<(Mode, usize, u64)> = match transit {
            Some(mode) => Some((mode, TO, (from.id << 32 | to.id) | 1 << 63)),
            None => owner(|s| s.act.mode.simulates()).map(|(s, slot)| (s.act.mode, slot, s.id)),
        };
        let swarm = owner(|s| s.act.mode.flocks());
        let splat = owner(|s| s.act.backdrop.splatted());
        self.upload_inputs(glyphs, from, to, crossing, swarm.map(|(s, _)| s));
        self.resize(cols, rows);
        let (born_from, born_to) = (self.born(from.id, frame.now), self.born(to.id, frame.now));
        self.tracker
            .born
            .retain(|(id, _)| *id == from.id || *id == to.id);

        let tracker = &mut *self.tracker;
        tracker.frame = tracker.frame.wrapping_add(1);
        // Fixed steps, so simulations run at the same speed at any frame rate.
        tracker.clock += f64::from(frame.dt.min(0.25));
        let mut ticks = num::floor_u32(num::narrow(tracker.clock * TICKS)).min(MAX_TICKS);
        tracker.clock = (tracker.clock - f64::from(ticks) / TICKS).min(1.0 / TICKS);
        let fresh = sim.is_some_and(|(_, _, id)| id != tracker.sim);
        if fresh {
            // A new simulation starts this frame, whatever the clock says.
            ticks = ticks.max(1);
            tracker.ticks = 0;
        }
        let before = tracker.ticks;
        tracker.ticks = tracker.ticks.saturating_add(u64::from(ticks));
        let grid = num::u32_of(cells.saturating_mul(8));
        let mut sims = Vec::new();
        let mut steps = 0;
        if let Some((mode, _, id)) = sim {
            tracker.sim = id;
            let stepped =
                |entry: &'static str, n: u32| (0..n).map(move |k| (flipped(entry, k % 2), grid));
            let numbered_steps = |entry: &'static str, n: u32, threads: u32| {
                (0..n).map(move |k| (numbered(entry, k), threads))
            };
            match mode {
                Mode::Fluid | Mode::Ink => {
                    for _ in 0..ticks {
                        sims.extend(
                            [
                                "fluid_advect",
                                "fluid_curl",
                                "fluid_vorticity",
                                "fluid_divergence",
                            ]
                            .map(|e| (pass(e), grid)),
                        );
                        sims.extend(stepped("fluid_pressure", PRESSURE));
                        sims.extend([(pass("fluid_project"), grid), (pass("fluid_copy"), grid)]);
                    }
                    steps = ticks;
                }
                Mode::Reaction => {
                    steps = ticks.saturating_mul(REACTION);
                    sims.extend(stepped("reaction", steps));
                }
                Mode::Life => {
                    let due = (tracker.ticks / LIFE_EVERY).saturating_sub(before / LIFE_EVERY);
                    steps = u32::try_from(due)
                        .unwrap_or(0)
                        .max(u32::from(fresh))
                        .min(MAX_TICKS);
                    sims.extend(numbered_steps("life", steps, grid));
                }
                Mode::Fire => {
                    steps = ticks.min(MAX_TICKS);
                    sims.extend(numbered_steps("fire", steps, grid));
                }
                Mode::Ripple => {
                    steps = ticks.min(MAX_TICKS);
                    sims.extend(numbered_steps("ripple", steps, grid));
                }
                Mode::Slime => {
                    // Each step: the agents move and leave trails, then the
                    // trails spread and fade.
                    steps = ticks.min(MAX_TICKS);
                    for k in 0..steps {
                        sims.extend([
                            (numbered("slime_agents", k), AGENTS),
                            (numbered("slime_diffuse", k), grid),
                        ]);
                    }
                }
                Mode::Sand => {
                    // Two steps a tick, each on 2 x 2 blocks of the grid.
                    steps = ticks.saturating_mul(2).min(MAX_TICKS * 2);
                    sims.extend(numbered_steps("sand", steps, sand_blocks(cols, rows)));
                }
                Mode::Crystal => {
                    steps = ticks.min(MAX_TICKS);
                    // Its threads also reset the crystal grid.
                    sims.extend(numbered_steps("crystal", steps, AGENTS.max(grid)));
                }
                Mode::Flat | Mode::Turntable | Mode::Swarm | Mode::Boids => {}
            }
        }

        let particles = crossing.map_or(0, Crossing::threads);
        let offsets = tracker.offsets;
        let (sources, targets) = crossing.map_or((0, 0), Crossing::counts);
        // Distance fields can be kept while nothing moves the art.
        let still = crossing.is_none() && to.act.idle == Idle::Still;
        let low = |id: u64| u32::try_from(id & 0xffff_ffff).unwrap_or(0);
        let mut header = Words::new(HEADER);
        let mut put = |slot: H, value: u32| header.put(slot.at(), value);
        let code = |stage: &Stage| {
            num::u32_of(
                BACKDROPS
                    .iter()
                    .position(|&b| b == stage.act.backdrop)
                    .unwrap_or(0),
            )
        };
        let block = num::u32_of;
        put(H::Cols, u32::from(cols));
        put(H::Rows, u32::from(rows));
        put(H::Time, frame.t.to_bits());
        put(H::Aspect, to.screen_aspect().to_bits());
        put(H::Look, to.act.look.code());
        put(H::Glyphs, glyphs.len());
        put(H::Kind, crossing.map_or(0, |c| c.kind.code()));
        put(
            H::Progress,
            crossing.map_or(1.0, Crossing::progress).to_bits(),
        );
        put(H::Grain, crossing.map_or(1.0, |c| c.grain).to_bits());
        put(H::Sources, sources);
        put(H::Targets, targets);
        put(H::Frame, tracker.frame);
        put(H::Dt, frame.dt.to_bits());
        put(H::Post, to.act.post.code());
        put(H::Dither, u32::from(frame.dither));
        put(H::Swarm, if swarm.is_some() { SWARM } else { 0 });
        put(H::SwarmScene, swarm.map_or(0, |(_, slot)| block(slot)));
        put(H::SimScene, sim.map_or(0, |(_, slot, _)| block(slot)));
        put(H::SimId, sim.map_or(0, |(_, _, id)| low(id ^ id >> 32)));
        put(H::SimSteps, steps);
        put(H::FieldKey, if still { low(to.id) } else { 0 });
        put(
            H::SplatId,
            splat.map_or(0, |(s, _)| code(s).saturating_add(1)),
        );
        put(H::Fade, FLAME_FADE.powf(frame.dt.min(0.1) * 60.0).to_bits());
        put(H::InGlyphs, 0);
        put(H::InAscii, offsets.ascii);
        put(H::AsciiCount, glyphs.ascii_len());
        put(H::InPoints, offsets.points);
        put(H::InHomes, offsets.homes);
        put(H::Homes, offsets.homes_count);
        put(H::InArt, offsets.art);
        put(H::InOrbit, offsets.orbit);
        put(H::OrbitLen, offsets.orbit_len);
        put(
            H::FlameThreads,
            match splat {
                Some((s, _)) if s.act.backdrop == Backdrop::ParticleLife => LIFE,
                Some(_) => FLAME_THREADS,
                None => 0,
            },
        );
        let living = splat.is_some_and(|(s, _)| s.act.backdrop == Backdrop::ParticleLife);
        put(H::FlameSteps, if living { 4 } else { FLAME_STEPS });
        put(H::InText, offsets.text);
        put(H::TextLen, offsets.text_len);
        for (i, &start) in layout.regions.iter().enumerate() {
            header.put(H::Regions.at().saturating_add(i), start);
        }
        let mut params = header.into_vec();
        let from_len = if from.id == to.id {
            0
        } else {
            num::u32_of(from.texels_len())
        };
        let age = |born: f64| num::narrow(frame.now - born);
        params.extend(from.block(0, age(born_from)));
        params.extend(to.block(from_len, age(born_to)));

        let image = num::u32_of(cells.saturating_mul(32));
        let cell_count = num::u32_of(cells);
        let mut passes = Vec::new();
        if particles > 0 || swarm.is_some() {
            passes.push((pass("clear"), cell_count));
        }
        if particles > 0 {
            passes.push((pass("particles"), particles));
        }
        match swarm {
            Some((s, _)) if s.act.mode == Mode::Boids => passes.extend([
                (pass("bins_clear"), num::u32_of(bin_words(cols, rows))),
                (pass("boid_bin"), SWARM),
                (pass("boids"), SWARM),
            ]),
            Some(_) => passes.push((pass("swarm"), SWARM)),
            None => {}
        }
        let living = splat.is_some_and(|(s, _)| s.act.backdrop == Backdrop::ParticleLife);
        if living {
            passes.extend([
                (pass("splat_fade"), image),
                (pass("life_forces"), LIFE),
                (pass("life_move"), LIFE),
            ]);
        } else if splat.is_some() {
            passes.extend([(pass("splat_fade"), image), (pass("flame"), FLAME_THREADS)]);
        }
        let extent = u32::from(cols)
            .saturating_mul(2)
            .max(u32::from(rows).saturating_mul(4));
        passes.extend([(pass("field_cover"), grid), (pass("field_seed"), grid)]);
        passes.extend(jumps(extent).into_iter().map(|p| (p, grid)));
        passes.extend([(pass("field_finish"), grid), (pass("backdrops"), grid)]);
        // The old scene's simulation, frozen as the new one's starts.
        if crossing.is_some()
            && from.act.mode.simulates()
            && sim.is_some_and(|(_, _, id)| id != from.id)
        {
            passes.push((pass("snapshot"), grid));
        }
        passes.extend(sims);
        passes.push((pass("scene"), image));
        if to.act.post.blooms() {
            for level in 0..BLOOM_LEVELS {
                let size = (u32::from(cols) >> level)
                    .max(1)
                    .saturating_mul((u32::from(rows).saturating_mul(2) >> level).max(1));
                passes.extend([
                    (leveled("bloom_down", level, 0), size),
                    (leveled("bloom_blur", level, 0), size),
                    (leveled("bloom_blur", level, 1), size),
                ]);
            }
        }
        passes.extend([
            (pass("post"), image),
            (pass("encode"), cell_count),
            (pass("finish"), 1),
        ]);
        self.job.params = params;
        self.job.passes = passes;
        self.job.state = layout.total;
        None
    }

    /// Rebuilds the inputs buffer when the art, particles or swarm change.
    fn upload_inputs(
        &mut self,
        glyphs: &Glyphs,
        from: &Stage,
        to: &Stage,
        crossing: Option<&Crossing>,
        swarm: Option<&Stage>,
    ) {
        if let Some(stage) = swarm
            && self.tracker.homes.0 != stage.id
        {
            let homes = transition::homes(stage, usize::try_from(SWARM).unwrap_or(usize::MAX));
            self.tracker.homes = (stage.id, homes);
        }
        if self.tracker.orbit.is_empty() {
            self.tracker.orbit = backdrop::reference_orbit();
        }
        let key = (
            from.id,
            to.id,
            crossing.is_some(),
            swarm.map_or(0, |s| s.id),
        );
        if self.tracker.inputs == Some(key) {
            return;
        }
        self.tracker.inputs = Some(key);
        let mut data = glyphs.pack();
        let ascii = num::u32_of(data.len());
        data.extend(glyphs.pack_ascii());
        let text = num::u32_of(data.len());
        let readme = Glyphs::pack_text();
        let text_len = num::u32_of(readme.len());
        data.extend(readme);
        let orbit = num::u32_of(data.len());
        data.extend_from_slice(&self.tracker.orbit);
        let art = num::u32_of(data.len());
        data.extend(from.texels());
        if from.id != to.id {
            data.extend(to.texels());
        }
        let points = num::u32_of(data.len());
        if let Some(crossing) = crossing {
            data.extend(crossing.particles());
        }
        let homes = num::u32_of(data.len());
        let homes_count = if swarm.is_some() {
            data.extend_from_slice(&self.tracker.homes.1);
            num::u32_of(self.tracker.homes.1.len() / 3)
        } else {
            0
        };
        let orbit_len = num::u32_of(self.tracker.orbit.len() / 2);
        self.tracker.offsets = Inputs {
            ascii,
            orbit,
            orbit_len,
            art,
            points,
            homes,
            homes_count,
            text,
            text_len,
        };
        let handle = self.job.inputs.clone();
        self.replace(&handle, data);
    }

    /// Makes the per-cell buffers and the readback fit `cells` cells.
    fn resize(&mut self, cols: u16, rows: u16) {
        let cells = usize::from(cols).saturating_mul(usize::from(rows));
        if (cols, rows) == self.tracker.size && self.tracker.readback.is_some() {
            return;
        }
        self.tracker.size = (cols, rows);
        let output = self.buffers.add(buffer(vec![
            0;
            cells
                .saturating_mul(3)
                .saturating_add(2)
                .saturating_add(MARKS)
        ]));
        self.job.output = output.clone();
        self.job.cover = self.buffers.add(buffer(vec![0; cells.saturating_mul(32)]));
        self.job.color = self.buffers.add(buffer(vec![0; cells.saturating_mul(8)]));
        self.job.hist = self.buffers.add(buffer(vec![0; cells.saturating_mul(64)]));
        self.job.frames = self.buffers.add(buffer(vec![0; cells.saturating_mul(64)]));
        self.job.bins = self.buffers.add(buffer(vec![0; bin_words(cols, rows)]));
        if let Some(old) = self.tracker.readback.take() {
            self.commands.entity(old).despawn();
        }
        let entity = self
            .commands
            .spawn(Readback::buffer(output))
            .observe(|event: On<ReadbackComplete>, mut frame: ResMut<GpuFrame>| {
                frame.data = event.to_shader_type();
            })
            .id();
        self.tracker.readback = Some(entity);
    }
}

/// The pipelines of one shader generation.
struct Set {
    generation: u32,
    ids: HashMap<Pass, CachedComputePipelineId>,
}

#[derive(Resource)]
struct Pipelines {
    layout: BindGroupLayoutDescriptor,
    /// The pipelines frames are drawn with, those of a newer shader being
    /// built, and the generation that failed to build.
    active: Option<Set>,
    building: Option<Set>,
    failed: Option<u32>,
}

impl Pipelines {
    fn queue(&self, cache: &PipelineCache, shader: &Handle<Shader>, generation: u32) -> Set {
        let ids = all_passes()
            .into_iter()
            .map(|p| {
                let id = cache.queue_compute_pipeline(ComputePipelineDescriptor {
                    label: Some(p.entry.into()),
                    layout: vec![self.layout.clone()],
                    shader: shader.clone(),
                    entry_point: Some(p.entry.into()),
                    constants: vec![
                        ("STEP".into(), f64::from(p.step)),
                        ("FLIP".into(), f64::from(p.flip)),
                    ],
                    ..default()
                });
                (p, id)
            })
            .collect();
        Set { generation, ids }
    }
}

fn init_pipelines(mut commands: Commands) {
    let read = || storage_buffer_read_only_sized(false, None);
    let write = || storage_buffer_sized(false, None);
    let layout = BindGroupLayoutDescriptor::new(
        "showcase",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (
                read(),
                read(),
                write(),
                write(),
                write(),
                write(),
                write(),
                write(),
                write(),
            ),
        ),
    );
    commands.insert_resource(Pipelines {
        layout,
        active: None,
        building: None,
        failed: None,
    });
}

/// Builds the pipelines of a new shader beside the running ones, and
/// switches to them once every one is ready; if one fails, the running
/// ones stay and the error is reported.
fn update_pipelines(
    mut pipelines: ResMut<Pipelines>,
    cache: Res<PipelineCache>,
    job: Option<Res<Job>>,
    status: Res<Status>,
) {
    let Some(job) = job else { return };
    let newest = job.generation;
    let known = |set: &Option<Set>| set.as_ref().is_some_and(|s| s.generation == newest);
    if !known(&pipelines.active) && !known(&pipelines.building) && pipelines.failed != Some(newest)
    {
        let set = pipelines.queue(&cache, &job.shader, newest);
        pipelines.building = Some(set);
    }
    let Some(building) = &pipelines.building else {
        return;
    };
    let mut ready = true;
    for &id in building.ids.values() {
        match cache.get_compute_pipeline_state(id) {
            CachedPipelineState::Ok(_) => {}
            CachedPipelineState::Queued | CachedPipelineState::Creating(_) => ready = false,
            CachedPipelineState::Err(error) => {
                let message = error.to_string();
                let generation = building.generation;
                status.with(|r| r.error = Some(message));
                pipelines.failed = Some(generation);
                pipelines.building = None;
                return;
            }
        }
    }
    if ready {
        let generation = building.generation;
        pipelines.active = pipelines.building.take();
        status.with(|r| {
            r.active = generation;
            r.error = None;
        });
    }
}

/// Buffers that live in the render world: the params, rewritten every frame,
/// and the state, kept while its size fits.
#[derive(Resource, Clone)]
struct Owned {
    params: Buffer,
    state: Buffer,
}

#[derive(Resource)]
struct Bound(BindGroup);

fn prepare_bind_group(
    mut commands: Commands,
    pipelines: Res<Pipelines>,
    (device, queue, cache): (Res<RenderDevice>, Res<RenderQueue>, Res<PipelineCache>),
    job: Option<Res<Job>>,
    owned: Option<Res<Owned>>,
    buffers: Res<RenderAssets<GpuShaderBuffer>>,
) {
    let Some(job) = job else { return };
    let bytes: &[u8] = bytemuck::cast_slice(&job.params);
    let params_size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    let state_size = job.state.saturating_mul(16).max(16);
    let make = |label: &'static str, size: u64| {
        device.create_buffer(&BufferDescriptor {
            label: Some(label),
            size,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    };
    let owned = match owned {
        Some(owned) if owned.params.size() >= params_size && owned.state.size() == state_size => {
            owned.clone()
        }
        _ => {
            let fresh = Owned {
                params: make("showcase params", params_size),
                state: make("showcase state", state_size),
            };
            commands.insert_resource(fresh.clone());
            fresh
        }
    };
    queue.write_buffer(&owned.params, 0, bytes);
    let get = |handle: &Handle<ShaderBuffer>| {
        buffers
            .get(handle)
            .map(|b| b.buffer.as_entire_buffer_binding())
    };
    let (
        Some(inputs),
        Some(output),
        Some(cover),
        Some(color),
        Some(hist),
        Some(frames),
        Some(bins),
    ) = (
        get(&job.inputs),
        get(&job.output),
        get(&job.cover),
        get(&job.color),
        get(&job.hist),
        get(&job.frames),
        get(&job.bins),
    )
    else {
        commands.remove_resource::<Bound>();
        return;
    };
    let bind_group = device.create_bind_group(
        "showcase",
        &cache.get_bind_group_layout(&pipelines.layout),
        &BindGroupEntries::sequential((
            owned.params.as_entire_buffer_binding(),
            inputs,
            owned.state.as_entire_buffer_binding(),
            output,
            cover,
            color,
            hist,
            frames,
            bins,
        )),
    );
    commands.insert_resource(Bound(bind_group));
}

/// Workgroups for `threads` threads, spread over two axes when one would
/// hold more than a GPU allows; shaders number threads row by row.
fn groups(threads: u32) -> (u32, u32) {
    let total = threads.div_ceil(WORKGROUP);
    let x = total.min(MAX_GROUPS);
    (x, total.div_ceil(x.max(1)))
}

fn dispatch(
    mut context: RenderContext,
    cache: Res<PipelineCache>,
    pipelines: Res<Pipelines>,
    bound: Option<Res<Bound>>,
    job: Option<Res<Job>>,
    mut queries: Option<ResMut<Queries>>,
) {
    let (Some(bound), Some(job), Some(active)) = (bound, job, &pipelines.active) else {
        return;
    };
    // Until every pass this frame needs is ready, keep the last frame.
    let ready: Option<Vec<_>> = job
        .passes
        .iter()
        .map(|(p, threads)| {
            let pipeline = active
                .ids
                .get(p)
                .and_then(|&id| cache.get_compute_pipeline(id))?;
            Some((phase(p), pipeline, *threads))
        })
        .collect();
    let Some(ready) = ready else { return };
    let encoder = context.command_encoder();
    // One compute pass per phase, timed when profiling.
    let mut names = Vec::new();
    let mut rest = ready.as_slice();
    while let Some(&(name, _, _)) = rest.first() {
        let count = rest.iter().take_while(|(p, _, _)| *p == name).count();
        let (group, tail) = rest.split_at_checked(count).unwrap_or((rest, &[]));
        rest = tail;
        let index = num::u32_of(names.len());
        let timed = queries.as_ref().filter(|_| index < profile::PHASES);
        let timestamp_writes = timed.map(|q| wgpu::ComputePassTimestampWrites {
            query_set: &q.set,
            beginning_of_pass_write_index: Some(index.saturating_mul(2)),
            end_of_pass_write_index: Some(index.saturating_mul(2).saturating_add(1)),
        });
        if timed.is_some() {
            names.push(name);
        }
        let mut compute = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some(name),
            timestamp_writes,
        });
        compute.set_bind_group(0, &bound.0, &[]);
        for (_, pipeline, threads) in group {
            let (x, y) = groups(*threads);
            if x > 0 {
                compute.set_pipeline(pipeline);
                compute.dispatch_workgroups(x, y, 1);
            }
        }
    }
    if let Some(queries) = queries.as_mut() {
        queries.resolve(encoder, names);
    }
}

/// Sends the recorded passes to the GPU. Bevy's core pipeline does this for
/// cameras; without cameras, nothing else would.
fn submit(mut pending: ResMut<PendingCommandBuffers>, queue: Res<RenderQueue>) {
    queue.submit(pending.finish());
}

/// Writes a frame read back from the GPU into the terminal buffer.
pub fn draw(cells: &[u32], buf: &mut ratatui::buffer::Buffer) {
    let area = buf.area;
    let positions = (0..area.height)
        .flat_map(|y| (0..area.width).map(move |x| ratatui::layout::Position::new(x, y)));
    for (position, cell) in positions.zip(cells.as_chunks::<3>().0) {
        let [glyph, fg, bg] = *cell;
        let rgb = |c: u32| {
            let [_, r, g, b] = c.to_be_bytes();
            ratatui::style::Color::Rgb(r, g, b)
        };
        if let Some(target) = buf.cell_mut(position) {
            target
                .set_char(char::from_u32(glyph).unwrap_or(' '))
                .set_fg(rgb(fg))
                .set_bg(rgb(bg));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jumps_end_in_the_first_field_buffer() {
        // The seed writes field 0, so the last jump must too: an even count,
        // each reading what the one before wrote, ending with a jump of 1.
        for extent in [1, 2, 3, 64, 320, 1000, 4096, 100_000] {
            let all = jumps(extent);
            assert!(all.len().is_multiple_of(2), "{extent}");
            assert_eq!(all.first().map(|p| p.flip), Some(0));
            assert_eq!(all.last().map(|p| p.step), Some(1));
            assert!(all.iter().all(|p| p.step < extent.max(2)), "{extent}");
            assert!(
                all.iter()
                    .zip(all.iter().skip(1))
                    .all(|(a, b)| a.flip != b.flip)
            );
        }
    }

    #[test]
    fn state_regions_follow_each_other() {
        let layout = Layout::new(120, 40);
        let starts = &layout.regions;
        assert!(starts.iter().zip(starts.iter().skip(1)).all(|(a, b)| a < b));
        assert_eq!(
            layout
                .regions
                .last()
                .map(|&r| u64::from(r) + u64::from(FLAME_THREADS)),
            Some(layout.total)
        );
    }

    #[test]
    fn huge_grids_dispatch_in_two_dimensions() {
        assert_eq!(groups(64), (1, 1));
        let (x, y) = groups(640 * 216 * 32);
        assert!(x <= MAX_GROUPS && x * y * WORKGROUP >= 640 * 216 * 32);
    }

    #[test]
    fn program_and_every_pipeline_compile() {
        let result = shader::validate(&shader::built_in(), &entries());
        assert!(result.is_ok(), "{}", result.err().unwrap_or_default());
    }
}
