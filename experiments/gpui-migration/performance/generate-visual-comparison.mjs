#!/usr/bin/env node

import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import sharp from "sharp";

const [baselinePath, candidatePath, outputPrefix] = process.argv.slice(2);

if (!baselinePath || !candidatePath || !outputPrefix) {
  console.error(
    "Usage: node generate-visual-comparison.mjs <electron.png> <gpui.png> <output-prefix>",
  );
  process.exit(2);
}

const baselineFile = await readFile(baselinePath);
const candidateFile = await readFile(candidatePath);
const baseline = await sharp(baselineFile).ensureAlpha().raw().toBuffer({
  resolveWithObject: true,
});
const candidate = await sharp(candidateFile).ensureAlpha().raw().toBuffer({
  resolveWithObject: true,
});

const { width, height, channels } = baseline.info;
if (
  candidate.info.width !== width ||
  candidate.info.height !== height ||
  candidate.info.channels !== channels
) {
  throw new Error(
    `Capture dimensions differ: Electron ${width}x${height}x${channels}, ` +
      `GPUI ${candidate.info.width}x${candidate.info.height}x${candidate.info.channels}`,
  );
}

const regions = {
  "full-frame": { x: 0, y: 0, width, height },
  "window-chrome": { x: 0, y: 0, width: 1152, height: 62 },
  "document-tabs": { x: 0, y: 62, width: 1152, height: 47 },
  "workspace-toolbar": { x: 0, y: 109, width: 1152, height: 45 },
  "left-navigation": { x: 0, y: 109, width: 334, height: 659 },
  "document-viewport": { x: 334, y: 154, width: 734, height: 614 },
  "right-tools": { x: 1068, y: 109, width: 84, height: 659 },
};

const threshold = 8;
const overlay = Buffer.alloc(baseline.data.length);
const difference = Buffer.alloc(baseline.data.length);
const sideBySide = Buffer.alloc(width * 2 * height * channels);

for (let y = 0; y < height; y += 1) {
  const rowBytes = width * channels;
  const sourceOffset = y * rowBytes;
  const targetOffset = y * rowBytes * 2;
  baseline.data.copy(sideBySide, targetOffset, sourceOffset, sourceOffset + rowBytes);
  candidate.data.copy(
    sideBySide,
    targetOffset + rowBytes,
    sourceOffset,
    sourceOffset + rowBytes,
  );
}

for (let index = 0; index < baseline.data.length; index += channels) {
  let peak = 0;
  for (let channel = 0; channel < 3; channel += 1) {
    const a = baseline.data[index + channel];
    const b = candidate.data[index + channel];
    overlay[index + channel] = Math.round((a + b) / 2);
    peak = Math.max(peak, Math.abs(a - b));
  }
  const heat = Math.min(255, peak * 6);
  difference[index] = 255;
  difference[index + 1] = 255 - heat;
  difference[index + 2] = 255 - heat;
  overlay[index + 3] = difference[index + 3] = 255;
}

function analyzeRegion(region) {
  let exact = 0;
  let aboveThreshold = 0;
  let totalError = 0;
  let maximumError = 0;
  let minX = width;
  let minY = height;
  let maxX = -1;
  let maxY = -1;

  for (let y = region.y; y < region.y + region.height; y += 1) {
    for (let x = region.x; x < region.x + region.width; x += 1) {
      const index = (y * width + x) * channels;
      const dr = Math.abs(baseline.data[index] - candidate.data[index]);
      const dg = Math.abs(baseline.data[index + 1] - candidate.data[index + 1]);
      const db = Math.abs(baseline.data[index + 2] - candidate.data[index + 2]);
      const peak = Math.max(dr, dg, db);
      totalError += dr + dg + db;
      maximumError = Math.max(maximumError, peak);
      if (peak === 0) exact += 1;
      if (peak > threshold) {
        aboveThreshold += 1;
        minX = Math.min(minX, x);
        minY = Math.min(minY, y);
        maxX = Math.max(maxX, x);
        maxY = Math.max(maxY, y);
      }
    }
  }

  const pixels = region.width * region.height;
  return {
    bounds: region,
    exactPercent: Number(((100 * exact) / pixels).toFixed(3)),
    aboveThresholdPercent: Number(((100 * aboveThreshold) / pixels).toFixed(3)),
    meanAbsoluteChannelError: Number((totalError / (pixels * 3)).toFixed(3)),
    maximumChannelError: maximumError,
    mismatchBounds:
      aboveThreshold > 0 ? { minX, minY, maxX, maxY } : null,
  };
}

const outputDir = path.dirname(outputPrefix);
await mkdir(outputDir, { recursive: true });
await Promise.all([
  sharp(sideBySide, {
    raw: { width: width * 2, height, channels },
  }).png().toFile(`${outputPrefix}-side-by-side.png`),
  sharp(overlay, { raw: { width, height, channels } })
    .png()
    .toFile(`${outputPrefix}-overlay.png`),
  sharp(difference, { raw: { width, height, channels } })
    .png()
    .toFile(`${outputPrefix}-difference.png`),
]);

const metrics = {
  generatedAt: new Date().toISOString(),
  threshold,
  dimensions: { width, height },
  baseline: {
    path: baselinePath,
    sha256: createHash("sha256").update(baselineFile).digest("hex"),
  },
  candidate: {
    path: candidatePath,
    sha256: createHash("sha256").update(candidateFile).digest("hex"),
  },
  regions: Object.fromEntries(
    Object.entries(regions).map(([name, region]) => [name, analyzeRegion(region)]),
  ),
};

await writeFile(`${outputPrefix}-metrics.json`, `${JSON.stringify(metrics, null, 2)}\n`);
console.log(JSON.stringify(metrics, null, 2));
