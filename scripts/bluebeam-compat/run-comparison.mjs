#!/usr/bin/env node
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { createCanvas, loadImage } from '@napi-rs/canvas';
import {
  analyzeOutlineContinuity, basicSsim, boundaryDistance, comparisonHeatmap, comparisonOverlay,
  connectedComponents, createExclusionMask, createInkMask, cropImage, encodePgm,
  excludeMaskPixels, luminanceDifference, registerMasks, translateImage,
} from './image-analysis.mjs';
import { assertComparableManifests, verifyManifestArtifacts } from './manifest.mjs';
import { writeCompatibilityReports } from './report.mjs';
import { loadToolContract, validateInspectedComponents, validateOperationResults } from './tool-contract.mjs';

export async function runComparison({ baselineManifestPath, candidateManifestPath, operationResultsPath, candidateInspectionPath, outputDirectory, comparisonMode = 'same-environment' }) {
  const [baseline, candidate, operationResults, contract, candidateInspection] = await Promise.all([
    readJson(baselineManifestPath), readJson(candidateManifestPath), readJson(operationResultsPath), loadToolContract(),
    candidateInspectionPath ? readJson(candidateInspectionPath) : undefined,
  ]);
  // This deliberately precedes decoding or metric work: stale/incompatible baselines are never compared.
  assertComparableManifests(baseline, candidate, { mode: comparisonMode });
  const [baselineProvenance, candidateProvenance] = await Promise.all([
    verifyManifestArtifacts(baseline, baselineManifestPath),
    verifyManifestArtifacts(candidate, candidateManifestPath),
  ]);
  const operationSummary = validateOperationResults(contract, operationResults.results ?? operationResults);
  const expectedTools = candidate.expectedTools ?? [];
  if (expectedTools.length > 0 && !candidateInspection) {
    throw new Error('A candidate structural inspection is required when expectedTools are declared');
  }
  const structuralSummary = expectedTools.length > 0
    ? validateInspectedComponents(contract, expectedTools, candidateInspection)
    : { passed: true, missing: [], unexpected: [] };
  const specimens = [];
  await mkdir(outputDirectory, { recursive: true });
  for (const baselineRoi of baseline.rois ?? []) {
    const candidateRoi = candidate.rois.find((roi) => roi.id === baselineRoi.id);
    if (!candidateRoi) throw new Error(`Candidate manifest has no ROI named ${baselineRoi.id}`);
    const baselineImage = await decodeImage(resolve(dirname(baselineManifestPath), baselineRoi.image));
    const candidateImage = await decodeImage(resolve(dirname(candidateManifestPath), candidateRoi.image));
    const left = cropImage(baselineImage, baselineRoi);
    const right = cropImage(candidateImage, candidateRoi);
    if (left.width !== right.width || left.height !== right.height) throw new Error(`ROI dimensions differ for ${baselineRoi.id}`);
    const exclusionMask = await loadExclusionMask(baselineRoi, baselineManifestPath, baselineImage);
    const candidateExclusionMask = await loadExclusionMask(candidateRoi, candidateManifestPath, candidateImage);
    if (!equalMasks(exclusionMask, candidateExclusionMask)) throw provenanceError(`ROI ${baselineRoi.id} exclusion-mask pixels differ between manifests`);
    const leftMask = excludeMaskPixels(createInkMask(left, baselineRoi.mask), exclusionMask);
    const rightMask = excludeMaskPixels(createInkMask(right, candidateRoi.mask), exclusionMask);
    const registration = registerMasks(leftMask, rightMask, { maximumOffset: baselineRoi.maximumRegistrationOffset ?? 8, exclusionMask });
    const boundary = boundaryDistance(leftMask, rightMask, registration);
    const components = { baseline: connectedComponents(leftMask), candidate: connectedComponents(rightMask) };
    const registeredCandidate = translateImage(right, registration);
    const continuity = {
      baseline: analyzeOutlineContinuity(leftMask, baselineRoi.continuity),
      candidate: analyzeOutlineContinuity(rightMask, candidateRoi.continuity),
    };
    const metrics = {
      iou: registration.iou,
      registration: { x: registration.x, y: registration.y },
      boundary,
      ssim: basicSsim(left, registeredCandidate, { exclusionMask }),
      luminance: luminanceDifference(left, registeredCandidate, { exclusionMask }),
      components,
      continuity,
    };
    const thresholds = baselineRoi.thresholds ?? {};
    const failures = thresholdFailures(metrics, thresholds);
    const artifactDirectory = join(outputDirectory, safeName(baselineRoi.id));
    await mkdir(artifactDirectory, { recursive: true });
    const artifacts = {
      baseline: `${safeName(baselineRoi.id)}/baseline.png`,
      candidate: `${safeName(baselineRoi.id)}/candidate.png`,
      candidateRegistered: `${safeName(baselineRoi.id)}/candidate-registered.png`,
      heatmap: `${safeName(baselineRoi.id)}/heatmap.png`,
      overlay: `${safeName(baselineRoi.id)}/overlay.png`,
      baselineMask: `${safeName(baselineRoi.id)}/baseline-mask.png`,
      candidateMask: `${safeName(baselineRoi.id)}/candidate-mask.png`,
      metrics: `${safeName(baselineRoi.id)}/metrics.json`,
    };
    await Promise.all([
      writeFile(join(artifactDirectory, 'baseline-mask.pgm'), encodePgm(leftMask)),
      writeFile(join(artifactDirectory, 'candidate-mask.pgm'), encodePgm(rightMask)),
      writeFile(join(artifactDirectory, 'baseline.png'), encodePng(left)),
      writeFile(join(artifactDirectory, 'candidate.png'), encodePng(right)),
      writeFile(join(artifactDirectory, 'candidate-registered.png'), encodePng(registeredCandidate)),
      writeFile(join(artifactDirectory, 'heatmap.png'), encodePng(comparisonHeatmap(left, registeredCandidate, { exclusionMask }))),
      writeFile(join(artifactDirectory, 'overlay.png'), encodePng(comparisonOverlay(leftMask, rightMask, registration, exclusionMask))),
      writeFile(join(artifactDirectory, 'baseline-mask.png'), encodePng(maskImage(leftMask))),
      writeFile(join(artifactDirectory, 'candidate-mask.png'), encodePng(maskImage(rightMask))),
      writeFile(join(artifactDirectory, 'metrics.json'), `${JSON.stringify({ id: baselineRoi.id, metrics, thresholds, failures }, null, 2)}\n`),
    ]);
    specimens.push({ id: baselineRoi.id, passed: failures.length === 0, failure: failures.join('; '), metrics, thresholds, artifacts });
  }
  for (const key of operationSummary.missing) specimens.push({ id: `operation:${key}`, passed: false, failure: 'missing operation result' });
  for (const key of operationSummary.failed) specimens.push({ id: `operation:${key}`, passed: false, failure: 'operation failed' });
  for (const key of structuralSummary.missing) specimens.push({ id: `structure:missing:${key}`, passed: false, failure: 'missing native component' });
  for (const key of structuralSummary.unexpected) specimens.push({ id: `structure:unexpected:${key}`, passed: false, failure: 'unexpected native component' });
  const passed = specimens.every((item) => item.passed) && operationSummary.passed && structuralSummary.passed;
  const report = {
    schema: 'butter-paper/bluebeam-compat-report', version: 1, passed,
    summary: passed ? 'All compatibility checks passed.' : 'One or more compatibility checks failed.',
    comparisonMode, baselineManifest: baselineManifestPath, candidateManifest: candidateManifestPath,
    provenance: { baseline: baselineProvenance, candidate: candidateProvenance },
    operationSummary, structuralSummary, specimens,
  };
  await writeCompatibilityReports(outputDirectory, report);
  return report;
}

