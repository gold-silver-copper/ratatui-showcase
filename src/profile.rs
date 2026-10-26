//! GPU time per phase, with `--profile`: a timestamp before and after each
//! phase's compute pass, resolved into one of a few readback buffers and
//! read a frame or two later, once the GPU has finished with it.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use bevy::prelude::*;
use bevy::render::render_resource::{Buffer, BufferDescriptor, BufferUsages, MapMode};
use bevy::render::renderer::{RenderDevice, RenderQueue};

/// Phases a frame can time.
pub const PHASES: u32 = 16;
/// Readback buffers in rotation.
const SLOTS: usize = 6;

const FREE: u8 = 0;
const WRITTEN: u8 = 1;
/// A frame after writing: the commands that fill the slot may only reach
/// the GPU with the next submit, so it is mapped one frame later still.
const AGED: u8 = 2;
const MAPPING: u8 = 3;
const MAPPED: u8 = 4;

/// Milliseconds per phase, shared with the main world.
#[derive(Resource, Clone, Default)]
pub struct Timings(pub Arc<Mutex<Vec<(&'static str, f64)>>>);

struct Slot {
    buffer: Buffer,
    state: Arc<AtomicU8>,
    names: Vec<&'static str>,
}

/// The query set and its readback buffers, in the render world.
#[derive(Resource)]
pub struct Queries {
    pub set: wgpu::QuerySet,
    resolve: Buffer,
    slots: Vec<Slot>,
    period: f32,
    timings: Timings,
}

impl Queries {
    /// Queries for timing, if the GPU can write timestamps.
    pub fn new(device: &RenderDevice, queue: &RenderQueue, timings: Timings) -> Option<Self> {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }
        let set = device
            .wgpu_device()
            .create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("showcase timestamps"),
                ty: wgpu::QueryType::Timestamp,
                count: PHASES * 2,
            });
        let size = u64::from(PHASES * 2 * 8);
        let make = |usage| {
            device.create_buffer(&BufferDescriptor {
                label: Some("showcase timing"),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        let resolve = make(BufferUsages::QUERY_RESOLVE | BufferUsages::COPY_SRC);
        let slots = (0..SLOTS)
            .map(|_| Slot {
                buffer: make(BufferUsages::MAP_READ | BufferUsages::COPY_DST),
                state: Arc::new(AtomicU8::new(FREE)),
                names: Vec::new(),
            })
            .collect();
        Some(Self {
            set,
            resolve,
            slots,
            period: queue.get_timestamp_period(),
            timings,
        })
    }

    /// Resolves this frame's timestamps for `names` into a free slot.
    pub fn resolve(&mut self, encoder: &mut wgpu::CommandEncoder, names: Vec<&'static str>) {
        let count = u32::try_from(names.len()).unwrap_or(PHASES).min(PHASES);
        let Some(slot) = self
            .slots
            .iter_mut()
            .find(|s| s.state.load(Ordering::Acquire) == FREE)
        else {
            return;
        };
        encoder.resolve_query_set(&self.set, 0..count.saturating_mul(2), &self.resolve, 0);
        encoder.copy_buffer_to_buffer(
            &self.resolve,
            0,
            &slot.buffer,
            0,
            u64::from(count.saturating_mul(16)),
        );
        slot.names = names;
        slot.state.store(WRITTEN, Ordering::Release);
    }

    /// After the frame is submitted: asks for slots written a frame ago,
    /// and reads the ones that arrived.
    pub fn collect(&mut self) {
        for slot in &mut self.slots {
            match slot.state.load(Ordering::Acquire) {
                WRITTEN => slot.state.store(AGED, Ordering::Release),
                AGED => {
                    slot.state.store(MAPPING, Ordering::Release);
                    let state = slot.state.clone();
                    slot.buffer
                        .slice(..)
                        .map_async(MapMode::Read, move |result| {
                            state.store(
                                if result.is_ok() { MAPPED } else { FREE },
                                Ordering::Release,
                            );
                        });
                }
                MAPPED => {
                    let stamps: Vec<u64> = match slot.buffer.slice(..).get_mapped_range() {
                        Ok(view) => view
                            .as_chunks::<8>()
                            .0
                            .iter()
                            .map(|b| u64::from_le_bytes(*b))
                            .collect(),
                        Err(_) => Vec::new(),
                    };
                    slot.buffer.unmap();
                    slot.state.store(FREE, Ordering::Release);
                    let pairs = stamps.as_chunks::<2>().0;
                    let mut shared = self
                        .timings
                        .0
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner);
                    // Phases that stopped running fade out of the list.
                    shared.retain(|(name, _)| slot.names.contains(name));
                    for (&name, &[start, end]) in slot.names.iter().zip(pairs) {
                        let ms = f64::from(self.period) * end.saturating_sub(start) as f64 / 1e6;
                        match shared.iter_mut().find(|(n, _)| *n == name) {
                            // Smoothed over the last few frames.
                            Some((_, value)) => *value += (ms - *value) * 0.1,
                            None => shared.push((name, ms)),
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
