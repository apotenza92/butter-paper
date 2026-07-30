#!/usr/bin/env node

const { createHash } = require('node:crypto');
const { spawn, spawnSync } = require('node:child_process');
const fs = require('node:fs');
const http = require('node:http');
const os = require('node:os');
const path = require('node:path');
const asar = require('@electron/asar');
const YAML = require('yaml');
const { createTestRepositoryMetadata } = require('./create-test-tuf-repository.cjs');

function option(argv, name) {
  const index = argv.indexOf(name);
  return index >= 0 ? argv[index + 1] : null;
}

function digest(filePath, algorithm = 'sha256', encoding = 'hex') {
  return createHash(algorithm).update(fs.readFileSync(filePath)).digest(encoding);
}

function artifactName(value) {
  if (typeof value !== 'string' || !value) {
    throw new Error('Updater metadata contains an invalid artifact URL.');
  }
  const candidate = /^https?:\/\//.test(value) ? new URL(value).pathname : value;
  const decoded = decodeURIComponent(path.posix.basename(candidate));
  if (!decoded || decoded !== path.posix.basename(decoded) || decoded.includes('\\')) {
    throw new Error(`Unsafe updater artifact name: ${value}`);
  }
  return decoded;
}

function prepareTarget({ baseUrl, candidateDirectory, candidateMetadata, channel }) {
  const metadata = YAML.parse(fs.readFileSync(candidateMetadata, 'utf8'));
  if (!metadata?.version || !Array.isArray(metadata.files) || metadata.files.length === 0) {
    throw new Error('Candidate updater metadata is incomplete.');
  }
  if (!['stable', 'beta'].includes(channel)) {
    throw new Error('Candidate updater metadata requires a stable or beta channel.');
  }
  if (metadata.butterPaperChannel !== undefined && metadata.butterPaperChannel !== channel) {
    throw new Error(`Candidate updater metadata channel does not match ${channel}.`);
  }
  metadata.butterPaperChannel = channel;
  const names = new Set();
  metadata.files = metadata.files.map((entry) => {
    const name = artifactName(entry.url);
    const candidate = path.join(candidateDirectory, name);
    if (names.has(name) || !fs.statSync(candidate, { throwIfNoEntry: false })?.isFile()) {
      throw new Error(`Candidate updater artifact is missing or duplicated: ${name}`);
    }
    names.add(name);
    if (digest(candidate, 'sha512', 'base64') !== entry.sha512) {
      throw new Error(`Candidate updater SHA-512 does not match: ${name}`);
    }
    if (entry.size !== undefined && fs.statSync(candidate).size !== entry.size) {
      throw new Error(`Candidate updater size does not match: ${name}`);
    }
    return { ...entry, url: `${baseUrl}/assets/${encodeURIComponent(name)}` };
  });
  if (metadata.path) {
    const name = artifactName(metadata.path);
    if (!names.has(name)) throw new Error('Legacy updater path does not match a files entry.');
    metadata.path = `${baseUrl}/assets/${encodeURIComponent(name)}`;
  }
  return {
    artifactNames: names,
    bytes: Buffer.from(`${YAML.stringify(metadata, { lineWidth: 0 }).trimEnd()}\n`),
    version: metadata.version,
  };
}

function mutateSignature(metadata) {
  const parsed = JSON.parse(metadata.toString('utf8'));
  const signature = parsed?.signatures?.[0]?.sig;
  if (typeof signature !== 'string' || signature.length < 2) {
    throw new Error('Cannot create wrong-signature TUF fixture.');
  }
  parsed.signatures[0].sig = `${signature[0] === '0' ? '1' : '0'}${signature.slice(1)}`;
  return Buffer.from(JSON.stringify(parsed));
}

function serveFile(request, response, filePath) {
  const bytes = fs.readFileSync(filePath);
  const range = request.headers.range?.match(/^bytes=(\d+)-(\d*)$/);
  if (!range) {
    response.writeHead(200, {
      'Accept-Ranges': 'bytes',
      'Content-Length': bytes.length,
    });
    response.end(request.method === 'HEAD' ? undefined : bytes);
    return;
  }
  const start = Number(range[1]);
  const end = range[2] ? Math.min(Number(range[2]), bytes.length - 1) : bytes.length - 1;
  if (!Number.isSafeInteger(start) || start < 0 || start > end || start >= bytes.length) {
    response.writeHead(416, { 'Content-Range': `bytes */${bytes.length}` }).end();
    return;
  }
  response.writeHead(206, {
    'Accept-Ranges': 'bytes',
    'Content-Length': end - start + 1,
    'Content-Range': `bytes ${start}-${end}/${bytes.length}`,
  });
  response.end(request.method === 'HEAD' ? undefined : bytes.subarray(start, end + 1));
}

