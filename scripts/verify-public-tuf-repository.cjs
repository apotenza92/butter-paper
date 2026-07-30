#!/usr/bin/env node

const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { BaseFetcher, Updater } = require('tuf-js');
const { DownloadHTTPError } = require('tuf-js/dist/error');

class NoRedirectFetcher extends BaseFetcher {
  constructor(repositoryUrl) {
    super();
    this.repositoryUrl = repositoryUrl;
  }

  async fetch(url) {
    const requested = new URL(url);
    const repository = new URL(`${this.repositoryUrl}/`);
    if (
      requested.origin !== repository.origin
      || (!requested.pathname.startsWith(`${repository.pathname}metadata/`)
        && !requested.pathname.startsWith(`${repository.pathname}targets/`))
      || requested.username
      || requested.password
      || requested.search
      || requested.hash
    ) {
      throw new Error(`TUF attempted an unexpected public URL: ${url}`);
    }
    const response = await fetch(requested, {
      redirect: 'error',
      signal: AbortSignal.timeout(30_000),
    });
    if (!response.ok || !response.body) {
      if ([403, 404].includes(response.status)) {
        throw new DownloadHTTPError('TUF metadata was not found.', response.status);
      }
      throw new Error(`TUF public download failed with HTTP ${response.status}: ${requested}`);
    }
    if (response.redirected || response.url !== requested.toString()) {
      throw new Error(`TUF public metadata redirects are not allowed: ${requested}`);
    }
    return response.body;
  }
}

function option(argv, name) {
  const index = argv.indexOf(name);
  return index >= 0 ? argv[index + 1] : null;
}

async function verifyPublicTufRepository({
  expectedPath,
  repositoryUrl,
  rootPath,
  targetName,
}) {
  const parsedRepository = new URL(repositoryUrl);
  if (
    parsedRepository.protocol !== 'https:'
    || parsedRepository.username
    || parsedRepository.password
    || parsedRepository.search
    || parsedRepository.hash
  ) {
    throw new Error('Public TUF verification requires a credential-free HTTPS repository URL.');
  }
  if (
    !targetName
    || targetName !== path.posix.basename(targetName)
    || targetName.includes('\\')
    || targetName.includes('\0')
  ) {
    throw new Error(`Unsafe public TUF target name: ${targetName}`);
  }

  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'butter-paper-public-tuf-'));
  try {
    const metadataDir = path.join(temporary, 'metadata');
    const targetDir = path.join(temporary, 'targets');
    fs.mkdirSync(metadataDir, { recursive: true, mode: 0o700 });
    fs.mkdirSync(targetDir, { recursive: true, mode: 0o700 });
    fs.copyFileSync(rootPath, path.join(metadataDir, 'root.json'), fs.constants.COPYFILE_EXCL);
    const updater = new Updater({
      metadataBaseUrl: `${repositoryUrl.replace(/\/$/, '')}/metadata`,
      targetBaseUrl: `${repositoryUrl.replace(/\/$/, '')}/targets`,
      metadataDir,
      targetDir,
      fetcher: new NoRedirectFetcher(repositoryUrl.replace(/\/$/, '')),
      config: { userAgent: 'Butter Paper public release verifier' },
    });
    await updater.refresh();
    const target = await updater.getTargetInfo(targetName);
    if (!target) {
      throw new Error(`Public TUF repository has no ${targetName} target.`);
    }
    const downloadedPath = path.join(targetDir, targetName);
    await updater.downloadTarget(target, downloadedPath);
    if (!fs.readFileSync(downloadedPath).equals(fs.readFileSync(expectedPath))) {
      throw new Error(`Public TUF target differs from the sealed ${targetName} bytes.`);
    }
  } finally {
    fs.rmSync(temporary, { recursive: true, force: true });
  }
}

async function main(argv = process.argv.slice(2)) {
  const expectedPath = option(argv, '--expected');
  const repositoryUrl = option(argv, '--repository');
  const rootPath = option(argv, '--root');
  const targetName = option(argv, '--target-name');
  if (!expectedPath || !repositoryUrl || !rootPath || !targetName) {
    throw new Error(
      'Usage: verify-public-tuf-repository.cjs --root FILE --repository URL '
      + '--target-name NAME --expected FILE',
    );
  }
  await verifyPublicTufRepository({
    expectedPath: path.resolve(expectedPath),
    repositoryUrl,
    rootPath: path.resolve(rootPath),
    targetName,
  });
  process.stdout.write(`Verified public TUF target ${targetName}.\n`);
}

if (require.main === module) {
  main().catch(error => {
    process.stderr.write(`${error.stack || error}\n`);
    process.exitCode = 1;
  });
}

module.exports = { verifyPublicTufRepository };
