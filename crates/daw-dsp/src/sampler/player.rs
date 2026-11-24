// SPDX-License-Identifier: GPL-3.0-or-later

//! Plays a [`SamplerBank`]: finds the regions each note starts and runs one
//! voice per sound, with its envelope, loop, filter and choke group.

use std::sync::Arc;

use super::{EnvelopeSpec, LoopMode, OffMode, Region, Sample, SamplerBank, Trigger};
use crate::synth::FilterMode;
use crate::synth::parts::{Random, VoiceFilter};
use crate::{Control, Instrument, InstrumentSettings, NoteShape, Performance, db_to_gain};

/// The most sounds the sampler plays at once.
pub const VOICES: usize = 96;

/// The most regions one note starts.
const MAX_STARTS: usize = 64;

/// How long a sound that is cut off takes to fade (choke groups, voice
/// limits), in seconds.
const FAST_FADE: f32 = 0.006;

/// How long the sounds take to fade when everything stops at once.
const PANIC_FADE: f32 = 0.003;

/// How often a voice's filter follows its envelope, in frames.
const FILTER_STEP: u32 = 16;

/// The quietest level an envelope keeps; below it, it ends (-80 dB).
const FLOOR: f32 = 1e-4;

/// ln(10 000): an exponential segment covers 80 dB in its time.
const SEGMENT: f32 = 9.210_34;

/// What an envelope is doing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Stage {
    #[default]
    Idle,
    Delay,
    Attack,
    Hold,
    Decay,
    Sustain,
    Release,
}

/// An envelope's settings for one note: lengths in frames, falls as factors
/// per frame.
#[derive(Clone, Copy, Debug, Default)]
struct EnvRates {
    delay: u32,
    start: f32,
    attack: u32,
    attack_step: f32,
    hold: u32,
    decay: f32,
    sustain: f32,
    release: f32,
}

impl EnvRates {
    fn new(spec: &EnvelopeSpec, velocity: f32, sample_rate: f32) -> Self {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped to a day of frames"
        )]
        let frames = |seconds: f32| (seconds.clamp(0.0, 86_400.0) * sample_rate).round() as u32;
        // A fall covers 80 dB in its time; the shortest takes half a
        // millisecond, so nothing clicks.
        let factor = |seconds: f32| (-SEGMENT / (seconds.max(0.0005) * sample_rate)).exp();
        let start = spec.start.clamp(0.0, 1.0);
        let attack = frames(spec.attack + spec.vel2attack * velocity);
        #[allow(clippy::cast_precision_loss, reason = "frame counts")]
        let attack_step = (1.0 - start) / attack.max(1) as f32;
        Self {
            delay: frames(spec.delay + spec.vel2delay * velocity),
            start,
            attack,
            attack_step,
            hold: frames(spec.hold + spec.vel2hold * velocity),
            decay: factor(spec.decay + spec.vel2decay * velocity),
            sustain: (spec.sustain + spec.vel2sustain * velocity).clamp(0.0, 1.0),
            release: factor(spec.release + spec.vel2release * velocity),
        }
    }
}

/// A delay, attack, hold, decay, sustain and release envelope.
#[derive(Clone, Copy, Debug, Default)]
struct Envelope {
    stage: Stage,
    level: f32,
    /// Frames left in a delay, attack or hold.
    left: u32,
}

impl Envelope {
    fn start(&mut self, rates: &EnvRates) {
        self.level = 0.0;
        if rates.delay > 0 {
            self.stage = Stage::Delay;
            self.left = rates.delay;
        } else {
            self.attack(rates);
        }
    }

    fn attack(&mut self, rates: &EnvRates) {
        if rates.attack > 0 {
            self.stage = Stage::Attack;
            self.left = rates.attack;
            self.level = rates.start;
        } else {
            self.level = 1.0;
            self.hold(rates);
        }
    }

    fn hold(&mut self, rates: &EnvRates) {
        if rates.hold > 0 {
            self.stage = Stage::Hold;
            self.left = rates.hold;
        } else {
            self.stage = Stage::Decay;
        }
    }

    /// The key is up: falls to silence (at once, if it had not begun).
    fn release(&mut self) {
        match self.stage {
            Stage::Idle => {}
            Stage::Delay => self.stop(),
            _ => self.stage = Stage::Release,
        }
    }

    fn stop(&mut self) {
        self.stage = Stage::Idle;
        self.level = 0.0;
    }

    /// Whether the sound it shapes has ended: idle, or held at silence.
    fn has_ended(self, rates: &EnvRates) -> bool {
        match self.stage {
            Stage::Idle => true,
            Stage::Decay | Stage::Sustain => rates.sustain < FLOOR && self.level < FLOOR,
            _ => false,
        }
    }

