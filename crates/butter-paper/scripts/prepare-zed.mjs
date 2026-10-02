#!/usr/bin/env node
import { access, mkdir, readFile, rename, rm, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { spawnSync } from 'node:child_process';
import { deterministicTreeDigest, fileSha256, loadPolicy, probeDirectory, validateZedPatch } from './source-preparation.mjs';

function git(args, cwd) {
  const result = spawnSync('git', args, {
    cwd,
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
    env: {
      ...process.env,
      GIT_CONFIG_COUNT: '2',
      GIT_CONFIG_KEY_0: 'core.autocrlf',
      GIT_CONFIG_VALUE_0: 'false',
      GIT_CONFIG_KEY_1: 'core.eol',
      GIT_CONFIG_VALUE_1: 'lf',
    },
  });
  if (result.status !== 0) throw new Error(`git ${args.join(' ')}: ${result.stderr}`);
  return result.stdout.trim();
}
export async function verifyZed(root, policy, projectRoot = probeDirectory) {
  const prepared = policy.zedPrepared;
  if (git(['rev-parse', 'HEAD'], root) !== policy.zed.revision || git(['rev-parse', 'HEAD^{tree}'], root) !== policy.zed.tree) throw new Error('prepared Zed revision/tree drifted');
  if (await fileSha256(join(root, policy.zed.licenseFile)) !== policy.zed.licenseSha256) throw new Error('prepared Zed license drifted');
  validateZedPatch(await readFile(join(projectRoot, prepared.patch.path)), policy);
  const changed = git(['diff', 'HEAD', '--name-only'], root).split('\n').filter(Boolean).sort();
  if (JSON.stringify(changed) !== JSON.stringify([...prepared.changedFiles].sort())
      || git(['diff', '--name-only'], root)
      || git(['ls-files', '--others', '--exclude-standard'], root)) throw new Error('prepared Zed patch scope drifted');
  const digest = await deterministicTreeDigest(root);
  if (digest !== prepared.treeSha256) throw new Error(`prepared Zed tree checksum drifted: ${digest}`);
  return { output: root, revision: policy.zed.revision, patchSha256: prepared.patch.sha256, digest };
}
export async function prepareZed(policy, source, projectRoot = probeDirectory) {
  const output = join(projectRoot, policy.zedPrepared.directory);
  try { await access(output); return { status: 'reused', ...await verifyZed(output, policy, projectRoot) }; }
  catch (error) { if (error.code !== 'ENOENT') throw error; }
  validateZedPatch(await readFile(join(projectRoot, policy.zedPrepared.patch.path)), policy);
  await mkdir(dirname(output), { recursive: true });
  const temporary = `${output}.tmp-${process.pid}`;
  try {
    git(source ? ['clone', '--no-hardlinks', '--no-checkout', resolve(source), temporary] : ['clone', '--filter=blob:none', '--no-checkout', policy.zed.url, temporary], projectRoot);
    git(['checkout', '--detach', policy.zed.revision], temporary);
    git(['apply', '--index', join(projectRoot, policy.zedPrepared.patch.path)], temporary);
    const manifestPath = join(temporary, 'Cargo.toml');
    const manifest = await readFile(manifestPath, 'utf8');
    const before = 'ztracing = { path = "crates/ztracing" }';
    if (manifest.split(before).length !== 2) throw new Error('Zed tracing dependency drifted');
    await writeFile(manifestPath, manifest.replace(before, 'ztracing = { path = "../../vendor/ztracing-shim" }'));
    git(['add', 'Cargo.toml'], temporary);
    await verifyZed(temporary, policy, projectRoot);
    await rename(temporary, output);
    return { status: 'prepared', ...await verifyZed(output, policy, projectRoot) };
  } catch (error) { await rm(temporary, { recursive: true, force: true }); throw error; }
}
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const policy = await loadPolicy();
  const command = process.argv[2] ?? 'prepare';
  const sourceIndex = process.argv.indexOf('--source');
  if (!['prepare', 'verify'].includes(command)) throw new Error('use prepare or verify');
  console.log(JSON.stringify(command === 'prepare'
    ? await prepareZed(policy, sourceIndex < 0 ? undefined : process.argv[sourceIndex + 1])
    : await verifyZed(join(probeDirectory, policy.zedPrepared.directory), policy), null, 2));
}
