use betternte_core::Color;
use serde::{Deserialize, Serialize};

/// 矩形绘制对象
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RectDrawable {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub color: Color,
    pub thickness: u32,
    pub name: Option<String>,
}

impl RectDrawable {
    pub fn new(x: i32, y: i32, width: u32, height: u32, color: Color, thickness: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
            color,
            thickness,
            name: None,
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }
}

/// 文本绘制对象
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextDrawable {
    pub x: i32,
    pub y: i32,
    pub text: String,
    pub color: Color,
    pub font_size: u32,
    pub background: Option<Color>,
    pub name: Option<String>,
}

impl TextDrawable {
    pub fn new(x: i32, y: i32, text: impl Into<String>, color: Color, font_size: u32) -> Self {
        Self {
            x,
            y,
            text: text.into(),
            color,
            font_size,
            background: None,
            name: None,
        }
    }

    pub fn with_background(mut self, bg: Color) -> Self {
        self.background = Some(bg);
        self
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

/// 线段绘制对象
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineDrawable {
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
    pub color: Color,
    pub thickness: u32,
    pub name: Option<String>,
}

impl LineDrawable {
    pub fn new(x1: i32, y1: i32, x2: i32, y2: i32, color: Color, thickness: u32) -> Self {
        Self {
            x1,
            y1,
            x2,
            y2,
            color,
            thickness,
            name: None,
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

/// 模板匹配结果绘制对象
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchResultDrawable {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub confidence: f32,
    pub name: Option<String>,
}

impl MatchResultDrawable {
    pub fn new(x: i32, y: i32, width: u32, height: u32, confidence: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
            confidence,
            name: None,
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

/// 十字准星绘制对象
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrosshairDrawable {
    pub x: i32,
    pub y: i32,
    pub size: u32,
    pub color: Color,
    pub name: Option<String>,
}

impl CrosshairDrawable {
    pub fn new(x: i32, y: i32, size: u32, color: Color) -> Self {
        Self {
            x,
            y,
            size,
            color,
            name: None,
        }
    }
}

/// 进度条绘制对象
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressBarDrawable {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub progress: f32,
    pub fg_color: Color,
    pub bg_color: Color,
    pub name: Option<String>,
}

impl ProgressBarDrawable {
    pub fn new(
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        progress: f32,
        fg: Color,
        bg: Color,
    ) -> Self {
        Self {
            x,
            y,
            width,
            height,
            progress,
            fg_color: fg,
            bg_color: bg,
            name: None,
        }
    }
}
