//! Engine configuration shape for `~/.betternte/engine.yaml`.

pub mod loader;

pub use loader::{
    default_config_dir, default_engine_config_path, load_default_engine_config, load_engine_config,
    save_default_engine_config, save_engine_config,
};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::input::{ForegroundInputBackend, InputMode};
use crate::replay::ReplayConfig;

// ============================================================================
// Top-level engine config
// ============================================================================

/// Root engine configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineConfig {
    pub capture: CaptureConfig,
    pub hotkeys: HotkeyConfig,
    #[serde(default)]
    pub hotkey_triggers: HotkeyTriggersConfig,
    pub key_bindings: KeyBindingsConfig,
    pub overlay: OverlayConfig,
    pub scripts: ScriptConfig,
    pub triggers: HashMap<String, TriggerState>,
    /// Default / last-used task script params from `params_schema`, keyed by manifest script `name`.
    #[serde(default)]
    pub task_script_params: HashMap<String, serde_json::Value>,
    pub notifications: NotificationConfig,
    pub api: ApiConfig,
    pub game: GameConfig,
    pub advanced: AdvancedConfig,
    pub replay: ReplayConfig,
    #[serde(default = "default_active_plugin")]
    pub active_plugin: String,
    #[serde(default = "default_plugin_search_paths")]
    pub plugin_search_paths: Vec<String>,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            capture: CaptureConfig::default(),
            hotkeys: HotkeyConfig::default(),
            hotkey_triggers: HotkeyTriggersConfig::default(),
            key_bindings: KeyBindingsConfig::default(),
            overlay: OverlayConfig::default(),
            scripts: ScriptConfig::default(),
            triggers: HashMap::new(),
            task_script_params: HashMap::new(),
            notifications: NotificationConfig::default(),
            api: ApiConfig::default(),
            game: GameConfig::default(),
            advanced: AdvancedConfig::default(),
            replay: ReplayConfig::default(),
            active_plugin: default_active_plugin(),
            plugin_search_paths: default_plugin_search_paths(),
        }
    }
}

// ============================================================================
// Capture
// ============================================================================

/// Capture backend selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureMethod {
    Auto,
    BitBlt,
    PrintWindow,
    DwmSharedSurface,
    WindowsGraphicsCapture,
    DxgiDesktopDuplication,
    AdbScreencap,
    AdbScrcpy,
    AdbMinicap,
    MumuExtras,
    LdExtras,
}

/// Capture target type used by engine runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureTargetType {
    Window,
    Display,
}

impl Default for CaptureTargetType {
    fn default() -> Self {
        Self::Window
    }
}

/// Cropping strategy for window capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureCropMode {
    /// Crop to window client area (exclude title bar / shadow).
    ClientOnly,
    /// Keep full window area.
    Window,
}

impl Default for CaptureCropMode {
    fn default() -> Self {
        Self::ClientOnly
    }
}

/// HDR processing policy for capture backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HdrPolicy {
    Off,
    Auto,
    Force,
}

impl Default for HdrPolicy {
    fn default() -> Self {
        Self::Auto
    }
}

/// Minimized-window behavior in capture loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MinimizedBehavior {
    Pause,
    KeepTrying,
    /// Reserved for future pseudo-background support.
    PseudoBackground,
}

impl Default for MinimizedBehavior {
    fn default() -> Self {
        Self::KeepTrying
    }
}

impl Default for CaptureMethod {
    fn default() -> Self {
        Self::Auto
    }
}

impl std::fmt::Display for CaptureMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => write!(f, "auto"),
            Self::BitBlt => write!(f, "bitblt"),
            Self::PrintWindow => write!(f, "print_window"),
            Self::DwmSharedSurface => write!(f, "dwm_shared_surface"),
            Self::WindowsGraphicsCapture => write!(f, "windows_graphics_capture"),
            Self::DxgiDesktopDuplication => write!(f, "dxgi_desktop_duplication"),
            Self::AdbScreencap => write!(f, "adb_screencap"),
            Self::AdbScrcpy => write!(f, "adb_scrcpy"),
            Self::AdbMinicap => write!(f, "adb_minicap"),
            Self::MumuExtras => write!(f, "mumu_extras"),
            Self::LdExtras => write!(f, "ld_extras"),
        }
    }
}

