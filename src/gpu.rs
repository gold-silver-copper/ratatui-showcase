//! Rendering on the GPU. The CPU lays scenes out and packs them into flat
//! buffers; compute passes (src/shaders) draw the frame, run simulations and
//! post-processing, and turn it into characters; the result is read back
//! each frame for the terminal.
//!
//! Buffers:
//! - params: the frame, both scenes and the transition (every frame)
//! - inputs: glyph tables, the Mandelbrot reference orbit, both scenes' art,
//!   particle and swarm positions (when they change)
//! - state: images, backgrounds, distance fields, bloom, simulations, swarm
//!   particles and flame points; it lives on the GPU between frames
//! - output: `[cols, rows]` then a glyph, foreground and background per cell
//! - cover, color: scratch where particles are splatted
//! - hist: hit counts of flames and attractors, fading frame to frame
//!
//! With `--profile`, each phase is timed on the GPU and shown in the caption.

use std::collections::HashMap;

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
    BufferDescriptor, BufferUsages, CachedComputePipelineId, ComputePassDescriptor,
    ComputePipelineDescriptor, PipelineCache, ShaderStages,
};
use bevy::render::renderer::{
    PendingCommandBuffers, RenderContext, RenderDevice, RenderGraph, RenderGraphSystems,
    RenderQueue,
};
use bevy::render::storage::{GpuShaderBuffer, ShaderBuffer};
use bevy::render::{Render, RenderApp, RenderStartup, RenderSystems};
use bevy::shader::Shader;

use crate::backdrop;
use crate::glyphs::Glyphs;
use crate::num;
use crate::profile::{self, Queries, Timings};
use crate::shader::{self, Watch};
use crate::stage::{Mode, Stage};
use crate::transition::{self, Crossing};

const SHADER: Handle<Shader> = uuid_handle!("6f0c3b8e-2d4a-4c1e-9b7f-5a8e3d2c1b0a");
const WORKGROUP: u32 = 64;
/// Swarm particles.
const SWARM: u32 = 24_000;
/// Words before the first scene's block in `params`.
const HEADER: usize = 64;
/// Jump-flooding passes: jumps of 2048 down to 1.
const JUMPS: u32 = 12;
/// Pressure solver iterations for the fluid.
const PRESSURE: u32 = 20;
/// Reaction-diffusion steps per frame.
const REACTION: u32 = 12;
/// Flame and attractor points, and the steps each takes per frame.
const FLAME_THREADS: u32 = 65_536;
const FLAME_STEPS: u32 = 24;
/// Bloom levels, each half the size of the one before.
const BLOOM_LEVELS: u32 = 3;

pub struct GpuPlugin {
    pub watch: bool,
    pub profile: bool,
}

impl Plugin for GpuPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ExtractResourcePlugin::<Job>::default())
            .init_resource::<GpuFrame>()
            .add_systems(Startup, setup);
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
            .add_systems(RenderStartup, init_pipelines)
            .add_systems(
                Render,
                prepare_bind_group.in_set(RenderSystems::PrepareBindGroups),
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

/// Jump `k` of the distance field: a jump of 2^k. Jumps run from the
/// largest down, each reading the buffer the previous one wrote.
fn jump(k: u32) -> Pass {
    let order = JUMPS.saturating_sub(1).saturating_sub(k);
    Pass {
        entry: "field_jump",
        step: 1_u32.checked_shl(k).unwrap_or(1),
        flip: order % 2,
    }
}

/// What a pass is part of, for timing.
fn phase(p: &Pass) -> &'static str {
    match p.entry {
        "clear" | "particles" | "swarm" => "particles",
        "splat_fade" | "flame" => "flames",
        "field_seed" | "field_jump" | "field_finish" => "fields",
        "backdrops" => "backdrops",
        "scene" => "scene",
        "bloom_down" | "bloom_blur" => "bloom",
        "post" => "post",
        "encode" => "encode",
        _ => "sims",
    }
}

