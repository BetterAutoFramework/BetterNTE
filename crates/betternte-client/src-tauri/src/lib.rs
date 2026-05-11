use std::fs;
use std::io::Write as _;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;

use betternte_core::{EngineConfig, EngineEvent};
use betternte_engine::Engine;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{prelude::*, reload, Layer};

pub mod commands;
pub mod hotkeys;

// Global reload handle so init_engine can swap in the EventBus layer
type ReloadHandle = reload::Handle<Option<EventBusLayer>, tracing_subscriber::Registry>;
static RELOAD_HANDLE: OnceLock<ReloadHandle> = OnceLock::new();

// Global handle to the event bridge task, so we can abort old ones
static EVENT_BRIDGE_HANDLE: OnceLock<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>> =
    OnceLock::new();
static FILE_LOG_GUARD: OnceLock<WorkerGuard> = OnceLock::new();

struct RotatingFileWriter {
    base_path: PathBuf,
    max_bytes: u64,
    max_files: u64,
    file: Option<std::fs::File>,
    current_size: u64,
}

impl RotatingFileWriter {
    fn new(base_path: PathBuf, max_bytes: u64, max_files: u64) -> std::io::Result<Self> {
        if let Some(parent) = base_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut writer = Self {
            base_path,
            max_bytes,
            max_files: max_files.max(1),
            file: None,
            current_size: 0,
        };
        writer.open_append_file()?;
        Ok(writer)
    }

    fn suffixed_path(&self, idx: u64) -> PathBuf {
        PathBuf::from(format!("{}.{}", self.base_path.to_string_lossy(), idx))
    }

    fn open_append_file(&mut self) -> std::io::Result<()> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.base_path)?;
        let len = file.metadata().map(|m| m.len()).unwrap_or(0);
        self.current_size = len;
        self.file = Some(file);
        Ok(())
    }

    fn rotate(&mut self) -> std::io::Result<()> {
        if let Some(mut f) = self.file.take() {
            let _ = f.flush();
        }

        for i in (1..self.max_files).rev() {
            let from = self.suffixed_path(i);
            let to = self.suffixed_path(i + 1);
            if from.exists() {
                let _ = std::fs::rename(from, to);
            }
        }
        if self.base_path.exists() {
            let _ = std::fs::rename(&self.base_path, self.suffixed_path(1));
        }

        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.base_path)?;
        self.current_size = 0;
        self.file = Some(file);
        Ok(())
    }
}

impl std::io::Write for RotatingFileWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.max_bytes > 0 && self.current_size.saturating_add(buf.len() as u64) > self.max_bytes
        {
            self.rotate()?;
        }
        if self.file.is_none() {
            self.open_append_file()?;
        }
        if let Some(file) = self.file.as_mut() {
            let n = file.write(buf)?;
            self.current_size = self.current_size.saturating_add(n as u64);
            Ok(n)
        } else {
            Ok(0)
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if let Some(file) = self.file.as_mut() {
            file.flush()
        } else {
            Ok(())
        }
    }
}

fn level_from_config(level: &str) -> &'static str {
    match level.to_ascii_lowercase().as_str() {
        "debug" => "debug",
        "warn" => "warn",
        "error" => "error",
        _ => "info",
    }
}

#[cfg(windows)]
fn query_windows_elevated() -> Result<bool, String> {
    use std::mem::size_of;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)
            .map_err(|e| format!("OpenProcessToken failed: {}", e))?;

        let mut elevation = TOKEN_ELEVATION::default();
        let mut out_len: u32 = 0;
        let result = GetTokenInformation(
            token,
            TokenElevation,
            Some((&mut elevation as *mut TOKEN_ELEVATION).cast()),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut out_len,
        );
        let _ = CloseHandle(token);
        result.map_err(|e| format!("GetTokenInformation(TokenElevation) failed: {}", e))?;

        Ok(elevation.TokenIsElevated != 0)
    }
}

