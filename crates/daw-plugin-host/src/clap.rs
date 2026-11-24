// SPDX-License-Identifier: GPL-3.0-or-later

//! CLAP plugins, through `clack-host`.
//!
//! The main-thread half owns the plugin instance; the audio-thread half owns
//! its audio processor, started on the first block and stopped before it
//! goes back. Plugin libraries stay loaded for the life of the program
//! (unloading them is a common source of crashes at exit).

#![allow(
    unsafe_code,
    reason = "loading a plugin library and handing it a native window are unsafe by nature"
)]

use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use clack_extensions::audio_ports::{
    AudioPortInfoBuffer, AudioPortRescanFlags, HostAudioPorts, HostAudioPortsImpl, PluginAudioPorts,
};
use clack_extensions::gui::{
    GuiApiType, GuiConfiguration, GuiSize, HostGui, HostGuiImpl, PluginGui, Window,
};
use clack_extensions::log::{HostLog, HostLogImpl, LogSeverity};
use clack_extensions::note_ports::{
    HostNotePorts, HostNotePortsImpl, NoteDialects, NotePortInfoBuffer, NotePortRescanFlags,
    PluginNotePorts,
};
use clack_extensions::params::{
    HostParams, HostParamsImplMainThread, HostParamsImplShared, ParamClearFlags, ParamInfoBuffer,
    ParamInfoFlags, ParamRescanFlags, PluginParams,
};
use clack_extensions::state::{HostState, HostStateImpl, PluginState};
use clack_extensions::timer::{HostTimer, HostTimerImpl, PluginTimer, TimerId};
use clack_host::events::event_types::{MidiEvent, NoteOffEvent, NoteOnEvent, ParamValueEvent};
use clack_host::prelude::*;

use crate::window::ParentHandle;
use crate::{
    AudioConfig, AudioSide, Event, Kind, MAX_EVENTS, MainSide, ParamInfo, PluginInfo, Transport,
};

/// The host side of every CLAP plugin.
pub struct Host;

impl HostHandlers for Host {
    type Shared<'a> = Shared;
    type MainThread<'a> = MainThread<'a>;
    type AudioProcessor<'a> = ();

    fn declare_extensions(builder: &mut HostExtensions<'_, Self>, _shared: &Self::Shared<'_>) {
        builder
            .register::<HostLog>()
            .register::<HostGui>()
            .register::<HostTimer>()
            .register::<HostParams>()
            .register::<HostState>()
            .register::<HostAudioPorts>()
            .register::<HostNotePorts>();
        #[cfg(unix)]
        builder.register::<clack_extensions::posix_fd::HostPosixFd>();
    }
}

/// Host data any of the plugin's threads may touch.
pub struct Shared {
    callback_requested: AtomicBool,
    gui_closed: AtomicBool,
    requested_size: Mutex<Option<GuiSize>>,
    changed: AtomicBool,
    touched: AtomicU32,
    touched_set: AtomicBool,
}

impl SharedHandler<'_> for Shared {
    fn request_restart(&self) {}
    fn request_process(&self) {}
    fn request_callback(&self) {
        self.callback_requested.store(true, Ordering::Release);
    }
}

impl HostLogImpl for Shared {
    fn log(&self, _severity: LogSeverity, _message: &str) {
        // Plugins log freely; nothing here may block or allocate on the
        // audio thread, so messages are dropped.
    }
}

impl HostGuiImpl for Shared {
    fn resize_hints_changed(&self) {}
    fn request_resize(&self, new_size: GuiSize) -> Result<(), HostError> {
        if let Ok(mut size) = self.requested_size.lock() {
            *size = Some(new_size);
        }
        Ok(())
    }
    fn request_show(&self) -> Result<(), HostError> {
        Ok(())
    }
    fn request_hide(&self) -> Result<(), HostError> {
        Ok(())
    }
    fn closed(&self, _was_destroyed: bool) {
        self.gui_closed.store(true, Ordering::Release);
    }
}

impl HostParamsImplShared for Shared {
    fn request_flush(&self) {}
}