fn all_passes() -> Vec<Pass> {
    let mut passes: Vec<Pass> = [
        "clear",
        "particles",
        "swarm",
        "splat_fade",
        "flame",
        "field_seed",
        "field_finish",
        "backdrops",
        "fluid_advect",
        "fluid_curl",
        "fluid_vorticity",
        "fluid_divergence",
        "fluid_project",
        "fluid_copy",
        "scene",
        "post",
        "encode",
    ]
    .into_iter()
    .map(pass)
    .collect();
    passes.extend((0..JUMPS).map(jump));
    for entry in ["fluid_pressure", "reaction", "life", "fire", "ripple"] {
        passes.extend([flipped(entry, 0), flipped(entry, 1)]);
    }
    for level in 0..BLOOM_LEVELS {
        passes.extend([
            leveled("bloom_down", level, 0),
            leveled("bloom_blur", level, 0),
            leveled("bloom_blur", level, 1),
        ]);
    }
    passes
}

/// Handles to the buffers, and this frame's work.
#[derive(Resource, ExtractResource, Clone)]
#[extract_app(RenderApp)]
struct Job {
    params: Vec<u32>,
    inputs: Handle<ShaderBuffer>,
    output: Handle<ShaderBuffer>,
    cover: Handle<ShaderBuffer>,
    color: Handle<ShaderBuffer>,
    hist: Handle<ShaderBuffer>,
    /// Size of the state buffer in vec4s.
    state: u64,
    passes: Vec<(Pass, u32)>,
}

/// What the GPU already holds, and what the CPU tracks between frames for it.
#[derive(Default)]
pub struct Tracker {
    readback: Option<Entity>,
    cells: usize,
    inputs: Option<(u64, u64, bool, u64)>,
    offsets: Inputs,
    homes: (u64, Vec<u32>),
    orbit: Vec<u32>,
    born: Vec<(u64, f32)>,
    frame: u32,
    last_to: u64,
    ticks: u32,
    flip: u32,
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
}

/// The CPU's side of the GPU renderer.
#[derive(SystemParam)]
pub struct Gpu<'w, 's> {
    job: ResMut<'w, Job>,
    buffers: ResMut<'w, Assets<ShaderBuffer>>,
    tracker: Local<'s, Tracker>,
    commands: Commands<'w, 's>,
}

/// What applies to the whole frame.
#[derive(Clone, Copy)]
pub struct Frame {
    pub t: f32,
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
        state: 1,
        passes: Vec::new(),
    };
    commands.insert_resource(job);
}

/// Reloads the shader when its files change; a broken edit keeps the
/// working shader and reports the error.
fn watch(
    mut watch: ResMut<Watch>,
    mut shaders: ResMut<Assets<Shader>>,
    mut frame: ResMut<GpuFrame>,
) {
    let Some(changed) = watch.changed() else {
        return;
    };
    match changed.and_then(|source| shader::validate(&source).map(|()| source)) {
        Ok(source) => {
            let _ = shaders.insert(&SHADER, Shader::from_wgsl(source, "showcase.wgsl"));
            frame.shader_error = None;
        }
        Err(error) => frame.shader_error = Some(error),
    }
}

/// GPU milliseconds per phase, for the caption.
fn show_profile(timings: Res<Timings>, mut frame: ResMut<GpuFrame>) {
    let mut phases = timings
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
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
    regions: [u32; REGIONS],
    total: u64,
}

/// Regions: scene image, two final images (this frame's and the last), bloom
/// level 0 (two), two distance-field buffers, four simulation buffers, the
/// swarm, old and new backgrounds, bloom levels 1 and 2 (two each), flames.
const REGIONS: usize = 19;

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
        let (b0, b1, b2) = (level(0), level(1), level(2));
        let sizes = [
            image,
            image,
            image,
            b0,
            b0,
            grid,
            grid,
            grid,
            grid,
            grid,
            grid,
            SWARM,
            grid,
            grid,
            b1,
            b1,
            b2,
            b2,
            FLAME_THREADS,
        ];
        let mut regions = [0; REGIONS];
        let mut at = 0_u32;
        for (slot, size) in regions.iter_mut().zip(sizes) {
            *slot = at;
            at = at.saturating_add(size);
        }
        Self {
            regions,
            total: u64::from(at),
        }
    }

    fn region(&self, i: usize) -> u32 {
        self.regions.get(i).copied().unwrap_or(0)
    }
}