fn log_startup_privilege_status() {
    #[cfg(windows)]
    {
        match query_windows_elevated() {
            Ok(true) => tracing::info!("Startup privilege check: running as administrator"),
            Ok(false) => tracing::warn!("Startup privilege check: running without administrator"),
            Err(e) => tracing::warn!(error = %e, "Startup privilege check failed"),
        }
    }
}

fn init_tracing(app: &AppHandle) {
    let config = app
        .path()
        .app_config_dir()
        .ok()
        .map(|dir| load_config(&dir.join("config.json")))
        .unwrap_or_default();

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let log_path = {
        let configured = PathBuf::from(config.advanced.log_file.clone());
        if configured.is_absolute() {
            configured
        } else {
            cwd.join(configured)
        }
    };
    let max_bytes = config.advanced.log_max_size.saturating_mul(1024 * 1024);
    let max_files = config.advanced.log_max_files.max(1);
    let level = level_from_config(&config.advanced.log_level);

    let filter = tracing_subscriber::EnvFilter::from_default_env().add_directive(
        format!("betternte={}", level)
            .parse()
            .unwrap_or_else(|_| "betternte=info".parse().expect("valid directive")),
    );

    let (reload_layer, reload_handle) = reload::Layer::new(None::<EventBusLayer>);
    let _ = RELOAD_HANDLE.set(reload_handle);

    let file_writer = RotatingFileWriter::new(log_path.clone(), max_bytes, max_files).ok();
    let (non_blocking, guard) = tracing_appender::non_blocking(file_writer.unwrap_or_else(|| {
        RotatingFileWriter::new(PathBuf::from("logs/betternte.log"), 50 * 1024 * 1024, 5)
            .expect("fallback file logger must be creatable")
    }));
    let _ = FILE_LOG_GUARD.set(guard);

    tracing_subscriber::registry()
        .with(reload_layer)
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_thread_ids(true)
                .with_filter(filter.clone()),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_target(true)
                .with_thread_ids(true)
                .with_writer(non_blocking)
                .with_filter(filter),
        )
        .init();
}

// ============================================================================
// EventBus tracing layer — bridges Rust tracing logs to EngineEvent::LogMessage
// ============================================================================

#[derive(Clone)]
struct EventBusLayer {
    tx: tokio::sync::mpsc::UnboundedSender<EngineEvent>,
}

impl EventBusLayer {
    fn new(event_bus: betternte_engine::EventBus) -> Self {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<EngineEvent>();

        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                let _ = event_bus.publish(event);
            }
        });

        Self { tx }
    }
}

impl<S> tracing_subscriber::Layer<S> for EventBusLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let metadata = event.metadata();
        if *metadata.level() > tracing::Level::INFO {
            return;
        }

        let mut visitor = FieldVisitor(String::new());
        event.record(&mut visitor);

        let level = match *metadata.level() {
            tracing::Level::ERROR => "error",
            tracing::Level::WARN => "warn",
            tracing::Level::INFO => "info",
            tracing::Level::DEBUG => "debug",
            tracing::Level::TRACE => "debug",
        };

        let module = metadata.module_path().unwrap_or("unknown").to_string();

        let log_event = EngineEvent::LogMessage {
            level: level.to_string(),
            module,
            message: visitor.0,
            timestamp: chrono::Utc::now(),
        };

        let _ = self.tx.send(log_event);
    }
}

struct FieldVisitor(String);

impl tracing::field::Visit for FieldVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        let formatted = format!("{:?}", value);
        if self.0.is_empty() {
            // First field is the message — strip surrounding quotes for cleaner display
            self.0 = formatted.trim_matches('"').to_string();
        } else {
            self.0.push_str(&format!(" {}={}", field.name(), formatted));
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if self.0.is_empty() {
            self.0 = value.to_string();
        } else {
            self.0.push_str(&format!(" {}={}", field.name(), value));
        }
    }
}

