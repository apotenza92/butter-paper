#!/usr/bin/env node

import { createHash } from "node:crypto";
import { lstat, mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { isAbsolute, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const commonRequired = [
  "gpui-migration.exe",
  "butter-paper-pdf-worker.exe",
  "butter-paper-signature-phone.exe",
  "pdfium.dll",
  "README.md",
  "THIRD_PARTY_NOTICES.md",
  "PHONE_HELPER_THIRD_PARTY_NOTICES.md",
  "QRCP_LICENSE",
  "SIGNATURE_PAD_LICENSE",
];

const targets = {
  x86_64: { rust: "x86_64-pc-windows-msvc", receipt: "production-pdfium-windows-x86_64.json", machine: 0x8664 },
  arm64: { rust: "aarch64-pc-windows-msvc", receipt: "production-pdfium-windows-arm64.json", machine: 0xaa64 },
};

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function fail(message) {
  throw new Error(message);
}

function validateIco(bytes) {
  if (!Buffer.isBuffer(bytes) || bytes.length < 22 || bytes.readUInt16LE(0) !== 0 || bytes.readUInt16LE(2) !== 1) return false;
  const count = bytes.readUInt16LE(4);
  if (count < 1) return false;
  const directoryEnd = 6 + count * 16;
  for (let index = 0; index < count; index += 1) {
    const entry = 6 + index * 16;
    const size = bytes.readUInt32LE(entry + 8);
    const offset = bytes.readUInt32LE(entry + 12);
    if (!size || offset < directoryEnd || offset + size > bytes.length) return false;
  }
  return true;
}

function validatePe64(bytes, target, label, dll) {
  if (bytes.length < 0x40 || bytes.toString("ascii", 0, 2) !== "MZ") fail(`${label} is not a valid PE file`);
  const peOffset = bytes.readUInt32LE(0x3c);
  if (peOffset < 0x40 || peOffset + 26 > bytes.length || bytes.toString("ascii", peOffset, peOffset + 4) !== "PE\0\0") fail(`${label} is not a valid PE file`);
  const optionalSize = bytes.readUInt16LE(peOffset + 20);
  const characteristics = bytes.readUInt16LE(peOffset + 22);
  if (bytes.readUInt16LE(peOffset + 4) !== target.machine || optionalSize < 2 || peOffset + 24 + optionalSize > bytes.length || bytes.readUInt16LE(peOffset + 24) !== 0x20b) {
    fail(`${label} is not a 64-bit PE file for ${target.rust}`);
  }
  if (((characteristics & 0x2000) !== 0) !== dll) fail(`${label} has the wrong PE image type`);
}

function psLiteral(value) { return `'${String(value).replaceAll("'", "''")}'`; }

function installScript({ version, architecture, files }) {
  const packageNames = [...Object.keys(files), "install.ps1", "uninstall.ps1"].sort();
  const nameArray = [...packageNames, "MANIFEST.json"].map(psLiteral).join(", ");
  const manifestNames = packageNames.map(psLiteral).join(", ");
  const progId = `ButterPaper.PDF.${version}.${architecture}`;
  const shortcutName = `Butter Paper ${version} (${architecture}).lnk`;
  return `# Per-user install for Butter Paper ${version} (${architecture}); no elevation is requested.
$ErrorActionPreference = 'Stop'
$version = ${psLiteral(version)}
$architecture = ${psLiteral(architecture)}
$progId = ${psLiteral(progId)}
$source = $PSScriptRoot
$localAppData = [Environment]::GetFolderPath('LocalApplicationData')
if ([string]::IsNullOrWhiteSpace($localAppData)) { throw 'LocalAppData is unavailable.' }
$installRoot = Join-Path $localAppData ('Programs\\Butter Paper\\' + $version + '\\' + $architecture)
$shortcutPath = Join-Path ([Environment]::GetFolderPath('Programs')) ${psLiteral(shortcutName)}
if (Test-Path -LiteralPath $installRoot) { throw ('Install destination already exists: ' + $installRoot) }
if (Test-Path -LiteralPath $shortcutPath) { throw ('Shortcut already exists: ' + $shortcutPath) }
$readClasses = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Software\\Classes')
if ($null -ne $readClasses) {
  try {
    $existingProgId = $readClasses.OpenSubKey($progId)
    if ($null -ne $existingProgId) { $existingProgId.Dispose(); throw ('ProgID already exists: ' + $progId) }
    $existingOpenWith = $readClasses.OpenSubKey('.pdf\\OpenWithProgids')
    if ($null -ne $existingOpenWith) {
      try { if ($existingOpenWith.GetValueNames() -contains $progId) { throw ('OpenWith value already exists: ' + $progId) } } finally { $existingOpenWith.Dispose() }
    }
  } finally { $readClasses.Dispose() }
}
$parent = Split-Path -Parent $installRoot
New-Item -ItemType Directory -Path $parent -Force | Out-Null
New-Item -ItemType Directory -Path $installRoot | Out-Null
$packageFiles = @(${nameArray})
$expectedManifestFiles = @(${manifestNames})
$createdOpenWithValue = $false
$createdProgId = $false
$createdShortcut = $false
try {
  foreach ($name in $packageFiles) {
    $from = Join-Path $source $name
    if (-not (Test-Path -LiteralPath $from -PathType Leaf)) { throw ('Package file is missing: ' + $name) }
    Copy-Item -LiteralPath $from -Destination (Join-Path $installRoot $name)
  }
  $exe = Join-Path $installRoot 'gpui-migration.exe'
  $manifestPath = Join-Path $installRoot 'MANIFEST.json'
  $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
  $expectedTarget = if ($architecture -eq 'arm64') { 'aarch64-pc-windows-msvc' } else { 'x86_64-pc-windows-msvc' }
  if ($manifest.product -ne 'Butter Paper' -or $manifest.version -ne $version -or $manifest.target -ne $expectedTarget) { throw 'Copied package manifest identity is invalid.' }
  $actualManifestFiles = @($manifest.files.PSObject.Properties.Name | Sort-Object)
  if (Compare-Object $expectedManifestFiles $actualManifestFiles) { throw 'Copied package manifest inventory is invalid.' }
  foreach ($name in $expectedManifestFiles) {
    $record = $manifest.files.$name
    $copiedFile = Join-Path $installRoot $name
    if (-not (Test-Path -LiteralPath $copiedFile -PathType Leaf)) { throw ('Manifest package file is missing: ' + $name) }
    $fileInfo = Get-Item -LiteralPath $copiedFile
    $fileHash = (Get-FileHash -LiteralPath $copiedFile -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($fileInfo.Length -ne $record.bytes -or $fileHash -ne $record.sha256) { throw ('Copied package file does not match manifest: ' + $name) }
  }
  $marker = [pscustomobject]@{ product = 'Butter Paper'; version = $version; architecture = $architecture; installPath = [IO.Path]::GetFullPath($installRoot) }
  $marker | ConvertTo-Json -Compress | Set-Content -LiteralPath (Join-Path $installRoot '.butter-paper-install.json') -Encoding UTF8
  $classes = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Software\\Classes')
  try {
    $pdf = $classes.CreateSubKey('.pdf\\OpenWithProgids')
    try { $createdOpenWithValue = $true; $pdf.SetValue($progId, [byte[]]@(), [Microsoft.Win32.RegistryValueKind]::None) } finally { $pdf.Dispose() }
    $registration = $classes.CreateSubKey($progId)
    if ($null -eq $registration) { throw 'Could not create the package ProgID.' }
    $createdProgId = $true
    try {
      $registration.SetValue('', 'Butter Paper PDF Document', [Microsoft.Win32.RegistryValueKind]::String)
      $icon = $registration.CreateSubKey('DefaultIcon')
      try { $icon.SetValue('', ('"' + (Join-Path $installRoot 'butter-paper.ico') + '"'), [Microsoft.Win32.RegistryValueKind]::String) } finally { $icon.Dispose() }
      $command = $registration.CreateSubKey('shell\\open\\command')
      try { $command.SetValue('', ('"' + $exe + '" "%1"'), [Microsoft.Win32.RegistryValueKind]::String) } finally { $command.Dispose() }
    } finally { $registration.Dispose() }
  } finally { $classes.Dispose() }
  $shell = New-Object -ComObject WScript.Shell
  $shortcut = $shell.CreateShortcut($shortcutPath)
  $shortcut.TargetPath = $exe
  $shortcut.WorkingDirectory = $installRoot
  $shortcut.IconLocation = (Join-Path $installRoot 'butter-paper.ico')
  $shortcut.Save()
  $createdShortcut = $true
} catch {
  if ($createdShortcut) { try { Remove-Item -LiteralPath $shortcutPath -Force -ErrorAction SilentlyContinue } catch {} }
  if ($createdOpenWithValue -or $createdProgId) {
    try {
      $rollbackClasses = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Software\\Classes', $true)
      if ($null -ne $rollbackClasses) {
        try {
          if ($createdOpenWithValue) { $rollbackPdf = $rollbackClasses.OpenSubKey('.pdf\\OpenWithProgids', $true); if ($null -ne $rollbackPdf) { try { $rollbackPdf.DeleteValue($progId, $false) } finally { $rollbackPdf.Dispose() } } }
          if ($createdProgId) { $rollbackClasses.DeleteSubKeyTree($progId, $false) }
        } finally { $rollbackClasses.Dispose() }
      }
    } catch {}
  }
  if (Test-Path -LiteralPath $installRoot) { Remove-Item -LiteralPath $installRoot -Recurse -Force -ErrorAction SilentlyContinue }
  throw
}
Write-Output ('Installed for this user at ' + $installRoot + '. PDF default choice was not changed.')
`;
}

function uninstallScript({ version, architecture }) {
  const progId = `ButterPaper.PDF.${version}.${architecture}`;
  const shortcutName = `Butter Paper ${version} (${architecture}).lnk`;
  return `# Reversible per-user uninstall for Butter Paper ${version} (${architecture}).
$ErrorActionPreference = 'Stop'
$version = ${psLiteral(version)}
$architecture = ${psLiteral(architecture)}
$progId = ${psLiteral(progId)}
$localAppData = [Environment]::GetFolderPath('LocalApplicationData')
$installRoot = Join-Path $localAppData ('Programs\\Butter Paper\\' + $version + '\\' + $architecture)
$expectedRoot = [IO.Path]::GetFullPath($installRoot)
if (-not (Test-Path -LiteralPath $expectedRoot -PathType Container)) { throw ('Owned install directory is missing: ' + $expectedRoot) }
$markerPath = Join-Path $expectedRoot '.butter-paper-install.json'
if (-not (Test-Path -LiteralPath $markerPath -PathType Leaf)) { throw 'Ownership marker is missing; preserving the install directory.' }
try { $marker = Get-Content -LiteralPath $markerPath -Raw | ConvertFrom-Json } catch { throw 'Ownership marker is invalid; preserving the install directory.' }
if ($marker.product -ne 'Butter Paper' -or $marker.version -ne $version -or $marker.architecture -ne $architecture -or [IO.Path]::GetFullPath([string]$marker.installPath) -ne $expectedRoot) { throw 'Ownership marker does not match this exact package install; preserving the install directory.' }
$shortcutPath = Join-Path ([Environment]::GetFolderPath('Programs')) ${psLiteral(shortcutName)}
$shortcutMatches = $false
if (Test-Path -LiteralPath $shortcutPath) {
  $shell = New-Object -ComObject WScript.Shell
  $shortcut = $shell.CreateShortcut($shortcutPath)
  $shortcutMatches = ($shortcut.TargetPath -eq (Join-Path $expectedRoot 'gpui-migration.exe'))
}
if ($shortcutMatches) { Remove-Item -LiteralPath $shortcutPath -Force }
$classes = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Software\\Classes', $true)
if ($null -ne $classes) {
  try {
    $registration = $classes.OpenSubKey($progId)
    $owned = $false
    if ($null -ne $registration) { try { $command = $registration.OpenSubKey('shell\\open\\command'); if ($null -ne $command) { try { $owned = ($command.GetValue('') -eq ('"' + (Join-Path $expectedRoot 'gpui-migration.exe') + '" "%1"')) } finally { $command.Dispose() } } } finally { $registration.Dispose() } }
    if ($owned) {
      $pdf = $classes.OpenSubKey('.pdf\\OpenWithProgids', $true)
      if ($null -ne $pdf) { try { $pdf.DeleteValue($progId, $false) } finally { $pdf.Dispose() } }
      $classes.DeleteSubKeyTree($progId, $false)
    }
  } finally { $classes.Dispose() }
}
Remove-Item -LiteralPath $expectedRoot -Recurse -Force
Write-Output ('Removed this user installation at ' + $expectedRoot + '. Other PDF associations were preserved.')
`;
}

function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit += 1) crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0);
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function deterministicZip(files) {
  const local = [];
  const central = [];
  let offset = 0;
  for (const [name, bytes] of [...files].sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)) {
    const filename = Buffer.from(name, "utf8");
    const crc = crc32(bytes);
    const localHeader = Buffer.alloc(30);
    localHeader.writeUInt32LE(0x04034b50, 0);
    localHeader.writeUInt16LE(20, 4);
    localHeader.writeUInt16LE(0x0800, 6);
    localHeader.writeUInt16LE(0, 8);
    localHeader.writeUInt16LE(0, 10);
    localHeader.writeUInt16LE(0x21, 12);
    localHeader.writeUInt32LE(crc, 14);
    localHeader.writeUInt32LE(bytes.length, 18);
    localHeader.writeUInt32LE(bytes.length, 22);
    localHeader.writeUInt16LE(filename.length, 26);
    localHeader.writeUInt16LE(0, 28);
    local.push(localHeader, filename, bytes);

    const centralHeader = Buffer.alloc(46);
    centralHeader.writeUInt32LE(0x02014b50, 0);
    centralHeader.writeUInt16LE(0x0314, 4);
    centralHeader.writeUInt16LE(20, 6);
    centralHeader.writeUInt16LE(0x0800, 8);
    centralHeader.writeUInt16LE(0, 10);
    centralHeader.writeUInt16LE(0, 12);
    centralHeader.writeUInt16LE(0x21, 14);
    centralHeader.writeUInt32LE(crc, 16);
    centralHeader.writeUInt32LE(bytes.length, 20);
    centralHeader.writeUInt32LE(bytes.length, 24);
    centralHeader.writeUInt16LE(filename.length, 28);
    centralHeader.writeUInt16LE(0, 30);
    centralHeader.writeUInt16LE(0, 32);
    centralHeader.writeUInt16LE(0, 34);
    centralHeader.writeUInt16LE(0, 36);
    centralHeader.writeUInt32LE(0, 38);
    centralHeader.writeUInt32LE(offset, 42);
    central.push(centralHeader, filename);
    offset += localHeader.length + filename.length + bytes.length;
  }
  const centralBytes = Buffer.concat(central);
  const localBytes = Buffer.concat(local);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(0, 4);
  end.writeUInt16LE(0, 6);
  end.writeUInt16LE(files.size, 8);
  end.writeUInt16LE(files.size, 10);
  end.writeUInt32LE(centralBytes.length, 12);
  end.writeUInt32LE(localBytes.length, 16);
  end.writeUInt16LE(0, 20);
  return Buffer.concat([localBytes, centralBytes, end]);
}

