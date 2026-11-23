// SPDX-License-Identifier: GPL-3.0-or-later

//! Writing the song to a sound file (WAV or FLAC) in the background. A second engine renders
//! it offline through exactly the same processing as live playback, block by
//! block, so a long song never has to fit in memory; the window keeps working
//! and shows the progress, and the export can be cancelled. The song plays
//! once from its start (its loop region is for playing, not exporting) to
//! the end of its last clip, and on while notes and effects still ring; the
//! file ends where the sound does, with no silence after it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread::JoinHandle;

use daw_engine::{EngineConfig, OfflineRender, SoundLibrary};
use daw_formats::flac::{FlacBits, FlacWriter};
use daw_formats::wav::{WavFormat, WavWriter};
use daw_model::Project;

/// What an export writes (decision 0020).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExportTarget {
    /// A WAV sound file.
    #[default]
    Wav,
    /// A FLAC sound file (smaller, lossless).
    Flac,
    /// A DAWproject file, for other music programs.
    DawProject,
    /// A Standard MIDI File: the notes and the tempo.
    Midi,
}

impl ExportTarget {
    /// The file ending, without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Flac => "flac",
            Self::DawProject => "dawproject",
            Self::Midi => "mid",
        }
    }

    /// Whether it is a sound file (rendered in the background).
    pub fn is_sound(self) -> bool {
        matches!(self, Self::Wav | Self::Flac)
    }

    /// `path` with this ending (added unless it has it already).
    pub fn with_extension(self, path: &Path) -> PathBuf {
        let wanted = self.extension();
        let has_it = path.extension().is_some_and(|e| {
            e.eq_ignore_ascii_case(wanted) || (self == Self::Midi && e.eq_ignore_ascii_case("midi"))
        });
        if has_it {
            path.to_path_buf()
        } else {
            let mut name = path.as_os_str().to_owned();
            name.push(".");
            name.push(wanted);
            PathBuf::from(name)
        }
    }
}

/// A sound file being written: WAV or FLAC.
enum SoundWriter {
    Wav(WavWriter),
    Flac(Box<FlacWriter>),
}

impl SoundWriter {
    fn create(
        path: &Path,
        target: ExportTarget,
        format: ExportFormat,
        rate: u32,
    ) -> std::io::Result<Self> {
        Ok(match target {
            ExportTarget::Flac => {
                Self::Flac(Box::new(FlacWriter::create(path, format.flac(), rate)?))
            }
            _ => Self::Wav(WavWriter::create(path, format.wav(), rate)?),
        })
    }

    fn write(&mut self, left: &[f32], right: &[f32]) -> std::io::Result<()> {
        match self {
            Self::Wav(w) => w.write(left, right),
            Self::Flac(w) => w.write(left, right),
        }
    }

    fn finish(self) -> std::io::Result<()> {
        match self {
            Self::Wav(w) => w.finish(),
            Self::Flac(w) => w.finish(),
        }
    }

    fn cancel(self) {
        match self {
            Self::Wav(w) => w.cancel(),
            Self::Flac(w) => w.cancel(),
        }
    }
}

/// The kinds of file offered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExportFormat {
    /// 44.1 kHz, 16 bits: CD quality, plays everywhere.
    Cd,
    /// 48 kHz, 24 bits: studio quality, for further work.
    #[default]
    Studio,
}

impl ExportFormat {
    /// The sample rate of the file.
    pub fn sample_rate(self) -> u32 {
        match self {
            Self::Cd => 44_100,
            Self::Studio => 48_000,
        }
    }

    fn wav(self) -> WavFormat {
        match self {
            Self::Cd => WavFormat::Pcm16,
            Self::Studio => WavFormat::Pcm24,
        }
    }

    fn flac(self) -> FlacBits {
        match self {
            Self::Cd => FlacBits::Bits16,
            Self::Studio => FlacBits::Bits24,
        }
    }
}

/// How quiet the end must be for the tail to count as over (-80 dBFS).
const QUIET: f32 = 1e-4;
/// How long it must stay that quiet, in seconds, for the tail to be over
/// (a delay's next echo can come after a short silence). This silence is
/// not written.
const QUIET_SECONDS: u32 = 1;
/// The longest tail written after the song's end, in seconds.
const MAX_TAIL_SECONDS: u32 = 15;
/// Frames rendered at a time.
const BLOCK: usize = 4096;

/// What an export ended with: the file written, nothing (cancelled), or
/// what went wrong.
pub type ExportResult = Result<Option<PathBuf>, String>;

/// An export running in the background.
pub struct Export {
    thread: Option<JoinHandle<ExportResult>>,
    result: Option<ExportResult>,
    progress: Arc<AtomicU32>,
    cancel: Arc<AtomicBool>,
}

