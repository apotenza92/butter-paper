#!/usr/bin/env node

import { copyFile, mkdir, readFile, writeFile } from 'node:fs/promises';
import { resolve, relative, dirname, basename } from 'node:path';
import { pathToFileURL } from 'node:url';
import { createCanvas, loadImage } from '@napi-rs/canvas';
import { createRunManifest, sha256File } from './manifest.mjs';
import { loadToolContract, validateInspectedComponents } from './tool-contract.mjs';
import { ParallelsRevu, hostPathToParallelsGuest } from './parallels-revu.mjs';

export function parseCliOptions(argv, environment = process.env) {
  const options = {
    vmName: environment.BP_PARALLELS_VM ?? 'Windows 11',
    repoRoot: process.cwd(),
    specimen: 'butter-paper-all-tools',
    timeoutMilliseconds: 25_000,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    const value = () => {
      const next = argv[++index];
      if (!next || next.startsWith('--')) throw new Error(`Missing value for ${token}`);
      return next;
    };
    if (token === '--vm') options.vmName = value();
    else if (token === '--pdf') options.pdfPath = value();
    else if (token === '--guest-pdf') options.guestPdfPath = value();
    else if (token === '--output') options.outputDirectory = value();
    else if (token === '--repo') options.repoRoot = value();
    else if (token === '--specimen') options.specimen = value();
    else if (token === '--expected-tools') options.expectedTools = value().split(',').map((item) => item.trim()).filter(Boolean);
    else if (token === '--inspection') options.inspectionPath = value();
    else if (token === '--select') options.select = parseSelection(value());
    else if (token === '--timeout-ms') options.timeoutMilliseconds = Number(value());
    else if (token === '--help') options.help = true;
    else throw new Error(`Unknown option: ${token}`);
  }
  if (options.help) return options;
  if (!options.pdfPath) throw new Error('--pdf is required');
  if (!options.outputDirectory) throw new Error('--output is required');
  if (options.expectedTools?.length && !options.inspectionPath) throw new Error('--inspection is required when --expected-tools is provided');
  if (!Number.isFinite(options.timeoutMilliseconds) || options.timeoutMilliseconds < 1) throw new Error('--timeout-ms must be positive');
  return options;
}

export function createOperationScaffold(contract, observed = []) {
  const evidenceByKey = new Map(observed.map((result) => [`${result.tool}:${result.operation}`, result]));
  return {
    schema: 'butter-paper/bluebeam-operation-results',
    version: 1,
    statusContract: {
      passed: 'Machine-verifiable result or explicitly reviewed evidence',
      'evidence-captured': 'Evidence exists but still needs a human compatibility judgment',
      'not-run': 'Operation was not exercised in this run',
    },
    results: contract.tools.flatMap((tool) => tool.operations.map((operation) => evidenceByKey.get(`${tool.id}:${operation}`) ?? {
      tool: tool.id,
      operation,
      status: 'not-run',
    })),
  };
}

