import { describe, expect, it } from 'vitest';
import { createCalloutMarkup, createCloudPlusMarkup, createPageTransform, createRectangleMarkup, pdfPoint, rect } from '@butter-paper/core';
import { getVerticallyCenteredAnnotationTextContentStyle } from '../pdf-tools/annotationStyles';
import { expandViewportRect, isPointNearSelectionChromeEdge, projectChromeHandlePoint } from '../pdf-tools/selectionHitZones';
import { layoutTextBoxLines, splitAnnotationTextLines } from '../pdf-tools/textLayout';
import { autosizeTextBoxRect, autosizeTextBoxRectDownward, centeredCompositeTextBoxRect, cloudCursorHintPosition, isCloudPolygonClosePoint, isPostPlacementSelectionActive, scaleAnnotationDashArray, scaleAnnotationFirstBaselineOffset, scaleAnnotationFontSize, scaleAnnotationLineHeight, scaleAnnotationStrokeWidth, scaleAnnotationTextInset, selectionAfterMarkupClick, selectionCursorHintPosition, selectionToggleCursorIntent, selectionZoneCursorIntent, shouldCancelDraftForToolChange, shouldRenderMarkupAtZoom, shouldSelectMarkupAfterHandleTransform, textBoxCaretGeometry, updateMarkupTextAndCenterOnLeader } from './AnnotationLayer';

