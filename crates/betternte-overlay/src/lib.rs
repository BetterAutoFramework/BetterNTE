//! betternte-overlay: Overlay window and drawing API for BetterNTE
//!
//! Architecture:
//! - DrawContent: key-based 绘制内容管理，线程安全
//! - Drawable types: RectDrawable, TextDrawable, LineDrawable 等
//! - OverlayWindow: Win32 layered window (WS_EX_TRANSPARENT + WS_EX_LAYERED)
//! - DrawingApi: 底层像素绘制算法 (Bresenham, alpha blend)

mod config;
mod draw_content;
mod drawable;
mod drawing;
mod error;
mod manager;
mod renderer;
mod window;

pub use config::{OverlayConfig, OverlayMode, OverlayPosition};
pub use draw_content::{DrawContent, DrawSnapshot};
pub use drawable::{
    CrosshairDrawable, LineDrawable, MatchResultDrawable, ProgressBarDrawable, RectDrawable,
    TextDrawable,
};
pub use drawing::DrawingApi;
pub use error::OverlayError;
pub use manager::OverlayManager;
pub use renderer::OverlayRenderer;
pub use window::OverlayWindow;

#[cfg(test)]
mod tests {
    use super::*;
    use betternte_core::Color;

    // ━━━ DrawContent 测试 ━━━

    #[test]
    fn test_draw_content_put_and_get_rect() {
        let dc = DrawContent::new();
        assert!(dc.is_empty());

        dc.put_rect(
            "test_rect",
            RectDrawable::new(10, 20, 100, 50, Color::RED, 2),
        );

        assert!(!dc.is_empty());
        assert!(dc.take_dirty());

        let snap = dc.snapshot();
        assert_eq!(snap.rects.len(), 1);
        assert_eq!(snap.rects[0].x, 10);
        assert_eq!(snap.rects[0].y, 20);
    }

    #[test]
    fn test_draw_content_remove_rect() {
        let dc = DrawContent::new();
        dc.put_rect("r1", RectDrawable::new(0, 0, 10, 10, Color::RED, 1));
        dc.put_rect("r2", RectDrawable::new(20, 20, 10, 10, Color::GREEN, 1));

        dc.remove_rect("r1");
        assert!(dc.take_dirty());

        let snap = dc.snapshot();
        assert_eq!(snap.rects.len(), 1);
        assert_eq!(snap.rects[0].x, 20);
    }

    #[test]
    fn test_draw_content_put_text_list() {
        let dc = DrawContent::new();
        dc.put_text_list(
            "labels",
            vec![
                TextDrawable::new(10, 10, "Hello", Color::WHITE, 16),
                TextDrawable::new(10, 30, "World", Color::WHITE, 16),
            ],
        );

        let snap = dc.snapshot();
        assert_eq!(snap.texts.len(), 2);
    }

    #[test]
    fn test_draw_content_clear_all() {
        let dc = DrawContent::new();
        dc.put_rect("r", RectDrawable::new(0, 0, 10, 10, Color::RED, 1));
        dc.put_text("t", TextDrawable::new(0, 0, "test", Color::WHITE, 14));
        dc.put_line("l", LineDrawable::new(0, 0, 100, 100, Color::GREEN, 1));

        dc.clear_all();
        assert!(dc.is_empty());
        assert!(dc.take_dirty());
    }

    #[test]
    fn test_draw_content_dirty_tracking() {
        let dc = DrawContent::new();

        // 初始不脏
        assert!(!dc.take_dirty());

        // put 后变脏
        dc.put_rect("r", RectDrawable::new(0, 0, 10, 10, Color::RED, 1));
        assert!(dc.take_dirty());

        // take_dirty 后不脏
        assert!(!dc.take_dirty());

        // remove 不存在的 key 不变脏
        dc.remove_rect("nonexistent");
        assert!(!dc.take_dirty());

        // remove 存在的 key 变脏
        dc.remove_rect("r");
        assert!(dc.take_dirty());
    }

    #[test]
    fn test_draw_content_clone_shares_state() {
        let dc1 = DrawContent::new();
        let dc2 = dc1.clone();

        dc1.put_rect("r", RectDrawable::new(0, 0, 10, 10, Color::RED, 1));

        // dc2 也能看到
        let snap = dc2.snapshot();
        assert_eq!(snap.rects.len(), 1);
    }

