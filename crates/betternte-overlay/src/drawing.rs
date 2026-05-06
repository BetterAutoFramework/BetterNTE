use crate::drawable::MatchResultDrawable;
use crate::error::OverlayError;
use betternte_core::Color;

pub struct DrawingApi<'a> {
    buffer: &'a mut [u8],
    width: u32,
    height: u32,
    stride: usize,
}

impl<'a> DrawingApi<'a> {
    pub fn new(buffer: &'a mut [u8], width: u32, height: u32) -> Self {
        let stride = width as usize * 4;
        Self {
            buffer,
            width,
            height,
            stride,
        }
    }

    pub fn buffer(&self) -> &[u8] {
        &self.buffer
    }

    pub fn buffer_mut(&mut self) -> &mut [u8] {
        &mut self.buffer
    }

    fn set_pixel(&mut self, x: i32, y: i32, color: Color) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let offset = (y as usize * self.stride) + (x as usize * 4);
        if offset + 3 >= self.buffer.len() {
            return;
        }

        let alpha = color.a as f32 / 255.0;
        let inv_alpha = 1.0 - alpha;

        self.buffer[offset] =
            (color.b as f32 * alpha + self.buffer[offset] as f32 * inv_alpha) as u8;
        self.buffer[offset + 1] =
            (color.g as f32 * alpha + self.buffer[offset + 1] as f32 * inv_alpha) as u8;
        self.buffer[offset + 2] =
            (color.r as f32 * alpha + self.buffer[offset + 2] as f32 * inv_alpha) as u8;
        self.buffer[offset + 3] = 255;
    }

    pub fn draw_text(
        &mut self,
        x: i32,
        y: i32,
        text: &str,
        color: Color,
        font_size: u32,
    ) -> Result<u32, OverlayError> {
        let char_width = font_size / 2;
        let mut current_x = x;

        for ch in text.chars() {
            if ch.is_ascii() && ch as u32 >= 32 {
                self.draw_ascii_char(current_x, y, ch, color, font_size);
                current_x += char_width as i32;
            }
        }

        Ok(text.len() as u32 * char_width)
    }

    fn draw_ascii_char(&mut self, x: i32, y: i32, ch: char, color: Color, font_size: u32) {
        let char_width = font_size / 2;
        let char_height = font_size;

        let glyph = get_ascii_glyph(ch);

        for row in 0..8 {
            for col in 0..8 {
                if glyph[row] & (1 << (7 - col)) != 0 {
                    let scale_x = char_width as f32 / 8.0;
                    let scale_y = char_height as f32 / 8.0;

                    let px = x + (col as f32 * scale_x) as i32;
                    let py = y + (row as f32 * scale_y) as i32;

                    for dy in 0..scale_y as i32 {
                        for dx in 0..scale_x as i32 {
                            self.set_pixel(px + dx, py + dy, color);
                        }
                    }
                }
            }
        }
    }

    pub fn draw_rect(
        &mut self,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        color: Color,
        thickness: u32,
    ) -> Result<(), OverlayError> {
        for t in 0..thickness {
            let t = t as i32;
            for i in 0..width as i32 {
                self.set_pixel(x + i, y + t, color);
                self.set_pixel(x + i, y + height as i32 - 1 - t, color);
            }
            for i in 0..height as i32 {
                self.set_pixel(x + t, y + i, color);
                self.set_pixel(x + width as i32 - 1 - t, y + i, color);
            }
        }
        Ok(())
    }

    pub fn fill_rect(
        &mut self,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        color: Color,
    ) -> Result<(), OverlayError> {
        for dy in 0..height as i32 {
            for dx in 0..width as i32 {
                self.set_pixel(x + dx, y + dy, color);
            }
        }
        Ok(())
    }

    pub fn draw_circle(
        &mut self,
        cx: i32,
        cy: i32,
        radius: u32,
        color: Color,
        thickness: u32,
    ) -> Result<(), OverlayError> {
        let r = radius as i32;
        for t in 0..thickness as i32 {
            let current_r = r - t;
            if current_r <= 0 {
                continue;
            }

            let mut x = 0;
            let mut y = current_r;
            let mut d = 3 - 2 * current_r;

            while x <= y {
                self.draw_circle_points(cx, cy, x, y, color);

                if d < 0 {
                    d += 4 * x + 6;
                } else {
                    d += 4 * (x - y) + 10;
                    y -= 1;
                }
                x += 1;
            }
        }
        Ok(())
    }

    fn draw_circle_points(&mut self, cx: i32, cy: i32, x: i32, y: i32, color: Color) {
        self.set_pixel(cx + x, cy + y, color);
        self.set_pixel(cx - x, cy + y, color);
        self.set_pixel(cx + x, cy - y, color);
        self.set_pixel(cx - x, cy - y, color);
        self.set_pixel(cx + y, cy + x, color);
        self.set_pixel(cx - y, cy + x, color);
        self.set_pixel(cx + y, cy - x, color);
        self.set_pixel(cx - y, cy - x, color);
    }

    pub fn fill_circle(
        &mut self,
        cx: i32,
        cy: i32,
        radius: u32,
        color: Color,
    ) -> Result<(), OverlayError> {
        let r = radius as i32;
        for y in -r..=r {
            for x in -r..=r {
                if x * x + y * y <= r * r {
                    self.set_pixel(cx + x, cy + y, color);
                }
            }
        }
        Ok(())
    }

    pub fn draw_crosshair(
        &mut self,
        x: i32,
        y: i32,
        size: u32,
        color: Color,
    ) -> Result<(), OverlayError> {
        let size = size as i32;
        for i in -size..=size {
            self.set_pixel(x + i, y, color);
            self.set_pixel(x, y + i, color);
        }
        Ok(())
    }

    pub fn draw_line(
        &mut self,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        color: Color,
        thickness: u32,
    ) -> Result<(), OverlayError> {
        let dx = (x2 - x1).abs();
        let dy = (y2 - y1).abs();
        let sx = if x1 < x2 { 1 } else { -1 };
        let sy = if y1 < y2 { 1 } else { -1 };
        let mut err = dx - dy;

        let mut x = x1;
        let mut y = y1;

        loop {
            for t in 0..thickness as i32 {
                self.set_pixel(x + t, y, color);
                self.set_pixel(x, y + t, color);
            }

            if x == x2 && y == y2 {
                break;
            }

            let e2 = 2 * err;
            if e2 > -dy {
                err -= dy;
                x += sx;
            }
            if e2 < dx {
                err += dx;
                y += sy;
            }
        }
        Ok(())
    }

    pub fn draw_match_result(&mut self, result: &MatchResultDrawable) -> Result<(), OverlayError> {
        let border_color = Color {
            r: 0,
            g: 255,
            b: 0,
            a: 255,
        };
        let text_color = Color {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        };

        self.draw_rect(
            result.x,
            result.y,
            result.width,
            result.height,
            border_color,
            2,
        )?;

        let confidence_text = format!("{}%", (result.confidence * 100.0) as u32);
        self.draw_text(result.x, result.y - 16, &confidence_text, text_color, 14)?;

        Ok(())
    }

    pub fn draw_progress_bar(
        &mut self,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        progress: f32,
        fg: Color,
        bg: Color,
    ) -> Result<(), OverlayError> {
        let progress = progress.clamp(0.0, 1.0);
        let filled_width = (width as f32 * progress) as u32;

        self.fill_rect(x, y, width, height, bg)?;
        self.fill_rect(x, y, filled_width, height, fg)?;

        Ok(())
    }

    pub fn draw_point(
        &mut self,
        x: i32,
        y: i32,
        size: u32,
        color: Color,
    ) -> Result<(), OverlayError> {
        let size = size as i32;
        for dy in -size..=size {
            for dx in -size..=size {
                if dx * dx + dy * dy <= size * size {
                    self.set_pixel(x + dx, y + dy, color);
                }
            }
        }
        Ok(())
    }

    pub fn clear(&mut self) {
        self.buffer.fill(0);
    }
}

