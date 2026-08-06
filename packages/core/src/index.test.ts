import { describe, expect, it } from 'vitest';
import {
  createCalloutMarkup,
  createCloudMarkup,
  createDocument,
  createEllipseMarkup,
  createArrowMarkup,
  createHighlightMarkup,
  createLineMarkup,
  createDimensionMarkup,
  createPenMarkup,
  createPolylineMarkup,
  createPolygonMarkup,
  createPageTransform,
  parsePageScaleRanges,
  createRectangleMarkup,
  createSelection,
  createTextBoxMarkup,
  createViewportState,
  moveMarkup,
  pdfPoint,
  applyPageScale,
  BUILT_IN_SCALE_PRESETS,
  calibratePageScale,
  convertScaledAreaUnit,
  convertScaledValueUnit,
  convertPdfDistanceToReal,
  createCustomPageScale,
  deleteUserScalePreset,
  formatScaledValue,
  formatScaledAreaLabel,
  formatScaledLengthLabel,
  formatPageScaleRatio,
  measureScaledLength,
  measureScaledPolygonArea,
  measureScaledPolyline,
  rotateDocumentPage,
  getPageScale,
  requirePageScale,
  saveScalePreset,
  screenPoint,
  selectMarkups,
  updateMarkupText,
} from './index.js';
import { createMarkup } from './commands.js';

describe('core geometry', () => {
  it('round-trips a point through the page transform', () => {
    const transform = createPageTransform(
      { size: { width: 200, height: 100 }, rotation: 0 },
      2,
      screenPoint(10, 20),
    );

    const pdf = pdfPoint(15, 25);
    const screen = transform.pdfToScreen(pdf);
    expect(screen).toEqual(screenPoint(40, 170));
    expect(transform.screenToPdf(screen)).toEqual(pdf);
  });

  it('round-trips a viewport point', () => {
    const transform = createPageTransform(
      { size: { width: 100, height: 100 }, rotation: 0 },
      1,
    );

    const point = pdfPoint(24, 39);
    expect(transform.viewportToPdf(transform.pdfToViewport(point))).toEqual(point);
  });

  it.each([
    { rotation: 0 as const, size: { width: 540, height: 720 }, expectedPoint: { x: 36, y: 684 }, expectedRect: { x: 36, y: 648, width: 72, height: 36 } },
    { rotation: 90 as const, size: { width: 720, height: 540 }, expectedPoint: { x: 36, y: 36 }, expectedRect: { x: 36, y: 36, width: 36, height: 72 } },
    { rotation: 180 as const, size: { width: 540, height: 720 }, expectedPoint: { x: 504, y: 36 }, expectedRect: { x: 432, y: 36, width: 72, height: 36 } },
    { rotation: 270 as const, size: { width: 720, height: 540 }, expectedPoint: { x: 684, y: 504 }, expectedRect: { x: 648, y: 432, width: 36, height: 72 } },
  ])('matches PDF.js CropBox geometry at $rotation degrees', ({ rotation, size: pageSize, expectedPoint, expectedRect }) => {
    const transform = createPageTransform({
      size: pageSize,
      rotation,
      viewBox: { x: 36, y: 72, width: 540, height: 720 },
      userUnit: 1,
    }, 1);
    const point = pdfPoint(72, 108);
    const pdfRect = { x: 72, y: 108, width: 72, height: 36 };

    expect(transform.pdfToViewport(point)).toEqual(expectedPoint);
    expect(transform.viewportToPdf(expectedPoint as never)).toEqual(point);
    expect(transform.pdfRectToViewport(pdfRect)).toEqual(expectedRect);
    expect(transform.viewportRectToPdf(expectedRect)).toEqual(pdfRect);
  });

  it('applies PDF UserUnit while preserving raw PDF coordinates', () => {
    const transform = createPageTransform({
      size: { width: 1080, height: 1440 },
      rotation: 0,
      viewBox: { x: 36, y: 72, width: 540, height: 720 },
      userUnit: 2,
    }, 1);

    const point = pdfPoint(72, 108);
    expect(transform.pdfToViewport(point)).toEqual({ x: 72, y: 1368 });
    expect(transform.viewportToPdf({ x: 72, y: 1368 } as never)).toEqual(point);
  });

  it('derives unrotated legacy bounds from rotated layout size', () => {
    const transform = createPageTransform({
      size: { width: 720, height: 540 },
      rotation: 90,
    }, 1);

    expect(transform.geometry.viewBox).toEqual({ x: 0, y: 0, width: 540, height: 720 });
    expect(transform.viewportToPdf(transform.pdfToViewport(pdfPoint(72, 108)))).toEqual(pdfPoint(72, 108));
  });

  it('keeps deep viewer zooms in the page transform', () => {
    const transform = createPageTransform(
      { size: { width: 792, height: 612 }, rotation: 0 },
      64,
    );

    const point = pdfPoint(500, 300);
    expect(transform.zoom).toBe(64);
    expect(transform.pdfToViewport(point)).toEqual({ x: 32_000, y: 19_968 });
    expect(transform.viewportToPdf(transform.pdfToViewport(point))).toEqual(point);
  });
});

