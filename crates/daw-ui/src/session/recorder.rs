// SPDX-License-Identifier: GPL-3.0-or-later

//! The recorder (decision 0017): a thread that takes the sound of takes as
//! the audio thread passes it on, writes each take to a WAV file in the
//! song's audio folder as it comes, keeps a summary of its loudness for
//! drawing it while it records, and, when the take ends, hands back its
//! sound ready to play.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use daw_dsp::audio::{AudioBuffer, Peaks};
use daw_engine::{RecordedAudio, RecordedChunk};
use daw_formats::wav::{WavFormat, WavWriter};

/// How long to wait for the last chunks of a take after it ended.
const LATE_CHUNKS: Duration = Duration::from_secs(2);

/// What the session tells the recorder.
enum Order {
    /// Take `take` is recording: write it to `path`.
    Begin { take: u32, path: PathBuf },
    /// Take `take` ended after `frames` frames.
    End { take: u32, frames: u64 },
}

/// A finished take.
pub(crate) struct Finished {
    pub take: u32,
    pub result: Result<TakeSound, String>,
}

/// The sound of a finished take.
pub(crate) struct TakeSound {
    /// The WAV file it was written to.
    pub path: PathBuf,
    /// Its sound, at the engine's rate, to play.
    pub sound: Arc<AudioBuffer>,
    /// A summary of its loudness, for drawing.
    pub peaks: Arc<Peaks>,
    /// Its length, in frames at the input's rate.
    pub frames: u64,
    /// The input's rate.
    pub sample_rate: u32,
    /// 1 or 2.
    pub channels: u16,
}

/// The take being recorded, for drawing it as it grows.
#[derive(Clone)]
pub(crate) struct Live {
    pub take: u32,
    pub peaks: Peaks,
}

/// The recorder thread.
pub(crate) struct Recorder {
    orders: Option<mpsc::Sender<Order>>,
    finished: mpsc::Receiver<Finished>,
    live: Arc<Mutex<Option<Live>>>,
    thread: Option<JoinHandle<()>>,
}

impl Recorder {
    /// Starts the recorder on the engine's recorded sound; finished takes
    /// are converted to `engine_rate`.
    pub fn start(source: RecordedAudio, engine_rate: u32) -> Self {
        let (orders, receive_orders) = mpsc::channel();
        let (send_finished, finished) = mpsc::channel();
        let live = Arc::new(Mutex::new(None));
        let shared_live = Arc::clone(&live);
        let thread = std::thread::Builder::new()
            .name("recorder".to_owned())
            .spawn(move || {
                run(
                    source,
                    &receive_orders,
                    &send_finished,
                    &shared_live,
                    engine_rate,
                );
            })
            .ok();
        Self {
            orders: Some(orders),
            finished,
            live,
            thread,
        }
    }

    /// Take `take` started: it is written to `path`.
    pub fn begin(&self, take: u32, path: PathBuf) {
        if let Some(orders) = &self.orders {
            let _ = orders.send(Order::Begin { take, path });
        }
    }

    /// Take `take` ended after `frames` frames.
    pub fn end(&self, take: u32, frames: u64) {
        if let Some(orders) = &self.orders {
            let _ = orders.send(Order::End { take, frames });
        }
    }

    /// A take that has finished since the last call.
    pub fn poll(&self) -> Option<Finished> {
        self.finished.try_recv().ok()
    }

