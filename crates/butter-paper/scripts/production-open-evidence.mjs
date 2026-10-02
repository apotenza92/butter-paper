import { createHash } from "node:crypto";
import { lstat, readFile, realpath } from "node:fs/promises";
import { join, resolve, win32 } from "node:path";

const MAX_INDEX_BYTES = 16 * 1024;
const MAX_HEAD_BYTES = 128 * 1024;
const MAX_BASE_PDF_BYTES = 2 * 1024 * 1024 * 1024;
const MAX_TIMELINE_BYTES = 256 * 1024 * 1024;
const HASH = /^[0-9a-f]{64}$/;
const DOCUMENT_ID = /^[0-9a-f]{32}$/;
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");
const fail = (message) => { throw new Error(message); };

function assert(condition, message) {
  if (!condition) fail(message);
}

function exactKeys(value, keys, label) {
  assert(value && typeof value === "object" && !Array.isArray(value), label + " must be a JSON object");
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  assert(JSON.stringify(actual) === JSON.stringify(expected), label + " has missing or unknown fields");
}

async function boundedRegularFile(path, maxBytes, label) {
  const stat = await lstat(path);
  assert(stat.isFile() && !stat.isSymbolicLink() && stat.nlink === 1, label + " must be a regular single-link file");
  assert(stat.size > 0 && stat.size <= maxBytes, label + " exceeds its bounded size contract");
  return readFile(path);
}

async function strictJson(path, maxBytes, label) {
  const bytes = await boundedRegularFile(path, maxBytes, label);
  try {
    return JSON.parse(bytes.toString("utf8"));
  } catch {
    fail(label + " must contain valid JSON");
  }
}

export function stableRecoveryStoreRoot({ platform, effectiveHome, appData, xdgDataHome }) {
  if (platform === "darwin") {
    assert(typeof effectiveHome === "string" && effectiveHome, "macOS recovery evidence requires the effective account home");
    return join(resolve(effectiveHome), "Library", "Application Support", "com.butterpaper.desktop", "native-v1", "session-state", "document-recovery-v1");
  }
  if (platform === "win32") {
    assert(typeof appData === "string" && appData, "Windows recovery evidence requires APPDATA");
    return join(resolve(appData), "com.butterpaper.desktop", "native-v1", "session-state", "document-recovery-v1");
  }
  if (platform === "linux") {
    assert(typeof xdgDataHome === "string" && xdgDataHome, "Linux recovery evidence requires XDG_DATA_HOME");
    return join(resolve(xdgDataHome), "com.butterpaper.desktop", "native-v1", "session-state", "document-recovery-v1");
  }
  fail("unsupported production recovery evidence platform: " + platform);
}

function decodeHex(value, label) {
  assert(typeof value === "string" && value.length % 2 === 0 && /^[0-9a-f]*$/.test(value), label + " must be canonical lowercase hexadecimal");
  return Buffer.from(value, "hex");
}

function windowsPathIdentity(value) {
  let path = win32.normalize(value).replaceAll("/", "\\");
  if (path.toLowerCase().startsWith("\\\\?\\unc\\")) path = "\\\\" + path.slice(8);
  else if (path.toLowerCase().startsWith("\\\\?\\")) path = path.slice(4);
  return path.toLowerCase();
}

export function decodeRecoverySourcePath(head, platform) {
  if (platform === "win32") {
    assert(head.path_encoding === "windows-utf16le", "recovery source path has the wrong Windows encoding");
    const bytes = decodeHex(head.source_path, "recovery source path");
    assert(bytes.length > 0 && bytes.length % 2 === 0, "recovery Windows source path is empty or truncated");
    return bytes.toString("utf16le");
  }
  assert(head.path_encoding === "unix-bytes", "recovery source path has the wrong Unix encoding");
  const bytes = decodeHex(head.source_path, "recovery source path");
  assert(bytes.length > 0, "recovery Unix source path is empty");
  return bytes;
}

