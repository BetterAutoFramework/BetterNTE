//! Build a [`betternte_notify::NotificationManager`] from [`betternte_core::config::NotificationConfig`].
//!
//! Channels are **only** registered when both **`enabled == true`** and the channel
//! is fully configured, so `NotificationManager::send_all` reports *success with no results*
//! (instead of noisy `NotConfigured` errors) when users haven’t set up every channel.

use betternte_core::config::NotificationConfig;
use betternte_notify::{
    BarkNotifier, NotificationManager, ServerChanNotifier, TelegramNotifier, WebhookNotifier,
};
use tracing::{debug, info, warn};

/// Construct a `NotificationManager` reflecting **`cfg.enabled`** and the per-channel **`enabled`** flags.
pub fn build_notification_manager(cfg: &NotificationConfig) -> NotificationManager {
    let mut mgr = NotificationManager::new();
    mgr.set_enabled(cfg.enabled);

    if !cfg.enabled {
        debug!("Notifications globally disabled; no channels registered");
        return mgr;
    }

    if cfg.telegram.enabled {
        if cfg.telegram.bot_token.is_empty() || cfg.telegram.chat_id.is_empty() {
            warn!("telegram enabled but bot_token/chat_id empty; skipping");
        } else {
            mgr.register(TelegramNotifier::new(
                cfg.telegram.bot_token.clone(),
                cfg.telegram.chat_id.clone(),
            ));
            info!("notification channel registered: telegram");
        }
    }

    if cfg.discord.enabled {
        if cfg.discord.webhook_url.is_empty() {
            warn!("discord enabled but webhook_url empty; skipping");
        } else {
            mgr.register(WebhookNotifier::discord(cfg.discord.webhook_url.clone()));
            info!("notification channel registered: discord");
        }
    }

    if cfg.serverchan.enabled {
        if cfg.serverchan.send_key.is_empty() {
            warn!("serverchan enabled but send_key empty; skipping");
        } else {
            mgr.register(ServerChanNotifier::new(cfg.serverchan.send_key.clone()));
            info!("notification channel registered: serverchan");
        }
    }

    if cfg.bark.enabled {
        if cfg.bark.server_url.is_empty() || cfg.bark.device_key.is_empty() {
            warn!("bark enabled but server_url/device_key empty; skipping");
        } else {
            mgr.register(BarkNotifier::new(
                cfg.bark.server_url.clone(),
                cfg.bark.device_key.clone(),
            ));
            info!("notification channel registered: bark");
        }
    }

    mgr
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn disabled_produces_disabled_manager_with_no_channels() {
        let mgr = build_notification_manager(&base_cfg(false));
        assert!(!mgr.is_enabled());
        assert!(mgr.list_channels().is_empty());
    }

    #[test]
    fn channel_registered_only_when_fully_configured() {
        let mut cfg = base_cfg(true);
        cfg.telegram.enabled = true;
        // tokens empty on purpose
        let mgr = build_notification_manager(&cfg);
        assert!(
            mgr.list_channels().is_empty(),
            "empty tokens must NOT register telegram"
        );

        cfg.telegram.bot_token = "123:abc".into();
        cfg.telegram.chat_id = "-10012345".into();
        let mgr = build_notification_manager(&cfg);
        assert_eq!(mgr.list_channels().len(), 1);
        assert_eq!(mgr.list_channels()[0].name, "telegram");
    }

    #[test]
    fn multi_channel_registration_counts() {
        let mut cfg = base_cfg(true);
        cfg.discord.enabled = true;
        cfg.discord.webhook_url = "https://discord.com/api/webhooks/1/abc".into();
        cfg.serverchan.enabled = true;
        cfg.serverchan.send_key = "SCTxxx".into();
        cfg.bark.enabled = true;
        cfg.bark.server_url = "https://api.day.app".into();
        cfg.bark.device_key = "devkey".into();

        let mgr = build_notification_manager(&cfg);
        assert_eq!(mgr.list_channels().len(), 3);
    }
}
