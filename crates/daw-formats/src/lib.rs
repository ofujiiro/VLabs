// SPDX-License-Identifier: GPL-3.0-or-later

#![doc = include_str!("../README.md")]

mod atomic;
pub mod audio;
mod native;
pub mod wav;

pub use atomic::{write_atomically, write_atomically_with};
pub use native::{
    FORMAT_VERSION, FileError, free_name, is_plain_name, load_project, move_audio,
    project_from_bytes, project_from_json, project_to_bytes, save_project,
};