export async function readProductionOpenEvidence({ recoveryStoreRoot, fixturePath, fixtureBytes, platform }) {
  assert(Buffer.isBuffer(fixtureBytes) && fixtureBytes.length > 0, "fixture bytes are required for recovery evidence");
  const storeRoot = resolve(recoveryStoreRoot);
  const index = await strictJson(join(storeRoot, "index.json"), MAX_INDEX_BYTES, "recovery index");
  exactKeys(index, ["version", "active"], "recovery index");
  assert(index.version === 1 && Array.isArray(index.active) && index.active.length === 1, "recovery index must identify exactly one active version-1 document");
  const documentId = index.active[0];
  assert(typeof documentId === "string" && DOCUMENT_ID.test(documentId), "recovery index contains an invalid document id");

  const head = await strictJson(join(storeRoot, "heads", documentId + ".json"), MAX_HEAD_BYTES, "recovery head");
  exactKeys(head, [
    "version", "document_id", "path_encoding", "source_path", "source_kind", "source_sha256",
    "base_object_sha256", "timeline_object_sha256", "checkpoint_sequence", "current_revision",
    "saved_revision", "requires_save_as",
  ], "recovery head");
  assert(head.version === 2 && head.document_id === documentId, "recovery head identity or version is invalid");
  assert(head.source_kind === "opened" && head.requires_save_as === false, "recovery head does not represent a normally opened document");
  assert(Number.isSafeInteger(head.checkpoint_sequence) && head.checkpoint_sequence > 0, "recovery checkpoint sequence is invalid");
  assert(head.current_revision === 0 && head.saved_revision === 0, "initial opened document recovery checkpoint is not clean revision zero");
  for (const [name, value] of [["source", head.source_sha256], ["base object", head.base_object_sha256], ["timeline object", head.timeline_object_sha256]]) {
    assert(typeof value === "string" && HASH.test(value), "recovery " + name + " hash is invalid");
  }

  const decodedPath = decodeRecoverySourcePath(head, platform);
  if (platform === "win32") {
    assert(windowsPathIdentity(decodedPath) === windowsPathIdentity(win32.resolve(fixturePath)), "recovery source path does not identify the exact fixture");
  } else {
    let recordedRealPath;
    try {
      recordedRealPath = await realpath(decodedPath, { encoding: "buffer" });
    } catch {
      fail("recovery source path does not identify the exact fixture");
    }
    const fixtureRealPath = await realpath(fixturePath, { encoding: "buffer" });
    assert(recordedRealPath.equals(fixtureRealPath), "recovery source path does not identify the exact fixture");
  }

  const fixtureSha256 = sha256(fixtureBytes);
  assert(head.source_sha256 === fixtureSha256 && head.base_object_sha256 === fixtureSha256, "recovery source/base hash does not match the exact fixture bytes");
  const baseObject = await boundedRegularFile(join(storeRoot, "objects", head.base_object_sha256), MAX_BASE_PDF_BYTES, "recovery base object");
  assert(baseObject.equals(fixtureBytes) && sha256(baseObject) === head.base_object_sha256, "recovery base object does not contain the exact fixture bytes");
  const timelineObject = await boundedRegularFile(join(storeRoot, "objects", head.timeline_object_sha256), MAX_TIMELINE_BYTES, "recovery timeline object");
  assert(sha256(timelineObject) === head.timeline_object_sha256, "recovery timeline object digest does not match its content address");

  return {
    storeRoot,
    documentId,
    fixturePath: platform === "win32" ? win32.resolve(fixturePath) : resolve(fixturePath),
    sourceSha256: fixtureSha256,
    baseObjectSha256: head.base_object_sha256,
    timelineObjectSha256: head.timeline_object_sha256,
    timelineBytes: timelineObject.length,
    checkpointSequence: head.checkpoint_sequence,
    currentRevision: head.current_revision,
    savedRevision: head.saved_revision,
    requiresSaveAs: head.requires_save_as,
  };
}

export async function tryReadProductionOpenEvidence(options) {
  try {
    return await readProductionOpenEvidence(options);
  } catch (error) {
    if (error?.code === "ENOENT") return null;
    throw error;
  }
}
