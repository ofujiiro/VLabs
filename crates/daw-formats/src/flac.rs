// SPDX-License-Identifier: GPL-3.0-or-later

//! Writing FLAC files, for exporting songs (decision 0020). The sound is
//! encoded with `flacenc` one frame at a time as it comes, so a long song
//! never has to fit in memory, into a temporary file that replaces the
//! target only once complete (the stream's summary at the start of the
//! file is filled in at the end: its length, frame sizes and checksum).

use std::fs::File;
use std::io::{self, BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use flacenc::bitsink::ByteSink;
use flacenc::component::{BitRepr, StreamInfo};
use flacenc::error::{Verified, Verify};
use flacenc::source::{Context, Fill, FrameBuf};

/// Frames per FLAC frame (the usual block size).
const BLOCK: usize = 4096;
/// Where the stream summary's body starts: after "fLaC" and the metadata
/// block header.
const STREAM_INFO_AT: u64 = 8;

/// How samples are stored.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlacBits {
    /// 16 bits, with dither: CD quality.
    Bits16,
    /// 24 bits: studio quality.
    #[default]
    Bits24,
}

impl FlacBits {
    fn bits(self) -> usize {
        match self {
            Self::Bits16 => 16,
            Self::Bits24 => 24,
        }
    }
}

/// A stereo FLAC file being written.
pub struct FlacWriter {
    out: BufWriter<File>,
    temporary: PathBuf,
    target: PathBuf,
    bits: FlacBits,
    config: Verified<flacenc::config::Encoder>,
    info: StreamInfo,
    buffer: (FrameBuf, Context),
    /// Interleaved samples waiting for a whole block.
    pending: Vec<i32>,
    frame: usize,
    noise: u32,
}

fn encode_error(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(format!("FLAC encoding failed: {error}"))
}

impl FlacWriter {
    /// Starts writing a stereo file at `sample_rate`.
    pub fn create(path: &Path, bits: FlacBits, sample_rate: u32) -> io::Result<Self> {
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
        let mut config = flacenc::config::Encoder::default();
        config.block_size = BLOCK;
        let config = config.into_verified().map_err(|(_, e)| encode_error(e))?;
        let info = StreamInfo::new(sample_rate as usize, 2, bits.bits()).map_err(encode_error)?;
        let buffer = (
            FrameBuf::with_size(2, BLOCK).map_err(encode_error)?,
            Context::new(bits.bits(), 2),
        );
        let mut writer = Self {
            out: BufWriter::new(File::create(&temporary)?),
            temporary,
            target: path.to_path_buf(),
            bits,
            config,
            info,
            buffer,
            pending: Vec::with_capacity(BLOCK * 2),
            frame: 0,
            noise: 0x2545_f491,
        };
        writer.out.write_all(b"fLaC")?;
        writer.write_stream_info()?;
        Ok(writer)
    }

    /// The metadata block holding the stream summary (the only one, so it
    /// is marked last).
    fn stream_info_bytes(&self) -> io::Result<Vec<u8>> {
        let mut sink = ByteSink::new();
        self.info.write(&mut sink).map_err(encode_error)?;
        let body = sink.as_slice();
        let mut bytes = vec![0x80, 0, 0, u8::try_from(body.len()).unwrap_or(34)];
        bytes.extend_from_slice(body);
        Ok(bytes)
    }

    fn write_stream_info(&mut self) -> io::Result<()> {
        let bytes = self.stream_info_bytes()?;
        self.out.write_all(&bytes)
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

    fn quantise(&mut self, sample: f32) -> i32 {
        let sample = sample.clamp(-1.0, 1.0);
        #[allow(clippy::cast_possible_truncation, reason = "clamped to the range")]
        match self.bits {
            FlacBits::Bits16 => {
                // Triangular dither of one step.
                let dither = f32::midpoint(self.random(), self.random()) / 32_768.0;
                ((sample + dither) * 32_767.0)
                    .round()
                    .clamp(-32_768.0, 32_767.0) as i32
            }
            FlacBits::Bits24 => (sample * 8_388_607.0).round() as i32,
        }
    }

    /// Appends frames (both slices the same length).
    pub fn write(&mut self, left: &[f32], right: &[f32]) -> io::Result<()> {
        for (&l, &r) in left.iter().zip(right) {
            let (l, r) = (self.quantise(l), self.quantise(r));
            self.pending.push(l);
            self.pending.push(r);
            if self.pending.len() == BLOCK * 2 {
                self.encode_pending()?;
            }
        }
        Ok(())
    }

    fn encode_pending(&mut self) -> io::Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        self.buffer
            .fill_interleaved(&self.pending)
            .map_err(encode_error)?;
        let frame =
            flacenc::encode_fixed_size_frame(&self.config, &self.buffer.0, self.frame, &self.info)
                .map_err(encode_error)?;
        self.info.update_frame_info(&frame);
        let mut sink = ByteSink::new();
        frame.write(&mut sink).map_err(encode_error)?;
        self.out.write_all(sink.as_slice())?;
        self.frame += 1;
        self.pending.clear();
        Ok(())
    }

    /// Frames written so far (whole blocks).
    pub fn frames(&self) -> u64 {
        self.info.total_samples() as u64
    }

    /// Ends the file: the last block, the summary, then the file takes the
    /// target's place.
    pub fn finish(mut self) -> io::Result<()> {
        self.encode_pending()?;
        self.info.set_md5_digest(&self.buffer.1.md5_digest());
        let bytes = self.stream_info_bytes()?;
        self.out.seek(SeekFrom::Start(STREAM_INFO_AT - 4))?;
        self.out.write_all(&bytes)?;
        self.out.flush()?;
        self.out.get_ref().sync_all()?;
        let Self {
            out,
            temporary,
            target,
            ..
        } = self;
        drop(out);
        std::fs::rename(&temporary, &target).inspect_err(|_| {
            let _ = std::fs::remove_file(&temporary);
        })
    }

    /// Abandons the file.
    pub fn cancel(self) {
        let Self { out, temporary, .. } = self;
        drop(out);
        let _ = std::fs::remove_file(temporary);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_written_file_decodes_to_what_went_in() {
        let path = std::env::temp_dir().join(format!("flac-writer-{}.flac", std::process::id()));
        let mut writer = FlacWriter::create(&path, FlacBits::Bits24, 48_000).unwrap();
        let frames = 10_000; // two whole blocks and a part
        #[allow(clippy::cast_precision_loss, reason = "test signal")]
        let left: Vec<f32> = (0..frames).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
        let right: Vec<f32> = left.iter().map(|s| -s * 0.5).collect();
        writer.write(&left[..3000], &right[..3000]).unwrap();
        writer.write(&left[3000..], &right[3000..]).unwrap();
        writer.finish().unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let mut reader = claxon::FlacReader::new(std::io::Cursor::new(&bytes)).unwrap();
        let info = reader.streaminfo();
        assert_eq!(info.sample_rate, 48_000);
        assert_eq!(info.channels, 2);
        assert_eq!(info.bits_per_sample, 24);
        assert_eq!(
            info.samples,
            Some(frames as u64),
            "the length is in the summary"
        );
        let samples: Vec<i32> = reader.samples().map(Result::unwrap).collect();
        assert_eq!(samples.len(), frames * 2);
        for (i, pair) in samples.chunks(2).enumerate() {
            #[allow(clippy::cast_possible_truncation, reason = "test arithmetic")]
            let expected = (left[i] * 8_388_607.0).round() as i32;
            assert_eq!(pair[0], expected, "lossless at frame {i}");
        }
        std::fs::remove_file(path).ok();
    }
}
