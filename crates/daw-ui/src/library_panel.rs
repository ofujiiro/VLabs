// SPDX-License-Identifier: GPL-3.0-or-later

//! The sound library (decision 0023), in place of the keyboard: the
//! person's folders of sounds, MIDI files and SoundFonts, listed in the
//! background however many files they hold, browsed by folder or searched
//! all at once. A click plays a sound; dragging an item onto the song, or
//! double-clicking it, puts it in the song.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};

use daw_formats::library::{ItemKind, Listing};
use egui::{Align, Align2, CornerRadius, FontId, Layout, RichText, Sense, Ui, vec2};

use crate::icons::{self, Icon};
use crate::session::Session;
use crate::settings::Settings;
use crate::text;
use crate::theme::{Palette, size};
use crate::workspace::Host;

/// The library's height, under the header.
pub const LIBRARY_HEIGHT: f32 = 248.0;
/// Width of the column of folders.
const FOLDERS_WIDTH: f32 = 230.0;
/// Height of a row.
const ROW: f32 = 24.0;
/// The most search results listed.
const MAX_RESULTS: usize = 2_000;

/// Library files being dragged (onto the song view).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryDrag(pub Vec<PathBuf>);

/// Something the person asked the library for, which the workspace does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LibraryAction {
    /// Put a file in the song at the play position.
    Add(PathBuf),
}

/// A library folder and its listing.
struct Root {
    path: PathBuf,
    listing: Option<Arc<Listing>>,
    loading: Option<(mpsc::Receiver<Listing>, Arc<AtomicBool>)>,
}

impl Root {
    fn new(path: PathBuf) -> Self {
        let mut root = Self {
            path,
            listing: None,
            loading: None,
        };
        root.list();
        root
    }

    /// Lists the folder (again) in the background.
    fn list(&mut self) {
        self.stop();
        let stop = Arc::new(AtomicBool::new(false));
        let (send, receive) = mpsc::channel();
        let path = self.path.clone();
        let flag = Arc::clone(&stop);
        let _ = std::thread::Builder::new()
            .name("library".to_owned())
            .spawn(move || {
                let _ = send.send(daw_formats::library::list(&path, &flag));
            });
        self.loading = Some((receive, stop));
    }

    /// Takes a finished listing. Returns whether one arrived.
    fn poll(&mut self) -> bool {
        let arrived = self
            .loading
            .as_ref()
            .and_then(|(receive, _)| receive.try_recv().ok());
        match arrived {
            Some(listing) => {
                self.listing = Some(Arc::new(listing));
                self.loading = None;
                true
            }
            None => false,
        }
    }

    fn stop(&mut self) {
        if let Some((_, stop)) = self.loading.take() {
            stop.store(true, Ordering::Relaxed);
        }
    }

    fn name(&self) -> String {
        self.path.file_name().map_or_else(
            || self.path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        )
    }

    /// A file of the listing, as a path on the disk.
    fn path_of(&self, relative: &str) -> PathBuf {
        relative
            .split('/')
            .fold(self.path.clone(), |path, part| path.join(part))
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        self.stop();
    }
}

/// One row of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Row {
    /// A folder inside the one shown, by name.
    Folder(String),
    /// A file: which library folder, and where in its listing.
    Item { root: usize, index: usize },
}

/// What the rows were worked out from.
#[derive(Clone, Debug, PartialEq, Eq)]
struct RowsKey {
    current: usize,
    folder: String,
    search: String,
    generation: usize,
}

/// The library's state.
#[derive(Default)]
pub(crate) struct LibraryPanel {
    roots: Vec<Root>,
    /// The library folder open.
    current: usize,
    /// The folder shown inside it ("" for its top; parts separated by `/`).
    folder: String,
    search: String,
    rows: Vec<Row>,
    rows_key: Option<RowsKey>,
    /// How many files the search found in all.
    found: usize,
    /// Changes whenever a listing arrives.
    generation: usize,
    /// The file last clicked.
    selected: Option<PathBuf>,
}

impl LibraryPanel {
    /// Whether a folder is still being listed (to draw again soon).
    pub fn is_listing(&self) -> bool {
        self.roots.iter().any(|root| root.loading.is_some())
    }

