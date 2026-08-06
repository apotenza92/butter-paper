#!/usr/bin/env node

import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { copyFile, mkdir, readFile, rm, stat, writeFile } from 'node:fs/promises';
import { homedir } from 'node:os';
import { basename, join, relative, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { inspectAnnotations } from './pdf-inspector.mjs';
import { encodePowerShell, parseJsonOutput, powershellQuote } from './parallels-revu.mjs';

export const DEFAULT_SCRIPT_ENGINE = String.raw`C:\Program Files\Bluebeam Software\Bluebeam Revu\21\Revu\ScriptEngine.exe`;

const TOOLS = [
  { tool: 'text-box', page: 1, ids: ['bp:compat-text-box'], geometryId: 'bp:compat-text-box', resize: true, restyle: true, text: true },
  { tool: 'arrow', page: 1, ids: ['bp:compat-arrow'], geometryId: 'bp:compat-arrow', restyle: true },
  { tool: 'pen', page: 1, ids: ['bp:compat-pen'], geometryId: 'bp:compat-pen', restyle: true },
  { tool: 'highlight', page: 1, ids: ['bp:compat-highlight'], geometryId: 'bp:compat-highlight', restyle: true },
  { tool: 'cloud', page: 1, ids: ['bp:compat-cloud'], geometryId: 'bp:compat-cloud', resize: true, restyle: true },
  { tool: 'cloud-plus', page: 1, ids: ['bp:compat-cloud-plus:cloud', 'bp:compat-cloud-plus:text'], geometryId: 'bp:compat-cloud-plus:text', resize: true, restyle: true, text: true, group: true },
  { tool: 'callout', page: 1, ids: ['bp:compat-callout'], geometryId: 'bp:compat-callout', resize: true, restyle: true, text: true },
  { tool: 'image', page: 1, ids: ['bp:compat-image'], geometryId: 'bp:compat-image', resize: true },
  { tool: 'snapshot', page: 1, ids: ['bp:compat-snapshot'], geometryId: 'bp:compat-snapshot', resize: true },
  { tool: 'rectangle', page: 2, ids: ['bp:compat-rectangle'], geometryId: 'bp:compat-rectangle', resize: true, restyle: true },
  { tool: 'ellipse', page: 2, ids: ['bp:compat-ellipse'], geometryId: 'bp:compat-ellipse', resize: true, restyle: true },
  { tool: 'line', page: 2, ids: ['bp:compat-line'], geometryId: 'bp:compat-line', restyle: true },
  { tool: 'arc', page: 2, ids: ['bp:compat-arc'], geometryId: 'bp:compat-arc', restyle: true },
  { tool: 'polyline', page: 2, ids: ['bp:compat-polyline'], geometryId: 'bp:compat-polyline', restyle: true },
  { tool: 'polygon', page: 2, ids: ['bp:compat-polygon'], geometryId: 'bp:compat-polygon', restyle: true },
  { tool: 'dimension', page: 2, ids: ['bp:compat-dimension'], geometryId: 'bp:compat-dimension', restyle: true },
  { tool: 'length', page: 2, ids: ['bp:compat-length'], geometryId: 'bp:compat-length', restyle: true },
  { tool: 'polylength', page: 2, ids: ['bp:compat-polylength'], geometryId: 'bp:compat-polylength', restyle: true },
  { tool: 'area', page: 2, ids: ['bp:compat-area'], geometryId: 'bp:compat-area', restyle: true },
];

const SUPPLEMENTAL_GROUP = ['bp:compat-cloud-plus-inline:cloud', 'bp:compat-cloud-plus-inline:text'];

export function parseCliOptions(argv, environment = process.env) {
  const options = {
    vmName: environment.BP_PARALLELS_VM ?? 'Windows 11',
    repoRoot: process.cwd(),
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
    else if (token === '--output') options.outputDirectory = value();
    else if (token === '--repo') options.repoRoot = value();
    else if (token === '--help') options.help = true;
    else throw new Error(`Unknown option: ${token}`);
  }
  if (options.help) return options;
  if (!options.pdfPath) throw new Error('--pdf is required');
  if (!options.outputDirectory) throw new Error('--output is required');
  return options;
}

export function quoteBciString(value) {
  return `"${String(value).replaceAll('|', '||').replaceAll('"', '|"').replaceAll('\r', '|r').replaceAll('\n', '|n')}"`;
}

export function markupDictionary(properties) {
  const entries = Object.entries(properties).map(([key, value]) => `${quoteBciString(key)}:${quoteBciString(value)}`);
  return `{${entries.join(',')}}`;
}

export function markupSetCommand(page, id, properties) {
  return `MarkupSet(${page},${quoteBciString(id)},${quoteBciString(markupDictionary(properties))})`;
}

export function parseBluebeamDictionary(source) {
  let index = 0;
  const skip = () => { while (/\s/.test(source[index] ?? '')) index += 1; };
  const parseString = () => {
    const quote = source[index];
    if (quote !== "'" && quote !== '"') throw new Error(`Expected string at ${index}`);
    index += 1;
    let value = '';
    while (index < source.length) {
      const character = source[index++];
      if (character === '|') {
        const escaped = source[index++];
        if (escaped === 'r') value += '\r';
        else if (escaped === 'n') value += '\n';
        else if (escaped === 't') value += '\t';
        else if (escaped !== undefined) value += escaped;
      } else if (character === quote) {
        return value;
      } else value += character;
    }
    throw new Error('Unterminated Bluebeam string');
  };
  const result = {};
  skip();
  if (source[index++] !== '{') throw new Error('Expected dictionary');
  skip();
  while (source[index] !== '}') {
    const key = parseString();
    skip();
    if (source[index++] !== ':') throw new Error(`Expected colon at ${index - 1}`);
    skip();
    result[key] = parseString();
    skip();
    if (source[index] === ',') { index += 1; skip(); }
    else if (source[index] !== '}') throw new Error(`Expected comma or closing brace at ${index}`);
  }
  index += 1;
  skip();
  if (index !== source.length) throw new Error(`Unexpected dictionary suffix at ${index}`);
  return result;
}

export function parseInventoryOutput(output) {
  const lines = String(output).split(/\r?\n/).map((line) => line.trim()).filter(Boolean);
  const dictionaryLine = [...lines].reverse().find((line) => line.startsWith('{') && line.endsWith('}'));
  if (!dictionaryLine) throw new Error(`MarkupGetExList did not return a dictionary: ${String(output).trim()}`);
  const outer = parseBluebeamDictionary(dictionaryLine);
  return Object.fromEntries(Object.entries(outer).map(([id, properties]) => [id, parseBluebeamDictionary(properties)]));
}

export function createMutationPlans(inventory) {
  const plans = [
    { id: 'move', propertyKeys: ['x', 'y'], delta: { x: 10, y: 10 } },
    { id: 'resize', propertyKeys: ['width', 'height'], delta: { width: 8, height: 6 }, eligibility: (tool) => tool.resize },
    { id: 'restyle', propertyKeys: ['color', 'opacity', 'linewidth', 'linestyle'], values: { color: '#0080FF', opacity: '0.65', linewidth: '2', linestyle: 'dashed2' }, eligibility: (tool) => tool.restyle },
    { id: 'rotation', propertyKeys: ['rotation'], values: { rotation: '15' } },
    { id: 'comment-text', propertyKeys: ['comment'], values: { comment: 'Butter Paper ScriptEngine compatibility edit' } },
  ];
  return plans.map((plan) => ({
    ...plan,
    tools: TOOLS.map((tool) => {
      const properties = inventory[tool.geometryId];
      if (plan.eligibility && !plan.eligibility(tool)) return { ...tool, status: 'unsupported', reason: 'operation-not-in-tool-contract' };
      if (!properties) return { ...tool, status: 'unsupported', reason: 'markup-id-not-in-specimen' };
      const missing = plan.propertyKeys.filter((key) => properties[key] === undefined);
      if (missing.length) return { ...tool, status: 'unsupported', reason: `ScriptEngine-property-not-exposed:${missing.join(',')}` };
      const changed = {};
      for (const key of plan.propertyKeys) {
        changed[key] = plan.values?.[key] ?? String(Number(properties[key]) + plan.delta[key]);
      }
      return { ...tool, status: 'planned', targetId: tool.geometryId, properties: changed };
    }),
  }));
}

export function assessMutation(plan, before, after) {
  return plan.tools.map((item) => {
    if (item.status !== 'planned') return { tool: item.tool, operation: plan.id, status: 'unsupported', reason: item.reason };
    const target = after[item.targetId];
    if (!target) return { tool: item.tool, operation: plan.id, status: 'failed', reason: 'target-missing-after-save' };
    const observed = Object.fromEntries(Object.keys(item.properties).map((key) => [key, target[key]]));
    const changedKeys = Object.keys(item.properties).filter((key) => observed[key] !== before[item.targetId]?.[key]);
    const exactKeys = Object.keys(item.properties).filter((key) => equivalentScriptValue(observed[key], item.properties[key]));
    return {
      tool: item.tool,
      operation: plan.id,
      status: exactKeys.length === Object.keys(item.properties).length ? 'passed' : changedKeys.length ? 'partial' : 'unsupported',
      targetId: item.targetId,
      requested: item.properties,
      observed,
      changedKeys,
      reason: exactKeys.length === Object.keys(item.properties).length ? undefined : changedKeys.length ? 'ScriptEngine-normalized-or-ignored-some-properties' : 'ScriptEngine-accepted-command-but-properties-did-not-change',
    };
  });
}

export function assessDeletion(before, after) {
  return TOOLS.map((tool) => {
    const removed = tool.ids.filter((id) => before[id] && !after[id]);
    return {
      tool: tool.tool,
      operation: 'delete',
      status: removed.length === tool.ids.length ? 'passed' : removed.length ? 'partial' : 'failed',
      removed,
      expectedRemoved: tool.ids,
    };
  });
}

export async function runRevuScriptMatrix(options, dependencies = {}) {
  const repoRoot = resolve(options.repoRoot);
  const sourcePdf = resolve(options.pdfPath);
  const outputDirectory = resolve(options.outputDirectory);
  const execute = dependencies.execute ?? defaultExecute;
  const now = dependencies.now ?? (() => new Date());
  const runId = `${now().toISOString().replaceAll(/[^0-9]/g, '').slice(0, 14)}-${(await sha256File(sourcePdf)).slice(0, 12)}`;
  const hostStageName = `ButterPaperScriptMatrix-${runId}`;
  const hostStage = join(homedir(), 'Desktop', hostStageName);
  const sharedStage = `${String.raw`\\Mac\Home\Desktop`}\\${hostStageName}`;
  const actions = [];
  const perform = async (name, action) => {
    const startedAt = now().toISOString();
    try {
      const details = await action();
      actions.push({ name, status: 'passed', startedAt, completedAt: now().toISOString(), details });
      return details;
    } catch (error) {
      actions.push({ name, status: 'failed', startedAt, completedAt: now().toISOString(), error: String(error?.message ?? error) });
      throw error;
    }
  };
  await mkdir(outputDirectory, { recursive: true });
  await mkdir(hostStage, { recursive: true });
  const stagedSource = join(hostStage, 'source.pdf');
  await copyFile(sourcePdf, stagedSource);
  const sourceSha256 = await sha256File(sourcePdf);
  if (await sha256File(stagedSource) !== sourceSha256) throw new Error('Host staging changed the source PDF');

  const guest = new GuestScriptEngine({ vmName: options.vmName, sharedStage, hostStage, execute });
  try {
    const environment = await perform('locate-script-engine', () => guest.locateAndStage(sourceSha256, runId));
    const beforePages = await perform('inventory-source', () => guest.inventory('source.pdf', [1, 2], 'source'));
    const before = { ...beforePages[1], ...beforePages[2] };
    const plans = createMutationPlans(before);
    const resultFiles = [];
    const operationResults = [];

    for (const plan of plans) {
      const commands = plan.tools.filter((item) => item.status === 'planned').map((item) => markupSetCommand(item.page, item.targetId, item.properties));
      const guestName = `${plan.id}.pdf`;
      const executed = await perform(`script-${plan.id}`, () => guest.mutate('source.pdf', guestName, commands, plan.id));
      const pages = await perform(`inventory-${plan.id}`, () => guest.inventory(guestName, [1, 2], `${plan.id}-inventory`));
      const after = { ...pages[1], ...pages[2] };
      operationResults.push(...assessMutation(plan, before, after));
      operationResults.push(...assessPreservation(plan.id, before, after));
      resultFiles.push(await perform(`collect-${plan.id}`, () => guest.collect(guestName, outputDirectory)));
      executed.inventoryCounts = [Object.keys(pages[1]).length, Object.keys(pages[2]).length];
    }

    const deleteCommands = TOOLS.flatMap((tool) => tool.ids.map((id) => `MarkupDelete(${tool.page},${quoteBciString(id)})`));
    const deleted = await perform('script-delete', () => guest.mutate('source.pdf', 'delete.pdf', deleteCommands, 'delete'));
    const deletedPages = await perform('inventory-delete', () => guest.inventory('delete.pdf', [1, 2], 'delete-inventory'));
    const afterDelete = { ...deletedPages[1], ...deletedPages[2] };
    operationResults.push(...assessDeletion(before, afterDelete));
    operationResults.push({
      tool: 'cloud-plus-inline-supplemental', operation: 'delete',
      status: SUPPLEMENTAL_GROUP.every((id) => afterDelete[id]) ? 'passed' : 'failed',
      reason: 'Supplemental inline Cloud+ is intentionally outside the 19-tool delete target and must remain grouped.',
    });
    resultFiles.push(await perform('collect-delete', () => guest.collect('delete.pdf', outputDirectory)));
    deleted.inventoryCounts = [Object.keys(deletedPages[1]).length, Object.keys(deletedPages[2]).length];

    const inspections = {};
    const inspectionByFile = {};
    for (const file of resultFiles) {
      const inspection = await inspectAnnotations(file.path);
      inspectionByFile[basename(file.path)] = inspection;
      const inspectionPath = file.path.replace(/\.pdf$/i, '.inspection.json');
      await writeJson(inspectionPath, inspection);
      inspections[basename(file.path)] = { file: basename(inspectionPath), sha256: await sha256File(inspectionPath), counts: inspection.pages.map((page) => page.annotations.length) };
    }
    const reimports = await perform('butter-reimport', () => butterReimport(repoRoot, resultFiles.map((file) => file.path), outputDirectory));
    const reimportByFile = Object.fromEntries(reimports.map((document) => [basename(document.file), document]));
    for (const result of operationResults) {
      const file = result.operation === 'delete' ? 'delete.pdf' : `${result.operation}.pdf`;
      if (inspections[file]) result.evidence = { pdf: file, inspection: inspections[file].file, butterReimport: 'butter-reimport.json' };
      if (result.tool === 'all-tools-structure' && inspectionByFile[file] && reimportByFile[file]) {
        const nativeGroupProof = inspectCloudPlusGroups(inspectionByFile[file]);
        const reimport = reimportByFile[file];
        const butterGroupProof = reimport.kinds.filter((kind) => kind === 'cloud-plus').length === 2
          && reimport.sourceAnnotationCounts.filter((count, index) => reimport.kinds[index] === 'cloud-plus').every((count) => count === 2);
        result.nativeGroupProof = nativeGroupProof;
        result.butterGroupProof = { passed: butterGroupProof, pageCounts: reimport.pageCounts, logicalKinds: reimport.kinds.length };
        if (!nativeGroupProof.passed || !butterGroupProof) result.status = 'failed';
      }
    }

    const report = {
      schema: 'butter-paper/revu-script-matrix', version: 1, status: summarizeStatus(operationResults),
      source: { file: relative(repoRoot, sourcePdf), sha256: sourceSha256, bytes: (await stat(sourcePdf)).size },
      environment,
      officialCommandLane: {
        inventory: ['MarkupList', 'MarkupGetExList'],
        mutation: 'MarkupSet', deletion: 'MarkupDelete', persistence: ['Save', 'Close'],
        documentation: 'https://support.bluebeam.com/wp-content/uploads/2019/08/Bluebeam-Script-Reference-2018.pdf',
        limitations: [
          'ScriptEngine exposes bounding-box x/y/width/height only for some markup types; line-based geometry and vertex reshaping are not script-addressable through MarkupSet.',
          'A command accepted without an exact post-save property change is reported as unsupported or partial, never passed.',
          'This headless lane verifies native IDs, exposed properties, save/reopen, structural inspection, and Butter reimport; it does not substitute for licensed Revu GUI pixel review.',
        ],
      },
      results: operationResults,
      files: resultFiles.map((file) => ({ file: basename(file.path), sha256: file.sha256, bytes: file.bytes })),
      inspections,
      butterReimport: { file: 'butter-reimport.json', sha256: await sha256File(join(outputDirectory, 'butter-reimport.json')), documents: reimports.length },
      actions: 'revu-script-actions.json',
    };
    await Promise.all([
      writeJson(join(outputDirectory, 'revu-script-matrix.json'), report),
      writeJson(join(outputDirectory, 'revu-script-actions.json'), { schema: 'butter-paper/revu-script-actions', version: 1, actions }),
    ]);
    return { report, actions, outputDirectory };
  } finally {
    await rm(hostStage, { recursive: true, force: true });
  }
}

class GuestScriptEngine {
  constructor({ vmName, sharedStage, hostStage, execute }) {
    this.vmName = vmName;
    this.sharedStage = sharedStage;
    this.hostStage = hostStage;
    this.execute = execute;
  }

  powershell(script) {
    return this.execute('prlctl', ['exec', this.vmName, '--current-user', 'powershell.exe', '-NoLogo', '-NoProfile', '-NonInteractive', '-OutputFormat', 'Text', '-EncodedCommand', encodePowerShell(`$ErrorActionPreference = 'Stop'\n[Console]::OutputEncoding = [Text.Encoding]::UTF8\n${script}`)]);
  }

  locateAndStage(expectedSha256, runId) {
    const result = parseJsonOutput(this.powershell(String.raw`
$engine = Get-ChildItem -LiteralPath 'C:\Program Files\Bluebeam Software' -Filter ScriptEngine.exe -File -Recurse | Sort-Object FullName | Select-Object -First 1
if (-not $engine) { throw 'ScriptEngine.exe was not found under the installed Bluebeam Software directory' }
$runDirectory = Join-Path $env:TEMP ${powershellQuote(`ButterPaperScriptMatrix-${runId}`)}
New-Item -ItemType Directory -Path $runDirectory -Force | Out-Null
$source = Join-Path $runDirectory 'source.pdf'
Copy-Item -LiteralPath ${powershellQuote(`${this.sharedStage}\\source.pdf`)} -Destination $source -Force
$sourceHash = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant()
[ordered]@{
  scriptEnginePath = $engine.FullName
  scriptEngineVersion = $engine.VersionInfo.ProductVersion
  scriptEngineSha256 = (Get-FileHash -LiteralPath $engine.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
  sourceGuestPath = $source
  sourceSha256 = $sourceHash
  runDirectory = $runDirectory
} | ConvertTo-Json -Compress
`));
    if (result.sourceSha256 !== expectedSha256) throw new Error('Guest-staged source hash differs from the host source');
    this.enginePath = result.scriptEnginePath;
    this.runDirectory = result.runDirectory;
    return result;
  }

  runBci(name, body) {
    const hostName = `${name}.bci`;
    return writeFile(join(this.hostStage, hostName), `${body.trim()}\n`, 'utf8').then(() => {
      const result = parseJsonOutput(this.powershell(String.raw`
$engine = ${powershellQuote(this.enginePath)}
$runDirectory = ${powershellQuote(this.runDirectory)}
$bci = Join-Path $runDirectory ${powershellQuote(hostName)}
Copy-Item -LiteralPath ${powershellQuote(`${this.sharedStage}\\${hostName}`)} -Destination $bci -Force
$lines = @(& $engine ('Script("' + $bci + '")') 2>&1 | ForEach-Object { $_.ToString() })
$exitCode = $LASTEXITCODE
[ordered]@{ exitCode = $exitCode; output = @($lines); bci = $bci } | ConvertTo-Json -Compress -Depth 4
`));
      if (result.exitCode !== 0 || result.output.some((line) => /(?:Exception:|Parse\.Error)/i.test(line))) {
        throw new Error(`ScriptEngine ${name} failed: ${result.output.join('\n')}`);
      }
      return result;
    });
  }

  async inventory(pdfName, pages, label) {
    const inventories = {};
    for (const page of pages) {
      const commands = [`Open(${quoteBciString(joinWindows(this.runDirectory, pdfName))})`, `MarkupGetExList(${page})`, 'Close()'];
      const result = await this.runBci(`${label}-page-${page}`, commands.join('\n'));
      const dictionary = result.output.find((line) => line.trim().startsWith('{') && line.trim().endsWith('}'));
      inventories[page] = dictionary ? parseInventoryOutput(dictionary) : {};
    }
    return inventories;
  }

  async mutate(sourceName, outputName, commands, label) {
    const source = joinWindows(this.runDirectory, sourceName);
    const output = joinWindows(this.runDirectory, outputName);
    const body = [`Open(${quoteBciString(source)})`, ...commands, `Save(${quoteBciString(output)},0)`, 'Close()'].join('\n');
    const result = await this.runBci(label, body);
    const verified = parseJsonOutput(this.powershell(String.raw`
$file = ${powershellQuote(output)}
if (-not (Test-Path -LiteralPath $file)) { throw ${powershellQuote(`ScriptEngine did not create ${outputName}`)} }
[ordered]@{ path = $file; sha256 = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant(); bytes = (Get-Item -LiteralPath $file).Length } | ConvertTo-Json -Compress
`));
    return { ...result, output: verified };
  }

  async collect(guestName, outputDirectory) {
    const guestPath = joinWindows(this.runDirectory, guestName);
    const copied = parseJsonOutput(this.powershell(String.raw`
$source = ${powershellQuote(guestPath)}
$destination = ${powershellQuote(`${this.sharedStage}\\${guestName}`)}
Copy-Item -LiteralPath $source -Destination $destination -Force
[ordered]@{ sha256 = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant(); bytes = (Get-Item -LiteralPath $source).Length } | ConvertTo-Json -Compress
`));
    const hostPath = join(this.hostStage, guestName);
    if (await sha256File(hostPath) !== copied.sha256) throw new Error(`Collected ${guestName} differs from the guest output`);
    const destination = join(outputDirectory, guestName);
    await copyFile(hostPath, destination);
    return { path: destination, sha256: copied.sha256, bytes: copied.bytes };
  }
}

function assessPreservation(operation, before, after) {
  const beforeIds = Object.keys(before).sort();
  const afterIds = Object.keys(after).sort();
  const externalGrouped = TOOLS.find((tool) => tool.tool === 'cloud-plus').ids;
  const groups = [...externalGrouped, ...SUPPLEMENTAL_GROUP];
  return [{
    tool: 'all-tools-structure', operation,
    status: JSON.stringify(beforeIds) === JSON.stringify(afterIds) ? 'passed' : 'failed',
    expectedNativeCount: beforeIds.length,
    observedNativeCount: afterIds.length,
    cloudPlusComponents: Object.fromEntries(groups.map((id) => [id, { present: Boolean(after[id]), scriptInventoryGrouped: after[id]?.grouped ?? null, scriptInventoryParent: after[id]?.parent ?? null }])),
  }];
}

function inspectCloudPlusGroups(inspection) {
  const annotations = inspection.pages.flatMap((page) => page.annotations);
  const expected = [
    ['bp:compat-cloud-plus:text', 'bp:compat-cloud-plus:cloud'],
    ['bp:compat-cloud-plus-inline:text', 'bp:compat-cloud-plus-inline:cloud'],
  ];
  const groups = expected.map(([parent, child]) => {
    const annotation = annotations.find((item) => item.name === parent);
    const names = (annotation?.canonical?.GroupNesting ?? []).map((item) => item?.$name).filter(Boolean);
    return { parent, child, groupNesting: names, passed: names.includes(parent) && names.includes(child) };
  });
  return { passed: groups.every((group) => group.passed), groups };
}

async function butterReimport(repoRoot, files, outputDirectory) {
  const helperPath = join(outputDirectory, 'butter-reimport.mts');
  const outputPath = join(outputDirectory, 'butter-reimport.json');
  const documentModule = pathToFileURL(join(repoRoot, 'packages/pdf/src/document.ts')).href;
  await writeFile(helperPath, `import { writeFile } from 'node:fs/promises';\nimport { openPdfDocument } from ${JSON.stringify(documentModule)};\nconst [output, ...files] = process.argv.slice(2);\nconst results = [];\nfor (const file of files) {\n  const handle = await openPdfDocument(file);\n  try {\n    const pages = await handle.annotations.readAllPageAnnotations();\n    results.push({ file, pageCounts: pages.map((page) => page.length), kinds: pages.flat().map((markup) => markup.kind), sourceAnnotationCounts: pages.flat().map((markup) => markup.source?.annotationIds?.length ?? 0) });\n  } finally { await handle.close(); }\n}\nawait writeFile(output, JSON.stringify(results, null, 2) + '\\n');\n`, 'utf8');
  execFileSync(join(repoRoot, 'node_modules/.bin/tsx'), [helperPath, outputPath, ...files], { cwd: repoRoot, encoding: 'utf8', maxBuffer: 32 * 1024 * 1024, timeout: 120_000 });
  return JSON.parse(await readFile(outputPath, 'utf8'));
}

function equivalentScriptValue(actual, expected) {
  const actualNumber = Number(actual);
  const expectedNumber = Number(expected);
  if (actual !== '' && expected !== '' && Number.isFinite(actualNumber) && Number.isFinite(expectedNumber)) return Math.abs(actualNumber - expectedNumber) < 0.001;
  return String(actual).toLowerCase() === String(expected).toLowerCase();
}

function summarizeStatus(results) {
  return results.some((result) => result.status === 'failed') ? 'failed' : results.some((result) => result.status === 'partial') ? 'partial' : 'passed-with-unsupported';
}

function joinWindows(directory, name) {
  return `${directory.replace(/[\\/]+$/, '')}\\${name}`;
}

async function sha256File(path) {
  return createHash('sha256').update(await readFile(path)).digest('hex');
}

async function writeJson(path, value) {
  await writeFile(path, `${JSON.stringify(value, null, 2)}\n`, 'utf8');
}

function defaultExecute(command, args) {
  return execFileSync(command, args, { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, timeout: 90_000, killSignal: 'SIGTERM' });
}

function printHelp() {
  process.stdout.write('Usage: node scripts/bluebeam-compat/revu-script-matrix.mjs --pdf INPUT.pdf --output DIRECTORY [--vm "Windows 11"]\n');
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  const options = parseCliOptions(process.argv.slice(2));
  if (options.help) printHelp();
  else {
    const result = await runRevuScriptMatrix(options);
    process.stdout.write(`${JSON.stringify({ outputDirectory: result.outputDirectory, status: result.report.status, results: result.report.results.length }, null, 2)}\n`);
  }
}
