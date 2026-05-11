//! ctx API bridge — inject ScriptContext into JS global object.
//!
//! Uses synchronous `__invoke` dispatcher + JS Promise wrappers to bridge
//! async ScriptContext methods to JS.

use anyhow::Result;
use rquickjs::{function::Func, Ctx, Object};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::Instant;

use crate::engine::{
    ColorMatchAllOpts, ColorMatchPoint, FindTemplateBatchEntry, FindTemplateOpts, LogLevel, Region,
    ScriptContext,
};

// ━━━ Job abstraction for async operations ━━━

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Running,
    Done,
    Failed,
    Cancelled,
}

struct JobEntry {
    status: JobStatus,
    result: Option<serde_json::Value>,
    error: Option<String>,
    cancel: crate::engine::CancellationToken,
    notify: Arc<tokio::sync::Notify>,
}

/// Thread-safe job manager for async operations.
/// Each `post_*` call creates a Job, spawns it as a tokio task, and returns a job ID.
#[derive(Clone)]
pub struct JobManager {
    inner: Arc<Mutex<JobManagerInner>>,
}

struct JobManagerInner {
    next_id: u64,
    jobs: HashMap<u64, JobEntry>,
}

impl JobManager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(JobManagerInner {
                next_id: 1,
                jobs: HashMap::new(),
            })),
        }
    }

    /// Create a new pending job, return its ID and a cancellation token.
    fn create(&self) -> (u64, crate::engine::CancellationToken) {
        let mut inner = self.inner.lock().unwrap();
        let id = inner.next_id;
        inner.next_id += 1;
        let cancel = crate::engine::CancellationToken::new();
        inner.jobs.insert(id, JobEntry {
            status: JobStatus::Pending,
            result: None,
            error: None,
            cancel: cancel.clone(),
            notify: Arc::new(tokio::sync::Notify::new()),
        });
        (id, cancel)
    }

    /// Spawn an async task for a job.
    fn spawn<F, Fut>(&self, id: u64, ctx: Arc<dyn crate::engine::ScriptContext>, f: F)
    where
        F: FnOnce(Arc<dyn crate::engine::ScriptContext>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = std::result::Result<serde_json::Value, String>> + Send + 'static,
    {
        let mgr = self.clone();
        tokio::spawn(async move {
            // Mark running
            {
                let mut inner = mgr.inner.lock().unwrap();
                if let Some(job) = inner.jobs.get_mut(&id) {
                    if job.status == JobStatus::Cancelled {
                        return;
                    }
                    job.status = JobStatus::Running;
                }
            }

            let result = f(ctx).await;

            // Mark done
            let notify = {
                let mut inner = mgr.inner.lock().unwrap();
                if let Some(job) = inner.jobs.get_mut(&id) {
                    if job.status == JobStatus::Cancelled {
                        return;
                    }
                    match result {
                        Ok(val) => {
                            job.status = JobStatus::Done;
                            job.result = Some(val);
                        }
                        Err(e) => {
                            job.status = JobStatus::Failed;
                            job.error = Some(e);
                        }
                    }
                    job.notify.clone()
                } else {
                    return;
                }
            };
            notify.notify_waiters();
        });
    }

    /// Get job status.
    pub fn status(&self, id: u64) -> Option<JobStatus> {
        self.inner.lock().unwrap().jobs.get(&id).map(|j| j.status)
    }

    /// Get job result (only valid when status is Done).
    pub fn result(&self, id: u64) -> Option<serde_json::Value> {
        self.inner.lock().unwrap().jobs.get(&id).and_then(|j| j.result.clone())
    }

    /// Get job error (only valid when status is Failed).
    pub fn error(&self, id: u64) -> Option<String> {
        self.inner.lock().unwrap().jobs.get(&id).and_then(|j| j.error.clone())
    }

    /// Cancel a job.
    pub fn cancel(&self, id: u64) -> bool {
        let mut inner = self.inner.lock().unwrap();
        if let Some(job) = inner.jobs.get_mut(&id) {
            if job.status == JobStatus::Pending || job.status == JobStatus::Running {
                job.status = JobStatus::Cancelled;
                job.cancel.cancel();
                return true;
            }
        }
        false
    }

    /// Get a notify handle for waiting on job completion.
    pub fn notify_handle(&self, id: u64) -> Option<Arc<tokio::sync::Notify>> {
        self.inner.lock().unwrap().jobs.get(&id).map(|j| j.notify.clone())
    }

    /// Check if a job is terminal (Done/Failed/Cancelled).
    pub fn is_terminal(&self, id: u64) -> bool {
        matches!(
            self.inner.lock().unwrap().jobs.get(&id).map(|j| j.status),
            Some(JobStatus::Done | JobStatus::Failed | JobStatus::Cancelled)
        )
    }

    /// Remove completed jobs older than the given count (keeps most recent N).
    pub fn gc(&self, keep: usize) {
        let mut inner = self.inner.lock().unwrap();
        let mut terminal_ids: Vec<u64> = inner.jobs.iter()
            .filter(|(_, j)| matches!(j.status, JobStatus::Done | JobStatus::Failed | JobStatus::Cancelled))
            .map(|(id, _)| *id)
            .collect();
        if terminal_ids.len() <= keep {
            return;
        }
        terminal_ids.sort_unstable();
        terminal_ids.truncate(terminal_ids.len() - keep);
        for id in terminal_ids {
            inner.jobs.remove(&id);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BridgePerfMode {
    Off,
    Steps,
    Verbose,
}

fn bridge_perf_mode() -> BridgePerfMode {
    static MODE: OnceLock<BridgePerfMode> = OnceLock::new();
    *MODE.get_or_init(|| match std::env::var("BETTERNTE_PERF_LOG") {
        Ok(v) => {
            let v = v.to_ascii_lowercase();
            match v.as_str() {
                "1" | "true" | "yes" | "steps" => BridgePerfMode::Steps,
                "2" | "verbose" | "all" | "full" => BridgePerfMode::Verbose,
                _ => BridgePerfMode::Off,
            }
        }
        Err(_) => BridgePerfMode::Off,
    })
}

fn bridge_slow_threshold_ms() -> f64 {
    static TH: OnceLock<f64> = OnceLock::new();
    *TH.get_or_init(|| {
        std::env::var("BETTERNTE_PERF_BRIDGE_SLOW_MS")
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .filter(|&v| v >= 0.0)
            .unwrap_or(5.0)
    })
}

// ━━━ Dedicated bridge thread for async-to-sync conversion ━━━
//
// `block_in_place` panics when called from within a tokio::time::timeout
// or any nested async context (which is exactly where QuickJS callbacks run).
// Instead, we send the async work to a dedicated background thread that owns
// its own tokio runtime, and wait synchronously for the result.

type BridgeRequest = Box<dyn FnOnce() + Send + 'static>;

struct BridgeThread {
    tx: mpsc::Sender<BridgeRequest>,
}

static BRIDGE: OnceLock<std::sync::Mutex<Option<BridgeThread>>> = OnceLock::new();

/// Nesting depth for synchronous `__invoke` (e.g. library calls `await ctx.click()`).
/// The dedicated bridge thread can only process one queued task at a time; nested
/// invokes must not wait on that same thread or we deadlock.
static INVOKE_SYNC_DEPTH: AtomicU32 = AtomicU32::new(0);

struct InvokeSyncDepthGuard;

impl Drop for InvokeSyncDepthGuard {
    fn drop(&mut self) {
        INVOKE_SYNC_DEPTH.fetch_sub(1, Ordering::SeqCst);
    }
}

fn get_bridge_sender() -> mpsc::Sender<BridgeRequest> {
    let guard = BRIDGE.get_or_init(|| std::sync::Mutex::new(None));
    let mut bridge = guard.lock().expect("Failed to lock bridge state");
    if bridge.is_none() {
        let (tx, rx) = mpsc::channel::<BridgeRequest>();
        std::thread::Builder::new()
            .name("qjs-async-bridge".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                    .expect("Failed to create bridge tokio runtime");
                let _guard = rt.enter();
                while let Ok(task) = rx.recv() {
                    task();
                }
            })
            .expect("Failed to spawn bridge thread");
        *bridge = Some(BridgeThread { tx });
    }
    bridge
        .as_ref()
        .expect("Bridge should be initialized")
        .tx
        .clone()
}

pub type ScriptCtxSlot = Arc<std::sync::RwLock<Option<Arc<dyn ScriptContext>>>>;
pub type CancelTokenSlot = Arc<std::sync::RwLock<Option<crate::engine::CancellationToken>>>;

/// Register ctx API to JS global object.
pub fn register_ctx_api<'js>(
    js_ctx: &Ctx<'js>,
    ctx_slot: ScriptCtxSlot,
    cancel_slot: CancelTokenSlot,
) -> Result<()> {
    let globals = js_ctx.globals();
    let ctx_obj = Object::new(js_ctx.clone())?;
    let job_mgr = JobManager::new();

    // ━━━ Synchronous methods (no async, no block_on needed) ━━━
    let is_cancelled_ctx = ctx_slot.clone();
    ctx_obj.set(
        "isCancelled",
        Func::from(move || -> bool {
            is_cancelled_ctx
                .read()
                .ok()
                .and_then(|g| g.clone())
                .map(|s| s.is_cancelled())
                .unwrap_or(false)
        }),
    )?;
    let progress_ctx = ctx_slot.clone();
    ctx_obj.set(
        "progress",
        Func::from(move |current: u32, total: u32| {
            if let Some(s) = progress_ctx.read().ok().and_then(|g| g.clone()) {
                s.progress(current, total);
            }
        }),
    )?;
    let fps_ctx = ctx_slot.clone();
    ctx_obj.set(
        "getFps",
        Func::from(move || -> f64 {
            fps_ctx
                .read()
                .ok()
                .and_then(|g| g.clone())
                .map(|s| s.get_fps())
                .unwrap_or(0.0)
        }),
    )?;
    let frame_ctx = ctx_slot.clone();
    ctx_obj.set(
        "getFrameNumber",
        Func::from(move || -> u64 {
            frame_ctx
                .read()
                .ok()
                .and_then(|g| g.clone())
                .map(|s| s.get_frame_number())
                .unwrap_or(0)
        }),
    )?;
    let log_ctx = ctx_slot.clone();
    ctx_obj.set(
        "log",
        Func::from(move |level: String, message: String| {
            let lvl = match level.as_str() {
                "debug" => LogLevel::Debug,
                "info" => LogLevel::Info,
                "warn" => LogLevel::Warn,
                "error" => LogLevel::Error,
                _ => LogLevel::Info,
            };
            if let Some(s) = log_ctx.read().ok().and_then(|g| g.clone()) {
                s.log(lvl, &message);
            }
        }),
    )?;
    let log_info_ctx = ctx_slot.clone();
    ctx_obj.set(
        "logInfo",
        Func::from(move |msg: String| {
            if let Some(s) = log_info_ctx.read().ok().and_then(|g| g.clone()) {
                s.log(LogLevel::Info, &msg);
            }
        }),
    )?;
    let log_warn_ctx = ctx_slot.clone();
    ctx_obj.set(
        "logWarn",
        Func::from(move |msg: String| {
            if let Some(s) = log_warn_ctx.read().ok().and_then(|g| g.clone()) {
                s.log(LogLevel::Warn, &msg);
            }
        }),
    )?;
    let log_error_ctx = ctx_slot.clone();
    ctx_obj.set(
        "logError",
        Func::from(move |msg: String| {
            if let Some(s) = log_error_ctx.read().ok().and_then(|g| g.clone()) {
                s.log(LogLevel::Error, &msg);
            }
        }),
    )?;
    let log_debug_ctx = ctx_slot.clone();
    ctx_obj.set(
        "logDebug",
        Func::from(move |msg: String| {
            if let Some(s) = log_debug_ctx.read().ok().and_then(|g| g.clone()) {
                s.log(LogLevel::Debug, &msg);
            }
        }),
    )?;

    // ━━━ Core: synchronous __invoke dispatcher (global) ━━━
    // Must be a global (not on ctx) so JS wrapper closures can find it
    // when executed later via execute_pending_job.
    let invoke_job_mgr = job_mgr.clone();
    globals.set(
        "__invoke",
        Func::from(move |method: String, args_json: String| -> String {
            let result = invoke_ctx_method_sync(&ctx_slot, &cancel_slot, &method, &args_json, &invoke_job_mgr);
            match result {
                Ok(val) => serde_json::to_string(&val).unwrap_or_else(|_| "null".into()),
                Err(e) => format!("__ERROR__:{}", e),
            }
        }),
    )?;

    // ━━━ Job management methods (synchronous, on ctx object) ━━━
    let job_mgr_status = job_mgr.clone();
    ctx_obj.set(
        "jobStatus",
        Func::from(move |id: u64| -> String {
            job_mgr_status.status(id)
                .map(|s| serde_json::to_string(&s).unwrap_or_else(|_| "null".into()))
                .unwrap_or_else(|| "null".into())
        }),
    )?;
    let job_mgr_result = job_mgr.clone();
    ctx_obj.set(
        "jobResult",
        Func::from(move |id: u64| -> String {
            match job_mgr_result.result(id) {
                Some(val) => serde_json::to_string(&val).unwrap_or_else(|_| "null".into()),
                None => "null".into(),
            }
        }),
    )?;
    let job_mgr_error = job_mgr.clone();
    ctx_obj.set(
        "jobError",
        Func::from(move |id: u64| -> String {
            job_mgr_error.error(id).unwrap_or_default()
        }),
    )?;
    let job_mgr_cancel = job_mgr.clone();
    ctx_obj.set(
        "jobCancel",
        Func::from(move |id: u64| -> bool {
            job_mgr_cancel.cancel(id)
        }),
    )?;
    let job_mgr_gc = job_mgr.clone();
    ctx_obj.set(
        "jobGc",
        Func::from(move |keep: u32| {
            job_mgr_gc.gc(keep as usize);
        }),
    )?;

    // ━━━ Inject JS wrapper functions via eval ━━━
    let wrapper_code = r#"
    (function() {
        function wrapAsync(method) {
            return function() {
                var args = Array.prototype.slice.call(arguments);
                var p = new Promise(function(resolve, reject) {
                    try {
                        var result = __invoke(method, JSON.stringify(args));
                        if (result.indexOf("__ERROR__:") === 0) {
                            reject(new Error(result.substring(10)));
                        } else {
                            resolve(JSON.parse(result));
                        }
                    } catch(e) {
                        reject(e);
                    }
                });
                return p;
            };
        }
        function wrapVoidAsync(method) {
            return function() {
                var args = Array.prototype.slice.call(arguments);
                var p = new Promise(function(resolve, reject) {
                    try {
                        var result = __invoke(method, JSON.stringify(args));
                        if (result.indexOf("__ERROR__:") === 0) {
                            reject(new Error(result.substring(10)));
                        } else {
                            resolve();
                        }
                    } catch(e) {
                        reject(e);
                    }
                });
                return p;
            };
        }
        function wrapJobWait() {
            return function(jobId) {
                var p = new Promise(function(resolve, reject) {
                    try {
                        var result = ctx.jobWait(jobId);
                        if (result.indexOf("__ERROR__:") === 0) {
                            reject(new Error(result.substring(10)));
                        } else {
                            resolve(JSON.parse(result));
                        }
                    } catch(e) {
                        reject(e);
                    }
                });
                return p;
            };
        }
        return {
            // Job
            jobWait: wrapJobWait(),
            // Capture
            capture: wrapAsync("capture"),
            captureRegion: wrapAsync("captureRegion"),
            // Recognition
            findTemplate: wrapAsync("findTemplate"),
            findTemplates: wrapAsync("findTemplates"),
            findTemplateBatch: wrapAsync("findTemplateBatch"),
            ocr: wrapAsync("ocr"),
            ocrAll: wrapAsync("ocrAll"),
            getColor: wrapAsync("getColor"),
            getColors: wrapAsync("getColors"),
            colorMatch: wrapAsync("colorMatch"),
            colorMatchAll: wrapAsync("colorMatchAll"),
            scanSliderStrip: wrapAsync("scanSliderStrip"),
            scanStripEdges: wrapAsync("scanStripEdges"),
            countColor: wrapAsync("countColor"),
            // Input
            click: wrapVoidAsync("click"),
            doubleClick: wrapVoidAsync("doubleClick"),
            rightClick: wrapVoidAsync("rightClick"),
            mouseMove: wrapVoidAsync("mouseMove"),
            mouseDown: wrapVoidAsync("mouseDown"),
            mouseUp: wrapVoidAsync("mouseUp"),
            scroll: wrapVoidAsync("scroll"),
            swipe: wrapVoidAsync("swipe"),
            keyDown: wrapVoidAsync("keyDown"),
            keyUp: wrapVoidAsync("keyUp"),
            keyPress: wrapVoidAsync("keyPress"),
            keyCombo: wrapVoidAsync("keyCombo"),
            typeText: wrapVoidAsync("typeText"),
            // Wait (time-based)
            sleep: wrapVoidAsync("sleep"),
            waitForTemplate: wrapAsync("waitForTemplate"),
            waitGone: wrapAsync("waitGone"),
            waitForColor: wrapAsync("waitForColor"),
            // Wait (frame-based)
            sleepFrames: wrapVoidAsync("sleepFrames"),
            waitForTemplateFrames: wrapAsync("waitForTemplateFrames"),
            waitGoneFrames: wrapAsync("waitGoneFrames"),
            waitForColorFrames: wrapAsync("waitForColorFrames"),
            // Window
            findWindow: wrapAsync("findWindow"),
            activateWindow: wrapVoidAsync("activateWindow"),
            getWindowRect: wrapAsync("getWindowRect"),
            getScreenSize: wrapAsync("getScreenSize"),
            getScaleFactors: wrapAsync("getScaleFactors"),
            getFrameSize: wrapAsync("getFrameSize"),
            // Inter-script
            runScript: wrapAsync("runScript"),
            call: wrapAsync("call"),
            // Utilities
            notify: wrapVoidAsync("notify"),
            // File ops (manifest-scoped)
            readStoreFile: wrapAsync("readStoreFile"),
            writeStoreFile: wrapVoidAsync("writeStoreFile"),
            listStoreFiles: wrapAsync("listStoreFiles"),
            // File ops (system-level)
            readFile: wrapAsync("readFile"),
            writeFile: wrapVoidAsync("writeFile"),
            listFiles: wrapAsync("listFiles"),
            fileExists: wrapAsync("fileExists"),
            // Network
            httpGet: wrapAsync("httpGet"),
            httpPost: wrapAsync("httpPost"),
            // Storage
            storageGet: wrapAsync("storageGet"),
            storageSet: wrapVoidAsync("storageSet"),
            storageDelete: wrapVoidAsync("storageDelete"),
            storageKeys: wrapAsync("storageKeys"),
            // Post (async job)
            postCapture: wrapAsync("postCapture"),
            postClick: wrapVoidAsync("postClick"),
            postSwipe: wrapVoidAsync("postSwipe"),
            postKeyPress: wrapVoidAsync("postKeyPress"),
            postOcr: wrapAsync("postOcr"),
            postFindTemplate: wrapAsync("postFindTemplate"),
            postWaitForTemplate: wrapAsync("postWaitForTemplate"),
            // Aliases for script compatibility
            screenshot: wrapAsync("saveScreenshot"),
            matchTemplate: wrapAsync("findTemplate"),
        };
    })()
    "#;

    let async_fns: Object = js_ctx.eval(wrapper_code)?;
    let keys = [
        // Job
        "jobWait",
        // Capture
        "capture",
        "captureRegion",
        "saveScreenshot",
        // Recognition
        "findTemplate",
        "findTemplates",
        "findTemplateBatch",
        "ocr",
        "ocrAll",
        "getColor",
        "getColors",
        "colorMatch",
        "colorMatchAll",
        "scanSliderStrip",
        "scanStripEdges",
        "countColor",
        // Input
        "click",
        "doubleClick",
        "rightClick",
        "mouseMove",
        "mouseDown",
        "mouseUp",
        "scroll",
        "swipe",
        "keyDown",
        "keyUp",
        "keyPress",
        "keyCombo",
        "typeText",
        // Wait (time-based)
        "sleep",
        "waitForTemplate",
        "waitGone",
        "waitForColor",
        // Wait (frame-based)
        "sleepFrames",
        "waitForTemplateFrames",
        "waitGoneFrames",
        "waitForColorFrames",
        // Window
        "findWindow",
        "activateWindow",
        "getWindowRect",
        "getScreenSize",
        "getScaleFactors",
        "getFrameSize",
        // Inter-script
        "runScript",
        "call",
        // Utilities
        "notify",
        // File ops (manifest-scoped)
        "readStoreFile",
        "writeStoreFile",
        "listStoreFiles",
        // File ops (system-level)
        "readFile",
        "writeFile",
        "listFiles",
        "fileExists",
        // Network
        "httpGet",
        "httpPost",
        // Storage
        "storageGet",
        "storageSet",
        "storageDelete",
        "storageKeys",
        // Post (async job)
        "postCapture",
        "postClick",
        "postSwipe",
        "postKeyPress",
        "postOcr",
        "postFindTemplate",
        "postWaitForTemplate",
        // Aliases
        "screenshot",
        "matchTemplate",
    ];
    for key in keys {
        if let Ok(func) = async_fns.get::<_, rquickjs::Function>(key) {
            ctx_obj.set(key, func)?;
        }
    }

    // jobWait: synchronous blocking wait until job reaches terminal state
    let job_mgr_wait = job_mgr.clone();
    ctx_obj.set(
        "jobWait",
        Func::from(move |id: u64| -> String {
            // Block until job is terminal, checking every 10ms
            loop {
                if let Some(status) = job_mgr_wait.status(id) {
                    match status {
                        JobStatus::Done => {
                            return match job_mgr_wait.result(id) {
                                Some(val) => serde_json::to_string(&val).unwrap_or_else(|_| "null".into()),
                                None => "null".into(),
                            };
                        }
                        JobStatus::Failed => {
                            let err = job_mgr_wait.error(id).unwrap_or_else(|| "Unknown error".into());
                            return format!("__ERROR__:{}", err);
                        }
                        JobStatus::Cancelled => {
                            return "__ERROR__:Job cancelled".into();
                        }
                        _ => {
                            // Still pending/running, wait a bit
                            std::thread::sleep(std::time::Duration::from_millis(10));
                        }
                    }
                } else {
                    return "__ERROR__:Job not found".into();
                }
            }
        }),
    )?;

    globals.set("ctx", ctx_obj)?;

    // ━━━ Plugin proxy: ctx.plugin.<pluginId>.<method>(...args) ━━━
    // Uses a JS Proxy that intercepts property access to lazily create
    // per-plugin sub-proxies. Each sub-proxy intercepts method calls and
    // routes them through __invoke('pluginCall', ...).
    let plugin_proxy_code = r#"
    (function() {
        var pluginCache = {};
        ctx.plugin = new Proxy({}, {
            get: function(target, prop) {
                if (typeof prop === 'symbol') return undefined;
                if (prop === 'then') return undefined; // avoid Promise detection
                if (pluginCache[prop]) return pluginCache[prop];
                var pluginId = prop;
                var pluginProxy = new Proxy({}, {
                    get: function(_, method) {
                        if (typeof method === 'symbol') return undefined;
                        if (method === 'then') return undefined;
                        return function() {
                            var args = Array.prototype.slice.call(arguments);
                            var p = new Promise(function(resolve, reject) {
                                try {
                                    var result = __invoke('pluginCall', JSON.stringify([pluginId, method, args]));
                                    if (typeof result === 'string' && result.indexOf('__ERROR__:') === 0) {
                                        reject(new Error(result.substring(10)));
                                    } else {
                                        resolve(JSON.parse(result));
                                    }
                                } catch(e) {
                                    reject(e);
                                }
                            });
                            return p;
                        };
                    }
                });
                pluginCache[prop] = pluginProxy;
                return pluginProxy;
            }
        });
        ctx.pluginList = function() {
            var p = new Promise(function(resolve, reject) {
                try {
                    var result = __invoke('pluginList', '[]');
                    if (typeof result === 'string' && result.indexOf('__ERROR__:') === 0) {
                        reject(new Error(result.substring(10)));
                    } else {
                        resolve(JSON.parse(result));
                    }
                } catch(e) {
                    reject(e);
                }
            });
            return p;
        };
    })()
    "#;
    if let Err(e) = js_ctx.eval::<(), _>(plugin_proxy_code) {
        tracing::warn!("Failed to inject plugin proxy: {}", e);
    }

    // Library export helper: assigns to both `exports` and `__libraryExports` (pure JS, no __invoke).
    let library_register_code = r#"
    (function () {
        function registerLibrary(name, fn) {
            if (typeof name !== "string" || name.length === 0) {
                throw new Error("registerLibrary: name must be a non-empty string");
            }
            if (typeof fn !== "function") {
                throw new Error("registerLibrary: fn must be a function");
            }
            globalThis.__libraryExports = globalThis.__libraryExports || {};
            globalThis.exports = globalThis.exports || {};
            globalThis.exports[name] = fn;
            globalThis.__libraryExports[name] = fn;
        }
        ctx.registerLibrary = registerLibrary;
        globalThis.registerLibrary = registerLibrary;
    })()
    "#;
    js_ctx.eval::<(), _>(library_register_code)?;

    // Register console global (QuickJS has no built-in console)
    let console_code = r#"
    (function() {
        var c = {};
        c.log = function() {
            var msg = Array.prototype.slice.call(arguments).map(String).join(" ");
            ctx.logInfo(msg);
        };
        c.warn = function() {
            var msg = Array.prototype.slice.call(arguments).map(String).join(" ");
            ctx.logWarn(msg);
        };
        c.error = function() {
            var msg = Array.prototype.slice.call(arguments).map(String).join(" ");
            ctx.logError(msg);
        };
        c.info = function() {
            var msg = Array.prototype.slice.call(arguments).map(String).join(" ");
            ctx.logInfo(msg);
        };
        return c;
    })()
    "#;
    let console_obj: Object = js_ctx.eval(console_code)?;
    globals.set("console", console_obj)?;

    Ok(())
}

/// Shut down the async bridge thread (call once on app exit).
///
/// After this call, any pending `__invoke` calls will return an error.
/// This is optional — the thread will also be cleaned up on process exit.
pub fn shutdown_bridge() {
    if let Some(guard) = BRIDGE.get() {
        if let Ok(mut bridge) = guard.lock() {
            *bridge = None;
        }
    }
}

/// Run [`dispatch_ctx_method`] on a fresh thread + single-thread runtime so nested
/// `__invoke` does not block the global bridge consumer (avoids deadlock).
fn dispatch_ctx_method_nested_blocking(
    ctx: Arc<dyn ScriptContext>,
    method: String,
    args: Vec<serde_json::Value>,
    job_mgr: JobManager,
) -> std::result::Result<serde_json::Value, String> {
    let handle = std::thread::Builder::new()
        .name("qjs-nested-invoke".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| e.to_string())?;
            rt.block_on(async move { dispatch_ctx_method(&ctx, &method, &args, &job_mgr).await })
        })
        .map_err(|e| e.to_string())?;
    match handle.join() {
        Ok(r) => r,
        Err(_) => Err("nested ctx invoke thread panicked".to_string()),
    }
}