/// A timer the plugin asked for.
struct Timer {
    id: u32,
    period: Duration,
    next: Instant,
}

/// Host data for the plugin's main thread only.
pub struct MainThread<'a> {
    plugin: OnceCell<InitializedPluginHandle<'a>>,
    timers: RefCell<Vec<Timer>>,
    next_timer_id: Cell<u32>,
    state_dirty: Cell<bool>,
    #[cfg(unix)]
    fds: RefCell<Vec<(i32, clack_extensions::posix_fd::FdFlags)>>,
}

impl<'a> MainThreadHandler<'a> for MainThread<'a> {
    fn initialized(&self, instance: InitializedPluginHandle<'a>) {
        let _ = self.plugin.set(instance);
    }
}

impl HostTimerImpl for MainThread<'_> {
    fn register_timer(&self, period_ms: u32) -> Result<TimerId, HostError> {
        let id = self.next_timer_id.get();
        self.next_timer_id.set(id + 1);
        let period = Duration::from_millis(u64::from(period_ms.max(1)));
        self.timers.borrow_mut().push(Timer {
            id,
            period,
            next: Instant::now() + period,
        });
        Ok(TimerId(id))
    }

    fn unregister_timer(&self, timer_id: TimerId) -> Result<(), HostError> {
        self.timers.borrow_mut().retain(|t| t.id != timer_id.0);
        Ok(())
    }
}

#[cfg(unix)]
impl clack_extensions::posix_fd::HostPosixFdImpl for MainThread<'_> {
    fn register_fd(
        &self,
        fd: i32,
        flags: clack_extensions::posix_fd::FdFlags,
    ) -> Result<(), HostError> {
        self.fds.borrow_mut().push((fd, flags));
        Ok(())
    }

    fn modify_fd(
        &self,
        fd: i32,
        flags: clack_extensions::posix_fd::FdFlags,
    ) -> Result<(), HostError> {
        for entry in self.fds.borrow_mut().iter_mut().filter(|e| e.0 == fd) {
            entry.1 = flags;
        }
        Ok(())
    }

    fn unregister_fd(&self, fd: i32) -> Result<(), HostError> {
        self.fds.borrow_mut().retain(|e| e.0 != fd);
        Ok(())
    }
}

impl HostParamsImplMainThread for MainThread<'_> {
    fn rescan(&self, _flags: ParamRescanFlags) {}
    fn clear(&self, _param_id: ClapId, _flags: ParamClearFlags) {}
}

impl HostStateImpl for MainThread<'_> {
    fn mark_dirty(&self) {
        self.state_dirty.set(true);
    }
}

impl HostAudioPortsImpl for MainThread<'_> {
    fn is_rescan_flag_supported(&self, _flag: AudioPortRescanFlags) -> bool {
        false
    }
    fn rescan(&self, _flags: AudioPortRescanFlags) {}
}

impl HostNotePortsImpl for MainThread<'_> {
    fn supported_dialects(&self) -> NoteDialects {
        // Notes go as CLAP notes; a controller's wheels and knobs as MIDI
        // (decision 0033).
        NoteDialects::CLAP | NoteDialects::MIDI
    }
    fn rescan(&self, _flags: NotePortRescanFlags) {}
}

// ---------------------------------------------------------------------------
// Libraries

/// Loaded plugin libraries, kept for the life of the program.
static ENTRIES: Mutex<Option<HashMap<PathBuf, PluginEntry>>> = Mutex::new(None);

fn entry(path: &Path) -> Result<PluginEntry, String> {
    let mut entries = ENTRIES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let entries = entries.get_or_insert_with(HashMap::new);
    if let Some(entry) = entries.get(path) {
        return Ok(entry.clone());
    }
    // SAFETY: loading a plugin runs its code; that is what a plugin host
    // is for. The path comes from scanning the plugin folders.
    let entry = unsafe { PluginEntry::load(path) }
        .map_err(|e| format!("{} could not be loaded: {e}", crate::shown(path).display()))?;
    entries.insert(path.to_path_buf(), entry.clone());
    Ok(entry)
}

