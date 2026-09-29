import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  cleanupOwnedProcess,
  emergencyCleanupOwnedProcess,
  parseQpdfRectangleEvidence,
  readRectangleEditEvidence,
  parseProductionTar,
  parseProductionZip,
  assertSidecars,
} from "../scripts/smoke-nonmac-production-package.mjs";

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit += 1)
      crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0);
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function zip(entries) {
  const local = [],
    central = [];
  let offset = 0;
  for (const [name, content] of entries) {
    const n = Buffer.from(name),
      data = Buffer.from(content),
      crc = crc32(data),
      h = Buffer.alloc(30);
    h.writeUInt32LE(0x04034b50, 0);
    h.writeUInt16LE(20, 4);
    h.writeUInt16LE(0x0800, 6);
    h.writeUInt32LE(crc, 14);
    h.writeUInt32LE(data.length, 18);
    h.writeUInt32LE(data.length, 22);
    h.writeUInt16LE(n.length, 26);
    local.push(h, n, data);
    const c = Buffer.alloc(46);
    c.writeUInt32LE(0x02014b50, 0);
    c.writeUInt16LE(0x0314, 4);
    c.writeUInt16LE(20, 6);
    c.writeUInt16LE(0x0800, 8);
    c.writeUInt32LE(crc, 16);
    c.writeUInt32LE(data.length, 20);
    c.writeUInt32LE(data.length, 24);
    c.writeUInt16LE(n.length, 28);
    c.writeUInt32LE(offset, 42);
    central.push(c, n);
    offset += h.length + n.length + data.length;
  }
  const localBytes = Buffer.concat(local),
    centralBytes = Buffer.concat(central),
    end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(entries.length, 8);
  end.writeUInt16LE(entries.length, 10);
  end.writeUInt32LE(centralBytes.length, 12);
  end.writeUInt32LE(localBytes.length, 16);
  return Buffer.concat([localBytes, centralBytes, end]);
}

function octal(value, width) {
  return `${value.toString(8).padStart(width - 1, "0")}\0`;
}
function tarHeader(path, content, type = "0", mode = 0o644) {
  const data = Buffer.from(content),
    h = Buffer.alloc(512);
  h.write(path, 0);
  h.write(octal(mode, 8), 100);
  h.write(octal(0, 8), 108);
  h.write(octal(0, 8), 116);
  h.write(octal(type === "5" ? 0 : data.length, 12), 124);
  h.write(octal(0, 12), 136);
  h.fill(0x20, 148, 156);
  h.write(type, 156);
  h.write("ustar\0", 257);
  h.write("00", 263);
  h.write(
    octal(
      h.reduce((sum, byte) => sum + byte, 0),
      8,
    ),
    148,
  );
  const padded = Buffer.alloc(Math.ceil(data.length / 512) * 512);
  data.copy(padded);
  return Buffer.concat([h, ...(type === "5" ? [] : [padded])]);
}

function tar(root, entries) {
  return Buffer.concat([
    tarHeader(`${root}/`, "", "5", 0o755),
    ...entries.map(([name, data, type, mode]) =>
      tarHeader(`${root}/${name}`, data, type, mode),
    ),
    Buffer.alloc(1024),
  ]);
}

test("ZIP parser accepts the production stored format and rejects traversal, duplicates, and CRC damage", () => {
  const good = zip([
    ["gpui-migration.exe", "binary"],
    ["MANIFEST.json", "{}"],
  ]);
  assert.equal(parseProductionZip(good).size, 2);
  assert.throws(
    () => parseProductionZip(zip([["../outside", "x"]])),
    /unsafe or duplicate/,
  );
  assert.throws(
    () =>
      parseProductionZip(
        zip([
          ["a", "x"],
          ["a", "y"],
        ]),
      ),
    /unsafe or duplicate/,
  );
  const damaged = Buffer.from(good);
  damaged[good.indexOf(Buffer.from("binary"))] ^= 1;
  assert.throws(() => parseProductionZip(damaged), /CRC mismatch/);
});