async function createServer({
  candidateDirectory,
  candidateMetadata,
  channel,
  privateKeyPath,
  rootPath,
  scenario,
  targetName,
}) {
  const requests = [];
  let target;
  let metadata;
  const server = http.createServer((request, response) => {
    const pathname = decodeURIComponent(new URL(request.url, 'http://127.0.0.1').pathname);
    requests.push(pathname);
    if (!['GET', 'HEAD'].includes(request.method)) {
      response.writeHead(405).end();
      return;
    }
    const metadataMatch = pathname.match(/^\/tuf\/metadata\/([^/]+)$/);
    if (metadataMatch) {
      if (metadataMatch[1] === '2.root.json') {
        response.writeHead(404).end();
        return;
      }
      let bytes = metadata?.[metadataMatch[1]];
      if (scenario === 'wrong-signature' && metadataMatch[1] === 'targets.json') {
        bytes = mutateSignature(bytes);
      }
      if (!bytes) {
        response.writeHead(404).end();
        return;
      }
      response.writeHead(200, { 'Content-Length': bytes.length });
      response.end(request.method === 'HEAD' ? undefined : bytes);
      return;
    }
    if (pathname === `/tuf/targets/${encodeURIComponent(targetName)}`) {
      const bytes = scenario === 'corrupt-payload'
        ? Buffer.concat([target.bytes, Buffer.from('\ncorrupt\n')])
        : target.bytes;
      response.writeHead(200, { 'Content-Length': bytes.length });
      response.end(request.method === 'HEAD' ? undefined : bytes);
      return;
    }
    const assetMatch = pathname.match(/^\/assets\/([^/]+)$/);
    if (assetMatch && target.artifactNames.has(assetMatch[1])) {
      serveFile(request, response, path.join(candidateDirectory, assetMatch[1]));
      return;
    }
    response.writeHead(404).end();
  });
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  const baseUrl = `http://127.0.0.1:${server.address().port}`;
  target = prepareTarget({ baseUrl, candidateDirectory, candidateMetadata, channel });
  metadata = createTestRepositoryMetadata({
    privateKeyPath,
    rootPath,
    targetBytes: target.bytes,
    targetName,
  });
  return {
    baseUrl,
    close: () => new Promise((resolve, reject) => {
      server.close(error => error ? reject(error) : resolve());
    }),
    requests,
    targetBytes: target.bytes,
    version: target.version,
  };
}

function readEvents(eventPath) {
  const history = `${eventPath}.jsonl`;
  if (!fs.existsSync(history)) return [];
  return fs.readFileSync(history, 'utf8').trim().split('\n').filter(Boolean).map(JSON.parse);
}

async function waitForEvent(eventPath, accepted, timeoutMs = 300_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const event = readEvents(eventPath).find(candidate => accepted.has(candidate.name));
    if (event) return event;
    await new Promise(resolve => setTimeout(resolve, 250));
  }
  throw new Error(`Timed out waiting for updater event: ${[...accepted].join(', ')}.`);
}

