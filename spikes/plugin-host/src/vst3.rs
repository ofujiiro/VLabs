// SPDX-License-Identifier: GPL-3.0-or-later

//! Hosting a VST3 plugin on the `vst3` bindings.
//!
//! The bindings are a thin, unsafe layer over the C++ interfaces, so the
//! host side lives here: the objects a VST3 plugin expects from its host
//! (application, event list, parameter changes, memory stream, editor frame
//! and, on Linux, the editor run loop), and the call sequence to load,
//! connect, activate, process, save and restore a plugin.

#![allow(
    unsafe_code,
    reason = "VST3 is a C++ COM-style interface; every call into the plugin is unsafe"
)]
#![allow(
    non_snake_case,
    reason = "the trait methods mirror the VST3 interface names"
)]
#![allow(
    clippy::unnecessary_cast,
    reason = "the VST3 enum types are unsigned on Linux and signed on Windows; the casts are needed on one of them"
)]
#![allow(
    clippy::undocumented_unsafe_blocks,
    reason = "spike: the module-level comment covers the COM calls"
)]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_void};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
#[cfg(target_os = "linux")]
use std::time::{Duration, Instant};

use vst3::Steinberg::Vst::{
    AudioBusBuffers, AudioBusBuffers__type0, BusDirections_, BusInfo, Event, Event_, Event__type0,
    IAttributeList, IAttributeListTrait, IAudioProcessor, IAudioProcessorTrait, IComponent,
    IComponentHandler, IComponentHandlerTrait, IComponentTrait, IConnectionPoint,
    IConnectionPointTrait, IEditController, IEditControllerTrait, IEventList, IEventListTrait,
    IHostApplication, IHostApplicationTrait, IMessage, IMessageTrait, IParamValueQueue,
    IParamValueQueueTrait, IParameterChanges, IParameterChangesTrait, MediaTypes_, NoteOffEvent,
    NoteOnEvent, ParameterInfo, ProcessContext, ProcessData, ProcessModes_, ProcessSetup,
    String128, SymbolicSampleSizes_, TChar,
};
use vst3::Steinberg::{
    FUnknown, IBStream, IBStream_, IBStreamTrait, IPlugFrame, IPlugFrameTrait, IPlugView,
    IPlugViewTrait, IPluginBaseTrait, IPluginFactory, IPluginFactory3, IPluginFactory3Trait,
    IPluginFactoryTrait, PClassInfo, TUID, ViewRect, int32, int64, kInvalidArgument, kNoInterface,
    kNotImplemented, kResultFalse, kResultOk, kResultTrue, tresult, uint32,
};
use vst3::{Class, ComPtr, ComRef, ComWrapper, Interface};

use crate::window::{EditorWindow, ParentHandle};
use crate::{BLOCK, HostedPlugin, ParamDescription, SAMPLE_RATE, TimedEvent, for_each_block};

// ---------------------------------------------------------------------------
// Small helpers

fn utf16_to_string(text: &[TChar]) -> String {
    let len = text.iter().position(|&c| c == 0).unwrap_or(text.len());
    String::from_utf16_lossy(&text[..len])
}

