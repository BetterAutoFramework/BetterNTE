//! betternte-core: 核心类型、配置、共享抽象
//!
//! 所有其他 crate 的公共基础，不依赖任何内部 crate。

pub mod capture;
pub mod config;
pub mod consts;
pub mod error;
pub mod event;
pub mod image;
pub mod input;
pub mod replay;
pub mod script;
pub mod task;
pub mod vision;
pub mod window;

// 重导出常用类型
pub use config::CaptureMethod;
pub use config::{
    AdbConfig, AdvancedConfig, ApiConfig, BarkConfig, CaptureConfig, DiscordConfig, EmulatorConfig,
    EngineConfig, GameConfig, HardwareAcceleration, HotkeyConfig, HotkeyTriggersConfig,
    KeyBindingsConfig, NotificationConfig, NotificationLevel, OcrEngineType, OverlayConfig,
    OverlayMode, ScriptConfig, SecurityConfig, SecurityMode, ServerChanConfig, Subscription,
    TelegramConfig,
};
pub use consts::*;
pub use error::{CoreError, Result};
pub use event::{EngineEvent, ErrorSeverity, TaskStopReason};
pub use image::{BoundingBox, CaptureFrame, Color, PixelFormat, Point, PointF, Region};
pub use replay::{
    ReplayArtifactManifest, ReplayConfig, ReplayExpect, ReplayLastTaskStoppedExpect,
    ReplayManifestExpect, ReplayMode, ReplaySessionMetaInputPipelineExpect,
};
pub use window::{GameWindow, Rect, Size};

// capture 模块重导出
pub use capture::{CaptureRuntimeOptions, CaptureTarget, ScreenCapture, WindowFinder};

// input 模块重导出
pub use input::{
    ForegroundInputBackend, InputController, InputMode, InputTarget, Key, MouseButton,
    ParseKeyError,
};

// vision 模块重导出
pub use vision::{
    ColorDetector, ColorTolerance, MatchConfig, MatchResult, OcrConfig, OcrEngine, OcrError,
    TemplateMatchParams, TemplateMatcher, TextRegion,
};

// task 模块重导出
pub use task::{TaskError, TaskExecutor};

// script 模块重导出
pub use script::{FindTemplateOpts, LogLevel, OcrResult};

#[cfg(test)]
mod tests {
    use super::*;

    // ━━━ Color 测试 ━━━

    #[test]
    fn test_color_from_hex_6char() {
        let c = Color::from_hex("#FF0000").unwrap();
        assert_eq!(
            c,
            Color {
                r: 255,
                g: 0,
                b: 0,
                a: 255
            }
        );
    }

    #[test]
    fn test_color_from_hex_8char() {
        let c = Color::from_hex("#FF000080").unwrap();
        assert_eq!(
            c,
            Color {
                r: 255,
                g: 0,
                b: 0,
                a: 128
            }
        );
    }

    #[test]
    fn test_color_from_hex_no_hash() {
        let c = Color::from_hex("00FF00").unwrap();
        assert_eq!(
            c,
            Color {
                r: 0,
                g: 255,
                b: 0,
                a: 255
            }
        );
    }

    #[test]
    fn test_color_from_hex_invalid() {
        assert!(Color::from_hex("#GGG").is_none());
        assert!(Color::from_hex("#12345").is_none());
        assert!(Color::from_hex("").is_none());
    }

    #[test]
    fn test_color_to_hex_roundtrip() {
        let c = Color::rgb(0xAB, 0xCD, 0xEF);
        let hex = c.to_hex();
        let c2 = Color::from_hex(&hex).unwrap();
        assert_eq!(c, c2);
    }

