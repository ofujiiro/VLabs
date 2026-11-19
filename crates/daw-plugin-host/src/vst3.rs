// SPDX-License-Identifier: GPL-3.0-or-later

//! VST3 plugins, on the `vst3` bindings.
//!
//! The bindings are a thin layer over the C++ interfaces, so the host side
//! lives here: the objects a VST3 plugin expects from its host (application,
//! messages and attribute lists, a component handler that receives edits
//! made in the plugin's editor, memory streams, event lists and parameter
//! changes, the editor's frame and, on Linux, its run loop), and the call
//! sequence to load, connect, activate, process, save and restore a plugin.
//!
//! The objects used while processing (event list, parameter changes) are
//! created with all their memory when the plugin loads, so processing does
//! not allocate. Plugin libraries stay loaded for the life of the program.

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
    reason = "every unsafe block here calls a VST3 interface method or reads memory the interface hands us; the module comment explains the contract"
)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "VST3 uses 32-bit signed counts and sizes for small values"
)]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_void};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::sync::Mutex;
#[cfg(target_os = "linux")]
use std::time::{Duration, Instant};

use vst3::Steinberg::Vst::{
    AudioBusBuffers, AudioBusBuffers__type0, BusDirections_, BusInfo, Event as VstEvent, Event_,
    Event__type0, IAttributeList, IAttributeListTrait, IAudioProcessor, IAudioProcessorTrait,
    IComponent, IComponentHandler, IComponentHandlerTrait, IComponentTrait, IConnectionPoint,
    IConnectionPointTrait, IEditController, IEditControllerTrait, IEventList, IEventListTrait,
    IHostApplication, IHostApplicationTrait, IMessage, IMessageTrait, IParamValueQueue,
    IParamValueQueueTrait, IParameterChanges, IParameterChangesTrait, MediaTypes_, NoteOffEvent,
    NoteOnEvent, ParameterInfo, ProcessContext, ProcessData, ProcessModes_, ProcessSetup,
    SpeakerArrangement, String128, SymbolicSampleSizes_, TChar,
};
use vst3::Steinberg::{
    FUnknown, IBStream, IBStream_, IBStreamTrait, IPlugFrame, IPlugFrameTrait, IPlugView,
    IPlugViewTrait, IPluginBaseTrait, IPluginFactory, IPluginFactory2, IPluginFactory2Trait,
    IPluginFactory3, IPluginFactory3Trait, IPluginFactoryTrait, PClassInfo, PClassInfo2,
    PFactoryInfo, TUID, ViewRect, int32, int64, kInvalidArgument, kNoInterface, kResultFalse,
    kResultOk, kResultTrue, tresult, uint32,
};
use vst3::{Class, ComPtr, ComRef, ComWrapper, Interface};

use crate::window::ParentHandle;
use crate::{
    AudioConfig, AudioSide, Event, Kind, MAX_EVENTS, MainSide, ParamInfo, PluginInfo, Transport,
};

/// Parameter changes travelling between threads: (id, value 0 to 1).
type ParamQueueOut = rtrb::Consumer<(u32, f64)>;
type ParamQueueIn = rtrb::Producer<(u32, f64)>;

/// Parameters changed in one block, at most (more are dropped).
const MAX_CHANGED_PARAMS: usize = 64;
/// Changes of one parameter in one block, at most.
const MAX_POINTS: usize = 32;
/// Edits from the plugin's editor waiting for the audio thread.
const EDIT_QUEUE: usize = 1024;

// ---------------------------------------------------------------------------
// Small helpers

fn utf16_to_string(text: &[TChar]) -> String {
    let len = text.iter().position(|&c| c == 0).unwrap_or(text.len());
    let units: Vec<u16> = text[..len].iter().map(|&c| c as u16).collect();
    String::from_utf16_lossy(&units)
}

fn ascii_field(text: &[c_char]) -> String {
    let bytes: Vec<u8> = text
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).trim().to_owned()
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

