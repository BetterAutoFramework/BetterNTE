//! betternte-notify: Multi-channel notification push system.
//!
//! Provides concrete `Notifier` implementations (ServerChan, Telegram, Webhook, Bark)
//! and convenience functions to build a `NotificationManager` from configuration.
//!
//! The core trait (`Notifier`) and manager (`NotificationManager`) live in
//! `betternte_core::notify_trait` so that any crate can implement the plugin
//! contract without depending on HTTP libraries.

use async_trait::async_trait;
use std::collections::HashMap;

// Re-export the core types so downstream callers can use `betternte_notify::*`.
pub use betternte_core::notify_trait::{ChannelInfo, NotifyError, NotificationManager, Notifier};

use betternte_core::config::NotificationConfig;

// ============================================================================
// ServerChanNotifier
// ============================================================================

/// ServerChan (Server酱) notification sender.
///
/// Pushes notifications to WeChat via the ServerChan API.
/// API docs: <https://sct.ftqq.com/>
pub struct ServerChanNotifier {
    /// ServerChan SendKey
    send_key: String,
    /// HTTP client
    client: reqwest::Client,
    /// API base URL
    base_url: String,
}

impl ServerChanNotifier {
    /// Create a ServerChan notifier.
    ///
    /// # Arguments
    /// - `send_key`: ServerChan SendKey (from <https://sct.ftqq.com/>)
    pub fn new(send_key: String) -> Self {
        Self {
            send_key,
            client: reqwest::Client::new(),
            base_url: "https://sctapi.ftqq.com".to_string(),
        }
    }

    /// Create with a custom API base URL (for third-party compatible services).
    pub fn with_base_url(send_key: String, base_url: String) -> Self {
        Self {
            send_key,
            client: reqwest::Client::new(),
            base_url,
        }
    }
}

#[async_trait]
impl Notifier for ServerChanNotifier {
    fn name(&self) -> &str {
        "serverchan"
    }

    fn display_name(&self) -> &str {
        "Server酱"
    }

    async fn send(&self, title: &str, body: &str) -> Result<(), NotifyError> {
        if !self.is_configured() {
            return Err(NotifyError::NotConfigured("serverchan".to_string()));
        }

        let url = format!("{}/{}.send", self.base_url, self.send_key);

        let resp = self
            .client
            .post(&url)
            .form(&[("text", title), ("desp", body)])
            .send()
            .await
            .map_err(|e| NotifyError::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let message = resp.text().await.unwrap_or_default();
            return Err(NotifyError::ApiError { status, message });
        }

        tracing::info!("ServerChan notification sent: {}", title);
        Ok(())
    }

    fn is_configured(&self) -> bool {
        !self.send_key.is_empty()
    }

    async fn test(&self) -> Result<(), NotifyError> {
        self.send("BetterNTE Test", "ServerChan notification channel configured!")
            .await
    }
}

// ============================================================================
// TelegramNotifier
// ============================================================================

/// Telegram Bot notification sender.
///
/// Pushes notifications via the Telegram Bot API.
/// API docs: <https://core.telegram.org/bots/api>
pub struct TelegramNotifier {
    /// Bot Token (from @BotFather)
    bot_token: String,
    /// Target Chat ID (user/group/channel)
    chat_id: String,
    /// HTTP client
    client: reqwest::Client,
    /// API base URL (supports self-hosted Bot API servers)
    base_url: String,
    /// Parse mode (MarkdownV2 / HTML / plain text)
    parse_mode: String,
    /// Silent send (no notification sound)
    disable_notification: bool,
}

impl TelegramNotifier {
    /// Create a Telegram notifier.
    ///
    /// # Arguments
    /// - `bot_token`: Bot Token (format: "123456:ABC-DEF...")
    /// - `chat_id`: Target Chat ID (numeric ID or @channel_name)
    pub fn new(bot_token: String, chat_id: String) -> Self {
        Self {
            bot_token,
            chat_id,
            client: reqwest::Client::new(),
            base_url: "https://api.telegram.org".to_string(),
            parse_mode: String::new(),
            disable_notification: false,
        }
    }

