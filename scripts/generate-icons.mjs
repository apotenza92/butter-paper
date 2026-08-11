import {
  mkdir,
  readFile,
  writeFile,
} from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';
import {
  dirname,
  join,
  relative,
  resolve,
} from 'node:path';

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
    source: join(repositoryRoot, 'assets/butter-paper-icon.png'),
    darkSource: join(repositoryRoot, 'assets/butter-paper-icon-dark.png'),
    adaptiveArtwork: 'macos/Butter Paper.icon/Assets',
    output: join(repositoryRoot, 'apps/desktop/assets'),
  },
  {
    name: 'beta',
    source: join(repositoryRoot, 'assets/butter-paper-icon-beta.png'),
    darkSource: join(repositoryRoot, 'assets/butter-paper-icon-beta-dark.png'),
    adaptiveArtwork: 'macos/Butter Paper Beta.icon/Assets',
    output: join(repositoryRoot, 'apps/desktop/assets/beta'),
  },
];

const mismatches = [];
let generatedFileCount = 0;
for (const variant of variants) {
  if (checkOnly) {
    await verifyGeneratedVariant(variant, mismatches);
    generatedFileCount += expectedOutputPaths(variant).length;
    continue;
  }

  generatedFileCount += await generateVariant(variant);
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
    ? `Generated desktop icon check passed (${generatedFileCount} files).`
    : `Generated ${generatedFileCount} desktop icon files for stable and beta.`,
);

async function generateVariant(variant) {
  const source = await loadImage(variant.source);
  const pngBySize = new Map();
  const requiredSizes = new Set([...linuxSizes, ...icoSizes, ...icnsEntries.map(([, size]) => size)]);

  for (const size of requiredSizes) {
    pngBySize.set(size, renderIcon(source, size));
  }

  const generatedFiles = new Map([
    ['icon.png', pngBySize.get(512)],
    ['icon.ico', buildIco(icoSizes, pngBySize)],
    ['icon.icns', buildIcns(icnsEntries, pngBySize)],
    [`${variant.adaptiveArtwork}/01-artwork.png`, await readFile(variant.source)],
    [`${variant.adaptiveArtwork}/01-artwork-dark.png`, await readFile(variant.darkSource)],
  ]);
  for (const size of linuxSizes) {
    generatedFiles.set(`linux/${size}x${size}.png`, pngBySize.get(size));
  }

  for (const [relativePath, contents] of generatedFiles) {
    const filePath = join(variant.output, relativePath);
    await mkdir(dirname(filePath), { recursive: true });
    await writeFile(filePath, contents);
  }

  const sourceContents = await readFile(variant.source);
  const manifest = {
    schemaVersion: 1,
    source: repositoryRelativePath(variant.source),
    sourceSha256: sha256(sourceContents),
    outputs: Object.fromEntries(
      [...generatedFiles]
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([relativePath, contents]) => [
          relativePath,
          {
            sha256: sha256(contents),
            size: contents.length,
          },
        ]),
    ),
  };
  await writeFile(
    join(variant.output, 'generated-icon-manifest.json'),
    `${JSON.stringify(manifest, null, 2)}\n`,
  );

  return generatedFiles.size;
}

async function verifyGeneratedVariant(variant, variantMismatches) {
  const manifestPath = join(variant.output, 'generated-icon-manifest.json');
  let manifest;
  try {
    manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
  } catch {
    variantMismatches.push(`${manifestPath}: missing or invalid`);
    return;
  }

  const sourceContents = await readFile(variant.source);
  const expectedPaths = expectedOutputPaths(variant);
  if (manifest.schemaVersion !== 1) {
    variantMismatches.push(`${manifestPath}: unsupported schema version`);
  }
  if (manifest.source !== repositoryRelativePath(variant.source)) {
    variantMismatches.push(`${manifestPath}: source path does not match`);
  }
  if (manifest.sourceSha256 !== sha256(sourceContents)) {
    variantMismatches.push(`${manifestPath}: source hash does not match`);
  }
  const manifestPaths = Object.keys(manifest.outputs ?? {}).sort();
  if (JSON.stringify(manifestPaths) !== JSON.stringify(expectedPaths)) {
    variantMismatches.push(`${manifestPath}: output paths do not match`);
    return;
  }

  for (const relativePath of expectedPaths) {
    const filePath = join(variant.output, relativePath);
    let contents;
    try {
      contents = await readFile(filePath);
    } catch {
      variantMismatches.push(`${filePath}: missing`);
      continue;
    }
    const record = manifest.outputs[relativePath];
    if (record.size !== contents.length || record.sha256 !== sha256(contents)) {
      variantMismatches.push(`${filePath}: does not match generated icon manifest`);
      continue;
    }
    const validationError = validateNativeIcon(relativePath, contents);
    if (validationError) {
      variantMismatches.push(`${filePath}: ${validationError}`);
    }
  }
}

function expectedOutputPaths(variant) {
  return [
    'icon.icns',
    'icon.ico',
    'icon.png',
    `${variant.adaptiveArtwork}/01-artwork.png`,
    `${variant.adaptiveArtwork}/01-artwork-dark.png`,
    ...linuxSizes.map(size => `linux/${size}x${size}.png`),
  ].sort();
}

function validateNativeIcon(relativePath, contents) {
  if (relativePath.endsWith('.png')) {
    const signature = contents.subarray(0, 8).toString('hex');
    if (signature !== '89504e470d0a1a0a') {
      return 'invalid PNG signature';
    }
    const expectedSize = relativePath === 'icon.png'
      ? 512
      : relativePath.includes('/Assets/01-artwork')
        ? 1024
      : Number.parseInt(relativePath.match(/\/(\d+)x\d+\.png$/)?.[1] ?? '', 10);
    if (
      contents.readUInt32BE(16) !== expectedSize
      || contents.readUInt32BE(20) !== expectedSize
    ) {
      return `PNG dimensions do not match ${expectedSize}x${expectedSize}`;
    }
    return null;
  }
  if (relativePath === 'icon.ico') {
    if (contents.readUInt16LE(0) !== 0 || contents.readUInt16LE(2) !== 1) {
      return 'invalid ICO header';
    }
    const count = contents.readUInt16LE(4);
    const actualSizes = Array.from({ length: count }, (_, index) => {
      const value = contents.readUInt8(6 + (index * 16));
      return value === 0 ? 256 : value;
    });
    if (JSON.stringify(actualSizes) !== JSON.stringify(icoSizes)) {
      return 'ICO sizes do not match the release contract';
    }
    return null;
  }
  if (relativePath === 'icon.icns') {
    if (
      contents.subarray(0, 4).toString('ascii') !== 'icns'
      || contents.readUInt32BE(4) !== contents.length
    ) {
      return 'invalid ICNS header';
    }
    return null;
  }
  return 'unsupported generated icon format';
}

function repositoryRelativePath(filePath) {
  return relative(repositoryRoot, filePath).split('\\').join('/');
}

function sha256(contents) {
  return createHash('sha256').update(contents).digest('hex');
}

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
