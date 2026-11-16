// SPDX-License-Identifier: GPL-3.0-or-later

//! Hosting a CLAP plugin through `clack-host`.

#![allow(
    unsafe_code,
    reason = "loading a plugin library and handing it a native window are unsafe by nature"
)]

use std::cell::{Cell, OnceCell, RefCell};
use std::ffi::CString;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use clack_extensions::audio_ports::{
    AudioPortInfoBuffer, AudioPortRescanFlags, HostAudioPorts, HostAudioPortsImpl, PluginAudioPorts,
};
use clack_extensions::gui::{
    GuiApiType, GuiConfiguration, GuiSize, HostGui, HostGuiImpl, PluginGui, Window,
};
use clack_extensions::log::{HostLog, HostLogImpl, LogSeverity};
use clack_extensions::note_ports::{
    HostNotePorts, HostNotePortsImpl, NoteDialects, NotePortRescanFlags,
};
use clack_extensions::params::{
    HostParams, HostParamsImplMainThread, HostParamsImplShared, ParamClearFlags, ParamInfoBuffer,
    ParamRescanFlags, PluginParams,
};
use clack_extensions::state::{HostState, HostStateImpl, PluginState};
use clack_extensions::timer::{HostTimer, HostTimerImpl, PluginTimer, TimerId};
use clack_host::events::event_types::{NoteOffEvent, NoteOnEvent, ParamValueEvent};
use clack_host::prelude::*;

use crate::window::{EditorWindow, ParentHandle};
use crate::{BLOCK, HostedPlugin, ParamDescription, SAMPLE_RATE, TimedEvent, for_each_block};

pub struct SpikeHost;

impl HostHandlers for SpikeHost {
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

/// Host data any plugin thread may touch.
pub struct Shared {
    callback_requested: AtomicBool,
    gui_closed: AtomicBool,
    requested_size: Mutex<Option<GuiSize>>,
}

impl SharedHandler<'_> for Shared {
    fn request_restart(&self) {}
    fn request_process(&self) {}
    fn request_callback(&self) {
        self.callback_requested.store(true, Ordering::Release);
    }
}

impl HostLogImpl for Shared {
    #[allow(clippy::print_stderr, reason = "spike: show what the plugin logs")]
    fn log(&self, severity: LogSeverity, message: &str) {
        if severity >= LogSeverity::Warning {
            eprintln!("   [plugin {severity}] {message}");
        }
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

/// Host data for the main thread only.
pub struct MainThread<'a> {
    plugin: OnceCell<InitializedPluginHandle<'a>>,
    timers: RefCell<Vec<Timer>>,
    next_timer_id: Cell<u32>,
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
        self.timers
            .borrow_mut()
            .retain(|timer| timer.id != timer_id.0);
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
        for entry in self.fds.borrow_mut().iter_mut() {
            if entry.0 == fd {
                entry.1 = flags;
            }
        }
        Ok(())
    }

    fn unregister_fd(&self, fd: i32) -> Result<(), HostError> {
        self.fds.borrow_mut().retain(|entry| entry.0 != fd);
        Ok(())
    }
}

impl HostParamsImplMainThread for MainThread<'_> {
    fn rescan(&self, _flags: ParamRescanFlags) {}
    fn clear(&self, _param_id: ClapId, _flags: ParamClearFlags) {}
}

impl HostStateImpl for MainThread<'_> {
    fn mark_dirty(&self) {}
}

impl HostAudioPortsImpl for MainThread<'_> {
    fn is_rescan_flag_supported(&self, _flag: AudioPortRescanFlags) -> bool {
        false
    }
    fn rescan(&self, _flags: AudioPortRescanFlags) {}
}

impl HostNotePortsImpl for MainThread<'_> {
    fn supported_dialects(&self) -> NoteDialects {
        NoteDialects::CLAP
    }
    fn rescan(&self, _flags: NotePortRescanFlags) {}
}

/// Channel counts of the plugin's audio ports.
struct PortLayout {
    inputs: Vec<usize>,
    outputs: Vec<usize>,
}

pub struct ClapPlugin {
    description: String,
    // Field order matters: the instance must go before the entry it came from.
    processor: Option<StoppedPluginAudioProcessor<SpikeHost>>,
    instance: PluginInstance<SpikeHost>,
    _entry: PluginEntry,
    layout: PortLayout,
    pending_params: Vec<(u32, f64)>,
    gui: Option<PluginGui>,
    gui_open: bool,
}