impl Gpu<'_, '_> {
    fn replace(&mut self, handle: &Handle<ShaderBuffer>, data: Vec<u32>) {
        // Uploaded buffers leave the main world, so each change replaces them.
        let _ = self.buffers.insert(handle, buffer(data));
    }

    /// When the stage `id` first appeared.
    fn born(&mut self, id: u64, t: f32) -> f32 {
        if let Some(&(_, born)) = self.tracker.born.iter().find(|(i, _)| *i == id) {
            return born;
        }
        self.tracker.born.push((id, t));
        t
    }

    /// Queues a frame: `to` at rest, or `crossing` from its old scene to `to`.
    pub fn render(
        &mut self,
        glyphs: &Glyphs,
        to: &Stage,
        crossing: Option<&Crossing>,
        frame: Frame,
    ) {
        let from = crossing.map_or(to, |c| &c.from);
        let swarm = to.act.mode == Mode::Swarm;
        let flames =
            to.act.backdrop.splatted() || (crossing.is_some() && from.act.backdrop.splatted());
        self.upload_inputs(glyphs, from, to, crossing, swarm);
        let (cols, rows) = (to.cols, to.rows);
        let cells = usize::from(cols).saturating_mul(usize::from(rows));
        self.resize(cells);
        let (born_from, born_to) = (self.born(from.id, frame.t), self.born(to.id, frame.t));
        self.tracker
            .born
            .retain(|(id, _)| *id == from.id || *id == to.id);

        let tracker = &mut *self.tracker;
        tracker.frame = tracker.frame.wrapping_add(1);
        let reset = tracker.last_to != to.id;
        tracker.last_to = to.id;
        let layout = Layout::new(cols, rows);
        let even = tracker.frame.is_multiple_of(2);
        let (current, previous) = if even {
            (layout.region(1), layout.region(2))
        } else {
            (layout.region(2), layout.region(1))
        };

        // The simulation's passes, and which buffer ends up with its latest state.
        let grid = num::u32_of(cells.saturating_mul(8));
        let mut sims = Vec::new();
        let mut latest = 0;
        match to.act.mode {
            Mode::Fluid => {
                sims.extend(
                    [
                        "fluid_advect",
                        "fluid_curl",
                        "fluid_vorticity",
                        "fluid_divergence",
                    ]
                    .map(|e| (pass(e), grid)),
                );
                sims.extend((0..PRESSURE).map(|k| (flipped("fluid_pressure", k % 2), grid)));
                sims.extend([(pass("fluid_project"), grid), (pass("fluid_copy"), grid)]);
            }
            Mode::Reaction => {
                sims.extend((0..REACTION).map(|k| (flipped("reaction", k % 2), grid)))
            }
            Mode::Life | Mode::Fire | Mode::Ripple => {
                tracker.ticks = tracker.ticks.wrapping_add(1);
                // Life steps every third frame so it can be followed.
                if to.act.mode != Mode::Life || reset || tracker.ticks.is_multiple_of(3) {
                    tracker.flip ^= 1;
                    let entry = match to.act.mode {
                        Mode::Life => "life",
                        Mode::Fire => "fire",
                        Mode::Ripple
                        | Mode::Flat
                        | Mode::Turntable
                        | Mode::Fluid
                        | Mode::Reaction
                        | Mode::Swarm => "ripple",
                    };
                    sims.push((flipped(entry, tracker.flip), grid));
                }
                latest = tracker.flip ^ 1;
            }
            Mode::Flat | Mode::Turntable | Mode::Swarm => {}
        }

        let particles = crossing.map_or(0, |c| c.particles().1);
        let offsets = tracker.offsets;
        let (sources, targets) = crossing.map_or((0, 0), Crossing::counts);
        // Regions in header order: 16 scene, 17 this frame's final image, 18
        // the last one, then the rest as laid out.
        let regions = (0..REGIONS).map(|i| match i {
            1 => current,
            2 => previous,
            _ => layout.region(i),
        });
        let header: [(usize, u32); 31] = [
            (0, u32::from(cols)),
            (1, u32::from(rows)),
            (2, frame.t.to_bits()),
            (3, to.screen_aspect().to_bits()),
            (4, to.act.look.code()),
            (5, glyphs.len()),
            (6, crossing.map_or(0, |c| c.kind.code())),
            (7, crossing.map_or(1.0, Crossing::progress).to_bits()),
            (8, crossing.map_or(1.0, |c| c.grain).to_bits()),
            (9, sources),
            (10, targets),
            (11, tracker.frame),
            (12, frame.dt.to_bits()),
            (13, to.act.post.code()),
            (14, u32::from(frame.dither)),
            (15, if swarm { SWARM } else { 0 }),
            (34, 0),
            (35, offsets.ascii),
            (36, glyphs.ascii_len()),
            (37, offsets.points),
            (38, offsets.homes),
            (39, offsets.homes_count),
            (40, offsets.art),
            (41, offsets.orbit),
            (42, latest),
            (43, u32::from(reset)),
            (44, offsets.orbit_len),
            (45, layout.region(18)),
            (46, if flames { FLAME_THREADS } else { 0 }),
            (47, FLAME_STEPS),
            (48, 0),
        ];
        let mut params = vec![0_u32; HEADER];
        // Regions 0-15 go to words 16-31, the last three after the ones above.
        let slots = (16..34).chain([45]);
        for (i, value) in header.into_iter().chain(slots.zip(regions)) {
            if let Some(slot) = params.get_mut(i) {
                *slot = value;
            }
        }
        let from_len = if from.id == to.id {
            0
        } else {
            num::u32_of(from.texels_len())
        };
        params.extend(from.block(0, frame.t - born_from));
        params.extend(to.block(from_len, frame.t - born_to));

        let image = num::u32_of(cells.saturating_mul(32));
        let cell_count = num::u32_of(cells);
        let mut passes = Vec::new();
        if particles > 0 || swarm {
            passes.push((pass("clear"), cell_count));
        }
        if particles > 0 {
            passes.push((pass("particles"), particles));
        }
        if swarm {
            passes.push((pass("swarm"), SWARM));
        }
        if flames {
            passes.extend([(pass("splat_fade"), image), (pass("flame"), FLAME_THREADS)]);
        }
        passes.push((pass("field_seed"), grid));
        passes.extend((0..JUMPS).rev().map(|k| (jump(k), grid)));
        passes.extend([(pass("field_finish"), grid), (pass("backdrops"), grid)]);
        passes.extend(sims);
        passes.push((pass("scene"), image));
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
        passes.extend([(pass("post"), image), (pass("encode"), cell_count)]);
        self.job.params = params;
        self.job.passes = passes;
        self.job.state = layout.total;
    }

