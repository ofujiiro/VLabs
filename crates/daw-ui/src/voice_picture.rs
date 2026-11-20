// SPDX-License-Identifier: GPL-3.0-or-later

//! A voicebank's picture, drawn faintly where its voice is (decision 0029):
//! behind the piano roll's notes, on its track's header and at the start of
//! its clips in the song view.

use egui::{Align2, Color32, Id, Rect, Sense, Ui, pos2, vec2};

use crate::session::VoicePicture;

/// How much larger than its own size a picture may be drawn: a small icon
/// would otherwise become a blur filling the screen.
const MAX_SCALE: f32 = 2.5;

/// How visible the picture is behind the piano roll's notes.
pub const BEHIND_NOTES: f32 = 0.14;
/// How visible it is on a track's header and its clips.
pub const ON_TRACK: f32 = 0.3;

/// Where a picture of `size` is drawn in `area`: as tall as `height` at
/// most, never wider than the area nor larger than [`MAX_SCALE`] times its
/// own size, placed by `align`. `None` when there is no room.
pub fn placed(size: egui::Vec2, area: Rect, height: f32, align: Align2) -> Option<Rect> {
    let size = size.max(vec2(1.0, 1.0));
    let scale = (height / size.y).min(area.width() / size.x).min(MAX_SCALE);
    (scale > 0.0 && area.is_positive()).then(|| align.align_size_within_rect(size * scale, area))
}

/// Draws `picture` with `opacity` where [`placed`] puts it, only inside
/// `clip`, and names it `label` for screen readers.
#[allow(
    clippy::too_many_arguments,
    reason = "where, how large, how faint, and its name"
)]
pub fn paint_faint(
    ui: &Ui,
    picture: VoicePicture,
    area: Rect,
    height: f32,
    align: Align2,
    opacity: f32,
    clip: Rect,
    label: &str,
    id: Id,
) {
    let Some(rect) = placed(picture.size, area, height, align) else {
        return;
    };
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to 0..=255"
    )]
    let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    ui.painter_at(clip).image(
        picture.texture,
        rect,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        Color32::from_white_alpha(alpha),
    );
    let shown = rect.intersect(clip);
    if shown.is_positive() {
        ui.interact(shown, id, Sense::hover())
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, label));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pictures_fit_their_place_and_small_ones_are_not_blown_up() {
        let area = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 400.0));
        // A large portrait fills the height asked for, at the bottom right.
        let rect = placed(vec2(1000.0, 1000.0), area, 340.0, Align2::RIGHT_BOTTOM).unwrap();
        assert_eq!(rect.size(), vec2(340.0, 340.0));
        assert_eq!(rect.right_bottom(), area.right_bottom());
        // A 100-point icon grows two and a half times at most.
        let rect = placed(vec2(100.0, 100.0), area, 340.0, Align2::RIGHT_BOTTOM).unwrap();
        assert_eq!(rect.size(), vec2(250.0, 250.0));
        // A wide picture in a narrow place keeps within its width.
        let narrow = Rect::from_min_size(pos2(10.0, 10.0), vec2(50.0, 56.0));
        let rect = placed(vec2(300.0, 100.0), narrow, 56.0, Align2::LEFT_CENTER).unwrap();
        assert!((rect.width() - 50.0).abs() < 1e-4 && (rect.left() - 10.0).abs() < 1e-4);
        assert!(placed(vec2(100.0, 100.0), Rect::NOTHING, 50.0, Align2::LEFT_TOP).is_none());
    }
}
