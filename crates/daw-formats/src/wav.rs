// SPDX-License-Identifier: GPL-3.0-or-later

//! Writing WAV files, for exporting songs and keeping recordings. The sound
//! is written as it comes, so a long song never has to fit in memory, into a
//! temporary file that replaces the target only once complete.

use std::fs::File;
use std::io::{self, BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// How samples are stored.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WavFormat {
    /// 16-bit integers, with dither: the CD format, readable everywhere.
    Pcm16,
    /// 24-bit integers: the usual studio format.
    #[default]
    Pcm24,
    /// 32-bit floating point: lossless, for further processing.
    Float32,
}

impl WavFormat {
    fn bytes_per_sample(self) -> u16 {
        match self {
            Self::Pcm16 => 2,
            Self::Pcm24 => 3,
            Self::Float32 => 4,
        }
    }
}

/// A WAV file being written: stereo, or mono.
pub struct WavWriter {
    out: BufWriter<File>,
    temporary: PathBuf,
    target: PathBuf,
    format: WavFormat,
    sample_rate: u32,
    channels: u16,
    frames: u64,
    noise: u32,
}

/// Bytes before the sample data: the RIFF header, the format chunk, a
/// "fact" chunk (required for floating point) and the data chunk header.
const HEADER_BYTES: u64 = 12 + 26 + 12 + 8;

impl WavWriter {
    /// Starts writing a stereo file at `sample_rate`.
    pub fn create(path: &Path, format: WavFormat, sample_rate: u32) -> io::Result<Self> {
        Self::create_with_channels(path, format, sample_rate, 2)
    }

    /// Starts writing a file of `channels` channels (1 or 2) at
    /// `sample_rate`.
    pub fn create_with_channels(
        path: &Path,
        format: WavFormat,
        sample_rate: u32,
        channels: u16,
    ) -> io::Result<Self> {
        let channels = channels.clamp(1, 2);
        let name = path.file_name().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "the path has no file name")
        })?;
        let folder = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let temporary = folder.join(format!(
            ".{}.{}.partial",
            name.to_string_lossy(),
            std::process::id()
        ));
        let mut out = BufWriter::new(File::create(&temporary)?);
        write_header(&mut out, format, sample_rate, channels, 0)?;
        Ok(Self {
            out,
            temporary,
            target: path.to_path_buf(),
            format,
            sample_rate,
            channels,
            frames: 0,
            noise: 0x2545_f491,
        })
    }

    /// A random value from -1 to 1 (for dither).
    fn random(&mut self) -> f32 {
        self.noise ^= self.noise << 13;
        self.noise ^= self.noise >> 17;
        self.noise ^= self.noise << 5;
        #[allow(clippy::cast_precision_loss, reason = "noise values")]
        let value = self.noise as f32 / u32::MAX as f32;
        value * 2.0 - 1.0
    }

    /// Appends frames (both slices the same length; a mono file takes the
    /// left one).
    pub fn write(&mut self, left: &[f32], right: &[f32]) -> io::Result<()> {
        for (&l, &r) in left.iter().zip(right) {
            self.sample(l)?;
            if self.channels == 2 {
                self.sample(r)?;
            }
        }
        self.frames += left.len().min(right.len()) as u64;
        Ok(())
    }

    /// Appends interleaved frames of the file's channel count.
    pub fn write_interleaved(&mut self, samples: &[f32]) -> io::Result<()> {
        let channels = usize::from(self.channels);
        for frame in samples.chunks_exact(channels) {
            for &sample in frame {
                self.sample(sample)?;
            }
        }
        self.frames += (samples.len() / channels) as u64;
        Ok(())
    }

    fn sample(&mut self, sample: f32) -> io::Result<()> {
        let sample = if sample.is_finite() { sample } else { 0.0 };
        match self.format {
            WavFormat::Pcm16 => {
                // Triangular dither of one step hides the rounding.
                let (a, b) = (self.random(), self.random());
                let dither = f32::midpoint(a, b);
                #[allow(clippy::cast_possible_truncation, reason = "clamped to 16 bits")]
                let value = (sample * 32_767.0 + dither)
                    .round()
                    .clamp(-32_768.0, 32_767.0) as i16;
                self.out.write_all(&value.to_le_bytes())
            }
            WavFormat::Pcm24 => {
                #[allow(clippy::cast_possible_truncation, reason = "clamped to 24 bits")]
                let value = (sample * 8_388_607.0)
                    .round()
                    .clamp(-8_388_608.0, 8_388_607.0) as i32;
                self.out.write_all(&value.to_le_bytes()[..3])
            }
            WavFormat::Float32 => self.out.write_all(&sample.to_le_bytes()),
        }
    }

    /// Pushes buffered sound to the file without finishing it (as the
    /// system would before a crash; for tests).
    pub fn flush_for_test(&mut self) {
        let _ = self.out.flush();
    }

    /// Frames written so far.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Completes the file and puts it in place.
    pub fn finish(mut self) -> io::Result<()> {
        let data =
            self.frames * u64::from(self.channels) * u64::from(self.format.bytes_per_sample());
        if data % 2 == 1 {
            self.out.write_all(&[0])?; // Chunks are padded to an even size.
        }
        let frames = u32::try_from(self.frames)
            .map_err(|_| io::Error::other("the song is too long for a WAV file"))?;
        let mut file = self
            .out
            .into_inner()
            .map_err(io::IntoInnerError::into_error)?;
        file.seek(SeekFrom::Start(0))?;
        let mut header = BufWriter::new(&mut file);
        write_header(
            &mut header,
            self.format,
            self.sample_rate,
            self.channels,
            frames,
        )?;
        header.flush()?;
        drop(header);
        file.sync_all()?;
        drop(file);
        std::fs::rename(&self.temporary, &self.target)
    }

    /// Abandons the file (the target is left as it was).
    pub fn cancel(self) {
        drop(self.out);
        let _ = std::fs::remove_file(&self.temporary);
    }
}

