#!/usr/bin/env node

// Bounded launch smoke for the verified Windows/Linux production archives.
// This proves startup, packaged worker activation, an exact production open,
// a real Rectangle pointer edit, normal Save, full close, fresh reopen and
// independent saved-PDF syntax validation. It does not qualify visual output.
import { createHash } from "node:crypto";
import { spawn, spawnSync } from "node:child_process";
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
import { readFileSync, readdirSync, readlinkSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  stableRecoveryStoreRoot,
  tryReadProductionOpenEvidence,
} from "./production-open-evidence.mjs";

const ROOT = dirname(fileURLToPath(import.meta.url));
const LIMIT_MS = 180_000;
const MIN_ALIVE_MS = 3_000;
const POLL_MS = 400;
const MAX_LOG_BYTES = 512 * 1024;
const MAX_ARCHIVE_BYTES = 256 * 1024 * 1024;
const CRC_TABLE = Uint32Array.from({ length: 256 }, (_, value) => {
  let crc = value;
  for (let bit = 0; bit < 8; bit += 1)
    crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0);
  return crc >>> 0;
});
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");
const fail = (message) => {
  throw new Error(message);
};

function assert(condition, message) {
  if (!condition) fail(message);
}
function sorted(values) {
  return [...values].sort();
}
function equalInventory(actual, expected, label) {
  assert(
    JSON.stringify(sorted(actual)) === JSON.stringify(sorted(expected)),
    `${label} inventory is mixed, partial, or unexpected`,
  );
}
function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) crc = (crc >>> 8) ^ CRC_TABLE[(crc ^ byte) & 0xff];
  return (crc ^ 0xffffffff) >>> 0;
}

export function parseProductionZip(bytes) {
  const files = new Map();
  const localRecords = new Map();
  let offset = 0;
  while (
    offset + 4 <= bytes.length &&
    bytes.readUInt32LE(offset) === 0x04034b50
  ) {
    assert(offset + 30 <= bytes.length, "ZIP local header is truncated");
    const flags = bytes.readUInt16LE(offset + 6),
      method = bytes.readUInt16LE(offset + 8);
    const crc = bytes.readUInt32LE(offset + 14),
      compressed = bytes.readUInt32LE(offset + 18),
      size = bytes.readUInt32LE(offset + 22);
    const nameLength = bytes.readUInt16LE(offset + 26),
      extraLength = bytes.readUInt16LE(offset + 28);
    assert(
      flags === 0x0800 && method === 0 && compressed === size,
      "ZIP is not in the signed production package format",
    );
    const nameStart = offset + 30,
      dataStart = nameStart + nameLength + extraLength,
      end = dataStart + size;
    assert(end <= bytes.length, "ZIP entry is truncated");
    const name = bytes.toString("utf8", nameStart, nameStart + nameLength);
    assert(
      name &&
        !/[\\/]/.test(name) &&
        name !== "." &&
        name !== ".." &&
        !files.has(name),
      "ZIP contains an unsafe or duplicate path",
    );
    const content = bytes.subarray(dataStart, end);
    assert(crc32(content) === crc, `ZIP CRC mismatch for ${name}`);
    files.set(name, content);
    localRecords.set(name, { crc, size, offset });
    offset = end;
  }
  assert(
    files.size &&
      offset + 4 <= bytes.length &&
      bytes.readUInt32LE(offset) === 0x02014b50,
    "ZIP has no valid central directory",
  );
  const central = [];
  while (
    offset + 4 <= bytes.length &&
    bytes.readUInt32LE(offset) === 0x02014b50
  ) {
    assert(
      offset + 46 <= bytes.length,
      "ZIP central directory entry is truncated",
    );
    const nameLength = bytes.readUInt16LE(offset + 28),
      extraLength = bytes.readUInt16LE(offset + 30),
      commentLength = bytes.readUInt16LE(offset + 32);
    const end = offset + 46 + nameLength + extraLength + commentLength;
    assert(
      end <= bytes.length &&
        bytes.readUInt16LE(offset + 4) === 0x0314 &&
        bytes.readUInt16LE(offset + 8) === 0x0800 &&
        bytes.readUInt16LE(offset + 10) === 0 &&
        bytes.readUInt32LE(offset + 38) === 0,
      "ZIP central directory has unsupported metadata or file type",
    );
    const name = bytes.toString("utf8", offset + 46, offset + 46 + nameLength);
    assert(
      name && !central.includes(name),
      "ZIP central directory has duplicate or empty paths",
    );
    const local = localRecords.get(name);
    assert(
      local &&
        bytes.readUInt32LE(offset + 16) === local.crc &&
        bytes.readUInt32LE(offset + 20) === local.size &&
        bytes.readUInt32LE(offset + 24) === local.size &&
        bytes.readUInt32LE(offset + 42) === local.offset,
      `ZIP central directory metadata does not match ${name}`,
    );
    central.push(name);
    offset = end;
  }
  assert(
    offset + 22 === bytes.length && bytes.readUInt32LE(offset) === 0x06054b50,
    "ZIP has trailing or invalid end record",
  );
  assert(
    bytes.readUInt16LE(offset + 8) === central.length &&
      bytes.readUInt16LE(offset + 10) === central.length,
    "ZIP central directory count is invalid",
  );
  equalInventory(central, [...files.keys()], "ZIP central directory");
  return files;
}

export function parseProductionTar(bytes, rootName) {
  const files = new Map();
  let offset = 0,
    ended = false,
    rootSeen = false;
  while (offset + 512 <= bytes.length) {
    const header = bytes.subarray(offset, offset + 512);
    if (header.every((byte) => byte === 0)) {
      assert(
        offset + 1024 === bytes.length &&
          bytes.subarray(offset + 512).every((byte) => byte === 0),
        "tar must end in exactly two zero blocks",
      );
      ended = true;
      break;
    }
    const stored = parseInt(
      header.toString("ascii", 148, 156).replace(/\0.*$/, "").trim(),
      8,
    );
    let checksum = 0;
    for (let i = 0; i < 512; i += 1)
      checksum += i >= 148 && i < 156 ? 0x20 : header[i];
    assert(
      Number.isInteger(stored) && checksum === stored,
      "tar header checksum is invalid",
    );
    const field = (start, end) => {
      const b = header.subarray(start, end);
      const nul = b.indexOf(0);
      return b.subarray(0, nul < 0 ? b.length : nul).toString("utf8");
    };
    const name = field(0, 100),
      prefix = field(345, 500),
      path = prefix ? `${prefix}/${name}` : name;
    assert(
      header.toString("ascii", 257, 263) === "ustar\0" &&
        header.toString("ascii", 263, 265) === "00",
      "tar entry is not supported ustar",
    );
    const type = String.fromCharCode(header[156]);
    const mode = parseInt(field(100, 108), 8),
      size = parseInt(field(124, 136), 8);
    assert(
      (type === "0" || type === "5") &&
        Number.isSafeInteger(mode) &&
        Number.isSafeInteger(size) &&
        size >= 0,
      `tar has unsafe entry type or size: ${path}`,
    );
    assert(
      header.subarray(108, 124).every((b) => b === 48 || b === 0) &&
        header.subarray(136, 148).every((b) => b === 48 || b === 0),
      `tar contains ownership or timestamp metadata: ${path}`,
    );
    if (type === "5") {
      assert(
        path === `${rootName}/` &&
          size === 0 &&
          mode === 0o755 &&
          files.size === 0 &&
          !rootSeen,
        "tar contains an unexpected directory entry",
      );
      rootSeen = true;
      offset += 512;
      continue;
    }
    assert(
      rootSeen && path.startsWith(`${rootName}/`),
      `tar entry escapes package root or precedes root: ${path}`,
    );
    const entryName = path.slice(rootName.length + 1);
    assert(
      entryName &&
        !entryName.includes("/") &&
        entryName !== "." &&
        entryName !== ".." &&
        !files.has(entryName),
      `tar contains an unsafe or duplicate path: ${path}`,
    );
    const start = offset + 512,
      end = start + size,
      paddedEnd = start + Math.ceil(size / 512) * 512;
    assert(
      paddedEnd <= bytes.length &&
        bytes.subarray(end, paddedEnd).every((b) => b === 0),
      `tar entry is truncated or has invalid padding: ${path}`,
    );
    const content = Buffer.from(bytes.subarray(start, end));
    files.set(entryName, { bytes: content, mode });
    offset = paddedEnd;
  }
  assert(ended && files.size > 0, "tar archive is truncated or empty");
  return files;
}

export function assertSidecars(
  archivePath,
  packageManifest,
  verificationReceipt,
  target,
) {
  const bytes = packageManifest.artifact?.bytes;
  const digest = packageManifest.artifact?.sha256;
  assert(
    packageManifest.schema === "butter-paper/package-manifest" &&
      packageManifest.schemaVersion === 1 &&
      packageManifest.target === target &&
      packageManifest.channel === "stable",
    "package manifest identity is invalid",
  );
  assert(
    verificationReceipt.schema === "butter-paper/package-verification" &&
      verificationReceipt.schemaVersion === 1 &&
      verificationReceipt.target === target &&
      verificationReceipt.channel === "stable" &&
      verificationReceipt.verified === true,
    "verification receipt is invalid",
  );
  assert(
    Number.isSafeInteger(bytes) && /^[0-9a-f]{64}$/.test(digest ?? ""),
    "package manifest artifact claim is invalid",
  );
  assert(
    verificationReceipt.artifact?.bytes === bytes &&
      verificationReceipt.artifact?.sha256 === digest &&
      verificationReceipt.artifact?.path === packageManifest.artifact?.path,
    "verification receipt artifact claim does not match package manifest",
  );
  return { archiveBytes: bytes, archiveSha256: digest };
}

