import assert from 'node:assert/strict';
import { mkdtemp, mkdir, readFile, readdir, rm, stat, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { withOwnedWindowHarnessSandbox } from '../scripts/test-macos-window-harness.mjs';

test('removes the exact owned sandbox after setup failure and preserves the shared target', async () => {
  const temporaryRoot = await mkdtemp(join(tmpdir(), 'butter-paper-window-harness-'));
  const parent = join(temporaryRoot, 'window-harness');
  const sharedTarget = join(parent, 'target');
  const sharedSentinel = join(sharedTarget, 'sentinel');
  let ownedSandbox;

  try {
    await mkdir(sharedTarget, { recursive: true });
    await writeFile(sharedSentinel, 'shared target remains\n');

    await assert.rejects(
      withOwnedWindowHarnessSandbox(parent, async sandbox => {
        ownedSandbox = sandbox;
        await mkdir(join(sandbox, 'migration', 'gpui-migration'), { recursive: true });
        await writeFile(join(sandbox, 'copied-input'), 'partial setup\n');
        throw new Error('injected copy, prepare, or validation failure');
      }),
      /injected copy, prepare, or validation failure/,
    );

    assert.ok(ownedSandbox);
    await assert.rejects(stat(ownedSandbox), error => error?.code === 'ENOENT');
    assert.equal(await readFile(sharedSentinel, 'utf8'), 'shared target remains\n');
    assert.deepEqual(await readdir(parent), ['target']);
  } finally {
    await rm(temporaryRoot, { recursive: true, force: true });
  }
});
