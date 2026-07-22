import {
  createLineMarkup,
  createPenMarkup,
  createPolylineMarkup,
  pdfPoint,
  type ButterCanvasTraceOutputMode,
  type ButterCanvasTraceZone,
  type Markup,
  type Rect,
} from '@butter-paper/core';

export interface TraceImageSource {
  readonly width: number;
  readonly height: number;
  readonly data: Uint8ClampedArray | readonly number[];
}

export interface TraceImageToMarkupsParams {
  readonly image: TraceImageSource;
  readonly assetRect: Rect;
  readonly sensitivity: number;
  readonly zone?: ButterCanvasTraceZone | null;
  readonly pageIndex?: number;
  readonly idPrefix?: string;
  readonly color?: string;
  readonly outputMode?: ButterCanvasTraceOutputMode;
  readonly maxSegments?: number;
  readonly minSegmentPixels?: number;
}

interface Segment {
  readonly orientation: 'horizontal' | 'vertical';
  readonly startX: number;
  readonly startY: number;
  readonly endX: number;
  readonly endY: number;
}

export function traceImageToMarkups({
  image,
  assetRect,
  sensitivity,
  zone = null,
  pageIndex = 0,
  idPrefix = 'trace',
  color = '#2563eb',
  outputMode = 'polyline',
  maxSegments = 400,
  minSegmentPixels = 8,
}: TraceImageToMarkupsParams): Markup[] {
  if (image.width <= 0 || image.height <= 0 || assetRect.width <= 0 || assetRect.height <= 0) {
    return [];
  }

  const bounds = traceBounds(image.width, image.height, zone);
  const threshold = thresholdForSensitivity(sensitivity);
  const segments: Segment[] = [];

  for (let y = bounds.minY; y <= bounds.maxY && segments.length < maxSegments; y += 1) {
    collectLineSegments({
      fixed: y,
      min: bounds.minX,
      max: bounds.maxX,
      threshold,
      image,
      minSegmentPixels,
      orientation: 'horizontal',
      segments,
      maxSegments,
    });
  }

  for (let x = bounds.minX; x <= bounds.maxX && segments.length < maxSegments; x += 1) {
    collectLineSegments({
      fixed: x,
      min: bounds.minY,
      max: bounds.maxY,
      threshold,
      image,
      minSegmentPixels,
      orientation: 'vertical',
      segments,
      maxSegments,
    });
  }

  return segments.map((segment, index) => {
    const id = `${idPrefix}-${index + 1}`;
    const start = imagePointToCanvasPoint(segment.startX, segment.startY, image, assetRect);
    const end = imagePointToCanvasPoint(segment.endX, segment.endY, image, assetRect);
    if (outputMode === 'line') {
      return createLineMarkup({ id, pageIndex, color, start, end });
    }
    if (outputMode === 'pen') {
      return createPenMarkup({ id, pageIndex, color, paths: [[start, end]] });
    }
    return createPolylineMarkup({ id, pageIndex, color, points: [start, end] });
  });
}

function traceBounds(width: number, height: number, zone: ButterCanvasTraceZone | null): {
  readonly minX: number;
  readonly maxX: number;
  readonly minY: number;
  readonly maxY: number;
} {
  if (!zone) {
    return { minX: 0, maxX: width - 1, minY: 0, maxY: height - 1 };
  }

  const x1 = Math.max(0, Math.min(width - 1, Math.floor(zone.x * width)));
  const y1 = Math.max(0, Math.min(height - 1, Math.floor(zone.y * height)));
  const x2 = Math.max(0, Math.min(width - 1, Math.ceil((zone.x + zone.width) * width) - 1));
  const y2 = Math.max(0, Math.min(height - 1, Math.ceil((zone.y + zone.height) * height) - 1));
  return {
    minX: Math.min(x1, x2),
    maxX: Math.max(x1, x2),
    minY: Math.min(y1, y2),
    maxY: Math.max(y1, y2),
  };
}

function thresholdForSensitivity(sensitivity: number): number {
  const clamped = Math.max(0, Math.min(100, sensitivity));
  return 40 + clamped * 2.1;
}

function collectLineSegments({
  fixed,
  min,
  max,
  threshold,
  image,
  minSegmentPixels,
  orientation,
  segments,
  maxSegments,
}: {
  readonly fixed: number;
  readonly min: number;
  readonly max: number;
  readonly threshold: number;
  readonly image: TraceImageSource;
  readonly minSegmentPixels: number;
  readonly orientation: Segment['orientation'];
  readonly segments: Segment[];
  readonly maxSegments: number;
}): void {
  let runStart: number | null = null;
  for (let variable = min; variable <= max + 1; variable += 1) {
    const inside = variable <= max;
    const x = orientation === 'horizontal' ? variable : fixed;
    const y = orientation === 'horizontal' ? fixed : variable;
    const dark = inside && isDarkPixel(image, x, y, threshold);
    if (dark) {
      runStart ??= variable;
      continue;
    }

    if (runStart === null) {
      continue;
    }

    const runEnd = variable - 1;
    if (runEnd - runStart + 1 >= minSegmentPixels) {
      segments.push(orientation === 'horizontal'
        ? { orientation, startX: runStart, startY: fixed, endX: runEnd, endY: fixed }
        : { orientation, startX: fixed, startY: runStart, endX: fixed, endY: runEnd });
      if (segments.length >= maxSegments) {
        return;
      }
    }
    runStart = dark ? variable : null;
  }
}

function isDarkPixel(image: TraceImageSource, x: number, y: number, threshold: number): boolean {
  const index = (y * image.width + x) * 4;
  const red = image.data[index] ?? 255;
  const green = image.data[index + 1] ?? 255;
  const blue = image.data[index + 2] ?? 255;
  const alpha = image.data[index + 3] ?? 255;
  if (alpha < 16) {
    return false;
  }
  const luminance = 0.2126 * red + 0.7152 * green + 0.0722 * blue;
  return luminance <= threshold;
}

function imagePointToCanvasPoint(x: number, y: number, image: TraceImageSource, assetRect: Rect) {
  return pdfPoint(
    assetRect.x + (x / Math.max(1, image.width - 1)) * assetRect.width,
    assetRect.y + (y / Math.max(1, image.height - 1)) * assetRect.height,
  );
}