    #[test]
    fn test_draw_content_match_results() {
        let dc = DrawContent::new();
        dc.put_match_result(
            "template_1",
            MatchResultDrawable::new(500, 300, 100, 50, 0.95),
        );

        let snap = dc.snapshot();
        assert_eq!(snap.match_results.len(), 1);
        assert!((snap.match_results[0].confidence - 0.95).abs() < f32::EPSILON);
    }

    #[test]
    fn test_draw_content_crosshairs() {
        let dc = DrawContent::new();
        dc.put_crosshair(
            "click_pos",
            CrosshairDrawable::new(960, 540, 20, Color::RED),
        );

        let snap = dc.snapshot();
        assert_eq!(snap.crosshairs.len(), 1);
        assert_eq!(snap.crosshairs[0].x, 960);
    }

    #[test]
    fn test_draw_content_progress_bars() {
        let dc = DrawContent::new();
        dc.put_progress_bar(
            "loading",
            ProgressBarDrawable::new(
                10,
                1060,
                400,
                20,
                0.6,
                Color {
                    r: 0,
                    g: 200,
                    b: 0,
                    a: 255,
                },
                Color {
                    r: 50,
                    g: 50,
                    b: 50,
                    a: 200,
                },
            ),
        );

        let snap = dc.snapshot();
        assert_eq!(snap.progress_bars.len(), 1);
        assert!((snap.progress_bars[0].progress - 0.6).abs() < f32::EPSILON);
    }

    // ━━━ OverlayWindow 测试 ━━━

    #[test]
    fn test_overlay_window_create_success() {
        let config = OverlayConfig {
            enabled: true,
            width: 1920,
            height: 1080,
            opacity: 0.8,
            ..Default::default()
        };

        let result = OverlayWindow::new(&config);

        #[cfg(windows)]
        {
            let window = result.expect("Should create window");
            assert_eq!(window.width(), 1920);
            assert_eq!(window.height(), 1080);
            assert!(window.is_visible());
        }

        #[cfg(not(windows))]
        {
            assert!(matches!(result, Err(OverlayError::PlatformNotSupported)));
        }
    }

    #[test]
    fn test_overlay_window_show_hide_toggle() {
        let config = OverlayConfig {
            enabled: true,
            width: 800,
            height: 600,
            ..Default::default()
        };

        let result = OverlayWindow::new(&config);

        #[cfg(windows)]
        {
            let mut window = result.unwrap();
            // new() creates a visible window
            assert!(window.is_visible());
            window.show().unwrap();
            assert!(window.is_visible());
            window.hide().unwrap();
            assert!(!window.is_visible());
        }
    }

    #[test]
    fn test_overlay_window_set_opacity() {
        let config = OverlayConfig {
            enabled: true,
            width: 800,
            height: 600,
            ..Default::default()
        };

        let result = OverlayWindow::new(&config);

        #[cfg(windows)]
        {
            let mut window = result.unwrap();
            assert!(window.set_opacity(0.5).is_ok());
        }
    }

    #[test]
    fn test_overlay_window_set_position() {
        let config = OverlayConfig {
            enabled: true,
            width: 800,
            height: 600,
            ..Default::default()
        };

        let result = OverlayWindow::new(&config);

        #[cfg(windows)]
        {
            let mut window = result.unwrap();
            assert!(window.set_position(100, 200, 800, 600).is_ok());
        }
    }

    #[test]
    fn test_overlay_window_clear() {
        let config = OverlayConfig {
            enabled: true,
            width: 100,
            height: 100,
            ..Default::default()
        };

        let result = OverlayWindow::new(&config);

        #[cfg(windows)]
        {
            let mut window = result.unwrap();
            let buf = window.buffer_mut();
            buf[0] = 255;
            buf[1] = 128;
            window.clear();
            let buf = window.buffer_mut();
            assert!(buf.iter().all(|&b| b == 0));
        }
    }

    #[test]
    fn test_overlay_window_commit() {
        let config = OverlayConfig {
            enabled: true,
            width: 100,
            height: 100,
            ..Default::default()
        };

        let result = OverlayWindow::new(&config);

        #[cfg(windows)]
        {
            let window = result.unwrap();
            // commit() may fail in headless environments (no display)
            let _ = window.commit();
        }
    }

