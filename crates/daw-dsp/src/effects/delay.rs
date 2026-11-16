// SPDX-License-Identifier: GPL-3.0-or-later

//! The delay: echoes, each one quieter and a little darker than the last,
//! optionally bouncing between left and right ("ping-pong"). Changing the
//! time while it plays glides smoothly instead of clicking.

/// The delay's settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DelayParams {
    /// Time between echoes, in seconds (the engine converts note values
    /// such as a dotted eighth into seconds at the song's tempo).
    pub time_seconds: f32,
    /// How much of each echo comes back as the next one, 0 to 0.95.
    pub feedback: f32,
    /// Balance between the dry sound and the echoes, 0 to 1.
    pub mix: f32,
    /// Echoes lose the frequencies above this, in hertz.
    pub tone_hz: f32,
    /// Echoes alternate between left and right.
    pub ping_pong: bool,
}

impl Default for DelayParams {
    fn default() -> Self {
        Self {
            time_seconds: 0.375,
            feedback: 0.35,
            mix: 0.25,
            tone_hz: 6000.0,
            ping_pong: false,
        }
    }
}

/// The longest delay time, in seconds.
pub const MAX_DELAY_SECONDS: f32 = 4.0;

/// A stereo feedback delay.
pub struct Delay {
    params: DelayParams,
    sample_rate: f32,
    buffers: [Vec<f32>; 2],
    write: usize,
    /// The delay in frames, gliding towards the target.
    current: f32,
    target: f32,
    glide: f32,
    tone: f32,
    tone_state: [f32; 2],
}

impl Delay {
    /// A delay with these settings. All memory is reserved here.
    pub fn new(params: &DelayParams, sample_rate: u32) -> Self {
        #[allow(clippy::cast_precision_loss, reason = "sample rates are small")]
        let rate = sample_rate as f32;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a bounded, positive length"
        )]
        let length = (MAX_DELAY_SECONDS * rate) as usize + 4;
        let mut delay = Self {
            params: *params,
            sample_rate: rate,
            buffers: [vec![0.0; length], vec![0.0; length]],
            write: 0,
            current: 0.0,
            target: 0.0,
            // About 60 ms to glide to a new time.
            glide: (-1.0 / (0.060 * rate)).exp(),
            tone: 0.0,
            tone_state: [0.0; 2],
        };
        delay.set_params(params);
        delay.current = delay.target;
        delay
    }

    /// Changes the settings; the echoes already sounding carry on.
    pub fn set_params(&mut self, params: &DelayParams) {
        self.params = *params;
        #[allow(clippy::cast_precision_loss, reason = "buffers of a few seconds")]
        let max = self.buffers[0].len() as f32 - 3.0;
        self.target = (params.time_seconds * self.sample_rate).clamp(1.0, max);
        let cutoff = params.tone_hz.clamp(200.0, self.sample_rate * 0.45);
        self.tone = (-std::f32::consts::TAU * cutoff / self.sample_rate).exp();
    }

    /// Reads a channel `delay` frames back, between samples.
    #[inline]
    fn read(&self, channel: usize, delay: f32) -> f32 {
        let length = self.buffers[channel].len();
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "positions inside the buffer"
        )]
        let position = (self.write + length) as f32 - delay;
        let whole = position.floor();
        let fraction = position - whole;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "positions inside the buffer"
        )]
        let index = whole as usize % length;
        let next = (index + 1) % length;
        let buffer = &self.buffers[channel];
        buffer[index] + (buffer[next] - buffer[index]) * fraction
    }

    /// Processes stereo sound in place.
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let feedback = self.params.feedback.clamp(0.0, 0.95);
        let mix = self.params.mix.clamp(0.0, 1.0);
        let dry = 1.0 - mix;
        let ping_pong = self.params.ping_pong;
        let length = self.buffers[0].len();
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            self.current = self.target + self.glide * (self.current - self.target);
            let mut echo = [self.read(0, self.current), self.read(1, self.current)];
            // Each pass through the loop loses some highs.
            for (channel, value) in echo.iter_mut().enumerate() {
                self.tone_state[channel] = *value + self.tone * (self.tone_state[channel] - *value);
                *value = self.tone_state[channel];
            }
            let (input_left, input_right) = (*l, *r);
            if ping_pong {
                // The input enters on the left; each echo crosses over.
                self.buffers[0][self.write] =
                    f32::midpoint(input_left, input_right) + echo[1] * feedback;
                self.buffers[1][self.write] = echo[0] * feedback;
            } else {
                self.buffers[0][self.write] = input_left + echo[0] * feedback;
                self.buffers[1][self.write] = input_right + echo[1] * feedback;
            }
            self.write = (self.write + 1) % length;
            *l = input_left * dry + echo[0] * mix;
            *r = input_right * dry + echo[1] * mix;
        }
    }

    /// Silences the echoes at once.
    pub fn reset(&mut self) {
        for buffer in &mut self.buffers {
            buffer.fill(0.0);
        }
        self.tone_state = [0.0; 2];
    }
}
