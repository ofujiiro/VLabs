// SPDX-License-Identifier: GPL-3.0-or-later

//! The Plugins window (decision 0023): every plugin found and the files
//! that could not be used; adding plugins from files, folders and
//! downloads; removing the ones added here; the folders searched; and, on
//! Linux, Windows plugins through Wine and yabridge.
//!
//! The window only asks: the program around it copies, removes and
//! installs off the interface thread, then says what happened
//! ([`PluginJobDone`]).

use std::path::{Path, PathBuf};

use daw_engine::{PluginInfo, PluginKind};
use daw_plugin_host::install::Skip;
use egui::{Button, RichText, Ui};

use crate::mixer::format_name;
use crate::session::Session;
use crate::settings::Settings;
use crate::text;
use crate::theme::Palette;
use crate::workspace::{Host, Request};

/// Where plugins come from, as the program around the window knows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PluginPlaces {
    /// The usual folders, always searched.
    pub usual: Vec<PathBuf>,
    /// The program's own plugin folder, where added plugins are kept.
    pub own: Option<PathBuf>,
    /// The plugins added here (which can be removed here), Windows plugins
    /// waiting for yabridge included.
    pub added: Vec<PathBuf>,
    /// Wine and yabridge, as found (Linux only).
    pub bridge: Option<BridgeState>,
    /// Adding, removing or installing is under way.
    pub busy: bool,
}

/// Whether Windows plugins can be used on Linux.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BridgeState {
    /// Wine is installed.
    pub wine: bool,
    /// yabridge is installed.
    pub yabridge: bool,
}

impl BridgeState {
    /// Both are installed.
    pub fn is_ready(self) -> bool {
        self.wine && self.yabridge
    }
}

/// What happened to an installer the person added.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallerRun {
    /// It was opened (Windows): the person finishes it, then looks again.
    Opened,
    /// It ran to the end (through Wine on Linux).
    Finished,
    /// It could not be run: why.
    Failed(String),
    /// Wine is needed to run it (Linux).
    NeedsWine,
}

/// What adding or removing plugins did, for the window to tell.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PluginJobDone {
    /// Plugins put in place.
    pub added: Vec<PathBuf>,
    /// Windows plugins put aside for yabridge.
    pub windows: Vec<PathBuf>,
    /// What was not put in place, and why.
    pub skipped: Vec<(PathBuf, Skip)>,
    /// Installers, and what became of them.
    pub installers: Vec<(PathBuf, InstallerRun)>,
    /// Plugins removed.
    pub removed: Vec<PathBuf>,
    /// yabridge's answer, when it was asked to set plugins up.
    pub bridge: Option<Result<(), String>>,
    /// A plugin that could not be removed, and why.
    pub remove_failed: Option<(PathBuf, String)>,
    /// Wine or yabridge was looked for and is still missing.
    pub bridge_missing: bool,
}

/// Whether a dropped file or folder is for the Plugins window: a plugin, a
/// download that may hold plugins, an installer, or a folder with plugins
/// inside.
pub fn is_for_plugins(path: &Path) -> bool {
    if daw_plugin_host::Format::of_path(path).is_some() {
        return true;
    }
    let ending = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    match ending.as_deref() {
        Some("zip" | "dll" | "exe" | "msi") => true,
        _ => path.is_dir() && daw_plugin_host::install::holds_plugins(path),
    }
}

/// The file to remove for a plugin found at `path`, if it is one the
/// program added: the plugin itself, or for a Windows plugin set up by
/// yabridge, the Windows plugin it stands for.
pub fn removal_target(path: &Path, added: &[PathBuf]) -> Option<PathBuf> {
    if added.iter().any(|a| a == path) {
        return Some(path.to_path_buf());
    }
    let bridged = path.components().any(|c| c.as_os_str() == "yabridge");
    let name = path.file_name()?;
    bridged
        .then(|| {
            added
                .iter()
                .find(|a| a.file_name() == Some(name) && is_windows_plugin(a))
                .cloned()
        })
        .flatten()
}

