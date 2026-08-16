import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import {
  assertBenchmarkRendererIdentity,
  alignStartupMilestones,
  deriveStartupPhases,
  summarizeSamples,
  summarizeScenario,
} from '../scripts/benchmark-desktop-performance.mjs';

describe('desktop performance benchmark summaries', () => {
  it('keeps feature-only PDF and phone modules off the critical main-process startup path', () => {
    const windowSource = readFileSync('apps/desktop/src/main/window.ts', 'utf8');
    const blankStoreSource = readFileSync('apps/desktop/src/main/blankPdfTemporaryStore.ts', 'utf8');
    const templateStoreSource = readFileSync('apps/desktop/src/main/pdfTemplateStore.ts', 'utf8');

    expect(windowSource).not.toMatch(/import \{[^}]*loadDocumentPayload[^}]*\} from '.\/pdfSession'/s);
    expect(windowSource).toContain("import('./pdfSession')");
    expect(windowSource).toContain("import('./phoneSignatureTransfer')");
    expect(windowSource).toContain("import('./signatureImageSanitizer')");
    expect(blankStoreSource).not.toContain("from '@butter-paper/pdf/blank'");
    expect(blankStoreSource).toContain("import('@butter-paper/pdf/blank')");
    expect(templateStoreSource).not.toContain("from 'pdf-lib'");
    expect(templateStoreSource).toContain("import('pdf-lib')");
  });

  it('reports min, median, p95, and max deterministically', () => {
    expect(summarizeSamples([5, 1, 3, 2, 4])).toEqual({ min: 1, median: 3, p95: 5, max: 5 });
    expect(summarizeSamples([])).toBeNull();
  });

  it('summarises launch, render, heap, working-set, cache, and long-task metrics', () => {
    const sample = {
      startupTargetMs: 100,
      shellReadyMs: 150,
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
      shellReadyMs: { median: 150 },
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

  it('aligns main-process milestones to process launch time', () => {
    expect(alignStartupMilestones([
      { name: 'main-module-loaded', capturedAtEpochMs: 125, processUptimeMs: 100 },
      { name: 'app-ready', capturedAtEpochMs: 300, processUptimeMs: 275 },
    ], 0)).toEqual({
      mainModuleLoadedMs: 125,
      mainProcessStartedMs: 25,
      appReadyMs: 300,
    });
  });

  it('derives non-overlapping startup phases when their endpoints exist', () => {
    expect(deriveStartupPhases({
      mainProcessStartedMs: 25,
      mainModuleLoadedMs: 120,
      pdfSessionPreparationStartedMs: 125,
      pdfSessionPreparationCompletedMs: 225,
      appReadyMs: 300,
      bootstrapReadyEnteredMs: 302,
      bootstrapStoresCreatedMs: 310,
      bootstrapIpcRegisteredMs: 315,
      bootstrapMenuInstalledMs: 325,
      browserWindowCreatedMs: 340,
      bootstrapWindowLoadRequestedMs: 343,
      rendererNavigationStartedMs: 350,
      preloadModuleEvaluatedMs: 400,
      preloadBridgeExposedMs: 405,
      rendererModuleEvaluatedMs: 500,
      reactRenderRequestedMs: 510,
      reactCommittedMs: 530,
      firstAnimationFrameMs: 545,
      firstPageImageVisibleMs: 650,
      firstPageFullVisibleMs: 700,
    })).toEqual({
      processLaunchToMainStartMs: 25,
      mainProcessModuleLoadMs: 95,
      pdfSessionPreparationMs: 100,
      appReadyWaitMs: 180,
      readyToBootstrapEntryMs: 2,
      bootstrapStoreSetupMs: 8,
      bootstrapIpcRegistrationMs: 5,
      bootstrapMenuInstallMs: 10,
      browserWindowConstructionMs: 15,
      windowLoadRequestMs: 3,
      windowToNavigationMs: 10,
      navigationToPreloadMs: 50,
      preloadBridgeMs: 5,
      rendererModuleEvaluationMs: 95,
      reactCommitMs: 20,
      commitToFirstFrameMs: 15,
      commitToDocumentVisibleMs: 120,
      documentPreviewToFullMs: 50,
    });
  });
});
