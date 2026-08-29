import { createRequire } from 'node:module';
import { readFile, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';

const repositoryRoot = resolve(import.meta.dirname, '..');
const require = createRequire(join(repositoryRoot, 'packages/pdf/package.json'));
const { createCanvas, loadImage } = require('@napi-rs/canvas');

const canvasSize = 1024;
const visibleWidthRatio = 0.78;
const macosDockTileSize = 64;
const macosDockEdgeInset = 6;
const macosVisibleWidthRatio = (
  macosDockTileSize - (macosDockEdgeInset * 2)
) / macosDockTileSize;
const opticalYOffsetRatio = 0.018;
const centralFoldBounds = { top: 31, bottom: 127 };
const centralFoldFiveTwelfthsFromBottom = centralFoldBounds.bottom
  - ((centralFoldBounds.bottom - centralFoldBounds.top) * (5 / 12));
const sourceViewBox = { width: 256, height: 200 };
const visibleBounds = { left: 22, right: 234, top: 10, bottom: 189 };
const visibleWidth = visibleBounds.right - visibleBounds.left;
const variants = [
  {
    outputs: ['butter-paper-icon.png', 'butter-paper-icon-dark.png'],
    macosOutputs: ['butter-paper-icon-macos.png', 'butter-paper-icon-macos-dark.png'],
    source: 'butter-paper-origami.svg',
  },
  {
    outputs: ['butter-paper-icon-beta.png', 'butter-paper-icon-beta-dark.png'],
    macosOutputs: ['butter-paper-icon-beta-macos.png', 'butter-paper-icon-beta-macos-dark.png'],
    source: 'butter-paper-origami-beta.svg',
  },
];

for (const variant of variants) {
  const source = await readFile(join(repositoryRoot, 'assets/icon-source', variant.source));
  const image = await loadImage(source);
  const artwork = renderArtwork(image, visibleWidthRatio);
  const macosArtwork = renderArtwork(image, macosVisibleWidthRatio, {
    sourceY: centralFoldFiveTwelfthsFromBottom,
    targetY: canvasSize / 2,
  });

  for (const output of variant.outputs) {
    await writeFile(join(repositoryRoot, 'assets', output), artwork);
  }
  for (const output of variant.macosOutputs) {
    await writeFile(join(repositoryRoot, 'assets', output), macosArtwork);
  }
}

console.log(
  `Rendered brand icons with ${Math.round(visibleWidthRatio * 100)}% visible-width sizing and `
  + `${Math.round(opticalYOffsetRatio * 1000) / 10}% downward optical alignment.`,
);
console.log(
  `Rendered macOS Icon Composer artwork at ${Math.round(macosVisibleWidthRatio * 100)}% visible width `
  + `for ${macosDockEdgeInset}px insets in a ${macosDockTileSize}px Dock tile.`,
);

function renderArtwork(image, targetVisibleWidthRatio, verticalAnchor) {
  const renderedWidth = canvasSize * targetVisibleWidthRatio * (sourceViewBox.width / visibleWidth);
  const renderedHeight = renderedWidth * (sourceViewBox.height / sourceViewBox.width);
  const renderedLeft = (canvasSize - renderedWidth) / 2;
  const renderedTop = verticalAnchor
    ? verticalAnchor.targetY - ((verticalAnchor.sourceY / sourceViewBox.height) * renderedHeight)
    : (canvasSize - renderedHeight) / 2 + (canvasSize * opticalYOffsetRatio);
  const canvas = createCanvas(canvasSize, canvasSize);
  const context = canvas.getContext('2d');
  context.imageSmoothingEnabled = true;
  context.imageSmoothingQuality = 'high';
  context.drawImage(image, renderedLeft, renderedTop, renderedWidth, renderedHeight);
  return canvas.toBuffer('image/png');
}