/// Whether an added plugin is a Windows plugin set aside for yabridge.
fn is_windows_plugin(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == "windows")
}

/// The name of a plugin file, without its ending.
pub(crate) fn file_title(path: &Path) -> String {
    path.file_stem().map_or_else(
        || path.display().to_string(),
        |s| s.to_string_lossy().into_owned(),
    )
}

/// What the window works with, from the workspace.
pub(crate) struct Parts<'a> {
    pub session: &'a Session,
    pub settings: &'a mut Settings,
    pub host: &'a mut dyn Host,
    pub requests: &'a mut Vec<Request>,
    pub places: &'a PluginPlaces,
    pub messages: &'a [String],
}

/// The window's own state: the search text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PluginsWindow {
    pub search: String,
    /// The search field has had the keyboard once (when the window opened).
    focused: bool,
}

/// Width of the window's contents.
const WIDTH: f32 = 640.0;
/// Width of a plugin's name in the list.
const NAME_WIDTH: f32 = 240.0;
/// Width of the maker and format in the list.
const DETAIL_WIDTH: f32 = 260.0;

impl PluginsWindow {
    /// Draws the window's contents. Returns whether it closes.
    pub fn contents(&mut self, ui: &mut Ui, parts: &mut Parts<'_>) -> bool {
        let mut close = false;
        ui.set_width(WIDTH);
        ui.heading(text::PLUGINS_TITLE);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let idle = !parts.places.busy;
            if ui
                .add_enabled(idle, Button::new(text::ADD_PLUGIN_FILES))
                .on_hover_text(text::ADD_PLUGIN_FILES_HINT)
                .clicked()
            {
                let files = parts.host.choose_plugin_files();
                if !files.is_empty() {
                    parts.requests.push(Request::AddPlugins(files));
                }
            }
            if ui
                .add_enabled(idle, Button::new(text::ADD_PLUGIN_FOLDER))
                .on_hover_text(text::ADD_PLUGIN_FOLDER_HINT)
                .clicked()
                && let Some(folder) = parts.host.choose_folder(text::ADD_PLUGIN_FOLDER)
            {
                parts.requests.push(Request::AddPlugins(vec![folder]));
            }
            let search = parts.session.plugin_search();
            if ui
                .add_enabled(search.done && idle, Button::new(text::LOOK_AGAIN))
                .on_hover_text(text::LOOK_AGAIN_HINT)
                .clicked()
            {
                parts.requests.push(Request::FindPlugins);
            }
        });
        ui.label(RichText::new(text::DROP_PLUGINS_HINT).small().weak());
        if parts.places.busy {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(text::PLUGINS_BUSY);
            });
        }
        if !parts.messages.is_empty() {
            ui.add_space(6.0);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(WIDTH - 16.0);
                for line in parts.messages {
                    ui.label(line);
                }
            });
        }
        ui.add_space(8.0);
        let field = ui.add(
            egui::TextEdit::singleline(&mut self.search)
                .desired_width(WIDTH)
                .hint_text(text::SEARCH),
        );
        if !self.focused {
            field.request_focus();
            self.focused = true;
        }
        ui.add_space(6.0);
        // The list and the sections under it scroll together, so the
        // window always fits the screen.
        let room = (ui.ctx().content_rect().height() - 300.0).max(160.0);
        egui::ScrollArea::vertical()
            .id_salt("plugins-window")
            .max_height(room)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                self.list(ui, parts);
                ui.add_space(6.0);
                if let Some(bridge) = parts.places.bridge {
                    windows_plugins(ui, parts, bridge);
                }
                folders(ui, parts);
            });
        ui.add_space(10.0);
        if ui.button(text::CLOSE).clicked() {
            close = true;
        }
        close
    }

    /// The plugins found, instruments then effects, with Remove for the
    /// ones added here; then the search's state and its problems.
    fn list(&self, ui: &mut Ui, parts: &mut Parts<'_>) {
        let needle = self.search.trim().to_lowercase();
        let wanted = |info: &PluginInfo| {
            needle.is_empty()
                || info.name.to_lowercase().contains(&needle)
                || info.vendor.to_lowercase().contains(&needle)
        };
        let instruments = parts.session.plugins_of(PluginKind::Instrument);
        let effects = parts.session.plugins_of(PluginKind::Effect);
        let search = parts.session.plugin_search().clone();
        {
            {
                let mut shown = 0;
                for (heading, plugins) in [
                    (text::INSTRUMENTS_HEADING, &instruments),
                    (text::EFFECTS_HEADING, &effects),
                ] {
                    let matching: Vec<&PluginInfo> =
                        plugins.iter().filter(|info| wanted(info)).collect();
                    if matching.is_empty() {
                        continue;
                    }
                    ui.label(RichText::new(text::counted(heading, matching.len())).weak());
                    for info in matching {
                        shown += 1;
                        plugin_row(ui, info, parts);
                    }
                    ui.add_space(6.0);
                }
                let waiting: Vec<&PathBuf> = parts
                    .places
                    .added
                    .iter()
                    .filter(|p| is_windows_plugin(p))
                    .filter(|p| {
                        !instruments.iter().chain(&effects).any(|info| {
                            removal_target(&info.path, &parts.places.added).as_deref()
                                == Some(p.as_path())
                        })
                    })
                    .collect();
                if !waiting.is_empty() {
                    ui.label(RichText::new(text::WINDOWS_PLUGINS_WAITING).weak());
                    for path in waiting {
                        ui.horizontal(|ui| {
                            cell(
                                ui,
                                NAME_WIDTH + DETAIL_WIDTH,
                                RichText::new(file_title(path)),
                            );
                            remove_button(ui, path, parts);
                        });
                    }
                    ui.add_space(6.0);
                }
                let none = instruments.is_empty() && effects.is_empty();
                crate::workspace::search_state(ui, &search, none, shown);
            }
        }
    }
}