fn ascii_field(text: &[c_char]) -> String {
    let bytes: Vec<u8> = text
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn check(result: tresult, what: &str) -> Result<(), String> {
    if result == kResultOk {
        Ok(())
    } else {
        Err(format!("{what} failed (result {result})"))
    }
}

fn iid_ptr<I: Interface>() -> *const c_char {
    I::IID.as_ptr().cast()
}

// ---------------------------------------------------------------------------
// Host objects the plugin talks to

/// The host application object handed to the plugin at initialisation.
struct HostApplication;

impl Class for HostApplication {
    type Interfaces = (IHostApplication,);
}

impl IHostApplicationTrait for HostApplication {
    unsafe fn getName(&self, name: *mut String128) -> tresult {
        let text: Vec<u16> = "Plugin hosting spike".encode_utf16().collect();
        let name = unsafe { &mut *name };
        name.fill(0);
        name[..text.len()].copy_from_slice(&text);
        kResultOk
    }

    unsafe fn createInstance(
        &self,
        _cid: *mut TUID,
        iid: *mut TUID,
        obj: *mut *mut c_void,
    ) -> tresult {
        let iid = unsafe { &*(iid as *const [u8; 16]) };
        if *iid == IMessage::IID {
            let message = ComWrapper::new(HostMessage::default());
            unsafe {
                *obj = message
                    .to_com_ptr::<IMessage>()
                    .map_or(null_mut(), |p| p.into_raw().cast());
            }
            kResultOk
        } else if *iid == IAttributeList::IID {
            let list = ComWrapper::new(AttributeList::default());
            unsafe {
                *obj = list
                    .to_com_ptr::<IAttributeList>()
                    .map_or(null_mut(), |p| p.into_raw().cast());
            }
            kResultOk
        } else {
            unsafe { *obj = null_mut() };
            kNoInterface
        }
    }
}

/// A message between the plugin's processor and its controller.
#[derive(Default)]
struct HostMessage {
    id: RefCell<Option<CString>>,
    attributes: RefCell<Option<ComWrapper<AttributeList>>>,
}

impl Class for HostMessage {
    type Interfaces = (IMessage,);
}

impl IMessageTrait for HostMessage {
    unsafe fn getMessageID(&self) -> *const c_char {
        self.id.borrow().as_ref().map_or(null(), |id| id.as_ptr())
    }

    unsafe fn setMessageID(&self, id: *const c_char) {
        let id = (!id.is_null()).then(|| unsafe { CStr::from_ptr(id) }.to_owned());
        *self.id.borrow_mut() = id;
    }

    unsafe fn getAttributes(&self) -> *mut IAttributeList {
        let mut attributes = self.attributes.borrow_mut();
        let list = attributes.get_or_insert_with(|| ComWrapper::new(AttributeList::default()));
        list.as_com_ref::<IAttributeList>()
            .map_or(null_mut(), |r| r.as_ptr())
    }
}

enum Attribute {
    Int(i64),
    Float(f64),
    Text(Vec<u16>),
    Binary(Vec<u8>),
}

#[derive(Default)]
struct AttributeList {
    values: RefCell<HashMap<CString, Attribute>>,
}

impl Class for AttributeList {
    type Interfaces = (IAttributeList,);
}

fn attribute_key(id: *const c_char) -> Option<CString> {
    (!id.is_null()).then(|| unsafe { CStr::from_ptr(id) }.to_owned())
}

impl IAttributeListTrait for AttributeList {
    unsafe fn setInt(&self, id: *const c_char, value: int64) -> tresult {
        let Some(key) = attribute_key(id) else {
            return kInvalidArgument;
        };
        self.values.borrow_mut().insert(key, Attribute::Int(value));
        kResultOk
    }

    unsafe fn getInt(&self, id: *const c_char, value: *mut int64) -> tresult {
        let Some(key) = attribute_key(id) else {
            return kInvalidArgument;
        };
        match self.values.borrow().get(&key) {
            Some(Attribute::Int(v)) => {
                unsafe { *value = *v };
                kResultOk
            }
            _ => kResultFalse,
        }
    }

    unsafe fn setFloat(&self, id: *const c_char, value: f64) -> tresult {
        let Some(key) = attribute_key(id) else {
            return kInvalidArgument;
        };
        self.values
            .borrow_mut()
            .insert(key, Attribute::Float(value));
        kResultOk
    }

    unsafe fn getFloat(&self, id: *const c_char, value: *mut f64) -> tresult {
        let Some(key) = attribute_key(id) else {
            return kInvalidArgument;
        };
        match self.values.borrow().get(&key) {
            Some(Attribute::Float(v)) => {
                unsafe { *value = *v };
                kResultOk
            }
            _ => kResultFalse,
        }
    }

    unsafe fn setString(&self, id: *const c_char, string: *const TChar) -> tresult {
        let Some(key) = attribute_key(id) else {
            return kInvalidArgument;
        };
        let mut text = Vec::new();
        let mut index = 0;
        while !string.is_null() && unsafe { *string.add(index) } != 0 {
            text.push(unsafe { *string.add(index) });
            index += 1;
        }
        self.values.borrow_mut().insert(key, Attribute::Text(text));
        kResultOk
    }

    unsafe fn getString(
        &self,
        id: *const c_char,
        string: *mut TChar,
        size_in_bytes: uint32,
    ) -> tresult {
        let Some(key) = attribute_key(id) else {
            return kInvalidArgument;
        };
        match self.values.borrow().get(&key) {
            Some(Attribute::Text(text)) => {
                let capacity = size_in_bytes as usize / 2;
                if capacity == 0 {
                    return kResultFalse;
                }
                let n = text.len().min(capacity - 1);
                unsafe {
                    std::ptr::copy_nonoverlapping(text.as_ptr(), string, n);
                    *string.add(n) = 0;
                }
                kResultOk
            }
            _ => kResultFalse,
        }
    }

    unsafe fn setBinary(
        &self,
        id: *const c_char,
        data: *const c_void,
        size_in_bytes: uint32,
    ) -> tresult {
        let Some(key) = attribute_key(id) else {
            return kInvalidArgument;
        };
        let bytes = if data.is_null() {
            Vec::new()
        } else {
            unsafe { std::slice::from_raw_parts(data.cast::<u8>(), size_in_bytes as usize) }
                .to_vec()
        };
        self.values
            .borrow_mut()
            .insert(key, Attribute::Binary(bytes));
        kResultOk
    }

    unsafe fn getBinary(
        &self,
        id: *const c_char,
        data: *mut *const c_void,
        size_in_bytes: *mut uint32,
    ) -> tresult {
        let Some(key) = attribute_key(id) else {
            return kInvalidArgument;
        };
        match self.values.borrow().get(&key) {
            Some(Attribute::Binary(bytes)) => {
                // The data stays owned by the list, as the interface requires.
                unsafe {
                    *data = bytes.as_ptr().cast();
                    *size_in_bytes = bytes.len() as u32;
                }
                kResultOk
            }
            _ => kResultFalse,
        }
    }
}

/// Receives parameter edits made in the plugin's own editor.
#[derive(Default)]
struct ComponentHandler {
    edits: Cell<usize>,
}

impl Class for ComponentHandler {
    type Interfaces = (IComponentHandler,);
}

impl IComponentHandlerTrait for ComponentHandler {
    unsafe fn beginEdit(&self, _id: u32) -> tresult {
        kResultOk
    }
    unsafe fn performEdit(&self, _id: u32, _value: f64) -> tresult {
        self.edits.set(self.edits.get() + 1);
        kResultOk
    }
    unsafe fn endEdit(&self, _id: u32) -> tresult {
        kResultOk
    }
    unsafe fn restartComponent(&self, _flags: int32) -> tresult {
        kResultOk
    }
}

/// A byte stream in memory, for saving and loading state.
#[derive(Default)]
struct MemoryStream {
    data: RefCell<Vec<u8>>,
    position: Cell<usize>,
}

impl Class for MemoryStream {
    type Interfaces = (IBStream,);
}

impl MemoryStream {
    fn with_data(data: &[u8]) -> Self {
        Self {
            data: RefCell::new(data.to_vec()),
            position: Cell::new(0),
        }
    }
}

impl IBStreamTrait for MemoryStream {
    unsafe fn read(
        &self,
        buffer: *mut c_void,
        num_bytes: int32,
        num_bytes_read: *mut int32,
    ) -> tresult {
        let data = self.data.borrow();
        let start = self.position.get().min(data.len());
        let n = (num_bytes.max(0) as usize).min(data.len() - start);
        unsafe { std::ptr::copy_nonoverlapping(data.as_ptr().add(start), buffer.cast::<u8>(), n) };
        self.position.set(start + n);
        if !num_bytes_read.is_null() {
            unsafe { *num_bytes_read = n as int32 };
        }
        kResultOk
    }

    unsafe fn write(
        &self,
        buffer: *mut c_void,
        num_bytes: int32,
        num_bytes_written: *mut int32,
    ) -> tresult {
        let n = num_bytes.max(0) as usize;
        let bytes = unsafe { std::slice::from_raw_parts(buffer.cast::<u8>(), n) };
        let mut data = self.data.borrow_mut();
        let start = self.position.get();
        if data.len() < start + n {
            data.resize(start + n, 0);
        }
        data[start..start + n].copy_from_slice(bytes);
        self.position.set(start + n);
        if !num_bytes_written.is_null() {
            unsafe { *num_bytes_written = n as int32 };
        }
        kResultOk
    }

    unsafe fn seek(&self, pos: int64, mode: int32, result: *mut int64) -> tresult {
        let len = self.data.borrow().len() as i64;
        let base = match mode as IBStream_::IStreamSeekMode {
            IBStream_::IStreamSeekMode_::kIBSeekSet => 0,
            IBStream_::IStreamSeekMode_::kIBSeekCur => self.position.get() as i64,
            IBStream_::IStreamSeekMode_::kIBSeekEnd => len,
            _ => return kInvalidArgument,
        };
        let target = (base + pos).clamp(0, len);
        self.position.set(target as usize);
        if !result.is_null() {
            unsafe { *result = target };
        }
        kResultOk
    }

    unsafe fn tell(&self, pos: *mut int64) -> tresult {
        if !pos.is_null() {
            unsafe { *pos = self.position.get() as int64 };
        }
        kResultOk
    }
}

/// Note events for one processing block.
struct EventList {
    events: RefCell<Vec<Event>>,
}

impl Class for EventList {
    type Interfaces = (IEventList,);
}

impl IEventListTrait for EventList {
    unsafe fn getEventCount(&self) -> int32 {
        self.events.borrow().len() as int32
    }

    unsafe fn getEvent(&self, index: int32, e: *mut Event) -> tresult {
        match self.events.borrow().get(index as usize) {
            Some(event) => {
                unsafe { std::ptr::copy_nonoverlapping(event, e, 1) };
                kResultOk
            }
            None => kInvalidArgument,
        }
    }

    unsafe fn addEvent(&self, e: *mut Event) -> tresult {
        let event = unsafe { std::ptr::read(e) };
        self.events.borrow_mut().push(event);
        kResultOk
    }
}

/// Parameter changes for one processing block.
#[derive(Default)]
struct ParameterChanges {
    queues: RefCell<Vec<ComWrapper<ParamValueQueue>>>,
}

impl Class for ParameterChanges {
    type Interfaces = (IParameterChanges,);
}

impl ParameterChanges {
    fn add(&self, id: u32, offset: i32, value: f64) {
        let queue = ComWrapper::new(ParamValueQueue {
            id,
            points: RefCell::new(vec![(offset, value)]),
        });
        self.queues.borrow_mut().push(queue);
    }
}

impl IParameterChangesTrait for ParameterChanges {
    unsafe fn getParameterCount(&self) -> int32 {
        self.queues.borrow().len() as int32
    }

    unsafe fn getParameterData(&self, index: int32) -> *mut IParamValueQueue {
        self.queues
            .borrow()
            .get(index as usize)
            .and_then(|queue| queue.as_com_ref::<IParamValueQueue>().map(|r| r.as_ptr()))
            .unwrap_or(null_mut())
    }

    unsafe fn addParameterData(&self, id: *const u32, index: *mut int32) -> *mut IParamValueQueue {
        let id = unsafe { *id };
        let mut queues = self.queues.borrow_mut();
        queues.push(ComWrapper::new(ParamValueQueue {
            id,
            points: RefCell::new(Vec::new()),
        }));
        if !index.is_null() {
            unsafe { *index = (queues.len() - 1) as int32 };
        }
        queues
            .last()
            .and_then(|q| q.as_com_ref::<IParamValueQueue>().map(|r| r.as_ptr()))
            .unwrap_or(null_mut())
    }
}

struct ParamValueQueue {
    id: u32,
    points: RefCell<Vec<(i32, f64)>>,
}

impl Class for ParamValueQueue {
    type Interfaces = (IParamValueQueue,);
}

impl IParamValueQueueTrait for ParamValueQueue {
    unsafe fn getParameterId(&self) -> u32 {
        self.id
    }

    unsafe fn getPointCount(&self) -> int32 {
        self.points.borrow().len() as int32
    }

    unsafe fn getPoint(&self, index: int32, sample_offset: *mut int32, value: *mut f64) -> tresult {
        match self.points.borrow().get(index as usize) {
            Some(&(offset, v)) => {
                unsafe {
                    *sample_offset = offset;
                    *value = v;
                }
                kResultOk
            }
            None => kInvalidArgument,
        }
    }

    unsafe fn addPoint(&self, sample_offset: int32, value: f64, index: *mut int32) -> tresult {
        let mut points = self.points.borrow_mut();
        points.push((sample_offset, value));
        if !index.is_null() {
            unsafe { *index = (points.len() - 1) as int32 };
        }
        kResultOk
    }
}

// ---------------------------------------------------------------------------
// The editor frame (and, on Linux, the run loop the editor needs)

#[cfg(target_os = "linux")]
use vst3::Steinberg::Linux::{
    IEventHandler, IEventHandlerTrait, IRunLoop, IRunLoopTrait, ITimerHandler, ITimerHandlerTrait,
};

#[derive(Default)]
struct PlugFrame {
    requested_size: Cell<Option<(u32, u32)>>,
    #[cfg(target_os = "linux")]
    fd_handlers: RefCell<Vec<(ComPtr<IEventHandler>, i32)>>,
    #[cfg(target_os = "linux")]
    timers: RefCell<Vec<(ComPtr<ITimerHandler>, Duration, Instant)>>,
}

#[cfg(target_os = "linux")]
impl Class for PlugFrame {
    type Interfaces = (IPlugFrame, IRunLoop);
}

#[cfg(not(target_os = "linux"))]
impl Class for PlugFrame {
    type Interfaces = (IPlugFrame,);
}

impl IPlugFrameTrait for PlugFrame {
    unsafe fn resizeView(&self, view: *mut IPlugView, new_size: *mut ViewRect) -> tresult {
        if new_size.is_null() {
            return kInvalidArgument;
        }
        let rect = unsafe { &*new_size };
        self.requested_size.set(Some((
            (rect.right - rect.left).max(1) as u32,
            (rect.bottom - rect.top).max(1) as u32,
        )));
        // Tell the view its new size, as the protocol requires.
        if !view.is_null() {
            unsafe { ((*(*view).vtbl).onSize)(view, new_size) };
        }
        kResultOk
    }
}

#[cfg(target_os = "linux")]
impl IRunLoopTrait for PlugFrame {
    unsafe fn registerEventHandler(&self, handler: *mut IEventHandler, fd: i32) -> tresult {
        let Some(handler) = (unsafe { ComRef::from_raw(handler) }) else {
            return kInvalidArgument;
        };
        self.fd_handlers
            .borrow_mut()
            .push((handler.to_com_ptr(), fd));
        kResultOk
    }

    unsafe fn unregisterEventHandler(&self, handler: *mut IEventHandler) -> tresult {
        self.fd_handlers
            .borrow_mut()
            .retain(|(h, _)| h.as_ptr() != handler);
        kResultOk
    }

    unsafe fn registerTimer(&self, handler: *mut ITimerHandler, milliseconds: u64) -> tresult {
        let Some(handler) = (unsafe { ComRef::from_raw(handler) }) else {
            return kInvalidArgument;
        };
        let period = Duration::from_millis(milliseconds.max(1));
        self.timers
            .borrow_mut()
            .push((handler.to_com_ptr(), period, Instant::now() + period));
        kResultOk
    }

    unsafe fn unregisterTimer(&self, handler: *mut ITimerHandler) -> tresult {
        self.timers
            .borrow_mut()
            .retain(|(h, _, _)| h.as_ptr() != handler);
        kResultOk
    }
}

impl PlugFrame {
    /// Runs what the editor registered: ready file descriptors and due timers.
    #[cfg(target_os = "linux")]
    fn run_once(&self) {
        // Copy the lists first: handlers may register or unregister while running.
        let handlers: Vec<(ComPtr<IEventHandler>, i32)> = self.fd_handlers.borrow().clone();
        if !handlers.is_empty() {
            let mut pollfds: Vec<libc::pollfd> = handlers
                .iter()
                .map(|(_, fd)| libc::pollfd {
                    fd: *fd,
                    events: libc::POLLIN,
                    revents: 0,
                })
                .collect();
            let ready =
                unsafe { libc::poll(pollfds.as_mut_ptr(), pollfds.len() as libc::nfds_t, 0) };
            if ready > 0 {
                for ((handler, fd), poll) in handlers.iter().zip(&pollfds) {
                    if poll.revents != 0 {
                        unsafe { handler.onFDIsSet(*fd) };
                    }
                }
            }
        }
        let now = Instant::now();
        let due: Vec<ComPtr<ITimerHandler>> = self
            .timers
            .borrow_mut()
            .iter_mut()
            .filter(|(_, _, next)| *next <= now)
            .map(|(handler, period, next)| {
                *next = now + *period;
                handler.clone()
            })
            .collect();
        for handler in due {
            unsafe { handler.onTimer() };
        }
    }

    #[cfg(not(target_os = "linux"))]
    #[allow(
        clippy::unused_self,
        reason = "Windows editors run on the window message loop instead"
    )]
    fn run_once(&self) {}
}

