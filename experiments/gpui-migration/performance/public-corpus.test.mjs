import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  lockedPublicCorpus,
  materializeLockedCorpus,
  verifyLockedCorpusFile,
} from "./public-corpus.mjs";

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

test("declares the two heavy public comparison inputs with immutable identities", () => {
  assert.deepEqual(Object.keys(lockedPublicCorpus), [
    "nasa-apollo-summary-526-v1",
    "usgs-usa-geology-sheet-v1",
  ]);
  assert.equal(lockedPublicCorpus["nasa-apollo-summary-526-v1"].bytes, 27_842_805);
  assert.equal(lockedPublicCorpus["usgs-usa-geology-sheet-v1"].bytes, 180_360_864);
  for (const item of Object.values(lockedPublicCorpus)) {
    assert.match(item.url, /^https:\/\//);
    assert.match(item.sha256, /^[a-f0-9]{64}$/);
  }
});

test("rejects cached bytes whose size or digest does not match", async () => {
  const directory = await mkdtemp(join(tmpdir(), "bp-corpus-test-"));
  const path = join(directory, "probe.pdf");
  await writeFile(path, "wrong", "utf8");
  const descriptor = {
    id: "probe-v1",
    bytes: 5,
    sha256: sha256(Buffer.from("right")),
  };
  await assert.rejects(verifyLockedCorpusFile(path, descriptor), /SHA-256 mismatch/);
});

test("downloads to a temporary file, verifies it, then publishes by digest", async () => {
  const directory = await mkdtemp(join(tmpdir(), "bp-corpus-test-"));
  const payload = Buffer.from("%PDF-1.4\nlocked public probe\n");
  const descriptor = {
    id: "probe-v1",
    url: "https://example.invalid/probe.pdf",
    bytes: payload.length,
    sha256: sha256(payload),
  };
  let requests = 0;
  const fetchImpl = async () => {
    requests += 1;
    return new Response(payload, { status: 200 });
  };

  const first = await materializeLockedCorpus(descriptor, directory, { fetchImpl });
  assert.equal(first.cache_hit, false);
  assert.equal(first.sha256, descriptor.sha256);
  assert.deepEqual(await readFile(first.path), payload);

  const second = await materializeLockedCorpus(descriptor, directory, { fetchImpl });
  assert.equal(second.cache_hit, true);
  assert.equal(second.path, first.path);
  assert.equal(requests, 1);
});
