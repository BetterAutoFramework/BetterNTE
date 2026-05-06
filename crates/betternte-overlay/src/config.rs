use betternte_core::Color;
use serde::{Deserialize, Serialize};

fn default_opacity() -> f32 {
    0.8
}
fn default_width() -> u32 {
    1920
}
fn default_height() -> u32 {
    1080
}
fn default_true() -> bool {
    true
}
fn default_font_size() -> u32 {
    14
}
fn default_target_fps() -> u32 {
    30
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayConfig {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default = "default_opacity")]
    pub opacity: f32,

    #[serde(default = "default_width")]
    pub width: u32,

    #[serde(default = "default_height")]
    pub height: u32,

    #[serde(default)]
    pub show_fps: bool,

    #[serde(default)]
    pub show_trigger_status: bool,

    #[serde(default = "default_true")]
    pub show_recognition_results: bool,

    #[serde(default)]
    pub position: OverlayPosition,

    #[serde(default)]
    pub mode: OverlayMode,

    #[serde(default = "default_font_size")]
    pub font_size: u32,

    #[serde(default)]
    pub background_color: Option<Color>,

    #[serde(default)]
    pub fps_offset: Option<(i32, i32)>,

    #[serde(default = "default_target_fps")]
    pub target_fps: u32,
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            opacity: 0.8,
            width: 1920,
            height: 1080,
            show_fps: false,
            show_trigger_status: false,
            show_recognition_results: true,
            position: OverlayPosition::FollowGameWindow,
            mode: OverlayMode::Minimal,
            font_size: 14,
            background_color: None,
            fps_offset: None,
            target_fps: 30,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum OverlayPosition {
    #[serde(rename = "follow_game")]
    FollowGameWindow,

    #[serde(rename = "fixed")]
    Fixed { x: i32, y: i32 },

    #[serde(rename = "fixed_rect")]
    FixedRect {
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    },
}

impl Default for OverlayPosition {
    fn default() -> Self {
        Self::FollowGameWindow
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OverlayMode {
    #[serde(rename = "hidden")]
    Hidden,

    #[serde(rename = "minimal")]
    Minimal,

    #[serde(rename = "detailed")]
    Detailed,

    #[serde(rename = "custom")]
    Custom,
}

impl Default for OverlayMode {
    fn default() -> Self {
        Self::Minimal
    }
}