// ---------------------------------------------------------------------------
// Loading the module

struct Module {
    factory: Option<ComPtr<IPluginFactory>>,
    library: Option<libloading::Library>,
}

impl Module {
    fn factory(&self) -> &ComPtr<IPluginFactory> {
        self.factory
            .as_ref()
            .expect("the factory lives as long as the module")
    }
}

impl Drop for Module {
    fn drop(&mut self) {
        // The factory must be released before the module exits.
        drop(self.factory.take());
        let Some(library) = self.library.take() else {
            return;
        };
        unsafe {
            let exit_name: &[u8] = if cfg!(windows) {
                b"ExitDll\0"
            } else {
                b"ModuleExit\0"
            };
            if let Ok(exit) = library.get::<unsafe extern "system" fn() -> bool>(exit_name) {
                exit();
            }
        }
        // Unloading plugin libraries is a common source of crashes at exit
        // (threads still running inside them); keep it loaded, as most hosts do.
        std::mem::forget(library);
    }
}

fn binary_path(bundle: &Path) -> Result<PathBuf, String> {
    let stem = bundle
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("bad bundle name")?;
    let path = if cfg!(windows) {
        bundle
            .join("Contents")
            .join("x86_64-win")
            .join(format!("{stem}.vst3"))
    } else {
        bundle
            .join("Contents")
            .join("x86_64-linux")
            .join(format!("{stem}.so"))
    };
    if path.exists() {
        Ok(path)
    } else if bundle.is_file() {
        Ok(bundle.to_owned()) // A single-file plugin.
    } else {
        Err(format!("{} not found inside the bundle", path.display()))
    }
}

