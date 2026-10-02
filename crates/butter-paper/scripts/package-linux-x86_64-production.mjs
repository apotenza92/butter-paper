#!/usr/bin/env node

import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import {
  lstat,
  mkdir,
  readFile,
  readdir,
  writeFile,
} from "node:fs/promises";
import { basename, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const commonRequired = [
  "gpui-migration",
  "butter-paper-pdf-worker",
  "butter-paper-signature-phone",
  "libpdfium.so",
  "README.md",
  "THIRD_PARTY_NOTICES.md",
  "PHONE_HELPER_THIRD_PARTY_NOTICES.md",
  "QRCP_LICENSE",
  "SIGNATURE_PAD_LICENSE",
];

const targets = {
  x86_64: { rust: "x86_64-unknown-linux-gnu", receipt: "production-pdfium-linux-x86_64.json" },
  arm64: { rust: "aarch64-unknown-linux-gnu", receipt: "production-pdfium-linux-arm64.json" },
};

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function octal(value, width) {
  return `${value.toString(8).padStart(width - 1, "0")}\0`;
}

function tarHeader(name, size, mode, type = "0") {
  const header = Buffer.alloc(512);
  header.write(name, 0, 100, "utf8");
  header.write(octal(mode, 8), 100, 8, "ascii");
  header.write(octal(0, 8), 108, 8, "ascii");
  header.write(octal(0, 8), 116, 8, "ascii");
  header.write(octal(size, 12), 124, 12, "ascii");
  header.write(octal(0, 12), 136, 12, "ascii");
  header.fill(0x20, 148, 156);
  header.write(type, 156, 1, "ascii");
  header.write("ustar\0", 257, 6, "ascii");
  header.write("00", 263, 2, "ascii");
  const checksum = header.reduce((total, byte) => total + byte, 0);
  header.write(octal(checksum, 8), 148, 8, "ascii");
  return header;
}

function deterministicTar(files, rootName) {
  const chunks = [];
  chunks.push(tarHeader(`${rootName}/`, 0, 0o755, "5"));
  for (const [name, bytes] of [...files].sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)) {
    const mode = name === "gpui-migration" || name === "butter-paper-pdf-worker" || name === "butter-paper-signature-phone" || name.endsWith(".sh") ? 0o755 : 0o644;
    chunks.push(tarHeader(`${rootName}/${name}`, bytes.length, mode));
    chunks.push(bytes);
    const padding = (512 - (bytes.length % 512)) % 512;
    if (padding) chunks.push(Buffer.alloc(padding));
  }
  chunks.push(Buffer.alloc(1024));
  return Buffer.concat(chunks);
}

function fail(message) {
  throw new Error(message);
}

async function readProductIcon(path) {
  if (typeof path !== "string" || !path) fail("a product PNG icon input is required");
  await regularSingleLink(path, "product PNG icon");
  const bytes = await readFile(path);
  if (bytes.length < 45 || !bytes.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]))) fail("product icon must be a valid 1024x1024 PNG");
  let offset = 8;
  let first = true;
  let sawImageData = false;
  let ended = false;
  while (offset + 12 <= bytes.length) {
    const length = bytes.readUInt32BE(offset);
    const end = offset + 12 + length;
    if (end > bytes.length) fail("product icon PNG chunk is truncated");
    const type = bytes.toString("ascii", offset + 4, offset + 8);
    if (first && (type !== "IHDR" || length !== 13 || bytes.readUInt32BE(offset + 8) !== 1024 || bytes.readUInt32BE(offset + 12) !== 1024)) fail("product icon must be a valid 1024x1024 PNG");
    first = false;
    if (type === "IDAT") sawImageData = true;
    let crc = 0xffffffff;
    for (let i = offset + 4; i < end - 4; i += 1) {
      crc ^= bytes[i];
      for (let bit = 0; bit < 8; bit += 1) crc = (crc >>> 1) ^ (0xedb88320 & -(crc & 1));
    }
    if (((crc ^ 0xffffffff) >>> 0) !== bytes.readUInt32BE(end - 4)) fail("product icon PNG has an invalid chunk checksum");
    if (type === "IEND") {
      if (length !== 0 || end !== bytes.length) fail("product icon PNG has trailing or invalid IEND data");
      ended = true;
      break;
    }
    offset = end;
  }
  if (first || !sawImageData || !ended) fail("product icon must be a complete PNG image");
  return bytes;
}