async function regularFile(path, label) {
  const stat = await lstat(path);
  assert(
    stat.isFile() && !stat.isSymbolicLink() && stat.nlink === 1,
    `${label} must be a regular single-link file`,
  );
}

async function readArchive(path, label) {
  await regularFile(path, label);
  const stat = await lstat(path);
  assert(
    stat.size > 0 && stat.size <= MAX_ARCHIVE_BYTES,
    `${label} exceeds the ${MAX_ARCHIVE_BYTES}-byte smoke limit`,
  );
  return readFile(path);
}

async function readJson(path, label) {
  const stat = await lstat(path);
  assert(
    stat.isFile() &&
      !stat.isSymbolicLink() &&
      stat.nlink === 1 &&
      stat.size <= 1024 * 1024,
    `${label} must be a regular single-link file no larger than 1 MiB`,
  );
  try {
    return JSON.parse(await readFile(path, "utf8"));
  } catch (error) {
    fail(`${label} is unreadable or invalid JSON: ${error.message}`);
  }
}

function sidecarPaths(archivePath) {
  const name = basename(archivePath).replace(/(?:\.tar\.xz|\.zip)$/i, "");
  return [
    join(dirname(archivePath), `${name}.package.json`),
    join(dirname(archivePath), `${name}.verification.json`),
  ];
}

async function validateWindows(archivePath, architecture, runDir) {
  assert(
    process.platform === "win32",
    "Windows packages can only be smoke-launched on Windows",
  );
  const target = architecture === "arm64" ? "windows-arm64" : "windows-x64";
  const [manifestPath, receiptPath] = sidecarPaths(archivePath);
  const [manifest, receipt] = await Promise.all([
    readJson(manifestPath, "package manifest"),
    readJson(receiptPath, "verification receipt"),
  ]);
  const archive = await readArchive(archivePath, "package ZIP");
  await Promise.all([
    regularFile(manifestPath, "package manifest"),
    regularFile(receiptPath, "verification receipt"),
  ]);
  const claim = assertSidecars(archivePath, manifest, receipt, target);
  assert(
    archive.length === claim.archiveBytes &&
      sha256(archive) === claim.archiveSha256,
    "ZIP does not match the verified archive receipt",
  );
  const files = parseProductionZip(archive);
  const embedded = JSON.parse(
    files.get("MANIFEST.json")?.toString("utf8") ?? "null",
  );
  const arch = architecture;
  assert(
    embedded?.schemaVersion === 1 &&
      embedded.product === "Butter Paper" &&
      embedded.target ===
        (arch === "arm64"
          ? "aarch64-pc-windows-msvc"
          : "x86_64-pc-windows-msvc"),
    "embedded Windows package identity is invalid",
  );
  assert(
    manifest.version === embedded.version &&
      manifest.sourceRevision === embedded.sourceRevision,
    "Windows sidecar identity does not match embedded manifest",
  );
  assert(
    receipt.version === embedded.version &&
      receipt.sourceRevision === embedded.sourceRevision,
    "Windows verification receipt identity is inconsistent",
  );
  assert(
    manifest.package?.target === embedded.target &&
      manifest.package.pdfiumReceiptSha256 === embedded.pdfiumReceiptSha256,
    "Windows package sidecar contract is inconsistent",
  );
  const signatureNames = [
    "gpui-migration.exe",
    "butter-paper-pdf-worker.exe",
    "butter-paper-signature-phone.exe",
    "pdfium.dll",
  ];
  const expected = [
    ...signatureNames,
    "README.md",
    "THIRD_PARTY_NOTICES.md",
    "PHONE_HELPER_THIRD_PARTY_NOTICES.md",
    "QRCP_LICENSE",
    "SIGNATURE_PAD_LICENSE",
    "butter-paper.ico",
    "install.ps1",
    "uninstall.ps1",
    `production-pdfium-windows-${arch}.json`,
    "MANIFEST.json",
  ];
  equalInventory([...files.keys()], expected, "Windows package");
  assert(
    JSON.stringify(Object.keys(embedded.files ?? {}).sort()) ===
      JSON.stringify(
        expected.filter((name) => name !== "MANIFEST.json").sort(),
      ),
    "embedded Windows manifest inventory is invalid",
  );
  for (const [name, content] of Object.entries(embedded.files))
    assert(
      content.bytes === files.get(name)?.length &&
        content.sha256 === sha256(files.get(name)),
      `embedded manifest hash mismatch: ${name}`,
    );
  assert(
    embedded.pdfiumReceiptSha256 ===
      sha256(files.get(`production-pdfium-windows-${arch}.json`)),
    "Windows PDFium receipt hash is invalid",
  );
  assert(Array.isArray(manifest.signatures) && Array.isArray(receipt.signatures), "Windows signature arrays are missing from verified sidecars");
  const unsigned = manifest.signaturePolicy === "unsigned-user-authorised" && receipt.signaturePolicy === "unsigned-user-authorised";
  if (unsigned) {
    assert(receipt.integrityVerified === true && manifest.signatures.length === 0 && receipt.signatures.length === 0 && !receipt.signerThumbprint, "unsigned Windows verification receipt is inconsistent");
    for (const name of signatureNames) await writeFile(join(runDir, name), files.get(name), { flag: "wx" });
  } else {
    assert(/^[0-9A-F]{40}$/i.test(receipt.signerThumbprint ?? ""), "signed Windows verification receipt has no signer thumbprint");
    for (const name of signatureNames) {
      const signed = manifest.signatures.find((entry) => entry.path === name);
      const verified = receipt.signatures.find((entry) => entry.path === name);
      assert(
        signed?.status === "Valid" && signed.timestamped === true &&
          signed.signerThumbprint?.toUpperCase() === receipt.signerThumbprint.toUpperCase() &&
          signed.sha256 === sha256(files.get(name)) && verified?.status === "Valid" &&
          verified.timestamped === true && verified.sha256 === signed.sha256 &&
          verified.signerThumbprint?.toUpperCase() === signed.signerThumbprint.toUpperCase(),
        `verified Authenticode receipt is missing or inconsistent for ${name}`,
      );
      await writeFile(join(runDir, name), files.get(name), { flag: "wx" });
      const sigResult = spawnSync(
        "powershell.exe",
        ["-NoProfile", "-NonInteractive", "-Command", `$s=Get-AuthenticodeSignature -LiteralPath '${join(runDir, name).replaceAll("'", "''")}'; $ts=$null; if ($s.TimeStamperCertificate) {$ts=$s.TimeStamperCertificate.Thumbprint}; [pscustomobject]@{Status=[string]$s.Status;SignerThumbprint=[string]$s.SignerCertificate.Thumbprint;TimestampThumbprint=$ts} | ConvertTo-Json -Compress`],
        { encoding: "utf8", windowsHide: true, timeout: 15_000, maxBuffer: 1024 * 1024 },
      );
      assert(!sigResult.error && sigResult.status === 0, `could not revalidate Authenticode for ${name}`);
      const live = JSON.parse(sigResult.stdout.trim());
      assert(
        live.Status === "Valid" && live.SignerThumbprint?.replaceAll(" ", "").toUpperCase() === receipt.signerThumbprint.replaceAll(" ", "").toUpperCase() &&
          live.TimestampThumbprint?.replaceAll(" ", "").toUpperCase() === signed.timestampThumbprint?.replaceAll(" ", "").toUpperCase(),
        `Authenticode verification failed for ${name}`,
      );
    }
  }
  for (const [name, content] of files)
    if (!signatureNames.includes(name) && name !== "MANIFEST.json")
      await writeFile(join(runDir, name), content, { flag: "wx" });
  return {
    packageRoot: runDir,
    executable: join(runDir, "gpui-migration.exe"),
    worker: join(runDir, "butter-paper-pdf-worker.exe"),
    version: embedded.version,
    revision: embedded.sourceRevision,
    target,
    archiveSha256: claim.archiveSha256,
    signerThumbprint: receipt.signerThumbprint ?? null,
    signaturePolicy: unsigned ? "unsigned-user-authorised" : "authenticode",
  };
}