fn load_module(bundle: &Path) -> Result<Module, String> {
    let binary = binary_path(bundle)?;
    unsafe {
        let library = libloading::Library::new(&binary)
            .map_err(|e| format!("cannot load {}: {e}", binary.display()))?;
        #[cfg(unix)]
        let library = {
            let raw = libloading::os::unix::Library::from(library).into_raw();
            if let Ok(entry) = libloading::os::unix::Library::from_raw(raw)
                .get::<unsafe extern "C" fn(*mut c_void) -> bool>(b"ModuleEntry\0")
                && !entry(raw)
            {
                return Err("ModuleEntry returned false".to_owned());
            }
            libloading::Library::from(libloading::os::unix::Library::from_raw(raw))
        };
        #[cfg(windows)]
        if let Ok(init) = library.get::<unsafe extern "system" fn() -> bool>(b"InitDll\0")
            && !init()
        {
            return Err("InitDll returned false".to_owned());
        }
        let get_factory = library
            .get::<unsafe extern "system" fn() -> *mut IPluginFactory>(b"GetPluginFactory\0")
            .map_err(|e| format!("not a VST3 plugin (no GetPluginFactory): {e}"))?;
        let factory = ComPtr::from_raw(get_factory()).ok_or("GetPluginFactory returned nothing")?;
        Ok(Module {
            factory: Some(factory),
            library: Some(library),
        })
    }
}

