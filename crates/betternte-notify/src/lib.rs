//! betternte-notify: 多渠道通知推送系统
//!
//! 支持 Server酱、Telegram、自定义 Webhook 等通知渠道，通过 trait 抽象统一接口。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

// ============================================================================
// 错误类型
// ============================================================================

/// 通知模块错误类型。
#[derive(Error, Debug)]
pub enum NotifyError {
    /// 通知已全局禁用
    #[error("Notifications are disabled")]
    Disabled,

    /// 通知渠道未找到
    #[error("Notification channel not found: {0}")]
    ChannelNotFound(String),

    /// 通知渠道未配置
    #[error("Notification channel not configured: {0}")]
    NotConfigured(String),

    /// HTTP 请求失败
    #[error("HTTP request failed: {0}")]
    HttpError(String),

    /// 配置错误
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// JSON 序列化/反序列化错误
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// IO 错误
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// 所有渠道发送失败
    #[error("All notification channels failed")]
    AllFailed,

    /// API 响应错误
    #[error("API error (status {status}): {message}")]
    ApiError { status: u16, message: String },
}

// ============================================================================
// 通知渠道信息
// ============================================================================

/// 通知渠道信息，用于展示已注册渠道的状态。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChannelInfo {
    /// 渠道名称（唯一标识）
    pub name: String,
    /// 渠道显示名称
    pub display_name: String,
    /// 是否已配置
    pub configured: bool,
}

// ============================================================================
// Notifier trait
// ============================================================================

/// 通知发送器 trait。
///
/// 所有通知渠道实现此 trait，通过统一接口发送通知消息。
#[async_trait]
pub trait Notifier: Send + Sync {
    /// 通知渠道名称（唯一标识）。
    fn name(&self) -> &str;

    /// 通知渠道显示名称（用于前端展示）。
    fn display_name(&self) -> &str;

    /// 发送通知消息。
    ///
    /// # 参数
    /// - `title`: 通知标题
    /// - `body`: 通知正文内容
    async fn send(&self, title: &str, body: &str) -> Result<(), NotifyError>;

    /// 检查通知渠道是否已正确配置。
    fn is_configured(&self) -> bool;

    /// 测试通知渠道连接。
    ///
    /// 发送一条测试消息验证渠道是否可用。
    async fn test(&self) -> Result<(), NotifyError>;
}

// ============================================================================
// ServerChanNotifier
// ============================================================================

/// Server酱（ServerChan）通知发送器。
///
/// 通过 Server酱 API 推送通知到微信。
/// API 文档: <https://sct.ftqq.com/>
pub struct ServerChanNotifier {
    /// Server酱 SendKey
    send_key: String,
    /// HTTP 客户端
    client: reqwest::Client,
    /// API 基础 URL
    base_url: String,
}

impl ServerChanNotifier {
    /// 创建 Server酱通知发送器。
    ///
    /// # 参数
    /// - `send_key`: Server酱 SendKey（从 <https://sct.ftqq.com/> 获取）
    pub fn new(send_key: String) -> Self {
        Self {
            send_key,
            client: reqwest::Client::new(),
            base_url: "https://sctapi.ftqq.com".to_string(),
        }
    }

    /// 创建带自定义 API URL 的 Server酱通知发送器。
    ///
    /// # 参数
    /// - `send_key`: Server酱 SendKey
    /// - `base_url`: 自定义 API 基础 URL（如使用第三方兼容服务）
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

        tracing::info!("Server酱通知发送成功: {}", title);
        Ok(())
    }

    fn is_configured(&self) -> bool {
        !self.send_key.is_empty()
    }

    async fn test(&self) -> Result<(), NotifyError> {
        self.send("BetterNTE 测试通知", "Server酱通知渠道配置成功！")
            .await
    }
}

// ============================================================================
// TelegramNotifier
// ============================================================================

/// Telegram Bot 通知发送器。
///
/// 通过 Telegram Bot API 推送通知。
/// API 文档: <https://core.telegram.org/bots/api>
pub struct TelegramNotifier {
    /// Bot Token（从 @BotFather 获取）
    bot_token: String,
    /// 目标 Chat ID（用户/群组/频道）
    chat_id: String,
    /// HTTP 客户端
    client: reqwest::Client,
    /// API 基础 URL（支持自建 Bot API 服务器）
    base_url: String,
    /// 解析模式（MarkdownV2 / HTML / 纯文本）
    parse_mode: String,
    /// 静默发送（不播放通知音）
    disable_notification: bool,
}