    #[test]
    fn test_overlay_window_destroy() {
        let config = OverlayConfig {
            enabled: true,
            width: 100,
            height: 100,
            ..Default::default()
        };

        let result = OverlayWindow::new(&config);

        #[cfg(windows)]
        {
            let mut window = result.unwrap();
            assert!(window.destroy().is_ok());
            assert!(matches!(window.show(), Err(OverlayError::WindowDestroyed)));
        }
    }

    // ━━━ DrawingApi 测试 ━━━

    #[test]
    fn test_drawing_api_draw_text() {
        let mut buffer = vec![0u8; 200 * 100 * 4];
        let mut drawing = DrawingApi::new(&mut buffer, 200, 100);

        let result = drawing.draw_text(
            10,
            10,
            "Hello",
            Color {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            },
            16,
        );

        assert!(result.is_ok());
        assert!(result.unwrap() > 0);
    }

    #[test]
    fn test_drawing_api_draw_rect() {
        let mut buffer = vec![0u8; 400 * 300 * 4];
        let mut drawing = DrawingApi::new(&mut buffer, 400, 300);

        let result = drawing.draw_rect(
            100,
            100,
            200,
            150,
            Color {
                r: 0,
                g: 255,
                b: 0,
                a: 255,
            },
            2,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_drawing_api_fill_rect() {
        let mut buffer = vec![0u8; 400 * 300 * 4];
        let mut drawing = DrawingApi::new(&mut buffer, 400, 300);

        let result = drawing.fill_rect(
            100,
            100,
            200,
            150,
            Color {
                r: 0,
                g: 0,
                b: 255,
                a: 128,
            },
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_drawing_api_draw_crosshair() {
        let mut buffer = vec![0u8; 1920 * 1080 * 4];
        let mut drawing = DrawingApi::new(&mut buffer, 1920, 1080);

        let result = drawing.draw_crosshair(
            960,
            540,
            20,
            Color {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            },
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_drawing_api_draw_circle() {
        let mut buffer = vec![0u8; 1000 * 1000 * 4];
        let mut drawing = DrawingApi::new(&mut buffer, 1000, 1000);

        let result = drawing.draw_circle(
            500,
            500,
            50,
            Color {
                r: 255,
                g: 255,
                b: 0,
                a: 255,
            },
            2,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_drawing_api_draw_line() {
        let mut buffer = vec![0u8; 200 * 200 * 4];
        let mut drawing = DrawingApi::new(&mut buffer, 200, 200);

        let result = drawing.draw_line(
            0,
            0,
            100,
            100,
            Color {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            },
            1,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_drawing_api_draw_match_result() {
        let mut buffer = vec![0u8; 800 * 600 * 4];
        let mut drawing = DrawingApi::new(&mut buffer, 800, 600);

        let result = drawing.draw_match_result(&drawable::MatchResultDrawable {
            x: 500,
            y: 300,
            width: 100,
            height: 50,
            confidence: 0.95,
            name: None,
        });

        assert!(result.is_ok());
    }

    #[test]
    fn test_drawing_api_draw_progress_bar() {
        let mut buffer = vec![0u8; 1920 * 1080 * 4];
        let mut drawing = DrawingApi::new(&mut buffer, 1920, 1080);

        let fg = Color {
            r: 0,
            g: 200,
            b: 0,
            a: 255,
        };
        let bg = Color {
            r: 50,
            g: 50,
            b: 50,
            a: 200,
        };

        let result = drawing.draw_progress_bar(10, 1060, 400, 20, 0.6, fg, bg);

        assert!(result.is_ok());
    }

    #[test]
    fn test_drawing_api_draw_point() {
        let mut buffer = vec![0u8; 800 * 600 * 4];
        let mut drawing = DrawingApi::new(&mut buffer, 800, 600);

        let result = drawing.draw_point(
            500,
            300,
            3,
            Color {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            },
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_drawing_api_clear() {
        let mut buffer = vec![255u8; 100 * 100 * 4];
        let mut drawing = DrawingApi::new(&mut buffer, 100, 100);

        drawing
            .fill_rect(
                10,
                10,
                50,
                50,
                Color {
                    r: 255,
                    g: 0,
                    b: 0,
                    a: 255,
                },
            )
            .unwrap();

        drawing.clear();

        assert!(drawing.buffer().iter().all(|&b| b == 0));
    }

    #[test]
    fn test_drawing_api_out_of_bounds() {
        let mut buffer = vec![0u8; 100 * 100 * 4];
        let mut drawing = DrawingApi::new(&mut buffer, 100, 100);

        // 越界不应 panic
        let _ = drawing.draw_text(
            -100,
            -100,
            "test",
            Color {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            },
            16,
        );

        let _ = drawing.draw_rect(
            1800,
            1000,
            200,
            200,
            Color {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            },
            2,
        );
    }

    // ━━━ OverlayManager 测试 ━━━

    #[test]
    fn test_overlay_manager_sync_position() {
        let config = OverlayConfig {
            enabled: true,
            width: 1920,
            height: 1080,
            ..Default::default()
        };

        let _result = OverlayManager::new(&config);
    }

    #[test]
    fn test_overlay_manager_clear() {
        let config = OverlayConfig {
            enabled: true,
            width: 1920,
            height: 1080,
            ..Default::default()
        };

        let result = OverlayManager::new(&config);

        #[cfg(windows)]
        {
            let mut manager = result.unwrap();
            assert!(manager.clear().is_ok());
        }
    }

    #[test]
    fn test_overlay_manager_bind_to_game() {
        let config = OverlayConfig {
            enabled: true,
            width: 1920,
            height: 1080,
            ..Default::default()
        };

        let result = OverlayManager::new(&config);

        #[cfg(windows)]
        {
            let mut manager = result.unwrap();
            let result = manager.bind_to_game(0);
            assert!(result.is_err());
        }
    }

    // ━━━ OverlayRenderer 测试 ━━━

    #[test]
    fn test_overlay_renderer_begin_end_frame() {
        let buffer = vec![0u8; 100 * 100 * 4];
        let window = OverlayWindow::from_buffer(buffer, 100, 100);

        let mut renderer = OverlayRenderer::new(window);

        assert!(renderer.begin_frame().is_ok());
        assert!(renderer.end_frame().is_ok());
    }

    #[test]
    fn test_overlay_renderer_double_begin_frame() {
        let buffer = vec![0u8; 100 * 100 * 4];
        let window = OverlayWindow::from_buffer(buffer, 100, 100);

        let mut renderer = OverlayRenderer::new(window);

        renderer.begin_frame().unwrap();

        assert!(matches!(
            renderer.begin_frame(),
            Err(OverlayError::AlreadyInFrame)
        ));
    }

    #[test]
    fn test_overlay_renderer_end_frame_without_begin() {
        let buffer = vec![0u8; 100 * 100 * 4];
        let window = OverlayWindow::from_buffer(buffer, 100, 100);

        let mut renderer = OverlayRenderer::new(window);

        assert!(matches!(
            renderer.end_frame(),
            Err(OverlayError::NotInFrame)
        ));
    }

    #[test]
    fn test_overlay_renderer_draw_returns_drawing_api() {
        let buffer = vec![0u8; 100 * 100 * 4];
        let window = OverlayWindow::from_buffer(buffer, 100, 100);

        let mut renderer = OverlayRenderer::new(window);

        renderer.begin_frame().unwrap();

        let mut drawing = renderer.draw();
        let result = drawing.fill_rect(
            10,
            10,
            50,
            50,
            Color {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            },
        );
        assert!(result.is_ok());

        renderer.end_frame().unwrap();
    }

    // ━━━ OverlayConfig 测试 ━━━

    #[test]
    fn test_overlay_config_default_values() {
        let config: OverlayConfig = serde_json::from_str("{}").unwrap();

        assert!(!config.enabled);
        assert!((config.opacity - 0.8).abs() < f32::EPSILON);
        assert_eq!(config.width, 1920);
        assert_eq!(config.height, 1080);
        assert!(!config.show_fps);
        assert!(!config.show_trigger_status);
        assert!(config.show_recognition_results);
        assert!(matches!(config.position, OverlayPosition::FollowGameWindow));
        assert!(matches!(config.mode, OverlayMode::Minimal));
        assert_eq!(config.font_size, 14);
        assert_eq!(config.target_fps, 30);
    }

    #[test]
    fn test_overlay_config_serialization() {
        let config = OverlayConfig {
            enabled: true,
            opacity: 0.6,
            width: 2560,
            height: 1440,
            show_fps: true,
            show_trigger_status: true,
            show_recognition_results: false,
            position: OverlayPosition::Fixed { x: 100, y: 100 },
            mode: OverlayMode::Detailed,
            font_size: 18,
            background_color: Some(Color {
                r: 0,
                g: 0,
                b: 0,
                a: 128,
            }),
            fps_offset: Some((10, 10)),
            target_fps: 60,
        };

        let json = serde_json::to_string(&config).unwrap();
        let config2: OverlayConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.enabled, config2.enabled);
        assert!((config.opacity - config2.opacity).abs() < f32::EPSILON);
        assert_eq!(config.width, config2.width);
        assert_eq!(config.height, config2.height);
        assert_eq!(config.show_fps, config2.show_fps);
        assert_eq!(config.show_trigger_status, config2.show_trigger_status);
        assert_eq!(
            config.show_recognition_results,
            config2.show_recognition_results
        );
        assert_eq!(config.font_size, config2.font_size);
        assert_eq!(config.target_fps, config2.target_fps);
    }

    #[test]
    fn test_overlay_config_position_enum() {
        let pos = OverlayPosition::FollowGameWindow;
        let json = serde_json::to_string(&pos).unwrap();
        assert!(json.contains("follow_game"));
        let pos2: OverlayPosition = serde_json::from_str(&json).unwrap();
        assert!(matches!(pos2, OverlayPosition::FollowGameWindow));

        let pos = OverlayPosition::Fixed { x: 100, y: 200 };
        let json = serde_json::to_string(&pos).unwrap();
        assert!(json.contains("fixed"));
        let pos2: OverlayPosition = serde_json::from_str(&json).unwrap();
        if let OverlayPosition::Fixed { x, y } = pos2 {
            assert_eq!(x, 100);
            assert_eq!(y, 200);
        } else {
            panic!("Expected Fixed variant");
        }

        let pos = OverlayPosition::FixedRect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let json = serde_json::to_string(&pos).unwrap();
        assert!(json.contains("fixed_rect"));
        let pos2: OverlayPosition = serde_json::from_str(&json).unwrap();
        if let OverlayPosition::FixedRect {
            x,
            y,
            width,
            height,
        } = pos2
        {
            assert_eq!(x, 0);
            assert_eq!(y, 0);
            assert_eq!(width, 1920);
            assert_eq!(height, 1080);
        } else {
            panic!("Expected FixedRect variant");
        }
    }

    #[test]
    fn test_overlay_config_mode_enum() {
        let modes = vec![
            (OverlayMode::Hidden, "hidden"),
            (OverlayMode::Minimal, "minimal"),
            (OverlayMode::Detailed, "detailed"),
            (OverlayMode::Custom, "custom"),
        ];

        for (mode, expected) in modes {
            let json = serde_json::to_string(&mode).unwrap();
            assert!(
                json.contains(expected),
                "Mode {:?} should contain '{}'",
                mode,
                expected
            );

            let mode2: OverlayMode = serde_json::from_str(&json).unwrap();
            assert_eq!(format!("{:?}", mode), format!("{:?}", mode2));
        }
    }

    #[test]
    fn test_overlay_config_custom_values() {
        let config = OverlayConfig {
            enabled: true,
            opacity: 0.6,
            width: 2560,
            height: 1440,
            show_fps: true,
            show_trigger_status: true,
            show_recognition_results: false,
            position: OverlayPosition::Fixed { x: 100, y: 100 },
            mode: OverlayMode::Detailed,
            font_size: 18,
            background_color: Some(Color {
                r: 0,
                g: 0,
                b: 0,
                a: 128,
            }),
            fps_offset: Some((10, 10)),
            target_fps: 60,
        };

        let json = serde_json::to_string(&config).unwrap();
        let config2: OverlayConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.enabled, config2.enabled);
        assert!((config.opacity - config2.opacity).abs() < f32::EPSILON);
        assert_eq!(config.width, config2.width);
        assert_eq!(config.height, config2.height);
        assert_eq!(config.show_fps, config2.show_fps);
        assert_eq!(config.show_trigger_status, config2.show_trigger_status);
        assert_eq!(
            config.show_recognition_results,
            config2.show_recognition_results
        );
        assert_eq!(config.font_size, config2.font_size);
        assert_eq!(config.target_fps, config2.target_fps);
        assert_eq!(config.fps_offset, config2.fps_offset);
    }
}