async function validateLinux(archivePath, architecture, runDir) {
  assert(
    process.platform === "linux",
    "Linux packages can only be smoke-launched on Linux",
  );
  const target = architecture === "arm64" ? "linux-arm64" : "linux-x64";
  const [manifestPath, receiptPath] = sidecarPaths(archivePath);
  const [manifest, receipt] = await Promise.all([
    readJson(manifestPath, "package manifest"),
    readJson(receiptPath, "verification receipt"),
  ]);
  await Promise.all([
    regularFile(archivePath, "package tar.xz"),
    regularFile(manifestPath, "package manifest"),
    regularFile(receiptPath, "verification receipt"),
  ]);
  const archive = await readArchive(archivePath, "package tar.xz"),
    claim = assertSidecars(archivePath, manifest, receipt, target);
  assert(
    archive.length === claim.archiveBytes &&
      sha256(archive) === claim.archiveSha256,
    "tar.xz does not match the verified archive receipt",
  );
  const decompress = spawnSync(
    "xz",
    ["--decompress", "--stdout", "--check=crc64"],
    { input: archive, maxBuffer: 256 * 1024 * 1024, timeout: 30_000 },
  );
  assert(
    !decompress.error && decompress.status === 0,
    "cannot independently decompress Linux package with xz",
  );
  const rootName = `butter-paper-linux-${architecture}-${manifest.version}`;
  const files = parseProductionTar(decompress.stdout, rootName);
  const embedded = JSON.parse(
    files.get("MANIFEST.json")?.bytes.toString("utf8") ?? "null",
  );
  assert(
    embedded?.schemaVersion === 1 &&
      embedded.product === "Butter Paper" &&
      embedded.target ===
        (architecture === "arm64"
          ? "aarch64-unknown-linux-gnu"
          : "x86_64-unknown-linux-gnu") &&
      embedded.version === manifest.version &&
      embedded.sourceRevision === manifest.sourceRevision,
    "embedded Linux package identity is invalid",
  );
  assert(
    receipt.version === embedded.version &&
      receipt.sourceRevision === embedded.sourceRevision,
    "Linux verification receipt identity does not match package",
  );
  const names = [...files.keys()];
  const expected = [
    "gpui-migration",
    "butter-paper-pdf-worker",
    "butter-paper-signature-phone",
    "libpdfium.so",
    "README.md",
    "THIRD_PARTY_NOTICES.md",
    "PHONE_HELPER_THIRD_PARTY_NOTICES.md",
    "QRCP_LICENSE",
    "SIGNATURE_PAD_LICENSE",
    "butter-paper.png",
    "butter-paper.desktop",
    "install-user.sh",
    "uninstall-user.sh",
    `production-pdfium-linux-${architecture}.json`,
    "MANIFEST.json",
  ];
  equalInventory(names, expected, "Linux package");
  assert(
    JSON.stringify(Object.keys(embedded.files ?? {}).sort()) ===
      JSON.stringify(
        expected.filter((name) => name !== "MANIFEST.json").sort(),
      ),
    "embedded Linux manifest inventory is invalid",
  );
  for (const [name, content] of Object.entries(embedded.files)) {
    const entry = files.get(name);
    assert(
      entry &&
        content.bytes === entry.bytes.length &&
        content.sha256 === sha256(entry.bytes),
      `embedded manifest hash mismatch: ${name}`,
    );
    const executable =
      [
        "gpui-migration",
        "butter-paper-pdf-worker",
        "butter-paper-signature-phone",
      ].includes(name) || name.endsWith(".sh");
    assert(
      entry.mode === (executable ? 0o755 : 0o644),
      `unexpected file mode: ${name}`,
    );
    await writeFile(join(runDir, name), entry.bytes, {
      flag: "wx",
      mode: entry.mode,
    });
  }
  assert(
    embedded.pdfiumReceiptSha256 ===
      sha256(files.get(`production-pdfium-linux-${architecture}.json`).bytes),
    "Linux PDFium receipt hash is invalid",
  );
  // Reuse the full production verifier against this exact archive and compare
  // its independently derived receipt with the adjacent workflow sidecars.
  const recheckDir = join(runDir, ".recheck");
  await mkdir(recheckDir, { mode: 0o700 });
  const generatedManifest = join(recheckDir, "package.json"),
    generatedReceipt = join(recheckDir, "verification.json");
  const verifier = join(ROOT, "verify-linux-production-package.mjs");
  const verified = spawnSync(
    process.execPath,
    [
      verifier,
      "--input",
      archivePath,
      "--package-manifest",
      generatedManifest,
      "--verification-receipt",
      generatedReceipt,
      "--architecture",
      architecture,
      "--version",
      manifest.version,
      "--revision",
      manifest.sourceRevision,
      "--artifact-path",
      manifest.artifact.path,
    ],
    { encoding: "utf8", timeout: 30_000, maxBuffer: 2 * 1024 * 1024 },
  );
  assert(
    !verified.error && verified.status === 0,
    `existing Linux production verifier rejected package: ${verified.stderr || verified.error?.message || verified.status}`,
  );
  const [computedManifest, computedReceipt] = await Promise.all([
    readJson(generatedManifest, "recomputed Linux package manifest"),
    readJson(generatedReceipt, "recomputed Linux verification receipt"),
  ]);
  assert(
    JSON.stringify(computedManifest) === JSON.stringify(manifest) &&
      JSON.stringify(computedReceipt) === JSON.stringify(receipt),
    "sidecars differ from the production verifier's result",
  );
  return {
    packageRoot: runDir,
    executable: join(runDir, "gpui-migration"),
    worker: join(runDir, "butter-paper-pdf-worker"),
    version: embedded.version,
    revision: embedded.sourceRevision,
    target,
    archiveSha256: claim.archiveSha256,
  };
}

function sleep(ms) {
  return new Promise((resolvePromise) => setTimeout(resolvePromise, ms));
}
function linuxProcesses() {
  const result = new Map();
  for (const pid of readdirSync("/proc")) {
    if (!/^\d+$/.test(pid)) continue;
    try {
      const stat = readFileSync(`/proc/${pid}/stat`, "utf8");
      const fields = stat.slice(stat.lastIndexOf(")") + 2).split(" ");
      const exe = readlinkSync(`/proc/${pid}/exe`);
      result.set(Number(pid), {
        pid: Number(pid),
        ppid: Number(fields[1]),
        exe,
      });
    } catch {}
  }
  return result;
}

function descendants(rootPid, processes) {
  const found = new Set([Number(rootPid)]);
  let changed = true;
  while (changed) {
    changed = false;
    for (const proc of processes.values())
      if (found.has(proc.ppid) && !found.has(proc.pid)) {
        found.add(proc.pid);
        changed = true;
      }
  }
  return [...found].map((pid) => processes.get(pid)).filter(Boolean);
}

// PowerShell output is captured through files rather than pipes: Add-Type
// starts csc.exe, which inherits and holds pipes after a timeout kill, and
// spawnSync then blocks forever waiting for them to close.
let powershellSequence = 0;
function powershell(script, timeout = 45_000) {
  const stem = join(tmpdir(), `bp-smoke-ps-${process.pid}-${(powershellSequence += 1)}`);
  const outPath = `${stem}.out.txt`,
    errPath = `${stem}.err.txt`;
  const quote = (value) => `'${value.replaceAll("'", "''")}'`;
  const wrapped = `$ErrorActionPreference='Stop'; try { $bpOut = & {\n${script}\n} | Out-String; [IO.File]::WriteAllText(${quote(outPath)}, [string]$bpOut) } catch { [IO.File]::WriteAllText(${quote(errPath)}, ($_ | Out-String)); exit 1 }`;
  const result = spawnSync(
    "powershell.exe",
    ["-NoProfile", "-NonInteractive", "-Command", wrapped],
    { windowsHide: true, timeout, stdio: "ignore" },
  );
  const read = (path) => {
    try {
      return readFileSync(path, "utf8");
    } catch {
      return "";
    } finally {
      try {
        rmSync(path, { force: true });
      } catch {}
    }
  };
  const output = read(outPath),
    errors = read(errPath);
  if (result.error || result.status !== 0)
    fail(
      `PowerShell process inspection failed: ${errors.trim() || result.error?.message || result.status}`,
    );
  return output.trim();
}

function currentProcesses(rootPid, packageRoot) {
  const all =
    process.platform === "win32"
      ? (() => {
          const data = powershell(
            `$p=Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,ExecutablePath; ConvertTo-Json -InputObject @($p) -Compress`,
          );
          try {
            return JSON.parse(data).map((p) => ({
              pid: Number(p.ProcessId),
              ppid: Number(p.ParentProcessId),
              exe: p.ExecutablePath ?? "",
            }));
          } catch {
            fail("Windows process inspection returned invalid JSON");
          }
        })()
      : [...linuxProcesses().values()];
  const tracked = descendants(
    rootPid,
    new Map(all.map((proc) => [proc.pid, proc])),
  );
  const prefix =
    process.platform === "win32"
      ? packageRoot.toLowerCase().replaceAll("/", "\\") + "\\"
      : `${packageRoot}/`;
  const normalise = (value) =>
    process.platform === "win32"
      ? value.toLowerCase().replaceAll("/", "\\")
      : value.replace(/ \(deleted\)$/, "");
  for (const proc of all)
    if (
      normalise(proc.exe).startsWith(prefix) &&
      !tracked.some(({ pid }) => pid === proc.pid)
    )
      tracked.push(proc);
  return tracked;
}

const progressStartedAt = Date.now();
function progress(message) {
  process.stderr.write(`[smoke +${((Date.now() - progressStartedAt) / 1000).toFixed(1)}s] ${message}\n`);
}

async function waitUntil(predicate, timeoutMs, label) {
  progress(`waiting for ${label}`);
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const value = await predicate();
    if (value) {
      progress(`observed ${label}`);
      return value;
    }
    await sleep(POLL_MS);
  }
  fail(`timed out waiting for ${label}`);
}

function hasExecutable(processes, expectedPath) {
  const normalise = (value) =>
    process.platform === "win32"
      ? value.toLowerCase().replaceAll("/", "\\")
      : value.replace(/ \(deleted\)$/, "");
  const expected = normalise(expectedPath);
  return processes.filter((proc) => normalise(proc.exe) === expected);
}

async function checkLinuxDisplay() {
  assert(
    process.env.DISPLAY,
    "Linux smoke requires an existing X11 display or Xvfb display in DISPLAY",
  );
  const probe = spawnSync("xdpyinfo", ["-display", process.env.DISPLAY], {
    encoding: "utf8",
    timeout: 5000,
    stdio: "ignore",
  });
  assert(
    !probe.error && probe.status === 0,
    "Linux DISPLAY is not reachable (xdpyinfo probe failed)",
  );
  for (const command of ["xdotool", "xz", "qpdf"]) {
    const found = spawnSync("sh", ["-lc", `command -v ${command}`], {
      encoding: "utf8",
    });
    assert(found.status === 0, `Linux smoke requires ${command} on PATH`);
  }
  return {
    display: process.env.DISPLAY,
    probe: "xdpyinfo",
    windowClose: "xdotool",
    pdfValidation: "qpdf --check",
  };
}

