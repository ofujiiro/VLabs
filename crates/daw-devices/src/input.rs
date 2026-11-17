// SPDX-License-Identifier: GPL-3.0-or-later

//! Sound input from the system's default input device (decision 0017): the
//! built-in microphone, or an audio interface, with no setup.
//!
//! A watcher thread owns the input stream. The device is opened at its own
//! sample rate and format (Windows opens inputs only that way); the
//! callback is given float samples, the channel count and the rate. When
//! the device fails, disappears or stops delivering, it is reopened; when
//! there is none, the program carries on and says so.

#![allow(
    unsafe_code,
    reason = "the input callback is shared between streams through a cell guarded by an atomic flag"
)]

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, FromSample, Sample, SampleFormat, SizedSample, StreamConfig};

/// The input callback: interleaved float samples, how many channels a
/// frame has, and the sample rate.
pub type InputCallback = Box<dyn FnMut(&[f32], usize, u32) + Send>;

/// How long to wait between attempts to open a device.
const RETRY_INTERVAL: Duration = Duration::from_millis(1000);

/// How long a device may go without delivering sound before it is
/// reopened.
const STALL_LIMIT: Duration = Duration::from_secs(3);

/// The most samples converted at once (longer deliveries are split).
const SCRATCH_SAMPLES: usize = 16_384;

/// The input callback, shared by successive streams; only one runs at a
/// time, and the flag keeps the callback from being entered twice without
/// ever blocking (a second caller drops its sound).
struct CallbackSlot {
    busy: AtomicBool,
    callback: UnsafeCell<InputCallback>,
}

// SAFETY: `callback` is only accessed by whoever wins the `busy` flag, so
// there is never more than one access at a time.
unsafe impl Sync for CallbackSlot {}

impl CallbackSlot {
    fn run(&self, samples: &[f32], channels: usize, rate: u32) {
        if self
            .busy
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return;
        }
        // SAFETY: we hold the `busy` flag, so this is the only access.
        unsafe { (*self.callback.get())(samples, channels, rate) };
        self.busy.store(false, Ordering::Release);
    }
}

/// The state of the input, for display.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InputStatus {
    /// The device sound comes from, once one is open.
    pub device: Option<String>,
    /// Its sample rate.
    pub sample_rate: Option<u32>,
    /// Its channels.
    pub channels: Option<u16>,
    /// The last problem met, if any (no device, refused, failed).
    pub problem: Option<String>,
}

struct Shared {
    slot: CallbackSlot,
    status: Mutex<InputStatus>,
    stop: AtomicBool,
    /// How many times the device has delivered sound.
    calls: AtomicU64,
    /// The delay from the microphone to the callback, in microseconds, as
    /// the device last reported it (0 for unknown).
    latency_us: AtomicU32,
}

impl Shared {
    fn update(&self, change: impl FnOnce(&mut InputStatus)) {
        change(&mut self.status.lock().unwrap_or_else(PoisonError::into_inner));
    }
}

/// Sound input. Dropping it closes the device.
pub struct AudioInput {
    shared: Arc<Shared>,
    watcher: Option<JoinHandle<()>>,
}

impl AudioInput {
    /// Starts listening to the default input device; it opens a moment
    /// later, in the background. `callback` runs on the device's thread and
    /// must follow the real-time rules.
    pub fn start(callback: impl FnMut(&[f32], usize, u32) + Send + 'static) -> Self {
        let shared = Arc::new(Shared {
            slot: CallbackSlot {
                busy: AtomicBool::new(false),
                callback: UnsafeCell::new(Box::new(callback)),
            },
            status: Mutex::new(InputStatus::default()),
            stop: AtomicBool::new(false),
            calls: AtomicU64::new(0),
            latency_us: AtomicU32::new(0),
        });
        let watcher_shared = Arc::clone(&shared);
        let watcher = std::thread::Builder::new()
            .name("input-watcher".to_owned())
            .spawn(move || watch(&watcher_shared))
            .ok();
        Self { shared, watcher }
    }

    /// The state of the input now.
    pub fn status(&self) -> InputStatus {
        self.shared
            .status
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The delay from the microphone to the program, as the device reports
    /// it (`None` when it does not say).
    pub fn latency(&self) -> Option<Duration> {
        match self.shared.latency_us.load(Ordering::Relaxed) {
            0 => None,
            us => Some(Duration::from_micros(u64::from(us))),
        }
    }

    /// Whether sound has come in at all yet.
    pub fn has_delivered(&self) -> bool {
        self.shared.calls.load(Ordering::Relaxed) > 0
    }
}

impl Drop for AudioInput {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(watcher) = self.watcher.take() {
            let _ = watcher.join();
        }
    }
}