// ============================================================================
// App State
// ============================================================================

pub struct AppState {
    engine: tokio::sync::RwLock<Option<Engine>>,
    config_path: Mutex<Option<PathBuf>>,
}

struct LockMetrics {
    read_calls: AtomicU64,
    write_calls: AtomicU64,
    read_timeouts: AtomicU64,
    write_timeouts: AtomicU64,
    read_wait_total_ms: AtomicU64,
    write_wait_total_ms: AtomicU64,
}

impl LockMetrics {
    const fn new() -> Self {
        Self {
            read_calls: AtomicU64::new(0),
            write_calls: AtomicU64::new(0),
            read_timeouts: AtomicU64::new(0),
            write_timeouts: AtomicU64::new(0),
            read_wait_total_ms: AtomicU64::new(0),
            write_wait_total_ms: AtomicU64::new(0),
        }
    }
}

static ENGINE_LOCK_METRICS: LockMetrics = LockMetrics::new();

impl AppState {
    fn new() -> Self {
        Self {
            engine: tokio::sync::RwLock::new(None),
            config_path: Mutex::new(None),
        }
    }

    pub(crate) async fn read_engine<'a>(
        &'a self,
        op: &'static str,
    ) -> Result<tokio::sync::RwLockReadGuard<'a, Option<Engine>>, String> {
        let start = std::time::Instant::now();
        let result = tokio::time::timeout(
            tokio::time::Duration::from_millis(500),
            self.engine.read(),
        )
        .await;
        let elapsed_ms = start.elapsed().as_millis() as u64;
        ENGINE_LOCK_METRICS.read_calls.fetch_add(1, Ordering::Relaxed);
        ENGINE_LOCK_METRICS
            .read_wait_total_ms
            .fetch_add(elapsed_ms, Ordering::Relaxed);
        match result {
            Ok(guard) => {
                if elapsed_ms >= 80 {
                    tracing::warn!(
                        op,
                        elapsed_ms,
                        "engine read lock waited longer than expected"
                    );
                }
                Ok(guard)
            }
            Err(_) => {
                let timeout_count = ENGINE_LOCK_METRICS
                    .read_timeouts
                    .fetch_add(1, Ordering::Relaxed)
                    + 1;
                tracing::warn!(
                    op,
                    elapsed_ms,
                    timeout_count,
                    "engine read lock timed out"
                );
                Err(format!("Engine busy while {}, please retry", op))
            }
        }
    }

    pub(crate) async fn write_engine<'a>(
        &'a self,
        op: &'static str,
    ) -> Result<tokio::sync::RwLockWriteGuard<'a, Option<Engine>>, String> {
        let start = std::time::Instant::now();
        let result = tokio::time::timeout(
            tokio::time::Duration::from_millis(500),
            self.engine.write(),
        )
        .await;
        let elapsed_ms = start.elapsed().as_millis() as u64;
        ENGINE_LOCK_METRICS.write_calls.fetch_add(1, Ordering::Relaxed);
        ENGINE_LOCK_METRICS
            .write_wait_total_ms
            .fetch_add(elapsed_ms, Ordering::Relaxed);
        match result {
            Ok(guard) => {
                if elapsed_ms >= 80 {
                    tracing::warn!(
                        op,
                        elapsed_ms,
                        "engine write lock waited longer than expected"
                    );
                }
                Ok(guard)
            }
            Err(_) => {
                let timeout_count = ENGINE_LOCK_METRICS
                    .write_timeouts
                    .fetch_add(1, Ordering::Relaxed)
                    + 1;
                tracing::warn!(
                    op,
                    elapsed_ms,
                    timeout_count,
                    "engine write lock timed out"
                );
                Err(format!("Engine busy while {}, please retry", op))
            }
        }
    }
}

// ============================================================================
// Config file management
// ============================================================================