/// Synchronous dispatch: send async work to a dedicated bridge thread.
///
/// `block_in_place` panics when called from within a `tokio::time::timeout`
/// or other nested async context — which is exactly where QuickJS callbacks
/// run. Instead, we send the async closure to a dedicated background thread
/// that owns its own tokio runtime, and wait synchronously on a channel.
///
/// When `__invoke` nests (e.g. `ctx.call` into a library that awaits `ctx.click`),
/// the bridge thread is still blocked on the outer dispatch — nested calls must
/// use [`dispatch_ctx_method_nested_blocking`] instead of queueing another task.
fn invoke_ctx_method_sync(
    ctx_slot: &ScriptCtxSlot,
    cancel_slot: &CancelTokenSlot,
    method: &str,
    args_json: &str,
    job_mgr: &JobManager,
) -> std::result::Result<serde_json::Value, String> {
    let depth = INVOKE_SYNC_DEPTH.fetch_add(1, Ordering::SeqCst) + 1;
    let _depth_guard = InvokeSyncDepthGuard;

    let perf_mode = bridge_perf_mode();
    let perf_timer = match perf_mode {
        BridgePerfMode::Off => None,
        BridgePerfMode::Steps | BridgePerfMode::Verbose => Some(Instant::now()),
    };
    let parse_start = Instant::now();
    let args: Vec<serde_json::Value> = if args_json == "[]" {
        Vec::new()
    } else {
        serde_json::from_str(args_json).unwrap_or_default()
    };
    let parse_ms = parse_start.elapsed().as_secs_f64() * 1000.0;

    let ctx: Arc<dyn ScriptContext> = ctx_slot
        .read()
        .ok()
        .and_then(|g| g.clone())
        .ok_or_else(|| wrap_invoke_err(method, &[], "ScriptContext not set".into()))?;

    let method_owned = method.to_string();

    let dispatch_start = Instant::now();
    let outcome = if depth > 1 {
        dispatch_ctx_method_nested_blocking(ctx, method_owned.clone(), args, job_mgr.clone())
    } else {
        // Send the async work to the bridge thread and wait for the result.
        let (resp_tx, resp_rx) = mpsc::channel::<std::result::Result<serde_json::Value, String>>();

        let bridge_tx = get_bridge_sender();
        let method_for_task = method_owned.clone();
        let job_mgr_owned = job_mgr.clone();
        bridge_tx
            .send(Box::new(move || {
                let result = tokio::runtime::Handle::current()
                    .block_on(async { dispatch_ctx_method(&ctx, &method_for_task, &args, &job_mgr_owned).await });
                let _ = resp_tx.send(result);
            }))
            .map_err(|_| "Bridge thread unavailable".to_string())?;

        // Use recv_timeout in a loop so we can check the cancellation token.
        // This allows force-stop to interrupt a blocked __invoke call within ~50ms.
        let timeout = std::time::Duration::from_millis(50);
        loop {
            match resp_rx.recv_timeout(timeout) {
                Ok(result) => break result,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // Check if the script has been cancelled
                    let cancelled = cancel_slot
                        .read()
                        .ok()
                        .and_then(|g| g.clone())
                        .map(|t| t.is_cancelled())
                        .unwrap_or(false);
                    if cancelled {
                        break Err("Script cancelled".to_string());
                    }
                    // Not cancelled, keep waiting
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    break Err("Bridge thread dropped without response".to_string());
                }
            }
        }
    };
    let dispatch_ms = dispatch_start.elapsed().as_secs_f64() * 1000.0;

    if let Some(started) = perf_timer {
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        match perf_mode {
            BridgePerfMode::Off => {}
            BridgePerfMode::Verbose => {
                tracing::info!(
                    target: "betternte_perf",
                    method = method_owned.as_str(),
                    ms = ms,
                    parse_ms = parse_ms,
                    dispatch_ms = dispatch_ms,
                    ok = outcome.is_ok(),
                    "ctx_invoke"
                );
            }
            BridgePerfMode::Steps => {
                let thr = bridge_slow_threshold_ms();
                if ms >= thr {
                    tracing::info!(
                        target: "betternte_perf",
                        method = method_owned.as_str(),
                        ms = ms,
                        parse_ms = parse_ms,
                        dispatch_ms = dispatch_ms,
                        ok = outcome.is_ok(),
                        slow_threshold_ms = thr,
                        "ctx_invoke_slow"
                    );
                }
            }
        }
    }

    outcome
}

