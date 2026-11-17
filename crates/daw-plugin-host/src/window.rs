// SPDX-License-Identifier: GPL-3.0-or-later

//! The window a plugin's editor lives in: a plain top-level native window
//! the plugin attaches its own view to. On Linux an X11 window through
//! `x11rb` (which also works under XWayland on Wayland desktops); on
//! Windows a Win32 window. Its close button asks for the editor to close;
//! its size follows the editor's.

/// What a plugin needs to attach its editor to the window.
#[derive(Clone, Copy, Debug)]
pub enum ParentHandle {
    /// An X11 window id.
    #[cfg_attr(windows, allow(dead_code, reason = "made outside Windows only"))]
    X11(u32),
    /// A Win32 window handle, as an address.
    #[cfg_attr(not(windows), allow(dead_code, reason = "made on Windows only"))]
    Win32(usize),
}

#[cfg(target_os = "linux")]
pub use linux::{EditorWindow, prepare_display};
#[cfg(windows)]
pub use windows::EditorWindow;

/// Nothing to prepare outside X11.
#[cfg(not(target_os = "linux"))]
pub fn prepare_display() {}

#[cfg(not(any(target_os = "linux", windows)))]
pub use other::EditorWindow;

#[cfg(target_os = "linux")]
mod linux {
    use std::cell::Cell;

    use x11rb::connection::Connection;
    use x11rb::protocol::Event;
    use x11rb::protocol::xproto::{
        Atom, AtomEnum, ConfigureWindowAux, ConnectionExt, CreateWindowAux, EventMask, PropMode,
        StackMode, WindowClass,
    };
    use x11rb::rust_connection::RustConnection;
    use x11rb::wrapper::ConnectionExt as _;

    use super::ParentHandle;

    /// Atoms a window manager normally creates (EWMH and ICCCM).
    const STANDARD_ATOMS: &[&str] = &[
        "WM_PROTOCOLS",
        "WM_TAKE_FOCUS",
        "WM_DELETE_WINDOW",
        "WM_CHANGE_STATE",
        "WM_STATE",
        "_NET_WM_PING",
        "_NET_WM_PID",
        "_NET_WM_NAME",
        "_NET_WM_ICON",
        "_NET_WM_USER_TIME",
        "_NET_ACTIVE_WINDOW",
        "_NET_SUPPORTED",
        "_NET_FRAME_EXTENTS",
        "_NET_WM_WINDOW_TYPE",
        "_NET_WM_WINDOW_TYPE_NORMAL",
        "_NET_WM_WINDOW_TYPE_DIALOG",
        "_NET_WM_WINDOW_TYPE_UTILITY",
        "_NET_WM_WINDOW_TYPE_MENU",
        "_NET_WM_WINDOW_TYPE_POPUP_MENU",
        "_NET_WM_WINDOW_TYPE_TOOLTIP",
        "_NET_WM_WINDOW_TYPE_COMBO",
        "_NET_WM_WINDOW_TYPE_DND",
        "_NET_WM_STATE",
        "_NET_WM_STATE_HIDDEN",
        "_NET_WM_STATE_ABOVE",
        "_NET_WM_STATE_SKIP_TASKBAR",
        "_NET_WM_STATE_FULLSCREEN",
        "_NET_WM_STATE_MAXIMIZED_VERT",
        "_NET_WM_STATE_MAXIMIZED_HORZ",
        "_NET_WM_STATE_FOCUSED",
        "_MOTIF_WM_HINTS",
        "_XEMBED",
        "_XEMBED_INFO",
        "UTF8_STRING",
        "CLIPBOARD",
    ];

    /// Plugin toolkits (JUCE in particular) assume the standard desktop
    /// atoms exist, because a window manager normally creates them, and some
    /// look them up as soon as the plugin loads; without them they crash.
    /// This creates them before any plugin loads. On a normal desktop it
    /// changes nothing. The connection stays open for the life of the
    /// program: an X server with no clients left forgets every atom.
    ///
    /// It also gives the process an X11 error handler that ignores errors,
    /// for the toolkits inside plugins: Xlib's default handler ends the
    /// whole program on any error, and plugin toolkits trip harmless ones
    /// (asking about a window that has just closed, for instance). Call it
    /// before the program's own window opens, whose toolkit then installs
    /// its own handler.
    pub fn prepare_display() {
        let Ok((connection, _)) = x11rb::connect(None) else {
            return;
        };
        for name in STANDARD_ATOMS {
            if let Ok(cookie) = connection.intern_atom(false, name.as_bytes()) {
                let _ = cookie.reply();
            }
        }
        std::mem::forget(connection);
        forgive_x_errors();
    }