impl ClapPlugin {
    pub fn load(path: &Path) -> Result<Self, String> {
        // SAFETY: loading a plugin runs its code; that is the point of a
        // plugin host. The path names a CLAP bundle chosen by the user.
        let entry = unsafe { PluginEntry::load(path) }
            .map_err(|e| format!("could not load {}: {e}", path.display()))?;
        let factory = entry
            .get_plugin_factory()
            .ok_or("the plugin has no plugin factory")?;
        let descriptor = factory
            .plugin_descriptors()
            .next()
            .ok_or("the plugin file contains no plugin")?;
        let id = CString::from(descriptor.id().ok_or("the plugin has no id")?);
        let text = |value: Option<&std::ffi::CStr>| {
            value.map_or_else(String::new, |v| v.to_string_lossy().into_owned())
        };
        let description = format!(
            "{} {} by {} (CLAP, id {})",
            text(descriptor.name()),
            text(descriptor.version()),
            text(descriptor.vendor()),
            id.to_string_lossy()
        );
        let host_info = HostInfo::new(
            "VLabs plugin spike",
            "VLabs",
            "https://example.invalid",
            "0.0.0",
        )
        .map_err(|e| e.to_string())?;

        let mut instance = PluginInstance::<SpikeHost>::new(
            |()| Shared {
                callback_requested: AtomicBool::new(false),
                gui_closed: AtomicBool::new(false),
                requested_size: Mutex::new(None),
            },
            |_| MainThread {
                plugin: OnceCell::new(),
                timers: RefCell::new(Vec::new()),
                next_timer_id: Cell::new(1),
                #[cfg(unix)]
                fds: RefCell::new(Vec::new()),
            },
            &entry,
            &id,
            &host_info,
        )
        .map_err(|e| format!("could not create the plugin instance: {e}"))?;

        let layout = port_layout(&mut instance);
        let gui = instance.access_handler(|h| {
            h.plugin
                .get()
                .and_then(InitializedPluginHandle::get_extension::<PluginGui>)
        });
        let processor = instance
            .activate(
                |_, _| (),
                PluginAudioConfiguration {
                    sample_rate: f64::from(SAMPLE_RATE),
                    min_frames_count: 1,
                    max_frames_count: BLOCK as u32,
                },
            )
            .map_err(|e| format!("could not activate: {e}"))?;

        Ok(Self {
            description,
            processor: Some(processor),
            instance,
            _entry: entry,
            layout,
            pending_params: Vec::new(),
            gui,
            gui_open: false,
        })
    }

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

    fn handle_main_thread_requests(&mut self) {
        let requested = self
            .instance
            .access_shared_handler(|s| s.callback_requested.swap(false, Ordering::AcqRel));
        if requested {
            self.instance.call_on_main_thread_callback();
        }
    }
}

fn port_layout(instance: &mut PluginInstance<SpikeHost>) -> PortLayout {
    let ports = instance.access_handler(|h| {
        h.plugin
            .get()
            .and_then(InitializedPluginHandle::get_extension::<PluginAudioPorts>)
    });
    let mut layout = PortLayout {
        inputs: Vec::new(),
        outputs: vec![2],
    };
    let Some(ports) = ports else {
        return layout;
    };
    let handle = instance.plugin_handle();
    let mut buffer = AudioPortInfoBuffer::new();
    let mut read = |is_input: bool| -> Vec<usize> {
        (0..ports.count(&handle, is_input))
            .filter_map(|index| {
                ports
                    .get(&handle, index, is_input, &mut buffer)
                    .map(|info| info.channel_count as usize)
            })
            .collect()
    };
    layout.inputs = read(true);
    layout.outputs = read(false);
    layout
}

impl HostedPlugin for ClapPlugin {
    fn description(&self) -> String {
        self.description.clone()
    }

    fn render(&mut self, events: &[TimedEvent], frames: usize) -> Result<[Vec<f32>; 2], String> {
        let processor = self.processor.take().ok_or("the plugin is not active")?;
        let layout_inputs = self.layout.inputs.clone();
        let layout_outputs = self.layout.outputs.clone();
        let params = std::mem::take(&mut self.pending_params);

        // Process on a separate thread, as a real audio thread would.
        let (processor, result) = std::thread::scope(|scope| {
            scope
                .spawn(move || {
                    render_on_audio_thread(
                        processor,
                        &layout_inputs,
                        &layout_outputs,
                        &params,
                        events,
                        frames,
                    )
                })
                .join()
                .map_err(|_| "the processing thread panicked".to_owned())
        })?;
        self.processor = Some(processor);
        self.handle_main_thread_requests();
        result
    }