export async function runRevuCapture(options, { controller, validateCapture = validatePngCapture, pause = delay } = {}) {
  const repoRoot = resolve(options.repoRoot);
  const pdfPath = resolve(options.pdfPath);
  const outputDirectory = resolve(options.outputDirectory);
  const screenshotPath = resolve(outputDirectory, 'revu-current.png');
  const openScreenshotPath = resolve(outputDirectory, 'revu-open.png');
  const revu = controller ?? new ParallelsRevu({ vmName: options.vmName });
  const actions = [];
  const perform = async (name, action) => {
    const startedAt = new Date().toISOString();
    try {
      const details = await action();
      actions.push({ name, status: 'passed', startedAt, completedAt: new Date().toISOString(), details });
      return details;
    } catch (error) {
      actions.push({ name, status: 'failed', startedAt, completedAt: new Date().toISOString(), error: String(error?.message ?? error) });
      throw error;
    }
  };
  const captureWithRetry = async (capturePath, expectedResolution) => {
    let lastError;
    for (let attempt = 1; attempt <= 6; attempt += 1) {
      await revu.capture(capturePath);
      try {
        const stats = await validateCapture(capturePath, expectedResolution);
        return { file: basename(capturePath), attempt, stats };
      } catch (error) {
        lastError = error;
        if (attempt < 6) await pause(1000);
      }
    }
    throw lastError;
  };

  await mkdir(outputDirectory, { recursive: true });
  let guestPdfPath = options.guestPdfPath;
  let stagedPdfPath;
  let sourceGuestPath;
  if (!guestPdfPath) {
    stagedPdfPath = resolve(outputDirectory, 'revu-specimen.pdf');
    await perform('stage-fixture-copy', async () => {
      await copyFile(pdfPath, stagedPdfPath);
      const [sourceSha256, stagedSha256] = await Promise.all([sha256File(pdfPath), sha256File(stagedPdfPath)]);
      if (sourceSha256 !== stagedSha256) throw new Error('Staged Revu fixture does not match the source PDF');
      return { file: basename(stagedPdfPath), sha256: stagedSha256 };
    });
    sourceGuestPath = hostPathToParallelsGuest(pdfPath);
  }
  await perform('vm-running', () => revu.assertRunning());
  const environment = await perform('capture-environment', () => revu.environment());
  const contract = await loadToolContract();
  let inspectionEvidence;
  if (options.expectedTools?.length) {
    inspectionEvidence = await perform('validate-inspection', async () => {
      const inspectionPath = resolve(options.inspectionPath);
      const inspection = JSON.parse(await readFile(inspectionPath, 'utf8'));
      const summary = validateInspectedComponents(contract, options.expectedTools, inspection);
      if (!summary.passed) {
        const error = new Error(`Structural inspection failed: missing ${summary.missing.join(', ') || 'none'}; unexpected ${summary.unexpected.join(', ') || 'none'}`);
        error.code = 'BLUEBEAM_INSPECTION_MISMATCH';
        throw error;
      }
      return { file: relative(repoRoot, inspectionPath), sha256: await sha256File(inspectionPath), summary };
    });
  }
  let stagedGuest;
  if (!guestPdfPath) {
    const sourceSha256 = await sha256File(stagedPdfPath);
    stagedGuest = await perform('stage-fixture-in-guest', async () => revu.copyPdfToTemp(sourceGuestPath, {
      sha256: sourceSha256,
      name: `${basename(pdfPath, '.pdf')}-${Date.now()}.pdf`,
    }));
    guestPdfPath = stagedGuest.path;
  }
  const expectedSha256 = await sha256File(pdfPath);
  const verifiedGuest = await perform('verify-guest-pdf', () => revu.verifyPdf(guestPdfPath, expectedSha256));
  const opened = await perform('open-pdf', () => revu.openPdf(guestPdfPath, { timeoutMilliseconds: options.timeoutMilliseconds }));
  await perform('focus-revu', () => revu.focus());
  await perform('clear-selection', () => revu.sendKeys('{ESC}'));
  const openCapture = await perform('wait-for-page-render', () => captureWithRetry(openScreenshotPath, environment.displayResolution));
  if (options.select) {
    await perform('activate-select-tool', () => revu.sendKeys('{ESC}v'));
    await perform(`select-${options.select.tool}`, () => revu.click(options.select));
  }
  const windowState = await perform('capture-window-state', () => revu.windowState());
  const capture = await perform('capture-screen', () => captureWithRetry(screenshotPath, environment.displayResolution));

  const display = parseResolution(environment.displayResolution);
  const manifest = await createRunManifest({
    repoRoot,
    pdfPath,
    specimen: options.specimen,
    producer: 'Bluebeam Revu via Parallels Desktop',
    environment,
    expectedTools: options.expectedTools ?? [],
    rois: [{
      id: options.select ? `revu-${options.select.tool}-selected` : 'revu-all-tools-open',
      page: 1,
      x: 0,
      y: 0,
      width: display.width,
      height: display.height,
      image: basename(screenshotPath),
    }],
  });
  manifest.evidence = {
    vm: options.vmName,
    guestPdf: guestPdfPath,
    stagedPdf: stagedPdfPath ? basename(stagedPdfPath) : null,
    stagedGuest: stagedGuest ?? null,
    verifiedGuest,
    inspection: inspectionEvidence ?? null,
    opened,
    windowState,
    openCapture,
    screenshot: relative(outputDirectory, screenshotPath),
    capture,
    actions: 'revu-actions.json',
  };

  const observed = [
    { tool: 'select', operation: 'activate', status: options.select ? 'evidence-captured' : 'not-run', evidence: options.select ? basename(screenshotPath) : undefined },
  ];
  if (options.select) observed.push(
    { tool: options.select.tool, operation: 'open', status: 'evidence-captured', evidence: basename(openScreenshotPath) },
    { tool: options.select.tool, operation: 'select', status: 'evidence-captured', evidence: basename(screenshotPath), point: { x: options.select.x, y: options.select.y } },
  );
  const operations = createOperationScaffold(contract, observed);
  operations.run = { specimen: options.specimen, pdfSha256: manifest.pdf.sha256, manifest: 'revu-manifest.json' };

  await Promise.all([
    writeJson(resolve(outputDirectory, 'revu-manifest.json'), manifest),
    writeJson(resolve(outputDirectory, 'revu-actions.json'), { schema: 'butter-paper/revu-actions', version: 1, actions }),
    writeJson(resolve(outputDirectory, 'revu-operation-results.json'), operations),
  ]);
  return { manifest, actions, operations, outputDirectory };
}