fn text(value: Option<&CStr>) -> String {
    value.map_or_else(String::new, |v| v.to_string_lossy().into_owned())
}

/// The plugins in a CLAP file, from the factory's descriptions (no
/// instance is created).
pub fn describe(path: &Path) -> Result<Vec<PluginInfo>, String> {
    let entry = entry(path)?;
    let factory = entry
        .get_plugin_factory()
        .ok_or("the file has no plugin factory")?;
    let mut plugins = Vec::new();
    for descriptor in factory.plugin_descriptors() {
        let Some(id) = descriptor.id() else {
            continue;
        };
        let features: Vec<String> = descriptor
            .features()
            .map(|f| f.to_string_lossy().into_owned())
            .collect();
        let kind = if features.iter().any(|f| f == "instrument") {
            Kind::Instrument
        } else if features.iter().any(|f| f == "audio-effect") {
            Kind::Effect
        } else {
            Kind::Other
        };
        plugins.push(PluginInfo {
            format: crate::Format::Clap,
            path: path.to_path_buf(),
            id: id.to_string_lossy().into_owned(),
            name: text(descriptor.name()),
            vendor: text(descriptor.vendor()),
            version: text(descriptor.version()),
            kind,
        });
    }
    Ok(plugins)
}

// ---------------------------------------------------------------------------
// Loading

/// Loads a plugin and activates it.
pub fn load(info: &PluginInfo, config: AudioConfig) -> Result<(ClapMain, ClapAudio), String> {
    let entry = entry(&info.path)?;
    let id = CString::new(info.id.clone()).map_err(|_| "a bad plugin id".to_owned())?;
    let identity = crate::identity();
    let host_info = HostInfo::new(
        &identity.name,
        &identity.vendor,
        &identity.url,
        &identity.version,
    )
    .map_err(|e| e.to_string())?;
    let mut instance = PluginInstance::<Host>::new(
        |()| Shared {
            callback_requested: AtomicBool::new(false),
            gui_closed: AtomicBool::new(false),
            requested_size: Mutex::new(None),
            changed: AtomicBool::new(false),
            touched: AtomicU32::new(0),
            touched_set: AtomicBool::new(false),
        },
        |_| MainThread {
            plugin: OnceCell::new(),
            timers: RefCell::new(Vec::new()),
            next_timer_id: Cell::new(1),
            state_dirty: Cell::new(false),
            #[cfg(unix)]
            fds: RefCell::new(Vec::new()),
        },
        &entry,
        &id,
        &host_info,
    )
    .map_err(|e| format!("{} could not start: {e}", info.name))?;

    let (inputs, outputs) = port_layout(&mut instance);
    let ranges = param_ranges(&mut instance);
    let midi_port = midi_port(&mut instance);
    let max_block = config.max_block.max(1);
    let processor = instance
        .activate(
            |_, _| (),
            PluginAudioConfiguration {
                sample_rate: f64::from(config.sample_rate),
                min_frames_count: 1,
                max_frames_count: u32::try_from(max_block).unwrap_or(u32::MAX),
            },
        )
        .map_err(|e| format!("{} could not be activated: {e}", info.name))?;
    let channels = |ports: &[usize]| ports.iter().sum::<usize>();
    let audio = ClapAudio {
        input_buffers: inputs
            .iter()
            .map(|&n| vec![vec![0.0; max_block]; n])
            .collect(),
        output_buffers: outputs
            .iter()
            .map(|&n| vec![vec![0.0; max_block]; n])
            .collect(),
        input_ports: AudioPorts::with_capacity(channels(&inputs), inputs.len()),
        output_ports: AudioPorts::with_capacity(channels(&outputs), outputs.len()),
        events: EventBuffer::with_capacity(MAX_EVENTS),
        output_events: EventBuffer::with_capacity(MAX_EVENTS),
        ranges,
        midi_port,
        steady_time: 0,
        processor: Some(PluginAudioProcessor::from(processor)),
    };
    let main = ClapMain {
        instance,
        _entry: entry,
        gui_open: false,
    };
    Ok((main, audio))
}

