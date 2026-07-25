import { createHash } from 'node:crypto';
import { readFileSync, statSync, writeFileSync } from 'node:fs';
import { basename } from 'node:path';
import YAML from 'yaml';

export function updateArtifactName(value) {
  if (typeof value !== 'string' || value.length === 0 || value.includes('\\')) {
    throw new Error(`Invalid updater artifact URL: ${String(value)}`);
  }
  const encodedName = /^https?:\/\//.test(value)
    ? basename(new URL(value).pathname)
    : value;
  if (basename(encodedName) !== encodedName || encodedName === '.' || encodedName === '..') {
    throw new Error(`Updater artifact URL must resolve to a filename: ${value}`);
  }
  const name = decodeURIComponent(encodedName);
  if (!name || name === '.' || name === '..' || name.includes('/') || name.includes('\\')) {
    throw new Error(`Unsafe updater artifact filename: ${value}`);
  }
  return name;
}

function sha512File(filePath) {
  return createHash('sha512').update(readFileSync(filePath)).digest('base64');
}

export function validateUpdateMetadataArtifacts(metadata, artifactPaths, expectedVersion) {
  if (!metadata || typeof metadata !== 'object' || Array.isArray(metadata)
    || metadata.version !== expectedVersion || !Array.isArray(metadata.files)) {
    throw new Error('Updater metadata identity or file list is invalid');
  }
  const expectedNames = Object.keys(artifactPaths).sort();
  if (metadata.files.length !== expectedNames.length) {
    throw new Error(`Updater metadata must reference exactly: ${expectedNames.join(', ')}`);
  }
  const seen = new Set();
  const verifiedSha512 = new Map();
  for (const file of metadata.files) {
    const name = updateArtifactName(file?.url);
    const artifactPath = artifactPaths[name];
    if (!artifactPath || seen.has(name)) {
      throw new Error(`Updater metadata contains an unexpected or duplicate artifact: ${name}`);
    }
    const expectedSha512 = sha512File(artifactPath);
    if (file.sha512 !== expectedSha512 || file.size !== statSync(artifactPath).size) {
      throw new Error(`Updater metadata digest or size is stale for ${name}`);
    }
    seen.add(name);
    verifiedSha512.set(name, expectedSha512);
  }
  if (seen.size !== expectedNames.length) {
    throw new Error(`Updater metadata is missing an artifact: ${expectedNames.find((name) => !seen.has(name))}`);
  }
  const legacyName = updateArtifactName(metadata.path);
  if (!seen.has(legacyName) || metadata.sha512 !== verifiedSha512.get(legacyName)) {
    throw new Error('Legacy updater path and SHA-512 must match a verified artifact');
  }
}

export function validateUpdateMetadataFile(metadataPath, artifactPaths, expectedVersion) {
  const metadata = YAML.parse(readFileSync(metadataPath, 'utf8'));
  validateUpdateMetadataArtifacts(metadata, artifactPaths, expectedVersion);
  return metadata;
}

export function refreshUpdateMetadataArtifact(metadataPath, artifactPath) {
  const metadata = YAML.parse(readFileSync(metadataPath, 'utf8'));
  if (!metadata || typeof metadata !== 'object' || Array.isArray(metadata)
    || !Array.isArray(metadata.files)) {
    throw new Error(`Updater metadata is invalid: ${metadataPath}`);
  }
  const artifactName = basename(artifactPath);
  const matches = metadata.files.filter((file) => updateArtifactName(file?.url) === artifactName);
  if (matches.length !== 1) {
    throw new Error(`Updater metadata must reference ${artifactName} exactly once`);
  }
  const sha512 = sha512File(artifactPath);
  matches[0].sha512 = sha512;
  matches[0].size = statSync(artifactPath).size;
  if (updateArtifactName(metadata.path) === artifactName) {
    metadata.sha512 = sha512;
  }
  writeFileSync(metadataPath, YAML.stringify(metadata, { lineWidth: 0 }), { mode: 0o644 });
}
