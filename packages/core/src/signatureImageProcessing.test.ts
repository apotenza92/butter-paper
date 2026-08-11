import { describe, expect, it } from 'vitest';
import { processSignaturePixels } from './signatureImageProcessing.js';

describe('signature image processing', () => {
  it('removes white paper, normalizes ink, smooths edges, and crops the result', () => {
    const pixels = solidImage(20, 12, [255, 255, 255, 255]);
    paintRect(pixels, 20, 6, 5, 8, 2, [0, 0, 0, 255]);

    const processed = processSignaturePixels({ data: pixels, width: 20, height: 12 });

    expect(processed.width).toBeLessThan(20);
    expect(processed.height).toBe(10);
    expect([...processed.data].some((value, index) => index % 4 === 3 && value === 255)).toBe(true);
    expect([...processed.data].some((value, index) => index % 4 === 3 && value > 0 && value < 255)).toBe(true);
    const firstOpaqueOffset = [...processed.data].findIndex((value, index) => index % 4 === 3 && value > 0) - 3;
    expect([...processed.data.slice(firstOpaqueOffset, firstOpaqueOffset + 3)]).toEqual([17, 24, 39]);
  });

  it('preserves existing transparency while cleaning drawn ink', () => {
    const pixels = solidImage(12, 8, [0, 0, 0, 0]);
    paintRect(pixels, 12, 3, 3, 6, 2, [0, 0, 0, 255]);

    const processed = processSignaturePixels({ data: pixels, width: 12, height: 8 });

    expect(processed.data[3]).toBe(0);
    expect(Math.max(...processed.data.filter((_value, index) => index % 4 === 3))).toBe(255);
  });

  it('rejects blank or invalid images', () => {
    expect(() => processSignaturePixels({
      data: solidImage(8, 8, [255, 255, 255, 255]),
      width: 8,
      height: 8,
    })).toThrow('No signature was found');
    expect(() => processSignaturePixels({ data: new Uint8ClampedArray(3), width: 1, height: 1 }))
      .toThrow('pixel data is invalid');
  });

  it('rejects a scene that is not mostly light paper', () => {
    expect(() => processSignaturePixels({
      data: solidImage(20, 20, [30, 30, 30, 255]),
      width: 20,
      height: 20,
    })).toThrow('Fill the view with white paper');
  });

  it('is self-contained for the isolated desktop sanitizer', () => {
    const standalone = Function(`return (${processSignaturePixels.toString()})`)() as typeof processSignaturePixels;
    const pixels = solidImage(8, 8, [255, 255, 255, 255]);
    paintRect(pixels, 8, 3, 3, 2, 2, [0, 0, 0, 255]);

    expect(standalone({ data: pixels, width: 8, height: 8 }).width).toBeGreaterThan(0);
  });
});

function solidImage(width: number, height: number, color: readonly [number, number, number, number]): Uint8ClampedArray {
  const pixels = new Uint8ClampedArray(width * height * 4);
  for (let offset = 0; offset < pixels.length; offset += 4) pixels.set(color, offset);
  return pixels;
}

function paintRect(
  pixels: Uint8ClampedArray,
  imageWidth: number,
  x: number,
  y: number,
  width: number,
  height: number,
  color: readonly [number, number, number, number],
): void {
  for (let row = y; row < y + height; row += 1) {
    for (let column = x; column < x + width; column += 1) {
      pixels.set(color, (row * imageWidth + column) * 4);
    }
  }
}
