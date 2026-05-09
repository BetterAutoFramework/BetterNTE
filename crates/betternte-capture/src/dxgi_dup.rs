//! DXGI Desktop Duplication capture engine.
//!
//! Uses the DXGI Desktop Duplication API to capture screenshots of windows.
//! This captures the entire desktop and crops to the window's client area.

use std::sync::OnceLock;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;
use betternte_core::CaptureFrame;
use windows::core::Interface;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
    D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::{
    Common as DxgiCommon, CreateDXGIFactory, IDXGIAdapter, IDXGIAdapter1, IDXGIFactory,
    IDXGIOutput, IDXGIOutput1, IDXGIOutputDuplication, IDXGIResource,
};
use windows::Win32::Graphics::Gdi::{ClientToScreen, MonitorFromWindow, MONITOR_DEFAULTTONEAREST};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
use windows::Win32::UI::WindowsAndMessaging::{GetClientRect, IsIconic, IsWindow};

use crate::error::CaptureError;
use crate::{CaptureTarget, ScreenCapture};
use anyhow::Result as AnyhowResult;

#[derive(Debug, Clone, Copy)]
struct SafeHwnd(u64);

#[allow(dead_code)]
impl SafeHwnd {
    fn to_hwnd(&self) -> HWND {
        HWND(self.0 as *mut _)
    }

    fn from_hwnd(hwnd: HWND) -> Self {
        Self(hwnd.0 as u64)
    }
}

struct DxgiDupState {
    frame_counter: u64,
    last_latency_ms: Option<f64>,
    last_frame: Option<CaptureFrame>,
    capturing: bool,
    start_time: Option<Instant>,
    width: u32,
    height: u32,
    options: betternte_core::CaptureRuntimeOptions,
}

#[derive(Clone, Copy)]
struct WindowRegion {
    left: i32,
    top: i32,
    width: u32,
    height: u32,
    monitor: isize,
}

/// DXGI Desktop Duplication capture engine.
pub struct DxgiDupCapture {
    hwnd: Option<SafeHwnd>,
    d3d_device: Option<ID3D11Device>,
    d3d_context: Option<ID3D11DeviceContext>,
    dxgi_factory: Option<IDXGIFactory>,
    dxgi_adapter: Arc<Mutex<Option<IDXGIAdapter1>>>,
    dxgi_output: Arc<Mutex<Option<IDXGIOutput1>>>,
    dxgi_dup: Arc<Mutex<Option<IDXGIOutputDuplication>>>,
    staging_texture: Arc<Mutex<Option<ID3D11Texture2D>>>,
    state: Arc<Mutex<DxgiDupState>>,
    window_region: Arc<Mutex<WindowRegion>>,
    /// Reusable pixel buffer to avoid allocation per frame.
    pixel_buffer: Arc<Mutex<Vec<u8>>>,
}

