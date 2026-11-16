// SPDX-License-Identifier: GPL-3.0-or-later

//! Sound output on the system's default device, with no setup.
//!
//! A watcher thread owns the output stream. When the device disappears or
//! fails, it reopens the default device; when there is no device at all, a
//! silent stand-in keeps calling the audio callback in real time, so the
//! rest of the program (the transport, recording, meters) behaves normally.

#![allow(
    unsafe_code,
    reason = "the audio callback is shared between streams through a cell guarded by an atomic flag"
)]

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, FromSample, SampleFormat, SizedSample, StreamConfig};

/// The audio callback: fill the interleaved buffer (frames × channels).
pub type AudioCallback = Box<dyn FnMut(&mut [f32], usize) + Send>;

/// The buffer size asked for: about 5 ms at 48 kHz. The system may give a
/// different one (Windows' shared mode uses its own period).
const PREFERRED_BUFFER: u32 = 256;

/// Frames the stand-in renders per call when there is no device.
const SILENT_BLOCK: u32 = 512;

/// How often the watcher checks the device.
const WATCH_INTERVAL: Duration = Duration::from_millis(1000);

/// The audio callback, shared by successive streams. Only one stream runs at
/// a time; the flag makes sure the callback is never entered twice, without
/// ever blocking the audio thread (a second caller outputs silence).
struct CallbackSlot {
    busy: AtomicBool,
    callback: UnsafeCell<AudioCallback>,
}

// SAFETY: `callback` is only accessed by whoever wins the `busy` flag, so
// there is never more than one access at a time.
unsafe impl Sync for CallbackSlot {}

impl CallbackSlot {
    /// Runs the callback if no one else is; returns whether it ran.
    fn run(&self, buffer: &mut [f32], channels: usize) -> bool {
        if self
            .busy
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return false;
        }
        // SAFETY: we hold the `busy` flag, so this is the only access.
        unsafe { (*self.callback.get())(buffer, channels) };
        self.busy.store(false, Ordering::Release);
        true
    }
}

/// Where sound is going.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutputMode {
    /// Starting up.
    Starting,
    /// Playing through a sound device.
    Device {
        /// The device's name.
        name: String,
    },
    /// No device is available: time runs, nothing is heard.
    Silent,
}

/// The state of the output, for display.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputStatus {
    /// Where sound is going.
    pub mode: OutputMode,
    /// The sample rate in use.
    pub sample_rate: u32,
    /// The buffer size in frames, when the system reports it.
    pub buffer_frames: Option<u32>,
    /// Whether the audio thread runs at real-time priority (Linux only;
    /// Windows' audio system raises its own threads).
    pub realtime: Option<bool>,
    /// The last problem met, if any.
    pub problem: Option<String>,
}

struct Shared {
    slot: CallbackSlot,
    status: Mutex<OutputStatus>,
    /// Set from the audio thread, which may not lock: 0 not tried, 1 raised,
    /// 2 refused, 3 left to the system.
    realtime: AtomicU8,
    stop: AtomicBool,
    sample_rate: u32,
}

impl Shared {
    fn update(&self, change: impl FnOnce(&mut OutputStatus)) {
        change(&mut self.status.lock().unwrap_or_else(PoisonError::into_inner));
    }
}

/// Sound output. Dropping it stops the sound.
pub struct AudioOutput {
    shared: Arc<Shared>,
    watcher: Option<JoinHandle<()>>,
}

impl AudioOutput {
    /// Starts sound output at `sample_rate` (the default device is asked for
    /// that rate; every common device supports 48 kHz). `callback` is called
    /// on the audio thread and must follow the real-time rules.
    pub fn start(
        sample_rate: u32,
        callback: impl FnMut(&mut [f32], usize) + Send + 'static,
    ) -> Self {
        Self::start_with(sample_rate, Box::new(callback), true)
    }

    /// Like [`AudioOutput::start`], but never opens a device: the silent
    /// stand-in runs from the start. For tests and for machines without sound.
    pub fn start_silent(
        sample_rate: u32,
        callback: impl FnMut(&mut [f32], usize) + Send + 'static,
    ) -> Self {
        Self::start_with(sample_rate, Box::new(callback), false)
    }

