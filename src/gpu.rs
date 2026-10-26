//! Rendering on the GPU: the CPU lays out a scene and packs it into flat
//! `u32` buffers; a compute shader (`render.wgsl`) draws every cell, glyph
//! and colours included; the result is read back each frame for the terminal.
//!
//! Buffers, all `array<u32>` on the GPU with floats stored as their bits:
//! - params: the frame, both scenes and the transition (every frame)
//! - art: both scenes' pixels and glow (when a scene changes)
//! - particles: transition particles' home positions (when a transition starts)
//! - glyphs: the block glyph shapes (once)
//! - output: `[cols, rows]` then a glyph, foreground and background per cell
//! - cover, color: scratch where particles are splatted

use bevy::asset::{RenderAssetUsages, embedded_asset};
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

use bevy::ecs::system::SystemParam;

use crate::glyphs::Glyphs;
use crate::num;
use crate::stage::Stage;
use crate::transition::Crossing;

const SHADER: &str = "embedded://ratatui_showcase/render.wgsl";
const WORKGROUP: u32 = 64;

pub struct GpuPlugin;

impl Plugin for GpuPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "render.wgsl");
        app.add_plugins(ExtractResourcePlugin::<Job>::default())
            .init_resource::<GpuFrame>()
            .add_systems(Startup, setup);
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
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
                ),
            );
    }
}

/// The latest frame read back from the GPU.
#[derive(Resource, Default)]
pub struct GpuFrame {
    pub data: Vec<u32>,
}

impl GpuFrame {
    /// The glyph and colours of each cell, if the frame is `cols` x `rows`.
    pub fn cells(&self, cols: u16, rows: u16) -> Option<&[u32]> {
        let (&c, rest) = self.data.split_first()?;
        let (&r, cells) = rest.split_first()?;
        (c == u32::from(cols) && r == u32::from(rows)).then_some(cells)
    }
}

/// Handles to the buffers, and how much work this frame needs.
#[derive(Resource, ExtractResource, Clone)]
#[extract_app(RenderApp)]
struct Job {
    /// This frame's parameters, written straight into a buffer that lives
    /// in the render world.
    params: Vec<u32>,
    art: Handle<ShaderBuffer>,
    particles: Handle<ShaderBuffer>,
    glyphs: Handle<ShaderBuffer>,
    output: Handle<ShaderBuffer>,
    cover: Handle<ShaderBuffer>,
    color: Handle<ShaderBuffer>,
    cells: u32,
    particle_threads: u32,
}

/// The CPU's side of the GPU renderer.
#[derive(SystemParam)]
pub struct Gpu<'w, 's> {
    job: ResMut<'w, Job>,
    buffers: ResMut<'w, Assets<ShaderBuffer>>,
    readback: Local<'s, Option<Entity>>,
    uploaded: Local<'s, Uploaded>,
    commands: Commands<'w, 's>,
}

/// What the GPU already holds, so it is sent only when it changes.
#[derive(Default)]
struct Uploaded {
    glyphs: bool,
    art: Option<(u64, u64)>,
    particles: Option<(u64, u64)>,
}

/// Words before the first scene's block in `params`.
const HEADER: usize = 16;

fn buffer(data: Vec<u32>) -> ShaderBuffer {
    // Empty buffers can't be bound.
    let data = if data.is_empty() { vec![0] } else { data };
    ShaderBuffer::new(data, RenderAssetUsages::RENDER_WORLD)
}

fn setup(mut commands: Commands, mut buffers: ResMut<Assets<ShaderBuffer>>) {
    let mut add = |data: Vec<u32>| buffers.add(buffer(data));
    let job = Job {
        params: vec![0],
        art: add(vec![]),
        particles: add(vec![]),
        glyphs: add(vec![]),
        output: add(vec![]),
        cover: add(vec![]),
        color: add(vec![]),
        cells: 0,
        particle_threads: 0,
    };
    commands.insert_resource(job);
}