impl TelegramNotifier {
    /// 创建 Telegram 通知发送器。
    ///
    /// # 参数
    /// - `bot_token`: Bot Token（格式: "123456:ABC-DEF..."）
    /// - `chat_id`: 目标 Chat ID（可以是数字 ID 或 @channel_name）
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

    /// 设置解析模式。
    ///
    /// # 参数
    /// - `mode`: 解析模式，可选 "MarkdownV2", "HTML", 或空字符串（纯文本）
    pub fn with_parse_mode(mut self, mode: &str) -> Self {
        self.parse_mode = mode.to_string();
        self
    }

    /// 设置自定义 API 基础 URL。
    ///
    /// 用于自建 Bot API 服务器。
    pub fn with_base_url(mut self, url: &str) -> Self {
        self.base_url = url.to_string();
        self
    }

    /// 启用/禁用静默发送（不播放通知音）。
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

        tracing::info!("Telegram 通知发送成功: {}", title);
        Ok(())
    }

    fn is_configured(&self) -> bool {
        !self.bot_token.is_empty() && !self.chat_id.is_empty()
    }

    async fn test(&self) -> Result<(), NotifyError> {
        self.send("BetterNTE 测试", "Telegram 通知渠道配置成功！")
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
    ///
    /// # Arguments
    /// - `url`: Webhook URL
    /// - `platform`: Target platform (determines payload format)
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
    ///
    /// # Arguments
    /// - `url`: Webhook URL
    /// - `headers`: Custom request headers (e.g., Authorization)
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
        "自定义 Webhook"
    }

    async fn send(&self, title: &str, body: &str) -> Result<(), NotifyError> {
        if !self.is_configured() {
            return Err(NotifyError::NotConfigured("webhook".to_string()));
        }

        let payload = self.platform.format_payload(title, body);

        let mut req = self.client.post(&self.url);

        // 设置默认 Content-Type（如果用户没有指定）
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

        tracing::info!("Webhook 通知发送成功: {}", title);
        Ok(())
    }

    fn is_configured(&self) -> bool {
        !self.url.is_empty()
    }

    async fn test(&self) -> Result<(), NotifyError> {
        self.send("BetterNTE 测试", "Webhook 通知渠道配置成功！")
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

        tracing::info!("Bark 通知发送成功: {}", title);
        Ok(())
    }

    fn is_configured(&self) -> bool {
        !self.server_url.is_empty() && !self.device_key.is_empty()
    }

    async fn test(&self) -> Result<(), NotifyError> {
        self.send("BetterNTE 测试", "Bark 通知渠道配置成功！").await
    }
}

// ============================================================================
// NotificationManager
// ============================================================================

/// 通知管理器。
///
/// 统一管理所有通知渠道，提供批量发送和定向发送能力。
pub struct NotificationManager {
    /// 已注册的通知渠道列表
    notifiers: Vec<Box<dyn Notifier>>,
    /// 是否全局启用通知
    enabled: bool,
}

impl NotificationManager {
    /// 创建通知管理器（默认启用）。
    pub fn new() -> Self {
        Self {
            notifiers: Vec::new(),
            enabled: true,
        }
    }

    /// 设置是否启用通知。
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// 全局通知是否启用。
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// 注册通知渠道。
    pub fn register(&mut self, notifier: impl Notifier + 'static) {
        self.notifiers.push(Box::new(notifier));
    }

    /// 注销通知渠道。
    ///
    /// # 返回
    /// - `true`: 注销成功
    /// - `false`: 未找到该渠道
    pub fn unregister(&mut self, name: &str) -> bool {
        let len_before = self.notifiers.len();
        self.notifiers.retain(|n| n.name() != name);
        self.notifiers.len() < len_before
    }

    /// 发送通知到所有已注册的渠道。
    ///
    /// 返回每个渠道的发送结果，单个渠道失败不影响其他渠道。
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

    /// 发送通知到指定渠道。
    ///
    /// # 参数
    /// - `name`: 目标通知渠道名称
    /// - `title`: 通知标题
    /// - `body`: 通知正文
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

    /// 获取所有已注册的通知渠道信息列表。
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

    /// 测试指定渠道的通知。
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

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    // ── MockNotifier（记录调用） ───────────────────────────