// ---------------------------------------------------------------------------
// The hosted plugin

pub struct Vst3Plugin {
    description: String,
    view: Option<ComPtr<IPlugView>>,
    frame: ComWrapper<PlugFrame>,
    processor: ComPtr<IAudioProcessor>,
    controller: ComPtr<IEditController>,
    component: ComPtr<IComponent>,
    connection: Option<(ComPtr<IConnectionPoint>, ComPtr<IConnectionPoint>)>,
    separate_controller: bool,
    inputs: Vec<i32>,
    outputs: Vec<i32>,
    pending_params: Vec<(u32, f64)>,
    _handler: ComWrapper<ComponentHandler>,
    _host: ComWrapper<HostApplication>,
    module: Module,
}

impl Vst3Plugin {
    #[allow(
        clippy::too_many_lines,
        reason = "the VST3 setup sequence reads best in order"
    )]
    pub fn load(bundle: &Path) -> Result<Self, String> {
        let module = load_module(bundle)?;
        let host = ComWrapper::new(HostApplication);
        let host_unknown = host
            .as_com_ref::<IHostApplication>()
            .ok_or("host object")?
            .as_ptr()
            .cast::<FUnknown>();

        unsafe {
            if let Some(factory3) = module.factory().cast::<IPluginFactory3>() {
                factory3.setHostContext(host_unknown);
            }

            // Find the processor class ("Audio Module Class").
            let mut class = None;
            for index in 0..module.factory().countClasses() {
                let mut info: PClassInfo = std::mem::zeroed();
                if module.factory().getClassInfo(index, &raw mut info) == kResultOk
                    && ascii_field(&info.category) == "Audio Module Class"
                {
                    class = Some(info);
                    break;
                }
            }
            let class = class.ok_or("the plugin contains no audio module")?;
            let description = format!("{} (VST3)", ascii_field(&class.name));

            let mut object = null_mut();
            check(
                module.factory().createInstance(
                    class.cid.as_ptr(),
                    iid_ptr::<IComponent>(),
                    &raw mut object,
                ),
                "creating the component",
            )?;
            let component = ComPtr::from_raw(object.cast::<IComponent>()).ok_or("no component")?;
            check(
                component.initialize(host_unknown),
                "initialising the component",
            )?;

            // The controller: a separate object, or the component itself.
            let mut controller_cid: TUID = [0; 16];
            let mut separate_controller = false;
            let controller = if component.getControllerClassId(&raw mut controller_cid) == kResultOk
                && controller_cid.iter().any(|&b| b != 0)
            {
                let mut object = null_mut();
                check(
                    module.factory().createInstance(
                        controller_cid.as_ptr(),
                        iid_ptr::<IEditController>(),
                        &raw mut object,
                    ),
                    "creating the controller",
                )?;
                let controller =
                    ComPtr::from_raw(object.cast::<IEditController>()).ok_or("no controller")?;
                check(
                    controller.initialize(host_unknown),
                    "initialising the controller",
                )?;
                separate_controller = true;
                controller
            } else {
                component
                    .cast::<IEditController>()
                    .ok_or("the plugin has no edit controller")?
            };

            // Connect processor and controller so they can message each other.
            let connection = if separate_controller {
                match (
                    component.cast::<IConnectionPoint>(),
                    controller.cast::<IConnectionPoint>(),
                ) {
                    (Some(a), Some(b)) => {
                        a.connect(b.as_ptr());
                        b.connect(a.as_ptr());
                        Some((a, b))
                    }
                    _ => None,
                }
            } else {
                None
            };

            // Give the controller the processor's initial state.
            let stream = ComWrapper::new(MemoryStream::default());
            let stream_ptr = stream.as_com_ref::<IBStream>().ok_or("stream")?.as_ptr();
            if component.getState(stream_ptr) == kResultOk {
                stream.position.set(0);
                controller.setComponentState(stream_ptr);
            }

            let handler = ComWrapper::new(ComponentHandler::default());
            controller.setComponentHandler(
                handler
                    .as_com_ref::<IComponentHandler>()
                    .ok_or("handler")?
                    .as_ptr(),
            );

            // Buses: activate the main audio buses and the first note input.
            let bus_channels = |direction: i32| -> Vec<i32> {
                (0..component.getBusCount(MediaTypes_::kAudio as i32, direction))
                    .map(|index| {
                        let mut info: BusInfo = std::mem::zeroed();
                        component.getBusInfo(
                            MediaTypes_::kAudio as i32,
                            direction,
                            index,
                            &raw mut info,
                        );
                        info.channelCount
                    })
                    .collect()
            };
            let inputs = bus_channels(BusDirections_::kInput as i32);
            let outputs = bus_channels(BusDirections_::kOutput as i32);
            if outputs.is_empty() {
                return Err("the plugin has no audio output".to_owned());
            }
            for (direction, count) in [
                (BusDirections_::kInput, inputs.len()),
                (BusDirections_::kOutput, outputs.len()),
            ] {
                if count > 0 {
                    component.activateBus(MediaTypes_::kAudio as i32, direction as i32, 0, 1);
                }
            }
            if component.getBusCount(MediaTypes_::kEvent as i32, BusDirections_::kInput as i32) > 0
            {
                component.activateBus(
                    MediaTypes_::kEvent as i32,
                    BusDirections_::kInput as i32,
                    0,
                    1,
                );
            }

            let processor = component
                .cast::<IAudioProcessor>()
                .ok_or("the plugin has no audio processor")?;
            let mut setup = ProcessSetup {
                processMode: ProcessModes_::kRealtime as i32,
                symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
                maxSamplesPerBlock: BLOCK as i32,
                sampleRate: f64::from(SAMPLE_RATE),
            };
            check(processor.setupProcessing(&raw mut setup), "setupProcessing")?;
            check(component.setActive(1), "activating")?;

            Ok(Self {
                description,
                view: None,
                frame: ComWrapper::new(PlugFrame::default()),
                processor,
                controller,
                component,
                connection,
                separate_controller,
                inputs,
                outputs,
                pending_params: Vec::new(),
                _handler: handler,
                _host: host,
                module,
            })
        }
    }
}

