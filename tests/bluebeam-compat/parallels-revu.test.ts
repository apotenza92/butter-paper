import { mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createCanvas } from '@napi-rs/canvas';
import { describe, expect, it, vi } from 'vitest';
import { encodePowerShell, hostPathToParallelsGuest, ParallelsRevu, parseJsonOutput, powershellQuote } from '../../scripts/bluebeam-compat/parallels-revu.mjs';
import { createOperationScaffold, parseCliOptions, runRevuCapture, validatePngCapture } from '../../scripts/bluebeam-compat/revu-capture-run.mjs';
import { loadToolContract } from '../../scripts/bluebeam-compat/tool-contract.mjs';

describe('Parallels Revu adapter', () => {
  it('quotes and encodes guest PowerShell without shell interpolation', () => {
    expect(powershellQuote("C:\\Reviewer's\\file.pdf")).toBe("'C:\\Reviewer''s\\file.pdf'");
    expect(Buffer.from(encodePowerShell("Write-Output 'ok'"), 'base64').toString('utf16le')).toBe("Write-Output 'ok'");
    expect(parseJsonOutput('Parallels Tools\r\n{"ok":true}\r\n')).toEqual({ ok: true });
  });

  it('maps only paths inside the host home unless an explicit guest path is supplied', () => {
    expect(hostPathToParallelsGuest('/workspace/alex/Desktop/specimen.pdf', { hostHome: '/workspace/alex' })).toBe('\\\\Mac\\Home\\Desktop\\specimen.pdf');
    expect(() => hostPathToParallelsGuest('/private/tmp/specimen.pdf', { hostHome: '/workspace/alex' })).toThrow(/--guest-pdf/);
  });

  it('copies a shared fixture to guest temp and verifies the guest hash', () => {
    const sha256 = '9186e465b66d1b88b5a97b2f43a3d5e5a0d2949e2d36b966c28f3ce785c7bc9c';
    const execute = vi.fn(() => JSON.stringify({ path: 'C:\\Temp\\fixture.pdf', sha256, bytes: 7 }));
    const revu = new ParallelsRevu({ vmName: 'Windows 11', execute });
    expect(revu.copyPdfToTemp('\\\\Mac\\Home\\Desktop\\fixture.pdf', { sha256, name: 'fixture.pdf' })).toEqual({ path: 'C:\\Temp\\fixture.pdf', sha256, bytes: 7 });
    expect(execute).toHaveBeenCalledTimes(1);
  });

  it('collects architecture, clock, timezone, and binary identity in the read-only environment script', () => {
    const execute = vi.fn(() => JSON.stringify({
      os: 'Microsoft Windows 11 Pro', windowsBuild: '26200', osArchitecture: 'ARM 64-bit Processor',
      systemType: 'ARM64-based PC', processArchitecture: 'ARM64', appVersion: '21.2.0.1883',
      appFileVersion: '21.2.0.1883', timezone: 'AUS Eastern Standard Time',
      observedAt: '2026-08-05T05:00:00.0000000Z', fonts: [],
    }));
    const environment = new ParallelsRevu({ vmName: 'Windows 11', execute }).environment();
    expect(environment).toMatchObject({ osArchitecture: 'ARM 64-bit Processor', processArchitecture: 'ARM64', timezone: 'AUS Eastern Standard Time' });
    const encoded = execute.mock.calls[0][1].at(-1);
    const script = Buffer.from(encoded, 'base64').toString('utf16le');
    expect(script).toContain('Get-TimeZone');
    expect(script).toContain('OSArchitecture');
  });

  it('parses parameterized selection options and creates a complete pending operation matrix', async () => {
    const options = parseCliOptions(['--pdf', 'fixture.pdf', '--output', 'run', '--select', 'cloud:560,400', '--expected-tools', 'cloud-plus,cloud-plus', '--inspection', 'inspection.json'], {});
    expect(options).toMatchObject({ vmName: 'Windows 11', select: { tool: 'cloud', x: 560, y: 400 }, expectedTools: ['cloud-plus', 'cloud-plus'], inspectionPath: 'inspection.json' });
    const contract = await loadToolContract();
    const scaffold = createOperationScaffold(contract, [{ tool: 'cloud', operation: 'select', status: 'evidence-captured' }]);
    expect(scaffold.results).toHaveLength(contract.tools.reduce((sum: number, tool: { operations: string[] }) => sum + tool.operations.length, 0));
    expect(scaffold.results.find((result: { tool: string; operation: string }) => result.tool === 'cloud' && result.operation === 'select')?.status).toBe('evidence-captured');
    expect(scaffold.results.find((result: { tool: string; operation: string }) => result.tool === 'cloud' && result.operation === 'move')?.status).toBe('not-run');
  });

  it('rejects transient black VM captures before accepting evidence', async () => {
    const directory = await mkdtemp(join(tmpdir(), 'revu-capture-'));
    const blackPath = join(directory, 'black.png');
    const visiblePath = join(directory, 'visible.png');
    const canvas = createCanvas(64, 32);
    const context = canvas.getContext('2d');
    context.fillStyle = 'black'; context.fillRect(0, 0, 64, 32);
    await writeFile(blackPath, canvas.toBuffer('image/png'));
    await expect(validatePngCapture(blackPath, '64x32')).rejects.toThrow(/blank/);
    context.fillStyle = 'white'; context.fillRect(0, 0, 64, 24);
    await writeFile(visiblePath, canvas.toBuffer('image/png'));
    await expect(validatePngCapture(visiblePath, '64x32')).resolves.toMatchObject({ width: 64, height: 32, minimumLuminance: 0, maximumLuminance: 255 });
  });

  it('runs semantic actions and writes hash-linked evidence without requiring Parallels in tests', async () => {
    const directory = await mkdtemp(join(tmpdir(), 'revu-runner-'));
    const pdfPath = join(directory, 'fixture.pdf');
    const inspectionPath = join(directory, 'inspection.json');
    const outputDirectory = join(directory, 'evidence');
    await writeFile(pdfPath, 'fixture-pdf');
    await writeFile(inspectionPath, JSON.stringify({ pages: [{ annotations: [
      { subtype: 'Polygon', intent: 'PolygonCloud', name: 'cloud' },
      { subtype: 'FreeText', intent: 'FreeTextCallout', name: 'text' },
    ] }] }));
    const controller = {
      assertRunning: vi.fn(() => 'Windows 11 running'),
      environment: vi.fn(() => ({ os: 'Microsoft Windows 11 Pro', windowsBuild: '26100', appVersion: '21.2.0.1883', displayResolution: '1500x962', displayScale: 1, locale: 'en-AU', theme: 'dark', fonts: ['Arial'] })),
      openPdf: vi.fn(() => ({ processId: 123, title: 'fixture.pdf - Bluebeam Revu', pdf: 'fixture' })),
      verifyPdf: vi.fn(() => ({ path: '\\\\Mac\\Home\\fixture.pdf', sha256: 'unused-in-fake', bytes: 11 })),
      focus: vi.fn(() => ({ focused: true })),
      sendKeys: vi.fn(() => ({ keysSent: '{ESC}v' })),
      click: vi.fn(() => ({ x: 560, y: 400 })),
      windowState: vi.fn(() => ({ title: 'fixture.pdf - Bluebeam Revu', window: { x: 0, y: 0, width: 1500, height: 962 } })),
      capture: vi.fn(async (path: string) => writeFile(path, 'png')),
    };
    const result = await runRevuCapture({ repoRoot: process.cwd(), pdfPath, guestPdfPath: '\\\\Mac\\Home\\fixture.pdf', outputDirectory, vmName: 'Windows 11', specimen: 'fixture', timeoutMilliseconds: 1000, expectedTools: ['cloud-plus'], inspectionPath, select: { tool: 'cloud', x: 560, y: 400, count: 1 } }, { controller, validateCapture: async () => ({ width: 1500, height: 962 }) });
    expect(result.actions.map((action: { name: string }) => action.name)).toEqual(['vm-running', 'capture-environment', 'validate-inspection', 'verify-guest-pdf', 'open-pdf', 'focus-revu', 'clear-selection', 'wait-for-page-render', 'activate-select-tool', 'select-cloud', 'capture-window-state', 'capture-screen']);
    const manifest = JSON.parse(await readFile(join(outputDirectory, 'revu-manifest.json'), 'utf8'));
    expect(manifest).toMatchObject({ schema: 'butter-paper/bluebeam-compat-run', specimen: 'fixture', expectedTools: ['cloud-plus'], environment: { appVersion: '21.2.0.1883' }, evidence: { actions: 'revu-actions.json', inspection: { summary: { passed: true } } } });
    expect(manifest.pdf.sha256).toMatch(/^[a-f0-9]{64}$/);
    const operations = JSON.parse(await readFile(join(outputDirectory, 'revu-operation-results.json'), 'utf8'));
    expect(operations.run.pdfSha256).toBe(manifest.pdf.sha256);
    expect(operations.results.find((item: { tool: string; operation: string }) => item.tool === 'cloud' && item.operation === 'select').status).toBe('evidence-captured');
  });
});