/// One plugin: its name, maker and format, and Remove when it was added
/// here.
fn plugin_row(ui: &mut Ui, info: &PluginInfo, parts: &mut Parts<'_>) {
    ui.horizontal(|ui| {
        cell(ui, NAME_WIDTH, RichText::new(&info.name).strong())
            .on_hover_text(info.path.display().to_string());
        cell(
            ui,
            DETAIL_WIDTH,
            RichText::new(text::plugin_detail(&info.vendor, format_name(info))).weak(),
        );
        if let Some(target) = removal_target(&info.path, &parts.places.added) {
            remove_button(ui, &target, parts);
        }
    });
}

/// A column of a row: text from the left, cut short with "…" if too long.
fn cell(ui: &mut Ui, width: f32, words: RichText) -> egui::Response {
    ui.allocate_ui_with_layout(
        egui::vec2(width, 22.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_width(width);
            ui.add(egui::Label::new(words).truncate())
        },
    )
    .inner
}

fn remove_button(ui: &mut Ui, path: &Path, parts: &mut Parts<'_>) {
    let label = text::remove_plugin(&file_title(path));
    if ui
        .add_enabled(!parts.places.busy, Button::new(text::REMOVE).small())
        .on_hover_text(&label)
        .clicked()
    {
        parts
            .requests
            .push(Request::RemovePlugin(path.to_path_buf()));
    }
}