    type ErrorHandler = Option<
        unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> std::ffi::c_int,
    >;

    #[allow(unsafe_code, reason = "a C callback that Xlib calls")]
    unsafe extern "C" fn ignore_x_error(
        _display: *mut std::ffi::c_void,
        _event: *mut std::ffi::c_void,
    ) -> std::ffi::c_int {
        0
    }

    #[allow(unsafe_code, reason = "setting Xlib's process-wide error handler")]
    fn forgive_x_errors() {
        // SAFETY: libX11's XSetErrorHandler takes a plain C function; the
        // library stays loaded for the life of the program, as the handler
        // must.
        unsafe {
            let Ok(library) = libloading::Library::new("libX11.so.6") else {
                return;
            };
            if let Ok(set) = library
                .get::<unsafe extern "C" fn(ErrorHandler) -> ErrorHandler>(b"XSetErrorHandler\0")
            {
                set(Some(ignore_x_error));
            }
            std::mem::forget(library);
        }
    }

    fn atom(connection: &RustConnection, name: &str) -> Result<Atom, String> {
        connection
            .intern_atom(false, name.as_bytes())
            .map_err(|e| e.to_string())?
            .reply()
            .map(|reply| reply.atom)
            .map_err(|e| e.to_string())
    }

    /// An editor's window.
    pub struct EditorWindow {
        connection: RustConnection,
        window: u32,
        delete_window: Atom,
        size: Cell<(u32, u32)>,
    }

    impl EditorWindow {
        /// Creates the window (not shown yet).
        pub fn new(title: &str, width: u32, height: u32) -> Result<Self, String> {
            let (connection, screen_number) = x11rb::connect(None).map_err(|e| {
                format!("plugin editors need an X11 display (or XWayland), and none was found: {e}")
            })?;
            let screen = &connection.setup().roots[screen_number];
            let root = screen.root;
            let visual = screen.root_visual;
            let black = screen.black_pixel;
            let window = connection.generate_id().map_err(|e| e.to_string())?;
            let values = CreateWindowAux::new()
                .background_pixel(black)
                .event_mask(EventMask::STRUCTURE_NOTIFY);
            connection
                .create_window(
                    x11rb::COPY_DEPTH_FROM_PARENT,
                    window,
                    root,
                    80,
                    80,
                    clamp16(width),
                    clamp16(height),
                    0,
                    WindowClass::INPUT_OUTPUT,
                    visual,
                    &values,
                )
                .map_err(|e| e.to_string())?;
            let protocols = atom(&connection, "WM_PROTOCOLS")?;
            let delete_window = atom(&connection, "WM_DELETE_WINDOW")?;
            let net_wm_name = atom(&connection, "_NET_WM_NAME")?;
            let utf8 = atom(&connection, "UTF8_STRING")?;
            connection
                .change_property32(
                    PropMode::REPLACE,
                    window,
                    protocols,
                    AtomEnum::ATOM,
                    &[delete_window],
                )
                .map_err(|e| e.to_string())?;
            connection
                .change_property8(
                    PropMode::REPLACE,
                    window,
                    AtomEnum::WM_NAME,
                    AtomEnum::STRING,
                    title.as_bytes(),
                )
                .map_err(|e| e.to_string())?;
            connection
                .change_property8(
                    PropMode::REPLACE,
                    window,
                    net_wm_name,
                    utf8,
                    title.as_bytes(),
                )
                .map_err(|e| e.to_string())?;
            let made = Self {
                connection,
                window,
                delete_window,
                size: Cell::new((width, height)),
            };
            made.fix_size(width, height);
            made.connection.flush().map_err(|e| e.to_string())?;
            Ok(made)
        }

        /// What the plugin attaches its editor to.
        pub fn handle(&self) -> ParentHandle {
            ParentHandle::X11(self.window)
        }

