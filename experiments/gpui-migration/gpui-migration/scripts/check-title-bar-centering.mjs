import sharp from "sharp";

const [imagePath, toleranceArgument = "1", maximumGlyphHeightArgument] = process.argv.slice(2);
if (!imagePath) {
  throw new Error("usage: node scripts/check-title-bar-centering.mjs <title-bar-crop> [tolerance-px]");
}
const tolerance = Number(toleranceArgument);
if (!Number.isFinite(tolerance) || tolerance < 0) {
  throw new Error("tolerance must be a non-negative number");
}
const maximumGlyphHeight = maximumGlyphHeightArgument === undefined
  ? undefined
  : Number(maximumGlyphHeightArgument);
if (maximumGlyphHeight !== undefined && (!Number.isFinite(maximumGlyphHeight) || maximumGlyphHeight < 1)) {
  throw new Error("maximum glyph height must be a positive number");
}

const { data, info } = await sharp(imagePath)
  .removeAlpha()
  .raw()
  .toBuffer({ resolveWithObject: true });
if (info.height > 40) {
  throw new Error(`expected a focused title-bar crop, received ${info.width}×${info.height}`);
}

let averageLuminance = 0;
for (let offset = 0; offset < data.length; offset += info.channels) {
  averageLuminance +=
    data[offset] * 0.2126 + data[offset + 1] * 0.7152 + data[offset + 2] * 0.0722;
}
averageLuminance /= info.width * info.height;
const darkAppearance = averageLuminance < 128;

let minX = info.width;
let maxX = -1;
let minY = info.height;
let maxY = -1;
for (let y = 2; y < info.height - 4; y += 1) {
  for (let x = 66; x < info.width - 10; x += 1) {
    const offset = (y * info.width + x) * info.channels;
    const luminance =
      data[offset] * 0.2126 + data[offset + 1] * 0.7152 + data[offset + 2] * 0.0722;
    const isForeground = darkAppearance
      ? luminance > averageLuminance + 35
      : luminance < averageLuminance - 35;
    if (isForeground) {
      minX = Math.min(minX, x);
      maxX = Math.max(maxX, x);
      minY = Math.min(minY, y);
      maxY = Math.max(maxY, y);
    }
  }
}
if (maxX < minX) {
  throw new Error("could not identify title foreground pixels");
}

const titleCentre = (minX + maxX) / 2;
const windowCentre = info.width / 2;
const delta = titleCentre - windowCentre;
const glyphHeight = maxY - minY + 1;
console.log(
  JSON.stringify({
    width: info.width,
    titleBounds: [minX, maxX],
    titleCentre,
    windowCentre,
    delta,
    tolerance,
    glyphHeight,
    maximumGlyphHeight,
  }),
);
if (
  Math.abs(delta) > tolerance ||
  (maximumGlyphHeight !== undefined && glyphHeight > maximumGlyphHeight)
) {
  process.exitCode = 1;
}