    /// Rebuilds the inputs buffer when the art, particles or swarm change.
    fn upload_inputs(
        &mut self,
        glyphs: &Glyphs,
        from: &Stage,
        to: &Stage,
        crossing: Option<&Crossing>,
        swarm: bool,
    ) {
        if swarm && self.tracker.homes.0 != to.id {
            let homes = transition::homes(to, usize::try_from(SWARM).unwrap_or(usize::MAX));
            self.tracker.homes = (to.id, homes);
        }
        if self.tracker.orbit.is_empty() {
            self.tracker.orbit = backdrop::reference_orbit();
        }
        let key = (
            from.id,
            to.id,
            crossing.is_some(),
            if swarm { to.id } else { 0 },
        );
        if self.tracker.inputs == Some(key) {
            return;
        }
        self.tracker.inputs = Some(key);
        let mut data = glyphs.pack();
        let ascii = num::u32_of(data.len());
        data.extend(glyphs.pack_ascii());
        let orbit = num::u32_of(data.len());
        data.extend_from_slice(&self.tracker.orbit);
        let art = num::u32_of(data.len());
        data.extend(from.texels());
        if from.id != to.id {
            data.extend(to.texels());
        }
        let points = num::u32_of(data.len());
        if let Some(crossing) = crossing {
            data.extend(crossing.particles().0);
        }
        let homes = num::u32_of(data.len());
        let homes_count = if swarm {
            num::u32_of(self.tracker.homes.1.len() / 3)
        } else {
            0
        };
        if swarm {
            data.extend_from_slice(&self.tracker.homes.1);
        }
        let orbit_len = num::u32_of(self.tracker.orbit.len() / 2);
        self.tracker.offsets = Inputs {
            ascii,
            orbit,
            orbit_len,
            art,
            points,
            homes,
            homes_count,
        };
        let handle = self.job.inputs.clone();
        self.replace(&handle, data);
    }

