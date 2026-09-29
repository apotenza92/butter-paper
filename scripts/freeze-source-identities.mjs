#!/usr/bin/env node
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { lstat, mkdir, readFile, realpath, rename, rm, writeFile } from 'node:fs/promises';
import { dirname, isAbsolute, relative, resolve, sep } from 'node:path';
import { pathToFileURL } from 'node:url';

const schema = 'butter-paper/source-identity-freeze';
const version = 1;
const boundaryPolicy = 'git-index-and-nonignored-worktree-v1';
const utf8 = new TextDecoder('utf-8', { fatal: true });

function sha256(value) {
  return createHash('sha256').update(value).digest('hex');
}

function git(root, args, { allowFailure = false } = {}) {
  const result = spawnSync('git', args, {
    cwd: root,
    encoding: null,
    env: { ...process.env, GIT_OPTIONAL_LOCKS: '0' },
    maxBuffer: 128 * 1024 * 1024,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  if (result.error) throw result.error;
  if (result.status !== 0 && !allowFailure) {
    throw new Error(`git ${args[0]} exited ${result.status}: ${result.stderr.toString('utf8').trim()}`);
  }
  return result;
}

function nulRecords(bytes) {
  if (bytes.length === 0) return [];
  if (bytes.at(-1) !== 0) throw new Error('Git emitted an unterminated path record');
  return bytes.subarray(0, -1).toString('binary').split('\0').map((record) => Buffer.from(record, 'binary'));
}

function decodePath(bytes) {
  let path;
  try {
    path = utf8.decode(bytes);
  } catch {
    throw new Error('Source boundary contains a non-UTF-8 path');
  }
  if (Buffer.compare(Buffer.from(path, 'utf8'), bytes) !== 0) {
    throw new Error('Source boundary path is not canonical UTF-8');
  }
  if (!path || isAbsolute(path) || path.split('/').some((part) => part === '' || part === '.' || part === '..')) {
    throw new Error(`Unsafe source-boundary path: ${JSON.stringify(path)}`);
  }
  return path;
}

function parseIndex(bytes) {
  const entries = new Map();
  for (const record of nulRecords(bytes)) {
    const tab = record.indexOf(0x09);
    if (tab < 0) throw new Error('Malformed Git index record');
    const header = record.subarray(0, tab).toString('ascii');
    const match = /^(\d{6}) ([0-9a-f]{40,64}) ([0-3])$/.exec(header);
    if (!match) throw new Error('Malformed Git index metadata');
    const path = decodePath(record.subarray(tab + 1));
    if (match[3] !== '0') throw new Error(`Unmerged source-boundary path: ${path}`);
    if (entries.has(path)) throw new Error(`Duplicate source-boundary path: ${path}`);
    if (match[1] === '120000') throw new Error(`Symlink is forbidden in the source boundary: ${path}`);
    if (match[1] === '160000') throw new Error(`Gitlink is forbidden in the source boundary: ${path}`);
    if (match[1] !== '100644' && match[1] !== '100755') {
      throw new Error(`Unsupported indexed mode ${match[1]} in the source boundary: ${path}`);
    }
    entries.set(path, { indexMode: match[1], indexOid: match[2] });
  }
  return entries;
}

function statIdentity(info) {
  return [info.dev, info.ino, info.mode, info.size, info.mtimeNs, info.ctimeNs].map(String).join(':');
}

async function inspectPath(root, path) {
  const components = path.split('/');
  let current = root;
  const chain = [];
  for (let index = 0; index < components.length; index += 1) {
    current = resolve(current, components[index]);
    let info;
    try {
      info = await lstat(current, { bigint: true });
    } catch (error) {
      if (error?.code === 'ENOENT') return { exists: false, chainIdentity: chain.join('|') };
      throw error;
    }
    if (info.isSymbolicLink()) {
      throw new Error(`Symlink is forbidden in the source boundary: ${path}`);
    }
    chain.push(statIdentity(info));
    if (index < components.length - 1 && !info.isDirectory()) {
      throw new Error(`Non-directory source-boundary ancestor: ${path}`);
    }
    if (index === components.length - 1) {
      return { exists: true, info, absolute: current, chainIdentity: chain.join('|') };
    }
  }
  throw new Error(`Empty source-boundary path: ${path}`);
}

export async function readStableFile(root, path, { afterFirstRead } = {}) {
  const inspected = await inspectPath(root, path);
  if (!inspected.exists) return null;
  if (!inspected.info.isFile()) throw new Error(`Non-regular source-boundary entry: ${path}`);
  const before = inspected.info;
  const first = await readFile(inspected.absolute);
  await afterFirstRead?.();
  const middleInspection = await inspectPath(root, path);
  if (!middleInspection.exists || !middleInspection.info.isFile()) {
    throw new Error(`Source-boundary entry changed type during stable read: ${path}`);
  }
  const second = await readFile(inspected.absolute);
  const afterInspection = await inspectPath(root, path);
  if (!afterInspection.exists || !afterInspection.info.isFile()) {
    throw new Error(`Source-boundary entry changed type during stable read: ${path}`);
  }
  const middle = middleInspection.info;
  const after = afterInspection.info;
  if (
    inspected.chainIdentity !== middleInspection.chainIdentity ||
    middleInspection.chainIdentity !== afterInspection.chainIdentity ||
    statIdentity(before) !== statIdentity(middle) ||
    statIdentity(middle) !== statIdentity(after) ||
    !first.equals(second)
  ) {
    throw new Error(`Source-boundary entry changed during stable read: ${path}`);
  }
  return {
    worktreeMode: (Number(after.mode) & 0o111) === 0 ? '100644' : '100755',
    bytes: first.length,
    sha256: sha256(first),
  };
}

async function canonicalGitRoot(input) {
  if (!isAbsolute(input)) throw new Error('Checkout roots must be absolute paths');
  const root = await realpath(input);
  const topLevelResult = git(root, ['rev-parse', '--show-toplevel']);
  const topLevel = await realpath(topLevelResult.stdout.toString('utf8').trim());
  if (root !== topLevel) throw new Error(`Checkout root must name the Git worktree root: ${input}`);
  return root;
}

function sourceSnapshot(root) {
  const head = git(root, ['rev-parse', '--verify', 'HEAD']).stdout.toString('ascii').trim();
  if (!/^[0-9a-f]{40,64}$/.test(head)) throw new Error('Git HEAD is not a full object identity');
  const branchResult = git(root, ['symbolic-ref', '--quiet', '--short', 'HEAD'], { allowFailure: true });
  if (branchResult.status !== 0 && branchResult.status !== 1) {
    throw new Error(`git symbolic-ref exited ${branchResult.status}`);
  }
  const branch = branchResult.status === 0 ? branchResult.stdout.toString('utf8').trim() : null;
  const index = git(root, ['ls-files', '--stage', '-z']).stdout;
  const untracked = git(root, ['ls-files', '--others', '--exclude-standard', '-z']).stdout;
  const status = git(root, [
    '-c',
    'status.renames=false',
    'status',
    '--porcelain=v2',
    '-z',
    '--untracked-files=all',
    '--ignore-submodules=none',
  ]).stdout;
  return { head, branch, index, untracked, status };
}

function snapshotsEqual(left, right) {
  return (
    left.head === right.head &&
    left.branch === right.branch &&
    left.index.equals(right.index) &&
    left.untracked.equals(right.untracked) &&
    left.status.equals(right.status)
  );
}

async function capturePass(root, snapshot) {
  const indexed = parseIndex(snapshot.index);
  const untracked = nulRecords(snapshot.untracked).map(decodePath);
  for (const path of untracked) {
    if (indexed.has(path)) throw new Error(`Path is both indexed and untracked: ${path}`);
    indexed.set(path, null);
  }
  const files = [];
  for (const path of [...indexed.keys()].sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)))) {
    const index = indexed.get(path);
    const worktree = await readStableFile(root, path);
    if (!worktree && !index) throw new Error(`Untracked source-boundary path disappeared: ${path}`);
    files.push({
      path,
      tracked: index !== null,
      ...(index ?? {}),
      ...(worktree ?? { worktreeMissing: true }),
    });
  }
  const boundarySha256 = sha256(
    files.map((file) => `${JSON.stringify(file)}\n`).join(''),
  );
  return { files, boundarySha256 };
}

