import { describe, expect, it } from 'vitest';
import { createTextBoxMarkup, pdfPoint, rect, resolveMarkupAppearance, type RectangleMarkup } from '@butter-paper/core';
import { PDF_TOOL_REGISTRY } from '../pdf-tools/toolRegistry';
import { groupToolProperties, prototypeValuesForMarkup, toolPropertyCategory, updateSelectedMarkupProperty } from './ToolPropertiesPanel';

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
        annotationMetadata: [
          {
            annotationId: 'annotation-1',
            subject: 'Door',
            contents: 'Check clearance',
          },
        ],
      },
    };

    expect(prototypeValuesForMarkup(markup)).toMatchObject({
      locked: false,
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

  it('writes and reads the selected markup lock state', () => {
    const markup: RectangleMarkup = {
      id: 'locked-rectangle',
      kind: 'rectangle',
      pageIndex: 0,
      rect: rect(0, 0, 20, 20),
    };

    const locked = updateSelectedMarkupProperty(markup, 'locked', true);
    expect(locked.locked).toBe(true);
    expect(prototypeValuesForMarkup(locked).locked).toBe(true);
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

  it('writes selected text appearance and geometry into the markup', () => {
    const markup = createTextBoxMarkup({
      id: 'text-1',
      pageIndex: 0,
      rect: rect(10, 20, 100, 40),
      text: 'Live text',
      color: '#ff0000',
      fontFamily: 'Helvetica',
      fontSizePt: 12,
      lineHeightPt: 13.8,
    });
    const styled = updateSelectedMarkupProperty(markup, 'typography', {
      ...prototypeValuesForMarkup(markup).typography,
      size: 24,
      lineSpacing: 1.25,
      alignment: 'center',
    });
    expect(resolveMarkupAppearance(styled).text).toMatchObject({ fontSizePt: 24, lineHeightPt: 30, align: 'center' });

    const moved = updateSelectedMarkupProperty(styled, 'x', 35);
    const resized = updateSelectedMarkupProperty(moved, 'width', 180);
    expect(prototypeValuesForMarkup(resized)).toMatchObject({ x: 35, y: 20, width: 180, height: 40 });
  });

  it('groups tool defaults by what they customize', () => {
    expect(
      groupToolProperties([
        {
          kind: 'color',
          key: 'strokeColor',
          label: 'Stroke',
          default: '#ff0000',
        },
        { kind: 'color', key: 'textColor', label: 'Text', default: '#ff0000' },
        { kind: 'number', key: 'fontSizePt', label: 'Font size', default: 12 },
        { kind: 'color', key: 'fillColor', label: 'Fill', default: null },
        { kind: 'number', key: 'cloudIntensity', label: 'Cloud intensity', default: 2 },
        { kind: 'number', key: 'opacity', label: 'Opacity', default: 1 },
      ]),
    ).toEqual([
      {
        title: 'Text',
        properties: [expect.objectContaining({ key: 'textColor' }), expect.objectContaining({ key: 'fontSizePt' })],
      },
      {
        title: 'Appearance',
        properties: [
          expect.objectContaining({ key: 'strokeColor' }),
          expect.objectContaining({ key: 'fillColor' }),
          expect.objectContaining({ key: 'opacity' }),
        ],
      },
      {
        title: 'Shape',
        properties: [expect.objectContaining({ key: 'cloudIntensity' })],
      },
    ]);
  });

  it('assigns every registered tool property to a shared category', () => {
    const properties = PDF_TOOL_REGISTRY.flatMap((tool) => tool.properties.properties);
    expect([...new Set(properties.map((property) => toolPropertyCategory(property.key)))]).not.toContain('Other');
    expect(toolPropertyCategory('opacity')).toBe('Appearance');
  });
});
