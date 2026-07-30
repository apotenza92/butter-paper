import { createHash } from 'node:crypto';
import { readFile, stat, writeFile, mkdir } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import YAML from 'yaml';

const CHANNELS = new Set(['stable', 'beta']);
const PLATFORMS = new Set(['darwin', 'win32', 'linux']);
const ARCHITECTURES = new Set(['arm64', 'x64']);

export function metadataFileName(platform, arch) {
  requireChoice('platform', platform, PLATFORMS);
  requireChoice('arch', arch, ARCHITECTURES);
  if (platform === 'darwin') return 'latest-mac.yml';
  if (platform === 'win32') return 'latest.yml';
  return arch === 'arm64' ? 'latest-linux-arm64.yml' : 'latest-linux.yml';
}

function requireChoice(label, value, choices) {
  if (!choices.has(value)) {
    throw new Error(`${label} must be one of: ${[...choices].join(', ')}`);
  }
}

function validateRepository(repository) {
  if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository)) {
    throw new Error('repository must use the owner/name form');
  }
}

function validateTag(tag) {
  if (!/^v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(tag)) {
    throw new Error(`Invalid release tag: ${tag}`);
  }
}

function artifactNameFromUrl(value) {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error('Every update file must have a non-empty url');
  }

  if (value.includes('\\')) {
    throw new Error(`Update artifact URL must not contain backslashes: ${value}`);
  }

  let artifactName;
  if (/^https?:\/\//.test(value)) {
    const parsed = new URL(value);
    artifactName = path.posix.basename(parsed.pathname);
  } else {
    if (path.posix.basename(value) !== value || value === '.' || value === '..') {
      throw new Error(`Update artifact URL must be a filename: ${value}`);
    }
    artifactName = value;
  }

  const decoded = decodeURIComponent(artifactName);
  if (!decoded || decoded === '.' || decoded === '..' || decoded.includes('/') || decoded.includes('\\')) {
    throw new Error(`Unsafe update artifact filename: ${value}`);
  }
  return decoded;
}

async function digestFile(filePath, algorithm, encoding) {
  const bytes = await readFile(filePath);
  return createHash(algorithm).update(bytes).digest(encoding);
}

async function verifyArtifact(artifactDir, file) {
  const artifactName = artifactNameFromUrl(file.url);
  const artifactPath = path.resolve(artifactDir, artifactName);
  const relativePath = path.relative(path.resolve(artifactDir), artifactPath);
  if (relativePath.startsWith('..') || path.isAbsolute(relativePath)) {
    throw new Error(`Artifact escapes its directory: ${artifactName}`);
  }

  const artifactStat = await stat(artifactPath).catch(() => null);
  if (!artifactStat?.isFile()) {
    throw new Error(`Referenced update artifact does not exist: ${artifactPath}`);
  }
  if (typeof file.sha512 !== 'string' || file.sha512.length === 0) {
    throw new Error(`Missing sha512 for update artifact: ${artifactName}`);
  }

  const actualSha512 = await digestFile(artifactPath, 'sha512', 'base64');
  if (actualSha512 !== file.sha512) {
    throw new Error(`sha512 mismatch for update artifact: ${artifactName}`);
  }
  if (file.size !== undefined && file.size !== artifactStat.size) {
    throw new Error(`Size mismatch for update artifact: ${artifactName}`);
  }

  return artifactName;
}

