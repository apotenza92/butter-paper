import { _electron as electron } from '@playwright/test';
import { createHash, randomUUID } from 'node:crypto';
import { existsSync } from 'node:fs';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import YAML from 'yaml';
import {
  BUTTER_PAPER_SIGNING_CERTIFICATE_SHA256,
  createSmokeEnvironment,
  resolvePriorSigningFingerprints,
  verifyTrustedMacApp,
} from './verify-macos-package.mjs';

const SCENARIOS = new Set(['valid', 'channel', 'corrupt', 'signature']);
const CHANNELS = new Set(['stable', 'beta']);
const UPDATE_TIMEOUT_MS = 180_000;

function trustExpectations({ prior = false } = {}) {
  const currentFingerprint = process.env.APPLE_SIGNING_CERTIFICATE_SHA256?.trim()
    || BUTTER_PAPER_SIGNING_CERTIFICATE_SHA256;
  return {
    ...(prior
      ? {
          fingerprints: resolvePriorSigningFingerprints(
            currentFingerprint,
            process.env.APPLE_PRIOR_SIGNING_CERTIFICATE_SHA256?.trim(),
          ),
        }
      : { fingerprint: currentFingerprint }),
    ...(process.env.APPLE_SIGNING_IDENTITY?.trim()
      ? { identity: process.env.APPLE_SIGNING_IDENTITY.trim() }
      : {}),
    ...(process.env.APPLE_TEAM_ID?.trim() ? { teamId: process.env.APPLE_TEAM_ID.trim() } : {}),
  };
}

export function parseMacUpdateArguments(argv) {
  const values = new Map();
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (!argument.startsWith('--')) {
      throw new Error(`Unexpected positional argument: ${argument}`);
    }
    const name = argument.slice(2);
    const value = argv[index + 1];
    if (!value || value.startsWith('--')) {
      throw new Error(`Missing value for --${name}.`);
    }
    if (values.has(name)) {
      throw new Error(`Duplicate --${name} argument.`);
    }
    values.set(name, value);
    index += 1;
  }

  const required = ['prior-zip', 'candidate-zip', 'channel', 'expected-version', 'scenario'];
  for (const name of required) {
    if (!values.has(name)) {
      throw new Error(`Missing required --${name} argument.`);
    }
  }
  for (const name of values.keys()) {
    if (!required.includes(name) && name !== 'evidence') {
      throw new Error(`Unknown --${name} argument.`);
    }
  }

  const channel = values.get('channel');
  const scenario = values.get('scenario');
  if (!CHANNELS.has(channel)) {
    throw new Error('--channel must be stable or beta.');
  }
  if (!SCENARIOS.has(scenario)) {
    throw new Error('--scenario must be valid, channel, corrupt, or signature.');
  }

  return {
    priorZip: resolve(values.get('prior-zip')),
    candidateZip: resolve(values.get('candidate-zip')),
    channel,
    expectedVersion: values.get('expected-version'),
    scenario,
    evidencePath: values.has('evidence') ? resolve(values.get('evidence')) : null,
  };
}

export function buildMacUpdateMetadata({ artifactName, sha512, size, version, channel }) {
  if (basename(artifactName) !== artifactName || artifactName.includes('..')) {
    throw new Error(`Unsafe update artifact name: ${artifactName}`);
  }
  if (!/^[A-Za-z0-9+/]{86}==$/.test(sha512)) {
    throw new Error('Update SHA-512 must be base64 encoded.');
  }
  if (!Number.isSafeInteger(size) || size <= 0) {
    throw new Error('Update artifact size must be a positive integer.');
  }
  if (typeof version !== 'string' || version.trim() === '') {
    throw new Error('Update version must be non-empty.');
  }
  if (!CHANNELS.has(channel)) {
    throw new Error('Update channel must be stable or beta.');
  }

  return YAML.stringify({
    version,
    butterPaperChannel: channel,
    files: [{ url: artifactName, sha512, size }],
    path: artifactName,
    sha512,
    releaseDate: new Date(0).toISOString(),
  });
}

export function expectedMacIdentity(channel) {
  if (!CHANNELS.has(channel)) {
    throw new Error(`Unsupported update channel: ${channel}`);
  }
  return channel === 'beta'
    ? { productName: 'Butter Paper Beta', bundleId: 'com.butterpaper.desktop.beta' }
    : { productName: 'Butter Paper', bundleId: 'com.butterpaper.desktop' };
}

