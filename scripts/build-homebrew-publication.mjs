import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { basename, join, resolve } from 'node:path';
import process from 'node:process';
import { pathToFileURL } from 'node:url';
import { renderCasks } from './render-homebrew-casks.mjs';

const sha256 = path => createHash('sha256').update(readFileSync(path)).digest('hex');

export function buildHomebrewPublication({ release, assetsDirectory, outputDirectory, repository, commit, runId, runAttempt }) {
  const channel = release.tag.includes('-beta.') ? 'beta' : 'stable';
  const channels = Object.keys(release.channels);
  const expectedChannels = channel === 'beta' ? ['beta'] : ['stable', 'beta'];
  if (JSON.stringify(channels) !== JSON.stringify(expectedChannels)) throw new Error('Homebrew channel contract mismatch');
  const casks = renderCasks(release);
  mkdirSync(join(outputDirectory, 'Casks'), { recursive: true });
  for (const [name, content] of Object.entries(casks)) writeFileSync(join(outputDirectory, 'Casks', name), content);
  const artifacts = [];
  for (const publicationChannel of channels) {
    for (const architecture of ['arm64', 'x64']) {
      const file = release.channels[publicationChannel].files[architecture];
      const name = basename(new URL(file.url).pathname);
      const path = join(assetsDirectory, name);
      if (sha256(path) !== file.sha256) throw new Error(`Homebrew artifact digest mismatch: ${name}`);
      artifacts.push({
        name,
        url: file.url,
        size: statSync(path).size,
        sha256: file.sha256,
        channel: publicationChannel,
        architecture,
      });
    }
  }
  const manifest = {
    schema_version: 1,
    product: 'butter-paper',
    source_repository: repository,
    release_tag: release.tag,
    release_commit: commit,
    channel,
    casks: Object.keys(casks),
    artifacts,
    applications: Object.fromEntries(channels.map(value => [value, release.channels[value].app])),
    bundle_identifiers: Object.fromEntries(channels.map(value => [value, value === 'beta' ? 'com.butterpaper.desktop.beta' : 'com.butterpaper.desktop'])),
    architectures: ['arm64', 'x64'],
    minimum_macos: '12.0',
    native_validation: {
      workflow_run_id: Number(runId),
      workflow_run_attempt: Number(runAttempt),
      jobs: ['Test Homebrew casks (arm64)', 'Test Homebrew casks (x64)'],
    },
  };
  writeFileSync(join(outputDirectory, 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`);
  return manifest;
}

function option(name) {
  const index = process.argv.indexOf(name);
  return index < 0 ? null : process.argv[index + 1];
}

export function main() {
  const releasePath = resolve(option('--release') ?? 'publish/assets/homebrew-release.json');
  const assetsDirectory = resolve(option('--assets') ?? 'publish/assets');
  const outputDirectory = resolve(option('--output') ?? 'homebrew-publication');
  buildHomebrewPublication({
    release: JSON.parse(readFileSync(releasePath, 'utf8')),
    assetsDirectory,
    outputDirectory,
    repository: process.env.GITHUB_REPOSITORY,
    commit: process.env.GITHUB_SHA,
    runId: process.env.GITHUB_RUN_ID,
    runAttempt: process.env.GITHUB_RUN_ATTEMPT,
  });
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) main();