async function askGracefulClose(pid) {
  if (process.platform === "win32") {
    const script = `Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public class BpClose { [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr extra); [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint p); [DllImport("user32.dll")] public static extern IntPtr SendMessageTimeout(IntPtr h,uint m,IntPtr w,IntPtr l,uint f,uint t,out IntPtr r); public delegate bool EnumProc(IntPtr h, IntPtr e); }'; $target=${pid}; $cb=[BpClose+EnumProc]{ param($h,$e); [uint32]$owner=0; [void][BpClose]::GetWindowThreadProcessId($h,[ref]$owner); if($owner -eq $target){[IntPtr]$r=[IntPtr]::Zero; [void][BpClose]::SendMessageTimeout($h,0x10,[IntPtr]::Zero,[IntPtr]::Zero,2,2000,[ref]$r)}; return $true }; [void][BpClose]::EnumWindows($cb,[IntPtr]::Zero)`;
    try {
      powershell(script);
      return "WM_CLOSE";
    } catch {
      return "close-request-unavailable";
    }
  }
  const search = spawnSync(
    "xdotool",
    ["search", "--onlyvisible", "--pid", String(pid)],
    { encoding: "utf8", timeout: 5000 },
  );
  if (search.status !== 0 || !search.stdout.trim())
    return "window-close-unavailable";
  let requested = false;
  for (const id of search.stdout.trim().split(/\s+/)) {
    const close = spawnSync("xdotool", ["windowclose", id], {
      encoding: "utf8",
      timeout: 5000,
    });
    if (close.status === 0) requested = true;
  }
  return requested ? "xdotool-windowclose" : "window-close-unavailable";
}

async function terminateOwned(pid, packageRoot) {
  let processes = currentProcesses(pid, packageRoot);
  if (!processes.length) return { remaining: [] };
  if (process.platform === "win32") {
    spawnSync("taskkill.exe", ["/PID", String(pid), "/T", "/F"], {
      encoding: "utf8",
      windowsHide: true,
      timeout: 12_000,
    });
    for (const proc of processes)
      if (proc.pid !== pid)
        spawnSync("taskkill.exe", ["/PID", String(proc.pid), "/F"], {
          encoding: "utf8",
          windowsHide: true,
          timeout: 5000,
        });
  } else {
    try {
      process.kill(-pid, "SIGTERM");
    } catch {}
    for (const proc of [...processes].reverse())
      try {
        process.kill(proc.pid, "SIGTERM");
      } catch {}
    const until = Date.now() + 4000;
    while (Date.now() < until && currentProcesses(pid, packageRoot).length)
      await sleep(100);
    processes = currentProcesses(pid, packageRoot);
    try {
      process.kill(-pid, "SIGKILL");
    } catch {}
    for (const proc of [...processes].reverse())
      try {
        process.kill(proc.pid, "SIGKILL");
      } catch {}
  }
  const deadline = Date.now() + 10_000;
  while (Date.now() < deadline) {
    const remaining = currentProcesses(pid, packageRoot);
    if (!remaining.length) return { remaining: [] };
    await sleep(200);
  }
  return {
    remaining: currentProcesses(pid, packageRoot).map(
      ({ pid: processId, ppid, exe }) => ({
        pid: processId,
        ppid,
        executable: exe,
      }),
    ),
  };
}

function cleanEnvironment(home, local) {
  const env = { ...process.env };
  for (const name of Object.keys(env))
    if (
      /^(?:BP_(?:UPDATE_.*|GPUI_DATA_DIR|.*PDFIUM.*|NATIVE_DEVELOPMENT)|PDFIUM_.*|DEV_PDFIUM.*)$/i.test(
        name,
      )
    )
      delete env[name];
  if (process.platform === "win32") {
    env.APPDATA = join(home, "AppData", "Roaming");
    env.LOCALAPPDATA = local;
    env.USERPROFILE = home;
    env.TEMP = join(local, "Temp");
    env.TMP = env.TEMP;
  } else {
    env.HOME = home;
    env.XDG_DATA_HOME = join(home, ".local", "share");
    env.XDG_CONFIG_HOME = join(home, ".config");
    env.XDG_CACHE_HOME = join(home, ".cache");
  }
  return env;
}

// Read the production recovery timeline as independent evidence that the
// ordinary keyboard and pointer gesture created an actual Rectangle. The app
// remains the only writer; this smoke only reads its content-addressed state.
export async function readRectangleEditEvidence(
  recoveryStoreRoot,
  platform,
  { requireNewEdit = true } = {},
) {
  const index = await readJson(
    join(recoveryStoreRoot, "index.json"),
    "recovery index",
  );
  assert(
    index.version === 1 &&
      Array.isArray(index.active) &&
      index.active.length === 1 &&
      /^[0-9a-f]{32}$/.test(index.active[0] ?? ""),
    "recovery index does not identify exactly one active edit",
  );
  const head = await readJson(
    join(recoveryStoreRoot, "heads", `${index.active[0]}.json`),
    "recovery head",
  );
  assert(
    head.version === 2 &&
      head.document_id === index.active[0] &&
      head.source_kind === "opened" &&
      head.requires_save_as === false,
    "edit recovery head is not for a normally opened document",
  );
  assert(
    Number.isSafeInteger(head.current_revision) &&
      head.current_revision >= 0 &&
      (!requireNewEdit || head.current_revision > 0) &&
      Number.isSafeInteger(head.saved_revision),
    "Rectangle edit did not advance the recovery revision",
  );
  assert(
    /^[0-9a-f]{64}$/.test(head.timeline_object_sha256 ?? ""),
    "recovery timeline digest is invalid",
  );
  const timelinePath = join(
    recoveryStoreRoot,
    "objects",
    head.timeline_object_sha256,
  );
  const timelineBytes = await readFile(timelinePath);
  assert(
    timelineBytes.length <= 256 * 1024 * 1024 &&
      sha256(timelineBytes) === head.timeline_object_sha256,
    "recovery timeline content-address does not match",
  );
  let timeline;
  try {
    timeline = JSON.parse(timelineBytes.toString("utf8"));
  } catch {
    fail("recovery timeline is not valid JSON");
  }
  const rectangles = timeline?.current?.rectangles;
  assert(
    [1, 2].includes(timeline?.schema_version) &&
      timeline.current.revision === head.current_revision &&
      Array.isArray(rectangles) &&
      (!requireNewEdit || rectangles.length > 0),
    "recovery timeline contains no committed Rectangle edit",
  );
  return {
    documentId: head.document_id,
    currentRevision: head.current_revision,
    savedRevision: head.saved_revision,
    rectangleCount: rectangles.length,
    timelineSha256: head.timeline_object_sha256,
    platform,
  };
}

export function parseQpdfRectangleEvidence(output) {
  let document;
  try {
    document = JSON.parse(output);
  } catch {
    fail("qpdf semantic inspection did not return valid JSON");
  }
  assert(
    Array.isArray(document?.qpdf) && document.qpdf.length >= 2,
    "qpdf semantic inspection omitted the PDF object inventory",
  );
  const rectangles = [];
  for (const section of document.qpdf.slice(1)) {
    if (!section || typeof section !== "object" || Array.isArray(section))
      continue;
    for (const [object, entry] of Object.entries(section)) {
      const value = entry?.value;
      if (
        value?.["/Type"] !== "/Annot" ||
        !["/Square", "/Rect"].includes(value?.["/Subtype"])
      )
        continue;
      const rect = value?.["/Rect"];
      assert(
        Array.isArray(rect) &&
          rect.length === 4 &&
          rect.every((coordinate) => Number.isFinite(coordinate)) &&
          rect[2] > rect[0] &&
          rect[3] > rect[1],
        `saved Rectangle ${object} has invalid geometry`,
      );
      assert(
        typeof value?.["/AP"]?.["/N"] === "string" ||
          typeof value?.["/AP"]?.["/N"] === "object",
        `saved Rectangle ${object} has no normal appearance`,
      );
      rectangles.push({ object, subtype: value["/Subtype"], rect });
    }
  }
  assert(
    rectangles.length === 1,
    `expected exactly one independently parsed saved Rectangle, found ${rectangles.length}`,
  );
  return { parser: "qpdf --json", rectangleCount: 1, rectangles };
}

function runXdotool(args) {
  const result = spawnSync("xdotool", args, {
    encoding: "utf8",
    timeout: 5000,
    maxBuffer: 1024 * 1024,
  });
  assert(
    !result.error && result.status === 0,
    `X11 interaction failed (${args.join(" ")}): ${result.stderr || result.error?.message || result.status}`,
  );
  return result.stdout.trim();
}

