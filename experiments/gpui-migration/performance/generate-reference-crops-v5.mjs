#!/usr/bin/env node

import { createHash } from "node:crypto";
import { execFile } from "node:child_process";
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { basename, resolve } from "node:path";
import process from "node:process";
import { promisify } from "node:util";

import { loadComparisonWorkloadV5 } from "./comparison-workload-v5.mjs";
import { renderReferenceCrop } from "./registered-crop-v5.mjs";

const execFileAsync = promisify(execFile);

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function option(name) {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : null;
}

const pdfPath = option("--pdf");
const outputDirectory = option("--output");
if (!pdfPath || !outputDirectory) {
  console.error(
    "Usage: node generate-reference-crops-v5.mjs --pdf <NASA PDF> --output <directory>",
  );
  process.exit(2);
}

const workload = await loadComparisonWorkloadV5();
const fixture = workload.fixtures.find(
  ({ id }) => id === "nasa-apollo-summary-526-v1",
);
const pdfBytes = await readFile(pdfPath);
if (sha256(pdfBytes) !== fixture?.sha256) {
  throw new Error("NASA source PDF does not match the frozen fixture SHA-256");
}
const command = workload.journeys
  .flatMap(({ commands }) => commands)
  .find(({ id }) => id === "viewer:dynamic-fidelity-scroll");
if (!command) throw new Error("dynamic fidelity command is missing");

const output = resolve(outputDirectory);
await mkdir(output, { recursive: true });
const popplerVersion = await execFileAsync("pdftoppm", ["-v"])
  .then(({ stdout, stderr }) => `${stdout}${stderr}`.split("\n")[0].trim())
  .catch(() => "unavailable");
const receipts = [];
for (const crop of command.registered_crops) {
  const expectedSha256 = crop.reference_raster.reference_crop_sha256;
  if (!/^[0-9a-f]{64}$/.test(expectedSha256 ?? "")) {
    throw new Error(`${crop.crop_id}: reference crop SHA-256 is not pinned`);
  }
  const outputPath = resolve(output, `${crop.crop_id}.png`);
  const receipt = await renderReferenceCrop({
    pdfPath: resolve(pdfPath),
    pageNumber: crop.page_number,
    pdfRect: crop.pdf_rect,
    dpi: crop.reference_raster.dpi,
    outputPath,
  });
  if (receipt.sha256 !== expectedSha256) {
    await rm(outputPath, { force: true });
    throw new Error(
      `${crop.crop_id}: generated ${receipt.sha256}, expected ${expectedSha256}; use the reviewed transferred reference instead of accepting renderer drift`,
    );
  }
  receipts.push({
    crop_id: crop.crop_id,
    file: basename(outputPath),
    registration_sha256: crop.registration_sha256,
    reference_crop_sha256: receipt.sha256,
    page_number: receipt.page_number,
    page_size_points: receipt.page_size_points,
    dpi: receipt.dpi,
    dimensions: { width: receipt.width, height: receipt.height },
  });
}

const manifest = {
  schema_version: 1,
  fixture_id: fixture.id,
  fixture_sha256: fixture.sha256,
  generator: "Poppler pdftoppm with locked v5 PDF-point registrations",
  generator_version: popplerVersion,
  receipts,
};
await writeFile(
  resolve(output, "manifest.json"),
  `${JSON.stringify(manifest, null, 2)}\n`,
);
console.log(JSON.stringify(manifest, null, 2));
