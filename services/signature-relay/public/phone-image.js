export const MAX_SOURCE_IMAGE_BYTES = 10 * 1024 * 1024;
export const MAX_SOURCE_DIMENSION = 4_096;
export const MAX_SOURCE_PIXELS = 16 * 1024 * 1024;
export const MAX_ASPECT_RATIO = 25;
export const MAX_OUTPUT_DIMENSION = 2_048;
export const MIN_OUTPUT_DIMENSION = 128;

const DOWNSCALE_FACTOR = 0.75;

export function processSignaturePixels({ data, width, height }) {
  const strength = new Float32Array(width * height);
  for (let pixel = 0; pixel < width * height; pixel += 1) {
    const offset = pixel * 4;
    const luminance = (data[offset] * 0.2126) + (data[offset + 1] * 0.7152) + (data[offset + 2] * 0.0722);
    strength[pixel] = ((255 - luminance) / 255) * (data[offset + 3] / 255);
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
      let weightedStrength = strength[pixel] * 4;
      let weight = 4;
      if (x > 0) { weightedStrength += strength[pixel - 1]; weight += 1; }
      if (x + 1 < width) { weightedStrength += strength[pixel + 1]; weight += 1; }
      if (y > 0) { weightedStrength += strength[pixel - width]; weight += 1; }
      if (y + 1 < height) { weightedStrength += strength[pixel + width]; weight += 1; }
      const scaled = Math.max(0, Math.min(1, ((weightedStrength / weight) - 0.1) / 0.3));
      const alpha = Math.round(255 * scaled * scaled * (3 - 2 * scaled));
      const offset = pixel * 4;
      normalized.set([17, 24, 39, alpha], offset);
      if (alpha >= 16) {
        inkPixelCount += 1;
        left = Math.min(left, x);
        top = Math.min(top, y);
        right = Math.max(right, x);
        bottom = Math.max(bottom, y);
      }
    }
  }
  if (right < left || bottom < top) throw new Error('No signature was found. Use dark ink on white paper.');
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
    const start = ((top + y) * width + left) * 4;
    output.set(normalized.subarray(start, start + outputWidth * 4), y * outputWidth * 4);
  }
  return { data: output, width: outputWidth, height: outputHeight };
}

export function planSanitizedDimensions(width, height) {
  if (!Number.isInteger(width) || !Number.isInteger(height) || width <= 0 || height <= 0) {
    throw new Error('The image dimensions are invalid.');
  }
  if (width > MAX_SOURCE_DIMENSION || height > MAX_SOURCE_DIMENSION) {
    throw new Error('The image dimensions are too large.');
  }
  if (width * height > MAX_SOURCE_PIXELS) throw new Error('The image contains too many pixels.');
  const aspectRatio = Math.max(width / height, height / width);
  if (aspectRatio > MAX_ASPECT_RATIO) throw new Error('The image aspect ratio is too extreme.');

  const scale = Math.min(1, MAX_OUTPUT_DIMENSION / Math.max(width, height));
  return {
    width: Math.max(1, Math.round(width * scale)),
    height: Math.max(1, Math.round(height * scale)),
  };
}

const browserImageAdapter = {
  async inspect(file) {
    return readImageHeaderDimensions(new Uint8Array(await file.arrayBuffer()), file.type);
  },
  decode(file) {
    return createImageBitmap(file, { imageOrientation: 'from-image' });
  },
  draw(source, width, height, mediaType) {
    const canvas = document.createElement('canvas');
    canvas.width = width;
    canvas.height = height;
    const context = canvas.getContext('2d');
    if (!context) throw new Error('This browser cannot process the image.');
    if (mediaType === 'image/jpeg') {
      context.fillStyle = '#ffffff';
      context.fillRect(0, 0, width, height);
    }
    context.drawImage(source, 0, 0, width, height);
    return canvas;
  },
  process(canvas) {
    const context = canvas.getContext('2d');
    if (!context) throw new Error('This browser cannot process the image.');
    const pixels = context.getImageData(0, 0, canvas.width, canvas.height);
    const processed = processSignaturePixels({ data: pixels.data, width: canvas.width, height: canvas.height });
    const output = document.createElement('canvas');
    output.width = processed.width;
    output.height = processed.height;
    const outputContext = output.getContext('2d');
    if (!outputContext) throw new Error('This browser cannot process the image.');
    const outputPixels = outputContext.createImageData(processed.width, processed.height);
    outputPixels.data.set(processed.data);
    outputContext.putImageData(outputPixels, 0, 0);
    return output;
  },
  encode(canvas, mediaType, quality) {
    return new Promise((resolve, reject) => {
      canvas.toBlob(
        (blob) => {
          if (blob) resolve(blob);
          else reject(new Error('The image could not be re-encoded.'));
        },
        mediaType,
        quality,
      );
    });
  },
};