    fn start_with(sample_rate: u32, callback: AudioCallback, use_devices: bool) -> Self {
        let shared = Arc::new(Shared {
            slot: CallbackSlot {
                busy: AtomicBool::new(false),
                callback: UnsafeCell::new(callback),
            },
            status: Mutex::new(OutputStatus {
                mode: OutputMode::Starting,
                sample_rate,
                buffer_frames: None,
                realtime: None,
                problem: None,
            }),
            realtime: AtomicU8::new(0),
            stop: AtomicBool::new(false),
            sample_rate,
        });
        let watcher_shared = Arc::clone(&shared);
        let watcher = std::thread::Builder::new()
            .name("audio-watcher".to_owned())
            .spawn(move || watch(&watcher_shared, use_devices))
            .ok();
        Self { shared, watcher }
    }

    /// The state of the output now.
    pub fn status(&self) -> OutputStatus {
        let mut status = self
            .shared
            .status
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        status.realtime = match self.shared.realtime.load(Ordering::Relaxed) {
            1 => Some(true),
            2 => Some(false),
            _ => None,
        };
        if status.mode == OutputMode::Silent {
            status.realtime = None;
        }
        status
    }
}

impl Drop for AudioOutput {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(watcher) = self.watcher.take() {
            let _ = watcher.join();
        }
    }
}

/// The watcher: keeps a working output at all times.
fn watch(shared: &Arc<Shared>, use_devices: bool) {
    let mut stream: Option<(cpal::Stream, Arc<AtomicBool>)> = None;
    let mut silent: Option<(JoinHandle<()>, Arc<AtomicBool>)> = None;
    let mut last_attempt: Option<Instant> = None;
    while !shared.stop.load(Ordering::Relaxed) {
        // A stream that reported a fatal error is dropped and replaced.
        if let Some((_, failed)) = &stream
            && failed.load(Ordering::Relaxed)
        {
            stream = None;
            shared.update(|s| {
                s.problem = Some("the sound device stopped; looking for another".to_owned());
            });
        }
        if stream.is_none()
            && use_devices
            && last_attempt.is_none_or(|t| t.elapsed() >= WATCH_INTERVAL)
        {
            last_attempt = Some(Instant::now());
            match open_default(shared) {
                Ok(opened) => {
                    if let Some((handle, running)) = silent.take() {
                        running.store(false, Ordering::Relaxed);
                        let _ = handle.join();
                    }
                    stream = Some(opened);
                }
                Err(problem) => shared.update(|s| s.problem = Some(problem)),
            }
        }
        if stream.is_none() && silent.is_none() {
            silent = Some(start_silent_runner(Arc::clone(shared)));
            shared.update(|s| {
                s.mode = OutputMode::Silent;
                s.buffer_frames = None;
            });
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    drop(stream);
    if let Some((handle, running)) = silent {
        running.store(false, Ordering::Relaxed);
        let _ = handle.join();
    }
}

/// Calls the audio callback in real time with a scratch buffer whose sound
/// goes nowhere.
fn start_silent_runner(shared: Arc<Shared>) -> (JoinHandle<()>, Arc<AtomicBool>) {
    let running = Arc::new(AtomicBool::new(true));
    let flag = Arc::clone(&running);
    let handle = std::thread::Builder::new()
        .name("audio-silent".to_owned())
        .spawn(move || {
            let mut buffer = vec![0.0_f32; SILENT_BLOCK as usize * 2];
            let block =
                Duration::from_secs_f64(f64::from(SILENT_BLOCK) / f64::from(shared.sample_rate));
            let mut next = Instant::now();
            while flag.load(Ordering::Relaxed) {
                {
                    let _real_time = daw_rt::RtGuard::enter();
                    shared.slot.run(&mut buffer, 2);
                }
                next += block;
                let now = Instant::now();
                if next > now {
                    std::thread::sleep(next - now);
                } else {
                    next = now; // Fell behind (the machine was busy): carry on.
                }
            }
        })
        .unwrap_or_else(|_| std::thread::spawn(|| {}));
    (handle, running)
}

/// Opens the default output device.
fn open_default(shared: &Arc<Shared>) -> Result<(cpal::Stream, Arc<AtomicBool>), String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or("no sound output device")?;
    let name = device
        .description()
        .map_or_else(|_| "Sound output".to_owned(), |d| d.to_string());
    let default = device
        .default_output_config()
        .map_err(|e| format!("{name}: {e}"))?;
    let channels = default.channels();
    let buffer = match default.buffer_size() {
        cpal::SupportedBufferSize::Range { min, max }
            if (*min..=*max).contains(&PREFERRED_BUFFER) =>
        {
            BufferSize::Fixed(PREFERRED_BUFFER)
        }
        _ => BufferSize::Default,
    };
    let config = StreamConfig {
        channels,
        sample_rate: shared.sample_rate,
        buffer_size: buffer,
    };
    let failed = Arc::new(AtomicBool::new(false));
    let stream = match default.sample_format() {
        SampleFormat::F32 => build::<f32>(&device, config, shared, &failed),
        SampleFormat::I16 => build::<i16>(&device, config, shared, &failed),
        SampleFormat::I32 => build::<i32>(&device, config, shared, &failed),
        SampleFormat::U16 => build::<u16>(&device, config, shared, &failed),
        other => return Err(format!("{name}: unsupported sample format {other}")),
    }
    .map_err(|e| format!("{name}: {e}"))?;
    stream.play().map_err(|e| format!("{name}: {e}"))?;
    let frames = stream.buffer_size().ok();
    shared.update(|s| {
        s.mode = OutputMode::Device { name };
        s.buffer_frames = frames;
        s.problem = None;
    });
    Ok((stream, failed))
}

/// Builds a stream for sample type `T`. The callback renders into a float
/// buffer reserved here, then converts.
fn build<T>(
    device: &cpal::Device,
    config: StreamConfig,
    shared: &Arc<Shared>,
    failed: &Arc<AtomicBool>,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let channels = usize::from(config.channels);
    let render_channels = channels.max(1);
    let callback_shared = Arc::clone(shared);
    let mut scratch = vec![0.0_f32; 8192 * render_channels];
    let mut priority_tried = false;
    let data_callback = move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
        let _real_time = daw_rt::RtGuard::enter();
        if !priority_tried {
            priority_tried = true;
            let state = match raise_priority() {
                Some(true) => 1,
                Some(false) => 2,
                None => 3,
            };
            callback_shared.realtime.store(state, Ordering::Relaxed);
        }
        for chunk in data.chunks_mut(scratch.len()) {
            let buffer = &mut scratch[..chunk.len()];
            if !callback_shared.slot.run(buffer, render_channels) {
                buffer.fill(0.0);
            }
            for (out, value) in chunk.iter_mut().zip(buffer.iter()) {
                *out = T::from_sample(*value);
            }
        }
    };
    let error_flag = Arc::clone(failed);
    let error_callback = move |error: cpal::Error| {
        // Dropouts and rerouting are not fatal; anything else replaces the stream.
        let fatal = !matches!(
            error.kind(),
            cpal::ErrorKind::Xrun
                | cpal::ErrorKind::DeviceChanged
                | cpal::ErrorKind::RealtimeDenied
        );
        if fatal {
            error_flag.store(true, Ordering::Relaxed);
        }
    };
    device.build_output_stream(config, data_callback, error_callback, None)
}

