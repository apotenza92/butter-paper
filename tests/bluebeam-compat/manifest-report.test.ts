import { mkdtemp, readFile, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createCanvas } from '@napi-rs/canvas';
import { describe, expect, it } from 'vitest';
import { assertComparableManifests, sha256File, validateRunManifest, verifyManifestArtifacts } from '../../scripts/bluebeam-compat/manifest.mjs';
import { writeCompatibilityReports } from '../../scripts/bluebeam-compat/report.mjs';
import { runComparison } from '../../scripts/bluebeam-compat/run-comparison.mjs';
import { loadToolContract } from '../../scripts/bluebeam-compat/tool-contract.mjs';

const base = {
  schema: 'butter-paper/bluebeam-compat-run', version: 1, specimen: 'cloud-rectangle',
  pdf: { sha256: 'a'.repeat(64) },
  environment: { os: 'Windows 11', appVersion: 'Revu 21', displayResolution: '1920x1080', displayScale: 1, locale: 'en-AU', theme: 'light', fonts: ['Arial'] },
  rois: [{ id: 'cloud', page: 1, x: 10, y: 20, width: 100, height: 50 }],
};

describe('Bluebeam capture manifests and reports', () => {
  it('refuses comparison when a compatibility field changes', () => {
    expect(() => assertComparableManifests(base, structuredClone(base))).not.toThrow();
    const candidate = structuredClone(base);
    candidate.environment.displayScale = 1.25;
    expect(() => assertComparableManifests(base, candidate)).toThrowError(/displayScale/);
    try { assertComparableManifests(base, candidate); } catch (error) {
      expect((error as NodeJS.ErrnoException).code).toBe('BLUEBEAM_MANIFEST_MISMATCH');
    }
  });

  it('allows application and OS differences only in interoperability mode', () => {
    const candidate = structuredClone(base);
    candidate.environment.os = 'macOS 16';
    candidate.environment.appVersion = 'Butter Paper 0.0.17';
    expect(() => assertComparableManifests(base, candidate)).toThrowError(/os/);
    expect(() => assertComparableManifests(base, candidate, { mode: 'interoperability' })).not.toThrow();
    candidate.environment.displayScale = 2;
    expect(() => assertComparableManifests(base, candidate, { mode: 'interoperability' })).toThrowError(/displayScale/);
  });

  it('hashes files and writes JSON, JUnit, and HTML scaffolding', async () => {
    const directory = await mkdtemp(join(tmpdir(), 'bluebeam-compat-'));
    const sample = join(directory, 'sample.pdf');
    await writeFile(sample, 'fixture');
    expect(await sha256File(sample)).toBe('f16d05ec6b29248d2c61adb1e9263f78e4f7bace1b955014a2d17872cfe4064d');
    await writeCompatibilityReports(directory, { summary: 'one failure', specimens: [{ id: 'cloud', passed: false, failure: 'IoU', metrics: { iou: 0.5 } }] });
    expect(JSON.parse(await readFile(join(directory, 'report.json'), 'utf8')).summary).toBe('one failure');
    expect(await readFile(join(directory, 'report.junit.xml'), 'utf8')).toContain('<failure message="IoU">');
    expect(await readFile(join(directory, 'report.html'), 'utf8')).toContain('class="fail"');
  });

  it('validates manifest shape and verifies declared capture provenance', async () => {
    const directory = await mkdtemp(join(tmpdir(), 'bluebeam-provenance-'));
    const capture = join(directory, 'capture.png');
    await writeFile(capture, 'capture bytes');
    const digest = await sha256File(capture);
    const manifest = { ...base, provenance: { requireArtifactHashes: true }, rois: [{ ...base.rois[0], image: 'capture.png', imageSha256: digest }] };
    expect(validateRunManifest(manifest, { requireImages: true })).toBe(manifest);
    await expect(verifyManifestArtifacts(manifest, join(directory, 'manifest.json'))).resolves.toMatchObject({
      passed: true,
      requireHashes: true,
      artifacts: [{ file: 'capture.png', sha256: digest, hashDeclared: true }],
    });
    await expect(verifyManifestArtifacts({ ...manifest, rois: [{ ...manifest.rois[0], imageSha256: 'b'.repeat(64) }] }, join(directory, 'manifest.json')))
      .rejects.toMatchObject({ code: 'BLUEBEAM_PROVENANCE_INVALID' });
  });

  it('rejects capture symlinks even when the linked bytes match', async () => {
    const directory = await mkdtemp(join(tmpdir(), 'bluebeam-provenance-link-'));
    const outside = await mkdtemp(join(tmpdir(), 'bluebeam-provenance-outside-'));
    const externalCapture = join(outside, 'capture.png');
    await writeFile(externalCapture, 'capture bytes');
    await symlink(externalCapture, join(directory, 'capture.png'));
    const manifest = {
      ...base,
      provenance: { requireArtifactHashes: true },
      rois: [{ ...base.rois[0], image: 'capture.png', imageSha256: await sha256File(externalCapture) }],
    };
    await expect(verifyManifestArtifacts(manifest, join(directory, 'manifest.json'))).rejects.toMatchObject({
      code: 'BLUEBEAM_PROVENANCE_INVALID',
      message: expect.stringContaining('symlink'),
    });
  });

  it('stops an orchestration run at the manifest gate before image comparison', async () => {
    const directory = await mkdtemp(join(tmpdir(), 'bluebeam-mismatch-'));
    const baselinePath = join(directory, 'baseline.json');
    const candidatePath = join(directory, 'candidate.json');
    const operationsPath = join(directory, 'operations.json');
    await writeFile(baselinePath, JSON.stringify(base));
    await writeFile(candidatePath, JSON.stringify({ ...base, pdf: { sha256: 'different' } }));
    await writeFile(operationsPath, JSON.stringify([]));
    await expect(runComparison({ baselineManifestPath: baselinePath, candidateManifestPath: candidatePath, operationResultsPath: operationsPath, outputDirectory: join(directory, 'output') }))
      .rejects.toMatchObject({ code: 'BLUEBEAM_MANIFEST_MISMATCH' });
  });

  it('executes contract gates, ROI metrics, artifacts, and reports together', async () => {
    const directory = await mkdtemp(join(tmpdir(), 'bluebeam-run-'));
    const canvas = createCanvas(8, 8);
    const context = canvas.getContext('2d');
    context.fillStyle = 'white'; context.fillRect(0, 0, 8, 8);
    context.fillStyle = 'black'; context.fillRect(2, 2, 4, 4);
    await writeFile(join(directory, 'baseline.png'), canvas.toBuffer('image/png'));
    await writeFile(join(directory, 'candidate.png'), canvas.toBuffer('image/png'));
    const roi = { id: 'cloud', page: 1, x: 0, y: 0, width: 8, height: 8, image: 'baseline.png', thresholds: { minimumIoU: 1, minimumSsim: 1, maximumComponents: 1 } };
    const candidateRoi = { ...roi, image: 'candidate.png' };
    const baselinePath = join(directory, 'baseline.json'); const candidatePath = join(directory, 'candidate.json'); const operationsPath = join(directory, 'operations.json');
    await writeFile(baselinePath, JSON.stringify({ ...base, rois: [roi] }));
    await writeFile(candidatePath, JSON.stringify({ ...base, rois: [candidateRoi] }));
    const contract = await loadToolContract();
    const results = contract.tools.flatMap((tool: { id: string; operations: string[] }) => tool.operations.map((operation) => ({ tool: tool.id, operation, status: 'passed' })));
    await writeFile(operationsPath, JSON.stringify({ results }));
    const output = join(directory, 'output');
    const report = await runComparison({ baselineManifestPath: baselinePath, candidateManifestPath: candidatePath, operationResultsPath: operationsPath, outputDirectory: output });
    expect(report.passed).toBe(true);
    expect(report.specimens[0].metrics).toMatchObject({ iou: 1, ssim: 1, components: { baseline: [{ area: 16 }], candidate: [{ area: 16 }] } });
    expect((await readFile(join(output, 'cloud', 'heatmap.png'))).subarray(1, 4).toString()).toBe('PNG');
    expect(JSON.parse(await readFile(join(output, 'cloud', 'metrics.json'), 'utf8')).metrics).toMatchObject({ iou: 1, ssim: 1 });
    expect(report.provenance).toMatchObject({ baseline: { passed: true }, candidate: { passed: true } });
  });

  it('fails a synthetic cloud outline with an open gap and records the evidence', async () => {
    const directory = await mkdtemp(join(tmpdir(), 'bluebeam-cloud-gap-'));
    const render = async (path: string, open: boolean) => {
      const canvas = createCanvas(12, 12); const context = canvas.getContext('2d');
      context.fillStyle = 'white'; context.fillRect(0, 0, 12, 12);
      context.fillStyle = 'black';
      context.fillRect(2, 2, 8, 1); context.fillRect(2, 9, 8, 1);
      context.fillRect(2, 2, 1, 8); context.fillRect(9, open ? 4 : 2, 1, open ? 6 : 8);
      await writeFile(path, canvas.toBuffer('image/png'));
    };
    await render(join(directory, 'baseline.png'), false);
    await render(join(directory, 'candidate.png'), true);
    const baselineRoi = { id: 'cloud-gap', page: 1, x: 0, y: 0, width: 12, height: 12, image: 'baseline.png', thresholds: { minimumEnclosedBackgroundRegions: 1 } };
    const candidateRoi = { ...baselineRoi, image: 'candidate.png' };
    const baselinePath = join(directory, 'baseline.json'); const candidatePath = join(directory, 'candidate.json'); const operationsPath = join(directory, 'operations.json');
    await writeFile(baselinePath, JSON.stringify({ ...base, rois: [baselineRoi] }));
    await writeFile(candidatePath, JSON.stringify({ ...base, rois: [candidateRoi] }));
    const contract = await loadToolContract();
    await writeFile(operationsPath, JSON.stringify({ results: contract.tools.flatMap((tool: { id: string; operations: string[] }) => tool.operations.map((operation) => ({ tool: tool.id, operation, status: 'passed' }))) }));
    const output = join(directory, 'output');
    const report = await runComparison({ baselineManifestPath: baselinePath, candidateManifestPath: candidatePath, operationResultsPath: operationsPath, outputDirectory: output });
    expect(report.passed).toBe(false);
    expect(report.specimens[0]).toMatchObject({ passed: false, metrics: { continuity: { baseline: { enclosedBackgroundRegions: 1 }, candidate: { enclosedBackgroundRegions: 0 } } } });
    expect(await readFile(join(output, 'report.junit.xml'), 'utf8')).toContain('enclosed background regions 0 &lt; 1');
    expect((await readFile(join(output, 'cloud-gap', 'overlay.png'))).subarray(1, 4).toString()).toBe('PNG');
  });
});