export async function captureSourceIdentity(input, label) {
  const root = await canonicalGitRoot(input);
  const before = sourceSnapshot(root);
  const first = await capturePass(root, before);
  const middle = sourceSnapshot(root);
  if (!snapshotsEqual(before, middle)) throw new Error(`${label} Git identity changed during source freeze`);
  const second = await capturePass(root, middle);
  const after = sourceSnapshot(root);
  if (!snapshotsEqual(middle, after) || JSON.stringify(first) !== JSON.stringify(second)) {
    throw new Error(`${label} source boundary changed during source freeze`);
  }
  return {
    label,
    head: before.head,
    branch: before.branch,
    dirty: before.status.length > 0,
    statusSha256: sha256(before.status),
    boundaryPolicy,
    boundarySha256: first.boundarySha256,
    fileCount: first.files.length,
    files: first.files,
  };
}

export async function createSourceIdentityReceipt({ gpuiRoot, electronRoot }) {
  const gpui = await canonicalGitRoot(gpuiRoot);
  const electron = await canonicalGitRoot(electronRoot);
  if (gpui === electron) throw new Error('GPUI and Electron checkouts must be distinct');
  const sources = {
    gpui: await captureSourceIdentity(gpui, 'gpui'),
    electron: await captureSourceIdentity(electron, 'electron'),
  };
  const payload = {
    schema,
    version,
    policy: {
      boundary: boundaryPolicy,
      tracked: 'index identity plus regular worktree bytes; deleted tracked files are explicit',
      untracked: 'all non-ignored regular files from git ls-files --others --exclude-standard',
      ignored: 'excluded by Git ignore policy',
      links: 'symlinks and gitlinks rejected',
      stability: 'two complete captures; every present file read twice; Git snapshots stable before, between and after',
      checkoutPathsInReceipt: false,
    },
    sources,
  };
  return { ...payload, receiptSha256: sha256(JSON.stringify(payload)) };
}

