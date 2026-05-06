//! Opt-in timing logs for comparing runtime behavior before/after optimizations.
//!
//! Enable with environment variable `BETTERNTE_PERF_LOG`:
//! - `1`, `true`, `yes`, `steps` — log per-step breakdown; script bridge logs only slow calls
//!   (see `BETTERNTE_PERF_BRIDGE_SLOW_MS`, default 5.0).
//! - `2`, `verbose`, `all`, `full` — same as above, plus every `ctx` bridge invoke from scripts.
//!
//! Filter tracing output, for example:
//! `RUST_LOG=betternte_perf=info`
//!
//! Log lines use target `betternte_perf` so they can be enabled without raising global log noise.

use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PerfLogMode {
    Off,
    /// Flow step breakdown; JS bridge logs invokes slower than the threshold only.
    Steps,
    /// Flow step breakdown + every JS `ctx` bridge invoke.
    Verbose,
}

pub fn mode() -> PerfLogMode {
    static MODE: OnceLock<PerfLogMode> = OnceLock::new();
    *MODE.get_or_init(|| match std::env::var("BETTERNTE_PERF_LOG") {
        Ok(v) => {
            let v = v.to_ascii_lowercase();
            match v.as_str() {
                "1" | "true" | "yes" | "steps" => PerfLogMode::Steps,
                "2" | "verbose" | "all" | "full" => PerfLogMode::Verbose,
                _ => PerfLogMode::Off,
            }
        }
        Err(_) => PerfLogMode::Off,
    })
}

pub fn is_enabled() -> bool {
    matches!(mode(), PerfLogMode::Steps | PerfLogMode::Verbose)
}

pub fn log_flow_interrupt(flow_id: &str, step_id: &str, interrupt_check_ms: f64, to: &str) {
    if !is_enabled() {
        return;
    }
    tracing::info!(
        target: "betternte_perf",
        flow_id = flow_id,
        step_id = step_id,
        interrupt_check_ms = interrupt_check_ms,
        interrupt_to = to,
        "flow_perf_interrupt"
    );
}

#[allow(clippy::too_many_arguments)]
pub fn log_flow_step_success(
    flow_id: &str,
    step_id: &str,
    interrupt_check_ms: f64,
    attempts: u32,
    resolve_input_ms: f64,
    step_executor_ms: f64,
    apply_output_ms: f64,
    find_next_step_ms: f64,
) {
    if !is_enabled() {
        return;
    }
    tracing::info!(
        target: "betternte_perf",
        flow_id = flow_id,
        step_id = step_id,
        interrupt_check_ms = interrupt_check_ms,
        attempts = attempts,
        resolve_input_ms = resolve_input_ms,
        step_executor_ms = step_executor_ms,
        apply_output_ms = apply_output_ms,
        find_next_step_ms = find_next_step_ms,
        "flow_step_perf"
    );
}

pub fn log_flow_step_error(
    flow_id: &str,
    step_id: &str,
    interrupt_check_ms: f64,
    attempts: u32,
    resolve_input_ms: f64,
    step_executor_ms: f64,
) {
    if !is_enabled() {
        return;
    }
    tracing::info!(
        target: "betternte_perf",
        flow_id = flow_id,
        step_id = step_id,
        interrupt_check_ms = interrupt_check_ms,
        attempts = attempts,
        resolve_input_ms = resolve_input_ms,
        step_executor_ms = step_executor_ms,
        "flow_step_perf_error"
    );
}
