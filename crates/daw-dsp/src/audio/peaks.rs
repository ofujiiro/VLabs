// SPDX-License-Identifier: GPL-3.0-or-later

//! Summaries of loudness for drawing waveforms: for each run of frames, the
//! lowest and the highest sample of either channel, kept for runs of several
//! lengths, so a waveform at any zoom is drawn from a few values per pixel.
//! Built in one go for a file, or piece by piece while recording.

/// Frames summed up by each value of the finest level.
const BASE_RUN: usize = 32;
/// Each level sums up this many values of the level below.
const FACTOR: usize = 4;

/// The lowest and highest sample of a run.
type Extremes = (f32, f32);

const NOTHING: Extremes = (f32::INFINITY, f32::NEG_INFINITY);

fn widen(a: Extremes, b: Extremes) -> Extremes {
    (a.0.min(b.0), a.1.max(b.1))
}

/// The loudness summary of a sound.
#[derive(Clone, Debug, PartialEq)]
pub struct Peaks {
    sample_rate: u32,
    frames: usize,
    /// `levels[k]` holds one value per run of `BASE_RUN × FACTORᵏ` frames.
    levels: Vec<Vec<Extremes>>,
    /// The run still being filled at each level, and how much it holds
    /// (frames at level 0, values of the level below above it).
    open: Vec<(Extremes, usize)>,
}

impl Peaks {
    /// An empty summary, for sound at `sample_rate`.
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate: sample_rate.max(1),
            frames: 0,
            levels: vec![Vec::new()],
            open: vec![(NOTHING, 0)],
        }
    }

    /// The summary of a whole buffer.
    pub fn of(buffer: &super::AudioBuffer) -> Self {
        let mut peaks = Self::new(buffer.sample_rate());
        peaks.push(buffer.left(), buffer.right());
        peaks
    }

    /// Adds frames at the end: the two channels side by side (pass the same
    /// slice twice for mono).
    pub fn push(&mut self, left: &[f32], right: &[f32]) {
        for (&l, &r) in left.iter().zip(right) {
            self.push_frame((l.min(r), l.max(r)));
        }
    }

    /// Adds interleaved frames of `channels` samples (the first two count).
    pub fn push_interleaved(&mut self, samples: &[f32], channels: usize) {
        let channels = channels.max(1);
        for frame in samples.chunks_exact(channels) {
            let kept = &frame[..channels.min(2)];
            let low = kept.iter().copied().fold(f32::INFINITY, f32::min);
            let high = kept.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            self.push_frame((low, high));
        }
    }

    fn push_frame(&mut self, extremes: Extremes) {
        self.frames += 1;
        let mut value = extremes;
        let mut level = 0;
        loop {
            let (open, count) = &mut self.open[level];
            *open = widen(*open, value);
            *count += 1;
            let full = if level == 0 { BASE_RUN } else { FACTOR };
            if *count < full {
                return;
            }
            value = *open;
            *open = NOTHING;
            *count = 0;
            self.levels[level].push(value);
            level += 1;
            if level == self.levels.len() {
                self.levels.push(Vec::new());
                self.open.push((NOTHING, 0));
            }
        }
    }

    /// Frames per second of the sound summed up.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// How many frames are summed up.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// The lowest and highest sample from frame `start` to `end` (rounded
    /// out to whole runs of the finest level), or `None` for no frames.
    pub fn range(&self, start: usize, end: usize) -> Option<(f32, f32)> {
        let end = end.min(self.frames);
        if start >= end {
            return None;
        }
        let found = self.range_at(self.levels.len() - 1, start, end);
        (found.0 <= found.1).then_some(found)
    }

    /// The extremes of `start..end` from the values of `level` that lie
    /// wholly inside it, and finer levels for its edges.
    fn range_at(&self, level: usize, start: usize, end: usize) -> Extremes {
        if start >= end {
            return NOTHING;
        }
        let length = run(level);
        let values = &self.levels[level];
        let covered = values.len() * length;
        if level == 0 {
            let first = (start / length).min(values.len());
            let last = end.min(covered).div_ceil(length).clamp(first, values.len());
            let found = values[first..last]
                .iter()
                .fold(NOTHING, |a, &b| widen(a, b));
            // The run still being filled holds the very end.
            return if end > covered {
                widen(found, self.open[0].0)
            } else {
                found
            };
        }
        let inner_start = start.div_ceil(length) * length;
        let inner_end = (end / length * length).min(covered);
        if inner_start >= inner_end {
            return self.range_at(level - 1, start, end);
        }
        let inside = values[inner_start / length..inner_end / length]
            .iter()
            .fold(NOTHING, |a, &b| widen(a, b));
        widen(
            inside,
            widen(
                self.range_at(level - 1, start, inner_start),
                self.range_at(level - 1, inner_end, end),
            ),
        )
    }
}

/// Frames per value at a level.
fn run(level: usize) -> usize {
    BASE_RUN * FACTOR.pow(u32::try_from(level).unwrap_or(u32::MAX).min(20))
}

#[cfg(test)]
#[allow(clippy::cast_precision_loss, reason = "test signals")]
mod tests {
    use super::super::AudioBuffer;
    use super::*;

    fn ramp(frames: usize) -> Vec<f32> {
        (0..frames).map(|i| i as f32 / frames as f32).collect()
    }

    #[test]
    fn the_extremes_of_a_span_are_found_at_any_zoom() {
        let samples = ramp(100_000);
        let peaks = Peaks::of(&AudioBuffer::new(48_000, vec![samples.clone()]));
        assert_eq!(peaks.frames(), 100_000);
        // Spans on the finest runs' boundaries give exact answers.
        for (start, end) in [
            (0, 32),
            (3200, 6400),
            (0, 65_536),
            (32_768, 98_304),
            (96, 99_968),
        ] {
            let (low, high) = peaks.range(start, end).unwrap();
            assert_eq!(
                (low, high),
                (samples[start], samples[end - 1]),
                "{start}..{end}"
            );
        }
        // The very end, not yet a whole run at the coarse levels.
        let (_, high) = peaks.range(90_000, 100_000).unwrap();
        assert_eq!(high, samples[99_999]);
        assert_eq!(peaks.range(100_000, 200_000), None);
    }

    #[test]
    fn a_single_loud_sample_shows_when_zoomed_out() {
        let mut quiet = vec![0.01_f32; 1_000_000];
        quiet[777_777] = -0.9;
        let peaks = Peaks::of(&AudioBuffer::new(48_000, vec![quiet.clone(), quiet]));
        assert_eq!(peaks.range(0, 1_000_000), Some((-0.9, 0.01)));
        assert_eq!(peaks.range(0, 700_000), Some((0.01, 0.01)));
    }

    #[test]
    fn building_piece_by_piece_matches_building_at_once() {
        let left = ramp(10_000);
        let right: Vec<f32> = left.iter().map(|s| -s).collect();
        let whole = Peaks::of(&AudioBuffer::new(44_100, vec![left.clone(), right.clone()]));
        let mut pieces = Peaks::new(44_100);
        let interleaved: Vec<f32> = left
            .iter()
            .zip(&right)
            .flat_map(|(&l, &r)| [l, r])
            .collect();
        for chunk in interleaved.chunks(2 * 777) {
            pieces.push_interleaved(chunk, 2);
        }
        assert_eq!(pieces, whole);
        assert_eq!(
            whole.range(0, 10_000).map(|(l, h)| l <= -0.99 && h >= 0.99),
            Some(true)
        );
    }
}
