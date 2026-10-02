#!/usr/bin/env node
// Owned test overlay of the same checksum-verified Zed source used in production.
import { cp, lstat, mkdir, mkdtemp, readFile, readdir, rm, symlink, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import { prepareZed, verifyZed } from './prepare-zed.mjs';
import { validateCargoMetadata } from './source-preparation.mjs';
const root = resolve(import.meta.dirname, '..');
const policy = JSON.parse(await readFile(join(root, 'source-preparation-policy.json'), 'utf8'));
const cargoCacheTag = `Signature: 8a477f597d28d172789f06886806bc55
# This file is a cache directory tag created by Butter Paper's Cargo harness.
# For information about cache directory tags, see https://bford.info/cachedir/
`;
function run(command, args, cwd, capture = false) {
  const result = spawnSync(command, args, { cwd, encoding: 'utf8', maxBuffer: 128 * 1024 * 1024,
    stdio: capture ? ['ignore', 'pipe', 'pipe'] : 'inherit' });
  if (result.status !== 0) throw new Error(`${command} failed (${result.status}): ${result.stderr ?? ''}`);
  return result.stdout?.trim();
}

export async function withOwnedWindowHarnessSandbox(parent, operation) {
  await mkdir(parent, { recursive: true });
  const sandbox = await mkdtemp(join(parent, 'run-'));
  try {
    return await operation(sandbox);
  } finally {
    await rm(sandbox, { recursive: true, force: true });
  }
}

export async function main(args = process.argv.slice(2)) {
  if (process.platform !== 'darwin') throw new Error('This harness is scoped to macOS.');
  if (args.some(arg => /^(--manifest-path|--config|--target-dir|--lockfile-path)/.test(arg))) throw new Error('Harness isolation paths may not be overridden.');
  const lockBefore = await readFile(join(root, 'Cargo.lock'));
  const manifestBefore = await readFile(join(root, 'Cargo.toml'));
  const source = join(root, policy.zedPrepared.directory);
  const parent = join(root, '.prepared', 'window-harness');
  try {
    await verifyZed(source, policy);
    const metadata = JSON.parse(run('cargo', ['metadata', '--locked', '--offline', '--format-version', '1'], root, true));
    validateCargoMetadata(metadata, policy);
    await withOwnedWindowHarnessSandbox(parent, async sandbox => {
      const project = join(sandbox, 'migration', 'gpui-migration');
      await mkdir(project, { recursive: true });
      for (const entry of await readdir(root)) {
        if (entry !== 'target' && entry !== '.prepared') await cp(join(root, entry), join(project, entry), { recursive: true });
      }
      await mkdir(join(project, '.prepared'), { recursive: true });
      await symlink(join(root, policy.prepared.directory), join(project, policy.prepared.directory), 'dir');
      await symlink(resolve(root, '../performance'), join(sandbox, 'migration/performance'), 'dir');
      await prepareZed(policy, source, project);
      const zed = join(project, policy.zedPrepared.directory);
      const expected = [...policy.zedPrepared.changedFiles].sort();
      const changed = run('git', ['diff', 'HEAD', '--name-only'], zed, true).split('\n').sort();
      if (JSON.stringify(changed) !== JSON.stringify(expected)) throw new Error('Test overlay scope drifted.');
      console.log(`Isolated test checkout: ${project}`);
      // Both gpui-component build scripts export the absolute icon directory from
      // CARGO_MANIFEST_DIR. The shared target therefore cannot reuse their output
      // after the owning run-* sandbox has been removed. Keep the expensive shared
      // target, but rebuild only the two path-bearing packages for this sandbox.
      const sharedTarget = join(parent, 'target');
      await mkdir(sharedTarget, { recursive: true });
      const targetInfo = await lstat(sharedTarget);
      if (!targetInfo.isDirectory() || targetInfo.isSymbolicLink()) {
        throw new Error('Shared window-harness target must be a real directory.');
      }
      const cacheTag = join(sharedTarget, 'CACHEDIR.TAG');
      try {
        await writeFile(cacheTag, cargoCacheTag, { flag: 'wx' });
      } catch (error) {
        if (error?.code !== 'EEXIST') throw error;
      }
      run('cargo', ['clean', '--locked', '--offline', '--manifest-path', join(project, 'Cargo.toml'),
        '--target-dir', sharedTarget, '--package', 'gpui-component',
        '--package', 'gpui-component-assets'], root);
      run('python3', [join(root, 'scripts/run-native-bounded.py'), 'cargo', 'test', '--locked',
        '--manifest-path', join(project, 'Cargo.toml'), '--target-dir', join(parent, 'target'),
        '--no-fail-fast', ...args], root);
    });
  } finally {
    const productionInputsChanged = !(await readFile(join(root, 'Cargo.lock'))).equals(lockBefore)
      || !(await readFile(join(root, 'Cargo.toml'))).equals(manifestBefore);
    if (productionInputsChanged) throw new Error('Production manifest or lock changed during isolated tests.');
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  await main();
}
