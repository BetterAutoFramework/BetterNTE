//! 图像和帧数据类型

use image::{DynamicImage, ImageBuffer, RgbImage, RgbaImage};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 像素格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PixelFormat {
    /// BGRA（Windows 默认）
    Bgra,
    /// RGBA（通用）
    Rgba,
    /// BGR（OpenCV 默认）
    Bgr,
    /// RGB
    Rgb,
    /// 灰度
    Gray,
}

impl PixelFormat {
    /// 每像素字节数
    pub fn bytes_per_pixel(&self) -> u32 {
        match self {
            PixelFormat::Bgra | PixelFormat::Rgba => 4,
            PixelFormat::Bgr | PixelFormat::Rgb => 3,
            PixelFormat::Gray => 1,
        }
    }
}

/// 截图帧数据。
///
/// 一帧完整的截图数据，包含像素数据、尺寸、格式和时间戳。
///
/// # Buffer recycling
///
/// When constructed via [`CaptureFrame::new_with_recycle`], the frame carries
/// a `recycle_fn` callback.  On drop, the callback is invoked with the pixel
/// `data` Vec, returning it to the caller's pool.  This eliminates per-frame
/// heap allocation in the hot capture loop (especially at 1080p where each
/// frame is ~8 MiB).
///
/// `Clone` intentionally does **not** copy the recycle callback — only the
/// original frame returns its buffer to the pool; clones allocate normally.
pub struct CaptureFrame {
    /// 宽度（像素）
    pub width: u32,
    /// 高度（像素）
    pub height: u32,
    /// 像素数据（格式由 `format` 字段指定）
    pub data: Vec<u8>,
    /// 像素格式
    pub format: PixelFormat,
    /// 帧捕获时间戳
    pub timestamp: DateTime<Utc>,
    /// 帧序号（自截图引擎启动起的递增序号）
    pub sequence: u64,
    /// 来源引擎名称
    pub source: String,
    /// Optional callback invoked on drop to return `data` to a buffer pool.
    recycle_fn: Option<Box<dyn FnOnce(Vec<u8>) + Send>>,
}

impl Clone for CaptureFrame {
    fn clone(&self) -> Self {
        Self {
            width: self.width,
            height: self.height,
            data: self.data.clone(),
            format: self.format,
            timestamp: self.timestamp,
            sequence: self.sequence,
            source: self.source.clone(),
            // Clones do NOT carry the recycle callback — only the original
            // returns its buffer to the pool.  The clone's buffer is freed
            // normally on drop.
            recycle_fn: None,
        }
    }
}

impl std::fmt::Debug for CaptureFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CaptureFrame")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("data_len", &self.data.len())
            .field("format", &self.format)
            .field("timestamp", &self.timestamp)
            .field("sequence", &self.sequence)
            .field("source", &self.source)
            .field("has_recycle_fn", &self.recycle_fn.is_some())
            .finish()
    }
}

impl Drop for CaptureFrame {
    fn drop(&mut self) {
        if let Some(recycle) = self.recycle_fn.take() {
            let data = std::mem::take(&mut self.data);
            recycle(data);
        }
    }
}

impl CaptureFrame {
    /// 创建新帧（无 buffer 回收）
    pub fn new(
        width: u32,
        height: u32,
        data: Vec<u8>,
        format: PixelFormat,
        source: String,
    ) -> Self {
        Self {
            width,
            height,
            data,
            format,
            timestamp: Utc::now(),
            sequence: 0,
            source,
            recycle_fn: None,
        }
    }

    /// 创建带 buffer 回收回调的帧。
    ///
    /// 当帧被 drop 时，`recycle_fn` 会被调用，将 `data` Vec 归还到调用者的
    /// buffer pool 中。这避免了热路径上每帧 ~8 MiB 的堆分配。
    ///
    /// `Clone` 不会复制回调——只有原始帧会归还 buffer，clone 的帧正常释放。
    pub fn new_with_recycle(
        width: u32,
        height: u32,
        data: Vec<u8>,
        format: PixelFormat,
        source: String,
        recycle_fn: impl FnOnce(Vec<u8>) + Send + 'static,
    ) -> Self {
        Self {
            width,
            height,
            data,
            format,
            timestamp: Utc::now(),
            sequence: 0,
            source,
            recycle_fn: Some(Box::new(recycle_fn)),
        }
    }

