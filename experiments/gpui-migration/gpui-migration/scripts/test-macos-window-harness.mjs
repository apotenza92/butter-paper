#!/usr/bin/env node
// Test-only source overlay. Production manifests, locks and Cargo cache stay untouched.
import { cp, mkdir, mkdtemp, readFile, readdir, symlink, writeFile } from 'node:fs/promises';
import { dirname, join, relative, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const root = resolve(import.meta.dirname, '..');
const policy = JSON.parse(await readFile(join(root, 'source-preparation-policy.json'), 'utf8'));
function run(command, args, cwd, capture = false) {
  const result = spawnSync(command, args, { cwd, encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
    stdio: capture ? ['ignore', 'pipe', 'pipe'] : 'inherit' });
  if (result.status !== 0) throw new Error(`${command} failed (${result.status}): ${result.stderr ?? ''}`);
  return result.stdout?.trim();
}
if (process.platform !== 'darwin') throw new Error('This harness is scoped to macOS.');
const args = process.argv.slice(2);
if (args.some(arg => /^(--manifest-path|--config|--target-dir|--lockfile-path)/.test(arg))) {
  throw new Error('Harness isolation paths may not be overridden.');
}
const lockBefore = await readFile(join(root, 'Cargo.lock'));
const manifestBefore = await readFile(join(root, 'Cargo.toml'));
const metadata = JSON.parse(run('cargo', ['metadata', '--locked', '--format-version', '1'], root, true));
const gpui = metadata.packages.find(pkg => pkg.name === 'gpui');
if (!gpui?.source?.includes(policy.zed.revision)) throw new Error('GPUI pin drifted.');
const source = run('git', ['rev-parse', '--show-toplevel'], dirname(gpui.manifest_path), true);
const parent = join(root, '.prepared', 'window-harness');
await mkdir(parent, { recursive: true });
const sandbox = await mkdtemp(join(parent, 'run-'));
const zed = join(sandbox, 'zed');
run('git', ['clone', '--shared', '--no-checkout', source, zed], root);
run('git', ['checkout', '--detach', policy.zed.revision], zed);
if (run('git', ['rev-parse', 'HEAD^{tree}'], zed, true) !== policy.zed.tree) throw new Error('GPUI tree drifted.');
run('git', ['apply', '--check', join(root, 'patches/gpui-test-window-handles.patch')], zed);
run('git', ['apply', join(root, 'patches/gpui-test-window-handles.patch')], zed);
const changed = run('git', ['diff', '--name-only'], zed, true);
if (changed !== 'crates/gpui/src/platform/test/window.rs') throw new Error('Test overlay scope drifted.');
// A path workspace bypasses the production Git-source ztracing patch. Preserve
// that existing shim explicitly; this is dependency wiring, not a code change.
const zedManifestPath = join(zed, 'Cargo.toml');
const zedManifest = await readFile(zedManifestPath, 'utf8');
const tracingDependency = 'ztracing = { path = "crates/ztracing" }';
if (!zedManifest.includes(tracingDependency)) throw new Error('Zed tracing dependency drifted.');
await writeFile(zedManifestPath, zedManifest.replace(tracingDependency,
  `ztracing = { path = ${JSON.stringify(join(root, 'vendor/ztracing-shim'))} }`));
const project = join(sandbox, 'migration', 'gpui-migration');
await mkdir(project, { recursive: true });
for (const entry of await readdir(root)) {
  if (entry !== 'target' && entry !== '.prepared') {
    await cp(join(root, entry), join(project, entry), { recursive: true });
  }
}
await mkdir(join(project, '.prepared'), { recursive: true });
await symlink(join(root, policy.prepared.directory), join(project, policy.prepared.directory), 'dir');
await symlink(resolve(root, '../performance'), join(sandbox, 'migration/performance'), 'dir');
console.log(`Isolated test checkout: ${project}`);
// Keep all Zed workspace types on one source identity, including GPUI's
// collections/util dependencies. Their contents remain the exact pinned tree.
const overrides = metadata.packages.filter(pkg => pkg.source?.includes(policy.zed.revision))
  .map(pkg => `patch."${policy.zed.url}".${pkg.name}.path=${JSON.stringify(join(zed, relative(source, dirname(pkg.manifest_path))))}`);
const config = join(sandbox, 'test-overrides.toml');
await writeFile(config, `${overrides.join('\n')}\n`);
console.log(`Test-only Cargo overrides: ${config}`);
try {
  run('cargo', ['test', '--manifest-path', join(project, 'Cargo.toml'),
    '--target-dir', join(parent, 'target'), '--config', config, '--no-fail-fast',
    ...args], project);
} finally {
  if (!(await readFile(join(root, 'Cargo.lock'))).equals(lockBefore)
      || !(await readFile(join(root, 'Cargo.toml'))).equals(manifestBefore)) {
    throw new Error('Production manifest or lock changed during isolated tests.');
  }
}