/// Dispatch a void async call: ctx.method(args...).await → null
macro_rules! dispatch_void {
    ($ctx:expr, $method:ident $(, $args:expr)*) => {
        $ctx.$method($($args),*).await.map(|_| serde_json::Value::Null)
    };
}

/// Dispatch an async call returning String → JSON string.
macro_rules! dispatch_str {
    ($ctx:expr, $method:ident $(, $args:expr)*) => {
        $ctx.$method($($args),*).await.map(serde_json::Value::String)
    };
}

/// Dispatch an async call returning bool → JSON bool.
macro_rules! dispatch_bool {
    ($ctx:expr, $method:ident $(, $args:expr)*) => {
        $ctx.$method($($args),*).await.map(serde_json::Value::Bool)
    };
}

/// Dispatch an async call returning a serde-serializable value.
macro_rules! dispatch_serde {
    ($ctx:expr, $method:ident $(, $args:expr)*) => {
        $ctx.$method($($args),*).await
            .map(|v| serde_json::to_value(v).unwrap_or(serde_json::Value::Null))
    };
}

/// Dispatch an async call returning Option<T>, mapping None → null.
macro_rules! dispatch_opt_json {
    ($ctx:expr, $method:ident, $map:expr $(, $args:expr)*) => {
        $ctx.$method($($args),*).await
            .map(|m| match m {
                Some(v) => ($map)(v),
                None => serde_json::Value::Null,
            })
    };
}