export function compareReleaseVersions(left, right) {
  const parse = (value) => {
    const match = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z.-]+))?$/.exec(value);
    if (match == null) {
      throw new Error(`Invalid release version: ${value}`);
    }
    return {
      core: match.slice(1, 4).map(Number),
      prerelease: match[4]?.split('.') ?? null,
    };
  };
  const leftVersion = parse(left);
  const rightVersion = parse(right);
  for (let index = 0; index < 3; index += 1) {
    if (leftVersion.core[index] !== rightVersion.core[index]) {
      return Math.sign(leftVersion.core[index] - rightVersion.core[index]);
    }
  }
  if (leftVersion.prerelease == null || rightVersion.prerelease == null) {
    return leftVersion.prerelease == null
      ? rightVersion.prerelease == null ? 0 : 1
      : -1;
  }
  const length = Math.max(leftVersion.prerelease.length, rightVersion.prerelease.length);
  for (let index = 0; index < length; index += 1) {
    const leftPart = leftVersion.prerelease[index];
    const rightPart = rightVersion.prerelease[index];
    if (leftPart == null || rightPart == null) {
      return leftPart == null ? -1 : 1;
    }
    if (leftPart === rightPart) {
      continue;
    }
    const leftNumeric = /^\d+$/.test(leftPart);
    const rightNumeric = /^\d+$/.test(rightPart);
    if (leftNumeric && rightNumeric) {
      return Math.sign(Number(leftPart) - Number(rightPart));
    }
    if (leftNumeric !== rightNumeric) {
      return leftNumeric ? -1 : 1;
    }
    return leftPart < rightPart ? -1 : 1;
  }
  return 0;
}

export function parseExecutableProcessIds(output, executablePath) {
  const prefix = `${executablePath} `;
  return String(output).split(/\r?\n/).flatMap((line) => {
    const match = /^\s*(\d+)\s+(.+?)\s*$/.exec(line);
    if (match == null || (match[2] !== executablePath && !match[2].startsWith(prefix))) {
      return [];
    }
    return [Number(match[1])];
  });
}