fn validate_path_in(base: &Path, user_input: &str) -> Result<PathBuf, String> {
    let full = base.join(user_input);
    let canonical = full
        .canonicalize()
        .map_err(|e| format!("Path not found: {}", e))?;
    let canonical_base = base
        .canonicalize()
        .map_err(|e| format!("Base dir error: {}", e))?;
    if !canonical.starts_with(&canonical_base) {
        return Err("Path traversal detected".into());
    }
    Ok(canonical)
}

fn find_workspace_root(start: &PathBuf) -> Option<PathBuf> {
    let mut dir = start.as_path();
    loop {
        if dir.join("scripts").is_dir() && dir.join("Cargo.toml").exists() {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
    }
}

pub(crate) fn ensure_game_identity_defaults(cfg: &mut EngineConfig) {
    let d = betternte_core::config::GameConfig::default();
    if cfg.game.game_name.trim().is_empty() {
        cfg.game.game_name = d.game_name.clone();
    }
    if cfg.game.window_title_keyword.trim().is_empty() {
        cfg.game.window_title_keyword = d.window_title_keyword.clone();
    }
    if cfg.game.process_name.trim().is_empty() {
        cfg.game.process_name = d.process_name.clone();
    }
}

/// Repo root in development; per-user writable dir when installed (MSI / release without workspace).
pub(crate) fn resolve_engine_base_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| {
        app.path()
            .app_config_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
    });
    if let Some(ws) = find_workspace_root(&cwd) {
        tracing::info!(path = %ws.display(), "Engine base dir: workspace root");
        return Ok(ws);
    }
    let data = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("Failed to resolve app local data dir: {}", e))?;
    tracing::info!(path = %data.display(), "Engine base dir: app local data (packaged / no workspace)");
    Ok(data)
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Copy bundled `data/` and `assets/` from Tauri resources into the user base dir on first run.
pub(crate) fn seed_bundled_user_data(app: &AppHandle, user_base: &Path) -> Result<(), String> {
    let res_dir = match app.path().resource_dir() {
        Ok(p) => p,
        Err(e) => {
            tracing::debug!(error = %e, "resource_dir unavailable, skip bundled data seed");
            return Ok(());
        }
    };

    for name in ["data", "assets"] {
        let src = res_dir.join(name);
        if !src.is_dir() {
            tracing::debug!(path = %src.display(), "Bundled {} not in resources, skip", name);
            continue;
        }
        let dst = user_base.join(name);
        if dst.exists() {
            tracing::debug!(path = %dst.display(), "User {} already exists, skip seed", name);
            continue;
        }
        copy_dir_recursive(&src, &dst).map_err(|e| {
            format!(
                "Failed to seed {} from {} to {}: {}",
                name,
                src.display(),
                dst.display(),
                e
            )
        })?;
        tracing::info!(from = %src.display(), to = %dst.display(), "Seeded {} from bundle", name);
    }

    // Per-directory hash-based sync: only update scripts/triggers/plugins whose content changed.
    let current_version = app.package_info().version.to_string();
    let data_dir = user_base.join("data");

    for subdir in &["main", "plugins"] {
        let bundled = res_dir.join("data").join(subdir);
        if !bundled.is_dir() {
            continue;
        }
        let user_dir = data_dir.join(subdir);
        let marker = data_dir.join(format!(".{}_bundle_hashes", subdir));

        // Load previous hashes { dir_name -> hash_string }
        let prev_hashes: std::collections::HashMap<String, String> =
            fs::read_to_string(&marker)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default();

        // Walk bundled top-level directories and compute hashes
        let mut new_hashes: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let entries = match fs::read_dir(&bundled) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let entry_path = entry.path();
            if !entry_path.is_dir() {
                continue;
            }
            let dir_name = entry.file_name().to_string_lossy().to_string();
            let hash = hash_directory(&entry_path);
            new_hashes.insert(dir_name.clone(), hash.clone());

            let prev_hash = prev_hashes.get(&dir_name);
            if prev_hash == Some(&hash) {
                tracing::debug!(subdir, dir = %dir_name, "Directory unchanged, skip");
                continue;
            }

            // Content changed or new directory — replace this specific directory
            let dst = user_dir.join(&dir_name);
            if dst.exists() {
                fs::remove_dir_all(&dst).map_err(|e| {
                    format!("Failed to clear {}/{}: {}", subdir, dir_name, e)
                })?;
            }
            copy_dir_recursive(&entry_path, &dst).map_err(|e| {
                format!(
                    "Failed to sync {}/{}: {}",
                    subdir, dir_name, e
                )
            })?;
            tracing::info!(
                subdir,
                dir = %dir_name,
                prev = prev_hash.map(|s| s.as_str()).unwrap_or("none"),
                new = %hash,
                "Synced directory"
            );
        }

        // Remove user directories that no longer exist in the bundle
        if user_dir.exists() {
            for entry in fs::read_dir(&user_dir).into_iter().flatten().flatten() {
                let dir_name = entry.file_name().to_string_lossy().to_string();
                if !new_hashes.contains_key(&dir_name) && !dir_name.starts_with('.') {
                    let dst = user_dir.join(&dir_name);
                    if dst.is_dir() {
                        let _ = fs::remove_dir_all(&dst);
                        tracing::info!(subdir, dir = %dir_name, "Removed stale bundled directory");
                    }
                }
            }
        }

        // Write updated hashes
        fs::create_dir_all(&data_dir).ok();
        if let Ok(json) = serde_json::to_string_pretty(&new_hashes) {
            let _ = fs::write(&marker, json);
        }
        // Also write version marker for compatibility
        let version_marker = data_dir.join(format!(".{}_bundle_version", subdir));
        let _ = fs::write(&version_marker, &current_version);
    }

    Ok(())
}