test("tar parser accepts flat regular files and rejects traversal, links, unexpected directories, and mode changes", () => {
  const good = tar("candidate", [["gpui-migration", "binary", "0", 0o755]]);
  assert.equal(
    parseProductionTar(good, "candidate").get("gpui-migration").mode,
    0o755,
  );
  assert.throws(
    () =>
      parseProductionTar(tar("candidate", [["../outside", "x"]]), "candidate"),
    /unsafe or duplicate|escapes/,
  );
  assert.throws(
    () =>
      parseProductionTar(
        tar("candidate", [["link", "target", "2"]]),
        "candidate",
      ),
    /unsafe entry type/,
  );
  assert.throws(
    () =>
      parseProductionTar(
        tar("candidate", [["subdir/", "", "5", 0o755]]),
        "candidate",
      ),
    /unexpected directory/,
  );
});

test("sidecars must be mutually consistent and identify a verified stable artifact", () => {
  const manifest = {
    schema: "butter-paper/package-manifest",
    schemaVersion: 1,
    target: "linux-x64",
    channel: "stable",
    artifact: { path: "linux-x64.tar.xz", bytes: 7, sha256: "a".repeat(64) },
  };
  const receipt = {
    schema: "butter-paper/package-verification",
    schemaVersion: 1,
    target: "linux-x64",
    channel: "stable",
    verified: true,
    artifact: { ...manifest.artifact },
  };
  assert.deepEqual(assertSidecars("unused", manifest, receipt, "linux-x64"), {
    archiveBytes: 7,
    archiveSha256: "a".repeat(64),
  });
  assert.throws(
    () =>
      assertSidecars(
        "unused",
        manifest,
        { ...receipt, verified: false },
        "linux-x64",
      ),
    /verification receipt is invalid/,
  );
  assert.throws(
    () => assertSidecars("unused", manifest, receipt, "windows-x64"),
    /package manifest identity is invalid/,
  );
});

test("cleanup requests graceful close, terminates owned processes, and reports survivors", async () => {
  const actions = [];
  let live = [{ pid: 10 }, { pid: 11 }];
  const result = await cleanupOwnedProcess({
    rootPid: 10,
    listProcesses: async () => live,
    requestClose: async () => {
      actions.push("close");
      return "fixture-close";
    },
    terminate: async () => {
      actions.push("terminate");
      live = [];
    },
    gracefulMs: 0,
  });
  assert.deepEqual(actions, ["close", "terminate"]);
  assert.equal(result.remaining.length, 0);

  const survivors = await cleanupOwnedProcess({
    rootPid: 10,
    listProcesses: async () => [{ pid: 11 }],
    requestClose: async () => "close-request-unavailable",
    terminate: async () => actions.push("terminate-survivor"),
    gracefulMs: 0,
  });
  assert.equal(survivors.remaining[0].pid, 11);
  assert.equal(actions.at(-1), "terminate-survivor");

  let terminationRan = false;
  const closeFailure = await cleanupOwnedProcess({
    rootPid: 10,
    listProcesses: async () => [],
    requestClose: async () => {
      throw new Error("window manager unavailable");
    },
    terminate: async () => {
      terminationRan = true;
    },
    gracefulMs: 0,
  });
  assert.equal(terminationRan, true);
  assert.equal(closeFailure.closeError, "window manager unavailable");
});

test("emergency cleanup reinspects after termination and represents failed inspection as unknown", async () => {
  const order = [];
  const survivor = await emergencyCleanupOwnedProcess({
    rootPid: 20,
    terminate: async () => {
      order.push("terminate");
    },
    listProcesses: async () => {
      order.push("inspect");
      return [{ pid: 21, ppid: 20, exe: "/owned/butter-paper-pdf-worker" }];
    },
  });
  assert.deepEqual(order, ["terminate", "inspect"]);
  assert.equal(survivor.remaining[0].pid, 21);
  assert.equal(survivor.inspectionError, undefined);

  const unknown = await emergencyCleanupOwnedProcess({
    rootPid: 20,
    terminate: async () => {
      throw new Error("kill command failed");
    },
    listProcesses: async () => {
      throw new Error("process table unavailable");
    },
  });
  assert.equal(unknown.remaining, null);
  assert.equal(unknown.terminationError, "kill command failed");
  assert.equal(unknown.inspectionError, "process table unavailable");
});

