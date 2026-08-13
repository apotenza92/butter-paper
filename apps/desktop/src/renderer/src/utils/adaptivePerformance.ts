import type { DesktopPerformanceResourcesSnapshot } from '../../../shared/protocol';
import type { DiagnosticsSnapshot } from '../services/documentSession';

export type AdaptivePerformanceLevel = 0 | 1 | 2 | 3;

export interface AdaptivePerformanceSnapshot {
  readonly level: AdaptivePerformanceLevel;
  readonly detectedRefreshHz: number;
  readonly targetFrameMs: number;
  readonly p95FrameMs: number;
  readonly p95InputLatencyMs: number;
  readonly framePressure: number;
  readonly renderPressure: number;
  readonly resourcePressure: number;
  readonly reason: 'headroom' | 'frame-time' | 'input-latency' | 'render-backlog' | 'resource-pressure' | 'stability';
}

type RenderDiagnostics = Pick<
  DiagnosticsSnapshot,
  'queuedPageRenders' | 'queuedThumbnailRenders' | 'inflightPageRenders' | 'inflightThumbnailRenders'
>;

const DISPLAY_REFRESH_RATES = [60, 90, 100, 120, 144, 165, 240] as const;
const FRAME_WINDOW_SIZE = 120;
const RECOVERY_EVALUATIONS = 6;

export class AdaptivePerformanceController {
  private frameDurations: number[] = [];
  private previousFrameAt: number | null = null;
  private pendingInputAt: number | null = null;
  private inputLatencies: number[] = [];
  private level: AdaptivePerformanceLevel = 0;
  private recoveryEvaluations = 0;
  private detectedFrameIntervalMs: number | null = null;
  private snapshot: AdaptivePerformanceSnapshot = {
    level: 0,
    detectedRefreshHz: 60,
    targetFrameMs: 1000 / 60,
    p95FrameMs: 0,
    p95InputLatencyMs: 0,
    framePressure: 0,
    renderPressure: 0,
    resourcePressure: 0,
    reason: 'headroom',
  };

  observeFrame(frameAt: number): void {
    if (this.pendingInputAt !== null && frameAt >= this.pendingInputAt) {
      this.inputLatencies.push(frameAt - this.pendingInputAt);
      if (this.inputLatencies.length > FRAME_WINDOW_SIZE) {
        this.inputLatencies.splice(0, this.inputLatencies.length - FRAME_WINDOW_SIZE);
      }
      this.pendingInputAt = null;
    }
    if (this.previousFrameAt !== null) {
      const duration = frameAt - this.previousFrameAt;
      if (Number.isFinite(duration) && duration > 0 && duration <= 100) {
        this.frameDurations.push(duration);
        if (this.frameDurations.length > FRAME_WINDOW_SIZE) {
          this.frameDurations.splice(0, this.frameDurations.length - FRAME_WINDOW_SIZE);
        }
      }
    }
    this.previousFrameAt = frameAt;
  }

  observeInput(inputAt: number): void {
    if (this.pendingInputAt === null || inputAt < this.pendingInputAt) {
      this.pendingInputAt = inputAt;
    }
  }

  resetFrames(): void {
    this.frameDurations = [];
    this.previousFrameAt = null;
    this.pendingInputAt = null;
  }

