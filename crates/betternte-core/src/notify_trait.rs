//! Notification plugin contract.
//!
//! This module defines the **trait interface** and **manager** for the notification
//! system.  Any crate that wants to send notifications only needs to depend on
//! `betternte-core` and implement the [`Notifier`] trait.
//!
//! Concrete HTTP-based notifiers (ServerChan, Telegram, etc.) live in
//! `betternte-notify`, which depends on `reqwest`.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ============================================================================
// Error type
// ============================================================================

/// Notification module error type.
#[derive(Error, Debug)]
pub enum NotifyError {
    /// Notifications are globally disabled
    #[error("Notifications are disabled")]
    Disabled,

    /// Notification channel not found
    #[error("Notification channel not found: {0}")]
    ChannelNotFound(String),

    /// Notification channel not configured
    #[error("Notification channel not configured: {0}")]
    NotConfigured(String),

    /// HTTP request failed
    #[error("HTTP request failed: {0}")]
    HttpError(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// JSON serialization/deserialization error
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// IO error
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// All channels failed
    #[error("All notification channels failed")]
    AllFailed,

    /// API response error
    #[error("API error (status {status}): {message}")]
    ApiError { status: u16, message: String },
}

// ============================================================================
// Channel info
// ============================================================================

/// Notification channel metadata, used to display registered channel status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChannelInfo {
    /// Channel name (unique identifier)
    pub name: String,
    /// Channel display name
    pub display_name: String,
    /// Whether the channel is configured
    pub configured: bool,
}

// ============================================================================
// Notifier trait (the plugin contract)
// ============================================================================

/// Notification sender trait.
///
/// All notification channels implement this trait to send messages through
/// a unified interface.  This is the **plugin contract** — any crate can
/// implement it without depending on `betternte-notify`.
#[async_trait]
pub trait Notifier: Send + Sync {
    /// Channel name (unique identifier).
    fn name(&self) -> &str;

    /// Channel display name (for UI).
    fn display_name(&self) -> &str;

    /// Send a notification message.
    ///
    /// # Arguments
    /// - `title`: notification title
    /// - `body`: notification body
    async fn send(&self, title: &str, body: &str) -> Result<(), NotifyError>;

    /// Check whether the channel is properly configured.
    fn is_configured(&self) -> bool;

    /// Test the channel connection by sending a test message.
    async fn test(&self) -> Result<(), NotifyError>;
}

// ============================================================================
// NotificationManager
// ============================================================================

/// Manages all registered notification channels.
///
/// Provides batch send, targeted send, and channel listing capabilities.
pub struct NotificationManager {
    notifiers: Vec<Box<dyn Notifier>>,
    enabled: bool,
}

impl NotificationManager {
    /// Create a new manager (enabled by default).
    pub fn new() -> Self {
        Self {
            notifiers: Vec::new(),
            enabled: true,
        }
    }

    /// Set whether notifications are globally enabled.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Whether notifications are globally enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Register a notification channel.
    pub fn register(&mut self, notifier: impl Notifier + 'static) {
        self.notifiers.push(Box::new(notifier));
    }

    /// Unregister a notification channel by name.
    ///
    /// Returns `true` if the channel was found and removed.
    pub fn unregister(&mut self, name: &str) -> bool {
        let len_before = self.notifiers.len();
        self.notifiers.retain(|n| n.name() != name);
        self.notifiers.len() < len_before
    }

    /// Send a notification to all registered channels.
    ///
    /// Returns per-channel results; a single channel failure does not affect others.
    pub async fn send_all(&self, title: &str, body: &str) -> Vec<Result<(), NotifyError>> {
        if !self.enabled {
            return vec![Err(NotifyError::Disabled)];
        }

        let mut results = Vec::with_capacity(self.notifiers.len());
        for notifier in &self.notifiers {
            let result = notifier.send(title, body).await;
            results.push(result);
        }
        results
    }

    /// Send a notification to a specific channel.
    pub async fn send_to(&self, name: &str, title: &str, body: &str) -> Result<(), NotifyError> {
        if !self.enabled {
            return Err(NotifyError::Disabled);
        }

        let notifier = self
            .notifiers
            .iter()
            .find(|n| n.name() == name)
            .ok_or_else(|| NotifyError::ChannelNotFound(name.to_string()))?;

        notifier.send(title, body).await
    }

    /// List all registered notification channels.
    pub fn list_channels(&self) -> Vec<ChannelInfo> {
        self.notifiers
            .iter()
            .map(|n| ChannelInfo {
                name: n.name().to_string(),
                display_name: n.display_name().to_string(),
                configured: n.is_configured(),
            })
            .collect()
    }

    /// Test a specific notification channel.
    pub async fn test_channel(&self, name: &str) -> Result<(), NotifyError> {
        let notifier = self
            .notifiers
            .iter()
            .find(|n| n.name() == name)
            .ok_or_else(|| NotifyError::ChannelNotFound(name.to_string()))?;

        notifier.test().await
    }
}

impl Default for NotificationManager {
    fn default() -> Self {
        Self::new()
    }
}
