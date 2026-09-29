import { createHash } from 'node:crypto';
import { mkdir, mkdtemp, readFile, readdir, rename, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it, vi } from 'vitest';
import type { ApplicationMetadata, ElectronMigrationExportRequest, UpdateFrequency } from '../shared/protocol';
import {
  createElectronMigrationExportPlan,
  installDownloadedAfterMigration,
  publishElectronMigrationExport,
  verifyElectronMigrationPublication,
} from './electronDataMigration';

const renderer: ElectronMigrationExportRequest = {
  menuBarVisible: false,
  lastTemplateId: 'imported-01234567-89ab-cdef-0123-456789abcdef',
  generatedTemplates: [{
    id: 'custom-site-grid',
    name: 'Site grid',
    title: 'Untitled',
    request: {
      widthMm: 420,
      heightMm: 297,
      pattern: { type: 'grid', spacingMm: 10, color: '#d1d5db' },
    },
  }],
};
const updateSettings: {
  readonly frequency: UpdateFrequency;
  readonly lastSuccessfulCheckAt: string | null;
} = {
  frequency: 'sixHours',
  lastSuccessfulCheckAt: '2026-09-27T18:00:00.000Z',
};

function metadata(channel: 'stable' | 'beta'): ApplicationMetadata {
  const productName = channel === 'beta' ? 'Butter Paper Beta' : 'Butter Paper';
  return {
    channel,
    productName,
    version: '0.0.26',
    commit: null,
    branch: null,
    dirty: false,
    development: false,
    checkoutId: null,
    statusFingerprint: null,
    windowTitle: productName,
  };
}

function fixture(
  channel: 'stable' | 'beta' = 'stable',
  updater = updateSettings,
) {
  const bytes = new Uint8Array([37, 80, 68, 70, 45, 49, 46, 55]);
  return createElectronMigrationExportPlan({
    metadata: metadata(channel),
    renderer,
    updateSettings: updater,
    imported: {
      records: [{
        id: 'imported-01234567-89ab-cdef-0123-456789abcdef',
        name: 'Title block',
        kind: 'imported-pdf',
        pageCount: 1,
        createdAt: '2026-09-27T00:00:00.000Z',
      }],
      sources: [{ id: 'imported-01234567-89ab-cdef-0123-456789abcdef', bytes }],
    },
    createdAt: '2026-09-28T00:00:00.000Z',
  });
}