  evaluate(
    diagnostics: RenderDiagnostics,
    resources: DesktopPerformanceResourcesSnapshot | null,
  ): AdaptivePerformanceSnapshot {
    if (this.frameDurations.length < 12) {
      return this.snapshot;
    }

    const sortedFrames = [...this.frameDurations].sort((a, b) => a - b);
    const observedInterval = percentile(sortedFrames, 0.2);
    const resourceDisplayRefreshHz = resources?.displayRefreshHz;
    const detectedInterval = resourceDisplayRefreshHz && resourceDisplayRefreshHz > 0
      ? 1000 / resourceDisplayRefreshHz
      : resolveDisplayFrameInterval(observedInterval);
    this.detectedFrameIntervalMs = detectedInterval;

    const targetFrameMs = this.detectedFrameIntervalMs;
    const p95FrameMs = percentile(sortedFrames, 0.95);
    const p95InputLatencyMs = this.inputLatencies.length > 0
      ? percentile([...this.inputLatencies].sort((a, b) => a - b), 0.95)
      : 0;
    const targetMissRatio = ratioAbove(this.frameDurations, targetFrameMs * 1.25);
    const sixtyFpsMissRatio = ratioAbove(this.frameDurations, 1000 / 60 * 1.08);
    const framePressure = clamp01(Math.max(
      (p95FrameMs / targetFrameMs - 1) / 1.5,
      targetMissRatio * 2,
      sixtyFpsMissRatio * 3,
      p95InputLatencyMs / 50,
    ));
    const renderPressure = resolveRenderPressure(diagnostics);
    const resourcePressure = resolveResourcePressure(resources);
    const desiredLevel = resolveDesiredPerformanceLevel({
      p95FrameMs,
      targetFrameMs,
      targetMissRatio,
      sixtyFpsMissRatio,
      renderPressure,
      resourcePressure,
      p95InputLatencyMs,
    });

    if (desiredLevel > this.level) {
      this.level = desiredLevel === 3 || p95InputLatencyMs >= 33
        ? desiredLevel
        : nextLevel(this.level);
      this.recoveryEvaluations = 0;
    } else if (desiredLevel < this.level) {
      this.recoveryEvaluations += 1;
      if (this.recoveryEvaluations >= RECOVERY_EVALUATIONS) {
        this.level = previousLevel(this.level);
        this.recoveryEvaluations = 0;
      }
    } else {
      this.recoveryEvaluations = 0;
    }

    const reason = resolvePressureReason({
      level: this.level,
      p95FrameMs,
      renderPressure,
      resourcePressure,
      p95InputLatencyMs,
      targetFrameMs,
    });
    this.snapshot = {
      level: this.level,
      detectedRefreshHz: Math.round(1000 / targetFrameMs),
      targetFrameMs: round(targetFrameMs),
      p95FrameMs: round(p95FrameMs),
      p95InputLatencyMs: round(p95InputLatencyMs),
      framePressure: round(framePressure),
      renderPressure: round(renderPressure),
      resourcePressure: round(resourcePressure),
      reason,
    };
    return this.snapshot;
  }

  current(): AdaptivePerformanceSnapshot {
    return this.snapshot;
  }
}

export function resolveAdaptiveHqDelayMs({
  level,
  isTargetPage,
  viewportInMotion,
  immediateTargetPromotion = false,
}: {
  level: AdaptivePerformanceLevel;
  isTargetPage: boolean;
  viewportInMotion: boolean;
  immediateTargetPromotion?: boolean;
}): number {
  if (immediateTargetPromotion) {
    return 0;
  }

  const targetDelays = viewportInMotion ? [16, 32, 64, 96] : [0, 8, 24, 48];
  const visibleDelays = viewportInMotion ? [48, 72, 112, 160] : [24, 40, 64, 96];
  return (isTargetPage ? targetDelays : visibleDelays)[level]!;
}

export function resolveAdaptiveMotionOverscanPx(level: AdaptivePerformanceLevel): number {
  return [1200, 1200, 900, 600][level]!;
}

export function shouldAllowAdaptivePrefetch(level: AdaptivePerformanceLevel): boolean {
  return level <= 1;
}

export function normalizeAdaptiveInputTimestamp(eventTimestamp: number, currentTime: number): number {
  if (
    !Number.isFinite(eventTimestamp)
    || eventTimestamp <= 0
    || eventTimestamp > currentTime + 1
    || currentTime - eventTimestamp > 1000
  ) {
    return currentTime;
  }
  return eventTimestamp;
}

export function resolveResourcePressure(resources: DesktopPerformanceResourcesSnapshot | null): number {
  if (!resources || resources.totalMemoryKiB <= 0) {
    return 0;
  }
  const reclaimableRatio = resources.reclaimableMemoryKiB / resources.totalMemoryKiB;
  const appMemoryRatio = resources.appWorkingSetKiB / resources.totalMemoryKiB;
  const memoryPressure = reclaimableRatio < 0.04 || appMemoryRatio > 0.35
    ? 1
    : reclaimableRatio < 0.08 || appMemoryRatio > 0.25
      ? 0.7
      : reclaimableRatio < 0.14 || appMemoryRatio > 0.15
        ? 0.35
        : 0;
  const systemCpuPressure = resources.systemCpuUsagePercent === null
    ? 0
    : resources.systemCpuUsagePercent >= 95
      ? 1
      : resources.systemCpuUsagePercent >= 85
        ? 0.7
        : resources.systemCpuUsagePercent >= 70
          ? 0.35
          : 0;
  const appCpuPressure = resources.appCpuPercent >= 300 ? 0.7 : resources.appCpuPercent >= 150 ? 0.35 : 0;
  return Math.max(memoryPressure, systemCpuPressure, appCpuPressure);
}