/// Channel counts of the plugin's audio ports (inputs, outputs).
fn port_layout(instance: &mut PluginInstance<Host>) -> (Vec<usize>, Vec<usize>) {
    let ports = instance.access_handler(|h| {
        h.plugin
            .get()
            .and_then(InitializedPluginHandle::get_extension::<PluginAudioPorts>)
    });
    let Some(ports) = ports else {
        return (Vec::new(), vec![2]);
    };
    let handle = instance.plugin_handle();
    let mut buffer = AudioPortInfoBuffer::new();
    let mut read = |is_input: bool| -> Vec<usize> {
        (0..ports.count(&handle, is_input))
            .filter_map(|index| {
                ports
                    .get(&handle, index, is_input, &mut buffer)
                    .map(|info| usize::try_from(info.channel_count).unwrap_or(0))
            })
            .collect()
    };
    let inputs = read(true);
    let outputs = read(false);
    (inputs, outputs)
}

/// The first input note port that takes MIDI, where a controller's wheels
/// and knobs go (decision 0033); `None` if the plugin takes no MIDI.
fn midi_port(instance: &mut PluginInstance<Host>) -> Option<u16> {
    let ports = instance.access_handler(|h| {
        h.plugin
            .get()
            .and_then(InitializedPluginHandle::get_extension::<PluginNotePorts>)
    })?;
    let handle = instance.plugin_handle();
    let mut buffer = NotePortInfoBuffer::new();
    (0..ports.count(&handle, true))
        .find(|&index| {
            ports
                .get(&handle, index, true, &mut buffer)
                .is_some_and(|info| info.supported_dialects.contains(NoteDialects::MIDI))
        })
        .and_then(|index| u16::try_from(index).ok())
}

/// Each parameter's range, by id, for turning fractions into values.
fn param_ranges(instance: &mut PluginInstance<Host>) -> Vec<(u32, f64, f64, bool)> {
    let params = instance.access_handler(|h| {
        h.plugin
            .get()
            .and_then(InitializedPluginHandle::get_extension::<PluginParams>)
    });
    let Some(params) = params else {
        return Vec::new();
    };
    let handle = instance.plugin_handle();
    let mut buffer = ParamInfoBuffer::new();
    let mut ranges: Vec<(u32, f64, f64, bool)> = (0..params.count(&handle))
        .filter_map(|index| {
            let info = params.get_info(&handle, index, &mut buffer)?;
            let stepped = info.flags.contains(ParamInfoFlags::IS_STEPPED);
            Some((info.id.get(), info.min_value, info.max_value, stepped))
        })
        .collect();
    ranges.sort_by_key(|r| r.0);
    ranges
}

// ---------------------------------------------------------------------------
// The main-thread half

/// The main-thread half of a CLAP plugin.
pub struct ClapMain {
    // Field order matters: the instance goes before the library it came from.
    instance: PluginInstance<Host>,
    _entry: PluginEntry,
    gui_open: bool,
}

impl ClapMain {
    fn extension<
        E: clack_host::extensions::Extension<
                ExtensionSide = clack_host::extensions::PluginExtensionSide,
            > + Copy,
    >(
        &mut self,
    ) -> Option<E> {
        self.instance.access_handler(|h| {
            h.plugin
                .get()
                .and_then(InitializedPluginHandle::get_extension::<E>)
        })
    }

    fn range(&mut self, id: u32) -> Option<(f64, f64)> {
        let params = self.extension::<PluginParams>()?;
        let handle = self.instance.plugin_handle();
        let mut buffer = ParamInfoBuffer::new();
        (0..params.count(&handle)).find_map(|index| {
            let info = params.get_info(&handle, index, &mut buffer)?;
            (info.id.get() == id).then_some((info.min_value, info.max_value))
        })
    }