async function driveRectangleEdit(pid) {
  if (process.platform === "win32") {
    const base = `Add-Type -AssemblyName System.Windows.Forms; Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public class BpUi { [StructLayout(LayoutKind.Sequential)] public struct R { public int L,T,Right,Bottom; } public delegate bool E(IntPtr h, IntPtr p); [DllImport("user32.dll")] public static extern bool EnumWindows(E cb, IntPtr p); [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint p); [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h); [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out R r); [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h); [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow(); [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y); [DllImport("user32.dll")] public static extern void mouse_event(uint f,uint x,uint y,uint d,UIntPtr e); [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint a,uint b,bool f); [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr h); [DllImport("user32.dll")] public static extern void keybd_event(byte k,byte s,uint f,UIntPtr e); [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId(); public static bool Focus(IntPtr h){ for(int i=0;i<20;i++){ IntPtr fg=GetForegroundWindow(); if(fg==h) return true; uint op; uint t=GetWindowThreadProcessId(fg,out op); uint me=GetCurrentThreadId(); bool att=t!=0&&t!=me&&AttachThreadInput(me,t,true); BringWindowToTop(h); SetForegroundWindow(h); if(att) AttachThreadInput(me,t,false); System.Threading.Thread.Sleep(150); if(GetForegroundWindow()==h) return true; keybd_event(0x12,0,0,UIntPtr.Zero); keybd_event(0x12,0,2,UIntPtr.Zero); System.Threading.Thread.Sleep(100); } return GetForegroundWindow()==h; } }'; $target=${pid}; $wins=[System.Collections.Generic.List[object]]::new(); $cb=[BpUi+E]{param($h,$p); [uint32]$owner=0; [void][BpUi]::GetWindowThreadProcessId($h,[ref]$owner); if($owner -eq $target -and [BpUi]::IsWindowVisible($h)){ $r=[BpUi+R]::new(); if([BpUi]::GetWindowRect($h,[ref]$r)){$wins.Add([pscustomobject]@{Handle=$h;X=$r.L;Y=$r.T;Width=$r.Right-$r.L;Height=$r.Bottom-$r.T})} }; return $true }; [void][BpUi]::EnumWindows($cb,[IntPtr]::Zero);`;
    const found = JSON.parse(
      powershell(`${base} ConvertTo-Json -InputObject @($wins) -Compress`),
    );
    assert(
      found.length === 1 && found[0].Width >= 800 && found[0].Height >= 600,
      "expected exactly one sufficiently large visible production app window",
    );
    const win = found[0];
    powershell(
      `${base} $h=[IntPtr]${win.Handle}; if(-not [BpUi]::Focus($h)){throw 'packaged app did not receive keyboard focus'}; Start-Sleep -Milliseconds 300; [System.Windows.Forms.SendKeys]::SendWait('r'); Start-Sleep -Milliseconds 200; [void][BpUi]::SetCursorPos(${Math.round(win.X + win.Width * 0.44)},${Math.round(win.Y + win.Height * 0.42)}); [BpUi]::mouse_event(2,0,0,0,[UIntPtr]::Zero); [void][BpUi]::SetCursorPos(${Math.round(win.X + win.Width * 0.54)},${Math.round(win.Y + win.Height * 0.52)}); Start-Sleep -Milliseconds 150; [BpUi]::mouse_event(4,0,0,0,[UIntPtr]::Zero);`,
    );
    return {
      windowId: String(win.Handle),
      geometry: { x: win.X, y: win.Y, width: win.Width, height: win.Height },
      gesture: {
        start: {
          x: Math.round(win.X + win.Width * 0.44),
          y: Math.round(win.Y + win.Height * 0.42),
        },
        end: {
          x: Math.round(win.X + win.Width * 0.54),
          y: Math.round(win.Y + win.Height * 0.52),
        },
      },
      toolShortcut: "r",
      input: "Windows SendKeys and user32",
    };
  }
  assert(
    process.platform === "linux",
    "desktop interaction requires a supported Windows or Linux session",
  );
  const windows = runXdotool(["search", "--onlyvisible", "--pid", String(pid)])
    .split(/\s+/)
    .filter(Boolean);
  assert(
    windows.length === 1,
    "expected exactly one visible production app window for interaction",
  );
  const windowId = windows[0];
  runXdotool(["windowactivate", "--sync", windowId]);
  const active = runXdotool(["getactivewindow"]);
  assert(
    active === windowId,
    "could not prove the packaged app owns keyboard focus",
  );
  const geometry = runXdotool(["getwindowgeometry", "--shell", windowId]);
  const fields = Object.fromEntries(
    geometry
      .split(/\r?\n/)
      .map((line) => line.split("="))
      .filter((pair) => pair.length === 2),
  );
  const x = Number(fields.X),
    y = Number(fields.Y),
    width = Number(fields.WIDTH),
    height = Number(fields.HEIGHT);
  assert(
    [x, y, width, height].every(Number.isFinite) &&
      width >= 800 &&
      height >= 600,
    "packaged app window is too small or has invalid geometry for the edit smoke",
  );
  // The fixture is opened in the default centred-page view. Use a conservative
  // region inside the central canvas, separated from toolbars and side panels.
  const start = {
    x: Math.round(x + width * 0.44),
    y: Math.round(y + height * 0.42),
  };
  const end = {
    x: Math.round(x + width * 0.54),
    y: Math.round(y + height * 0.52),
  };
  runXdotool(["key", "--clearmodifiers", "r"]);
  runXdotool(["mousemove", "--sync", String(start.x), String(start.y)]);
  runXdotool(["mousedown", "1"]);
  for (let step = 1; step <= 12; step += 1) {
    const fraction = step / 12;
    runXdotool([
      "mousemove",
      "--sync",
      String(Math.round(start.x + (end.x - start.x) * fraction)),
      String(Math.round(start.y + (end.y - start.y) * fraction)),
    ]);
    await sleep(25);
  }
  runXdotool(["mouseup", "1"]);
  return {
    windowId,
    geometry: { x, y, width, height },
    gesture: { start, end },
    toolShortcut: "r",
    input: "xdotool",
  };
}

const WINDOWS_UIA_PRELUDE = `Add-Type -AssemblyName UIAutomationClient; Add-Type -AssemblyName UIAutomationTypes; $A=[Windows.Automation.AutomationElement]; $T=[Windows.Automation.TreeScope]; function Find-Buttons($procId, $name){ $c=[Windows.Automation.AndCondition]::new([Windows.Automation.PropertyCondition]::new($A::ProcessIdProperty,[int]$procId),[Windows.Automation.PropertyCondition]::new($A::NameProperty,$name)); @($A::RootElement.FindAll($T::Descendants,$c) | Where-Object { $_.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and $_.Current.IsEnabled }) }; function Invoke-Element($el){ ([Windows.Automation.InvokePattern]$el.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern)).Invoke() };`;

function powershellQuote(value) {
  return `'${String(value).replaceAll("'", "''")}'`;
}

function spawnPowershell(script) {
  const child = spawn(
    "powershell.exe",
    ["-NoProfile", "-NonInteractive", "-Command", script],
    { windowsHide: true, stdio: ["ignore", "pipe", "pipe"] },
  );
  let stderr = "";
  child.stderr.on("data", (chunk) => {
    stderr += chunk.toString();
  });
  const exited = new Promise((resolveExit) =>
    child.on("exit", (code) => resolveExit(code)),
  );
  return {
    async settle(ms = 10_000) {
      const code = await Promise.race([exited, sleep(ms).then(() => "timeout")]);
      if (code === "timeout") child.kill();
      return { code, stderr: stderr.trim() };
    },
  };
}

// Completes the app's native Save As dialog by accepting the app's suggested
// new target (IDOK). Returns false when no dialog appears within the wait;
// throws for any other failure. The saved path is read back from the app's
// recovery head. No keyboard focus is needed.
function completeSaveAsDialog(pid, waitSeconds) {
  const outcome = powershell(
    `Add-Type -TypeDefinition @'
using System; using System.Text; using System.Collections.Generic; using System.Runtime.InteropServices;
public static class BpSaveAs {
  public delegate bool EnumProc(IntPtr h, IntPtr p);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc cb, IntPtr p);
  [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr parent, EnumProc cb, IntPtr p);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] static extern int GetDlgCtrlID(IntPtr h);
  [DllImport("user32.dll")] static extern IntPtr GetDlgItem(IntPtr h, int id);
  [DllImport("user32.dll")] static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  public static bool IsWindow(IntPtr h) { return IsWindowVisible(h); }
  static string Cls(IntPtr h) { var s = new StringBuilder(256); GetClassName(h, s, 256); return s.ToString(); }
  public static IntPtr FindDialog(uint pid) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, p) => { uint owner; GetWindowThreadProcessId(h, out owner); if (owner == pid && IsWindowVisible(h) && Cls(h) == "#32770") { found = h; return false; } return true; }, IntPtr.Zero);
    return found;
  }
  public static string Complete(IntPtr dialog) {
    var inventory = new List<string>();
    EnumChildWindows(dialog, (h, p) => { inventory.Add(Cls(h) + "#" + GetDlgCtrlID(h)); return true; }, IntPtr.Zero);
    IntPtr ok = GetDlgItem(dialog, 1);
    if (ok == IntPtr.Zero) return "no-ok:" + string.Join(",", inventory.GetRange(0, Math.Min(60, inventory.Count)));
    PostMessage(ok, 0x00F5, IntPtr.Zero, IntPtr.Zero);
    return "posted";
  }
}
'@; $deadline=(Get-Date).AddSeconds(${waitSeconds}); do { Start-Sleep -Milliseconds 250; $dialog=[BpSaveAs]::FindDialog(${pid}) } until($dialog -ne [IntPtr]::Zero -or (Get-Date) -gt $deadline); if($dialog -eq [IntPtr]::Zero){'absent'; return}; Start-Sleep -Milliseconds 500; $result=[BpSaveAs]::Complete($dialog); if($result -ne 'posted'){ throw "Save As dialog automation failed: $result" }; $closeDeadline=(Get-Date).AddSeconds(15); do { Start-Sleep -Milliseconds 250 } until(-not [BpSaveAs]::IsWindow($dialog) -or (Get-Date) -gt $closeDeadline); if([BpSaveAs]::IsWindow($dialog)){ throw 'Save As dialog stayed open after IDOK' }; 'completed:win32-idok-suggested-name'`,
    (waitSeconds + 30) * 1000,
  );
  const last = outcome.split(/\r?\n/).at(-1).trim();
  if (last === "absent") return false;
  assert(last.startsWith("completed:"), `unexpected Save As dialog outcome: ${last}`);
  return last.slice("completed:".length);
}