/// A pointer that may cross to the processing thread for the duration of a
/// render, while the main thread waits.
struct SendPtr<T>(*mut T);
// SAFETY: used only while the owning thread is blocked in `thread::scope`.
unsafe impl<T> Send for SendPtr<T> {}

impl HostedPlugin for Vst3Plugin {
    fn description(&self) -> String {
        self.description.clone()
    }

    fn render(&mut self, events: &[TimedEvent], frames: usize) -> Result<[Vec<f32>; 2], String> {
        let processor = SendPtr(self.processor.as_ptr());
        let inputs = self.inputs.clone();
        let outputs = self.outputs.clone();
        let params = std::mem::take(&mut self.pending_params);
        std::thread::scope(|scope| {
            scope
                .spawn(move || {
                    let processor = processor;
                    render_on_audio_thread(processor.0, &inputs, &outputs, &params, events, frames)
                })
                .join()
                .map_err(|_| "the processing thread panicked".to_owned())?
        })
    }

    fn params(&mut self) -> Vec<ParamDescription> {
        unsafe {
            (0..self.controller.getParameterCount())
                .filter_map(|index| {
                    let mut info: ParameterInfo = std::mem::zeroed();
                    (self.controller.getParameterInfo(index, &raw mut info) == kResultOk).then(
                        || ParamDescription {
                            id: info.id,
                            name: utf16_to_string(&info.title),
                        },
                    )
                })
                .collect()
        }
    }

