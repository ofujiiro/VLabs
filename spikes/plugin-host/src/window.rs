// SPDX-License-Identifier: GPL-3.0-or-later

//! A plain native window to hold a plugin editor, plus a screenshot of it.
//!
//! Linux: an X11 window through `x11rb`, which also works under XWayland.
//! Windows: a Win32 window.
//!
//! The window background is magenta, a colour no plugin editor uses as its
//! background, so the screenshot can tell how much of the window the plugin
//! actually drew.

use std::path::Path;

/// The background colour, as 0xRRGGBB.
const BACKGROUND: u32 = 0x00FF_00FF;

/// What a plugin needs to attach its editor to the window.
#[derive(Clone, Copy, Debug)]
pub enum ParentHandle {
    /// An X11 window id.
    X11(u32),
    /// A Win32 HWND, as an address.
    Win32(usize),
}

#[cfg(target_os = "linux")]
pub use linux::{EditorWindow, prepare_display};
#[cfg(windows)]
pub use windows::EditorWindow;

/// Nothing to prepare outside X11.
#[cfg(not(target_os = "linux"))]
pub fn prepare_display() {}

/// Writes a 24-bit BMP, from rows of 0xRRGGBB pixels, top row first.
fn write_bmp(path: &Path, width: u32, height: u32, pixels: &[u32]) -> Result<(), String> {
    let row_bytes = (width * 3).div_ceil(4) * 4;
    let image_bytes = row_bytes * height;
    let mut bytes = Vec::with_capacity(54 + image_bytes as usize);
    bytes.extend_from_slice(b"BM");
    bytes.extend_from_slice(&(54 + image_bytes).to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&54_u32.to_le_bytes());
    bytes.extend_from_slice(&40_u32.to_le_bytes());
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&24_u16.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&image_bytes.to_le_bytes());
    bytes.extend_from_slice(&2835_u32.to_le_bytes());
    bytes.extend_from_slice(&2835_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    // BMP rows go bottom to top.
    for y in (0..height).rev() {
        let row = &pixels[(y * width) as usize..((y + 1) * width) as usize];
        for &pixel in row {
            bytes.extend_from_slice(&[
                (pixel & 0xFF) as u8,
                ((pixel >> 8) & 0xFF) as u8,
                ((pixel >> 16) & 0xFF) as u8,
            ]);
        }
        bytes.resize(bytes.len() + (row_bytes - width * 3) as usize, 0);
    }
    std::fs::write(path, bytes).map_err(|error| format!("{}: {error}", path.display()))
}

/// The fraction of pixels that are not the window background.
fn drawn_fraction(pixels: &[u32]) -> f64 {
    if pixels.is_empty() {
        return 0.0;
    }
    let drawn = pixels
        .iter()
        .filter(|&&p| (p & 0x00FF_FFFF) != BACKGROUND)
        .count();
    drawn as f64 / pixels.len() as f64
}

#[cfg(target_os = "linux")]
mod linux {
    use std::cell::Cell;
    use std::path::Path;

    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{
        ConfigureWindowAux, ConnectionExt, CreateWindowAux, EventMask, ImageFormat, PropMode,
        WindowClass,
    };
    use x11rb::rust_connection::RustConnection;
    use x11rb::wrapper::ConnectionExt as _;

    use super::{BACKGROUND, ParentHandle};

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