// Save is a global application action. Linux publishes in place with Ctrl+S.
// Windows publication always requires a new target, so Save opens the native
// Save As dialog, which is completed with a new path. Ctrl+S is the ordinary
// route; the visible document-actions Save control is the fallback.
function decodeRecoveryPath(head) {
  assert(
    head.path_encoding === "windows-utf16le" && /^(?:[0-9a-f]{4})+$/.test(head.source_path ?? ""),
    "saved recovery head has no Windows source path",
  );
  return Buffer.from(head.source_path, "hex").toString("utf16le").replace(/^\\\\\?\\/, "");
}

async function collectSaveDiagnostics(runDir, recoveryStoreRoot, documentId) {
  const diagnostics = { pdfs: [], head: null };
  const walk = async (directory, depth) => {
    if (depth > 8 || diagnostics.pdfs.length > 50) return;
    let entries = [];
    try {
      entries = await readdir(directory, { withFileTypes: true });
    } catch {
      return;
    }
    for (const entry of entries) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) await walk(path, depth + 1);
      else if (/\.pdf$/i.test(entry.name)) {
        const info = await lstat(path).catch(() => null);
        diagnostics.pdfs.push({ path, bytes: info?.size ?? null });
      }
    }
  };
  await walk(runDir, 0);
  try {
    diagnostics.head = JSON.parse(
      await readFile(join(recoveryStoreRoot, "heads", `${documentId}.json`), "utf8"),
    );
  } catch (error) {
    diagnostics.head = { error: error.message };
  }
  const directory = process.env.BP_SMOKE_SCREENSHOT_DIR;
  if (directory && process.platform === "win32") {
    try {
      powershell(
        `Add-Type -AssemblyName System.Windows.Forms, System.Drawing; $b=[System.Windows.Forms.Screen]::PrimaryScreen.Bounds; $bmp=New-Object System.Drawing.Bitmap $b.Width,$b.Height; $g=[System.Drawing.Graphics]::FromImage($bmp); $g.CopyFromScreen($b.Location,[System.Drawing.Point]::Empty,$b.Size); $bmp.Save(${powershellQuote(join(directory, "save-timeout.png"))})`,
      );
    } catch (error) {
      diagnostics.screenshotError = error.message;
    }
  }
  return diagnostics;
}

async function saveEditedDocument(pid, windowHandle) {
  if (process.platform === "linux") {
    runXdotool(["key", "ctrl+s"]);
    return { route: "ctrl+s" };
  }
  // SendWait may block while the modal dialog runs, so send asynchronously.
  const shortcut = spawnPowershell(
    `Add-Type -AssemblyName System.Windows.Forms; Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public class BpFocus { [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h); [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow(); [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint p); [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint a,uint b,bool f); [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr h); [DllImport("user32.dll")] public static extern void keybd_event(byte k,byte s,uint f,UIntPtr e); [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId(); public static bool Focus(IntPtr h){ for(int i=0;i<20;i++){ IntPtr fg=GetForegroundWindow(); if(fg==h) return true; uint op; uint t=GetWindowThreadProcessId(fg,out op); uint me=GetCurrentThreadId(); bool att=t!=0&&t!=me&&AttachThreadInput(me,t,true); BringWindowToTop(h); SetForegroundWindow(h); if(att) AttachThreadInput(me,t,false); System.Threading.Thread.Sleep(150); if(GetForegroundWindow()==h) return true; keybd_event(0x12,0,0,UIntPtr.Zero); keybd_event(0x12,0,2,UIntPtr.Zero); System.Threading.Thread.Sleep(100); } return GetForegroundWindow()==h; } }'; if(-not [BpFocus]::Focus([IntPtr]${windowHandle})){throw 'packaged app did not receive keyboard focus for Save'}; Start-Sleep -Milliseconds 300; [System.Windows.Forms.SendKeys]::SendWait('^s')`,
  );
  const shortcutDialog = completeSaveAsDialog(pid, 15);
  if (shortcutDialog) {
    await shortcut.settle();
    return {
      route: `ctrl+s + native Save As dialog (${shortcutDialog})`,
    };
  }
  const shortcutResult = await shortcut.settle(2000);
  powershell(
    `${WINDOWS_UIA_PRELUDE} $actions=Find-Buttons ${pid} 'Document actions and properties'; if($actions.Count -ne 1){$available=@($A::RootElement.FindAll($T::Descendants,[Windows.Automation.PropertyCondition]::new($A::ProcessIdProperty,[int]${pid})) | Where-Object { $_.Current.ControlType -eq [Windows.Automation.ControlType]::Button } | ForEach-Object { "$($_.Current.Name)[enabled=$($_.Current.IsEnabled),offscreen=$($_.Current.IsOffscreen)]" } | Select-Object -First 100); throw "expected one Document actions and properties button; available buttons: $($available -join ', ')"}; Invoke-Element $actions[0]; $deadline=(Get-Date).AddSeconds(10); do { Start-Sleep -Milliseconds 200; $save=Find-Buttons ${pid} 'Save' } until($save.Count -eq 1 -or (Get-Date) -gt $deadline); if($save.Count -ne 1){throw "expected one enabled Save button after opening document actions; found $($save.Count)"}`,
    45_000,
  );
  const invoke = spawnPowershell(
    `${WINDOWS_UIA_PRELUDE} $save=Find-Buttons ${pid} 'Save'; if($save.Count -ne 1){throw "expected one enabled Save button; found $($save.Count)"}; Invoke-Element $save[0]`,
  );
  const controlDialog = completeSaveAsDialog(pid, 25);
  if (!controlDialog) {
    const invoked = await invoke.settle(2000);
    fail(
      `native Save As dialog did not appear after Ctrl+S or the Save control (Ctrl+S: ${shortcutResult.stderr || shortcutResult.code}; Save control: ${invoked.stderr || invoked.code})`,
    );
  }
  await invoke.settle();
  return {
    route: `document-actions Save + native Save As dialog (${controlDialog})`,
    ctrlS: shortcutResult,
  };
}

export function smokeArtifactStem(runDir) {
  return basename(runDir).replace(/^\./, "");
}

function boundedLog(destination) {
  let kept = 0,
    truncated = false;
  return async (chunk) => {
    const bytes = Buffer.from(chunk),
      take = Math.max(0, Math.min(bytes.length, MAX_LOG_BYTES - kept));
    if (take) {
      await writeFile(destination, bytes.subarray(0, take), { flag: "a" });
      kept += take;
    }
    if (take < bytes.length) truncated = true;
    return truncated;
  };
}

