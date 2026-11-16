// SPDX-License-Identifier: GPL-3.0-or-later

//! Phase 0 spike (a): low-latency audio output on Windows and Linux.
//!
//! Opens the system's default output device with no configuration, asks for
//! progressively larger buffers until one is accepted, plays a short beeping
//! test tone and measures what actually happened: the buffer size the system
//! gave us, how regularly the audio callback ran, the output latency the
//! system reported, dropouts, and whether the callback ever allocated memory.
//!
//! Usage: `spike-audio-latency [--seconds N] [--buffer FRAMES] [--host NAME] [--list]`
//!
//! Throwaway code: it exists to answer a question, and its findings are
//! recorded in docs/decisions/0009-phase-0-spike-results.md.

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a measuring tool reports by printing"
)]
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "statistics on small numbers"
)]

use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, FromSample, SampleFormat, SizedSample, StreamConfig};

#[global_allocator]
static ALLOCATOR: daw_rt::RtCheckedAllocator = daw_rt::RtCheckedAllocator;

/// One audio callback, as seen from inside it.
#[derive(Clone, Copy, Default)]
struct CallbackRecord {
    /// When the callback started, in nanoseconds since the stream was built.
    started_ns: u64,
    /// How long our own processing took, in nanoseconds.
    busy_ns: u64,
    /// Frames the system asked for.
    frames: u32,
    /// The system's own estimate of output latency (playback minus callback
    /// time), in nanoseconds, when it provides one.
    reported_latency_ns: u64,
}

struct Options {
    seconds: f64,
    buffer: Option<u32>,
    host: Option<String>,
    list: bool,
}

fn parse_options() -> Result<Options, String> {
    let mut options = Options {
        seconds: 5.0,
        buffer: None,
        host: None,
        list: false,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--seconds" => {
                let value = args.next().ok_or("--seconds needs a value")?;
                options.seconds = value.parse().map_err(|_| "--seconds needs a number")?;
            }
            "--buffer" => {
                let value = args.next().ok_or("--buffer needs a value")?;
                options.buffer = Some(value.parse().map_err(|_| "--buffer needs a number")?);
            }
            "--host" => options.host = Some(args.next().ok_or("--host needs a value")?),
            "--list" => options.list = true,
            other => return Err(format!("unknown option `{other}`")),
        }
    }
    Ok(options)
}

fn main() -> ExitCode {
    let options = match parse_options() {
        Ok(options) => options,
        Err(message) => {
            eprintln!("error: {message}");
            return ExitCode::FAILURE;
        }
    };
    // Report violations at the end instead of aborting, so the report still
    // prints; the exit code reflects them.
    daw_rt::set_on_violation(daw_rt::OnViolation::Count);

    println!("== Audio hosts compiled in and available on this system");
    for id in cpal::available_hosts() {
        println!("  {}", id.name());
    }

    let host = match &options.host {
        None => cpal::default_host(),
        Some(name) => {
            let Some(id) = cpal::available_hosts()
                .into_iter()
                .find(|id| id.name().eq_ignore_ascii_case(name))
            else {
                eprintln!("error: no audio host called `{name}` is available");
                return ExitCode::FAILURE;
            };
            match cpal::host_from_id(id) {
                Ok(host) => host,
                Err(error) => {
                    eprintln!("error: could not open host `{name}`: {error}");
                    return ExitCode::FAILURE;
                }
            }
        }
    };
    println!("== Using host: {}", host.id().name());

    if options.list {
        list_devices(&host);
    }

    let Some(device) = host.default_output_device() else {
        println!("RESULT no-device: this system has no default output device");
        return ExitCode::from(2);
    };
    let name = device
        .description()
        .map_or_else(|error| format!("(unnamed: {error})"), |d| d.to_string());
    println!("== Default output device: {name}");

    let default_config = match device.default_output_config() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("error: the device has no default configuration: {error}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "   default: {} channel(s), {} Hz, {:?}, buffer sizes {:?}",
        default_config.channels(),
        default_config.sample_rate(),
        default_config.sample_format(),
        default_config.buffer_size()
    );

    let outcome = match default_config.sample_format() {
        SampleFormat::F32 => run::<f32>(&device, &default_config, &options),
        SampleFormat::I16 => run::<i16>(&device, &default_config, &options),
        SampleFormat::I32 => run::<i32>(&device, &default_config, &options),
        SampleFormat::U16 => run::<u16>(&device, &default_config, &options),
        other => Err(format!(
            "sample format {other} is not handled by this spike"
        )),
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("RESULT failed: {message}");
            ExitCode::FAILURE
        }
    }
}