    /// Plugin GUI toolkits (JUCE in particular) assume the standard desktop
    /// atoms exist, because a window manager normally creates them, and some
    /// look them up as soon as the plugin is loaded. On an X server without a
    /// window manager they would crash, so create the atoms before loading
    /// any plugin. On a normal desktop this changes nothing.
    ///
    /// The connection is kept open for the life of the program: an X server
    /// with no clients left resets itself and forgets every atom.
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
    }

    pub struct EditorWindow {
        connection: RustConnection,
        window: u32,
        size: Cell<(u32, u32)>,
    }

    impl EditorWindow {
        pub fn new(title: &str, width: u32, height: u32) -> Result<Self, String> {
            let (connection, screen_number) =
                x11rb::connect(None).map_err(|e| format!("cannot reach the X server: {e}"))?;
            let screen = &connection.setup().roots[screen_number];
            let window = connection.generate_id().map_err(|e| e.to_string())?;
            let values = CreateWindowAux::new()
                .background_pixel(BACKGROUND)
                .event_mask(EventMask::EXPOSURE | EventMask::STRUCTURE_NOTIFY);
            connection
                .create_window(
                    x11rb::COPY_DEPTH_FROM_PARENT,
                    window,
                    screen.root,
                    0,
                    0,
                    width as u16,
                    height as u16,
                    0,
                    WindowClass::INPUT_OUTPUT,
                    screen.root_visual,
                    &values,
                )
                .map_err(|e| e.to_string())?;
            connection
                .change_property8(
                    PropMode::REPLACE,
                    window,
                    x11rb::protocol::xproto::AtomEnum::WM_NAME,
                    x11rb::protocol::xproto::AtomEnum::STRING,
                    title.as_bytes(),
                )
                .map_err(|e| e.to_string())?;
            connection.map_window(window).map_err(|e| e.to_string())?;
            connection.flush().map_err(|e| e.to_string())?;
            Ok(Self {
                connection,
                window,
                size: Cell::new((width, height)),
            })
        }

        pub fn handle(&self) -> ParentHandle {
            ParentHandle::X11(self.window)
        }

        pub fn resize(&self, width: u32, height: u32) {
            let values = ConfigureWindowAux::new().width(width).height(height);
            let _ = self.connection.configure_window(self.window, &values);
            let _ = self.connection.flush();
            self.size.set((width, height));
        }

        /// Handles pending events for our own window (none need action).
        pub fn pump(&self) {
            while let Ok(Some(_event)) = self.connection.poll_for_event() {}
        }

        /// Saves the window's content, including the plugin's child windows,
        /// and returns the fraction of it the plugin drew.
        pub fn screenshot(&self, path: &Path) -> Result<f64, String> {
            let (width, height) = self.size.get();
            let reply = self
                .connection
                .get_image(
                    ImageFormat::Z_PIXMAP,
                    self.window,
                    0,
                    0,
                    width as u16,
                    height as u16,
                    !0,
                )
                .map_err(|e| e.to_string())?
                .reply()
                .map_err(|e| format!("could not read the window's pixels: {e}"))?;
            let bytes_per_pixel = reply.data.len() / (width * height) as usize;
            if bytes_per_pixel != 4 {
                return Err(format!(
                    "unexpected pixel format ({bytes_per_pixel} bytes per pixel)"
                ));
            }
            let pixels: Vec<u32> = reply
                .data
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| u32::from(p[2]) << 16 | u32::from(p[1]) << 8 | u32::from(p[0]))
                .collect();
            super::write_bmp(path, width, height, &pixels)?;
            Ok(super::drawn_fraction(&pixels))
        }
    }

    impl Drop for EditorWindow {
        fn drop(&mut self) {
            let _ = self.connection.destroy_window(self.window);
            let _ = self.connection.flush();
        }
    }
}