async function validateInput(inputDir, iconPath, architecture) {
  const target = targets[architecture];
  if (!target) fail("architecture must be x86_64 or arm64");
  const required = [...commonRequired, target.receipt];
  const rootStat = await lstat(inputDir);
  if (!rootStat.isDirectory() || rootStat.isSymbolicLink()) fail("input directory must be a real directory");
  const names = (await readdir(inputDir)).sort();
  if (JSON.stringify(names) !== JSON.stringify([...required].sort())) {
    fail(`input inventory must contain exactly: ${required.join(", ")}`);
  }
  const files = {};
  const contents = new Map();
  for (const name of required) {
    const path = join(inputDir, name);
    const stat = await lstat(path);
    if (!stat.isFile() || stat.isSymbolicLink() || stat.nlink !== 1) fail(`${name} must be a regular single-link file`);
    const bytes = await readFile(path);
    files[name] = { bytes: bytes.length, sha256: sha256(bytes) };
    contents.set(name, bytes);
  }
  for (const name of ["gpui-migration.exe", "butter-paper-pdf-worker.exe", "butter-paper-signature-phone.exe"]) {
    validatePe64(contents.get(name), target, name, false);
  }
  validatePe64(contents.get("pdfium.dll"), target, "pdfium.dll", true);
  const iconStat = await lstat(iconPath).catch(() => null);
  if (!iconStat || !iconStat.isFile() || iconStat.isSymbolicLink() || iconStat.nlink !== 1) fail("app icon must be a regular single-link ICO file");
  const iconBytes = await readFile(iconPath);
  if (!validateIco(iconBytes)) fail("app icon must be a valid ICO file");
  files["butter-paper.ico"] = { bytes: iconBytes.length, sha256: sha256(iconBytes) };
  contents.set("butter-paper.ico", iconBytes);
  const receipt = JSON.parse(contents.get(target.receipt).toString("utf8"));
  const library = receipt.library;
  if (
    receipt.schemaVersion !== 1 || receipt.purpose !== "production-distribution" ||
    receipt.productionApproved !== true || receipt.target !== target.rust ||
    library?.path !== "pdfium.dll" || library.bytes !== files["pdfium.dll"].bytes ||
    library.sha256 !== files["pdfium.dll"].sha256 || !/^[0-9a-f]{40}$/.test(receipt.source?.revision ?? "") ||
    typeof receipt.build?.provenance !== "string" || !receipt.build.provenance.trim() ||
    typeof receipt.redistributionReview?.reference !== "string" || !receipt.redistributionReview.reference.trim()
  ) fail("missing or invalid production Windows PDFium staging receipt; refusing to package");
  const readme = contents.get("README.md").toString("utf8");
  if (!/runtime dependencies/i.test(readme) || !/visual c\+\+|msvc runtime/i.test(readme)) {
    fail("README.md must document Windows runtime dependencies, including the MSVC runtime");
  }
  if (!contents.get("THIRD_PARTY_NOTICES.md").toString("utf8").trim()) fail("THIRD_PARTY_NOTICES.md must not be empty");
  return { files, contents };
}