describe('core commands', () => {
  it('keeps explicit safe native annotation metadata when markups are transformed', () => {
    const document = createDocument({
      id: 'metadata-doc',
      path: '/tmp/metadata.pdf',
      metadata: {},
      pages: [{ id: 'page-1', index: 0, size: { width: 200, height: 200 }, rotation: 0 }],
      markups: [createRectangleMarkup({
        id: 'metadata-rectangle',
        pageIndex: 0,
        rect: { x: 10, y: 20, width: 40, height: 50 },
        source: {
          annotationId: 'nm:BB-RECT',
          annotationIds: ['nm:BB-RECT'],
          annotationMetadata: [{
            annotationId: 'nm:BB-RECT',
            role: 'primary',
            author: 'A. Reviewer',
            subject: 'Structural review',
            creationDate: 'D:20260804101500+10\'00\'',
            contents: 'Keep this comment',
            flags: 4,
            statusModel: 'Review',
            status: 'Accepted',
          }],
          source: 'imported',
        },
      })],
    });

    const moved = moveMarkup(document, 'metadata-rectangle', { x: 5, y: -5 });
    expect(moved.markups[0]?.source?.annotationMetadata).toEqual(document.markups[0]?.source?.annotationMetadata);
  });

  it('creates, moves and edits markups', () => {
    const document = createDocument({
      id: 'doc-1',
      path: '/tmp/test.pdf',
      metadata: {},
      pages: [{ id: 'page-1', index: 0, size: { width: 200, height: 200 }, rotation: 0 }],
    });

    const rectangle = createRectangleMarkup({
      id: 'markup-1',
      pageIndex: 0,
      rect: { x: 10, y: 20, width: 40, height: 50 },
    });
    const callout = createCalloutMarkup({
      id: 'markup-2',
      pageIndex: 0,
      leader: { points: [pdfPoint(10, 10), pdfPoint(30, 30), pdfPoint(40, 30)] },
      textBox: { x: 50, y: 60, width: 90, height: 40 },
      text: 'Check this detail',
    });
    const textBox = createTextBoxMarkup({
      id: 'markup-3',
      pageIndex: 0,
      rect: { x: 20, y: 30, width: 60, height: 20 },
      text: 'Default text',
    });
    const ellipse = createEllipseMarkup({
      id: 'markup-4',
      pageIndex: 0,
      rect: { x: 70, y: 80, width: 30, height: 20 },
    });
    const line = createLineMarkup({
      id: 'markup-5',
      pageIndex: 0,
      start: pdfPoint(10, 10),
      end: pdfPoint(20, 20),
    });
    const arrow = createArrowMarkup({
      id: 'markup-6',
      pageIndex: 0,
      start: pdfPoint(30, 30),
      end: pdfPoint(40, 40),
    });
    const dimension = createDimensionMarkup({
      id: 'markup-12',
      pageIndex: 0,
      start: pdfPoint(30, 90),
      end: pdfPoint(100, 90),
      dimensionLineOffset: 24,
      text: 'Door width',
    });
    const polyline = createPolylineMarkup({
      id: 'markup-7',
      pageIndex: 0,
      points: [pdfPoint(50, 50), pdfPoint(60, 70), pdfPoint(80, 60)],
    });
    const polygon = createPolygonMarkup({
      id: 'markup-8',
      pageIndex: 0,
      points: [pdfPoint(90, 90), pdfPoint(100, 110), pdfPoint(120, 95)],
    });
    const pen = createPenMarkup({
      id: 'markup-9',
      pageIndex: 0,
      paths: [[pdfPoint(20, 120), pdfPoint(40, 130)]],
    });
    const highlight = createHighlightMarkup({
      id: 'markup-10',
      pageIndex: 0,
      paths: [[pdfPoint(20, 150), pdfPoint(80, 150)]],
    });
    const cloud = createCloudMarkup({
      id: 'markup-11',
      pageIndex: 0,
      controlPath: [pdfPoint(130, 130), pdfPoint(150, 150), pdfPoint(170, 130)],
      appearancePath: 'M 130 130 C 140 145 160 145 170 130 Z',
    });

    const withMarkup = createMarkup(createMarkup(createMarkup(createMarkup(createMarkup(createMarkup(createMarkup(createMarkup(createMarkup(createMarkup(createMarkup(createMarkup(document, rectangle), callout), textBox), ellipse), line), arrow), polyline), polygon), pen), highlight), cloud), dimension);
    expect(withMarkup.markups).toHaveLength(12);

    const moved = moveMarkup(withMarkup, 'markup-1', { x: 5, y: -5 });
    expect(moved.markups[0]).toMatchObject({
      rect: { x: 15, y: 15, width: 40, height: 50 },
    });

    const edited = updateMarkupText(moved, 'markup-2', 'Revised note');
    expect(edited.markups[1]).toMatchObject({ text: 'Revised note' });

    const editedTextBox = updateMarkupText(edited, 'markup-3', 'Revised text box');
    expect(editedTextBox.markups[2]).toMatchObject({ text: 'Revised text box' });

    const movedEllipse = moveMarkup(editedTextBox, 'markup-4', { x: -10, y: 10 });
    expect(movedEllipse.markups[3]).toMatchObject({
      rect: { x: 60, y: 90, width: 30, height: 20 },
    });

    const movedLine = moveMarkup(movedEllipse, 'markup-5', { x: 5, y: 5 });
    expect(movedLine.markups[4]).toMatchObject({
      start: pdfPoint(15, 15),
      end: pdfPoint(25, 25),
    });

    const movedArrow = moveMarkup(movedLine, 'markup-6', { x: -5, y: -5 });
    expect(movedArrow.markups[5]).toMatchObject({
      start: pdfPoint(25, 25),
      end: pdfPoint(35, 35),
    });

    const movedPolyline = moveMarkup(movedArrow, 'markup-7', { x: 10, y: -10 });
    expect(movedPolyline.markups[6]).toMatchObject({
      points: [pdfPoint(60, 40), pdfPoint(70, 60), pdfPoint(90, 50)],
    });

    const movedPolygon = moveMarkup(movedPolyline, 'markup-8', { x: -10, y: 10 });
    expect(movedPolygon.markups[7]).toMatchObject({
      points: [pdfPoint(80, 100), pdfPoint(90, 120), pdfPoint(110, 105)],
    });

    const movedPen = moveMarkup(movedPolygon, 'markup-9', { x: 3, y: 4 });
    expect(movedPen.markups[8]).toMatchObject({
      paths: [[pdfPoint(23, 124), pdfPoint(43, 134)]],
    });

    const movedHighlight = moveMarkup(movedPen, 'markup-10', { x: -5, y: -5 });
    expect(movedHighlight.markups[9]).toMatchObject({
      paths: [[pdfPoint(15, 145), pdfPoint(75, 145)]],
    });

    const movedCloud = moveMarkup(movedHighlight, 'markup-11', { x: 2, y: 3 });
    expect(movedCloud.markups[10]).toMatchObject({
      controlPath: [pdfPoint(132, 133), pdfPoint(152, 153), pdfPoint(172, 133)],
      appearancePath: 'M 132 133 C 142 148 162 148 172 133 Z',
    });

    const movedDimension = moveMarkup(movedCloud, 'markup-12', { x: 5, y: -2 });
    expect(movedDimension.markups[11]).toMatchObject({
      start: pdfPoint(35, 88),
      end: pdfPoint(105, 88),
      dimensionLineOffset: 24,
    });
  });

  it('deduplicates selection ids and keeps the first focused', () => {
    const selection = createSelection(['a', 'b', 'a']);
    expect(selection).toEqual({
      markupIds: ['a', 'b'],
      focusedMarkupId: 'a',
    });

    expect(selectMarkups(selection, ['c', 'c', 'd'])).toEqual({
      markupIds: ['c', 'd'],
      focusedMarkupId: 'c',
    });
  });

  it('creates a viewport state with tight defaults', () => {
    expect(createViewportState()).toMatchObject({
      zoom: 1,
      pan: { x: 0, y: 0 },
      pageSpacing: 24,
    });
  });
});