    #[cfg(unix)]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "a handful of file descriptors"
    )]
    fn poll_fds(&mut self) {
        use clack_extensions::posix_fd::{FdFlags, PluginPosixFd};
        let fds = self.instance.access_handler(|h| h.fds.borrow().clone());
        if fds.is_empty() {
            return;
        }
        let mut pollfds: Vec<libc::pollfd> = fds
            .iter()
            .map(|(fd, flags)| {
                let mut events = 0;
                if flags.contains(FdFlags::READ) {
                    events |= libc::POLLIN;
                }
                if flags.contains(FdFlags::WRITE) {
                    events |= libc::POLLOUT;
                }
                libc::pollfd {
                    fd: *fd,
                    events,
                    revents: 0,
                }
            })
            .collect();
        // SAFETY: `pollfds` is a valid array of the given length; a zero
        // timeout never blocks.
        let ready = unsafe { libc::poll(pollfds.as_mut_ptr(), pollfds.len() as libc::nfds_t, 0) };
        if ready <= 0 {
            return;
        }
        let Some(posix_fd) = self.extension::<PluginPosixFd>() else {
            return;
        };
        for entry in pollfds.iter().filter(|p| p.revents != 0) {
            let mut flags = FdFlags::empty();
            if entry.revents & libc::POLLIN != 0 {
                flags |= FdFlags::READ;
            }
            if entry.revents & libc::POLLOUT != 0 {
                flags |= FdFlags::WRITE;
            }
            if entry.revents & (libc::POLLERR | libc::POLLHUP) != 0 {
                flags |= FdFlags::ERROR;
            }
            posix_fd.on_fd(&self.instance.plugin_handle(), entry.fd, flags);
        }
    }
}