fn list_devices(host: &cpal::Host) {
    println!("== Output devices");
    match host.output_devices() {
        Ok(devices) => {
            for device in devices {
                let name = device
                    .description()
                    .map_or_else(|_| "(unnamed)".to_owned(), |d| d.to_string());
                println!("  {name}");
                if let Ok(configs) = device.supported_output_configs() {
                    for config in configs {
                        println!(
                            "     {} ch, {}-{} Hz, {:?}, buffer {:?}",
                            config.channels(),
                            config.min_sample_rate(),
                            config.max_sample_rate(),
                            config.sample_format(),
                            config.buffer_size()
                        );
                    }
                }
            }
        }
        Err(error) => println!("  (could not list devices: {error})"),
    }
}

/// The buffer sizes to try, smallest first. The first one the system
/// accepts is the one measured.
fn candidate_buffers(requested: Option<u32>) -> Vec<BufferSize> {
    match requested {
        Some(frames) => vec![BufferSize::Fixed(frames)],
        None => vec![
            BufferSize::Fixed(64),
            BufferSize::Fixed(128),
            BufferSize::Fixed(256),
            BufferSize::Fixed(512),
            BufferSize::Default,
        ],
    }
}

#[allow(clippy::too_many_lines, reason = "a linear measuring script")]
fn run<T>(
    device: &cpal::Device,
    default_config: &cpal::SupportedStreamConfig,
    options: &Options,
) -> Result<(), String>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let channels = usize::from(default_config.channels());
    let sample_rate = default_config.sample_rate();

    // Room for every callback of the run, reserved before audio starts.
    let capacity = ((options.seconds + 2.0) * f64::from(sample_rate) / 16.0) as usize;
    let errors = Arc::new(AtomicUsize::new(0));
    let xruns = Arc::new(AtomicUsize::new(0));
    let priority = Arc::new(AtomicU32::new(PRIORITY_UNTRIED));

    let mut chosen = None;
    for buffer in candidate_buffers(options.buffer) {
        let config = StreamConfig {
            channels: default_config.channels(),
            sample_rate,
            buffer_size: buffer,
        };
        let (producer, consumer) = rtrb::RingBuffer::<CallbackRecord>::new(capacity);
        let callback = make_callback::<T>(channels, sample_rate, producer, Arc::clone(&priority));
        let errors_seen = Arc::clone(&errors);
        let xruns_seen = Arc::clone(&xruns);
        let on_error = move |error: cpal::Error| {
            if error.kind() == cpal::ErrorKind::Xrun {
                xruns_seen.fetch_add(1, Ordering::Relaxed);
            } else {
                errors_seen.fetch_add(1, Ordering::Relaxed);
                eprintln!("   stream error: {error}");
            }
        };
        match device.build_output_stream(config, callback, on_error, None) {
            Ok(stream) => {
                println!("== Accepted buffer request: {buffer:?}");
                chosen = Some((stream, consumer, buffer));
                break;
            }
            Err(error) => println!("   buffer request {buffer:?} refused: {error}"),
        }
    }
    let Some((stream, mut consumer, requested)) = chosen else {
        return Err("no buffer size was accepted".to_owned());
    };
    let granted = stream.buffer_size().ok();
    println!("   the stream reports a buffer of {granted:?} frames");

    stream
        .play()
        .map_err(|error| format!("could not start: {error}"))?;
    let started = Instant::now();
    let mut records = Vec::with_capacity(capacity);
    while started.elapsed() < Duration::from_secs_f64(options.seconds) {
        std::thread::sleep(Duration::from_millis(50));
        while let Ok(record) = consumer.pop() {
            records.push(record);
        }
    }
    drop(stream);
    while let Ok(record) = consumer.pop() {
        records.push(record);
    }

    report(&Report {
        requested,
        granted,
        sample_rate,
        channels,
        records: &records,
        errors: errors.load(Ordering::Relaxed),
        xruns: xruns.load(Ordering::Relaxed),
        priority: priority.load(Ordering::Relaxed),
        seconds: options.seconds,
    })
}

