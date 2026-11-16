// SPDX-License-Identifier: GPL-3.0-or-later

//! The second-order filter every EQ band is made of, with the standard
//! coefficient formulas (R. Bristow-Johnson's "Audio EQ Cookbook").
//! Computed in double precision, so low frequencies stay accurate.

use std::f64::consts::TAU;

/// The shape of a filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterKind {
    /// Removes what is above the frequency.
    LowPass,
    /// Removes what is below the frequency.
    HighPass,
    /// A bell around the frequency.
    Peak,
    /// Raises or lowers everything below the frequency.
    LowShelf,
    /// Raises or lowers everything above the frequency.
    HighShelf,
}

/// A stereo second-order filter (transposed direct form II).
#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: [f64; 2],
    z2: [f64; 2],
}

impl Default for Biquad {
    fn default() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            z1: [0.0; 2],
            z2: [0.0; 2],
        }
    }
}

impl Biquad {
    /// Sets the filter's shape. `q` is the bandwidth (0.707 is the
    /// flattest response for low- and high-pass filters); `gain_db` is used
    /// by the peak and shelf shapes. The state is kept, so changing a
    /// setting while sound plays does not click.
    pub fn set(
        &mut self,
        kind: FilterKind,
        sample_rate: f64,
        frequency: f64,
        q: f64,
        gain_db: f64,
    ) {
        let frequency = frequency.clamp(10.0, sample_rate * 0.45);
        let q = q.clamp(0.1, 24.0);
        let w0 = TAU * frequency / sample_rate;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        let a = 10.0_f64.powf(gain_db / 40.0);
        let (b0, b1, b2, a0, a1, a2) = match kind {
            FilterKind::LowPass => {
                let b = f64::midpoint(1.0, -cos);
                (b, 1.0 - cos, b, 1.0 + alpha, -2.0 * cos, 1.0 - alpha)
            }
            FilterKind::HighPass => {
                let b = f64::midpoint(1.0, cos);
                (b, -(1.0 + cos), b, 1.0 + alpha, -2.0 * cos, 1.0 - alpha)
            }
            FilterKind::Peak => (
                1.0 + alpha * a,
                -2.0 * cos,
                1.0 - alpha * a,
                1.0 + alpha / a,
                -2.0 * cos,
                1.0 - alpha / a,
            ),
            FilterKind::LowShelf => {
                let s = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) - (a - 1.0) * cos + s),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
                    a * ((a + 1.0) - (a - 1.0) * cos - s),
                    (a + 1.0) + (a - 1.0) * cos + s,
                    -2.0 * ((a - 1.0) + (a + 1.0) * cos),
                    (a + 1.0) + (a - 1.0) * cos - s,
                )
            }
            FilterKind::HighShelf => {
                let s = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) + (a - 1.0) * cos + s),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
                    a * ((a + 1.0) + (a - 1.0) * cos - s),
                    (a + 1.0) - (a - 1.0) * cos + s,
                    2.0 * ((a - 1.0) - (a + 1.0) * cos),
                    (a + 1.0) - (a - 1.0) * cos - s,
                )
            }
        };
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }

    /// Filters one sample of channel `channel` (0 or 1).
    #[inline]
    pub fn process(&mut self, channel: usize, input: f32) -> f32 {
        let x = f64::from(input);
        let y = self.b0 * x + self.z1[channel];
        self.z1[channel] = self.b1 * x - self.a1 * y + self.z2[channel];
        self.z2[channel] = self.b2 * x - self.a2 * y;
        #[allow(clippy::cast_possible_truncation, reason = "audio samples")]
        let y = y as f32;
        y
    }

    /// Forgets the filter's memory (silence in, silence out at once).
    pub fn reset(&mut self) {
        self.z1 = [0.0; 2];
        self.z2 = [0.0; 2];
    }

    /// How much the filter amplifies a sine at `frequency`, in decibels,
    /// from its coefficients (for tests and for drawing the EQ curve).
    pub fn response_db(&self, sample_rate: f64, frequency: f64) -> f64 {
        let w = TAU * frequency / sample_rate;
        let (sin1, cos1) = w.sin_cos();
        let (sin2, cos2) = (2.0 * w).sin_cos();
        let num_re = self.b0 + self.b1 * cos1 + self.b2 * cos2;
        let num_im = -(self.b1 * sin1 + self.b2 * sin2);
        let den_re = 1.0 + self.a1 * cos1 + self.a2 * cos2;
        let den_im = -(self.a1 * sin1 + self.a2 * sin2);
        let magnitude = (num_re.hypot(num_im)) / den_re.hypot(den_im).max(1e-30);
        20.0 * magnitude.max(1e-30).log10()
    }
}