    /// Advances one frame and gives the new level.
    fn next(&mut self, rates: &EnvRates) -> f32 {
        match self.stage {
            Stage::Idle | Stage::Sustain => {}
            Stage::Delay => {
                self.left = self.left.saturating_sub(1);
                if self.left == 0 {
                    self.attack(rates);
                }
            }
            Stage::Attack => {
                self.level = (self.level + rates.attack_step).min(1.0);
                self.left = self.left.saturating_sub(1);
                if self.left == 0 {
                    self.level = 1.0;
                    self.hold(rates);
                }
            }
            Stage::Hold => {
                self.left = self.left.saturating_sub(1);
                if self.left == 0 {
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                self.level = rates.sustain + (self.level - rates.sustain) * rates.decay;
                if (self.level - rates.sustain).abs() < FLOOR {
                    self.level = rates.sustain;
                    self.stage = Stage::Sustain;
                }
            }
            Stage::Release => {
                self.level *= rates.release;
                if self.level < FLOOR {
                    self.stop();
                }
            }
        }
        self.level
    }
}

/// One sound playing.
#[derive(Clone, Copy, Default)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "a voice's flags, read in its inner loop"
)]
struct Voice {
    active: bool,
    sample: usize,
    key: u8,
    /// Ends (releases) when its key comes up; one-shots and release
    /// sounds play on.
    follows_key: bool,
    released: bool,
    /// Frames to wait before it starts (the region's delay).
    wait: u32,
    /// Where it is in its sound, in frames.
    position: f64,
    /// How far it moves each frame of output.
    step: f64,
    /// How far it moves with the pitch wheel at rest.
    base_step: f64,
    /// How far the pitch wheel bends it up and down, in cents (decision
    /// 0033).
    bend_up: f32,
    bend_down: f32,
    /// One past the last frame it plays.
    end: f64,
    /// The loop's first frame, and one past its last.
    loop_start: f64,
    loop_end: f64,
    /// Whether it loops now.
    looping: bool,
    /// It stops looping when its key comes up.
    loop_until_release: bool,
    stereo: bool,
    gain_left: f32,
    gain_right: f32,
    width: f32,
    amp: Envelope,
    amp_rates: EnvRates,
    filter: Option<FilterMode>,
    filters: [VoiceFilter; 2],
    /// The cutoff before its envelope, in hertz.
    cutoff: f32,
    resonance: f32,
    filter_env: Envelope,
    filter_rates: EnvRates,
    /// How far the envelope moves the cutoff at its top, in cents.
    filter_depth: f32,
    retune_in: u32,
    /// A quick fade from 1 to 0 when it is cut off.
    fade: f32,
    fade_step: f32,
    group: i64,
    off_by: Option<i64>,
    off_mode: OffMode,
    /// When it started, to find the oldest.
    started: u64,
}

impl Voice {
    fn release(&mut self) {
        self.released = true;
        self.amp.release();
        self.filter_env.release();
        if self.loop_until_release {
            self.looping = false;
        }
    }

    fn fade_out(&mut self, seconds: f32, sample_rate: f32) {
        self.fade_step = self.fade_step.max(1.0 / (seconds * sample_rate).max(1.0));
    }

    fn is_fading(&self) -> bool {
        self.fade_step > 0.0
    }

    /// The sound at its position, both channels.
    fn read(&self, left: &[f32], right: &[f32]) -> (f32, f32) {
        let whole = self.position.floor();
        #[allow(
            clippy::cast_possible_truncation,
            reason = "a fraction of a frame; positions stay far below 2^63"
        )]
        let (fraction, index) = ((self.position - whole) as f32, whole as i64);
        let l = self.interpolate(left, index, fraction);
        let r = if self.stereo {
            self.interpolate(right, index, fraction)
        } else {
            l
        };
        (l, r)
    }

    /// Four-point interpolation around `index` (a straight copy of the
    /// samples away from the edges; the loop and the end handled near
    /// them).
    fn interpolate(&self, data: &[f32], index: i64, fraction: f32) -> f32 {
        let limit = if self.looping {
            self.loop_end
        } else {
            self.end
        };
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss,
            reason = "checked to be inside the sound"
        )]
        if index >= 1 && ((index + 2) as f64) < limit && ((index + 2) as usize) < data.len() {
            let at = index as usize - 1;
            return hermite(data[at], data[at + 1], data[at + 2], data[at + 3], fraction);
        }
        hermite(
            self.at(data, index - 1),
            self.at(data, index),
            self.at(data, index + 1),
            self.at(data, index + 2),
            fraction,
        )
    }

    /// One sample, the loop followed and silence outside the sound.
    fn at(&self, data: &[f32], index: i64) -> f32 {
        #[allow(
            clippy::cast_possible_truncation,
            reason = "loop points are frame positions"
        )]
        let (loop_start, loop_end) = (self.loop_start as i64, self.loop_end as i64);
        let mut index = index;
        if self.looping && index >= loop_end && loop_end > loop_start {
            index = loop_start + (index - loop_start) % (loop_end - loop_start);
        }
        #[allow(clippy::cast_precision_loss, reason = "frame positions")]
        if index < 0 || index as f64 >= self.end {
            return 0.0;
        }
        usize::try_from(index)
            .ok()
            .and_then(|i| data.get(i))
            .copied()
            .unwrap_or(0.0)
    }

    /// Points the filter at its present cutoff (its envelope moved).
    fn retune(&mut self, sample_rate: f32) {
        if let Some(mode) = self.filter {
            let cents = self.filter_env.level * self.filter_depth;
            let cutoff =
                (self.cutoff * 2.0_f32.powf(cents / 1200.0)).clamp(10.0, 0.45 * sample_rate);
            for filter in &mut self.filters {
                filter.tune(mode, cutoff, self.resonance, sample_rate);
            }
        }
    }

    /// Adds `left.len()` frames of this voice to the output.
    fn render(&mut self, sample: &Sample, left: &mut [f32], right: &mut [f32], sample_rate: f32) {
        let (source_left, source_right) = (&sample.left[..], sample.right_or_left());
        for (out_left, out_right) in left.iter_mut().zip(right.iter_mut()) {
            if self.wait > 0 {
                self.wait -= 1;
                continue;
            }
            let level = self.amp.next(&self.amp_rates);
            if self.amp.has_ended(&self.amp_rates) {
                self.active = false;
                return;
            }
            if self.fade_step > 0.0 {
                self.fade -= self.fade_step;
                if self.fade <= 0.0 {
                    self.active = false;
                    return;
                }
            }
            let (mut l, mut r) = self.read(source_left, source_right);
            if self.filter.is_some() {
                self.filter_env.next(&self.filter_rates);
                if self.retune_in == 0 {
                    self.retune(sample_rate);
                    self.retune_in = FILTER_STEP;
                }
                self.retune_in -= 1;
                l = self.filters[0].process(l);
                r = if self.stereo {
                    self.filters[1].process(r)
                } else {
                    l
                };
            }
            if self.stereo && (self.width - 1.0).abs() > f32::EPSILON {
                let mid = f32::midpoint(l, r);
                let side = (l - r) * 0.5 * self.width;
                l = mid + side;
                r = mid - side;
            }
            let gain = level * self.fade;
            *out_left += l * self.gain_left * gain;
            *out_right += r * self.gain_right * gain;
            self.position += self.step;
            if self.looping {
                let length = self.loop_end - self.loop_start;
                if self.position >= self.loop_end && length > 0.0 {
                    self.position = self.loop_start + (self.position - self.loop_start) % length;
                }
            } else if self.position >= self.end {
                self.active = false;
                return;
            }
        }
    }
}

