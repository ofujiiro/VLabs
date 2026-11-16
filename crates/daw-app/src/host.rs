// SPDX-License-Identifier: GPL-3.0-or-later

//! The desktop's own open, save and export dialogs.

use std::path::{Path, PathBuf};

use daw_ui::{Host, text};

/// Shows the system's file dialogs, starting in the folder last used.
#[derive(Default)]
pub struct DialogHost {
    folder: Option<PathBuf>,
}

impl DialogHost {
    fn dialog(&self) -> rfd::FileDialog {
        let dialog = rfd::FileDialog::new()
            .add_filter(text::file_type_name(), &[daw_ident::PROJECT_FILE_EXTENSION]);
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
        let path = self.dialog().set_title(text::OPEN_TITLE).pick_file();
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

    fn choose_export_file(&mut self, suggested_name: &str) -> Option<PathBuf> {
        let mut dialog = rfd::FileDialog::new()
            .add_filter(text::EXPORT_FILE_TYPE, &["wav"])
            .set_title(text::EXPORT_TITLE)
            .set_file_name(suggested_name);
        if let Some(folder) = &self.folder {
            dialog = dialog.set_directory(folder);
        }
        let path = dialog.save_file();
        self.remember(path.as_deref());
        path
    }
}
