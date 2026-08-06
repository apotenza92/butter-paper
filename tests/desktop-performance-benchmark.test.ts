import { describe, expect, it } from 'vitest';
import { assertBenchmarkRendererIdentity, summarizeSamples, summarizeScenario } from '../scripts/benchmark-desktop-performance.mjs';

describe('desktop performance benchmark summaries', () => {
  it('reports min, median, p95, and max deterministically', () => {
    expect(summarizeSamples([5, 1, 3, 2, 4])).toEqual({ min: 1, median: 3, p95: 5, max: 5 });
    expect(summarizeSamples([])).toBeNull();
  });

  it('summarises launch, render, heap, working-set, cache, and long-task metrics', () => {
    const sample = {
      startupTargetMs: 100,
      documentReadyMs: 250,
      rendererHeapAfterGcBytes: 8 * 1024 * 1024,
      processMetrics: { totalWorkingSetKiB: 256 * 1024 },
      diagnostics: {
        renderCacheBytes: 2 * 1024 * 1024,
        thumbnailCacheBytes: 1024 * 1024,
        openStageTimings: { totalOpenMs: 90 },
      },
      perf: {
        firstPageImageVisibleMs: 200,
        firstPageFullVisibleMs: 225,
        longTasks: { count: 1, maxDuration: 55 },
      },
    };

    expect(summarizeScenario([sample])).toMatchObject({
      startupTargetMs: { median: 100 },
      documentReadyMs: { median: 250 },
      rendererHeapAfterGcMiB: { median: 8 },
      totalWorkingSetMiB: { median: 256 },
      renderCacheMiB: { median: 2 },
      thumbnailCacheMiB: { median: 1 },
      maxLongTaskMs: { median: 55 },
    });
  });

  it('rejects stale, foreign, and generic Electron renderers', () => {
    const source = {
      version: '0.0.18',
      commit: 'a'.repeat(40),
      branch: 'codex/recover-v0.0.18-nonsigning',
      dirty: false,
      checkoutId: 'b'.repeat(64),
      statusFingerprint: 'c'.repeat(64),
    };
    const title = 'Butter Paper Dev · codex/recover-v0.0.18-nonsigning@aaaaaaaa';
    const identity = {
      hasAppRoot: true,
      href: 'file:///butter-paper/index.html',
      title,
      metadata: { ...source, development: true, windowTitle: title },
    };
    expect(assertBenchmarkRendererIdentity(identity, source)).toMatchObject(source);
    expect(() => assertBenchmarkRendererIdentity({ ...identity, hasAppRoot: false }, source)).toThrow(/non-Butter Paper/);
    expect(() => assertBenchmarkRendererIdentity({
      ...identity,
      metadata: { ...identity.metadata, commit: 'd'.repeat(40) },
    }, source)).toThrow(/provenance mismatch for commit/);
  });
});