    /// Set the parse mode.
    pub fn with_parse_mode(mut self, mode: &str) -> Self {
        self.parse_mode = mode.to_string();
        self
    }

    /// Set a custom API base URL (for self-hosted Bot API servers).
    pub fn with_base_url(mut self, url: &str) -> Self {
        self.base_url = url.to_string();
        self
    }

    /// Enable/disable silent send (no notification sound).
    pub fn with_silent(mut self, silent: bool) -> Self {
        self.disable_notification = silent;
        self
    }
}

#[async_trait]
impl Notifier for TelegramNotifier {
    fn name(&self) -> &str {
        "telegram"
    }

    fn display_name(&self) -> &str {
        "Telegram Bot"
    }

    async fn send(&self, title: &str, body: &str) -> Result<(), NotifyError> {
        if !self.is_configured() {
            return Err(NotifyError::NotConfigured("telegram".to_string()));
        }

        let url = format!("{}/bot{}/sendMessage", self.base_url, self.bot_token);

        let text = if title.is_empty() {
            body.to_string()
        } else {
            format!("**{}**\n\n{}", title, body)
        };

        let mut payload = serde_json::json!({
            "chat_id": self.chat_id,
            "text": text,
        });

        if !self.parse_mode.is_empty() {
            payload["parse_mode"] = serde_json::json!(self.parse_mode);
        }
        if self.disable_notification {
            payload["disable_notification"] = serde_json::json!(true);
        }

        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| NotifyError::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let message = resp.text().await.unwrap_or_default();
            return Err(NotifyError::ApiError { status, message });
        }

        tracing::info!("Telegram notification sent: {}", title);
        Ok(())
    }

    fn is_configured(&self) -> bool {
        !self.bot_token.is_empty() && !self.chat_id.is_empty()
    }

    async fn test(&self) -> Result<(), NotifyError> {
        self.send("BetterNTE Test", "Telegram notification channel configured!")
            .await
    }
}

// ============================================================================
// WebhookNotifier
// ============================================================================

/// Webhook platform type with platform-specific payload formatting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebhookPlatform {
    /// WeChat Work (企业微信)
    WeCom,
    /// DingTalk (钉钉)
    DingTalk,
    /// Feishu/Lark (飞书)
    Feishu,
    /// Discord
    Discord,
    /// Slack
    Slack,
    /// Custom/generic webhook
    Custom,
}

impl WebhookPlatform {
    /// Default Content-Type header for this platform.
    fn default_content_type(&self) -> &str {
        "application/json"
    }

    /// Format the notification payload for this platform.
    fn format_payload(&self, title: &str, body: &str) -> serde_json::Value {
        match self {
            WebhookPlatform::WeCom => serde_json::json!({
                "msgtype": "markdown",
                "markdown": {
                    "content": format!("**{}**\n\n{}", title, body)
                }
            }),
            WebhookPlatform::DingTalk => serde_json::json!({
                "msgtype": "markdown",
                "markdown": {
                    "title": title,
                    "text": format!("**{}**\n\n{}", title, body)
                }
            }),
            WebhookPlatform::Feishu => serde_json::json!({
                "msg_type": "text",
                "content": {
                    "text": format!("{}\n\n{}", title, body)
                }
            }),
            WebhookPlatform::Discord => serde_json::json!({
                "content": format!("**{}**\n{}", title, body)
            }),
            WebhookPlatform::Slack => serde_json::json!({
                "text": format!("*{}*\n{}", title, body)
            }),
            WebhookPlatform::Custom => {
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                serde_json::json!({
                    "title": title,
                    "body": body,
                    "timestamp": timestamp,
                })
            }
        }
    }
}

/// Webhook notification sender.
///
/// Sends notifications via HTTP POST to a custom URL.
/// Supports platform-specific payload formatting via `WebhookPlatform`.
pub struct WebhookNotifier {
    /// Webhook URL
    url: String,
    /// Custom request headers
    headers: HashMap<String, String>,
    /// HTTP client
    client: reqwest::Client,
    /// Target platform
    platform: WebhookPlatform,
}