function thresholdFailures(metrics, thresholds) {
  const failures = [];
  if (thresholds.minimumIoU !== undefined && metrics.iou < thresholds.minimumIoU) failures.push(`IoU ${metrics.iou} < ${thresholds.minimumIoU}`);
  if (thresholds.minimumSsim !== undefined && metrics.ssim < thresholds.minimumSsim) failures.push(`SSIM ${metrics.ssim} < ${thresholds.minimumSsim}`);
  if (thresholds.maximumMeanBoundaryDistance !== undefined && metrics.boundary.mean > thresholds.maximumMeanBoundaryDistance) failures.push(`mean boundary ${metrics.boundary.mean} > ${thresholds.maximumMeanBoundaryDistance}`);
  if (thresholds.maximumP95BoundaryDistance !== undefined && metrics.boundary.p95 > thresholds.maximumP95BoundaryDistance) failures.push(`p95 boundary ${metrics.boundary.p95} > ${thresholds.maximumP95BoundaryDistance}`);
  if (thresholds.maximumHausdorffDistance !== undefined && metrics.boundary.hausdorff > thresholds.maximumHausdorffDistance) failures.push(`Hausdorff ${metrics.boundary.hausdorff} > ${thresholds.maximumHausdorffDistance}`);
  if (thresholds.maximumComponents !== undefined && metrics.components.candidate.length > thresholds.maximumComponents) failures.push(`components ${metrics.components.candidate.length} > ${thresholds.maximumComponents}`);
  if (thresholds.expectedComponentCount !== undefined && metrics.components.candidate.length !== thresholds.expectedComponentCount) failures.push(`components ${metrics.components.candidate.length} != ${thresholds.expectedComponentCount}`);
  if (thresholds.maximumDisconnectedPixels !== undefined && metrics.continuity.candidate.disconnectedPixels > thresholds.maximumDisconnectedPixels) failures.push(`disconnected pixels ${metrics.continuity.candidate.disconnectedPixels} > ${thresholds.maximumDisconnectedPixels}`);
  if (thresholds.maximumOutlineGap !== undefined && metrics.continuity.candidate.maximumNearestComponentGap > thresholds.maximumOutlineGap) failures.push(`outline gap ${metrics.continuity.candidate.maximumNearestComponentGap} > ${thresholds.maximumOutlineGap}`);
  if (thresholds.minimumLargestComponentRatio !== undefined && metrics.continuity.candidate.largestComponentRatio < thresholds.minimumLargestComponentRatio) failures.push(`largest component ratio ${metrics.continuity.candidate.largestComponentRatio} < ${thresholds.minimumLargestComponentRatio}`);
  if (thresholds.minimumEnclosedBackgroundRegions !== undefined && metrics.continuity.candidate.enclosedBackgroundRegions < thresholds.minimumEnclosedBackgroundRegions) failures.push(`enclosed background regions ${metrics.continuity.candidate.enclosedBackgroundRegions} < ${thresholds.minimumEnclosedBackgroundRegions}`);
  if (thresholds.maximumMeanLuminanceError !== undefined && metrics.luminance.meanAbsoluteError > thresholds.maximumMeanLuminanceError) failures.push(`mean luminance error ${metrics.luminance.meanAbsoluteError} > ${thresholds.maximumMeanLuminanceError}`);
  if (thresholds.maximumLuminanceRmse !== undefined && metrics.luminance.rootMeanSquareError > thresholds.maximumLuminanceRmse) failures.push(`luminance RMSE ${metrics.luminance.rootMeanSquareError} > ${thresholds.maximumLuminanceRmse}`);
  return failures;
}

