export interface SignaturePixelImage {
  readonly data: Uint8ClampedArray;
  readonly width: number;
  readonly height: number;
}

export interface ProcessedSignaturePixels extends SignaturePixelImage {}

/**
 * Converts dark marks on a light background into normalized ink on transparency.
 * The small cross-shaped blur softens camera noise and stair-stepped edges without
 * introducing a native image-processing dependency.
 */
export function processSignaturePixels(image: SignaturePixelImage): ProcessedSignaturePixels {
  const INK_RED = 17;
  const INK_GREEN = 24;
  const INK_BLUE = 39;
  const BACKGROUND_CUTOFF = 0.1;
  const SOLID_INK_CUTOFF = 0.4;
  const CROP_ALPHA_CUTOFF = 16;
  const { data, width, height } = image;
  if (!Number.isInteger(width) || !Number.isInteger(height) || width <= 0 || height <= 0
    || data.length !== width * height * 4) {
    throw new TypeError('Signature pixel data is invalid.');
  }

  const strength = new Float32Array(width * height);
  for (let pixel = 0; pixel < width * height; pixel += 1) {
    const offset = pixel * 4;
    const luminance = (data[offset]! * 0.2126) + (data[offset + 1]! * 0.7152) + (data[offset + 2]! * 0.0722);
    strength[pixel] = ((255 - luminance) / 255) * (data[offset + 3]! / 255);
  }

  const normalized = new Uint8ClampedArray(data.length);
  let left = width;
  let top = height;
  let right = -1;
  let bottom = -1;
  let inkPixelCount = 0;
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const pixel = y * width + x;
      let weightedStrength = strength[pixel]! * 4;
      let weight = 4;
      if (x > 0) { weightedStrength += strength[pixel - 1]!; weight += 1; }
      if (x + 1 < width) { weightedStrength += strength[pixel + 1]!; weight += 1; }
      if (y > 0) { weightedStrength += strength[pixel - width]!; weight += 1; }
      if (y + 1 < height) { weightedStrength += strength[pixel + width]!; weight += 1; }
      const alpha = Math.round(255 * smoothstep(
        BACKGROUND_CUTOFF,
        SOLID_INK_CUTOFF,
        weightedStrength / weight,
      ));
      const offset = pixel * 4;
      normalized[offset] = INK_RED;
      normalized[offset + 1] = INK_GREEN;
      normalized[offset + 2] = INK_BLUE;
      normalized[offset + 3] = alpha;
      if (alpha >= CROP_ALPHA_CUTOFF) {
        inkPixelCount += 1;
        left = Math.min(left, x);
        top = Math.min(top, y);
        right = Math.max(right, x);
        bottom = Math.max(bottom, y);
      }
    }
  }

  if (right < left || bottom < top) {
    throw new Error('No signature was found. Use dark ink on white paper.');
  }
  if (inkPixelCount / (width * height) > 0.35) {
    throw new Error('Too much background was detected. Fill the view with white paper.');
  }

  const padding = Math.max(4, Math.ceil(Math.max(right - left + 1, bottom - top + 1) * 0.04));
  left = Math.max(0, left - padding);
  top = Math.max(0, top - padding);
  right = Math.min(width - 1, right + padding);
  bottom = Math.min(height - 1, bottom + padding);
  const outputWidth = right - left + 1;
  const outputHeight = bottom - top + 1;
  const output = new Uint8ClampedArray(outputWidth * outputHeight * 4);
  for (let y = 0; y < outputHeight; y += 1) {
    const sourceStart = ((top + y) * width + left) * 4;
    const sourceEnd = sourceStart + outputWidth * 4;
    output.set(normalized.subarray(sourceStart, sourceEnd), y * outputWidth * 4);
  }

  return { data: output, width: outputWidth, height: outputHeight };

  function smoothstep(minimum: number, maximum: number, value: number): number {
    const scaled = Math.max(0, Math.min(1, (value - minimum) / (maximum - minimum)));
    return scaled * scaled * (3 - 2 * scaled);
  }
}
