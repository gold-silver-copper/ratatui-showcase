//! The room's sound, with `--audio`: the default microphone's loudness and
//! three bands, each scaled against its own recent peak so quiet rooms and
//! loud ones both move the picture.

use bevy::prelude::*;

use crate::gpu::Sound;

pub struct AudioPlugin {
    pub enabled: bool,
}

/// The sound now, smoothed for the picture, and whether listening works.
#[derive(Resource, Default)]
pub struct Hearing {
    pub sound: Sound,
    pub status: Option<&'static str>,
}

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Hearing>();
        if self.enabled {
            listen(app);
        }
    }
}

#[cfg(not(feature = "audio"))]
fn listen(app: &mut App) {
    app.insert_resource(Hearing {
        sound: Sound::default(),
        status: Some("built without audio"),
    });
}

#[cfg(feature = "audio")]
fn listen(app: &mut App) {
    match mic::open() {
        Some((stream, shared)) => {
            app.insert_non_send(stream)
                .insert_resource(shared)
                .add_systems(PreUpdate, mic::hear);
        }
        None => {
            app.insert_resource(Hearing {
                sound: Sound::default(),
                status: Some("no microphone"),
            });
        }
    }
}

#[cfg(feature = "audio")]
mod mic {
    use std::sync::{Arc, Mutex, PoisonError};

    use bevy::prelude::*;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};

    use super::Hearing;
    use crate::gpu::Sound;
    use crate::num;

    /// The latest measurement, shared with the audio thread.
    #[derive(Resource, Clone, Default)]
    pub struct Shared(Arc<Mutex<Sound>>);

    /// Splits the signal into bands with one-pole filters: below about
    /// 200 Hz, 200 Hz to 3 kHz, and above.
    struct Analyser {
        low: f32,
        high: f32,
        bass_filter: f32,
        mid_filter: f32,
        peaks: [f32; 4],
    }

    impl Analyser {
        fn new(rate: f32) -> Self {
            let pole = |hz: f32| 1.0 - (-std::f32::consts::TAU * hz / rate.max(1.0)).exp();
            Self {
                low: pole(200.0),
                high: pole(3000.0),
                bass_filter: 0.0,
                mid_filter: 0.0,
                peaks: [0.05; 4],
            }
        }

        /// Measures one buffer of mono samples.
        fn measure(&mut self, samples: impl Iterator<Item = f32>) -> Option<Sound> {
            let mut sums = [0.0_f32; 4];
            let mut n = 0.0_f32;
            for x in samples {
                self.bass_filter += self.low * (x - self.bass_filter);
                self.mid_filter += self.high * (x - self.mid_filter);
                let bands = [
                    x,
                    self.bass_filter,
                    self.mid_filter - self.bass_filter,
                    x - self.mid_filter,
                ];
                for (sum, band) in sums.iter_mut().zip(bands) {
                    *sum += band * band;
                }
                n += 1.0;
            }
            if n == 0.0 {
                return None;
            }
            let mut out = [0.0_f32; 4];
            for ((o, sum), peak) in out.iter_mut().zip(sums).zip(self.peaks.iter_mut()) {
                let rms = (sum / n).sqrt();
                *peak = (*peak * 0.995).max(rms).max(0.01);
                *o = (rms / *peak).clamp(0.0, 1.0);
            }
            let [level, bass, mid, treble] = out;
            Some(Sound {
                level,
                bass,
                mid,
                treble,
            })
        }
    }

    pub fn open() -> Option<(cpal::Stream, Shared)> {
        let device = cpal::default_host().default_input_device()?;
        let supported = device.default_input_config().ok()?;
        let format = supported.sample_format();
        let config: StreamConfig = supported.into();
        let shared = Shared::default();
        let stream = match format {
            SampleFormat::F32 => build::<f32>(&device, config, &shared),
            SampleFormat::I16 => build::<i16>(&device, config, &shared),
            SampleFormat::U16 => build::<u16>(&device, config, &shared),
            SampleFormat::I32 => build::<i32>(&device, config, &shared),
            SampleFormat::I8
            | SampleFormat::I24
            | SampleFormat::I64
            | SampleFormat::U8
            | SampleFormat::U24
            | SampleFormat::U32
            | SampleFormat::U64
            | SampleFormat::F64
            | SampleFormat::DsdU8
            | SampleFormat::DsdU16
            | SampleFormat::DsdU32 => None,
            // Formats cpal adds later.
            _ => None,
        }?;
        stream.play().ok()?;
        Some((stream, shared))
    }

    fn build<T>(
        device: &cpal::Device,
        config: StreamConfig,
        shared: &Shared,
    ) -> Option<cpal::Stream>
    where
        T: SizedSample,
        f32: FromSample<T>,
    {
        let channels = usize::from(config.channels.max(1));
        let rate = num::float(usize::try_from(config.sample_rate).unwrap_or(48_000));
        let mut analyser = Analyser::new(rate.max(8000.0));
        let shared = shared.0.clone();
        device
            .build_input_stream::<T, _, _>(
                config,
                move |data: &[T], _| {
                    let mono = data
                        .iter()
                        .step_by_channels(channels)
                        .map(|&s| s.to_sample::<f32>());
                    if let Some(sound) = analyser.measure(mono) {
                        *shared.lock().unwrap_or_else(PoisonError::into_inner) = sound;
                    }
                },
                |_| {},
                None,
            )
            .ok()
    }

    /// Takes every `n`th sample: the first channel of interleaved audio.
    trait StepByChannels: Iterator + Sized {
        fn step_by_channels(self, n: usize) -> impl Iterator<Item = Self::Item> {
            self.enumerate()
                .filter(move |(i, _)| i.checked_rem(n) == Some(0))
                .map(|(_, x)| x)
        }
    }

    impl<I: Iterator> StepByChannels for I {}

    /// Eases the measurement into the picture: quick to rise, slow to fall.
    pub fn hear(shared: Res<Shared>, mut hearing: ResMut<Hearing>) {
        let now = *shared.0.lock().unwrap_or_else(PoisonError::into_inner);
        let ease = |old: f32, new: f32| {
            if new > old {
                old + (new - old) * 0.6
            } else {
                old + (new - old) * 0.12
            }
        };
        let s = &mut hearing.sound;
        *s = Sound {
            level: ease(s.level, now.level),
            bass: ease(s.bass, now.bass),
            mid: ease(s.mid, now.mid),
            treble: ease(s.treble, now.treble),
        };
    }
}
