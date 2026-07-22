import type { PdfCacheStats, PdfRenderedPage } from './types.js';

interface CacheEntry {
  readonly key: string;
  readonly page: PdfRenderedPage;
  readonly bytes: number;
}

export class PdfRenderCache {
  private readonly entries = new Map<string, CacheEntry>();

  constructor(
    private readonly limits: { maxEntries: number; maxBytes: number },
  ) {}

  get(key: string): PdfRenderedPage | undefined {
    const entry = this.entries.get(key);
    if (!entry) {
      return undefined;
    }

    this.entries.delete(key);
    this.entries.set(key, entry);
    return entry.page;
  }

  set(key: string, page: PdfRenderedPage): void {
    const bytes = estimateSurfaceBytes(page.canvas, page.width, page.height);
    if (this.entries.has(key)) {
      this.entries.delete(key);
    }

    this.entries.set(key, { key, page, bytes });
    this.evictIfNeeded();
  }

  clear(): void {
    this.entries.clear();
  }

  stats(): PdfCacheStats {
    let estimatedBytes = 0;
    for (const entry of this.entries.values()) {
      estimatedBytes += entry.bytes;
    }

    return {
      entries: this.entries.size,
      estimatedBytes,
      maxEntries: this.limits.maxEntries,
      maxBytes: this.limits.maxBytes,
    };
  }

  private evictIfNeeded(): void {
    while (this.entries.size > this.limits.maxEntries || this.stats().estimatedBytes > this.limits.maxBytes) {
      const oldestKey = this.entries.keys().next().value as string | undefined;
      if (!oldestKey) {
        return;
      }

      this.entries.delete(oldestKey);
    }
  }
}

function estimateSurfaceBytes(canvas: { width: number; height: number }, width: number, height: number): number {
  if (width > 0 && height > 0) {
    return width * height * 4;
  }

  return canvas.width * canvas.height * 4;
}
