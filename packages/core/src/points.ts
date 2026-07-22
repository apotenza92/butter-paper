export type Brand<K extends string> = {
  readonly __brand: K;
};

export type ScreenPoint = Readonly<{
  x: number;
  y: number;
}> & Brand<'ScreenPoint'>;

export type ViewportPoint = Readonly<{
  x: number;
  y: number;
}> & Brand<'ViewportPoint'>;

export type PdfPoint = Readonly<{
  x: number;
  y: number;
}> & Brand<'PdfPoint'>;

export type Size = Readonly<{
  width: number;
  height: number;
}>;

export type Rect = Readonly<{
  x: number;
  y: number;
  width: number;
  height: number;
}>;

export function screenPoint(x: number, y: number): ScreenPoint {
  return { x, y } as ScreenPoint;
}

export function viewportPoint(x: number, y: number): ViewportPoint {
  return { x, y } as ViewportPoint;
}

export function pdfPoint(x: number, y: number): PdfPoint {
  return { x, y } as PdfPoint;
}

export function size(width: number, height: number): Size {
  return { width, height };
}

export function rect(x: number, y: number, width: number, height: number): Rect {
  return { x, y, width, height };
}

export function translatePoint<T extends PointLike>(point: T, delta: PointLike): T {
  return {
    ...point,
    x: point.x + delta.x,
    y: point.y + delta.y,
  } as T;
}

export function scalePoint<T extends PointLike>(point: T, factor: number): T {
  return {
    ...point,
    x: point.x * factor,
    y: point.y * factor,
  } as T;
}

export function translateRect(box: Rect, delta: PointLike): Rect {
  return rect(box.x + delta.x, box.y + delta.y, box.width, box.height);
}

export function rectFromPoints(a: PointLike, b: PointLike): Rect {
  const x = Math.min(a.x, b.x);
  const y = Math.min(a.y, b.y);
  const width = Math.abs(a.x - b.x);
  const height = Math.abs(a.y - b.y);
  return rect(x, y, width, height);
}

export function normalizeRect(box: Rect): Rect {
  return rect(
    box.x + Math.min(0, box.width),
    box.y + Math.min(0, box.height),
    Math.abs(box.width),
    Math.abs(box.height),
  );
}

export type PointLike = Readonly<{
  x: number;
  y: number;
}>;
