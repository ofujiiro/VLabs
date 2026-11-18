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
//!   Alt while dragging to move by beats instead of bars;
//! - a track's automation lanes sit under it (decision 0016): click a lane
//!   to add a point, drag a point to move it, right-click it to remove it;
//! - audio clips (decision 0017) show their waveform: drag either end to
//!   trim (on beats; exactly with Alt), drag the small handles at their top
//!   corners to fade in or out, drag one to another audio track; click an
//!   empty spot of an audio track to start recording from there; drop sound
//!   files on the view to import them;
//! - under the ruler, the signature row (decision 0018) holds the tempo,
//!   time-signature and key changes (`signature_row`).

use std::path::PathBuf;

use daw_dsp::audio::Peaks;
use daw_dsp::drums::PadSound;
use daw_engine::PluginTarget;
use daw_model::{
    AutomationPoint, AutomationTarget, Clip, ClipContent, ClipId, Instrument, Pattern, Project,
    Region, Ticks, TrackId,
};
use egui::{
    Align, Align2, Button, Color32, CornerRadius, CursorIcon, FontId, Label, Layout as EguiLayout,
    Mesh, PointerButton, PopupCloseBehavior, Pos2, Rect, Response, RichText, Sense, Shape, Slider,
    Stroke, StrokeKind, TextEdit, Ui, UiBuilder, pos2, vec2,
};

use crate::icons::{Icon, Look, icon_button};
use crate::session::{LaneChoice, Session, SoundState};
use crate::signature_row::{Axis, SignatureRow};
use crate::step_grid::PAD_ORDER;
use crate::text;
use crate::theme::{Palette, size, track_color};
use crate::workspace::Intent;

const HEADER_WIDTH: f32 = 236.0;
const RULER_HEIGHT: f32 = 28.0;
const ROW_HEIGHT: f32 = 64.0;
const LANE_HEIGHT: f32 = 52.0;
/// Room above and below a lane's line.
const LANE_MARGIN: f32 = 7.0;
/// How close to an automation point the pointer must be to take it.
const POINT_GRAB: f32 = 7.0;
const ADD_ROW_HEIGHT: f32 = 80.0;
/// How close to a clip's end the pointer must be to trim or resize it.
const EDGE_GRAB: f32 = 8.0;
const DRAG_THRESHOLD: f32 = 4.0;
/// How close to a fade handle the pointer must be to take it.
const FADE_GRAB: f32 = 7.0;
/// How far fade handles sit inside a clip's top corners.
const HANDLE_INSET: f32 = 5.0;

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
    /// Moving a clip's start, what it plays staying where it is.
    TrimStart { clip: ClipId },
    /// Changing an audio clip's fade in, or out.
    Fade { clip: ClipId, out: bool },
    /// Along the ruler: a click moves the play position, a drag loops.
    Ruler {
        anchor: Ticks,
        origin: Pos2,
        looping: bool,
    },
    /// Moving the view.
    Pan { last: Pos2 },
    /// Moving an automation point.
    Point {
        track: TrackId,
        target: AutomationTarget,
        index: usize,
    },
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
    /// Where "Split here" splits: the moment right-clicked.
    menu_split: Ticks,
    scrolled_at: f64,
    /// The settings offered while a track's lane chooser is open.
    lane_menu: Option<(TrackId, Vec<LaneChoice>)>,
    lane_search: String,
    /// The tempo and signature changes, under the ruler.
    signatures: SignatureRow,
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
            menu_split: Ticks::ZERO,
            scrolled_at: f64::NEG_INFINITY,
            lane_menu: None,
            lane_search: String::new(),
            signatures: SignatureRow::default(),
        }
    }
}

/// What a row of the view shows.
#[derive(Clone, Copy, Debug, PartialEq)]
enum RowKind {
    /// A track, with its clips.
    Track,
    /// One of the track's automation lanes.
    Lane(AutomationTarget),
}

/// A row of the view: a track, or one of its lanes under it.
#[derive(Clone, Copy, Debug)]
struct Row {
    /// The track's place in the list.
    index: usize,
    track: TrackId,
    kind: RowKind,
    /// From the top of the first row, in points.
    offset: f32,
    height: f32,
}

/// The parts of the view, worked out for each frame.
#[derive(Clone, Debug)]
struct Layout {
    whole: Rect,
    headers: Rect,
    ruler: Rect,
    /// The signature row, across the whole width, under the ruler.
    conductor: Rect,
    timeline: Rect,
    top: f32,
    rows: Vec<Row>,
    /// Where the rows end, from the top of the first.
    rows_end: f32,
}

impl Layout {
    fn new(whole: Rect, top: f32, project: &Project, conductor: f32) -> Self {
        let left = whole.left() + HEADER_WIDTH;
        let ruler_bottom = whole.top() + RULER_HEIGHT;
        let below_ruler = ruler_bottom + conductor;
        let mut rows = Vec::new();
        let mut offset = 0.0;
        for (index, track) in project.tracks.iter().enumerate() {
            rows.push(Row {
                index,
                track: track.id,
                kind: RowKind::Track,
                offset,
                height: ROW_HEIGHT,
            });
            offset += ROW_HEIGHT;
            for lane in &track.automation {
                rows.push(Row {
                    index,
                    track: track.id,
                    kind: RowKind::Lane(lane.target),
                    offset,
                    height: LANE_HEIGHT,
                });
                offset += LANE_HEIGHT;
            }
        }
        Self {
            whole,
            headers: Rect::from_min_max(
                pos2(whole.left(), below_ruler),
                pos2(left, whole.bottom()),
            ),
            ruler: Rect::from_min_max(pos2(left, whole.top()), pos2(whole.right(), ruler_bottom)),
            conductor: Rect::from_min_max(
                pos2(whole.left(), ruler_bottom),
                pos2(whole.right(), below_ruler),
            ),
            timeline: Rect::from_min_max(pos2(left, below_ruler), whole.max),
            top,
            rows,
            rows_end: offset,
        }
    }

    /// The top of a row on screen.
    fn top_of(&self, row: &Row) -> f32 {
        self.timeline.top() + row.offset - self.top
    }

