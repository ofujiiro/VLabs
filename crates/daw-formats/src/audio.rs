// SPDX-License-Identifier: GPL-3.0-or-later

//! Reading audio files (decision 0017): WAV, AIFF, FLAC, MP3 and Ogg
//! Vorbis, for importing them and for playing a song's audio files (AIFF,
//! common in sample collections, since decision 0023).
//!
//! Files come from outside and are untrusted: damaged or hostile input gives
//! an error, never a crash, and never more sound than an hour's worth
//! (a tiny file that claims or decodes to days of sound is refused before
//! it can fill the memory).

use std::fmt;
use std::io::{Cursor, Read};
use std::path::Path;

/// The longest sound accepted, in seconds.
pub const MAX_SECONDS: u64 = 60 * 60;

/// The largest file accepted, in bytes.
pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// The kinds of audio file that can be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioFormat {
    /// WAV: uncompressed samples.
    Wav,
    /// AIFF (and AIFF-C without compression): uncompressed samples.
    Aiff,
    /// FLAC: compressed without loss.
    Flac,
    /// MP3.
    Mp3,
    /// Ogg Vorbis.
    Ogg,
}

impl AudioFormat {
    /// The usual file name ending.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Aiff => "aiff",
            Self::Flac => "flac",
            Self::Mp3 => "mp3",
            Self::Ogg => "ogg",
        }
    }

    /// Recognises a file by its first bytes (never by its name).
    pub fn sniff(bytes: &[u8]) -> Option<Self> {
        if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
            return Some(Self::Wav);
        }
        if bytes.len() >= 12 && &bytes[..4] == b"FORM" && matches!(&bytes[8..12], b"AIFF" | b"AIFC")
        {
            return Some(Self::Aiff);
        }
        let untagged = skip_id3(bytes);
        if untagged.starts_with(b"fLaC") {
            return Some(Self::Flac);
        }
        if bytes.starts_with(b"OggS") {
            return Some(Self::Ogg);
        }
        nanomp3::detect(bytes).then_some(Self::Mp3)
    }
}

/// Sound read from a file: its rate and its first two channels.
#[derive(Clone, Debug, PartialEq)]
pub struct Decoded {
    /// Frames per second.
    pub sample_rate: u32,
    /// One channel (mono) or two (stereo), the same length.
    pub channels: Vec<Vec<f32>>,
    /// What kind of file it was.
    pub format: AudioFormat,
}

impl Decoded {
    /// How many frames it holds.
    pub fn frames(&self) -> usize {
        self.channels.first().map_or(0, Vec::len)
    }
}

/// Why an audio file could not be read.
#[derive(Debug)]
pub enum AudioError {
    /// The file could not be read from the disk.
    Io(std::io::Error),
    /// It is not a kind of audio file this program reads.
    Unknown,
    /// It is a known kind, in a variant this program does not read.
    Unsupported(String),
    /// It is damaged.
    Damaged(String),
    /// It holds more than an hour of sound, or is larger than 2 GB.
    TooLong,
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Unknown => write!(
                f,
                "it is not a WAV, AIFF, FLAC, MP3 or Ogg Vorbis file, or it is damaged"
            ),
            Self::Unsupported(what) => write!(f, "{what} is not supported"),
            Self::Damaged(why) => write!(f, "it is damaged ({why})"),
            Self::TooLong => write!(f, "it is longer than an hour"),
        }
    }
}

impl std::error::Error for AudioError {}

impl From<std::io::Error> for AudioError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Reads a whole audio file from the disk (refusing one over 2 GB).
pub fn read_file(path: &Path) -> Result<Vec<u8>, AudioError> {
    let file = std::fs::File::open(path)?;
    if file.metadata()?.len() > MAX_FILE_BYTES {
        return Err(AudioError::TooLong);
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(AudioError::TooLong);
    }
    Ok(bytes)
}

