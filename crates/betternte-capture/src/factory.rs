//! Factory and chain-of-responsibility for creating capture engines.
//!
//! Auto-tiered selection order:
//! 1. WGC (GPU, persistent session, best for games)
//! 2. DXGI Desktop Duplication (GPU, desktop-level)
//! 3. PrintWindow (GDI, captures obscured windows via WM_PRINT)
//! 4. ScreenDC (GDI, captures obscured windows)
//! 5. BitBlt (GDI, most compatible fallback)

use betternte_core::config::CaptureMethod;

use crate::bitblt::BitBltCapture;
use crate::buffer::FrameRingBuffer;
use crate::dxgi_dup::DxgiDupCapture;
use crate::error::CaptureError;
use crate::print_window::PrintWindowCapture;
use crate::screen_dc::ScreenDCCapture;
use crate::wgc::WgcCapture;
use crate::{CaptureTarget, ScreenCapture};

// ============================================================================
// CaptureProvider trait — Chain of Responsibility
// ============================================================================

/// A capture backend provider. Each backend implements this trait
/// to participate in the auto-selection chain.
pub trait CaptureProvider: Send + Sync {
    /// The capture method this provider represents.
    fn method(&self) -> CaptureMethod;

    /// Human-readable name for logging.
    fn name(&self) -> &'static str;

    /// Whether this backend is available on the current platform.
    fn is_available(&self) -> bool;

    /// Create a new capture engine instance.
    fn create(&self, fps_cap: u32) -> Result<Box<dyn ScreenCapture>, CaptureError>;
}

// ============================================================================
// Concrete providers
// ============================================================================

pub struct WgcProvider;
impl CaptureProvider for WgcProvider {
    fn method(&self) -> CaptureMethod {
        CaptureMethod::WindowsGraphicsCapture
    }
    fn name(&self) -> &'static str {
        "WindowsGraphicsCapture"
    }
    fn is_available(&self) -> bool {
        WgcCapture::is_supported()
    }
    fn create(&self, fps_cap: u32) -> Result<Box<dyn ScreenCapture>, CaptureError> {
        tracing::info!("Using WGC");
        Ok(Box::new(WgcCapture::new_with_fps(fps_cap)))
    }
}

pub struct DxgiDupProvider;
impl CaptureProvider for DxgiDupProvider {
    fn method(&self) -> CaptureMethod {
        CaptureMethod::DxgiDesktopDuplication
    }
    fn name(&self) -> &'static str {
        "DxgiDesktopDuplication"
    }
    fn is_available(&self) -> bool {
        DxgiDupCapture::is_supported()
    }
    fn create(&self, _fps_cap: u32) -> Result<Box<dyn ScreenCapture>, CaptureError> {
        tracing::info!("Using DXGI Desktop Duplication");
        DxgiDupCapture::new().map(|e| Box::new(e) as Box<dyn ScreenCapture>)
    }
}

pub struct BitBltProvider;
impl CaptureProvider for BitBltProvider {
    fn method(&self) -> CaptureMethod {
        CaptureMethod::BitBlt
    }
    fn name(&self) -> &'static str {
        "BitBlt"
    }
    fn is_available(&self) -> bool {
        true
    }
    fn create(&self, _fps_cap: u32) -> Result<Box<dyn ScreenCapture>, CaptureError> {
        tracing::info!("Using BitBlt");
        Ok(Box::new(BitBltCapture::new()))
    }
}

pub struct PrintWindowProvider;
impl CaptureProvider for PrintWindowProvider {
    fn method(&self) -> CaptureMethod {
        CaptureMethod::PrintWindow
    }
    fn name(&self) -> &'static str {
        "PrintWindow"
    }
    fn is_available(&self) -> bool {
        true
    }
    fn create(&self, _fps_cap: u32) -> Result<Box<dyn ScreenCapture>, CaptureError> {
        tracing::info!("Using PrintWindow");
        Ok(Box::new(PrintWindowCapture::new()))
    }
}

pub struct ScreenDCProvider;
impl CaptureProvider for ScreenDCProvider {
    fn method(&self) -> CaptureMethod {
        // NOTE: `DwmSharedSurface` is currently backed by the ScreenDC implementation.
        // Keep this mapping for config compatibility until a dedicated DWM backend lands.
        CaptureMethod::DwmSharedSurface
    }
    fn name(&self) -> &'static str {
        "ScreenDC"
    }
    fn is_available(&self) -> bool {
        true
    }
    fn create(&self, _fps_cap: u32) -> Result<Box<dyn ScreenCapture>, CaptureError> {
        tracing::info!("Using ScreenDC (fallback)");
        Ok(Box::new(ScreenDCCapture::new()))
    }
}