describe('Electron native migration export planning', () => {
  it('verifies publication evidence against the trusted on-disk export', async () => {
    const userData = await mkdtemp(join(tmpdir(), 'butter-paper-electron-evidence-'));
    const plan = fixture();
    const forged = {
      outcome: 'created' as const,
      exportId: plan.exportId,
      createdAt: plan.createdAt,
    };

    await expect(verifyElectronMigrationPublication(userData, forged)).resolves.toBe(false);
    const publication = await publishElectronMigrationExport(userData, plan);
    await expect(verifyElectronMigrationPublication(userData, publication)).resolves.toBe(true);
    await expect(verifyElectronMigrationPublication(userData, {
      ...publication,
      createdAt: '2026-09-28T00:00:01.000Z',
    })).resolves.toBe(false);
  });

  it('publishes a complete manifest-only export for a clean profile', async () => {
    const userData = await mkdtemp(join(tmpdir(), 'butter-paper-electron-clean-'));
    const plan = createElectronMigrationExportPlan({
      metadata: metadata('stable'),
      renderer: {
        menuBarVisible: true,
        lastTemplateId: 'built-in-blank',
        generatedTemplates: [],
      },
      updateSettings,
      imported: { records: [], sources: [] },
      createdAt: '2026-09-28T00:00:00.000Z',
    });

    await expect(publishElectronMigrationExport(userData, plan)).resolves.toEqual({
      outcome: 'created',
      exportId: plan.exportId,
      createdAt: plan.createdAt,
    });
    const exportRoot = join(userData, 'gpui-migration-export', 'v1');
    expect(await readdir(exportRoot)).toEqual(['manifest.json']);
    expect(JSON.parse(await readFile(join(exportRoot, 'manifest.json'), 'utf8')).templates).toEqual({
      lastTemplateId: 'built-in-blank',
      generated: [],
      imported: [],
    });
  });

  it('builds the exact Rust v1 manifest shape and explicit blocked-domain markers', () => {
    const plan = fixture();
    const manifest = JSON.parse(Buffer.from(plan.manifestBytes).toString('utf8'));
    expect(manifest).toEqual({
      schema: 'butter-paper/electron-native-migration',
      version: 1,
      channel: 'stable',
      exportId: plan.exportId,
      createdAt: '2026-09-28T00:00:00.000Z',
      source: { productName: 'Butter Paper', bundleIdentifier: 'com.butterpaper.desktop', version: '0.0.26' },
      preferences: {
        menuBarVisible: false,
        updateFrequency: 'sixHours',
        lastSuccessfulUpdateCheckAt: '2026-09-27T18:00:00.000Z',
      },
      templates: {
        lastTemplateId: renderer.lastTemplateId,
        generated: [{
          id: 'custom-site-grid', name: 'Site grid', title: 'Untitled', widthMm: 420, heightMm: 297,
          pattern: { kind: 'grid', spacingMm: 10, color: '#d1d5db' },
        }],
        imported: [{
          id: 'imported-01234567-89ab-cdef-0123-456789abcdef',
          name: 'Title block',
          createdAt: '2026-09-27T00:00:00.000Z',
          pageCount: 1,
          source: {
            path: 'templates/imported-01234567-89ab-cdef-0123-456789abcdef/source.pdf',
            bytes: 8,
            sha256: createHash('sha256').update(new Uint8Array([37, 80, 68, 70, 45, 49, 46, 55])).digest('hex'),
          },
        }],
      },
      unsupported: {
        recentSignatures: 'requires-secure-bridge',
        session: 'not-persisted-by-electron',
        colourPresets: 'not-imported-v1',
      },
    });
    expect(plan.files).toEqual([{
      path: 'templates/imported-01234567-89ab-cdef-0123-456789abcdef/source.pdf',
      bytes: new Uint8Array([37, 80, 68, 70, 45, 49, 46, 55]),
    }]);
  });

  it('binds trusted stable/beta identity and keeps unchanged data byte-stable', () => {
    const stableA = fixture('stable');
    const stableB = fixture('stable');
    const beta = fixture('beta');
    const differentUpdatePreference = fixture('stable', {
      frequency: 'monthly',
      lastSuccessfulCheckAt: updateSettings.lastSuccessfulCheckAt,
    });
    expect(stableA.exportId).toBe(stableB.exportId);
    expect(stableA.manifestBytes).toEqual(stableB.manifestBytes);
    expect(beta.exportId).not.toBe(stableA.exportId);
    expect(differentUpdatePreference.exportId).not.toBe(stableA.exportId);
    expect(JSON.parse(Buffer.from(beta.manifestBytes).toString('utf8')).source).toEqual({
      productName: 'Butter Paper Beta',
      bundleIdentifier: 'com.butterpaper.desktop.beta',
      version: '0.0.26',
    });
  });

  it('rejects development identity and incomplete or duplicated sources', () => {
    expect(() => createElectronMigrationExportPlan({
      metadata: { ...metadata('stable'), development: true }, renderer,
      updateSettings,
      imported: { records: [], sources: [] }, createdAt: '2026-09-28T00:00:00.000Z',
    })).toThrow(/Development builds/);
    expect(() => createElectronMigrationExportPlan({
      metadata: { ...metadata('stable'), productName: 'Butter Paper Beta' }, renderer,
      updateSettings,
      imported: { records: [], sources: [] }, createdAt: '2026-09-28T00:00:00.000Z',
    })).toThrow(/identity is inconsistent/);
    expect(() => createElectronMigrationExportPlan({
      metadata: metadata('stable'), renderer,
      updateSettings,
      imported: {
        records: [{ id: 'imported-a', name: 'A', kind: 'imported-pdf', pageCount: 1, createdAt: '2026-09-27T00:00:00.000Z' }],
        sources: [],
      },
      createdAt: '2026-09-28T00:00:00.000Z',
    })).toThrow(/incomplete or duplicated/);
    expect(() => createElectronMigrationExportPlan({
      metadata: metadata('stable'), renderer: { ...renderer, lastTemplateId: 'imported-ffffffff-ffff-ffff-ffff-ffffffffffff' },
      updateSettings,
      imported: { records: [], sources: [] }, createdAt: '2026-09-28T00:00:00.000Z',
    })).toThrow(/last selection is unavailable/);
    expect(() => createElectronMigrationExportPlan({
      metadata: metadata('stable'), renderer: {
        ...renderer,
        lastTemplateId: 'custom-site-grid',
        generatedTemplates: [{ ...renderer.generatedTemplates[0], request: { widthMm: 0, heightMm: 297 } }],
      },
      updateSettings,
      imported: { records: [], sources: [] }, createdAt: '2026-09-28T00:00:00.000Z',
    })).toThrow(/Renderer migration state is invalid/);
    expect(() => createElectronMigrationExportPlan({
      metadata: metadata('stable'),
      renderer: { ...renderer, lastTemplateId: 'built-in-blank' },
      imported: { records: [], sources: [] },
      updateSettings: { ...updateSettings, lastSuccessfulCheckAt: '2026-09-27' },
      createdAt: '2026-09-28T00:00:00.000Z',
    })).toThrow(/Updater migration state is invalid/);
  });

  it('publishes exact exchange inventory and preserves an unchanged export byte-for-byte', async () => {
    const userData = await mkdtemp(join(tmpdir(), 'butter-paper-electron-export-'));
    const plan = fixture();
    const first = await publishElectronMigrationExport(userData, plan);
    const root = join(userData, 'gpui-migration-export', 'v1');
    const before = await readFile(join(root, 'manifest.json'));
    expect(first).toEqual({ outcome: 'created', exportId: plan.exportId, createdAt: plan.createdAt });
    expect((await readdir(root)).sort()).toEqual(['manifest.json', 'templates']);
    expect([...(await readFile(join(root, plan.files[0].path)))]).toEqual([...plan.files[0].bytes]);

    const second = await publishElectronMigrationExport(userData, plan);
    expect(second).toEqual({ outcome: 'already-present', exportId: plan.exportId, createdAt: plan.createdAt });
    expect(await readFile(join(root, 'manifest.json'))).toEqual(before);
  });

  it('replaces a changed export and recovers an interrupted previous-tree rename', async () => {
    const userData = await mkdtemp(join(tmpdir(), 'butter-paper-electron-replace-'));
    const first = fixture();
    await publishElectronMigrationExport(userData, first);
    const parent = join(userData, 'gpui-migration-export');
    const root = join(parent, 'v1');
    const changed = createElectronMigrationExportPlan({
      metadata: metadata('stable'),
      renderer: { ...renderer, menuBarVisible: true },
      updateSettings,
      imported: {
        records: [{
          id: 'imported-01234567-89ab-cdef-0123-456789abcdef', name: 'Title block',
          kind: 'imported-pdf', pageCount: 1, createdAt: '2026-09-27T00:00:00.000Z',
        }],
        sources: [{
          id: 'imported-01234567-89ab-cdef-0123-456789abcdef',
          bytes: new Uint8Array([37, 80, 68, 70, 45, 49, 46, 55]),
        }],
      },
      createdAt: '2026-09-28T00:01:00.000Z',
    });
    expect((await publishElectronMigrationExport(userData, changed)).outcome).toBe('created');
    expect(JSON.parse(await readFile(join(root, 'manifest.json'), 'utf8')).preferences.menuBarVisible).toBe(true);
    expect(await readdir(parent)).toEqual(['v1']);

    await rename(root, join(parent, '.v1.previous'));
    expect(await publishElectronMigrationExport(userData, changed)).toEqual({
      outcome: 'already-present', exportId: changed.exportId, createdAt: changed.createdAt,
    });
    expect(await readdir(parent)).toEqual(['v1']);
  });

  it('removes only sentinel-owned crash staging and fails closed on unknown state', async () => {
    const userData = await mkdtemp(join(tmpdir(), 'butter-paper-electron-recovery-'));
    const plan = fixture();
    const parent = join(userData, 'gpui-migration-export');
    const stageName = `.v1.staging-${plan.exportId}`;
    await mkdir(join(parent, stageName), { recursive: true });
    await writeFile(join(parent, stageName, 'partial'), 'partial');
    await writeFile(join(parent, `${stageName}.owner`), `${plan.exportId}\n`);
    expect((await publishElectronMigrationExport(userData, plan)).outcome).toBe('created');
    expect(await readdir(parent)).toEqual(['v1']);
    const ownerOnlyId = 'b'.repeat(64);
    await writeFile(join(parent, `.v1.staging-${ownerOnlyId}.owner`), `${ownerOnlyId}\n`);
    expect((await publishElectronMigrationExport(userData, plan)).outcome).toBe('already-present');
    expect(await readdir(parent)).toEqual(['v1']);

    const invalidUserData = await mkdtemp(join(tmpdir(), 'butter-paper-electron-unknown-'));
    await mkdir(join(invalidUserData, 'gpui-migration-export'), { recursive: true });
    await writeFile(join(invalidUserData, 'gpui-migration-export', 'unknown'), 'do not remove');
    await expect(publishElectronMigrationExport(invalidUserData, plan)).rejects.toThrow(/unrecognised entry/);
    expect(await readFile(join(invalidUserData, 'gpui-migration-export', 'unknown'), 'utf8')).toBe('do not remove');
  });

  it('rejects a manifest changed after publication without replacing its files', async () => {
    const userData = await mkdtemp(join(tmpdir(), 'butter-paper-electron-tamper-'));
    const plan = fixture();
    await publishElectronMigrationExport(userData, plan);
    const manifestPath = join(userData, 'gpui-migration-export', 'v1', 'manifest.json');
    const manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
    manifest.unsupported.session = 'copied';
    await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
    await expect(publishElectronMigrationExport(userData, plan)).rejects.toThrow(/identity checksum failed/);
    expect(JSON.parse(await readFile(manifestPath, 'utf8')).unsupported.session).toBe('copied');
  });

  it.each(['created', 'already-present'] as const)(
    'installs only after a valid %s migration publication receipt',
    async (outcome) => {
      const install = vi.fn(async () => true);
      const publication = {
        outcome,
        exportId: 'a'.repeat(64),
        createdAt: '2026-09-28T00:00:00.000Z',
      };
      const exportMigration = vi.fn(async () => publication);
      await installDownloadedAfterMigration(exportMigration, install);
      expect(exportMigration).toHaveBeenCalledOnce();
      expect(install).toHaveBeenCalledWith(publication);
    },
  );

  it('never invokes updater installation without valid publication evidence', async () => {
    const failingInstall = vi.fn(async () => true);
    await expect(installDownloadedAfterMigration(async () => {
      throw new Error('export failed');
    }, failingInstall)).rejects.toThrow('export failed');
    expect(failingInstall).not.toHaveBeenCalled();

    const invalidReceipts: unknown[] = [
      undefined,
      { outcome: 'created', exportId: 'not-a-digest', createdAt: '2026-09-28T00:00:00.000Z' },
      { outcome: 'invalid', exportId: 'a'.repeat(64), createdAt: '2026-09-28T00:00:00.000Z' },
      { outcome: 'created', exportId: 'a'.repeat(64), createdAt: '2026-09-28' },
    ];
    for (const receipt of invalidReceipts) {
      const install = vi.fn(async () => true);
      await expect(installDownloadedAfterMigration(async () => receipt, install))
        .rejects.toThrow('Migration export did not return valid publication evidence.');
      expect(install).not.toHaveBeenCalled();
    }
  });

  it('retains the downloaded update when installation is no longer ready', async () => {
    const publication = {
      outcome: 'created' as const,
      exportId: 'a'.repeat(64),
      createdAt: '2026-09-28T00:00:00.000Z',
    };
    const install = vi.fn(async () => false);
    await expect(installDownloadedAfterMigration(async () => publication, install))
      .rejects.toThrow('No downloaded Butter Paper update is ready to install.');
    expect(install).toHaveBeenCalledOnce();
  });
});