/// Decodes an audio file's bytes.
pub fn decode(bytes: &[u8]) -> Result<Decoded, AudioError> {
    let format = AudioFormat::sniff(bytes).ok_or(AudioError::Unknown)?;
    // The decoders are well tested, but a file from anywhere may still
    // find a corner of one: it fails, the program carries on.
    let decoded = std::panic::catch_unwind(|| match format {
        AudioFormat::Wav => decode_wav(bytes),
        AudioFormat::Aiff => decode_aiff(bytes),
        AudioFormat::Flac => decode_flac(skip_id3(bytes)),
        AudioFormat::Mp3 => decode_mp3(bytes),
        AudioFormat::Ogg => decode_ogg(bytes),
    })
    .unwrap_or_else(|_| Err(AudioError::Damaged("the decoder gave up".to_owned())))?;
    let (sample_rate, mut channels) = decoded;
    if sample_rate == 0 {
        return Err(AudioError::Damaged("it has no sample rate".to_owned()));
    }
    channels.truncate(2);
    if channels.is_empty() || channels[0].is_empty() {
        return Err(AudioError::Damaged("it holds no sound".to_owned()));
    }
    let frames = channels.iter().map(Vec::len).max().unwrap_or(0);
    for channel in &mut channels {
        channel.resize(frames, 0.0);
        for sample in channel.iter_mut() {
            if !sample.is_finite() {
                *sample = 0.0;
            }
        }
    }
    Ok(Decoded {
        sample_rate,
        channels,
        format,
    })
}

type Channels = (u32, Vec<Vec<f32>>);

/// The most frames an hour holds at `rate`.
fn frame_limit(rate: u32) -> u64 {
    MAX_SECONDS * u64::from(rate)
}

/// The bytes after an ID3v2 tag at the start, if there is one.
fn skip_id3(bytes: &[u8]) -> &[u8] {
    if bytes.len() < 10 || &bytes[..3] != b"ID3" {
        return bytes;
    }
    // The size is four 7-bit bytes; a footer adds ten more.
    let size = bytes[6..10]
        .iter()
        .fold(0_usize, |size, &b| (size << 7) | usize::from(b & 0x7F));
    let footer = if bytes[5] & 0x10 != 0 { 10 } else { 0 };
    bytes.get(10 + size + footer..).unwrap_or(&[])
}

fn le16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn le32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// What a WAV file tells a sampler (its `smpl` chunk): the key its sound
/// is at, and its first loop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SamplerInfo {
    /// The key at which the sound plays at its own pitch.
    pub root_key: Option<u8>,
    /// The first loop: its first and last frame.
    pub loop_frames: Option<(u64, u64)>,
}

/// Reads what a WAV file tells a sampler; nothing for other files, or a
/// WAV file without a `smpl` chunk.
pub fn sampler_info(bytes: &[u8]) -> SamplerInfo {
    let mut info = SamplerInfo::default();
    if AudioFormat::sniff(bytes) != Some(AudioFormat::Wav) {
        return info;
    }
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let size = le32(bytes, at + 4).unwrap_or(0) as usize;
        let body_start = at + 8;
        let body = &bytes[body_start..body_start.saturating_add(size).min(bytes.len())];
        if &bytes[at..at + 4] == b"smpl" {
            info.root_key = le32(body, 12)
                .filter(|&key| key <= 127)
                .and_then(|key| u8::try_from(key).ok());
            if le32(body, 28).is_some_and(|loops| loops > 0)
                && let (Some(start), Some(end)) = (le32(body, 44), le32(body, 48))
                && end > start
            {
                info.loop_frames = Some((u64::from(start), u64::from(end)));
            }
            break;
        }
        at = body_start.saturating_add(size).saturating_add(size & 1);
    }
    info
}

/// The WAV format chunk's essentials.
struct WavFormat {
    /// 1 for integers, 3 for floating point.
    tag: u16,
    channels: usize,
    rate: u32,
    bits: u16,
}