async function main() {
  if (process.platform !== 'darwin') {
    throw new Error('The macOS updater integration test must run on macOS.');
  }
  const options = parseMacUpdateArguments(process.argv.slice(2));
  for (const archive of [options.priorZip, options.candidateZip]) {
    if (!existsSync(archive)) {
      throw new Error(`Update archive does not exist: ${archive}`);
    }
  }

  const workspace = await mkdtemp(join(tmpdir(), 'butter-paper-update-'));
  const installedRoot = join(workspace, 'installed');
  const candidateRoot = join(workspace, 'candidate');
  const homePath = join(workspace, 'home');
  const userDataPath = join(workspace, 'user-data');
  const markerPath = join(userDataPath, 'update-test-marker.json');
  const marker = { id: randomUUID(), createdAt: new Date().toISOString() };
  let app;
  let server;
  const stderr = [];
  const evidence = {
    schemaVersion: 1,
    scenario: options.scenario,
    channel: options.channel,
    architecture: process.arch,
    expectedVersion: options.expectedVersion,
    result: 'failed',
  };

  try {
    await Promise.all([mkdir(installedRoot), mkdir(candidateRoot), mkdir(homePath), mkdir(userDataPath)]);
    extractZip(options.priorZip, installedRoot);
    extractZip(options.candidateZip, candidateRoot);

    const identity = expectedMacIdentity(options.channel);
    const installedApp = join(installedRoot, `${identity.productName}.app`);
    const candidateApp = join(candidateRoot, `${identity.productName}.app`);
    verifyAppIdentity(installedApp, identity.bundleId);
    verifyAppIdentity(candidateApp, identity.bundleId);
    const certificateDirectory = join(workspace, 'prior-certificates');
    await mkdir(certificateDirectory, { mode: 0o700 });
    verifyTrustedMacApp(installedApp, {
      arch: process.arch,
      bundleId: identity.bundleId,
      certificateDirectory,
      ...trustExpectations({ prior: true }),
    });

    const priorVersion = readPlistValue(installedApp, 'CFBundleShortVersionString');
    const candidateVersion = readPlistValue(candidateApp, 'CFBundleShortVersionString');
    if (candidateVersion !== options.expectedVersion) {
      throw new Error(`Candidate version ${candidateVersion} does not match ${options.expectedVersion}.`);
    }
    if (priorVersion === candidateVersion) {
      throw new Error('Prior and candidate versions must differ for an updater integration test.');
    }
    if (compareReleaseVersions(candidateVersion, priorVersion) <= 0) {
      throw new Error(`Candidate ${candidateVersion} must be newer than prior ${priorVersion}.`);
    }

    const executableName = readPlistValue(installedApp, 'CFBundleExecutable');
    const executablePath = join(installedApp, 'Contents', 'MacOS', executableName);
    const candidateBytes = await readFile(options.candidateZip);
    const servedBytes = options.scenario === 'corrupt'
      ? corruptArchive(candidateBytes)
      : candidateBytes;
    const metadata = buildMacUpdateMetadata({
      artifactName: basename(options.candidateZip),
      sha512: createHash('sha512').update(candidateBytes).digest('base64'),
      size: candidateBytes.length,
      version: candidateVersion,
      channel: options.scenario === 'channel'
        ? options.channel === 'stable' ? 'beta' : 'stable'
        : options.channel,
    });

    server = await startFeedServer({
      artifactName: basename(options.candidateZip),
      artifactBytes: servedBytes,
      metadata,
    });
    const address = server.address();
    if (address == null || typeof address === 'string') {
      throw new Error('Loopback update server did not expose a TCP address.');
    }

    await writeFile(markerPath, `${JSON.stringify(marker)}\n`, { mode: 0o600 });
    await writeFile(join(userDataPath, 'update-settings.json'), `${JSON.stringify({
      schemaVersion: 1,
      frequency: 'never',
      lastSuccessfulCheckAt: null,
    }, null, 2)}\n`, { mode: 0o600 });

    app = await electron.launch({
      executablePath,
      env: updaterEnvironment(userDataPath, homePath, `http://127.0.0.1:${address.port}/`),
      timeout: 60_000,
    });
    app.process().stderr?.on('data', chunk => stderr.push(String(chunk)));
    const page = await app.firstWindow({ timeout: 60_000 });
    await page.waitForFunction(() => Boolean(window.butterPaper?.updates), undefined, { timeout: 30_000 });
    const initialStatus = await page.evaluate(() => window.butterPaper.updates.getStatus());
    if (!initialStatus.enabled || initialStatus.channel !== options.channel) {
      throw new Error(`Updater was not enabled for ${options.channel}: ${JSON.stringify(initialStatus)}`);
    }
    if (initialStatus.currentVersion !== priorVersion) {
      throw new Error(`Runtime prior version ${initialStatus.currentVersion} does not match ${priorVersion}.`);
    }

    await page.evaluate(() => window.butterPaper.updates.checkNow());
    const downloadStatus = await waitForUpdateStatus(page, status => (
      status.phase === 'downloaded' || status.phase === 'error'
    ));

    if (options.scenario === 'channel') {
      assertRejected(downloadStatus, 'cross-channel update', /channel/i);
      await assertAppUnchanged(app, installedApp, priorVersion);
    } else if (options.scenario === 'corrupt') {
      assertRejected(downloadStatus, 'corrupted download', /sha(?:-?512)?|checksum|integrity/i);
      await assertAppUnchanged(app, installedApp, priorVersion);
    } else {
      if (downloadStatus.phase !== 'downloaded' || downloadStatus.availableVersion !== candidateVersion) {
        throw new Error(`Candidate did not download: ${JSON.stringify(downloadStatus)}`);
      }

      if (options.scenario === 'signature') {
        await page.evaluate(() => window.butterPaper.updates.installDownloaded());
        const rejection = await waitForUpdateStatus(page, status => status.phase === 'error');
        assertRejected(rejection, 'invalid signature', /code.?sign|signature|signed/i);
        await assertAppUnchanged(app, installedApp, priorVersion);
      } else {
        const originalPid = app.process().pid;
        const automaticRelaunch = waitForRelaunchedProcess(executablePath, originalPid, UPDATE_TIMEOUT_MS);
        const exitPromise = waitForExit(app.process(), UPDATE_TIMEOUT_MS);
        await page.evaluate(() => window.butterPaper.updates.installDownloaded()).catch(error => {
          if (!String(error).includes('Target page, context or browser has been closed')) {
            throw error;
          }
        });
        await exitPromise;
        app = null;
        const relaunchedPid = await automaticRelaunch;
        evidence.automaticRelaunchPid = relaunchedPid;
        await waitForInstalledVersion(installedApp, candidateVersion, UPDATE_TIMEOUT_MS);
        const updatedCertificateDirectory = join(workspace, 'updated-certificates');
        await mkdir(updatedCertificateDirectory, { mode: 0o700 });
        verifyTrustedMacApp(installedApp, {
          arch: process.arch,
          bundleId: identity.bundleId,
          certificateDirectory: updatedCertificateDirectory,
          ...trustExpectations(),
        });

        const preservedMarker = JSON.parse(await readFile(markerPath, 'utf8'));
        if (preservedMarker.id !== marker.id) {
          throw new Error('Updater did not preserve the isolated user-data marker.');
        }

        process.kill(relaunchedPid, 'SIGTERM');
        await waitForPidExit(relaunchedPid, 30_000);

        const updatedExecutable = join(
          installedApp,
          'Contents',
          'MacOS',
          readPlistValue(installedApp, 'CFBundleExecutable'),
        );
        app = await electron.launch({
          executablePath: updatedExecutable,
          env: {
            ...updaterEnvironment(userDataPath, homePath, null),
            BP_DISABLE_UPDATE_CHECKS: '1',
          },
          timeout: 60_000,
        });
        const updatedPage = await app.firstWindow({ timeout: 60_000 });
        await updatedPage.waitForFunction(() => Boolean(window.butterPaper?.updates), undefined, { timeout: 30_000 });
        const updatedStatus = await updatedPage.evaluate(() => window.butterPaper.updates.getStatus());
        if (updatedStatus.currentVersion !== candidateVersion || updatedStatus.channel !== options.channel) {
          throw new Error(`Updated runtime identity is wrong: ${JSON.stringify(updatedStatus)}`);
        }
      }
    }

    evidence.priorVersion = priorVersion;
    evidence.result = options.scenario === 'valid' ? 'updated' : 'rejected';
    evidence.completedAt = new Date().toISOString();
    console.log(`macOS ${options.channel} updater ${options.scenario} scenario passed (${process.arch}).`);
  } catch (error) {
    if (stderr.length > 0) {
      console.error(`Packaged app stderr:\n${stderr.join('').trim()}`);
    }
    evidence.error = error instanceof Error ? error.message : String(error);
    throw error;
  } finally {
    await app?.close().catch(() => undefined);
    if (server != null) {
      await new Promise(resolveClose => server.close(resolveClose));
    }
    if (options.evidencePath != null) {
      await mkdir(dirname(options.evidencePath), { recursive: true });
      await writeFile(options.evidencePath, `${JSON.stringify(evidence, null, 2)}\n`);
    }
    await rm(workspace, { recursive: true, force: true });
  }
}