    fn params(&mut self) -> Vec<ParamDescription> {
        let Some(params) = self.extension::<PluginParams>() else {
            return Vec::new();
        };
        let handle = self.instance.plugin_handle();
        let mut buffer = ParamInfoBuffer::new();
        (0..params.count(&handle))
            .filter_map(|index| {
                params
                    .get_info(&handle, index, &mut buffer)
                    .map(|info| ParamDescription {
                        id: info.id.get(),
                        name: String::from_utf8_lossy(info.name)
                            .trim_end_matches('\0')
                            .to_owned(),
                    })
            })
            .collect()
    }

    fn set_param(&mut self, id: u32, fraction: f64) -> Result<(), String> {
        let (min, max) = param_range(self, id).ok_or("unknown parameter")?;
        self.pending_params.push((id, min + fraction * (max - min)));
        Ok(())
    }

    fn param_fraction(&mut self, id: u32) -> Option<f64> {
        let params = self.extension::<PluginParams>()?;
        let (min, max) = param_range(self, id)?;
        let value = params.get_value(&self.instance.plugin_handle(), ClapId::new(id))?;
        Some((value - min) / (max - min))
    }

    fn save_state(&mut self) -> Result<Vec<u8>, String> {
        let state = self
            .extension::<PluginState>()
            .ok_or("the plugin cannot save its state")?;
        let mut bytes = Vec::new();
        state
            .save(&self.instance.plugin_handle(), &mut bytes)
            .map_err(|e| format!("saving failed: {e}"))?;
        Ok(bytes)
    }

    fn load_state(&mut self, data: &[u8]) -> Result<(), String> {
        let state = self
            .extension::<PluginState>()
            .ok_or("the plugin cannot load a state")?;
        let mut reader = data;
        state
            .load(&self.instance.plugin_handle(), &mut reader)
            .map_err(|e| format!("loading failed: {e}"))
    }

    fn open_editor(&mut self, window: &EditorWindow) -> Result<(u32, u32), String> {
        let gui = self.gui.ok_or("the plugin has no editor")?;
        let api_type =
            GuiApiType::default_for_current_platform().ok_or("no editor API for this system")?;
        let configuration = GuiConfiguration {
            api_type,
            is_floating: false,
        };
        let handle = self.instance.plugin_handle();
        if !gui.is_api_supported(&handle, configuration) {
            return Err(format!(
                "the editor does not support embedding with {api_type:?}"
            ));
        }
        gui.create(&handle, configuration)
            .map_err(|e| format!("create: {e}"))?;
        let size = gui.get_size(&handle).unwrap_or(GuiSize {
            width: 800,
            height: 600,
        });
        let parent = match window.handle() {
            ParentHandle::X11(id) => Window::from_x11_handle(std::ffi::c_ulong::from(id)),
            // SAFETY: the HWND belongs to `window`, which outlives the editor.
            ParentHandle::Win32(hwnd) => unsafe {
                Window::from_win32_hwnd(hwnd as *mut std::ffi::c_void)
            },
        };
        // SAFETY: the parent window stays alive until close_editor() destroys the editor.
        unsafe { gui.set_parent(&handle, parent) }.map_err(|e| format!("set_parent: {e}"))?;
        let _ = gui.show(&handle);
        self.gui_open = true;
        Ok((size.width, size.height))
    }