/// Compute a SHA-256-like hash of all files in a directory (recursively).
/// Uses file content + relative path for deterministic comparison.
fn hash_directory(dir: &Path) -> String {
    use std::collections::BTreeMap;

    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    collect_files(dir, dir, &mut files);

    // Simple hash: concat "path:content_len:content_hash" for each file
    let mut hasher_input = Vec::new();
    for (path, content) in &files {
        hasher_input.extend_from_slice(path.as_bytes());
        hasher_input.push(0);
        hasher_input.extend_from_slice(&content.len().to_le_bytes());
        // Use first 64 bytes + last 64 bytes as a quick fingerprint
        let fingerprint = if content.len() <= 128 {
            content.clone()
        } else {
            let mut fp = Vec::with_capacity(128);
            fp.extend_from_slice(&content[..64]);
            fp.extend_from_slice(&content[content.len() - 64..]);
            fp
        };
        hasher_input.extend_from_slice(&fingerprint);
    }

    // Simple hash using std::hash
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    hasher_input.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Recursively collect all files in a directory as (relative_path, content) pairs.
fn collect_files(base: &Path, dir: &Path, out: &mut std::collections::BTreeMap<String, Vec<u8>>) {
    use std::io::Read;
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let rel = path
            .strip_prefix(base)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if path.is_dir() {
            collect_files(base, &path, out);
        } else if let Ok(mut f) = fs::File::open(&path) {
            let mut buf = Vec::new();
            let _ = f.read_to_end(&mut buf);
            out.insert(rel, buf);
        }
    }
}

fn load_config(path: &PathBuf) -> EngineConfig {
    match std::fs::read_to_string(path) {
        Ok(content) => match serde_json::from_str::<EngineConfig>(&content) {
            Ok(mut config) => {
                normalize_subscription_names(&mut config);
                ensure_game_identity_defaults(&mut config);
                tracing::info!(path = %path.display(), "Config loaded from file");
                config
            }
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "Config parse failed, using default");
                EngineConfig::default()
            }
        },
        Err(_) => {
            tracing::info!(path = %path.display(), "Config file not found, using default");
            EngineConfig::default()
        }
    }
}