/// Windows plugins on Linux: ready, or what to install.
fn windows_plugins(ui: &mut Ui, parts: &mut Parts<'_>, bridge: BridgeState) {
    let heading = if bridge.is_ready() {
        text::WINDOWS_PLUGINS_READY_HEADING
    } else {
        text::WINDOWS_PLUGINS_HEADING
    };
    egui::CollapsingHeader::new(heading)
        .id_salt("windows-plugins")
        .default_open(!bridge.is_ready())
        .show(ui, |ui| {
            ui.set_width(WIDTH - 24.0);
            if bridge.is_ready() {
                ui.label(text::WINDOWS_PLUGINS_READY);
            } else {
                ui.label(text::windows_plugins_missing(bridge.wine, bridge.yabridge));
                for step in text::WINDOWS_PLUGINS_STEPS {
                    ui.label(RichText::new(*step).small());
                }
            }
            if ui
                .add_enabled(!parts.places.busy, Button::new(text::CHECK_AGAIN))
                .on_hover_text(text::CHECK_AGAIN_HINT)
                .clicked()
            {
                parts.requests.push(Request::SetUpWindowsPlugins);
            }
        });
}

/// The folders searched: the program's own, the usual ones, and the
/// person's own, which can be added and removed.
fn folders(ui: &mut Ui, parts: &mut Parts<'_>) {
    egui::CollapsingHeader::new(text::FOLDERS_SEARCHED)
        .id_salt("plugin-folders")
        .show(ui, |ui| {
            ui.set_width(WIDTH - 24.0);
            let weak = Palette::of(ui.ctx()).text_weak;
            if let Some(own) = &parts.places.own {
                ui.horizontal(|ui| {
                    ui.label(text::OWN_PLUGIN_FOLDER);
                    if ui
                        .small_button(text::SHOW_FOLDER)
                        .on_hover_text(own.display().to_string())
                        .clicked()
                    {
                        parts.host.open_folder(own);
                    }
                });
            }
            for folder in &parts.places.usual {
                ui.label(
                    RichText::new(folder.display().to_string())
                        .small()
                        .color(weak),
                );
            }
            let mut removed = None;
            for (index, folder) in parts.settings.plugin_folders.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(folder.display().to_string()).small());
                    if ui
                        .small_button(text::REMOVE)
                        .on_hover_text(text::STOP_SEARCHING_FOLDER)
                        .clicked()
                    {
                        removed = Some(index);
                    }
                });
            }
            if let Some(index) = removed {
                parts.settings.plugin_folders.remove(index);
                parts.requests.push(Request::FindPlugins);
            }
            if ui
                .button(text::SEARCH_ANOTHER_FOLDER)
                .on_hover_text(text::SEARCH_ANOTHER_FOLDER_HINT)
                .clicked()
                && let Some(folder) = parts.host.choose_folder(text::SEARCH_ANOTHER_FOLDER)
                && !parts.settings.plugin_folders.contains(&folder)
            {
                parts.settings.plugin_folders.push(folder);
                parts.requests.push(Request::FindPlugins);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugins_added_here_and_their_bridges_can_be_removed() {
        let own = PathBuf::from("/data/plugins");
        let added = vec![
            own.join("clap/Mine.clap"),
            own.join("windows/vst3/Win.vst3"),
        ];
        assert_eq!(
            removal_target(&own.join("clap/Mine.clap"), &added),
            Some(own.join("clap/Mine.clap"))
        );
        // yabridge's stand-in for the Windows plugin.
        let bridge = PathBuf::from("/home/x/.vst3/yabridge/vst3/Win.vst3");
        assert_eq!(
            removal_target(&bridge, &added),
            Some(own.join("windows/vst3/Win.vst3"))
        );
        // Installed elsewhere: left to its own uninstaller.
        assert_eq!(
            removal_target(Path::new("/usr/lib/clap/Other.clap"), &added),
            None
        );
        assert_eq!(
            removal_target(Path::new("/home/x/.vst3/Win.vst3"), &added),
            None
        );
    }

    #[test]
    fn plugins_downloads_and_installers_are_for_the_plugins_window() {
        for name in ["a.vst3", "b.clap", "c.zip", "d.DLL", "setup.exe", "e.msi"] {
            assert!(is_for_plugins(Path::new(name)), "{name}");
        }
        for name in ["song.wav", "tune.mid", "notes.txt"] {
            assert!(!is_for_plugins(Path::new(name)), "{name}");
        }
    }
}