impl MainSide for ClapMain {
    fn params(&mut self) -> Vec<ParamInfo> {
        let Some(params) = self.extension::<PluginParams>() else {
            return Vec::new();
        };
        let handle = self.instance.plugin_handle();
        let mut buffer = ParamInfoBuffer::new();
        (0..params.count(&handle))
            .filter_map(|index| {
                let info = params.get_info(&handle, index, &mut buffer)?;
                if info.flags.contains(ParamInfoFlags::IS_HIDDEN) {
                    return None;
                }
                let span = info.max_value - info.min_value;
                let default = if span > 0.0 {
                    (info.default_value - info.min_value) / span
                } else {
                    0.0
                };
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "a parameter's step count"
                )]
                let steps = if info.flags.contains(ParamInfoFlags::IS_STEPPED) {
                    span.round().max(0.0) as u32
                } else {
                    0
                };
                Some(ParamInfo {
                    id: info.id.get(),
                    name: bytes_text(info.name),
                    module: bytes_text(info.module),
                    default: default.clamp(0.0, 1.0),
                    steps,
                    automatable: info.flags.contains(ParamInfoFlags::IS_AUTOMATABLE)
                        && !info.flags.contains(ParamInfoFlags::IS_READONLY),
                })
            })
            .collect()
    }

    fn param_value(&mut self, id: u32) -> Option<f64> {
        let (min, max) = self.range(id)?;
        let params = self.extension::<PluginParams>()?;
        let value = params.get_value(&self.instance.plugin_handle(), ClapId::new(id))?;
        Some(if max > min {
            ((value - min) / (max - min)).clamp(0.0, 1.0)
        } else {
            0.0
        })
    }

    fn param_text(&mut self, id: u32, value: f64) -> Option<String> {
        let (min, max) = self.range(id)?;
        let params = self.extension::<PluginParams>()?;
        let mut buffer = [0_u8; 128];
        let text = params
            .value_to_text(
                &self.instance.plugin_handle(),
                ClapId::new(id),
                min + value.clamp(0.0, 1.0) * (max - min),
                &mut buffer,
            )
            .ok()?;
        Some(bytes_text(text))
    }

    fn save_state(&mut self) -> Result<Vec<u8>, String> {
        let state = self
            .extension::<PluginState>()
            .ok_or("this plugin cannot save its settings")?;
        let mut bytes = Vec::new();
        state
            .save(&self.instance.plugin_handle(), &mut bytes)
            .map_err(|e| format!("its settings could not be saved: {e}"))?;
        Ok(bytes)
    }

    fn load_state(&mut self, state: &[u8]) -> Result<(), String> {
        let extension = self
            .extension::<PluginState>()
            .ok_or("this plugin cannot load settings")?;
        let mut reader = state;
        extension
            .load(&self.instance.plugin_handle(), &mut reader)
            .map_err(|e| format!("its settings could not be loaded: {e}"))
    }

    fn has_editor(&mut self) -> bool {
        let Some(gui) = self.extension::<PluginGui>() else {
            return false;
        };
        let Some(api_type) = GuiApiType::default_for_current_platform() else {
            return false;
        };
        gui.is_api_supported(
            &self.instance.plugin_handle(),
            GuiConfiguration {
                api_type,
                is_floating: false,
            },
        )
    }

    fn attach_editor(&mut self, parent: ParentHandle) -> Result<(u32, u32), String> {
        let gui = self
            .extension::<PluginGui>()
            .ok_or("this plugin has no editor")?;
        let api_type =
            GuiApiType::default_for_current_platform().ok_or("no editor support on this system")?;
        let configuration = GuiConfiguration {
            api_type,
            is_floating: false,
        };
        let handle = self.instance.plugin_handle();
        if !gui.is_api_supported(&handle, configuration) {
            return Err("this plugin's editor cannot open on this system".to_owned());
        }
        gui.create(&handle, configuration)
            .map_err(|e| format!("its editor could not open: {e}"))?;
        let size = gui.get_size(&handle).unwrap_or(GuiSize {
            width: 800,
            height: 600,
        });
        let window = match parent {
            ParentHandle::X11(id) => Window::from_x11_handle(std::ffi::c_ulong::from(id)),
            // SAFETY: the window belongs to the plugin handle, which closes
            // the editor before the window goes away.
            ParentHandle::Win32(hwnd) => unsafe {
                Window::from_win32_hwnd(hwnd as *mut std::ffi::c_void)
            },
        };
        // SAFETY: the parent window outlives the editor (detach_editor runs
        // before the window is dropped).
        if let Err(error) = unsafe { gui.set_parent(&handle, window) } {
            gui.destroy(&handle);
            return Err(format!("its editor could not open: {error}"));
        }
        let _ = gui.show(&handle);
        self.gui_open = true;
        Ok((size.width, size.height))
    }

    fn detach_editor(&mut self) {
        if !self.gui_open {
            return;
        }
        if let Some(gui) = self.extension::<PluginGui>() {
            gui.destroy(&self.instance.plugin_handle());
        }
        self.gui_open = false;
    }

    fn take_requested_size(&mut self) -> Option<(u32, u32)> {
        let size = self.instance.access_shared_handler(|s| {
            s.requested_size
                .lock()
                .ok()
                .and_then(|mut size| size.take())
        })?;
        Some((size.width, size.height))
    }

    fn editor_closed_by_plugin(&mut self) -> bool {
        self.instance
            .access_shared_handler(|s| s.gui_closed.swap(false, Ordering::AcqRel))
    }

    fn idle(&mut self) {
        let requested = self
            .instance
            .access_shared_handler(|s| s.callback_requested.swap(false, Ordering::AcqRel));
        if requested {
            self.instance.call_on_main_thread_callback();
        }
        let now = Instant::now();
        let due: Vec<u32> = self.instance.access_handler(|h| {
            h.timers
                .borrow_mut()
                .iter_mut()
                .filter(|t| t.next <= now)
                .map(|t| {
                    t.next = now + t.period;
                    t.id
                })
                .collect()
        });
        if !due.is_empty()
            && let Some(timer) = self.extension::<PluginTimer>()
        {
            for id in due {
                timer.on_timer(&self.instance.plugin_handle(), TimerId(id));
            }
        }
        #[cfg(unix)]
        self.poll_fds();
    }

    fn take_touched_param(&mut self) -> Option<u32> {
        self.instance.access_shared_handler(|s| {
            s.touched_set
                .swap(false, Ordering::AcqRel)
                .then(|| s.touched.load(Ordering::Acquire))
        })
    }

    fn take_changed(&mut self) -> bool {
        let dirty = self
            .instance
            .access_handler(|h| h.state_dirty.replace(false));
        let edited = self
            .instance
            .access_shared_handler(|s| s.changed.swap(false, Ordering::AcqRel));
        dirty || edited
    }
}

