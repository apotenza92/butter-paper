use std::{cmp::Ordering, collections::VecDeque, time::Instant};

const FRAME_WINDOW_SIZE: usize = 120;
const MINIMUM_FRAME_SAMPLES: usize = 12;
const RECOVERY_EVALUATIONS: usize = 6;
const DISPLAY_REFRESH_RATES: [f64; 7] = [60., 90., 100., 120., 144., 165., 240.];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ViewerRenderDiagnostics {
    pub queued_page_renders: usize,
    pub queued_thumbnail_renders: usize,
    pub inflight_page_renders: usize,
    pub inflight_thumbnail_renders: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdaptiveResourceSnapshot {
    pub total_memory_kib: u64,
    pub reclaimable_memory_kib: u64,
    pub app_working_set_kib: u64,
    pub app_cpu_percent: f64,
    pub system_cpu_usage_percent: Option<f64>,
    pub display_refresh_hz: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AdaptivePerformanceReason {
    #[default]
    Headroom,
    FrameTime,
    InputLatency,
    RenderBacklog,
    ResourcePressure,
    Stability,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdaptivePerformanceSnapshot {
    pub level: usize,
    pub detected_refresh_hz: u32,
    pub target_frame_ms: f64,
    pub p95_frame_ms: f64,
    pub p95_input_latency_ms: f64,
    pub frame_pressure: f64,
    pub render_pressure: f64,
    pub resource_pressure: f64,
    pub reason: AdaptivePerformanceReason,
}

impl Default for AdaptivePerformanceSnapshot {
    fn default() -> Self {
        Self {
            level: 0,
            detected_refresh_hz: 60,
            target_frame_ms: 1_000. / 60.,
            p95_frame_ms: 0.,
            p95_input_latency_ms: 0.,
            frame_pressure: 0.,
            render_pressure: 0.,
            resource_pressure: 0.,
            reason: AdaptivePerformanceReason::Headroom,
        }
    }
}

/// Application-owned port of Electron's adaptive viewer-performance policy.
///
/// GPUI does not expose the platform event timestamp or a portable resource
/// sampler at the pinned revision. Input latency therefore starts when GPUI
/// receives an event, and callers pass `None` for resources until a reviewed
/// platform capability supplies them.
#[derive(Default)]
pub struct AdaptiveViewerPerformance {
    frame_durations_ms: VecDeque<f64>,
    previous_frame_at: Option<Instant>,
    pending_input_at: Option<Instant>,
    input_latencies_ms: VecDeque<f64>,
    level: usize,
    recovery_evaluations: usize,
    snapshot: AdaptivePerformanceSnapshot,
}

impl AdaptiveViewerPerformance {
    pub fn observe_frame(&mut self, frame_at: Instant) {
        if let Some(input_at) = self.pending_input_at
            && frame_at >= input_at
        {
            push_bounded(
                &mut self.input_latencies_ms,
                frame_at.duration_since(input_at).as_secs_f64() * 1_000.,
            );
            self.pending_input_at = None;
        }
        if let Some(previous) = self.previous_frame_at
            && frame_at >= previous
        {
            let duration_ms = frame_at.duration_since(previous).as_secs_f64() * 1_000.;
            if duration_ms.is_finite() && duration_ms > 0. && duration_ms <= 100. {
                push_bounded(&mut self.frame_durations_ms, duration_ms);
            }
        }
        self.previous_frame_at = Some(frame_at);
    }

    pub fn observe_input(&mut self, input_at: Instant) {
        if self
            .pending_input_at
            .is_none_or(|pending| input_at < pending)
        {
            self.pending_input_at = Some(input_at);
        }
    }

    pub fn reset_frames(&mut self) {
        self.frame_durations_ms.clear();
        self.previous_frame_at = None;
        self.pending_input_at = None;
    }

    pub fn evaluate(
        &mut self,
        diagnostics: ViewerRenderDiagnostics,
    ) -> AdaptivePerformanceSnapshot {
        self.evaluate_with_resources(diagnostics, None)
    }

    pub fn evaluate_with_resources(
        &mut self,
        diagnostics: ViewerRenderDiagnostics,
        resources: Option<AdaptiveResourceSnapshot>,
    ) -> AdaptivePerformanceSnapshot {
        if self.frame_durations_ms.len() < MINIMUM_FRAME_SAMPLES {
            return self.snapshot;
        }

        let mut sorted_frames = self.frame_durations_ms.iter().copied().collect::<Vec<_>>();
        sorted_frames.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        let observed_interval = percentile(&sorted_frames, 0.2);
        let target_frame_ms = resources
            .and_then(|snapshot| snapshot.display_refresh_hz)
            .filter(|refresh| *refresh > 0.)
            .map(|refresh| 1_000. / refresh)
            .unwrap_or_else(|| resolve_display_frame_interval(observed_interval));
        let p95_frame_ms = percentile(&sorted_frames, 0.95);
        let mut sorted_input = self.input_latencies_ms.iter().copied().collect::<Vec<_>>();
        sorted_input.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        let p95_input_latency_ms = if sorted_input.is_empty() {
            0.
        } else {
            percentile(&sorted_input, 0.95)
        };
        let target_miss_ratio = ratio_above(&self.frame_durations_ms, target_frame_ms * 1.25);
        let sixty_fps_miss_ratio = ratio_above(&self.frame_durations_ms, 1_000. / 60. * 1.08);
        let frame_pressure = clamp01(
            ((p95_frame_ms / target_frame_ms - 1.) / 1.5)
                .max(target_miss_ratio * 2.)
                .max(sixty_fps_miss_ratio * 3.)
                .max(p95_input_latency_ms / 50.),
        );
        let render_pressure = resolve_render_pressure(diagnostics);
        let resource_pressure = resolve_resource_pressure(resources);
        let desired_level = resolve_desired_level(
            p95_frame_ms,
            target_frame_ms,
            target_miss_ratio,
            sixty_fps_miss_ratio,
            render_pressure,
            resource_pressure,
            p95_input_latency_ms,
        );

        if desired_level > self.level {
            self.level = if desired_level == 3 || p95_input_latency_ms >= 33. {
                desired_level
            } else {
                (self.level + 1).min(3)
            };
            self.recovery_evaluations = 0;
        } else if desired_level < self.level {
            self.recovery_evaluations += 1;
            if self.recovery_evaluations >= RECOVERY_EVALUATIONS {
                self.level = self.level.saturating_sub(1);
                self.recovery_evaluations = 0;
            }
        } else {
            self.recovery_evaluations = 0;
        }

        let reason = if self.level == 3 {
            AdaptivePerformanceReason::Stability
        } else if p95_input_latency_ms >= 20_f64.max(target_frame_ms * 2.) {
            AdaptivePerformanceReason::InputLatency
        } else if resource_pressure >= render_pressure.max(0.35) {
            AdaptivePerformanceReason::ResourcePressure
        } else if render_pressure >= 0.35 {
            AdaptivePerformanceReason::RenderBacklog
        } else if p95_frame_ms > target_frame_ms * 1.2 {
            AdaptivePerformanceReason::FrameTime
        } else {
            AdaptivePerformanceReason::Headroom
        };
        self.snapshot = AdaptivePerformanceSnapshot {
            level: self.level,
            detected_refresh_hz: (1_000. / target_frame_ms).round() as u32,
            target_frame_ms: round(target_frame_ms),
            p95_frame_ms: round(p95_frame_ms),
            p95_input_latency_ms: round(p95_input_latency_ms),
            frame_pressure: round(frame_pressure),
            render_pressure: round(render_pressure),
            resource_pressure: round(resource_pressure),
            reason,
        };
        self.snapshot
    }

    pub const fn current(&self) -> AdaptivePerformanceSnapshot {
        self.snapshot
    }
}

fn push_bounded(values: &mut VecDeque<f64>, value: f64) {
    values.push_back(value);
    while values.len() > FRAME_WINDOW_SIZE {
        values.pop_front();
    }
}

fn resolve_render_pressure(diagnostics: ViewerRenderDiagnostics) -> f64 {
    let queued = diagnostics.queued_page_renders + diagnostics.queued_thumbnail_renders;
    let inflight = diagnostics.inflight_page_renders + diagnostics.inflight_thumbnail_renders;
    if queued >= 6 || inflight >= 8 {
        1.
    } else if queued >= 3 || inflight >= 5 {
        0.7
    } else if queued >= 1 || inflight >= 3 {
        0.35
    } else {
        0.
    }
}

fn resolve_resource_pressure(resources: Option<AdaptiveResourceSnapshot>) -> f64 {
    let Some(resources) = resources.filter(|snapshot| snapshot.total_memory_kib > 0) else {
        return 0.;
    };
    let reclaimable = resources.reclaimable_memory_kib as f64 / resources.total_memory_kib as f64;
    let app_memory = resources.app_working_set_kib as f64 / resources.total_memory_kib as f64;
    let memory: f64 = if reclaimable < 0.04 || app_memory > 0.35 {
        1.
    } else if reclaimable < 0.08 || app_memory > 0.25 {
        0.7
    } else if reclaimable < 0.14 || app_memory > 0.15 {
        0.35
    } else {
        0.
    };
    let system_cpu = resources.system_cpu_usage_percent.map_or(0., |cpu| {
        if cpu >= 95. {
            1.
        } else if cpu >= 85. {
            0.7
        } else if cpu >= 70. {
            0.35
        } else {
            0.
        }
    });
    let app_cpu = if resources.app_cpu_percent >= 300. {
        0.7
    } else if resources.app_cpu_percent >= 150. {
        0.35
    } else {
        0.
    };
    memory.max(system_cpu).max(app_cpu)
}

#[allow(clippy::too_many_arguments)]
fn resolve_desired_level(
    p95_frame_ms: f64,
    target_frame_ms: f64,
    target_miss_ratio: f64,
    sixty_fps_miss_ratio: f64,
    render_pressure: f64,
    resource_pressure: f64,
    p95_input_latency_ms: f64,
) -> usize {
    if p95_frame_ms >= 25.
        || p95_input_latency_ms >= 50.
        || sixty_fps_miss_ratio >= 0.2
        || render_pressure >= 1.
    {
        3
    } else if p95_frame_ms > 1_000. / 60. * 1.08
        || p95_input_latency_ms >= 33.
        || sixty_fps_miss_ratio >= 0.08
        || render_pressure >= 0.7
        || resource_pressure >= 0.7
    {
        2
    } else if p95_frame_ms > target_frame_ms * 1.2
        || p95_input_latency_ms >= 20_f64.max(target_frame_ms * 2.)
        || target_miss_ratio >= 0.12
        || render_pressure >= 0.35
        || resource_pressure >= 0.35
    {
        1
    } else {
        0
    }
}

fn resolve_display_frame_interval(observed_interval: f64) -> f64 {
    DISPLAY_REFRESH_RATES
        .into_iter()
        .map(|refresh| 1_000. / refresh)
        .min_by(|a, b| {
            (a - observed_interval)
                .abs()
                .partial_cmp(&(b - observed_interval).abs())
                .unwrap_or(Ordering::Equal)
        })
        .unwrap_or(1_000. / 60.)
}

fn percentile(sorted: &[f64], ratio: f64) -> f64 {
    sorted[((sorted.len() as f64 * ratio).floor() as usize).min(sorted.len() - 1)]
}

fn ratio_above(values: &VecDeque<f64>, threshold: f64) -> f64 {
    values.iter().filter(|value| **value > threshold).count() as f64 / values.len().max(1) as f64
}

fn clamp01(value: f64) -> f64 {
    value.clamp(0., 1.)
}

fn round(value: f64) -> f64 {
    (value * 1_000.).round() / 1_000.
}
