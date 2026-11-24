// SPDX-License-Identifier: GPL-3.0-or-later

//! The play surface at the bottom of the window: an on-screen piano that
//! shows which computer key plays each note, lights up what is being
//! played, and can be played with the mouse. Its header carries the octave
//! and velocity of the computer keyboard, and the status line.

use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Layout, Pos2, Rect, RichText, Sense, Stroke, Ui,
    pos2, vec2,
};

use crate::icons::{Icon, Look, icon_button};
use crate::keys::{Command, KeyLabels, Keymap, NOTE_KEYS};
use crate::piano_roll::is_black_key;
use crate::session::Session;
use crate::text;
use crate::theme::{Palette, size};

/// How many keys the on-screen keyboard shows: three octaves and a C.
const SHOWN_KEYS: u8 = 37;
/// Height of the keys.
pub const KEYS_HEIGHT: f32 = 96.0;

/// The header line: status on the left, keyboard settings on the right.
/// While the program waits for a MIDI control (decision 0033), that comes
/// first, whatever else there is to say.
pub fn header(
    ui: &mut Ui,
    session: &mut Session,
    keymap: &Keymap,
    notice: Option<&str>,
    actions: &mut Vec<Command>,
) {
    let p = Palette::of(ui.ctx());
    ui.horizontal_centered(|ui| {
        if session.learning_midi().is_some() {
            ui.label(
                RichText::new(text::WAITING_FOR_KNOB)
                    .color(p.accent)
                    .strong(),
            );
        } else if let Some(notice) = notice {
            ui.label(RichText::new(notice).color(p.record));
        } else if let Some(status) = session.status() {
            ui.label(RichText::new(status).color(p.text_weak));
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let mut velocity = session.keyboard_velocity();
            let slider = ui.add(egui::Slider::new(&mut velocity, 0.05..=1.0).show_value(false));
            if slider.changed() {
                session.set_keyboard_velocity(velocity);
            }
            let label = ui.label(RichText::new(text::VELOCITY).weak());
            slider.labelled_by(label.id);
            ui.add_space(16.0);
            let up = icon_button(
                ui,
                Icon::Plus,
                text::OCTAVE_UP,
                &keymap.text(Command::KeyboardOctaveUp),
                Look::Normal,
                26.0,
            );
            if up.clicked() {
                actions.push(Command::KeyboardOctaveUp);
            }
            let low = session.keyboard_base();
            let high = low.saturating_add(crate::keys::HIGHEST_OFFSET);
            ui.label(RichText::new(text::keyboard_range(low, high)).monospace())
                .on_hover_text(text::KEYBOARD_HINT);
            let down = icon_button(
                ui,
                Icon::Minus,
                text::OCTAVE_DOWN,
                &keymap.text(Command::KeyboardOctaveDown),
                Look::Normal,
                26.0,
            );
            if down.clicked() {
                actions.push(Command::KeyboardOctaveDown);
            }
            ui.label(RichText::new(text::KEYBOARD).weak())
                .on_hover_text(text::KEYBOARD_HINT);
        });
    });
}

/// The on-screen keyboard's own state.
#[derive(Debug, Default)]
pub struct KeyboardPanel {
    /// Whether the mouse is playing a key here (so only this panel releases
    /// it).
    playing: bool,
}

impl KeyboardPanel {
    /// Draws the keys and plays them with the mouse.
    pub fn keys(&mut self, ui: &mut Ui, session: &mut Session, labels: &KeyLabels) {
        let (rect, response) = ui.allocate_exact_size(
            vec2(ui.available_width(), KEYS_HEIGHT),
            Sense::click_and_drag(),
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Canvas, true, text::ON_SCREEN_KEYBOARD)
        });
        let low = first_shown(session);
        let layout = KeyLayout::new(rect, low);
        let (pos, down) = ui.input(|i| (i.pointer.interact_pos(), i.pointer.primary_down()));
        if down && (response.is_pointer_button_down_on() || self.playing) {
            if let Some(key) = pos.and_then(|pos| layout.key_at(pos)) {
                session.pointer_on(key, session.keyboard_velocity());
                self.playing = true;
            }
        } else if self.playing {
            session.pointer_off();
            self.playing = false;
        }
        paint_keys(ui, session, labels, rect, &layout);
    }
}

