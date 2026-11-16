// SPDX-License-Identifier: GPL-3.0-or-later

//! The file a recorded kit is shipped in: its sounds as FLAC (lossless,
//! half the size of raw samples), each tagged with the pad key and the
//! highest velocity it plays.
//!
//! Layout (numbers little-endian): the 8 bytes `DRUMKIT1`, a `u16` count,
//! then for each sound: `u8` key, `u8` top velocity, `u32` length and that
//! many bytes of FLAC.

use super::Sample;

/// The first bytes of a kit bundle.
pub const BUNDLE_MAGIC: &[u8; 8] = b"DRUMKIT1";

/// The largest sound accepted (a damaged file must not exhaust memory).
const MAX_ENTRY_BYTES: u32 = 64 * 1024 * 1024;

/// One sound read from a bundle.
#[derive(Clone, Debug)]
pub struct BundleEntry {
    /// The pad's key.
    pub key: u8,
    /// The highest velocity it plays.
    pub top: u8,
    /// The decoded sound.
    pub sample: Sample,
}

/// Writes a bundle from FLAC-encoded sounds: (key, top velocity, FLAC).
pub fn write_bundle(entries: &[(u8, u8, Vec<u8>)]) -> Result<Vec<u8>, String> {
    let count = u16::try_from(entries.len()).map_err(|_| "too many sounds".to_owned())?;
    let mut out = Vec::new();
    out.extend_from_slice(BUNDLE_MAGIC);
    out.extend_from_slice(&count.to_le_bytes());
    for (key, top, flac) in entries {
        let length = u32::try_from(flac.len()).map_err(|_| "a sound is too large".to_owned())?;
        out.push(*key);
        out.push(*top);
        out.extend_from_slice(&length.to_le_bytes());
        out.extend_from_slice(flac);
    }
    Ok(out)
}

/// Reads and decodes every sound in a bundle.
pub fn read_bundle(bytes: &[u8]) -> Result<Vec<BundleEntry>, String> {
    let mut rest = bytes
        .strip_prefix(BUNDLE_MAGIC.as_slice())
        .ok_or_else(|| "not a drum kit bundle".to_owned())?;
    let count = u16::from_le_bytes(take::<2>(&mut rest)?);
    let mut entries = Vec::with_capacity(usize::from(count));
    for _ in 0..count {
        let [key, top] = take::<2>(&mut rest)?;
        let length = u32::from_le_bytes(take::<4>(&mut rest)?);
        if length > MAX_ENTRY_BYTES {
            return Err("a sound in the bundle is too large".to_owned());
        }
        let length = usize::try_from(length).map_err(|_| "a sound is too large".to_owned())?;
        if rest.len() < length {
            return Err("the bundle is cut short".to_owned());
        }
        let (flac, after) = rest.split_at(length);
        rest = after;
        entries.push(BundleEntry {
            key,
            top: top.clamp(1, 127),
            sample: decode_flac(flac)?,
        });
    }
    Ok(entries)
}

fn take<const N: usize>(rest: &mut &[u8]) -> Result<[u8; N], String> {
    let (head, tail) = rest
        .split_first_chunk::<N>()
        .ok_or_else(|| "the bundle is cut short".to_owned())?;
    *rest = tail;
    Ok(*head)
}

/// Decodes a mono or stereo FLAC sound to floating point.
pub fn decode_flac(bytes: &[u8]) -> Result<Sample, String> {
    let mut reader = claxon::FlacReader::new(bytes).map_err(|e| format!("FLAC: {e}"))?;
    let info = reader.streaminfo();
    let channels = info.channels as usize;
    if !(1..=2).contains(&channels) {
        return Err(format!("FLAC with {channels} channels"));
    }
    #[allow(clippy::cast_precision_loss, reason = "bit depths are small")]
    let scale = 1.0 / (1_u64 << (info.bits_per_sample - 1)) as f32;
    let mut left = Vec::new();
    let mut right = Vec::new();
    for (index, sample) in reader.samples().enumerate() {
        let sample = sample.map_err(|e| format!("FLAC: {e}"))?;
        #[allow(clippy::cast_precision_loss, reason = "audio samples fit in f32")]
        let value = sample as f32 * scale;
        if channels == 1 || index % 2 == 0 {
            left.push(value);
        } else {
            right.push(value);
        }
    }
    right.truncate(left.len());
    Ok(Sample {
        sample_rate: info.sample_rate,
        left,
        right,
    })
}
