import {
  mkdir,
  readFile,
  writeFile,
} from 'node:fs/promises';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';

const repositoryRoot = resolve(import.meta.dirname, '..');
const require = createRequire(join(repositoryRoot, 'packages/pdf/package.json'));
const { createCanvas, loadImage } = require('@napi-rs/canvas');
const checkOnly = process.argv.slice(2).includes('--check');
const unexpectedArguments = process.argv.slice(2).filter(argument => argument !== '--check');

if (unexpectedArguments.length > 0) {
  throw new Error(`Unsupported icon generator arguments: ${unexpectedArguments.join(', ')}`);
}

const linuxSizes = [16, 22, 24, 32, 48, 64, 72, 96, 128, 256, 512, 1024];
const icoSizes = [16, 24, 32, 48, 64, 128, 256];
const icnsEntries = [
  ['icp4', 16],
  ['icp5', 32],
  ['icp6', 64],
  ['ic07', 128],
  ['ic08', 256],
  ['ic09', 512],
  ['ic10', 1024],
  ['ic11', 32],
  ['ic12', 64],
  ['ic13', 256],
  ['ic14', 512],
];
const variants = [
  {
    name: 'stable',
    source: join(repositoryRoot, 'assets/butter-paper-icon.svg'),
    output: join(repositoryRoot, 'apps/desktop/assets'),
  },
  {
    name: 'beta',
    source: join(repositoryRoot, 'assets/butter-paper-icon-beta.svg'),
    output: join(repositoryRoot, 'apps/desktop/assets/beta'),
  },
];

const generatedFiles = new Map();

for (const variant of variants) {
  const source = await loadImage(variant.source);
  const pngBySize = new Map();
  const requiredSizes = new Set([...linuxSizes, ...icoSizes, ...icnsEntries.map(([, size]) => size)]);

  for (const size of requiredSizes) {
    pngBySize.set(size, renderIcon(source, size));
  }

  generatedFiles.set(join(variant.output, 'icon.png'), pngBySize.get(512));
  generatedFiles.set(join(variant.output, 'icon.ico'), buildIco(icoSizes, pngBySize));
  generatedFiles.set(join(variant.output, 'icon.icns'), buildIcns(icnsEntries, pngBySize));

  for (const size of linuxSizes) {
    generatedFiles.set(join(variant.output, 'linux', `${size}x${size}.png`), pngBySize.get(size));
  }
}

const mismatches = [];
for (const [filePath, expected] of generatedFiles) {
  if (checkOnly) {
    let actual;
    try {
      actual = await readFile(filePath);
    } catch {
      mismatches.push(`${filePath}: missing`);
      continue;
    }
    if (!actual.equals(expected)) {
      mismatches.push(`${filePath}: does not match generated icon`);
    }
    continue;
  }

  await mkdir(dirname(filePath), { recursive: true });
  await writeFile(filePath, expected);
}

if (mismatches.length > 0) {
  console.error('Generated desktop icon check failed:');
  for (const mismatch of mismatches) {
    console.error(`- ${mismatch}`);
  }
  process.exit(1);
}

console.log(
  checkOnly
    ? `Generated desktop icon check passed (${generatedFiles.size} files).`
    : `Generated ${generatedFiles.size} desktop icon files for stable and beta.`,
);

function renderIcon(source, size) {
  const canvas = createCanvas(size, size);
  const context = canvas.getContext('2d');

  context.imageSmoothingEnabled = true;
  context.imageSmoothingQuality = 'high';
  context.drawImage(source, 0, 0, size, size);

  return canvas.toBuffer('image/png');
}

function buildIco(sizes, pngBySize) {
  const headerSize = 6 + (sizes.length * 16);
  const header = Buffer.alloc(headerSize);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(sizes.length, 4);

  let imageOffset = headerSize;
  const images = [];
  sizes.forEach((size, index) => {
    const image = pngBySize.get(size);
    const entryOffset = 6 + (index * 16);
    header.writeUInt8(size === 256 ? 0 : size, entryOffset);
    header.writeUInt8(size === 256 ? 0 : size, entryOffset + 1);
    header.writeUInt8(0, entryOffset + 2);
    header.writeUInt8(0, entryOffset + 3);
    header.writeUInt16LE(1, entryOffset + 4);
    header.writeUInt16LE(32, entryOffset + 6);
    header.writeUInt32LE(image.length, entryOffset + 8);
    header.writeUInt32LE(imageOffset, entryOffset + 12);
    imageOffset += image.length;
    images.push(image);
  });

  return Buffer.concat([header, ...images]);
}

function buildIcns(entries, pngBySize) {
  const chunks = entries.map(([type, size]) => {
    const image = pngBySize.get(size);
    const chunk = Buffer.alloc(8 + image.length);
    chunk.write(type, 0, 4, 'ascii');
    chunk.writeUInt32BE(chunk.length, 4);
    image.copy(chunk, 8);
    return chunk;
  });
  const totalSize = 8 + chunks.reduce((sum, chunk) => sum + chunk.length, 0);
  const header = Buffer.alloc(8);
  header.write('icns', 0, 4, 'ascii');
  header.writeUInt32BE(totalSize, 4);
  return Buffer.concat([header, ...chunks]);
}