export async function packageWindowsProduction({ inputDir, iconPath, outputDir, version, revision, architecture = "x86_64" }) {
  if (!/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(version ?? "")) fail("version must be a release version");
  if (!/^[0-9a-f]{40}$/.test(revision ?? "")) fail("revision must be a full lowercase Git commit");
  inputDir = resolve(inputDir);
  outputDir = resolve(outputDir);
  const outputFromInput = relative(inputDir, outputDir);
  if (outputFromInput === "" || (!isAbsolute(outputFromInput) && outputFromInput !== ".." && !outputFromInput.startsWith(`..${process.platform === "win32" ? "\\" : "/"}`))) {
    fail("output directory must be outside the input directory");
  }
  const target = targets[architecture];
  if (!target) fail("architecture must be x86_64 or arm64");
  if (!/^[A-Za-z0-9.-]+$/.test(version)) fail("version must use safe release characters for Windows registration");
  if (typeof iconPath !== "string" || !iconPath) fail("an app .ico input is required");
  const { files, contents } = await validateInput(inputDir, resolve(iconPath), architecture);
  const receiptBytes = contents.get(target.receipt);
  const manifest = {
    schemaVersion: 1,
    product: "Butter Paper",
    target: target.rust,
    version,
    sourceRevision: revision,
    pdfiumReceiptSha256: sha256(receiptBytes),
    files,
  };
  const archiveFiles = new Map(contents);
  archiveFiles.set("install.ps1", Buffer.from(installScript({ version, architecture, files })));
  archiveFiles.set("uninstall.ps1", Buffer.from(uninstallScript({ version, architecture })));
  for (const name of ["install.ps1", "uninstall.ps1"]) {
    const bytes = archiveFiles.get(name);
    files[name] = { bytes: bytes.length, sha256: sha256(bytes) };
  }
  archiveFiles.set("MANIFEST.json", Buffer.from(`${JSON.stringify(manifest, null, 2)}\n`));
  const archiveBytes = deterministicZip(archiveFiles);
  await mkdir(outputDir, { recursive: true });
  const archive = join(outputDir, `butter-paper-windows-${architecture}-${version}.zip`);
  await writeFile(archive, archiveBytes, { flag: "wx", mode: 0o644 });
  return { archive, sha256: sha256(archiveBytes), bytes: archiveBytes.length, manifest };
}

async function main(args) {
  if (args.length !== 12 || args[0] !== "--input" || args[2] !== "--icon" || args[4] !== "--output" || args[6] !== "--version" || args[8] !== "--revision" || args[10] !== "--architecture") {
    fail("usage: package-windows-x86_64-production.mjs --input DIR --icon APP.ico --output DIR --version VERSION --revision GIT_SHA --architecture x86_64|arm64");
  }
  const result = await packageWindowsProduction({ inputDir: args[1], iconPath: args[3], outputDir: args[5], version: args[7], revision: args[9], architecture: args[11] });
  console.log(JSON.stringify(result, null, 2));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2)).catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
