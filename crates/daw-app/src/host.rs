// SPDX-License-Identifier: GPL-3.0-or-later

//! The desktop's own open, save, export and import dialogs.

use std::path::{Path, PathBuf};

use daw_ui::{ExportTarget, Host, text};

/// Shows the system's file dialogs, starting in the folder last used.
#[derive(Default)]
pub struct DialogHost {
    folder: Option<PathBuf>,
    /// Where sound files were last imported from.
    sounds: Option<PathBuf>,
    /// Where plugins (or folders) were last chosen from.
    plugins: Option<PathBuf>,
}

impl DialogHost {
    fn dialog(&self) -> rfd::FileDialog {
        let dialog = rfd::FileDialog::new()
            .add_filter(text::file_type_name(), &[daw_ident::PROJECT_FILE_EXTENSION]);
        self.in_folder(dialog)
    }

    fn in_folder(&self, dialog: rfd::FileDialog) -> rfd::FileDialog {
        match &self.folder {
            Some(folder) => dialog.set_directory(folder),
            None => dialog,
        }
    }

    fn remember(&mut self, path: Option<&Path>) {
        if let Some(folder) = path.and_then(Path::parent) {
            self.folder = Some(folder.to_path_buf());
        }
    }
}

impl Host for DialogHost {
    fn choose_file_to_open(&mut self) -> Option<PathBuf> {
        // this program's songs, and other programs' (decision 0020).
        let native = daw_ident::PROJECT_FILE_EXTENSION;
        let dialog = rfd::FileDialog::new()
            .add_filter(
                text::ANY_SONG_FILE_TYPE,
                &[native, "dawproject", "mid", "midi"],
            )
            .add_filter(text::file_type_name(), &[native])
            .add_filter(text::DAWPROJECT_FILE_TYPE, &["dawproject"])
            .add_filter(text::MIDI_FILE_TYPE, &["mid", "midi"]);
        let path = self
            .in_folder(dialog)
            .set_title(text::OPEN_TITLE)
            .pick_file();
        self.remember(path.as_deref());
        path
    }

    fn choose_file_to_save(&mut self, suggested_name: &str) -> Option<PathBuf> {
        let path = self
            .dialog()
            .set_title(text::SAVE_TITLE)
            .set_file_name(suggested_name)
            .save_file();
        self.remember(path.as_deref());
        path
    }

    fn choose_export_file(
        &mut self,
        suggested_name: &str,
        target: ExportTarget,
    ) -> Option<PathBuf> {
        let dialog = rfd::FileDialog::new()
            .add_filter(text::export_file_type(target), &[target.extension()])
            .set_title(text::EXPORT_TITLE)
            .set_file_name(suggested_name);
        let path = self.in_folder(dialog).save_file();
        self.remember(path.as_deref());
        path
    }

    fn open_user_guide(&mut self) {
        // Beside the program (Windows, a dist folder), or in the AppImage's
        // documents; else the copy on the project's site.
        let exe = std::env::current_exe().ok();
        let folder = exe.as_deref().and_then(Path::parent);
        let page = "user-guide.html";
        let local = folder.and_then(|folder| {
            [
                folder.join(page),
                folder
                    .join("../share/doc")
                    .join(daw_ident::EXECUTABLE_NAME)
                    .join(page),
            ]
            .into_iter()
            .find(|p| p.is_file())
        });
        let target = local.map_or_else(
            || daw_ident::USER_GUIDE_URL.to_owned(),
            |p| p.display().to_string(),
        );
        open_in_browser(&target);
    }

    fn choose_audio_files(&mut self) -> Vec<PathBuf> {
        let mut dialog = rfd::FileDialog::new()
            .add_filter(
                text::IMPORT_FILE_TYPE,
                &["wav", "aif", "aiff", "flac", "mp3", "ogg", "mid", "midi"],
            )
            .set_title(text::IMPORT_TITLE);
        if let Some(folder) = self.sounds.as_ref().or(self.folder.as_ref()) {
            dialog = dialog.set_directory(folder);
        }
        let files = dialog.pick_files().unwrap_or_default();
        if let Some(folder) = files.first().and_then(|f| f.parent()) {
            self.sounds = Some(folder.to_path_buf());
        }
        files
    }

    fn choose_plugin_files(&mut self) -> Vec<PathBuf> {
        let mut dialog = rfd::FileDialog::new()
            .add_filter(
                text::PLUGIN_FILE_TYPE,
                &["vst3", "clap", "zip", "exe", "msi", "dll"],
            )
            .set_title(text::CHOOSE_PLUGINS_TITLE);
        if let Some(folder) = &self.plugins {
            dialog = dialog.set_directory(folder);
        }
        let files = dialog.pick_files().unwrap_or_default();
        if let Some(folder) = files.first().and_then(|f| f.parent()) {
            self.plugins = Some(folder.to_path_buf());
        }
        files
    }

    fn choose_soundfont(&mut self) -> Option<PathBuf> {
        let mut dialog = rfd::FileDialog::new()
            .add_filter(text::SOUNDFONT_FILE_TYPE, &["sf2"])
            .set_title(text::CHOOSE_SOUNDFONT_TITLE);
        if let Some(folder) = self.sounds.as_ref().or(self.folder.as_ref()) {
            dialog = dialog.set_directory(folder);
        }
        let file = dialog.pick_file();
        if let Some(folder) = file.as_deref().and_then(Path::parent) {
            self.sounds = Some(folder.to_path_buf());
        }
        file
    }

    fn choose_folder(&mut self, title: &str) -> Option<PathBuf> {
        let mut dialog = rfd::FileDialog::new().set_title(title);
        if let Some(folder) = &self.plugins {
            dialog = dialog.set_directory(folder);
        }
        let folder = dialog.pick_folder();
        if let Some(parent) = folder.as_deref().and_then(Path::parent) {
            self.plugins = Some(parent.to_path_buf());
        }
        folder
    }

    fn open_folder(&mut self, folder: &Path) {
        let _ = std::fs::create_dir_all(folder);
        open_in_browser(&folder.display().to_string());
    }
}

/// Opens a page or an address with the desktop's own handler.
fn open_in_browser(target: &str) {
    let opener = if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(opener).arg(target).spawn();
}