fn decode_wav(bytes: &[u8]) -> Result<Channels, AudioError> {
    let damaged = |why: &str| AudioError::Damaged(why.to_owned());
    let mut format = None;
    let mut data: Option<&[u8]> = None;
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = le32(bytes, at + 4).unwrap_or(0) as usize;
        let body_start = at + 8;
        let body_end = body_start.saturating_add(size).min(bytes.len());
        let body = &bytes[body_start..body_end];
        match id {
            b"fmt " => {
                let mut tag = le16(body, 0).ok_or_else(|| damaged("short format"))?;
                let channels = le16(body, 2).ok_or_else(|| damaged("short format"))?;
                let rate = le32(body, 4).ok_or_else(|| damaged("short format"))?;
                let bits = le16(body, 14).ok_or_else(|| damaged("short format"))?;
                if tag == 0xFFFE {
                    // WAVE_FORMAT_EXTENSIBLE: the real format leads the GUID.
                    tag = le16(body, 24).ok_or_else(|| damaged("short format"))?;
                }
                format = Some(WavFormat {
                    tag,
                    channels: usize::from(channels),
                    rate,
                    bits,
                });
            }
            b"data" => {
                // Streaming writers leave the size at 0 or the maximum:
                // the data then runs to the end of the file.
                data = Some(if size == 0 || size == u32::MAX as usize {
                    &bytes[body_start..]
                } else {
                    body
                });
                if format.is_some() {
                    break;
                }
            }
            _ => {}
        }
        at = body_start.saturating_add(size).saturating_add(size & 1);
    }
    let format = format.ok_or_else(|| damaged("no format chunk"))?;
    let data = data.ok_or_else(|| damaged("no sound data"))?;
    let width = usize::from(format.bits / 8);
    let supported = matches!(
        (format.tag, format.bits),
        (1, 8 | 16 | 24 | 32) | (3, 32 | 64)
    );
    if !supported {
        return Err(AudioError::Unsupported(format!(
            "WAV with {}-bit samples of kind {}",
            format.bits, format.tag
        )));
    }
    if format.channels == 0 || format.channels > 64 {
        return Err(damaged("its channel count"));
    }
    if format.rate == 0 {
        return Err(damaged("its sample rate"));
    }
    let block = width * format.channels;
    let frames = data.len() / block;
    if frames as u64 > frame_limit(format.rate) {
        return Err(AudioError::TooLong);
    }
    let kept = format.channels.min(2);
    let mut channels = vec![Vec::with_capacity(frames); kept];
    for frame in data.chunks_exact(block) {
        for (channel, sample) in channels.iter_mut().zip(frame.chunks_exact(width)) {
            channel.push(wav_sample(format.tag, sample));
        }
    }
    Ok((format.rate, channels))
}

/// One WAV sample as a float from -1 to 1.
#[allow(clippy::cast_precision_loss, reason = "audio samples")]
fn wav_sample(tag: u16, bytes: &[u8]) -> f32 {
    match (tag, bytes.len()) {
        (1, 1) => (f32::from(bytes[0]) - 128.0) / 128.0,
        (1, 2) => f32::from(i16::from_le_bytes([bytes[0], bytes[1]])) / 32_768.0,
        (1, 3) => (i32::from_le_bytes([0, bytes[0], bytes[1], bytes[2]]) >> 8) as f32 / 8_388_608.0,
        (1, 4) => {
            i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f32 / 2_147_483_648.0
        }
        (3, 4) => f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        (3, 8) => {
            #[allow(clippy::cast_possible_truncation, reason = "audio samples")]
            let value = f64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]) as f32;
            value
        }
        _ => 0.0,
    }
}