describe('page scale foundation', () => {
  it('matches Bluebeam metric built-in scale presets', () => {
    expect(BUILT_IN_SCALE_PRESETS.map((preset) => preset.name)).toEqual([
      '1:1',
      '1:10',
      '1:20',
      '1:50',
      '1:100',
      '1:200',
      '1:500',
      '1:1000',
    ]);
    expect(BUILT_IN_SCALE_PRESETS.every((preset) => preset.pdfUnits === 'cm' && preset.realUnits === 'm')).toBe(true);
    expect(BUILT_IN_SCALE_PRESETS.some((preset) => preset.realUnits === 'ft' || preset.pdfUnits === 'in')).toBe(false);
  });

  it('creates custom and calibrated scales and converts PDF distances', () => {
    const custom = createCustomPageScale({
      pageIndex: 0,
      name: '1/4 in = 1 ft',
      pdfUnits: 'in',
      realUnits: 'ft',
      pdfLength: 0.25,
      realLength: 1,
    });
    const calibrated = calibratePageScale({
      pageIndex: 0,
      start: pdfPoint(0, 0),
      end: pdfPoint(25, 0),
      realLength: 100,
      realUnits: 'ft',
    });

    expect(convertPdfDistanceToReal(custom, 18)).toBe(1);
    expect(convertPdfDistanceToReal(calibrated, 25)).toBe(100);
  });

  it('formats decimal and fractional precision values', () => {
    expect(formatScaledValue(12.346, { mode: 'decimal', value: 0.01 })).toBe('12.35');
    expect(formatScaledValue(2.375, { mode: 'fraction', value: 8 })).toBe('2 3/8');
  });

  it('formats compact dimensionless scale ratios', () => {
    const metric = createCustomPageScale({
      pageIndex: 0,
      name: '1 cm = 1 m',
      pdfUnits: 'cm',
      realUnits: 'm',
      pdfLength: 1,
      realLength: 1,
    });
    expect(formatPageScaleRatio(metric)).toBe('1:100');
  });

  it('measures scaled length, polylength and area', () => {
    const scale = createCustomPageScale({
      pageIndex: 0,
      name: 'Separate axes',
      pdfUnits: 'cm',
      realUnits: 'm',
      scaleX: 2,
      scaleY: 3,
    });

    expect(measureScaledLength(pdfPoint(0, 0), pdfPoint(3, 4), scale)).toBeCloseTo(Math.hypot(6, 12));
    expect(measureScaledPolyline([pdfPoint(0, 0), pdfPoint(3, 0), pdfPoint(3, 4)], scale)).toBeCloseTo(6 + 12);
    expect(measureScaledPolygonArea([pdfPoint(0, 0), pdfPoint(3, 0), pdfPoint(3, 4), pdfPoint(0, 4)], scale)).toBeCloseTo(72);
    expect(convertScaledValueUnit(1, 'm', 'cm')).toBeCloseTo(100);
    expect(convertScaledAreaUnit(1, 'm', 'cm')).toBeCloseTo(10000);
    expect(formatScaledLengthLabel(5, { ...scale, precision: { mode: 'decimal', value: 0.01 } }, 'cm')).toBe('500.00 cm');
    expect(formatScaledAreaLabel(2, { ...scale, precision: { mode: 'decimal', value: 0.01 } }, 'cm')).toBe('20000.00 cm^2');
  });

  it('applies page scale to current, all and explicit page ranges', () => {
    const document = createDocument({
      id: 'doc-1',
      path: '/tmp/test.pdf',
      metadata: {},
      pages: [
        { id: 'page-1', index: 0, size: { width: 200, height: 200 }, rotation: 0 },
        { id: 'page-2', index: 1, size: { width: 200, height: 200 }, rotation: 0 },
        { id: 'page-3', index: 2, size: { width: 200, height: 200 }, rotation: 0 },
      ],
    });
    const scale = createCustomPageScale({
      pageIndex: 0,
      name: 'Custom',
      pdfUnits: 'in',
      realUnits: 'ft',
      pdfLength: 1,
      realLength: 2,
    });
    const current = applyPageScale(document, scale, { kind: 'current', pageIndex: 1 });
    const ranges = applyPageScale(current, { ...scale, scaleX: 3, scaleY: 3 }, { kind: 'ranges', ranges: [{ startPageIndex: 0, endPageIndex: 0 }] });
    const all = applyPageScale(ranges, { ...scale, scaleX: 4, scaleY: 4 }, { kind: 'all' });

    expect(getPageScale(current, 1)?.scaleX).toBeCloseTo(2 / 72);
    expect(getPageScale(ranges, 0)?.scaleX).toBe(3);
    expect(all.pageScales?.map((pageScale) => pageScale.pageIndex)).toEqual([0, 1, 2]);
    expect(all.pageScales?.every((pageScale) => pageScale.scaleX === 4)).toBe(true);
  });

  it('rotates one page left or right and swaps its visible dimensions', () => {
    const document = createDocument({
      id: 'doc-rotation',
      path: '/tmp/test.pdf',
      metadata: {},
      pages: [
        { id: 'page-1', index: 0, size: { width: 200, height: 300 }, rotation: 0 },
        { id: 'page-2', index: 1, size: { width: 400, height: 500 }, rotation: 90 },
      ],
    });

    expect(rotateDocumentPage(document, 0, 'right').pages).toEqual([
      { id: 'page-1', index: 0, size: { width: 300, height: 200 }, rotation: 90 },
      document.pages[1],
    ]);
    expect(rotateDocumentPage(document, 1, 'left').pages[1]).toEqual({
      id: 'page-2',
      index: 1,
      size: { width: 500, height: 400 },
      rotation: 0,
    });
  });

  it('parses Bluebeam-style custom page ranges from one-based input', () => {
    expect(parsePageScaleRanges('1-3, 5, 9', 10)).toEqual([
      { startPageIndex: 0, endPageIndex: 2 },
      { startPageIndex: 4, endPageIndex: 4 },
      { startPageIndex: 8, endPageIndex: 8 },
    ]);
    expect(() => parsePageScaleRanges('1-a', 10)).toThrow('Enter page ranges like 1-3, 5, 9.');
    expect(() => parsePageScaleRanges('11', 10)).toThrow('Page range must be between 1 and 10.');
  });

  it('blocks measurement placement when page scale is missing and protects built-in presets', () => {
    const document = createDocument({
      id: 'doc-1',
      path: '/tmp/test.pdf',
      metadata: {},
      pages: [{ id: 'page-1', index: 0, size: { width: 200, height: 200 }, rotation: 0 }],
    });
    const withPreset = saveScalePreset(document, {
      id: 'saved-scale',
      name: 'Saved scale',
      pdfUnits: 'in',
      realUnits: 'ft',
      scaleX: 4,
      scaleY: 4,
      source: 'custom',
    });

    expect(() => requirePageScale(document, 0)).toThrow('Set page scale before placing measurement markups.');
    expect(deleteUserScalePreset(withPreset, 'saved-scale').scalePresets).toEqual([]);
    expect(() => deleteUserScalePreset({
      ...document,
      scalePresets: [{ ...withPreset.scalePresets![0], builtIn: true }],
    }, 'saved-scale')).toThrow('Built-in scale presets cannot be deleted.');
  });
});
