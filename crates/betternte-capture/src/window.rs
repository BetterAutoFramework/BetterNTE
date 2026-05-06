//! Window enumeration and information types.

use anyhow::Result as AnyhowResult;
use betternte_core::window::{GameWindow, Rect};
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::sync::mpsc;
use std::time::Duration;
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindow,
    IsWindowVisible,
};

use crate::error::{CaptureError, Result};
use crate::WindowFinder;

/// Window information type alias (now backed by GameWindow from core).
pub type WindowInfo = GameWindow;

/// WindowFinder implementation using Win32 APIs.
pub struct WindowFinderImpl {
    _priv: (),
}

struct EnumHwndData {
    hwnds: Vec<u64>,
}

fn process_name_matches(actual: &str, configured: &str) -> bool {
    fn normalize(s: &str) -> String {
        let t = s.trim();
        let lower = t.to_ascii_lowercase();
        lower
            .strip_suffix(".exe")
            .unwrap_or(lower.as_str())
            .to_string()
    }
    let c = configured.trim();
    if c.is_empty() {
        return true;
    }
    normalize(actual) == normalize(c)
}

impl WindowFinderImpl {
    /// Create a new WindowFinderImpl
    pub fn new() -> Self {
        Self { _priv: () }
    }

    fn get_window_info_impl(hwnd: HWND) -> Result<GameWindow> {
        unsafe {
            // Get window title
            let mut title_buf = [0u16; 512];
            let title_len = GetWindowTextW(hwnd, &mut title_buf);
            let title = if title_len > 0 {
                OsString::from_wide(&title_buf[..title_len as usize])
                    .to_string_lossy()
                    .into_owned()
            } else {
                String::new()
            };

            // Get class name
            let mut class_buf = [0u16; 512];
            let class_len = GetClassNameW(hwnd, &mut class_buf);
            let class_name = if class_len > 0 {
                OsString::from_wide(&class_buf[..class_len as usize])
                    .to_string_lossy()
                    .into_owned()
            } else {
                String::new()
            };

            // Get window rect
            let mut rect = RECT::default();
            let _ = windows::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut rect);
            let window_rect = Rect::new(rect.left, rect.top, rect.right, rect.bottom);

            // Get client rect
            let mut client_rect = RECT::default();
            let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut client_rect);
            let client = Rect::new(
                client_rect.left,
                client_rect.top,
                client_rect.right,
                client_rect.bottom,
            );

            // Get process ID
            let pid = GetWindowThreadProcessId(hwnd, None);

            // Get process name
            let process_name = get_process_name(pid);

            // Check if minimized
            let is_minimized = IsIconic(hwnd).as_bool();
            let dpi = GetDpiForWindow(hwnd);
            let dpi_scale = if dpi == 0 { 1.0 } else { dpi as f64 / 96.0 };

            Ok(GameWindow {
                hwnd: hwnd.0 as u64,
                title,
                class_name,
                pid,
                process_name,
                rect: window_rect,
                client_rect: client,
                is_minimized,
                dpi_scale,
            })
        }
    }
}

fn get_process_name(pid: u32) -> String {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid);
        if handle.is_err() {
            return String::new();
        }
        let handle = handle.unwrap();
        let mut name_buf = [0u16; 512];
        let mut size = 512u32;
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_FORMAT(0),
            windows::core::PWSTR(name_buf.as_mut_ptr()),
            &mut size,
        );
        if ok.is_ok() {
            let path = OsString::from_wide(&name_buf[..size as usize])
                .to_string_lossy()
                .into_owned();
            let name = path.split('\\').last().unwrap_or(&path).to_string();
            name
        } else {
            String::new()
        }
    }
}

unsafe extern "system" fn enum_hwnd_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let data = &mut *(lparam.0 as *mut EnumHwndData);
    if !IsWindowVisible(hwnd).as_bool() {
        return BOOL(1);
    }
    data.hwnds.push(hwnd.0 as u64);
    BOOL(1)
}

fn enum_visible_hwnds() -> Result<Vec<u64>> {
    let mut data = EnumHwndData { hwnds: Vec::new() };
    unsafe {
        let ok = EnumWindows(
            Some(enum_hwnd_callback),
            LPARAM(&mut data as *mut _ as isize),
        );
        if ok.is_err() {
            return Err(CaptureError::Internal("EnumWindows failed".into()));
        }
    }
    Ok(data.hwnds)
}