function run(command, args) {
  const result = spawnSync(command, args, { encoding: 'utf8', stdio: 'inherit' });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} failed with status ${result.status}.`);
}

function installedArchive(executable) {
  return path.join(path.dirname(executable), 'resources', 'app.asar');
}

function archiveVersion(archive) {
  asar.uncache(archive);
  return JSON.parse(asar.extractFile(archive, 'package.json').toString('utf8')).version;
}

function installedVersion(executable) {
  return archiveVersion(installedArchive(executable));
}

function isPidAlive(pid) {
  if (!Number.isInteger(pid) || pid <= 0) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return error?.code === 'EPERM';
  }
}

async function stopPid(pid) {
  if (!isPidAlive(pid)) return;
  if (process.platform === 'win32') {
    spawnSync('taskkill.exe', ['/PID', String(pid), '/T', '/F'], { stdio: 'ignore' });
  } else {
    process.kill(pid);
  }
  const deadline = Date.now() + 15_000;
  while (isPidAlive(pid) && Date.now() < deadline) {
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  if (isPidAlive(pid)) throw new Error(`Updater process ${pid} did not stop.`);
}

function windowsProcessIds(executable) {
  const result = spawnSync('powershell.exe', [
    '-NoProfile',
    '-NonInteractive',
    '-Command',
    '$p=@(Get-CimInstance Win32_Process | Where-Object {$_.ExecutablePath -eq $env:BP_AUDIT_EXECUTABLE} | Select-Object -ExpandProperty ProcessId); ConvertTo-Json -Compress -InputObject $p',
  ], {
    encoding: 'utf8',
    env: { ...process.env, BP_AUDIT_EXECUTABLE: executable },
  });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error('Could not inspect installed Butter Paper processes.');
  const parsed = JSON.parse(result.stdout.trim() || '[]');
  return (Array.isArray(parsed) ? parsed : [parsed])
    .map(Number)
    .filter(pid => Number.isInteger(pid) && pid > 0);
}

async function waitForPathRemoval(target, timeoutMs = 60_000) {
  const deadline = Date.now() + timeoutMs;
  while (fs.existsSync(target) && Date.now() < deadline) {
    await new Promise(resolve => setTimeout(resolve, 250));
  }
  if (fs.existsSync(target)) throw new Error(`Native uninstaller did not remove ${target}.`);
}

function restrictedEnvironment(overrides) {
  const allowed = process.platform === 'win32'
    ? [
      'ALLUSERSPROFILE', 'APPDATA', 'ComSpec', 'HOMEDRIVE', 'HOMEPATH',
      'LOCALAPPDATA', 'OS', 'Path', 'PATHEXT', 'PROCESSOR_ARCHITECTURE',
      'ProgramData', 'ProgramFiles', 'SystemDrive', 'SystemRoot', 'TEMP', 'TMP',
      'USERNAME', 'USERPROFILE', 'windir',
    ]
    : ['DBUS_SESSION_BUS_ADDRESS', 'DISPLAY', 'HOME', 'LANG', 'LC_ALL', 'PATH', 'TEMP', 'TMP', 'TMPDIR', 'USER', 'XAUTHORITY'];
  return Object.fromEntries([
    ...allowed.flatMap(name => process.env[name] ? [[name, process.env[name]]] : []),
    ...Object.entries(overrides).filter(([, value]) => value !== undefined),
  ]);
}

function findWindowsArchives(root, depth = 0) {
  if (depth > 5 || !fs.statSync(root, { throwIfNoEntry: false })?.isDirectory()) return [];
  const archives = [];
  for (const entry of fs.readdirSync(root, { withFileTypes: true })) {
    const candidate = path.join(root, entry.name);
    if (entry.isDirectory()) {
      archives.push(...findWindowsArchives(candidate, depth + 1));
    } else if (entry.isFile() && entry.name === 'app.asar') {
      try {
        archives.push({
          path: candidate,
          sha256: digest(candidate),
          version: archiveVersion(candidate),
        });
      } catch (error) {
        archives.push({ path: candidate, error: error.message });
      }
    }
  }
  return archives;
}

function powershellJson(command) {
  const result = spawnSync('powershell.exe', [
    '-NoLogo',
    '-NoProfile',
    '-NonInteractive',
    '-Command',
    command,
  ], { encoding: 'utf8' });
  if (result.error) return { error: result.error.message };
  if (result.status !== 0) {
    return { error: result.stderr.trim() || `PowerShell exited with status ${result.status}` };
  }
  try {
    return JSON.parse(result.stdout.trim() || '[]');
  } catch (error) {
    return { error: error.message, stdout: result.stdout.trim() };
  }
}

function windowsReplacementSnapshot(executable, candidateArchive, version, appPid) {
  let installed;
  try {
    const archive = installedArchive(executable);
    installed = {
      executableExists: fs.existsSync(executable),
      archive,
      archiveExists: fs.existsSync(archive),
      sha256: fs.existsSync(archive) ? digest(archive) : null,
      version: fs.existsSync(archive) ? archiveVersion(archive) : null,
    };
  } catch (error) {
    installed = { error: error.message };
  }
  return {
    expected: {
      archive: candidateArchive,
      sha256: digest(candidateArchive),
      version,
    },
    installed,
    appPid,
    appPidAlive: isPidAlive(appPid),
    archivesUnderLocalPrograms: findWindowsArchives(
      path.join(process.env.LOCALAPPDATA, 'Programs'),
    ),
    processes: powershellJson(
      '$items=@(Get-CimInstance Win32_Process | '
      + "Where-Object {$_.Name -match 'Butter|Setup|Uninstall' -or "
      + "$_.ExecutablePath -like \"$env:LOCALAPPDATA\\Programs\\*\"} | "
      + 'Select-Object Name,ProcessId,ParentProcessId,ExecutablePath,CommandLine); '
      + 'ConvertTo-Json -Compress -Depth 3 -InputObject $items',
    ),
    uninstallEntries: powershellJson(
      "$items=@(Get-ItemProperty 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\*' "
      + "| Where-Object {$_.DisplayName -like 'Butter Paper*'} "
      + '| Select-Object DisplayName,DisplayVersion,InstallLocation,UninstallString); '
      + 'ConvertTo-Json -Compress -Depth 3 -InputObject $items',
    ),
  };
}

async function waitForWindowsReplacement(executable, candidateArchive, version, appPid) {
  const expected = digest(candidateArchive);
  const deadline = Date.now() + 180_000;
  while (Date.now() < deadline) {
    try {
      if (digest(installedArchive(executable)) === expected && installedVersion(executable) === version) {
        return expected;
      }
    } catch {
      // The installer briefly makes the package unreadable during replacement.
    }
    await new Promise(resolve => setTimeout(resolve, 500));
  }
  const snapshot = windowsReplacementSnapshot(executable, candidateArchive, version, appPid);
  throw new Error(
    `Timed out waiting for byte-exact Windows N-1 replacement.\n${JSON.stringify(snapshot, null, 2)}`,
  );
}

function writeEvidence({
  arch,
  candidateDirectory,
  candidateMetadata,
  evidenceDirectory,
  failure,
  previousArtifact,
  rootPath,
  scenarios,
}) {
  fs.mkdirSync(evidenceDirectory, { recursive: true });
  fs.writeFileSync(path.join(evidenceDirectory, failure ? 'FAILURE.txt' : 'RESULT.txt'),
    failure
      ? `Native updater audit failed closed: ${failure.stack || failure}\n`
      : 'Native updater audit rejected unauthenticated updates, installed the authenticated candidate, and preserved user data.\n');
  fs.writeFileSync(path.join(evidenceDirectory, 'ENVIRONMENT.txt'), [
    `Platform: ${process.platform}`,
    `Architecture: ${arch}`,
    `Previous artifact: ${path.basename(previousArtifact)}`,
    `Candidate metadata: ${path.basename(candidateMetadata)}`,
    'Trust: ephemeral loopback-only TUF test root',
    '',
  ].join('\n'));
  fs.copyFileSync(rootPath, path.join(evidenceDirectory, 'root.json'));
  fs.copyFileSync(candidateMetadata, path.join(evidenceDirectory, path.basename(candidateMetadata)));
  for (const [name, scenario] of Object.entries(scenarios)) {
    fs.writeFileSync(path.join(evidenceDirectory, `${name}-requests.txt`), `${scenario.requests.join('\n')}\n`);
    if (fs.existsSync(`${scenario.eventPath}.jsonl`)) {
      fs.copyFileSync(`${scenario.eventPath}.jsonl`, path.join(evidenceDirectory, `${name}-events.jsonl`));
    }
  }
  const artifacts = [previousArtifact, ...fs.readdirSync(candidateDirectory)
    .map(name => path.join(candidateDirectory, name))
    .filter(candidate => fs.statSync(candidate).isFile())];
  fs.writeFileSync(path.join(evidenceDirectory, 'PACKAGE_SHA256SUMS'), artifacts
    .map(artifact => `${digest(artifact)}  ${path.basename(artifact)}`)
    .sort()
    .join('\n')
    .concat('\n'));
}

async function main(argv = process.argv.slice(2)) {
  if (!['win32', 'linux'].includes(process.platform)) {
    throw new Error('Butter Paper non-macOS updater tests require a native Windows or Linux runner.');
  }
  const arch = option(argv, '--arch') || process.arch;
  const channel = option(argv, '--channel');
  if (arch !== process.arch || !['arm64', 'x64'].includes(arch)) {
    throw new Error(`Updater audit requires native ${arch}; current Node is ${process.arch}.`);
  }
  if (!['stable', 'beta'].includes(channel)) throw new Error('Updater audit channel must be stable or beta.');
  const previousArtifact = path.resolve(option(argv, '--previous-artifact'));
  const candidateDirectory = path.resolve(option(argv, '--candidate-directory'));
  const candidateMetadata = path.resolve(option(argv, '--candidate-metadata'));
  const evidenceDirectory = path.resolve(option(argv, '--evidence'));
  const privateKeyPath = path.resolve(option(argv, '--private-key'));
  const rootPath = path.resolve(option(argv, '--root'));
  if (fs.existsSync(evidenceDirectory)) throw new Error('Updater evidence directory must not already exist.');
  for (const required of [previousArtifact, candidateDirectory, candidateMetadata, privateKeyPath, rootPath]) {
    if (!fs.existsSync(required)) throw new Error(`Updater audit input is missing: ${required}`);
  }

  const productName = channel === 'beta' ? 'Butter Paper Beta' : 'Butter Paper';
  const targetName = process.platform === 'win32'
    ? 'latest.yml'
    : arch === 'arm64' ? 'latest-linux-arm64.yml' : 'latest-linux.yml';
  const temporaryRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'butter-paper-updater-'));
  const userData = path.join(temporaryRoot, 'user-data');
  const markerPath = path.join(userData, 'preservation-marker.bin');
  const settingsPath = path.join(userData, 'update-settings.json');
  const marker = Buffer.from('Butter Paper preserved updater data\n');
  fs.mkdirSync(userData, { recursive: true });
  fs.writeFileSync(markerPath, marker);
  fs.writeFileSync(settingsPath, `${JSON.stringify({
    schemaVersion: 1,
    frequency: 'startup',
    lastSuccessfulCheckAt: null,
  })}\n`);

  let installedExecutable;
  let installedAppImage;
  let child;
  let relaunchedPid;
  let failure;
  const scenarios = {};
  try {
    if (process.platform === 'win32') {
      run(previousArtifact, ['/S']);
      installedExecutable = path.join(process.env.LOCALAPPDATA, 'Programs', productName, `${productName}.exe`);
      if (!fs.existsSync(installedExecutable)) throw new Error('Previous Windows installer did not install the application.');
    } else {
      installedAppImage = path.join(temporaryRoot, path.basename(previousArtifact));
      fs.copyFileSync(previousArtifact, installedAppImage);
      fs.chmodSync(installedAppImage, 0o755);
      installedExecutable = installedAppImage;
    }
    const initialInstalledDigest = process.platform === 'win32'
      ? digest(installedArchive(installedExecutable))
      : digest(installedAppImage);

    for (const scenarioName of ['wrong-signature', 'corrupt-payload', 'valid']) {
      const eventPath = path.join(temporaryRoot, 'events', `${scenarioName}.json`);
      const server = await createServer({
        candidateDirectory,
        candidateMetadata,
        channel,
        privateKeyPath,
        rootPath,
        scenario: scenarioName,
        targetName,
      });
      scenarios[scenarioName] = {
        close: server.close,
        closed: false,
        eventPath,
        requests: server.requests,
      };
      const environment = restrictedEnvironment({
        APPIMAGE: process.platform === 'linux' ? installedAppImage : undefined,
        APPIMAGE_EXTRACT_AND_RUN: process.platform === 'linux' ? '1' : undefined,
        BP_TEST_MODE: '1',
        BP_TEST_USER_DATA_DIR: userData,
        BP_TUF_REPOSITORY_URL: `${server.baseUrl}/tuf`,
        BP_UPDATE_EVENT_PATH: eventPath,
        BP_UPDATE_EXPECT_VERSION: scenarioName === 'valid' ? server.version : undefined,
        BP_UPDATE_INSTALL: scenarioName === 'valid' ? '1' : undefined,
        BP_UPDATE_TEST_MODE: '1',
      });
      child = spawn(installedExecutable, [], { env: environment, stdio: 'inherit' });
      const event = await waitForEvent(
        eventPath,
        new Set(scenarioName === 'valid'
          ? [process.platform === 'win32' ? 'update-downloaded' : 'updated-runtime-launched', 'error']
          : ['error', 'update-downloaded']),
      );
      if (scenarioName !== 'valid') {
        if (event.name !== 'error') throw new Error(`${scenarioName} update did not fail closed.`);
        if (server.requests.some(request => request.startsWith('/assets/'))) {
          throw new Error(`${scenarioName} update reached unauthenticated package bytes.`);
        }
        const actualDigest = process.platform === 'win32'
          ? digest(installedArchive(installedExecutable))
          : digest(installedAppImage);
        if (actualDigest !== initialInstalledDigest) throw new Error(`${scenarioName} update changed installed bytes.`);
        await stopPid(child.pid);
        child = undefined;
        await server.close();
        scenarios[scenarioName].closed = true;
        const persistedMetadata = path.join(userData, 'update-trust', 'metadata');
        for (const name of fs.readdirSync(persistedMetadata, { withFileTypes: true })) {
          if (name.name !== 'root.json') {
            fs.rmSync(path.join(persistedMetadata, name.name), { recursive: true, force: true });
          }
        }
        fs.rmSync(path.join(userData, 'update-trust', 'targets'), { recursive: true, force: true });
        continue;
      }
      if (event.name === 'error') throw new Error(`Native updater failed: ${event.message || '<missing>'}`);
      if (process.platform === 'win32') {
        const unpacked = arch === 'x64' ? 'win-unpacked' : `win-${arch}-unpacked`;
        const candidateArchive = path.join(candidateDirectory, unpacked, 'resources', 'app.asar');
        await waitForWindowsReplacement(
          installedExecutable,
          candidateArchive,
          server.version,
          child.pid,
        );
        const relaunched = await waitForEvent(
          eventPath,
          new Set(['updated-runtime-launched', 'error']),
          180_000,
        );
        if (relaunched.name === 'error') {
          throw new Error(`Updated Windows runtime failed: ${relaunched.message || '<missing>'}`);
        }
        if (relaunched.currentVersion !== server.version
          || !Number.isInteger(relaunched.pid)
          || relaunched.pid <= 0) {
          throw new Error('Updated Windows runtime did not relaunch at the candidate version.');
        }
        relaunchedPid = relaunched.pid;
      } else {
        if (event.currentVersion !== server.version) throw new Error('Updated AppImage runtime reported the wrong version.');
        relaunchedPid = event.pid;
        if (!Number.isInteger(relaunchedPid) || relaunchedPid <= 0) {
          throw new Error('Updated AppImage runtime reported an invalid process ID.');
        }
        const candidateName = artifactName(YAML.parse(fs.readFileSync(candidateMetadata, 'utf8')).files[0].url);
        if (digest(installedAppImage) !== digest(path.join(candidateDirectory, candidateName))) {
          throw new Error('AppImage updater did not replace the installed bytes.');
        }
      }
      if (!fs.readFileSync(markerPath).equals(marker)) throw new Error('Updater changed existing user data.');
      const persistedRoot = path.join(userData, 'update-trust', 'metadata', 'root.json');
      if (JSON.parse(fs.readFileSync(persistedRoot, 'utf8')).signed.version !== 1) {
        throw new Error('Updated runtime did not preserve its TUF root.');
      }
      if (!server.requests.includes(`/tuf/targets/${encodeURIComponent(targetName)}`)
        || !server.requests.some(request => request.startsWith('/assets/'))) {
        throw new Error('Updater did not request both authenticated metadata and package bytes.');
      }
      await stopPid(child.pid);
      child = undefined;
      await server.close();
      scenarios[scenarioName].closed = true;
    }
  } catch (error) {
    failure = error;
    throw error;
  } finally {
    let cleanupFailure;
    await stopPid(relaunchedPid).catch(() => undefined);
    await stopPid(child?.pid).catch(() => undefined);
    if (process.platform === 'win32' && installedExecutable) {
      for (const pid of windowsProcessIds(installedExecutable)) {
        await stopPid(pid).catch(() => undefined);
      }
    }
    for (const scenario of Object.values(scenarios)) {
      if (!scenario.closed) await scenario.close().catch(() => undefined);
    }
    try {
      if (process.platform === 'win32' && installedExecutable && fs.existsSync(path.dirname(installedExecutable))) {
        const uninstallers = fs.readdirSync(path.dirname(installedExecutable))
          .filter(name => /^uninstall.*\.exe$/i.test(name));
        if (uninstallers.length !== 1) throw new Error('Native audit could not resolve exactly one NSIS uninstaller.');
        const installDirectory = path.dirname(installedExecutable);
        run(path.join(installDirectory, uninstallers[0]), ['/S']);
        await waitForPathRemoval(installDirectory);
      }
    } catch (error) {
      cleanupFailure = error;
    }
    writeEvidence({
      arch,
      candidateDirectory,
      candidateMetadata,
      evidenceDirectory,
      failure: failure || cleanupFailure,
      previousArtifact,
      rootPath,
      scenarios,
    });
    fs.rmSync(temporaryRoot, { recursive: true, force: true, maxRetries: 20, retryDelay: 250 });
    if (!failure && cleanupFailure) throw cleanupFailure;
  }
}

if (require.main === module) {
  main().catch((error) => {
    process.stderr.write(`${error.stack || error}\n`);
    process.exitCode = 1;
  });
}

module.exports = {
  artifactName,
  prepareTarget,
};