/// What a repaired WAV file holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Repaired {
    /// Frames of sound.
    pub frames: u64,
    /// Frames per second.
    pub sample_rate: u32,
    /// Channels.
    pub channels: u16,
}

/// Mends a WAV file this module was writing when the program stopped (a
/// crash in the middle of a take, decision 0021): its sizes are worked
/// out from the sound written so far, a part-written last frame is cut
/// off. Files written by other programs are left alone.
pub fn repair(path: &Path) -> io::Result<Repaired> {
    let damaged = || io::Error::new(io::ErrorKind::InvalidData, "not a take this program wrote");
    let mut file = File::options().read(true).write(true).open(path)?;
    let mut header = [0_u8; 58];
    debug_assert_eq!(header.len() as u64, HEADER_BYTES);
    std::io::Read::read_exact(&mut file, &mut header).map_err(|_| damaged())?;
    let le16 = |at: usize| u16::from_le_bytes([header[at], header[at + 1]]);
    let le32 = |at: usize| {
        u32::from_le_bytes([header[at], header[at + 1], header[at + 2], header[at + 3]])
    };
    if &header[..4] != b"RIFF"
        || &header[8..16] != b"WAVEfmt "
        || le32(16) != 18
        || &header[38..42] != b"fact"
        || &header[50..54] != b"data"
    {
        return Err(damaged());
    }
    let tag = le16(20);
    let channels = le16(22);
    let sample_rate = le32(24);
    let bits = le16(34);
    let format = match (tag, bits) {
        (1, 16) => WavFormat::Pcm16,
        (1, 24) => WavFormat::Pcm24,
        (3, 32) => WavFormat::Float32,
        _ => return Err(damaged()),
    };
    if !(1..=2).contains(&channels) || sample_rate == 0 {
        return Err(damaged());
    }
    let block = u64::from(channels) * u64::from(format.bytes_per_sample());
    let length = file.metadata()?.len();
    let frames = length.saturating_sub(HEADER_BYTES) / block;
    let frames = u32::try_from(frames).unwrap_or(u32::MAX / 8);
    file.set_len(HEADER_BYTES + u64::from(frames) * block)?;
    file.seek(SeekFrom::Start(0))?;
    write_header(&mut file, format, sample_rate, channels, frames)?;
    file.sync_all()?;
    Ok(Repaired {
        frames: u64::from(frames),
        sample_rate,
        channels,
    })
}