function updaterEnvironment(userDataPath, homePath, feedUrl) {
  const environment = createSmokeEnvironment(process.env, {
    HOME: homePath,
    BP_TEST_MODE: '1',
    BP_UPDATE_TEST_MODE: '1',
    BP_DISABLE_RENDERER_DEV_SERVER: '1',
    BP_TEST_USER_DATA_DIR: userDataPath,
  });
  if (feedUrl != null) {
    environment.BP_UPDATE_FEED_URL = feedUrl;
  } else {
    delete environment.BP_UPDATE_FEED_URL;
  }
  return environment;
}

function extractZip(archive, destination) {
  run('ditto', ['-x', '-k', archive, destination]);
}

function verifyAppIdentity(appPath, expectedBundleId) {
  if (!existsSync(appPath)) {
    throw new Error(`Expected app bundle is missing: ${appPath}`);
  }
  const bundleId = readPlistValue(appPath, 'CFBundleIdentifier');
  if (bundleId !== expectedBundleId) {
    throw new Error(`Expected bundle ID ${expectedBundleId}, received ${bundleId}.`);
  }
}

function readPlistValue(appPath, key) {
  return run('/usr/libexec/PlistBuddy', ['-c', `Print :${key}`, join(appPath, 'Contents', 'Info.plist')]);
}

function run(command, args) {
  const result = spawnSync(command, args, { encoding: 'utf8' });
  if (result.status !== 0) {
    throw new Error(`${command} failed: ${(result.stderr || result.stdout).trim()}`);
  }
  return result.stdout.trim();
}

async function waitForRelaunchedProcess(executablePath, excludedPid, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const processList = run('ps', ['-axo', 'pid=,command=']);
    const relaunchedPid = parseExecutableProcessIds(processList, executablePath)
      .find((pid) => pid !== excludedPid);
    if (relaunchedPid != null) {
      return relaunchedPid;
    }
    await new Promise(resolveWait => setTimeout(resolveWait, 250));
  }
  throw new Error(`Updater did not automatically relaunch ${executablePath}.`);
}

async function waitForPidExit(pid, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      process.kill(pid, 0);
    } catch (error) {
      if (error?.code === 'ESRCH') {
        return;
      }
      throw error;
    }
    await new Promise(resolveWait => setTimeout(resolveWait, 100));
  }
  throw new Error(`Automatically relaunched process ${pid} did not exit.`);
}

