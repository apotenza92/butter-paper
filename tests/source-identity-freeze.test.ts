import { mkdtemp, mkdir, readFile, rm, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { describe, expect, it } from 'vitest';

import {
  captureSourceIdentity,
  createSourceIdentityReceipt,
  parseArguments,
  readStableFile,
} from '../scripts/freeze-source-identities.mjs';

function git(root: string, args: string[]) {
  const result = spawnSync('git', args, {
    cwd: root,
    encoding: 'utf8',
    env: { ...process.env, GIT_OPTIONAL_LOCKS: '0' },
  });
  if (result.status !== 0) throw new Error(result.stderr);
  return result.stdout.trim();
}

async function repository(name: string) {
  const root = await mkdtemp(join(tmpdir(), `bp-source-freeze-${name}-`));
  git(root, ['init', '-b', 'main']);
  await writeFile(join(root, '.gitignore'), 'ignored/\n');
  await writeFile(join(root, 'tracked.txt'), 'tracked\n');
  git(root, ['add', '.gitignore', 'tracked.txt']);
  git(root, ['-c', 'user.name=Source Freeze Test', '-c', 'user.email=source-freeze@example.invalid', 'commit', '-m', 'fixture']);
  return root;
}

describe('source identity freeze', () => {
  it('requires explicit, non-duplicated checkout and receipt paths', () => {
    expect(parseArguments(['--gpui', '/gpui', '--electron', '/electron', '--output', '/receipt.json'])).toEqual({
      gpuiRoot: '/gpui',
      electronRoot: '/electron',
      output: '/receipt.json',
    });
    expect(() => parseArguments(['--gpui', '/gpui'])).toThrow('--electron is required');
    expect(() => parseArguments(['--gpui', '/one', '--gpui', '/two'])).toThrow(
      '--gpui may be provided only once',
    );
  });

  it('binds HEAD, branch, staged state and every non-ignored worktree byte without absolute paths', async () => {
    const gpui = await repository('gpui');
    const electron = await repository('electron');
    try {
      await writeFile(join(gpui, 'tracked.txt'), 'dirty one\n');
      await writeFile(join(gpui, 'untracked.txt'), 'untracked\n');
      await mkdir(join(gpui, 'ignored'));
      await writeFile(join(gpui, 'ignored/output.bin'), 'ignored output\n');
      git(electron, ['mv', 'tracked.txt', 'renamed.txt']);

      const receipt = await createSourceIdentityReceipt({ gpuiRoot: gpui, electronRoot: electron });
      expect(receipt.schema).toBe('butter-paper/source-identity-freeze');
      expect(receipt.sources.gpui.branch).toBe('main');
      expect(receipt.sources.gpui.dirty).toBe(true);
      expect(receipt.sources.electron.dirty).toBe(true);
      expect(receipt.sources.gpui.files.map((file: { path: string }) => file.path)).toContain('untracked.txt');
      expect(receipt.sources.gpui.files.map((file: { path: string }) => file.path)).not.toContain('ignored/output.bin');
      expect(JSON.stringify(receipt)).not.toContain(gpui);
      expect(JSON.stringify(receipt)).not.toContain(electron);
      expect(receipt.receiptSha256).toMatch(/^[0-9a-f]{64}$/);

      const same = await createSourceIdentityReceipt({ gpuiRoot: gpui, electronRoot: electron });
      expect(same).toEqual(receipt);
    } finally {
      await Promise.all([rm(gpui, { recursive: true, force: true }), rm(electron, { recursive: true, force: true })]);
    }
  });

  it('detects byte drift even when the dirty status classification is unchanged', async () => {
    const root = await repository('drift');
    try {
      await writeFile(join(root, 'tracked.txt'), 'dirty one\n');
      const first = await captureSourceIdentity(root, 'fixture');
      await writeFile(join(root, 'tracked.txt'), 'dirty two\n');
      const second = await captureSourceIdentity(root, 'fixture');
      expect(first.statusSha256).toBe(second.statusSha256);
      expect(first.boundarySha256).not.toBe(second.boundarySha256);

      await expect(
        readStableFile(root, 'tracked.txt', {
          afterFirstRead: () => writeFile(join(root, 'tracked.txt'), 'changed during read\n'),
        }),
      ).rejects.toThrow('changed during stable read');
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });

  it('rejects worktree symlinks instead of following them', async () => {
    const root = await repository('symlink');
    try {
      await writeFile(join(root, 'outside.txt'), 'outside\n');
      await symlink(join(root, 'outside.txt'), join(root, 'alias.txt'));
      await expect(captureSourceIdentity(root, 'fixture')).rejects.toThrow(
        'Symlink is forbidden in the source boundary: alias.txt',
      );
      expect(await readFile(join(root, 'outside.txt'), 'utf8')).toBe('outside\n');
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });
});
