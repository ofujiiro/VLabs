// SPDX-License-Identifier: GPL-3.0-or-later

//! A voice recording, analysed for the resampler (decision 0027): its pitch
//! along the way (YIN), and a mark on every period of its voiced parts (a
//! fixed short step through the unvoiced ones).

/// The lowest and highest pitch looked for, in Hz.
const MIN_HZ: f64 = 70.0;
const MAX_HZ: f64 = 1100.0;
/// How often the pitch is measured.
const HOP_SECONDS: f64 = 0.0075;
/// YIN's threshold: below it, the frame is voiced.
const YIN_THRESHOLD: f64 = 0.15;
/// Quieter frames count as silence.
const SILENCE_RMS: f64 = 1e-3;
/// The step between marks where the sound is unvoiced.
const UNVOICED_STEP_SECONDS: f64 = 0.005;

/// A point the resampler takes a grain around: the peak of a period (or a
/// fixed step in unvoiced sound), and the period there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Mark {
    /// The sample the grain is centred on.
    pub at: usize,
    /// The period there, in samples (the grain spans one each side).
    pub period: f64,
    /// Whether the sound is voiced there.
    pub voiced: bool,
}

/// One mono recording of a voicebank, analysed.
#[derive(Clone, Debug)]
pub struct VoiceSample {
    rate: u32,
    samples: Vec<f32>,
    marks: Vec<Mark>,
    /// The pitch of each frame (`HOP_SECONDS` apart), if voiced.
    pitch: Vec<Option<f64>>,
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "sample positions are far below 2^52 and never negative"
)]
impl VoiceSample {
    /// Analyses a mono recording at `rate`.
    pub fn new(rate: u32, samples: Vec<f32>) -> Self {
        let rate = rate.max(1);
        let pitch = pitch_track(&samples, rate);
        let marks = pitch_marks(&samples, rate, &pitch);
        Self {
            rate,
            samples,
            marks,
            pitch,
        }
    }

    /// The recording's sample rate.
    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// The recording's samples.
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    /// How long the recording lasts, in seconds.
    pub fn seconds(&self) -> f64 {
        self.samples.len() as f64 / f64::from(self.rate)
    }

    /// The median pitch of the voiced frames between two times (seconds),
    /// if any are voiced.
    pub fn pitch_between(&self, from: f64, to: f64) -> Option<f64> {
        let first = (from / HOP_SECONDS).max(0.0) as usize;
        let last = ((to / HOP_SECONDS).max(0.0) as usize).min(self.pitch.len());
        let mut voiced: Vec<f64> = self
            .pitch
            .get(first..last)?
            .iter()
            .filter_map(|p| *p)
            .collect();
        if voiced.is_empty() {
            return None;
        }
        voiced.sort_by(f64::total_cmp);
        Some(voiced[voiced.len() / 2])
    }

    /// The sample at a fractional position (linear interpolation; silence
    /// outside the recording).
    pub(crate) fn at(&self, position: f64) -> f32 {
        if position < 0.0 {
            return 0.0;
        }
        let index = position as usize;
        let Some(&a) = self.samples.get(index) else {
            return 0.0;
        };
        let b = self.samples.get(index + 1).copied().unwrap_or(0.0);
        let fraction = (position - index as f64) as f32;
        a + (b - a) * fraction
    }

    /// The mark nearest a sample position.
    pub(crate) fn mark_near(&self, position: f64) -> Mark {
        if self.marks.is_empty() {
            return Mark {
                at: position.max(0.0) as usize,
                period: f64::from(self.rate) * UNVOICED_STEP_SECONDS,
                voiced: false,
            };
        }
        let index = self.marks.partition_point(|m| (m.at as f64) < position);
        let after = self.marks.get(index);
        let before = index.checked_sub(1).and_then(|i| self.marks.get(i));
        match (before, after) {
            (Some(b), Some(a)) => {
                if position - b.at as f64 <= a.at as f64 - position {
                    *b
                } else {
                    *a
                }
            }
            (Some(m), None) | (None, Some(m)) => *m,
            (None, None) => self.marks[0],
        }
    }
}