impl Drop for ClapMain {
    fn drop(&mut self) {
        self.detach_editor();
        // Deactivates now if the audio half has already been dropped; if
        // not, the plugin is cleaned up when it is.
        let _ = self.instance.try_deactivate();
    }
}

fn bytes_text(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).trim().to_owned()
}

// ---------------------------------------------------------------------------
// The audio-thread half

/// The audio-thread half of a CLAP plugin.
pub struct ClapAudio {
    processor: Option<PluginAudioProcessor<Host>>,
    /// One buffer per channel, grouped by port.
    input_buffers: Vec<Vec<Vec<f32>>>,
    output_buffers: Vec<Vec<Vec<f32>>>,
    input_ports: AudioPorts,
    output_ports: AudioPorts,
    events: EventBuffer,
    output_events: EventBuffer,
    /// (id, minimum, maximum, stepped), sorted by id.
    ranges: Vec<(u32, f64, f64, bool)>,
    /// The note port that takes MIDI, if one does.
    midi_port: Option<u16>,
    steady_time: u64,
}

impl ClapAudio {
    fn plain_value(&self, id: u32, fraction: f64) -> Option<f64> {
        let index = self.ranges.binary_search_by_key(&id, |r| r.0).ok()?;
        let (_, min, max, stepped) = self.ranges[index];
        let value = min + fraction.clamp(0.0, 1.0) * (max - min);
        Some(if stepped { value.round() } else { value })
    }
}