#[cfg(windows)]
#[allow(unsafe_code, reason = "Win32 window calls")]
mod windows {
    use std::cell::Cell;
    use std::path::Path;
    use std::ptr::null_mut;

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleBitmap, CreateCompatibleDC,
        CreateSolidBrush, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC,
        SelectObject,
    };
    use windows_sys::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AdjustWindowRect, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, MSG,
        PM_REMOVE, PeekMessageW, RegisterClassW, SWP_NOMOVE, SWP_NOZORDER, SetWindowPos,
        TranslateMessage, WNDCLASSW, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
    };

    use super::{BACKGROUND, ParentHandle};

    pub struct EditorWindow {
        hwnd: HWND,
        size: Cell<(u32, u32)>,
    }

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        // SAFETY: forwarding the message unchanged to the default handler.
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    impl EditorWindow {
        pub fn new(title: &str, width: u32, height: u32) -> Result<Self, String> {
            let class_name = wide("VLabsSpikeEditorWindow");
            let title = wide(title);
            // SAFETY: plain Win32 calls with valid, nul-terminated strings
            // that outlive the calls.
            unsafe {
                let instance = GetModuleHandleW(null_mut());
                // GDI colours are 0x00BBGGRR.
                let bgr = ((BACKGROUND & 0xFF) << 16)
                    | (BACKGROUND & 0xFF00)
                    | ((BACKGROUND >> 16) & 0xFF);
                let class = WNDCLASSW {
                    lpfnWndProc: Some(window_proc),
                    hInstance: instance,
                    lpszClassName: class_name.as_ptr(),
                    hbrBackground: CreateSolidBrush(bgr),
                    ..std::mem::zeroed()
                };
                RegisterClassW(&raw const class);
                let mut rect = RECT {
                    left: 0,
                    top: 0,
                    right: width as i32,
                    bottom: height as i32,
                };
                AdjustWindowRect(&raw mut rect, WS_OVERLAPPEDWINDOW, 0);
                let hwnd = CreateWindowExW(
                    0,
                    class_name.as_ptr(),
                    title.as_ptr(),
                    WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                    50,
                    50,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    null_mut(),
                    null_mut(),
                    instance,
                    null_mut(),
                );
                if hwnd.is_null() {
                    return Err("CreateWindowExW failed".to_owned());
                }
                Ok(Self {
                    hwnd,
                    size: Cell::new((width, height)),
                })
            }
        }

        pub fn handle(&self) -> ParentHandle {
            ParentHandle::Win32(self.hwnd as usize)
        }

        pub fn resize(&self, width: u32, height: u32) {
            let mut rect = RECT {
                left: 0,
                top: 0,
                right: width as i32,
                bottom: height as i32,
            };
            // SAFETY: valid window handle and a rect we own.
            unsafe {
                AdjustWindowRect(&raw mut rect, WS_OVERLAPPEDWINDOW, 0);
                SetWindowPos(
                    self.hwnd,
                    null_mut(),
                    0,
                    0,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    SWP_NOMOVE | SWP_NOZORDER,
                );
            }
            self.size.set((width, height));
        }

        #[allow(clippy::unused_self, reason = "same signature as the X11 version")]
        pub fn pump(&self) {
            // SAFETY: a standard message loop over messages for this thread.
            unsafe {
                let mut message: MSG = std::mem::zeroed();
                while PeekMessageW(&raw mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
                    TranslateMessage(&raw const message);
                    DispatchMessageW(&raw const message);
                }
            }
        }

        pub fn screenshot(&self, path: &Path) -> Result<f64, String> {
            let (width, height) = self.size.get();
            let mut pixels = vec![0_u32; (width * height) as usize];
            // SAFETY: GDI objects are created, used and released in order;
            // the pixel buffer is exactly width*height 32-bit pixels.
            unsafe {
                let window_dc = GetDC(self.hwnd);
                let memory_dc = CreateCompatibleDC(window_dc);
                let bitmap = CreateCompatibleBitmap(window_dc, width as i32, height as i32);
                let previous = SelectObject(memory_dc, bitmap);
                // PW_CLIENTONLY | PW_RENDERFULLCONTENT
                let printed = PrintWindow(self.hwnd, memory_dc, 1 | 2 as PRINT_WINDOW_FLAGS);
                let mut info: BITMAPINFO = std::mem::zeroed();
                info.bmiHeader = BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width as i32,
                    biHeight: -(height as i32), // top-down rows
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    ..std::mem::zeroed()
                };
                let rows = GetDIBits(
                    memory_dc,
                    bitmap,
                    0,
                    height,
                    pixels.as_mut_ptr().cast(),
                    &raw mut info,
                    DIB_RGB_COLORS,
                );
                SelectObject(memory_dc, previous);
                DeleteObject(bitmap);
                DeleteDC(memory_dc);
                ReleaseDC(self.hwnd, window_dc);
                if printed == 0 || rows == 0 {
                    return Err("could not capture the window".to_owned());
                }
            }
            // GetDIBits gives 0x00RRGGBB already in a little-endian u32 (BGRX bytes).
            super::write_bmp(path, width, height, &pixels)?;
            Ok(super::drawn_fraction(&pixels))
        }
    }

    impl Drop for EditorWindow {
        fn drop(&mut self) {
            // SAFETY: the handle is ours and still valid.
            unsafe {
                DestroyWindow(self.hwnd);
            }
        }
    }
}
