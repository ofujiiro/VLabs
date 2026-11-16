// SPDX-License-Identifier: GPL-3.0-or-later

//! The EQ: a low cut, a low shelf, a bell in the middle, a high shelf and a
//! high cut. Five bands cover nearly every mixing task while staying easy to
//! read; a band left at 0 dB (or a cut left off) costs nothing.

use super::biquad::{Biquad, FilterKind};

/// The EQ's settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EqParams {
    /// Removes rumble below this frequency (12 dB per octave), if set.
    pub low_cut_hz: Option<f32>,
    /// Low shelf: gain in dB and corner frequency.
    pub low_gain_db: f32,
    /// The low shelf's corner frequency.
    pub low_hz: f32,
    /// Bell: gain in dB, centre frequency and width.
    pub mid_gain_db: f32,
    /// The bell's centre frequency.
    pub mid_hz: f32,
    /// The bell's width (Q): higher is narrower.
    pub mid_q: f32,
    /// High shelf: gain in dB and corner frequency.
    pub high_gain_db: f32,
    /// The high shelf's corner frequency.
    pub high_hz: f32,
    /// Removes hiss above this frequency (12 dB per octave), if set.
    pub high_cut_hz: Option<f32>,
}

impl Default for EqParams {
    fn default() -> Self {
        Self {
            low_cut_hz: None,
            low_gain_db: 0.0,
            low_hz: 120.0,
            mid_gain_db: 0.0,
            mid_hz: 1000.0,
            mid_q: 0.9,
            high_gain_db: 0.0,
            high_hz: 6000.0,
            high_cut_hz: None,
        }
    }
}

/// A five-band stereo EQ.
pub struct Eq {
    sample_rate: f64,
    params: EqParams,
    bands: [Biquad; 5],
    active: [bool; 5],
}

impl Eq {
    /// An EQ with these settings.
    pub fn new(params: &EqParams, sample_rate: u32) -> Self {
        let mut eq = Self {
            sample_rate: f64::from(sample_rate),
            params: *params,
            bands: [Biquad::default(); 5],
            active: [false; 5],
        };
        eq.configure();
        eq
    }

    /// Changes the settings, keeping the filters' memory.
    pub fn set_params(&mut self, params: &EqParams) {
        if *params != self.params {
            self.params = *params;
            self.configure();
        }
    }

    fn configure(&mut self) {
        let p = self.params;
        let rate = self.sample_rate;
        let flat = |gain: f32| gain.abs() < 0.01;
        let bands = [
            (p.low_cut_hz
                .map(|hz| (FilterKind::HighPass, hz, 0.707, 0.0))),
            (!flat(p.low_gain_db)).then_some((
                FilterKind::LowShelf,
                p.low_hz,
                0.707,
                p.low_gain_db,
            )),
            (!flat(p.mid_gain_db)).then_some((FilterKind::Peak, p.mid_hz, p.mid_q, p.mid_gain_db)),
            (!flat(p.high_gain_db)).then_some((
                FilterKind::HighShelf,
                p.high_hz,
                0.707,
                p.high_gain_db,
            )),
            (p.high_cut_hz
                .map(|hz| (FilterKind::LowPass, hz, 0.707, 0.0))),
        ];
        for (index, band) in bands.into_iter().enumerate() {
            match band {
                Some((kind, hz, q, gain)) => {
                    if !self.active[index] {
                        self.bands[index].reset();
                    }
                    self.bands[index].set(kind, rate, f64::from(hz), f64::from(q), f64::from(gain));
                    self.active[index] = true;
                }
                None => self.active[index] = false,
            }
        }
    }

    /// Processes stereo sound in place.
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (band, _) in self.bands.iter_mut().zip(self.active).filter(|(_, on)| *on) {
            for sample in left.iter_mut() {
                *sample = band.process(0, *sample);
            }
            for sample in right.iter_mut() {
                *sample = band.process(1, *sample);
            }
        }
    }

    /// Forgets the filters' memory.
    pub fn reset(&mut self) {
        for band in &mut self.bands {
            band.reset();
        }
    }

    /// The EQ's total response at a frequency, in dB (for drawing its curve).
    pub fn response_db(&self, frequency: f32) -> f32 {
        let total: f64 = self
            .bands
            .iter()
            .zip(self.active)
            .filter(|(_, on)| *on)
            .map(|(band, _)| band.response_db(self.sample_rate, f64::from(frequency)))
            .sum();
        #[allow(clippy::cast_possible_truncation, reason = "a display value")]
        let total = total as f32;
        total
    }
}