fn be16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn be32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// An 80-bit extended float (AIFF's sample rate) as a whole number of
/// frames per second; `None` when it is not a sensible rate.
fn extended_rate(bytes: &[u8]) -> Option<u32> {
    let exponent = i32::from(be16(bytes, 0)? & 0x7FFF);
    let mantissa = u64::from_be_bytes(bytes.get(2..10)?.try_into().ok()?);
    // value = mantissa × 2^(exponent − 16383 − 63)
    let shift = 16_383 + 63 - exponent;
    if !(0..64).contains(&shift) {
        return None;
    }
    let rate = mantissa >> shift;
    u32::try_from(rate)
        .ok()
        .filter(|r| (1..=1_536_000).contains(r))
}

/// How AIFF samples are stored.
#[derive(Clone, Copy, PartialEq, Eq)]
enum AiffSamples {
    /// Big-endian integers (AIFF, and AIFF-C's `NONE`).
    BigEndian,
    /// Little-endian integers (AIFF-C's `sowt`).
    LittleEndian,
    /// Big-endian floats (AIFF-C's `fl32` and `fl64`).
    Float,
}

fn decode_aiff(bytes: &[u8]) -> Result<Channels, AudioError> {
    let damaged = |why: &str| AudioError::Damaged(why.to_owned());
    let compressed = &bytes[8..12] == b"AIFC";
    let mut format: Option<(usize, u16, u32, AiffSamples)> = None;
    let mut data: Option<&[u8]> = None;
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = be32(bytes, at + 4).unwrap_or(0) as usize;
        let body_start = at + 8;
        let body_end = body_start.saturating_add(size).min(bytes.len());
        let body = &bytes[body_start..body_end];
        match id {
            b"COMM" => {
                let short = || damaged("short format");
                let channels = usize::from(be16(body, 0).ok_or_else(short)?);
                let bits = be16(body, 6).ok_or_else(short)?;
                let rate = extended_rate(body.get(8..18).ok_or_else(short)?)
                    .ok_or_else(|| damaged("its sample rate"))?;
                let kind = if compressed {
                    match body.get(18..22).ok_or_else(short)? {
                        b"NONE" | b"twos" => AiffSamples::BigEndian,
                        b"sowt" => AiffSamples::LittleEndian,
                        b"fl32" | b"FL32" | b"fl64" | b"FL64" => AiffSamples::Float,
                        other => {
                            return Err(AudioError::Unsupported(format!(
                                "AIFF-C compressed as \"{}\"",
                                String::from_utf8_lossy(other)
                            )));
                        }
                    }
                } else {
                    AiffSamples::BigEndian
                };
                format = Some((channels, bits, rate, kind));
            }
            b"SSND" => {
                let offset = be32(body, 0).ok_or_else(|| damaged("short sound chunk"))? as usize;
                data = Some(body.get(8 + offset.min(body.len())..).unwrap_or(&[]));
            }
            _ => {}
        }
        at = body_start.saturating_add(size).saturating_add(size & 1);
    }
    let (channel_count, bits, rate, kind) = format.ok_or_else(|| damaged("no format chunk"))?;
    let data = data.ok_or_else(|| damaged("no sound data"))?;
    if channel_count == 0 || channel_count > 64 {
        return Err(damaged("its channel count"));
    }
    let width = match (kind, bits) {
        (AiffSamples::Float, 32) => 4,
        (AiffSamples::Float, 64) => 8,
        (AiffSamples::Float, _) => return Err(damaged("its sample size")),
        // Integer samples are stored in whole bytes, padded at the bottom.
        (_, 1..=32) => usize::from(bits.div_ceil(8)),
        _ => {
            return Err(AudioError::Unsupported(format!(
                "AIFF with {bits}-bit samples"
            )));
        }
    };
    let block = width * channel_count;
    let frames = data.len() / block;
    if frames as u64 > frame_limit(rate) {
        return Err(AudioError::TooLong);
    }
    let kept = channel_count.min(2);
    let mut channels = vec![Vec::with_capacity(frames); kept];
    for frame in data.chunks_exact(block) {
        for (channel, sample) in channels.iter_mut().zip(frame.chunks_exact(width)) {
            channel.push(aiff_sample(kind, sample));
        }
    }
    Ok((rate, channels))
}