/// Dispatch a single ctx method call to the async ScriptContext.
async fn dispatch_ctx_method(
    ctx: &Arc<dyn ScriptContext>,
    method: &str,
    args: &[serde_json::Value],
    job_mgr: &JobManager,
) -> std::result::Result<serde_json::Value, String> {
    if let Err(e) = ctx.check_manifest_api_permission(method) {
        return Err(e);
    }

    // Helper: template match result → JSON
    let tmpl_to_json = |m: crate::engine::MatchResult| {
        serde_json::json!({"x": m.x, "y": m.y, "width": m.width, "height": m.height, "confidence": m.confidence})
    };

    match method {
        // ━━━ Capture ━━━
        "capture" => {
            let force = args.first().and_then(|v| v.as_bool()).unwrap_or(false);
            ctx.capture(force).await.map(|f| {
                serde_json::json!({"width": f.width, "height": f.height, "data_len": f.data.len()})
            })
        }
        "captureRegion" => {
            let (x, y, w, h) = (arg_i32(args, 0), arg_i32(args, 1), arg_u32(args, 2), arg_u32(args, 3));
            let force = args.get(4).and_then(|v| v.as_bool()).unwrap_or(false);
            ctx.capture_region(&Region { x, y, width: w, height: h }, force).await
                .map(|f| serde_json::json!({"width": f.width, "height": f.height}))
        }
        "saveScreenshot" => {
            let force = args.first().and_then(|v| v.as_bool()).unwrap_or(false);
            ctx.save_screenshot(force).await.map(serde_json::Value::String)
        }

        // ━━━ Recognition ━━━
        "findTemplate" => {
            let opts: Option<FindTemplateOpts> = args.get(1).and_then(|v| serde_json::from_value(v.clone()).ok());
            dispatch_opt_json!(ctx, find_template, tmpl_to_json, &arg_str(args, 0), opts)
        }
        "findTemplates" => {
            let opts: Option<FindTemplateOpts> = args.get(1).and_then(|v| serde_json::from_value(v.clone()).ok());
            dispatch_serde!(ctx, find_templates, &arg_str(args, 0), opts)
        }
        "findTemplateBatch" => {
            let entries: Vec<FindTemplateBatchEntry> = args.get(0)
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| serde_json::from_value(v.clone()).ok()).collect())
                .unwrap_or_default();
            ctx.find_template_batch(&entries).await.map(|results| {
                serde_json::Value::Array(results.into_iter().map(|m| match m {
                    Some(v) => tmpl_to_json(v),
                    None => serde_json::Value::Null,
                }).collect())
            })
        }
        "ocr" => {
            let (x, y, w, h) = (arg_i32(args, 0), arg_i32(args, 1), arg_u32(args, 2), arg_u32(args, 3));
            let opts = args.get(4).cloned().unwrap_or(serde_json::Value::Null);
            let text_color = opts.get("textColor").and_then(|v| v.as_str()).map(|s| s.to_string());
            let text_color_tolerance = opts.get("textColorTolerance").and_then(|v| v.as_u64()).unwrap_or(32) as u8;
            ctx.ocr(&Region { x, y, width: w, height: h }, text_color.as_deref(), text_color_tolerance).await.map(serde_json::Value::String)
        }
        "ocrAll" => dispatch_serde!(ctx, ocr_all),
        "getColor" => dispatch_str!(ctx, get_color, arg_i32(args, 0), arg_i32(args, 1)),
        "getColors" => {
            let points: Vec<(i32, i32)> = args
                .first()
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| {
                            let a = v.as_array()?;
                            let x = a.first()?.as_i64()? as i32;
                            let y = a.get(1)?.as_i64()? as i32;
                            Some((x, y))
                        })
                        .collect()
                })
                .unwrap_or_default();
            ctx.get_colors(&points).await
                .map(|v| serde_json::to_value(v).unwrap_or(serde_json::Value::Null))
        }
        "colorMatch" => {
            dispatch_bool!(ctx, color_match, arg_i32(args, 0), arg_i32(args, 1), &arg_str(args, 2), arg_u8(args, 3))
        }
        "colorMatchAll" => {
            let points = arg_color_points(args, 0);
            let opts = resolve_color_match_all_opts(args);
            let r = ctx.color_match_all(&points, &opts).await.map_err(|e| e.to_string())?;
            Ok(if opts.debug {
                serde_json::to_value(&r).unwrap_or(serde_json::Value::Null)
            } else {
                serde_json::Value::Bool(r.all_match)
            })
        }
        "scanSliderStrip" => {
            let opts = args.get(0).cloned().unwrap_or(serde_json::Value::Null);
            ctx.scan_slider_strip(&opts).await
        }
        "scanStripEdges" => {
            let opts = args.get(0).cloned().unwrap_or(serde_json::Value::Null);
            ctx.scan_strip_edges(&opts).await
        }
        "countColor" => {
            let color = arg_str(args, 0);
            let opts = args.get(1).cloned();
            ctx.count_color(&color, opts.as_ref()).await.map(|n| serde_json::Value::Number(n.into()))
        }

        // ━━━ Input ━━━
        "click"           => dispatch_void!(ctx, click, arg_i32(args, 0), arg_i32(args, 1)),
        "doubleClick"     => dispatch_void!(ctx, double_click, arg_i32(args, 0), arg_i32(args, 1)),
        "rightClick"      => dispatch_void!(ctx, right_click, arg_i32(args, 0), arg_i32(args, 1)),
        "mouseMove"       => dispatch_void!(ctx, mouse_move, arg_i32(args, 0), arg_i32(args, 1)),
        "mouseDown"       => dispatch_void!(ctx, mouse_down, &arg_str(args, 0)),
        "mouseUp"         => dispatch_void!(ctx, mouse_up, &arg_str(args, 0)),
        "scroll"          => dispatch_void!(ctx, scroll, arg_i32(args, 0)),
        "swipe"           => dispatch_void!(ctx, swipe, arg_i32(args, 0), arg_i32(args, 1), arg_i32(args, 2), arg_i32(args, 3), arg_u32(args, 4)),
        "keyDown"         => dispatch_void!(ctx, key_down, &arg_str(args, 0)),
        "keyUp"           => dispatch_void!(ctx, key_up, &arg_str(args, 0)),
        "keyPress"        => dispatch_void!(ctx, key_press, &arg_str(args, 0), args.get(1).and_then(|v| v.as_u64()).map(|v| v as u32)),
        "keyCombo"        => dispatch_void!(ctx, key_combo, &arg_str_array(args, 0)),
        "typeText"        => dispatch_void!(ctx, type_text, &arg_str(args, 0)),

        // ━━━ Wait (time-based) ━━━
        "sleep"           => dispatch_void!(ctx, sleep, arg_u64(args, 0)),
        "waitForTemplate" => {
            let opts: Option<FindTemplateOpts> = args.get(2).and_then(|v| serde_json::from_value(v.clone()).ok());
            dispatch_opt_json!(ctx, wait_for_template, tmpl_to_json, &arg_str(args, 0), arg_u64(args, 1), opts)
        }
        "waitGone"        => dispatch_bool!(ctx, wait_gone, &arg_str(args, 0), arg_u64(args, 1)),
        "waitForColor"    => dispatch_bool!(ctx, wait_for_color, arg_i32(args, 0), arg_i32(args, 1), &arg_str(args, 2), arg_u64(args, 3)),

        // ━━━ Wait (frame-based) ━━━
        "sleepFrames"           => dispatch_void!(ctx, sleep_frames, arg_u32(args, 0)),
        "waitForTemplateFrames" => {
            let opts: Option<FindTemplateOpts> = args.get(2).and_then(|v| serde_json::from_value(v.clone()).ok());
            dispatch_opt_json!(ctx, wait_for_template_frames, tmpl_to_json, &arg_str(args, 0), arg_u32(args, 1), opts)
        }
        "waitGoneFrames"        => dispatch_bool!(ctx, wait_gone_frames, &arg_str(args, 0), arg_u32(args, 1)),
        "waitForColorFrames"    => dispatch_bool!(ctx, wait_for_color_frames, arg_i32(args, 0), arg_i32(args, 1), &arg_str(args, 2), arg_u32(args, 3)),

        // ━━━ Window ━━━
        "findWindow"      => dispatch_opt_json!(ctx, find_window, |h| serde_json::json!(h), &arg_str(args, 0)),
        "activateWindow"  => dispatch_void!(ctx, activate_window, arg_u64(args, 0)),
        "getWindowRect"   => {
            ctx.get_window_rect(arg_u64(args, 0)).await
                .map(|r| serde_json::json!({"x": r.x, "y": r.y, "width": r.width, "height": r.height}))
        }
        "getScreenSize"   => {
            ctx.get_screen_size().await.map(|(w, h)| serde_json::json!([w, h]))
        }
        "getScaleFactors" => {
            Ok(match ctx.get_scale_factors() {
                Some((sx, sy)) => serde_json::json!({"scaleX": sx, "scaleY": sy}),
                None => serde_json::Value::Null,
            })
        }
        "getFrameSize"    => {
            Ok(match ctx.get_frame_size() {
                Some((w, h)) => serde_json::json!({"width": w, "height": h}),
                None => serde_json::Value::Null,
            })
        }

        // ━━━ Inter-script ━━━
        "runScript" => {
            let params = args.get(1).cloned().unwrap_or(serde_json::Value::Null);
            ctx.run_script(&arg_str(args, 0), params).await
        }
        "call" => {
            let args_val = args.get(2).cloned().unwrap_or(serde_json::Value::Null);
            ctx.call_library(&arg_str(args, 0), &arg_str(args, 1), args_val).await
        }

        // ━━━ Utilities ━━━
        "notify" => dispatch_void!(ctx, notify, &arg_str(args, 0), &arg_str(args, 1)),

        // ━━━ File ops (manifest-scoped) ━━━
        "readStoreFile"   => dispatch_str!(ctx, read_store_file, &arg_str(args, 0)),
        "writeStoreFile"  => dispatch_void!(ctx, write_store_file, &arg_str(args, 0), &arg_str(args, 1)),
        "listStoreFiles"  => dispatch_serde!(ctx, list_store_files, &arg_str(args, 0)),

        // ━━━ File ops (system-level) ━━━
        "readFile"        => dispatch_str!(ctx, read_file, &arg_str(args, 0)),
        "writeFile"       => dispatch_void!(ctx, write_file, &arg_str(args, 0), &arg_str(args, 1)),
        "listFiles"       => dispatch_serde!(ctx, list_files, &arg_str(args, 0)),
        "fileExists"      => dispatch_bool!(ctx, file_exists, &arg_str(args, 0)),

        // ━━━ Network ━━━
        "httpGet"         => dispatch_str!(ctx, http_get, &arg_str(args, 0)),
        "httpPost"        => dispatch_str!(ctx, http_post, &arg_str(args, 0), &arg_str(args, 1)),

        // ━━━ Storage ━━━
        "storageGet"      => {
            ctx.storage_get(&arg_str(args, 0)).await.map(|v| v.unwrap_or(serde_json::Value::Null))
        }
        "storageSet"      => {
            let value = args.get(1).cloned().unwrap_or(serde_json::Value::Null);
            dispatch_void!(ctx, storage_set, &arg_str(args, 0), value)
        }
        "storageDelete"   => dispatch_void!(ctx, storage_delete, &arg_str(args, 0)),
        "storageKeys"     => dispatch_serde!(ctx, storage_keys),

        // ━━━ Plugin ━━━
        "pluginCall" => {
            let plugin_id = arg_str(args, 0);
            let method = arg_str(args, 1);
            // args[2] is a JSON Array (not a string) — serialize it back to JSON
            let args_json = args.get(2)
                .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "[]".into()))
                .unwrap_or_else(|| "[]".into());
            let result = ctx.plugin_call(&plugin_id, &method, &args_json).await.map_err(|e| e.to_string())?;
            Ok(serde_json::from_str(&result).unwrap_or(serde_json::Value::Null))
        }
        "pluginList" => {
            let result = ctx.plugin_list().await.map_err(|e| e.to_string())?;
            Ok(serde_json::from_str(&result).unwrap_or(serde_json::Value::Array(vec![])))
        }

        // ━━━ Post (async job) ━━━
        "postCapture" => {
            let force = args.first().and_then(|v| v.as_bool()).unwrap_or(false);
            let (id, _cancel) = job_mgr.create();
            let jm = job_mgr.clone();
            jm.spawn(id, ctx.clone(), move |c| async move {
                c.capture(force).await
                    .map(|f| serde_json::json!({"width": f.width, "height": f.height, "data_len": f.data.len()}))
                    .map_err(|e| e.to_string())
            });
            Ok(serde_json::json!(id))
        }
        "postClick" => {
            let (x, y) = (arg_i32(args, 0), arg_i32(args, 1));
            let (id, _cancel) = job_mgr.create();
            let jm = job_mgr.clone();
            jm.spawn(id, ctx.clone(), move |c| async move {
                c.click(x, y).await.map(|_| serde_json::Value::Null).map_err(|e| e.to_string())
            });
            Ok(serde_json::json!(id))
        }
        "postSwipe" => {
            let (x1, y1, x2, y2, dur) = (arg_i32(args, 0), arg_i32(args, 1), arg_i32(args, 2), arg_i32(args, 3), arg_u32(args, 4));
            let (id, _cancel) = job_mgr.create();
            let jm = job_mgr.clone();
            jm.spawn(id, ctx.clone(), move |c| async move {
                c.swipe(x1, y1, x2, y2, dur).await.map(|_| serde_json::Value::Null).map_err(|e| e.to_string())
            });
            Ok(serde_json::json!(id))
        }
        "postKeyPress" => {
            let key = arg_str(args, 0);
            let dur = args.get(1).and_then(|v| v.as_u64()).map(|v| v as u32);
            let (id, _cancel) = job_mgr.create();
            let jm = job_mgr.clone();
            jm.spawn(id, ctx.clone(), move |c| async move {
                c.key_press(&key, dur).await.map(|_| serde_json::Value::Null).map_err(|e| e.to_string())
            });
            Ok(serde_json::json!(id))
        }
        "postOcr" => {
            let (x, y, w, h) = (arg_i32(args, 0), arg_i32(args, 1), arg_u32(args, 2), arg_u32(args, 3));
            let (id, _cancel) = job_mgr.create();
            let jm = job_mgr.clone();
            let opts = args.get(4).cloned().unwrap_or(serde_json::Value::Null);
            let text_color = opts.get("textColor").and_then(|v| v.as_str()).map(|s| s.to_string());
            let text_color_tolerance = opts.get("textColorTolerance").and_then(|v| v.as_u64()).unwrap_or(32) as u8;
            jm.spawn(id, ctx.clone(), move |c| async move {
                c.ocr(&Region { x, y, width: w, height: h }, text_color.as_deref(), text_color_tolerance).await
                    .map(serde_json::Value::String)
                    .map_err(|e| e.to_string())
            });
            Ok(serde_json::json!(id))
        }
        "postFindTemplate" => {
            let name = arg_str(args, 0);
            let opts: Option<FindTemplateOpts> = args.get(1).and_then(|v| serde_json::from_value(v.clone()).ok());
            let (id, _cancel) = job_mgr.create();
            let jm = job_mgr.clone();
            jm.spawn(id, ctx.clone(), move |c| async move {
                c.find_template(&name, opts).await
                    .map(|m| match m {
                        Some(m) => serde_json::json!({"x": m.x, "y": m.y, "width": m.width, "height": m.height, "confidence": m.confidence}),
                        None => serde_json::Value::Null,
                    })
                    .map_err(|e| e.to_string())
            });
            Ok(serde_json::json!(id))
        }
        "postWaitForTemplate" => {
            let name = arg_str(args, 0);
            let timeout_ms = arg_u64(args, 1);
            let opts: Option<FindTemplateOpts> = args.get(2).and_then(|v| serde_json::from_value(v.clone()).ok());
            let (id, cancel) = job_mgr.create();
            let jm = job_mgr.clone();
            jm.spawn(id, ctx.clone(), move |c| async move {
                // Poll cancellation token alongside the actual work
                let cancel_fut = async {
                    loop {
                        if cancel.is_cancelled() {
                            return;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    }
                };
                tokio::select! {
                    r = c.wait_for_template(&name, timeout_ms, opts) => {
                        r.map(|m| match m {
                            Some(m) => serde_json::json!({"x": m.x, "y": m.y, "width": m.width, "height": m.height, "confidence": m.confidence}),
                            None => serde_json::Value::Null,
                        }).map_err(|e| e.to_string())
                    }
                    _ = cancel_fut => Err("Job cancelled".into()),
                }
            });
            Ok(serde_json::json!(id))
        }

        _ => Err(anyhow::anyhow!("Unknown ctx method")),
    }.map_err(|e| wrap_invoke_err(method, args, e.to_string()))
}