    fn set_param(&mut self, id: u32, fraction: f64) -> Result<(), String> {
        unsafe { self.controller.setParamNormalized(id, fraction) };
        self.pending_params.push((id, fraction));
        Ok(())
    }

    fn param_fraction(&mut self, id: u32) -> Option<f64> {
        Some(unsafe { self.controller.getParamNormalized(id) })
    }

    fn save_state(&mut self) -> Result<Vec<u8>, String> {
        let component_state = ComWrapper::new(MemoryStream::default());
        let controller_state = ComWrapper::new(MemoryStream::default());
        unsafe {
            check(
                self.component.getState(
                    component_state
                        .as_com_ref::<IBStream>()
                        .ok_or("stream")?
                        .as_ptr(),
                ),
                "saving the processor state",
            )?;
            if self.separate_controller {
                let _ = self.controller.getState(
                    controller_state
                        .as_com_ref::<IBStream>()
                        .ok_or("stream")?
                        .as_ptr(),
                );
            }
        }
        // Our container: two length-prefixed blocks.
        let a = component_state.data.borrow();
        let b = controller_state.data.borrow();
        let mut bytes = Vec::with_capacity(8 + a.len() + b.len());
        bytes.extend_from_slice(&(a.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&a);
        bytes.extend_from_slice(&(b.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&b);
        Ok(bytes)
    }

    fn load_state(&mut self, data: &[u8]) -> Result<(), String> {
        let take = |data: &[u8], at: usize| -> Result<(Vec<u8>, usize), String> {
            let len_bytes: [u8; 4] = data
                .get(at..at + 4)
                .ok_or("truncated state")?
                .try_into()
                .map_err(|_| "bad state")?;
            let len = u32::from_le_bytes(len_bytes) as usize;
            let block = data
                .get(at + 4..at + 4 + len)
                .ok_or("truncated state")?
                .to_vec();
            Ok((block, at + 4 + len))
        };
        let (component_state, next) = take(data, 0)?;
        let (controller_state, _) = take(data, next)?;
        unsafe {
            let stream = ComWrapper::new(MemoryStream::with_data(&component_state));
            let ptr = stream.as_com_ref::<IBStream>().ok_or("stream")?.as_ptr();
            check(
                self.component.setState(ptr),
                "restoring the processor state",
            )?;
            stream.position.set(0);
            let _ = self.controller.setComponentState(ptr);
            if self.separate_controller && !controller_state.is_empty() {
                let stream = ComWrapper::new(MemoryStream::with_data(&controller_state));
                let _ = self
                    .controller
                    .setState(stream.as_com_ref::<IBStream>().ok_or("stream")?.as_ptr());
            }
        }
        Ok(())
    }

    fn open_editor(&mut self, window: &EditorWindow) -> Result<(u32, u32), String> {
        unsafe {
            let view = self.controller.createView(c"editor".as_ptr());
            let view = ComPtr::from_raw(view).ok_or("the plugin has no editor")?;
            let (platform, parent) = match window.handle() {
                ParentHandle::X11(id) => (
                    vst3::Steinberg::kPlatformTypeX11EmbedWindowID,
                    id as usize as *mut c_void,
                ),
                ParentHandle::Win32(hwnd) => {
                    (vst3::Steinberg::kPlatformTypeHWND, hwnd as *mut c_void)
                }
            };
            if view.isPlatformTypeSupported(platform) != kResultTrue {
                return Err("the editor does not support this window system".to_owned());
            }
            view.setFrame(
                self.frame
                    .as_com_ref::<IPlugFrame>()
                    .ok_or("frame")?
                    .as_ptr(),
            );
            let mut rect: ViewRect = std::mem::zeroed();
            view.getSize(&raw mut rect);
            check(view.attached(parent, platform), "attaching the editor")?;
            let size = (
                (rect.right - rect.left).max(1) as u32,
                (rect.bottom - rect.top).max(1) as u32,
            );
            self.view = Some(view);
            Ok(size)
        }
    }

    fn idle(&mut self) {
        self.frame.run_once();
    }

    fn close_editor(&mut self) {
        if let Some(view) = self.view.take() {
            unsafe {
                view.removed();
                view.setFrame(null_mut());
            }
        }
    }
}

impl Drop for Vst3Plugin {
    fn drop(&mut self) {
        self.close_editor();
        unsafe {
            self.component.setActive(0);
            if let Some((a, b)) = self.connection.take() {
                a.disconnect(b.as_ptr());
                b.disconnect(a.as_ptr());
            }
            if self.separate_controller {
                self.controller.terminate();
            }
            self.component.terminate();
        }
        // `module` is dropped last (fields drop in declaration order), after
        // every interface pointer above has been released.
        let _ = &self.module;
    }
}

/// Runs on the audio thread: processes the whole render block by block.
#[allow(
    clippy::too_many_lines,
    reason = "the VST3 process setup reads best in one place"
)]
fn render_on_audio_thread(
    processor: *mut IAudioProcessor,
    inputs: &[i32],
    outputs: &[i32],
    params: &[(u32, f64)],
    events: &[TimedEvent],
    frames: usize,
) -> Result<[Vec<f32>; 2], String> {
    let processor = unsafe { ComRef::from_raw(processor) }.ok_or("no processor")?;
    unsafe { processor.setProcessing(1) };

    // One buffer per channel, and the pointer arrays VST3 wants per bus.
    let mut input_buffers: Vec<Vec<f32>> = inputs
        .iter()
        .flat_map(|&n| (0..n).map(|_| vec![0.0_f32; BLOCK]))
        .collect();
    let mut output_buffers: Vec<Vec<f32>> = outputs
        .iter()
        .flat_map(|&n| (0..n).map(|_| vec![0.0_f32; BLOCK]))
        .collect();
    let mut input_pointers: Vec<*mut f32> = input_buffers.iter_mut().map(Vec::as_mut_ptr).collect();
    let mut output_pointers: Vec<*mut f32> =
        output_buffers.iter_mut().map(Vec::as_mut_ptr).collect();
    let bus_buffers = |counts: &[i32], pointers: &mut Vec<*mut f32>| -> Vec<AudioBusBuffers> {
        let mut offset = 0;
        counts
            .iter()
            .map(|&n| {
                let bus = AudioBusBuffers {
                    numChannels: n,
                    silenceFlags: 0,
                    __field0: AudioBusBuffers__type0 {
                        channelBuffers32: unsafe { pointers.as_mut_ptr().add(offset) },
                    },
                };
                offset += n as usize;
                bus
            })
            .collect()
    };
    let mut input_buses = bus_buffers(inputs, &mut input_pointers);
    let mut output_buses = bus_buffers(outputs, &mut output_pointers);

    let event_list = ComWrapper::new(EventList {
        events: RefCell::new(Vec::with_capacity(64)),
    });
    let parameter_changes = ComWrapper::new(ParameterChanges::default());
    let empty_changes = ComWrapper::new(ParameterChanges::default());
    let mut context: ProcessContext = unsafe { std::mem::zeroed() };
    context.sampleRate = f64::from(SAMPLE_RATE);
    context.tempo = 120.0;
    context.timeSigNumerator = 4;
    context.timeSigDenominator = 4;
    // kPlaying | kTempoValid | kTimeSigValid
    context.state = 2 | (1 << 10) | (1 << 13);

    for &(id, value) in params {
        parameter_changes.add(id, 0, value);
    }
    let mut first_block = true;
    let mut left = Vec::with_capacity(frames);
    let mut right = Vec::with_capacity(frames);

    let outcome = for_each_block(events, frames, |start, len, block_events| {
        {
            let mut list = event_list.events.borrow_mut();
            list.clear();
            for event in block_events {
                let mut e: Event = unsafe { std::mem::zeroed() };
                match *event {
                    TimedEvent::NoteOn {
                        frame,
                        key,
                        velocity,
                    } => {
                        e.sampleOffset = frame as i32;
                        e.r#type = Event_::EventTypes_::kNoteOnEvent as u16;
                        e.__field0 = Event__type0 {
                            noteOn: NoteOnEvent {
                                channel: 0,
                                pitch: i16::from(key),
                                tuning: 0.0,
                                velocity,
                                length: 0,
                                noteId: -1,
                            },
                        };
                    }
                    TimedEvent::NoteOff { frame, key } => {
                        e.sampleOffset = frame as i32;
                        e.r#type = Event_::EventTypes_::kNoteOffEvent as u16;
                        e.__field0 = Event__type0 {
                            noteOff: NoteOffEvent {
                                channel: 0,
                                pitch: i16::from(key),
                                velocity: 0.0,
                                noteId: -1,
                                tuning: 0.0,
                            },
                        };
                    }
                }
                list.push(e);
            }
        }
        context.projectTimeSamples = start as i64;
        context.continousTimeSamples = start as i64;
        let changes = if first_block {
            &parameter_changes
        } else {
            &empty_changes
        };
        first_block = false;
        let mut data = ProcessData {
            processMode: ProcessModes_::kRealtime as i32,
            symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
            numSamples: len as i32,
            numInputs: input_buses.len() as i32,
            numOutputs: output_buses.len() as i32,
            inputs: if input_buses.is_empty() {
                null_mut()
            } else {
                input_buses.as_mut_ptr()
            },
            outputs: output_buses.as_mut_ptr(),
            inputParameterChanges: changes
                .as_com_ref::<IParameterChanges>()
                .map_or(null_mut(), |r| r.as_ptr()),
            outputParameterChanges: null_mut(),
            inputEvents: event_list
                .as_com_ref::<IEventList>()
                .map_or(null_mut(), |r| r.as_ptr()),
            outputEvents: null_mut(),
            processContext: &raw mut context,
        };
        let result = unsafe { processor.process(&raw mut data) };
        if result != kResultOk && result != kNotImplemented {
            return Err(format!("process failed (result {result})"));
        }
        left.extend_from_slice(&output_buffers[0][..len]);
        right.extend_from_slice(&output_buffers[outputs[0].clamp(1, 2) as usize - 1][..len]);
        Ok(())
    });
    unsafe { processor.setProcessing(0) };
    outcome.map(|()| [left, right])
}