    /// Makes the per-cell buffers and the readback fit `cells` cells.
    fn resize(&mut self, cells: usize) {
        if cells == self.tracker.cells && self.tracker.readback.is_some() {
            return;
        }
        self.tracker.cells = cells;
        // A glyph, foreground and background per cell, after the size.
        let output = self
            .buffers
            .add(buffer(vec![0; cells.saturating_mul(3).saturating_add(2)]));
        self.job.output = output.clone();
        self.job.cover = self.buffers.add(buffer(vec![0; cells.saturating_mul(32)]));
        self.job.color = self.buffers.add(buffer(vec![0; cells.saturating_mul(8)]));
        self.job.hist = self.buffers.add(buffer(vec![0; cells.saturating_mul(64)]));
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

#[derive(Resource)]
struct Pipelines {
    layout: BindGroupLayoutDescriptor,
    ids: HashMap<Pass, CachedComputePipelineId>,
}

fn init_pipelines(mut commands: Commands, cache: Res<PipelineCache>) {
    let read = || storage_buffer_read_only_sized(false, None);
    let write = || storage_buffer_sized(false, None);
    let layout = BindGroupLayoutDescriptor::new(
        "showcase",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (read(), read(), write(), write(), write(), write(), write()),
        ),
    );
    let ids = all_passes()
        .into_iter()
        .map(|p| {
            let id = cache.queue_compute_pipeline(ComputePipelineDescriptor {
                label: Some(p.entry.into()),
                layout: vec![layout.clone()],
                shader: SHADER,
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
    commands.insert_resource(Pipelines { layout, ids });
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
    let (Some(inputs), Some(output), Some(cover), Some(color), Some(hist)) = (
        get(&job.inputs),
        get(&job.output),
        get(&job.cover),
        get(&job.color),
        get(&job.hist),
    ) else {
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
        )),
    );
    commands.insert_resource(Bound(bind_group));
}

fn dispatch(
    mut context: RenderContext,
    cache: Res<PipelineCache>,
    pipelines: Res<Pipelines>,
    bound: Option<Res<Bound>>,
    job: Option<Res<Job>>,
    mut queries: Option<ResMut<Queries>>,
) {
    let (Some(bound), Some(job)) = (bound, job) else {
        return;
    };
    // Until every pass this frame needs has compiled, keep the last frame.
    let ready: Option<Vec<_>> = job
        .passes
        .iter()
        .map(|(p, threads)| {
            let pipeline = pipelines
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
            let workgroups = threads.div_ceil(WORKGROUP);
            if workgroups > 0 {
                compute.set_pipeline(pipeline);
                compute.dispatch_workgroups(workgroups, 1, 1);
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
        // The seed writes field 0, so the last jump (a jump of 1) must too:
        // it reads field 1, i.e. it is flipped.
        assert_eq!(jump(0).step, 1);
        assert_eq!(jump(0).flip, 1);
        assert_eq!(jump(JUMPS - 1).flip, 0);
    }

    #[test]
    fn state_regions_follow_each_other() {
        let layout = Layout::new(120, 40);
        let starts = (0..REGIONS).map(|i| layout.region(i));
        assert!(starts.clone().zip(starts.skip(1)).all(|(a, b)| a < b));
        assert_eq!(
            u64::from(layout.region(REGIONS - 1)) + u64::from(FLAME_THREADS),
            layout.total
        );
    }

    #[test]
    fn every_pass_a_frame_can_ask_for_has_a_pipeline() {
        let all = all_passes();
        let wanted = [
            "fluid_curl",
            "fluid_vorticity",
            "field_finish",
            "backdrops",
            "splat_fade",
            "flame",
        ];
        assert!(wanted.iter().all(|e| all.iter().any(|p| p.entry == *e)));
        assert!((0..BLOOM_LEVELS).all(|l| all.contains(&leveled("bloom_blur", l, 1))));
    }
}