    /// The top of a track's row on screen.
    fn row_top(&self, index: usize) -> f32 {
        self.rows
            .iter()
            .find(|r| r.index == index && r.kind == RowKind::Track)
            .map_or(self.timeline.top(), |r| self.top_of(r))
    }

    /// The bottom of the last row on screen.
    fn rows_bottom(&self) -> f32 {
        self.timeline.top() + self.rows_end - self.top
    }

    /// The row under a y position.
    fn row_at(&self, y: f32) -> Option<Row> {
        let at = y - self.timeline.top() + self.top;
        self.rows
            .iter()
            .find(|r| at >= r.offset && at < r.offset + r.height)
            .copied()
    }

    /// A lane's row.
    fn lane_row(&self, track: TrackId, target: AutomationTarget) -> Option<Row> {
        self.rows
            .iter()
            .find(|r| r.track == track && r.kind == RowKind::Lane(target))
            .copied()
    }

    /// Where a lane value (0 to 1) is drawn in a lane's row.
    fn lane_y(&self, row: &Row, value: f32) -> f32 {
        let bottom = self.top_of(row) + row.height - LANE_MARGIN;
        bottom - value.clamp(0.0, 1.0) * (row.height - 2.0 * LANE_MARGIN)
    }

    /// The lane value at a height in a lane's row.
    fn lane_value(&self, row: &Row, y: f32) -> f32 {
        let bottom = self.top_of(row) + row.height - LANE_MARGIN;
        ((bottom - y) / (row.height - 2.0 * LANE_MARGIN)).clamp(0.0, 1.0)
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
        let mut layout = Layout::new(rect, self.top, session.project(), self.signatures.height());
        let content = layout.rows_end + ADD_ROW_HEIGHT;
        if !self.fitted && layout.timeline.width() > 50.0 {
            self.fit(session, &layout);
            self.fitted = true;
        }
        self.handle_view_input(ui, &layout);
        self.take_dropped_files(ui, session, &layout);
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
        self.paint_lanes(ui, session, &layout);
        self.paint_clips(ui, session, &layout);
        self.paint_ruler(ui, session, &layout);
        let axis = Axis {
            left: layout.timeline.left(),
            left_tick: self.left_tick,
            ticks_per_point: self.ticks_per_point,
        };
        self.signatures
            .ui(ui, session, layout.conductor, layout.headers.right(), axis);
        self.headers(ui, session, &layout, intents);
        self.clip_menu(ui, session);
        if ui.input(|i| !i.raw.hovered_files.is_empty()) {
            let painter = ui.painter_at(layout.timeline);
            painter.rect_filled(
                layout.timeline,
                CornerRadius::ZERO,
                p.accent.gamma_multiply(0.12),
            );
            painter.text(
                layout.timeline.center(),
                Align2::CENTER_CENTER,
                text::DROP_HERE,
                FontId::proportional(size::HEADING),
                p.text,
            );
        }
    }

    /// Sound files dropped on the view: on the audio track under the
    /// pointer at its beat, or on new tracks (at the play position if the
    /// pointer is not over the tracks).
    fn take_dropped_files(&mut self, ui: &Ui, session: &mut Session, layout: &Layout) {
        let files: Vec<PathBuf> = ui.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        if files.is_empty() {
            return;
        }
        let pointer = ui
            .input(|i| i.pointer.latest_pos())
            .filter(|p| layout.timeline.contains(*p));
        let (track, at) = match pointer {
            Some(pos) => {
                let track = layout
                    .row_at(pos.y)
                    .filter(|r| r.kind == RowKind::Track && session.is_audio_track(r.track))
                    .map(|r| r.track);
                (
                    track,
                    Self::snap_nearest(session, self.x_to_tick(layout, pos.x), true),
                )
            }
            None => (None, session.song_position()),
        };
        session.import_audio(files, track, at);
    }

    /// Whether a drag is in progress.
    pub fn is_dragging(&self) -> bool {
        self.drag != Drag::None || self.signatures.is_dragging()
    }

    /// Abandons the drag in progress, putting everything back.
    pub fn cancel_drag(&mut self, session: &mut Session) -> bool {
        if self.signatures.cancel_drag(session) {
            return true;
        }
        if self.drag == Drag::None {
            return false;
        }
        if !matches!(self.drag, Drag::Pan { .. }) {
            session.cancel_gesture();
        }
        self.drag = Drag::None;
        true
    }

