import type { PdfPoint, Rect } from '@butter-paper/core';
import { pdfPoint, rect } from '@butter-paper/core';

export function normalizePdfRect(box: Rect | readonly number[]): Rect {
  if ('x' in box) {
    return rect(
      Math.min(box.x, box.x + box.width),
      Math.min(box.y, box.y + box.height),
      Math.abs(box.width),
      Math.abs(box.height),
    );
  }

  const [x1, y1, x2, y2] = box;
  return rect(Math.min(x1, x2), Math.min(y1, y2), Math.abs(x2 - x1), Math.abs(y2 - y1));
}

export function pdfRectToQuadPoints(box: Rect): readonly number[] {
  const x1 = box.x;
  const y1 = box.y;
  const x2 = box.x + box.width;
  const y2 = box.y + box.height;
  return [x1, y2, x2, y2, x1, y1, x2, y1];
}

export function quadPointsToRect(quadPoints: readonly number[]): Rect {
  const xs = [] as number[];
  const ys = [] as number[];
  for (let index = 0; index < quadPoints.length; index += 2) {
    xs.push(quadPoints[index]);
    ys.push(quadPoints[index + 1]);
  }

  const minX = Math.min(...xs);
  const maxX = Math.max(...xs);
  const minY = Math.min(...ys);
  const maxY = Math.max(...ys);
  return rect(minX, minY, maxX - minX, maxY - minY);
}

export function pointArrayToPdfPoints(points: readonly number[] | undefined): readonly PdfPoint[] {
  if (!points || points.length < 4) {
    return [];
  }

  const converted: PdfPoint[] = [];
  for (let index = 0; index < points.length; index += 2) {
    converted.push(pdfPoint(points[index], points[index + 1]));
  }
  return converted;
}