/// Four-point, third-order Hermite interpolation between `here` and `next`.
fn hermite(before: f32, here: f32, next: f32, after: f32, fraction: f32) -> f32 {
    let slope = (next - before) * 0.5;
    let fall = here - next;
    let middle = slope + fall;
    let cubic = middle + fall + (after - here) * 0.5;
    let square = middle + cubic;
    ((cubic * fraction - square) * fraction + slope) * fraction + here
}

/// A velocity from 0 to 1 as a MIDI velocity (1 to 127).
fn velocity_number(velocity: f32) -> u8 {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to 1..=127"
    )]
    let number = (velocity.clamp(0.0, 1.0) * 127.0).round().clamp(1.0, 127.0) as u8;
    number
}

/// The resonance of a voice's filter (0 to 1) for a resonance in decibels:
/// the peak of the gentle low-pass rises by about that much.
fn resonance_amount(decibels: f32) -> f32 {
    let damping = std::f32::consts::SQRT_2 / 10.0_f32.powf(decibels.clamp(0.0, 40.0) / 20.0);
    ((std::f32::consts::SQRT_2 - damping) / (std::f32::consts::SQRT_2 - 0.05)).clamp(0.0, 1.0)
}

/// Plays a [`SamplerBank`].
pub struct Sampler {
    bank: Arc<SamplerBank>,
    sample_rate: f32,
    voices: Vec<Voice>,
    /// The regions a key going down may start, for each key.
    attacks: Vec<Vec<u32>>,
    /// The regions a key coming up may start, for each key.
    releases: Vec<Vec<u32>>,
    /// Each region's round-robin count.
    turns: Vec<u32>,
    held: [bool; 128],
    velocity: [u8; 128],
    pressed_at: [u64; 128],
    keyswitch: Option<u8>,
    /// The key played before the present note.
    previous: Option<u8>,
    /// Frames played so far.
    clock: u64,
    /// Voices started so far.
    starts: u64,
    random: Random,
    /// How notes start and end, over the regions' own (decision 0031).
    shape: NoteShape,
    /// Where the pitch wheel and the expression pedal stand (decision
    /// 0033).
    performance: Performance,
    /// The expression pedal's gain, gliding towards the pedal's.
    expression: f32,
    /// How fast that gain glides, per frame.
    expression_glide: f32,
    /// A drum kit made of sounds answers no controller.
    drum_kit: bool,
}

impl Sampler {
    /// A sampler for a bank at the engine's sample rate.
    pub fn new(bank: Arc<SamplerBank>, sample_rate: u32) -> Self {
        let mut attacks = vec![Vec::new(); 128];
        let mut releases = vec![Vec::new(); 128];
        for (index, region) in bank.regions.iter().enumerate() {
            let Ok(index) = u32::try_from(index) else {
                break;
            };
            let lists = if region.trigger == Trigger::Release {
                &mut releases
            } else {
                &mut attacks
            };
            for key in region.lokey..=region.hikey.min(127) {
                lists[usize::from(key)].push(index);
            }
        }
        #[allow(clippy::cast_precision_loss, reason = "sample rates are small")]
        let rate = sample_rate.max(1) as f32;
        Self {
            turns: vec![0; bank.regions.len()],
            keyswitch: bank.keyswitch.and_then(|k| k.default),
            bank,
            sample_rate: rate,
            voices: vec![Voice::default(); VOICES],
            attacks,
            releases,
            held: [false; 128],
            velocity: [0; 128],
            pressed_at: [0; 128],
            previous: None,
            clock: 0,
            starts: 0,
            random: Random::new(0x5EED_5A3F),
            shape: NoteShape::default(),
            performance: Performance::REST,
            expression: 1.0,
            // About 5 ms to follow the pedal: no clicks, no lag.
            expression_glide: 1.0 - (-1.0 / (0.005 * rate)).exp(),
            drum_kit: false,
        }
    }

    /// Makes it a drum kit (a custom kit's sounds, decision 0025): the
    /// pitch wheel and the expression pedal leave it alone, as drums are
    /// left alone (decision 0033).
    pub fn set_drum_kit(&mut self, drum_kit: bool) {
        self.drum_kit = drum_kit;
    }

    /// How notes start and end from now on (decision 0031): the attack and
    /// release replace the regions' own; with a release, sounds that play
    /// to their end (one-shots) stop when their key comes up.
    pub fn set_shape(&mut self, shape: NoteShape) {
        self.shape = shape.sane();
    }