fn is_likely_mojibake_name(name: &str) -> bool {
    name.contains('\u{FFFD}')
        || name.contains("鏈")
        || name.contains("婧")
        || name.contains("樻")
        || name.contains("鍦")
        || name.contains("锟")
}

fn normalize_subscription_names(config: &mut EngineConfig) {
    for sub in &mut config.scripts.subscriptions {
        let expected_name = match sub.directory.as_str() {
            "local" => Some("本地源"),
            "main" => Some("官方源"),
            _ => None,
        };

        if let Some(expected) = expected_name {
            if sub.name.is_empty() || is_likely_mojibake_name(&sub.name) {
                tracing::warn!(
                    directory = %sub.directory,
                    old_name = %sub.name,
                    new_name = %expected,
                    "Detected garbled subscription name, normalized it"
                );
                sub.name = expected.to_string();
            }
        }
    }
}

fn save_config(path: &PathBuf, config: &EngineConfig) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config dir: {}", e))?;
    }
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    std::fs::write(path, json).map_err(|e| format!("Failed to write config: {}", e))?;
    tracing::info!(path = %path.display(), "Config saved");
    Ok(())
}

/// Write the in-memory engine config to disk (if `config_path` is set).
pub(crate) async fn persist_engine_config_file(
    state: &tauri::State<'_, AppState>,
) -> Result<(), String> {
    let path_opt = {
        let guard = state.config_path.lock().await;
        guard.clone()
    };
    let Some(ref path) = path_opt else {
        return Ok(());
    };
    let cfg = {
        let guard = state.engine.read().await;
        guard.as_ref().map(|e| e.config().clone())
    };
    if let Some(cfg) = cfg {
        save_config(path, &cfg)?;
    }
    Ok(())
}

// ============================================================================
// Event bridge: EventBus -> Tauri app.emit()
// ============================================================================

