//! betternte-input/src/recorder.rs
//! Input recorder and macro

use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::action::{InputAction, InputEvent};
use crate::controller::InputController;

use anyhow::Result as AnyhowResult;

/// Input recorder.
///
/// Records user's keyboard and mouse operations for macro recording and playback.
#[derive(Debug)]
pub struct InputRecorder {
    /// Recorded events
    events: Vec<InputEvent>,

    /// Recording start time
    start_time: Option<Instant>,

    /// Whether currently recording
    recording: bool,
}

impl InputRecorder {
    /// Create a new recorder.
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            start_time: None,
            recording: false,
        }
    }

    /// Start recording.
    pub fn start(&mut self) {
        self.events.clear();
        self.start_time = Some(Instant::now());
        self.recording = true;
    }

    /// Stop recording and return the recorded macro.
    pub fn stop(&mut self) -> Macro {
        self.recording = false;
        let total_duration = self
            .start_time
            .map(|t| t.elapsed().as_millis() as u64)
            .unwrap_or(0);

        Macro {
            name: String::new(),
            events: self.events.clone(),
            total_duration_ms: total_duration,
            loop_count: 1,
        }
    }

    /// Record an input event.
    pub fn record(&mut self, action: InputAction) {
        if !self.recording {
            return;
        }
        let offset = self
            .start_time
            .map(|t| t.elapsed().as_millis() as u64)
            .unwrap_or(0);
        self.events.push(InputEvent {
            offset_ms: offset,
            action,
        });
    }

    /// Whether currently recording.
    pub fn is_recording(&self) -> bool {
        self.recording
    }
}

impl Default for InputRecorder {
    fn default() -> Self {
        Self::new()
    }
}

/// Recorded macro.
///
/// Contains a series of input events that can be played back.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Macro {
    /// Macro name
    pub name: String,

    /// Event list
    pub events: Vec<InputEvent>,

    /// Total duration (milliseconds)
    pub total_duration_ms: u64,

    /// Loop count (0 = infinite loop)
    pub loop_count: u32,
}

/// Macro player.
pub struct MacroPlayer {
    /// Input controller
    controller: Arc<dyn InputController>,
}

impl MacroPlayer {
    /// Create a new macro player.
    pub fn new(controller: Arc<dyn InputController>) -> Self {
        Self { controller }
    }

    /// Play back a macro.
    ///
    /// Executes events in order according to recorded timing intervals.
    pub async fn play(&self, mac: &Macro) -> AnyhowResult<()> {
        let loops = if mac.loop_count == 0 {
            u32::MAX
        } else {
            mac.loop_count
        };

        for _ in 0..loops {
            let mut last_offset = 0u64;
            for event in &mac.events {
                // Wait until event time
                let delay = event.offset_ms.saturating_sub(last_offset);
                if delay > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                }
                last_offset = event.offset_ms;

                // Execute event
                match &event.action {
                    InputAction::MouseMove { x, y } => {
                        self.controller.mouse_move(*x, *y).await?;
                    }
                    InputAction::MouseDown { button } => {
                        self.controller.mouse_down(*button).await?;
                    }
                    InputAction::MouseUp { button } => {
                        self.controller.mouse_up(*button).await?;
                    }
                    InputAction::KeyDown { key } => {
                        self.controller.key_press(*key).await?;
                    }
                    InputAction::KeyUp { key } => {
                        self.controller.key_release(*key).await?;
                    }
                    InputAction::Scroll { delta } => {
                        self.controller.mouse_scroll(*delta).await?;
                    }
                    InputAction::Sleep { ms } => {
                        tokio::time::sleep(std::time::Duration::from_millis(*ms)).await;
                    }
                }
            }
        }
        Ok(())
    }

    /// Stop playback (requires external cancellation flag).
    pub async fn stop(&self) -> AnyhowResult<()> {
        // Release any keys that might be held
        Ok(())
    }
}
