import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import YAML from 'yaml';
import {
  refreshUpdateMetadataArtifact,
  validateUpdateMetadataFile,
} from '../scripts/update-metadata-contract.mjs';

const sha512 = (path: string) => createHash('sha512').update(readFileSync(path)).digest('base64');

describe('signed macOS updater metadata contract', () => {
  it('refreshes a stapled DMG entry and validates every advertised artifact', () => {
    const root = mkdtempSync(join(tmpdir(), 'butter-paper-update-metadata-'));
    const dmg = join(root, 'Butter-Paper-macOS-arm64.dmg');
    const zip = join(root, 'Butter-Paper-macOS-arm64.zip');
    const metadataPath = join(root, 'latest-mac.yml');
    try {
      writeFileSync(dmg, 'pre-staple');
      writeFileSync(zip, 'signed zip');
      writeFileSync(metadataPath, YAML.stringify({
        version: '1.2.3',
        files: [
          { url: 'Butter-Paper-macOS-arm64.zip', sha512: sha512(zip), size: statSync(zip).size },
          { url: 'Butter-Paper-macOS-arm64.dmg', sha512: sha512(dmg), size: statSync(dmg).size },
        ],
        path: 'Butter-Paper-macOS-arm64.zip',
        sha512: sha512(zip),
      }));

      writeFileSync(dmg, 'post-staple bytes are different');
      expect(() => validateUpdateMetadataFile(metadataPath, {
        'Butter-Paper-macOS-arm64.dmg': dmg,
        'Butter-Paper-macOS-arm64.zip': zip,
      }, '1.2.3')).toThrow(/stale/);

      refreshUpdateMetadataArtifact(metadataPath, dmg);
      expect(() => validateUpdateMetadataFile(metadataPath, {
        'Butter-Paper-macOS-arm64.dmg': dmg,
        'Butter-Paper-macOS-arm64.zip': zip,
      }, '1.2.3')).not.toThrow();
      const refreshed = YAML.parse(readFileSync(metadataPath, 'utf8'));
      const dmgEntry = refreshed.files.find(({ url }: { url: string }) => url.endsWith('.dmg'));
      expect(dmgEntry).toMatchObject({ sha512: sha512(dmg), size: statSync(dmg).size });
      expect(refreshed.sha512).toBe(sha512(zip));
    } finally {
      rmSync(root, { force: true, recursive: true });
    }
  });

  it('rejects missing, duplicate, and path-escaping artifact references', () => {
    const root = mkdtempSync(join(tmpdir(), 'butter-paper-update-metadata-invalid-'));
    const artifact = join(root, 'candidate.zip');
    const metadataPath = join(root, 'latest-mac.yml');
    try {
      writeFileSync(artifact, 'candidate');
      const entry = { url: 'candidate.zip', sha512: sha512(artifact), size: statSync(artifact).size };
      for (const files of [[], [entry, entry], [{ ...entry, url: '../candidate.zip' }]]) {
        writeFileSync(metadataPath, YAML.stringify({
          version: '1.2.3', files, path: 'candidate.zip', sha512: sha512(artifact),
        }));
        expect(() => validateUpdateMetadataFile(metadataPath, { 'candidate.zip': artifact }, '1.2.3')).toThrow();
      }
    } finally {
      rmSync(root, { force: true, recursive: true });
    }
  });
});
