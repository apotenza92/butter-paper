#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join, resolve } from 'node:path';

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');

export function parseTarget(argv, host = { platform: process.platform, arch: process.arch }) {
  const options = {};
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument !== '--platform' && argument !== '--arch') throw new Error(`Unknown sidecar build argument: ${argument}`);
    const value = argv[index + 1];
    if (!value || value.startsWith('--')) throw new Error(`${argument} requires a value`);
    options[argument.slice(2)] = value;
    index += 1;
  }
  const platform = options.platform ?? host.platform;
  const arch = options.arch ?? host.arch;
  if (!['darwin', 'win32', 'linux'].includes(platform)) throw new Error(`Unsupported sidecar build platform: ${platform}`);
  if (!['arm64', 'x64'].includes(arch)) throw new Error(`Unsupported sidecar build architecture: ${arch}`);
  if (platform !== host.platform || arch !== host.arch) {
    throw new Error(`PDF signature core packages are native-only; requested ${platform}-${arch} on ${host.platform}-${host.arch}.`);
  }
  return { platform, arch };
}

export function buildCommands(target) {
  const sidecarRoot = join(repositoryRoot, 'native/pdf-signature-core');
  const packageRoot = join(sidecarRoot, 'build/package', `${target.platform}-${target.arch}`);
  const sourceArtifactRoot = join(sidecarRoot, 'build/source-artifact', `${target.platform}-${target.arch}`);
  const sourceArtifact = join(sourceArtifactRoot, 'complete-source-artifact-selected-after-build');
  return {
    build: {
      command: 'bash',
      args: [join(sidecarRoot, 'scripts/build-native-package.sh'), target.platform, target.arch],
    },
    verify: {
      command: process.execPath,
      args: [
        join(repositoryRoot, 'scripts/verify-pdf-signature-core-package.mjs'),
        '--package-root', packageRoot,
        '--platform', target.platform,
        '--arch', target.arch,
        '--verification-mode', 'proof',
        '--source-artifact', sourceArtifact,
      ],
    },
    packageRoot,
    sourceArtifactRoot,
  };
}

function run(command) {
  const result = spawnSync(command.command, command.args, {
    cwd: repositoryRoot,
    env: process.env,
    stdio: 'inherit',
    windowsHide: true,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command.command} failed with exit code ${result.status ?? 'unknown'}.`);
}

function main() {
  const commands = buildCommands(parseTarget(process.argv.slice(2)));
  run(commands.build);
  const descriptor = JSON.parse(readFileSync(join(commands.packageRoot, 'complete-source-artifact.json'), 'utf8'));
  const sourceArtifact = join(commands.sourceArtifactRoot, descriptor.delivery.canonicalFileName);
  const markerIndex = commands.verify.args.findIndex((argument) => (
    argument.endsWith('complete-source-artifact-selected-after-build')
  ));
  if (markerIndex === -1) throw new Error('Source artifact verifier argument is missing.');
  commands.verify.args[markerIndex] = sourceArtifact;
  run(commands.verify);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  }
}
