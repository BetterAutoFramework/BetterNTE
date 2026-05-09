//! betternte-input tests
//!
//! Tests based on docs/tests/betternte-input-tests.md

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;

    use crate::action::{InputAction, InputEvent};
    use crate::adb::AdbInput;
    use crate::config::InputMode;
    use crate::controller::InputController;
    use crate::error::InputError;
    use crate::key::{Key, MouseButton};
    use crate::mapper::KeyMapper;
    use crate::queue::InputQueue;
    use crate::recorder::{InputRecorder, Macro, MacroPlayer};
    use crate::target::InputTarget;

    #[cfg(windows)]
    use crate::win32::Win32Input;

    /// Mock InputController for testing
    #[derive(Debug, Clone)]
    struct MockInputController {
        action_log: Arc<Mutex<Vec<String>>>,
        initialized: Arc<Mutex<bool>>,
    }

    impl MockInputController {
        fn new() -> Self {
            Self {
                action_log: Arc::new(Mutex::new(Vec::new())),
                initialized: Arc::new(Mutex::new(false)),
            }
        }

        fn take_action_log(&self) -> Vec<String> {
            self.action_log.lock().unwrap().drain(..).collect()
        }

        fn is_initialized(&self) -> bool {
            *self.initialized.lock().unwrap()
        }

        fn set_initialized(&self, val: bool) {
            *self.initialized.lock().unwrap() = val;
        }

        fn init_sync(&mut self, _target: &InputTarget) {
            self.set_initialized(true);
        }
    }

    #[async_trait]
    impl InputController for MockInputController {
        fn name(&self) -> &str {
            "Mock"
        }

        async fn init(&mut self, _target: &InputTarget) -> anyhow::Result<()> {
            self.set_initialized(true);
            Ok(())
        }

        async fn mouse_move(&self, x: i32, y: i32) -> anyhow::Result<()> {
            if !self.is_initialized() {
                return Err(InputError::NotInitialized.into());
            }
            self.action_log
                .lock()
                .unwrap()
                .push(format!("mouse_move({}, {})", x, y));
            Ok(())
        }

        async fn click(&self, x: i32, y: i32) -> anyhow::Result<()> {
            if !self.is_initialized() {
                return Err(InputError::NotInitialized.into());
            }
            self.action_log
                .lock()
                .unwrap()
                .push(format!("click({}, {})", x, y));
            Ok(())
        }

        async fn double_click(&self, x: i32, y: i32) -> anyhow::Result<()> {
            self.click(x, y).await?;
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            self.click(x, y).await
        }

        async fn right_click(&self, x: i32, y: i32) -> anyhow::Result<()> {
            self.action_log
                .lock()
                .unwrap()
                .push(format!("right_click({}, {})", x, y));
            Ok(())
        }

        async fn mouse_down(&self, button: MouseButton) -> anyhow::Result<()> {
            self.action_log
                .lock()
                .unwrap()
                .push(format!("mouse_down({:?})", button));
            Ok(())
        }

        async fn mouse_up(&self, button: MouseButton) -> anyhow::Result<()> {
            self.action_log
                .lock()
                .unwrap()
                .push(format!("mouse_up({:?})", button));
            Ok(())
        }

        async fn mouse_scroll(&self, delta: i32) -> anyhow::Result<()> {
            self.action_log
                .lock()
                .unwrap()
                .push(format!("scroll({})", delta));
            Ok(())
        }

        async fn swipe(
            &self,
            x1: i32,
            y1: i32,
            x2: i32,
            y2: i32,
            duration_ms: u32,
        ) -> anyhow::Result<()> {
            self.mouse_move(x1, y1).await?;
            self.mouse_down(MouseButton::Left).await?;
            let steps = (duration_ms / 16).max(10);
            let dx = (x2 - x1) as f64 / steps as f64;
            let dy = (y2 - y1) as f64 / steps as f64;
            for i in 1..=steps {
                let px = x1 + (dx * i as f64) as i32;
                let py = y1 + (dy * i as f64) as i32;
                self.mouse_move(px, py).await?;
                tokio::time::sleep(std::time::Duration::from_millis(16)).await;
            }
            self.mouse_up(MouseButton::Left).await
        }

        async fn key_press(&self, key: Key) -> anyhow::Result<()> {
            self.action_log
                .lock()
                .unwrap()
                .push(format!("key_press({:?})", key));
            Ok(())
        }

        async fn key_release(&self, key: Key) -> anyhow::Result<()> {
            self.action_log
                .lock()
                .unwrap()
                .push(format!("key_release({:?})", key));
            Ok(())
        }

        async fn key_tap(&self, key: Key, _duration_ms: Option<u32>) -> anyhow::Result<()> {
            self.key_press(key).await?;
            self.key_release(key).await
        }

        async fn type_text(&self, text: &str) -> anyhow::Result<()> {
            self.action_log
                .lock()
                .unwrap()
                .push(format!("type({})", text));
            Ok(())
        }

        async fn key_combo(&self, keys: &[Key]) -> anyhow::Result<()> {
            for &key in keys {
                self.key_press(key).await?;
            }
            for &key in keys.iter().rev() {
                self.key_release(key).await?;
            }
            Ok(())
        }

        fn supports_background(&self) -> bool {
            true
        }
        fn last_latency_ms(&self) -> Option<f64> {
            None
        }
        fn mode(&self) -> InputMode {
            InputMode::Foreground
        }
    }

    fn create_test_controller() -> MockInputController {
        MockInputController::new()
    }

    fn create_initialized_controller() -> MockInputController {
        let mut controller = MockInputController::new();
        controller.init_sync(&InputTarget::NativeWindow { hwnd: 0x12345 });
        controller
    }

    fn create_initialized_controller_arc() -> Arc<MockInputController> {
        Arc::new(create_initialized_controller())
    }

    // === 1. InputController tests ===

    #[tokio::test]
    async fn test_click_sends_at_correct_position() {
        let controller = create_initialized_controller();

        let result = controller.click(100, 200).await;

        assert!(result.is_ok(), "click should not error");
        let log = controller.take_action_log();
        assert!(
            log.contains(&"click(100, 200)".to_string()),
            "should log click(100, 200), actual: {:?}",
            log
        );
    }

    #[tokio::test]
    async fn test_double_click_fires_twice() {
        let controller = create_initialized_controller();

        let result = controller.double_click(300, 400).await;

        assert!(result.is_ok());
        let log = controller.take_action_log();
        let click_count = log
            .iter()
            .filter(|s| s.starts_with("click(300, 400)"))
            .count();
        assert_eq!(
            click_count, 2,
            "double_click should trigger 2 clicks, actual: {}",
            click_count
        );
    }

    #[tokio::test]
    async fn test_swipe_moves_from_a_to_b() {
        let controller = create_initialized_controller();

        let start = std::time::Instant::now();
        let result = controller.swipe(100, 100, 500, 500, 200).await;
        let elapsed = start.elapsed();

        assert!(result.is_ok(), "swipe should not error");
        let log = controller.take_action_log();
        assert!(
            log.iter().any(|s| s.contains("mouse_down")),
            "swipe should contain mouse_down"
        );
        assert!(
            log.iter().any(|s| s.contains("mouse_move")),
            "swipe should contain mouse_move"
        );
        assert!(
            log.iter().any(|s| s.contains("mouse_up")),
            "swipe should contain mouse_up"
        );
        assert!(
            elapsed >= std::time::Duration::from_millis(150),
            "swipe duration should be >= 150ms, actual: {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn test_key_press_release_sequence() {
        let controller = create_initialized_controller();

        controller.key_press(Key::A).await.unwrap();
        controller.key_release(Key::A).await.unwrap();

        let log = controller.take_action_log();
        let press_idx = log.iter().position(|s| s.contains("key_press(A)")).unwrap();
        let release_idx = log
            .iter()
            .position(|s| s.contains("key_release(A)"))
            .unwrap();
        assert!(
            press_idx < release_idx,
            "key_press should be before key_release"
        );
    }

    #[tokio::test]
    async fn test_type_text_sends_correct_characters() {
        let controller = create_initialized_controller();

        let result = controller.type_text("Hello").await;

        assert!(result.is_ok());
        let log = controller.take_action_log();
        assert!(
            log.iter()
                .any(|s| s.contains("Hello") || s.contains("type")),
            "should log text input operation"
        );
    }

    #[tokio::test]
    async fn test_mouse_move_updates_position() {
        let controller = create_initialized_controller();

        controller.mouse_move(640, 480).await.unwrap();

        let log = controller.take_action_log();
        assert!(log.contains(&"mouse_move(640, 480)".to_string()));
    }

    #[tokio::test]
    async fn test_mouse_scroll_sends_delta() {
        let controller = create_initialized_controller();

        controller.mouse_scroll(3).await.unwrap();
        controller.mouse_scroll(-5).await.unwrap();

        let log = controller.take_action_log();
        assert!(
            log.iter().any(|s| s.contains("scroll") && s.contains("3")),
            "should log scroll up 3"
        );
        assert!(
            log.iter().any(|s| s.contains("scroll") && s.contains("-5")),
            "should log scroll down 5"
        );
    }

    // === 2. Key enum tests ===

    #[test]
    fn test_key_parse_from_string() {
        // Letter keys
        assert_eq!(Key::try_parse("A"), Some(Key::A));
        assert_eq!(Key::try_parse("a"), Some(Key::A));
        assert_eq!(Key::try_parse("VK_A"), Some(Key::A));

        // Number keys
        assert_eq!(Key::try_parse("0"), Some(Key::Num0));
        assert_eq!(Key::try_parse("VK_0"), Some(Key::Num0));

        // Function keys
        assert_eq!(Key::try_parse("F1"), Some(Key::F1));
        assert_eq!(Key::try_parse("F12"), Some(Key::F12));

        // Space
        assert_eq!(Key::try_parse("Space"), Some(Key::Space));
        assert_eq!(Key::try_parse("SPACE"), Some(Key::Space));
        assert_eq!(Key::try_parse("VK_SPACE"), Some(Key::Space));
    }

    #[test]
    fn test_key_serialize_to_string() {
        let key = Key::A;
        let json = serde_json::to_string(&key).unwrap();
        assert_eq!(json, "\"a\"", "serde serialization should use snake_case");

        let key = Key::Control;
        let json = serde_json::to_string(&key).unwrap();
        assert_eq!(json, "\"control\"");
    }

    #[test]
    fn test_key_parse_special_keys() {
        // Modifiers
        assert_eq!(Key::try_parse("CTRL"), Some(Key::Control));
        assert_eq!(Key::try_parse("CONTROL"), Some(Key::Control));
        assert_eq!(Key::try_parse("VK_CONTROL"), Some(Key::Control));

        assert_eq!(Key::try_parse("ALT"), Some(Key::Alt));
        assert_eq!(Key::try_parse("VK_ALT"), Some(Key::Alt));

        assert_eq!(Key::try_parse("SHIFT"), Some(Key::Shift));
        assert_eq!(Key::try_parse("VK_SHIFT"), Some(Key::Shift));

        // Enter / Return
        assert_eq!(Key::try_parse("ENTER"), Some(Key::Return));
        assert_eq!(Key::try_parse("RETURN"), Some(Key::Return));
        assert_eq!(Key::try_parse("VK_RETURN"), Some(Key::Return));

        // Navigation keys
        assert_eq!(Key::try_parse("UP"), Some(Key::Up));
        assert_eq!(Key::try_parse("DOWN"), Some(Key::Down));
        assert_eq!(Key::try_parse("LEFT"), Some(Key::Left));
        assert_eq!(Key::try_parse("RIGHT"), Some(Key::Right));

        // Edit keys
        assert_eq!(Key::try_parse("ESCAPE"), Some(Key::Escape));
        assert_eq!(Key::try_parse("ESC"), Some(Key::Escape));
        assert_eq!(Key::try_parse("TAB"), Some(Key::Tab));
        assert_eq!(Key::try_parse("BACKSPACE"), Some(Key::Backspace));
        assert_eq!(Key::try_parse("DELETE"), Some(Key::Delete));
        assert_eq!(Key::try_parse("DEL"), Some(Key::Delete));
    }

    #[test]
    fn test_key_parse_invalid_returns_none() {
        assert_eq!(Key::try_parse("INVALID_KEY"), None);
        assert_eq!(Key::try_parse(""), None);
        assert_eq!(Key::try_parse("123abc"), None);
    }

    // === 3. InputQueue tests ===

    #[tokio::test]
    async fn test_input_queue_submit() {
        let _controller = create_initialized_controller_arc();
        let queue = InputQueue::new(0);

        let executed = Arc::new(AtomicBool::new(false));
        let flag = executed.clone();

        queue
            .submit(move || async move {
                flag.store(true, Ordering::SeqCst);
                Ok(())
            })
            .await
            .unwrap();

        assert!(
            executed.load(Ordering::SeqCst),
            "submitted operation should be executed"
        );
    }

    #[tokio::test]
    async fn test_input_queue_fifo_order() {
        let _controller = create_initialized_controller_arc();
        let queue = InputQueue::new(0);

        let order = Arc::new(Mutex::new(Vec::new()));

        for i in 0..5 {
            let order_clone = order.clone();
            queue
                .submit(move || async move {
                    order_clone.lock().unwrap().push(i);
                    Ok(())
                })
                .await
                .unwrap();
        }

        let final_order = order.lock().unwrap();
        assert_eq!(
            *final_order,
            vec![0, 1, 2, 3, 4],
            "operations should execute in FIFO order"
        );
    }

    #[tokio::test]
    async fn test_input_queue_rate_limiting() {
        let _controller = create_initialized_controller_arc();
        let queue = InputQueue::new(10); // max 10 per second

        let start = std::time::Instant::now();
        for _ in 0..3 {
            queue.submit(|| async { Ok(()) }).await.unwrap();
        }
        let elapsed = start.elapsed();

        // 3 operations at 10/sec = 100ms intervals = ~200ms minimum
        assert!(
            elapsed >= std::time::Duration::from_millis(150),
            "rate limited 3 operations should take >= 150ms, actual: {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn test_input_queue_set_rate_limit_takes_effect() {
        let _controller = create_initialized_controller_arc();
        let queue = InputQueue::new(0);

        // Warm up so the consumer is parked on recv.
        queue.submit(|| async { Ok(()) }).await.unwrap();

        queue.set_rate_limit(20);
        let start = std::time::Instant::now();
        for _ in 0..3 {
            queue.submit(|| async { Ok(()) }).await.unwrap();
        }
        let elapsed = start.elapsed();
        assert!(
            elapsed >= std::time::Duration::from_millis(80),
            "after set_rate_limit(20), 3 ops should take >= 80ms, actual: {:?}",
            elapsed
        );

        queue.set_rate_limit(0);
        let start = std::time::Instant::now();
        for _ in 0..10 {
            queue.submit(|| async { Ok(()) }).await.unwrap();
        }
        assert!(
            start.elapsed() < std::time::Duration::from_millis(100),
            "after set_rate_limit(0), throughput should be unrestricted",
        );
    }

    #[tokio::test]
    async fn test_input_queue_lifecycle() {
        let _controller = create_initialized_controller_arc();
        let queue = InputQueue::new(0);

        // Normal submit should succeed.
        queue.submit(|| async { Ok(()) }).await.unwrap();

        // Drop queue; the consumer task should terminate gracefully.
        drop(queue);

        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    #[tokio::test]
    async fn test_input_queue_propagates_job_error() {
        let _controller = create_initialized_controller_arc();
        let queue = InputQueue::new(0);

        let result: anyhow::Result<()> = queue
            .submit(|| async { Err(anyhow::anyhow!("boom")) })
            .await;

        assert!(result.is_err(), "queue should propagate job error");
        assert!(
            result.unwrap_err().to_string().contains("boom"),
            "error message should be preserved",
        );
    }

    // === 4. InputAction tests ===

    #[test]
    fn test_input_action_serialize_deserialize() {
        let actions = vec![
            InputAction::MouseMove { x: 100, y: 200 },
            InputAction::MouseDown {
                button: MouseButton::Left,
            },
            InputAction::MouseUp {
                button: MouseButton::Right,
            },
            InputAction::KeyDown { key: Key::A },
            InputAction::KeyUp { key: Key::Space },
            InputAction::Scroll { delta: 3 },
            InputAction::Sleep { ms: 500 },
        ];

        for action in &actions {
            let json = serde_json::to_string(action).unwrap();
            let deserialized: InputAction = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&deserialized).unwrap();
            assert_eq!(
                json, json2,
                "roundtrip serialization should match: {:?}",
                action
            );
        }
    }

    #[test]
    fn test_input_action_json_format() {
        let action = InputAction::KeyDown { key: Key::Enter };
        let json = serde_json::to_value(&action).unwrap();

        assert!(json.get("type").is_some(), "should have type field");
        assert_eq!(json["type"], "key_down");
        assert!(json.get("params").is_some(), "should have params field");
        assert_eq!(json["params"]["key"], "enter");
    }

    #[test]
    fn test_mouse_button_serialization() {
        let btn = MouseButton::Left;
        let json = serde_json::to_string(&btn).unwrap();
        assert_eq!(json, "\"left\"");

        let btn = MouseButton::Right;
        let json = serde_json::to_string(&btn).unwrap();
        assert_eq!(json, "\"right\"");

        let btn = MouseButton::X1;
        let json = serde_json::to_string(&btn).unwrap();
        assert_eq!(json, "\"x1\"");
    }

    // === 5. InputRecorder tests ===

    #[test]
    fn test_recorder_start_begins_capture() {
        let mut recorder = InputRecorder::new();

        assert!(
            !recorder.is_recording(),
            "initial state should not be recording"
        );
        recorder.start();
        assert!(recorder.is_recording(), "after start should be recording");
    }

    #[test]
    fn test_recorder_stop_returns_macro() {
        let mut recorder = InputRecorder::new();
        recorder.start();

        // Record some actions
        recorder.record(InputAction::MouseMove { x: 100, y: 200 });
        recorder.record(InputAction::MouseDown {
            button: MouseButton::Left,
        });
        recorder.record(InputAction::MouseUp {
            button: MouseButton::Left,
        });

        let mac = recorder.stop();

        assert!(
            !recorder.is_recording(),
            "after stop should not be recording"
        );
        assert_eq!(mac.events.len(), 3, "macro should have 3 events");
        // total_duration_ms is u64, always >= 0
        let _ = mac.total_duration_ms;
    }

    #[test]
    fn test_recorder_events_have_timestamps() {
        let mut recorder = InputRecorder::new();
        recorder.start();

        recorder.record(InputAction::KeyDown { key: Key::A });
        std::thread::sleep(std::time::Duration::from_millis(100));
        recorder.record(InputAction::KeyUp { key: Key::A });
        std::thread::sleep(std::time::Duration::from_millis(100));
        recorder.record(InputAction::KeyDown { key: Key::B });

        let mac = recorder.stop();

        // First event offset should be near 0
        assert!(
            mac.events[0].offset_ms < 50,
            "first event offset should be < 50ms, actual: {}",
            mac.events[0].offset_ms
        );

        // Subsequent event timestamps should be increasing
        assert!(
            mac.events[1].offset_ms > mac.events[0].offset_ms,
            "second event should be after first"
        );
        assert!(
            mac.events[2].offset_ms > mac.events[1].offset_ms,
            "third event should be after second"
        );

        // Second event offset should be ~100ms
        assert!(
            mac.events[1].offset_ms >= 80 && mac.events[1].offset_ms <= 150,
            "second event offset should be ~100ms, actual: {}",
            mac.events[1].offset_ms
        );
    }

    #[tokio::test]
    async fn test_recorder_play_replays_actions() {
        let controller = create_initialized_controller_arc();
        let player = MacroPlayer::new(controller.clone());

        let mac = Macro {
            name: "test_macro".to_string(),
            events: vec![
                InputEvent {
                    offset_ms: 0,
                    action: InputAction::MouseMove { x: 100, y: 200 },
                },
                InputEvent {
                    offset_ms: 50,
                    action: InputAction::MouseDown {
                        button: MouseButton::Left,
                    },
                },
                InputEvent {
                    offset_ms: 100,
                    action: InputAction::MouseUp {
                        button: MouseButton::Left,
                    },
                },
            ],
            total_duration_ms: 100,
            loop_count: 1,
        };

        let result = player.play(&mac).await;

        assert!(result.is_ok(), "macro playback should not error");
        let log = controller.take_action_log();
        assert!(
            log.iter().any(|s| s.contains("mouse_move(100, 200)")),
            "should replay mouse_move"
        );
        assert!(
            log.iter().any(|s| s.contains("mouse_down")),
            "should replay mouse_down"
        );
        assert!(
            log.iter().any(|s| s.contains("mouse_up")),
            "should replay mouse_up"
        );
    }

    #[test]
    fn test_recorder_record_without_start_is_noop() {
        let mut recorder = InputRecorder::new();

        // Record without calling start
        recorder.record(InputAction::MouseMove { x: 100, y: 200 });
        recorder.record(InputAction::KeyDown { key: Key::A });

        let mac = recorder.stop();

        assert!(
            mac.events.is_empty(),
            "record without start should be ignored"
        );
    }

    // === 6. Macro tests ===

    #[tokio::test]
    async fn test_macro_empty() {
        let controller = create_initialized_controller_arc();
        let player = MacroPlayer::new(controller);

        let mac = Macro {
            name: "empty".to_string(),
            events: vec![],
            total_duration_ms: 0,
            loop_count: 1,
        };

        let result = player.play(&mac).await;
        assert!(result.is_ok(), "empty macro playback should not error");
    }

    #[tokio::test]
    async fn test_macro_single_action() {
        let controller = create_initialized_controller_arc();
        let player = MacroPlayer::new(controller.clone());

        let mac = Macro {
            name: "single_click".to_string(),
            events: vec![InputEvent {
                offset_ms: 0,
                action: InputAction::MouseDown {
                    button: MouseButton::Left,
                },
            }],
            total_duration_ms: 0,
            loop_count: 1,
        };

        player.play(&mac).await.unwrap();

        let log = controller.take_action_log();
        assert_eq!(
            log.len(),
            1,
            "single action macro should execute 1 operation"
        );
    }

    #[tokio::test]
    async fn test_macro_multi_action_with_timing() {
        let controller = create_initialized_controller_arc();
        let player = MacroPlayer::new(controller.clone());

        let mac = Macro {
            name: "combo".to_string(),
            events: vec![
                InputEvent {
                    offset_ms: 0,
                    action: InputAction::KeyDown { key: Key::Control },
                },
                InputEvent {
                    offset_ms: 10,
                    action: InputAction::KeyDown { key: Key::C },
                },
                InputEvent {
                    offset_ms: 50,
                    action: InputAction::KeyUp { key: Key::C },
                },
                InputEvent {
                    offset_ms: 60,
                    action: InputAction::KeyUp { key: Key::Control },
                },
            ],
            total_duration_ms: 60,
            loop_count: 1,
        };

        let start = std::time::Instant::now();
        player.play(&mac).await.unwrap();
        let elapsed = start.elapsed();

        // Total duration should be >= 50ms (last event offset)
        assert!(
            elapsed >= std::time::Duration::from_millis(50),
            "multi-action macro execution time should be >= 50ms, actual: {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn test_macro_loop_execution() {
        let controller = create_initialized_controller_arc();
        let player = MacroPlayer::new(controller.clone());

        let mac = Macro {
            name: "loop_test".to_string(),
            events: vec![
                InputEvent {
                    offset_ms: 0,
                    action: InputAction::KeyDown { key: Key::A },
                },
                InputEvent {
                    offset_ms: 10,
                    action: InputAction::KeyUp { key: Key::A },
                },
            ],
            total_duration_ms: 10,
            loop_count: 3,
        };

        player.play(&mac).await.unwrap();

        let log = controller.take_action_log();
        let key_press_count = log.iter().filter(|s| s.contains("key_press(A)")).count();
        assert_eq!(
            key_press_count, 3,
            "loop 3 times should execute key_press(A) 3 times"
        );
    }

    // === 7. KeyMapper tests ===

    #[test]
    fn test_key_mapper_valid_key() {
        let bindings = HashMap::new();
        let mapper = KeyMapper::new(bindings);

        assert_eq!(mapper.map_key("A").unwrap(), Key::A);
        assert_eq!(mapper.map_key("SPACE").unwrap(), Key::Space);
        assert_eq!(mapper.map_key("ENTER").unwrap(), Key::Return);
        assert_eq!(mapper.map_key("CTRL").unwrap(), Key::Control);
        assert_eq!(mapper.map_key("VK_A").unwrap(), Key::A);
    }

    #[test]
    fn test_key_mapper_invalid_key_returns_error() {
        let bindings = HashMap::new();
        let mapper = KeyMapper::new(bindings);

        let result = mapper.map_key("NONEXISTENT_KEY");
        assert!(result.is_err(), "invalid key should return error");
        match result.unwrap_err() {
            InputError::InvalidKey(name) => {
                assert_eq!(name, "NONEXISTENT_KEY");
            }
            e => panic!("expected InvalidKey, got: {:?}", e),
        }
    }

    #[test]
    fn test_key_mapper_custom_binding() {
        let mut bindings = HashMap::new();
        bindings.insert("attack".to_string(), "VK_A".to_string());
        bindings.insert("jump".to_string(), "SPACE".to_string());
        bindings.insert("skill1".to_string(), "F1".to_string());
        let mapper = KeyMapper::new(bindings);

        assert_eq!(mapper.map_key("attack").unwrap(), Key::A);
        assert_eq!(mapper.map_key("jump").unwrap(), Key::Space);
        assert_eq!(mapper.map_key("skill1").unwrap(), Key::F1);

        // Custom key names should not be directly parseable
        assert!(
            Key::try_parse("attack").is_none(),
            "custom key names should not be directly parseable"
        );
    }

    #[test]
    fn test_key_mapper_update_bindings() {
        let mut mapper = KeyMapper::new(HashMap::new());

        assert!(
            mapper.map_key("custom").is_err(),
            "initial mapping should fail"
        );

        let mut new_bindings = HashMap::new();
        new_bindings.insert("custom".to_string(), "A".to_string());
        mapper.update_bindings(new_bindings);

        assert_eq!(
            mapper.map_key("custom").unwrap(),
            Key::A,
            "after update, custom should map to A"
        );
    }

    // === 8. Concurrency safety tests ===

    #[tokio::test]
    async fn test_concurrent_submit_no_deadlock() {
        let _controller = create_initialized_controller_arc();
        let queue = Arc::new(InputQueue::new(0));

        let mut handles = vec![];
        for _ in 0..20 {
            let q = queue.clone();
            handles.push(tokio::spawn(async move {
                q.submit(move || async { Ok(()) }).await
            }));
        }

        let results = futures::future::join_all(handles).await;
        for (i, result) in results.iter().enumerate() {
            assert!(result.is_ok(), "task {} should complete normally", i);
        }
    }

    #[tokio::test]
    async fn test_queue_drains_correctly() {
        let _controller = create_initialized_controller_arc();
        let queue = InputQueue::new(0);

        let counter = Arc::new(AtomicU32::new(0));

        for _ in 0..50 {
            let c = counter.clone();
            queue
                .submit(move || async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
                .await
                .unwrap();
        }

        assert_eq!(
            counter.load(Ordering::SeqCst),
            50,
            "all 50 operations should be executed"
        );
    }

    // === 9. Error handling tests ===

    #[test]
    fn test_error_invalid_key_name() {
        let mapper = KeyMapper::new(HashMap::new());

        let result = mapper.map_key("NOT_A_REAL_KEY_12345");

        assert!(result.is_err());
        match result.unwrap_err() {
            InputError::InvalidKey(name) => {
                assert_eq!(name, "NOT_A_REAL_KEY_12345");
            }
            e => panic!("expected InvalidKey, got: {:?}", e),
        }
    }

    #[test]
    fn test_error_recorder_not_started() {
        let mut recorder = InputRecorder::new();

        // Stop without calling start
        let mac = recorder.stop();

        // Should return empty macro, not panic
        assert!(
            mac.events.is_empty(),
            "macro from unstarted recorder should be empty"
        );
        assert_eq!(
            mac.total_duration_ms, 0,
            "unstarted macro duration should be 0"
        );
    }

    #[tokio::test]
    async fn test_error_controller_not_initialized() {
        let controller = create_test_controller(); // Not initialized

        let result = controller.click(100, 200).await;

        assert!(
            result.is_err(),
            "click on uninitialized controller should error"
        );
        let err = result.unwrap_err();
        if let Some(input_err) = err.downcast_ref::<InputError>() {
            match input_err {
                InputError::NotInitialized => {}
                InputError::SimulationFailed(_) => {}
                e => panic!("expected NotInitialized or SimulationFailed, got: {:?}", e),
            }
        } else {
            panic!("expected InputError, got: {:?}", err);
        }
    }

    #[tokio::test]
    #[cfg(windows)]
    async fn test_error_win32_with_adb_target() {
        let mut win32 = Win32Input::new(KeyMapper::new(HashMap::new()));
        let target = InputTarget::AdbDevice {
            serial: "emulator-5554".to_string(),
        };

        let result = win32.init(&target).await;

        assert!(result.is_err(), "Win32 engine should not accept ADB target");
    }

    #[tokio::test]
    async fn test_error_adb_with_win32_target() {
        let mut adb = AdbInput::new("emulator-5554".to_string(), KeyMapper::new(HashMap::new()));
        let target = InputTarget::NativeWindow { hwnd: 0x12345 };

        let result = adb.init(&target).await;

        assert!(result.is_err(), "ADB engine should not accept Win32 target");
    }
}