function desktopIntegration(version, architecture) {
  const desktop = "[Desktop Entry]\nType=Application\nName=Butter Paper\nComment=Read and annotate PDF documents\nExec=butter-paper-gpui %F\nIcon=butter-paper\nTerminal=false\nCategories=Office;Viewer;\nMimeType=application/pdf;\nStartupNotify=true\n";
  const common = ["#!/bin/sh", "set -eu", "fail() { printf '%s\\n' \"$*\" >&2; exit 1; }", "[ \"$(id -u)\" -ne 0 ] || fail 'Run this script as your user, not root.'", "[ -n \"${HOME:-}\" ] || fail 'HOME must be set.'", "case \"$HOME\" in /*) ;; *) fail 'HOME must be an absolute path.' ;; esac", "case \"$HOME\" in *'/../'*|*/..|../*|..) fail 'Unsafe HOME.' ;; esac", "data=${XDG_DATA_HOME:-\"$HOME/.local/share\"}", "case \"$data\" in /*) ;; *) fail 'XDG_DATA_HOME must be an absolute path.' ;; esac", "case \"$data\" in *'/../'*|*/..|../*|..) fail 'Unsafe XDG_DATA_HOME.' ;; esac", "bin=\"$HOME/.local/bin\"", `target=\"$data/butter-paper/${version}\"`, "launcher=\"$bin/butter-paper-gpui\"", "desktop=\"$data/applications/butter-paper.desktop\"", "icon=\"$data/icons/hicolor/1024x1024/apps/butter-paper.png\"", "root=$(CDPATH= cd -- \"$(dirname -- \"$0\")\" && pwd -P) || fail 'Cannot locate package directory.'"];
  const launcherBuild = ["make_launcher() {", "  quoted=$(printf '%s' \"$target/gpui-migration\" | sed \"s/'/'\\\\''/g\")", "  printf '#!/bin/sh\\nexec '\\\''%s'\\\'' \"$@\"\\n' \"$quoted\" > \"$1\"", "  chmod 0755 \"$1\"", "}"];
  const desktopBuild = ["make_desktop() {", "  BP_EXEC=\"$launcher\" awk 'function esc(s, o,i,c) { for (i=1;i<=length(s);i++) { c=substr(s,i,1); if (c==\"\\\\\" || c==\"\\\"\") o=o \"\\\\\" c; else if (c==\"%\") o=o \"%%\"; else o=o c } return o } /^Exec=/ { printf \"Exec=\\\"%s\\\" %%F\\n\", esc(ENVIRON[\"BP_EXEC\"]); next } { print }' \"$root/butter-paper.desktop\" > \"$1\"", "}"];
  const payloadNames = [...commonRequired, targets[architecture].receipt, "butter-paper.png", "butter-paper.desktop", "install-user.sh", "uninstall-user.sh", "MANIFEST.json"];
  const copyPayload = ["for name in " + payloadNames.map((name) => `'${name}'`).join(" "), "do", "  if [ -f \"$root/$name\" ] && [ ! -L \"$root/$name\" ]; then cp -p -- \"$root/$name\" \"$stage/$name\"; else fail 'Package payload is missing or unsafe.'; fi", "done"];
  const rollback = [
    "rollback() {",
    "  result=$?",
    "  trap - EXIT HUP INT TERM",
    "  set +e",
    "  changed=0",
    "  if [ -n \"${stage:-}\" ] && [ -d \"$stage\" ]; then rm -rf -- \"$stage\"; fi",
    "  if [ \"${owned_icon:-0}\" -eq 1 ] && [ -f \"${icon_tmp:-}\" ] && [ \"$icon_tmp\" -ef \"$icon\" ] && cmp -s \"$root/butter-paper.png\" \"$icon\"; then rm -f -- \"$icon\"; changed=1; fi",
    "  if [ \"${owned_desktop:-0}\" -eq 1 ] && [ -f \"${desktop_tmp:-}\" ] && [ \"$desktop_tmp\" -ef \"$desktop\" ] && cmp -s \"$desktop_tmp\" \"$desktop\"; then rm -f -- \"$desktop\"; changed=1; fi",
    "  if [ \"${owned_launcher:-0}\" -eq 1 ] && [ -f \"${launcher_tmp:-}\" ] && [ \"$launcher_tmp\" -ef \"$launcher\" ] && cmp -s \"$launcher_tmp\" \"$launcher\"; then rm -f -- \"$launcher\"; changed=1; fi",
    "  if [ \"${owned_target:-0}\" -eq 1 ] || { [ -n \"${owner_token:-}\" ] && [ -f \"$target/.bp-owner\" ] && [ \"$(cat -- \"$target/.bp-owner\")\" = \"$owner_token\" ]; }; then",
    "    for name in " + payloadNames.map((name) => `'${name}'`).join(" "),
    "    do if cmp -s \"$root/$name\" \"$target/$name\"; then rm -f -- \"$target/$name\"; changed=1; fi; done",
    "    rm -f -- \"$target/.bp-owner\"",
    "    rmdir -- \"$target\" 2>/dev/null || :",
    "  fi",
    "  if [ \"$changed\" -eq 1 ]; then",
    "    if command -v update-desktop-database >/dev/null 2>&1 && [ -d \"$data/applications\" ]; then update-desktop-database \"$data/applications\" || :; fi",
    "    if command -v update-mime-database >/dev/null 2>&1 && [ -d \"$data/mime\" ]; then update-mime-database \"$data/mime\" || :; fi",
    "  fi",
    "  rm -f -- \"${launcher_tmp:-}\" \"${desktop_tmp:-}\" \"${icon_tmp:-}\"",
    "  exit \"$result\"",
    "}",
  ];
  const install = [
    ...common,
    "[ -x \"$root/gpui-migration\" ] || fail 'Package executable is missing.'",
    "[ -f \"$root/MANIFEST.json\" ] && [ -f \"$root/butter-paper.desktop\" ] && [ -f \"$root/butter-paper.png\" ] || fail 'Package integration files are missing.'",
    "[ ! -e \"$target\" ] && [ ! -L \"$target\" ] && [ ! -e \"$launcher\" ] && [ ! -L \"$launcher\" ] && [ ! -e \"$desktop\" ] && [ ! -L \"$desktop\" ] && [ ! -e \"$icon\" ] && [ ! -L \"$icon\" ] || fail 'Butter Paper files already exist; remove the owned installation first.'",
    "mkdir -p \"$data/butter-paper\" \"$data/applications\" \"$data/icons/hicolor/1024x1024/apps\" \"$bin\"",
    "stage= launcher_tmp= desktop_tmp= icon_tmp= owner_token=",
    "owned_target=0 owned_launcher=0 owned_desktop=0 owned_icon=0",
    ...rollback,
    "trap rollback EXIT",
    "trap 'exit 1' HUP INT TERM",
    "stage=$(mktemp -d \"$data/butter-paper/.install-XXXXXX\")",
    ...copyPayload,
    "owner_token=${stage##*/}",
    "printf '%s' \"$owner_token\" > \"$stage/.bp-owner\"",
    "# Atomic mv -T -- \"$stage\" \"$target\" with -n prevents replacing a raced-in path.",
    "mv -T -n -- \"$stage\" \"$target\"",
    "[ ! -e \"$stage\" ] || fail 'Versioned install path appeared during installation.'",
    "owned_target=1",
    "rm -f -- \"$target/.bp-owner\"",
    "launcher_tmp=$(mktemp \"$bin/.butter-paper-launcher-XXXXXX\")",
    ...launcherBuild,
    "make_launcher \"$launcher_tmp\"",
    "ln -- \"$launcher_tmp\" \"$launcher\"",
    "owned_launcher=1",
    "desktop_tmp=$(mktemp \"$data/applications/.butter-paper-desktop-XXXXXX\")",
    ...desktopBuild,
    "make_desktop \"$desktop_tmp\"",
    "chmod 0644 \"$desktop_tmp\"",
    "ln -- \"$desktop_tmp\" \"$desktop\"",
    "owned_desktop=1",
    "icon_tmp=$(mktemp \"$data/icons/hicolor/1024x1024/apps/.butter-paper-icon-XXXXXX\")",
    "cp -p -- \"$root/butter-paper.png\" \"$icon_tmp\"",
    "ln -- \"$icon_tmp\" \"$icon\"",
    "owned_icon=1",
    "if command -v update-desktop-database >/dev/null 2>&1; then update-desktop-database \"$data/applications\"; fi",
    "if command -v update-mime-database >/dev/null 2>&1 && [ -d \"$data/mime\" ]; then update-mime-database \"$data/mime\"; fi",
    "rm -f -- \"$launcher_tmp\" \"$desktop_tmp\" \"$icon_tmp\"",
    "trap - EXIT HUP INT TERM",
    "printf '%s\\n' 'Butter Paper installed for this user. Select it as the default PDF application in your desktop settings if desired.'",
    "",
  ].join("\n");
  const uninstall = [
    ...common,
    "[ -f \"$root/MANIFEST.json\" ] && [ ! -L \"$root/MANIFEST.json\" ] || fail 'Package identity manifest is missing or unsafe.'",
    "[ -d \"$target\" ] && [ ! -L \"$target\" ] || fail 'Owned versioned installation was not found.'",
    "actual_count=0",
    "for entry in \"$target\"/* \"$target\"/.[!.]* \"$target\"/..?*",
    "do",
    "  [ -e \"$entry\" ] || [ -L \"$entry\" ] || continue",
    "  case \"$(basename -- \"$entry\")\" in " + payloadNames.map((name) => "'" + name + "'").join("|") + ") ;; *) fail 'Versioned install directory contains unrelated files; preserved it.' ;; esac",
    "  [ -f \"$entry\" ] && [ ! -L \"$entry\" ] || fail 'Installed payload contains an unsafe entry.'",
    "  actual_count=$((actual_count + 1))",
    "done",
    "[ \"$actual_count\" -eq " + payloadNames.length + " ] || fail 'Installed payload inventory does not match this package.'",
    "for name in " + payloadNames.map((name) => "'" + name + "'").join(" "),
    "do",
    "  [ -f \"$root/$name\" ] && [ ! -L \"$root/$name\" ] || fail 'Package payload is missing or unsafe.'",
    "  cmp -s \"$root/$name\" \"$target/$name\" || fail 'Installed payload differs from the package; preserving it.'",
    "done",
    "launcher_tmp= desktop_tmp= tmpdir=",
    "tmpdir=$(printenv TMPDIR || true)",
    "[ -n \"$tmpdir\" ] || tmpdir=/tmp",
    "trap 'rm -f -- \"$launcher_tmp\" \"$desktop_tmp\"' EXIT",
    "trap 'exit 1' HUP INT TERM",
    "launcher_tmp=$(mktemp \"$tmpdir/butter-paper-launcher-XXXXXX\")",
    ...launcherBuild,
    "make_launcher \"$launcher_tmp\"",
    "cmp -s \"$launcher_tmp\" \"$launcher\" || fail 'Launcher identity does not match this installation.'",
    "desktop_tmp=$(mktemp \"$tmpdir/butter-paper-desktop-XXXXXX\")",
    ...desktopBuild,
    "make_desktop \"$desktop_tmp\"",
    "cmp -s \"$desktop_tmp\" \"$desktop\" || fail 'Desktop entry identity does not match this installation.'",
    "cmp -s \"$root/butter-paper.png\" \"$icon\" || fail 'Icon identity does not match this installation.'",
    "rm -f -- \"$launcher_tmp\" \"$desktop_tmp\"",
    "trap - EXIT HUP INT TERM",
    "for name in " + payloadNames.map((name) => "'" + name + "'").join(" "),
    "do rm -f -- \"$target/$name\"; done",
    "rmdir -- \"$target\" || fail 'Versioned install directory contains unrelated files; preserved it.'",
    "rm -- \"$launcher\" \"$desktop\" \"$icon\"",
    "if command -v update-desktop-database >/dev/null 2>&1 && [ -d \"$data/applications\" ]; then update-desktop-database \"$data/applications\"; fi",
    "if command -v update-mime-database >/dev/null 2>&1 && [ -d \"$data/mime\" ]; then update-mime-database \"$data/mime\"; fi",
    "printf '%s\\n' 'Butter Paper desktop integration removed for this user.'",
    "",
  ].join("\n");
  return new Map([["butter-paper.desktop", Buffer.from(desktop)], ["install-user.sh", Buffer.from(install)], ["uninstall-user.sh", Buffer.from(uninstall)]]);
}