export function parseArguments(argv) {
  const values = new Map();
  for (let index = 0; index < argv.length; index += 2) {
    const key = argv[index];
    const value = argv[index + 1];
    if (!['--gpui', '--electron', '--output'].includes(key)) throw new Error(`Unknown argument: ${key ?? '<missing>'}`);
    if (!value || value.startsWith('--')) throw new Error(`${key} requires a value`);
    if (values.has(key)) throw new Error(`${key} may be provided only once`);
    values.set(key, value);
  }
  for (const key of ['--gpui', '--electron', '--output']) {
    if (!values.has(key)) throw new Error(`${key} is required`);
  }
  return { gpuiRoot: values.get('--gpui'), electronRoot: values.get('--electron'), output: values.get('--output') };
}

async function assertIgnoredWhenInside(output, root) {
  const path = relative(root, output);
  if (path === '' || path === '..' || path.startsWith(`..${sep}`) || isAbsolute(path)) return;
  const result = git(root, ['check-ignore', '--quiet', '--no-index', '--', path], { allowFailure: true });
  if (result.status !== 0) throw new Error(`Receipt output inside a checkout must be ignored: ${path}`);
}

async function main() {
  const options = parseArguments(process.argv.slice(2));
  if (!isAbsolute(options.output)) throw new Error('--output must be an absolute path');
  const output = resolve(options.output);
  const gpui = await canonicalGitRoot(options.gpuiRoot);
  const electron = await canonicalGitRoot(options.electronRoot);
  await assertIgnoredWhenInside(output, gpui);
  await assertIgnoredWhenInside(output, electron);
  const receipt = await createSourceIdentityReceipt({ gpuiRoot: gpui, electronRoot: electron });
  await mkdir(dirname(output), { recursive: true });
  const temporary = `${output}.tmp-${process.pid}`;
  try {
    await writeFile(temporary, `${JSON.stringify(receipt, null, 2)}\n`, { flag: 'wx', mode: 0o600 });
    await rename(temporary, output);
  } finally {
    await rm(temporary, { force: true });
  }
  process.stdout.write(`${JSON.stringify({ output, receiptSha256: receipt.receiptSha256 })}\n`);
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  main().catch((error) => {
    process.stderr.write(`${error.stack ?? error}\n`);
    process.exitCode = 1;
  });
}
