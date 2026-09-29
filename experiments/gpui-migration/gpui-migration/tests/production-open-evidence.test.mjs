import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  readProductionOpenEvidence,
  stableRecoveryStoreRoot,
  tryReadProductionOpenEvidence,
} from "../scripts/production-open-evidence.mjs";

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");
const fixtureBytes = Buffer.from("%PDF-1.7\nfixture\n%%EOF\n");
const timelineBytes = Buffer.from("{\"version\":1,\"entries\":[]}");
const documentId = "12".repeat(16);

async function createStore(root, fixturePath, platform = "linux") {
  const objects = join(root, "objects"), heads = join(root, "heads");
  await Promise.all([mkdir(objects, { recursive: true }), mkdir(heads, { recursive: true })]);
  const baseHash = sha256(fixtureBytes), timelineHash = sha256(timelineBytes);
  const sourcePath = platform === "win32"
    ? Buffer.from(fixturePath, "utf16le").toString("hex")
    : Buffer.from(fixturePath).toString("hex");
  const head = {
    version: 2,
    document_id: documentId,
    path_encoding: platform === "win32" ? "windows-utf16le" : "unix-bytes",
    source_path: sourcePath,
    source_kind: "opened",
    source_sha256: baseHash,
    base_object_sha256: baseHash,
    timeline_object_sha256: timelineHash,
    checkpoint_sequence: 1,
    current_revision: 0,
    saved_revision: 0,
    requires_save_as: false,
  };
  await Promise.all([
    writeFile(join(objects, baseHash), fixtureBytes),
    writeFile(join(objects, timelineHash), timelineBytes),
    writeFile(join(heads, documentId + ".json"), JSON.stringify(head)),
  ]);
  await writeFile(join(root, "index.json"), JSON.stringify({ version: 1, active: [documentId] }));
  return { head, baseHash, timelineHash };
}

test("derives stable recovery stores for each supported production platform", () => {
  assert.equal(
    stableRecoveryStoreRoot({ platform: "darwin", effectiveHome: "/private/disposable-home" }),
    "/private/disposable-home/Library/Application Support/com.butterpaper.desktop/native-v1/session-state/document-recovery-v1",
  );
  assert.equal(
    stableRecoveryStoreRoot({ platform: "linux", xdgDataHome: "/var/tmp/bp-data" }),
    "/var/tmp/bp-data/com.butterpaper.desktop/native-v1/session-state/document-recovery-v1",
  );
  assert.match(
    stableRecoveryStoreRoot({ platform: "win32", appData: "/var/tmp/AppData/Roaming" }),
    /AppData\/Roaming\/com\.butterpaper\.desktop\/native-v1\/session-state\/document-recovery-v1$/,
  );
  assert.throws(() => stableRecoveryStoreRoot({ platform: "freebsd" }), /unsupported/);
});

test("accepts an exact clean opened-document checkpoint and content-addressed objects", async (t) => {
  const scratch = await mkdtemp(join(tmpdir(), "bp-open-evidence-"));
  t.after(() => rm(scratch, { recursive: true, force: true }));
  const fixturePath = join(scratch, "fixture.pdf"), store = join(scratch, "store");
  await writeFile(fixturePath, fixtureBytes);
  const expected = await createStore(store, fixturePath);
  const evidence = await readProductionOpenEvidence({ recoveryStoreRoot: store, fixturePath, fixtureBytes, platform: "linux" });
  assert.equal(evidence.documentId, documentId);
  assert.equal(evidence.sourceSha256, expected.baseHash);
  assert.equal(evidence.timelineObjectSha256, expected.timelineHash);
  assert.equal(evidence.currentRevision, 0);
  assert.equal(evidence.requiresSaveAs, false);
});

test("returns not-ready only for missing publication and fails closed after publication", async (t) => {
  const scratch = await mkdtemp(join(tmpdir(), "bp-open-evidence-failure-"));
  t.after(() => rm(scratch, { recursive: true, force: true }));
  const fixturePath = join(scratch, "fixture.pdf"), store = join(scratch, "store");
  await writeFile(fixturePath, fixtureBytes);
  assert.equal(await tryReadProductionOpenEvidence({ recoveryStoreRoot: store, fixturePath, fixtureBytes, platform: "linux" }), null);
  const { head } = await createStore(store, fixturePath);
  await writeFile(join(store, "heads", documentId + ".json"), JSON.stringify({ ...head, current_revision: 1 }));
  await assert.rejects(
    tryReadProductionOpenEvidence({ recoveryStoreRoot: store, fixturePath, fixtureBytes, platform: "linux" }),
    /clean revision zero/,
  );
});

test("rejects a mismatched source path, object bytes, and unknown metadata", async (t) => {
  const scratch = await mkdtemp(join(tmpdir(), "bp-open-evidence-mutation-"));
  t.after(() => rm(scratch, { recursive: true, force: true }));
  const fixturePath = join(scratch, "fixture.pdf"), store = join(scratch, "store");
  await writeFile(fixturePath, fixtureBytes);
  const { head, baseHash } = await createStore(store, fixturePath);

  await writeFile(join(store, "heads", documentId + ".json"), JSON.stringify({ ...head, source_path: Buffer.from(join(scratch, "other.pdf")).toString("hex") }));
  await assert.rejects(readProductionOpenEvidence({ recoveryStoreRoot: store, fixturePath, fixtureBytes, platform: "linux" }), /exact fixture/);

  await writeFile(join(store, "heads", documentId + ".json"), JSON.stringify(head));
  await writeFile(join(store, "objects", baseHash), Buffer.from("damaged"));
  await assert.rejects(readProductionOpenEvidence({ recoveryStoreRoot: store, fixturePath, fixtureBytes, platform: "linux" }), /exact fixture bytes/);

  await writeFile(join(store, "index.json"), JSON.stringify({ version: 1, active: [documentId], extra: true }));
  await assert.rejects(readProductionOpenEvidence({ recoveryStoreRoot: store, fixturePath, fixtureBytes, platform: "linux" }), /unknown fields/);
});

test("matches Windows recovery paths case-insensitively with the native UTF-16 encoding", async (t) => {
  const scratch = await mkdtemp(join(tmpdir(), "bp-open-evidence-windows-"));
  t.after(() => rm(scratch, { recursive: true, force: true }));
  const store = join(scratch, "store");
  const recorded = "C:\\Smoke\\Fixture.pdf";
  await createStore(store, recorded, "win32");
  const evidence = await readProductionOpenEvidence({ recoveryStoreRoot: store, fixturePath: "c:\\smoke\\fixture.pdf", fixtureBytes, platform: "win32" });
  assert.equal(evidence.documentId, documentId);
});