export async function assembleUpdateMetadata({
  input,
  artifactDir,
  outputRoot,
  auditOutput,
  channel,
  platform,
  arch,
  tag,
  repository,
}) {
  requireChoice('channel', channel, CHANNELS);
  requireChoice('platform', platform, PLATFORMS);
  requireChoice('arch', arch, ARCHITECTURES);
  validateRepository(repository);
  validateTag(tag);

  const source = YAML.parse(await readFile(input, 'utf8'));
  if (!source || typeof source !== 'object' || Array.isArray(source)) {
    throw new Error('Update metadata must contain a YAML mapping');
  }
  if (typeof source.version !== 'string' || source.version !== tag.slice(1)) {
    throw new Error(`Update metadata version ${String(source.version)} does not match tag ${tag}`);
  }
  if (!Array.isArray(source.files) || source.files.length === 0) {
    throw new Error('Update metadata must contain at least one file');
  }

  const releaseBaseUrl = `https://github.com/${repository}/releases/download/${encodeURIComponent(tag)}`;
  const seenArtifacts = new Set();
  const verifiedSha512ByArtifact = new Map();
  const rewrittenFiles = [];
  for (const file of source.files) {
    if (!file || typeof file !== 'object' || Array.isArray(file)) {
      throw new Error('Every update file entry must be a YAML mapping');
    }
    const artifactName = await verifyArtifact(artifactDir, file);
    if (seenArtifacts.has(artifactName)) {
      throw new Error(`Duplicate update artifact: ${artifactName}`);
    }
    seenArtifacts.add(artifactName);
    verifiedSha512ByArtifact.set(artifactName, file.sha512);
    rewrittenFiles.push({
      ...file,
      url: `${releaseBaseUrl}/${encodeURIComponent(artifactName)}`,
    });
  }

  if (source.butterPaperChannel !== undefined && source.butterPaperChannel !== channel) {
    throw new Error(`Update metadata channel ${String(source.butterPaperChannel)} does not match ${channel}`);
  }
  const rewritten = { ...source, butterPaperChannel: channel, files: rewrittenFiles };
  if (typeof source.path === 'string') {
    const legacyArtifactName = artifactNameFromUrl(source.path);
    if (!seenArtifacts.has(legacyArtifactName)) {
      throw new Error(`Legacy path does not match a files entry: ${source.path}`);
    }
    const verifiedSha512 = verifiedSha512ByArtifact.get(legacyArtifactName);
    if (typeof source.sha512 !== 'string' || source.sha512.length === 0) {
      throw new Error(`Legacy sha512 is missing for path: ${source.path}`);
    }
    if (source.sha512 !== verifiedSha512) {
      throw new Error(`Legacy sha512 does not match the verified files entry: ${source.path}`);
    }
    rewritten.path = `${releaseBaseUrl}/${encodeURIComponent(legacyArtifactName)}`;
    rewritten.sha512 = verifiedSha512;
  }

  const contents = YAML.stringify(rewritten, { lineWidth: 0 }).trimEnd() + '\n';
  const outputPath = path.join(outputRoot, channel, platform, arch, metadataFileName(platform, arch));
  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(outputPath, contents, 'utf8');

  if (auditOutput) {
    await mkdir(path.dirname(auditOutput), { recursive: true });
    await writeFile(auditOutput, contents, 'utf8');
  }

  return { outputPath, contents, artifacts: [...seenArtifacts] };
}

function parseArguments(argv) {
  const values = {};
  for (let index = 0; index < argv.length; index += 2) {
    const key = argv[index];
    const value = argv[index + 1];
    if (!key?.startsWith('--') || value === undefined || value.startsWith('--')) {
      throw new Error(`Expected --name value arguments; received ${key ?? '<end>'}`);
    }
    values[key.slice(2)] = value;
  }

  const required = ['input', 'artifact-dir', 'output-root', 'channel', 'platform', 'arch', 'tag', 'repository'];
  for (const key of required) {
    if (!values[key]) {
      throw new Error(`Missing required argument: --${key}`);
    }
  }
  return values;
}

export async function main(argv = process.argv.slice(2)) {
  const args = parseArguments(argv);
  const result = await assembleUpdateMetadata({
    input: path.resolve(args.input),
    artifactDir: path.resolve(args['artifact-dir']),
    outputRoot: path.resolve(args['output-root']),
    auditOutput: args['audit-output'] ? path.resolve(args['audit-output']) : undefined,
    channel: args.channel,
    platform: args.platform,
    arch: args.arch,
    tag: args.tag,
    repository: args.repository,
  });
  process.stdout.write(`${result.outputPath}\n`);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1])) {
  main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  });
}
