// SPDX-License-Identifier: GPL-3.0-or-later

//! The sound editor's side of the session (decision 0031): which audio
//! clip it shows, and the changes it makes (where the clip starts and ends
//! in its sound, backwards, pitch, as loud as it can be).

use daw_model::{ClipContent, ClipId, Ticks};

use super::{Session, SoundState};
use crate::text;

/// Where an audio clip lies in its sound, counted in seconds of the sound
/// as the clip plays it (backwards and at its pitch's speed): from the
/// first moment of the sound it could play to the last.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipStretch {
    /// How long the whole sound lasts, played this way.
    pub total: f64,
    /// Where the clip starts in it.
    pub start: f64,
    /// Where the clip ends in it.
    pub end: f64,
    /// How fast the sound plays (its pitch change).
    pub speed: f64,
    /// Whether it plays backwards.
    pub reverse: bool,
}

/// The loudest a clip is made by "as loud as it can be": just under full
/// scale (-0.2 dB), so nothing clips.
const NORMAL_PEAK: f32 = 0.977;

impl Session {
    /// Opens the sound editor on an audio clip.
    pub fn open_sound_editor(&mut self, clip: ClipId) {
        if self.project().clip(clip).is_some_and(|(_, c)| c.is_audio()) {
            self.sound_editor = Some(clip);
        }
    }

    /// Closes the sound editor.
    pub fn close_sound_editor(&mut self) {
        self.sound_editor = None;
    }

    /// The clip the sound editor shows, if it is open (it closes when its
    /// clip is gone).
    pub fn sound_editor(&mut self) -> Option<ClipId> {
        let clip = self.sound_editor?;
        if self.project().clip(clip).is_some_and(|(_, c)| c.is_audio()) {
            Some(clip)
        } else {
            self.sound_editor = None;
            None
        }
    }

    /// An audio clip's pitch change (in semitones) and whether it plays
    /// backwards.
    pub fn clip_motion(&self, clip: ClipId) -> Option<(f32, bool)> {
        match self.project().clip(clip)?.1.content {
            ClipContent::Audio { pitch, reverse, .. } => Some((pitch, reverse)),
            ClipContent::Pattern { .. } => None,
        }
    }

    /// Plays an audio clip backwards, or forwards again: the same stretch
    /// of its sound.
    pub fn set_clip_reverse(&mut self, clip: ClipId, reverse: bool) {
        if self.clip_motion(clip).is_none_or(|(_, r)| r == reverse) {
            return;
        }
        self.doc.edit(text::EDIT_CLIP_REVERSE, |p| {
            p.set_clip_reverse(clip, reverse);
        });
    }

    /// Changes an audio clip's pitch, in semitones: it plays faster or
    /// slower, and so gets shorter or longer.
    pub fn set_clip_pitch(&mut self, clip: ClipId, semitones: f32) {
        if self
            .clip_motion(clip)
            .is_none_or(|(p, _)| (p - semitones).abs() < 1e-4)
        {
            return;
        }
        self.doc.edit(text::EDIT_CLIP_PITCH, |p| {
            p.set_clip_pitch(clip, semitones);
        });
    }

    /// Where an audio clip lies in its sound (see [`ClipStretch`]).
    pub fn clip_stretch(&self, clip: ClipId) -> Option<ClipStretch> {
        let project = self.project();
        let (_, c) = project.clip(clip)?;
        let ClipContent::Audio {
            audio,
            offset,
            reverse,
            pitch,
            ..
        } = c.content
        else {
            return None;
        };
        let file = project.audio_file(audio)?;
        let speed = daw_model::pitch_speed(pitch);
        let at = file.seconds_in(offset);
        let start = if reverse { file.seconds() - at } else { at } / speed;
        let length = project.seconds_between(c.start, c.end());
        Some(ClipStretch {
            total: file.seconds() / speed,
            start,
            end: start + length,
            speed,
            reverse,
        })
    }

    /// Moves where an audio clip starts and ends in its sound (in seconds
    /// of the sound as the clip plays it), as trimming its ends on the
    /// timeline does: the sound stays where it is in the song. One step.
    pub fn set_clip_stretch(&mut self, clip: ClipId, start: f64, end: f64) {
        let Some(now) = self.clip_stretch(clip) else {
            return;
        };
        let Some((_, c)) = self.project().clip(clip).map(|(t, c)| (t, *c)) else {
            return;
        };
        let start = start.clamp(0.0, now.total);
        let end = end.clamp(start, now.total);
        if (start - now.start).abs() < 1e-6 && (end - now.end).abs() < 1e-6 {
            return;
        }
        let tempo = self.project().tempo.clone();
        let begins = tempo.seconds_at(c.start) - now.start;
        let tick_at = |seconds: f64| {
            #[allow(clippy::cast_possible_truncation, reason = "tick positions fit easily")]
            let tick = tempo.ticks_at(seconds.max(0.0)).round() as i64;
            Ticks(tick)
        };
        let (new_start, new_end) = (tick_at(begins + start), tick_at(begins + end));
        self.doc.edit(text::EDIT_TRIM_CLIP, |p| {
            p.trim_clip_start(clip, new_start);
            if let Some((_, c)) = p.clip(clip).map(|(t, c)| (t, *c)) {
                p.set_clip_length(clip, (new_end - c.start).max(Ticks(1)));
            }
        });
    }

    /// Makes an audio clip as loud as it can be without clipping: its
    /// volume set so its loudest moment is just under full scale.
    pub fn normalize_clip(&mut self, clip: ClipId) {
        let Some(stretch) = self.clip_stretch(clip) else {
            return;
        };
        let Some(audio) = self.project().clip(clip).and_then(|(_, c)| c.audio()) else {
            return;
        };
        let Some(file) = self.project().audio_file(audio).cloned() else {
            return;
        };
        let SoundState::Ready(peaks) = self.sound_state(&file.file) else {
            return;
        };
        // The stretch in the sound's own seconds, forwards.
        let (from, to) = if stretch.reverse {
            (stretch.total - stretch.end, stretch.total - stretch.start)
        } else {
            (stretch.start, stretch.end)
        };
        let rate = f64::from(peaks.sample_rate());
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "frame positions, clamped at zero"
        )]
        let frame = |seconds: f64| (seconds * stretch.speed * rate).max(0.0) as usize;
        let Some((low, high)) = peaks.range(frame(from), frame(to).max(frame(from) + 1)) else {
            return;
        };
        let loudest = low.abs().max(high.abs());
        if loudest < 1e-5 {
            return;
        }
        let gain = (NORMAL_PEAK / loudest).clamp(0.0, 4.0);
        if self
            .clip_gain(clip)
            .is_some_and(|g| (g - gain).abs() < 1e-4)
        {
            return;
        }
        self.doc.edit(text::EDIT_NORMALIZE_CLIP, |p| {
            p.update_clip(clip, |c| {
                if let ClipContent::Audio { gain: g, .. } = &mut c.content {
                    *g = gain;
                }
            });
        });
    }
}