impl Export {
    /// Starts rendering `project` into a sound file at `path` (WAV or
    /// FLAC, as `target` says).
    pub fn start(
        project: Project,
        library: SoundLibrary,
        config: EngineConfig,
        path: PathBuf,
        format: ExportFormat,
        target: ExportTarget,
    ) -> Self {
        let progress = Arc::new(AtomicU32::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        let job = Job {
            project,
            library,
            config: EngineConfig {
                sample_rate: format.sample_rate(),
                max_block: config.max_block.max(64),
            },
            path,
            format,
            target,
            progress: Arc::clone(&progress),
            cancel: Arc::clone(&cancel),
        };
        let thread = std::thread::Builder::new()
            .name("export".to_owned())
            .spawn(move || job.run());
        match thread {
            Ok(thread) => Self {
                thread: Some(thread),
                result: None,
                progress,
                cancel,
            },
            Err(error) => Self {
                thread: None,
                result: Some(Err(error.to_string())),
                progress,
                cancel,
            },
        }
    }

    /// How far it has come, from 0 to 1.
    pub fn progress(&self) -> f32 {
        f32::from_bits(self.progress.load(Ordering::Relaxed))
    }

    /// Asks it to stop; the file is not written.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Waits until it has ended.
    pub fn wait(&mut self) {
        if let Some(thread) = self.thread.take() {
            self.result = Some(
                thread
                    .join()
                    .unwrap_or_else(|_| Err("the export stopped unexpectedly".to_owned())),
            );
        }
    }

    /// The outcome, once it has ended (given once).
    pub fn result(&mut self) -> Option<ExportResult> {
        if self.thread.as_ref().is_some_and(JoinHandle::is_finished) {
            self.wait();
        }
        self.result.take()
    }
}

impl Drop for Export {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Everything the background thread needs.
struct Job {
    project: Project,
    library: SoundLibrary,
    config: EngineConfig,
    path: PathBuf,
    format: ExportFormat,
    target: ExportTarget,
    progress: Arc<AtomicU32>,
    cancel: Arc<AtomicBool>,
}

impl Job {
    fn run(self) -> ExportResult {
        let rate = self.config.sample_rate;
        let mut render = OfflineRender::new(&self.project, self.library, self.config);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a song's length in frames"
        )]
        let song_frames = (self.project.tempo.seconds_at(self.project.song_end()) * f64::from(rate))
            .ceil() as u64;
        let tail_limit = song_frames + u64::from(MAX_TAIL_SECONDS * rate);
        let quiet_needed = u64::from(QUIET_SECONDS * rate);
        let mut writer = SoundWriter::create(&self.path, self.target, self.format, rate)
            .map_err(|e| e.to_string())?;
        let mut left = vec![0.0_f32; BLOCK];
        let mut right = vec![0.0_f32; BLOCK];
        // After the song's end, quiet frames wait here: they are written
        // only if sound comes after them, so the file ends where the sound
        // does.
        let mut held_left: Vec<f32> = Vec::new();
        let mut held_right: Vec<f32> = Vec::new();
        loop {
            if self.cancel.load(Ordering::Relaxed) {
                writer.cancel();
                return Ok(None);
            }
            let before = render.frames();
            render.render(&mut left, &mut right);
            let done = render.frames();
            // The frames up to the song's end are always written.
            #[allow(clippy::cast_possible_truncation, reason = "within one block")]
            let in_song = song_frames.saturating_sub(before).min(BLOCK as u64) as usize;
            let last_loud = (in_song..BLOCK)
                .rev()
                .find(|&i| left[i].abs() >= QUIET || right[i].abs() >= QUIET);
            let keep = last_loud.map_or(in_song, |i| i + 1);
            if keep > in_song || in_song == BLOCK {
                // Sound goes on: what waited is part of the song after all.
                let written = writer
                    .write(&held_left, &held_right)
                    .and_then(|()| writer.write(&left[..keep], &right[..keep]));
                if let Err(error) = written {
                    writer.cancel();
                    return Err(error.to_string());
                }
                held_left.clear();
                held_right.clear();
                held_left.extend_from_slice(&left[keep..]);
                held_right.extend_from_slice(&right[keep..]);
            } else {
                if let Err(error) = writer.write(&left[..in_song], &right[..in_song]) {
                    writer.cancel();
                    return Err(error.to_string());
                }
                held_left.extend_from_slice(&left[in_song..]);
                held_right.extend_from_slice(&right[in_song..]);
            }
            if done >= song_frames && (held_left.len() as u64 >= quiet_needed || done >= tail_limit)
            {
                break;
            }
            #[allow(clippy::cast_precision_loss, reason = "a fraction for display")]
            let fraction = (done as f32 / song_frames.max(1) as f32).min(0.99);
            self.progress.store(fraction.to_bits(), Ordering::Relaxed);
        }
        writer.finish().map_err(|e| e.to_string())?;
        self.progress.store(1.0_f32.to_bits(), Ordering::Relaxed);
        Ok(Some(self.path))
    }
}

/// Adds ".wav" to a path that has another ending or none.
pub fn with_wav_extension(path: &Path) -> PathBuf {
    ExportTarget::Wav.with_extension(path)
}
