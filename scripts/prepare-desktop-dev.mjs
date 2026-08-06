import { execFileSync, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const repositoryRoot = resolve(fileURLToPath(new URL('..', import.meta.url)));

export function assertVersionParity(rootVersion, desktopVersion) {
  if (rootVersion !== desktopVersion) {
    throw new Error(
      `Desktop version ${desktopVersion} does not match repository version ${rootVersion}.`,
    );
  }
  return rootVersion;
}

export function classifySourceFreshness({ head, remoteHead, remoteIsAncestor }) {
  if (head === remoteHead) {
    return 'current';
  }
  if (remoteIsAncestor) {
    return 'ahead';
  }
  return 'behind-or-diverged';
}

export function assertFreshSource(args) {
  const classification = classifySourceFreshness(args);
  if (classification === 'behind-or-diverged') {
    throw new Error(
      `Local HEAD ${shortSha(args.head)} is behind or diverged from ${args.remoteLabel} ${shortSha(args.remoteHead)}. `
      + 'Update the branch before launching the desktop development app.',
    );
  }
  return classification;
}

export function readPackageVersion(packagePath) {
  const parsed = JSON.parse(readFileSync(packagePath, 'utf8'));
  if (typeof parsed.version !== 'string' || parsed.version.trim().length === 0) {
    throw new Error(`Missing package version in ${packagePath}`);
  }
  return parsed.version;
}

export function prepareGeneratedOutput(root = repositoryRoot) {
  rmSync(resolve(root, 'apps/desktop/.vite'), { recursive: true, force: true });
}

export function createDevProvenance({ root, version, head, branch, freshness, remoteHead, releaseTag }) {
  const status = git(root, ['status', '--porcelain=v1', '--untracked-files=all']);
  return {
    schemaVersion: 1,
    version,
    commit: head,
    branch,
    dirty: status.length > 0,
    statusFingerprint: createHash('sha256').update(status).digest('hex'),
    checkoutId: createHash('sha256').update(resolve(root)).digest('hex'),
    freshness,
    remoteCommit: remoteHead,
    releaseTag,
    generatedAt: new Date().toISOString(),
  };
}

export function writeDevProvenance(root, provenance) {
  const outputPath = resolve(root, 'test-results/desktop-dev-provenance.json');
  mkdirSync(resolve(root, 'test-results'), { recursive: true });
  writeFileSync(outputPath, `${JSON.stringify(provenance, null, 2)}\n`, { mode: 0o600 });
  return outputPath;
}

export function runDevPreflight({
  root = repositoryRoot,
  remote = process.env.BP_DEV_REMOTE?.trim() || 'origin',
  baseBranch = process.env.BP_DEV_BASE_BRANCH?.trim() || 'main',
  skipFreshnessCheck = process.env.BP_DEV_SKIP_FRESHNESS_CHECK === '1',
} = {}) {
  const version = assertVersionParity(
    readPackageVersion(resolve(root, 'package.json')),
    readPackageVersion(resolve(root, 'apps/desktop/package.json')),
  );
  const head = git(root, ['rev-parse', 'HEAD']);
  const branch = git(root, ['branch', '--show-current']) || '(detached)';
  let freshness = 'unverified';
  let remoteHead = null;

  if (skipFreshnessCheck) {
    process.stderr.write(
      'Warning: BP_DEV_SKIP_FRESHNESS_CHECK=1; remote source freshness is not verified.\n',
    );
  } else {
    const fetch = spawnSync('git', ['fetch', '--quiet', '--no-tags', remote, baseBranch], {
      cwd: root,
      encoding: 'utf8',
      env: process.env,
    });
    if (fetch.status !== 0) {
      const details = fetch.stderr?.trim() || fetch.stdout?.trim() || 'unknown git fetch error';
      throw new Error(
        `Unable to verify ${remote}/${baseBranch} before desktop launch: ${details}. `
        + 'Connect to the remote or explicitly set BP_DEV_SKIP_FRESHNESS_CHECK=1.',
      );
    }

    remoteHead = git(root, ['rev-parse', 'FETCH_HEAD']);
    const ancestry = spawnSync('git', ['merge-base', '--is-ancestor', remoteHead, head], {
      cwd: root,
      encoding: 'utf8',
      env: process.env,
    });
    if (ancestry.status !== 0 && ancestry.status !== 1) {
      throw new Error(ancestry.stderr?.trim() || 'Unable to compare local and remote source history.');
    }
    freshness = assertFreshSource({
      head,
      remoteHead,
      remoteIsAncestor: ancestry.status === 0,
      remoteLabel: `${remote}/${baseBranch}`,
    });
  }

  prepareGeneratedOutput(root);
  const releaseTag = tagForHead(root, head);
  const provenance = createDevProvenance({ root, version, head, branch, freshness, remoteHead, releaseTag });
  writeDevProvenance(root, provenance);
  process.stdout.write(
    `Desktop dev preflight: v${version} ${shortSha(head)} on ${branch}; `
    + `${remote}/${baseBranch} ${freshness}${releaseTag ? `; tag ${releaseTag}` : ''}.\n`
    + `Generated development provenance (${provenance.dirty ? 'dirty' : 'clean'}); shared packages will be rebuilt before launch.\n`,
  );

  return provenance;
}

function git(root, args) {
  return execFileSync('git', args, {
    cwd: root,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
    env: process.env,
  }).trim();
}

function tagForHead(root, head) {
  const tags = git(root, ['tag', '--points-at', head])
    .split('\n')
    .map((tag) => tag.trim())
    .filter((tag) => /^v\d+\.\d+\.\d+(?:-beta\.\d+)?$/.test(tag));
  return tags.sort((left, right) => left.localeCompare(right, undefined, { numeric: true })).at(-1) ?? null;
}

function shortSha(sha) {
  return sha.slice(0, 8);
}

const invokedPath = process.argv[1] ? pathToFileURL(resolve(process.argv[1])).href : null;
if (invokedPath === import.meta.url) {
  try {
    runDevPreflight();
  } catch (error) {
    process.stderr.write(`Desktop dev preflight failed: ${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  }
}