    /// The regions a key starts now, into `chosen`; how many there are.
    fn choose(
        &mut self,
        release: bool,
        key: u8,
        velocity: u8,
        chance: f32,
        chosen: &mut [u32; MAX_STARTS],
    ) -> usize {
        let others_held = self
            .held
            .iter()
            .enumerate()
            .any(|(k, &h)| h && k != usize::from(key));
        let candidates = if release {
            self.releases[usize::from(key)].len()
        } else {
            self.attacks[usize::from(key)].len()
        };
        let mut count = 0;
        for i in 0..candidates {
            let index = if release {
                self.releases[usize::from(key)][i]
            } else {
                self.attacks[usize::from(key)][i]
            };
            let region = &self.bank.regions[index as usize];
            if !(region.lovel..=region.hivel).contains(&velocity) {
                continue;
            }
            if region.sw_last.is_some() && region.sw_last != self.keyswitch {
                continue;
            }
            if region
                .sw_down
                .is_some_and(|k| !self.held[usize::from(k & 0x7F)])
                || region
                    .sw_up
                    .is_some_and(|k| self.held[usize::from(k & 0x7F)])
                || (region.sw_previous.is_some() && region.sw_previous != self.previous)
            {
                continue;
            }
            match region.trigger {
                Trigger::First if others_held => continue,
                Trigger::Legato if !others_held => continue,
                _ => {}
            }
            // The top of the range takes the top value too.
            let lucky = chance >= region.lorand && (chance < region.hirand || region.hirand >= 1.0);
            if !lucky {
                continue;
            }
            if region.seq_length > 1 {
                let turn = self.turns[index as usize];
                self.turns[index as usize] = turn.wrapping_add(1);
                if turn % region.seq_length + 1 != region.seq_position {
                    continue;
                }
            }
            if count < MAX_STARTS {
                chosen[count] = index;
                count += 1;
            }
        }
        count
    }

    /// Cuts off the sounds the choke group `group` turns off.
    fn cut_off(&mut self, group: i64) {
        let rate = self.sample_rate;
        for voice in self
            .voices
            .iter_mut()
            .filter(|v| v.active && v.off_by == Some(group))
        {
            match voice.off_mode {
                OffMode::Fast => voice.fade_out(FAST_FADE, rate),
                OffMode::Normal => voice.release(),
            }
        }
    }

    /// Fades the oldest sounds `matches` picks until fewer than `most`
    /// remain.
    fn limit(&mut self, most: u32, matches: impl Fn(&Voice) -> bool) {
        let most = usize::try_from(most.max(1)).unwrap_or(usize::MAX);
        loop {
            let playing = self
                .voices
                .iter()
                .filter(|v| v.active && !v.is_fading() && matches(v))
                .count();
            if playing < most {
                return;
            }
            let Some(oldest) = self
                .voices
                .iter_mut()
                .filter(|v| v.active && !v.is_fading() && matches(v))
                .min_by_key(|v| v.started)
            else {
                return;
            };
            oldest.fade_out(FAST_FADE, self.sample_rate);
        }
    }

    /// A voice to use: a silent one, else the quietest released one, else
    /// the oldest.
    fn free_voice(&self) -> usize {
        if let Some(index) = self.voices.iter().position(|v| !v.active) {
            return index;
        }
        let released = (0..VOICES)
            .filter(|&i| self.voices[i].released || self.voices[i].is_fading())
            .min_by(|&a, &b| {
                let level = |i: usize| self.voices[i].amp.level * self.voices[i].fade;
                level(a).total_cmp(&level(b))
            });
        released.unwrap_or_else(|| {
            (0..VOICES)
                .min_by_key(|&i| self.voices[i].started)
                .unwrap_or(0)
        })
    }

    /// Starts every region chosen for a note (choke groups first).
    fn start_all(&mut self, chosen: &[u32], key: u8, velocity: u8, gain: impl Fn(&Region) -> f32) {
        let bank = Arc::clone(&self.bank);
        for &index in chosen {
            self.cut_off(bank.regions[index as usize].group);
        }
        for &index in chosen {
            let region = &bank.regions[index as usize];
            let release = region.trigger == Trigger::Release;
            self.start(&bank, region, key, velocity, gain(region), release);
        }
    }