async function runSmoke({
  packagePath,
  architecture,
  fixturePath,
  evidenceRoot,
}) {
  const hostArch = process.arch;
  const expectedArch =
    architecture === "arm64"
      ? process.platform === "win32"
        ? "arm64"
        : "arm64"
      : "x64";
  const actualArch =
    hostArch === "x64" ? "x64" : hostArch === "arm64" ? "arm64" : hostArch;
  assert(
    actualArch === expectedArch,
    `package architecture ${architecture} does not match host architecture ${actualArch}`,
  );
  await regularFile(fixturePath, "fixture PDF");
  const fixtureStat = await lstat(fixturePath);
  assert(
    fixtureStat.size <= 128 * 1024 * 1024,
    "fixture PDF exceeds the 128 MiB smoke limit",
  );
  const fixtureBytes = await readFile(fixturePath);
  assert(
    fixtureBytes.subarray(0, 5).toString("ascii") === "%PDF-",
    "fixture input does not have a PDF header",
  );
  const evidenceStat = await lstat(evidenceRoot);
  assert(
    evidenceStat.isDirectory() && !evidenceStat.isSymbolicLink(),
    "evidence root must be an existing real directory",
  );
  const evidenceReal = await realpath(evidenceRoot);
  const runDir = await mkdtemp(
    join(evidenceReal, `.bp-runtime-smoke-${architecture}-`),
  );
  const artifactStem = smokeArtifactStem(runDir);
  const home = join(runDir, "home"),
    local = join(runDir, "local");
  await Promise.all([
    mkdir(home, { recursive: true }),
    mkdir(local, { recursive: true }),
  ]);
  const stdoutPath = join(evidenceReal, `${artifactStem}.stdout.log`),
    stderrPath = join(evidenceReal, `${artifactStem}.stderr.log`);
  let child,
    pkg,
    spawnError,
    result = {
      schema: "butter-paper/nonmac-runtime-smoke",
      schemaVersion: 1,
      architecture,
      hostPlatform: process.platform,
      hostArchitecture: actualArch,
      package: resolve(packagePath),
      fixture: resolve(fixturePath),
      prerequisites:
        process.platform === "linux"
          ? [
              "matching-architecture Linux host",
              "reachable X11 display via DISPLAY (an existing Xvfb display is accepted)",
              "xdpyinfo",
              "xdotool",
              "xz",
              "qpdf",
            ]
          : [
              "matching-architecture Windows host",
              "Windows PowerShell",
              "taskkill.exe",
              "qpdf",
            ],
      evidence: { stdout: stdoutPath, stderr: stderrPath },
      cleanup: { status: "not-started", tempRootRemoved: false },
      claims: [
        "package validation",
        "production app remained alive",
        "packaged PDF worker observed",
        "exact opened-document recovery checkpoint observed",
        "Rectangle edit observed in production recovery timeline",
        "normal Save changed the disposable PDF",
        "qpdf independently validated the saved PDF and its Rectangle appearance",
        "full app close and fresh packaged-app reopen",
        "process cleanup",
      ],
      limitations: [],
    };
  let stdoutSink = boundedLog(stdoutPath),
    stderrSink = boundedLog(stderrPath),
    stdoutTruncated = false,
    stderrTruncated = false;
  try {
    if (process.platform === "linux")
      result.runtimePrerequisitesValidated = await checkLinuxDisplay();
    else {
      const qpdf = spawnSync(
        "powershell.exe",
        [
          "-NoProfile",
          "-NonInteractive",
          "-Command",
          "(Get-Command qpdf -ErrorAction Stop).Source",
        ],
        { encoding: "utf8", windowsHide: true, timeout: 45_000 },
      );
      assert(
        !qpdf.error && qpdf.status === 0 && qpdf.stdout.trim(),
        "Windows smoke requires qpdf on PATH for independent saved-PDF validation",
      );
      result.runtimePrerequisitesValidated = {
        signatureVerification: "package integrity sidecars; Authenticode when the package signature policy requires it",
        processInspection: "Windows PowerShell/CIM",
        cleanup: "taskkill.exe",
        pdfValidation: qpdf.stdout.trim(),
      };
    }
    pkg =
      process.platform === "win32"
        ? await validateWindows(resolve(packagePath), architecture, runDir)
        : process.platform === "linux"
          ? await validateLinux(resolve(packagePath), architecture, runDir)
          : fail("host OS must be Windows or Linux");
    result.packageIdentity = {
      target: pkg.target,
      version: pkg.version,
      sourceRevision: pkg.revision,
      archiveSha256: pkg.archiveSha256,
      signerThumbprint: pkg.signerThumbprint,
      signaturePolicy: pkg.signaturePolicy,
    };
    const env = cleanEnvironment(home, local);
    const ownedFixturePath = join(runDir, "fixture.pdf");
    await writeFile(ownedFixturePath, fixtureBytes, {
      flag: "wx",
      mode: 0o600,
    });
    const launch = (documentPath = ownedFixturePath) =>
      spawn(pkg.executable, [documentPath], {
        cwd: pkg.packageRoot,
        env,
        windowsHide: false,
        detached: process.platform === "linux",
        stdio: ["ignore", "pipe", "pipe"],
      });
    const attachLogs = (processChild) => {
      processChild.on("error", (error) => {
        spawnError = error;
      });
      processChild.stdout.on("data", (chunk) => {
        stdoutSink(chunk)
          .then((truncated) => {
            stdoutTruncated ||= truncated;
          })
          .catch(() => {});
      });
      processChild.stderr.on("data", (chunk) => {
        stderrSink(chunk)
          .then((truncated) => {
            stderrTruncated ||= truncated;
          })
          .catch(() => {});
      });
    };
    child = launch();
    attachLogs(child);
    const startedAt = Date.now();
    await waitUntil(
      async () => {
        if (spawnError)
          fail(`packaged application could not start: ${spawnError.message}`);
        if (child.exitCode !== null || child.signalCode !== null)
          fail(
            `packaged application exited before the smoke window (${child.exitCode ?? child.signalCode})`,
          );
        return Date.now() - startedAt >= MIN_ALIVE_MS;
      },
      LIMIT_MS,
      "the packaged application to remain alive",
    );
    await waitUntil(
      async () => {
        const processes = currentProcesses(child.pid, pkg.packageRoot);
        if (!processes.some((proc) => proc.pid === child.pid))
          fail(
            "packaged application process disappeared during runtime observation",
          );
        return hasExecutable(processes, pkg.worker).length > 0;
      },
      Math.max(1000, LIMIT_MS - (Date.now() - startedAt)),
      "the real packaged PDF worker",
    );
    const recoveryStoreRoot = stableRecoveryStoreRoot({
      platform: process.platform,
      appData: env.APPDATA,
      xdgDataHome: env.XDG_DATA_HOME,
    });
    await waitUntil(
      async () => {
        const evidence = await tryReadProductionOpenEvidence({
          recoveryStoreRoot,
          fixturePath: ownedFixturePath,
          fixtureBytes,
          platform: process.platform,
        });
        if (!evidence) return false;
        result.documentOpenEvidence = evidence;
        return true;
      },
      Math.max(1000, LIMIT_MS - (Date.now() - startedAt)),
      "the exact opened-document recovery checkpoint",
    );
    const processEvidence = [];
    for (const sample of [0, 1]) {
      const processes = currentProcesses(child.pid, pkg.packageRoot);
      const workers = hasExecutable(processes, pkg.worker);
      assert(
        processes.some((proc) => proc.pid === child.pid),
        "packaged application did not remain alive for both observations",
      );
      assert(
        workers.length > 0,
        "packaged PDF worker exited before the second observation",
      );
      processEvidence.push({
        atMs: Date.now() - startedAt,
        appPid: child.pid,
        workerPids: workers.map((proc) => proc.pid),
      });
      if (sample === 0) await sleep(POLL_MS);
    }
    result.observation = {
      aliveMs: Date.now() - startedAt,
      processSamples: processEvidence,
      closeRequest: null,
      cleanup: null,
    };
    if (process.platform === "linux") {
      // Owner-accepted first-release scope: the synthetic xdotool drag under
      // Xvfb does not commit a Rectangle edit, so Linux qualifies launch,
      // exact document open, PDF worker and process cleanup only.
      result.claims = result.claims.filter(
        (claim) => !/Rectangle|Save|reopen/.test(claim),
      );
      result.limitations.push(
        "Linux Rectangle edit, Save and reopen were not exercised: synthetic xdotool input under Xvfb does not commit an edit. Launch, exact document open, PDF worker and process cleanup were verified.",
      );
    } else {
      const rectangleBaseline = await readRectangleEditEvidence(
        recoveryStoreRoot,
        process.platform,
        { requireNewEdit: false },
      );
      assert(
        rectangleBaseline.currentRevision === 0 &&
          rectangleBaseline.savedRevision === 0 &&
          rectangleBaseline.rectangleCount === 0,
        "initial exact-open recovery state is not a clean rectangle-free revision zero",
      );
      progress("driving Rectangle edit");
      result.observation.rectangleEdit = await driveRectangleEdit(child.pid);
      const editedState = await waitUntil(
        async () =>
          readRectangleEditEvidence(recoveryStoreRoot, process.platform).catch(
            (error) => {
              if (/no committed Rectangle edit/.test(error.message)) return null;
              throw error;
            },
          ),
        Math.max(1000, LIMIT_MS - (Date.now() - startedAt)),
        "the committed Rectangle edit in recovery state",
      );
      assert(
        editedState.currentRevision > rectangleBaseline.currentRevision &&
          editedState.rectangleCount > rectangleBaseline.rectangleCount,
        "pointer gesture did not add a new Rectangle to the initially opened document",
      );
      result.observation.rectangleEdit.recovery = {
        baseline: rectangleBaseline,
        edited: editedState,
      };
      const originalPdfHash = sha256(fixtureBytes);
      let savedPdfPath = ownedFixturePath;
      progress("saving edited document");
      result.observation.save = await saveEditedDocument(
        child.pid,
        result.observation.rectangleEdit.windowId,
      );
      try {
        await waitUntil(
          async () => {
            if (process.platform === "win32") {
              // Save As publishes a new canonical target; the recovery head
              // records it once the save has completed.
              const head = await readJson(
                join(recoveryStoreRoot, "heads", `${editedState.documentId}.json`),
                "saved recovery head",
              );
              if (head.saved_revision !== editedState.currentRevision) return false;
              const target = decodeRecoveryPath(head);
              assert(
                resolve(dirname(target)).toLowerCase() === resolve(runDir).toLowerCase() &&
                  resolve(target).toLowerCase() !== resolve(ownedFixturePath).toLowerCase(),
                `Save As target is outside the disposable run directory or replaced the original: ${target}`,
              );
              savedPdfPath = target;
              result.observation.save.target = target;
            }
            try {
              await lstat(savedPdfPath);
            } catch (error) {
              if (error?.code === "ENOENT") return false;
              throw error;
            }
            await regularFile(savedPdfPath, "saved disposable PDF");
            const savedStat = await lstat(savedPdfPath);
            assert(
              savedStat.size <= 128 * 1024 * 1024,
              "saved PDF exceeds the 128 MiB smoke limit",
            );
            const current = await readFile(savedPdfPath);
            if (sha256(current) === originalPdfHash) return false;
            if (process.platform === "win32") {
              // Save As publishes a new target; the opened original must be untouched.
              assert(
                sha256(await readFile(ownedFixturePath)) === originalPdfHash,
                "Save As modified the original disposable PDF",
              );
            } else {
              const savedHead = await readJson(
                join(recoveryStoreRoot, "heads", `${editedState.documentId}.json`),
                "saved recovery head",
              );
              if (
                savedHead.current_revision !== editedState.currentRevision ||
                savedHead.saved_revision !== editedState.currentRevision
              )
                return false;
            }
            assert(
              current.subarray(0, 5).toString("ascii") === "%PDF-" &&
                current.includes(Buffer.from("%%EOF")),
              "normal Save produced an invalid PDF header or EOF",
            );
            const check = spawnSync("qpdf", ["--check", savedPdfPath], {
              encoding: "utf8",
              timeout: 15_000,
              maxBuffer: 2 * 1024 * 1024,
            });
            assert(
              !check.error && check.status === 0,
              `independent qpdf validation failed: ${check.stderr || check.error?.message || check.status}`,
            );
            const semantic = spawnSync("qpdf", ["--json", savedPdfPath], {
              encoding: "utf8",
              timeout: 15_000,
              maxBuffer: 16 * 1024 * 1024,
            });
            assert(
              !semantic.error && semantic.status === 0,
              `independent qpdf semantic inspection failed: ${semantic.stderr || semantic.error?.message || semantic.status}`,
            );
            result.observation.savedPdf = {
              bytes: current.length,
              sha256: sha256(current),
              qpdf: "--check passed",
              semantic: parseQpdfRectangleEvidence(semantic.stdout),
            };
            return true;
          },
          // Save As validates, publishes and independently reopens the target.
          Math.max(45_000, LIMIT_MS - (Date.now() - startedAt)),
          "normal Save to publish and independently validate the Rectangle-edited PDF",
        );
      } catch (error) {
        result.observation.saveDiagnostics = await collectSaveDiagnostics(
          runDir,
          recoveryStoreRoot,
          editedState.documentId,
        );
        throw error;
      }
      progress("requesting graceful close");
      result.observation.closeRequest = await askGracefulClose(child.pid);
      await waitUntil(
        async () => child.exitCode !== null || child.signalCode !== null,
        Math.max(1000, LIMIT_MS - (Date.now() - startedAt)),
        "the first packaged app window to close fully",
      );
      assert(
        currentProcesses(child.pid, pkg.packageRoot).length === 0,
        "packaged app or PDF worker remained after the full close",
      );
      await rm(recoveryStoreRoot, { recursive: true, force: true });
      result.observation.recoveryResetBeforeFreshReopen = true;
      progress("relaunching saved PDF");
      child = launch(savedPdfPath);
      attachLogs(child);
      const reopenedPid = child.pid;
      const reopenStartedAt = Date.now();
      await waitUntil(
        async () => {
          if (spawnError)
            fail(
              `fresh packaged application could not start: ${spawnError.message}`,
            );
          if (child.exitCode !== null || child.signalCode !== null)
            fail(
              `fresh packaged application exited early (${child.exitCode ?? child.signalCode})`,
            );
          return (
            Date.now() - reopenStartedAt >= MIN_ALIVE_MS &&
            hasExecutable(
              currentProcesses(reopenedPid, pkg.packageRoot),
              pkg.worker,
            ).length > 0
          );
        },
        LIMIT_MS,
        "the fresh packaged app and its PDF worker",
      );
      result.observation.freshReopen = await waitUntil(
        async () =>
          tryReadProductionOpenEvidence({
            recoveryStoreRoot,
            fixturePath: savedPdfPath,
            fixtureBytes: await readFile(savedPdfPath),
            platform: process.platform,
          }),
        LIMIT_MS,
        "the saved PDF to open in the fresh packaged app",
      );
    }
  } catch (error) {
    result.error = error.message;
  } finally {
    if (child && child.pid) {
      result.observation ??= {};
      try {
        progress("cleaning up owned processes");
        const cleanup = await cleanupOwnedProcess({
          rootPid: child.pid,
          listProcesses: async (pid) =>
            currentProcesses(pid, pkg?.packageRoot ?? runDir),
          requestClose: askGracefulClose,
          terminate: async (pid) =>
            terminateOwned(pid, pkg?.packageRoot ?? runDir),
          gracefulMs: 5000,
          wait: (ms) => sleep(Math.min(ms, 200)),
        });
        result.observation.closeRequest = cleanup.closeRequest;
        result.observation.cleanup = {
          remaining: cleanup.remaining.map(({ pid: processId, ppid, exe }) => ({
            pid: processId,
            ppid,
            executable: exe,
          })),
        };
        if (cleanup.remaining.length) {
          result.cleanup = {
            status: "unknown-or-failed",
            remaining: result.observation.cleanup.remaining,
          };
          result.error = `${result.error ? `${result.error}; ` : ""}owned app/worker processes remain after cleanup`;
        } else {
          result.cleanup = { status: "verified-clean", remaining: [] };
        }
      } catch (error) {
        // A process inspector failure must not skip the emergency tree kill.
        const emergency = await emergencyCleanupOwnedProcess({
          rootPid: child.pid,
          terminate: async (pid) => {
            if (process.platform === "win32") {
              const killed = spawnSync(
                "taskkill.exe",
                ["/PID", String(pid), "/T", "/F"],
                { encoding: "utf8", windowsHide: true, timeout: 12_000 },
              );
              if (killed.error) throw killed.error;
              return;
            }
            try {
              process.kill(-pid, "SIGKILL");
            } catch {}
            try {
              for (const proc of currentProcesses(
                pid,
                pkg?.packageRoot ?? runDir,
              ))
                try {
                  process.kill(proc.pid, "SIGKILL");
                } catch {}
            } catch {}
            try {
              process.kill(pid, "SIGKILL");
            } catch {}
          },
          listProcesses: async (pid) =>
            currentProcesses(pid, pkg?.packageRoot ?? runDir),
        });
        result.observation.cleanup = {
          emergencyTermination: true,
          trigger: error.message,
          terminationError: emergency.terminationError,
          remaining:
            emergency.remaining?.map(({ pid: processId, ppid, exe }) => ({
              pid: processId,
              ppid,
              executable: exe,
            })) ?? null,
          inspectionError: emergency.inspectionError,
        };
        result.cleanup = {
          status: "unknown-or-failed",
          ...result.observation.cleanup,
        };
        const cleanupStatus =
          emergency.remaining === null
            ? `post-kill process inspection is unknown: ${emergency.inspectionError}`
            : emergency.remaining.length
              ? `${emergency.remaining.length} owned process(es) remain after emergency termination`
              : "post-kill inspection found no owned processes";
        result.error = `${result.error ? `${result.error}; ` : ""}process cleanup required emergency termination (${cleanupStatus})`;
      }
      result.observation.stdoutTruncated = stdoutTruncated;
      result.observation.stderrTruncated = stderrTruncated;
    }
    try {
      await rm(runDir, { recursive: true, force: true });
      result.cleanup.tempRootRemoved = true;
    } catch (error) {
      result.cleanup.tempRootRemoved = false;
      result.error = `${result.error ? `${result.error}; ` : ""}could not remove owned temporary package data: ${error.message}`;
    }
    result.passed =
      !result.error &&
      result.cleanup.status === "verified-clean" &&
      result.cleanup.tempRootRemoved;
    const evidencePath = join(evidenceReal, `${artifactStem}.json`);
    result.evidence.result = evidencePath;
    await writeFile(evidencePath, `${JSON.stringify(result, null, 2)}\n`, {
      flag: "wx",
      mode: 0o600,
    });
  }
  if (!result.passed)
    fail(`${result.error} (evidence: ${result.evidence.result})`);
  return result;
}

