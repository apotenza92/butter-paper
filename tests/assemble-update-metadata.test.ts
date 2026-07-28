import { createHash } from 'node:crypto';
import { mkdtemp, mkdir, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import YAML from 'yaml';
import { assembleUpdateMetadata, metadataFileName } from '../scripts/assemble-update-metadata.mjs';

async function fixture() {
  const root = await mkdtemp(path.join(tmpdir(), 'butter-paper-update-metadata-'));
  const artifacts = path.join(root, 'artifacts');
  await mkdir(artifacts);
  const artifactName = 'Butter-Paper-Beta-macOS-arm64.zip';
  const bytes = Buffer.from('deterministic installer bytes');
  await writeFile(path.join(artifacts, artifactName), bytes);
  const metadata = {
    version: '1.2.3-beta.4',
    releaseNotes: '- Added updater-facing release notes.',
    files: [{
      url: artifactName,
      sha512: createHash('sha512').update(bytes).digest('base64'),
      size: bytes.length,
    }],
    path: artifactName,
    sha512: createHash('sha512').update(bytes).digest('base64'),
    releaseDate: '2026-07-22T00:00:00.000Z',
  };
  const input = path.join(artifacts, 'latest-mac.yml');
  await writeFile(input, YAML.stringify(metadata));
  return { root, artifacts, artifactName, input };
}

describe('assembleUpdateMetadata', () => {
  it('writes architecture-isolated metadata with versioned release asset URLs', async () => {
    const { root, artifacts, artifactName, input } = await fixture();
    const outputRoot = path.join(root, 'feed');
    const auditOutput = path.join(root, 'audit', 'beta-win32-arm64.yml');

    const result = await assembleUpdateMetadata({
      input,
      artifactDir: artifacts,
      outputRoot,
      auditOutput,
      channel: 'beta',
      platform: 'darwin',
      arch: 'arm64',
      tag: 'v1.2.3-beta.4',
      repository: 'apotenza92/butter-paper',
    });

    expect(result.outputPath).toBe(path.join(outputRoot, 'beta', 'darwin', 'arm64', 'latest-mac.yml'));
    const parsed = YAML.parse(await readFile(result.outputPath, 'utf8'));
    const expectedUrl = `https://github.com/apotenza92/butter-paper/releases/download/v1.2.3-beta.4/${artifactName}`;
    expect(parsed.files[0].url).toBe(expectedUrl);
    expect(parsed.path).toBe(expectedUrl);
    expect(parsed.butterPaperChannel).toBe('beta');
    expect(parsed.releaseNotes).toBe('- Added updater-facing release notes.');
    expect(await readFile(auditOutput, 'utf8')).toBe(await readFile(result.outputPath, 'utf8'));
  });

  it('rejects source metadata that claims another release channel', async () => {
    const { root, artifacts, input } = await fixture();
    const metadata = YAML.parse(await readFile(input, 'utf8'));
    metadata.butterPaperChannel = 'stable';
    await writeFile(input, YAML.stringify(metadata));

    await expect(assembleUpdateMetadata({
      input,
      artifactDir: artifacts,
      outputRoot: path.join(root, 'feed'),
      channel: 'beta',
      platform: 'darwin',
      arch: 'arm64',
      tag: 'v1.2.3-beta.4',
      repository: 'apotenza92/butter-paper',
    })).rejects.toThrow('does not match beta');
  });

  it('rejects metadata whose digest does not match the artifact', async () => {
    const { root, artifacts, input } = await fixture();
    const metadata = YAML.parse(await readFile(input, 'utf8'));
    metadata.files[0].sha512 = Buffer.alloc(64).toString('base64');
    await writeFile(input, YAML.stringify(metadata));

    await expect(assembleUpdateMetadata({
      input,
      artifactDir: artifacts,
      outputRoot: path.join(root, 'feed'),
      channel: 'beta',
      platform: 'darwin',
      arch: 'arm64',
      tag: 'v1.2.3-beta.4',
      repository: 'apotenza92/butter-paper',
    })).rejects.toThrow('sha512 mismatch');
  });

  it('rejects paths instead of silently publishing an unexpected file', async () => {
    const { root, artifacts, input } = await fixture();
    const metadata = YAML.parse(await readFile(input, 'utf8'));
    metadata.files[0].url = '../installer.exe';
    await writeFile(input, YAML.stringify(metadata));

    await expect(assembleUpdateMetadata({
      input,
      artifactDir: artifacts,
      outputRoot: path.join(root, 'feed'),
      channel: 'beta',
      platform: 'darwin',
      arch: 'arm64',
      tag: 'v1.2.3-beta.4',
      repository: 'apotenza92/butter-paper',
    })).rejects.toThrow('must be a filename');
  });

  it('rejects a legacy digest that disagrees with the verified files entry', async () => {
    const { root, artifacts, input } = await fixture();
    const metadata = YAML.parse(await readFile(input, 'utf8'));
    metadata.sha512 = Buffer.alloc(64).toString('base64');
    await writeFile(input, YAML.stringify(metadata));

    await expect(assembleUpdateMetadata({
      input,
      artifactDir: artifacts,
      outputRoot: path.join(root, 'feed'),
      channel: 'beta',
      platform: 'darwin',
      arch: 'arm64',
      tag: 'v1.2.3-beta.4',
      repository: 'apotenza92/butter-paper',
    })).rejects.toThrow('Legacy sha512 does not match the verified files entry');
  });

  it('rejects a missing legacy digest when a legacy path is present', async () => {
    const { root, artifacts, input } = await fixture();
    const metadata = YAML.parse(await readFile(input, 'utf8'));
    delete metadata.sha512;
    await writeFile(input, YAML.stringify(metadata));

    await expect(assembleUpdateMetadata({
      input,
      artifactDir: artifacts,
      outputRoot: path.join(root, 'feed'),
      channel: 'beta',
      platform: 'darwin',
      arch: 'arm64',
      tag: 'v1.2.3-beta.4',
      repository: 'apotenza92/butter-paper',
    })).rejects.toThrow('Legacy sha512 is missing');
  });

  it('uses the updater filename expected by each platform', () => {
    expect(metadataFileName('darwin')).toBe('latest-mac.yml');
    expect(() => metadataFileName('win32')).toThrow('Unsupported update platform');
    expect(() => metadataFileName('linux')).toThrow('Unsupported update platform');
  });
});
