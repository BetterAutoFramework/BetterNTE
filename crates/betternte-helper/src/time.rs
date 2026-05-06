//! Time utilities

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// SpeedTimer - Performance timing utility
pub struct SpeedTimer {
    #[allow(dead_code)]
    name: String,
    stopwatch: Instant,
    records: HashMap<String, Duration>,
}

impl SpeedTimer {
    /// Create a new SpeedTimer
    pub fn new() -> Self {
        Self {
            name: String::new(),
            stopwatch: Instant::now(),
            records: HashMap::new(),
        }
    }

    /// Create a named SpeedTimer
    pub fn with_name(name: &str) -> Self {
        Self {
            name: name.to_string(),
            stopwatch: Instant::now(),
            records: HashMap::new(),
        }
    }

    /// Record elapsed time for a named checkpoint
    pub fn record(&mut self, name: &str) {
        let elapsed = self.stopwatch.elapsed();
        self.records.insert(name.to_string(), elapsed);
        self.stopwatch = Instant::now();
    }

    /// Get elapsed time since last record
    pub fn elapsed(&self) -> Duration {
        self.stopwatch.elapsed()
    }

    /// Get recorded time for a checkpoint
    pub fn get(&self, name: &str) -> Option<Duration> {
        self.records.get(name).copied()
    }

    /// Format debug output
    pub fn debug_string(&self) -> String {
        let parts: Vec<String> = self
            .records
            .iter()
            .map(|(k, v)| format!("{}:{:?}", k, v))
            .collect();

        if !parts.is_empty() {
            parts.join(",")
        } else {
            String::new()
        }
    }
}

impl Default for SpeedTimer {
    fn default() -> Self {
        Self::new()
    }
}

/// Server time provider trait
pub trait ServerTimeProvider: Send + Sync {
    fn get_server_time(&self) -> chrono::DateTime<chrono::FixedOffset>;
    fn get_server_offset(&self) -> chrono::FixedOffset;
}

/// Default server time provider (UTC+8 - Beijing time)
pub struct DefaultServerTimeProvider;

impl DefaultServerTimeProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DefaultServerTimeProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerTimeProvider for DefaultServerTimeProvider {
    fn get_server_time(&self) -> chrono::DateTime<chrono::FixedOffset> {
        let offset = self.get_server_offset();
        chrono::Local::now().with_timezone(&offset)
    }

    fn get_server_offset(&self) -> chrono::FixedOffset {
        // UTC+8 (Beijing, Hong Kong, etc.)
        chrono::FixedOffset::east_opt(8 * 3600).expect("Invalid offset")
    }
}

/// Thread-safe server time accessor
pub struct ServerTime {
    provider: Box<dyn ServerTimeProvider>,
}

impl ServerTime {
    pub fn new(provider: Box<dyn ServerTimeProvider>) -> Self {
        Self { provider }
    }

    pub fn now(&self) -> chrono::DateTime<chrono::FixedOffset> {
        self.provider.get_server_time()
    }

    pub fn offset(&self) -> chrono::FixedOffset {
        self.provider.get_server_offset()
    }
}

impl Default for ServerTime {
    fn default() -> Self {
        Self::new(Box::new(DefaultServerTimeProvider::new()))
    }
}

/// Parse timestamp to chrono DateTime
pub fn parse_timestamp(ts: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp(ts, 0)
        .unwrap_or_else(|| chrono::DateTime::from_timestamp(0, 0).unwrap())
}

/// Format duration to human readable string
pub fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_speed_timer() {
        let mut timer = SpeedTimer::with_name("test");
        std::thread::sleep(Duration::from_millis(10));
        timer.record("first");
        std::thread::sleep(Duration::from_millis(10));
        timer.record("second");

        assert!(timer.get("first").is_some());
        assert!(timer.get("second").is_some());
    }

    #[test]
    fn test_speed_timer_new() {
        let timer = SpeedTimer::new();
        assert!(timer.elapsed().as_nanos() < 1_000_000); // < 1ms
    }

    #[test]
    fn test_speed_timer_default() {
        let timer = SpeedTimer::default();
        assert!(timer.elapsed().as_nanos() < 1_000_000);
    }

    #[test]
    fn test_speed_timer_get_missing() {
        let timer = SpeedTimer::new();
        assert!(timer.get("nonexistent").is_none());
    }

    #[test]
    fn test_speed_timer_debug_string_empty() {
        let timer = SpeedTimer::new();
        assert_eq!(timer.debug_string(), "");
    }

    #[test]
    fn test_speed_timer_debug_string_with_records() {
        let mut timer = SpeedTimer::new();
        timer.record("step1");
        let s = timer.debug_string();
        assert!(s.contains("step1"));
    }

    #[test]
    fn test_parse_timestamp_epoch() {
        let dt = parse_timestamp(0);
        assert_eq!(dt.to_rfc3339(), "1970-01-01T00:00:00+00:00");
    }

    #[test]
    fn test_parse_timestamp_known() {
        let dt = parse_timestamp(1_000_000_000);
        assert_eq!(dt.to_rfc3339(), "2001-09-09T01:46:40+00:00");
    }

    #[test]
    fn test_format_duration_seconds() {
        assert_eq!(format_duration(Duration::from_secs(30)), "30s");
        assert_eq!(format_duration(Duration::from_secs(0)), "0s");
    }

    #[test]
    fn test_format_duration_minutes() {
        assert_eq!(format_duration(Duration::from_secs(90)), "1m 30s");
        assert_eq!(format_duration(Duration::from_secs(60)), "1m 0s");
    }

    #[test]
    fn test_format_duration_hours() {
        assert_eq!(format_duration(Duration::from_secs(3661)), "1h 1m");
        assert_eq!(format_duration(Duration::from_secs(7200)), "2h 0m");
    }

    #[test]
    fn test_default_server_time_provider_offset() {
        let provider = DefaultServerTimeProvider::new();
        let offset = provider.get_server_offset();
        // UTC+8
        assert_eq!(offset.local_minus_utc(), 8 * 3600);
    }

    #[test]
    fn test_server_time_trait_default_no_stack_overflow() {
        let st: ServerTime = Default::default();
        let offset = st.offset();
        assert_eq!(offset.local_minus_utc(), 8 * 3600);
    }
}