fn spawn_event_bridge(
    app: AppHandle,
    event_bus: betternte_engine::EventBus,
) -> tauri::async_runtime::JoinHandle<()> {
    let mut rx = event_bus.subscribe();
    let mut control_rx = event_bus.subscribe_control();

    tauri::async_runtime::spawn(async move {
        loop {
            tokio::select! {
                recv = rx.recv() => {
                    match recv {
                        Ok(event) => {
                            if let Err(e) = app.emit("engine-event", &event) {
                                tracing::warn!(error = %e, "Failed to emit engine event");
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(skipped = n, "Event bridge lagged, skipped events");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            tracing::info!("Event bus closed, event bridge stopping");
                            break;
                        }
                    }
                }
                recv = control_rx.recv() => {
                    match recv {
                        Ok(event) => {
                            if let Err(e) = app.emit("engine-control-event", &event) {
                                tracing::warn!(error = %e, "Failed to emit engine control event");
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(skipped = n, "Control event bridge lagged, skipped events");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            tracing::info!("Control event bus closed, control bridge stopping");
                            break;
                        }
                    }
                }
            }
        }
    })
}

// ============================================================================
// Preflight
// ============================================================================

fn preflight_bind_window(engine: &betternte_engine::Engine) -> Result<(), String> {
    let keyword = engine.config().game.window_title_keyword.trim();
    let process = engine.config().game.process_name.trim();
    if keyword.is_empty() && process.is_empty() {
        return Ok(());
    }

    let window = engine
        .find_game_window()
        .map_err(|e| format!("游戏窗口未找到: {}。请确认游戏已启动。", e))?;

    tracing::info!(
        hwnd = window.hwnd,
        title = %window.title,
        process = %window.process_name,
        "Game window preflight validated"
    );
    Ok(())
}

// ============================================================================
// Tauri entry point
// ============================================================================

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Set console output codepage to UTF-8 so Chinese characters display correctly
    #[cfg(windows)]
    unsafe {
        extern "system" {
            fn SetConsoleOutputCP(wCodePageID: u32) -> i32;
            fn SetConsoleCP(wCodePageID: u32) -> i32;
            fn SetDllDirectoryW(lpPathName: *const u16) -> i32;
        }
        SetConsoleOutputCP(65001); // CP_UTF8
        SetConsoleCP(65001); // CP_UTF8

        // Add exe directory to DLL search path so opencv_world490.dll is found at runtime.
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                let wide: Vec<u16> = exe_dir
                    .as_os_str()
                    .encode_wide()
                    .chain(std::iter::once(0))
                    .collect();
                SetDllDirectoryW(wide.as_ptr());
            }
        }
    }

    let mut builder = tauri::Builder::default();

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        builder = builder
            .plugin(tauri_plugin_updater::Builder::new().build())
            .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                } else {
                    tracing::warn!("single-instance: main window not found");
                }
            }));
    }

    builder
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(AppState::new())
        .setup(|app| {
            let handle = app.handle().clone();
            init_tracing(&handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::engine::init_engine,
            commands::engine::start_engine,
            commands::engine::stop_engine,
            commands::engine::get_status,
            commands::engine::stop_all,
            commands::scripts::reload_scripts,
            commands::scripts::list_scripts,
            commands::scripts::run_script,
            commands::scripts::stop_task,
            commands::scripts::enable_trigger,
            commands::scripts::disable_trigger,
            commands::scripts::reload_triggers,
            commands::scripts::list_triggers,
            commands::scripts::create_script,
            commands::scripts::delete_script,
            commands::scripts::list_script_files,
            commands::scripts::read_script_source,
            commands::scripts::save_script_source,
            commands::scripts::import_script_asset,
            commands::flows::list_task_groups,
            commands::flows::run_task_group,
            commands::flows::stop_task_group,
            commands::flows::get_task_group_progress,
            commands::flows::run_flow,
            commands::flows::stop_flow,
            commands::flows::get_flow_progress,
            commands::flows::save_task_group,
            commands::flows::delete_task_group,
            commands::flows::list_flows,
            commands::flows::save_flow,
            commands::flows::delete_flow,
            commands::replay::replay_verify_session,
            commands::replay::replay_verify_artifacts,
            commands::input::input_list_windows,
            commands::input::input_bind_window,
            commands::input::input_key_down,
            commands::input::input_key_up,
            commands::input::input_key_tap,
            commands::input::input_mouse_move,
            commands::input::input_mouse_scroll,
            commands::input::input_mouse_button,
            commands::input::input_mouse_click,
            commands::input::input_demo_alt_move,
            commands::input::input_demo_move_left_click,
            commands::input::input_demo_middle_hold_move_click,
            commands::input::input_run_js_snippet,
            commands::settings::get_config,
            commands::settings::save_config_cmd,
            commands::settings::get_capture_methods,
            commands::settings::list_subscriptions,
            commands::settings::save_subscription,
            commands::settings::delete_subscription,
            commands::settings::list_windows,
            commands::settings::find_game_window,
            commands::settings::test_screenshot,
            commands::settings::test_notification_channel,
            commands::settings::export_logs,
            commands::settings::better_nte_debug_enabled,
            commands::settings::get_scan_dirs,
            commands::settings::list_plugins,
            commands::settings::set_plugin_enabled,
        ])
        .setup(|app| {
            use tauri::menu::{MenuBuilder, MenuItemBuilder};
            use tauri::tray::TrayIconBuilder;

            log_startup_privilege_status();

            let show_item = MenuItemBuilder::with_id("show", "显示主窗口").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "退出").build(app)?;
            let menu = MenuBuilder::new(app)
                .item(&show_item)
                .separator()
                .item(&quit_item)
                .build()?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .tooltip("BetterNTE")
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            if let Some(window) = app.get_webview_window("main") {
                let window_clone = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = window_clone.hide();
                    }
                });
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