fn get_window_info_with_timeout(hwnd_u64: u64, timeout: Duration) -> Option<GameWindow> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let hwnd = HWND(hwnd_u64 as isize as *mut _);
        let result = WindowFinderImpl::get_window_info_impl(hwnd);
        let _ = tx.send(result);
    });
    match rx.recv_timeout(timeout) {
        Ok(Ok(info)) => Some(info),
        _ => None,
    }
}

impl Default for WindowFinderImpl {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowFinder for WindowFinderImpl {
    fn find_by_title(&self, title: &str) -> AnyhowResult<Vec<GameWindow>> {
        let hwnds = enum_visible_hwnds()?;
        let title_trimmed = title.trim();
        let mut results = Vec::new();
        for hwnd in hwnds {
            if let Some(info) = get_window_info_with_timeout(hwnd, Duration::from_millis(120)) {
                if info.title.trim() == title_trimmed {
                    results.push(info);
                }
            }
        }
        Ok(results)
    }

    fn find_by_class(&self, class_name: &str) -> AnyhowResult<Vec<GameWindow>> {
        let hwnds = enum_visible_hwnds()?;
        let mut results = Vec::new();
        for hwnd in hwnds {
            if let Some(info) = get_window_info_with_timeout(hwnd, Duration::from_millis(120)) {
                if info.class_name == class_name {
                    results.push(info);
                }
            }
        }
        Ok(results)
    }

    fn find_by_process(&self, process_name: &str) -> AnyhowResult<Vec<GameWindow>> {
        let hwnds = enum_visible_hwnds()?;
        let mut results = Vec::new();
        for hwnd in hwnds {
            if let Some(info) = get_window_info_with_timeout(hwnd, Duration::from_millis(120)) {
                if info.process_name == process_name {
                    results.push(info);
                }
            }
        }
        Ok(results)
    }

    fn find_by_keyword(&self, keyword: &str) -> AnyhowResult<Vec<GameWindow>> {
        self.find_by_keyword_and_process(keyword, None)
    }

    fn find_by_keyword_and_process(
        &self,
        keyword: &str,
        process_name: Option<&str>,
    ) -> AnyhowResult<Vec<GameWindow>> {
        let proc_filter = process_name.map(str::trim).filter(|s| !s.is_empty());
        let hwnds = enum_visible_hwnds()?;
        let mut results = Vec::new();
        for hwnd in hwnds {
            let Some(info) = get_window_info_with_timeout(hwnd, Duration::from_millis(120)) else {
                continue;
            };
            let kw_ok = keyword.is_empty()
                || info.title.contains(keyword)
                || info.class_name.contains(keyword);
            let proc_ok = match proc_filter {
                None => true,
                Some(p) => process_name_matches(&info.process_name, p),
            };
            if kw_ok && proc_ok {
                results.push(info);
            }
        }
        Ok(results)
    }

    fn get_window_info(&self, hwnd: u64) -> AnyhowResult<GameWindow> {
        let hwnd = HWND(hwnd as *mut _);
        unsafe {
            if !IsWindow(Some(hwnd)).as_bool() {
                return Err(CaptureError::WindowNotFound("Invalid HWND".into()).into());
            }
        }
        Self::get_window_info_impl(hwnd).map_err(Into::into)
    }

    fn get_client_rect(&self, hwnd: u64) -> AnyhowResult<Rect> {
        let hwnd = HWND(hwnd as *mut _);
        unsafe {
            if !IsWindow(Some(hwnd)).as_bool() {
                return Err(CaptureError::WindowNotFound("Invalid HWND".into()).into());
            }
        }

        unsafe {
            let mut rect = RECT::default();
            let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect);
            Ok(Rect::new(rect.left, rect.top, rect.right, rect.bottom))
        }
    }

    fn is_minimized(&self, hwnd: u64) -> bool {
        let hwnd = HWND(hwnd as *mut _);
        unsafe { IsIconic(hwnd).as_bool() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_finder_list_windows() {
        let finder = WindowFinderImpl::new();
        let windows = finder.find_by_keyword("");
        assert!(windows.is_ok());
        let wins = windows.unwrap();
        if !wins.is_empty() {
            println!("Found {} windows", wins.len());
            for w in wins.iter().take(5) {
                println!("  - {} ({}): {}", w.title, w.class_name, w.hwnd);
            }
        }
    }

    #[test]
    fn test_get_window_info_invalid() {
        let finder = WindowFinderImpl::new();
        let result = finder.get_window_info(999999999);
        assert!(result.is_err());
    }
}