/// The pitch of each frame by YIN (de Cheveigné and Kawahara, 2002): the
/// cumulative mean normalised difference, its first dip under the
/// threshold, refined by a parabola. `None` for silent or unvoiced frames.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "lags and positions are small and never negative"
)]
fn pitch_track(x: &[f32], rate: u32) -> Vec<Option<f64>> {
    let rate = f64::from(rate);
    let hop = ((rate * HOP_SECONDS) as usize).max(1);
    let max_lag = (rate / MIN_HZ).ceil() as usize;
    let min_lag = ((rate / MAX_HZ).floor() as usize).max(2);
    let window = max_lag;
    let frames = x.len() / hop + 1;
    let mut pitch = Vec::with_capacity(frames);
    let mut difference = vec![0.0_f64; max_lag + 1];
    for frame in 0..frames {
        let centre = frame * hop;
        let start = centre.saturating_sub(window / 2);
        if start + window + max_lag > x.len() {
            pitch.push(None);
            continue;
        }
        let energy: f64 = x[start..start + window]
            .iter()
            .map(|&s| f64::from(s) * f64::from(s))
            .sum::<f64>()
            / window as f64;
        if energy.sqrt() < SILENCE_RMS {
            pitch.push(None);
            continue;
        }
        for (lag, d) in difference.iter_mut().enumerate().skip(1) {
            let mut sum = 0.0;
            for j in start..start + window {
                let delta = f64::from(x[j]) - f64::from(x[j + lag]);
                sum += delta * delta;
            }
            *d = sum;
        }
        // Normalised by the running mean of the differences so far.
        let mut running = 0.0;
        let mut normalised = vec![1.0_f64; max_lag + 1];
        for lag in 1..=max_lag {
            running += difference[lag];
            normalised[lag] = if running > 0.0 {
                difference[lag] * lag as f64 / running
            } else {
                1.0
            };
        }
        let mut found = None;
        let mut lag = min_lag;
        while lag < max_lag {
            if normalised[lag] < YIN_THRESHOLD {
                while lag + 1 < max_lag && normalised[lag + 1] < normalised[lag] {
                    lag += 1;
                }
                found = Some(lag);
                break;
            }
            lag += 1;
        }
        pitch.push(found.map(|lag| {
            let (a, b, c) = (
                normalised[lag - 1],
                normalised[lag],
                normalised[(lag + 1).min(max_lag)],
            );
            let curve = a - 2.0 * b + c;
            let shift = if curve.abs() > 1e-12 {
                (0.5 * (a - c) / curve).clamp(-1.0, 1.0)
            } else {
                0.0
            };
            rate / (lag as f64 + shift)
        }));
    }
    smooth(&mut pitch);
    pitch
}

/// Removes lone voiced or unvoiced frames and octave slips with a median
/// over five frames.
fn smooth(pitch: &mut [Option<f64>]) {
    let original = pitch.to_vec();
    for i in 0..pitch.len() {
        let lo = i.saturating_sub(2);
        let hi = (i + 3).min(original.len());
        let around = &original[lo..hi];
        let voiced: Vec<f64> = around.iter().filter_map(|p| *p).collect();
        if voiced.len() * 2 <= around.len() {
            pitch[i] = None;
            continue;
        }
        let mut sorted = voiced;
        sorted.sort_by(f64::total_cmp);
        let median = sorted[sorted.len() / 2];
        pitch[i] = Some(match original[i] {
            // An octave slip: take the median instead.
            Some(p) if (p / median).log2().abs() < 0.5 => p,
            _ => median,
        });
    }
}

/// A mark on every period of the voiced parts, at the period's peak, and
/// every few milliseconds through the rest.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "positions are small and never negative"
)]
fn pitch_marks(x: &[f32], rate: u32, pitch: &[Option<f64>]) -> Vec<Mark> {
    let rate = f64::from(rate);
    let hop = ((rate * HOP_SECONDS) as usize).max(1);
    let unvoiced = (rate * UNVOICED_STEP_SECONDS).max(1.0);
    let mut marks = Vec::new();
    let mut position = 0_usize;
    let mut was_voiced = false;
    while position < x.len() {
        if let Some(hz) = pitch.get(position / hop).copied().flatten() {
            let period = rate / hz;
            // The first period: its peak; later ones: the peak near
            // where the period says the next one is.
            let (from, to) = if was_voiced {
                let reach = (period * 0.25) as usize;
                (position.saturating_sub(reach), position + reach)
            } else {
                (position, position + period as usize)
            };
            let to = to.min(x.len().saturating_sub(1));
            let at = (from..=to.max(from))
                .filter(|&i| i < x.len())
                .max_by(|&a, &b| x[a].total_cmp(&x[b]))
                .unwrap_or(position);
            marks.push(Mark {
                at,
                period,
                voiced: true,
            });
            position = at + (period.round() as usize).max(1);
            was_voiced = true;
        } else {
            marks.push(Mark {
                at: position,
                period: unvoiced,
                voiced: false,
            });
            position += unvoiced as usize;
            was_voiced = false;
        }
    }
    marks
}