/// ADB capture settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AdbConfig {
    pub server_address: String,
    pub device_serial: String,
    pub auto_benchmark: bool,
}

impl Default for AdbConfig {
    fn default() -> Self {
        Self {
            server_address: "127.0.0.1:5037".into(),
            device_serial: String::new(),
            auto_benchmark: true,
        }
    }
}

/// Emulator-specific capture settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EmulatorConfig {
    pub emulator_type: String,
    pub instance_index: u32,
}

impl Default for EmulatorConfig {
    fn default() -> Self {
        Self {
            emulator_type: "auto".into(),
            instance_index: 0,
        }
    }
}

/// Screen capture configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CaptureConfig {
    pub method: CaptureMethod,
    pub method_whitelist: Vec<CaptureMethod>,
    pub target_type: CaptureTargetType,
    pub display_index: u32,
    pub fps_cap: u32,
    pub crop_mode: CaptureCropMode,
    pub hdr_policy: HdrPolicy,
    pub minimized_behavior: MinimizedBehavior,
    pub recover_on_resize: bool,
    pub recover_on_monitor_switch: bool,
    pub crop_shadow: bool,
    pub hdr_to_sdr: bool,
    pub adb: AdbConfig,
    pub emulator: EmulatorConfig,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            method: CaptureMethod::Auto,
            method_whitelist: vec![
                CaptureMethod::BitBlt,
                CaptureMethod::PrintWindow,
                CaptureMethod::DwmSharedSurface,
                CaptureMethod::WindowsGraphicsCapture,
                CaptureMethod::DxgiDesktopDuplication,
            ],
            target_type: CaptureTargetType::Window,
            display_index: 0,
            fps_cap: 30,
            crop_mode: CaptureCropMode::ClientOnly,
            hdr_policy: HdrPolicy::Auto,
            minimized_behavior: MinimizedBehavior::KeepTrying,
            recover_on_resize: true,
            recover_on_monitor_switch: true,
            crop_shadow: true,
            hdr_to_sdr: true,
            adb: AdbConfig::default(),
            emulator: EmulatorConfig::default(),
        }
    }
}

// ============================================================================
// Hotkeys
// ============================================================================

/// Global hotkey bindings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HotkeyConfig {
    pub toggle_task: String,
    pub emergency_stop: String,
    pub toggle_overlay: String,
    pub toggle_pause: String,
    pub screenshot: String,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            toggle_task: "Ctrl+L".into(),
            emergency_stop: "Ctrl+P".into(),
            toggle_overlay: "Ctrl+O".into(),
            toggle_pause: "Ctrl+I".into(),
            screenshot: "Ctrl+U".into(),
        }
    }
}

/// Global shortcuts that start a solo-task script or a task group (flow id / uuid).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HotkeyTriggersConfig {
    /// Shortcut string (e.g. `Ctrl+Shift+F9`) → script manifest `name`.
    pub scripts: HashMap<String, String>,
    /// Shortcut string → task group flow id / uuid.
    pub task_groups: HashMap<String, String>,
}

impl Default for HotkeyTriggersConfig {
    fn default() -> Self {
        Self {
            scripts: HashMap::new(),
            task_groups: HashMap::new(),
        }
    }
}

// ============================================================================
// Key bindings
// ============================================================================

/// Logical action → key name mapping for scripts / helpers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyBindingsConfig {
    pub bindings: HashMap<String, String>,
}

impl Default for KeyBindingsConfig {
    fn default() -> Self {
        let mut bindings = HashMap::new();
        bindings.insert("attack".into(), "VK_LBUTTON".into());
        bindings.insert("skill".into(), "VK_E".into());
        bindings.insert("burst".into(), "VK_Q".into());
        bindings.insert("jump".into(), "VK_SPACE".into());
        bindings.insert("sprint".into(), "VK_SHIFT".into());
        bindings.insert("interact".into(), "VK_F".into());
        bindings.insert("map".into(), "VK_M".into());
        bindings.insert("inventory".into(), "VK_I".into());
        bindings.insert("menu".into(), "VK_ESCAPE".into());
        bindings.insert("forward".into(), "VK_W".into());
        bindings.insert("backward".into(), "VK_S".into());
        bindings.insert("left".into(), "VK_A".into());
        bindings.insert("right".into(), "VK_D".into());
        bindings.insert("char_1".into(), "VK_1".into());
        bindings.insert("char_2".into(), "VK_2".into());
        bindings.insert("char_3".into(), "VK_3".into());
        bindings.insert("char_4".into(), "VK_4".into());
        Self { bindings }
    }
}