    fn idle(&mut self) {
        self.handle_main_thread_requests();
        let now = Instant::now();
        let due: Vec<u32> = self.instance.access_handler(|h| {
            h.timers
                .borrow_mut()
                .iter_mut()
                .filter(|timer| timer.next <= now)
                .map(|timer| {
                    timer.next = now + timer.period;
                    timer.id
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

    fn close_editor(&mut self) {
        if let (Some(gui), true) = (self.gui, self.gui_open) {
            gui.destroy(&self.instance.plugin_handle());
            self.gui_open = false;
        }
    }
}

#[cfg(unix)]
impl ClapPlugin {
    #[allow(
        unsafe_code,
        reason = "poll() on file descriptors the plugin registered"
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
        // SAFETY: `pollfds` is a valid, initialised array of the given length.
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

fn param_range(plugin: &mut ClapPlugin, id: u32) -> Option<(f64, f64)> {
    let params = plugin.extension::<PluginParams>()?;
    let handle = plugin.instance.plugin_handle();
    let mut buffer = ParamInfoBuffer::new();
    (0..params.count(&handle)).find_map(|index| {
        let info = params.get_info(&handle, index, &mut buffer)?;
        (info.id.get() == id).then_some((info.min_value, info.max_value))
    })
}

/// Runs on the audio thread: starts processing, renders, stops.
fn render_on_audio_thread(
    processor: StoppedPluginAudioProcessor<SpikeHost>,
    inputs: &[usize],
    outputs: &[usize],
    params: &[(u32, f64)],
    events: &[TimedEvent],
    frames: usize,
) -> (
    StoppedPluginAudioProcessor<SpikeHost>,
    Result<[Vec<f32>; 2], String>,
) {
    let mut started = match processor.start_processing() {
        Ok(started) => started,
        Err(error) => {
            return (
                error.into_stopped_processor(),
                Err("start_processing failed".to_owned()),
            );
        }
    };

    let input_channels: usize = inputs.iter().sum();
    let output_channels: usize = outputs.iter().sum();
    // One buffer per channel, grouped by port.
    let mut input_buffers: Vec<Vec<[f32; BLOCK]>> =
        inputs.iter().map(|&n| vec![[0.0_f32; BLOCK]; n]).collect();
    let mut output_buffers: Vec<Vec<[f32; BLOCK]>> =
        outputs.iter().map(|&n| vec![[0.0_f32; BLOCK]; n]).collect();
    let mut input_ports = AudioPorts::with_capacity(input_channels, inputs.len());
    let mut output_ports = AudioPorts::with_capacity(output_channels, outputs.len());
    let mut event_buffer = EventBuffer::with_capacity(64);
    let mut output_events = EventBuffer::with_capacity(64);
    let mut left = Vec::with_capacity(frames);
    let mut right = Vec::with_capacity(frames);
    let mut params_sent = false;
    let mut steady_time = 0_u64;

    let outcome = for_each_block(events, frames, |_, len, block_events| {
        event_buffer.clear();
        if !params_sent {
            for &(id, value) in params {
                event_buffer.push(&ParamValueEvent::new(
                    0,
                    ClapId::new(id),
                    Pckn::match_all(),
                    value,
                ));
            }
            params_sent = true;
        }
        for event in block_events {
            match *event {
                TimedEvent::NoteOn {
                    frame,
                    key,
                    velocity,
                } => {
                    event_buffer.push(&NoteOnEvent::new(
                        frame as u32,
                        Pckn::new(0u16, 0u16, u16::from(key), u32::from(key)),
                        f64::from(velocity),
                    ));
                }
                TimedEvent::NoteOff { frame, key } => {
                    event_buffer.push(&NoteOffEvent::new(
                        frame as u32,
                        Pckn::new(0u16, 0u16, u16::from(key), u32::from(key)),
                        0.0,
                    ));
                }
            }
        }
        output_events.clear();

        let input_audio = input_ports.with_input_buffers(input_buffers.iter_mut().map(|port| {
            AudioPortBuffer {
                latency: 0,
                channels: AudioPortBufferType::f32_input_only(
                    port.iter_mut()
                        .map(|b| InputChannel::constant(&mut b[..len])),
                ),
            }
        }));
        let mut output_audio =
            output_ports.with_output_buffers(output_buffers.iter_mut().map(|port| {
                AudioPortBuffer {
                    latency: 0,
                    channels: AudioPortBufferType::f32_output_only(
                        port.iter_mut().map(|b| &mut b[..len]),
                    ),
                }
            }));
        started
            .process(
                &input_audio,
                &mut output_audio,
                &InputEvents::from_buffer(&event_buffer),
                &mut OutputEvents::from_buffer(&mut output_events),
                Some(steady_time),
                None,
            )
            .map_err(|e| format!("process: {e}"))?;
        steady_time += len as u64;
        let main = &output_buffers[0];
        left.extend_from_slice(&main[0][..len]);
        right.extend_from_slice(&main[main.len().min(2) - 1][..len]);
        Ok(())
    });
    (started.stop_processing(), outcome.map(|()| [left, right]))
}