    /// 数据大小（字节）
    pub fn size_bytes(&self) -> usize {
        self.data.len()
    }

    /// 每行字节数
    pub fn stride(&self) -> u32 {
        self.width * self.format.bytes_per_pixel()
    }

    /// 裁剪指定区域
    pub fn crop(&self, region: &Region) -> Option<Self> {
        // 边界检查
        if region.x < 0 || region.y < 0 {
            return None;
        }
        let rx = region.x as u32;
        let ry = region.y as u32;
        if rx + region.width > self.width || ry + region.height > self.height {
            return None;
        }

        let bpp = self.format.bytes_per_pixel() as usize;
        let stride = self.stride() as usize;
        let mut cropped = Vec::with_capacity((region.width * region.height * bpp as u32) as usize);

        for y in ry..ry + region.height {
            let row_start = (y as usize * stride) + (rx as usize * bpp);
            let row_end = row_start + (region.width as usize * bpp);
            if row_end <= self.data.len() {
                cropped.extend_from_slice(&self.data[row_start..row_end]);
            } else {
                return None;
            }
        }

        Some(Self {
            width: region.width,
            height: region.height,
            data: cropped,
            format: self.format,
            timestamp: self.timestamp,
            sequence: self.sequence,
            source: self.source.clone(),
            recycle_fn: None,
        })
    }

    /// Convert to image::DynamicImage
    pub fn to_dynamic_image(&self) -> Result<DynamicImage, String> {
        match self.format {
            PixelFormat::Bgra => {
                let img: RgbaImage =
                    ImageBuffer::from_raw(self.width, self.height, self.bgra_to_rgba())
                        .ok_or_else(|| {
                            format!(
                                "Buffer size mismatch for BGRa image ({}x{})",
                                self.width, self.height
                            )
                        })?;
                Ok(DynamicImage::ImageRgba8(img))
            }
            PixelFormat::Rgba => {
                let img: RgbaImage =
                    ImageBuffer::from_raw(self.width, self.height, self.data.clone()).ok_or_else(
                        || {
                            format!(
                                "Buffer size mismatch for RGBa image ({}x{})",
                                self.width, self.height
                            )
                        },
                    )?;
                Ok(DynamicImage::ImageRgba8(img))
            }
            PixelFormat::Bgr => {
                let img: RgbImage =
                    ImageBuffer::from_raw(self.width, self.height, self.bgr_to_rgb()).ok_or_else(
                        || {
                            format!(
                                "Buffer size mismatch for BGR image ({}x{})",
                                self.width, self.height
                            )
                        },
                    )?;
                Ok(DynamicImage::ImageRgb8(img))
            }
            PixelFormat::Rgb => {
                let img: RgbImage =
                    ImageBuffer::from_raw(self.width, self.height, self.data.clone()).ok_or_else(
                        || {
                            format!(
                                "Buffer size mismatch for RGB image ({}x{})",
                                self.width, self.height
                            )
                        },
                    )?;
                Ok(DynamicImage::ImageRgb8(img))
            }
            PixelFormat::Gray => {
                let img = ImageBuffer::from_raw(self.width, self.height, self.data.clone())
                    .ok_or_else(|| {
                        format!(
                            "Buffer size mismatch for gray image ({}x{})",
                            self.width, self.height
                        )
                    })?;
                Ok(DynamicImage::ImageLuma8(img))
            }
        }
    }

    /// Resize to specified dimensions
    pub fn resize(&self, width: u32, height: u32) -> Result<Self, String> {
        let img = self.to_dynamic_image()?;
        let resized = img.resize_exact(width, height, image::imageops::FilterType::Nearest);
        let data = resized.to_rgba8().into_raw();

        Ok(Self {
            width,
            height,
            data,
            format: PixelFormat::Rgba,
            timestamp: self.timestamp,
            sequence: self.sequence,
            source: self.source.clone(),
            recycle_fn: None,
        })
    }

