#!/usr/bin/env node

import { createHash } from "node:crypto";
import {
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  realpath,
  rm,
  writeFile,
} from "node:fs/promises";
import { tmpdir, userInfo } from "node:os";
import { basename, dirname, isAbsolute, join, resolve } from "node:path";
import { spawn, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import {
  stableRecoveryStoreRoot,
  tryReadProductionOpenEvidence,
} from "./production-open-evidence.mjs";

const MAX_ARCHIVE = 256 * 1024 * 1024;
const MAX_RECEIPT = 1024 * 1024;
const MAX_FIXTURE = 32 * 1024 * 1024;
const MAX_LOG = 512 * 1024;
const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");
const fail = (message) => {
  throw new Error(message);
};
const bounded = (text) => String(text).slice(-MAX_LOG);

export function validateFixturePdf(bytes) {
  if (
    !Buffer.isBuffer(bytes) ||
    bytes.length < 8 ||
    bytes.length > MAX_FIXTURE ||
    bytes.subarray(0, 5).toString("ascii") !== "%PDF-" ||
    !bytes.includes(Buffer.from("%%EOF"))
  )
    fail(
      "fixture must be a non-empty, bounded PDF with a header and EOF marker",
    );
  return { bytes: bytes.length, sha256: hash(bytes) };
}

export function validateExtractedInventory(entries) {
  if (entries.length !== 1 || entries[0] !== "Butter Paper.app")
    fail(`unexpected extracted top-level inventory: ${entries.join(", ")}`);
  return true;
}

export function isolatedAppEnvironment({ home, root }) {
  return {
    PATH: "/usr/bin:/bin:/usr/sbin:/sbin",
    HOME: home,
    CFFIXED_USER_HOME: home,
    TMPDIR: join(root, "tmp"),
    XDG_CACHE_HOME: join(root, "cache"),
    XDG_CONFIG_HOME: join(root, "config"),
    XDG_DATA_HOME: join(root, "data"),
    XDG_STATE_HOME: join(root, "state"),
    LANG: "en_US.UTF-8",
    LC_ALL: "en_US.UTF-8",
  };
}

export function validateDisposableMacosRunner({ environment, effectiveHome }) {
  if (environment.GITHUB_ACTIONS !== "true" || environment.CI !== "true") {
    fail(
      "signed macOS package smoke is restricted to a disposable GitHub Actions runner",
    );
  }
  if (
    typeof environment.RUNNER_TEMP !== "string" ||
    !environment.RUNNER_TEMP ||
    !isAbsolute(environment.RUNNER_TEMP)
  ) {
    fail("disposable macOS runner must expose an absolute RUNNER_TEMP");
  }
  if (
    typeof effectiveHome !== "string" ||
    !effectiveHome ||
    !isAbsolute(effectiveHome)
  ) {
    fail("effective macOS account home is unavailable");
  }
  const stableNativeRoot = join(
    resolve(effectiveHome),
    "Library",
    "Application Support",
    "com.butterpaper.desktop",
    "native-v1",
  );
  return {
    effectiveHome: resolve(effectiveHome),
    runnerTemp: resolve(environment.RUNNER_TEMP),
    stableNativeRoot,
  };
}

export function validatePackageReceipts(
  archivePath,
  archiveBytes,
  packageManifest,
  verification,
) {
  const artifact = {
    path: basename(archivePath),
    bytes: archiveBytes.length,
    sha256: hash(archiveBytes),
  };
  const packageFields = [
    "schema",
    "schemaVersion",
    "target",
    "channel",
    "version",
    "sourceRevision",
    "artifact",
    "archiveFormat",
    "appBundle",
    "assemblyManifestSha256",
    "signingReceiptSha256",
    "signingCertificateSha256",
    "notarisation",
    "stapled",
  ];
  const verificationFields = [
    "schema",
    "schemaVersion",
    "target",
    "channel",
    "version",
    "sourceRevision",
    "artifact",
    "verified",
    "verification",
    "signingCertificateSha256",
    "assemblyManifestSha256",
    "signingReceiptSha256",
    "notarisation",
    "stapled",
    "extractedAppVerified",
  ];
  if (
    JSON.stringify(Object.keys(packageManifest ?? {}).sort()) !==
      JSON.stringify(packageFields.sort()) ||
    JSON.stringify(Object.keys(verification ?? {}).sort()) !==
      JSON.stringify(verificationFields.sort())
  )
    fail("package receipts have missing or unknown JSON fields");
  if (
    packageManifest?.schema !== "butter-paper/package-manifest" ||
    packageManifest.schemaVersion !== 1 ||
    verification?.schema !== "butter-paper/package-verification" ||
    verification.schemaVersion !== 1
  )
    fail("unsupported package receipt schema");
  for (const receipt of [packageManifest, verification]) {
    if (
      receipt.channel !== "stable" ||
      !["macos-arm64", "macos-x64"].includes(receipt.target) ||
      JSON.stringify(receipt.artifact) !== JSON.stringify(artifact)
    )
      fail("archive identity does not match adjacent package receipts");
  }
  if (
    packageManifest.archiveFormat !== "zip" ||
    packageManifest.appBundle !== "Butter Paper.app" ||
    verification.verified !== true ||
    verification.verification !== "strict-signed-macos-production" ||
    verification.extractedAppVerified !== true ||
    !/^\d+\.\d+\.\d+(?:\+[0-9A-Za-z.-]+)?$/.test(
      packageManifest.version ?? "",
    ) ||
    !/^[0-9a-f]{40}$/.test(packageManifest.sourceRevision ?? "") ||
    !/^[0-9a-f]{64}$/i.test(packageManifest.signingCertificateSha256 ?? "") ||
    !/^[0-9a-f]{64}$/.test(packageManifest.assemblyManifestSha256 ?? "") ||
    !/^[0-9a-f]{64}$/.test(packageManifest.signingReceiptSha256 ?? "") ||
    packageManifest.version !== verification.version ||
    packageManifest.sourceRevision !== verification.sourceRevision ||
    packageManifest.target !== verification.target ||
    packageManifest.signingCertificateSha256 !==
      verification.signingCertificateSha256 ||
    packageManifest.assemblyManifestSha256 !==
      verification.assemblyManifestSha256 ||
    packageManifest.signingReceiptSha256 !==
      verification.signingReceiptSha256 ||
    packageManifest.notarisation?.status !== "Accepted" ||
    typeof packageManifest.notarisation?.submissionId !== "string" ||
    !packageManifest.notarisation.submissionId ||
    JSON.stringify(packageManifest.notarisation) !==
      JSON.stringify(verification.notarisation) ||
    packageManifest.stapled !== true ||
    verification.stapled !== true ||
    artifact.path !==
      `butter-paper-macos-${packageManifest.target === "macos-arm64" ? "arm64" : "x64"}-${packageManifest.version}.zip`
  )
    fail(
      "package and verification receipts disagree or contain invalid fields",
    );
  return {
    target: packageManifest.target,
    version: packageManifest.version,
    sourceRevision: packageManifest.sourceRevision,
    archiveSha256: artifact.sha256,
    signingCertificateSha256:
      packageManifest.signingCertificateSha256.toUpperCase(),
  };
}

async function checkedFile(path, maxBytes, label) {
  const stat = await lstat(path);
  if (
    !stat.isFile() ||
    stat.isSymbolicLink() ||
    stat.nlink !== 1 ||
    stat.size <= 0 ||
    stat.size > maxBytes
  )
    fail(
      `${label} must be a regular single-link file no larger than ${maxBytes} bytes`,
    );
  return readFile(path);
}
async function readJson(path, label) {
  const bytes = await checkedFile(path, MAX_RECEIPT, label);
  try {
    return JSON.parse(bytes.toString("utf8"));
  } catch {
    fail(`${label} must contain valid JSON`);
  }
}
function execute(
  command,
  args,
  logs,
  { timeout = 30_000, maxBuffer = 2 * 1024 * 1024 } = {},
) {
  const result = spawnSync(command, args, {
    encoding: "utf8",
    timeout,
    maxBuffer,
  });
  const out = bounded(result.stdout ?? ""),
    err = bounded(result.stderr ?? result.error?.message ?? "");
  logs.stdout = bounded(`${logs.stdout}${out}`);
  logs.stderr = bounded(`${logs.stderr}${err}`);
  if (result.error || result.status !== 0)
    fail(`${command} failed (${result.status ?? "unknown"}): ${err || out}`);
  return `${out}${err}`.trim();
}
function sleep(ms) {
  return new Promise((resolveSleep) => setTimeout(resolveSleep, ms));
}
function processCommand(pid) {
  const result = spawnSync("/bin/ps", ["-p", String(pid), "-o", "command="], {
    encoding: "utf8",
    timeout: 5000,
  });
  if (result.error || ![0, 1].includes(result.status))
    fail(`could not inspect process ${pid}`);
  return result.stdout.trim();
}
function isOwnedPath(pid, paths) {
  const command = processCommand(pid);
  return paths.some(
    (path) => command === path || command.startsWith(`${path} `),
  );
}
function pidsForExecutable(path, logs) {
  const result = spawnSync(
    "/usr/bin/pgrep",
    ["-f", `^${path.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}( |$)`],
    { encoding: "utf8", timeout: 5000 },
  );
  logs.stdout = bounded(`${logs.stdout}${result.stdout ?? ""}`);
  logs.stderr = bounded(`${logs.stderr}${result.stderr ?? ""}`);
  if (![0, 1].includes(result.status) || result.error)
    fail(
      `could not inspect packaged worker process table: ${result.error?.message ?? result.stderr ?? result.status}`,
    );
  return (result.stdout ?? "")
    .trim()
    .split(/\s+/)
    .filter(Boolean)
    .map(Number)
    .filter(
      (pid) => Number.isSafeInteger(pid) && pid > 0 && isOwnedPath(pid, [path]),
    );
}

export async function smokeSignedMacosPackage({
  archivePath,
  fixturePdfPath,
  evidenceRoot,
  timeoutMs = 60_000,
}) {
  const requestedEvidenceStat = await lstat(evidenceRoot);
  if (
    !requestedEvidenceStat.isDirectory() ||
    requestedEvidenceStat.isSymbolicLink()
  )
    fail("evidence root must be an existing non-symlink directory");
  const evidenceDir = await realpath(evidenceRoot);
  const evidenceStat = await lstat(evidenceDir);
  if (!evidenceStat.isDirectory() || evidenceStat.isSymbolicLink())
    fail("evidence root must be an existing real directory");
  const evidencePath = join(
    evidenceDir,
    `macos-runtime-smoke-${Date.now()}-${process.pid}.json`,
  );
  const logs = { stdout: "", stderr: "" };
  const result = {
    schema: "butter-paper/macos-runtime-smoke",
    schemaVersion: 1,
    startedAt: new Date().toISOString(),
    passed: false,
    claims: [
      "exact signed package identity",
      "normal accessible Rectangle selection",
      "CoreGraphics pointer drag",
      "normal Save",
      "PDFKit rectangle inspection",
      "normal app exit",
      "fresh packaged app reopen",
      "verified process cleanup",
    ],
    limitations: [
      "requires macOS Accessibility trust for the standalone smoke driver",
    ],
    logs: {},
    cleanup: { status: "not-started" },
  };
  let root,
    child,
    workerPids = [];
  const children = [];
  let failure;
  try {
    if (process.platform !== "darwin")
      fail("signed macOS package runtime smoke requires macOS");
    const disposableRunner = validateDisposableMacosRunner({
      environment: process.env,
      effectiveHome: userInfo().homedir,
    });
    try {
      await lstat(disposableRunner.stableNativeRoot);
      fail(
        "disposable macOS runner already contains stable native application data",
      );
    } catch (error) {
      if (error?.code !== "ENOENT") throw error;
    }
    result.storageAuthority = {
      mode: "disposable-github-host-effective-user",
      stableNativeRoot: disposableRunner.stableNativeRoot,
      note: "production macOS resolves the effective account home independently of HOME; the entire hosted runner is the isolation boundary",
    };
    if (
      !Number.isSafeInteger(timeoutMs) ||
      timeoutMs < 10_000 ||
      timeoutMs > 180_000
    )
      fail("timeoutMs must be between 10000 and 180000");
    const archive = await checkedFile(
      archivePath,
      MAX_ARCHIVE,
      "package archive",
    );
    const pkgPath = resolve(
      dirname(archivePath),
      `${basename(archivePath).replace(/\.zip$/i, "")}.package.json`,
    );
    const receiptPath = resolve(
      dirname(archivePath),
      `${basename(archivePath).replace(/\.zip$/i, "")}.verification.json`,
    );
    const [pkg, receipt] = await Promise.all([
      readJson(pkgPath, "package manifest"),
      readJson(receiptPath, "verification receipt"),
    ]);
    const fixture = await checkedFile(
      fixturePdfPath,
      MAX_FIXTURE,
      "fixture PDF",
    );
    result.fixture = validateFixturePdf(fixture);
    const identity = validatePackageReceipts(
      archivePath,
      archive,
      pkg,
      receipt,
    );
    const hostArch = execute("/usr/bin/uname", ["-m"], logs).trim();
    if (
      (identity.target === "macos-arm64" && hostArch !== "arm64") ||
      (identity.target === "macos-x64" && hostArch !== "x86_64")
    )
      fail(
        `host architecture ${hostArch} does not natively match ${identity.target}`,
      );
    result.identity = { ...identity, target: identity.target, hostArch };
    root = await mkdtemp(join(tmpdir(), "bp-signed-macos-runtime-"));
    const extracted = join(root, "extract");
    await mkdir(extracted, { mode: 0o700 });
    execute(
      "/usr/bin/ditto",
      ["-x", "-k", resolve(archivePath), extracted],
      logs,
    );
    const inventory = await readdir(extracted);
    validateExtractedInventory(inventory);
    const app = join(extracted, "Butter Paper.app"),
      executable = join(app, "Contents", "MacOS", "Butter Paper"),
      worker = join(app, "Contents", "MacOS", "butter-paper-pdf-worker");
    execute(
      "/usr/bin/codesign",
      ["--verify", "--deep", "--strict", "--verbose=2", app],
      logs,
    );
    const signature = execute(
      "/usr/bin/codesign",
      ["-dv", "--verbose=4", app],
      logs,
    );
    if (
      !signature.includes(
        "Authority=Developer ID Application: Alexander Potenza (27JL2VERNC)",
      ) ||
      !signature.includes("TeamIdentifier=27JL2VERNC")
    )
      fail(
        "extracted app signing identity does not match the trusted stable identity",
      );
    for (const [role, codePath] of [
      ["app", app],
      ["worker", worker],
    ]) {
      const certificatePrefix = join(root, `signing-certificate-${role}-`);
      execute(
        "/usr/bin/codesign",
        ["-d", `--extract-certificates=${certificatePrefix}`, codePath],
        logs,
      );
      const leafCertificate = await checkedFile(
        `${certificatePrefix}0`,
        1024 * 1024,
        `extracted ${role} leaf signing certificate`,
      );
      if (
        hash(leafCertificate).toUpperCase() !==
        identity.signingCertificateSha256
      )
        fail(
          `extracted ${role} leaf signing certificate does not match the package receipt fingerprint`,
        );
    }
    execute(
      "/usr/sbin/spctl",
      ["--assess", "--type", "execute", "--verbose=4", app],
      logs,
    );
    const archs = execute(
      "/usr/bin/lipo",
      ["-archs", join(app, "Contents", "MacOS", "Butter Paper")],
      logs,
    ).split(/\s+/);
    if (
      identity.target === "macos-arm64"
        ? !archs.includes("arm64") || archs.includes("x86_64")
        : !archs.includes("x86_64") || archs.includes("arm64")
    )
      fail("application binary architectures do not match the package target");
    const home = join(root, "home"),
      cache = join(root, "cache"),
      config = join(root, "config"),
      data = join(root, "data"),
      state = join(root, "state"),
      temporary = join(root, "tmp");
    for (const path of [
      home,
      join(home, "Library", "Application Support"),
      join(home, "Library", "Caches"),
      join(home, "Library", "Preferences"),
      cache,
      config,
      data,
      state,
      temporary,
    ])
      await mkdir(path, { recursive: true, mode: 0o700 });
    const outputPath = join(root, "edited.pdf");
    await writeFile(outputPath, fixture, { flag: "wx", mode: 0o600 });
    const env = isolatedAppEnvironment({ home, root });
    const helperSource = join(
      dirname(fileURLToPath(import.meta.url)),
      "PackagedEditSmoke.swift",
    );
    const helper = join(root, "PackagedEditSmoke");
    execute(
      "/usr/bin/xcrun",
      [
        "swiftc",
        "-swift-version",
        "5",
        "-O",
        helperSource,
        "-framework",
        "AppKit",
        "-framework",
        "ApplicationServices",
        "-framework",
        "PDFKit",
        "-o",
        helper,
      ],
      logs,
      { timeout: 60_000 },
    );
    const driver = (command, ...args) => {
      const output = execute(helper, [command, ...args.map(String)], logs, {
        timeout: 20_000,
      });
      try {
        return JSON.parse(output);
      } catch {
        fail(`native edit driver returned invalid JSON for ${command}`);
      }
    };
    result.capabilityProbe = driver("probe");
    result.baselinePdf = driver("inspect-baseline", outputPath);
    if (result.baselinePdf.rectangleCount !== 0)
      fail("fixture must start without rectangle annotations");
    const launch = (path) => {
      const process = spawn(executable, [path], {
        cwd: root,
        env,
        stdio: ["ignore", "pipe", "pipe"],
        detached: true,
      });
      process.stdout.on("data", (chunk) => {
        logs.stdout = bounded(logs.stdout + chunk.toString());
      });
      process.stderr.on("data", (chunk) => {
        logs.stderr = bounded(logs.stderr + chunk.toString());
      });
      children.push(process);
      return process;
    };
    child = launch(outputPath);
    const observations = [],
      deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline && observations.length < 2) {
      if (child.exitCode !== null)
        fail(`packaged app exited early (${child.exitCode})`);
      workerPids = pidsForExecutable(worker, logs);
      if (workerPids.length)
        observations.push({
          atMs: timeoutMs - (deadline - Date.now()),
          appPid: child.pid,
          workerPids: [...workerPids],
        });
      await sleep(400);
    }
    if (observations.length < 2)
      fail("packaged PDF worker was not observed alive twice");
    const recoveryStoreRoot = stableRecoveryStoreRoot({
      platform: "darwin",
      effectiveHome: disposableRunner.effectiveHome,
    });
    while (Date.now() < deadline && !result.documentOpenEvidence) {
      if (child.exitCode !== null)
        fail(
          `packaged app exited before document-open evidence (${child.exitCode})`,
        );
      result.documentOpenEvidence = await tryReadProductionOpenEvidence({
        recoveryStoreRoot,
        fixturePath: outputPath,
        fixtureBytes: fixture,
        platform: "darwin",
      });
      if (!result.documentOpenEvidence) await sleep(400);
    }
    if (!result.documentOpenEvidence)
      fail(
        "exact opened-document recovery checkpoint was not observed before timeout",
      );
    result.observations = observations;
    result.appAlive = child.exitCode === null;
    if (!result.appAlive)
      fail(
        "packaged app did not remain alive through both worker observations",
      );
    result.edit = driver("edit", child.pid);
    const saveDeadline = Date.now() + 15_000;
    let outputBytes;
    while (Date.now() < saveDeadline) {
      try {
        outputBytes = await checkedFile(
          outputPath,
          MAX_FIXTURE,
          "edited output PDF",
        );
        const inspection = driver("inspect", outputPath);
        result.savedPdf = {
          bytes: outputBytes.length,
          sha256: hash(outputBytes),
          ...inspection,
        };
        break;
      } catch (error) {
        if (
          /expected exactly one saved rectangle annotation|not a readable one-page PDF/.test(
            error.message,
          )
        ) {
          await sleep(300);
          continue;
        }
        if (error?.code === "ENOENT") {
          await sleep(300);
          continue;
        }
        throw error;
      }
    }
    if (!result.savedPdf)
      fail(
        "normal Save did not produce a PDF containing exactly one rectangle annotation",
      );
    if (result.savedPdf.sha256 === result.fixture.sha256)
      fail("normal Save did not change the fixture PDF bytes");
    result.normalClose = driver("close", child.pid);
    const closeDeadline = Date.now() + 15_000;
    while (Date.now() < closeDeadline && child.exitCode === null)
      await sleep(150);
    if (child.exitCode === null)
      fail("packaged app did not exit after normal Cmd-Q");
    const exitedPid = child.pid;
    const workerExitDeadline = Date.now() + 8_000;
    do {
      workerPids = pidsForExecutable(worker, logs);
      if (workerPids.length) await sleep(200);
    } while (workerPids.length && Date.now() < workerExitDeadline);
    if (workerPids.length)
      fail(`PDF worker survived normal app exit: ${workerPids.join(", ")}`);
    result.normalExit = { pid: exitedPid, exitCode: child.exitCode };
    child = launch(outputPath);
    const reopenDeadline = Date.now() + timeoutMs;
    let reopened = false;
    while (Date.now() < reopenDeadline) {
      if (child.exitCode !== null)
        fail(`reopened packaged app exited early (${child.exitCode})`);
      try {
        result.reopened = driver("reopened", child.pid, outputPath);
        reopened = true;
        break;
      } catch (error) {
        if (
          /no accessible window|window title does not identify/.test(
            error.message,
          )
        ) {
          await sleep(400);
          continue;
        }
        throw error;
      }
    }
    if (!reopened)
      fail("edited PDF did not reopen in a fresh packaged app process");
    result.reopenedPdf = driver("inspect", outputPath);
    if (result.reopenedPdf.rectangleCount !== 1)
      fail("freshly reopened output lost its rectangle annotation");
  } catch (error) {
    failure = error;
    result.error = error.message;
  } finally {
    let safeToRemoveRoot = true;
    try {
      if (!children.length)
        result.cleanup = { status: "no-process-launched", workerPids: [] };
      else {
        const appPaths = [
          join(
            root,
            "extract",
            "Butter Paper.app",
            "Contents",
            "MacOS",
            "Butter Paper",
          ),
        ];
        const workerPath = join(
          root,
          "extract",
          "Butter Paper.app",
          "Contents",
          "MacOS",
          "butter-paper-pdf-worker",
        );
        const owned = () => {
          const pids = children
            .filter(
              (process) =>
                process.exitCode === null && isOwnedPath(process.pid, appPaths),
            )
            .map((process) => process.pid);
          pids.push(...pidsForExecutable(workerPath, logs));
          return [...new Set(pids)];
        };
        for (const process of children) {
          if (process.exitCode === null) {
            try {
              process.kill("SIGTERM");
            } catch (error) {
              if (error.code !== "ESRCH") throw error;
            }
          }
          try {
            process.kill(-process.pid, "SIGTERM");
          } catch (error) {
            if (error.code !== "ESRCH") throw error;
          }
        }
        const until = Date.now() + 8000;
        while (
          Date.now() < until &&
          (children.some((process) => process.exitCode === null) ||
            pidsForExecutable(workerPath, logs).length)
        )
          await sleep(200);
        for (const process of children)
          if (process.exitCode === null) {
            try {
              process.kill(-process.pid, "SIGKILL");
            } catch (error) {
              if (error.code !== "ESRCH") throw error;
            }
          }
        const finalWorkers = pidsForExecutable(workerPath, logs);
        for (const pid of finalWorkers) {
          try {
            process.kill(pid, "SIGKILL");
          } catch (error) {
            if (error.code !== "ESRCH") throw error;
          }
        }
        await Promise.all(
          children.map((process) =>
            process.exitCode !== null
              ? Promise.resolve()
              : Promise.race([
                  new Promise((resolveExit) =>
                    process.once("exit", resolveExit),
                  ),
                  sleep(2000),
                ]),
          ),
        );
        await sleep(300);
        const remaining = owned();
        if (
          remaining.length ||
          children.some((process) => process.exitCode === null)
        )
          fail(
            `owned processes remain after termination: ${remaining.join(", ")}`,
          );
        result.cleanup = {
          status: "verified-clean",
          appPids: children.map((process) => process.pid),
          workerPids: [...new Set([...workerPids, ...finalWorkers])],
          remaining,
        };
      }
    } catch (error) {
      safeToRemoveRoot = false;
      result.cleanup = {
        status: "unknown-or-failed",
        error: error.message,
        appPids: children.map((process) => process.pid),
        workerPids,
      };
      result.error = `${result.error ? `${result.error}; ` : ""}cleanup failed: ${error.message}`;
    }
    if (root && safeToRemoveRoot) {
      try {
        await rm(root, { recursive: true, force: true });
        result.cleanup.tempRootRemoved = true;
      } catch (error) {
        result.cleanup.tempRootRemoved = false;
        result.error = `${result.error ? `${result.error}; ` : ""}temporary-root cleanup failed: ${error.message}`;
      }
    } else if (root) {
      result.cleanup.tempRootRemoved = false;
      result.cleanup.retainedTempRoot = root;
    }
    result.finishedAt = new Date().toISOString();
    result.logs = {
      stdout: bounded(logs.stdout),
      stderr: bounded(logs.stderr),
      truncated: logs.stdout.length >= MAX_LOG || logs.stderr.length >= MAX_LOG,
    };
    result.passed =
      !result.error &&
      result.capabilityProbe?.accessibilityTrusted === true &&
      result.baselinePdf?.rectangleCount === 0 &&
      result.savedPdf?.rectangleCount === 1 &&
      result.savedPdf?.sha256 !== result.fixture?.sha256 &&
      result.reopenedPdf?.rectangleCount === 1 &&
      result.normalExit?.exitCode === 0 &&
      result.cleanup.status === "verified-clean" &&
      result.cleanup.tempRootRemoved;
    try {
      await writeFile(evidencePath, `${JSON.stringify(result, null, 2)}\n`, {
        flag: "wx",
        mode: 0o600,
      });
      result.evidence = evidencePath;
    } catch (error) {
      result.passed = false;
      result.error = `${result.error ? `${result.error}; ` : ""}could not write evidence: ${error.message}`;
    }
  }
  if (failure && !result.error) result.error = failure.message;
  if (!result.passed) {
    const error = new Error(
      `${result.error ?? "runtime smoke failed"} (evidence: ${evidencePath})`,
    );
    error.evidence = result;
    throw error;
  }
  return result;
}