// ============================================================================
// CaptureChain — priority-ordered provider chain
// ============================================================================

/// Priority-ordered chain of capture providers.
///
/// `auto_select` iterates the chain and returns the first available provider
/// whose method is in the whitelist.
pub struct CaptureChain {
    providers: Vec<Box<dyn CaptureProvider>>,
}

impl CaptureChain {
    /// Create a new chain with the default priority order.
    pub fn new() -> Self {
        Self {
            providers: vec![
                Box::new(WgcProvider),
                Box::new(DxgiDupProvider),
                Box::new(PrintWindowProvider),
                Box::new(ScreenDCProvider),
                Box::new(BitBltProvider),
            ],
        }
    }

    /// Select a capture engine for the given method, with auto-tiered fallback.
    ///
    /// For `Auto`, iterates the chain by priority and picks the first available
    /// provider whose method is in the whitelist. For specific methods, finds
    /// the matching provider directly.
    pub fn select(
        &self,
        method: &CaptureMethod,
        whitelist: &[CaptureMethod],
        fps_cap: u32,
        target: Option<&CaptureTarget>,
    ) -> Result<Box<dyn ScreenCapture>, CaptureError> {
        match method {
            CaptureMethod::Auto => self.auto_select(whitelist, fps_cap, target),
            _ => {
                // Find the specific provider
                if let Some(provider) = self.providers.iter().find(|p| p.method() == *method) {
                    provider.create(fps_cap)
                } else {
                    tracing::warn!(
                        "Capture method {:?} not implemented, using auto selection",
                        method
                    );
                    self.auto_select(whitelist, fps_cap, target)
                }
            }
        }
    }

    /// Auto-select the best available capture engine from the whitelist.
    pub fn auto_select(
        &self,
        whitelist: &[CaptureMethod],
        fps_cap: u32,
        target: Option<&CaptureTarget>,
    ) -> Result<Box<dyn ScreenCapture>, CaptureError> {
        for provider in &self.providers {
            if !whitelist.contains(&provider.method()) {
                continue;
            }
            if provider.is_available() {
                match provider.create(fps_cap) {
                    Ok(engine) => {
                        tracing::info!("Auto-selected: {}", provider.name());
                        return Ok(engine);
                    }
                    Err(e) => {
                        tracing::debug!("{} not available: {}", provider.name(), e);
                    }
                }
            }
        }

        // Ultimate fallback depends on target type.
        let fallback: Box<dyn ScreenCapture> = match target {
            Some(CaptureTarget::Display { .. }) => {
                tracing::info!("Auto-selected: BitBlt (display fallback)");
                Box::new(BitBltCapture::new())
            }
            Some(CaptureTarget::Window { .. }) | None => {
                tracing::info!("Auto-selected: ScreenDC (window fallback)");
                Box::new(ScreenDCCapture::new())
            }
            _ => {
                tracing::info!("Auto-selected: BitBlt (generic fallback)");
                Box::new(BitBltCapture::new())
            }
        };
        Ok(fallback)
    }

    /// Resolve which method auto-selection would pick, without creating it.
    pub fn resolve_auto_method(&self, whitelist: &[CaptureMethod]) -> &'static str {
        for provider in &self.providers {
            if !whitelist.contains(&provider.method()) {
                continue;
            }
            if provider.is_available() {
                return provider.name();
            }
        }
        "ScreenDC"
    }
}

