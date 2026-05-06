use crate::config::OverlayConfig;
use crate::error::OverlayError;

#[cfg(windows)]
unsafe extern "system" fn overlay_wndproc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, HTTRANSPARENT, MA_NOACTIVATE, WM_MOUSEACTIVATE, WM_NCHITTEST,
    };
    match msg {
        // Ensure the overlay never consumes mouse hit tests.
        WM_NCHITTEST => windows::Win32::Foundation::LRESULT(HTTRANSPARENT as isize),
        // Never activate when receiving mouse events.
        WM_MOUSEACTIVATE => windows::Win32::Foundation::LRESULT(MA_NOACTIVATE as isize),
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

pub struct OverlayWindow {
    buffer: Vec<u8>,
    width: u32,
    height: u32,
    visible: bool,
    destroyed: bool,

    #[cfg(windows)]
    hwnd: Option<isize>,

    /// Stored class name for proper cleanup on drop
    #[cfg(windows)]
    _class_name: Option<Vec<u16>>,
}

impl OverlayWindow {
    /// Create a new standalone overlay window with a real Win32 layered window
    pub fn new(config: &OverlayConfig) -> Result<Self, OverlayError> {
        #[cfg(not(windows))]
        {
            return Err(OverlayError::PlatformNotSupported);
        }

        #[cfg(windows)]
        {
            use windows::Win32::Foundation::HWND;
            use windows::Win32::Graphics::Gdi::HBRUSH;
            use windows::Win32::UI::WindowsAndMessaging::{
                CreateWindowExW, GetClassInfoExW, GetForegroundWindow, GetWindowRect,
                RegisterClassExW, SetLayeredWindowAttributes, LWA_ALPHA, WS_EX_LAYERED,
                WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
                WS_VISIBLE,
            };

            let buffer = vec![0u8; (config.width * config.height * 4) as usize];

            // Register a window class for the overlay
            // Store class_name in struct to keep it alive for the window lifetime
            let class_name: Vec<u16> = "BetterNTEOverlay\0".encode_utf16().collect();
            unsafe {
                let hmodule = windows::Win32::System::LibraryLoader::GetModuleHandleW(
                    windows::core::PCWSTR::null(),
                )
                .map(|h| h)
                .unwrap_or_default();
                let hinstance = windows::Win32::Foundation::HINSTANCE(hmodule.0);

                let wc = windows::Win32::UI::WindowsAndMessaging::WNDCLASSEXW {
                    cbSize: std::mem::size_of::<windows::Win32::UI::WindowsAndMessaging::WNDCLASSEXW>(
                    ) as u32,
                    style: windows::Win32::UI::WindowsAndMessaging::CS_HREDRAW
                        | windows::Win32::UI::WindowsAndMessaging::CS_VREDRAW,
                    lpfnWndProc: Some(overlay_wndproc),
                    cbClsExtra: 0,
                    cbWndExtra: 0,
                    hInstance: hinstance,
                    hIcon: windows::Win32::UI::WindowsAndMessaging::HICON::default(),
                    hCursor: windows::Win32::UI::WindowsAndMessaging::HCURSOR::default(),
                    hbrBackground: HBRUSH::default(),
                    lpszMenuName: windows::core::PCWSTR::null(),
                    lpszClassName: windows::core::PCWSTR::from_raw(class_name.as_ptr()),
                    hIconSm: windows::Win32::UI::WindowsAndMessaging::HICON::default(),
                };

                let atom = RegisterClassExW(&wc);
                if atom == 0 {
                    // Check if class already exists
                    let mut info: windows::Win32::UI::WindowsAndMessaging::WNDCLASSEXW =
                        std::mem::zeroed();
                    let existing = GetClassInfoExW(
                        Some(hinstance),
                        windows::core::PCWSTR::from_raw(class_name.as_ptr()),
                        &mut info,
                    );
                    if existing.is_err() {
                        return Err(OverlayError::CreateWindowFailed);
                    }
                    // Class already exists, that's fine - continue
                }
            }

            // Get the foreground window (game window) to position our overlay
            let game_hwnd = unsafe { GetForegroundWindow() };
            let (window_x, window_y, window_width, window_height) = if game_hwnd.is_invalid() {
                (0i32, 0i32, config.width as i32, config.height as i32)
            } else {
                let mut rect = windows::Win32::Foundation::RECT::default();
                unsafe {
                    let _ = GetWindowRect(game_hwnd, &mut rect);
                }
                let w = if rect.right > rect.left {
                    rect.right - rect.left
                } else {
                    config.width as i32
                };
                let h = if rect.bottom > rect.top {
                    rect.bottom - rect.top
                } else {
                    config.height as i32
                };
                (rect.left, rect.top, w, h)
            };

            // Create a layered, transparent, topmost overlay window
            let ex_style = WS_EX_LAYERED
                | WS_EX_TRANSPARENT
                | WS_EX_TOPMOST
                | WS_EX_TOOLWINDOW
                | WS_EX_NOACTIVATE;
            let style = WS_POPUP | WS_VISIBLE;

            let overlay_hwnd = unsafe {
                CreateWindowExW(
                    ex_style,
                    windows::core::PCWSTR::from_raw(class_name.as_ptr()),
                    windows::core::PCWSTR::null(),
                    style,
                    window_x,
                    window_y,
                    window_width,
                    window_height,
                    None,
                    None,
                    None,
                    None,
                )
            };

            if overlay_hwnd.is_err() {
                return Err(OverlayError::CreateWindowFailed);
            }

            let overlay_hwnd = overlay_hwnd.unwrap();
            let hwnd_value = overlay_hwnd.0 as isize;

            // Apply opacity via SetLayeredWindowAttributes
            if config.opacity < 1.0 {
                unsafe {
                    let alpha = (config.opacity * 255.0) as u8;
                    let _ = SetLayeredWindowAttributes(
                        HWND(hwnd_value as *mut _),
                        windows::Win32::Foundation::COLORREF(0),
                        alpha,
                        LWA_ALPHA,
                    );
                }
            }

            Ok(Self {
                buffer,
                width: config.width,
                height: config.height,
                visible: true,
                destroyed: false,
                hwnd: Some(hwnd_value),
                _class_name: Some(class_name),
            })
        }
    }

    /// Bind overlay to a game window and create a layered window
    #[cfg(windows)]
    pub fn bind_to_game(hwnd: isize, width: u32, height: u32) -> Result<Self, OverlayError> {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::Graphics::Gdi::HBRUSH;
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, GetClassInfoExW, GetWindowRect, RegisterClassExW, WS_CHILD,
            WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
            WS_VISIBLE,
        };

        // Convert isize to HWND properly
        let game_hwnd = HWND(hwnd as *mut std::ffi::c_void);

        // Get the window rect to determine position
        let mut rect = windows::Win32::Foundation::RECT::default();
        unsafe {
            let result = GetWindowRect(game_hwnd, &mut rect);
            if result.is_err() {
                return Err(OverlayError::CreateWindowFailed);
            }
        }

        let window_width = if rect.right > rect.left {
            rect.right - rect.left
        } else {
            width as i32
        };
        let window_height = if rect.bottom > rect.top {
            rect.bottom - rect.top
        } else {
            height as i32
        };

        // Register window class (store in struct to keep it alive)
        let class_name: Vec<u16> = "BetterNTEOverlay\0".encode_utf16().collect();
        unsafe {
            let hmodule = windows::Win32::System::LibraryLoader::GetModuleHandleW(
                windows::core::PCWSTR::null(),
            )
            .map(|h| h)
            .unwrap_or_default();
            let hinstance = windows::Win32::Foundation::HINSTANCE(hmodule.0);

            let wc = windows::Win32::UI::WindowsAndMessaging::WNDCLASSEXW {
                cbSize: std::mem::size_of::<windows::Win32::UI::WindowsAndMessaging::WNDCLASSEXW>()
                    as u32,
                style: windows::Win32::UI::WindowsAndMessaging::CS_HREDRAW
                    | windows::Win32::UI::WindowsAndMessaging::CS_VREDRAW,
                lpfnWndProc: Some(overlay_wndproc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinstance,
                hIcon: windows::Win32::UI::WindowsAndMessaging::HICON::default(),
                hCursor: windows::Win32::UI::WindowsAndMessaging::HCURSOR::default(),
                hbrBackground: HBRUSH::default(),
                lpszMenuName: windows::core::PCWSTR::null(),
                lpszClassName: windows::core::PCWSTR::from_raw(class_name.as_ptr()),
                hIconSm: windows::Win32::UI::WindowsAndMessaging::HICON::default(),
            };

            let atom = RegisterClassExW(&wc);
            if atom == 0 {
                // Check if class already exists
                let mut info: windows::Win32::UI::WindowsAndMessaging::WNDCLASSEXW =
                    std::mem::zeroed();
                let existing = GetClassInfoExW(
                    Some(hinstance),
                    windows::core::PCWSTR::from_raw(class_name.as_ptr()),
                    &mut info,
                );
                if existing.is_err() {
                    return Err(OverlayError::CreateWindowFailed);
                }
                // Class already exists, that's fine - continue
            }
        }

        // Create layered window as child of the game window
        let ex_style =
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
        let style = WS_CHILD | WS_VISIBLE;

        let overlay_hwnd = unsafe {
            CreateWindowExW(
                ex_style,
                windows::core::PCWSTR::from_raw(class_name.as_ptr()),
                windows::core::PCWSTR::null(),
                style,
                rect.left,
                rect.top,
                window_width,
                window_height,
                Some(game_hwnd),
                None,
                None,
                None,
            )
        };

        if overlay_hwnd.is_err() {
            return Err(OverlayError::CreateWindowFailed);
        }

        let overlay_hwnd = overlay_hwnd.unwrap();
        let hwnd_value = overlay_hwnd.0 as isize;

        let buffer = vec![0u8; (width * height * 4) as usize];

        Ok(Self {
            buffer,
            width,
            height,
            visible: true,
            destroyed: false,
            hwnd: Some(hwnd_value),
            _class_name: Some(class_name),
        })
    }

    #[cfg(not(windows))]
    pub fn bind_to_game(_hwnd: isize, _width: u32, _height: u32) -> Result<Self, OverlayError> {
        Err(OverlayError::PlatformNotSupported)
    }

    pub fn from_buffer(buffer: Vec<u8>, width: u32, height: u32) -> Self {
        Self {
            buffer,
            width,
            height,
            visible: false,
            destroyed: false,
            #[cfg(windows)]
            hwnd: None,
            #[cfg(windows)]
            _class_name: None,
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn show(&mut self) -> Result<(), OverlayError> {
        if self.destroyed {
            return Err(OverlayError::WindowDestroyed);
        }
        self.visible = true;
        #[cfg(windows)]
        {
            if let Some(hwnd) = self.hwnd {
                unsafe {
                    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOW};
                    let _ = ShowWindow(windows::Win32::Foundation::HWND(hwnd as *mut _), SW_SHOW);
                }
            }
        }
        Ok(())
    }

    pub fn hide(&mut self) -> Result<(), OverlayError> {
        if self.destroyed {
            return Err(OverlayError::WindowDestroyed);
        }
        self.visible = false;
        #[cfg(windows)]
        {
            if let Some(hwnd) = self.hwnd {
                unsafe {
                    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
                    let _ = ShowWindow(windows::Win32::Foundation::HWND(hwnd as *mut _), SW_HIDE);
                }
            }
        }
        Ok(())
    }

    pub fn set_opacity(&mut self, opacity: f32) -> Result<(), OverlayError> {
        if self.destroyed {
            return Err(OverlayError::WindowDestroyed);
        }
        #[cfg(windows)]
        {
            if let Some(hwnd) = self.hwnd {
                unsafe {
                    use windows::Win32::Foundation::HWND;
                    use windows::Win32::UI::WindowsAndMessaging::{
                        SetLayeredWindowAttributes, LWA_ALPHA,
                    };
                    let alpha = (opacity.clamp(0.0, 1.0) * 255.0) as u8;
                    let _ = SetLayeredWindowAttributes(
                        HWND(hwnd as *mut _),
                        windows::Win32::Foundation::COLORREF(0),
                        alpha,
                        LWA_ALPHA,
                    );
                }
            }
        }
        Ok(())
    }

    pub fn set_position(
        &mut self,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> Result<(), OverlayError> {
        if self.destroyed {
            return Err(OverlayError::WindowDestroyed);
        }
        #[cfg(windows)]
        {
            if let Some(hwnd) = self.hwnd {
                unsafe {
                    use windows::Win32::UI::WindowsAndMessaging::{
                        SetWindowPos, HWND_TOP, SWP_NOACTIVATE,
                    };

                    let hwnd_win = windows::Win32::Foundation::HWND(hwnd as *mut _);
                    let _ = SetWindowPos(
                        hwnd_win,
                        Some(HWND_TOP),
                        x,
                        y,
                        width as i32,
                        height as i32,
                        SWP_NOACTIVATE,
                    );
                }
            }
        }
        Ok(())
    }

    pub fn clear(&mut self) {
        self.buffer.fill(0);
    }

    pub fn buffer_mut(&mut self) -> &mut [u8] {
        &mut self.buffer
    }

    pub fn commit(&self) -> Result<(), OverlayError> {
        if self.destroyed {
            return Err(OverlayError::WindowDestroyed);
        }
        #[cfg(windows)]
        {
            if let Some(hwnd) = self.hwnd {
                unsafe {
                    use windows::Win32::Foundation::{HWND, POINT, SIZE};
                    use windows::Win32::Graphics::Gdi::BITMAPINFO;
                    use windows::Win32::Graphics::Gdi::BITMAPINFOHEADER;
                    use windows::Win32::Graphics::Gdi::{
                        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC,
                        ReleaseDC, SelectObject, DIB_RGB_COLORS,
                    };
                    use windows::Win32::UI::WindowsAndMessaging::{
                        GetWindowRect, UpdateLayeredWindow, ULW_ALPHA,
                    };

                    let hwnd_win = HWND(hwnd as *mut _);

                    // Get screen DC
                    let screen_hdc = GetDC(Some(hwnd_win));
                    if screen_hdc.is_invalid() {
                        return Err(OverlayError::CommitFailed);
                    }

                    // Create memory DC
                    let mem_dc = CreateCompatibleDC(Some(screen_hdc));
                    if mem_dc.is_invalid() {
                        ReleaseDC(Some(hwnd_win), screen_hdc);
                        return Err(OverlayError::CommitFailed);
                    }

                    // Prepare bitmap info header
                    let mut bmi = BITMAPINFO::default();
                    bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
                    bmi.bmiHeader.biWidth = self.width as i32;
                    bmi.bmiHeader.biHeight = -(self.height as i32); // Negative for top-down DIB
                    bmi.bmiHeader.biPlanes = 1;
                    bmi.bmiHeader.biBitCount = 32;
                    bmi.bmiHeader.biCompression = 0; // BI_RGB

                    // Get destination position from window rect
                    let mut dst_rect = windows::Win32::Foundation::RECT::default();
                    let _ = GetWindowRect(hwnd_win, &mut dst_rect);

                    let dst_pos = POINT {
                        x: dst_rect.left,
                        y: dst_rect.top,
                    };
                    let dst_size = SIZE {
                        cx: self.width as i32,
                        cy: self.height as i32,
                    };

                    // Create a DIBSection from our buffer
                    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
                    let bitmap = match CreateDIBSection(
                        Some(mem_dc),
                        &bmi,
                        DIB_RGB_COLORS,
                        &mut bits,
                        None,
                        0,
                    ) {
                        Ok(b) => b,
                        Err(_) => {
                            let _ = DeleteDC(mem_dc);
                            let _ = ReleaseDC(Some(hwnd_win), screen_hdc);
                            return Err(OverlayError::CommitFailed);
                        }
                    };

                    if bits.is_null() {
                        let _ = DeleteDC(mem_dc);
                        let _ = ReleaseDC(Some(hwnd_win), screen_hdc);
                        return Err(OverlayError::CommitFailed);
                    }

                    // Copy our buffer to the DIBSection
                    std::ptr::copy_nonoverlapping(
                        self.buffer.as_ptr(),
                        bits as *mut u8,
                        self.buffer.len(),
                    );

                    // Select the bitmap into the memory DC
                    let old_bitmap = SelectObject(mem_dc, bitmap.into());

                    // Update the layered window with our buffer
                    let result = UpdateLayeredWindow(
                        hwnd_win,
                        Some(screen_hdc),
                        Some(&dst_pos as *const POINT),
                        Some(&dst_size as *const SIZE),
                        Some(mem_dc),
                        Some(&POINT { x: 0, y: 0 }),
                        windows::Win32::Foundation::COLORREF(0),
                        None,
                        ULW_ALPHA,
                    );

                    // Clean up
                    let _ = SelectObject(mem_dc, old_bitmap);
                    let _ = DeleteObject(bitmap.into());
                    let _ = DeleteDC(mem_dc);
                    let _ = ReleaseDC(Some(hwnd_win), screen_hdc);

                    if result.is_err() {
                        return Err(OverlayError::CommitFailed);
                    }
                }
            }
        }
        Ok(())
    }

    pub fn destroy(&mut self) -> Result<(), OverlayError> {
        #[cfg(windows)]
        {
            if let Some(hwnd) = self.hwnd {
                unsafe {
                    use windows::Win32::UI::WindowsAndMessaging::DestroyWindow;
                    let _ = DestroyWindow(windows::Win32::Foundation::HWND(hwnd as *mut _));
                }
                self.hwnd = None;
            }
        }
        self.destroyed = true;
        self.visible = false;
        Ok(())
    }
}

impl Drop for OverlayWindow {
    fn drop(&mut self) {
        let _ = self.destroy();
    }
}
