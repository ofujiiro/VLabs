// SPDX-License-Identifier: GPL-3.0-or-later

//! The sound editor (decision 0031): a window for one audio clip, opened
//! from its menu or by double-clicking it. It shows the clip's whole sound
//! as the clip plays it (backwards, at its pitch), with the part the clip
//! plays lit: drag its ends to cut it exactly (no snapping to beats), the
//! small squares to fade it in and out. Under the sound: the volume, the
//! pitch, "Backwards" and "As loud as it can be".
//!
//! Every change is an ordinary edit of the clip (one undo step each), so
//! the song view shows it at once and the editor keeps nothing of its own.

use daw_model::ClipId;
use egui::{
    Align2, Button, Color32, CornerRadius, FontId, Id, Rect, RichText, Sense, Shape, Slider,
    Stroke, Ui, pos2, vec2,
};

use crate::session::{ClipStretch, Session, SoundState};
use crate::text;
use crate::theme::{Palette, size};

/// The waveform's size.
const WAVE_WIDTH: f32 = 680.0;
const WAVE_HEIGHT: f32 = 150.0;
/// How close to a handle the pointer must be to take it.
const GRAB: f32 = 8.0;
/// The shortest a clip can be cut to, in seconds.
const SHORTEST: f64 = 0.01;

/// What is being dragged in the waveform.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Handle {
    Start,
    End,
    FadeIn,
    FadeOut,
}

/// Shows the sound editor, if it is open.
pub fn show(ctx: &egui::Context, session: &mut Session) {
    let Some(clip) = session.sound_editor() else {
        return;
    };
    let name = session
        .project()
        .clip(clip)
        .and_then(|(_, c)| c.audio())
        .and_then(|a| session.project().audio_file(a))
        .map_or_else(String::new, |f| f.name.clone());
    let mut open = true;
    egui::Window::new(text::sound_editor_title(&name))
        .id(Id::unique("sound-editor"))
        .open(&mut open)
        .resizable(false)
        .collapsible(false)
        .default_pos(pos2(250.0, 150.0))
        .show(ctx, |ui| contents(ui, session, clip));
    if !open {
        session.close_sound_editor();
    }
}

/// The window's contents.
fn contents(ui: &mut Ui, session: &mut Session, clip: ClipId) {
    let Some(stretch) = session.clip_stretch(clip) else {
        return;
    };
    ui.set_width(WAVE_WIDTH);
    waveform(ui, session, clip, stretch);
    ui.add_space(4.0);
    ui.label(
        RichText::new(text::clip_span(stretch.start, stretch.end, stretch.total))
            .small()
            .color(Palette::of(ui.ctx()).text_weak),
    );
    ui.add_space(8.0);
    controls(ui, session, clip);
}

/// The volume, the pitch, backwards, and as loud as it can be.
fn controls(ui: &mut Ui, session: &mut Session, clip: ClipId) {
    let (pitch, reverse) = session.clip_motion(clip).unwrap_or((0.0, false));
    let gain = session.clip_gain(clip).unwrap_or(1.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        let shown = crate::widgets::gain_to_db(gain);
        let mut db = shown;
        let volume = ui
            .add(
                Slider::new(&mut db, -60.0..=12.0)
                    .suffix(" dB")
                    .fixed_decimals(1)
                    .text(text::CLIP_VOLUME),
            )
            .on_hover_text(text::CLIP_VOLUME_HINT);
        if volume.drag_started() {
            session.begin_gesture(text::EDIT_CLIP_VOLUME);
        }
        // (The slider shows tenths of a decibel: a volume set elsewhere,
        // with more decimals, is not changed by being shown.)
        if volume.changed() && (db - shown).abs() > 0.04 {
            session.set_clip_gain(clip, crate::widgets::db_to_gain(db));
        }
        if volume.drag_stopped() {
            session.end_gesture();
        }

        let mut semitones = pitch;
        let range = daw_model::CLIP_PITCH_RANGE;
        let slider = ui
            .add(
                Slider::new(&mut semitones, range.0..=range.1)
                    .step_by(1.0)
                    .custom_formatter(|v, _| {
                        #[allow(clippy::cast_possible_truncation, reason = "semitones")]
                        text::semitones(v as f32)
                    })
                    .text(text::CLIP_PITCH),
            )
            .on_hover_text(text::CLIP_PITCH_HINT);
        if slider.drag_started() {
            session.begin_gesture(text::EDIT_CLIP_PITCH);
        }
        if slider.changed() {
            session.set_clip_pitch(clip, semitones);
        }
        if slider.drag_stopped() {
            session.end_gesture();
        }
    });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        if ui
            .add(Button::new(text::CLIP_REVERSE).selected(reverse))
            .on_hover_text(text::CLIP_REVERSE_HINT)
            .clicked()
        {
            session.set_clip_reverse(clip, !reverse);
        }
        if ui
            .button(text::CLIP_NORMALIZE)
            .on_hover_text(text::CLIP_NORMALIZE_HINT)
            .clicked()
        {
            session.normalize_clip(clip);
        }
    });
}