/// Asks for real-time priority for the current (audio) thread.
#[cfg(target_os = "linux")]
#[allow(
    clippy::unnecessary_wraps,
    reason = "same signature as on other systems, where the answer is unknown"
)]
fn raise_priority() -> Option<bool> {
    let param = libc::sched_param { sched_priority: 70 };
    // SAFETY: pthread_self() is valid for the calling thread and `param` is
    // a fully initialised struct that outlives the call.
    let result = unsafe {
        libc::pthread_setschedparam(libc::pthread_self(), libc::SCHED_FIFO, &raw const param)
    };
    Some(result == 0)
}

/// Windows' audio system raises its own threads.
#[cfg(not(target_os = "linux"))]
fn raise_priority() -> Option<bool> {
    None
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use super::{AudioOutput, OutputMode};

    #[test]
    fn the_silent_stand_in_keeps_time() {
        let frames = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&frames);
        let output = AudioOutput::start_silent(48_000, move |buffer, channels| {
            counter.fetch_add(buffer.len() / channels, Ordering::Relaxed);
        });
        std::thread::sleep(Duration::from_millis(600));
        let status = output.status();
        drop(output);
        assert_eq!(status.mode, OutputMode::Silent);
        let rendered = frames.load(Ordering::Relaxed);
        // About half a second of audio (24,000 frames), with slack for a
        // busy test machine.
        assert!(
            (12_000..=40_000).contains(&rendered),
            "rendered {rendered} frames in 0.6 s"
        );
    }
}
