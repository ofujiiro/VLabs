// SPDX-License-Identifier: GPL-3.0-or-later

#![doc = include_str!("../README.md")]
// A release build on Windows opens no console window next to the program.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod host;
mod icon;
mod layout;
mod paths;
mod plugin_jobs;
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

/// The bundled acoustic drum kit (CC0, see THIRD_PARTY.md).
static ACOUSTIC_KIT: &[u8] = include_bytes!("../../../assets/drums/acoustic.kit");

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

/// The argument that writes the program's icon files into a folder and
/// exits (for the installers, decision 0022).
const WRITE_ICONS_ARGUMENT: &str = "--write-icons";

fn main() {
    // A separate run that only describes one plugin file (decision 0015).
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.first().map(String::as_str) == Some(daw_plugin_host::scan::DESCRIBE_ARGUMENT) {
        daw_plugin_host::scan::describe_and_print(&arguments[1..]);
        return;
    }
    if arguments.first().map(String::as_str) == Some(WRITE_ICONS_ARGUMENT) {
        let folder = arguments
            .get(1)
            .map_or_else(|| PathBuf::from("."), PathBuf::from);
        if let Err(error) = icon::write_icons(&folder) {
            report_fatal(&error.to_string());
            std::process::exit(1);
        }
        return;
    }
    daw_plugin_host::set_identity(daw_plugin_host::HostIdentity {
        name: daw_ident::PRODUCT_NAME.to_owned(),
        vendor: String::new(),
        url: String::new(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
    });
    // Before the window opens (its toolkit then installs its own handlers).
    daw_plugin_host::prepare_display();
    let options = options();
    let paths = Paths::find();
    let settings = paths
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p.settings_file()).ok())
        .map_or_else(Settings::default, |json| Settings::from_json(&json));

    let library = sound_library(paths.as_ref().map(paths::Paths::voicebanks));
    let config = EngineConfig {
        sample_rate: SAMPLE_RATE,
        max_block: 1024,
    };
    let (mut engine, mut processor) = Engine::new(config, library);
    // The microphone or interface feeds the engine (decision 0017); it is
    // opened only while recording sound.
    let input = engine
        .take_input_port()
        .filter(|_| !options.silent)
        .map(|mut port| {
            daw_devices::AudioInput::start(move |samples, channels, rate| {
                port.push(samples, channels, rate);
            })
        });
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
            if let Some((bytes, index)) = japanese_font() {
                daw_ui::theme::use_fallback_font(&cc.egui_ctx, bytes, index);
            }
            let mut app = App::new(
                &cc.egui_ctx,
                Parts {
                    workspace,
                    audio,
                    input,
                    paths,
                    autosaver,
                    settings,
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

/// The instruments' sounds: the bundled piano and drum kit, and readers
/// for the person's SFZ instruments (decision 0024) and voicebanks (in the
/// program's own folder, decision 0027), read on threads of their own.
fn sound_library(voicebanks: Option<std::path::PathBuf>) -> SoundLibrary {
    let mut library = SoundLibrary::new();
    library.register_builtin(daw_model::BUILTIN_PIANO, PIANO);
    library.register_kit(daw_model::ACOUSTIC_KIT, ACOUSTIC_KIT);
    library.set_sfz_reader(|path| daw_formats::sfz::load(path).map_err(|e| e.to_string()));
    library.set_voice_reader(std::sync::Arc::new(move |name| match &voicebanks {
        Some(root) if daw_formats::utau::read_info(&root.join(name)).is_ok() => {
            daw_formats::utau::read_voicebank(&root.join(name))
        }
        _ => Err(text::voicebank_not_installed(name)),
    }));
    library.set_background_loading(true);
    library
}

/// A font on this computer that draws Japanese (for lyrics and voicebank
/// names, decision 0027): none is bundled, as none we could bundle has
/// the right licence. The first one found, and its index in a collection.
fn japanese_font() -> Option<(Vec<u8>, u32)> {
    let candidates: &[&str] = if cfg!(windows) {
        &[
            "C:\\Windows\\Fonts\\YuGothR.ttc",
            "C:\\Windows\\Fonts\\meiryo.ttc",
            "C:\\Windows\\Fonts\\msgothic.ttc",
        ]
    } else {
        &[
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJKjp-Regular.otf",
            "/usr/share/fonts/opentype/ipafont-gothic/ipag.ttf",
            "/usr/share/fonts/truetype/fonts-japanese-gothic.ttf",
            "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
            "/usr/share/fonts/droid/DroidSansFallbackFull.ttf",
        ]
    };
    candidates
        .iter()
        .find_map(|path| std::fs::read(path).ok())
        .map(|bytes| (bytes, 0))
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