impl AudioSide for ClapAudio {
    #[allow(
        clippy::cast_possible_truncation,
        reason = "frame counts within one block"
    )]
    #[allow(
        clippy::too_many_lines,
        reason = "one processing call, prepared in order"
    )]
    fn process(
        &mut self,
        transport: &Transport,
        events: &[Event],
        left: &mut [f32],
        right: &mut [f32],
    ) {
        let len = left.len().min(right.len());
        let max = self
            .output_buffers
            .first()
            .and_then(|port| port.first())
            .map_or(0, Vec::len);
        let len = len.min(max);
        if len == 0 {
            return;
        }
        self.events.clear();
        for event in events.iter().take(MAX_EVENTS) {
            match *event {
                Event::NoteOn {
                    frame,
                    key,
                    velocity,
                } => self.events.push(&NoteOnEvent::new(
                    frame.min(len as u32 - 1),
                    Pckn::new(0u16, 0u16, u16::from(key), u32::from(key)),
                    f64::from(velocity),
                )),
                Event::NoteOff { frame, key } => self.events.push(&NoteOffEvent::new(
                    frame.min(len as u32 - 1),
                    Pckn::new(0u16, 0u16, u16::from(key), u32::from(key)),
                    0.0,
                )),
                Event::Param { frame, id, value } => {
                    if let Some(plain) = self.plain_value(id, value) {
                        self.events.push(&ParamValueEvent::new(
                            frame.min(len as u32 - 1),
                            ClapId::new(id),
                            Pckn::match_all(),
                            plain,
                        ));
                    }
                }
                Event::PitchBend { frame, .. }
                | Event::Controller { frame, .. }
                | Event::Pressure { frame, .. } => {
                    if let (Some(port), Some(bytes)) = (self.midi_port, event.midi_bytes()) {
                        self.events
                            .push(&MidiEvent::new(frame.min(len as u32 - 1), port, bytes));
                    }
                }
            }
        }
        self.output_events.clear();
        // The input, on the first input port (silence elsewhere).
        for (port_index, port) in self.input_buffers.iter_mut().enumerate() {
            for (channel, buffer) in port.iter_mut().enumerate() {
                let source: &[f32] = match (port_index, channel) {
                    (0, 0) => left,
                    (0, _) => right,
                    _ => &[],
                };
                if source.len() >= len {
                    buffer[..len].copy_from_slice(&source[..len]);
                } else {
                    buffer[..len].fill(0.0);
                }
            }
        }
        let Some(processor) = self.processor.as_mut() else {
            return;
        };
        let input_audio = self
            .input_ports
            .with_input_buffers(self.input_buffers.iter_mut().map(|port| {
                AudioPortBuffer {
                    latency: 0,
                    channels: AudioPortBufferType::f32_input_only(
                        port.iter_mut()
                            .map(|b| InputChannel::variable(&mut b[..len])),
                    ),
                }
            }));
        let mut output_audio =
            self.output_ports
                .with_output_buffers(self.output_buffers.iter_mut().map(|port| AudioPortBuffer {
                    latency: 0,
                    channels: AudioPortBufferType::f32_output_only(
                        port.iter_mut().map(|b| &mut b[..len]),
                    ),
                }));
        let steady_time = self.steady_time;
        let transport = transport_event(transport);
        let events = &self.events;
        let output_events = &mut self.output_events;
        let processed = daw_rt::permit_alloc(|| {
            let started = processor.ensure_processing_started().ok()?;
            started
                .process(
                    &input_audio,
                    &mut output_audio,
                    &InputEvents::from_buffer(events),
                    &mut OutputEvents::from_buffer(output_events),
                    Some(steady_time),
                    Some(&transport),
                )
                .ok()
        });
        self.steady_time += len as u64;
        match (processed, self.output_buffers.first()) {
            (Some(_), Some(main)) if !main.is_empty() => {
                left[..len].copy_from_slice(&main[0][..len]);
                let second = usize::from(main.len() > 1);
                right[..len].copy_from_slice(&main[second][..len]);
            }
            _ => {
                left[..len].fill(0.0);
                right[..len].fill(0.0);
            }
        }
        // What the plugin reported: a parameter touched or changed in its
        // editor.
        let mut touched = None;
        let mut changed = false;
        for event in &self.output_events {
            match event.as_core_event() {
                Some(clack_host::events::spaces::CoreEventSpace::ParamGestureBegin(e)) => {
                    touched = e.param_id().map(ClapId::get);
                }
                Some(clack_host::events::spaces::CoreEventSpace::ParamValue(_)) => changed = true,
                _ => {}
            }
        }
        if touched.is_some() || changed {
            processor.access_shared_handler(|s| {
                if let Some(id) = touched {
                    s.touched.store(id, Ordering::Release);
                    s.touched_set.store(true, Ordering::Release);
                }
                if changed {
                    s.changed.store(true, Ordering::Release);
                }
            });
        }
    }

    fn stop(&mut self) {
        if let Some(processor) = self.processor.as_mut() {
            daw_rt::permit_alloc(|| {
                processor.ensure_processing_stopped();
            });
        }
    }
}

/// The song's position as a CLAP transport event.
fn transport_event(transport: &Transport) -> clack_host::events::event_types::TransportEvent {
    use clack_host::events::event_types::{TransportEvent, TransportFlags};
    use clack_host::events::{EventFlags, EventHeader};
    use clack_host::utils::{BeatTime, SecondsTime};
    let mut flags = TransportFlags::HAS_TEMPO
        | TransportFlags::HAS_BEATS_TIMELINE
        | TransportFlags::HAS_TIME_SIGNATURE;
    if transport.playing {
        flags |= TransportFlags::IS_PLAYING;
    }
    TransportEvent {
        header: EventHeader::new_core(0, EventFlags::empty()),
        flags,
        song_pos_beats: BeatTime::from_float(transport.beats),
        song_pos_seconds: SecondsTime::from_int(0),
        tempo: transport.tempo,
        tempo_inc: 0.0,
        loop_start_beats: BeatTime::from_int(0),
        loop_end_beats: BeatTime::from_int(0),
        loop_start_seconds: SecondsTime::from_int(0),
        loop_end_seconds: SecondsTime::from_int(0),
        bar_start: BeatTime::from_float(transport.bar_start),
        bar_number: i32::try_from(transport.bar).unwrap_or(i32::MAX),
        time_signature_numerator: u16::from(transport.numerator),
        time_signature_denominator: u16::from(transport.denominator),
    }
}