/// The watcher: keeps an input open while it can.
fn watch(shared: &Arc<Shared>) {
    let mut stream: Option<(cpal::Stream, Arc<AtomicBool>)> = None;
    let mut last_attempt: Option<Instant> = None;
    let mut seen = (shared.calls.load(Ordering::Relaxed), Instant::now());
    while !shared.stop.load(Ordering::Relaxed) {
        if let Some((_, failed)) = &stream
            && failed.load(Ordering::Relaxed)
        {
            stream = None;
            shared.update(|s| {
                s.device = None;
                s.problem = Some("the sound input stopped; looking for another".to_owned());
            });
        }
        let calls = shared.calls.load(Ordering::Relaxed);
        if calls != seen.0 {
            seen = (calls, Instant::now());
        } else if stream.is_some() && seen.1.elapsed() >= STALL_LIMIT {
            stream = None;
            last_attempt = None;
            seen.1 = Instant::now();
            shared.update(|s| {
                s.device = None;
                s.problem = Some("the sound input stopped delivering; reopening it".to_owned());
            });
        }
        if stream.is_none() && last_attempt.is_none_or(|t| t.elapsed() >= RETRY_INTERVAL) {
            last_attempt = Some(Instant::now());
            match open_default(shared) {
                Ok(opened) => {
                    stream = Some(opened);
                    seen.1 = Instant::now();
                }
                Err(problem) => shared.update(|s| s.problem = Some(problem)),
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    drop(stream);
}

/// Opens the default input device at its own rate and format.
fn open_default(shared: &Arc<Shared>) -> Result<(cpal::Stream, Arc<AtomicBool>), String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or("no microphone or sound input was found")?;
    let name = device
        .description()
        .map_or_else(|_| "Sound input".to_owned(), |d| d.to_string());
    let default = device
        .default_input_config()
        .map_err(|e| format!("{name}: {e}"))?;
    let config = StreamConfig {
        channels: default.channels(),
        sample_rate: default.sample_rate(),
        buffer_size: BufferSize::Default,
    };
    let failed = Arc::new(AtomicBool::new(false));
    let stream = match default.sample_format() {
        SampleFormat::F32 => build::<f32>(&device, config, shared, &failed),
        SampleFormat::I16 => build::<i16>(&device, config, shared, &failed),
        SampleFormat::I32 => build::<i32>(&device, config, shared, &failed),
        SampleFormat::U16 => build::<u16>(&device, config, shared, &failed),
        SampleFormat::U8 => build::<u8>(&device, config, shared, &failed),
        other => return Err(format!("{name}: unsupported sample format {other}")),
    }
    .map_err(|e| format!("{name}: {e}"))?;
    stream.play().map_err(|e| format!("{name}: {e}"))?;
    shared.update(|s| {
        s.device = Some(name);
        s.sample_rate = Some(config.sample_rate);
        s.channels = Some(config.channels);
        s.problem = None;
    });
    Ok((stream, failed))
}

/// Builds an input stream for sample type `T`, converting to floats in a
/// buffer reserved here.
fn build<T>(
    device: &cpal::Device,
    config: StreamConfig,
    shared: &Arc<Shared>,
    failed: &Arc<AtomicBool>,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    let channels = usize::from(config.channels).max(1);
    let rate = config.sample_rate;
    let callback_shared = Arc::clone(shared);
    // Whole frames per conversion.
    let mut scratch = vec![0.0_f32; SCRATCH_SAMPLES - SCRATCH_SAMPLES % channels];
    let data_callback = move |data: &[T], info: &cpal::InputCallbackInfo| {
        let _real_time = daw_rt::RtGuard::enter();
        callback_shared.calls.fetch_add(1, Ordering::Relaxed);
        let stamp = info.timestamp();
        let delay = stamp.callback.duration_since(stamp.capture);
        callback_shared.latency_us.store(
            u32::try_from(delay.as_micros()).unwrap_or(u32::MAX),
            Ordering::Relaxed,
        );
        for chunk in data.chunks(scratch.len()) {
            let buffer = &mut scratch[..chunk.len()];
            for (out, &value) in buffer.iter_mut().zip(chunk) {
                *out = f32::from_sample(value);
            }
            callback_shared.slot.run(buffer, channels, rate);
        }
    };
    let error_flag = Arc::clone(failed);
    let error_callback = move |error: cpal::Error| {
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
    device.build_input_stream(config, data_callback, error_callback, None)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::AudioInput;

    #[test]
    fn starting_and_stopping_never_hangs_with_or_without_a_device() {
        let input = AudioInput::start(|_, _, _| {});
        std::thread::sleep(Duration::from_millis(300));
        let status = input.status();
        // Either a device opened, or the reason it did not is known.
        assert!(
            status.device.is_some() || status.problem.is_some() || !input.has_delivered(),
            "{status:?}"
        );
        drop(input);
    }
}