function resolveDesiredPerformanceLevel({
  p95FrameMs,
  targetFrameMs,
  targetMissRatio,
  sixtyFpsMissRatio,
  renderPressure,
  resourcePressure,
  p95InputLatencyMs,
}: {
  p95FrameMs: number;
  targetFrameMs: number;
  targetMissRatio: number;
  sixtyFpsMissRatio: number;
  renderPressure: number;
  resourcePressure: number;
  p95InputLatencyMs: number;
}): AdaptivePerformanceLevel {
  if (p95FrameMs >= 25 || p95InputLatencyMs >= 50 || sixtyFpsMissRatio >= 0.2 || renderPressure >= 1) {
    return 3;
  }
  if (p95FrameMs > 1000 / 60 * 1.08 || p95InputLatencyMs >= 33 || sixtyFpsMissRatio >= 0.08 || renderPressure >= 0.7 || resourcePressure >= 0.7) {
    return 2;
  }
  if (p95FrameMs > targetFrameMs * 1.2 || p95InputLatencyMs >= Math.max(20, targetFrameMs * 2) || targetMissRatio >= 0.12 || renderPressure >= 0.35 || resourcePressure >= 0.35) {
    return 1;
  }
  return 0;
}

function resolveRenderPressure(diagnostics: RenderDiagnostics): number {
  const queued = diagnostics.queuedPageRenders + diagnostics.queuedThumbnailRenders;
  const inflight = diagnostics.inflightPageRenders + diagnostics.inflightThumbnailRenders;
  if (queued >= 6 || inflight >= 8) return 1;
  if (queued >= 3 || inflight >= 5) return 0.7;
  if (queued >= 1 || inflight >= 3) return 0.35;
  return 0;
}

function resolvePressureReason({
  level,
  p95FrameMs,
  renderPressure,
  resourcePressure,
  p95InputLatencyMs,
  targetFrameMs,
}: {
  level: AdaptivePerformanceLevel;
  p95FrameMs: number;
  renderPressure: number;
  resourcePressure: number;
  p95InputLatencyMs: number;
  targetFrameMs: number;
}): AdaptivePerformanceSnapshot['reason'] {
  if (level === 3) return 'stability';
  if (p95InputLatencyMs >= Math.max(20, targetFrameMs * 2)) return 'input-latency';
  if (resourcePressure >= Math.max(renderPressure, 0.35)) return 'resource-pressure';
  if (renderPressure >= 0.35) return 'render-backlog';
  if (p95FrameMs > targetFrameMs * 1.2) return 'frame-time';
  return 'headroom';
}

function resolveDisplayFrameInterval(observedInterval: number): number {
  let bestInterval = 1000 / DISPLAY_REFRESH_RATES[0];
  let bestDistance = Number.POSITIVE_INFINITY;
  for (const refreshRate of DISPLAY_REFRESH_RATES) {
    const interval = 1000 / refreshRate;
    const distance = Math.abs(interval - observedInterval);
    if (distance < bestDistance) {
      bestDistance = distance;
      bestInterval = interval;
    }
  }
  return bestInterval;
}

function percentile(sorted: readonly number[], ratio: number): number {
  return sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * ratio))] ?? 0;
}

function ratioAbove(values: readonly number[], threshold: number): number {
  return values.filter((value) => value > threshold).length / Math.max(1, values.length);
}

function nextLevel(level: AdaptivePerformanceLevel): AdaptivePerformanceLevel {
  return Math.min(3, level + 1) as AdaptivePerformanceLevel;
}

function previousLevel(level: AdaptivePerformanceLevel): AdaptivePerformanceLevel {
  return Math.max(0, level - 1) as AdaptivePerformanceLevel;
}

function clamp01(value: number): number {
  return Math.max(0, Math.min(1, value));
}

function round(value: number): number {
  return Math.round(value * 1000) / 1000;
}
