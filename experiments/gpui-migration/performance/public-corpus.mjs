#!/usr/bin/env node

import { createHash, randomUUID } from "node:crypto";
import { createReadStream, createWriteStream } from "node:fs";
import { link, mkdir, stat, unlink } from "node:fs/promises";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const lockedPublicCorpus = Object.freeze({
  "nasa-apollo-summary-526-v1": Object.freeze({
    id: "nasa-apollo-summary-526-v1",
    url: "https://ntrs.nasa.gov/api/citations/19750013242/downloads/19750013242.pdf",
    bytes: 27_842_805,
    sha256: "68d0f3bb93fd1c4e4f3adf99d483e9161f14293a311e25aa6c31241bbbb84049",
  }),
  "usgs-usa-geology-sheet-v1": Object.freeze({
    id: "usgs-usa-geology-sheet-v1",
    url: "https://pubs.usgs.gov/dds/dds11/USA_Geology_KB_Map.pdf",
    bytes: 180_360_864,
    sha256: "f058179e193ccbc15ca662feff3554102f64ff2114436a5ce6116d6fa5d2a6e2",
  }),
});

async function fileSha256(path) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  return hash.digest("hex");
}

export async function verifyLockedCorpusFile(path, descriptor) {
  const metadata = await stat(path);
  if (metadata.size !== descriptor.bytes) {
    throw new Error(
      `${descriptor.id}: byte count mismatch: expected ${descriptor.bytes}, got ${metadata.size}`,
    );
  }
  const actualSha256 = await fileSha256(path);
  if (actualSha256 !== descriptor.sha256) {
    throw new Error(
      `${descriptor.id}: SHA-256 mismatch: expected ${descriptor.sha256}, got ${actualSha256}`,
    );
  }
  return { bytes: metadata.size, sha256: actualSha256 };
}

function isMissing(error) {
  return error && typeof error === "object" && error.code === "ENOENT";
}

export async function materializeLockedCorpus(
  descriptor,
  cacheDirectory,
  { fetchImpl = globalThis.fetch } = {},
) {
  if (typeof fetchImpl !== "function") throw new Error("a fetch implementation is required");
  const cacheRoot = resolve(cacheDirectory);
  const destination = resolve(cacheRoot, `${descriptor.sha256}.pdf`);
  await mkdir(cacheRoot, { recursive: true });

  try {
    const identity = await verifyLockedCorpusFile(destination, descriptor);
    return { id: descriptor.id, path: destination, cache_hit: true, ...identity };
  } catch (error) {
    if (!isMissing(error)) {
      throw new Error(`${descriptor.id}: refusing to replace an invalid cache entry: ${error.message}`);
    }
  }

  const response = await fetchImpl(descriptor.url, { redirect: "follow" });
  if (!response.ok || !response.body) {
    throw new Error(`${descriptor.id}: download failed with HTTP ${response.status}`);
  }
  const temporary = resolve(cacheRoot, `.${descriptor.sha256}.${randomUUID()}.partial`);
  try {
    await pipeline(Readable.fromWeb(response.body), createWriteStream(temporary, { flags: "wx" }));
    const identity = await verifyLockedCorpusFile(temporary, descriptor);
    try {
      await link(temporary, destination);
    } catch (error) {
      if (!error || typeof error !== "object" || error.code !== "EEXIST") throw error;
      await verifyLockedCorpusFile(destination, descriptor);
    }
    return { id: descriptor.id, path: destination, cache_hit: false, ...identity };
  } finally {
    await unlink(temporary).catch((error) => {
      if (!isMissing(error)) throw error;
    });
  }
}

function parseArguments(argv) {
  const options = { ids: [] };
  for (let index = 0; index < argv.length; index += 1) {
    const option = argv[index];
    if (option === "--help" || option === "-h") return { help: true };
    if (option !== "--cache" && option !== "--id") throw new Error(`unknown option: ${option}`);
    const value = argv[++index];
    if (!value || value.startsWith("--")) throw new Error(`${option} requires a value`);
    if (option === "--cache") options.cache = value;
    else options.ids.push(value);
  }
  if (!options.cache) throw new Error("--cache is required");
  if (options.ids.length === 0 || options.ids.includes("all")) {
    options.ids = Object.keys(lockedPublicCorpus);
  }
  for (const id of options.ids) {
    if (!lockedPublicCorpus[id]) throw new Error(`unknown corpus id: ${id}`);
  }
  return options;
}

function usage() {
  return `Usage: node public-corpus.mjs --cache <directory> [--id <id|all>]\n\nDownloads only the locked public NASA and USGS comparison files. Each file is\nverified by byte count and SHA-256 before content-addressed publication.\n`;
}

async function main() {
  const options = parseArguments(process.argv.slice(2));
  if (options.help) {
    process.stdout.write(usage());
    return;
  }
  const results = [];
  for (const id of options.ids) {
    results.push(await materializeLockedCorpus(lockedPublicCorpus[id], options.cache));
  }
  process.stdout.write(`${JSON.stringify({ status: "passed", results }, null, 2)}\n`);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  await main();
}
