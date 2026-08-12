import { createRequire } from 'node:module';
import { readFile, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';

const repositoryRoot = resolve(import.meta.dirname, '..');
const require = createRequire(join(repositoryRoot, 'packages/pdf/package.json'));
const { createCanvas, loadImage } = require('@napi-rs/canvas');

const canvasSize = 1024;
const visibleWidthRatio = 0.80;
const sourceViewBox = { width: 256, height: 200 };
const visibleBounds = { left: 10, right: 246, top: 10, bottom: 189 };
const visibleWidth = visibleBounds.right - visibleBounds.left;
const renderedWidth = canvasSize * visibleWidthRatio * (sourceViewBox.width / visibleWidth);
const renderedHeight = renderedWidth * (sourceViewBox.height / sourceViewBox.width);
const renderedLeft = (canvasSize - renderedWidth) / 2;
const renderedTop = (canvasSize - renderedHeight) / 2;

const variants = [
  {
    outputs: ['butter-paper-icon.png', 'butter-paper-icon-dark.png'],
    source: 'butter-paper-origami.svg',
  },
  {
    outputs: ['butter-paper-icon-beta.png', 'butter-paper-icon-beta-dark.png'],
    source: 'butter-paper-origami-beta.svg',
  },
];

for (const variant of variants) {
  const source = await readFile(join(repositoryRoot, 'assets/icon-source', variant.source));
  const image = await loadImage(source);
  const canvas = createCanvas(canvasSize, canvasSize);
  const context = canvas.getContext('2d');
  context.imageSmoothingEnabled = true;
  context.imageSmoothingQuality = 'high';
  context.drawImage(image, renderedLeft, renderedTop, renderedWidth, renderedHeight);
  const artwork = canvas.toBuffer('image/png');

  for (const output of variant.outputs) {
    await writeFile(join(repositoryRoot, 'assets', output), artwork);
  }
}

console.log(`Rendered brand icons with ${Math.round(visibleWidthRatio * 100)}% visible-width optical sizing.`);