async function decodeImage(path) {
  const source = await loadImage(path);
  const canvas = createCanvas(source.width, source.height);
  const context = canvas.getContext('2d');
  context.drawImage(source, 0, 0);
  const pixels = context.getImageData(0, 0, source.width, source.height);
  return { width: source.width, height: source.height, data: new Uint8Array(pixels.data) };
}

async function loadExclusionMask(roi, manifestPath, fullImage) {
  const config = roi.exclusionMask;
  if (!config?.image) return undefined;
  const decoded = await decodeImage(resolve(dirname(manifestPath), config.image));
  const image = decoded.width === roi.width && decoded.height === roi.height ? decoded : cropImage(decoded, roi);
  if (image.width !== roi.width || image.height !== roi.height) throw new Error(`Exclusion mask dimensions differ for ${roi.id}`);
  return createExclusionMask(image, config);
}

function equalMasks(left, right) {
  if (!left && !right) return true;
  if (!left || !right || left.width !== right.width || left.height !== right.height) return false;
  return left.data.every((value, index) => value === right.data[index]);
}

function encodePng(image) {
  const canvas = createCanvas(image.width, image.height);
  const context = canvas.getContext('2d');
  const pixels = context.createImageData(image.width, image.height);
  pixels.data.set(image.data);
  context.putImageData(pixels, 0, 0);
  return canvas.toBuffer('image/png');
}

function maskImage(mask) {
  const data = new Uint8Array(mask.width * mask.height * 4);
  for (let index = 0; index < mask.data.length; index += 1) data.set(mask.data[index] ? [0, 0, 0, 255] : [255, 255, 255, 255], index * 4);
  return { width: mask.width, height: mask.height, data };
}

function provenanceError(message) {
  const error = new Error(message);
  error.code = 'BLUEBEAM_PROVENANCE_INVALID';
  return error;
}

async function readJson(path) {
  return JSON.parse(await readFile(path, 'utf8'));
}

function safeName(value) {
  return value.replace(/[^a-z0-9._-]+/gi, '-');
}

function parseArguments(values) {
  const result = {};
  for (let index = 0; index < values.length; index += 2) {
    const key = values[index]?.replace(/^--/, ''); const value = values[index + 1];
    if (!key || !value) throw new Error('Expected --baseline-manifest, --candidate-manifest, --operation-results, and --output');
    result[key] = resolve(value);
  }
  return {
    baselineManifestPath: result['baseline-manifest'], candidateManifestPath: result['candidate-manifest'],
    operationResultsPath: result['operation-results'], candidateInspectionPath: result['candidate-inspection'], outputDirectory: result.output,
    comparisonMode: result['comparison-mode'] ?? 'same-environment',
  };
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  runComparison(parseArguments(process.argv.slice(2))).then((report) => {
    process.exitCode = report.passed ? 0 : 1;
  }).catch((error) => {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 2;
  });
}