// ============================================================================
// Overlay
// ============================================================================

/// Overlay visualization mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayMode {
    Hidden,
    Minimal,
    Detailed,
}

impl Default for OverlayMode {
    fn default() -> Self {
        Self::Minimal
    }
}

/// On-screen overlay configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OverlayConfig {
    pub enabled: bool,
    pub mode: OverlayMode,
    pub opacity: f32,
    pub font_size: u32,
    pub background_color: String,
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: OverlayMode::Minimal,
            opacity: 0.8,
            font_size: 14,
            background_color: "#00000080".into(),
        }
    }
}

// ============================================================================
// Script subscriptions
// ============================================================================

/// Remote or local script subscription entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Subscription {
    /// Display label (e.g. default preset names in zh-CN).
    pub name: String,
    /// Directory name under `data_root` (e.g. `"main"`).
    pub directory: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub auto_update: bool,
    /// Optional remote update URL.
    pub url: Option<String>,
}

/// Script roots and subscription list.
#[derive(Debug, Clone, Serialize)]
pub struct ScriptConfig {
    /// Data root directory (default `"data"`).
    pub data_root: String,
    pub auto_update: bool,
    /// Registered subscriptions.
    pub subscriptions: Vec<Subscription>,
}

impl Default for ScriptConfig {
    fn default() -> Self {
        Self {
            data_root: "data".into(),
            auto_update: false,
            subscriptions: vec![
                Subscription {
                    name: "官方源".into(),
                    directory: "main".into(),
                    enabled: true,
                    auto_update: true,
                    url: None,
                },
                Subscription {
                    name: "本地源".into(),
                    directory: "local".into(),
                    enabled: true,
                    auto_update: false,
                    url: None,
                },
            ],
        }
    }
}

/// Backward-compatible deserializer for legacy `engine.yaml` layouts.
///
/// Older files used `directory`, `triggers_directory`, etc.; modern layout uses `data_root` + `subscriptions`.
impl<'de> Deserialize<'de> for ScriptConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::MapAccess;

        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            DataRoot,
            Directory,
            TriggersDirectory,
            TaskGroupsDirectory,
            FlowsDirectory,
            AssetsDirectory,
            AutoUpdate,
            Subscriptions,
            Repos,
        }

        struct ScriptConfigVisitor;

        impl<'de> serde::de::Visitor<'de> for ScriptConfigVisitor {
            type Value = ScriptConfig;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("ScriptConfig")
            }

            fn visit_map<M>(self, mut map: M) -> Result<ScriptConfig, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut data_root: Option<String> = None;
                let mut _directory: Option<String> = None;
                let mut auto_update = false;
                let mut subscriptions: Option<Vec<Subscription>> = None;
                let mut repos: Option<Vec<serde_json::Value>> = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        Field::DataRoot => {
                            data_root = Some(map.next_value()?);
                        }
                        Field::Directory => {
                            _directory = Some(map.next_value()?);
                        }
                        Field::TriggersDirectory
                        | Field::TaskGroupsDirectory
                        | Field::FlowsDirectory
                        | Field::AssetsDirectory => {
                            // Ignore deprecated keys but consume their values.
                            let _: String = map.next_value()?;
                        }
                        Field::AutoUpdate => {
                            auto_update = map.next_value()?;
                        }
                        Field::Subscriptions => {
                            subscriptions = Some(map.next_value()?);
                        }
                        Field::Repos => {
                            repos = Some(map.next_value()?);
                        }
                    }
                }

                // Modern layout: subscriptions array present.
                if let Some(subs) = subscriptions {
                    return Ok(ScriptConfig {
                        data_root: data_root.unwrap_or_else(|| "data".into()),
                        auto_update,
                        subscriptions: subs,
                    });
                }

                // Legacy layout: `repos` array → subscriptions.
                if let Some(old_repos) = repos {
                    let subs: Vec<Subscription> = old_repos
                        .into_iter()
                        .filter_map(|v| {
                            let name = v.get("name")?.as_str()?.to_string();
                            let url = v.get("url")?.as_str()?.to_string();
                            let enabled =
                                v.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
                            Some(Subscription {
                                name,
                                directory: String::new(), // legacy entries had no directory
                                enabled,
                                auto_update: false,
                                url: Some(url),
                            })
                        })
                        .collect();
                    return Ok(ScriptConfig {
                        data_root: data_root.unwrap_or_else(|| "data".into()),
                        auto_update,
                        subscriptions: subs,
                    });
                }

                // Oldest layout: only generic directory metadata.
                Ok(ScriptConfig {
                    data_root: data_root.unwrap_or_else(|| "data".into()),
                    auto_update,
                    subscriptions: ScriptConfig::default().subscriptions,
                })
            }
        }

        deserializer.deserialize_map(ScriptConfigVisitor)
    }
}

