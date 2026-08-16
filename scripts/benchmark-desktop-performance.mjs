import { spawn } from 'node:child_process';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import { cpus, tmpdir, totalmem } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { PDFDocument } from 'pdf-lib';

const moduleDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = resolve(moduleDirectory, '..');
const mebibyte = 1024 * 1024;

export function summarizeSamples(values) {
  if (values.length === 0) return null;
  const sorted = [...values].sort((left, right) => left - right);
  return {
    min: round(sorted[0]),
    median: round(percentile(sorted, 0.5)),
    p95: round(percentile(sorted, 0.95)),
    max: round(sorted.at(-1)),
  };
}

export function summarizeScenario(samples) {
  const metrics = {
    startupTargetMs: samples.map((sample) => sample.startupTargetMs),
    shellReadyMs: samples.map((sample) => sample.shellReadyMs),
    documentReadyMs: samples.map((sample) => sample.documentReadyMs),
    firstPageImageVisibleMs: samples.map((sample) => sample.perf.firstPageImageVisibleMs).filter(isNumber),
    firstPageFullVisibleMs: samples.map((sample) => sample.perf.firstPageFullVisibleMs).filter(isNumber),
    openTotalMs: samples.map((sample) => sample.diagnostics.openStageTimings?.totalOpenMs).filter(isNumber),
    rendererHeapAfterGcMiB: samples.map((sample) => sample.rendererHeapAfterGcBytes / mebibyte),
    totalWorkingSetMiB: samples.map((sample) => sample.processMetrics.totalWorkingSetKiB / 1024),
    renderCacheMiB: samples.map((sample) => sample.diagnostics.renderCacheBytes / mebibyte),
    thumbnailCacheMiB: samples.map((sample) => sample.diagnostics.thumbnailCacheBytes / mebibyte),
    longTaskCount: samples.map((sample) => sample.perf.longTasks.count),
    maxLongTaskMs: samples.map((sample) => sample.perf.longTasks.maxDuration),
  };
  return {
    ...Object.fromEntries(
    Object.entries(metrics).map(([name, values]) => [name, summarizeSamples(values)]),
    ),
    startupTimeline: summarizeNamedMetrics(samples, 'startupTimeline'),
    startupPhases: summarizeNamedMetrics(samples, 'startupPhases'),
  };
}

export function alignStartupMilestones(entries, launchedAtEpochMs) {
  const milestones = {};
  for (const entry of entries) {
    if (!entry || typeof entry.name !== 'string' || !isNumber(entry.capturedAtEpochMs)) continue;
    const alignedTime = round(entry.capturedAtEpochMs - launchedAtEpochMs);
    milestones[`${toCamelCase(entry.name)}Ms`] ??= alignedTime;
    if (milestones.mainProcessStartedMs == null && isNumber(entry.processUptimeMs)) {
      milestones.mainProcessStartedMs = round(alignedTime - entry.processUptimeMs);
    }
  }
  return milestones;
}

export function deriveStartupPhases(timeline) {
  return compactMetrics({
    processLaunchToMainStartMs: difference(timeline.mainProcessStartedMs, 0),
    mainProcessModuleLoadMs: difference(timeline.mainModuleLoadedMs, timeline.mainProcessStartedMs),
    pdfSessionPreparationMs: difference(
      timeline.pdfSessionPreparationCompletedMs,
      timeline.pdfSessionPreparationStartedMs,
    ),
    appReadyWaitMs: difference(timeline.appReadyMs, timeline.mainModuleLoadedMs),
    readyToBootstrapEntryMs: difference(timeline.bootstrapReadyEnteredMs, timeline.appReadyMs),
    bootstrapStoreSetupMs: difference(timeline.bootstrapStoresCreatedMs, timeline.bootstrapReadyEnteredMs),
    bootstrapIpcRegistrationMs: difference(timeline.bootstrapIpcRegisteredMs, timeline.bootstrapStoresCreatedMs),
    bootstrapMenuInstallMs: difference(timeline.bootstrapMenuInstalledMs, timeline.bootstrapIpcRegisteredMs),
    browserWindowConstructionMs: difference(timeline.browserWindowCreatedMs, timeline.bootstrapMenuInstalledMs),
    windowLoadRequestMs: difference(timeline.bootstrapWindowLoadRequestedMs, timeline.browserWindowCreatedMs),
    windowToNavigationMs: difference(timeline.rendererNavigationStartedMs, timeline.browserWindowCreatedMs),
    navigationToPreloadMs: difference(timeline.preloadModuleEvaluatedMs, timeline.rendererNavigationStartedMs),
    preloadBridgeMs: difference(timeline.preloadBridgeExposedMs, timeline.preloadModuleEvaluatedMs),
    rendererModuleEvaluationMs: difference(timeline.rendererModuleEvaluatedMs, timeline.preloadBridgeExposedMs),
    themeBootstrapMs: difference(timeline.themeReadyMs, timeline.themeRequestedMs),
    reactCommitMs: difference(timeline.reactCommittedMs, timeline.reactRenderRequestedMs),
    commitToFirstFrameMs: difference(timeline.firstAnimationFrameMs, timeline.reactCommittedMs),
    commitToDocumentVisibleMs: difference(timeline.firstPageImageVisibleMs, timeline.reactCommittedMs),
    documentPreviewToFullMs: difference(timeline.firstPageFullVisibleMs, timeline.firstPageImageVisibleMs),
  });
}

