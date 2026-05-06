use crate::config::OverlayConfig;
use crate::drawing::DrawingApi;
use crate::error::OverlayError;
use crate::window::OverlayWindow;
use betternte_core::Color;

pub struct OverlayManager {
    window: OverlayWindow,
    #[allow(dead_code)]
    config: OverlayConfig,
    game_hwnd: Option<usize>,
}

impl OverlayManager {
    pub fn new(config: &OverlayConfig) -> Result<Self, OverlayError> {
        let window = OverlayWindow::new(config)?;
        Ok(Self {
            window,
            config: config.clone(),
            game_hwnd: None,
        })
    }

    pub fn bind_to_game(&mut self, hwnd: usize) -> Result<(), OverlayError> {
        if hwnd == 0 {
            return Err(OverlayError::GameWindowNotFound);
        }
        self.game_hwnd = Some(hwnd);
        Ok(())
    }

    pub fn sync_position(&mut self) -> Result<(), OverlayError> {
        let Some(hwnd) = self.game_hwnd else {
            return Err(OverlayError::GameWindowNotFound);
        };

        #[cfg(windows)]
        {
            use windows::Win32::Foundation::HWND;
            use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;

            let mut rect = windows::Win32::Foundation::RECT::default();
            unsafe {
                let _ = GetWindowRect(HWND(hwnd as *mut _), &mut rect);
            }
            let width = (rect.right - rect.left).max(1) as u32;
            let height = (rect.bottom - rect.top).max(1) as u32;
            self.window
                .set_position(rect.left, rect.top, width, height)?;
        }
        Ok(())
    }

    pub fn clear(&mut self) -> Result<(), OverlayError> {
        self.window.clear();
        self.window.commit()?;
        Ok(())
    }

    pub fn render_fps_text(&mut self, fps: f64) -> Result<(), OverlayError> {
        self.window.clear();
        let width = self.window.width();
        let height = self.window.height();
        let mut drawing = DrawingApi::new(self.window.buffer_mut(), width, height);
        let bg = Color {
            r: 0,
            g: 0,
            b: 0,
            a: 120,
        };
        let fg = Color {
            r: 0,
            g: 255,
            b: 0,
            a: 255,
        };
        drawing.fill_rect(8, 8, 140, 24, bg)?;
        drawing.draw_text(12, 12, &format!("FPS: {:.1}", fps.max(0.0)), fg, 14)?;
        self.window.commit()?;
        Ok(())
    }

    pub fn show(&mut self) -> Result<(), OverlayError> {
        self.window.show()
    }

    pub fn hide(&mut self) -> Result<(), OverlayError> {
        self.window.hide()
    }

    pub fn is_visible(&self) -> bool {
        self.window.is_visible()
    }
}
