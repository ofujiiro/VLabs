// SPDX-License-Identifier: GPL-3.0-or-later

#![doc = include_str!("../README.md")]
// A release build on Windows opens no console window next to the program.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod host;
mod icon;
mod layout;
mod paths;
mod recovery;

use std::path::PathBuf;

use daw_engine::{Engine, EngineConfig, SoundLibrary};
use daw_ui::{KeyLabels, Session, Settings, Workspace, text};

use crate::app::{App, Parts};
use crate::paths::Paths;
use crate::recovery::Autosaver;

// Debug builds stop at once if the audio thread allocates memory.
#[global_allocator]
static ALLOCATOR: daw_rt::RtCheckedAllocator = daw_rt::RtCheckedAllocator;

/// The bundled piano (CC0, see THIRD_PARTY.md).
static PIANO: &[u8] = include_bytes!("../../../assets/sounds/piano.sf2");

/// The sample rate everything runs at.
const SAMPLE_RATE: u32 = 48_000;

/// What the command line asked for.
struct Options {
    file: Option<PathBuf>,
    silent: bool,
}

fn options() -> Options {
    let mut options = Options {
        file: None,
        silent: false,
    };
    for argument in std::env::args_os().skip(1) {
        if argument == "--silent" {
            options.silent = true;
        } else if !argument.to_string_lossy().starts_with("--") {
            options.file = Some(PathBuf::from(argument));
        }
    }
    options
}

fn main() {
    let options = options();
    let paths = Paths::find();
    let settings = paths
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p.settings_file()).ok())
        .map_or_else(Settings::default, |json| Settings::from_json(&json));

    let mut library = SoundLibrary::new();
    library.register_builtin(daw_model::BUILTIN_PIANO, PIANO);
    let config = EngineConfig {
        sample_rate: SAMPLE_RATE,
        max_block: 1024,
    };
    let (engine, mut processor) = Engine::new(config, library);
    let midi_sender = engine.midi_sender();
    let render = move |buffer: &mut [f32], channels: usize| processor.process(buffer, channels);
    let audio = if options.silent {
        daw_devices::AudioOutput::start_silent(SAMPLE_RATE, render)
    } else {
        daw_devices::AudioOutput::start(SAMPLE_RATE, render)
    };
    let autosaver = paths
        .as_ref()
        .and_then(|p| Autosaver::start(&p.recovery()).ok());

    let viewport = egui::ViewportBuilder::default()
        .with_title(daw_ident::PRODUCT_NAME)
        .with_app_id(daw_ident::APPLICATION_ID)
        .with_inner_size([1280.0, 800.0])
        .with_min_inner_size([960.0, 600.0])
        .with_icon(icon::window_icon());
    let native = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    let file = options.file;
    let result = eframe::run_native(
        daw_ident::PRODUCT_NAME,
        native,
        Box::new(move |cc| {
            let labels = KeyLabels::new(layout::detect());
            let workspace = Workspace::new(
                &cc.egui_ctx,
                Session::starter(engine),
                settings.clone(),
                Box::new(host::DialogHost::default()),
                labels,
            );
            let mut app = App::new(
                &cc.egui_ctx,
                Parts {
                    workspace,
                    audio,
                    paths,
                    autosaver,
                    settings,
                    midi_sender,
                },
            );
            if let Some(file) = file {
                app.open(file);
            }
            Ok(Box::new(app))
        }),
    );
    if let Err(error) = result {
        report_fatal(&error.to_string());
        std::process::exit(1);
    }
}

/// Tells the person the program could not start: on the terminal, and in a
/// message box (a program started from the desktop has no terminal).
#[allow(clippy::print_stderr, reason = "the last resort when no window opens")]
fn report_fatal(error: &str) {
    let message = text::could_not_start(error);
    eprintln!("{message}");
    let _ = rfd::MessageDialog::new()
        .set_title(daw_ident::PRODUCT_NAME)
        .set_description(message)
        .set_level(rfd::MessageLevel::Error)
        .show();
}