    /// Draws the library and handles what the person does in it.
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        session: &mut Session,
        settings: &mut Settings,
        host: &mut dyn Host,
        actions: &mut Vec<LibraryAction>,
    ) {
        self.follow_settings(settings);
        for root in &mut self.roots {
            if root.poll() {
                self.generation += 1;
            }
        }
        let height = LIBRARY_HEIGHT - 8.0;
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                vec2(FOLDERS_WIDTH, height),
                Layout::top_down(Align::Min),
                |ui| self.folders_column(ui, settings, host),
            );
            ui.separator();
            ui.allocate_ui_with_layout(
                vec2(ui.available_width(), height),
                Layout::top_down(Align::Min),
                |ui| self.files_column(ui, session, actions),
            );
        });
    }

    /// Keeps the library folders in step with the settings.
    fn follow_settings(&mut self, settings: &Settings) {
        let wanted = &settings.sound_folders;
        if self.roots.len() == wanted.len()
            && self.roots.iter().zip(wanted).all(|(r, w)| r.path == *w)
        {
            return;
        }
        let mut old: Vec<Root> = std::mem::take(&mut self.roots);
        for path in wanted {
            match old.iter().position(|r| r.path == *path) {
                Some(index) => self.roots.push(old.remove(index)),
                None => self.roots.push(Root::new(path.clone())),
            }
        }
        if self.current >= self.roots.len() {
            self.current = self.roots.len().saturating_sub(1);
            self.folder.clear();
        }
        self.generation += 1;
    }

    /// The library folders, with how many files each holds.
    fn folders_column(&mut self, ui: &mut Ui, settings: &mut Settings, host: &mut dyn Host) {
        let p = Palette::of(ui.ctx());
        egui::ScrollArea::vertical()
            .id_salt("library-folders")
            .max_height(LIBRARY_HEIGHT - 48.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                let mut removed = None;
                for (index, root) in self.roots.iter_mut().enumerate() {
                    let count = match &root.listing {
                        Some(listing) => {
                            text::library_count(listing.items.len(), listing.truncated)
                        }
                        None => text::LIBRARY_LISTING.to_owned(),
                    };
                    let response = ui
                        .add_sized(
                            [FOLDERS_WIDTH - 8.0, ROW + 4.0],
                            egui::Button::new(root.name())
                                .selected(self.current == index)
                                .shortcut_text(RichText::new(count).small().color(p.text_weak)),
                        )
                        .on_hover_text(root.path.display().to_string());
                    if response.clicked() {
                        self.current = index;
                        self.folder.clear();
                        self.search.clear();
                    }
                    response.context_menu(|ui| {
                        if ui
                            .button(text::LIBRARY_REFRESH)
                            .on_hover_text(text::LIBRARY_REFRESH_HINT)
                            .clicked()
                        {
                            root.list();
                            ui.close();
                        }
                        if ui
                            .button(text::REMOVE_SOUND_FOLDER)
                            .on_hover_text(text::REMOVE_SOUND_FOLDER_HINT)
                            .clicked()
                        {
                            removed = Some(index);
                            ui.close();
                        }
                    });
                }
                if let Some(index) = removed {
                    settings.sound_folders.remove(index);
                }
            });
        ui.add_space(4.0);
        if ui
            .button(text::ADD_SOUND_FOLDER)
            .on_hover_text(text::ADD_SOUND_FOLDER_HINT)
            .clicked()
            && let Some(folder) = host.choose_folder(text::ADD_SOUND_FOLDER)
        {
            add_folder(settings, folder);
            self.current = settings.sound_folders.len() - 1;
            self.folder.clear();
        }
    }

    /// The files of the folder shown, or the search's results.
    fn files_column(
        &mut self,
        ui: &mut Ui,
        session: &mut Session,
        actions: &mut Vec<LibraryAction>,
    ) {
        let p = Palette::of(ui.ctx());
        if self.roots.is_empty() {
            ui.add_space(24.0);
            ui.label(RichText::new(text::LIBRARY_EMPTY).color(p.text_weak));
            return;
        }
        ui.horizontal(|ui| {
            let searching = !self.search.trim().is_empty();
            if !self.folder.is_empty()
                && !searching
                && ui
                    .small_button(text::LIBRARY_UP)
                    .on_hover_text(text::LIBRARY_UP_HINT)
                    .clicked()
            {
                self.go_up();
            }
            let place = if searching {
                text::LIBRARY.to_owned()
            } else {
                let root = self.roots[self.current].name();
                std::iter::once(root.as_str())
                    .chain(self.folder.split('/').filter(|s| !s.is_empty()))
                    .collect::<Vec<_>>()
                    .join(" › ")
            };
            ui.add(egui::Label::new(RichText::new(place).strong()).truncate());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .desired_width(220.0)
                        .hint_text(text::SEARCH),
                );
            });
        });
        ui.label(RichText::new(text::LIBRARY_HINT).small().color(p.text_weak));
        self.refresh_rows();
        let root_loading = self.roots[self.current].listing.is_none();
        if self.rows.is_empty() {
            ui.add_space(12.0);
            let message = if root_loading && self.search.trim().is_empty() {
                text::LIBRARY_LISTING
            } else if self.search.trim().is_empty() {
                text::LIBRARY_NOTHING_HERE
            } else {
                text::NOTHING_MATCHES
            };
            ui.label(RichText::new(message).color(p.text_weak));
            return;
        }
        let rows = self.rows.clone();
        let more = (self.found > MAX_RESULTS).then_some(self.found);
        let list_height = ui.available_height() - if more.is_some() { 20.0 } else { 0.0 };
        egui::ScrollArea::vertical()
            .id_salt("library-files")
            .max_height(list_height.max(ROW))
            .auto_shrink([false, false])
            .show_rows(ui, ROW, rows.len(), |ui, range| {
                for row in &rows[range] {
                    self.row(ui, row, session, actions);
                }
            });
        if let Some(total) = more {
            ui.label(
                RichText::new(text::library_more_found(MAX_RESULTS, total))
                    .small()
                    .color(p.text_weak),
            );
        }
    }

    /// Works the rows out again when what they show has changed.
    fn refresh_rows(&mut self) {
        let key = RowsKey {
            current: self.current,
            folder: self.folder.clone(),
            search: self.search.trim().to_owned(),
            generation: self.generation,
        };
        if self.rows_key.as_ref() == Some(&key) {
            return;
        }
        self.rows.clear();
        self.found = 0;
        if key.search.is_empty() {
            if let Some(listing) = &self.roots[self.current].listing {
                let inside = listing.inside(&self.folder);
                self.rows
                    .extend(inside.folders.into_iter().map(Row::Folder));
                self.rows
                    .extend(inside.items.into_iter().map(|index| Row::Item {
                        root: self.current,
                        index,
                    }));
            }
        } else {
            for (root_index, root) in self.roots.iter().enumerate() {
                let Some(listing) = &root.listing else {
                    continue;
                };
                let room = MAX_RESULTS.saturating_sub(self.rows.len());
                let (found, total) = listing.search(&key.search, room);
                self.found += total;
                self.rows.extend(found.into_iter().map(|index| Row::Item {
                    root: root_index,
                    index,
                }));
            }
        }
        self.rows_key = Some(key);
    }

    /// How a row looks: its name, the folder it is in (when searching),
    /// its icon, and for a file, its path and kind.
    fn describe(&self, row: &Row) -> Option<Look> {
        let folder_look = |name: String| Look {
            label: name,
            detail: String::new(),
            icon: Icon::Folder,
            file: None,
        };
        match row {
            Row::Folder(name) => Some(folder_look(name.clone())),
            Row::Item { root, index } => {
                let root = &self.roots[*root];
                let item = root.listing.as_ref()?.items.get(*index)?;
                let icon = match item.kind {
                    ItemKind::Sound => Icon::Sound,
                    ItemKind::Midi => Icon::Notes,
                    ItemKind::SoundFont | ItemKind::Sfz => Icon::Keyboard,
                };
                let detail = if self.search.trim().is_empty() {
                    String::new()
                } else if item.folder().is_empty() {
                    root.name()
                } else {
                    format!("{} › {}", root.name(), item.folder().replace('/', " › "))
                };
                Some(Look {
                    label: item.name().to_owned(),
                    detail,
                    icon,
                    file: Some((root.path_of(&item.relative), item.kind)),
                })
            }
        }
    }

    /// One row: an icon and a name; a click opens a folder or plays a
    /// sound, a drag takes a file to the song.
    fn row(
        &mut self,
        ui: &mut Ui,
        row: &Row,
        session: &mut Session,
        actions: &mut Vec<LibraryAction>,
    ) {
        let Some(look) = self.describe(row) else {
            return;
        };
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click_and_drag());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &look.label));
        let path = look.file.as_ref().map(|(path, _)| path.as_path());
        let playing = path.is_some() && session.previewing() == path;
        let selected = path.is_some() && self.selected.as_deref() == path;
        if ui.is_rect_visible(rect) {
            paint_row(
                ui,
                rect,
                &look,
                (selected || response.hovered(), selected, playing),
            );
        }
        match (row, look.file) {
            (Row::Folder(name), _) => {
                let inside = if self.folder.is_empty() {
                    name.clone()
                } else {
                    format!("{}/{name}", self.folder)
                };
                response.context_menu(|ui| {
                    // A custom kit of every sound in it (decision 0025).
                    if ui
                        .button(text::MAKE_KIT)
                        .on_hover_text(text::MAKE_KIT_HINT)
                        .clicked()
                    {
                        let sounds = self.sounds_in(&inside);
                        session.make_kit(name, sounds);
                        ui.close();
                    }
                });
                if response.clicked() {
                    self.folder = inside;
                }
            }
            (Row::Item { .. }, Some((path, kind))) => {
                let sound = kind == ItemKind::Sound;
                if response.double_clicked() {
                    session.stop_preview();
                    actions.push(LibraryAction::Add(path.clone()));
                } else if response.clicked() {
                    self.selected = Some(path.clone());
                    if playing {
                        session.stop_preview();
                    } else if sound {
                        session.preview_file(&path);
                    }
                }
                if response.drag_started() {
                    egui::DragAndDrop::set_payload(ui.ctx(), LibraryDrag(vec![path.clone()]));
                }
                if response.dragged() {
                    follow_pointer(ui, response.id, &look.label);
                }
                response.context_menu(|ui| {
                    if sound {
                        let words = if playing {
                            text::STOP_SOUND
                        } else {
                            text::PLAY_SOUND
                        };
                        if ui.button(words).clicked() {
                            if playing {
                                session.stop_preview();
                            } else {
                                session.preview_file(&path);
                            }
                            ui.close();
                        }
                    }
                    if ui.button(text::ADD_TO_SONG).clicked() {
                        session.stop_preview();
                        actions.push(LibraryAction::Add(path.clone()));
                        ui.close();
                    }
                });
            }
            (Row::Item { .. }, None) => {}
        }
    }

    /// Every sound in a folder of the open library folder, however deep
    /// (at most a full kit's worth).
    fn sounds_in(&self, folder: &str) -> Vec<PathBuf> {
        let Some(root) = self.roots.get(self.current) else {
            return Vec::new();
        };
        let Some(listing) = &root.listing else {
            return Vec::new();
        };
        let prefix = format!("{}/", folder.to_lowercase());
        listing
            .items
            .iter()
            .filter(|item| item.kind == ItemKind::Sound && item.lower.starts_with(&prefix))
            .take(daw_model::KIT_MAX_PADS)
            .map(|item| root.path.join(&item.relative))
            .collect()
    }

    /// Shows the folder above the one shown.
    fn go_up(&mut self) {
        let parent = parent_folder(&self.folder).len();
        self.folder.truncate(parent);
    }
}