impl DxgiDupCapture {
    pub fn new() -> Result<Self, CaptureError> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }

        Ok(Self {
            hwnd: None,
            d3d_device: None,
            d3d_context: None,
            dxgi_factory: None,
            dxgi_adapter: Arc::new(Mutex::new(None)),
            dxgi_output: Arc::new(Mutex::new(None)),
            dxgi_dup: Arc::new(Mutex::new(None)),
            staging_texture: Arc::new(Mutex::new(None)),
            state: Arc::new(Mutex::new(DxgiDupState {
                frame_counter: 0,
                last_latency_ms: None,
                last_frame: None,
                capturing: false,
                start_time: None,
                width: 0,
                height: 0,
                options: betternte_core::CaptureRuntimeOptions::default(),
            })),
            window_region: Arc::new(Mutex::new(WindowRegion {
                left: 0,
                top: 0,
                width: 0,
                height: 0,
                monitor: 0,
            })),
            pixel_buffer: Arc::new(Mutex::new(Vec::new())),
        })
    }

    pub fn is_supported() -> bool {
        static DXGI_SUPPORTED: OnceLock<bool> = OnceLock::new();
        *DXGI_SUPPORTED.get_or_init(|| {
            let mut probe = match Self::new() {
                Ok(p) => p,
                Err(_) => return false,
            };
            if probe.init_d3d_device().is_err() || probe.init_dxgi_factory().is_err() {
                return false;
            }

            let factory = match probe.dxgi_factory.as_ref() {
                Some(f) => f,
                None => return false,
            };
            let device = match probe.d3d_device.as_ref() {
                Some(d) => d,
                None => return false,
            };

            unsafe {
                let adapter: IDXGIAdapter = match factory.EnumAdapters(0) {
                    Ok(a) => a,
                    Err(_) => return false,
                };
                let adapter1: IDXGIAdapter1 = match adapter.cast() {
                    Ok(a) => a,
                    Err(_) => return false,
                };
                let output: IDXGIOutput = match adapter1.EnumOutputs(0) {
                    Ok(o) => o,
                    Err(_) => return false,
                };
                let output1: IDXGIOutput1 = match output.cast() {
                    Ok(o) => o,
                    Err(_) => return false,
                };
                output1.DuplicateOutput(device).is_ok()
            }
        })
    }

    fn init_d3d_device(&mut self) -> Result<(), CaptureError> {
        unsafe {
            let mut dev: Option<ID3D11Device> = None;
            let mut ctx: Option<ID3D11DeviceContext> = None;

            let result = D3D11CreateDevice(
                None,
                windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE,
                windows::Win32::Foundation::HMODULE::default(),
                Default::default(),
                None,
                windows::Win32::Graphics::Direct3D11::D3D11_SDK_VERSION,
                Some(&mut dev),
                None,
                Some(&mut ctx),
            )
            .or_else(|_| {
                D3D11CreateDevice(
                    None,
                    windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_WARP,
                    windows::Win32::Foundation::HMODULE::default(),
                    Default::default(),
                    None,
                    windows::Win32::Graphics::Direct3D11::D3D11_SDK_VERSION,
                    Some(&mut dev),
                    None,
                    Some(&mut ctx),
                )
            });

            result.map_err(|e| {
                CaptureError::InitFailed(format!("D3D11CreateDevice failed: {:?}", e))
            })?;

            self.d3d_device = dev.clone();
            self.d3d_context = ctx.clone();

            Ok(())
        }
    }

    fn init_dxgi_factory(&mut self) -> Result<(), CaptureError> {
        unsafe {
            let factory: IDXGIFactory = CreateDXGIFactory().map_err(|e| {
                CaptureError::InitFailed(format!("CreateDXGIFactory failed: {:?}", e))
            })?;
            self.dxgi_factory = Some(factory);
            Ok(())
        }
    }

    fn find_output_for_monitor(
        &self,
        monitor: windows::Win32::Graphics::Gdi::HMONITOR,
    ) -> Result<(IDXGIAdapter1, IDXGIOutput1), CaptureError> {
        let factory = self
            .dxgi_factory
            .as_ref()
            .ok_or_else(|| CaptureError::InitFailed("DXGI factory not initialized".into()))?;

        unsafe {
            for adapter_index in 0.. {
                let adapter: IDXGIAdapter = match factory.EnumAdapters(adapter_index) {
                    Ok(a) => a,
                    Err(_) => break,
                };

                let adapter1: IDXGIAdapter1 = adapter.cast().map_err(|e| {
                    CaptureError::InitFailed(format!("Adapter cast failed: {:?}", e))
                })?;

                for output_index in 0.. {
                    let output: IDXGIOutput = match adapter1.EnumOutputs(output_index) {
                        Ok(o) => o,
                        Err(_) => break,
                    };

                    let desc = output.GetDesc().map_err(|e| {
                        CaptureError::InitFailed(format!("GetDesc failed: {:?}", e))
                    })?;

                    if desc.Monitor == monitor {
                        let output1: IDXGIOutput1 = output.cast().map_err(|e| {
                            CaptureError::InitFailed(format!("Output cast failed: {:?}", e))
                        })?;
                        return Ok((adapter1, output1));
                    }
                }
            }

            Err(CaptureError::InitFailed(
                "Could not find output for monitor".into(),
            ))
        }
    }

    fn init_output_duplication_for_output(
        &self,
        output: &IDXGIOutput1,
    ) -> Result<IDXGIOutputDuplication, CaptureError> {
        let device = self
            .d3d_device
            .as_ref()
            .ok_or_else(|| CaptureError::InitFailed("D3D device not initialized".into()))?;

        unsafe {
            output
                .DuplicateOutput(device)
                .map_err(|e| CaptureError::InitFailed(format!("DuplicateOutput failed: {:?}", e)))
        }
    }

    fn rebuild_duplication_for_monitor(
        &self,
        monitor: windows::Win32::Graphics::Gdi::HMONITOR,
    ) -> Result<(), CaptureError> {
        let (adapter1, output1) = self.find_output_for_monitor(monitor)?;
        let dup = self.init_output_duplication_for_output(&output1)?;
        *self.dxgi_adapter.lock().unwrap_or_else(|e| e.into_inner()) = Some(adapter1);
        *self.dxgi_output.lock().unwrap_or_else(|e| e.into_inner()) = Some(output1);
        *self.dxgi_dup.lock().unwrap_or_else(|e| e.into_inner()) = Some(dup);
        Ok(())
    }

    fn refresh_window_region(&self) -> Result<(), CaptureError> {
        let hwnd = self
            .hwnd
            .ok_or_else(|| CaptureError::InitFailed("No window bound".into()))?
            .to_hwnd();
        unsafe {
            if !IsWindow(Some(hwnd)).as_bool() {
                return Err(CaptureError::WindowNotFound("Invalid HWND".into()));
            }
            if IsIconic(hwnd).as_bool() {
                return Err(CaptureError::WindowMinimized);
            }
        }

        let client_rect = unsafe {
            let mut rect = RECT::default();
            if GetClientRect(hwnd, &mut rect).is_err() {
                return Err(CaptureError::CaptureFailed("GetClientRect failed".into()));
            }
            rect
        };

        let (client_left, client_top) = {
            let mut top_left = windows::Win32::Foundation::POINT {
                x: client_rect.left,
                y: client_rect.top,
            };
            unsafe {
                let _ = ClientToScreen(hwnd, &mut top_left);
            }
            (top_left.x, top_left.y)
        };

        let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
        if monitor.is_invalid() {
            return Err(CaptureError::CaptureFailed(
                "MonitorFromWindow failed".into(),
            ));
        }

        let monitor_changed;
        let size_changed;
        {
            let mut region = self.window_region.lock().unwrap_or_else(|e| e.into_inner());
            size_changed = region.width != 0
                && region.height != 0
                && (region.width != (client_rect.right - client_rect.left) as u32
                    || region.height != (client_rect.bottom - client_rect.top) as u32);
            monitor_changed = region.monitor != 0 && region.monitor != monitor.0 as isize;
            region.left = client_left;
            region.top = client_top;
            region.width = (client_rect.right - client_rect.left) as u32;
            region.height = (client_rect.bottom - client_rect.top) as u32;
            region.monitor = monitor.0 as isize;
        }
        let recover_on_resize = {
            self.state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .options
                .recover_on_resize
        };
        if size_changed && !recover_on_resize {
            return Err(CaptureError::CaptureFailed(
                "Window size changed while recover_on_resize is disabled".into(),
            ));
        }
        if size_changed && recover_on_resize {
            *self
                .staging_texture
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = None;
        }
        let recover_monitor_switch = {
            self.state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .options
                .recover_on_monitor_switch
        };
        if monitor_changed && recover_monitor_switch {
            self.rebuild_duplication_for_monitor(monitor)?;
        }

        Ok(())
    }

    fn capture_frame_impl(&self) -> Result<CaptureFrame, CaptureError> {
        let _device = self
            .d3d_device
            .as_ref()
            .ok_or_else(|| CaptureError::InitFailed("D3D device not initialized".into()))?;

        let context = self
            .d3d_context
            .as_ref()
            .ok_or_else(|| CaptureError::InitFailed("D3D context not initialized".into()))?;

        let mut recovered_access_lost = false;
        let (active_dup, resource_opt): (IDXGIOutputDuplication, Option<IDXGIResource>) = loop {
            let dup = {
                self.dxgi_dup
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .as_ref()
                    .cloned()
                    .ok_or_else(|| {
                        CaptureError::InitFailed("DXGI duplication not initialized".into())
                    })?
            };

            let mut frame_info = windows::Win32::Graphics::Dxgi::DXGI_OUTDUPL_FRAME_INFO::default();
            let mut resource_opt: Option<IDXGIResource> = None;
            let acquire_result =
                unsafe { dup.AcquireNextFrame(500, &mut frame_info, &mut resource_opt) };
            if let Err(e) = acquire_result {
                let code = e.code().0 as u32;
                if code == 0x887A0027 {
                    let state = self.state.lock().unwrap_or_else(|pe| pe.into_inner());
                    if let Some(frame) = state.last_frame.clone() {
                        return Ok(frame);
                    }
                    return Err(CaptureError::Timeout(500));
                }
                if code == 0x887A0026 && !recovered_access_lost {
                    recovered_access_lost = true;
                    let monitor_raw = self
                        .window_region
                        .lock()
                        .unwrap_or_else(|pe| pe.into_inner())
                        .monitor;
                    if monitor_raw != 0 {
                        let monitor =
                            windows::Win32::Graphics::Gdi::HMONITOR(monitor_raw as *mut _);
                        self.rebuild_duplication_for_monitor(monitor)?;
                        continue;
                    }
                }
                return Err(CaptureError::CaptureFailed(format!(
                    "AcquireNextFrame failed: {:?}",
                    e
                )));
            }
            break (dup, resource_opt);
        };

        let desktop_resource =
            resource_opt.ok_or_else(|| CaptureError::CaptureFailed("No frame acquired".into()))?;

        let _drop_frame = DropFrameOnReturn(active_dup);

        let raw_texture: ID3D11Texture2D = desktop_resource
            .cast()
            .map_err(|e| CaptureError::CaptureFailed(format!("QueryInterface failed: {:?}", e)))?;

        let mut desc_val = D3D11_TEXTURE2D_DESC::default();
        unsafe {
            raw_texture.GetDesc(&mut desc_val);
        };

        let width = desc_val.Width;
        let height = desc_val.Height;

        let staging: ID3D11Texture2D = {
            let mut staging_guard = self
                .staging_texture
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if let Some(ref staging) = *staging_guard {
                let mut curr_desc = D3D11_TEXTURE2D_DESC::default();
                unsafe {
                    staging.GetDesc(&mut curr_desc);
                };

                if curr_desc.Width != width || curr_desc.Height != height {
                    let new_staging = self.create_staging_texture(width, height)?;
                    *staging_guard = Some(new_staging.clone());
                    new_staging
                } else {
                    staging.clone()
                }
            } else {
                let new_staging = self.create_staging_texture(width, height)?;
                *staging_guard = Some(new_staging.clone());
                new_staging
            }
        };

        unsafe {
            context.CopyResource(&staging, &raw_texture);
        }

        let mapped = unsafe {
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            context
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .map_err(|e| CaptureError::CaptureFailed(format!("Map failed: {:?}", e)))?;
            mapped
        };

        let row_pitch = mapped.RowPitch as usize;
        let src_data_ptr = mapped.pData as *const u8;

        let region = *self.window_region.lock().unwrap_or_else(|e| e.into_inner());
        let (left, top, crop_width, crop_height) = {
            let options = self.state.lock().unwrap_or_else(|e| e.into_inner()).options;
            if options.crop_to_client {
                (
                    region.left.max(0) as usize,
                    region.top.max(0) as usize,
                    region.width as usize,
                    region.height as usize,
                )
            } else {
                (0usize, 0usize, width as usize, height as usize)
            }
        };

        // Acquire reusable pixel buffer from pool (avoids allocation).
        let pixel_len = crop_width * crop_height * 4;
        let mut pixels = {
            let mut guard = self.pixel_buffer.lock().unwrap_or_else(|e| e.into_inner());
            if guard.capacity() >= pixel_len {
                // Reuse: clear length but keep capacity.
                guard.clear();
                std::mem::replace(&mut *guard, Vec::new())
            } else {
                Vec::with_capacity(pixel_len)
            }
        };
        if left == 0 && crop_width * 4 == row_pitch {
            let start_ptr = unsafe { src_data_ptr.add(top * row_pitch) };
            let total = crop_height * row_pitch;
            pixels.extend_from_slice(unsafe { std::slice::from_raw_parts(start_ptr, total) });
        } else {
            for y in top..(top + crop_height) {
                let row_ptr = unsafe { src_data_ptr.add(y * row_pitch + left * 4) };
                pixels.extend_from_slice(unsafe {
                    std::slice::from_raw_parts(row_ptr, crop_width * 4)
                });
            }
        }

        unsafe {
            context.Unmap(&staging, 0);
        }

        // Create frame with recycle callback to return buffer to pool.
        let pixel_buffer_ref = self.pixel_buffer.clone();
        let frame = CaptureFrame::new_with_recycle(
            crop_width as u32,
            crop_height as u32,
            pixels,
            betternte_core::PixelFormat::Bgra,
            "DxgiDesktopDuplication".to_string(),
            move |buf: Vec<u8>| {
                if let Ok(mut guard) = pixel_buffer_ref.lock() {
                    if guard.capacity() == 0 {
                        *guard = buf;
                    }
                }
            },
        );

        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.frame_counter += 1;
            state.last_frame = Some(frame.clone());
        }

        Ok(frame)
    }

    fn create_staging_texture(
        &self,
        width: u32,
        height: u32,
    ) -> Result<ID3D11Texture2D, CaptureError> {
        let device = self
            .d3d_device
            .as_ref()
            .ok_or_else(|| CaptureError::InitFailed("D3D device not initialized".into()))?;

        let desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DxgiCommon::DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DxgiCommon::DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_STAGING,
            BindFlags: Default::default(),
            CPUAccessFlags: windows::Win32::Graphics::Direct3D11::D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: Default::default(),
        };

        unsafe {
            let mut texture_opt: Option<ID3D11Texture2D> = None;
            device
                .CreateTexture2D(&desc, None, Some(&mut texture_opt))
                .map_err(|e| {
                    CaptureError::InitFailed(format!("CreateTexture2D failed: {:?}", e))
                })?;
            texture_opt
                .ok_or_else(|| CaptureError::InitFailed("CreateTexture2D returned None".into()))
        }
    }
}