async function regularSingleLink(path, label) {
  const stat = await lstat(path);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.nlink !== 1) {
    fail(`${label} must be a regular single-link file`);
  }
  return stat;
}

async function validateInput(inputDir, architecture) {
  const target = targets[architecture];
  if (!target) fail("architecture must be x86_64 or arm64");
  const required = [...commonRequired, target.receipt];
  const rootStat = await lstat(inputDir);
  if (!rootStat.isDirectory() || rootStat.isSymbolicLink()) {
    fail("input directory must be a real directory");
  }
  const names = (await readdir(inputDir)).sort();
  if (JSON.stringify(names) !== JSON.stringify([...required].sort())) {
    fail(`input inventory must contain exactly: ${required.join(", ")}`);
  }
  const files = {};
  for (const name of required) {
    const path = join(inputDir, name);
    await regularSingleLink(path, name);
    const bytes = await readFile(path);
    files[name] = { bytes: bytes.length, sha256: sha256(bytes) };
  }
  for (const executable of required.slice(0, 3)) {
    if ((await lstat(join(inputDir, executable))).mode & 0o111) {
      // Executable input is expected; normalise it in the output below.
    } else fail(`${executable} must be executable`);
  }
  const receipt = JSON.parse(
    await readFile(join(inputDir, target.receipt), "utf8"),
  );
  if (
    receipt.schemaVersion !== 1 ||
    receipt.purpose !== "production-distribution" ||
    receipt.productionApproved !== true ||
    receipt.target !== target.rust ||
    receipt.library?.path !== "libpdfium.so" ||
    receipt.library?.bytes !== files["libpdfium.so"].bytes ||
    receipt.library?.sha256 !== files["libpdfium.so"].sha256 ||
    !/^[0-9a-f]{40}$/.test(receipt.source?.revision ?? "") ||
    !receipt.build?.provenance || typeof receipt.build.provenance !== "string" ||
    !receipt.redistributionReview?.reference ||
    typeof receipt.redistributionReview.reference !== "string"
  ) {
    fail("missing or invalid production PDFium staging receipt; refusing to package");
  }
  const readme = await readFile(join(inputDir, "README.md"), "utf8");
  if (!/runtime dependencies/i.test(readme) || !/libc|glibc/i.test(readme)) {
    fail("README.md must include the Linux runtime dependencies, including glibc");
  }
  return { files, receipt };
}