impl Gpu<'_, '_> {
    fn replace(&mut self, handle: &Handle<ShaderBuffer>, data: Vec<u32>) {
        // Uploaded buffers leave the main world, so each frame replaces them.
        let _ = self.buffers.insert(handle, buffer(data));
    }

    /// Queues a frame: `to` at rest, or `crossing` from its old scene to `to`.
    pub fn render(&mut self, glyphs: &Glyphs, to: &Stage, crossing: Option<&Crossing>, t: f32) {
        if !self.uploaded.glyphs {
            self.uploaded.glyphs = true;
            let handle = self.job.glyphs.clone();
            self.replace(&handle, glyphs.pack());
        }
        let from = crossing.map_or(to, |c| &c.from);
        if self.uploaded.art != Some((from.id, to.id)) {
            self.uploaded.art = Some((from.id, to.id));
            let mut art = from.texels();
            if from.id != to.id {
                art.extend(to.texels());
            }
            let handle = self.job.art.clone();
            self.replace(&handle, art);
        }
        let from_len = if from.id == to.id {
            0
        } else {
            num::u32_of(from.texels_len())
        };
        match crossing {
            Some(crossing) if self.uploaded.particles != Some((from.id, to.id)) => {
                self.uploaded.particles = Some((from.id, to.id));
                let (data, threads) = crossing.particles();
                let handle = self.job.particles.clone();
                self.replace(&handle, data);
                self.job.particle_threads = threads;
            }
            Some(_) => {}
            None => self.job.particle_threads = 0,
        }

        let (sources, targets) = crossing.map_or((0, 0), Crossing::counts);
        let header = [
            u32::from(to.cols),
            u32::from(to.rows),
            t.to_bits(),
            to.screen_aspect().to_bits(),
            to.act.look.code(),
            glyphs.len(),
            crossing.map_or(0, |c| c.kind.code()),
            crossing.map_or(1.0, Crossing::progress).to_bits(),
            crossing.map_or(1.0, |c| c.grain).to_bits(),
            sources,
            targets,
        ];
        let mut params = vec![0; HEADER];
        for (slot, value) in params.iter_mut().zip(header) {
            *slot = value;
        }
        params.extend(from.block(0));
        params.extend(to.block(from_len));
        self.frame(params, to.cols, to.rows);
    }

    /// Queues this frame: `params` for a `cols` x `rows` terminal.
    fn frame(&mut self, params: Vec<u32>, cols: u16, rows: u16) {
        let cells = usize::from(cols).saturating_mul(usize::from(rows));
        self.job.params = params;
        if num::u32_of(cells) != self.job.cells || self.readback.is_none() {
            self.job.cells = num::u32_of(cells);
            // A glyph, foreground and background per cell, after the size.
            let output = self
                .buffers
                .add(buffer(vec![0; cells.saturating_mul(3).saturating_add(2)]));
            self.job.output = output.clone();
            self.job.cover = self.buffers.add(buffer(vec![0; cells.saturating_mul(32)]));
            self.job.color = self.buffers.add(buffer(vec![0; cells.saturating_mul(8)]));
            if let Some(old) = self.readback.take() {
                self.commands.entity(old).despawn();
            }
            let entity = self
                .commands
                .spawn(Readback::buffer(output))
                .observe(|event: On<ReadbackComplete>, mut frame: ResMut<GpuFrame>| {
                    frame.data = event.to_shader_type();
                })
                .id();
            *self.readback = Some(entity);
        }
    }
}

#[derive(Resource)]
struct Pipelines {
    layout: BindGroupLayoutDescriptor,
    clear: CachedComputePipelineId,
    particles: CachedComputePipelineId,
    main: CachedComputePipelineId,
}

