import { createHash } from 'node:crypto';
import { lstat, mkdtemp, readFile, rm, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { PdfSigningQuarantine } from './pdfSigningQuarantine';

const roots: string[] = [];
const id = '123e4567-e89b-42d3-a456-426614174000';
const pdf = Buffer.from('%PDF-1.7\nfailed-private-signing-output\n%%EOF\n');

afterEach(async () => {
  await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});

describe('PDF signing quarantine', () => {
  it('retains bounded PDF-only bytes privately and exposes no path', async () => {
    const root = await temporaryRoot();
    const quarantine = new PdfSigningQuarantine(join(root, 'quarantine'), {
      createOpaqueId: () => id,
      now: () => new Date('2026-08-06T00:00:00.000Z'),
    });
    const descriptor = await quarantine.retain(pdf, 'POSTVALIDATION_FAILURE');
    expect(descriptor).toMatchObject({ id, reason: 'POSTVALIDATION_FAILURE', byteLength: pdf.byteLength });
    expect(JSON.stringify(descriptor)).not.toContain(root);
    expect(Buffer.from((await quarantine.read(id)).bytes)).toEqual(pdf);
    const files = [`quarantine-${id}.pdf`, `quarantine-${id}.json`];
    if (process.platform !== 'win32') {
      expect((await lstat(join(root, 'quarantine'))).mode & 0o077).toBe(0);
      for (const file of files) expect((await lstat(join(root, 'quarantine', file))).mode & 0o077).toBe(0);
    }
    const metadata = JSON.parse(await readFile(join(root, 'quarantine', files[1]), 'utf8'));
    expect(Object.keys(metadata).sort()).toEqual(['byteLength', 'createdAt', 'fileName', 'id', 'reason', 'sha256']);
    await quarantine.remove(id);
    await expect(quarantine.read(id)).rejects.toMatchObject({ code: 'NOT_FOUND' });
  });

  it('rejects non-PDF identity material and enforces entry and total bounds', async () => {
    const root = await temporaryRoot();
    const quarantine = new PdfSigningQuarantine(join(root, 'quarantine'), {
      createOpaqueId: () => id,
      maxEntries: 1,
      maxEntryBytes: pdf.byteLength,
      maxTotalBytes: pdf.byteLength,
    });
    await expect(quarantine.retain(Buffer.from('encrypted identity material'), 'ENGINE_FAILURE'))
      .rejects.toMatchObject({ code: 'INVALID_INPUT' });
    await quarantine.retain(pdf, 'ENGINE_FAILURE');
    await expect(quarantine.retain(pdf, 'ENGINE_FAILURE')).rejects.toMatchObject({ code: 'LIMIT_EXCEEDED' });
  });

  it('rejects a symlinked storage root and corrupt or orphaned entries', async () => {
    const root = await temporaryRoot();
    const real = join(root, 'real');
    const linked = join(root, 'linked');
    await writeFile(real, 'not-a-directory');
    await symlink(real, linked);
    const unsafe = new PdfSigningQuarantine(linked);
    await expect(unsafe.list()).rejects.toMatchObject({ code: 'UNSAFE_STORAGE' });

    const storage = join(root, 'storage');
    const quarantine = new PdfSigningQuarantine(storage);
    await quarantine.list();
    await writeFile(join(storage, `quarantine-${id}.pdf`), pdf);
    await expect(quarantine.list()).rejects.toMatchObject({ code: 'CORRUPT_STORAGE' });
  });

  it('rejects undeclared quarantine metadata fields', async () => {
    const root = await temporaryRoot();
    const storage = join(root, 'strict-records');
    const quarantine = new PdfSigningQuarantine(storage, { createOpaqueId: () => id });
    await quarantine.retain(pdf, 'ENGINE_FAILURE');
    const recordPath = join(storage, `quarantine-${id}.json`);
    const record = JSON.parse(await readFile(recordPath, 'utf8'));
    await writeFile(recordPath, `${JSON.stringify({ ...record, sourcePath: '/must-not-be-accepted' })}\n`, { mode: 0o600 });
    await expect(quarantine.list()).rejects.toMatchObject({ code: 'CORRUPT_STORAGE' });
  });

  it('requires and invokes Windows ACL security hooks', async () => {
    const root = await temporaryRoot();
    expect(() => new PdfSigningQuarantine(join(root, 'missing-hooks'), { platform: 'win32' }))
      .toThrow(/ACL hooks/);
    const calls: string[] = [];
    const quarantine = new PdfSigningQuarantine(join(root, 'secured'), {
      platform: 'win32',
      secureRoot: async () => { calls.push('secure'); },
      verifyRoot: async () => { calls.push('verify'); },
    });
    await quarantine.list();
    expect(calls).toEqual(['secure', 'verify', 'verify']);
  });

  it('recovers interrupted create and delete transactions from private tombstones', async () => {
    const root = await temporaryRoot();
    const storage = join(root, 'recovery');
    const quarantine = new PdfSigningQuarantine(storage, {
      createOpaqueId: () => id,
      now: () => new Date('2026-08-06T00:00:00.000Z'),
    });
    await quarantine.list();
    const record = quarantineRecord();
    await writeFile(join(storage, `.creating-quarantine-${id}.json`), `${JSON.stringify(record)}\n`, { mode: 0o600 });
    await writeFile(join(storage, record.fileName), pdf, { mode: 0o600 });
    await expect(quarantine.list()).resolves.toEqual([]);
    await expect(lstat(join(storage, record.fileName))).rejects.toMatchObject({ code: 'ENOENT' });
    await expect(lstat(join(storage, `.creating-quarantine-${id}.json`))).rejects.toMatchObject({ code: 'ENOENT' });

    await quarantine.retain(pdf, 'ENGINE_FAILURE');
    await writeFile(join(storage, `.deleting-quarantine-${id}.json`), `${JSON.stringify({
      ...record, reason: 'ENGINE_FAILURE',
    })}\n`, { mode: 0o600 });
    await expect(quarantine.list()).resolves.toEqual([]);
    await expect(lstat(join(storage, `quarantine-${id}.json`))).rejects.toMatchObject({ code: 'ENOENT' });
    await expect(lstat(join(storage, `quarantine-${id}.pdf`))).rejects.toMatchObject({ code: 'ENOENT' });
  });
});

async function temporaryRoot(): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), 'bp-signing-quarantine-'));
  roots.push(root);
  return root;
}

function quarantineRecord() {
  return {
    id,
    reason: 'POSTVALIDATION_FAILURE',
    byteLength: pdf.byteLength,
    sha256: createHash('sha256').update(pdf).digest('hex'),
    createdAt: '2026-08-06T00:00:00.000Z',
    fileName: `quarantine-${id}.pdf`,
  };
}