fn paint_keys(ui: &Ui, session: &Session, labels: &KeyLabels, rect: Rect, layout: &KeyLayout) {
    let low = layout.low;
    let p = Palette::of(ui.ctx());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::ZERO, p.background);
    // White keys first, then black keys on top.
    for black in [false, true] {
        for key in low..low.saturating_add(SHOWN_KEYS).min(128) {
            if is_black_key(key) != black {
                continue;
            }
            let body = layout.key_rect(key);
            let lit = session.is_key_lit(key);
            let fill = match (lit, black) {
                (true, _) => p.accent,
                (false, true) => p.key_black,
                (false, false) => p.key_white,
            };
            painter.rect(
                body,
                CornerRadius {
                    nw: 0,
                    ne: 0,
                    sw: 4,
                    se: 4,
                },
                fill,
                Stroke::new(1.0, p.note_edge.gamma_multiply(0.5)),
                egui::StrokeKind::Inside,
            );
            let ink = if lit {
                p.on_accent
            } else if black {
                Color32::from_gray(220)
            } else {
                Color32::from_gray(70)
            };
            paint_labels(&painter, session, labels, key, body, ink);
        }
    }
}

/// The lowest key shown: the C at or below the computer keyboard's base.
fn first_shown(session: &Session) -> u8 {
    let base = session.keyboard_base();
    base.min(127 - SHOWN_KEYS + 1) / 12 * 12
}

/// Draws, on a key, the computer keys that play it.
fn paint_labels(
    painter: &egui::Painter,
    session: &Session,
    labels: &KeyLabels,
    key: u8,
    body: Rect,
    ink: Color32,
) {
    let Some(offset) = session.computer_key_for(key) else {
        if key.is_multiple_of(12) && !is_black_key(key) {
            painter.text(
                body.center_bottom() - vec2(0.0, 6.0),
                Align2::CENTER_BOTTOM,
                text::note_name(key),
                FontId::proportional(size::SMALL - 1.5),
                ink.gamma_multiply(0.6),
            );
        }
        return;
    };
    let names: Vec<String> = NOTE_KEYS
        .iter()
        .filter(|(_, o)| *o == offset)
        .map(|(physical, _)| labels.label(*physical))
        .collect();
    let font = FontId::proportional(if is_black_key(key) {
        size::SMALL - 1.0
    } else {
        size::SMALL
    });
    let mut y = body.bottom() - 8.0;
    for name in names.iter().rev() {
        painter.text(
            pos2(body.center().x, y),
            Align2::CENTER_BOTTOM,
            name,
            font.clone(),
            ink,
        );
        y -= size::SMALL + 3.0;
    }
    if key.is_multiple_of(12) && !is_black_key(key) {
        painter.text(
            pos2(body.center().x, body.top() + 6.0),
            Align2::CENTER_TOP,
            text::note_name(key),
            FontId::proportional(size::SMALL - 2.0),
            ink.gamma_multiply(0.6),
        );
    }
}

/// Where each key is drawn.
struct KeyLayout {
    rect: Rect,
    low: u8,
    white_width: f32,
}

impl KeyLayout {
    fn new(rect: Rect, low: u8) -> Self {
        let whites = (low..low.saturating_add(SHOWN_KEYS).min(128))
            .filter(|k| !is_black_key(*k))
            .count();
        #[allow(clippy::cast_precision_loss, reason = "a small count")]
        let white_width = (rect.width() / whites.max(1) as f32).min(64.0);
        Self {
            rect,
            low,
            white_width,
        }
    }

    fn whites_before(&self, key: u8) -> usize {
        (self.low..key).filter(|k| !is_black_key(*k)).count()
    }

    fn left(&self) -> f32 {
        let whites = (self.low..self.low.saturating_add(SHOWN_KEYS).min(128))
            .filter(|k| !is_black_key(*k))
            .count();
        #[allow(clippy::cast_precision_loss, reason = "a small count")]
        let used = self.white_width * whites as f32;
        self.rect.center().x - used / 2.0
    }

    fn key_rect(&self, key: u8) -> Rect {
        #[allow(clippy::cast_precision_loss, reason = "a small count")]
        let x = self.left() + self.white_width * self.whites_before(key) as f32;
        if is_black_key(key) {
            let width = self.white_width * 0.62;
            Rect::from_min_size(
                pos2(x - width / 2.0, self.rect.top()),
                vec2(width, self.rect.height() * 0.6),
            )
        } else {
            Rect::from_min_size(
                pos2(x, self.rect.top()),
                vec2(self.white_width, self.rect.height()),
            )
        }
    }

    fn key_at(&self, pos: Pos2) -> Option<u8> {
        let keys = self.low..self.low.saturating_add(SHOWN_KEYS).min(128);
        keys.clone()
            .filter(|k| is_black_key(*k))
            .find(|k| self.key_rect(*k).contains(pos))
            .or_else(|| {
                keys.filter(|k| !is_black_key(*k))
                    .find(|k| self.key_rect(*k).contains(pos))
            })
    }
}