    /// Mock 通知发送器，用于单元测试。
    ///
    /// 记录所有 send 调用的 (title, body) 对。
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
            self.send("测试", "测试消息").await
        }
    }

    // ── ServerChanNotifier 测试 ────────────────────────────

    #[test]
    fn test_serverchan_notifier_configured() {
        let notifier = ServerChanNotifier::new("SCTxxxxxxxxxxxxxxxxx".to_string());
        assert!(notifier.is_configured(), "有 send_key 时应已配置");
        assert_eq!(notifier.name(), "serverchan");
        assert_eq!(notifier.display_name(), "Server酱");
    }

    #[test]
    fn test_serverchan_notifier_empty_key() {
        let notifier = ServerChanNotifier::new(String::new());
        assert!(!notifier.is_configured(), "空 send_key 应未配置");
    }

    // ── TelegramNotifier 测试 ──────────────────────────────

    #[test]
    fn test_telegram_notifier_configured() {
        let notifier = TelegramNotifier::new(
            "123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11".to_string(),
            "-1001234567890".to_string(),
        );
        assert!(notifier.is_configured(), "有 token 和 chat_id 时应已配置");
        assert_eq!(notifier.name(), "telegram");
        assert_eq!(notifier.display_name(), "Telegram Bot");
    }

    #[test]
    fn test_telegram_notifier_empty_token_not_configured() {
        let notifier = TelegramNotifier::new(String::new(), "-1001234567890".to_string());
        assert!(!notifier.is_configured(), "空 token 应未配置");
    }

    #[test]
    fn test_telegram_notifier_empty_chat_id_not_configured() {
        let notifier = TelegramNotifier::new("123456:ABC-DEF".to_string(), String::new());
        assert!(!notifier.is_configured(), "空 chat_id 应未配置");
    }

    #[test]
    fn test_telegram_with_silent() {
        let notifier =
            TelegramNotifier::new("token".to_string(), "chat_id".to_string()).with_silent(true);
        assert!(notifier.is_configured());
    }

    // ── WebhookNotifier 测试 ───────────────────────────────

    #[test]
    fn test_webhook_notifier_configured() {
        let notifier =
            WebhookNotifier::new("https://example.com/webhook".to_string(), HashMap::new());
        assert!(notifier.is_configured(), "有 url 时应已配置");
        assert_eq!(notifier.name(), "webhook");
        assert_eq!(notifier.display_name(), "自定义 Webhook");
    }

    #[test]
    fn test_webhook_notifier_empty_url_not_configured() {
        let notifier = WebhookNotifier::new(String::new(), HashMap::new());
        assert!(!notifier.is_configured(), "空 url 应未配置");
    }

    #[test]
    fn test_webhook_wecom_factory() {
        let notifier = WebhookNotifier::wecom(
            "https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=test".to_string(),
        );
        assert!(notifier.is_configured());
        assert_eq!(notifier.name(), "webhook");
    }

    #[test]
    fn test_webhook_discord_factory() {
        let notifier =
            WebhookNotifier::discord("https://discord.com/api/webhooks/123/abc".to_string());
        assert!(notifier.is_configured());
    }

    // ── MockNotifier 基本测试 ──────────────────────────────

    #[tokio::test]
    async fn test_notifier_mock_implements_trait() {
        let notifier = MockNotifier::new("mock", true);
        assert_eq!(notifier.name(), "mock");
        assert!(notifier.is_configured(), "已配置时应返回 true");
    }

    #[test]
    fn test_notifier_name_returns_correct_string() {
        let notifier = MockNotifier::new("my_channel", true);
        assert_eq!(notifier.name(), "my_channel");
    }

    #[test]
    fn test_notifier_is_configured_true() {
        let notifier = MockNotifier::new("mock", true);
        assert!(notifier.is_configured());
    }

    #[test]
    fn test_notifier_is_configured_false() {
        let notifier = MockNotifier::new("mock", false);
        assert!(!notifier.is_configured());
    }

    #[tokio::test]
    async fn test_notifier_mock_send_succeeds() {
        let notifier = MockNotifier::new("mock", true);
        let result = notifier.send("测试标题", "测试内容").await;
        assert!(result.is_ok(), "Mock send 应成功");
    }

    // ── NotificationManager 测试 ───────────────────────────

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
        assert!(removed, "注销成功应返回 true");
        assert!(manager.list_channels().is_empty());

        let not_found = manager.unregister("nonexistent");
        assert!(!not_found, "注销不存在的渠道应返回 false");
    }

    #[tokio::test]
    async fn test_notification_manager_send_to_unknown() {
        let mut manager = NotificationManager::new();
        manager.register(MockNotifier::new("existing", true));

        let result = manager.send_to("nonexistent", "标题", "内容").await;

        assert!(result.is_err(), "不存在的渠道应返回错误");
        match result.unwrap_err() {
            NotifyError::ChannelNotFound(name) => {
                assert_eq!(name, "nonexistent");
            }
            e => panic!("期望 ChannelNotFound，实际: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_notification_manager_send_all_with_mock() {
        let mut manager = NotificationManager::new();

        let mock1 = MockNotifier::new("mock1", true);
        let calls1 = mock1.calls();
        let mock2 = MockNotifier::new("mock2", true);
        let calls2 = mock2.calls();
        let mock3 = MockNotifier::new("mock3", false); // 未配置

        manager.register(mock1);
        manager.register(mock2);
        manager.register(mock3);

        let results = manager.send_all("测试标题", "测试内容").await;

        // 3 个渠道，3 个结果
        assert_eq!(results.len(), 3);
        assert!(results[0].is_ok(), "mock1 应发送成功");
        assert!(results[1].is_ok(), "mock2 应发送成功");
        assert!(results[2].is_err(), "mock3（未配置）应失败");

        // 验证调用记录
        let c1 = calls1.lock().unwrap();
        assert_eq!(c1.len(), 1);
        assert_eq!(c1[0].0, "测试标题");
        assert_eq!(c1[0].1, "测试内容");

        let c2 = calls2.lock().unwrap();
        assert_eq!(c2.len(), 1);
        assert_eq!(c2[0].0, "测试标题");
    }

    #[tokio::test]
    async fn test_notification_manager_send_to_targeted() {
        let mut manager = NotificationManager::new();

        let mock_target = MockNotifier::new("target_channel", true);
        let target_calls = mock_target.calls();
        let mock_other = MockNotifier::new("other_channel", true);
        let other_calls = mock_other.calls();

        manager.register(mock_target);
        manager.register(mock_other);

        let result = manager.send_to("target_channel", "标题", "内容").await;
        assert!(result.is_ok(), "定向发送应成功");

        // 只有 target_channel 收到调用
        assert_eq!(target_calls.lock().unwrap().len(), 1);
        assert_eq!(other_calls.lock().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_notification_manager_disabled_send_all_error() {
        let mut manager = NotificationManager::new();
        manager.register(MockNotifier::new("mock", true));
        manager.set_enabled(false);

        let results = manager.send_all("标题", "内容").await;

        assert_eq!(results.len(), 1);
        assert!(results[0].is_err());
        match results[0].as_ref().unwrap_err() {
            NotifyError::Disabled => {} // 正确
            e => panic!("期望 Disabled，实际: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_notification_manager_disabled_send_to_error() {
        let mut manager = NotificationManager::new();
        manager.register(MockNotifier::new("mock", true));
        manager.set_enabled(false);

        let result = manager.send_to("mock", "标题", "内容").await;

        assert!(result.is_err());
        match result.unwrap_err() {
            NotifyError::Disabled => {} // 正确
            e => panic!("期望 Disabled，实际: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_notification_manager_test_channel() {
        let mut manager = NotificationManager::new();
        let mock = MockNotifier::new("testable", true);
        let calls = mock.calls();
        manager.register(mock);

        let result = manager.test_channel("testable").await;
        assert!(result.is_ok(), "测试已配置渠道应成功");

        let c = calls.lock().unwrap();
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].0, "测试");
    }

    #[tokio::test]
    async fn test_notification_manager_test_channel_not_found() {
        let manager = NotificationManager::new();
        let result = manager.test_channel("nonexistent").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            NotifyError::ChannelNotFound(_) => {}
            e => panic!("期望 ChannelNotFound，实际: {:?}", e),
        }
    }

    // ── NotifyError Display 测试 ───────────────────────────

    #[test]
    fn test_notify_error_display_disabled() {
        let err = NotifyError::Disabled;
        let msg = format!("{}", err);
        assert!(msg.contains("disabled"), "应包含 'disabled'，实际: {}", msg);
    }

    #[test]
    fn test_notify_error_display_channel_not_found() {
        let err = NotifyError::ChannelNotFound("my_channel".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("not found"), "应包含 'not found'");
        assert!(msg.contains("my_channel"), "应包含渠道名");
    }

    #[test]
    fn test_notify_error_display_not_configured() {
        let err = NotifyError::NotConfigured("telegram".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("not configured"), "应包含 'not configured'");
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
        assert!(msg.contains("403"), "应包含状态码");
        assert!(msg.contains("Forbidden"), "应包含错误消息");
    }

    #[test]
    fn test_notify_error_display_all_failed() {
        let err = NotifyError::AllFailed;
        let msg = format!("{}", err);
        assert!(
            msg.contains("All") || msg.contains("all"),
            "应包含 'all/All'"
        );
        assert!(msg.contains("failed"), "应包含 'failed'");
    }
}
