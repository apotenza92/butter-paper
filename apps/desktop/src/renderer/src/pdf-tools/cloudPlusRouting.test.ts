import { describe, expect, it } from 'vitest';
import { pdfPoint, rect, type PdfPoint, type Rect } from '@butter-paper/core';
import { CLOUD_LINE_TYPE_RENDERER, DEFAULT_CLOUD_LINE_OPTIONS } from './lineTypes';
import {
  isRectWhollyInsidePolygon,
  placeInitialCloudPlusTextBox,
  routeCloudPlusLeader,
  snapCloudPlusLeaderTip,
  type CloudPlusObstacle,
} from './cloudPlusRouting';

describe('Cloud+ routing', () => {
  const controlPath = rectanglePath(rect(10, 10, 80, 50));
  const visiblePath = cloudVisiblePath(controlPath);

  it.each([
    [['right'], rect(-120, 15, 100, 40)],
    [['left'], rect(130, 15, 100, 40)],
    [['left', 'right'], rect(0, -70, 100, 40)],
    [['left', 'right'], rect(0, 100, 100, 40)],
  ] as const)('builds a clean native vertically-centred three-point leader for %s', (allowedSides, textBox) => {
    const route = routeCloudPlusLeader({ controlPath, visiblePath, textBox });
    const connection = route.points[2];

    expect(allowedSides).toContain(route.side);
    expect(route.points).toHaveLength(3);
    expect(connection.y).toBe(textBox.y + textBox.height * 0.5);
    expect(connection.x).toBe(route.side === 'left' ? textBox.x : textBox.x + textBox.width);
    expect(distanceToPolyline(route.points[0], visiblePath)).toBeLessThan(0.000001);
    expect(route.points[1].y).toBe(connection.y);
    expect(route.points[1].x).toBeCloseTo((route.points[0].x + connection.x) * 0.5);
  });

  it('hides the leader only when the entire text box is inside the cloud', () => {
    const largeCloud = rectanglePath(rect(0, 0, 200, 120));
    const largeVisible = cloudVisiblePath(largeCloud);

    expect(routeCloudPlusLeader({
      controlPath: largeCloud,
      visiblePath: largeVisible,
      textBox: rect(35, 35, 100, 40),
    }).points).toEqual([]);

    // The center is inside, but the left half is outside.
    expect(routeCloudPlusLeader({
      controlPath: largeCloud,
      visiblePath: largeVisible,
      textBox: rect(-30, 35, 100, 40),
    }).points).toHaveLength(3);
  });

  it('does not treat a box crossed by a concave notch as wholly inline', () => {
    const concave = [
      pdfPoint(0, 0), pdfPoint(120, 0), pdfPoint(120, 120),
      pdfPoint(70, 120), pdfPoint(70, 45), pdfPoint(50, 45),
      pdfPoint(50, 120), pdfPoint(0, 120),
    ];
    const boxAcrossNotch = rect(40, 30, 40, 70);

    expect(isRectWhollyInsidePolygon(boxAcrossNotch, concave)).toBe(false);
    expect(isRectWhollyInsidePolygon(boxAcrossNotch, [...concave].reverse())).toBe(false);
    expect(routeCloudPlusLeader({
      controlPath: concave,
      visiblePath: cloudVisiblePath(concave),
      textBox: boxAcrossNotch,
    }).points).toHaveLength(3);
  });

  it('routes around a crossing while retaining the aesthetically facing attachment edge', () => {
    const textBox = rect(130, 15, 100, 40);
    const unobstructed = routeCloudPlusLeader({ controlPath, visiblePath, textBox });
    const blockingLine: CloudPlusObstacle = {
      kind: 'polyline',
      points: [pdfPoint(115, 0), pdfPoint(115, 80)],
    };
    const rerouted = routeCloudPlusLeader({ controlPath, visiblePath, textBox, obstacles: [blockingLine] });

    expect(unobstructed.side).toBe('left');
    expect(rerouted.side).toBe('left');
    expect(rerouted.points).toHaveLength(3);
    expect(rerouted.points[1].y < 0 || rerouted.points[1].y > 80).toBe(true);
  });

  it('uses the prior text edge as hysteresis for otherwise tied placements', () => {
    const square = rectanglePath(rect(0, 0, 100, 100));
    const squareVisible = cloudVisiblePath(square);
    const textBox = rect(120, 120, 40, 40);
    const fromLeftEdge = [pdfPoint(100, 130), pdfPoint(110, 130), pdfPoint(120, 140)];
    const fromBottomEdge = [pdfPoint(130, 100), pdfPoint(130, 110), pdfPoint(140, 120)];

    expect(routeCloudPlusLeader({ controlPath: square, visiblePath: squareVisible, textBox, previousLeader: fromLeftEdge }).side).toBe('left');
    expect(routeCloudPlusLeader({ controlPath: square, visiblePath: squareVisible, textBox, previousLeader: fromBottomEdge }).side).toBe('left');
  });

  it('places new text on a side that remains on the page', () => {
    const nearRightEdge = rectanglePath(rect(500, 300, 100, 80));
    const placement = placeInitialCloudPlusTextBox({
      controlPath: nearRightEdge,
      visiblePath: cloudVisiblePath(nearRightEdge),
      width: 150,
      height: 44,
      pageBounds: rect(0, 0, 612, 792),
    });

    expect(placement.textBox.x + placement.textBox.width).toBeLessThanOrEqual(612);
    expect(placement.textBox.x).toBeLessThan(500);
    expect(placement.leader.side).toBe('right');
  });

  it('avoids occupied space during deterministic initial placement', () => {
    const rightSideObstacle: CloudPlusObstacle = { kind: 'rect', rect: rect(105, -40, 200, 180) };
    const first = placeInitialCloudPlusTextBox({
      controlPath,
      visiblePath,
      width: 100,
      height: 40,
      obstacles: [rightSideObstacle],
    });
    const second = placeInitialCloudPlusTextBox({
      controlPath,
      visiblePath,
      width: 100,
      height: 40,
      obstacles: [rightSideObstacle],
    });

    expect(first).toEqual(second);
    expect(first.textBox.x).toBeLessThan(10);
  });

  it('prefers an unblocked placement over even a tiny text collision', () => {
    const tinyRightCollision: CloudPlusObstacle = { kind: 'rect', rect: rect(114, 36, 1, 1) };
    const placement = placeInitialCloudPlusTextBox({
      controlPath,
      visiblePath,
      width: 100,
      height: 40,
      obstacles: [tinyRightCollision],
    });

    expect(placement.textBox.x).toBeLessThan(10);
  });

  it('snaps a manually moved tip to the sampled scalloped outline', () => {
    const target = pdfPoint(94, 33);
    const snapped = snapCloudPlusLeaderTip(visiblePath, target);

    expect(distanceToPolyline(snapped, visiblePath)).toBeLessThan(0.000001);
    expect(Math.hypot(snapped.x - target.x, snapped.y - target.y)).toBeLessThan(15);
  });

  it('is translation invariant for cloud, label, page and obstacles', () => {
    const textBox = rect(130, 15, 100, 40);
    const obstacle: CloudPlusObstacle = { kind: 'rect', rect: rect(102, 25, 12, 20) };
    const original = routeCloudPlusLeader({ controlPath, visiblePath, textBox, obstacles: [obstacle], pageBounds: rect(-200, -200, 700, 700) });
    const delta = pdfPoint(301.25, -83.5);
    const translated = routeCloudPlusLeader({
      controlPath: translatePoints(controlPath, delta),
      visiblePath: translatePoints(visiblePath, delta),
      textBox: translateRect(textBox, delta),
      obstacles: [{ kind: 'rect', rect: translateRect(obstacle.rect, delta) }],
      pageBounds: translateRect(rect(-200, -200, 700, 700), delta),
    });

    expect(translated.side).toBe(original.side);
    translated.points.forEach((point, index) => {
      expect(point.x).toBeCloseTo(original.points[index].x + delta.x);
      expect(point.y).toBeCloseTo(original.points[index].y + delta.y);
    });
  });

  it('stays finite and deterministic for tiny, reversed and seeded irregular polygons', () => {
    let seed = 0x5eed1234;
    const random = () => {
      seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
      return seed / 0x1_0000_0000;
    };
    const cases: PdfPoint[][] = [
      [pdfPoint(0, 0), pdfPoint(0.01, 0), pdfPoint(0.01, 0.01), pdfPoint(0, 0.01)],
      [pdfPoint(0, 0), pdfPoint(0, 0), pdfPoint(40, 0), pdfPoint(40, 30), pdfPoint(0, 30)],
    ];
    for (let caseIndex = 0; caseIndex < 40; caseIndex += 1) {
      const count = 3 + Math.floor(random() * 6);
      const points = Array.from({ length: count }, (_, index) => {
        const angle = (Math.PI * 2 * index) / count;
        const radius = 25 + random() * 75;
        return pdfPoint(Math.cos(angle) * radius, Math.sin(angle) * radius);
      });
      cases.push(caseIndex % 2 === 0 ? points : points.reverse());
    }

    for (const polygon of cases) {
      const visible = cloudVisiblePath(polygon);
      const textBox = rect(140, -20, 80, 40);
      const first = routeCloudPlusLeader({ controlPath: polygon, visiblePath: visible, textBox });
      const second = routeCloudPlusLeader({ controlPath: polygon, visiblePath: visible, textBox });
      expect(first).toEqual(second);
      expect(first.points).toHaveLength(3);
      for (const point of first.points) {
        expect(Number.isFinite(point.x)).toBe(true);
        expect(Number.isFinite(point.y)).toBe(true);
      }
    }
  });
});