// ============================================================================
// Notifications
// ============================================================================

/// Minimum severity routed to external channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationLevel {
    Info,
    Warning,
    Error,
}

impl Default for NotificationLevel {
    fn default() -> Self {
        Self::Warning
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TelegramConfig {
    pub enabled: bool,
    pub bot_token: String,
    pub chat_id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DiscordConfig {
    pub enabled: bool,
    pub webhook_url: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerChanConfig {
    pub enabled: bool,
    pub send_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BarkConfig {
    pub enabled: bool,
    pub server_url: String,
    pub device_key: String,
}

impl Default for BarkConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            server_url: "https://api.day.app".into(),
            device_key: String::new(),
        }
    }
}

/// Notification channel bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationConfig {
    pub enabled: bool,
    pub level: NotificationLevel,
    pub telegram: TelegramConfig,
    pub discord: DiscordConfig,
    pub serverchan: ServerChanConfig,
    pub bark: BarkConfig,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            level: NotificationLevel::Warning,
            telegram: TelegramConfig::default(),
            discord: DiscordConfig::default(),
            serverchan: ServerChanConfig::default(),
            bark: BarkConfig::default(),
        }
    }
}

// ============================================================================
// HTTP / WS API
// ============================================================================

/// Embedded API server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ApiConfig {
    pub host: String,
    pub port: u16,
    pub auth_token: String,
    pub cors_enabled: bool,
    pub ws_heartbeat_interval: u64,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 23330,
            auth_token: String::new(),
            cors_enabled: true,
            ws_heartbeat_interval: 30,
        }
    }
}

// ============================================================================
// Game / window targeting
// ============================================================================

/// Game metadata used for window discovery and scaling hints.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GameConfig {
    pub game_name: String,
    pub window_title_keyword: String,
    /// Process executable name (e.g. `"HTGame.exe"`) for window matching.
    pub process_name: String,
    pub game_language: String,
    /// Client resolution string such as `"1920x1080"`.
    pub resolution: String,
    /// UI scale factor (e.g. `1.0`, `1.25`, `1.5`).
    pub scale: f64,
    /// Display DPI (e.g. `96`, `120`, `144`).
    pub dpi: u32,
    pub launch_args: String,
    pub auto_launch: bool,
    pub launch_delay: u32,
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            game_name: "异环".into(),
            window_title_keyword: "异环".into(),
            process_name: "HTGame.exe".into(),
            game_language: "zh-cn".into(),
            resolution: "1920x1080".into(),
            scale: 1.0,
            dpi: 96,
            launch_args: String::new(),
            auto_launch: false,
            launch_delay: 30,
        }
    }
}

// ============================================================================
// Advanced runtime toggles
// ============================================================================

/// OCR backend identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OcrEngineType {
    PaddleOcr,
}

impl Default for OcrEngineType {
    fn default() -> Self {
        Self::PaddleOcr
    }
}

/// Hardware acceleration preference for vision workloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HardwareAcceleration {
    Auto,
    Cuda,
    DirectMl,
    Cpu,
}

impl Default for HardwareAcceleration {
    fn default() -> Self {
        Self::Auto
    }
}

/// OCR tuning parameters bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OcrTuningProfile {
    pub max_side_len: u32,
    pub det_threshold: f64,
    pub rec_threshold: f64,
    pub batch_size: usize,
    pub unclip_ratio: f64,
}