test("Rectangle smoke evidence requires a hash-checked recovery timeline and a newly committed edit", async () => {
  const root = await mkdtemp(join(tmpdir(), "bp-smoke-rectangle-"));
  const store = join(root, "store"),
    id = "a".repeat(32),
    objects = join(store, "objects"),
    heads = join(store, "heads");
  await Promise.all([
    mkdir(objects, { recursive: true }),
    mkdir(heads, { recursive: true }),
  ]);
  const timelineBytes = Buffer.from(
    JSON.stringify({
      schema_version: 2,
      current: { revision: 1, rectangles: [{ id: "rectangle:smoke" }] },
    }),
  );
  const timelineHash = sha256(timelineBytes);
  await Promise.all([
    writeFile(
      join(store, "index.json"),
      JSON.stringify({ version: 1, active: [id] }),
    ),
    writeFile(
      join(heads, `${id}.json`),
      JSON.stringify({
        version: 2,
        document_id: id,
        source_kind: "opened",
        requires_save_as: false,
        current_revision: 1,
        saved_revision: 0,
        timeline_object_sha256: timelineHash,
      }),
    ),
    writeFile(join(objects, timelineHash), timelineBytes),
  ]);
  try {
    assert.deepEqual(await readRectangleEditEvidence(store, "linux"), {
      documentId: id,
      currentRevision: 1,
      savedRevision: 0,
      rectangleCount: 1,
      timelineSha256: timelineHash,
      platform: "linux",
    });
    assert.equal(
      (
        await readRectangleEditEvidence(store, "linux", {
          requireNewEdit: false,
        })
      ).rectangleCount,
      1,
    );
    const emptyTimeline = Buffer.from(
      JSON.stringify({
        schema_version: 2,
        current: { revision: 1, rectangles: [] },
      }),
    );
    const emptyHash = sha256(emptyTimeline);
    await writeFile(join(objects, emptyHash), emptyTimeline);
    const headPath = join(heads, `${id}.json`);
    const head = JSON.parse(await readFile(headPath, "utf8"));
    head.timeline_object_sha256 = emptyHash;
    await writeFile(headPath, JSON.stringify(head));
    await assert.rejects(
      readRectangleEditEvidence(store, "linux"),
      /no committed Rectangle edit/,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("qpdf semantic evidence requires exactly one valid Rectangle with an appearance", () => {
  const qpdf = (value) =>
    JSON.stringify({
      qpdf: [{ jsonversion: 2 }, { "obj:7 0 R": { value } }],
    });
  assert.deepEqual(
    parseQpdfRectangleEvidence(
      qpdf({
        "/Type": "/Annot",
        "/Subtype": "/Square",
        "/Rect": [10, 20, 110, 90],
        "/AP": { "/N": "6 0 R" },
      }),
    ),
    {
      parser: "qpdf --json",
      rectangleCount: 1,
      rectangles: [
        {
          object: "obj:7 0 R",
          subtype: "/Square",
          rect: [10, 20, 110, 90],
        },
      ],
    },
  );
  assert.throws(
    () =>
      parseQpdfRectangleEvidence(
        qpdf({
          "/Type": "/Annot",
          "/Subtype": "/Square",
          "/Rect": [10, 20, 110, 90],
        }),
      ),
    /no normal appearance/,
  );
  assert.throws(
    () => parseQpdfRectangleEvidence(JSON.stringify({ qpdf: [{}, {}] })),
    /exactly one independently parsed/,
  );
});

test("fresh reopen clears first-process recovery evidence before relaunch", async () => {
  const source = await readFile(
    new URL("../scripts/smoke-nonmac-production-package.mjs", import.meta.url),
    "utf8",
  );
  const fullClose = source.indexOf(
    '"packaged app or PDF worker remained after the full close"',
  );
  const reset = source.indexOf(
    "await rm(recoveryStoreRoot, { recursive: true, force: true })",
  );
  const relaunch = source.indexOf("child = launch();", reset);
  assert.ok(fullClose >= 0 && reset > fullClose && relaunch > reset);
  assert.match(source, /recoveryResetBeforeFreshReopen = true/);
});
