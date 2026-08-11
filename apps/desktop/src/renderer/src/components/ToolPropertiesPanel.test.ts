import { describe, expect, it } from 'vitest';
import { pdfPoint, rect, type RectangleMarkup } from '@butter-paper/core';
import { prototypeValuesForMarkup } from './ToolPropertiesPanel';

describe('ToolPropertiesPanel', () => {
  it('derives the displayed values from the selected markup', () => {
    const markup: RectangleMarkup = {
      id: 'rectangle-1',
      kind: 'rectangle',
      pageIndex: 0,
      rect: rect(12, 24, 80, 40),
      rotation: 15,
      appearance: {
        stroke: { color: '#123456', widthPt: 2.5 },
        fill: { color: '#abcdef' },
        opacity: 0.6,
      },
      source: {
        annotationMetadata: [{
          annotationId: 'annotation-1',
          subject: 'Door',
          contents: 'Check clearance',
        }],
      },
    };

    expect(prototypeValuesForMarkup(markup)).toMatchObject({
      subject: 'Door',
      comments: 'Check clearance',
      strokeColor: '#123456',
      fillColor: '#abcdef',
      opacity: 60,
      lineWidth: 2.5,
      x: 12,
      y: 24,
      width: 80,
      height: 40,
      rotation: 15,
    });
  });

  it('uses the selected line bounds for layout values', () => {
    const values = prototypeValuesForMarkup({
      id: 'line-1',
      kind: 'line',
      pageIndex: 0,
      start: pdfPoint(50, 10),
      end: pdfPoint(20, 35),
    });

    expect(values).toMatchObject({ x: 20, y: 10, width: 30, height: 25 });
  });
});