export async function validatePngCapture(imagePath, expectedResolution, { minimumBrightRatio = 0.4 } = {}) {
  const image = await loadImage(await readFile(imagePath));
  const canvas = createCanvas(image.width, image.height);
  const context = canvas.getContext('2d');
  context.drawImage(image, 0, 0);
  const { data } = context.getImageData(0, 0, image.width, image.height);
  let minimum = 255;
  let maximum = 0;
  let brightSamples = 0;
  let samples = 0;
  const stride = Math.max(1, Math.floor(Math.min(image.width, image.height) / 64));
  for (let y = 0; y < image.height; y += stride) {
    for (let x = 0; x < image.width; x += stride) {
      const offset = (y * image.width + x) * 4;
      const luminance = Math.round((data[offset] * 299 + data[offset + 1] * 587 + data[offset + 2] * 114) / 1000);
      minimum = Math.min(minimum, luminance);
      maximum = Math.max(maximum, luminance);
      if (luminance >= 48) brightSamples += 1;
      samples += 1;
    }
  }
  const actualResolution = `${image.width}x${image.height}`;
  const brightRatio = samples ? brightSamples / samples : 0;
  if (actualResolution !== expectedResolution) throw new Error(`Captured ${actualResolution}; expected ${expectedResolution}`);
  if (maximum - minimum < 20 || brightRatio < minimumBrightRatio) throw new Error(`Captured screen is blank or incomplete (luminance ${minimum}-${maximum}, bright ratio ${brightRatio})`);
  return { width: image.width, height: image.height, minimumLuminance: minimum, maximumLuminance: maximum, brightRatio };
}

function parseSelection(value) {
  const match = /^([a-z0-9-]+):(\d+),(\d+)$/i.exec(value);
  if (!match) throw new Error('--select must use TOOL:X,Y, for example cloud:560,400');
  return { tool: match[1], x: Number(match[2]), y: Number(match[3]), count: 1 };
}

function parseResolution(value) {
  const match = /^(\d+)x(\d+)$/.exec(String(value));
  if (!match) throw new Error(`Invalid display resolution from guest: ${value}`);
  return { width: Number(match[1]), height: Number(match[2]) };
}

async function writeJson(filePath, value) {
  await mkdir(dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(value, null, 2)}\n`);
}

function delay(milliseconds) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, milliseconds));
}

function usage() {
  return `Usage: node scripts/bluebeam-compat/revu-capture-run.mjs --pdf PDF --output DIR [options]\n\nOptions:\n  --vm NAME              Parallels VM name (default: BP_PARALLELS_VM or Windows 11)\n  --guest-pdf PATH       Open this existing Windows-visible path instead of a hash-verified guest-temp copy\n  --select TOOL:X,Y      Activate Revu Select and click the specimen at guest coordinates\n  --expected-tools LIST  Comma-separated structural tool IDs; duplicates are significant\n  --inspection PATH      Annotation inspection JSON required by --expected-tools\n  --specimen ID          Stable specimen identifier\n  --repo PATH            Repository root used for Git/PDF manifest identity\n  --timeout-ms NUMBER    Revu document-open timeout\n`;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try {
    const options = parseCliOptions(process.argv.slice(2));
    if (options.help) process.stdout.write(usage());
    else {
      const result = await runRevuCapture(options);
      process.stdout.write(`${JSON.stringify({ output: result.outputDirectory, pdfSha256: result.manifest.pdf.sha256, appVersion: result.manifest.environment.appVersion, actions: result.actions.length }, null, 2)}\n`);
    }
  } catch (error) {
    process.stderr.write(`${error?.stack ?? error}\n`);
    process.exitCode = 1;
  }
}