export async function sanitizeImageFile(file, adapter = browserImageAdapter) {
  if (file.type !== 'image/png' && file.type !== 'image/jpeg') throw new Error('Use a PNG or JPEG image.');
  if (file.size > MAX_SOURCE_IMAGE_BYTES) throw new Error('The source image must be 10 MiB or smaller.');

  const inspected = await adapter.inspect(file);
  planSanitizedDimensions(inspected.width, inspected.height);
  const source = await adapter.decode(file);
  try {
    let { width, height } = planSanitizedDimensions(source.width, source.height);

    while (true) {
      const sourceCanvas = adapter.draw(source, width, height, file.type);
      const canvas = adapter.process ? adapter.process(sourceCanvas) : sourceCanvas;
      const output = await adapter.encode(canvas, 'image/png');
      if (output.size <= 1024 * 1024) return output;

      if (Math.max(width, height) <= MIN_OUTPUT_DIMENSION) break;
      width = Math.max(1, Math.round(width * DOWNSCALE_FACTOR));
      height = Math.max(1, Math.round(height * DOWNSCALE_FACTOR));
    }
  } finally {
    source.close?.();
  }

  throw new Error('The processed image is still larger than 1 MiB. Choose a simpler or smaller image.');
}

export function readImageHeaderDimensions(bytes, mediaType) {
  if (!(bytes instanceof Uint8Array)) throw new TypeError('Image bytes are invalid.');
  if (mediaType === 'image/png') {
    if (bytes.length < 24
      || bytes[0] !== 0x89 || bytes[1] !== 0x50 || bytes[2] !== 0x4e || bytes[3] !== 0x47
      || bytes[12] !== 0x49 || bytes[13] !== 0x48 || bytes[14] !== 0x44 || bytes[15] !== 0x52) {
      throw new Error('The PNG header is invalid.');
    }
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    return { width: view.getUint32(16, false), height: view.getUint32(20, false) };
  }
  if (mediaType !== 'image/jpeg' || bytes.length < 4 || bytes[0] !== 0xff || bytes[1] !== 0xd8) {
    throw new Error('The JPEG header is invalid.');
  }
  let offset = 2;
  while (offset + 8 < bytes.length) {
    while (bytes[offset] === 0xff) offset += 1;
    const marker = bytes[offset++];
    if (marker === undefined || marker === 0xd9 || marker === 0xda) break;
    if (marker === 0x01 || (marker >= 0xd0 && marker <= 0xd8)) continue;
    if (offset + 1 >= bytes.length) break;
    const segmentLength = (bytes[offset] << 8) | bytes[offset + 1];
    if (segmentLength < 2 || offset + segmentLength > bytes.length) break;
    if ([0xc0, 0xc1, 0xc2, 0xc3, 0xc5, 0xc6, 0xc7, 0xc9, 0xca, 0xcb, 0xcd, 0xce, 0xcf].includes(marker)) {
      if (segmentLength < 7) break;
      return {
        height: (bytes[offset + 3] << 8) | bytes[offset + 4],
        width: (bytes[offset + 5] << 8) | bytes[offset + 6],
      };
    }
    offset += segmentLength;
  }
  throw new Error('The JPEG dimensions are invalid.');
}
