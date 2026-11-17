// SPDX-License-Identifier: GPL-3.0-or-later

//! The song view (decision 0014): the tracks down the left with their
//! names, mute and solo; their clips on a timeline to the right, each with
//! a small picture of its notes.
//!
//! - click an empty spot on a track: its pattern is placed there (keep the
//!   button down to drag it into place);
//! - drag a clip to move it; drag its right end to repeat its pattern;
//! - double-click a clip to open its pattern; right-click it for more;
//! - click the ruler to move the play position; drag along it to loop a
//!   part of the song;
//! - the wheel scrolls, Shift+wheel scrolls in time, Ctrl+wheel zooms; hold
//!   Alt while dragging to move by beats instead of bars.

use daw_dsp::drums::PadSound;
use daw_model::{Clip, ClipId, Instrument, Pattern, Region, Ticks, TrackId};
use egui::{
    Align, Align2, Button, CornerRadius, CursorIcon, FontId, Layout as EguiLayout, PointerButton,
    Pos2, Rect, Response, RichText, Sense, Shape, Stroke, StrokeKind, Ui, UiBuilder, pos2, vec2,
};

use crate::icons::{Icon, Look, icon_button};
use crate::session::Session;
use crate::step_grid::PAD_ORDER;
use crate::text;
use crate::theme::{Palette, size, track_color};
use crate::workspace::Intent;

const HEADER_WIDTH: f32 = 236.0;
const RULER_HEIGHT: f32 = 28.0;
const ROW_HEIGHT: f32 = 64.0;
const ADD_ROW_HEIGHT: f32 = 48.0;
/// How close to a clip's right end the pointer must be to resize it.
const EDGE_GRAB: f32 = 8.0;
const DRAG_THRESHOLD: f32 = 4.0;

/// What a mouse drag is doing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Drag {
    #[default]
    None,
    /// Moving a clip; `grab` is where in the clip it was taken.
    Move {
        clip: ClipId,
        grab: Ticks,
        origin: Pos2,
        moved: bool,
    },
    /// Changing a clip's length.
    Resize { clip: ClipId },
    /// Along the ruler: a click moves the play position, a drag loops.
    Ruler {
        anchor: Ticks,
        origin: Pos2,
        looping: bool,
    },
    /// Moving the view.
    Pan { last: Pos2 },
}

/// The song view's state between frames.
#[derive(Debug)]
pub struct SongView {
    /// Tick at the left edge of the timeline.
    left_tick: f64,
    /// Horizontal zoom.
    ticks_per_point: f64,
    /// How far the tracks are scrolled up, in points.
    top: f32,
    drag: Drag,
    fitted: bool,
    /// The clip a right-click menu is open for.
    menu_clip: Option<ClipId>,
    menu_at: Pos2,
    scrolled_at: f64,
}

impl Default for SongView {
    fn default() -> Self {
        Self {
            left_tick: 0.0,
            #[allow(clippy::cast_precision_loss, reason = "a constant")]
            ticks_per_point: Ticks::QUARTER.0 as f64 / 24.0,
            top: 0.0,
            drag: Drag::None,
            fitted: false,
            menu_clip: None,
            menu_at: Pos2::ZERO,
            scrolled_at: f64::NEG_INFINITY,
        }
    }
}

/// The parts of the view, worked out for each frame.
#[derive(Clone, Copy, Debug)]
struct Layout {
    whole: Rect,
    headers: Rect,
    ruler: Rect,
    timeline: Rect,
    top: f32,
}

impl Layout {
    fn new(whole: Rect, top: f32) -> Self {
        let left = whole.left() + HEADER_WIDTH;
        let below_ruler = whole.top() + RULER_HEIGHT;
        Self {
            whole,
            headers: Rect::from_min_max(
                pos2(whole.left(), below_ruler),
                pos2(left, whole.bottom()),
            ),
            ruler: Rect::from_min_max(pos2(left, whole.top()), pos2(whole.right(), below_ruler)),
            timeline: Rect::from_min_max(pos2(left, below_ruler), whole.max),
            top,
        }
    }

    #[allow(clippy::cast_precision_loss, reason = "small counts")]
    fn row_top(&self, row: usize) -> f32 {
        self.timeline.top() + row as f32 * ROW_HEIGHT - self.top
    }