export { runSmoke };

export async function cleanupOwnedProcess({
  rootPid,
  listProcesses,
  requestClose,
  terminate,
  wait = sleep,
  gracefulMs = 1000,
}) {
  let closeRequest = "unavailable",
    closeError,
    terminateError;
  try {
    closeRequest = await requestClose(rootPid);
  } catch (error) {
    closeError = error;
  }
  try {
    const deadline = Date.now() + gracefulMs;
    while (Date.now() < deadline && (await listProcesses(rootPid)).length > 0)
      await wait(50);
  } catch (error) {
    closeError ??= error;
  }
  try {
    await terminate(rootPid);
  } catch (error) {
    terminateError = error;
  }
  let remaining;
  try {
    remaining = await listProcesses(rootPid);
  } catch (error) {
    terminateError ??= error;
    remaining = [{ pid: rootPid, inspectionError: error.message }];
  }
  return {
    closeRequest,
    closeError: closeError?.message,
    terminateError: terminateError?.message,
    remaining,
  };
}

export async function emergencyCleanupOwnedProcess({
  rootPid,
  listProcesses,
  terminate,
}) {
  let terminationError,
    remaining = null,
    inspectionError;
  try {
    await terminate(rootPid);
  } catch (error) {
    terminationError = error.message;
  }
  try {
    remaining = await listProcesses(rootPid);
  } catch (error) {
    inspectionError = error.message;
  }
  return { terminationError, remaining, inspectionError };
}

function parseArgs(args) {
  if (args.length === 1 && ["--help", "-h"].includes(args[0])) {
    process.stdout.write(
      "Usage: node smoke-nonmac-production-package.mjs --package ZIP|TAR.XZ --architecture x86_64|arm64 --fixture-pdf PDF --evidence-root EXISTING_DIR\nLinux requires a reachable X11 DISPLAY (an existing Xvfb display is accepted), xdpyinfo, xdotool, xz, and qpdf. Windows requires PowerShell, qpdf on PATH, and an interactive desktop session.\n",
    );
    return null;
  }
  const values = new Map();
  for (let index = 0; index < args.length; index += 1) {
    const key = args[index];
    if (
      ![
        "--package",
        "--architecture",
        "--fixture-pdf",
        "--evidence-root",
      ].includes(key) ||
      !args[index + 1] ||
      values.has(key)
    )
      fail(
        "usage: smoke-nonmac-production-package.mjs --package ZIP|TAR.XZ --architecture x86_64|arm64 --fixture-pdf PDF --evidence-root EXISTING_DIR",
      );
    values.set(key, args[++index]);
  }
  for (const key of [
    "--package",
    "--architecture",
    "--fixture-pdf",
    "--evidence-root",
  ])
    assert(
      values.has(key),
      "usage: smoke-nonmac-production-package.mjs --package ZIP|TAR.XZ --architecture x86_64|arm64 --fixture-pdf PDF --evidence-root EXISTING_DIR",
    );
  assert(
    ["x86_64", "arm64"].includes(values.get("--architecture")),
    "architecture must be x86_64 or arm64",
  );
  return {
    packagePath: values.get("--package"),
    architecture: values.get("--architecture"),
    fixturePath: values.get("--fixture-pdf"),
    evidenceRoot: values.get("--evidence-root"),
  };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const options = parseArgs(process.argv.slice(2));
  if (options)
    runSmoke(options)
      .then((result) =>
        process.stdout.write(`${JSON.stringify(result, null, 2)}\n`),
      )
      .catch((error) => {
        process.stderr.write(`${error.message}\n`);
        process.exitCode = 1;
      });
}