/// One AIFF sample as a float from -1 to 1.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "audio samples"
)]
fn aiff_sample(kind: AiffSamples, bytes: &[u8]) -> f32 {
    match kind {
        AiffSamples::Float => match bytes.len() {
            4 => f32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            8 => f64::from_be_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]) as f32,
            _ => 0.0,
        },
        AiffSamples::BigEndian | AiffSamples::LittleEndian => {
            // Put the sample's bytes at the top of a 32-bit integer.
            let mut word = [0_u8; 4];
            for (index, &byte) in bytes.iter().enumerate().take(4) {
                let from_top = if kind == AiffSamples::BigEndian {
                    index
                } else {
                    bytes.len() - 1 - index
                };
                word[from_top] = byte;
            }
            i32::from_be_bytes(word) as f32 / 2_147_483_648.0
        }
    }
}

fn decode_flac(bytes: &[u8]) -> Result<Channels, AudioError> {
    let damaged = |e: claxon::Error| AudioError::Damaged(e.to_string());
    let mut reader = claxon::FlacReader::new(Cursor::new(bytes)).map_err(damaged)?;
    let info = reader.streaminfo();
    let limit = frame_limit(info.sample_rate);
    if info.samples.is_some_and(|total| total > limit) {
        return Err(AudioError::TooLong);
    }
    if !(1..=32).contains(&info.bits_per_sample) {
        return Err(AudioError::Damaged("its sample size".to_owned()));
    }
    #[allow(clippy::cast_precision_loss, reason = "a power of two")]
    let scale = 1.0 / (1_u64 << (info.bits_per_sample - 1)) as f32;
    let kept = info.channels.min(2);
    let mut channels: Vec<Vec<f32>> = (0..kept).map(|_| Vec::new()).collect();
    let mut blocks = reader.blocks();
    let mut buffer = Vec::new();
    while let Some(block) = blocks.read_next_or_eof(buffer).map_err(damaged)? {
        for (index, channel) in (0_u32..).zip(channels.iter_mut()) {
            #[allow(clippy::cast_precision_loss, reason = "audio samples")]
            channel.extend(block.channel(index).iter().map(|&s| s as f32 * scale));
        }
        if channels[0].len() as u64 > limit {
            return Err(AudioError::TooLong);
        }
        buffer = block.into_buffer();
    }
    Ok((info.sample_rate, channels))
}

fn decode_mp3(bytes: &[u8]) -> Result<Channels, AudioError> {
    // Count what the frames hold before decoding them.
    let mut frames = 0_u64;
    for frame in nanomp3::frames(bytes) {
        frames += frame.samples as u64;
        if frames > frame_limit(frame.sample_rate) {
            return Err(AudioError::TooLong);
        }
    }
    let decoded = nanomp3::decode_all::<f32>(bytes);
    let count = decoded.channels.map_or(1, |c| usize::from(c.num())).max(1);
    Ok((decoded.sample_rate, deinterleave(&decoded.samples, count)))
}

fn decode_ogg(bytes: &[u8]) -> Result<Channels, AudioError> {
    let damaged = |e: lewton::VorbisError| match e {
        lewton::VorbisError::OggError(_) | lewton::VorbisError::BadHeader(_) => {
            AudioError::Unsupported("Ogg without Vorbis sound (Opus, for instance)".to_owned())
        }
        other @ lewton::VorbisError::BadAudio(_) => AudioError::Damaged(other.to_string()),
    };
    let mut reader =
        lewton::inside_ogg::OggStreamReader::new(Cursor::new(bytes)).map_err(damaged)?;
    let rate = reader.ident_hdr.audio_sample_rate;
    let kept = usize::from(reader.ident_hdr.audio_channels).clamp(1, 2);
    let limit = frame_limit(rate);
    let mut channels: Vec<Vec<f32>> = (0..kept).map(|_| Vec::new()).collect();
    while let Some(packet) = reader
        .read_dec_packet_generic::<Vec<Vec<f32>>>()
        .map_err(|e| AudioError::Damaged(e.to_string()))?
    {
        for (channel, samples) in channels.iter_mut().zip(packet) {
            channel.extend(samples);
        }
        if channels[0].len() as u64 > limit {
            return Err(AudioError::TooLong);
        }
    }
    Ok((rate, channels))
}