fn get_ascii_glyph(ch: char) -> [u8; 8] {
    match ch {
        ' ' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        ':' => [0x00, 0x18, 0x18, 0x00, 0x00, 0x18, 0x18, 0x00],
        '0' => [0x00, 0x3C, 0x66, 0x6E, 0x76, 0x66, 0x3C, 0x00],
        '1' => [0x00, 0x18, 0x38, 0x18, 0x18, 0x18, 0x3C, 0x00],
        '2' => [0x00, 0x3C, 0x66, 0x06, 0x1C, 0x30, 0x7E, 0x00],
        '3' => [0x00, 0x3C, 0x66, 0x0C, 0x06, 0x66, 0x3C, 0x00],
        '4' => [0x00, 0x0C, 0x1C, 0x3C, 0x6C, 0x7E, 0x0C, 0x00],
        '5' => [0x00, 0x7E, 0x60, 0x7C, 0x06, 0x66, 0x3C, 0x00],
        '6' => [0x00, 0x3C, 0x60, 0x7C, 0x66, 0x66, 0x3C, 0x00],
        '7' => [0x00, 0x7E, 0x66, 0x0C, 0x18, 0x18, 0x18, 0x00],
        '8' => [0x00, 0x3C, 0x66, 0x3C, 0x66, 0x66, 0x3C, 0x00],
        '9' => [0x00, 0x3C, 0x66, 0x66, 0x3E, 0x06, 0x3C, 0x00],
        'F' => [0x00, 0x7E, 0x60, 0x7C, 0x60, 0x60, 0x60, 0x00],
        'P' => [0x00, 0x7C, 0x66, 0x66, 0x7C, 0x60, 0x60, 0x00],
        'S' => [0x00, 0x3E, 0x60, 0x3C, 0x06, 0x06, 0x7C, 0x00],
        'H' => [0x00, 0x42, 0x42, 0x7E, 0x42, 0x42, 0x00, 0x00],
        'e' => [0x00, 0x00, 0x3C, 0x42, 0x7E, 0x40, 0x3C, 0x00],
        'l' => [0x00, 0x20, 0x20, 0x20, 0x20, 0x20, 0x3C, 0x00],
        'o' => [0x00, 0x00, 0x3C, 0x42, 0x42, 0x42, 0x3C, 0x00],
        _ => [0x00, 0x00, 0x3C, 0x42, 0x42, 0x42, 0x3C, 0x00],
    }
}