/// The name of the file a [`WavWriter`] writes before it is complete, for
/// a target file `name` (any running copy of the program).
pub fn is_partial_of(candidate: &str, name: &str) -> bool {
    candidate
        .strip_prefix('.')
        .and_then(|rest| rest.strip_prefix(name))
        .and_then(|rest| rest.strip_prefix('.'))
        .is_some_and(|rest| rest.ends_with(".partial"))
}

fn write_header(
    out: &mut impl Write,
    format: WavFormat,
    sample_rate: u32,
    channels: u16,
    frames: u32,
) -> io::Result<()> {
    let bytes = format.bytes_per_sample();
    let block_align = channels * bytes;
    let data = u64::from(frames) * u64::from(block_align);
    let padded = data + data % 2;
    let riff = u32::try_from(HEADER_BYTES - 8 + padded).unwrap_or(u32::MAX);
    let tag: u16 = if format == WavFormat::Float32 { 3 } else { 1 };
    out.write_all(b"RIFF")?;
    out.write_all(&riff.to_le_bytes())?;
    out.write_all(b"WAVE")?;
    out.write_all(b"fmt ")?;
    out.write_all(&18_u32.to_le_bytes())?;
    out.write_all(&tag.to_le_bytes())?;
    out.write_all(&channels.to_le_bytes())?;
    out.write_all(&sample_rate.to_le_bytes())?;
    out.write_all(&(sample_rate * u32::from(block_align)).to_le_bytes())?;
    out.write_all(&block_align.to_le_bytes())?;
    out.write_all(&(bytes * 8).to_le_bytes())?;
    out.write_all(&0_u16.to_le_bytes())?; // No extension.
    out.write_all(b"fact")?;
    out.write_all(&4_u32.to_le_bytes())?;
    out.write_all(&frames.to_le_bytes())?;
    out.write_all(b"data")?;
    out.write_all(&u32::try_from(data).unwrap_or(u32::MAX).to_le_bytes())?;
    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::chunks_exact_to_as_chunks,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "test code: reads samples back in the plainest way"
)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("wav-test-{}-{name}.wav", std::process::id()))
    }

    #[test]
    fn a_take_cut_off_by_a_crash_is_repaired() {
        let path = temp("crash");
        let mut writer =
            WavWriter::create_with_channels(&path, WavFormat::Pcm24, 48_000, 1).unwrap();
        let samples: Vec<f32> = (0..1000).map(|i| (i as f32 * 0.01).sin() * 0.5).collect();
        writer.write_interleaved(&samples).unwrap();
        writer.out.flush().unwrap();
        // The program stops here: the file has its first header (no sound
        // in it) and a frame and a half more than whole frames.
        let partial = writer.temporary.clone();
        std::mem::forget(writer);
        {
            let mut file = File::options().append(true).open(&partial).unwrap();
            file.write_all(&[1, 2]).unwrap();
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let partial_name = partial.file_name().unwrap().to_string_lossy().into_owned();
        assert!(is_partial_of(&partial_name, &name));
        assert!(!is_partial_of("other.wav", &name));
        let repaired = repair(&partial).unwrap();
        assert_eq!(
            repaired,
            Repaired {
                frames: 1000,
                sample_rate: 48_000,
                channels: 1
            }
        );
        let (_, bits, rate, back) = read(&partial);
        assert_eq!((bits, rate, back.len()), (24, 48_000, 1000));
        assert!((back[500] - samples[500]).abs() < 1e-4);
        assert!(repair(Path::new("/nonexistent/take.wav")).is_err());
        std::fs::remove_file(partial).ok();
    }

    /// Reads back a file this module wrote: (format tag, bits, rate, samples).
    fn read(path: &Path) -> (u16, u16, u32, Vec<f32>) {
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        let riff = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        assert_eq!(riff + 8, bytes.len(), "the RIFF size covers the file");
        let tag = u16::from_le_bytes([bytes[20], bytes[21]]);
        let rate = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
        let bits = u16::from_le_bytes([bytes[34], bytes[35]]);
        let data_len = u32::from_le_bytes(bytes[54..58].try_into().unwrap()) as usize;
        let data = &bytes[58..58 + data_len];
        let samples = match (tag, bits) {
            (1, 16) => data
                .chunks_exact(2)
                .map(|c| f32::from(i16::from_le_bytes([c[0], c[1]])) / 32_768.0)
                .collect(),
            (1, 24) => data
                .chunks_exact(3)
                .map(|c| {
                    #[allow(clippy::cast_precision_loss, reason = "24-bit values")]
                    let value = (i32::from_le_bytes([0, c[0], c[1], c[2]]) >> 8) as f32;
                    value / 8_388_608.0
                })
                .collect(),
            (3, 32) => data
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect(),
            other => panic!("unexpected format {other:?}"),
        };
        (tag, bits, rate, samples)
    }

    #[test]
    fn every_format_round_trips() {
        let left: Vec<f32> = (0..1000).map(|i| ((i as f32) * 0.05).sin() * 0.5).collect();
        let right: Vec<f32> = left.iter().map(|s| -s).collect();
        for (format, tolerance) in [
            (WavFormat::Pcm16, 2.0 / 32_768.0),
            (WavFormat::Pcm24, 1.0 / 8_000_000.0),
            (WavFormat::Float32, 0.0),
        ] {
            let path = temp(&format!("{format:?}"));
            let mut writer = WavWriter::create(&path, format, 48_000).unwrap();
            writer.write(&left[..400], &right[..400]).unwrap();
            writer.write(&left[400..], &right[400..]).unwrap();
            assert_eq!(writer.frames(), 1000);
            writer.finish().unwrap();
            let (_, _, rate, samples) = read(&path);
            assert_eq!(rate, 48_000);
            assert_eq!(samples.len(), 2000);
            for (i, pair) in samples.chunks_exact(2).enumerate() {
                assert!(
                    (pair[0] - left[i]).abs() <= tolerance + 1e-7,
                    "{format:?} frame {i}"
                );
                assert!((pair[1] - right[i]).abs() <= tolerance + 1e-7);
            }
            std::fs::remove_file(path).ok();
        }
    }

    #[test]
    fn mono_files_hold_one_channel() {
        let path = temp("mono");
        let mut writer =
            WavWriter::create_with_channels(&path, WavFormat::Float32, 44_100, 1).unwrap();
        writer.write_interleaved(&[0.5, -0.5, 0.25]).unwrap();
        writer.write(&[1.0], &[-1.0]).unwrap();
        assert_eq!(writer.frames(), 4);
        writer.finish().unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(u16::from_le_bytes([bytes[22], bytes[23]]), 1, "one channel");
        let decoded = crate::audio::decode(&bytes).unwrap();
        assert_eq!(decoded.channels, vec![vec![0.5, -0.5, 0.25, 1.0]]);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn a_cancelled_export_leaves_nothing_behind() {
        let path = temp("cancelled");
        let mut writer = WavWriter::create(&path, WavFormat::Pcm24, 48_000).unwrap();
        writer.write(&[0.1; 10], &[0.1; 10]).unwrap();
        writer.cancel();
        assert!(!path.exists());
        let leftovers = std::fs::read_dir(std::env::temp_dir())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains("cancelled.wav."))
            .count();
        assert_eq!(leftovers, 0, "the temporary file is removed");
    }

    #[test]
    fn loud_or_broken_samples_never_wrap_around() {
        let path = temp("loud");
        let mut writer = WavWriter::create(&path, WavFormat::Pcm16, 44_100).unwrap();
        writer
            .write(&[2.0, -2.0, f32::NAN], &[0.0, 0.0, f32::INFINITY])
            .unwrap();
        writer.finish().unwrap();
        let (_, _, _, samples) = read(&path);
        assert!(
            samples[0] > 0.99 && samples[2] < -0.99,
            "clamped, not wrapped"
        );
        assert!(
            samples[4].abs() < 0.001 && samples[5].abs() < 0.001,
            "NaN and infinity become silence"
        );
        std::fs::remove_file(path).ok();
    }
}