function parseArgs(args) {
  if (args.length === 1 && ["-h", "--help"].includes(args[0])) {
    process.stdout.write(
      "Usage: node smoke-signed-macos-package.mjs --archive ZIP --fixture-pdf PDF --evidence-root EXISTING_DIR [--timeout-ms N]\n",
    );
    return null;
  }
  const values = new Map();
  for (let i = 0; i < args.length; i += 2) {
    if (
      ![
        "--archive",
        "--fixture-pdf",
        "--evidence-root",
        "--timeout-ms",
      ].includes(args[i]) ||
      !args[i + 1] ||
      values.has(args[i])
    )
      fail(
        "usage: smoke-signed-macos-package.mjs --archive ZIP --fixture-pdf PDF --evidence-root EXISTING_DIR [--timeout-ms N]",
      );
    values.set(args[i], args[i + 1]);
  }
  for (const key of ["--archive", "--fixture-pdf", "--evidence-root"])
    if (!values.has(key)) fail(`${key} is required`);
  return {
    archivePath: resolve(values.get("--archive")),
    fixturePdfPath: resolve(values.get("--fixture-pdf")),
    evidenceRoot: resolve(values.get("--evidence-root")),
    timeoutMs: Number(values.get("--timeout-ms") ?? 60_000),
  };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    const options = parseArgs(process.argv.slice(2));
    if (options)
      smokeSignedMacosPackage(options)
        .then((result) =>
          process.stdout.write(`${JSON.stringify(result, null, 2)}\n`),
        )
        .catch((error) => {
          if (error.evidence)
            process.stdout.write(
              `${JSON.stringify(error.evidence, null, 2)}\n`,
            );
          process.stderr.write(`${error.message}\n`);
          process.exitCode = 1;
        });
  } catch (error) {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  }
}
