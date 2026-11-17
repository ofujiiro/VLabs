// SPDX-License-Identifier: GPL-3.0-or-later

//! Recorded and imported sound in memory (decision 0017): buffers of
//! samples, conversion between sample rates, and summaries of loudness for
//! drawing waveforms.
//!
//! None of this runs on the audio thread except reading samples from an
//! [`AudioBuffer`]: buffers are made, converted and summarised in the
//! background, then shared with the engine.

mod peaks;
mod resample;

pub use peaks::Peaks;
pub use resample::resample;

/// Sound in memory: one channel (mono) or two (stereo) of samples at a
/// sample rate, every channel the same length.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioBuffer {
    sample_rate: u32,
    channels: Vec<Vec<f32>>,
}

impl AudioBuffer {
    /// A buffer from separate channels. Only the first two are kept; shorter
    /// channels are padded with silence; no channel at all gives one empty
    /// channel.
    pub fn new(sample_rate: u32, mut channels: Vec<Vec<f32>>) -> Self {
        channels.truncate(2);
        if channels.is_empty() {
            channels.push(Vec::new());
        }
        let frames = channels.iter().map(Vec::len).max().unwrap_or(0);
        for channel in &mut channels {
            channel.resize(frames, 0.0);
        }
        Self {
            sample_rate: sample_rate.max(1),
            channels,
        }
    }

    /// A buffer from interleaved frames of `channels` samples each. Of more
    /// than two channels, the first two are kept (front left and right).
    pub fn from_interleaved(sample_rate: u32, channels: usize, samples: &[f32]) -> Self {
        let channels = channels.max(1);
        let kept = channels.min(2);
        let frames = samples.len() / channels;
        let mut split = vec![Vec::with_capacity(frames); kept];
        for frame in samples.chunks_exact(channels) {
            for (channel, &sample) in split.iter_mut().zip(frame) {
                channel.push(sample);
            }
        }
        Self::new(sample_rate, split)
    }

    /// Frames per second.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// How many frames it holds.
    pub fn frames(&self) -> usize {
        self.channels[0].len()
    }

    /// How long it lasts, in seconds.
    #[allow(clippy::cast_precision_loss, reason = "frame counts fit easily")]
    pub fn seconds(&self) -> f64 {
        self.frames() as f64 / f64::from(self.sample_rate)
    }

    /// 1 for mono, 2 for stereo.
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    /// The left channel (the only one, for mono).
    pub fn left(&self) -> &[f32] {
        &self.channels[0]
    }

    /// The right channel (the same as the left, for mono).
    pub fn right(&self) -> &[f32] {
        self.channels.last().map_or(&[], Vec::as_slice)
    }

    /// The same sound at another sample rate (a copy if the rate is the
    /// same). Slow for long sound: do it in the background.
    #[must_use]
    pub fn resampled(&self, sample_rate: u32) -> Self {
        let sample_rate = sample_rate.max(1);
        if sample_rate == self.sample_rate {
            return self.clone();
        }
        Self {
            sample_rate,
            channels: self
                .channels
                .iter()
                .map(|channel| resample(channel, self.sample_rate, sample_rate))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interleaved_frames_are_split_into_channels() {
        let stereo = AudioBuffer::from_interleaved(48_000, 2, &[1.0, -1.0, 2.0, -2.0, 3.0]);
        assert_eq!(stereo.frames(), 2, "a partial frame is dropped");
        assert_eq!(stereo.left(), &[1.0, 2.0]);
        assert_eq!(stereo.right(), &[-1.0, -2.0]);

        let mono = AudioBuffer::from_interleaved(44_100, 1, &[0.5, 0.25]);
        assert_eq!(mono.channel_count(), 1);
        assert_eq!(mono.right(), mono.left());

        let surround = AudioBuffer::from_interleaved(48_000, 6, &[1., 2., 3., 4., 5., 6.]);
        assert_eq!(
            (surround.left(), surround.right()),
            (&[1.0][..], &[2.0][..])
        );
    }

    #[test]
    fn channels_of_different_lengths_are_padded() {
        let buffer = AudioBuffer::new(48_000, vec![vec![1.0; 3], vec![1.0; 1]]);
        assert_eq!(buffer.right(), &[1.0, 0.0, 0.0]);
        assert_eq!(AudioBuffer::new(48_000, Vec::new()).frames(), 0);
        assert!((AudioBuffer::new(4, vec![vec![0.0; 8]]).seconds() - 2.0).abs() < 1e-12);
    }
}