function rectanglePath(box: Rect): readonly PdfPoint[] {
  return [
    pdfPoint(box.x, box.y),
    pdfPoint(box.x, box.y + box.height),
    pdfPoint(box.x + box.width, box.y + box.height),
    pdfPoint(box.x + box.width, box.y),
  ];
}

function cloudVisiblePath(controlPath: readonly PdfPoint[]): readonly PdfPoint[] {
  return CLOUD_LINE_TYPE_RENDERER.render({
    controlPath,
    closed: true,
    strokeWidth: 1,
    options: DEFAULT_CLOUD_LINE_OPTIONS,
  }).points;
}

function distanceToPolyline(target: PdfPoint, points: readonly PdfPoint[]): number {
  let closest = Infinity;
  for (let index = 0; index < points.length - 1; index += 1) {
    const start = points[index];
    const end = points[index + 1];
    const dx = end.x - start.x;
    const dy = end.y - start.y;
    const denominator = dx * dx + dy * dy;
    const amount = denominator === 0 ? 0 : Math.max(0, Math.min(1, ((target.x - start.x) * dx + (target.y - start.y) * dy) / denominator));
    closest = Math.min(closest, Math.hypot(start.x + dx * amount - target.x, start.y + dy * amount - target.y));
  }
  return closest;
}

function translatePoints(points: readonly PdfPoint[], delta: PdfPoint): readonly PdfPoint[] {
  return points.map((point) => pdfPoint(point.x + delta.x, point.y + delta.y));
}

function translateRect(box: Rect, delta: PdfPoint): Rect {
  return rect(box.x + delta.x, box.y + delta.y, box.width, box.height);
}