const PRIORITY_UNTRIED: u32 = 0;
const PRIORITY_RAISED: u32 = 1;
const PRIORITY_REFUSED: u32 = 2;
const PRIORITY_LEFT_TO_SYSTEM: u32 = 3;

/// Builds the audio callback. Everything it needs is moved in and reserved
/// here; inside, it runs as real-time code.
fn make_callback<T>(
    channels: usize,
    sample_rate: u32,
    mut producer: rtrb::Producer<CallbackRecord>,
    priority: Arc<AtomicU32>,
) -> impl FnMut(&mut [T], &cpal::OutputCallbackInfo) + Send + 'static
where
    T: SizedSample + FromSample<f32>,
{
    let origin = Instant::now();
    let mut tone = Beeper::new(sample_rate as f32);
    move |data: &mut [T], info: &cpal::OutputCallbackInfo| {
        let started = Instant::now();
        let _rt = daw_rt::RtGuard::enter();

        if priority.load(Ordering::Relaxed) == PRIORITY_UNTRIED {
            priority.store(raise_thread_priority(), Ordering::Relaxed);
        }

        for frame in data.chunks_mut(channels) {
            let value = T::from_sample(tone.next_sample());
            for sample in frame {
                *sample = value;
            }
        }

        let timestamp = info.timestamp();
        let reported = timestamp
            .playback
            .checked_duration_since(timestamp.callback)
            .map_or(0, |latency| latency.as_nanos() as u64);
        let record = CallbackRecord {
            started_ns: started.duration_since(origin).as_nanos() as u64,
            busy_ns: started.elapsed().as_nanos() as u64,
            frames: (data.len() / channels) as u32,
            reported_latency_ns: reported,
        };
        // A full queue only loses measurements, never sound.
        let _ = producer.push(record);
    }
}

/// Asks the system to run the audio thread at real-time priority.
#[cfg(target_os = "linux")]
#[allow(unsafe_code, reason = "a plain system call with no memory handed over")]
fn raise_thread_priority() -> u32 {
    let param = libc::sched_param { sched_priority: 70 };
    // SAFETY: pthread_self() is always valid for the calling thread, and
    // `param` is a fully initialised struct that outlives the call.
    let result = unsafe {
        libc::pthread_setschedparam(libc::pthread_self(), libc::SCHED_FIFO, &raw const param)
    };
    if result == 0 {
        PRIORITY_RAISED
    } else {
        PRIORITY_REFUSED
    }
}

/// On Windows, cpal already raises its audio thread to time-critical priority.
#[cfg(not(target_os = "linux"))]
fn raise_thread_priority() -> u32 {
    PRIORITY_LEFT_TO_SYSTEM
}

/// A 440 Hz tone that beeps: 150 ms on, 350 ms off, with short fades so the
/// beeps do not click. Loud enough to hear, quiet enough not to startle.
struct Beeper {
    phase: f32,
    step: f32,
    position: u32,
    period: u32,
    on: u32,
    fade: u32,
}

impl Beeper {
    fn new(sample_rate: f32) -> Self {
        Self {
            phase: 0.0,
            step: 440.0 / sample_rate,
            position: 0,
            period: (sample_rate * 0.5) as u32,
            on: (sample_rate * 0.15) as u32,
            fade: (sample_rate * 0.005) as u32,
        }
    }

    fn next_sample(&mut self) -> f32 {
        let position = self.position;
        self.position = (self.position + 1) % self.period;
        self.phase = (self.phase + self.step).fract();
        if position >= self.on {
            return 0.0;
        }
        let fade_in = (position as f32 / self.fade as f32).min(1.0);
        let fade_out = ((self.on - position) as f32 / self.fade as f32).min(1.0);
        let level = 0.125; // about -18 dBFS
        (self.phase * std::f32::consts::TAU).sin() * level * fade_in * fade_out
    }
}