/// Interleaved frames of `count` samples, split into (at most two)
/// channels.
fn deinterleave(samples: &[f32], count: usize) -> Vec<Vec<f32>> {
    let kept = count.min(2);
    let frames = samples.len() / count;
    let mut channels = vec![Vec::with_capacity(frames); kept];
    for frame in samples.chunks_exact(count) {
        for (channel, &sample) in channels.iter_mut().zip(frame) {
            channel.push(sample);
        }
    }
    channels
}

#[cfg(test)]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "test signals"
)]
mod tests {
    use super::*;

    /// A WAV file's bytes with the given format chunk fields and samples.
    fn wav(tag: u16, channels: u16, rate: u32, bits: u16, data: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(b"WAVE");
        bytes.extend_from_slice(b"fmt ");
        bytes.extend_from_slice(&16_u32.to_le_bytes());
        bytes.extend_from_slice(&tag.to_le_bytes());
        bytes.extend_from_slice(&channels.to_le_bytes());
        bytes.extend_from_slice(&rate.to_le_bytes());
        let block = u32::from(channels) * u32::from(bits / 8);
        bytes.extend_from_slice(&(rate * block).to_le_bytes());
        bytes.extend_from_slice(&(block as u16).to_le_bytes());
        bytes.extend_from_slice(&bits.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(data);
        bytes
    }

    #[test]
    fn wav_files_of_every_common_kind_are_read() {
        let pcm16: Vec<u8> = [16_384_i16, -16_384, 32_767, -32_768]
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        let decoded = decode(&wav(1, 2, 44_100, 16, &pcm16)).unwrap();
        assert_eq!(decoded.format, AudioFormat::Wav);
        assert_eq!(decoded.sample_rate, 44_100);
        assert_eq!(
            decoded.channels,
            vec![vec![0.5, 32_767.0 / 32_768.0], vec![-0.5, -1.0]]
        );

        let pcm24: Vec<u8> = [4_194_304_i32, -4_194_304]
            .iter()
            .flat_map(|s| s.to_le_bytes()[..3].to_vec())
            .collect();
        let mono = decode(&wav(1, 1, 48_000, 24, &pcm24)).unwrap();
        assert_eq!(mono.channels, vec![vec![0.5, -0.5]]);

        let float: Vec<u8> = [0.25_f32, -0.75]
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        assert_eq!(
            decode(&wav(3, 1, 48_000, 32, &float)).unwrap().channels,
            vec![vec![0.25, -0.75]]
        );
        let eight = decode(&wav(1, 1, 8_000, 8, &[128, 192, 0])).unwrap();
        assert_eq!(eight.channels, vec![vec![0.0, 0.5, -1.0]]);

        // Six channels: the front left and right are kept.
        let surround: Vec<u8> = (1..=6_i16).flat_map(|s| (s * 1000).to_le_bytes()).collect();
        let front = decode(&wav(1, 6, 48_000, 16, &surround)).unwrap();
        assert_eq!(front.channels.len(), 2);
        assert!((front.channels[1][0] - 2000.0 / 32_768.0).abs() < 1e-6);
    }

    /// An AIFF (or AIFF-C, with `compression`) file's bytes.
    fn aiff(
        channels: u16,
        rate: u32,
        bits: u16,
        compression: Option<&[u8; 4]>,
        data: &[u8],
    ) -> Vec<u8> {
        let mut comm = Vec::new();
        comm.extend_from_slice(&channels.to_be_bytes());
        let frames = data.len() as u32 / (u32::from(channels) * u32::from(bits.div_ceil(8)));
        comm.extend_from_slice(&frames.to_be_bytes());
        comm.extend_from_slice(&bits.to_be_bytes());
        let top = rate.ilog2();
        comm.extend_from_slice(&(16_383 + top as u16).to_be_bytes());
        comm.extend_from_slice(&(u64::from(rate) << (63 - top)).to_be_bytes());
        if let Some(kind) = compression {
            comm.extend_from_slice(kind);
            comm.extend_from_slice(&[0, 0]); // An empty name.
        }
        let mut body = Vec::new();
        body.extend_from_slice(if compression.is_some() {
            b"AIFC"
        } else {
            b"AIFF"
        });
        body.extend_from_slice(b"COMM");
        body.extend_from_slice(&(comm.len() as u32).to_be_bytes());
        body.extend_from_slice(&comm);
        body.extend_from_slice(b"SSND");
        body.extend_from_slice(&(data.len() as u32 + 8).to_be_bytes());
        body.extend_from_slice(&[0; 8]);
        body.extend_from_slice(data);
        let mut bytes = b"FORM".to_vec();
        bytes.extend_from_slice(&(body.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&body);
        bytes
    }

    #[test]
    fn aiff_files_of_the_common_kinds_are_read() {
        let pcm16: Vec<u8> = [16_384_i16, -16_384, 32_767, -32_768]
            .iter()
            .flat_map(|s| s.to_be_bytes())
            .collect();
        let decoded = decode(&aiff(2, 44_100, 16, None, &pcm16)).unwrap();
        assert_eq!(decoded.format, AudioFormat::Aiff);
        assert_eq!(decoded.sample_rate, 44_100);
        assert_eq!(
            decoded.channels,
            vec![vec![0.5, 32_767.0 / 32_768.0], vec![-0.5, -1.0]]
        );

        let pcm24: Vec<u8> = [4_194_304_i32, -4_194_304]
            .iter()
            .flat_map(|s| s.to_be_bytes()[1..].to_vec())
            .collect();
        let mono = decode(&aiff(1, 48_000, 24, None, &pcm24)).unwrap();
        assert_eq!(mono.sample_rate, 48_000);
        assert_eq!(mono.channels, vec![vec![0.5, -0.5]]);

        // AIFF-C: little-endian integers, and floats.
        let swapped: Vec<u8> = [16_384_i16, -16_384]
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        let sowt = decode(&aiff(1, 96_000, 16, Some(b"sowt"), &swapped)).unwrap();
        assert_eq!(sowt.sample_rate, 96_000);
        assert_eq!(sowt.channels, vec![vec![0.5, -0.5]]);
        let float: Vec<u8> = [0.25_f32, -0.75]
            .iter()
            .flat_map(|s| s.to_be_bytes())
            .collect();
        let fl32 = decode(&aiff(1, 22_050, 32, Some(b"fl32"), &float)).unwrap();
        assert_eq!(fl32.sample_rate, 22_050);
        assert_eq!(fl32.channels, vec![vec![0.25, -0.75]]);

        // Compressed AIFF-C is explained, damaged AIFF is refused.
        let ima = decode(&aiff(1, 44_100, 16, Some(b"ima4"), &[0; 34]));
        assert!(matches!(ima, Err(AudioError::Unsupported(_))), "{ima:?}");
        let mut no_rate = aiff(1, 44_100, 16, None, &pcm16);
        no_rate[28..38].fill(0);
        assert!(decode(&no_rate).is_err());
        let truncated = &aiff(2, 44_100, 16, None, &pcm16)[..30];
        assert!(decode(truncated).is_err());
    }

    #[test]
    fn the_recording_writer_files_read_back() {
        let path = std::env::temp_dir().join(format!("audio-read-{}.wav", std::process::id()));
        let mut writer =
            crate::wav::WavWriter::create(&path, crate::wav::WavFormat::Pcm24, 48_000).unwrap();
        writer.write(&[0.5, -0.25], &[0.0, 0.125]).unwrap();
        writer.finish().unwrap();
        let decoded = decode(&read_file(&path).unwrap()).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(decoded.frames(), 2);
        assert!((decoded.channels[0][1] + 0.25).abs() < 1e-6);
        assert!((decoded.channels[1][1] - 0.125).abs() < 1e-6);
    }

    #[test]
    fn flac_files_are_read() {
        use flacenc::component::BitRepr;
        use flacenc::error::Verify;
        let sine: Vec<i32> = (0..4800)
            .map(|i| ((i as f32 * 0.05).sin() * 16_000.0) as i32)
            .collect();
        let config = flacenc::config::Encoder::default().into_verified().unwrap();
        let source = flacenc::source::MemSource::from_samples(&sine, 1, 16, 44_100);
        let stream =
            flacenc::encode_with_fixed_block_size(&config, source, config.block_size).unwrap();
        let mut bytes = flacenc::bitsink::ByteSink::new();
        stream.write(&mut bytes).unwrap();
        let decoded = decode(bytes.as_slice()).unwrap();
        assert_eq!(decoded.format, AudioFormat::Flac);
        assert_eq!((decoded.sample_rate, decoded.frames()), (44_100, 4800));
        assert!((decoded.channels[0][100] - sine[100] as f32 / 32_768.0).abs() < 1e-6);
    }

    #[test]
    fn damaged_and_hostile_files_give_errors() {
        assert!(matches!(decode(b"hello"), Err(AudioError::Unknown)));
        let mut cut = wav(1, 2, 44_100, 16, &[0; 400]);
        cut.truncate(30);
        assert!(decode(&cut).is_err(), "a cut-off header");
        assert!(matches!(
            decode(&wav(1, 2, 44_100, 12, &[0; 40])),
            Err(AudioError::Unsupported(_))
        ));
        // A data chunk claiming more than the file holds reads what is there.
        let mut claims_more = wav(1, 1, 8_000, 16, &[0, 64, 0, 64]);
        let size_at = claims_more.len() - 8;
        claims_more[size_at..size_at + 4].copy_from_slice(&0x7FFF_FFFF_u32.to_le_bytes());
        assert_eq!(decode(&claims_more).unwrap().frames(), 2);
        // An hour and a bit at a sample rate of 1: refused.
        assert!(matches!(
            decode(&wav(1, 1, 1, 8, &[128; 3601])),
            Err(AudioError::TooLong)
        ));
        assert!(matches!(
            decode(&wav(1, 1, 0, 16, &[0; 8])),
            Err(AudioError::Damaged(_))
        ));
        assert!(decode(b"OggS\0\x02 not really an ogg file at all").is_err());
        assert!(decode(b"fLaC broken").is_err());
    }

    #[test]
    fn files_are_recognised_by_their_contents() {
        assert_eq!(
            AudioFormat::sniff(b"RIFF\0\0\0\0WAVEfmt "),
            Some(AudioFormat::Wav)
        );
        assert_eq!(AudioFormat::sniff(b"fLaC\0"), Some(AudioFormat::Flac));
        let tagged: Vec<u8> = b"ID3\x04\0\0\0\0\0\x02xxfLaC".to_vec();
        assert_eq!(AudioFormat::sniff(&tagged), Some(AudioFormat::Flac));
        assert_eq!(AudioFormat::sniff(b"OggS"), Some(AudioFormat::Ogg));
        assert_eq!(AudioFormat::sniff(b"RIFF\0\0\0\0AVI "), None);
        assert_eq!(AudioFormat::Mp3.extension(), "mp3");
    }
}