export function assertBenchmarkRendererIdentity(identity, expectedSource) {
  if (!identity?.hasAppRoot || !identity.metadata) {
    throw new Error(`Benchmark connected to a non-Butter Paper renderer (${identity?.href ?? 'unknown page'}).`);
  }
  const metadata = identity.metadata;
  for (const key of ['version', 'commit', 'branch', 'dirty', 'checkoutId', 'statusFingerprint']) {
    if (metadata[key] !== expectedSource[key]) {
      throw new Error(`Benchmark renderer provenance mismatch for ${key}: expected ${expectedSource[key]}, received ${metadata[key]}.`);
    }
  }
  if (metadata.development !== true) {
    throw new Error('Benchmark requires a development renderer with verified provenance.');
  }
  const expectedTitle = `Butter Paper Dev · ${expectedSource.branch}@${expectedSource.commit.slice(0, 8)}${expectedSource.dirty ? ' dirty' : ''}`;
  if (identity.title !== expectedTitle || metadata.windowTitle !== expectedTitle) {
    throw new Error(
      `Benchmark renderer title mismatch: expected ${expectedTitle}; received document title ${identity.title} and metadata title ${metadata.windowTitle}.`,
    );
  }
  return metadata;
}

export async function runBenchmark(options = {}) {
  const iterations = positiveInteger(options.iterations ?? 5, 'iterations');
  const settleMs = positiveInteger(options.settleMs ?? 1000, 'settleMs');
  const outputPath = options.outputPath
    ? resolve(repositoryRoot, options.outputPath)
    : resolve(repositoryRoot, 'test-results', `desktop-performance-${timestampSlug()}.json`);
  const benchmarkDirectory = mkdtempSync(join(tmpdir(), 'butter-paper-performance-'));
  const manyPagePdf = join(benchmarkDirectory, 'engineering-100-pages.pdf');
  await createManyPagePdf(
    resolve(repositoryRoot, 'tests/fixtures/generated/engineering-large.pdf'),
    manyPagePdf,
    100,
  );

  const scenarios = [
    { name: 'empty-shell', pdfPath: null },
    {
      name: 'blank-document',
      pdfPath: null,
      blankRequest: { widthMm: 210, heightMm: 297 },
    },
    {
      name: 'cold-pdf-single-page',
      pdfPath: resolve(repositoryRoot, 'tests/fixtures/generated/engineering-large.pdf'),
      launchWithPdf: true,
    },
    {
      name: 'engineering-single-page',
      pdfPath: resolve(repositoryRoot, 'tests/fixtures/generated/engineering-large.pdf'),
    },
    { name: 'engineering-100-pages', pdfPath: manyPagePdf },
  ];
  const selectedNames = options.scenarios?.length ? new Set(options.scenarios) : null;
  const selectedScenarios = selectedNames
    ? scenarios.filter((scenario) => selectedNames.has(scenario.name))
    : scenarios;
  if (selectedScenarios.length === 0) {
    throw new Error(`No matching scenarios: ${[...selectedNames].join(', ')}`);
  }

  const report = {
    schemaVersion: 1,
    capturedAt: new Date().toISOString(),
    source: sourceMetadata(),
    runtime: runtimeMetadata(),
    configuration: { iterations, settleMs },
    scenarios: {},
  };

  try {
    for (const scenario of selectedScenarios) {
      const samples = [];
      for (let iteration = 1; iteration <= iterations; iteration += 1) {
        process.stdout.write(`Benchmark ${scenario.name} ${iteration}/${iterations}...\n`);
        samples.push(await runScenario({ ...scenario, settleMs, benchmarkDirectory, expectedSource: report.source }));
      }
      report.scenarios[scenario.name] = {
        workload: scenario.pdfPath
          ? basename(scenario.pdfPath)
          : scenario.blankRequest
            ? `${scenario.blankRequest.widthMm}x${scenario.blankRequest.heightMm}mm blank PDF`
            : null,
        launchMode: scenario.launchWithPdf ? 'command-line-pdf' : 'empty-shell',
        summary: summarizeScenario(samples),
        samples,
      };
    }
  } finally {
    rmSync(benchmarkDirectory, { recursive: true, force: true });
  }

  mkdirSync(dirname(outputPath), { recursive: true });
  writeFileSync(outputPath, `${JSON.stringify(report, null, 2)}\n`);
  process.stdout.write(`Desktop performance report: ${outputPath}\n`);
  return { report, outputPath };
}

