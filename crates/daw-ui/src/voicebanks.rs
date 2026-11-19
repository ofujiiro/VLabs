// SPDX-License-Identifier: GPL-3.0-or-later

//! The Voicebanks window (decision 0027): the classic UTAU voicebanks
//! installed in the program's own folder, each with its creator's terms.
//! Add one from a folder or a `.zip` (copied on a thread of its own, so the
//! window stays responsive), read its terms, remove it.

use std::path::{Path, PathBuf};
use std::sync::mpsc;

use daw_formats::utau::{self, VoicebankInfo};
use egui::{Button, RichText, Ui};

use crate::session::Session;
use crate::text;
use crate::theme::Palette;
use crate::workspace::Host;

/// What the window needs from around it.
pub(crate) struct Parts<'a> {
    /// The program's own voicebanks folder (none: voicebanks cannot be
    /// added on this computer).
    pub root: Option<&'a Path>,
    /// The voicebanks installed there.
    pub list: &'a mut Vec<VoicebankInfo>,
    pub host: &'a mut dyn Host,
    pub session: &'a mut Session,
}

/// The window's state between frames.
#[derive(Debug, Default)]
pub(crate) struct VoicebanksWindow {
    selected: Option<PathBuf>,
    adding: Option<mpsc::Receiver<Result<VoicebankInfo, String>>>,
    message: Option<String>,
    /// The voicebank just added: its terms are shown with a reminder.
    fresh: Option<PathBuf>,
}

/// The name of a voicebank's folder (what songs name it by).
pub(crate) fn folder_name(info: &VoicebankInfo) -> String {
    info.folder
        .file_name()
        .map_or_else(|| info.name.clone(), |n| n.to_string_lossy().into_owned())
}

impl VoicebanksWindow {
    /// Whether a voicebank is being added.
    pub fn is_busy(&self) -> bool {
        self.adding.is_some()
    }

    fn start_adding(&mut self, source: PathBuf, root: &Path) {
        let (sender, receiver) = mpsc::channel();
        let root = root.to_path_buf();
        let started = std::thread::Builder::new()
            .name("voicebank-adder".to_owned())
            .spawn(move || {
                let _ = sender.send(utau::install(&source, &root));
            });
        if started.is_ok() {
            self.adding = Some(receiver);
            self.message = Some(text::ADDING_VOICEBANK.to_owned());
        }
    }

    /// Takes the result of adding a voicebank, when it is there.
    fn take_added(&mut self, parts: &mut Parts<'_>) {
        let Some(receiver) = &self.adding else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err(text::ADDING_STOPPED.to_owned()),
        };
        self.adding = None;
        match result {
            Ok(info) => {
                if let Some(root) = parts.root {
                    *parts.list = utau::installed(root);
                }
                parts.session.forget_voicebank(&folder_name(&info));
                self.message = Some(text::voicebank_added(&info.name));
                self.selected = Some(info.folder.clone());
                self.fresh = Some(info.folder);
            }
            Err(reason) => self.message = Some(text::could_not_add_voicebank(&reason)),
        }
    }

    /// The selected voicebank: its creator's terms, and its Remove button.
    fn selected_voicebank(&mut self, ui: &mut Ui, parts: &mut Parts<'_>, root: &Path) {
        let p = Palette::of(ui.ctx());
        let selected = self
            .selected
            .as_ref()
            .and_then(|folder| parts.list.iter().find(|i| &i.folder == folder))
            .cloned();
        if let Some(info) = selected {
            ui.add_space(8.0);
            ui.label(RichText::new(text::voicebank_terms_title(&info.name)).strong());
            if self.fresh.as_ref() == Some(&info.folder) {
                ui.label(RichText::new(text::READ_THE_TERMS).color(p.accent));
            }
            egui::ScrollArea::vertical()
                .id_salt("voicebank-terms")
                .max_height(180.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    if info.terms.trim().is_empty() {
                        ui.label(RichText::new(text::NO_TERMS).weak());
                    } else {
                        ui.label(RichText::new(info.terms.trim()).small());
                    }
                });
            if ui
                .add_enabled(self.adding.is_none(), Button::new(text::REMOVE_VOICEBANK))
                .clicked()
            {
                match utau::remove(&info.folder, root) {
                    Ok(()) => {
                        parts.session.forget_voicebank(&folder_name(&info));
                        self.message = Some(text::voicebank_removed(&info.name));
                    }
                    Err(error) => {
                        self.message = Some(text::could_not_remove_voicebank(&error.to_string()));
                    }
                }
                *parts.list = utau::installed(root);
                self.selected = None;
                self.fresh = None;
            }
        }
    }

    /// Draws the window's contents; returns whether to close it.
    pub fn contents(&mut self, ui: &mut Ui, parts: &mut Parts<'_>) -> bool {
        self.take_added(parts);
        if self.adding.is_some() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        let mut close = false;
        ui.set_width(560.0);
        ui.heading(text::VOICEBANKS_TITLE);
        ui.label(RichText::new(text::VOICEBANKS_INTRO).weak());
        ui.add_space(8.0);
        let Some(root) = parts.root else {
            ui.label(text::VOICEBANKS_NO_FOLDER);
            ui.add_space(8.0);
            return ui.button(text::CLOSE).clicked();
        };
        ui.horizontal(|ui| {
            let idle = self.adding.is_none();
            if ui
                .add_enabled(idle, Button::new(text::ADD_VOICEBANK_FOLDER))
                .on_hover_text(text::ADD_VOICEBANK_FOLDER_HINT)
                .clicked()
                && let Some(folder) = parts.host.choose_folder(text::CHOOSE_VOICEBANK_FOLDER)
            {
                self.start_adding(folder, root);
            }
            if ui
                .add_enabled(idle, Button::new(text::ADD_VOICEBANK_ZIP))
                .on_hover_text(text::ADD_VOICEBANK_ZIP_HINT)
                .clicked()
                && let Some(archive) = parts.host.choose_voicebank_zip()
            {
                self.start_adding(archive, root);
            }
            if !idle {
                ui.spinner();
            }
        });
        if let Some(message) = &self.message {
            ui.label(RichText::new(message).small());
        }
        ui.add_space(8.0);
        egui::ScrollArea::vertical()
            .id_salt("voicebank-list")
            .max_height(170.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                if parts.list.is_empty() {
                    ui.label(RichText::new(text::NO_VOICEBANKS).weak());
                }
                for info in parts.list.iter() {
                    let chosen = self.selected.as_ref() == Some(&info.folder);
                    let detail = text::voicebank_detail(info.author.as_deref(), info.syllables);
                    let row = ui.add(
                        Button::selectable(chosen, RichText::new(&info.name).strong())
                            .shortcut_text(detail)
                            .min_size(egui::vec2(ui.available_width(), 28.0)),
                    );
                    if row.clicked() {
                        self.selected = Some(info.folder.clone());
                    }
                }
            });
        self.selected_voicebank(ui, parts, root);
        ui.add_space(10.0);
        if ui
            .add_enabled(self.adding.is_none(), Button::new(text::CLOSE))
            .clicked()
        {
            close = true;
        }
        close
    }
}
