use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const GPUI_PERF_WINDOW: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Default)]
pub(in crate::features) struct GpuiPerfContext {
    pub connection_count: usize,
    pub group_count: usize,
    pub flat_row_count: usize,
    pub cache_hit: Option<bool>,
    pub full_shell_paint_count: u64,
    pub surface_paint_count: u64,
    pub left_panel: Option<&'static str>,
    pub right_panel: Option<&'static str>,
    pub resize_active: bool,
}

#[derive(Default)]
struct PerfBucket {
    window_started_at: Option<Instant>,
    samples: Vec<f64>,
}

static GPUI_PERF_ENABLED: OnceLock<bool> = OnceLock::new();
static GPUI_PERF: OnceLock<Mutex<HashMap<&'static str, PerfBucket>>> = OnceLock::new();

pub(in crate::features) fn gpui_perf_enabled() -> bool {
    *GPUI_PERF_ENABLED.get_or_init(|| std::env::var("NYATERM_GPUI_PERF").as_deref() == Ok("1"))
}

pub(in crate::features) fn record_gpui_perf_sample(
    key: &'static str,
    duration: Duration,
    context: GpuiPerfContext,
) {
    if !gpui_perf_enabled() {
        return;
    }
    let now = Instant::now();
    let duration_ms = duration.as_secs_f64() * 1000.0;
    let buckets = GPUI_PERF.get_or_init(|| Mutex::new(HashMap::new()));
    let Ok(mut buckets) = buckets.lock() else {
        return;
    };
    let bucket = buckets.entry(key).or_default();
    let window_started_at = bucket.window_started_at.get_or_insert(now);
    bucket.samples.push(duration_ms);
    if now.duration_since(*window_started_at) < GPUI_PERF_WINDOW {
        return;
    }

    let count = bucket.samples.len();
    if count == 0 {
        bucket.window_started_at = Some(now);
        return;
    }
    bucket.samples.sort_by(|left, right| left.total_cmp(right));
    let sum = bucket.samples.iter().sum::<f64>();
    let avg_ms = sum / count as f64;
    let p95_index = ((count as f64 * 0.95).ceil() as usize)
        .saturating_sub(1)
        .min(count - 1);
    let p95_ms = bucket.samples[p95_index];
    let max_ms = *bucket.samples.last().unwrap_or(&0.0);
    tracing::debug!(
        diagnostic = "gpui_perf",
        key,
        count,
        avg_ms,
        p95_ms,
        max_ms,
        connection_count = context.connection_count,
        group_count = context.group_count,
        flat_row_count = context.flat_row_count,
        cache_hit = context.cache_hit,
        full_shell_paint_count = context.full_shell_paint_count,
        surface_paint_count = context.surface_paint_count,
        left_panel = context.left_panel.unwrap_or(""),
        right_panel = context.right_panel.unwrap_or(""),
        resize_active = context.resize_active,
        "gpui perf sample window"
    );
    bucket.samples.clear();
    bucket.window_started_at = Some(now);
}