    /// The take being recorded, as it is now.
    pub fn live(&self) -> Option<Live> {
        self.live
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.orders = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// One take on its way to a file.
#[derive(Default)]
struct TakeState {
    path: Option<PathBuf>,
    writer: Option<WavWriter>,
    sample_rate: u32,
    channels: u16,
    left: Vec<f32>,
    right: Vec<f32>,
    written: u64,
    expected: Option<u64>,
    ended: Option<Instant>,
    /// Chunks that came before the session named the take's file.
    early: Vec<RecordedChunk>,
    problem: Option<String>,
}

fn run(
    mut source: RecordedAudio,
    orders: &mpsc::Receiver<Order>,
    finished: &mpsc::Sender<Finished>,
    live: &Mutex<Option<Live>>,
    engine_rate: u32,
) {
    let mut takes: HashMap<u32, TakeState> = HashMap::new();
    loop {
        match orders.recv_timeout(Duration::from_millis(5)) {
            Ok(Order::Begin { take, path }) => {
                let state = takes.entry(take).or_default();
                state.path = Some(path);
                *live.lock().unwrap_or_else(PoisonError::into_inner) = Some(Live {
                    take,
                    peaks: Peaks::new(engine_rate),
                });
                for chunk in std::mem::take(&mut state.early) {
                    write(state, &chunk, live);
                }
            }
            Ok(Order::End { take, frames }) => {
                let state = takes.entry(take).or_default();
                state.expected = Some(frames);
                state.ended = Some(Instant::now());
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        while let Some(chunk) = source.pop() {
            let state = takes.entry(chunk.take).or_default();
            if state.path.is_some() {
                write(state, &chunk, live);
            } else {
                state.early.push(chunk);
            }
        }
        let done: Vec<u32> = takes
            .iter()
            .filter(|(_, s)| {
                s.path.is_some() && s.expected.is_some_and(|frames| s.written >= frames)
                    || s.ended.is_some_and(|at| at.elapsed() > LATE_CHUNKS)
            })
            .map(|(&take, _)| take)
            .collect();
        for take in done {
            if let Some(state) = takes.remove(&take) {
                let result = finish(state, engine_rate);
                let mut current = live.lock().unwrap_or_else(PoisonError::into_inner);
                if current.as_ref().is_some_and(|l| l.take == take) {
                    *current = None;
                }
                drop(current);
                let _ = finished.send(Finished { take, result });
            }
        }
    }
    // The program is closing: unfinished takes are let go.
    for (_, state) in takes {
        if let Some(writer) = state.writer {
            writer.cancel();
        }
    }
}

/// Adds a chunk to its take: to the file, the sound kept in memory and the
/// loudness summary.
fn write(state: &mut TakeState, chunk: &RecordedChunk, live: &Mutex<Option<Live>>) {
    if state.problem.is_some() {
        return;
    }
    if state.writer.is_none() {
        let Some(path) = state.path.clone() else {
            return;
        };
        state.sample_rate = chunk.sample_rate.max(1);
        state.channels = u16::from(chunk.channels.clamp(1, 2));
        match WavWriter::create_with_channels(
            &path,
            WavFormat::Pcm24,
            state.sample_rate,
            state.channels,
        ) {
            Ok(writer) => state.writer = Some(writer),
            Err(error) => {
                state.problem = Some(error.to_string());
                return;
            }
        }
    }
    // A gap (the recorder fell behind and chunks were lost) becomes
    // silence, so what follows stays in time.
    let gap = usize::try_from(chunk.first.saturating_sub(state.written)).unwrap_or(0);
    let silence = vec![0.0_f32; 2 * gap];
    for samples in [&silence[..], chunk.samples()] {
        let frames = samples.len() / 2;
        if frames == 0 {
            continue;
        }
        let pairs = samples.as_chunks::<2>().0;
        let result = match state.writer.as_mut() {
            Some(writer) if state.channels == 1 => {
                let mono: Vec<f32> = pairs.iter().map(|p| p[0]).collect();
                writer.write_interleaved(&mono)
            }
            Some(writer) => writer.write_interleaved(samples),
            None => Ok(()),
        };
        if let Err(error) = result {
            state.problem = Some(error.to_string());
            return;
        }
        state.left.extend(pairs.iter().map(|p| p[0]));
        if state.channels == 2 {
            state.right.extend(pairs.iter().map(|p| p[1]));
        }
        state.written += frames as u64;
        if let Some(current) = live
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_mut()
            .filter(|l| l.take == chunk.take)
        {
            if current.peaks.sample_rate() != state.sample_rate {
                current.peaks = Peaks::new(state.sample_rate);
            }
            current.peaks.push_interleaved(samples, 2);
        }
    }
}

/// Completes a take's file and makes its sound ready to play.
fn finish(mut state: TakeState, engine_rate: u32) -> Result<TakeSound, String> {
    if let Some(problem) = state.problem.take() {
        if let Some(writer) = state.writer.take() {
            writer.cancel();
        }
        return Err(problem);
    }
    let (Some(writer), Some(path)) = (state.writer.take(), state.path.take()) else {
        return Err(String::new());
    };
    if state.written == 0 {
        writer.cancel();
        return Err(String::new());
    }
    writer.finish().map_err(|e| e.to_string())?;
    let channels = if state.channels == 2 {
        vec![state.left, state.right]
    } else {
        vec![state.left]
    };
    let recorded = AudioBuffer::new(state.sample_rate, channels);
    let sound = recorded.resampled(engine_rate);
    let peaks = Peaks::of(&sound);
    Ok(TakeSound {
        path,
        sound: Arc::new(sound),
        peaks: Arc::new(peaks),
        frames: state.written,
        sample_rate: state.sample_rate,
        channels: state.channels,
    })
}
