export type CadRenderExperiment =
  | 'current-control'
  | 'stable-detail-crop-prototype';

const CAD_RENDER_EXPERIMENTS = new Set<CadRenderExperiment>([
  'current-control',
  'stable-detail-crop-prototype',
]);

const DEFAULT_CAD_RENDER_EXPERIMENT: CadRenderExperiment = 'current-control';
const DEFAULT_IMPORTANT_PREVIEW_RATIO = 0.68;
export const DEFAULT_DETAIL_QUALITY_DELAY_MS = 1200;
export const STABLE_DETAIL_QUALITY_DELAY_MS = 960;

export interface CadRenderExperimentConfig {
  readonly name: CadRenderExperiment;
  readonly importantPreviewRatio: number;
  readonly targetCropPrototype: boolean;
  readonly detailQualityDelayMs: number;
}

export function resolveCadRenderExperimentConfig(): CadRenderExperimentConfig {
  const rawName = window.butterPaper?.environment.cadRenderExperiment ?? null;
  const name = CAD_RENDER_EXPERIMENTS.has(rawName as CadRenderExperiment)
    ? rawName as CadRenderExperiment
    : DEFAULT_CAD_RENDER_EXPERIMENT;

  return {
    name,
    importantPreviewRatio: DEFAULT_IMPORTANT_PREVIEW_RATIO,
    targetCropPrototype: name === 'stable-detail-crop-prototype',
    detailQualityDelayMs: name === 'stable-detail-crop-prototype'
      ? STABLE_DETAIL_QUALITY_DELAY_MS
      : DEFAULT_DETAIL_QUALITY_DELAY_MS,
  };
}

export function isCadRenderExperiment(value: string | null | undefined): value is CadRenderExperiment {
  return CAD_RENDER_EXPERIMENTS.has(value as CadRenderExperiment);
}