/// A class id as 32 hexadecimal digits.
fn hex(cid: &TUID) -> String {
    let mut text = String::with_capacity(32);
    for &byte in cid {
        let _ = write!(text, "{:02X}", byte as u8);
    }
    text
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
        if name.is_null() {
            return kInvalidArgument;
        }
        let text: Vec<u16> = crate::identity().name.encode_utf16().take(127).collect();
        let name = unsafe { &mut *name };
        name.fill(0);
        for (slot, unit) in name.iter_mut().zip(&text) {
            *slot = *unit as TChar;
        }
        kResultOk
    }

    unsafe fn createInstance(
        &self,
        _cid: *mut TUID,
        iid: *mut TUID,
        obj: *mut *mut c_void,
    ) -> tresult {
        if iid.is_null() || obj.is_null() {
            return kInvalidArgument;
        }
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
    Text(Vec<TChar>),
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
            Some(Attribute::Int(v)) if !value.is_null() => {
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
            Some(Attribute::Float(v)) if !value.is_null() => {
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
        if !string.is_null() {
            let mut index = 0;
            while unsafe { *string.add(index) } != 0 {
                text.push(unsafe { *string.add(index) });
                index += 1;
            }
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
            Some(Attribute::Text(text)) if !string.is_null() => {
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
            Some(Attribute::Binary(bytes)) if !data.is_null() && !size_in_bytes.is_null() => {
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

/// Receives the edits made in the plugin's own editor and passes them on to
/// the audio thread (the processor learns of them only through the host).
struct ComponentHandler {
    edits: RefCell<rtrb::Producer<(u32, f64)>>,
    touched: Cell<Option<u32>>,
    changed: Cell<bool>,
}

impl Class for ComponentHandler {
    type Interfaces = (IComponentHandler,);
}

impl IComponentHandlerTrait for ComponentHandler {
    unsafe fn beginEdit(&self, id: u32) -> tresult {
        self.touched.set(Some(id));
        kResultOk
    }
    unsafe fn performEdit(&self, id: u32, value: f64) -> tresult {
        let _ = self.edits.borrow_mut().push((id, value));
        self.changed.set(true);
        kResultOk
    }
    unsafe fn endEdit(&self, _id: u32) -> tresult {
        kResultOk
    }
    unsafe fn restartComponent(&self, _flags: int32) -> tresult {
        self.changed.set(true);
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
        if buffer.is_null() {
            return kInvalidArgument;
        }
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
        if buffer.is_null() {
            return kInvalidArgument;
        }
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

/// Note events for one processing block, with room reserved up front.
struct EventList {
    events: RefCell<Vec<VstEvent>>,
}

impl Class for EventList {
    type Interfaces = (IEventList,);
}

impl IEventListTrait for EventList {
    unsafe fn getEventCount(&self) -> int32 {
        self.events.borrow().len() as int32
    }

    unsafe fn getEvent(&self, index: int32, e: *mut VstEvent) -> tresult {
        if e.is_null() {
            return kInvalidArgument;
        }
        match self.events.borrow().get(index as usize) {
            Some(event) => {
                unsafe { std::ptr::copy_nonoverlapping(event, e, 1) };
                kResultOk
            }
            None => kInvalidArgument,
        }
    }

    unsafe fn addEvent(&self, e: *mut VstEvent) -> tresult {
        if e.is_null() {
            return kInvalidArgument;
        }
        let mut events = self.events.borrow_mut();
        if events.len() == events.capacity() {
            return kResultFalse; // Full: never grow while processing.
        }
        events.push(unsafe { std::ptr::read(e) });
        kResultOk
    }
}

/// Parameter changes for one processing block: a fixed pool of queues.
struct ParameterChanges {
    queues: Vec<ComWrapper<ParamValueQueue>>,
    used: Cell<usize>,
}

impl Class for ParameterChanges {
    type Interfaces = (IParameterChanges,);
}

impl ParameterChanges {
    fn new() -> Self {
        Self {
            queues: (0..MAX_CHANGED_PARAMS)
                .map(|_| {
                    ComWrapper::new(ParamValueQueue {
                        id: Cell::new(0),
                        points: RefCell::new(Vec::with_capacity(MAX_POINTS)),
                    })
                })
                .collect(),
            used: Cell::new(0),
        }
    }

    fn clear(&self) {
        for queue in &self.queues[..self.used.get()] {
            queue.points.borrow_mut().clear();
        }
        self.used.set(0);
    }

    /// Adds a change at a frame; dropped if the pool is full.
    fn add(&self, id: u32, offset: i32, value: f64) {
        let used = self.used.get();
        let queue = match self.queues[..used].iter().find(|q| q.id.get() == id) {
            Some(queue) => queue,
            None if used < self.queues.len() => {
                let queue = &self.queues[used];
                queue.id.set(id);
                self.used.set(used + 1);
                queue
            }
            None => return,
        };
        let mut points = queue.points.borrow_mut();
        if points.len() < points.capacity() {
            points.push((offset, value));
        }
    }
}

impl IParameterChangesTrait for ParameterChanges {
    unsafe fn getParameterCount(&self) -> int32 {
        self.used.get() as int32
    }

    unsafe fn getParameterData(&self, index: int32) -> *mut IParamValueQueue {
        let index = index as usize;
        if index >= self.used.get() {
            return null_mut();
        }
        self.queues[index]
            .as_com_ref::<IParamValueQueue>()
            .map_or(null_mut(), |r| r.as_ptr())
    }

    unsafe fn addParameterData(&self, id: *const u32, index: *mut int32) -> *mut IParamValueQueue {
        if id.is_null() {
            return null_mut();
        }
        let id = unsafe { *id };
        let used = self.used.get();
        let position = match self.queues[..used].iter().position(|q| q.id.get() == id) {
            Some(position) => position,
            None if used < self.queues.len() => {
                self.queues[used].id.set(id);
                self.queues[used].points.borrow_mut().clear();
                self.used.set(used + 1);
                used
            }
            None => return null_mut(),
        };
        if !index.is_null() {
            unsafe { *index = position as int32 };
        }
        self.queues[position]
            .as_com_ref::<IParamValueQueue>()
            .map_or(null_mut(), |r| r.as_ptr())
    }
}

struct ParamValueQueue {
    id: Cell<u32>,
    points: RefCell<Vec<(i32, f64)>>,
}

impl Class for ParamValueQueue {
    type Interfaces = (IParamValueQueue,);
}

impl IParamValueQueueTrait for ParamValueQueue {
    unsafe fn getParameterId(&self) -> u32 {
        self.id.get()
    }

    unsafe fn getPointCount(&self) -> int32 {
        self.points.borrow().len() as int32
    }

    unsafe fn getPoint(&self, index: int32, sample_offset: *mut int32, value: *mut f64) -> tresult {
        if sample_offset.is_null() || value.is_null() {
            return kInvalidArgument;
        }
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
        if points.len() == points.capacity() {
            return kResultFalse;
        }
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
        if let Some(view) = unsafe { ComRef::from_raw(view) } {
            unsafe { view.onSize(new_size) };
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
    /// Runs what the editor registered: ready file descriptors and due
    /// timers.
    #[cfg(target_os = "linux")]
    fn run_once(&self) {
        // Copy the lists first: handlers may register or unregister while
        // they run.
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
// Loading libraries

/// A loaded plugin library and its factory, kept for the life of the
/// program.
struct Module {
    factory: ComPtr<IPluginFactory>,
}

/// A module that may be shared between threads: plugin libraries are only
/// ever used from main threads, one call at a time behind the modules lock.
struct SharedModule(&'static Module);
// SAFETY: see above; the factory pointer itself is never mutated by us.
unsafe impl Send for SharedModule {}

static MODULES: Mutex<Option<HashMap<PathBuf, SharedModule>>> = Mutex::new(None);

fn binary_path(bundle: &Path) -> Result<PathBuf, String> {
    if bundle.is_file() {
        return Ok(bundle.to_owned()); // A single-file plugin.
    }
    let stem = bundle
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("a bad bundle name")?;
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
    } else {
        Err(format!(
            "{} has no plugin for this system inside",
            crate::shown(bundle).display()
        ))
    }
}

fn module(bundle: &Path) -> Result<&'static Module, String> {
    let mut modules = MODULES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let modules = modules.get_or_insert_with(HashMap::new);
    if let Some(module) = modules.get(bundle) {
        return Ok(module.0);
    }
    let binary = binary_path(bundle)?;
    let shown = crate::shown(&binary);
    // SAFETY: loading a plugin runs its code; that is what a plugin host is
    // for. The entry points are called as the VST3 module protocol says.
    let factory = unsafe {
        let library = libloading::Library::new(&binary).map_err(|e| {
            format!(
                "{} could not be loaded: {}",
                shown.display(),
                crate::load_error(&e)
            )
        })?;
        #[cfg(unix)]
        let library = {
            // ModuleEntry is given the library's own handle. The handle is
            // taken out and put straight back, so the library stays open.
            let raw = libloading::os::unix::Library::from(library).into_raw();
            let library = libloading::os::unix::Library::from_raw(raw);
            if let Ok(entry) =
                library.get::<unsafe extern "C" fn(*mut c_void) -> bool>(b"ModuleEntry\0")
                && !entry(raw)
            {
                return Err(format!("{} refused to start", shown.display()));
            }
            libloading::Library::from(library)
        };
        #[cfg(windows)]
        if let Ok(init) = library.get::<unsafe extern "system" fn() -> bool>(b"InitDll\0")
            && !init()
        {
            return Err(format!("{} refused to start", shown.display()));
        }
        let get_factory = library
            .get::<unsafe extern "system" fn() -> *mut IPluginFactory>(b"GetPluginFactory\0")
            .map_err(|_| format!("{} is not a VST3 plugin", shown.display()))?;
        let factory = ComPtr::from_raw(get_factory())
            .ok_or_else(|| format!("{} has no plugin factory", shown.display()))?;
        // Never unloaded: plugins often leave threads running inside.
        std::mem::forget(library);
        factory
    };
    let module: &'static Module = Box::leak(Box::new(Module { factory }));
    modules.insert(bundle.to_path_buf(), SharedModule(module));
    Ok(module)
}

/// The plugins in a VST3 bundle, from the factory's class descriptions (no
/// instance is created).
pub fn describe(bundle: &Path) -> Result<Vec<PluginInfo>, String> {
    let module = module(bundle)?;
    let factory = &module.factory;
    let mut plugins = Vec::new();
    unsafe {
        let mut factory_info: PFactoryInfo = std::mem::zeroed();
        let factory_vendor = if factory.getFactoryInfo(&raw mut factory_info) == kResultOk {
            ascii_field(&factory_info.vendor)
        } else {
            String::new()
        };
        let factory2 = factory.cast::<IPluginFactory2>();
        for index in 0..factory.countClasses() {
            let mut info: PClassInfo = std::mem::zeroed();
            if factory.getClassInfo(index, &raw mut info) != kResultOk
                || ascii_field(&info.category) != "Audio Module Class"
            {
                continue;
            }
            let mut vendor = factory_vendor.clone();
            let mut version = String::new();
            let mut sub_categories = String::new();
            if let Some(factory2) = &factory2 {
                let mut info2: PClassInfo2 = std::mem::zeroed();
                if factory2.getClassInfo2(index, &raw mut info2) == kResultOk {
                    let class_vendor = ascii_field(&info2.vendor);
                    if !class_vendor.is_empty() {
                        vendor = class_vendor;
                    }
                    version = ascii_field(&info2.version);
                    sub_categories = ascii_field(&info2.subCategories);
                }
            }
            let categories: Vec<&str> = sub_categories.split('|').collect();
            // A class with no categories is taken for an effect.
            let kind = if categories.contains(&"Instrument") {
                Kind::Instrument
            } else if categories.contains(&"Fx") || sub_categories.is_empty() {
                Kind::Effect
            } else {
                Kind::Other
            };
            plugins.push(PluginInfo {
                format: crate::Format::Vst3,
                path: bundle.to_path_buf(),
                id: hex(&info.cid),
                name: ascii_field(&info.name),
                vendor,
                version,
                kind,
            });
        }
    }
    Ok(plugins)
}

// ---------------------------------------------------------------------------
// Loading a plugin

/// Loads a plugin, connects its halves and activates it.
#[allow(
    clippy::too_many_lines,
    reason = "the VST3 setup sequence reads best in order"
)]
pub fn load(info: &PluginInfo, config: AudioConfig) -> Result<(Vst3Main, Vst3Audio), String> {
    let module = module(&info.path)?;
    let factory = &module.factory;
    let host = ComWrapper::new(HostApplication);
    let host_unknown = host
        .as_com_ref::<IHostApplication>()
        .ok_or("host object")?
        .as_ptr()
        .cast::<FUnknown>();
    let (producer, consumer) = rtrb::RingBuffer::new(EDIT_QUEUE);
    // Values the song changes, from the audio thread back to the controller,
    // so the plugin's editor shows them.
    let (song_values, song_values_in) = rtrb::RingBuffer::new(EDIT_QUEUE);
    let handler = ComWrapper::new(ComponentHandler {
        edits: RefCell::new(producer),
        touched: Cell::new(None),
        changed: Cell::new(false),
    });

    unsafe {
        if let Some(factory3) = factory.cast::<IPluginFactory3>() {
            factory3.setHostContext(host_unknown);
        }
        let mut class = None;
        for index in 0..factory.countClasses() {
            let mut class_info: PClassInfo = std::mem::zeroed();
            if factory.getClassInfo(index, &raw mut class_info) == kResultOk
                && hex(&class_info.cid) == info.id
            {
                class = Some(class_info);
                break;
            }
        }
        let class = class.ok_or_else(|| format!("{} is no longer in its file", info.name))?;

        let mut object = null_mut();
        check(
            factory.createInstance(class.cid.as_ptr(), iid_ptr::<IComponent>(), &raw mut object),
            "creating the plugin",
        )?;
        let component = ComPtr::from_raw(object.cast::<IComponent>()).ok_or("no component")?;
        check(component.initialize(host_unknown), "starting the plugin")?;

        // The controller: a separate object, or the component itself.
        let mut controller_cid: TUID = [0; 16];
        let mut separate_controller = false;
        let controller = if component.getControllerClassId(&raw mut controller_cid) == kResultOk
            && controller_cid.iter().any(|&b| b != 0)
        {
            let mut object = null_mut();
            check(
                factory.createInstance(
                    controller_cid.as_ptr(),
                    iid_ptr::<IEditController>(),
                    &raw mut object,
                ),
                "creating the plugin's controller",
            )?;
            let controller =
                ComPtr::from_raw(object.cast::<IEditController>()).ok_or("no controller")?;
            check(
                controller.initialize(host_unknown),
                "starting the plugin's controller",
            )?;
            separate_controller = true;
            controller
        } else {
            component
                .cast::<IEditController>()
                .ok_or("the plugin has no controller")?
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
        controller.setComponentHandler(
            handler
                .as_com_ref::<IComponentHandler>()
                .ok_or("handler")?
                .as_ptr(),
        );

        // Buses: the main audio buses and the first note input.
        let bus_channels = |direction: i32| -> Vec<usize> {
            (0..component.getBusCount(MediaTypes_::kAudio as i32, direction))
                .map(|index| {
                    let mut bus: BusInfo = std::mem::zeroed();
                    component.getBusInfo(
                        MediaTypes_::kAudio as i32,
                        direction,
                        index,
                        &raw mut bus,
                    );
                    bus.channelCount.max(0) as usize
                })
                .collect()
        };
        let inputs = bus_channels(BusDirections_::kInput as i32);
        let outputs = bus_channels(BusDirections_::kOutput as i32);
        if outputs.is_empty() {
            return Err(format!("{} has no audio output", info.name));
        }
        let processor = component
            .cast::<IAudioProcessor>()
            .ok_or("the plugin has no audio processor")?;
        // Tell the plugin its speaker layouts before preparing it, as hosts
        // do: each bus as the plugin offers it.
        let layouts = |direction: i32, count: usize| -> Vec<SpeakerArrangement> {
            (0..count)
                .map(|index| {
                    let mut layout: SpeakerArrangement = 0;
                    processor.getBusArrangement(direction, index as i32, &raw mut layout);
                    layout
                })
                .collect()
        };
        let mut input_layouts = layouts(BusDirections_::kInput as i32, inputs.len());
        let mut output_layouts = layouts(BusDirections_::kOutput as i32, outputs.len());
        processor.setBusArrangements(
            if input_layouts.is_empty() {
                null_mut()
            } else {
                input_layouts.as_mut_ptr()
            },
            input_layouts.len() as i32,
            output_layouts.as_mut_ptr(),
            output_layouts.len() as i32,
        );
        for (direction, count) in [
            (BusDirections_::kInput, inputs.len()),
            (BusDirections_::kOutput, outputs.len()),
        ] {
            if count > 0 {
                component.activateBus(MediaTypes_::kAudio as i32, direction as i32, 0, 1);
            }
        }
        if component.getBusCount(MediaTypes_::kEvent as i32, BusDirections_::kInput as i32) > 0 {
            component.activateBus(
                MediaTypes_::kEvent as i32,
                BusDirections_::kInput as i32,
                0,
                1,
            );
        }

        let max_block = config.max_block.max(1);
        let mut setup = ProcessSetup {
            processMode: ProcessModes_::kRealtime as i32,
            symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
            maxSamplesPerBlock: max_block as i32,
            sampleRate: f64::from(config.sample_rate),
        };
        check(
            processor.setupProcessing(&raw mut setup),
            "preparing the plugin",
        )?;
        check(component.setActive(1), "activating the plugin")?;

        let audio = Vst3Audio::new(
            processor,
            &inputs,
            &outputs,
            max_block,
            config,
            (consumer, song_values),
        );
        let main = Vst3Main {
            song_values: song_values_in,
            view: None,
            frame: ComWrapper::new(PlugFrame::default()),
            controller,
            component,
            connection,
            separate_controller,
            handler,
            _host: host,
        };
        Ok((main, audio))
    }
}

// ---------------------------------------------------------------------------
// The main-thread half

/// The main-thread half of a VST3 plugin.
pub struct Vst3Main {
    song_values: rtrb::Consumer<(u32, f64)>,
    view: Option<ComPtr<IPlugView>>,
    frame: ComWrapper<PlugFrame>,
    controller: ComPtr<IEditController>,
    component: ComPtr<IComponent>,
    connection: Option<(ComPtr<IConnectionPoint>, ComPtr<IConnectionPoint>)>,
    separate_controller: bool,
    handler: ComWrapper<ComponentHandler>,
    _host: ComWrapper<HostApplication>,
}

impl MainSide for Vst3Main {
    fn params(&mut self) -> Vec<ParamInfo> {
        use vst3::Steinberg::Vst::ParameterInfo_::ParameterFlags_;
        unsafe {
            (0..self.controller.getParameterCount())
                .filter_map(|index| {
                    let mut info: ParameterInfo = std::mem::zeroed();
                    if self.controller.getParameterInfo(index, &raw mut info) != kResultOk {
                        return None;
                    }
                    let flags = info.flags;
                    if flags & ParameterFlags_::kIsHidden as int32 != 0 {
                        return None;
                    }
                    Some(ParamInfo {
                        id: info.id,
                        name: utf16_to_string(&info.title),
                        module: String::new(),
                        default: info.defaultNormalizedValue.clamp(0.0, 1.0),
                        steps: info.stepCount.max(0) as u32,
                        automatable: flags & ParameterFlags_::kCanAutomate as int32 != 0
                            && flags & ParameterFlags_::kIsReadOnly as int32 == 0,
                    })
                })
                .collect()
        }
    }

    fn param_value(&mut self, id: u32) -> Option<f64> {
        Some(unsafe { self.controller.getParamNormalized(id) }.clamp(0.0, 1.0))
    }

    fn param_text(&mut self, id: u32, value: f64) -> Option<String> {
        let mut text: String128 = [0; 128];
        let result = unsafe {
            self.controller
                .getParamStringByValue(id, value.clamp(0.0, 1.0), &raw mut text)
        };
        (result == kResultOk).then(|| utf16_to_string(&text))
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
                "saving the plugin's settings",
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

    fn load_state(&mut self, state: &[u8]) -> Result<(), String> {
        let take = |data: &[u8], at: usize| -> Result<(Vec<u8>, usize), String> {
            let length: [u8; 4] = data
                .get(at..at + 4)
                .ok_or("the settings are incomplete")?
                .try_into()
                .map_err(|_| "the settings are damaged")?;
            let length = u32::from_le_bytes(length) as usize;
            let block = data
                .get(at + 4..at + 4 + length)
                .ok_or("the settings are incomplete")?
                .to_vec();
            Ok((block, at + 4 + length))
        };
        let (component_state, next) = take(state, 0)?;
        let (controller_state, _) = take(state, next)?;
        unsafe {
            let stream = ComWrapper::new(MemoryStream::with_data(&component_state));
            let pointer = stream.as_com_ref::<IBStream>().ok_or("stream")?.as_ptr();
            check(
                self.component.setState(pointer),
                "loading the plugin's settings",
            )?;
            stream.position.set(0);
            let _ = self.controller.setComponentState(pointer);
            if self.separate_controller && !controller_state.is_empty() {
                let stream = ComWrapper::new(MemoryStream::with_data(&controller_state));
                let _ = self
                    .controller
                    .setState(stream.as_com_ref::<IBStream>().ok_or("stream")?.as_ptr());
            }
        }
        Ok(())
    }

    fn has_editor(&mut self) -> bool {
        unsafe {
            let view = self.controller.createView(c"editor".as_ptr());
            let Some(view) = ComPtr::from_raw(view) else {
                return false;
            };
            let platform = if cfg!(windows) {
                vst3::Steinberg::kPlatformTypeHWND
            } else {
                vst3::Steinberg::kPlatformTypeX11EmbedWindowID
            };
            view.isPlatformTypeSupported(platform) == kResultTrue
        }
    }

    fn attach_editor(&mut self, parent: ParentHandle) -> Result<(u32, u32), String> {
        unsafe {
            let view = self.controller.createView(c"editor".as_ptr());
            let view = ComPtr::from_raw(view).ok_or("this plugin has no editor")?;
            let (platform, parent) = match parent {
                ParentHandle::X11(id) => (
                    vst3::Steinberg::kPlatformTypeX11EmbedWindowID,
                    id as usize as *mut c_void,
                ),
                ParentHandle::Win32(hwnd) => {
                    (vst3::Steinberg::kPlatformTypeHWND, hwnd as *mut c_void)
                }
            };
            if view.isPlatformTypeSupported(platform) != kResultTrue {
                return Err("this plugin's editor cannot open on this system".to_owned());
            }
            view.setFrame(
                self.frame
                    .as_com_ref::<IPlugFrame>()
                    .ok_or("frame")?
                    .as_ptr(),
            );
            let mut rect: ViewRect = std::mem::zeroed();
            view.getSize(&raw mut rect);
            if view.attached(parent, platform) != kResultOk {
                view.setFrame(null_mut());
                return Err("this plugin's editor could not open".to_owned());
            }
            let size = (
                (rect.right - rect.left).max(1) as u32,
                (rect.bottom - rect.top).max(1) as u32,
            );
            self.view = Some(view);
            Ok(size)
        }
    }

    fn detach_editor(&mut self) {
        if let Some(view) = self.view.take() {
            unsafe {
                view.removed();
                view.setFrame(null_mut());
            }
        }
    }

    fn take_requested_size(&mut self) -> Option<(u32, u32)> {
        self.frame.requested_size.take()
    }

    fn editor_closed_by_plugin(&mut self) -> bool {
        false
    }

    fn idle(&mut self) {
        while let Ok((id, value)) = self.song_values.pop() {
            unsafe {
                self.controller.setParamNormalized(id, value);
            }
        }
        self.frame.run_once();
    }

    fn take_touched_param(&mut self) -> Option<u32> {
        self.handler.touched.take()
    }

    fn take_changed(&mut self) -> bool {
        self.handler.changed.replace(false)
    }
}

impl Drop for Vst3Main {
    fn drop(&mut self) {
        self.detach_editor();
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
    }
}

// ---------------------------------------------------------------------------
// The audio-thread half

/// The audio-thread half of a VST3 plugin: its processor and everything a
/// processing call needs, reserved up front.
pub struct Vst3Audio {
    processor: ComPtr<IAudioProcessor>,
    processing: bool,
    /// One buffer per channel, buses in order.
    input_buffers: Vec<Vec<f32>>,
    output_buffers: Vec<Vec<f32>>,
    input_pointers: Vec<*mut f32>,
    output_pointers: Vec<*mut f32>,
    input_buses: Vec<AudioBusBuffers>,
    output_buses: Vec<AudioBusBuffers>,
    events: ComWrapper<EventList>,
    changes: ComWrapper<ParameterChanges>,
    /// Where the plugin reports its own parameter changes and the events it
    /// sends: hosts always offer them.
    changes_out: ComWrapper<ParameterChanges>,
    events_out: ComWrapper<EventList>,
    edits: rtrb::Consumer<(u32, f64)>,
    song_values: rtrb::Producer<(u32, f64)>,
    context: ProcessContext,
}

// SAFETY: the processor and the objects above are created on the main
// thread and then used only by the audio thread (VST3 allows processing on a
// thread of the host's choice); the main-thread half never touches them.
unsafe impl Send for Vst3Audio {}

impl Vst3Audio {
    fn new(
        processor: ComPtr<IAudioProcessor>,
        inputs: &[usize],
        outputs: &[usize],
        max_block: usize,
        config: AudioConfig,
        (edits, song_values): (ParamQueueOut, ParamQueueIn),
    ) -> Self {
        let channels = |buses: &[usize]| buses.iter().sum::<usize>();
        let mut input_buffers = vec![vec![0.0_f32; max_block]; channels(inputs)];
        let mut output_buffers = vec![vec![0.0_f32; max_block]; channels(outputs)];
        let mut input_pointers: Vec<*mut f32> =
            input_buffers.iter_mut().map(Vec::as_mut_ptr).collect();
        let mut output_pointers: Vec<*mut f32> =
            output_buffers.iter_mut().map(Vec::as_mut_ptr).collect();
        let buses = |counts: &[usize], pointers: &mut Vec<*mut f32>| -> Vec<AudioBusBuffers> {
            let mut offset = 0;
            counts
                .iter()
                .map(|&n| {
                    let bus = AudioBusBuffers {
                        numChannels: n as i32,
                        silenceFlags: 0,
                        __field0: AudioBusBuffers__type0 {
                            channelBuffers32: if n == 0 {
                                null_mut()
                            } else {
                                // SAFETY: `offset` stays within `pointers`.
                                unsafe { pointers.as_mut_ptr().add(offset) }
                            },
                        },
                    };
                    offset += n;
                    bus
                })
                .collect()
        };
        let input_buses = buses(inputs, &mut input_pointers);
        let output_buses = buses(outputs, &mut output_pointers);
        // SAFETY: a plain C struct, all zeros is a valid value.
        let mut context: ProcessContext = unsafe { std::mem::zeroed() };
        context.sampleRate = f64::from(config.sample_rate);
        Self {
            processor,
            processing: false,
            input_buffers,
            output_buffers,
            input_pointers,
            output_pointers,
            input_buses,
            output_buses,
            events: ComWrapper::new(EventList {
                events: RefCell::new(Vec::with_capacity(MAX_EVENTS)),
            }),
            changes: ComWrapper::new(ParameterChanges::new()),
            changes_out: ComWrapper::new(ParameterChanges::new()),
            events_out: ComWrapper::new(EventList {
                events: RefCell::new(Vec::with_capacity(MAX_EVENTS)),
            }),
            edits,
            song_values,
            context,
        }
    }
}

impl AudioSide for Vst3Audio {
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
        use vst3::Steinberg::Vst::ProcessContext_::StatesAndFlags_;
        let max = self.output_buffers.first().map_or(0, Vec::len);
        let len = left.len().min(right.len()).min(max);
        if len == 0 {
            return;
        }
        if !self.processing {
            daw_rt::permit_alloc(|| unsafe {
                self.processor.setProcessing(1);
            });
            self.processing = true;
        }
        // Notes.
        {
            let mut list = self.events.events.borrow_mut();
            list.clear();
            for event in events {
                if list.len() == list.capacity() {
                    break;
                }
                // SAFETY: a plain C struct, all zeros is a valid value.
                let mut e: VstEvent = unsafe { std::mem::zeroed() };
                match *event {
                    Event::NoteOn {
                        frame,
                        key,
                        velocity,
                    } => {
                        e.sampleOffset = frame.min(len as u32 - 1) as i32;
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
                    Event::NoteOff { frame, key } => {
                        e.sampleOffset = frame.min(len as u32 - 1) as i32;
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
                    Event::Param { .. } => continue,
                }
                list.push(e);
            }
        }
        // Parameters: edits from the editor, then the song's changes.
        self.changes.clear();
        while let Ok((id, value)) = self.edits.pop() {
            self.changes.add(id, 0, value);
        }
        for event in events {
            if let Event::Param { frame, id, value } = *event {
                let value = value.clamp(0.0, 1.0);
                self.changes
                    .add(id, frame.min(len as u32 - 1) as i32, value);
                let _ = self.song_values.push((id, value));
            }
        }
        // Input on the first bus.
        for (index, buffer) in self.input_buffers.iter_mut().enumerate() {
            match index {
                0 => buffer[..len].copy_from_slice(&left[..len]),
                1 => buffer[..len].copy_from_slice(&right[..len]),
                _ => buffer[..len].fill(0.0),
            }
        }
        // Where the song is.
        let mut state = StatesAndFlags_::kTempoValid
            | StatesAndFlags_::kTimeSigValid
            | StatesAndFlags_::kProjectTimeMusicValid
            | StatesAndFlags_::kBarPositionValid;
        if transport.playing {
            state |= StatesAndFlags_::kPlaying;
        }
        self.context.state = state as u32;
        self.context.tempo = transport.tempo;
        self.context.timeSigNumerator = i32::from(transport.numerator);
        self.context.timeSigDenominator = i32::from(transport.denominator);
        self.context.projectTimeMusic = transport.beats;
        self.context.barPositionMusic = transport.bar_start;
        self.context.projectTimeSamples = transport.frames;
        // The plugin marks the outputs it leaves silent: the marks are
        // cleared for each call, as hosts do.
        for bus in &mut self.output_buses {
            bus.silenceFlags = 0;
        }
        self.changes_out.clear();
        self.events_out.events.borrow_mut().clear();
        let mut data = ProcessData {
            processMode: ProcessModes_::kRealtime as i32,
            symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
            numSamples: len as i32,
            numInputs: self.input_buses.len() as i32,
            numOutputs: self.output_buses.len() as i32,
            inputs: if self.input_buses.is_empty() {
                null_mut()
            } else {
                self.input_buses.as_mut_ptr()
            },
            outputs: self.output_buses.as_mut_ptr(),
            inputParameterChanges: self
                .changes
                .as_com_ref::<IParameterChanges>()
                .map_or(null_mut(), |r| r.as_ptr()),
            outputParameterChanges: self
                .changes_out
                .as_com_ref::<IParameterChanges>()
                .map_or(null_mut(), |r| r.as_ptr()),
            inputEvents: self
                .events
                .as_com_ref::<IEventList>()
                .map_or(null_mut(), |r| r.as_ptr()),
            outputEvents: self
                .events_out
                .as_com_ref::<IEventList>()
                .map_or(null_mut(), |r| r.as_ptr()),
            processContext: &raw mut self.context,
        };
        let result = daw_rt::permit_alloc(|| unsafe { self.processor.process(&raw mut data) });
        self.context.continousTimeSamples += len as i64;
        let channels = self.output_buses.first().map_or(0, |b| b.numChannels) as usize;
        if result == kResultOk && channels > 0 {
            left[..len].copy_from_slice(&self.output_buffers[0][..len]);
            let second = usize::from(channels > 1);
            right[..len].copy_from_slice(&self.output_buffers[second][..len]);
        } else {
            left[..len].fill(0.0);
            right[..len].fill(0.0);
        }
        // Keep the pointer arrays alive alongside the buses that use them.
        let _ = (&self.input_pointers, &self.output_pointers);
    }

    fn stop(&mut self) {
        if self.processing {
            daw_rt::permit_alloc(|| unsafe {
                self.processor.setProcessing(0);
            });
            self.processing = false;
        }
    }
}
