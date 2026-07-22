import type { PdfCanvasFactory, PdfCanvasLike } from './types.js';

export async function createNodeCanvasFactory(): Promise<PdfCanvasFactory> {
  const canvasModule = await import('@napi-rs/canvas');
  return {
    create(width: number, height: number): PdfCanvasLike {
      return canvasModule.createCanvas(width, height) as unknown as PdfCanvasLike;
    },
    destroy(): void {
      return;
    },
  };
}

export function createBrowserCanvas(width: number, height: number): PdfCanvasLike {
  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  return canvas;
}