function corruptArchive(bytes) {
  const corrupted = Buffer.from(bytes);
  const index = Math.max(0, corrupted.length - 64);
  corrupted[index] ^= 0xff;
  return corrupted;
}

async function startFeedServer({ artifactName, artifactBytes, metadata }) {
  const server = createServer((request, response) => {
    const requestPath = new URL(request.url ?? '/', 'http://127.0.0.1').pathname;
    if (requestPath === '/latest-mac.yml') {
      const bytes = Buffer.from(metadata);
      response.writeHead(200, {
        'Content-Type': 'text/yaml',
        'Content-Length': bytes.length,
        'Cache-Control': 'no-store',
      });
      response.end(bytes);
      return;
    }
    if (requestPath === `/${artifactName}`) {
      respondWithBytes(request, response, artifactBytes);
      return;
    }
    response.writeHead(404);
    response.end();
  });
  await new Promise((resolveListen, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolveListen);
  });
  return server;
}

function respondWithBytes(request, response, bytes) {
  const range = request.headers.range;
  if (range == null) {
    response.writeHead(200, {
      'Content-Type': 'application/zip',
      'Content-Length': bytes.length,
      'Accept-Ranges': 'bytes',
      'Cache-Control': 'no-store',
    });
    response.end(bytes);
    return;
  }

  const match = /^bytes=(\d+)-(\d*)$/.exec(range);
  if (match == null) {
    response.writeHead(416, { 'Content-Range': `bytes */${bytes.length}` });
    response.end();
    return;
  }
  const start = Number(match[1]);
  const end = match[2] === '' ? bytes.length - 1 : Math.min(Number(match[2]), bytes.length - 1);
  if (!Number.isSafeInteger(start) || !Number.isSafeInteger(end) || start > end || start >= bytes.length) {
    response.writeHead(416, { 'Content-Range': `bytes */${bytes.length}` });
    response.end();
    return;
  }
  const chunk = bytes.subarray(start, end + 1);
  response.writeHead(206, {
    'Content-Type': 'application/zip',
    'Content-Length': chunk.length,
    'Content-Range': `bytes ${start}-${end}/${bytes.length}`,
    'Accept-Ranges': 'bytes',
    'Cache-Control': 'no-store',
  });
  response.end(chunk);
}

async function waitForUpdateStatus(page, predicate) {
  const deadline = Date.now() + UPDATE_TIMEOUT_MS;
  while (Date.now() < deadline) {
    const status = await page.evaluate(() => window.butterPaper.updates.getStatus());
    if (predicate(status)) {
      return status;
    }
    await new Promise(resolveDelay => setTimeout(resolveDelay, 500));
  }
  throw new Error(`Timed out after ${UPDATE_TIMEOUT_MS}ms waiting for updater state.`);
}

function assertRejected(status, description, expectedMessage) {
  if (status.phase !== 'error' || !status.errorMessage) {
    throw new Error(`Expected ${description} rejection, received ${JSON.stringify(status)}.`);
  }
  if (!expectedMessage.test(status.errorMessage)) {
    throw new Error(
      `Expected ${description} error to match ${expectedMessage}, received: ${status.errorMessage}`,
    );
  }
}

async function assertAppUnchanged(app, appPath, priorVersion) {
  if (app.process().exitCode != null) {
    throw new Error('Rejected update unexpectedly exited the prior application.');
  }
  if (readPlistValue(appPath, 'CFBundleShortVersionString') !== priorVersion) {
    throw new Error('Rejected update modified the installed application version.');
  }
}

async function waitForExit(child, timeoutMs) {
  if (child.exitCode != null) {
    return;
  }
  await new Promise((resolveExit, reject) => {
    const timeout = setTimeout(() => {
      child.removeListener('exit', handleExit);
      reject(new Error('Timed out waiting for updater restart.'));
    }, timeoutMs);
    const handleExit = () => {
      clearTimeout(timeout);
      resolveExit();
    };
    child.once('exit', handleExit);
  });
}

async function waitForInstalledVersion(appPath, expectedVersion, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      if (readPlistValue(appPath, 'CFBundleShortVersionString') === expectedVersion) {
        return;
      }
    } catch {
      // ShipIt temporarily moves the app while applying the archive.
    }
    await new Promise(resolveDelay => setTimeout(resolveDelay, 500));
  }
  throw new Error(`Timed out waiting for installed version ${expectedVersion}.`);
}

const invokedDirectly = process.argv[1] != null
  && import.meta.url === pathToFileURL(resolve(process.argv[1])).href;
if (invokedDirectly) {
  await main();
}
