//! EventBus — 基于 tokio::broadcast 的事件广播。
//!
//! 使用 `betternte_core::EngineEvent` 作为事件类型，
//! 支持多订阅者广播。

use betternte_core::EngineEvent;
use tokio::sync::broadcast;

/// 事件总线，支持多订阅者广播。
///
/// 客户端（Tauri/CLI）通过 `subscribe()` 获取事件流，
/// 引擎内部通过 `publish()` 发布事件。
#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<EngineEvent>,
    control_tx: broadcast::Sender<EngineEvent>,
}

impl EventBus {
    /// 创建新的 EventBus。
    ///
    /// `capacity` 是广播通道的缓冲大小。
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        let (control_tx, _) = broadcast::channel(capacity);
        Self { tx, control_tx }
    }

    /// 发布事件。无订阅者时返回 Err（不 panic）。
    pub fn publish(
        &self,
        event: EngineEvent,
    ) -> Result<(), broadcast::error::SendError<EngineEvent>> {
        if is_control_event(&event) {
            let _ = self.control_tx.send(event.clone());
        }
        self.tx.send(event).map(|_| ())
    }

    /// 订阅事件流。
    pub fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
        self.tx.subscribe()
    }

    /// 订阅控制事件流（TaskStarted/TaskStopped 等关键状态事件）。
    pub fn subscribe_control(&self) -> broadcast::Receiver<EngineEvent> {
        self.control_tx.subscribe()
    }

    /// 当前订阅者数量。
    pub fn receiver_count(&self) -> usize {
        self.tx.receiver_count()
    }

    /// 当前控制事件订阅者数量。
    pub fn control_receiver_count(&self) -> usize {
        self.control_tx.receiver_count()
    }
}

fn is_control_event(event: &EngineEvent) -> bool {
    matches!(
        event,
        EngineEvent::TaskStarted { .. }
            | EngineEvent::TaskStopped { .. }
            | EngineEvent::CaptureStatusChanged { .. }
            | EngineEvent::ConfigChanged { .. }
            | EngineEvent::Error { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_subscribe_receives_published() {
        let bus = EventBus::new(64);
        let mut rx = bus.subscribe();

        bus.publish(EngineEvent::TaskStarted {
            task_name: "test".into(),
            task_type: "solo".into(),
            timestamp: chrono::Utc::now(),
        })
        .unwrap();

        let event = rx.recv().await.unwrap();
        match event {
            EngineEvent::TaskStarted { task_name, .. } => {
                assert_eq!(task_name, "test");
            }
            _ => panic!("Expected TaskStarted"),
        }
    }

    #[tokio::test]
    async fn test_multiple_subscribers_all_receive() {
        let bus = EventBus::new(64);
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();
        let mut rx3 = bus.subscribe();

        bus.publish(EngineEvent::Error {
            module: "test".into(),
            message: "hello".into(),
            severity: betternte_core::event::ErrorSeverity::Warning,
            recoverable: true,
        })
        .unwrap();

        assert!(rx1.recv().await.is_ok());
        assert!(rx2.recv().await.is_ok());
        assert!(rx3.recv().await.is_ok());
        assert_eq!(bus.receiver_count(), 3);
    }

    #[tokio::test]
    async fn test_publish_no_subscribers_returns_err() {
        let bus = EventBus::new(64);
        let result = bus.publish(EngineEvent::Error {
            module: "test".into(),
            message: "test".into(),
            severity: betternte_core::event::ErrorSeverity::Warning,
            recoverable: true,
        });
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_receiver_count() {
        let bus = EventBus::new(64);
        assert_eq!(bus.receiver_count(), 0);
        assert_eq!(bus.control_receiver_count(), 0);

        let _rx1 = bus.subscribe();
        assert_eq!(bus.receiver_count(), 1);

        let _rx2 = bus.subscribe();
        assert_eq!(bus.receiver_count(), 2);

        drop(_rx1);
        assert_eq!(bus.receiver_count(), 1);
    }

    #[tokio::test]
    async fn test_control_subscription_receives_control_events() {
        let bus = EventBus::new(64);
        let mut control_rx = bus.subscribe_control();
        let mut data_rx = bus.subscribe();

        bus.publish(EngineEvent::TaskStarted {
            task_name: "test".into(),
            task_type: "solo".into(),
            timestamp: chrono::Utc::now(),
        })
        .unwrap();

        assert!(matches!(
            control_rx.recv().await.unwrap(),
            EngineEvent::TaskStarted { .. }
        ));
        assert!(matches!(
            data_rx.recv().await.unwrap(),
            EngineEvent::TaskStarted { .. }
        ));
    }

    #[tokio::test]
    async fn test_control_subscription_filters_non_control_events() {
        let bus = EventBus::new(64);
        let mut control_rx = bus.subscribe_control();
        let _data_rx = bus.subscribe();

        bus.publish(EngineEvent::ScriptLoaded {
            script_name: "hello".into(),
            version: "1.0.0".into(),
            path: "x".into(),
        })
        .unwrap();

        let res =
            tokio::time::timeout(std::time::Duration::from_millis(20), control_rx.recv()).await;
        assert!(
            res.is_err(),
            "control channel should not receive ScriptLoaded"
        );
    }
}
