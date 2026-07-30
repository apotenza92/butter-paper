#!/usr/bin/env node

const { existsSync, mkdtempSync, readFileSync, rmSync, statSync } = require('node:fs');
const { createRequire } = require('node:module');
const { tmpdir } = require('node:os');
const path = require('node:path');
const asar = require('@electron/asar');

const expectedVersions = {
  braceExpansion: '5.0.9',
  minimatch: '10.2.6',
};

function packageMetadataFor(entryPath) {
  let directory = path.dirname(entryPath);
  while (directory !== path.dirname(directory)) {
    const candidate = path.join(directory, 'package.json');
    if (existsSync(candidate)) {
      const metadata = JSON.parse(readFileSync(candidate, 'utf8'));
      if (typeof metadata.name === 'string' && typeof metadata.version === 'string') {
        return metadata;
      }
    }
    directory = path.dirname(directory);
  }
  throw new Error(`Could not locate package metadata for ${entryPath}`);
}

function verifyDependencyTree(root) {
  const packagedRequire = createRequire(path.join(root, 'package.json'));
  const rolePath = packagedRequire.resolve('@tufjs/models/dist/role.js');
  const modelsRequire = createRequire(rolePath);
  const minimatchPath = modelsRequire.resolve('minimatch');
  const minimatchMetadata = packageMetadataFor(minimatchPath);
  if (minimatchMetadata.version !== expectedVersions.minimatch) {
    throw new Error(
      `Packaged @tufjs/models resolves minimatch ${minimatchMetadata.version}; `
      + `expected ${expectedVersions.minimatch}.`,
    );
  }

  const minimatchRequire = createRequire(minimatchPath);
  const braceExpansionPath = minimatchRequire.resolve('brace-expansion');
  const braceExpansionMetadata = packageMetadataFor(braceExpansionPath);
  if (braceExpansionMetadata.version !== expectedVersions.braceExpansion) {
    throw new Error(
      `Packaged minimatch resolves brace-expansion ${braceExpansionMetadata.version}; `
      + `expected ${expectedVersions.braceExpansion}.`,
    );
  }

  const { DelegatedRole } = packagedRequire(rolePath);
  const delegatedRole = new DelegatedRole({
    keyIDs: ['release-verification'],
    name: 'release-assets',
    paths: ['*.yml'],
    terminating: false,
    threshold: 1,
  });
  if (!delegatedRole.isDelegatedPath('latest.yml')) {
    throw new Error('Packaged TUF runtime rejected a valid delegated target path.');
  }
  if (delegatedRole.isDelegatedPath('nested/latest.yml')) {
    throw new Error('Packaged TUF runtime accepted a target outside its delegated path.');
  }

  return {
    braceExpansion: braceExpansionMetadata.version,
    minimatch: minimatchMetadata.version,
  };
}

function verifyPackagedRuntimeDependencies(inputPath) {
  const resolvedInput = path.resolve(inputPath);
  if (!existsSync(resolvedInput)) {
    throw new Error(`Packaged runtime input does not exist: ${resolvedInput}`);
  }
  if (statSync(resolvedInput).isDirectory()) {
    return verifyDependencyTree(resolvedInput);
  }

  const extractedRoot = mkdtempSync(path.join(tmpdir(), 'butter-paper-runtime-'));
  try {
    asar.extractAll(resolvedInput, extractedRoot);
    return verifyDependencyTree(extractedRoot);
  } finally {
    rmSync(extractedRoot, { force: true, recursive: true });
  }
}

if (require.main === module) {
  const inputPath = process.argv[2];
  if (!inputPath) {
    throw new Error('Usage: node scripts/verify-packaged-runtime-dependencies.cjs <app.asar|app-directory>');
  }
  const versions = verifyPackagedRuntimeDependencies(inputPath);
  process.stdout.write(
    `Packaged TUF runtime passed (minimatch ${versions.minimatch}, `
    + `brace-expansion ${versions.braceExpansion}).\n`,
  );
}

module.exports = {
  verifyPackagedRuntimeDependencies,
};
