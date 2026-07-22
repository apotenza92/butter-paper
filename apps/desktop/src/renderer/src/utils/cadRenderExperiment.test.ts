import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  DEFAULT_DETAIL_QUALITY_DELAY_MS,
  resolveCadRenderExperimentConfig,
  STABLE_DETAIL_QUALITY_DELAY_MS,
} from './cadRenderExperiment';

describe('CAD render experiment config', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('uses the non-crop control path as the default CAD render policy', () => {
    vi.stubGlobal('window', {
      butterPaper: {
        environment: {},
      },
    });

    expect(resolveCadRenderExperimentConfig()).toMatchObject({
      name: 'current-control',
      targetCropPrototype: false,
      detailQualityDelayMs: DEFAULT_DETAIL_QUALITY_DELAY_MS,
    });
  });

  it('keeps stable detail crop available as a gated experiment', () => {
    vi.stubGlobal('window', {
      butterPaper: {
        environment: {
          cadRenderExperiment: 'stable-detail-crop-prototype',
        },
      },
    });

    expect(resolveCadRenderExperimentConfig()).toMatchObject({
      name: 'stable-detail-crop-prototype',
      targetCropPrototype: true,
      detailQualityDelayMs: STABLE_DETAIL_QUALITY_DELAY_MS,
    });
  });

  it('falls back to the safe default for unknown experiment names', () => {
    vi.stubGlobal('window', {
      butterPaper: {
        environment: {
          cadRenderExperiment: 'old-render-experiment',
        },
      },
    });

    expect(resolveCadRenderExperimentConfig()).toMatchObject({
      name: 'current-control',
      targetCropPrototype: false,
      detailQualityDelayMs: DEFAULT_DETAIL_QUALITY_DELAY_MS,
    });
  });
});