#[derive(Clone)]
struct DropFrameOnReturn(IDXGIOutputDuplication);

impl Drop for DropFrameOnReturn {
    fn drop(&mut self) {
        unsafe {
            let _ = self.0.ReleaseFrame();
        }
    }
}

impl Default for DxgiDupCapture {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| panic!("Failed to create DxgiDupCapture"))
    }
}

impl Drop for DxgiDupCapture {
    fn drop(&mut self) {
        *self
            .staging_texture
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
        *self.dxgi_dup.lock().unwrap_or_else(|e| e.into_inner()) = None;
        *self.dxgi_output.lock().unwrap_or_else(|e| e.into_inner()) = None;
        *self.dxgi_adapter.lock().unwrap_or_else(|e| e.into_inner()) = None;
        self.dxgi_factory = None;
        self.d3d_context = None;
        self.d3d_device = None;
    }
}

#[async_trait]
impl ScreenCapture for DxgiDupCapture {
    fn name(&self) -> &str {
        "DxgiDesktopDuplication"
    }

    fn is_capturing(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .capturing
    }

    async fn start(&mut self, target: &CaptureTarget) -> AnyhowResult<()> {
        let _ = self.stop().await;
        let hwnd = match target {
            CaptureTarget::Window { hwnd } => *hwnd,
            _ => {
                return Err(CaptureError::UnsupportedTarget(
                    "DXGI capture only supports window capture".into(),
                )
                .into())
            }
        };

        let hw = HWND(hwnd as *mut _);

        unsafe {
            if !IsWindow(Some(hw)).as_bool() {
                return Err(CaptureError::WindowNotFound("Invalid HWND".into()).into());
            }
            if IsIconic(hw).as_bool() {
                return Err(CaptureError::WindowMinimized.into());
            }
        }

        self.init_d3d_device()?;
        self.init_dxgi_factory()?;

        let monitor = unsafe { MonitorFromWindow(hw, MONITOR_DEFAULTTONEAREST) };

        if monitor.is_invalid() {
            return Err(CaptureError::InitFailed("MonitorFromWindow failed".into()).into());
        }

        self.rebuild_duplication_for_monitor(monitor)?;

        let client_rect = unsafe {
            let mut rect = RECT::default();
            if GetClientRect(hw, &mut rect).is_err() {
                return Err(CaptureError::InitFailed("GetClientRect failed".into()).into());
            }
            rect
        };

        let (client_left, client_top) = {
            let mut top_left = windows::Win32::Foundation::POINT {
                x: client_rect.left,
                y: client_rect.top,
            };
            unsafe {
                let _ = ClientToScreen(hw, &mut top_left);
            }
            (top_left.x, top_left.y)
        };

        {
            let mut region = self.window_region.lock().unwrap_or_else(|e| e.into_inner());
            region.left = client_left;
            region.top = client_top;
            region.width = (client_rect.right - client_rect.left) as u32;
            region.height = (client_rect.bottom - client_rect.top) as u32;
            region.monitor = monitor.0 as isize;
        }

        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let region = self.window_region.lock().unwrap_or_else(|e| e.into_inner());
            state.width = region.width;
            state.height = region.height;
            state.start_time = Some(Instant::now());
            state.capturing = true;
        }