/// How a row looks.
struct Look {
    label: String,
    detail: String,
    icon: Icon,
    /// A file's path and kind (none for folders).
    file: Option<(PathBuf, ItemKind)>,
}

/// Draws a row: highlighted (hovered or selected), with its icon (a play
/// mark while it plays), its name and, on the right, its folder.
fn paint_row(ui: &Ui, rect: egui::Rect, look: &Look, (lit, selected, playing): (bool, bool, bool)) {
    let p = Palette::of(ui.ctx());
    let painter = ui.painter_at(rect);
    if lit {
        let fill = if selected {
            p.accent.gamma_multiply(0.22)
        } else {
            p.raised_hover
        };
        painter.rect_filled(rect, CornerRadius::same(3), fill);
    }
    let color = if playing { p.accent } else { p.text };
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 14.0, rect.center().y),
        vec2(18.0, 18.0),
    );
    let icon = if playing { Icon::Play } else { look.icon };
    icons::paint(&painter, icon, icon_rect, color);
    painter.text(
        egui::pos2(rect.left() + 30.0, rect.center().y),
        Align2::LEFT_CENTER,
        &look.label,
        FontId::proportional(size::BODY),
        color,
    );
    if !look.detail.is_empty() {
        painter.text(
            egui::pos2(rect.right() - 8.0, rect.center().y),
            Align2::RIGHT_CENTER,
            &look.detail,
            FontId::proportional(size::SMALL),
            p.text_weak,
        );
    }
}

/// The name of a file being dragged, beside the pointer.
fn follow_pointer(ui: &Ui, id: egui::Id, label: &str) {
    let Some(mouse) = ui.ctx().pointer_latest_pos() else {
        return;
    };
    let p = Palette::of(ui.ctx());
    let layer = ui
        .ctx()
        .layer_painter(egui::LayerId::new(egui::Order::Tooltip, id));
    let galley = layer.layout_no_wrap(label.to_owned(), FontId::proportional(size::BODY), p.text);
    let at = mouse + vec2(16.0, -galley.size().y / 2.0);
    let back = egui::Rect::from_min_size(at, galley.size()).expand2(vec2(8.0, 4.0));
    layer.rect_filled(back, CornerRadius::same(4), p.raised);
    layer.galley(at, galley, p.text);
}

/// Adds a folder to the library's folders (once).
pub(crate) fn add_folder(settings: &mut Settings, folder: PathBuf) {
    if !settings.sound_folders.contains(&folder) {
        settings.sound_folders.push(folder);
    }
}

/// The folder above `folder` ("" above the top's folders).
fn parent_folder(folder: &str) -> &str {
    folder.rsplit_once('/').map_or("", |(parent, _)| parent)
}