        /// Makes the window `width` by `height`, and keeps it that size.
        pub fn resize(&self, width: u32, height: u32) {
            let (width, height) = (width.max(1), height.max(1));
            if self.size.get() == (width, height) {
                return;
            }
            self.fix_size(width, height);
            let values = ConfigureWindowAux::new().width(width).height(height);
            let _ = self.connection.configure_window(self.window, &values);
            let _ = self.connection.flush();
            self.size.set((width, height));
        }

        /// Tells the window manager the window has exactly this size
        /// (`WM_NORMAL_HINTS` with equal minimum and maximum).
        fn fix_size(&self, width: u32, height: u32) {
            const MIN_SIZE: u32 = 16;
            const MAX_SIZE: u32 = 32;
            let mut hints = [0_u32; 18];
            hints[0] = MIN_SIZE | MAX_SIZE;
            hints[5] = width;
            hints[6] = height;
            hints[7] = width;
            hints[8] = height;
            let _ = self.connection.change_property32(
                PropMode::REPLACE,
                self.window,
                AtomEnum::WM_NORMAL_HINTS,
                AtomEnum::WM_SIZE_HINTS,
                &hints,
            );
        }

        /// Puts the window on screen.
        pub fn show(&self) {
            let _ = self.connection.map_window(self.window);
            let _ = self.connection.flush();
        }

        /// Brings the window to the front.
        pub fn raise(&self) {
            let values = ConfigureWindowAux::new().stack_mode(StackMode::ABOVE);
            let _ = self.connection.configure_window(self.window, &values);
            let _ = self.connection.map_window(self.window);
            let _ = self.connection.flush();
        }

        /// Whether the person clicked the window's close button (reads the
        /// window's pending events).
        pub fn close_requested(&self) -> bool {
            let mut close = false;
            while let Ok(Some(event)) = self.connection.poll_for_event() {
                if let Event::ClientMessage(message) = event
                    && message.data.as_data32()[0] == self.delete_window
                {
                    close = true;
                }
            }
            close
        }
    }

    impl Drop for EditorWindow {
        fn drop(&mut self) {
            let _ = self.connection.destroy_window(self.window);
            let _ = self.connection.flush();
        }
    }

    fn clamp16(value: u32) -> u16 {
        u16::try_from(value.clamp(1, 16_000)).unwrap_or(16_000)
    }
}