    /// The track row under a y position.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "checked before converting"
    )]
    fn row_at(&self, y: f32, rows: usize) -> Option<usize> {
        let at = ((y - self.timeline.top() + self.top) / ROW_HEIGHT).floor();
        (at >= 0.0 && (at as usize) < rows).then_some(at as usize)
    }
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "screen coordinates, clamped before converting"
)]
impl SongView {
    /// Shows the song in all the space `ui` has.
    pub fn ui(&mut self, ui: &mut Ui, session: &mut Session, intents: &mut Vec<Intent>) {
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Canvas, true, text::SONG_VIEW));
        let rows = session.project().tracks.len();
        let content = rows as f32 * ROW_HEIGHT + ADD_ROW_HEIGHT;
        let mut layout = Layout::new(rect, self.top);
        if !self.fitted && layout.timeline.width() > 50.0 {
            self.fit(session, &layout);
            self.fitted = true;
        }
        self.handle_view_input(ui, &layout);
        let most = (content - layout.timeline.height() + 8.0).max(0.0);
        self.top = self.top.clamp(0.0, most);
        self.left_tick = self.left_tick.max(0.0);
        layout.top = self.top;
        self.handle_pointer(ui, &response, &layout, session);
        self.follow_playhead(ui, session, &layout);

        let p = Palette::of(ui.ctx());
        ui.painter_at(rect)
            .rect_filled(rect, CornerRadius::ZERO, p.grid);
        self.paint_rows(ui, session, &layout);
        self.paint_clips(ui, session, &layout);
        self.paint_ruler(ui, session, &layout);
        Self::headers(ui, session, &layout, intents);
        self.clip_menu(ui, session);
    }

    /// Whether a drag is in progress.
    pub fn is_dragging(&self) -> bool {
        self.drag != Drag::None
    }

    /// Abandons the drag in progress, putting everything back.
    pub fn cancel_drag(&mut self, session: &mut Session) -> bool {
        if self.drag == Drag::None {
            return false;
        }
        if !matches!(self.drag, Drag::Pan { .. }) {
            session.cancel_gesture();
        }
        self.drag = Drag::None;
        true
    }

    /// Zooms in time around the middle (a factor above 1 zooms in).
    pub fn zoom(&mut self, factor: f64) {
        self.ticks_per_point = (self.ticks_per_point / factor).clamp(min_zoom(), max_zoom());
    }

    /// Shows the whole song on the next frame.
    pub fn refit(&mut self) {
        self.fitted = false;
    }

    fn fit(&mut self, session: &Session, layout: &Layout) {
        let length = session.song_length() + session.project().meter.bar_start(2);
        self.left_tick = 0.0;
        self.ticks_per_point = (length.0 as f64
            / f64::from(layout.timeline.width() - 16.0).max(1.0))
        .clamp(min_zoom(), max_zoom());
        self.top = 0.0;
    }

    // ----- Coordinates

    fn tick_to_x(&self, layout: &Layout, tick: Ticks) -> f32 {
        layout.timeline.left() + ((tick.0 as f64 - self.left_tick) / self.ticks_per_point) as f32
    }

    fn x_to_tick(&self, layout: &Layout, x: f32) -> Ticks {
        Ticks(
            (self.left_tick + f64::from(x - layout.timeline.left()) * self.ticks_per_point) as i64,
        )
    }

    /// A tick snapped down to the bar (or beat, with Alt) it falls in.
    fn snap(session: &Session, at: Ticks, fine: bool) -> Ticks {
        let meter = &session.project().meter;
        let unit = if fine {
            meter.signature_at(at).beat_length()
        } else {
            meter.signature_at(at).bar_length()
        };
        at.max(Ticks::ZERO).floor_to(unit)
    }

    fn snap_nearest(session: &Session, at: Ticks, fine: bool) -> Ticks {
        let meter = &session.project().meter;
        let unit = if fine {
            meter.signature_at(at).beat_length()
        } else {
            meter.signature_at(at).bar_length()
        };
        at.max(Ticks::ZERO).round_to(unit)
    }

    fn clip_rect(&self, layout: &Layout, row: usize, clip: &Clip) -> Rect {
        let top = layout.row_top(row);
        Rect::from_min_max(
            pos2(self.tick_to_x(layout, clip.start), top + 4.0),
            pos2(self.tick_to_x(layout, clip.end()), top + ROW_HEIGHT - 4.0),
        )
    }

    /// The clip under a point: its id and row.
    fn clip_at(
        &self,
        layout: &Layout,
        session: &Session,
        pos: Pos2,
    ) -> Option<(ClipId, usize, Rect)> {
        let rows = session.project().tracks.len();
        let row = layout.row_at(pos.y, rows)?;
        let track = &session.project().tracks[row];
        track.clips.iter().rev().find_map(|clip| {
            let rect = self.clip_rect(layout, row, clip);
            rect.contains(pos).then_some((clip.id, row, rect))
        })
    }

    // ----- Input

    fn handle_view_input(&mut self, ui: &Ui, layout: &Layout) {
        let Some(hover) = ui.input(|i| i.pointer.hover_pos()) else {
            return;
        };
        if !layout.whole.contains(hover) {
            return;
        }
        let (scroll, zoom, time) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.time));
        if (zoom - 1.0).abs() > f32::EPSILON {
            let x = hover.x.max(layout.timeline.left());
            let tick = self.x_to_tick(layout, x);
            self.ticks_per_point =
                (self.ticks_per_point / f64::from(zoom)).clamp(min_zoom(), max_zoom());
            self.left_tick =
                tick.0 as f64 - f64::from(x - layout.timeline.left()) * self.ticks_per_point;
            self.scrolled_at = time;
        }
        if scroll != egui::Vec2::ZERO {
            self.left_tick -= f64::from(scroll.x) * self.ticks_per_point;
            self.top -= scroll.y;
            self.scrolled_at = time;
        }
    }

    #[allow(
        clippy::too_many_lines,
        reason = "one place that decides what a press does"
    )]
    fn handle_pointer(
        &mut self,
        ui: &Ui,
        response: &Response,
        layout: &Layout,
        session: &mut Session,
    ) {
        let (pos, primary_pressed, secondary_pressed, middle_pressed, any_down, alt, time) = ui
            .input(|i| {
                (
                    i.pointer.interact_pos(),
                    i.pointer.button_pressed(PointerButton::Primary),
                    i.pointer.button_pressed(PointerButton::Secondary),
                    i.pointer.button_pressed(PointerButton::Middle),
                    i.pointer.any_down(),
                    i.modifiers.alt,
                    i.time,
                )
            });
        let Some(pos) = pos else {
            return;
        };
        let inside = layout.whole.contains(pos) && !layout.headers.contains(pos);
        if response.double_clicked()
            && let Some((clip, _, _)) = self.clip_at(layout, session, pos)
            && let Some((track, c)) = session.project().clip(clip)
            && let Some(pattern) = c.pattern()
        {
            session.cancel_gesture();
            self.drag = Drag::None;
            session.open_pattern(track, pattern);
            return;
        }
        if primary_pressed
            && inside
            && self.drag == Drag::None
            && response.is_pointer_button_down_on()
        {
            if layout.ruler.contains(pos) {
                let anchor = Self::snap_nearest(session, self.x_to_tick(layout, pos.x), true);
                self.drag = Drag::Ruler {
                    anchor,
                    origin: pos,
                    looping: false,
                };
            } else if let Some((clip, _, rect)) = self.clip_at(layout, session, pos) {
                session.select_clip(Some(clip));
                if pos.x >= rect.right() - EDGE_GRAB {
                    session.begin_gesture(text::EDIT_RESIZE_CLIP);
                    self.drag = Drag::Resize { clip };
                } else if let Some(start) = session.project().clip(clip).map(|(_, c)| c.start) {
                    session.begin_gesture(text::EDIT_MOVE_CLIP);
                    self.drag = Drag::Move {
                        clip,
                        grab: self.x_to_tick(layout, pos.x) - start,
                        origin: pos,
                        moved: false,
                    };
                }
            } else if let Some(row) = layout.row_at(pos.y, session.project().tracks.len()) {
                let track = session.project().tracks[row].id;
                let at = Self::snap(session, self.x_to_tick(layout, pos.x), alt);
                session.begin_gesture(text::EDIT_PLACE_PATTERN);
                if let Some(clip) = session.place_pattern(track, at) {
                    self.drag = Drag::Move {
                        clip,
                        grab: self.x_to_tick(layout, pos.x) - at,
                        origin: pos,
                        moved: false,
                    };
                } else {
                    session.end_gesture();
                    session.focus_track(track);
                }
            } else {
                session.select_clip(None);
            }
        }
        if secondary_pressed && inside {
            if let Some((clip, _, _)) = self.clip_at(layout, session, pos) {
                session.select_clip(Some(clip));
                self.menu_clip = Some(clip);
                self.menu_at = pos;
            } else {
                self.menu_clip = None;
            }
        }
        if middle_pressed && inside {
            self.drag = Drag::Pan { last: pos };
        }
        match self.drag {
            Drag::Move {
                clip,
                grab,
                origin,
                moved,
            } if any_down => {
                let moved = moved || pos.distance(origin) > DRAG_THRESHOLD;
                if moved {
                    let start =
                        Self::snap_nearest(session, self.x_to_tick(layout, pos.x) - grab, alt);
                    if !overlaps(session, clip, start, None) {
                        session.move_clip(clip, start);
                    }
                }
                self.drag = Drag::Move {
                    clip,
                    grab,
                    origin,
                    moved,
                };
            }
            Drag::Resize { clip } if any_down => {
                if let Some((_, c)) = session.project().clip(clip) {
                    let start = c.start;
                    let end = Self::snap_nearest(session, self.x_to_tick(layout, pos.x), alt);
                    let beat = session.project().meter.signature_at(start).beat_length();
                    let length = (end - start).max(beat);
                    if !overlaps(session, clip, start, Some(length)) {
                        session.resize_clip(clip, length);
                    }
                }
            }
            Drag::Ruler {
                anchor,
                origin,
                looping,
            } if any_down => {
                let looping = looping || pos.distance(origin) > DRAG_THRESHOLD;
                if looping {
                    let here = Self::snap_nearest(session, self.x_to_tick(layout, pos.x), alt);
                    let (start, end) = (anchor.min(here), anchor.max(here));
                    if !matches!(self.drag, Drag::Ruler { looping: true, .. }) {
                        session.begin_gesture(text::EDIT_LOOP);
                    }
                    if let Some(region) = Region::new(start, end) {
                        session.set_loop_region(Some(region));
                    }
                }
                self.drag = Drag::Ruler {
                    anchor,
                    origin,
                    looping,
                };
            }
            Drag::Pan { last } if any_down => {
                let delta = pos - last;
                self.left_tick -= f64::from(delta.x) * self.ticks_per_point;
                self.top -= delta.y;
                self.scrolled_at = time;
                self.drag = Drag::Pan { last: pos };
            }
            Drag::None => {}
            finished => {
                match finished {
                    Drag::Ruler {
                        anchor,
                        looping: false,
                        ..
                    } => session.locate_song(anchor),
                    Drag::Pan { .. } => {}
                    _ => session.end_gesture(),
                }
                self.drag = Drag::None;
            }
        }
        // The pointer's shape says what a press would do.
        if inside && self.drag == Drag::None && ui.rect_contains_pointer(layout.whole) {
            let icon = match self.clip_at(layout, session, pos) {
                Some((_, _, rect)) if pos.x >= rect.right() - EDGE_GRAB => {
                    CursorIcon::ResizeHorizontal
                }
                Some(_) => CursorIcon::Grab,
                None if layout.ruler.contains(pos) => CursorIcon::PointingHand,
                None => CursorIcon::Default,
            };
            ui.ctx().set_cursor_icon(icon);
        } else if matches!(self.drag, Drag::Move { moved: true, .. }) {
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        } else if matches!(self.drag, Drag::Resize { .. }) {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
        }
    }

    fn follow_playhead(&mut self, ui: &Ui, session: &Session, layout: &Layout) {
        let time = ui.input(|i| i.time);
        if !session.is_playing() || self.is_dragging() || time - self.scrolled_at < 1.5 {
            return;
        }
        let x = self.tick_to_x(layout, session.song_position());
        if x > layout.timeline.right() - 24.0 || x < layout.timeline.left() {
            let width = f64::from(layout.timeline.width()) * self.ticks_per_point;
            self.left_tick = (session.song_position().0 as f64 - width * 0.05).max(0.0);
        }
    }

    // ----- Drawing

    fn paint_rows(&self, ui: &Ui, session: &Session, layout: &Layout) {
        let p = Palette::of(ui.ctx());
        let painter = ui.painter_at(layout.timeline);
        let tracks = &session.project().tracks;
        for (row, track) in tracks.iter().enumerate() {
            let top = layout.row_top(row);
            let band = Rect::from_min_max(
                pos2(layout.timeline.left(), top),
                pos2(layout.timeline.right(), top + ROW_HEIGHT),
            );
            if track.id == session.track_id() {
                painter.rect_filled(band, CornerRadius::ZERO, p.grid_dark_row);
            }
            painter.hline(
                band.x_range(),
                band.bottom() - 0.5,
                Stroke::new(1.0, p.grid_line),
            );
        }
        // Bar lines.
        let meter = &session.project().meter;
        let bar_points = meter.initial().bar_length().0 as f64 / self.ticks_per_point;
        let every = ((24.0 / bar_points).ceil() as u32)
            .max(1)
            .next_power_of_two();
        let first = meter.position(Ticks(self.left_tick.max(0.0) as i64)).bar;
        let right = self.left_tick + f64::from(layout.timeline.width()) * self.ticks_per_point;
        let mut bar = first;
        loop {
            let start = meter.bar_start(bar);
            if start.0 as f64 > right {
                break;
            }
            let x = self.tick_to_x(layout, start);
            let stroke = if bar.is_multiple_of(every * 4) {
                Stroke::new(1.0, p.grid_bar)
            } else if bar.is_multiple_of(every) {
                Stroke::new(1.0, p.grid_beat)
            } else {
                Stroke::NONE
            };
            if stroke != Stroke::NONE {
                painter.vline(x, layout.timeline.y_range(), stroke);
            }
            bar += 1;
        }
        // The end of the song.
        let end = self.tick_to_x(layout, session.project().song_end());
        painter.vline(
            end,
            layout.timeline.y_range(),
            Stroke::new(1.0, p.grid_bar.gamma_multiply(0.6)),
        );
        // Hint along the bottom.
        let rows_bottom = layout.row_top(tracks.len());
        if rows_bottom + 40.0 < layout.timeline.bottom() {
            painter.text(
                pos2(layout.timeline.center().x, layout.timeline.bottom() - 18.0),
                Align2::CENTER_CENTER,
                text::SONG_HINT,
                FontId::proportional(size::SMALL),
                p.text_weak.gamma_multiply(0.8),
            );
        }
    }

    fn paint_clips(&self, ui: &Ui, session: &Session, layout: &Layout) {
        let p = Palette::of(ui.ctx());
        let painter = ui.painter_at(layout.timeline);
        let project = session.project();
        for (row, track) in project.tracks.iter().enumerate() {
            let color = track_color(row);
            let drums = track.instrument.is_drums();
            for clip in &track.clips {
                let rect = self.clip_rect(layout, row, clip);
                if rect.right() < layout.timeline.left() || rect.left() > layout.timeline.right() {
                    continue;
                }
                let pattern = clip.pattern().and_then(|id| project.pattern(id));
                let selected = session.selected_clip() == Some(clip.id);
                let muted = track.strip.mute;
                let fill = color.gamma_multiply(if muted { 0.12 } else { 0.28 });
                painter.rect_filled(rect, CornerRadius::same(6), fill);
                if let Some(pattern) = pattern {
                    self.paint_preview(&painter, layout, rect, clip, pattern, drums, color);
                    painter.text(
                        rect.left_top() + vec2(7.0, 4.0),
                        Align2::LEFT_TOP,
                        &pattern.name,
                        FontId::proportional(size::SMALL),
                        p.text,
                    );
                }
                let stroke = if selected {
                    Stroke::new(2.0, p.selected)
                } else {
                    Stroke::new(1.0, color.gamma_multiply(0.9))
                };
                painter.rect_stroke(rect, CornerRadius::same(6), stroke, StrokeKind::Inside);
            }
        }
        // The play position.
        let x = self.tick_to_x(layout, session.song_position());
        if x >= layout.timeline.left() && x <= layout.timeline.right() {
            let color = if session.is_recording() {
                p.record
            } else {
                p.accent
            };
            painter.vline(x, layout.timeline.y_range(), Stroke::new(1.5, color));
        }
    }

    /// Draws the pattern's notes inside a clip, once per repeat.
    #[allow(clippy::too_many_arguments, reason = "the parts of one drawing")]
    fn paint_preview(
        &self,
        painter: &egui::Painter,
        layout: &Layout,
        rect: Rect,
        clip: &Clip,
        pattern: &Pattern,
        drums: bool,
        color: egui::Color32,
    ) {
        let length = pattern.length.max(Ticks(1));
        let area = Rect::from_min_max(
            pos2(rect.left(), rect.top() + 22.0),
            pos2(rect.right(), rect.bottom() - 7.0),
        );
        if area.height() < 4.0 {
            return;
        }
        let offset = match clip.content {
            daw_model::ClipContent::Pattern { offset, .. } => offset,
        };
        // Drums: one line per sound the pattern uses, top to bottom in the
        // step grid's order. Other instruments: the notes' pitch range.
        let mut used: Vec<usize> = pattern
            .notes
            .iter()
            .filter_map(|n| PadSound::from_key(n.key))
            .filter_map(|sound| PAD_ORDER.iter().position(|&s| s == sound))
            .collect();
        used.sort_unstable();
        used.dedup();
        let (low, high) = if drums {
            (0.0, used.len().saturating_sub(1).max(1) as f32)
        } else {
            let low = pattern.notes.iter().map(|n| n.key).min().unwrap_or(60);
            let high = pattern.notes.iter().map(|n| n.key).max().unwrap_or(72);
            (f32::from(low) - 1.0, f32::from(high) + 1.0)
        };
        let span = (high - low).max(1.0);
        let row_of = |key: u8| -> Option<f32> {
            if drums {
                let sound = PadSound::from_key(key)?;
                let row = PAD_ORDER.iter().position(|&s| s == sound)?;
                used.iter().position(|&r| r == row).map(|r| r as f32)
            } else {
                Some(f32::from(key))
            }
        };
        let mark = color.gamma_multiply(0.95);
        let visible = layout.timeline.x_range();
        let mut repeat_start = clip.start - offset;
        let mut first = true;
        while repeat_start < clip.end() {
            if !first {
                let x = self.tick_to_x(layout, repeat_start);
                if x > rect.left() + 2.0 {
                    painter.vline(
                        x,
                        area.y_range(),
                        Stroke::new(1.0, color.gamma_multiply(0.35)),
                    );
                }
            }
            first = false;
            let left = self.tick_to_x(layout, repeat_start);
            let right = self.tick_to_x(layout, repeat_start + length);
            if right >= visible.min && left <= visible.max {
                for note in &pattern.notes {
                    let start = repeat_start + note.start;
                    if start < clip.start || start >= clip.end() {
                        continue;
                    }
                    let Some(row) = row_of(note.key) else {
                        continue;
                    };
                    let y = if drums {
                        area.top() + (row / span) * (area.height() - 3.0)
                    } else {
                        area.bottom() - 3.0 - ((row - low) / span) * (area.height() - 3.0)
                    };
                    let x0 = self.tick_to_x(layout, start);
                    let end = if drums {
                        start
                    } else {
                        (start + note.length).min(clip.end())
                    };
                    let x1 = self.tick_to_x(layout, end).max(x0 + 2.5);
                    painter.rect_filled(
                        Rect::from_min_max(pos2(x0, y), pos2(x1, y + 2.5)),
                        CornerRadius::same(1),
                        mark,
                    );
                }
            }
            repeat_start += length;
        }
    }

    fn paint_ruler(&self, ui: &Ui, session: &Session, layout: &Layout) {
        let p = Palette::of(ui.ctx());
        let ruler = layout.ruler;
        let painter = ui.painter_at(ruler);
        painter.rect_filled(ruler, CornerRadius::ZERO, p.surface);
        if let Some(region) = session.project().loop_region {
            let x0 = self.tick_to_x(layout, region.start);
            let x1 = self.tick_to_x(layout, region.end);
            let band =
                Rect::from_min_max(pos2(x0, ruler.top() + 3.0), pos2(x1, ruler.bottom() - 3.0));
            painter.rect_filled(band, CornerRadius::same(4), p.loop_region);
            painter.rect_stroke(
                band,
                CornerRadius::same(4),
                Stroke::new(1.0, p.accent.gamma_multiply(0.7)),
                StrokeKind::Inside,
            );
        }
        let meter = &session.project().meter;
        let bar_points = meter.initial().bar_length().0 as f64 / self.ticks_per_point;
        let every = ((40.0 / bar_points).ceil() as u32)
            .max(1)
            .next_power_of_two();
        let first = meter.position(Ticks(self.left_tick.max(0.0) as i64)).bar;
        let right = self.left_tick + f64::from(layout.timeline.width()) * self.ticks_per_point;
        let mut bar = first;
        loop {
            let start = meter.bar_start(bar);
            if start.0 as f64 > right {
                break;
            }
            if bar.is_multiple_of(every) {
                let x = self.tick_to_x(layout, start);
                painter.vline(
                    x,
                    (ruler.top() + 8.0)..=ruler.bottom(),
                    Stroke::new(1.0, p.grid_bar),
                );
                painter.text(
                    pos2(x + 5.0, ruler.center().y),
                    Align2::LEFT_CENTER,
                    (bar + 1).to_string(),
                    FontId::proportional(size::SMALL),
                    p.text_weak,
                );
            }
            bar += 1;
        }
        let x = self.tick_to_x(layout, session.song_position());
        if x >= ruler.left() - 6.0 && x <= ruler.right() + 6.0 {
            let color = if session.is_recording() {
                p.record
            } else {
                p.accent
            };
            let points = vec![
                pos2(x - 6.0, ruler.bottom() - 10.0),
                pos2(x + 6.0, ruler.bottom() - 10.0),
                pos2(x, ruler.bottom()),
            ];
            painter.add(Shape::convex_polygon(points, color, Stroke::NONE));
        }
        let corner = Rect::from_min_max(
            layout.whole.min,
            pos2(layout.headers.right(), ruler.bottom()),
        );
        let corner_painter = ui.painter_at(corner);
        corner_painter.rect_filled(corner, CornerRadius::ZERO, p.surface);
        corner_painter.text(
            pos2(corner.left() + 12.0, corner.center().y),
            Align2::LEFT_CENTER,
            text::TRACKS,
            FontId::proportional(size::SMALL),
            p.text_weak,
        );
        ui.painter().hline(
            layout.whole.x_range(),
            ruler.bottom() - 0.5,
            Stroke::new(1.0, p.divider),
        );
    }

    /// The track list on the left: real widgets (names, mute, solo, a menu)
    /// in a column that scrolls with the rows.
    #[allow(clippy::too_many_lines, reason = "the whole track list, top to bottom")]
    fn headers(ui: &mut Ui, session: &mut Session, layout: &Layout, intents: &mut Vec<Intent>) {
        let p = Palette::of(ui.ctx());
        ui.painter_at(layout.headers)
            .rect_filled(layout.headers, CornerRadius::ZERO, p.surface);
        ui.painter().vline(
            layout.headers.right() - 0.5,
            layout.whole.y_range(),
            Stroke::new(1.0, p.divider),
        );
        let tracks: Vec<(TrackId, String, Instrument, bool, bool)> = session
            .project()
            .tracks
            .iter()
            .map(|t| {
                (
                    t.id,
                    t.name.clone(),
                    t.instrument.clone(),
                    t.strip.mute,
                    t.strip.solo,
                )
            })
            .collect();
        let count = tracks.len();
        for (row, (track, name, instrument, mute, solo)) in tracks.into_iter().enumerate() {
            let top = layout.row_top(row);
            let rect = Rect::from_min_max(
                pos2(layout.headers.left(), top),
                pos2(layout.headers.right() - 1.0, top + ROW_HEIGHT),
            );
            if rect.bottom() < layout.headers.top() || rect.top() > layout.headers.bottom() {
                continue;
            }
            let focused = track == session.track_id();
            let background = ui.interact(
                rect.intersect(layout.headers),
                ui.scope_id().with(("track", track.0)),
                Sense::click(),
            );
            if background.clicked() {
                session.focus_track(track);
            }
            if background.double_clicked() {
                intents.push(Intent::RenameTrack(track));
            }
            let painter = ui.painter_at(layout.headers);
            if focused {
                painter.rect_filled(rect, CornerRadius::ZERO, p.raised);
            }
            painter.rect_filled(
                Rect::from_min_max(
                    rect.left_top() + vec2(0.0, 6.0),
                    pos2(rect.left() + 4.0, rect.bottom() - 6.0),
                ),
                CornerRadius::same(2),
                track_color(row),
            );
            painter.hline(
                rect.x_range(),
                rect.bottom() - 0.5,
                Stroke::new(1.0, p.divider),
            );
            let mut child = ui.new_child(
                UiBuilder::new()
                    .max_rect(rect.shrink2(vec2(14.0, 8.0)))
                    .layout(EguiLayout::top_down(Align::Min)),
            );
            child.set_clip_rect(layout.headers);
            child.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(RichText::new(&name).strong().size(size::BODY));
                ui.with_layout(EguiLayout::right_to_left(Align::Center), |ui| {
                    let menu =
                        icon_button(ui, Icon::Menu, text::TRACK_MENU, "", Look::Normal, 24.0);
                    egui::Popup::menu(&menu).show(|ui| {
                        ui.set_min_width(180.0);
                        if ui.button(text::RENAME_PATTERN).clicked() {
                            intents.push(Intent::RenameTrack(track));
                            ui.close();
                        }
                        if ui
                            .add_enabled(row > 0, Button::new(text::MOVE_UP))
                            .clicked()
                        {
                            session.move_track(track, -1);
                            ui.close();
                        }
                        if ui
                            .add_enabled(row + 1 < count, Button::new(text::MOVE_DOWN))
                            .clicked()
                        {
                            session.move_track(track, 1);
                            ui.close();
                        }
                        ui.separator();
                        if ui
                            .add_enabled(count > 1, Button::new(text::DELETE_TRACK))
                            .clicked()
                        {
                            session.remove_track(track);
                            ui.close();
                        }
                    });
                    let solo_button = ui
                        .add(
                            Button::new(text::SOLO_SHORT)
                                .selected(solo)
                                .min_size(vec2(26.0, 22.0)),
                        )
                        .on_hover_text(text::SOLO_HINT);
                    if solo_button.clicked() {
                        session.toggle_solo(track);
                    }
                    let mute_button = ui
                        .add(
                            Button::new(text::MUTE_SHORT)
                                .selected(mute)
                                .min_size(vec2(26.0, 22.0)),
                        )
                        .on_hover_text(text::MUTE_HINT);
                    if mute_button.clicked() {
                        session.toggle_mute(track);
                    }
                });
            });
            let kind = match &instrument {
                Instrument::Drums { kit, .. } => text::kit_description(text::kit_name(kit)),
                Instrument::SoundFont { .. } => text::FIRST_TRACK.to_owned(),
            };
            child.label(RichText::new(kind).small().color(p.text_weak));
        }
        // Adding tracks, under the list.
        let top = layout.row_top(count);
        let rect = Rect::from_min_max(
            pos2(layout.headers.left(), top),
            pos2(layout.headers.right() - 1.0, top + ADD_ROW_HEIGHT),
        );
        if rect.top() < layout.headers.bottom() {
            let mut child = ui.new_child(
                UiBuilder::new()
                    .max_rect(rect.shrink2(vec2(12.0, 10.0)))
                    .layout(EguiLayout::left_to_right(Align::Center)),
            );
            child.set_clip_rect(layout.headers);
            if child
                .button(format!("+ {}", text::ADD_DRUMS))
                .on_hover_text(text::ADD_DRUMS_HINT)
                .clicked()
            {
                session.add_drum_track();
            }
            if child
                .button(format!("+ {}", text::ADD_PIANO))
                .on_hover_text(text::ADD_PIANO_HINT)
                .clicked()
            {
                session.add_piano_track();
            }
        }
    }

    /// The right-click menu of a clip.
    fn clip_menu(&mut self, ui: &Ui, session: &mut Session) {
        let Some(clip) = self.menu_clip else {
            return;
        };
        let Some((track, pattern)) = session
            .project()
            .clip(clip)
            .and_then(|(t, c)| c.pattern().map(|p| (t, p)))
        else {
            self.menu_clip = None;
            return;
        };
        let mut close = false;
        let area = egui::Area::new(ui.scope_id().with("clip-menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(self.menu_at)
            .show(ui.ctx(), |ui| {
                egui::Frame::menu(ui.style()).show(ui, |ui| {
                    ui.set_min_width(170.0);
                    if ui.button(text::OPEN_PATTERN).clicked() {
                        session.open_pattern(track, pattern);
                        close = true;
                    }
                    if ui.button(text::DUPLICATE_CLIP).clicked() {
                        session.duplicate_clip(clip);
                        close = true;
                    }
                    if ui.button(text::REMOVE_CLIP).clicked() {
                        session.remove_clip(clip);
                        close = true;
                    }
                });
            });
        let pressed_outside = ui.input(|i| i.pointer.any_pressed())
            && !area.response.contains_pointer()
            && ui
                .input(|i| i.pointer.interact_pos())
                .is_some_and(|p| p != self.menu_at);
        if close || pressed_outside || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.menu_clip = None;
        }
    }
}

/// Whether a clip, moved to `start` (and given `length`), would overlap
/// another clip on its track.
fn overlaps(session: &Session, clip: ClipId, start: Ticks, length: Option<Ticks>) -> bool {
    let Some((track, current)) = session.project().clip(clip) else {
        return true;
    };
    let end = start + length.unwrap_or(current.length);
    session.project().track(track).is_some_and(|t| {
        t.clips
            .iter()
            .any(|c| c.id != clip && c.start < end && start < c.end())
    })
}

fn min_zoom() -> f64 {
    #[allow(clippy::cast_precision_loss, reason = "a constant")]
    let quarter = Ticks::QUARTER.0 as f64;
    quarter / 400.0
}

fn max_zoom() -> f64 {
    #[allow(clippy::cast_precision_loss, reason = "a constant")]
    let quarter = Ticks::QUARTER.0 as f64;
    quarter / 1.5
}