describe('annotation layer rendering', () => {
  it('scales PDF text font sizes into thumbnail viewport space', () => {
    expect(scaleAnnotationFontSize(10, { zoom: 220 / 792 })).toBeCloseTo(2.78);
  });

  it('scales the default text size as well as explicit sizes', () => {
    expect(scaleAnnotationFontSize(undefined, { zoom: 0.25 })).toBe(3);
  });

  it('scales line height and splits multiline annotation text', () => {
    expect(scaleAnnotationLineHeight(13.8, 12, { zoom: 2 })).toBe(27.6);
    expect(scaleAnnotationLineHeight(undefined, 10, { zoom: 2 })).toBe(23);
    expect(splitAnnotationTextLines('one\ntwo\r\nthree')).toEqual(['one', 'two', 'three']);
  });

  it('scales Bluebeam-compatible text box insets', () => {
    expect(scaleAnnotationTextInset(5, { zoom: 2 })).toBe(10);
    expect(scaleAnnotationFirstBaselineOffset(14.3146, 12, { zoom: 2 })).toBeCloseTo(28.6292);
  });

  it('scales PDF-space strokes and dashes with zoom', () => {
    expect(scaleAnnotationStrokeWidth(1.25, { zoom: 0.05 })).toBeCloseTo(0.0625);
    expect(scaleAnnotationStrokeWidth(0, { zoom: 0.05 })).toBe(0);
    expect(scaleAnnotationDashArray('6 3', { zoom: 0.05 })).toBe('0.3 0.15');
  });

  it('hides imported markups at low zoom to avoid duplicate PDF annotation ghosts', () => {
    const importedMarkup = createRectangleMarkup({
      id: 'imported-rect',
      pageIndex: 0,
      rect: rect(10, 10, 20, 20),
      source: { source: 'imported' },
    });
    const butterMarkup = createRectangleMarkup({
      id: 'butter-rect',
      pageIndex: 0,
      rect: rect(10, 10, 20, 20),
      source: { source: 'butter' },
    });

    expect(shouldRenderMarkupAtZoom(importedMarkup, { zoom: 0.06 })).toBe(false);
    expect(shouldRenderMarkupAtZoom(importedMarkup, { zoom: 0.35 })).toBe(true);
    expect(shouldRenderMarkupAtZoom(butterMarkup, { zoom: 0.06 })).toBe(true);
  });

  it('wraps text box content within the inset text area', () => {
    const lines = layoutTextBoxLines('one two three', {
      boxWidthPt: 8,
      insetPt: 1,
      fontSizePt: 12,
      measureText: (text) => text.length,
    });

    expect(lines.map((line) => line.text)).toEqual(['one', 'two', 'three']);
  });

  it('starts text placement at caret width and grows to explicit content lines', () => {
    const measureText = (text: string) => text.length * 5;

    expect(autosizeTextBoxRect(pdfPoint(20, 30), '', { fontSizePt: 12, lineHeightPt: 14, insetPt: 5, measureText })).toEqual(
      rect(20, 30, 11, 18),
    );
    expect(autosizeTextBoxRect(pdfPoint(20, 30), 'abc', { fontSizePt: 12, lineHeightPt: 14, insetPt: 5, measureText })).toEqual(
      rect(20, 30, 25, 18),
    );
    expect(autosizeTextBoxRect(pdfPoint(20, 30), 'abc\ndefgh', { fontSizePt: 12, lineHeightPt: 14, insetPt: 5, measureText })).toEqual(
      rect(20, 30, 35, 32),
    );
  });

  it('keeps the visible top edge fixed while new text lines grow downward', () => {
    const measureText = (text: string) => text.length * 5;
    const initial = rect(20, 30, 11, 18);

    expect(autosizeTextBoxRectDownward(initial, 'abc', { fontSizePt: 12, lineHeightPt: 14, insetPt: 5, measureText })).toEqual(
      rect(20, 30, 25, 18),
    );
    expect(autosizeTextBoxRectDownward(initial, 'abc\ndefgh', { fontSizePt: 12, lineHeightPt: 14, insetPt: 5, measureText })).toEqual(
      rect(20, 16, 35, 32),
    );
  });

  it('positions the editing caret from the same font size and line metrics as rendered text', () => {
    const markup = {
      rect: rect(20, 30, 100, 50),
      fontFamily: 'Helvetica' as const,
      fontSizePt: 24,
      lineHeightPt: 30,
    };
    const measureText = (text: string, context: { fontSizePt?: number }) => text.length * (context.fontSizePt ?? 0) * 0.5;

    expect(textBoxCaretGeometry(markup, 'ab\ncde', 2, { measureText })).toEqual({
      x: 49,
      y: 34.6292,
      height: 30,
    });
    expect(textBoxCaretGeometry(markup, 'ab\ncde', 6, { measureText })).toEqual({
      x: 61,
      y: 64.6292,
      height: 30,
    });
  });

  it.each([
    { align: 'center' as const, offset: 1, expectedX: 58 },
    { align: 'right' as const, offset: 1, expectedX: 76 },
  ])('positions a $align caret within the full stored line rather than at its end', ({ align, offset, expectedX }) => {
    const markup = {
      rect: rect(20, 30, 100, 50),
      appearance: {
        text: {
          color: '#654321',
          fontId: 'FutureFont',
          fontSizePt: 24,
          lineHeightPt: 30,
          align,
          insetPt: 8,
        },
      },
    };
    const measureText = (text: string, context: { fontSizePt?: number }) => text.length * (context.fontSizePt ?? 0) * 0.5;

    expect(textBoxCaretGeometry(markup, 'abcd', offset, { measureText })).toMatchObject({ x: expectedX, height: 30 });
  });

  it('projects chrome handle hit points onto the outward visual chrome box', () => {
    const source = { x: 10, y: 20, width: 100, height: 50 };
    const target = expandViewportRect(source, 8);

    expect(projectChromeHandlePoint({ x: 110, y: 45 }, source, target)).toEqual({ x: 118, y: 45 });
    expect(projectChromeHandlePoint({ x: 10, y: 70 }, source, target)).toEqual({ x: 2, y: 78 });
    expect(projectChromeHandlePoint({ x: 60, y: 82 }, source, target)).toEqual({ x: 60, y: 90 });
  });

  it('keeps the content edge inside the chrome-outset edge hit band', () => {
    const page = { id: 'page-1', index: 0, size: { width: 612, height: 792 }, rotation: 0 } as const;
    const transform = createPageTransform(page, 1);
    const box = rect(10, 10, 100, 50);

    expect(isPointNearSelectionChromeEdge(pdfPoint(11, 20), box, { transform })).toBe(true);
    expect(isPointNearSelectionChromeEdge(pdfPoint(50, 30), box, { transform })).toBe(false);
  });

  it('cancels an in-progress click placement draft when the active tool changes', () => {
    const lineDraft = { kind: 'line', start: pdfPoint(0, 0), current: pdfPoint(10, 0) } as const;

    expect(shouldCancelDraftForToolChange('line', 'rectangle', lineDraft)).toBe(true);
    expect(shouldCancelDraftForToolChange('line', 'line', lineDraft)).toBe(false);
    expect(shouldCancelDraftForToolChange('line', 'rectangle', null)).toBe(false);
  });

  it('keeps only the matching completed placement in the direct-manipulation submode', () => {
    const completed = { markupId: 'rect-1', tool: 'rectangle' } as const;

    expect(isPostPlacementSelectionActive(completed, 'rectangle', ['rect-1'], false)).toBe(true);
    expect(isPostPlacementSelectionActive(completed, 'rectangle', ['rect-1'], true)).toBe(false);
    expect(isPostPlacementSelectionActive(completed, 'line', ['rect-1'], false)).toBe(false);
    expect(isPostPlacementSelectionActive(completed, 'rectangle', [], false)).toBe(false);
  });

  it('selects on a handle click but keeps a latched handle drag transient', () => {
    const page = { id: 'page-1', index: 0, size: { width: 612, height: 792 }, rotation: 0 } as const;
    const transform = createPageTransform(page, 2);
    const gesture = { startPoint: pdfPoint(10, 10), dragStarted: false } as const;

    expect(shouldSelectMarkupAfterHandleTransform(gesture, pdfPoint(11, 10), transform)).toBe(true);
    expect(shouldSelectMarkupAfterHandleTransform(gesture, pdfPoint(12, 10), transform)).toBe(false);
    expect(shouldSelectMarkupAfterHandleTransform(
      { ...gesture, dragStarted: true },
      gesture.startPoint,
      transform,
    )).toBe(false);
  });

  it('replaces selection on a plain click and toggles membership on Shift-click', () => {
    expect(selectionAfterMarkupClick(['rect-1', 'line-1'], 'ellipse-1', false)).toEqual(['ellipse-1']);
    expect(selectionAfterMarkupClick(['rect-1', 'line-1'], 'line-1', false)).toEqual(['rect-1', 'line-1']);
    expect(selectionAfterMarkupClick(['rect-1'], 'ellipse-1', true)).toEqual(['rect-1', 'ellipse-1']);
    expect(selectionAfterMarkupClick(['rect-1', 'ellipse-1'], 'ellipse-1', true)).toEqual(['rect-1']);
  });

  it('shows add and remove cursors for Shift-toggle selection without masking handle cursors', () => {
    expect(selectionToggleCursorIntent('select', true, 'ellipse-1', ['rect-1'], false)).toBe('add');
    expect(selectionToggleCursorIntent('select', true, 'rect-1', ['rect-1'], false)).toBe('remove');
    expect(selectionToggleCursorIntent('select', true, 'rect-1', ['rect-1'], true)).toBeNull();
    expect(selectionToggleCursorIntent('select', false, 'rect-1', ['rect-1'], false)).toBeNull();
    expect(selectionToggleCursorIntent('rectangle', true, 'rect-1', ['rect-1'], false)).toBeNull();
    expect(selectionZoneCursorIntent('select', true, false, null, false)).toBe('add');
    expect(selectionZoneCursorIntent('select', false, true, null, false)).toBe('remove');
    expect(selectionZoneCursorIntent('select', true, false, 'rect-1', false)).toBeNull();
  });

  it('keeps the cursor-following selection shortcut hint inside the page', () => {
    expect(selectionCursorHintPosition({ x: 40, y: 50 }, { width: 600, height: 400 })).toEqual({ x: 56, y: 66 });
    expect(selectionCursorHintPosition({ x: 590, y: 390 }, { width: 600, height: 400 })).toEqual({ x: 298, y: 346 });
  });

  it('keeps the cloud gesture hint beside the pointer and inside the page', () => {
    expect(cloudCursorHintPosition({ x: 40, y: 50 }, { width: 600, height: 400 })).toEqual({ x: 54, y: 64 });
    expect(cloudCursorHintPosition({ x: 590, y: 390 }, { width: 600, height: 400 })).toEqual({ x: 390, y: 352 });
  });

  it('closes a cloud polygon within a zoom-independent screen-space threshold', () => {
    const page = { id: 'page-1', index: 0, size: { width: 612, height: 792 }, rotation: 0 } as const;
    const transform = createPageTransform(page, 2);

    expect(isCloudPolygonClosePoint(pdfPoint(10, 10), pdfPoint(14, 10), transform)).toBe(true);
    expect(isCloudPolygonClosePoint(pdfPoint(10, 10), pdfPoint(16, 10), transform)).toBe(false);
  });

  it.each(['callout', 'cloud-plus'] as const)('grows and vertically centers %s text around its leader connection', (kind) => {
    const base = {
      id: `${kind}-1`,
      pageIndex: 0,
      leader: { points: [pdfPoint(10, 22), pdfPoint(40, 22), pdfPoint(100, 22)] },
      textBox: rect(100, 0, 150, 44),
      text: 'One line',
    } as const;
    const markup = kind === 'callout'
      ? createCalloutMarkup(base)
      : createCloudPlusMarkup({ ...base, cloud: { controlPath: [pdfPoint(0, 0), pdfPoint(0, 40), pdfPoint(80, 40), pdfPoint(80, 0)] } });
    const text = 'First\nSecond\nThird';
    const centered = centeredCompositeTextBoxRect(markup, text);
    const updated = updateMarkupTextAndCenterOnLeader({
      id: 'document-1',
      path: '/document.pdf',
      metadata: {},
      pages: [],
      markups: [markup],
    }, markup.id, text).markups[0];
    const style = getVerticallyCenteredAnnotationTextContentStyle({ ...markup, text, textBox: centered } as never);

    expect(centered.height).toBeCloseTo(53.4);
    expect(centered.y + centered.height * 0.5).toBeCloseTo(22);
    expect(updated).toMatchObject({ text, textBox: centered });
    expect(style.firstBaselineOffsetPt).toBeCloseTo(16.5);
  });
});
