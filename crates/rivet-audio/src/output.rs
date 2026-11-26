//! Audio device output via cpal. The mixer runs on the audio thread; the game sends commands.

use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::mixer::{Command, Mixer};

/// Commands buffered between game frames and the audio callback.
const QUEUE: usize = 1024;

/// A running output stream. Dropping it stops the sound.
pub struct Audio {
    tx: SyncSender<Command>,
    _stream: cpal::Stream,
}

fn drain(rx: &Receiver<Command>, mixer: &mut Mixer) {
    while let Ok(cmd) = rx.try_recv() {
        mixer.apply(cmd);
    }
}

impl Audio {
    /// Opens the default output device; `None` (with a log line) when there is none.
    pub fn new() -> Option<Self> {
        let host = cpal::default_host();
        let Some(device) = host.default_output_device() else {
            log::warn!("no audio output device; running silent");
            return None;
        };
        let supported = match device.default_output_config() {
            Ok(c) => c,
            Err(e) => {
                log::warn!("audio device has no usable config ({e}); running silent");
                return None;
            }
        };
        let format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let channels = config.channels as usize;
        let rate = config.sample_rate as f32;
        // Bounded: if the device stops pulling (unplugged), commands are dropped, not hoarded.
        let (tx, rx) = sync_channel::<Command>(QUEUE);
        let mut mixer = Mixer::new(rate);
        let on_err = |e| log::warn!("audio stream error: {e}");
        let stream = match format {
            cpal::SampleFormat::F32 => device.build_output_stream(
                config,
                move |data: &mut [f32], _| {
                    drain(&rx, &mut mixer);
                    mixer.render(data, channels);
                },
                on_err,
                None,
            ),
            cpal::SampleFormat::I16 => {
                let mut scratch = vec![0.0f32; 16_384];
                device.build_output_stream(
                    config,
                    move |data: &mut [i16], _| {
                        drain(&rx, &mut mixer);
                        scratch.resize(data.len(), 0.0);
                        mixer.render(&mut scratch, channels);
                        for (d, s) in data.iter_mut().zip(&scratch) {
                            *d = (s * i16::MAX as f32) as i16;
                        }
                    },
                    on_err,
                    None,
                )
            }
            other => {
                log::warn!("unsupported audio sample format {other:?}; running silent");
                return None;
            }
        };
        let stream = match stream {
            Ok(s) => s,
            Err(e) => {
                log::warn!("opening the audio stream failed ({e}); running silent");
                return None;
            }
        };
        if let Err(e) = stream.play() {
            log::warn!("starting audio failed ({e}); running silent");
            return None;
        }
        log::info!("audio: {} Hz, {channels} channels", rate);
        Some(Self {
            tx,
            _stream: stream,
        })
    }

    /// Queues a command; dropped when the queue is full (device stalled or gone).
    pub fn send(&self, cmd: Command) {
        match self.tx.try_send(cmd) {
            Ok(()) | Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {}
        }
    }
}