        self.hwnd = Some(SafeHwnd(hwnd));

        Ok(())
    }

    async fn capture(&self) -> AnyhowResult<CaptureFrame> {
        if !self.is_capturing() {
            return Err(CaptureError::InitFailed("Capture not started".into()).into());
        }

        self.refresh_window_region()?;
        let start = Instant::now();
        let frame = self.capture_frame_impl()?;
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.last_latency_ms = Some(elapsed);
        Ok(frame)
    }

    async fn stop(&mut self) -> AnyhowResult<()> {
        *self
            .staging_texture
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
        *self.dxgi_dup.lock().unwrap_or_else(|e| e.into_inner()) = None;
        *self.dxgi_output.lock().unwrap_or_else(|e| e.into_inner()) = None;
        *self.dxgi_adapter.lock().unwrap_or_else(|e| e.into_inner()) = None;

        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.capturing = false;
            state.start_time = None;
            state.last_frame = None;
        }

        self.hwnd = None;

        Ok(())
    }

    fn last_latency_ms(&self) -> Option<f64> {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .last_latency_ms
    }

    fn fps(&self) -> f64 {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(start) = state.start_time {
            let elapsed = start.elapsed().as_secs_f64();
            if elapsed > 0.0 {
                return state.frame_counter as f64 / elapsed;
            }
        }
        0.0
    }

    fn configure(&self, options: betternte_core::CaptureRuntimeOptions) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.options = options;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_dxgi_is_supported() {
        let supported = super::DxgiDupCapture::is_supported();
        println!("DXGI Desktop Duplication supported: {}", supported);
    }
}