    /// Starts one region's sound.
    fn start(
        &mut self,
        bank: &SamplerBank,
        region: &Region,
        key: u8,
        velocity: u8,
        extra_gain: f32,
        from_release: bool,
    ) {
        let Some(sample_index) = region.sample else {
            return;
        };
        let Some(sample) = bank.samples.get(sample_index) else {
            return;
        };
        if sample.frames() < 2 {
            return;
        }
        let gain = self.gain(region, key, velocity) * extra_gain;
        if gain <= 0.0 {
            return;
        }
        if let Some(most) = region.note_polyphony {
            let group = region.group;
            self.limit(most, |v| v.key == key && v.group == group);
        }
        if let Some(most) = region.polyphony {
            let group = region.group;
            self.limit(most, |v| v.group == group);
        }
        let vel = f32::from(velocity) / 127.0;
        let place = self.placement(region, sample, from_release);
        let filter = self.filter(region, key, vel);

        let (base_step, bend_up, bend_down) = self.pitch(region, sample, key, vel);
        let step = base_step * bend_ratio(self.performance.bend, bend_up, bend_down);

        let mut ampeg = region.ampeg;
        if let Some(attack) = self.shape.attack {
            ampeg.attack = attack;
            ampeg.vel2attack = 0.0;
        }
        // A release region's sound (a damper) plays out as it is.
        if let Some(release) = self.shape.release.filter(|_| !from_release) {
            ampeg.release = release;
            ampeg.vel2release = 0.0;
        }
        let amp_rates = EnvRates::new(&ampeg, vel, self.sample_rate);
        let mut amp = Envelope::default();
        amp.start(&amp_rates);
        let mut filter_env = Envelope::default();
        if filter.mode.is_some() {
            filter_env.start(&filter.rates);
        }
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped to a minute of frames"
        )]
        let wait = ((region.delay + region.delay_random * self.random.next_bipolar().abs())
            .clamp(0.0, 60.0)
            * self.sample_rate) as u32;
        let pan = region.pan.clamp(-1.0, 1.0);
        self.starts += 1;
        let slot = self.free_voice();
        #[allow(clippy::cast_precision_loss, reason = "frame positions")]
        let mut voice = Voice {
            active: true,
            sample: sample_index,
            key,
            follows_key: !from_release
                && (place.mode != LoopMode::OneShot || self.shape.release.is_some()),
            released: false,
            wait,
            position: place.first as f64,
            step,
            base_step,
            bend_up,
            bend_down,
            end: (place.last + 1) as f64,
            loop_start: place.loop_start as f64,
            loop_end: (place.loop_end + 1) as f64,
            looping: place.looping,
            loop_until_release: place.mode == LoopMode::Sustain,
            stereo: !sample.right.is_empty(),
            gain_left: gain * (1.0 - pan).min(1.0),
            gain_right: gain * (1.0 + pan).min(1.0),
            width: region.width.clamp(-1.0, 1.0),
            amp,
            amp_rates,
            filter: filter.mode,
            filters: [VoiceFilter::default(); 2],
            cutoff: filter.cutoff,
            resonance: filter.resonance,
            filter_env,
            filter_rates: filter.rates,
            filter_depth: filter.depth,
            retune_in: 0,
            fade: 1.0,
            fade_step: 0.0,
            group: region.group,
            off_by: region.off_by,
            off_mode: region.off_mode,
            started: self.starts,
        };
        voice.retune(self.sample_rate);
        self.voices[slot] = voice;
    }

    /// How fast a region's sound plays for a key with the pitch wheel at
    /// rest, and how far the wheel bends it up and down, in cents (decision
    /// 0033; a drum kit is not bent).
    fn pitch(&mut self, region: &Region, sample: &Sample, key: u8, vel: f32) -> (f64, f32, f32) {
        #[allow(clippy::cast_precision_loss, reason = "semitones")]
        let cents = (f32::from(key) - f32::from(region.pitch_keycenter)) * region.pitch_keytrack
            + region.transpose as f32 * 100.0
            + region.tune
            + region.pitch_veltrack * vel
            + region.pitch_random * self.random.next_bipolar();
        let step = f64::from(2.0_f32.powf(cents.clamp(-12_000.0, 12_000.0) / 1200.0))
            * f64::from(sample.sample_rate)
            / f64::from(self.sample_rate);
        if self.drum_kit {
            (step, 0.0, 0.0)
        } else {
            (step, region.bend_up, region.bend_down)
        }
    }

    /// A region's gain for a note: its volume, level, velocity, key and
    /// crossfades.
    fn gain(&mut self, region: &Region, key: u8, velocity: u8) -> f32 {
        let decibels = region.volume
            + region.amp_keytrack * (f32::from(key) - f32::from(region.amp_keycenter))
            + region.amp_random * self.random.next_bipolar().abs();
        db_to_gain(decibels.clamp(-144.0, 48.0))
            * region.amplitude.max(0.0)
            * region.velocity_gain(velocity)
            * region.crossfade(key, velocity)
    }

    /// Where in its sound a region plays, and how it loops.
    fn placement(&mut self, region: &Region, sample: &Sample, from_release: bool) -> Placement {
        let frames = sample.frames() as u64;
        let last = region.end.map_or(frames - 1, |end| end.min(frames - 1));
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss,
            reason = "a random part of a frame count"
        )]
        let jitter = (self.random.next_unit() * region.offset_random as f64) as u64;
        let mode = region.loop_mode.unwrap_or(if sample.file_loop.is_some() {
            LoopMode::Continuous
        } else {
            LoopMode::NoLoop
        });
        // A release sound plays once, whatever its loop.
        let mode = if from_release && matches!(mode, LoopMode::Continuous | LoopMode::Sustain) {
            LoopMode::NoLoop
        } else {
            mode
        };
        let loop_start = region
            .loop_start
            .or(sample.file_loop.map(|l| l.0))
            .unwrap_or(0);
        let loop_end = region
            .loop_end
            .or(sample.file_loop.map(|l| l.1))
            .unwrap_or(last)
            .min(last);
        Placement {
            first: region.offset.saturating_add(jitter).min(last),
            last,
            loop_start,
            loop_end,
            looping: loop_end > loop_start
                && matches!(mode, LoopMode::Continuous | LoopMode::Sustain),
            mode,
        }
    }

    /// A region's filter for a note: its cutoff after key and velocity
    /// tracking, its resonance and its envelope.
    fn filter(&mut self, region: &Region, key: u8, vel: f32) -> FilterStart {
        let Some(spec) = &region.filter else {
            return FilterStart::default();
        };
        let cents = spec.keytrack * (f32::from(key) - f32::from(spec.keycenter))
            + spec.veltrack * vel
            + spec.random * self.random.next_bipolar();
        FilterStart {
            mode: Some(spec.mode),
            cutoff: spec.cutoff * 2.0_f32.powf(cents.clamp(-12_000.0, 12_000.0) / 1200.0),
            resonance: resonance_amount(spec.resonance),
            rates: EnvRates::new(&spec.envelope, vel, self.sample_rate),
            depth: spec.envelope.depth,
        }
    }
}

/// Where a voice plays in its sound: frames, first to last.
struct Placement {
    first: u64,
    last: u64,
    loop_start: u64,
    loop_end: u64,
    looping: bool,
    mode: LoopMode,
}