impl Default for CaptureChain {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Public API — preserved for backwards compatibility
// ============================================================================

/// Info about a capture method: its enum value, display name, and platform availability.
pub struct CaptureMethodInfo {
    pub method: CaptureMethod,
    pub available: bool,
}

/// Create a capture engine based on the specified method with auto-tiered fallback.
pub fn create_capture_engine(
    method: &CaptureMethod,
    whitelist: &[CaptureMethod],
) -> Result<Box<dyn ScreenCapture>, CaptureError> {
    create_capture_engine_with_fps_for_target(method, whitelist, 0, None)
}

/// Create a capture engine with optional fps cap hints for backends.
pub fn create_capture_engine_with_fps(
    method: &CaptureMethod,
    whitelist: &[CaptureMethod],
    fps_cap: u32,
) -> Result<Box<dyn ScreenCapture>, CaptureError> {
    create_capture_engine_with_fps_for_target(method, whitelist, fps_cap, None)
}

/// Create a capture engine with fps cap and target hints.
pub fn create_capture_engine_with_fps_for_target(
    method: &CaptureMethod,
    whitelist: &[CaptureMethod],
    fps_cap: u32,
    target: Option<&CaptureTarget>,
) -> Result<Box<dyn ScreenCapture>, CaptureError> {
    CaptureChain::new().select(method, whitelist, fps_cap, target)
}

/// Resolve which capture engine name auto-selection would pick, without creating it.
pub fn resolve_auto_capture_method(whitelist: &[CaptureMethod]) -> &'static str {
    CaptureChain::new().resolve_auto_method(whitelist)
}

/// Return all desktop capture methods with their platform availability.
pub fn available_capture_methods() -> Vec<CaptureMethodInfo> {
    let chain = CaptureChain::new();
    let all_methods = [
        CaptureMethod::Auto,
        CaptureMethod::BitBlt,
        CaptureMethod::PrintWindow,
        CaptureMethod::DwmSharedSurface,
        CaptureMethod::WindowsGraphicsCapture,
        CaptureMethod::DxgiDesktopDuplication,
        CaptureMethod::AdbScreencap,
        CaptureMethod::AdbScrcpy,
        CaptureMethod::AdbMinicap,
        CaptureMethod::MumuExtras,
        CaptureMethod::LdExtras,
    ];

    all_methods
        .iter()
        .map(|m| CaptureMethodInfo {
            method: *m,
            available: match m {
                CaptureMethod::Auto => true,
                _ => chain
                    .providers
                    .iter()
                    .find(|p| p.method() == *m)
                    .map_or(false, |p| p.is_available()),
            },
        })
        .collect()
}

/// Create a frame ring buffer with the specified capacity.
pub fn create_frame_buffer(capacity: usize) -> FrameRingBuffer {
    FrameRingBuffer::new(capacity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_bitblt_engine() {
        let whitelist = vec![CaptureMethod::BitBlt];
        let engine = create_capture_engine(&CaptureMethod::BitBlt, &whitelist).unwrap();
        assert_eq!(engine.name(), "BitBlt");
    }

    #[test]
    fn test_create_print_window_engine() {
        let whitelist = vec![CaptureMethod::PrintWindow];
        let engine = create_capture_engine(&CaptureMethod::PrintWindow, &whitelist).unwrap();
        assert_eq!(engine.name(), "PrintWindow");
    }

    #[test]
    fn test_create_wgc_engine() {
        let whitelist = vec![CaptureMethod::WindowsGraphicsCapture];
        let engine =
            create_capture_engine(&CaptureMethod::WindowsGraphicsCapture, &whitelist).unwrap();
        assert_eq!(engine.name(), "WindowsGraphicsCapture");
    }

    #[test]
    fn test_auto_select() {
        let whitelist = vec![
            CaptureMethod::WindowsGraphicsCapture,
            CaptureMethod::DxgiDesktopDuplication,
            CaptureMethod::BitBlt,
            CaptureMethod::PrintWindow,
        ];
        let engine = create_capture_engine(&CaptureMethod::Auto, &whitelist).unwrap();
        let name = engine.name();
        assert!(
            name == "WindowsGraphicsCapture"
                || name == "DxgiDesktopDuplication"
                || name == "BitBlt"
                || name == "PrintWindow"
        );
    }

    #[test]
    fn test_capture_chain_direct_select() {
        let chain = CaptureChain::new();
        let whitelist = vec![CaptureMethod::BitBlt];
        let engine = chain
            .select(&CaptureMethod::BitBlt, &whitelist, 0, None)
            .unwrap();
        assert_eq!(engine.name(), "BitBlt");
    }

    #[test]
    fn test_resolve_auto_method() {
        let whitelist = vec![CaptureMethod::BitBlt, CaptureMethod::PrintWindow];
        let name = resolve_auto_capture_method(&whitelist);
        assert!(name == "BitBlt" || name == "PrintWindow" || name == "ScreenDC");
    }
}
