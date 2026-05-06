use crate::drawing::DrawingApi;
use crate::error::OverlayError;
use crate::window::OverlayWindow;

pub struct OverlayRenderer {
    pub window: OverlayWindow,
    in_frame: bool,
}

impl OverlayRenderer {
    pub fn new(window: OverlayWindow) -> Self {
        Self {
            window,
            in_frame: false,
        }
    }

    pub fn begin_frame(&mut self) -> Result<(), OverlayError> {
        if self.in_frame {
            return Err(OverlayError::AlreadyInFrame);
        }
        self.in_frame = true;
        Ok(())
    }

    pub fn end_frame(&mut self) -> Result<(), OverlayError> {
        if !self.in_frame {
            return Err(OverlayError::NotInFrame);
        }
        self.in_frame = false;
        Ok(())
    }

    pub fn draw(&mut self) -> DrawingApi<'_> {
        let width = self.window.width();
        let height = self.window.height();
        DrawingApi::new(self.window.buffer_mut(), width, height)
    }
}