/// Format a concise summary of arguments for error messages.
fn format_args_summary(args: &[serde_json::Value]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(64);
    out.push('[');
    for (i, v) in args.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        match v {
            serde_json::Value::String(s) => {
                if s.len() > 32 {
                    let _ = write!(out, "\"{}...\"({} chars)", &s[..32], s.len());
                } else {
                    let _ = write!(out, "\"{}\"", s);
                }
            }
            serde_json::Value::Number(n) => { let _ = write!(out, "{}", n); }
            serde_json::Value::Bool(b) => { let _ = write!(out, "{}", b); }
            serde_json::Value::Null => out.push_str("null"),
            serde_json::Value::Array(a) => { let _ = write!(out, "[{} items]", a.len()); }
            serde_json::Value::Object(_) => out.push_str("{...}"),
        }
    }
    out.push(']');
    out
}

/// Wrap an error with method name and argument context.
fn wrap_invoke_err(method: &str, args: &[serde_json::Value], err: String) -> String {
    format!("ctx.{}{}: {}", method, format_args_summary(args), err)
}

// ━━━ Argument parsing helpers ━━━

fn arg_str(args: &[serde_json::Value], idx: usize) -> String {
    args.get(idx)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}
fn arg_i32(args: &[serde_json::Value], idx: usize) -> i32 {
    args.get(idx).and_then(|v| v.as_i64()).unwrap_or(0) as i32
}
fn arg_u32(args: &[serde_json::Value], idx: usize) -> u32 {
    args.get(idx).and_then(|v| v.as_u64()).unwrap_or(0) as u32
}
fn arg_u64(args: &[serde_json::Value], idx: usize) -> u64 {
    args.get(idx).and_then(|v| v.as_u64()).unwrap_or(0)
}
fn arg_u8(args: &[serde_json::Value], idx: usize) -> u8 {
    args.get(idx).and_then(|v| v.as_u64()).unwrap_or(0) as u8
}
fn arg_u8_default(args: &[serde_json::Value], idx: usize, default: u8) -> u8 {
    args.get(idx)
        .and_then(|v| v.as_u64())
        .map(|v| v as u8)
        .unwrap_or(default)
}
/// Second argument: options object (`{ defaultTolerance, debug, shiftMax }`), or legacy number + optional bool + optional shift object.
fn resolve_color_match_all_opts(args: &[serde_json::Value]) -> ColorMatchAllOpts {
    match args.get(1) {
        None => ColorMatchAllOpts::default(),
        Some(v) if v.is_object() => serde_json::from_value(v.clone()).unwrap_or_default(),
        Some(v) if v.is_number() => ColorMatchAllOpts {
            default_tolerance: arg_u8_default(args, 1, 32),
            default_rgba_tolerance: None,
            debug: args.get(2).and_then(|b| b.as_bool()).unwrap_or(false),
            shift_max: args.get(3).and_then(|o| {
                if o.is_object() {
                    serde_json::from_value(o.clone()).ok()
                } else {
                    None
                }
            }),
        },
        Some(_) => ColorMatchAllOpts::default(),
    }
}
fn arg_str_array(args: &[serde_json::Value], idx: usize) -> Vec<String> {
    args.get(idx)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(ToString::to_string))
                .collect()
        })
        .unwrap_or_default()
}
fn arg_color_points(args: &[serde_json::Value], idx: usize) -> Vec<ColorMatchPoint> {
    args.get(idx)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| serde_json::from_value::<ColorMatchPoint>(v.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}