impl WebhookNotifier {
    /// Create a webhook notifier for a specific platform.
    pub fn for_platform(url: String, platform: WebhookPlatform) -> Self {
        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            platform.default_content_type().to_string(),
        );
        Self {
            url,
            headers,
            client: reqwest::Client::new(),
            platform,
        }
    }

    /// Create a webhook notifier with custom headers (uses generic payload).
    pub fn new(url: String, headers: HashMap<String, String>) -> Self {
        Self {
            url,
            headers,
            client: reqwest::Client::new(),
            platform: WebhookPlatform::Custom,
        }
    }

    /// Create a WeChat Work (企业微信) webhook notifier.
    pub fn wecom(webhook_url: String) -> Self {
        Self::for_platform(webhook_url, WebhookPlatform::WeCom)
    }

    /// Create a DingTalk (钉钉) webhook notifier.
    pub fn dingtalk(webhook_url: String, _secret: Option<String>) -> Self {
        Self::for_platform(webhook_url, WebhookPlatform::DingTalk)
    }

    /// Create a Feishu/Lark (飞书) webhook notifier.
    pub fn feishu(webhook_url: String) -> Self {
        Self::for_platform(webhook_url, WebhookPlatform::Feishu)
    }

    /// Create a Discord webhook notifier.
    pub fn discord(webhook_url: String) -> Self {
        Self::for_platform(webhook_url, WebhookPlatform::Discord)
    }
}

#[async_trait]
impl Notifier for WebhookNotifier {
    fn name(&self) -> &str {
        "webhook"
    }

    fn display_name(&self) -> &str {
        "Custom Webhook"
    }

    async fn send(&self, title: &str, body: &str) -> Result<(), NotifyError> {
        if !self.is_configured() {
            return Err(NotifyError::NotConfigured("webhook".to_string()));
        }

        let payload = self.platform.format_payload(title, body);

        let mut req = self.client.post(&self.url);

        // Set default Content-Type if user did not specify one
        let has_content_type = self
            .headers
            .keys()
            .any(|k| k.eq_ignore_ascii_case("content-type"));

        if !has_content_type {
            req = req.header("Content-Type", "application/json");
        }

        for (key, value) in &self.headers {
            req = req.header(key.as_str(), value.as_str());
        }

        let resp = req
            .json(&payload)
            .send()
            .await
            .map_err(|e| NotifyError::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let message = resp.text().await.unwrap_or_default();
            return Err(NotifyError::ApiError { status, message });
        }

        tracing::info!("Webhook notification sent: {}", title);
        Ok(())
    }

    fn is_configured(&self) -> bool {
        !self.url.is_empty()
    }

    async fn test(&self) -> Result<(), NotifyError> {
        self.send("BetterNTE Test", "Webhook notification channel configured!")
            .await
    }
}

// ============================================================================
// BarkNotifier (iOS)
// ============================================================================

/// Bark (iOS) push notification sender.
///
/// Sends a JSON POST to **`{server_url}/{device_key}`** with `title` / `body`.
/// Server doc: <https://github.com/Finb/Bark>.
pub struct BarkNotifier {
    server_url: String,
    device_key: String,
    client: reqwest::Client,
}