    /// Closes the clip menu and the signature row's panel, if one is open
    /// (Escape: the keymap takes the key before the view sees it); returns
    /// whether one was.
    pub fn close_popups(&mut self) -> bool {
        let menu = self.menu_clip.take().is_some();
        let panel = self.signatures.close_panel();
        menu || panel
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

    /// A tick snapped down to the bar (or beat, with Alt) it falls in,
    /// counted from the bar lines (decision 0018).
    fn snap(session: &Session, at: Ticks, fine: bool) -> Ticks {
        let meter = &session.project().meter;
        let at = at.max(Ticks::ZERO);
        let unit = if fine {
            meter.signature_at(at).beat_length()
        } else {
            meter.signature_at(at).bar_length()
        };
        meter.grid_floor(at, unit)
    }

    fn snap_nearest(session: &Session, at: Ticks, fine: bool) -> Ticks {
        let meter = &session.project().meter;
        let at = at.max(Ticks::ZERO);
        let unit = if fine {
            meter.signature_at(at).beat_length()
        } else {
            meter.signature_at(at).bar_length()
        };
        meter.grid_round(at, unit)
    }

    /// Where a dragged clip edge lands: on bars for patterns (beats with
    /// Alt), on beats for sound (exactly where the pointer is with Alt).
    fn snap_edge(session: &Session, at: Ticks, alt: bool, audio: bool) -> Ticks {
        match (audio, alt) {
            (true, true) => at.max(Ticks::ZERO),
            (true, false) => Self::snap_nearest(session, at, true),
            (false, fine) => Self::snap_nearest(session, at, fine),
        }
    }

    /// Where an audio clip's fade handles are on screen: at the end of its
    /// fade in and the start of its fade out, near its top.
    fn fade_handles(
        &self,
        layout: &Layout,
        session: &Session,
        clip: &Clip,
        rect: Rect,
    ) -> Option<(Pos2, Pos2)> {
        let (fade_in, fade_out) = session.clip_fades(clip.id)?;
        let project = session.project();
        let in_end = if fade_in > 0.0 {
            clip.start + project.ticks_for(clip.start, f64::from(fade_in))
        } else {
            clip.start
        };
        let out_seconds = project.tempo.seconds_at(clip.end()) - f64::from(fade_out);
        let out_start = Ticks(project.tempo.ticks_at(out_seconds).round() as i64);
        let (left, right) = (rect.left() + HANDLE_INSET, rect.right() - HANDLE_INSET);
        let y = rect.top() + HANDLE_INSET;
        let x = |tick: Ticks| self.tick_to_x(layout, tick).clamp(left, right.max(left));
        Some((pos2(x(in_end), y), pos2(x(out_start), y)))
    }

    /// The fade handle of a clip under a point: `Some(true)` for the fade
    /// out, `Some(false)` for the fade in.
    fn fade_handle_at(
        &self,
        layout: &Layout,
        session: &Session,
        clip: ClipId,
        rect: Rect,
        pos: Pos2,
    ) -> Option<bool> {
        let (_, c) = session.project().clip(clip)?;
        let (fade_in, fade_out) = self.fade_handles(layout, session, c, rect)?;
        if pos.distance(fade_out) <= FADE_GRAB {
            Some(true)
        } else if pos.distance(fade_in) <= FADE_GRAB {
            Some(false)
        } else {
            None
        }
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
        let row = layout.row_at(pos.y)?;
        if row.kind != RowKind::Track {
            return None;
        }
        let track = session.project().tracks.get(row.index)?;
        track.clips.iter().rev().find_map(|clip| {
            let rect = self.clip_rect(layout, row.index, clip);
            rect.contains(pos).then_some((clip.id, row.index, rect))
        })
    }

    /// The automation point under a point of a lane's row, if any.
    fn point_at(
        &self,
        layout: &Layout,
        session: &Session,
        row: &Row,
        target: AutomationTarget,
        pos: Pos2,
    ) -> Option<usize> {
        let lane = session.project().lane(row.track, target)?;
        lane.points
            .iter()
            .enumerate()
            .filter_map(|(index, p)| {
                let at = pos2(self.tick_to_x(layout, p.at), layout.lane_y(row, p.value));
                let distance = at.distance(pos);
                (distance <= POINT_GRAB).then_some((index, distance))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(index, _)| index)
    }

    /// A moment of a lane under an x position: on the nearest beat, or
    /// exactly there with Alt.
    fn lane_tick(&self, layout: &Layout, session: &Session, x: f32, free: bool) -> Ticks {
        let at = self.x_to_tick(layout, x).max(Ticks::ZERO);
        if free {
            at
        } else {
            Self::snap_nearest(session, at, true)
        }
    }

    /// A press on a lane: takes the point there, or adds one and takes it.
    fn press_lane(
        &mut self,
        layout: &Layout,
        session: &mut Session,
        row: &Row,
        target: AutomationTarget,
        pos: Pos2,
        free: bool,
    ) {
        let track = row.track;
        if let Some(index) = self.point_at(layout, session, row, target, pos) {
            session.begin_gesture(text::EDIT_MOVE_POINT);
            self.drag = Drag::Point {
                track,
                target,
                index,
            };
            return;
        }
        let point = AutomationPoint {
            at: self.lane_tick(layout, session, pos.x, free),
            value: layout.lane_value(row, pos.y),
        };
        session.begin_gesture(text::EDIT_ADD_POINT);
        match session.add_lane_point(track, target, point) {
            Some(index) => {
                self.drag = Drag::Point {
                    track,
                    target,
                    index,
                };
            }
            None => session.end_gesture(),
        }
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
        let inside = layout.whole.contains(pos)
            && !layout.headers.contains(pos)
            && !layout.conductor.contains(pos);
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
        // (Over the view, not over a menu or popup on top of it. A quick tap
        // can press and release within one frame.)
        if primary_pressed
            && inside
            && self.drag == Drag::None
            && (response.hovered() || response.is_pointer_button_down_on())
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
                if let Some(out) = self.fade_handle_at(layout, session, clip, rect, pos) {
                    session.begin_gesture(text::EDIT_FADE);
                    self.drag = Drag::Fade { clip, out };
                } else if pos.x >= rect.right() - EDGE_GRAB {
                    session.begin_gesture(text::EDIT_RESIZE_CLIP);
                    self.drag = Drag::Resize { clip };
                } else if pos.x <= rect.left() + EDGE_GRAB && rect.width() >= 3.0 * EDGE_GRAB {
                    session.begin_gesture(text::EDIT_TRIM_CLIP);
                    self.drag = Drag::TrimStart { clip };
                } else if let Some(start) = session.project().clip(clip).map(|(_, c)| c.start) {
                    session.begin_gesture(text::EDIT_MOVE_CLIP);
                    self.drag = Drag::Move {
                        clip,
                        grab: self.x_to_tick(layout, pos.x) - start,
                        origin: pos,
                        moved: false,
                    };
                }
            } else if let Some(row) = layout.row_at(pos.y)
                && let RowKind::Lane(target) = row.kind
            {
                self.press_lane(layout, session, &row, target, pos, alt);
            } else if let Some(row) = layout.row_at(pos.y)
                && session.is_audio_track(row.track)
            {
                // An audio track has no pattern to place: the play position
                // moves there, so Record starts from it.
                session.focus_track(row.track);
                session.select_clip(None);
                let at = Self::snap_edge(session, self.x_to_tick(layout, pos.x), alt, true);
                session.locate_song(at);
            } else if let Some(row) = layout.row_at(pos.y) {
                let track = row.track;
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
            if let Some(row) = layout.row_at(pos.y)
                && let RowKind::Lane(target) = row.kind
            {
                if let Some(index) = self.point_at(layout, session, &row, target, pos) {
                    session.remove_lane_point(row.track, target, index);
                }
                self.menu_clip = None;
            } else if let Some((clip, _, _)) = self.clip_at(layout, session, pos) {
                session.select_clip(Some(clip));
                self.menu_clip = Some(clip);
                self.menu_at = pos;
                let audio = session
                    .project()
                    .clip(clip)
                    .is_some_and(|(_, c)| c.is_audio());
                self.menu_split =
                    Self::snap_edge(session, self.x_to_tick(layout, pos.x), alt, audio);
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
                    // Sound can go to another audio track, where it fits.
                    if let Some(row) = layout.row_at(pos.y)
                        && row.kind == RowKind::Track
                        && session.project().clip(clip).is_some_and(|(t, c)| {
                            t != row.track && c.is_audio() && session.is_audio_track(row.track)
                        })
                        && !overlaps_on(session, row.track, clip, None, None)
                    {
                        session.move_clip_to_track(clip, row.track);
                    }
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
                if let Some((_, c)) = session.project().clip(clip).map(|(t, c)| (t, *c)) {
                    let start = c.start;
                    let x = self.x_to_tick(layout, pos.x);
                    let end = Self::snap_edge(session, x, alt, c.is_audio());
                    let shortest = if c.is_audio() && alt {
                        Ticks(1)
                    } else {
                        session.project().meter.signature_at(start).beat_length()
                    };
                    let length = (end - start).max(shortest);
                    if !overlaps(session, clip, start, Some(length)) {
                        session.resize_clip(clip, length);
                    }
                }
            }
            Drag::TrimStart { clip } if any_down => {
                if let Some((_, c)) = session.project().clip(clip).map(|(t, c)| (t, *c)) {
                    let end = c.end();
                    let x = self.x_to_tick(layout, pos.x);
                    let shortest = if c.is_audio() && alt {
                        Ticks(1)
                    } else {
                        session.project().meter.signature_at(c.start).beat_length()
                    };
                    let start = Self::snap_edge(session, x, alt, c.is_audio()).min(end - shortest);
                    if !overlaps(session, clip, start, Some(end - start)) {
                        session.trim_clip_start(clip, start);
                    }
                }
            }
            Drag::Fade { clip, out } if any_down => {
                if let Some((_, c)) = session.project().clip(clip).map(|(t, c)| (t, *c)) {
                    let (fade_in, fade_out) = session.clip_fades(clip).unwrap_or_default();
                    let at = self.x_to_tick(layout, pos.x).clamp(c.start, c.end());
                    #[allow(clippy::cast_possible_truncation, reason = "seconds")]
                    if out {
                        let seconds = session.project().seconds_between(at, c.end()) as f32;
                        session.set_clip_fades(clip, fade_in, seconds);
                    } else {
                        let seconds = session.project().seconds_between(c.start, at) as f32;
                        session.set_clip_fades(clip, seconds, fade_out);
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
            Drag::Point {
                track,
                target,
                index,
            } if any_down => {
                if let Some(row) = layout.lane_row(track, target) {
                    let point = AutomationPoint {
                        at: self.lane_tick(layout, session, pos.x, alt),
                        value: layout.lane_value(&row, pos.y),
                    };
                    session.move_lane_point(track, target, index, point);
                }
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
            let lane = layout.row_at(pos.y).and_then(|row| match row.kind {
                RowKind::Lane(target) if !layout.ruler.contains(pos) => Some((row, target)),
                _ => None,
            });
            let icon = match self.clip_at(layout, session, pos) {
                Some((clip, _, rect))
                    if self
                        .fade_handle_at(layout, session, clip, rect, pos)
                        .is_some() =>
                {
                    CursorIcon::PointingHand
                }
                Some((_, _, rect))
                    if pos.x >= rect.right() - EDGE_GRAB
                        || (pos.x <= rect.left() + EDGE_GRAB
                            && rect.width() >= 3.0 * EDGE_GRAB) =>
                {
                    CursorIcon::ResizeHorizontal
                }
                Some(_) => CursorIcon::Grab,
                None if layout.ruler.contains(pos) => CursorIcon::PointingHand,
                None => match lane {
                    Some((row, target))
                        if self.point_at(layout, session, &row, target, pos).is_some() =>
                    {
                        CursorIcon::Grab
                    }
                    Some(_) => CursorIcon::Crosshair,
                    None => CursorIcon::Default,
                },
            };
            ui.ctx().set_cursor_icon(icon);
        } else if matches!(
            self.drag,
            Drag::Point { .. } | Drag::Move { moved: true, .. }
        ) {
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        } else if matches!(self.drag, Drag::Resize { .. } | Drag::TrimStart { .. }) {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
        } else if matches!(self.drag, Drag::Fade { .. }) {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
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
        for row in &layout.rows {
            let top = layout.top_of(row);
            let band = Rect::from_min_max(
                pos2(layout.timeline.left(), top),
                pos2(layout.timeline.right(), top + row.height),
            );
            if band.bottom() < layout.timeline.top() || band.top() > layout.timeline.bottom() {
                continue;
            }
            let empty_audio = row.kind == RowKind::Track
                && session
                    .project()
                    .track(row.track)
                    .is_some_and(|t| t.instrument.is_audio() && t.clips.is_empty())
                && session.live_take().is_none_or(|l| l.track != row.track);
            if empty_audio {
                painter.text(
                    pos2(layout.timeline.left() + 16.0, band.center().y),
                    Align2::LEFT_CENTER,
                    text::AUDIO_HINT,
                    FontId::proportional(size::SMALL),
                    p.text_weak.gamma_multiply(0.8),
                );
            }
            match row.kind {
                RowKind::Track if row.track == session.focused_track() => {
                    painter.rect_filled(band, CornerRadius::ZERO, p.grid_dark_row);
                }
                RowKind::Track => {}
                RowKind::Lane(_) => {
                    painter.rect_filled(
                        band,
                        CornerRadius::ZERO,
                        track_color(row.index).gamma_multiply(0.07),
                    );
                }
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
        let rows_bottom = layout.rows_bottom();
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

    /// Each lane's line through its points, and the points.
    fn paint_lanes(&self, ui: &Ui, session: &mut Session, layout: &Layout) {
        let p = Palette::of(ui.ctx());
        let painter = ui.painter_at(layout.timeline);
        let hover = ui.input(|i| i.pointer.hover_pos());
        for row in &layout.rows {
            let RowKind::Lane(target) = row.kind else {
                continue;
            };
            let top = layout.top_of(row);
            if top + row.height < layout.timeline.top() || top > layout.timeline.bottom() {
                continue;
            }
            let Some(lane) = session.project().lane(row.track, target).cloned() else {
                continue;
            };
            let color = track_color(row.index);
            let (left, right) = (layout.timeline.left(), layout.timeline.right());
            let points: Vec<Pos2> = lane
                .points
                .iter()
                .map(|pt| pos2(self.tick_to_x(layout, pt.at), layout.lane_y(row, pt.value)))
                .collect();
            if let (Some(first), Some(last)) = (points.first(), points.last()) {
                // Before the first point and after the last, the value holds.
                let mut path = vec![pos2(left.min(first.x), first.y)];
                path.extend(points.iter().copied());
                path.push(pos2(right.max(last.x), last.y));
                painter.add(Shape::line(path, Stroke::new(2.0, color)));
            }
            let dragging = match self.drag {
                Drag::Point {
                    track,
                    target: t,
                    index,
                } if track == row.track && t == target => Some(index),
                _ => None,
            };
            for (index, at) in points.iter().enumerate() {
                let near = hover.is_some_and(|h| h.distance(*at) <= POINT_GRAB);
                let radius = if near || dragging == Some(index) {
                    5.5
                } else {
                    4.0
                };
                painter.circle(*at, radius, color, Stroke::new(1.5, p.text));
            }
            // While a point moves, its value is written beside it.
            if let Some(index) = dragging
                && let (Some(at), Some(point)) = (points.get(index), lane.points.get(index))
            {
                let value = session.lane_value_text(row.track, target, point.value);
                painter.text(
                    *at + vec2(10.0, -10.0),
                    Align2::LEFT_BOTTOM,
                    value,
                    FontId::proportional(size::SMALL),
                    p.text,
                );
            }
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
                if clip.is_audio() {
                    self.paint_audio_clip(ui, &painter, layout, session, rect, clip, color);
                }
                let stroke = if selected {
                    Stroke::new(2.0, p.selected)
                } else {
                    Stroke::new(1.0, color.gamma_multiply(0.9))
                };
                painter.rect_stroke(rect, CornerRadius::same(6), stroke, StrokeKind::Inside);
            }
        }
        self.paint_live_take(&painter, layout, session);
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

    /// An audio clip's inside: its name, its waveform (at its volume), its
    /// fades and their handles.
    #[allow(clippy::too_many_arguments, reason = "the parts of one drawing")]
    fn paint_audio_clip(
        &self,
        ui: &Ui,
        painter: &egui::Painter,
        layout: &Layout,
        session: &Session,
        rect: Rect,
        clip: &Clip,
        color: Color32,
    ) {
        let p = Palette::of(ui.ctx());
        let ClipContent::Audio {
            audio,
            offset,
            gain,
            fade_in,
            fade_out,
        } = clip.content
        else {
            return;
        };
        let Some(file) = session.project().audio_file(audio) else {
            return;
        };
        let area = Rect::from_min_max(
            pos2(rect.left(), rect.top() + 20.0),
            pos2(rect.right(), rect.bottom() - 4.0),
        );
        let label = match session.sound_state(&file.file) {
            SoundState::Ready(peaks) => {
                if area.height() >= 4.0 {
                    let start_frame = file.seconds_in(offset) * f64::from(peaks.sample_rate());
                    self.paint_waveform(
                        painter,
                        layout,
                        session,
                        area,
                        clip.start,
                        start_frame,
                        &peaks,
                        gain,
                        color.gamma_multiply(0.95),
                    );
                }
                file.name.clone()
            }
            SoundState::Loading => format!("{} · {}", file.name, text::LOADING_SOUND),
            SoundState::Failed(_) => format!("⚠ {}", file.name),
        };
        painter.text(
            rect.left_top() + vec2(14.0, 4.0),
            Align2::LEFT_TOP,
            label,
            FontId::proportional(size::SMALL),
            p.text,
        );
        self.paint_fades(
            painter,
            layout,
            session,
            area,
            clip,
            (fade_in, fade_out),
            p.text,
        );
        if let Some((a, b)) = self.fade_handles(layout, session, clip, rect) {
            let hover = ui.input(|i| i.pointer.hover_pos());
            for handle in [a, b] {
                let near = hover.is_some_and(|h| h.distance(handle) <= FADE_GRAB);
                let side = if near { 4.5 } else { 3.5 };
                painter.rect_filled(
                    Rect::from_center_size(handle, vec2(side * 2.0, side * 2.0)),
                    CornerRadius::same(2),
                    if near { p.selected } else { p.text },
                );
            }
        }
    }

    /// The waveform of sound summed up in `peaks`, from frame `start_frame`
    /// at tick `start`, in `area` (one line of loudness per pixel).
    #[allow(clippy::too_many_arguments, reason = "the parts of one drawing")]
    fn paint_waveform(
        &self,
        painter: &egui::Painter,
        layout: &Layout,
        session: &Session,
        area: Rect,
        start: Ticks,
        start_frame: f64,
        peaks: &Peaks,
        gain: f32,
        color: Color32,
    ) {
        let tempo = &session.project().tempo;
        let rate = f64::from(peaks.sample_rate());
        let start_seconds = tempo.seconds_at(start);
        let frame_at = |x: f32| {
            let seconds = tempo.seconds_at(self.x_to_tick(layout, x)) - start_seconds;
            (start_frame + seconds * rate).max(0.0) as usize
        };
        let (mid, half) = (area.center().y, area.height() / 2.0 - 1.0);
        let right = area.right().min(layout.timeline.right());
        let mut x = area.left().max(layout.timeline.left()).floor();
        let mut from = frame_at(x);
        let stroke = Stroke::new(1.0, color);
        while x < right {
            let to = frame_at(x + 1.0).max(from + 1);
            if let Some((low, high)) = peaks.range(from, to) {
                let high = (high * gain).clamp(-1.0, 1.0);
                let low = (low * gain).clamp(-1.0, 1.0);
                let top = mid - high * half;
                let bottom = (mid - low * half).max(top + 1.0);
                painter.line_segment([pos2(x + 0.5, top), pos2(x + 0.5, bottom)], stroke);
            }
            from = to;
            x += 1.0;
        }
    }

    /// The fade curves over an audio clip, with the faded part shaded.
    #[allow(clippy::too_many_arguments, reason = "the parts of one drawing")]
    fn paint_fades(
        &self,
        painter: &egui::Painter,
        layout: &Layout,
        session: &Session,
        area: Rect,
        clip: &Clip,
        (fade_in, fade_out): (f32, f32),
        color: Color32,
    ) {
        let tempo = &session.project().tempo;
        let start = tempo.seconds_at(clip.start);
        let end = tempo.seconds_at(clip.end());
        for (seconds, out) in [(fade_in, false), (fade_out, true)] {
            if seconds <= 0.0 {
                continue;
            }
            let seconds = f64::from(seconds);
            let steps = 24;
            let curve: Vec<Pos2> = (0..=steps)
                .map(|i| {
                    let u = f64::from(i) / f64::from(steps);
                    let at = if out {
                        end - seconds + u * seconds
                    } else {
                        start + u * seconds
                    };
                    let tick = Ticks(tempo.ticks_at(at).round() as i64);
                    // Equal power: the level rises (or falls) along a sine.
                    let level = if out { 1.0 - u } else { u };
                    let gain = (std::f64::consts::FRAC_PI_2 * level).sin() as f32;
                    pos2(
                        self.tick_to_x(layout, tick),
                        area.bottom() - gain * area.height(),
                    )
                })
                .collect();
            // The faded part: between the curve and the clip's top.
            let corner = if out {
                pos2(curve[steps as usize].x, area.top())
            } else {
                pos2(curve[0].x, area.top())
            };
            let mut mesh = Mesh::default();
            let shade = Color32::from_black_alpha(70);
            mesh.colored_vertex(corner, shade);
            for point in &curve {
                mesh.colored_vertex(*point, shade);
            }
            for i in 1..=steps {
                mesh.add_triangle(0, i, i + 1);
            }
            painter.add(Shape::mesh(mesh));
            painter.add(Shape::line(
                curve,
                Stroke::new(1.5, color.gamma_multiply(0.8)),
            ));
        }
    }

    /// The take being recorded: from where recording started to the play
    /// position, with its waveform so far.
    fn paint_live_take(&self, painter: &egui::Painter, layout: &Layout, session: &Session) {
        let Some(live) = session.live_take() else {
            return;
        };
        let Some(row) = session
            .project()
            .tracks
            .iter()
            .position(|t| t.id == live.track)
        else {
            return;
        };
        let p = Palette::of(painter.ctx());
        let top = layout.row_top(row);
        let end = session.song_position().max(live.start);
        let left = self.tick_to_x(layout, live.start);
        let rect = Rect::from_min_max(
            pos2(left, top + 4.0),
            pos2(
                self.tick_to_x(layout, end).max(left + 3.0),
                top + ROW_HEIGHT - 4.0,
            ),
        );
        painter.rect_filled(rect, CornerRadius::same(6), p.record.gamma_multiply(0.22));
        if let Some(peaks) = &live.peaks {
            let area = Rect::from_min_max(
                pos2(rect.left(), rect.top() + 20.0),
                pos2(rect.right(), rect.bottom() - 4.0),
            );
            self.paint_waveform(
                painter, layout, session, area, live.start, 0.0, peaks, 1.0, p.record,
            );
        }
        painter.text(
            rect.left_top() + vec2(14.0, 4.0),
            Align2::LEFT_TOP,
            text::RECORD_SOUND,
            FontId::proportional(size::SMALL),
            p.text,
        );
        painter.rect_stroke(
            rect,
            CornerRadius::same(6),
            Stroke::new(1.5, p.record),
            StrokeKind::Inside,
        );
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
        let daw_model::ClipContent::Pattern { offset, .. } = clip.content else {
            return;
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
    /// in a column that scrolls with the rows, each track's lanes under it.
    fn headers(
        &mut self,
        ui: &mut Ui,
        session: &mut Session,
        layout: &Layout,
        intents: &mut Vec<Intent>,
    ) {
        let p = Palette::of(ui.ctx());
        ui.painter_at(layout.headers)
            .rect_filled(layout.headers, CornerRadius::ZERO, p.surface);
        ui.painter().vline(
            layout.headers.right() - 0.5,
            layout.whole.y_range(),
            Stroke::new(1.0, p.divider),
        );
        for row in &layout.rows {
            let top = layout.top_of(row);
            let rect = Rect::from_min_max(
                pos2(layout.headers.left(), top),
                pos2(layout.headers.right() - 1.0, top + row.height),
            );
            if rect.bottom() < layout.headers.top() || rect.top() > layout.headers.bottom() {
                continue;
            }
            match row.kind {
                RowKind::Track => self.track_header(ui, session, layout, row, rect, intents),
                RowKind::Lane(target) => lane_header(ui, session, layout, row, target, rect),
            }
        }
        // Adding tracks, under the list.
        let top = layout.rows_bottom();
        let rect = Rect::from_min_max(
            pos2(layout.headers.left(), top),
            pos2(layout.headers.right() - 1.0, top + ADD_ROW_HEIGHT),
        );
        if rect.top() < layout.headers.bottom() {
            let mut child = ui.new_child(
                UiBuilder::new()
                    .max_rect(rect.shrink2(vec2(12.0, 10.0)))
                    .layout(EguiLayout::left_to_right(Align::Min).with_main_wrap(true)),
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
            if child
                .button(format!("+ {}", text::ADD_AUDIO))
                .on_hover_text(text::ADD_AUDIO_HINT)
                .clicked()
            {
                session.add_audio_track();
            }
            if child
                .button(format!("+ {}", text::ADD_INSTRUMENT))
                .on_hover_text(text::ADD_INSTRUMENT_HINT)
                .clicked()
            {
                intents.push(Intent::ChooseInstrument);
            }
        }
    }

    /// One track's header: name, mute, solo and a menu; what it plays; its
    /// plugin's window and its lanes.
    #[allow(clippy::too_many_lines, reason = "one header, top to bottom")]
    fn track_header(
        &mut self,
        ui: &mut Ui,
        session: &mut Session,
        layout: &Layout,
        row: &Row,
        rect: Rect,
        intents: &mut Vec<Intent>,
    ) {
        let p = Palette::of(ui.ctx());
        let track = row.track;
        let Some(t) = session.project().track(track) else {
            return;
        };
        let (name, instrument, mute, solo) = (
            t.name.clone(),
            t.instrument.clone(),
            t.strip.mute,
            t.strip.solo,
        );
        let count = session.project().tracks.len();
        let focused = track == session.focused_track();
        let last_instrument = session.is_last_instrument(track);
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
            track_color(row.index),
        );
        painter.hline(
            rect.x_range(),
            rect.bottom() - 0.5,
            Stroke::new(1.0, p.divider),
        );
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(rect.shrink2(vec2(14.0, 7.0)))
                .layout(EguiLayout::top_down(Align::Min)),
        );
        child.set_clip_rect(layout.headers);
        child.spacing_mut().item_spacing.y = 3.0;
        child.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.with_layout(EguiLayout::right_to_left(Align::Center), |ui| {
                let menu = icon_button(ui, Icon::Menu, text::TRACK_MENU, "", Look::Normal, 24.0);
                egui::Popup::menu(&menu).show(|ui| {
                    ui.set_min_width(180.0);
                    if ui.button(text::RENAME_PATTERN).clicked() {
                        intents.push(Intent::RenameTrack(track));
                        ui.close();
                    }
                    if ui
                        .add_enabled(row.index > 0, Button::new(text::MOVE_UP))
                        .clicked()
                    {
                        session.move_track(track, -1);
                        ui.close();
                    }
                    if ui
                        .add_enabled(row.index + 1 < count, Button::new(text::MOVE_DOWN))
                        .clicked()
                    {
                        session.move_track(track, 1);
                        ui.close();
                    }
                    ui.separator();
                    if ui
                        .add_enabled(
                            count > 1 && !last_instrument,
                            Button::new(text::DELETE_TRACK),
                        )
                        .on_disabled_hover_text(text::LAST_INSTRUMENT)
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
                ui.with_layout(EguiLayout::left_to_right(Align::Center), |ui| {
                    ui.add(Label::new(RichText::new(&name).strong().size(size::BODY)).truncate());
                });
            });
        });
        let plugin = session.track_plugin(track);
        let missing = plugin.is_some_and(|target| session.is_plugin_missing(target));
        let kind = match &instrument {
            Instrument::Drums { kit, .. } => text::kit_description(text::kit_name(kit)),
            Instrument::SoundFont { .. } => text::FIRST_TRACK.to_owned(),
            Instrument::Plugin(setup) => text::plugin_track(&setup.plugin.name, missing),
            Instrument::Audio => text::AUDIO_TRACK_KIND.to_owned(),
            Instrument::Synth(settings) => text::synth_description(settings),
        };
        child.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.with_layout(EguiLayout::right_to_left(Align::Center), |ui| {
                let lanes = icon_button(
                    ui,
                    Icon::Automation,
                    text::AUTOMATION,
                    text::AUTOMATION_HINT,
                    Look::Normal,
                    20.0,
                );
                self.lane_chooser(&lanes, session, track);
                if let Some(target) = plugin.filter(|&target| {
                    !session.is_plugin_missing(target) && session.has_plugin_window(target)
                }) {
                    plugin_window_button(ui, session, target, 20.0);
                } else if instrument.synth().is_some() {
                    crate::synth_ui::window_button(ui, session, track, 20.0);
                }
                ui.with_layout(EguiLayout::left_to_right(Align::Center), |ui| {
                    let recording_here = session.live_take().is_some_and(|l| l.track == track)
                        || (session.is_recording_audio() && session.focused_track() == track);
                    if recording_here {
                        input_meter(ui, session.input_level());
                    } else {
                        ui.add(
                            Label::new(RichText::new(kind).small().color(p.text_weak)).truncate(),
                        );
                    }
                });
            });
        });
    }

    /// The list of settings a track's new lane can change, under the
    /// Automation button: searchable, the setting last moved in a plugin's
    /// window first.
    fn lane_chooser(&mut self, button: &Response, session: &mut Session, track: TrackId) {
        let shown = egui::Popup::menu(button)
            .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                if self.lane_menu.as_ref().is_none_or(|(t, _)| *t != track) {
                    self.lane_menu = Some((track, session.lane_choices(track)));
                    self.lane_search.clear();
                }
                let Some((_, choices)) = &self.lane_menu else {
                    return;
                };
                ui.set_min_width(280.0);
                ui.label(RichText::new(text::AUTOMATION).strong());
                if choices.len() > 8 {
                    ui.add(
                        TextEdit::singleline(&mut self.lane_search)
                            .desired_width(f32::INFINITY)
                            .hint_text(text::SEARCH_SETTINGS),
                    );
                }
                let needle = self.lane_search.trim().to_lowercase();
                let wanted: Vec<&LaneChoice> = choices
                    .iter()
                    .filter(|c| needle.is_empty() || c.name.to_lowercase().contains(&needle))
                    .collect();
                if wanted.is_empty() {
                    ui.label(RichText::new(text::NOTHING_MATCHES).weak());
                }
                let mut chosen = None;
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .auto_shrink([false, true])
                    .show_rows(ui, 24.0, wanted.len(), |ui, range| {
                        for choice in &wanted[range] {
                            let mut button = Button::new(&choice.name)
                                .min_size(vec2(ui.available_width(), 22.0));
                            if choice.last_moved {
                                button = button.shortcut_text(text::LAST_MOVED);
                            }
                            if ui.add(button).clicked() {
                                chosen = Some(choice.target);
                            }
                        }
                    });
                if let Some(target) = chosen {
                    session.add_lane(track, target);
                    ui.close();
                }
            });
        if shown.is_none() && self.lane_menu.as_ref().is_some_and(|(t, _)| *t == track) {
            self.lane_menu = None;
        }
    }

    /// The right-click menu of a clip.
    fn clip_menu(&mut self, ui: &Ui, session: &mut Session) {
        let Some(clip) = self.menu_clip else {
            return;
        };
        let Some((track, c)) = session.project().clip(clip).map(|(t, c)| (t, *c)) else {
            self.menu_clip = None;
            return;
        };
        let split_at = self.menu_split;
        let mut close = false;
        let area = egui::Area::new(ui.scope_id().with("clip-menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(self.menu_at)
            .show(ui.ctx(), |ui| {
                egui::Frame::menu(ui.style()).show(ui, |ui| {
                    ui.set_min_width(190.0);
                    if let Some(pattern) = c.pattern()
                        && ui.button(text::OPEN_PATTERN).clicked()
                    {
                        session.open_pattern(track, pattern);
                        close = true;
                    }
                    if ui
                        .add_enabled(
                            split_at > c.start && split_at < c.end(),
                            Button::new(text::SPLIT_HERE),
                        )
                        .clicked()
                    {
                        session.split_clip(clip, split_at);
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
                    if let Some(gain) = session.clip_gain(clip) {
                        ui.separator();
                        let mut db = crate::widgets::gain_to_db(gain);
                        let slider = ui
                            .add(
                                Slider::new(&mut db, -60.0..=12.0)
                                    .suffix(" dB")
                                    .fixed_decimals(1)
                                    .text(text::CLIP_VOLUME),
                            )
                            .on_hover_text(text::CLIP_VOLUME_HINT);
                        if slider.drag_started() {
                            session.begin_gesture(text::EDIT_CLIP_VOLUME);
                        }
                        if slider.changed() {
                            session.set_clip_gain(clip, crate::widgets::db_to_gain(db));
                        }
                        if slider.drag_stopped() {
                            session.end_gesture();
                        }
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

/// A lane's header: what it changes, its value where the song is, and a
/// button that removes it.
fn lane_header(
    ui: &mut Ui,
    session: &mut Session,
    layout: &Layout,
    row: &Row,
    target: AutomationTarget,
    rect: Rect,
) {
    let p = Palette::of(ui.ctx());
    let painter = ui.painter_at(layout.headers);
    painter.rect_filled(rect, CornerRadius::ZERO, p.background);
    painter.rect_filled(
        Rect::from_min_max(
            rect.left_top() + vec2(0.0, 4.0),
            pos2(rect.left() + 4.0, rect.bottom() - 4.0),
        ),
        CornerRadius::same(2),
        track_color(row.index).gamma_multiply(0.6),
    );
    painter.hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1.0, p.divider),
    );
    let name = session.lane_name(row.track, target);
    let value = session
        .project()
        .lane(row.track, target)
        .and_then(|lane| {
            #[allow(clippy::cast_precision_loss, reason = "tick positions fit easily")]
            lane.value_at(session.song_position().0 as f64)
        })
        .map_or_default(|value| session.lane_value_text(row.track, target, value));
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(vec2(14.0, 6.0)))
            .layout(EguiLayout::right_to_left(Align::Min)),
    );
    child.set_clip_rect(layout.headers);
    child.spacing_mut().item_spacing.x = 4.0;
    if icon_button(
        &mut child,
        Icon::Minus,
        text::REMOVE_LANE,
        "",
        Look::Normal,
        20.0,
    )
    .clicked()
    {
        session.remove_lane(row.track, target);
    }
    child.with_layout(EguiLayout::top_down(Align::Min), |ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        ui.add(Label::new(RichText::new(name).small().color(p.text)).truncate())
            .on_hover_text(text::LANE_HINT);
        ui.add(Label::new(RichText::new(value).small().monospace().color(p.text_weak)).truncate());
    });
}

/// The input's level while recording: a bar on a decibel scale, red when
/// it reaches full scale.
fn input_meter(ui: &mut Ui, level: f32) {
    let p = Palette::of(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(vec2(110.0, 8.0), Sense::hover());
    response.on_hover_text(text::INPUT_LEVEL_HINT);
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(3), p.grid);
    let db = if level > 0.0 {
        20.0 * level.log10()
    } else {
        -60.0
    };
    let fraction = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
    let color = if level >= 0.99 { p.record } else { p.accent };
    painter.rect_filled(
        Rect::from_min_max(
            rect.min,
            pos2(rect.left() + rect.width() * fraction, rect.bottom()),
        ),
        CornerRadius::same(3),
        color,
    );
}

/// The button that opens (or closes) a plugin's own window.
fn plugin_window_button(ui: &mut Ui, session: &mut Session, target: PluginTarget, size: f32) {
    let open = session.is_plugin_window_open(target);
    let label = if open {
        text::CLOSE_PLUGIN
    } else {
        text::OPEN_PLUGIN
    };
    let hint = session.plugin_name(target);
    if icon_button(ui, Icon::Window, label, &hint, Look::on(open), size).clicked() {
        session.toggle_plugin_window(target);
    }
}

/// Whether a clip, moved to `start` (and given `length`), would overlap
/// another clip on its track.
fn overlaps(session: &Session, clip: ClipId, start: Ticks, length: Option<Ticks>) -> bool {
    let Some((track, _)) = session.project().clip(clip) else {
        return true;
    };
    overlaps_on(session, track, clip, Some(start), length)
}

/// Whether a clip, on `track` at `start` (its own start if `None`) and
/// lasting `length` (its own if `None`), would overlap another clip there.
fn overlaps_on(
    session: &Session,
    track: TrackId,
    clip: ClipId,
    start: Option<Ticks>,
    length: Option<Ticks>,
) -> bool {
    let Some((_, current)) = session.project().clip(clip) else {
        return true;
    };
    let start = start.unwrap_or(current.start);
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
