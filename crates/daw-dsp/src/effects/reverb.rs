// SPDX-License-Identifier: GPL-3.0-or-later

//! The reverb: the sound of a room. Built on the classic public-domain
//! "Freeverb" design by Jezar at Dreampoint (2000): eight parallel comb
//! filters and four all-pass filters per channel, here retuned for any
//! sample rate, with a pre-delay in front.

/// The reverb's settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReverbParams {
    /// How big the room is, 0 to 1 (longer tails).
    pub size: f32,
    /// How quickly high frequencies fade, 0 to 1 (darker tails).
    pub damping: f32,
    /// Stereo width of the tail, 0 (mono) to 1.
    pub width: f32,
    /// Balance between the dry sound and the room, 0 to 1.
    pub mix: f32,
    /// Silence before the room answers, in milliseconds.
    pub pre_delay_ms: f32,
}

impl Default for ReverbParams {
    fn default() -> Self {
        Self {
            size: 0.6,
            damping: 0.5,
            width: 1.0,
            mix: 0.25,
            pre_delay_ms: 10.0,
        }
    }
}

/// Comb and all-pass lengths at 44.1 kHz, from the original design.
const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASSES: [usize; 4] = [556, 441, 341, 225];
/// The right channel's filters are this much longer, for width.
const SPREAD: usize = 23;
/// The longest pre-delay, in seconds.
const MAX_PRE_DELAY: f32 = 0.25;
/// Input gain into the filters (keeps the sum from overflowing).
const INPUT_GAIN: f32 = 0.015;
/// Output scaling of the wet signal.
const WET_SCALE: f32 = 3.0;

struct Comb {
    buffer: Vec<f32>,
    index: usize,
    store: f32,
}

impl Comb {
    fn new(length: usize) -> Self {
        Self {
            buffer: vec![0.0; length.max(1)],
            index: 0,
            store: 0.0,
        }
    }

    #[inline]
    fn process(&mut self, input: f32, feedback: f32, damp: f32) -> f32 {
        let output = self.buffer[self.index];
        self.store = output * (1.0 - damp) + self.store * damp;
        self.buffer[self.index] = input + self.store * feedback;
        self.index += 1;
        if self.index == self.buffer.len() {
            self.index = 0;
        }
        output
    }

    fn clear(&mut self) {
        self.buffer.fill(0.0);
        self.store = 0.0;
    }
}

struct AllPass {
    buffer: Vec<f32>,
    index: usize,
}

impl AllPass {
    fn new(length: usize) -> Self {
        Self {
            buffer: vec![0.0; length.max(1)],
            index: 0,
        }
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let delayed = self.buffer[self.index];
        let output = delayed - input;
        self.buffer[self.index] = input + delayed * 0.5;
        self.index += 1;
        if self.index == self.buffer.len() {
            self.index = 0;
        }
        output
    }

    fn clear(&mut self) {
        self.buffer.fill(0.0);
    }
}

/// A stereo room reverb.
pub struct Reverb {
    params: ReverbParams,
    sample_rate: f32,
    combs: [Vec<Comb>; 2],
    allpasses: [Vec<AllPass>; 2],
    pre_delay: Vec<f32>,
    pre_index: usize,
    pre_length: usize,
}

impl Reverb {
    /// A reverb with these settings. All memory is reserved here.
    pub fn new(params: &ReverbParams, sample_rate: u32) -> Self {
        #[allow(clippy::cast_precision_loss, reason = "sample rates are small")]
        let rate = sample_rate as f32;
        let scale = |length: usize| {
            #[allow(
                clippy::cast_precision_loss,
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "short filter lengths"
            )]
            let scaled = (length as f32 * rate / 44_100.0).round() as usize;
            scaled
        };
        let channel = |offset: usize| -> (Vec<Comb>, Vec<AllPass>) {
            (
                COMBS
                    .iter()
                    .map(|&n| Comb::new(scale(n + offset)))
                    .collect(),
                ALLPASSES
                    .iter()
                    .map(|&n| AllPass::new(scale(n + offset)))
                    .collect(),
            )
        };
        let (combs_left, allpasses_left) = channel(0);
        let (combs_right, allpasses_right) = channel(SPREAD);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a short, positive length"
        )]
        let max_pre = (MAX_PRE_DELAY * rate) as usize + 1;
        let mut reverb = Self {
            params: *params,
            sample_rate: rate,
            combs: [combs_left, combs_right],
            allpasses: [allpasses_left, allpasses_right],
            pre_delay: vec![0.0; max_pre],
            pre_index: 0,
            pre_length: 0,
        };
        reverb.set_params(params);
        reverb
    }

    /// Changes the settings; the tail carries on.
    pub fn set_params(&mut self, params: &ReverbParams) {
        self.params = *params;
        let max = self.pre_delay.len() - 1;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a short, positive length"
        )]
        let length = (params.pre_delay_ms.max(0.0) * 0.001 * self.sample_rate) as usize;
        self.pre_length = length.min(max);
    }

    /// Processes stereo sound in place.
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let p = self.params;
        let feedback = 0.7 + 0.28 * p.size.clamp(0.0, 1.0);
        let damp = 0.4 * p.damping.clamp(0.0, 1.0);
        let mix = p.mix.clamp(0.0, 1.0);
        let wet = mix * WET_SCALE;
        let width = p.width.clamp(0.0, 1.0);
        let wet_same = wet * (width / 2.0 + 0.5);
        let wet_other = wet * ((1.0 - width) / 2.0);
        let dry = 1.0 - mix;
        let frames = self.pre_delay.len();
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            // Pre-delay: a ring buffer holding the mono input sum.
            let input = (*l + *r) * INPUT_GAIN;
            self.pre_delay[self.pre_index] = input;
            let read = (self.pre_index + frames - self.pre_length) % frames;
            let delayed = self.pre_delay[read];
            self.pre_index = (self.pre_index + 1) % frames;

            let mut out = [0.0_f32; 2];
            for (channel, value) in out.iter_mut().enumerate() {
                let mut sum = 0.0;
                for comb in &mut self.combs[channel] {
                    sum += comb.process(delayed, feedback, damp);
                }
                for allpass in &mut self.allpasses[channel] {
                    sum = allpass.process(sum);
                }
                *value = sum;
            }
            let (dl, dr) = (*l, *r);
            *l = dl * dry + out[0] * wet_same + out[1] * wet_other;
            *r = dr * dry + out[1] * wet_same + out[0] * wet_other;
        }
    }

    /// Silences the tail at once.
    pub fn reset(&mut self) {
        for channel in &mut self.combs {
            channel.iter_mut().for_each(Comb::clear);
        }
        for channel in &mut self.allpasses {
            channel.iter_mut().for_each(AllPass::clear);
        }
        self.pre_delay.fill(0.0);
    }
}