impl BarkNotifier {
    /// Build a Bark notifier with a fully-qualified `server_url` (no trailing slash) and `device_key`.
    pub fn new(server_url: String, device_key: String) -> Self {
        Self {
            server_url: server_url.trim_end_matches('/').to_string(),
            device_key,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl Notifier for BarkNotifier {
    fn name(&self) -> &str {
        "bark"
    }

    fn display_name(&self) -> &str {
        "Bark (iOS)"
    }

    async fn send(&self, title: &str, body: &str) -> Result<(), NotifyError> {
        if !self.is_configured() {
            return Err(NotifyError::NotConfigured("bark".to_string()));
        }

        let url = format!("{}/{}", self.server_url, self.device_key);
        let payload = serde_json::json!({
            "title": title,
            "body": body,
        });

        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| NotifyError::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let message = resp.text().await.unwrap_or_default();
            return Err(NotifyError::ApiError { status, message });
        }

        tracing::info!("Bark notification sent: {}", title);
        Ok(())
    }

    fn is_configured(&self) -> bool {
        !self.server_url.is_empty() && !self.device_key.is_empty()
    }

    async fn test(&self) -> Result<(), NotifyError> {
        self.send("BetterNTE Test", "Bark notification channel configured!")
            .await
    }
}

// ============================================================================
// Plugin registration helpers
// ============================================================================

/// Register all built-in notifiers into a `NotificationManager` from config.
///
/// Channels are only registered when their per-channel `enabled` flag is set
/// **and** the required credentials are non-empty.  This keeps `send_all`
/// quiet when a channel is not yet configured.
pub fn register_built_in_notifiers(mgr: &mut NotificationManager, cfg: &NotificationConfig) {
    if cfg.telegram.enabled {
        if cfg.telegram.bot_token.is_empty() || cfg.telegram.chat_id.is_empty() {
            tracing::warn!("telegram enabled but bot_token/chat_id empty; skipping");
        } else {
            mgr.register(TelegramNotifier::new(
                cfg.telegram.bot_token.clone(),
                cfg.telegram.chat_id.clone(),
            ));
            tracing::info!("notification channel registered: telegram");
        }
    }

    if cfg.discord.enabled {
        if cfg.discord.webhook_url.is_empty() {
            tracing::warn!("discord enabled but webhook_url empty; skipping");
        } else {
            mgr.register(WebhookNotifier::discord(cfg.discord.webhook_url.clone()));
            tracing::info!("notification channel registered: discord");
        }
    }

    if cfg.serverchan.enabled {
        if cfg.serverchan.send_key.is_empty() {
            tracing::warn!("serverchan enabled but send_key empty; skipping");
        } else {
            mgr.register(ServerChanNotifier::new(cfg.serverchan.send_key.clone()));
            tracing::info!("notification channel registered: serverchan");
        }
    }

    if cfg.bark.enabled {
        if cfg.bark.server_url.is_empty() || cfg.bark.device_key.is_empty() {
            tracing::warn!("bark enabled but server_url/device_key empty; skipping");
        } else {
            mgr.register(BarkNotifier::new(
                cfg.bark.server_url.clone(),
                cfg.bark.device_key.clone(),
            ));
            tracing::info!("notification channel registered: bark");
        }
    }
}

/// Create a `NotificationManager` with all built-in notifiers from config.
///
/// Returns a disabled manager when `cfg.enabled` is false.
pub fn create_notification_manager(cfg: &NotificationConfig) -> NotificationManager {
    let mut mgr = NotificationManager::new();
    mgr.set_enabled(cfg.enabled);

    if !cfg.enabled {
        tracing::debug!("Notifications globally disabled; no channels registered");
        return mgr;
    }

    register_built_in_notifiers(&mut mgr, cfg);
    mgr
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    // ── MockNotifier ────────────────────────────────────────

    struct MockNotifier {
        name: String,
        configured: bool,
        calls: Arc<Mutex<Vec<(String, String)>>>,
    }

    impl MockNotifier {
        fn new(name: &str, configured: bool) -> Self {
            Self {
                name: name.to_string(),
                configured,
                calls: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn calls(&self) -> Arc<Mutex<Vec<(String, String)>>> {
            self.calls.clone()
        }
    }

    #[async_trait]
    impl Notifier for MockNotifier {
        fn name(&self) -> &str {
            &self.name
        }

        fn display_name(&self) -> &str {
            "Mock Notifier"
        }

        async fn send(&self, title: &str, body: &str) -> Result<(), NotifyError> {
            if !self.configured {
                return Err(NotifyError::NotConfigured(self.name.clone()));
            }
            self.calls
                .lock()
                .unwrap()
                .push((title.to_string(), body.to_string()));
            Ok(())
        }

        fn is_configured(&self) -> bool {
            self.configured
        }

        async fn test(&self) -> Result<(), NotifyError> {
            self.send("Test", "Test message").await
        }
    }

    // ── ServerChanNotifier tests ────────────────────────────

    #[test]
    fn test_serverchan_notifier_configured() {
        let notifier = ServerChanNotifier::new("SCTxxxxxxxxxxxxxxxxx".to_string());
        assert!(notifier.is_configured());
        assert_eq!(notifier.name(), "serverchan");
        assert_eq!(notifier.display_name(), "Server酱");
    }

    #[test]
    fn test_serverchan_notifier_empty_key() {
        let notifier = ServerChanNotifier::new(String::new());
        assert!(!notifier.is_configured());
    }

    // ── TelegramNotifier tests ──────────────────────────────

    #[test]
    fn test_telegram_notifier_configured() {
        let notifier = TelegramNotifier::new(
            "123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11".to_string(),
            "-1001234567890".to_string(),
        );
        assert!(notifier.is_configured());
        assert_eq!(notifier.name(), "telegram");
        assert_eq!(notifier.display_name(), "Telegram Bot");
    }

    #[test]
    fn test_telegram_notifier_empty_token_not_configured() {
        let notifier = TelegramNotifier::new(String::new(), "-1001234567890".to_string());
        assert!(!notifier.is_configured());
    }

    #[test]
    fn test_telegram_notifier_empty_chat_id_not_configured() {
        let notifier = TelegramNotifier::new("123456:ABC-DEF".to_string(), String::new());
        assert!(!notifier.is_configured());
    }

    #[test]
    fn test_telegram_with_silent() {
        let notifier =
            TelegramNotifier::new("token".to_string(), "chat_id".to_string()).with_silent(true);
        assert!(notifier.is_configured());
    }

    // ── WebhookNotifier tests ───────────────────────────────

    #[test]
    fn test_webhook_notifier_configured() {
        let notifier =
            WebhookNotifier::new("https://example.com/webhook".to_string(), HashMap::new());
        assert!(notifier.is_configured());
        assert_eq!(notifier.name(), "webhook");
        assert_eq!(notifier.display_name(), "Custom Webhook");
    }

    #[test]
    fn test_webhook_notifier_empty_url_not_configured() {
        let notifier = WebhookNotifier::new(String::new(), HashMap::new());
        assert!(!notifier.is_configured());
    }

    #[test]
    fn test_webhook_discord_factory() {
        let notifier =
            WebhookNotifier::discord("https://discord.com/api/webhooks/123/abc".to_string());
        assert!(notifier.is_configured());
    }

    // ── NotificationManager tests (via re-export) ──────────

    #[test]
    fn test_notification_manager_register_and_list() {
        let mut manager = NotificationManager::new();
        let mock1 = MockNotifier::new("channel_a", true);
        let mock2 = MockNotifier::new("channel_b", false);

        manager.register(mock1);
        manager.register(mock2);

        let channels = manager.list_channels();
        assert_eq!(channels.len(), 2);

        let ch_a = channels.iter().find(|c| c.name == "channel_a").unwrap();
        assert_eq!(ch_a.display_name, "Mock Notifier");
        assert!(ch_a.configured);

        let ch_b = channels.iter().find(|c| c.name == "channel_b").unwrap();
        assert!(!ch_b.configured);
    }

    #[test]
    fn test_notification_manager_unregister() {
        let mut manager = NotificationManager::new();
        manager.register(MockNotifier::new("temp_channel", true));

        let removed = manager.unregister("temp_channel");
        assert!(removed);
        assert!(manager.list_channels().is_empty());

        let not_found = manager.unregister("nonexistent");
        assert!(!not_found);
    }

    #[tokio::test]
    async fn test_notification_manager_send_to_unknown() {
        let mut manager = NotificationManager::new();
        manager.register(MockNotifier::new("existing", true));

        let result = manager.send_to("nonexistent", "Title", "Body").await;

        assert!(result.is_err());
        match result.unwrap_err() {
            NotifyError::ChannelNotFound(name) => {
                assert_eq!(name, "nonexistent");
            }
            e => panic!("Expected ChannelNotFound, got: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_notification_manager_send_all_with_mock() {
        let mut manager = NotificationManager::new();

        let mock1 = MockNotifier::new("mock1", true);
        let calls1 = mock1.calls();
        let mock2 = MockNotifier::new("mock2", true);
        let calls2 = mock2.calls();
        let mock3 = MockNotifier::new("mock3", false);

        manager.register(mock1);
        manager.register(mock2);
        manager.register(mock3);

        let results = manager.send_all("Title", "Body").await;

        assert_eq!(results.len(), 3);
        assert!(results[0].is_ok());
        assert!(results[1].is_ok());
        assert!(results[2].is_err());

        let c1 = calls1.lock().unwrap();
        assert_eq!(c1.len(), 1);
        assert_eq!(c1[0].0, "Title");
        assert_eq!(c1[0].1, "Body");

        let c2 = calls2.lock().unwrap();
        assert_eq!(c2.len(), 1);
    }

    #[tokio::test]
    async fn test_notification_manager_disabled_send_all_error() {
        let mut manager = NotificationManager::new();
        manager.register(MockNotifier::new("mock", true));
        manager.set_enabled(false);

        let results = manager.send_all("Title", "Body").await;

        assert_eq!(results.len(), 1);
        assert!(results[0].is_err());
        match results[0].as_ref().unwrap_err() {
            NotifyError::Disabled => {}
            e => panic!("Expected Disabled, got: {:?}", e),
        }
    }

    // ── Error display tests ─────────────────────────────────

    #[test]
    fn test_notify_error_display_disabled() {
        let err = NotifyError::Disabled;
        let msg = format!("{}", err);
        assert!(msg.contains("disabled"));
    }

    #[test]
    fn test_notify_error_display_channel_not_found() {
        let err = NotifyError::ChannelNotFound("telegram".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("not found"));
        assert!(msg.contains("telegram"));
    }

    #[test]
    fn test_notify_error_display_not_configured() {
        let err = NotifyError::NotConfigured("telegram".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("not configured"));
        assert!(msg.contains("telegram"));
    }

    #[test]
    fn test_notify_error_display_http_error() {
        let err = NotifyError::HttpError("connection refused".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("HTTP") || msg.contains("http"));
        assert!(msg.contains("connection refused"));
    }

    #[test]
    fn test_notify_error_display_api_error() {
        let err = NotifyError::ApiError {
            status: 403,
            message: "Forbidden".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("403"));
        assert!(msg.contains("Forbidden"));
    }

    #[test]
    fn test_notify_error_display_all_failed() {
        let err = NotifyError::AllFailed;
        let msg = format!("{}", err);
        assert!(msg.contains("failed"));
    }

    // ── register_built_in_notifiers tests ───────────────────

    use betternte_core::config::{
        BarkConfig, DiscordConfig, NotificationLevel, ServerChanConfig, TelegramConfig,
    };

    fn base_cfg(enabled: bool) -> NotificationConfig {
        NotificationConfig {
            enabled,
            level: NotificationLevel::Warning,
            telegram: TelegramConfig::default(),
            discord: DiscordConfig::default(),
            serverchan: ServerChanConfig::default(),
            bark: BarkConfig::default(),
        }
    }

    #[test]
    fn test_create_notification_manager_disabled() {
        let mgr = create_notification_manager(&base_cfg(false));
        assert!(!mgr.is_enabled());
        assert!(mgr.list_channels().is_empty());
    }

    #[test]
    fn test_channel_registered_only_when_fully_configured() {
        let mut cfg = base_cfg(true);
        cfg.telegram.enabled = true;
        // tokens empty on purpose
        let mgr = create_notification_manager(&cfg);
        assert!(
            mgr.list_channels().is_empty(),
            "empty tokens must NOT register telegram"
        );

        cfg.telegram.bot_token = "123:abc".into();
        cfg.telegram.chat_id = "-10012345".into();
        let mgr = create_notification_manager(&cfg);
        assert_eq!(mgr.list_channels().len(), 1);
        assert_eq!(mgr.list_channels()[0].name, "telegram");
    }

    #[test]
    fn test_multi_channel_registration_counts() {
        let mut cfg = base_cfg(true);
        cfg.discord.enabled = true;
        cfg.discord.webhook_url = "https://discord.com/api/webhooks/1/abc".into();
        cfg.serverchan.enabled = true;
        cfg.serverchan.send_key = "SCTxxx".into();
        cfg.bark.enabled = true;
        cfg.bark.server_url = "https://api.day.app".into();
        cfg.bark.device_key = "devkey".into();

        let mgr = create_notification_manager(&cfg);
        assert_eq!(mgr.list_channels().len(), 3);
    }
}