#[cfg(windows)]
#[allow(unsafe_code, reason = "Win32 window calls")]
mod windows {
    use std::cell::Cell;
    use std::ptr::null_mut;
    use std::sync::Once;
    use std::sync::atomic::{AtomicBool, Ordering};

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AdjustWindowRect, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
        GWLP_USERDATA, GetWindowLongPtrW, MSG, PM_REMOVE, PeekMessageW, RegisterClassW, SW_SHOW,
        SWP_NOMOVE, SWP_NOZORDER, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow,
        TranslateMessage, WM_CLOSE, WNDCLASSW, WS_CAPTION, WS_MINIMIZEBOX, WS_OVERLAPPED,
        WS_SYSMENU,
    };

    use super::ParentHandle;

    /// A window with a title, a close button and a fixed size.
    const STYLE: u32 = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn class_name() -> Vec<u16> {
        wide("PluginEditorWindow")
    }

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_CLOSE {
            // SAFETY: the user data is the window's close flag, set at
            // creation and valid until the window is destroyed.
            let flag = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const AtomicBool;
            if !flag.is_null() {
                // SAFETY: as above.
                unsafe { &*flag }.store(true, Ordering::Release);
            }
            return 0; // The editor closes it.
        }
        // SAFETY: forwarding the message unchanged to the default handler.
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    /// An editor's window.
    pub struct EditorWindow {
        hwnd: HWND,
        close_flag: Box<AtomicBool>,
        size: Cell<(u32, u32)>,
    }

    impl EditorWindow {
        /// Creates the window (not shown yet).
        pub fn new(title: &str, width: u32, height: u32) -> Result<Self, String> {
            static REGISTER: Once = Once::new();
            let class = class_name();
            let title = wide(title);
            // SAFETY: plain Win32 calls with valid, nul-terminated strings
            // that outlive the calls.
            unsafe {
                let instance = GetModuleHandleW(null_mut());
                REGISTER.call_once(|| {
                    let class = WNDCLASSW {
                        lpfnWndProc: Some(window_proc),
                        hInstance: instance,
                        lpszClassName: class.as_ptr(),
                        ..std::mem::zeroed()
                    };
                    RegisterClassW(&raw const class);
                });
                let (outer_width, outer_height) = outer_size(width, height);
                let hwnd = CreateWindowExW(
                    0,
                    class.as_ptr(),
                    title.as_ptr(),
                    STYLE,
                    80,
                    80,
                    outer_width,
                    outer_height,
                    null_mut(),
                    null_mut(),
                    instance,
                    null_mut(),
                );
                if hwnd.is_null() {
                    return Err("the editor's window could not be created".to_owned());
                }
                let close_flag = Box::new(AtomicBool::new(false));
                SetWindowLongPtrW(
                    hwnd,
                    GWLP_USERDATA,
                    std::ptr::from_ref::<AtomicBool>(&close_flag) as isize,
                );
                Ok(Self {
                    hwnd,
                    close_flag,
                    size: Cell::new((width, height)),
                })
            }
        }

        /// What the plugin attaches its editor to.
        pub fn handle(&self) -> ParentHandle {
            ParentHandle::Win32(self.hwnd as usize)
        }

        /// Makes the window's inside `width` by `height`.
        pub fn resize(&self, width: u32, height: u32) {
            if self.size.get() == (width, height) {
                return;
            }
            let (outer_width, outer_height) = outer_size(width, height);
            // SAFETY: a valid window handle.
            unsafe {
                SetWindowPos(
                    self.hwnd,
                    null_mut(),
                    0,
                    0,
                    outer_width,
                    outer_height,
                    SWP_NOMOVE | SWP_NOZORDER,
                );
            }
            self.size.set((width, height));
        }

        /// Puts the window on screen.
        pub fn show(&self) {
            // SAFETY: a valid window handle.
            unsafe {
                ShowWindow(self.hwnd, SW_SHOW);
            }
        }

        /// Brings the window to the front.
        pub fn raise(&self) {
            // SAFETY: a valid window handle.
            unsafe {
                ShowWindow(self.hwnd, SW_SHOW);
                SetForegroundWindow(self.hwnd);
            }
        }

        /// Whether the person clicked the window's close button. Handles the
        /// window's own pending messages (the program's event loop handles
        /// the rest).
        pub fn close_requested(&self) -> bool {
            // SAFETY: a message loop restricted to this window's messages.
            unsafe {
                let mut message: MSG = std::mem::zeroed();
                while PeekMessageW(&raw mut message, self.hwnd, 0, 0, PM_REMOVE) != 0 {
                    TranslateMessage(&raw const message);
                    DispatchMessageW(&raw const message);
                }
            }
            self.close_flag.swap(false, Ordering::AcqRel)
        }
    }

    impl Drop for EditorWindow {
        fn drop(&mut self) {
            // SAFETY: the handle is ours and still valid; the flag it points
            // to outlives it (dropped after this).
            unsafe {
                SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
                DestroyWindow(self.hwnd);
            }
        }
    }

    /// The window's outer size for an inside of `width` by `height`.
    fn outer_size(width: u32, height: u32) -> (i32, i32) {
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: i32::try_from(width).unwrap_or(800),
            bottom: i32::try_from(height).unwrap_or(600),
        };
        // SAFETY: a rect we own.
        unsafe {
            AdjustWindowRect(&raw mut rect, STYLE, 0);
        }
        (rect.right - rect.left, rect.bottom - rect.top)
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod other {
    use super::ParentHandle;

    /// Editors are not supported on this system yet.
    pub struct EditorWindow;

    impl EditorWindow {
        pub fn new(_title: &str, _width: u32, _height: u32) -> Result<Self, String> {
            Err("plugin editors are not supported on this system yet".to_owned())
        }
        pub fn handle(&self) -> ParentHandle {
            ParentHandle::X11(0)
        }
        pub fn resize(&self, _width: u32, _height: u32) {}
        pub fn show(&self) {}
        pub fn raise(&self) {}
        pub fn close_requested(&self) -> bool {
            false
        }
    }
}