async function runScenario({
  name,
  pdfPath,
  blankRequest,
  launchWithPdf,
  settleMs,
  benchmarkDirectory,
  expectedSource,
}) {
  const port = await availablePort();
  const userDataDirectory = mkdtempSync(join(benchmarkDirectory, `${name}-user-data-`));
  const electronExecutable = resolveElectronExecutable();
  const args = [
    `--remote-debugging-port=${port}`,
    'apps/desktop',
  ];
  if (launchWithPdf && pdfPath) args.push(pdfPath);
  const output = [];
  const startedAt = performance.now();
  const launchedAtEpochMs = Date.now();
  const child = spawn(electronExecutable, args, {
    cwd: repositoryRoot,
    env: {
      ...process.env,
      BP_TEST_MODE: '1',
      BP_TEST_THEME: 'light',
      BP_OPEN_SAMPLE_PDF: '0',
      BP_TEST_USER_DATA_DIR: userDataDirectory,
    },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  child.stdout.on('data', (chunk) => output.push(chunk.toString()));
  child.stderr.on('data', (chunk) => output.push(chunk.toString()));

  try {
    const target = await waitForPageTarget(port, child, output);
    const startupTargetMs = performance.now() - startedAt;
    const cdp = await createCdpClient(target.webSocketDebuggerUrl);
    try {
      await cdp.send('Runtime.enable');
      await cdp.send('Performance.enable');
      const identity = await cdp.evaluate(`(async () => {
        const deadline = Date.now() + 20000;
        while (Date.now() < deadline) {
          const hasAppRoot = Boolean(document.querySelector('[data-testid="app-root"]'));
          const getMetadata = window.butterPaper?.application?.getMetadata;
          if (hasAppRoot && getMetadata) {
            return {
              metadata: await getMetadata(),
              title: document.title,
              hasAppRoot,
              href: location.href,
            };
          }
          await new Promise((resolve) => setTimeout(resolve, 20));
        }
        return { metadata: null, title: document.title, hasAppRoot: false, href: location.href };
      })()`);
      assertBenchmarkRendererIdentity(identity, expectedSource);
      const shellReadyMs = performance.now() - startedAt;
      if (pdfPath && !launchWithPdf) {
        await cdp.evaluate(`window.__butterPaperTestHooks.openDocumentPath(${JSON.stringify(pdfPath)})`);
      }
      if (blankRequest) {
        await cdp.evaluate(`window.__butterPaperTestHooks.createBlankPdf(${JSON.stringify(blankRequest)})`);
      }
      const ready = await cdp.evaluate(`(async () => {
        const deadline = Date.now() + 20000;
        while (Date.now() < deadline) {
          const hooks = window.__butterPaperTestHooks;
          const diagnostics = hooks?.getDiagnostics() ?? null;
          const documentReady = ${Boolean(pdfPath || blankRequest)}
            ? Boolean(diagnostics?.documentPath && diagnostics.pageRenderReady)
            : Boolean(diagnostics && diagnostics.documentPath === null);
          if (hooks && documentReady) {
            return { diagnostics, perf: hooks.getPerfSnapshot() };
          }
          await new Promise((resolve) => setTimeout(resolve, 20));
        }
        return {
          timedOut: true,
          diagnostics: window.__butterPaperTestHooks?.getDiagnostics?.() ?? null,
          perf: window.__butterPaperTestHooks?.getPerfSnapshot?.() ?? null,
          bodyText: document.body.innerText.slice(0, 2000),
        };
      })()`);
      if (ready.timedOut) {
        throw new Error(`Timed out waiting for Butter Paper readiness: ${JSON.stringify(ready)}`);
      }
      const documentReadyMs = performance.now() - startedAt;
      await cdp.evaluate('window.butterPaper.test?.getProcessMetrics()');
      await delay(settleMs);
      const settled = await cdp.evaluate(`(async () => ({
        diagnostics: window.__butterPaperTestHooks.getDiagnostics(),
        perf: window.__butterPaperTestHooks.getPerfSnapshot(),
        processMetrics: await window.butterPaper.test.getProcessMetrics(),
        mainStartupMilestones: await window.butterPaper.test.getStartupMilestones(),
        navigation: performance.getEntriesByType('navigation')[0]?.toJSON() ?? null,
        timeOrigin: performance.timeOrigin,
        startupMarks: Object.fromEntries(
          performance.getEntriesByType('mark')
            .filter((entry) => entry.name.startsWith('bp-startup:'))
            .map((entry) => [entry.name.slice('bp-startup:'.length), entry.startTime]),
        ),
      }))()`);
      const heapBeforeGc = await cdp.send('Runtime.getHeapUsage');
      await cdp.send('HeapProfiler.collectGarbage');
      const heapAfterGc = await cdp.send('Runtime.getHeapUsage');
      const dom = await cdp.send('Memory.getDOMCounters');
      const browserMetrics = await cdp.send('Performance.getMetrics');

      const startupTimeline = {
        ...alignStartupMilestones(settled.mainStartupMilestones, launchedAtEpochMs),
        rendererNavigationStartedMs: round(settled.timeOrigin - launchedAtEpochMs),
        ...Object.fromEntries(Object.entries(settled.startupMarks).map(([markName, startTime]) => [
          `${toCamelCase(markName)}Ms`,
          round(settled.timeOrigin + startTime - launchedAtEpochMs),
        ])),
        cdpTargetAvailableMs: round(startupTargetMs),
        shellReadyMs: round(shellReadyMs),
        documentReadyMs: round(documentReadyMs),
      };

      return {
        startupTargetMs: round(startupTargetMs),
        shellReadyMs: round(shellReadyMs),
        documentReadyMs: round(documentReadyMs),
        startupTimeline,
        startupPhases: deriveStartupPhases(startupTimeline),
        rendererHeapBeforeGcBytes: heapBeforeGc.usedSize,
        rendererHeapAfterGcBytes: heapAfterGc.usedSize,
        dom,
        browserMetrics: Object.fromEntries(
          browserMetrics.metrics.map(({ name: metricName, value }) => [metricName, value]),
        ),
        diagnostics: settled.diagnostics ?? ready.diagnostics,
        perf: settled.perf ?? ready.perf,
        processMetrics: settled.processMetrics,
        navigation: settled.navigation,
      };
    } finally {
      cdp.close();
    }
  } finally {
    await terminateChild(child);
  }
}

function summarizeNamedMetrics(samples, property) {
  const names = new Set(samples.flatMap((sample) => Object.keys(sample[property] ?? {})));
  return Object.fromEntries([...names].map((name) => [
    name,
    summarizeSamples(samples.map((sample) => sample[property]?.[name]).filter(isNumber)),
  ]));
}

function compactMetrics(metrics) {
  return Object.fromEntries(Object.entries(metrics).filter(([, value]) => isNumber(value)));
}

function difference(end, start) {
  return isNumber(end) && isNumber(start) ? round(end - start) : null;
}

function toCamelCase(value) {
  return value.replace(/-([a-z0-9])/g, (_match, character) => character.toUpperCase());
}

async function createManyPagePdf(sourcePath, outputPath, pageCount) {
  const source = await PDFDocument.load(readFileSync(sourcePath));
  const output = await PDFDocument.create({ updateMetadata: false });
  const pages = await output.copyPages(source, Array.from({ length: pageCount }, () => 0));
  for (const page of pages) output.addPage(page);
  writeFileSync(outputPath, await output.save({ useObjectStreams: false, addDefaultPage: false }));
}

async function waitForPageTarget(port, child, output) {
  const deadline = performance.now() + 20000;
  while (performance.now() < deadline) {
    if (child.exitCode !== null) {
      throw new Error(`Electron exited before CDP became ready (${child.exitCode}).\n${output.join('')}`);
    }
    try {
      const response = await fetch(`http://127.0.0.1:${port}/json/list`);
      if (response.ok) {
        const targets = await response.json();
        const page = targets.find((target) => target.type === 'page');
        if (page?.webSocketDebuggerUrl) return page;
      }
    } catch {
      // Expected while Electron is starting.
    }
    await delay(20);
  }
  throw new Error(`Timed out waiting for Electron CDP on port ${port}.\n${output.join('')}`);
}

async function createCdpClient(url) {
  const socket = new WebSocket(url);
  const pending = new Map();
  let nextId = 1;
  socket.addEventListener('message', (event) => {
    const message = JSON.parse(String(event.data));
    if (!message.id) return;
    const request = pending.get(message.id);
    if (!request) return;
    pending.delete(message.id);
    if (message.error) request.reject(new Error(JSON.stringify(message.error)));
    else request.resolve(message.result);
  });
  await new Promise((resolve, reject) => {
    socket.addEventListener('open', resolve, { once: true });
    socket.addEventListener('error', reject, { once: true });
  });

  return {
    send(method, params = {}) {
      const id = nextId++;
      socket.send(JSON.stringify({ id, method, params }));
      return new Promise((resolve, reject) => pending.set(id, { resolve, reject }));
    },
    async evaluate(expression) {
      const result = await this.send('Runtime.evaluate', {
        expression,
        returnByValue: true,
        awaitPromise: true,
      });
      if (result.exceptionDetails) {
        throw new Error(result.exceptionDetails.exception?.description ?? 'Runtime evaluation failed');
      }
      return result.result.value;
    },
    close() {
      socket.close();
    },
  };
}

async function terminateChild(child) {
  if (child.exitCode !== null || child.signalCode !== null) return;
  child.kill('SIGTERM');
  const exited = await Promise.race([
    new Promise((resolve) => child.once('exit', () => resolve(true))),
    delay(3000).then(() => false),
  ]);
  if (!exited && child.exitCode === null) {
    child.kill('SIGKILL');
    await new Promise((resolve) => child.once('exit', resolve));
  }
}

function resolveElectronExecutable() {
  if (process.platform === 'darwin') {
    return resolve(repositoryRoot, 'node_modules/electron/dist/Electron.app/Contents/MacOS/Electron');
  }
  const name = process.platform === 'win32' ? 'electron.exe' : 'electron';
  return resolve(repositoryRoot, 'node_modules/electron/dist', name);
}

function sourceMetadata() {
  const rootPackage = JSON.parse(readFileSync(resolve(repositoryRoot, 'package.json'), 'utf8'));
  const status = git(['status', '--porcelain=v1', '--untracked-files=all']);
  return {
    version: rootPackage.version,
    commit: git(['rev-parse', 'HEAD']),
    branch: git(['branch', '--show-current']) || '(detached)',
    dirty: status.length > 0,
    statusFingerprint: createHash('sha256').update(status).digest('hex'),
    checkoutId: createHash('sha256').update(repositoryRoot).digest('hex'),
  };
}

function runtimeMetadata() {
  return {
    platform: process.platform,
    architecture: process.arch,
    node: process.version,
    electron: JSON.parse(readFileSync(resolve(repositoryRoot, 'node_modules/electron/package.json'), 'utf8')).version,
    cpuCount: cpus().length,
    memoryBytes: totalmem(),
  };
}

function git(args) {
  return execFileSync('git', args, { cwd: repositoryRoot, encoding: 'utf8' }).trim();
}

async function availablePort() {
  const server = createServer();
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  const address = server.address();
  const port = typeof address === 'object' && address ? address.port : null;
  await new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
  if (!port) throw new Error('Unable to reserve a local debugging port.');
  return port;
}

function percentile(sorted, fraction) {
  const index = Math.max(0, Math.ceil(sorted.length * fraction) - 1);
  return sorted[index];
}

function positiveInteger(value, name) {
  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed <= 0) throw new Error(`${name} must be a positive integer.`);
  return parsed;
}

function isNumber(value) {
  return typeof value === 'number' && Number.isFinite(value);
}

function round(value) {
  return Math.round(value * 1000) / 1000;
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function timestampSlug() {
  return new Date().toISOString().replace(/[:.]/g, '-');
}

function parseArguments(argv) {
  const options = {};
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === '--iterations') options.iterations = argv[++index];
    else if (argument === '--settle-ms') options.settleMs = argv[++index];
    else if (argument === '--output') options.outputPath = argv[++index];
    else if (argument === '--scenario') (options.scenarios ??= []).push(argv[++index]);
    else throw new Error(`Unknown benchmark argument: ${argument}`);
  }
  return options;
}

const invokedPath = process.argv[1] ? pathToFileURL(resolve(process.argv[1])).href : null;
if (invokedPath === import.meta.url) {
  try {
    await runBenchmark(parseArguments(process.argv.slice(2)));
  } catch (error) {
    process.stderr.write(`Desktop performance benchmark failed: ${error instanceof Error ? error.stack : String(error)}\n`);
    process.exitCode = 1;
  }
}