export async function packageLinuxProduction({ inputDir, outputDir, version, revision, iconPath, architecture = "x86_64" }) {
  if (!/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(version ?? "")) {
    fail("version must be a release version");
  }
  if (!/^[0-9a-f]{40}$/.test(revision ?? "")) fail("revision must be a full lowercase Git commit");
  inputDir = resolve(inputDir);
  outputDir = resolve(outputDir);
  if (outputDir === inputDir || outputDir.startsWith(`${inputDir}/`)) {
    fail("output directory must be outside the input directory");
  }
  const target = targets[architecture];
  if (!target) fail("architecture must be x86_64 or arm64");
  const { files } = await validateInput(inputDir, architecture);
  const icon = await readProductIcon(iconPath);
  await mkdir(outputDir, { recursive: true });
  const staging = join(outputDir, `butter-paper-linux-${architecture}-${version}`);
  const archive = `${staging}.tar.xz`;
  try {
    await mkdir(staging);
  } catch {
    fail(`package staging path already exists: ${staging}`);
  }
  const archiveFiles = new Map();
  for (const name of [...commonRequired, target.receipt]) archiveFiles.set(name, await readFile(join(inputDir, name)));
  for (const [name, bytes] of desktopIntegration(version, architecture)) archiveFiles.set(name, bytes);
  archiveFiles.set("butter-paper.png", icon);
  files["butter-paper.png"] = { bytes: icon.length, sha256: sha256(icon) };
  for (const name of ["butter-paper.desktop", "install-user.sh", "uninstall-user.sh"]) {
    const bytes = archiveFiles.get(name);
    files[name] = { bytes: bytes.length, sha256: sha256(bytes) };
  }
  const manifest = {
    schemaVersion: 1,
    product: "Butter Paper",
    target: target.rust,
    version,
    sourceRevision: revision,
    pdfiumReceiptSha256: sha256(await readFile(join(inputDir, target.receipt))),
    files,
  };
  archiveFiles.set("MANIFEST.json", Buffer.from(`${JSON.stringify(manifest, null, 2)}\n`));
  const tarBytes = deterministicTar(archiveFiles, basename(staging));
  let archiveBytes;
  try {
    archiveBytes = execFileSync("xz", ["-9e", "--threads=1", "--check=crc64", "--stdout"], { input: tarBytes, maxBuffer: 256 * 1024 * 1024 });
  } catch (error) {
    fail(`deterministic xz compression failed: ${error.message}`);
  }
  await writeFile(archive, archiveBytes, { flag: "wx", mode: 0o644 });
  for (const [name, bytes] of archiveFiles) {
    await writeFile(join(staging, name), bytes, { mode: name === "gpui-migration" || name === "butter-paper-pdf-worker" || name === "butter-paper-signature-phone" || name.endsWith(".sh") ? 0o755 : 0o644, flag: "wx" });
  }
  return { archive, sha256: sha256(archiveBytes), bytes: archiveBytes.length, manifest };
}

async function main(args) {
  if ((args.length !== 10 && args.length !== 12) || args[0] !== "--input" || args[2] !== "--output" || args[4] !== "--version" || args[6] !== "--revision" || args[8] !== "--icon") {
    fail("usage: package-linux-x86_64-production.mjs --input DIR --output DIR --version VERSION --revision GIT_SHA --icon PNG [--architecture x86_64|arm64]");
  }
  let architecture = "x86_64";
  if (args.length === 12 && args[10] === "--architecture") architecture = args[11];
  else if (args.length !== 10) fail("invalid architecture option");
  const result = await packageLinuxProduction({ inputDir: args[1], outputDir: args[3], version: args[5], revision: args[7], iconPath: args[9], architecture });
  console.log(JSON.stringify(result, null, 2));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2)).catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