/// The clip's whole sound as it plays it, its part lit, with handles for
/// its ends and fades.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    reason = "screen positions, seconds and frames (clamped at zero); one drawing with its handles"
)]
fn waveform(ui: &mut Ui, session: &mut Session, clip: ClipId, stretch: ClipStretch) {
    let p = Palette::of(ui.ctx());
    let (rect, response) =
        ui.allocate_exact_size(vec2(WAVE_WIDTH, WAVE_HEIGHT), Sense::click_and_drag());
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, text::SOUND_WAVEFORM));
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(4), p.raised);
    let total = stretch.total.max(1e-6);
    let x_of = |u: f64| rect.left() + (u / total) as f32 * rect.width();
    let u_of = |x: f32| (f64::from((x - rect.left()) / rect.width()) * total).clamp(0.0, total);
    let (fade_in, fade_out) = session.clip_fades(clip).unwrap_or_default();
    let gain = session.clip_gain(clip).unwrap_or(1.0);
    let file = session
        .project()
        .clip(clip)
        .and_then(|(_, c)| c.audio())
        .and_then(|a| session.project().audio_file(a))
        .cloned();

    // The sound.
    let (start_x, end_x) = (x_of(stretch.start), x_of(stretch.end));
    match file.map(|f| session.sound_state(&f.file)) {
        Some(SoundState::Ready(peaks)) => {
            let rate = f64::from(peaks.sample_rate());
            // A moment of the sound as played, as a frame of the sound.
            let frame = |u: f64| {
                let seconds = if stretch.reverse { total - u } else { u };
                (seconds * stretch.speed * rate).max(0.0) as usize
            };
            let (mid, half) = (rect.center().y, rect.height() / 2.0 - 6.0);
            let mut x = rect.left();
            while x < rect.right() {
                let (a, b) = (frame(u_of(x)), frame(u_of(x + 1.0)));
                let (from, to) = (a.min(b), a.max(b).max(a.min(b) + 1));
                if let Some((low, high)) = peaks.range(from, to) {
                    let inside = x + 0.5 >= start_x && x + 0.5 <= end_x;
                    let level = if inside { gain } else { 1.0 };
                    let top = mid - (high * level).clamp(-1.0, 1.0) * half;
                    let bottom = (mid - (low * level).clamp(-1.0, 1.0) * half).max(top + 1.0);
                    let color = if inside {
                        p.accent
                    } else {
                        p.text_weak.gamma_multiply(0.5)
                    };
                    painter.line_segment(
                        [pos2(x + 0.5, top), pos2(x + 0.5, bottom)],
                        Stroke::new(1.0, color),
                    );
                }
                x += 1.0;
            }
        }
        _ => {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                text::SOUND_NOT_READY,
                FontId::proportional(size::BODY),
                p.text_weak,
            );
        }
    }
    // What the clip leaves out, shaded.
    let shade = Color32::from_black_alpha(70);
    painter.rect_filled(
        Rect::from_min_max(rect.left_top(), pos2(start_x, rect.bottom())),
        CornerRadius::ZERO,
        shade,
    );
    painter.rect_filled(
        Rect::from_min_max(pos2(end_x, rect.top()), rect.right_bottom()),
        CornerRadius::ZERO,
        shade,
    );
    // The fades, as curves from the clip's top.
    let length = stretch.end - stretch.start;
    let fade_in_x = x_of(stretch.start + f64::from(fade_in).min(length));
    let fade_out_x = x_of(stretch.end - f64::from(fade_out).min(length));
    let top = rect.top() + 6.0;
    let bottom = rect.bottom() - 4.0;
    for (from, to, rising) in [(start_x, fade_in_x, true), (fade_out_x, end_x, false)] {
        if to - from < 1.0 {
            continue;
        }
        let points: Vec<egui::Pos2> = (0..=24)
            .map(|i| {
                let t = i as f32 / 24.0;
                let level = if rising {
                    (std::f32::consts::FRAC_PI_2 * t).sin()
                } else {
                    (std::f32::consts::FRAC_PI_2 * (1.0 - t)).sin()
                };
                pos2(from + (to - from) * t, bottom - (bottom - top) * level)
            })
            .collect();
        painter.add(Shape::line(points, Stroke::new(1.5, p.text)));
    }
    // The handles: the ends as bars, the fades as squares.
    let handles = [
        (
            Handle::Start,
            pos2(start_x, rect.center().y),
            text::SOUND_START_HANDLE,
        ),
        (
            Handle::End,
            pos2(end_x, rect.center().y),
            text::SOUND_END_HANDLE,
        ),
        (Handle::FadeIn, pos2(fade_in_x, top), text::CLIP_FADE_IN),
        (Handle::FadeOut, pos2(fade_out_x, top), text::CLIP_FADE_OUT),
    ];
    let drag_id = Id::unique("sound-editor-drag");
    let dragging: Option<Handle> = ui.data(|d| d.get_temp(drag_id));
    let mut taken = dragging;
    for (handle, at, label) in handles {
        let area = match handle {
            Handle::Start | Handle::End => {
                Rect::from_center_size(at, vec2(GRAB * 2.0, rect.height()))
            }
            Handle::FadeIn | Handle::FadeOut => {
                Rect::from_center_size(at, vec2(GRAB * 2.0, GRAB * 2.0))
            }
        };
        let id = Id::unique(("sound-editor-handle", label));
        let handle_response = ui.interact(area, id, Sense::drag());
        handle_response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Slider, true, label));
        let hot = handle_response.hovered() || handle_response.dragged();
        if handle_response.hovered() || handle_response.dragged() {
            ui.ctx().set_cursor_icon(match handle {
                Handle::Start | Handle::End => egui::CursorIcon::ResizeHorizontal,
                Handle::FadeIn | Handle::FadeOut => egui::CursorIcon::PointingHand,
            });
        }
        let color = if hot { p.selected } else { p.text };
        match handle {
            Handle::Start | Handle::End => {
                painter.line_segment(
                    [pos2(at.x, rect.top()), pos2(at.x, rect.bottom())],
                    Stroke::new(if hot { 3.0 } else { 2.0 }, color),
                );
            }
            Handle::FadeIn | Handle::FadeOut => {
                let side = if hot { 5.0 } else { 4.0 };
                painter.rect_filled(
                    Rect::from_center_size(at, vec2(side * 2.0, side * 2.0)),
                    CornerRadius::same(2),
                    color,
                );
            }
        }
        if handle_response.drag_started() && taken.is_none() {
            taken = Some(handle);
            session.begin_gesture(match handle {
                Handle::Start | Handle::End => text::EDIT_TRIM_CLIP,
                Handle::FadeIn | Handle::FadeOut => text::EDIT_FADE,
            });
        }
        if taken == Some(handle) && handle_response.dragged() {
            let Some(grabbed) = handle_response.interact_pointer_pos() else {
                continue;
            };
            let u = u_of(grabbed.x);
            match handle {
                Handle::Start => {
                    session.set_clip_stretch(clip, u.min(stretch.end - SHORTEST), stretch.end);
                }
                Handle::End => {
                    session.set_clip_stretch(clip, stretch.start, u.max(stretch.start + SHORTEST));
                }
                Handle::FadeIn => {
                    let seconds = (u - stretch.start).max(0.0) as f32;
                    session.set_clip_fades(clip, seconds, fade_out);
                }
                Handle::FadeOut => {
                    let seconds = (stretch.end - u).max(0.0) as f32;
                    session.set_clip_fades(clip, fade_in, seconds);
                }
            }
        }
        if taken == Some(handle) && handle_response.drag_stopped() {
            taken = None;
            session.end_gesture();
        }
    }
    ui.data_mut(|d| match taken {
        Some(handle) => {
            d.insert_temp(drag_id, handle);
        }
        None => d.remove::<Handle>(drag_id),
    });
}