    #[test]
    fn test_color_distance_same_is_zero() {
        let c = Color::rgb(100, 200, 50);
        assert!((c.distance(&c) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_color_distance_black_white() {
        let d = Color::BLACK.distance(&Color::WHITE);
        // sqrt(255^2 * 3) = 441.67
        assert!((d - 441.67).abs() < 0.1);
    }

    // ━━━ Region 测试 ━━━

    #[test]
    fn test_region_contains_point_inside() {
        let r = Region {
            x: 10,
            y: 20,
            width: 100,
            height: 50,
        };
        assert!(r.contains_point(50, 40));
    }

    #[test]
    fn test_region_contains_point_outside() {
        let r = Region {
            x: 10,
            y: 20,
            width: 100,
            height: 50,
        };
        assert!(!r.contains_point(0, 0));
        assert!(!r.contains_point(200, 200));
        assert!(!r.contains_point(110, 40)); // x = right edge
    }

    #[test]
    fn test_region_intersection_overlapping() {
        let a = Region {
            x: 0,
            y: 0,
            width: 100,
            height: 100,
        };
        let b = Region {
            x: 50,
            y: 50,
            width: 100,
            height: 100,
        };
        let i = a.intersection(&b).unwrap();
        assert_eq!(
            i,
            Region {
                x: 50,
                y: 50,
                width: 50,
                height: 50
            }
        );
    }

    #[test]
    fn test_region_intersection_non_overlapping() {
        let a = Region {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        };
        let b = Region {
            x: 100,
            y: 100,
            width: 10,
            height: 10,
        };
        assert!(a.intersection(&b).is_none());
    }

    #[test]
    fn test_region_area() {
        let r = Region {
            x: 0,
            y: 0,
            width: 10,
            height: 20,
        };
        assert_eq!(r.area(), 200);
    }

    // ━━━ Rect 测试 ━━━

    #[test]
    fn test_rect_width_height() {
        let r = Rect::new(10, 20, 110, 70);
        assert_eq!(r.width(), 100);
        assert_eq!(r.height(), 50);
    }

    #[test]
    fn test_rect_center() {
        let r = Rect::new(0, 0, 100, 100);
        assert_eq!(r.center(), Point::new(50, 50));
    }

    #[test]
    fn test_rect_contains() {
        let r = Rect::new(0, 0, 100, 100);
        assert!(r.contains(50, 50));
        assert!(!r.contains(100, 100)); // right/bottom exclusive
        assert!(!r.contains(-1, 0));
    }

    // ━━━ CaptureFrame 测试 ━━━

    #[test]
    fn test_capture_frame_crop_valid() {
        let data = vec![0u8; 400 * 300 * 4]; // 400x300 BGRA
        let frame = CaptureFrame::new(400, 300, data, PixelFormat::Bgra, "test".into());
        let cropped = frame
            .crop(&Region {
                x: 10,
                y: 10,
                width: 50,
                height: 50,
            })
            .unwrap();
        assert_eq!(cropped.width, 50);
        assert_eq!(cropped.height, 50);
        assert_eq!(cropped.data.len(), 50 * 50 * 4);
    }

    #[test]
    fn test_capture_frame_crop_out_of_bounds() {
        let data = vec![0u8; 100 * 100 * 4];
        let frame = CaptureFrame::new(100, 100, data, PixelFormat::Bgra, "test".into());
        assert!(frame
            .crop(&Region {
                x: 50,
                y: 50,
                width: 100,
                height: 100
            })
            .is_none());
    }

    // ━━━ PixelFormat 测试 ━━━

    #[test]
    fn test_pixel_format_bytes_per_pixel() {
        assert_eq!(PixelFormat::Bgra.bytes_per_pixel(), 4);
        assert_eq!(PixelFormat::Rgba.bytes_per_pixel(), 4);
        assert_eq!(PixelFormat::Bgr.bytes_per_pixel(), 3);
        assert_eq!(PixelFormat::Rgb.bytes_per_pixel(), 3);
        assert_eq!(PixelFormat::Gray.bytes_per_pixel(), 1);
    }

    // ━━━ EngineConfig 测试 ━━━

    #[test]
    fn test_engine_config_default() {
        let config = EngineConfig::default();
        assert_eq!(config.capture.method, CaptureMethod::Auto);
        assert_eq!(config.capture.fps_cap, 30);
        assert_eq!(config.api.port, 23330);
        assert_eq!(config.api.host, "127.0.0.1");
        assert!(config.hotkeys.toggle_task == "F9");
    }

    #[test]
    fn test_engine_config_roundtrip() {
        let config = EngineConfig::default();
        let yaml = serde_yaml::to_string(&config).unwrap();
        let config2: EngineConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(config.api.port, config2.api.port);
        assert_eq!(config.capture.fps_cap, config2.capture.fps_cap);
        assert_eq!(
            config.hotkeys.emergency_stop,
            config2.hotkeys.emergency_stop
        );
    }

    // ━━━ Config 验证测试 ━━━

    #[test]
    fn test_config_validation_fps_cap_too_high() {
        let yaml = r#"
capture:
  fps_cap: 300
"#;
        let config: EngineConfig = serde_yaml::from_str(yaml).unwrap();
        let result = config::loader::validate_engine_config(&config);
        assert!(result.is_err());
    }

    // ━━━ EngineEvent 测试 ━━━

    #[test]
    fn test_engine_event_serialize() {
        let event = EngineEvent::TaskStarted {
            task_name: "test".into(),
            task_type: "solo".into(),
            timestamp: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("TaskStarted"));
        assert!(json.contains("test"));
    }

    // ━━━ CoreError 测试 ━━━

    #[test]
    fn test_core_error_display() {
        let e = CoreError::ConfigNotFound("test.yaml".into());
        assert!(e.to_string().contains("test.yaml"));
    }

    // ━━━ CaptureFrame 补充测试 ━━━

    #[test]
    fn test_capture_frame_create_known_dimensions() {
        let data = vec![0u8; 320 * 240 * 4];
        let frame = CaptureFrame::new(320, 240, data, PixelFormat::Bgra, "test".into());

        assert_eq!(frame.width, 320);
        assert_eq!(frame.height, 240);
        assert_eq!(frame.data.len(), 320 * 240 * 4);
        assert_eq!(frame.format, PixelFormat::Bgra);
    }

    #[test]
    fn test_capture_frame_crop_correct_region() {
        let data = vec![0u8; 400 * 300 * 4];
        let frame = CaptureFrame::new(400, 300, data, PixelFormat::Bgra, "test".into());
        let region = Region {
            x: 100,
            y: 50,
            width: 200,
            height: 150,
        };

        let cropped = frame.crop(&region).unwrap();

        assert_eq!(cropped.width, 200);
        assert_eq!(cropped.height, 150);
        assert_eq!(cropped.data.len(), (200 * 150 * 4) as usize);
    }

    #[test]
    fn test_capture_frame_crop_out_of_bounds_error() {
        let frame = CaptureFrame::new(
            100,
            100,
            vec![0u8; 100 * 100 * 4],
            PixelFormat::Bgra,
            "test".into(),
        );
        let region = Region {
            x: 50,
            y: 50,
            width: 100,
            height: 100,
        };

        let result = frame.crop(&region);
        assert!(result.is_none(), "越界裁剪应返回 None");
    }

    // ━━━ Color 补充测试 ━━━

    #[test]
    fn test_color_from_hex_red() {
        let color = Color::from_hex("#FF0000").unwrap();

        assert_eq!(color.r, 255);
        assert_eq!(color.g, 0);
        assert_eq!(color.b, 0);
        assert_eq!(color.a, 255, "6位十六进制 alpha 应默认 255");
    }

    #[test]
    fn test_color_from_hex_with_alpha() {
        let color = Color::from_hex("#FF000080").unwrap();

        assert_eq!(color.r, 255);
        assert_eq!(color.g, 0);
        assert_eq!(color.b, 0);
        assert_eq!(color.a, 128);
    }

    #[test]
    fn test_color_from_hex_invalid_returns_none() {
        assert!(Color::from_hex("#GGG").is_none());
        assert!(Color::from_hex("#12345").is_none());
        assert!(Color::from_hex("").is_none());
        assert!(Color::from_hex("#1234567890").is_none());
    }

    #[test]
    fn test_color_distance_black_and_white() {
        let d = Color::BLACK.distance(&Color::WHITE);
        assert!(
            (d - 441.672955).abs() < 0.01,
            "黑白距离约 441.67, 实际: {}",
            d
        );
    }

    #[test]
    fn test_color_distance_same_color_is_zero() {
        let c = Color::rgb(100, 200, 50);
        assert!(c.distance(&c) < f64::EPSILON);
    }

    // ━━━ CoreError Display 补充测试 ━━━

    #[test]
    fn test_core_error_display_config_not_found() {
        let err = CoreError::ConfigNotFound("engine.yaml".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("配置文件未找到"));
        assert!(msg.contains("engine.yaml"));
    }

    #[test]
    fn test_core_error_display_config_parse_error() {
        let err = CoreError::ConfigParseError("invalid yaml".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("配置解析错误"));
        assert!(msg.contains("invalid yaml"));
    }

    #[test]
    fn test_core_error_display_config_validation_error() {
        let err = CoreError::ConfigValidationError("fps_cap 超出范围".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("配置验证错误"));
        assert!(msg.contains("fps_cap 超出范围"));
    }

    #[test]
    fn test_core_error_display_invalid_argument() {
        let err = CoreError::InvalidArgument("port must be 1-65535".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("参数无效"));
        assert!(msg.contains("port must be 1-65535"));
    }

    #[test]
    fn test_core_error_display_timeout() {
        let err = CoreError::Timeout(5000);
        let msg = format!("{}", err);
        assert!(msg.contains("超时"), "应包含 '超时'");
        assert!(msg.contains("5000"), "应包含超时毫秒数");
    }

    #[test]
    fn test_core_error_display_window_not_found() {
        let err = CoreError::WindowNotFound("原神".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("窗口未找到"));
        assert!(msg.contains("原神"));
    }

    #[test]
    fn test_core_error_display_other() {
        let err = CoreError::Other("something unexpected".to_string());
        let msg = format!("{}", err);
        assert_eq!(msg, "something unexpected");
    }

    // ━━━ EngineConfig 补充测试 ━━━

    #[test]
    fn test_engine_config_load_valid_yaml() {
        let yaml = r#"
capture:
  method: "bitblt"
  fps_cap: 60
hotkeys:
  toggle_task: "F9"
  emergency_stop: "F12"
api:
  port: 23330
"#;
        let config: EngineConfig = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(config.capture.method, CaptureMethod::BitBlt);
        assert_eq!(config.capture.fps_cap, 60);
        assert_eq!(config.hotkeys.toggle_task, "F9");
        assert_eq!(config.hotkeys.emergency_stop, "F12");
        assert_eq!(config.api.port, 23330);
    }

    #[test]
    fn test_engine_config_empty_yaml_uses_all_defaults() {
        let config: EngineConfig = serde_yaml::from_str("{}").unwrap();

        assert_eq!(config.capture.method, CaptureMethod::Auto);
        assert_eq!(config.capture.fps_cap, 30);
        assert_eq!(config.api.port, 23330);
        assert_eq!(config.api.host, "127.0.0.1");
        assert_eq!(config.hotkeys.toggle_task, "F9");
        assert_eq!(config.hotkeys.emergency_stop, "F12");
    }

    #[test]
    fn test_engine_config_malformed_yaml_returns_parse_error() {
        let yaml = r#"
capture:
  method: "auto"
  fps_cap: [invalid yaml
"#;
        let result: std::result::Result<EngineConfig, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err(), "格式错误的 YAML 应返回解析错误");
    }

    #[test]
    fn test_engine_config_missing_fields_use_defaults() {
        let yaml = r#"
capture:
  method: "bitblt"
"#;
        let config: EngineConfig = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(config.capture.method, CaptureMethod::BitBlt);
        assert_eq!(config.capture.fps_cap, 30, "fps_cap 应使用默认值 30");
        assert_eq!(config.api.port, 23330, "api.port 应使用默认值");
        assert_eq!(config.hotkeys.toggle_task, "F9", "热键应使用默认值");
    }

    #[test]
    fn test_engine_config_missing_file_returns_default() {
        let config: EngineConfig = serde_yaml::from_str("").unwrap_or_default();

        assert_eq!(config.capture.method, CaptureMethod::Auto);
        assert_eq!(config.api.port, 23330);
    }

    // ━━━ Config 验证补充测试 ━━━

    #[test]
    fn test_config_validation_fps_cap_out_of_range() {
        let yaml = r#"
capture:
  fps_cap: 300
"#;
        let config: EngineConfig = serde_yaml::from_str(yaml).unwrap();
        let result = config::loader::validate_engine_config(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_validation_opacity_out_of_range() {
        let yaml = r#"
overlay:
  opacity: 1.5
"#;
        let config: EngineConfig = serde_yaml::from_str(yaml).unwrap();
        let result = config::loader::validate_engine_config(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_invalid_capture_method_string_parses() {
        let yaml = r#"
capture:
  method: "nonexistent_method"
"#;
        let result: std::result::Result<EngineConfig, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err(), "无效的截图方式应解析失败");
    }

    #[test]
    fn test_config_invalid_hotkey_format_parses() {
        let yaml = r#"
hotkeys:
  toggle_task: "NotARealKey"
"#;
        let config: EngineConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.hotkeys.toggle_task, "NotARealKey");
    }

    #[test]
    fn test_config_round_trip_load_save_load() {
        let config1 = EngineConfig::default();
        let yaml = serde_yaml::to_string(&config1).unwrap();
        let config2: EngineConfig = serde_yaml::from_str(&yaml).unwrap();

        // 验证反序列化后所有关键字段一致
        assert_eq!(config1.api.port, config2.api.port);
        assert_eq!(config1.capture.fps_cap, config2.capture.fps_cap);
        assert_eq!(config1.capture.method, config2.capture.method);
        assert_eq!(config1.hotkeys.toggle_task, config2.hotkeys.toggle_task);
        assert_eq!(
            config1.hotkeys.emergency_stop,
            config2.hotkeys.emergency_stop
        );

        // 二次序列化也应能反序列化回来
        let yaml2 = serde_yaml::to_string(&config2).unwrap();
        let config3: EngineConfig = serde_yaml::from_str(&yaml2).unwrap();
        assert_eq!(config2.api.port, config3.api.port);
        assert_eq!(config2.capture.fps_cap, config3.capture.fps_cap);
    }

    // ━━━ EngineEvent 补充测试 ━━━

    #[test]
    fn test_engine_event_task_started_roundtrip() {
        let event = EngineEvent::TaskStarted {
            task_name: "auto_fish".into(),
            task_type: "solo".into(),
            timestamp: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let event2: EngineEvent = serde_json::from_str(&json).unwrap();

        if let EngineEvent::TaskStarted { task_name, .. } = event2 {
            assert_eq!(task_name, "auto_fish");
        } else {
            panic!("Expected TaskStarted");
        }
    }

    #[test]
    fn test_engine_event_task_stopped_roundtrip() {
        let event = EngineEvent::TaskStopped {
            task_name: "auto_fish".into(),
            reason: TaskStopReason::Completed,
            duration_ms: 12345,
            timestamp: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let event2: EngineEvent = serde_json::from_str(&json).unwrap();

        if let EngineEvent::TaskStopped {
            task_name,
            reason,
            duration_ms,
            ..
        } = event2
        {
            assert_eq!(task_name, "auto_fish");
            assert!(matches!(reason, TaskStopReason::Completed));
            assert_eq!(duration_ms, 12345);
        } else {
            panic!("Expected TaskStopped");
        }
    }

    #[test]
    fn test_engine_event_script_loaded_roundtrip() {
        let event = EngineEvent::ScriptLoaded {
            script_name: "fishing_bot".into(),
            version: "1.0.0".into(),
            path: "/scripts/fishing_bot".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let event2: EngineEvent = serde_json::from_str(&json).unwrap();

        if let EngineEvent::ScriptLoaded {
            script_name,
            version,
            ..
        } = event2
        {
            assert_eq!(script_name, "fishing_bot");
            assert_eq!(version, "1.0.0");
        } else {
            panic!("Expected ScriptLoaded");
        }
    }

    #[test]
    fn test_engine_event_config_changed_roundtrip() {
        let event = EngineEvent::ConfigChanged {
            key: "capture.fps_cap".into(),
            old_value: Some("30".into()),
            new_value: "60".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let event2: EngineEvent = serde_json::from_str(&json).unwrap();

        if let EngineEvent::ConfigChanged {
            key,
            old_value,
            new_value,
        } = event2
        {
            assert_eq!(key, "capture.fps_cap");
            assert_eq!(old_value, Some("30".into()));
            assert_eq!(new_value, "60");
        } else {
            panic!("Expected ConfigChanged");
        }
    }

    #[test]
    fn test_engine_event_error_roundtrip() {
        let event = EngineEvent::Error {
            module: "capture".into(),
            message: "failed to capture frame".into(),
            severity: ErrorSeverity::Error,
            recoverable: true,
        };
        let json = serde_json::to_string(&event).unwrap();
        let event2: EngineEvent = serde_json::from_str(&json).unwrap();

        if let EngineEvent::Error {
            module,
            severity,
            recoverable,
            ..
        } = event2
        {
            assert_eq!(module, "capture");
            assert!(matches!(severity, ErrorSeverity::Error));
            assert!(recoverable);
        } else {
            panic!("Expected Error");
        }
    }

    // ━━━ GameWindow 测试 ━━━

    #[test]
    fn test_game_window_create_all_fields() {
        let window = GameWindow {
            hwnd: 0x12345,
            title: "原神".into(),
            class_name: "UnityWndClass".into(),
            pid: 1234,
            process_name: "YuanShen.exe".into(),
            rect: Rect::new(0, 0, 1920, 1080),
            client_rect: Rect::new(0, 0, 1920, 1040),
            is_minimized: false,
            dpi_scale: 1.0,
        };

        assert_eq!(window.hwnd, 0x12345);
        assert_eq!(window.title, "原神");
        assert_eq!(window.class_name, "UnityWndClass");
        assert_eq!(window.pid, 1234);
        assert_eq!(window.process_name, "YuanShen.exe");
        assert!(!window.is_minimized);
        assert!((window.dpi_scale - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_game_window_json_roundtrip() {
        let window = GameWindow {
            hwnd: 0x12345,
            title: "原神".into(),
            class_name: "UnityWndClass".into(),
            pid: 1234,
            process_name: "YuanShen.exe".into(),
            rect: Rect::new(0, 0, 1920, 1080),
            client_rect: Rect::new(0, 0, 1920, 1040),
            is_minimized: false,
            dpi_scale: 1.5,
        };

        let json = serde_json::to_string(&window).unwrap();
        let window2: GameWindow = serde_json::from_str(&json).unwrap();

        assert_eq!(window.hwnd, window2.hwnd);
        assert_eq!(window.title, window2.title);
        assert_eq!(window.pid, window2.pid);
        assert!((window.dpi_scale - window2.dpi_scale).abs() < f64::EPSILON);
    }

    // ━━━ Region 补充测试 ━━━

    #[test]
    fn test_region_fully_contained_intersection() {
        let a = Region {
            x: 0,
            y: 0,
            width: 100,
            height: 100,
        };
        let b = Region {
            x: 10,
            y: 10,
            width: 20,
            height: 20,
        };
        let i = a.intersection(&b).unwrap();
        assert_eq!(
            i,
            Region {
                x: 10,
                y: 10,
                width: 20,
                height: 20
            }
        );
    }

    #[test]
    fn test_region_intersection_of_two_regions() {
        let a = Region {
            x: 0,
            y: 0,
            width: 100,
            height: 100,
        };
        let b = Region {
            x: 50,
            y: 50,
            width: 100,
            height: 100,
        };
        let i = a.intersection(&b).unwrap();
        assert_eq!(
            i,
            Region {
                x: 50,
                y: 50,
                width: 50,
                height: 50
            }
        );
    }

    #[test]
    fn test_region_non_overlapping_intersection() {
        let a = Region {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        };
        let b = Region {
            x: 100,
            y: 100,
            width: 10,
            height: 10,
        };
        assert!(a.intersection(&b).is_none());
    }

    #[test]
    fn test_capture_frame_resize_scales_correctly() {
        let data = vec![0u8; 800 * 600 * 4];
        let frame = CaptureFrame::new(800, 600, data, PixelFormat::Bgra, "test".into());

        let resized = frame.resize(400, 300).unwrap();

        assert_eq!(resized.width, 400, "缩放后宽度应为 400");
        assert_eq!(resized.height, 300, "缩放后高度应为 300");
    }

    #[test]
    fn test_capture_frame_to_bytes_non_empty() {
        let data = vec![128u8; 100 * 100 * 4];
        let frame = CaptureFrame::new(100, 100, data, PixelFormat::Rgba, "test".into());

        let bytes = frame.to_bytes("png").unwrap();

        assert!(!bytes.is_empty(), "导出字节不应为空");
        assert!(bytes.len() > 8, "PNG 数据应至少 8 字节");
        assert_eq!(bytes[0], 0x89, "PNG 首字节应为 0x89");
        assert_eq!(bytes[1], 0x50, "PNG 第二字节应为 0x50 ('P')");
    }

    #[test]
    fn test_capture_frame_to_image_correct_size() {
        let data = vec![0u8; 640 * 480 * 4];
        let frame = CaptureFrame::new(640, 480, data, PixelFormat::Rgba, "test".into());

        let image = frame.to_dynamic_image().unwrap();

        assert_eq!(image.width(), 640, "图像宽度应为 640");
        assert_eq!(image.height(), 480, "图像高度应为 480");
    }
}
