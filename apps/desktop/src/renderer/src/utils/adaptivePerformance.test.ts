import { describe, expect, it } from 'vitest';
import { AdaptivePerformanceController, normalizeAdaptiveInputTimestamp, resolveAdaptiveHqDelayMs, resolveAdaptiveMotionOverscanPx, resolveResourcePressure, shouldAllowAdaptivePrefetch } from './adaptivePerformance';

const idleDiagnostics = {
  queuedPageRenders: 0,
  queuedThumbnailRenders: 0,
  inflightPageRenders: 0,
  inflightThumbnailRenders: 0,
};

const frameTimes = new WeakMap<AdaptivePerformanceController, number>();

function feedFrames(controller: AdaptivePerformanceController, duration: number, count = 140): void {
  let frameAt = frameTimes.get(controller) ?? 0;
  for (let index = 0; index < count; index += 1) {
    frameAt += duration;
    controller.observeFrame(frameAt);
  }
  frameTimes.set(controller, frameAt);
}

describe('adaptive performance controller', () => {
  it('detects a 120 Hz budget and stays at maximum quality with headroom', () => {
    const controller = new AdaptivePerformanceController();
    feedFrames(controller, 8.33);
    expect(controller.evaluate(idleDiagnostics, null)).toMatchObject({
      level: 0,
      detectedRefreshHz: 120,
      targetFrameMs: 8.333,
      reason: 'headroom',
    });
  });

  it('drops directly to the stability tier when the 60 fps floor is missed repeatedly', () => {
    const controller = new AdaptivePerformanceController();
    feedFrames(controller, 8.33);
    controller.evaluate(idleDiagnostics, null);
    feedFrames(controller, 28);
    expect(controller.evaluate(idleDiagnostics, null)).toMatchObject({ level: 3, reason: 'stability' });
  });

  it('treats delayed input presentation as pressure even before average frame cadence collapses', () => {
    const controller = new AdaptivePerformanceController();
    let frameAt = 0;
    for (let index = 0; index < 80; index += 1) {
      frameAt += 8.33;
      controller.observeInput(frameAt - 35);
      controller.observeFrame(frameAt);
    }
    expect(controller.evaluate(idleDiagnostics, null)).toMatchObject({
      level: 2,
      reason: 'input-latency',
      p95InputLatencyMs: 35,
    });
  });

  it('uses render and resource pressure before frames collapse', () => {
    const controller = new AdaptivePerformanceController();
    feedFrames(controller, 8.33);
    const resources = {
      capturedAt: 1,
      totalMemoryKiB: 1000,
      reclaimableMemoryKiB: 30,
      appWorkingSetKiB: 200,
      appCpuPercent: 20,
      systemCpuUsagePercent: 50,
      displayRefreshHz: 120,
    };
    expect(controller.evaluate({ ...idleDiagnostics, queuedPageRenders: 3 }, resources).level).toBe(1);
    expect(controller.evaluate({ ...idleDiagnostics, queuedPageRenders: 3 }, resources).level).toBe(2);
  });

  it('recovers more slowly than it degrades', () => {
    const controller = new AdaptivePerformanceController();
    feedFrames(controller, 28);
    expect(controller.evaluate(idleDiagnostics, null).level).toBe(3);
    for (let index = 0; index < 5; index += 1) {
      feedFrames(controller, 8.33);
      expect(controller.evaluate(idleDiagnostics, null).level).toBe(3);
    }
    feedFrames(controller, 8.33);
    expect(controller.evaluate(idleDiagnostics, null).level).toBe(2);
  });
});

describe('adaptive rendering policy', () => {
  it('keeps the target page ahead of other visible pages at every pressure level', () => {
    expect(resolveAdaptiveHqDelayMs({ level: 0, isTargetPage: true, viewportInMotion: false })).toBe(0);
    expect(resolveAdaptiveHqDelayMs({ level: 0, isTargetPage: false, viewportInMotion: false })).toBe(24);
    expect(resolveAdaptiveHqDelayMs({ level: 3, isTargetPage: true, viewportInMotion: true })).toBe(96);
    expect(resolveAdaptiveHqDelayMs({ level: 3, isTargetPage: false, viewportInMotion: true })).toBe(160);
  });

  it('reduces speculative work only under measured pressure', () => {
    expect(resolveAdaptiveMotionOverscanPx(0)).toBe(1200);
    expect(resolveAdaptiveMotionOverscanPx(3)).toBe(600);
    expect(shouldAllowAdaptivePrefetch(1)).toBe(true);
    expect(shouldAllowAdaptivePrefetch(2)).toBe(false);
  });

  it('treats low reclaimable memory as supporting pressure evidence', () => {
    expect(resolveResourcePressure({
      capturedAt: 1,
      totalMemoryKiB: 1000,
      reclaimableMemoryKiB: 30,
      appWorkingSetKiB: 100,
      appCpuPercent: 10,
      systemCpuUsagePercent: 20,
      displayRefreshHz: 60,
    })).toBe(1);
  });

  it('rejects incompatible platform event timestamps', () => {
    expect(normalizeAdaptiveInputTimestamp(980, 1000)).toBe(980);
    expect(normalizeAdaptiveInputTimestamp(1_700_000_000_000, 1000)).toBe(1000);
    expect(normalizeAdaptiveInputTimestamp(0, 1000)).toBe(1000);
  });
});