struct Report<'a> {
    requested: BufferSize,
    granted: Option<u32>,
    sample_rate: u32,
    channels: usize,
    records: &'a [CallbackRecord],
    errors: usize,
    xruns: usize,
    priority: u32,
    seconds: f64,
}

fn report(report: &Report<'_>) -> Result<(), String> {
    let records = report.records;
    if records.len() < 3 {
        return Err(format!(
            "the audio callback ran only {} time(s) in {} s",
            records.len(),
            report.seconds
        ));
    }
    // Ignore the first few callbacks: start-up is allowed to be irregular.
    let steady = &records[2..];
    let frames: Vec<u32> = steady.iter().map(|r| r.frames).collect();
    let min_frames = frames.iter().copied().min().unwrap_or(0);
    let max_frames = frames.iter().copied().max().unwrap_or(0);
    let mean_frames = frames.iter().map(|&f| f64::from(f)).sum::<f64>() / frames.len() as f64;
    let period_ms = mean_frames / f64::from(report.sample_rate) * 1000.0;

    let mut intervals: Vec<f64> = steady
        .windows(2)
        .map(|pair| (pair[1].started_ns - pair[0].started_ns) as f64 / 1e6)
        .collect();
    intervals.sort_by(f64::total_cmp);
    let late = intervals.iter().filter(|&&ms| ms > period_ms * 1.5).count();

    let mut busy: Vec<f64> = steady.iter().map(|r| r.busy_ns as f64 / 1e6).collect();
    busy.sort_by(f64::total_cmp);

    let mut latency: Vec<f64> = steady
        .iter()
        .filter(|r| r.reported_latency_ns > 0)
        .map(|r| r.reported_latency_ns as f64 / 1e6)
        .collect();
    latency.sort_by(f64::total_cmp);

    let violations = daw_rt::violation_count();
    let priority = match report.priority {
        PRIORITY_RAISED => "raised to real-time (SCHED_FIFO)",
        PRIORITY_REFUSED => "real-time priority refused by the system",
        PRIORITY_LEFT_TO_SYSTEM => "set by the audio backend",
        _ => "not attempted",
    };

    println!("== Results over {:.1} s", report.seconds);
    println!("   requested buffer       {:?}", report.requested);
    println!("   stream buffer          {:?} frames", report.granted);
    println!(
        "   format                 {} Hz, {} channel(s)",
        report.sample_rate, report.channels
    );
    println!("   callbacks              {}", records.len());
    let first_s = records[0].started_ns as f64 / 1e9;
    let last_s = records[records.len() - 1].started_ns as f64 / 1e9;
    println!("   first/last callback    {first_s:.3} s / {last_s:.3} s after the stream was built");
    println!("   frames per callback    min {min_frames}, mean {mean_frames:.1}, max {max_frames}");
    println!("   callback period        {period_ms:.2} ms (mean frames / sample rate)");
    println!(
        "   interval between calls p50 {:.2} ms, p99 {:.2} ms, max {:.2} ms",
        percentile(&intervals, 0.50),
        percentile(&intervals, 0.99),
        percentile(&intervals, 1.0)
    );
    println!("   late callbacks (>1.5x)  {late}");
    println!(
        "   time spent in callback p50 {:.3} ms, max {:.3} ms",
        percentile(&busy, 0.50),
        percentile(&busy, 1.0)
    );
    if latency.is_empty() {
        println!("   reported output latency (not provided by this backend)");
    } else {
        println!(
            "   reported output latency p50 {:.2} ms, max {:.2} ms",
            percentile(&latency, 0.50),
            percentile(&latency, 1.0)
        );
    }
    println!("   dropouts reported      {}", report.xruns);
    println!("   stream errors          {}", report.errors);
    println!("   audio thread priority  {priority}");
    println!("   allocations in callback {violations}");

    if violations > 0 {
        return Err(format!(
            "the audio callback allocated memory {violations} time(s)"
        ));
    }
    println!(
        "RESULT ok: buffer {} frames ({period_ms:.2} ms at {} Hz)",
        mean_frames.round(),
        report.sample_rate
    );
    Ok(())
}

fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let index = ((sorted.len() - 1) as f64 * fraction).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}