fn init_pipelines(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    cache: Res<PipelineCache>,
) {
    let read = || storage_buffer_read_only_sized(false, None);
    let write = || storage_buffer_sized(false, None);
    let layout = BindGroupLayoutDescriptor::new(
        "showcase",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (read(), read(), read(), read(), write(), write(), write()),
        ),
    );
    let shader = asset_server.load(SHADER);
    let queue = |entry: &'static str| {
        cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: Some(entry.into()),
            layout: vec![layout.clone()],
            shader: shader.clone(),
            entry_point: Some(entry.into()),
            ..default()
        })
    };
    let pipelines = Pipelines {
        clear: queue("clear"),
        particles: queue("particles"),
        main: queue("main"),
        layout: layout.clone(),
    };
    commands.insert_resource(pipelines);
}

#[derive(Resource)]
struct Bound(BindGroup);

/// The params buffer, reused while it is big enough.
#[derive(Resource)]
struct Params(Buffer);

fn prepare_bind_group(
    mut commands: Commands,
    pipelines: Res<Pipelines>,
    (device, queue, cache): (Res<RenderDevice>, Res<RenderQueue>, Res<PipelineCache>),
    job: Option<Res<Job>>,
    params: Option<Res<Params>>,
    buffers: Res<RenderAssets<GpuShaderBuffer>>,
) {
    let Some(job) = job else { return };
    let bytes: &[u8] = bytemuck::cast_slice(&job.params);
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    let params = match params {
        Some(params) if params.0.size() >= size => params.0.clone(),
        _ => {
            let buffer = device.create_buffer(&BufferDescriptor {
                label: Some("showcase params"),
                size,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            commands.insert_resource(Params(buffer.clone()));
            buffer
        }
    };
    queue.write_buffer(&params, 0, bytes);
    let get = |handle: &Handle<ShaderBuffer>| {
        buffers
            .get(handle)
            .map(|b| b.buffer.as_entire_buffer_binding())
    };
    let all = (
        Some(params.as_entire_buffer_binding()),
        get(&job.art),
        get(&job.particles),
        get(&job.glyphs),
        get(&job.output),
        get(&job.cover),
        get(&job.color),
    );
    let (Some(a), Some(b), Some(c), Some(d), Some(e), Some(f), Some(g)) = all else {
        commands.remove_resource::<Bound>();
        return;
    };
    let bind_group = device.create_bind_group(
        "showcase",
        &cache.get_bind_group_layout(&pipelines.layout),
        &BindGroupEntries::sequential((a, b, c, d, e, f, g)),
    );
    commands.insert_resource(Bound(bind_group));
}

fn dispatch(
    mut context: RenderContext,
    cache: Res<PipelineCache>,
    pipelines: Res<Pipelines>,
    bound: Option<Res<Bound>>,
    job: Option<Res<Job>>,
) {
    let (Some(bound), Some(job)) = (bound, job) else {
        return;
    };
    let groups = |threads: u32| threads.div_ceil(WORKGROUP);
    let passes = [
        (pipelines.clear, groups(job.cells)),
        (pipelines.particles, groups(job.particle_threads)),
        (pipelines.main, groups(job.cells)),
    ];
    let mut pass = context
        .command_encoder()
        .begin_compute_pass(&ComputePassDescriptor {
            label: Some("showcase"),
            ..default()
        });
    pass.set_bind_group(0, &bound.0, &[]);
    for (id, workgroups) in passes {
        if let Some(pipeline) = cache.get_compute_pipeline(id)
            && workgroups > 0
        {
            pass.set_pipeline(pipeline);
            pass.dispatch_workgroups(workgroups, 1, 1);
        }
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
    #[test]
    fn shader_is_valid_wgsl() {
        let source = include_str!("render.wgsl");
        let parsed = naga::front::wgsl::parse_str(source);
        let message = parsed
            .as_ref()
            .err()
            .map(|e| e.emit_to_string(source))
            .unwrap_or_default();
        assert!(parsed.is_ok(), "{message}");
        if let Ok(module) = parsed {
            let flags = naga::valid::ValidationFlags::all();
            let result = naga::valid::Validator::new(flags, naga::valid::Capabilities::all())
                .validate(&module);
            assert!(result.is_ok(), "{result:?}");
        }
    }
}
