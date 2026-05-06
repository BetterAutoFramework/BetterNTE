//! 窗口和几何类型

use crate::image::{Point, Region};
use serde::{Deserialize, Serialize};

/// 矩形（屏幕坐标）。
///
/// 用 left/top 表示左上角，right/bottom 表示右下角。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    /// 从 (x, y, width, height) 创建
    pub fn from_xywh(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            left: x,
            top: y,
            right: x + width as i32,
            bottom: y + height as i32,
        }
    }

    pub fn width(&self) -> u32 {
        (self.right - self.left).max(0) as u32
    }

    pub fn height(&self) -> u32 {
        (self.bottom - self.top).max(0) as u32
    }

    pub fn center(&self) -> Point {
        Point {
            x: (self.left + self.right) / 2,
            y: (self.top + self.bottom) / 2,
        }
    }

    pub fn area(&self) -> u64 {
        self.width() as u64 * self.height() as u64
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }

    pub fn is_empty(&self) -> bool {
        self.width() == 0 || self.height() == 0
    }

    pub fn to_region(&self) -> Region {
        Region {
            x: self.left,
            y: self.top,
            width: self.width(),
            height: self.height(),
        }
    }
}

/// 尺寸。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    pub fn area(&self) -> u64 {
        self.width as u64 * self.height as u64
    }
}

/// 游戏窗口信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameWindow {
    /// 窗口句柄（Win32 HWND 的数值表示）
    pub hwnd: u64,
    /// 窗口标题
    pub title: String,
    /// 窗口类名
    pub class_name: String,
    /// 进程 ID
    pub pid: u32,
    /// 进程名称
    pub process_name: String,
    /// 窗口矩形（屏幕坐标）
    pub rect: Rect,
    /// 客户区矩形（不含标题栏和边框）
    pub client_rect: Rect,
    /// 是否最小化
    pub is_minimized: bool,
    /// DPI 缩放比例
    pub dpi_scale: f64,
}
