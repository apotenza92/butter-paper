#!/usr/bin/env node

import { readFile, rename, rm, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export async function canonicalizeGeneratedSbom(path) {
  const absolute = resolve(path);
  const bytes = await readFile(absolute);
  let text;
  try {
    text = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
  } catch {
    throw new Error('Generated PDF signature core SBOM is not valid UTF-8');
  }
  if (/\r(?!\n)/.test(text)) {
    throw new Error('Generated PDF signature core SBOM contains a lone CR byte');
  }
  const canonical = text.replaceAll('\r\n', '\n');
  let parsed;
  try {
    parsed = JSON.parse(canonical);
  } catch {
    throw new Error('Generated PDF signature core SBOM is not valid JSON');
  }
  validateGeneratedSbomForCveScanning(parsed);
  if (canonical === text) return { changed: false, path: absolute };

  const temporary = `${absolute}.canonical-lf-part`;
  await writeFile(temporary, canonical, { encoding: 'utf8', flag: 'wx' });
  try {
    await rename(temporary, absolute);
  } catch (error) {
    await rm(temporary, { force: true });
    throw error;
  }
  return { changed: true, path: absolute };
}

export function validateGeneratedSbomForCveScanning(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)
    || value.bomFormat !== 'CycloneDX'
    || value.specVersion !== '1.6'
    || value.version !== 1
    || value.metadata?.component?.['bom-ref'] !== 'pkg:maven/com.butterpaper/pdf-signature-core@0.1.0?type=jar'
    || value.metadata.component.purl !== value.metadata.component['bom-ref']
    || !Array.isArray(value.components) || value.components.length === 0
    || !Array.isArray(value.dependencies) || value.dependencies.length === 0) {
    throw new Error('Generated PDF signature core SBOM is not a scan-ready CycloneDX 1.6 inventory');
  }
  const componentRefs = new Set();
  const componentPurls = new Set();
  for (const component of value.components) {
    const coordinate = sbomCoordinate(component);
    const expectedPurl = `pkg:maven/${component.group}/${component.name}@${component.version}?type=jar`;
    const sha256Hashes = Array.isArray(component.hashes)
      ? component.hashes.filter((hash) => hash?.alg === 'SHA-256' && isSha256(hash?.content))
      : [];
    if (!coordinate
      || typeof component['bom-ref'] !== 'string'
      || component['bom-ref'] !== expectedPurl
      || component.purl !== expectedPurl
      || componentRefs.has(component['bom-ref'])
      || componentPurls.has(component.purl)
      || sha256Hashes.length !== 1
      || !Array.isArray(component.licenses) || component.licenses.length === 0
      || component.licenses.some((entry) => {
        const license = entry?.license;
        return !license || (typeof license.id !== 'string' && typeof license.name !== 'string');
      })) {
      throw new Error(`Generated PDF signature core SBOM component is not scan-ready: ${coordinate || 'unknown'}`);
    }
    componentRefs.add(component['bom-ref']);
    componentPurls.add(component.purl);
  }
  const allowedRefs = new Set([value.metadata.component['bom-ref'], ...componentRefs]);
  const dependencyRefs = new Set();
  for (const dependency of value.dependencies) {
    if (!dependency || typeof dependency.ref !== 'string'
      || !allowedRefs.has(dependency.ref)
      || dependencyRefs.has(dependency.ref)
      || !Array.isArray(dependency.dependsOn)
      || dependency.dependsOn.some((reference) => typeof reference !== 'string' || !allowedRefs.has(reference))) {
      throw new Error('Generated PDF signature core SBOM dependency graph is invalid');
    }
    dependencyRefs.add(dependency.ref);
  }
  if (dependencyRefs.size !== allowedRefs.size
    || [...allowedRefs].some((reference) => !dependencyRefs.has(reference))) {
    throw new Error('Generated PDF signature core SBOM dependency graph is incomplete');
  }
  return value;
}

function sbomCoordinate(component) {
  return typeof component?.group === 'string' && component.group.length > 0
    && typeof component?.name === 'string' && component.name.length > 0
    && typeof component?.version === 'string' && component.version.length > 0
    ? `${component.group}:${component.name}:${component.version}`
    : null;
}

function isSha256(value) {
  return typeof value === 'string' && /^[a-f0-9]{64}$/.test(value);
}

async function main() {
  if (process.argv.length !== 3) {
    throw new Error('usage: canonicalize-generated-source-inputs.mjs <generated-sbom.json>');
  }
  await canonicalizeGeneratedSbom(process.argv[2]);
  process.stdout.write('Generated PDF signature core SBOM is canonical LF JSON.\n');
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  });
}