impl Default for OcrTuningProfile {
    fn default() -> Self {
        Self {
            max_side_len: 960,
            det_threshold: 0.3,
            rec_threshold: 0.5,
            batch_size: 8,
            unclip_ratio: 2.0,
        }
    }
}

/// OCR presets for quick tuning in UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OcrPresetsConfig {
    pub performance: OcrTuningProfile,
    pub balanced: OcrTuningProfile,
    pub accuracy: OcrTuningProfile,
}

impl Default for OcrPresetsConfig {
    fn default() -> Self {
        Self {
            performance: OcrTuningProfile {
                max_side_len: 640,
                det_threshold: 0.25,
                rec_threshold: 0.45,
                batch_size: 16,
                unclip_ratio: 1.6,
            },
            balanced: OcrTuningProfile::default(),
            accuracy: OcrTuningProfile {
                max_side_len: 1280,
                det_threshold: 0.35,
                rec_threshold: 0.55,
                batch_size: 4,
                unclip_ratio: 2.4,
            },
        }
    }
}

/// Vision, logging, and input tuning knobs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AdvancedConfig {
    pub ocr_engine: OcrEngineType,
    pub ocr_model_dir: String,
    pub ocr_max_side_len: u32,
    pub ocr_det_threshold: f64,
    pub ocr_rec_threshold: f64,
    pub ocr_batch_size: usize,
    pub ocr_unclip_ratio: f64,
    /// Target text color for enhanced OCR recognition (e.g. "#FFFFFF").
    pub ocr_text_color: Option<String>,
    /// Tolerance for text color matching (per-channel, 0-255). Default: 32.
    pub ocr_text_color_tolerance: u8,
    pub ocr_presets: OcrPresetsConfig,
    pub template_match_threshold: f64,
    pub hardware_acceleration: HardwareAcceleration,
    pub input_mode: InputMode,
    pub foreground_input_backend: ForegroundInputBackend,
    pub input_fallback_enabled: bool,
    pub input_fallback_mode: InputMode,
    pub input_fail_threshold: u32,
    pub input_probe_every_ms: u64,
    pub input_action_timeout_ms: u64,
    pub input_log_switch: bool,
    pub input_rate_limit: u32,
    pub log_level: String,
    pub log_file: String,
    pub log_max_size: u64,
    pub log_max_files: u64,
    pub task_groups_file: String,
    pub debug_screenshot_dir: String,
    pub debug_mode: bool,
}

impl Default for AdvancedConfig {
    fn default() -> Self {
        Self {
            ocr_engine: OcrEngineType::PaddleOcr,
            ocr_model_dir: "assets/models/paddleocr".into(),
            ocr_max_side_len: 960,
            ocr_det_threshold: 0.3,
            ocr_rec_threshold: 0.5,
            ocr_batch_size: 8,
            ocr_unclip_ratio: 2.0,
            ocr_text_color: None,
            ocr_text_color_tolerance: 32,
            ocr_presets: OcrPresetsConfig::default(),
            template_match_threshold: 0.8,
            hardware_acceleration: HardwareAcceleration::Auto,
            input_mode: InputMode::Auto,
            foreground_input_backend: ForegroundInputBackend::Enigo,
            input_fallback_enabled: true,
            input_fallback_mode: InputMode::Background,
            input_fail_threshold: 3,
            input_probe_every_ms: 3000,
            input_action_timeout_ms: 300,
            input_log_switch: true,
            input_rate_limit: 0,
            log_level: "info".into(),
            log_file: "logs/betternte.log".into(),
            log_max_size: 50,
            log_max_files: 5,
            task_groups_file: "task_groups.json".into(),
            debug_screenshot_dir: String::new(),
            debug_mode: false,
        }
    }
}

// ============================================================================
// Trigger persistence
// ============================================================================

/// Serialized trigger enablement + params from `engine.yaml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TriggerState {
    pub enabled: bool,
    pub params: serde_json::Value,
}

impl Default for TriggerState {
    fn default() -> Self {
        Self {
            enabled: false,
            params: serde_json::Value::Object(serde_json::Map::new()),
        }
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn default_true() -> bool {
    true
}

fn default_active_plugin() -> String {
    "nte".to_string()
}

fn default_plugin_search_paths() -> Vec<String> {
    vec!["plugins".to_string()]
}