    /// Export to byte array in specified format
    pub fn to_bytes(&self, format: &str) -> std::result::Result<Vec<u8>, String> {
        let img = self.to_dynamic_image()?;
        let mut buf = std::io::Cursor::new(Vec::new());
        let img_format = match format.to_lowercase().as_str() {
            "png" => image::ImageFormat::Png,
            "jpeg" | "jpg" => image::ImageFormat::Jpeg,
            "bmp" => image::ImageFormat::Bmp,
            _ => return Err(format!("Unsupported format: {}", format)),
        };
        img.write_to(&mut buf, img_format)
            .map_err(|e| format!("Failed to encode image: {}", e))?;
        Ok(buf.into_inner())
    }

    /// BGRA -> RGBA 转换
    fn bgra_to_rgba(&self) -> Vec<u8> {
        let mut data = self.data.clone();
        for chunk in data.chunks_exact_mut(4) {
            chunk.swap(0, 2); // B <-> R
        }
        data
    }

    /// BGR -> RGB 转换
    fn bgr_to_rgb(&self) -> Vec<u8> {
        let mut data = self.data.clone();
        for chunk in data.chunks_exact_mut(3) {
            chunk.swap(0, 2); // B <-> R
        }
        data
    }
}

/// 矩形区域。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Region {
    /// 检查是否包含指定点
    pub fn contains_point(&self, x: i32, y: i32) -> bool {
        x >= self.x
            && x < self.x + self.width as i32
            && y >= self.y
            && y < self.y + self.height as i32
    }

    /// 两个区域是否相交
    pub fn intersects(&self, other: &Region) -> bool {
        self.x < other.x + other.width as i32
            && self.x + self.width as i32 > other.x
            && self.y < other.y + other.height as i32
            && self.y + self.height as i32 > other.y
    }

    /// 计算交集
    pub fn intersection(&self, other: &Region) -> Option<Region> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = (self.x + self.width as i32).min(other.x + other.width as i32);
        let bottom = (self.y + self.height as i32).min(other.y + other.height as i32);

        if right > x && bottom > y {
            Some(Region {
                x,
                y,
                width: (right - x) as u32,
                height: (bottom - y) as u32,
            })
        } else {
            None
        }
    }

    /// 面积
    pub fn area(&self) -> u64 {
        self.width as u64 * self.height as u64
    }
}

/// 边界框。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundingBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub confidence: f64,
    pub label: Option<String>,
}

/// 二维坐标点。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// 到另一个点的曼哈顿距离
    pub fn manhattan_distance(&self, other: &Point) -> u32 {
        ((self.x - other.x).abs() + (self.y - other.y).abs()) as u32
    }
}

/// 浮点坐标点。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PointF {
    pub x: f64,
    pub y: f64,
}

/// RGBA 颜色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const BLACK: Color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
    pub const WHITE: Color = Color {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    pub const RED: Color = Color {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    };
    pub const GREEN: Color = Color {
        r: 0,
        g: 255,
        b: 0,
        a: 255,
    };
    pub const BLUE: Color = Color {
        r: 0,
        g: 0,
        b: 255,
        a: 255,
    };
    pub const TRANSPARENT: Color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };

    pub fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// 从十六进制字符串解析（如 "#FF0000" 或 "#FF000080"）
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim_start_matches('#');
        match hex.len() {
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some(Self { r, g, b, a: 255 })
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                Some(Self { r, g, b, a })
            }
            _ => None,
        }
    }

    /// 转为十六进制字符串（带 # 前缀）
    pub fn to_hex(&self) -> String {
        if self.a == 255 {
            format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
        } else {
            format!("#{:02X}{:02X}{:02X}{:02X}", self.r, self.g, self.b, self.a)
        }
    }

    /// 两个颜色的欧氏距离 (0~441)
    pub fn distance(&self, other: &Color) -> f64 {
        let dr = self.r as f64 - other.r as f64;
        let dg = self.g as f64 - other.g as f64;
        let db = self.b as f64 - other.b as f64;
        (dr * dr + dg * dg + db * db).sqrt()
    }
}