/// A voice's filter as it starts.
#[derive(Default)]
struct FilterStart {
    mode: Option<FilterMode>,
    cutoff: f32,
    resonance: f32,
    rates: EnvRates,
    depth: f32,
}

impl Instrument for Sampler {
    fn note_on(&mut self, key: u8, velocity: f32) {
        let key = key & 0x7F;
        let velocity = velocity_number(velocity);
        if let Some(switch) = self.bank.keyswitch
            && (switch.lokey..=switch.hikey).contains(&key)
        {
            self.keyswitch = Some(key);
        }
        let chance = self.random.next_unit();
        #[allow(clippy::cast_possible_truncation, reason = "a number from 0 to 1")]
        let chance = chance as f32;
        let mut chosen = [0; MAX_STARTS];
        let count = self.choose(false, key, velocity, chance, &mut chosen);
        let k = usize::from(key);
        self.held[k] = true;
        self.velocity[k] = velocity;
        self.pressed_at[k] = self.clock;
        self.previous = Some(key);
        self.start_all(&chosen[..count], key, velocity, |_| 1.0);
    }

    fn note_off(&mut self, key: u8) {
        let key = key & 0x7F;
        let k = usize::from(key);
        let was_held = self.held[k];
        self.held[k] = false;
        for voice in self
            .voices
            .iter_mut()
            .filter(|v| v.active && v.key == key && v.follows_key && !v.released)
        {
            voice.release();
        }
        if !was_held || self.releases[k].is_empty() {
            return;
        }
        // The key's release sounds: quieter the longer it was held.
        let velocity = self.velocity[k];
        #[allow(clippy::cast_precision_loss, reason = "frame counts")]
        let held_for = self.clock.saturating_sub(self.pressed_at[k]) as f32 / self.sample_rate;
        let chance = self.random.next_unit();
        #[allow(clippy::cast_possible_truncation, reason = "a number from 0 to 1")]
        let chance = chance as f32;
        let mut chosen = [0; MAX_STARTS];
        let count = self.choose(true, key, velocity, chance, &mut chosen);
        self.start_all(&chosen[..count], key, velocity, |region| {
            db_to_gain(-(region.rt_decay.max(0.0) * held_for).min(144.0))
        });
    }

    fn apply_settings(&mut self, settings: &InstrumentSettings) {
        if let InstrumentSettings::Shape(shape) = settings {
            self.set_shape(*shape);
        }
    }

    fn all_notes_off(&mut self, immediate: bool) {
        self.held = [false; 128];
        let rate = self.sample_rate;
        for voice in self.voices.iter_mut().filter(|v| v.active) {
            if immediate {
                voice.fade_out(PANIC_FADE, rate);
            } else if voice.follows_key {
                voice.release();
            }
        }
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        left.fill(0.0);
        right.fill(0.0);
        let len = left.len().min(right.len());
        let bank = Arc::clone(&self.bank);
        let rate = self.sample_rate;
        for voice in self.voices.iter_mut().filter(|v| v.active) {
            if let Some(sample) = bank.samples.get(voice.sample) {
                voice.render(sample, &mut left[..len], &mut right[..len], rate);
            } else {
                voice.active = false;
            }
        }
        // The expression pedal (decision 0033), easing towards its value.
        let target = self.performance.expression_gain();
        if (target - self.expression).abs() > 1e-6 || self.expression < 1.0 {
            let glide = self.expression_glide;
            for (l, r) in left[..len].iter_mut().zip(&mut right[..len]) {
                self.expression += (target - self.expression) * glide;
                *l *= self.expression;
                *r *= self.expression;
            }
            if (target - self.expression).abs() < 1e-6 {
                self.expression = target;
            }
        }
        self.clock += len as u64;
    }

    fn is_sounding(&self) -> bool {
        self.voices.iter().any(|v| v.active)
    }

    /// The pitch wheel bends every sounding voice by its region's range;
    /// the expression pedal shapes the loudness (decision 0033).
    fn control(&mut self, control: Control) {
        if self.drum_kit {
            return;
        }
        let bend = self.performance.bend;
        if !self.performance.take(control) || self.performance.bend.to_bits() == bend.to_bits() {
            return;
        }
        let bend = self.performance.bend;
        for voice in self.voices.iter_mut().filter(|v| v.active) {
            voice.step = voice.base_step * bend_ratio(bend, voice.bend_up, voice.bend_down);
        }
    }
}

