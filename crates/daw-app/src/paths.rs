// SPDX-License-Identifier: GPL-3.0-or-later

//! Where the program keeps its files: settings in the user's configuration
//! folder, crash-recovery copies in the user's data folder. Both follow
//! each system's conventions (the XDG folders on Linux, the roaming and
//! local application-data folders on Windows).

use std::path::PathBuf;

/// The program's folders.
#[derive(Clone, Debug)]
pub struct Paths {
    /// Settings.
    pub config: PathBuf,
    /// Recovery copies and other data that belongs to this computer.
    pub data: PathBuf,
}

impl Paths {
    /// The folders for this user, if the system says where they are.
    pub fn find() -> Option<Self> {
        let (config, data) = system_folders()?;
        Some(Self {
            config: config.join(folder_name()),
            data: data.join(folder_name()),
        })
    }

    /// The settings file.
    pub fn settings_file(&self) -> PathBuf {
        self.config.join("settings.json")
    }

    /// Where the voicebanks added in the program are kept (decision 0027).
    pub fn voicebanks(&self) -> PathBuf {
        self.data.join("voicebanks")
    }

    /// Where crash-recovery copies go.
    pub fn recovery(&self) -> PathBuf {
        self.data.join("recovery")
    }

    /// What is known about the plugins on this computer, so they are not
    /// opened again at every start.
    pub fn plugin_cache(&self) -> PathBuf {
        self.data.join("plugins.json")
    }
}

/// The folder name: lower case on Linux, as is usual there; the product
/// name on Windows.
fn folder_name() -> &'static str {
    if cfg!(windows) {
        daw_ident::PRODUCT_NAME
    } else {
        daw_ident::EXECUTABLE_NAME
    }
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}

#[cfg(windows)]
fn system_folders() -> Option<(PathBuf, PathBuf)> {
    let roaming = env_path("APPDATA")?;
    let local = env_path("LOCALAPPDATA").unwrap_or_else(|| roaming.clone());
    Some((roaming, local))
}

#[cfg(not(windows))]
fn system_folders() -> Option<(PathBuf, PathBuf)> {
    let home = env_path("HOME");
    let config =
        env_path("XDG_CONFIG_HOME").or_else(|| home.as_ref().map(|h| h.join(".config")))?;
    let data = env_path("XDG_DATA_HOME")
        .or_else(|| home.as_ref().map(|h| h.join(".local").join("share")))?;
    Some((config, data))
}