/// How much faster a sound plays with the pitch wheel at `bend` (-1 to 1),
/// for a region bending `up` cents at the top and `down` cents at the
/// bottom (negative to bend down).
fn bend_ratio(bend: f32, up: f32, down: f32) -> f64 {
    let cents = if bend >= 0.0 { bend * up } else { -bend * down };
    if cents == 0.0 {
        1.0
    } else {
        f64::from(2.0_f32.powf(cents.clamp(-9600.0, 9600.0) / 1200.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sampler::{FilterSpec, KeySwitch};

    const RATE: u32 = 48_000;

    /// A second of a sine at `hz`, at 48 kHz.
    fn tone(hz: f32, level: f32) -> Sample {
        Sample {
            sample_rate: RATE,
            left: (0..48_000)
                .map(|i| {
                    #[allow(clippy::cast_precision_loss, reason = "test values")]
                    let t = i as f32 / 48_000.0;
                    level * (std::f32::consts::TAU * hz * t).sin()
                })
                .collect(),
            right: Vec::new(),
            file_loop: None,
        }
    }

    fn bank(regions: Vec<Region>, samples: Vec<Sample>) -> Arc<SamplerBank> {
        Arc::new(SamplerBank {
            regions,
            samples,
            ..SamplerBank::default()
        })
    }

    fn render(sampler: &mut Sampler, frames: usize) -> (Vec<f32>, Vec<f32>) {
        let (mut l, mut r) = (vec![0.0; frames], vec![0.0; frames]);
        sampler.render(&mut l, &mut r);
        (l, r)
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0, |m, s| m.max(s.abs()))
    }

    /// The frequency of a sound, from its rising zero crossings.
    fn frequency(samples: &[f32]) -> f32 {
        let crossings: Vec<usize> = (1..samples.len())
            .filter(|&i| samples[i - 1] < 0.0 && samples[i] >= 0.0)
            .collect();
        let (first, last) = (crossings[0], crossings[crossings.len() - 1]);
        #[allow(clippy::cast_precision_loss, reason = "test values")]
        let hz = (crossings.len() - 1) as f32 * RATE as f32 / (last - first) as f32;
        hz
    }

    #[test]
    fn a_region_plays_its_sound_at_the_pitch_of_each_key() {
        let region = Region {
            sample: Some(0),
            pitch_keycenter: 69,
            amp_veltrack: 0.0,
            ..Region::default()
        };
        let mut sampler = Sampler::new(bank(vec![region], vec![tone(440.0, 0.5)]), RATE);
        sampler.note_on(69, 1.0);
        let (l, r) = render(&mut sampler, 4800);
        assert!((frequency(&l) - 440.0).abs() < 1.0, "{}", frequency(&l));
        assert!((peak(&l) - 0.5).abs() < 0.01, "full level: {}", peak(&l));
        assert_eq!(l, r, "a mono sound sits in the middle");

        sampler.all_notes_off(true);
        render(&mut sampler, 4800);
        sampler.note_on(81, 1.0);
        let (octave, _) = render(&mut sampler, 4800);
        assert!((frequency(&octave) - 880.0).abs() < 2.0, "an octave up");

        let mut sampler = Sampler::new(
            bank(
                vec![Region {
                    sample: Some(0),
                    pitch_keycenter: 69,
                    tune: -1200.0,
                    ..Region::default()
                }],
                vec![tone(440.0, 0.5)],
            ),
            44_100,
        );
        sampler.note_on(69, 1.0);
        let (down, _) = render(&mut sampler, 8820);
        // Played at 44.1 kHz, measured as if 48 kHz: 220 × 48/44.1.
        let expected = 220.0 * 48_000.0 / 44_100.0;
        assert!(
            (frequency(&down) - expected).abs() < 1.5,
            "{}",
            frequency(&down)
        );
    }

    #[test]
    fn keys_velocities_and_turns_choose_the_regions() {
        let soft = Region {
            sample: Some(0),
            hivel: 63,
            amp_veltrack: 0.0,
            ..Region::default()
        };
        let loud = Region {
            sample: Some(1),
            lovel: 64,
            amp_veltrack: 0.0,
            ..Region::default()
        };
        let high = Region {
            sample: Some(0),
            lokey: 72,
            hikey: 72,
            ..Region::default()
        };
        let regions = vec![
            Region { hikey: 71, ..soft },
            Region { hikey: 71, ..loud },
            high,
        ];
        let mut sampler = Sampler::new(
            bank(regions, vec![tone(440.0, 0.2), tone(440.0, 0.8)]),
            RATE,
        );
        sampler.note_on(60, 0.3);
        assert!((peak(&render(&mut sampler, 2000).0) - 0.2).abs() < 0.01);
        sampler.all_notes_off(true);
        render(&mut sampler, 2000);
        sampler.note_on(60, 0.9);
        assert!((peak(&render(&mut sampler, 2000).0) - 0.8).abs() < 0.01);
        sampler.all_notes_off(true);
        render(&mut sampler, 2000);
        sampler.note_on(100, 1.0);
        assert!(!sampler.is_sounding(), "no region on key 100");

        // Round robin: two regions take turns.
        let turn = |position, level| Region {
            sample: Some(level),
            seq_length: 2,
            seq_position: position,
            amp_veltrack: 0.0,
            ..Region::default()
        };
        let mut sampler = Sampler::new(
            bank(
                vec![turn(1, 0), turn(2, 1)],
                vec![tone(440.0, 0.2), tone(440.0, 0.8)],
            ),
            RATE,
        );
        let mut peaks = Vec::new();
        for _ in 0..4 {
            sampler.note_on(60, 1.0);
            peaks.push((peak(&render(&mut sampler, 1000).0) * 10.0).round());
            sampler.all_notes_off(true);
            render(&mut sampler, 2000);
        }
        assert_eq!(peaks, [2.0, 8.0, 2.0, 8.0]);
    }

    #[test]
    fn keyswitches_choose_the_articulation() {
        let regions = vec![
            Region {
                sample: Some(0),
                lokey: 48,
                sw_last: Some(24),
                amp_veltrack: 0.0,
                ..Region::default()
            },
            Region {
                sample: Some(1),
                lokey: 48,
                sw_last: Some(25),
                amp_veltrack: 0.0,
                ..Region::default()
            },
        ];
        let mut sampler = Sampler::new(
            Arc::new(SamplerBank {
                regions,
                samples: vec![tone(440.0, 0.2), tone(440.0, 0.8)],
                keyswitch: Some(KeySwitch {
                    lokey: 24,
                    hikey: 25,
                    default: Some(24),
                }),
                ..SamplerBank::default()
            }),
            RATE,
        );
        sampler.note_on(60, 1.0);
        assert!(
            (peak(&render(&mut sampler, 1000).0) - 0.2).abs() < 0.01,
            "the default"
        );
        sampler.note_off(60);
        render(&mut sampler, 2000);
        sampler.note_on(25, 1.0);
        sampler.note_off(25);
        assert!(!sampler.is_sounding(), "a keyswitch itself is silent here");
        sampler.note_on(60, 1.0);
        assert!(
            (peak(&render(&mut sampler, 1000).0) - 0.8).abs() < 0.01,
            "switched"
        );
    }

    #[test]
    fn held_keys_and_the_previous_note_choose_regions() {
        let chirp = Region {
            sample: Some(0),
            lokey: 64,
            hikey: 64,
            sw_down: Some(41),
            amp_veltrack: 0.0,
            ..Region::default()
        };
        let after_c = Region {
            sample: Some(1),
            lokey: 62,
            hikey: 62,
            sw_previous: Some(60),
            amp_veltrack: 0.0,
            ..Region::default()
        };
        let plain = Region {
            sample: Some(0),
            hikey: 61,
            sw_up: Some(41),
            amp_veltrack: 0.0,
            ..Region::default()
        };
        let mut sampler = Sampler::new(
            bank(
                vec![chirp, after_c, plain],
                vec![tone(440.0, 0.2), tone(440.0, 0.8)],
            ),
            RATE,
        );
        sampler.note_on(64, 1.0);
        assert!(!sampler.is_sounding(), "only while key 41 is held");
        sampler.note_on(41, 1.0);
        sampler.note_on(64, 1.0);
        assert!(sampler.is_sounding());
        sampler.all_notes_off(true);
        render(&mut sampler, 2000);
        sampler.note_on(60, 1.0);
        assert!(sampler.is_sounding(), "41 is up again");
        sampler.note_on(62, 1.0);
        let (after, _) = render(&mut sampler, 1000);
        assert!(peak(&after) > 0.9, "62 after 60 plays its region");
        sampler.all_notes_off(true);
        render(&mut sampler, 2000);
        sampler.note_on(62, 1.0);
        assert!(!sampler.is_sounding(), "62 after 62 does not");
    }

    #[test]
    fn the_envelope_shapes_and_ends_each_sound() {
        let mut ampeg = EnvelopeSpec::amp();
        ampeg.attack = 0.01;
        ampeg.release = 0.1;
        let region = Region {
            sample: Some(0),
            ampeg,
            amp_veltrack: 0.0,
            loop_mode: Some(LoopMode::Continuous),
            loop_start: Some(0),
            loop_end: Some(47_999),
            ..Region::default()
        };
        let mut sampler = Sampler::new(bank(vec![region], vec![tone(480.0, 0.5)]), RATE);
        sampler.note_on(60, 1.0);
        let (rise, _) = render(&mut sampler, 480);
        assert!(peak(&rise[..100]) < 0.15, "rising: {}", peak(&rise[..100]));
        let (held, _) = render(&mut sampler, 96_000);
        assert!(
            peak(&held[90_000..]) > 0.49,
            "the loop sustains past the sound's end"
        );
        sampler.note_off(60);
        let (fall, _) = render(&mut sampler, 4800);
        assert!(peak(&fall[2400..]) < 0.02, "the release: -80 dB in 0.1 s");
        render(&mut sampler, 480);
        assert!(!sampler.is_sounding());
    }

    #[test]
    fn one_shots_release_sounds_and_choke_groups() {
        let hat = |sample, group, off_by| Region {
            sample: Some(sample),
            group,
            off_by,
            amp_veltrack: 0.0,
            loop_mode: Some(LoopMode::OneShot),
            ..Region::default()
        };
        let mut open = hat(0, 2, Some(1));
        open.lokey = 46;
        open.hikey = 46;
        let mut closed = hat(1, 1, None);
        closed.lokey = 42;
        closed.hikey = 42;
        let release = Region {
            sample: Some(1),
            lokey: 60,
            hikey: 60,
            trigger: Trigger::Release,
            rt_decay: 6.0,
            amp_veltrack: 0.0,
            ..Region::default()
        };
        let mut sampler = Sampler::new(
            bank(
                vec![open, closed, release],
                vec![tone(440.0, 0.5), tone(880.0, 0.5)],
            ),
            RATE,
        );
        sampler.note_on(46, 1.0);
        sampler.note_off(46);
        assert!(sampler.is_sounding(), "a one-shot plays on");
        sampler.note_on(42, 1.0);
        render(&mut sampler, 960);
        let open_left = sampler
            .voices
            .iter()
            .filter(|v| v.active && v.sample == 0)
            .count();
        assert_eq!(open_left, 0, "the closed hat cut off the open one");
        sampler.all_notes_off(true);
        render(&mut sampler, 960);

        // A release sound: on the key coming up, 6 dB quieter after a second.
        sampler.note_on(60, 1.0);
        assert!(!sampler.is_sounding(), "nothing on the key going down");
        render(&mut sampler, 48_000);
        sampler.note_off(60);
        let (after, _) = render(&mut sampler, 2000);
        assert!(
            (peak(&after) - 0.5 * db_to_gain(-6.0)).abs() < 0.01,
            "{}",
            peak(&after)
        );
    }

    #[test]
    fn a_filter_takes_the_highs_away() {
        let filtered = Region {
            sample: Some(0),
            amp_veltrack: 0.0,
            filter: Some(FilterSpec {
                mode: FilterMode::LowPass24,
                cutoff: 500.0,
                resonance: 0.0,
                keytrack: 0.0,
                keycenter: 60,
                veltrack: 0.0,
                random: 0.0,
                envelope: EnvelopeSpec::filter(),
            }),
            ..Region::default()
        };
        let mut sampler = Sampler::new(bank(vec![filtered], vec![tone(4000.0, 0.5)]), RATE);
        sampler.note_on(60, 1.0);
        let (l, _) = render(&mut sampler, 4800);
        assert!(peak(&l[2400..]) < 0.01, "3 octaves above a 24 dB cutoff");
    }
}
