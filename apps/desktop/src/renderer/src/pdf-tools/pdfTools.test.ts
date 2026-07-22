import { describe, expect, it } from 'vitest';
import { createArcMarkup, createAreaMarkup, createCalloutMarkup, createCloudPlusMarkup, createCustomPageScale, createDimensionMarkup, createImageMarkup, createImportedAnnotationMarkup, createLengthMarkup, createPageTransform, createPolylengthMarkup, createSnapshotMarkup, pdfPoint, rect } from '@butter-paper/core';
import type { ToolMode } from '../../../shared/protocol';
import { DEFAULT_IMAGE_DATA_URL } from './builtins/imageTool';
import { addCloudNodeDraftPoint, createCloudNodeDraft, createRectangleDraft, createTextBoxDraft, hasExceededDragThreshold, rectangleDraftToRect, resizeRectFromHandle, resizeRotatedRectFromHandle, shouldCommitRectangle, textBoxDraftToRect, updateCloudNodeDraft } from './annotationLifecycle';
import { getAnnotationContentStyle } from './annotationStyles';
import { hitTestHandles, hitTestMarkup, hitTestMarkups } from './hitTesting';
import { getChromeStyle, getMoveCursor, getResizeCursor, getRotateCursor, getResizeHandles, getRotationHandle } from './interactionChrome';
import { DEFAULT_CLOUD_LINE_OPTIONS, CLOUD_LINE_TYPE_RENDERER, generateCloudScallopPoints } from './lineTypes';
import { getMarkupToolDefinition, getToolDefinition, PDF_TOOL_REGISTRY } from './toolRegistry';

describe('PDF tool registry', () => {
  it('exposes only the implemented reset tools', () => {
    expect(PDF_TOOL_REGISTRY.map((tool) => tool.id)).toEqual(['select', 'pan', 'text-box', 'rectangle', 'ellipse', 'arc', 'line', 'arrow', 'dimension', 'length', 'polylength', 'area', 'polyline', 'polygon', 'pen', 'highlight', 'cloud', 'cloud-plus', 'callout', 'image', 'snapshot']);
    expect(getToolDefinition('text-box')).toMatchObject({ label: 'Text Box', shortcut: 'T', cursor: 'default' });
    expect(getToolDefinition('rectangle')).toMatchObject({ label: 'Rectangle', shortcut: 'R' });
    expect(getToolDefinition('ellipse')).toMatchObject({ label: 'Ellipse', shortcut: 'E' });
    expect(getToolDefinition('arc')).toMatchObject({ label: 'Arc', shortcut: 'Shift+C' });
    expect(getToolDefinition('line')).toMatchObject({ label: 'Line', shortcut: 'L' });
    expect(getToolDefinition('arrow')).toMatchObject({ label: 'Arrow', shortcut: 'A' });
    expect(getToolDefinition('dimension')).toMatchObject({ label: 'Dimension', shortcut: 'Shift+L' });
    expect(getToolDefinition('length')).toMatchObject({ label: 'Length', shortcut: 'Shift+Alt+L', category: 'measurement' });
    expect(getToolDefinition('polylength')).toMatchObject({ label: 'Polylength', shortcut: 'Shift+Alt+Q', category: 'measurement' });
    expect(getToolDefinition('area')).toMatchObject({ label: 'Area', shortcut: 'Shift+Alt+A', category: 'measurement' });
    expect(getToolDefinition('polyline')).toMatchObject({ label: 'Polyline', shortcut: 'Shift+N' });
    expect(getToolDefinition('polygon')).toMatchObject({ label: 'Polygon', shortcut: 'Shift+P' });
    expect(getToolDefinition('pen')).toMatchObject({ label: 'Pen', shortcut: 'P' });
    expect(getToolDefinition('highlight')).toMatchObject({ label: 'Highlight', shortcut: 'H' });
    expect(getToolDefinition('cloud')).toMatchObject({ label: 'Cloud', shortcut: 'C' });
    expect(getToolDefinition('cloud-plus')).toMatchObject({ label: 'Cloud+', shortcut: 'K' });
    expect(getToolDefinition('callout')).toMatchObject({ label: 'Callout', shortcut: 'Q' });
    expect(getToolDefinition('image')).toMatchObject({ label: 'Insert Image', shortcut: 'I' });
    expect(getToolDefinition('snapshot')).toMatchObject({ label: 'Snapshot', shortcut: 'G' });
  });

  it('starts sized drawing tools with click placement instead of pointer capture drag', () => {
    const clickPlacementTools: readonly ToolMode[] = [
      'text-box',
      'rectangle',
      'ellipse',
      'line',
      'arrow',
      'dimension',
      'polyline',
      'polygon',
      'cloud-plus',
      'callout',
      'image',
      'snapshot',
    ];

    expect(clickPlacementTools.map((tool) => [tool, getToolDefinition(tool).interaction?.placement])).toEqual(
      clickPlacementTools.map((tool) => [tool, 'click']),
    );
    expect(getToolDefinition('pen').interaction?.placement).toBeUndefined();
    expect(getToolDefinition('cloud').interaction?.placement).toBeUndefined();
  });

  it('commits click-placement line and rectangle drafts without a drag gesture', () => {
    const lineInteraction = getToolDefinition('line').interaction;
    const lineDraft = lineInteraction?.updateDraft?.(
      lineInteraction.createDraft?.({
        pointerId: 1,
        startPoint: pdfPoint(10, 10),
        currentPoint: pdfPoint(10, 10),
      }) as never,
      pdfPoint(40, 10),
    );
    const lineMarkup = lineInteraction?.commitDraft?.(lineDraft as never, {
      page: pageStub(),
      hasExceededDragThreshold: false,
      createMarkupId: (prefix) => `${prefix}-1`,
    });

    const rectangleInteraction = getToolDefinition('rectangle').interaction;
    const rectangleDraft = rectangleInteraction?.updateDraft?.(
      rectangleInteraction.createDraft?.({
        pointerId: 1,
        startPoint: pdfPoint(10, 10),
        currentPoint: pdfPoint(10, 10),
      }) as never,
      pdfPoint(40, 40),
    );
    const rectangleMarkup = rectangleInteraction?.commitDraft?.(rectangleDraft as never, {
      page: pageStub(),
      hasExceededDragThreshold: false,
      createMarkupId: (prefix) => `${prefix}-1`,
    });

    expect(lineMarkup).toMatchObject({ id: 'line-1', kind: 'line', start: pdfPoint(10, 10), end: pdfPoint(40, 10) });
    expect(rectangleMarkup).toMatchObject({ id: 'rect-1', kind: 'rectangle', rect: rect(10, 10, 30, 30) });
  });

  it('renders CAD measurement labels from live page scale', () => {
    const pageScale = createCustomPageScale({
      pageIndex: 0,
      name: '1:100',
      pdfUnits: 'cm',
      realUnits: 'm',
      scaleX: 0.01,
      scaleY: 0.01,
      precision: { mode: 'decimal', value: 0.01 },
    });
    const length = createLengthMarkup({ id: 'length-1', pageIndex: 0, start: pdfPoint(0, 0), end: pdfPoint(300, 400) });
    const polylength = createPolylengthMarkup({ id: 'polylength-1', pageIndex: 0, points: [pdfPoint(0, 0), pdfPoint(300, 0), pdfPoint(300, 400)] });
    const area = createAreaMarkup({ id: 'area-1', pageIndex: 0, points: [pdfPoint(0, 0), pdfPoint(300, 0), pdfPoint(300, 400), pdfPoint(0, 400)] });
    const centimeterLength = createLengthMarkup({ ...length, id: 'length-cm', displayUnit: 'cm' });

    expect(getMarkupToolDefinition(length)?.render?.getContentPrimitives(length, { page: pageStub(), pageScale, phase: 'idle' })).toEqual(expect.arrayContaining([
      expect.objectContaining({ kind: 'textBox', text: '5.00 m' }),
    ]));
    expect(getMarkupToolDefinition(polylength)?.render?.getContentPrimitives(polylength, { page: pageStub(), pageScale, phase: 'idle' })).toEqual(expect.arrayContaining([
      expect.objectContaining({ kind: 'textBox', text: '7.00 m' }),
    ]));
    expect(getMarkupToolDefinition(area)?.render?.getContentPrimitives(area, { page: pageStub(), pageScale, phase: 'idle' })).toEqual(expect.arrayContaining([
      expect.objectContaining({ kind: 'textBox', text: '12.00 m^2' }),
    ]));
    expect(getMarkupToolDefinition(centimeterLength)?.render?.getContentPrimitives(centimeterLength, { page: pageStub(), pageScale, phase: 'idle' })).toEqual(expect.arrayContaining([
      expect.objectContaining({ kind: 'textBox', text: '500.00 cm' }),
    ]));
  });

  it('backs Insert Image with media placement primitives', () => {
    const markup = createImageMarkup({
      id: 'image-1',
      pageIndex: 0,
      rect: rect(20, 30, 80, 50),
      dataUrl: DEFAULT_IMAGE_DATA_URL,
      mimeType: 'image/png',
    });
    const definition = getMarkupToolDefinition(markup);

    expect(definition?.id).toBe('image');
    expect(definition?.geometry?.getGeometry(markup, { page: pageStub() }).components[0]).toMatchObject({
      id: 'image.body',
      role: 'media',
      bodyDrag: 'moveSelf',
    });
    expect(definition?.geometry?.hitTest(markup, pdfPoint(40, 40), { page: pageStub(), tolerance: 3 })).toMatchObject({
      markupId: 'image-1',
      componentId: 'image.body',
      region: 'interior',
      bodyDrag: 'moveSelf',
    });
    expect(definition?.render?.getContentPrimitives(markup, { page: pageStub(), phase: 'idle' })[0]).toMatchObject({
      kind: 'image',
      rect: markup.rect,
      assetId: DEFAULT_IMAGE_DATA_URL,
      pointerEvents: 'all',
    });
    expect(definition?.selection?.getSelectionChrome(markup, { page: pageStub(), phase: 'focused' }).bounds).toMatchObject({
      kind: 'child',
      canResize: true,
      canRotate: true,
    });
  });

  it('backs Snapshot with stamp snapshot primitives', () => {
    const markup = createSnapshotMarkup({
      id: 'snapshot-1',
      pageIndex: 0,
      rect: rect(20, 30, 80, 50),
      dataUrl: DEFAULT_IMAGE_DATA_URL,
      mimeType: 'image/png',
    });
    const definition = getMarkupToolDefinition(markup);

    expect(definition?.id).toBe('snapshot');
    expect(definition?.geometry?.getGeometry(markup, { page: pageStub() }).components[0]).toMatchObject({
      id: 'snapshot.body',
      role: 'media',
      bodyDrag: 'moveSelf',
    });
    expect(definition?.render?.getContentPrimitives(markup, { page: pageStub(), phase: 'idle' })[0]).toMatchObject({
      kind: 'image',
      rect: markup.rect,
      assetId: DEFAULT_IMAGE_DATA_URL,
      pointerEvents: 'all',
    });
  });

  it('suppresses imported PDF link annotations', () => {
    const markup = createImportedAnnotationMarkup({
      id: 'link-1',
      pageIndex: 0,
      rect: rect(20, 30, 80, 20),
      subtype: 'Link',
      source: { source: 'imported' },
    });
    const definition = getMarkupToolDefinition(markup);

    expect(definition?.render?.getContentPrimitives(markup, { page: pageStub(), phase: 'idle' })).toEqual([]);
    expect(definition?.geometry?.hitTest(markup, pdfPoint(40, 40), { page: pageStub(), tolerance: 3 })).toBeNull();
    expect(definition?.selection?.getSelectionChrome(markup, { page: pageStub(), phase: 'focused' })).toEqual({});
  });

  it('backs Cloud+ with composite cloud, text and leader primitive contracts', () => {
    const markup = createCloudPlusMarkup({
      id: 'cloud-plus-1',
      pageIndex: 0,
      cloud: {
        controlPath: [pdfPoint(10, 10), pdfPoint(10, 60), pdfPoint(90, 60), pdfPoint(90, 10)],
        borderEffectIntensity: 2,
      },
      leader: { points: [pdfPoint(90, 35), pdfPoint(110, 35), pdfPoint(130, 35)] },
      textBox: rect(130, 15, 100, 40),
      text: 'Cloud+',
      color: '#ff0000',
    });
    const definition = getMarkupToolDefinition(markup);
    const geometry = definition?.geometry?.getGeometry(markup, { page: pageStub() });

    expect(getMarkupToolDefinition(markup)?.id).toBe('cloud-plus');
    expect(geometry?.components.map((component) => [component.id, component.bodyDrag])).toEqual([
      ['cloud-plus.cloud', 'moveGroup'],
      ['cloud-plus.textBox', 'moveSelf'],
      ['cloud-plus.leader', 'adjustOnly'],
    ]);
    expect(definition?.selection?.getSelectionChrome(markup, { page: pageStub(), phase: 'focused' }).bounds).toMatchObject({
      kind: 'group',
      canResize: false,
      canRotate: false,
    });
  });

  it('moves Cloud+ components according to Bluebeam-like composite semantics', () => {
    const markup = createCloudPlusMarkup({
      id: 'cloud-plus-1',
      pageIndex: 0,
      cloud: { controlPath: [pdfPoint(10, 10), pdfPoint(10, 60), pdfPoint(90, 60), pdfPoint(90, 10)] },
      leader: { points: [pdfPoint(90, 35), pdfPoint(110, 35), pdfPoint(130, 35)] },
      textBox: rect(130, 15, 100, 40),
      text: 'Cloud+',
    });
    const definition = getMarkupToolDefinition(markup);
    const groupMoved = definition?.interaction?.dragMarkup?.(markup, {
      componentId: 'cloud-plus.cloud',
      bodyDrag: 'moveGroup',
      delta: pdfPoint(5, 7),
    });
    const textMoved = definition?.interaction?.dragMarkup?.(markup, {
      componentId: 'cloud-plus.textBox',
      bodyDrag: 'moveSelf',
      delta: pdfPoint(5, 7),
    });

    expect(groupMoved?.kind).toBe('cloud-plus');
    if (groupMoved?.kind === 'cloud-plus') {
      expect(groupMoved.cloud.controlPath[0]).toEqual(pdfPoint(15, 17));
      expect(groupMoved.leader.points[0]).toEqual(pdfPoint(95, 42));
      expect(groupMoved.textBox).toEqual(rect(135, 22, 100, 40));
    }
    expect(textMoved?.kind).toBe('cloud-plus');
    if (textMoved?.kind === 'cloud-plus') {
      expect(textMoved.cloud.controlPath[0]).toEqual(pdfPoint(10, 10));
      expect(textMoved.leader.points[0]).toEqual(pdfPoint(90, 35));
      expect(textMoved.leader.points[2]).toEqual(pdfPoint(135, 42));
      expect(textMoved.textBox).toEqual(rect(135, 22, 100, 40));
    }
  });

  it('backs text box with the tool-maker primitive contracts', () => {
    const markup = {
      id: 'text-1',
      kind: 'text-box',
      pageIndex: 0,
      rect: rect(10, 10, 100, 50),
      text: 'Default text',
    } as const;
    const definition = getMarkupToolDefinition(markup);

    expect(definition?.geometry?.getGeometry(markup, { page: pageStub() }).components[0]).toMatchObject({
      id: 'text-box.body',
      role: 'textBox',
      bodyDrag: 'moveSelf',
    });
    expect(definition?.geometry?.hitTest(markup, pdfPoint(50, 30), { page: pageStub(), tolerance: 3 })).toMatchObject({
      markupId: 'text-1',
      componentId: 'text-box.body',
      region: 'interior',
      bodyDrag: 'moveSelf',
    });
    expect(definition?.render?.getContentPrimitives(markup, { page: pageStub(), phase: 'idle' })).toEqual(expect.arrayContaining([
      expect.objectContaining({ kind: 'textBox', text: 'Default text', pointerEvents: 'none' }),
    ]));
    expect(definition?.render?.getDraftPrimitives?.(createTextBoxDraft(pdfPoint(0, 0)), { page: pageStub(), phase: 'draft' })).toEqual(expect.arrayContaining([
      expect.objectContaining({ kind: 'textBox', text: '', pointerEvents: 'none' }),
    ]));
    expect(definition?.selection?.getDraftChrome?.(createTextBoxDraft(pdfPoint(0, 0)), { page: pageStub(), phase: 'draft' }).bounds).toMatchObject({
      kind: 'child',
      canResize: false,
      canRotate: false,
    });
  });

  it('keeps intentionally empty text boxes empty instead of showing the new-tool placeholder', () => {
    const markup = {
      id: 'text-1',
      kind: 'text-box',
      pageIndex: 0,
      rect: rect(10, 10, 100, 50),
      text: '',
    } as const;
    const definition = getMarkupToolDefinition(markup);

    expect(definition?.render?.getContentPrimitives(markup, { page: pageStub(), phase: 'idle' })).toEqual(expect.arrayContaining([
      expect.objectContaining({ kind: 'textBox', text: '' }),
    ]));
  });

  it('lets text box handles resize and rotate through the tool interaction primitive', () => {
    const markup = {
      id: 'text-1',
      kind: 'text-box',
      pageIndex: 0,
      rect: rect(10, 10, 100, 50),
      text: 'Default text',
    } as const;
    const definition = getMarkupToolDefinition(markup);
    const resized = definition?.interaction?.transformMarkup?.(markup, {
      handleId: 'text-box.resize.e',
      handleBehavior: 'resizeSelf',
      startPoint: pdfPoint(110, 35),
      currentPoint: pdfPoint(130, 35),
    });
    const rotated = definition?.interaction?.transformMarkup?.(markup, {
      handleId: 'text-box.rotate',
      handleBehavior: 'rotateSelf',
      startPoint: pdfPoint(60, 84),
      currentPoint: pdfPoint(135, 35),
    });

    expect(resized?.kind).toBe('text-box');
    if (resized?.kind === 'text-box') {
      expect(resized.rect).toEqual(rect(10, 10, 120, 50));
    }
    expect(rotated?.kind).toBe('text-box');
    if (rotated?.kind === 'text-box') {
      expect(rotated.rotation).toBeCloseTo(90);
      expect(rotated.rect).toEqual(markup.rect);
    }
  });

  it('backs rectangle with the tool-maker primitive contracts', () => {
    const markup = {
      id: 'rect-1',
      kind: 'rectangle',
      pageIndex: 0,
      rect: rect(10, 10, 100, 50),
    } as const;
    const definition = getMarkupToolDefinition(markup);

    expect(definition?.geometry?.getGeometry(markup, { page: pageStub() }).components[0]).toMatchObject({
      id: 'rectangle.body',
      bodyDrag: 'moveSelf',
    });
    expect(definition?.render?.getContentPrimitives(markup, { page: pageStub(), phase: 'idle' })[0]).toMatchObject({
      kind: 'rect',
      pointerEvents: 'visibleStroke',
    });
    expect(definition?.render?.getDraftPrimitives?.(createRectangleDraft(pdfPoint(0, 0)), { page: pageStub(), phase: 'draft' })[0]).toMatchObject({
      kind: 'rect',
      pointerEvents: 'none',
    });
    expect(definition?.selection?.getSelectionChrome(markup, { page: pageStub(), phase: 'focused' }).bounds).toMatchObject({
      kind: 'child',
      canResize: true,
      canRotate: true,
    });
    expect(definition?.selection?.getSelectionChrome(markup, { page: pageStub(), phase: 'hovered' }).bounds).toMatchObject({
      kind: 'child',
      canResize: true,
      canRotate: false,
    });
    expect(definition?.selection?.getDraftChrome?.(createRectangleDraft(pdfPoint(0, 0)), { page: pageStub(), phase: 'draft' }).bounds).toMatchObject({
      kind: 'child',
      canResize: false,
      canRotate: false,
    });
  });

  it('lets rectangle handles resize through the tool interaction primitive', () => {
    const markup = {
      id: 'rect-1',
      kind: 'rectangle',
      pageIndex: 0,
      rect: rect(10, 10, 100, 50),
    } as const;
    const definition = getMarkupToolDefinition(markup);
    const resized = definition?.interaction?.transformMarkup?.(markup, {
      handleId: 'rectangle.resize.e',
      handleBehavior: 'resizeSelf',
      startPoint: pdfPoint(110, 35),
      currentPoint: pdfPoint(130, 35),
    });

    expect(resized?.kind).toBe('rectangle');
    if (resized?.kind === 'rectangle') {
      expect(resized.rect).toEqual(rect(10, 10, 120, 50));
    }
  });

  it('lets the rectangle rotation handle update rotation without changing content style', () => {
    const markup = {
      id: 'rect-1',
      kind: 'rectangle',
      pageIndex: 0,
      rect: rect(10, 10, 100, 50),
    } as const;
    const definition = getMarkupToolDefinition(markup);
    const rotated = definition?.interaction?.transformMarkup?.(markup, {
      handleId: 'rectangle.rotate',
      handleBehavior: 'rotateSelf',
      startPoint: pdfPoint(60, 84),
      currentPoint: pdfPoint(135, 35),
    });

    expect(rotated?.kind).toBe('rectangle');
    if (rotated?.kind === 'rectangle') {
      expect(rotated.rotation).toBeCloseTo(90);
      expect(rotated.rect).toEqual(markup.rect);
    }
  });

  it('carries rectangle rotation into selection chrome bounds', () => {
    const markup = {
      id: 'rect-1',
      kind: 'rectangle',
      pageIndex: 0,
      rect: rect(10, 10, 100, 50),
      rotation: 37,
    } as const;
    const definition = getMarkupToolDefinition(markup);

    expect(definition?.selection?.getSelectionChrome(markup, { page: pageStub(), phase: 'focused' }).bounds).toMatchObject({
      rect: markup.rect,
      rotation: 37,
    });
  });

  it('backs ellipse with the tool-maker primitive contracts', () => {
    const markup = {
      id: 'ellipse-1',
      kind: 'ellipse',
      pageIndex: 0,
      rect: rect(10, 10, 100, 50),
    } as const;
    const definition = getMarkupToolDefinition(markup);

    expect(definition?.geometry?.getGeometry(markup, { page: pageStub() }).components[0]).toMatchObject({
      id: 'ellipse.body',
      bodyDrag: 'moveSelf',
    });
    expect(definition?.geometry?.hitTest(markup, pdfPoint(110, 35), { page: pageStub(), tolerance: 3 })).toMatchObject({
      markupId: 'ellipse-1',
      componentId: 'ellipse.body',
      region: 'edge',
      bodyDrag: 'moveSelf',
    });
    expect(definition?.geometry?.hitTest(markup, pdfPoint(60, 35), { page: pageStub(), tolerance: 3 })).toBeNull();
    expect(definition?.render?.getContentPrimitives(markup, { page: pageStub(), phase: 'idle' })[0]).toMatchObject({
      kind: 'ellipse',
      pointerEvents: 'visibleStroke',
    });
    expect(definition?.render?.getDraftPrimitives?.(createRectangleDraft(pdfPoint(0, 0)), { page: pageStub(), phase: 'draft' })[0]).toMatchObject({
      kind: 'ellipse',
      pointerEvents: 'none',
    });
    expect(definition?.selection?.getSelectionChrome(markup, { page: pageStub(), phase: 'focused' }).bounds).toMatchObject({
      kind: 'child',
      canResize: true,
      canRotate: true,
    });
  });

  it('backs arc with CircleArc primitive contracts', () => {
    const markup = createArcMarkup({
      id: 'arc-1',
      pageIndex: 0,
      rect: rect(20, 30, 80, 60),
      angle1: 90,
      angle2: 180,
    });
    const definition = getMarkupToolDefinition(markup);

    expect(definition?.id).toBe('arc');
    expect(definition?.geometry?.getGeometry(markup, { page: pageStub() }).components[0]).toMatchObject({
      id: 'arc.body',
      role: 'shape',
      bodyDrag: 'moveSelf',
    });
    expect(definition?.geometry?.hitTest(markup, pdfPoint(20, 70), { page: pageStub(), tolerance: 4 })).toMatchObject({
      markupId: 'arc-1',
      componentId: 'arc.body',
      region: 'edge',
    });
    expect(definition?.render?.getContentPrimitives(markup, { page: pageStub(), phase: 'idle' })[0]).toMatchObject({
      kind: 'path',
      pointerEvents: 'visibleStroke',
    });
    expect(definition?.selection?.getSelectionChrome(markup, { page: pageStub(), phase: 'focused' }).controlPaths?.[0]).toMatchObject({
      id: 'arc.path',
      closed: false,
    });
    expect(definition?.selection?.getSelectionChrome(markup, { page: pageStub(), phase: 'focused' }).controlPaths?.[0]?.points.length).toBeGreaterThan(2);
  });

  it('creates arc geometry from endpoints and a midpoint handle', () => {
    const definition = getToolDefinition('arc');
    const markup = definition.interaction?.transformMarkup?.(
      createArcMarkup({
        id: 'arc-1',
        pageIndex: 0,
        rect: rect(10, 10, 100, 100),
        angle1: 180,
        angle2: 0,
        start: pdfPoint(10, 60),
        end: pdfPoint(110, 60),
        mid: pdfPoint(60, 110),
      }),
      {
        handleId: 'arc.point.mid',
        handleBehavior: 'reshapeArc',
        startPoint: pdfPoint(60, 110),
        currentPoint: pdfPoint(72, 120),
      },
    );

    expect(markup).toMatchObject({ kind: 'arc' });
    if (markup?.kind === 'arc') {
      expect(markup.start).toEqual(pdfPoint(10, 60));
      expect(markup.end).toEqual(pdfPoint(110, 60));
      expect(markup.mid?.x).toBeCloseTo(60);
      expect(markup.mid?.y).toBeCloseTo(120);
      expect(markup.rect.width).toBeCloseTo(markup.rect.height);
    }
  });

  it('lets ellipse handles resize and rotate through the tool interaction primitive', () => {
    const markup = {
      id: 'ellipse-1',
      kind: 'ellipse',
      pageIndex: 0,
      rect: rect(10, 10, 100, 50),
    } as const;
    const definition = getMarkupToolDefinition(markup);
    const resized = definition?.interaction?.transformMarkup?.(markup, {
      handleId: 'ellipse.resize.e',
      handleBehavior: 'resizeSelf',
      startPoint: pdfPoint(110, 35),
      currentPoint: pdfPoint(130, 35),
    });
    const rotated = definition?.interaction?.transformMarkup?.(markup, {
      handleId: 'ellipse.rotate',
      handleBehavior: 'rotateSelf',
      startPoint: pdfPoint(60, 84),
      currentPoint: pdfPoint(135, 35),
    });

    expect(resized?.kind).toBe('ellipse');
    if (resized?.kind === 'ellipse') {
      expect(resized.rect).toEqual(rect(10, 10, 120, 50));
    }
    expect(rotated?.kind).toBe('ellipse');
    if (rotated?.kind === 'ellipse') {
      expect(rotated.rotation).toBeCloseTo(90);
      expect(rotated.rect).toEqual(markup.rect);
    }
  });

  it('backs line and arrow with shared endpoint line primitives', () => {
    const line = {
      id: 'line-1',
      kind: 'line',
      pageIndex: 0,
      start: pdfPoint(10, 10),
      end: pdfPoint(110, 60),
    } as const;
    const arrow = {
      id: 'arrow-1',
      kind: 'arrow',
      pageIndex: 0,
      start: pdfPoint(20, 20),
      end: pdfPoint(120, 70),
    } as const;
    const lineDefinition = getMarkupToolDefinition(line);
    const arrowDefinition = getMarkupToolDefinition(arrow);

    expect(lineDefinition?.geometry?.getGeometry(line, { page: pageStub() }).components[0]).toMatchObject({
      id: 'line.body',
      bodyDrag: 'moveSelf',
    });
    expect(lineDefinition?.geometry?.hitTest(line, pdfPoint(60, 35), { page: pageStub(), tolerance: 3 })).toMatchObject({
      markupId: 'line-1',
      componentId: 'line.body',
      region: 'edge',
    });
    expect(lineDefinition?.geometry?.getGeometry(line, { page: pageStub() }).handles?.map((handle) => handle.behavior)).toEqual(['moveEndpoint', 'moveEndpoint']);
    expect(lineDefinition?.render?.getContentPrimitives(line, { page: pageStub(), phase: 'idle' })).toEqual([
      expect.objectContaining({ kind: 'polyline', pointerEvents: 'visibleStroke' }),
    ]);
    expect(lineDefinition?.selection?.getSelectionChrome(line, { page: pageStub(), phase: 'focused' }).controlPaths).toEqual([
      { id: 'line.path', points: [line.start, line.end], closed: false },
    ]);

    expect(arrowDefinition?.render?.getContentPrimitives(arrow, { page: pageStub(), phase: 'idle' })).toEqual([
      expect.objectContaining({ kind: 'polyline' }),
      expect.objectContaining({ kind: 'polygon' }),
    ]);
    expect(arrowDefinition?.selection?.getSelectionChrome(arrow, { page: pageStub(), phase: 'focused' }).controlPaths).toEqual([
      { id: 'arrow.path', points: [arrow.start, arrow.end], closed: false },
    ]);
  });

  it('lets line and arrow endpoint handles reshape endpoints', () => {
    const line = {
      id: 'line-1',
      kind: 'line',
      pageIndex: 0,
      start: pdfPoint(10, 10),
      end: pdfPoint(110, 60),
    } as const;
    const arrow = {
      id: 'arrow-1',
      kind: 'arrow',
      pageIndex: 0,
      start: pdfPoint(20, 20),
      end: pdfPoint(120, 70),
    } as const;
    const movedLineStart = getMarkupToolDefinition(line)?.interaction?.transformMarkup?.(line, {
      handleId: 'line.endpoint.start',
      handleBehavior: 'moveEndpoint',
      startPoint: line.start,
      currentPoint: pdfPoint(0, 0),
    });
    const movedArrowEnd = getMarkupToolDefinition(arrow)?.interaction?.transformMarkup?.(arrow, {
      handleId: 'arrow.endpoint.end',
      handleBehavior: 'moveEndpoint',
      startPoint: arrow.end,
      currentPoint: pdfPoint(140, 100),
    });

    expect(movedLineStart).toMatchObject({ kind: 'line', start: pdfPoint(0, 0), end: line.end });
    expect(movedArrowEnd).toMatchObject({ kind: 'arrow', start: arrow.start, end: pdfPoint(140, 100) });
  });

  it('backs Dimension with Bluebeam line-dimension primitives', () => {
    const leftToRight = getToolDefinition('dimension').interaction?.commitDraft?.({
      kind: 'line',
      start: pdfPoint(10, 10),
      current: pdfPoint(110, 10),
    }, {
      page: pageStub(),
      hasExceededDragThreshold: true,
      createMarkupId: () => 'dimension-1',
    });
    const rightToLeft = getToolDefinition('dimension').interaction?.commitDraft?.({
      kind: 'line',
      start: pdfPoint(110, 10),
      current: pdfPoint(10, 10),
    }, {
      page: pageStub(),
      hasExceededDragThreshold: true,
      createMarkupId: () => 'dimension-2',
    });
    const markup = createDimensionMarkup({
      id: 'dimension-3',
      pageIndex: 0,
      start: pdfPoint(10, 10),
      end: pdfPoint(110, 10),
      dimensionLineOffset: 24,
      text: '100 ft',
    });
    const definition = getMarkupToolDefinition(markup);

    expect(leftToRight).toMatchObject({ kind: 'dimension', dimensionLineOffset: 24 });
    expect(rightToLeft).toMatchObject({ kind: 'dimension', dimensionLineOffset: -24 });
    expect(definition?.geometry?.getGeometry(markup, { page: pageStub() }).components.map((component) => component.id)).toEqual([
      'dimension.body',
      'dimension.caption',
    ]);
    expect(definition?.geometry?.getGeometry(markup, { page: pageStub() }).handles?.map((handle) => handle.id)).toEqual([
      'dimension.endpoint.start',
      'dimension.endpoint.end',
      'dimension.offset',
    ]);
    const primitives = definition?.render?.getContentPrimitives(markup, { page: pageStub(), phase: 'idle' });
    expect(primitives).toEqual([
      expect.objectContaining({ kind: 'polyline' }),
      expect.objectContaining({ kind: 'polyline' }),
      expect.objectContaining({ kind: 'polyline' }),
      expect.objectContaining({ kind: 'polyline' }),
      expect.objectContaining({ kind: 'polygon' }),
      expect.objectContaining({ kind: 'polygon' }),
      expect.objectContaining({ kind: 'textBox', text: '100 ft' }),
    ]);
    expect(primitives?.[0]).toMatchObject({
      kind: 'polyline',
      points: [pdfPoint(10, 34), pdfPoint(38, 34)],
    });
    expect(primitives?.[1]).toMatchObject({
      kind: 'polyline',
      points: [pdfPoint(82, 34), pdfPoint(110, 34)],
    });
  });

  it('lets Dimension handles reshape endpoints and dimension-line height', () => {
    const markup = createDimensionMarkup({
      id: 'dimension-1',
      pageIndex: 0,
      start: pdfPoint(10, 10),
      end: pdfPoint(110, 10),
      dimensionLineOffset: 24,
      text: '100 ft',
    });
    const movedEndpoint = getMarkupToolDefinition(markup)?.interaction?.transformMarkup?.(markup, {
      handleId: 'dimension.endpoint.end',
      handleBehavior: 'moveEndpoint',
      startPoint: markup.end,
      currentPoint: pdfPoint(120, 12),
    });
    const movedOffset = getMarkupToolDefinition(markup)?.interaction?.transformMarkup?.(markup, {
      handleId: 'dimension.offset',
      handleBehavior: 'moveKnee',
      startPoint: pdfPoint(60, 34),
      currentPoint: pdfPoint(60, 50),
    });

    expect(movedEndpoint).toMatchObject({ kind: 'dimension', end: pdfPoint(120, 12), dimensionLineOffset: 24 });
    expect(movedOffset).toMatchObject({ kind: 'dimension', dimensionLineOffset: 40 });
  });

  it('keeps Dimension extension marks perpendicular to diagonal dimensions', () => {
    const markup = createDimensionMarkup({
      id: 'dimension-1',
      pageIndex: 0,
      start: pdfPoint(10, 10),
      end: pdfPoint(110, 60),
      dimensionLineOffset: 20,
      text: '112 ft',
    });
    const primitives = getMarkupToolDefinition(markup)?.render?.getContentPrimitives(markup, { page: pageStub(), phase: 'idle' });
    const startExtension = primitives?.[2];
    const endExtension = primitives?.[3];

    expect(startExtension).toMatchObject({ kind: 'polyline' });
    expect(endExtension).toMatchObject({ kind: 'polyline' });
    if (startExtension?.kind !== 'polyline' || endExtension?.kind !== 'polyline') {
      throw new Error('Expected dimension extension polylines');
    }

    const unit = { x: 100 / Math.hypot(100, 50), y: 50 / Math.hypot(100, 50) };
    const startVector = {
      x: startExtension.points[2].x - startExtension.points[0].x,
      y: startExtension.points[2].y - startExtension.points[0].y,
    };
    const endVector = {
      x: endExtension.points[2].x - endExtension.points[0].x,
      y: endExtension.points[2].y - endExtension.points[0].y,
    };

    expect(startVector.x * unit.x + startVector.y * unit.y).toBeCloseTo(0, 6);
    expect(endVector.x * unit.x + endVector.y * unit.y).toBeCloseTo(0, 6);
  });

  it('backs polyline with vertex handles and PDF polyline mapping', () => {
    const markup = {
      id: 'polyline-1',
      kind: 'polyline',
      pageIndex: 0,
      points: [pdfPoint(10, 10), pdfPoint(60, 40), pdfPoint(110, 20)],
    } as const;
    const definition = getMarkupToolDefinition(markup);

    expect(definition?.geometry?.getGeometry(markup, { page: pageStub() }).components[0]).toMatchObject({
      id: 'polyline.body',
      bodyDrag: 'moveSelf',
    });
    expect(definition?.geometry?.getGeometry(markup, { page: pageStub() }).handles?.map((handle) => handle.behavior)).toEqual([
      'reshapeVertex',
      'reshapeVertex',
      'reshapeVertex',
    ]);
    expect(definition?.geometry?.hitTest(markup, pdfPoint(35, 25), { page: pageStub(), tolerance: 3 })).toMatchObject({
      markupId: 'polyline-1',
      componentId: 'polyline.body',
      region: 'edge',
    });
    expect(definition?.render?.getContentPrimitives(markup, { page: pageStub(), phase: 'idle' })[0]).toMatchObject({
      kind: 'polyline',
      pointerEvents: 'visibleStroke',
    });
    expect(definition?.selection?.getSelectionChrome(markup, { page: pageStub(), phase: 'focused' }).controlPaths).toEqual([
      { id: 'polyline.path', points: markup.points, closed: false },
    ]);
  });

  it('lets polyline vertex handles reshape a single vertex', () => {
    const markup = {
      id: 'polyline-1',
      kind: 'polyline',
      pageIndex: 0,
      points: [pdfPoint(10, 10), pdfPoint(60, 40), pdfPoint(110, 20)],
    } as const;
    const moved = getMarkupToolDefinition(markup)?.interaction?.transformMarkup?.(markup, {
      handleId: 'polyline.vertex.1',
      handleBehavior: 'reshapeVertex',
      startPoint: pdfPoint(60, 40),
      currentPoint: pdfPoint(70, 55),
    });

    expect(moved).toMatchObject({
      kind: 'polyline',
      points: [pdfPoint(10, 10), pdfPoint(70, 55), pdfPoint(110, 20)],
    });
  });

  it('backs polygon with closed vertex-shape primitives', () => {
    const markup = {
      id: 'polygon-1',
      kind: 'polygon',
      pageIndex: 0,
      points: [pdfPoint(10, 10), pdfPoint(80, 40), pdfPoint(110, 15), pdfPoint(60, -10)],
    } as const;
    const definition = getMarkupToolDefinition(markup);

    expect(definition?.geometry?.getGeometry(markup, { page: pageStub() }).components[0]).toMatchObject({
      id: 'polygon.body',
      bodyDrag: 'moveSelf',
    });
    expect(definition?.geometry?.getGeometry(markup, { page: pageStub() }).handles?.map((handle) => handle.behavior)).toEqual([
      'reshapeVertex',
      'reshapeVertex',
      'reshapeVertex',
      'reshapeVertex',
    ]);
    expect(definition?.geometry?.hitTest(markup, pdfPoint(45, 25), { page: pageStub(), tolerance: 3 })).toMatchObject({
      markupId: 'polygon-1',
      componentId: 'polygon.body',
      region: 'edge',
    });
    expect(definition?.render?.getContentPrimitives(markup, { page: pageStub(), phase: 'idle' })[0]).toMatchObject({
      kind: 'polygon',
      pointerEvents: 'visibleStroke',
    });
    expect(definition?.selection?.getSelectionChrome(markup, { page: pageStub(), phase: 'focused' }).controlPaths).toEqual([
      { id: 'polygon.path', points: markup.points, closed: true },
    ]);
  });

  it('lets polygon vertex handles reshape a single vertex', () => {
    const markup = {
      id: 'polygon-1',
      kind: 'polygon',
      pageIndex: 0,
      points: [pdfPoint(10, 10), pdfPoint(80, 40), pdfPoint(110, 15), pdfPoint(60, -10)],
    } as const;
    const moved = getMarkupToolDefinition(markup)?.interaction?.transformMarkup?.(markup, {
      handleId: 'polygon.vertex.2',
      handleBehavior: 'reshapeVertex',
      startPoint: pdfPoint(110, 15),
      currentPoint: pdfPoint(120, 30),
    });

    expect(moved).toMatchObject({
      kind: 'polygon',
      points: [pdfPoint(10, 10), pdfPoint(80, 40), pdfPoint(120, 30), pdfPoint(60, -10)],
    });
  });
});

describe('annotation lifecycle primitives', () => {
  it('normalizes rectangle drafts and enforces a commit size', () => {
    const draft = createRectangleDraft(pdfPoint(20, 30));
    const box = rectangleDraftToRect({ ...draft, current: pdfPoint(10, 12) });

    expect(box).toEqual(rect(10, 12, 10, 18));
    expect(shouldCommitRectangle(box)).toBe(true);
    expect(shouldCommitRectangle(rect(0, 0, 2, 40))).toBe(false);
  });

  it('normalizes text box drafts through the same bounds primitive', () => {
    const draft = createTextBoxDraft(pdfPoint(20, 30));
    const box = textBoxDraftToRect({ ...draft, current: pdfPoint(10, 12) });

    expect(box).toEqual(rect(10, 12, 10, 18));
    expect(shouldCommitRectangle(box)).toBe(true);
  });

  it('uses a pointer drag threshold', () => {
    expect(hasExceededDragThreshold({ x: 0, y: 0 }, { x: 2, y: 2 })).toBe(false);
    expect(hasExceededDragThreshold({ x: 0, y: 0 }, { x: 3, y: 4 })).toBe(true);
  });

  it('resizes rectangle bounds from edge and corner handles with a minimum size', () => {
    const original = rect(10, 20, 100, 50);

    expect(resizeRectFromHandle(original, 'e', pdfPoint(130, 40))).toEqual(rect(10, 20, 120, 50));
    expect(resizeRectFromHandle(original, 'nw', pdfPoint(0, 80))).toEqual(rect(0, 20, 110, 60));
    expect(resizeRectFromHandle(original, 'sw', pdfPoint(0, 10))).toEqual(rect(0, 10, 110, 60));
    expect(resizeRectFromHandle(original, 'w', pdfPoint(120, 40))).toEqual(rect(108, 20, 2, 50));
  });

  it('resizes rotated rectangle bounds in local axes while keeping the opposite anchor visually fixed', () => {
    const original = rect(10, 10, 100, 50);

    expect(resizeRotatedRectFromHandle(original, 90, 'e', pdfPoint(60, -35))).toEqual(rect(0, 0, 120, 50));
  });
});

describe('annotation content styles', () => {
  it('keeps the default rectangle body transparent so interaction chrome owns state affordances', () => {
    const markup = {
      id: 'rect-1',
      kind: 'rectangle',
      pageIndex: 0,
      rect: rect(10, 10, 100, 50),
    } as const;

    expect(getAnnotationContentStyle(markup).fill).toBe('none');
    expect(getAnnotationContentStyle(markup).stroke).toBe('#ff0000');
    expect(getAnnotationContentStyle(markup).strokeWidth).toBe(1);
  });
});

describe('hit testing', () => {
  it('selects rectangle edges without treating interiors or stroke colour as fill targets', () => {
    const markup = {
      id: 'rect-1',
      kind: 'rectangle',
      pageIndex: 0,
      rect: rect(10, 10, 100, 50),
      color: '#2563eb',
    } as const;

    expect(hitTestMarkup(markup, pdfPoint(11, 20), { tolerance: 3 })).toEqual({ markupId: 'rect-1', region: 'edge' });
    expect(hitTestMarkup(markup, pdfPoint(50, 30), { tolerance: 3 })).toBeNull();
  });

  it('hit-tests rotated rectangle edges where they are visually rendered', () => {
    const markup = {
      id: 'rect-1',
      kind: 'rectangle',
      pageIndex: 0,
      rect: rect(10, 10, 100, 50),
      rotation: 90,
    } as const;

    expect(hitTestMarkup(markup, pdfPoint(85, 35), { tolerance: 3 })).toEqual({ markupId: 'rect-1', region: 'edge' });
    expect(hitTestMarkup(markup, pdfPoint(60, 60), { tolerance: 3 })).toBeNull();
    expect(getMarkupToolDefinition(markup)?.geometry?.hitTest(markup, pdfPoint(85, 35), { page: pageStub(), tolerance: 3 })).toMatchObject({
      markupId: 'rect-1',
      componentId: 'rectangle.body',
      region: 'edge',
      bodyDrag: 'moveSelf',
    });
    expect(getMarkupToolDefinition(markup)?.geometry?.hitTest(markup, pdfPoint(85, 35), { page: pageStub(), tolerance: 3, transform: pageTransformStub() })).toMatchObject({
      markupId: 'rect-1',
      componentId: 'rectangle.body',
      region: 'edge',
      bodyDrag: 'moveSelf',
    });
    expect(getMarkupToolDefinition(markup)?.geometry?.hitTest(markup, pdfPoint(60, 60), { page: pageStub(), tolerance: 3, transform: pageTransformStub() })).toBeNull();
  });

  it('selects ellipse edges without treating interiors as fill targets', () => {
    const markup = {
      id: 'ellipse-1',
      kind: 'ellipse',
      pageIndex: 0,
      rect: rect(10, 10, 100, 50),
    } as const;

    expect(hitTestMarkup(markup, pdfPoint(110, 35), { tolerance: 3 })).toEqual({ markupId: 'ellipse-1', region: 'edge' });
    expect(hitTestMarkup(markup, pdfPoint(60, 35), { tolerance: 3 })).toBeNull();
  });

  it('hit-tests line and arrow strokes without treating their bounding boxes as fill targets', () => {
    const line = {
      id: 'line-1',
      kind: 'line',
      pageIndex: 0,
      start: pdfPoint(10, 10),
      end: pdfPoint(110, 60),
    } as const;
    const arrow = {
      id: 'arrow-1',
      kind: 'arrow',
      pageIndex: 0,
      start: pdfPoint(10, 10),
      end: pdfPoint(110, 60),
    } as const;

    expect(hitTestMarkup(line, pdfPoint(60, 35), { tolerance: 3 })).toEqual({ markupId: 'line-1', region: 'edge' });
    expect(hitTestMarkup(line, pdfPoint(60, 50), { tolerance: 3 })).toBeNull();
    expect(hitTestMarkup(arrow, pdfPoint(60, 35), { tolerance: 3 })).toEqual({ markupId: 'arrow-1', region: 'edge' });
  });

  it('hit-tests polyline segments without treating the whole bounds as fill', () => {
    const markup = {
      id: 'polyline-1',
      kind: 'polyline',
      pageIndex: 0,
      points: [pdfPoint(10, 10), pdfPoint(60, 40), pdfPoint(110, 20)],
    } as const;

    expect(hitTestMarkup(markup, pdfPoint(35, 25), { tolerance: 3 })).toEqual({ markupId: 'polyline-1', region: 'edge' });
    expect(hitTestMarkup(markup, pdfPoint(60, 20), { tolerance: 3 })).toBeNull();
  });

  it('hit-tests polygon edges without treating interiors as fill targets', () => {
    const markup = {
      id: 'polygon-1',
      kind: 'polygon',
      pageIndex: 0,
      points: [pdfPoint(10, 10), pdfPoint(80, 40), pdfPoint(110, 15), pdfPoint(60, -10)],
    } as const;

    expect(hitTestMarkup(markup, pdfPoint(45, 25), { tolerance: 3 })).toEqual({ markupId: 'polygon-1', region: 'edge' });
    expect(hitTestMarkup(markup, pdfPoint(70, 15), { tolerance: 3 })).toBeNull();
  });

  it('backs pen and highlight with ink path primitives and PDF ink mapping', () => {
    const pen = {
      id: 'pen-1',
      kind: 'pen',
      pageIndex: 0,
      paths: [[pdfPoint(10, 10), pdfPoint(60, 40), pdfPoint(110, 20)]],
    } as const;
    const highlight = {
      id: 'highlight-1',
      kind: 'highlight',
      pageIndex: 0,
      paths: [[pdfPoint(10, 60), pdfPoint(110, 60)]],
    } as const;

    expect(getMarkupToolDefinition(pen)?.render?.getContentPrimitives(pen, { page: pageStub(), phase: 'idle' })).toEqual([
      expect.objectContaining({ kind: 'polyline', pointerEvents: 'visibleStroke' }),
    ]);
    expect(getMarkupToolDefinition(highlight)?.render?.getContentPrimitives(highlight, { page: pageStub(), phase: 'idle' })).toEqual([
      expect.objectContaining({
        kind: 'polyline',
        style: expect.objectContaining({ stroke: '#ffff00', strokeWidth: 12, blendMode: 'multiply' }),
      }),
    ]);
  });

  it('hit-tests ink strokes using their visible stroke width', () => {
    const markup = {
      id: 'highlight-1',
      kind: 'highlight',
      pageIndex: 0,
      paths: [[pdfPoint(10, 60), pdfPoint(110, 60)]],
      strokeWidth: 12,
    } as const;

    expect(hitTestMarkup(markup, pdfPoint(60, 65), { tolerance: 2 })).toEqual({ markupId: 'highlight-1', region: 'edge' });
    expect(hitTestMarkup(markup, pdfPoint(60, 75), { tolerance: 2 })).toBeNull();
  });

  it('backs cloud with generated line-type geometry and PDF border-effect metadata', () => {
    const markup = {
      id: 'cloud-1',
      kind: 'cloud',
      pageIndex: 0,
      controlPath: [pdfPoint(10, 10), pdfPoint(10, 60), pdfPoint(100, 60), pdfPoint(100, 10)],
      borderEffectIntensity: 2,
    } as const;
    const definition = getMarkupToolDefinition(markup);

    expect(definition?.geometry?.getGeometry(markup, { page: pageStub() }).components[0]).toMatchObject({
      id: 'cloud.body',
      geometry: expect.objectContaining({
        kind: 'generatedPath',
        lineType: expect.objectContaining({ id: 'cloud' }),
      }),
      bodyDrag: 'moveSelf',
    });
    expect(definition?.render?.getContentPrimitives(markup, { page: pageStub(), phase: 'idle' })[0]).toMatchObject({
      kind: 'path',
      d: expect.stringContaining('C'),
    });
    expect(definition?.selection?.getSelectionChrome(markup, { page: pageStub(), phase: 'focused' }).controlPaths?.[0]).toMatchObject({
      id: 'cloud.controlPath',
      closed: true,
    });
  });

  it('supports nodal cloud drafts for click-created polygon clouds', () => {
    const definition = getToolDefinition('cloud');
    const draft = updateCloudNodeDraft(
      addCloudNodeDraftPoint(
        addCloudNodeDraftPoint(createCloudNodeDraft(pdfPoint(10, 10)), pdfPoint(40, 50)),
        pdfPoint(90, 10),
      ),
      pdfPoint(120, 45),
    );

    expect(definition.render?.getDraftPrimitives?.(draft, { page: pageStub(), phase: 'draft' })[0]).toMatchObject({
      kind: 'path',
      d: expect.stringContaining('C'),
    });
    expect(definition.selection?.getDraftChrome?.(draft, { page: pageStub(), phase: 'draft' })).toMatchObject({
      bounds: { kind: 'child', canResize: false, canRotate: false },
      handles: expect.arrayContaining([
        expect.objectContaining({ id: 'cloud.vertex.0', behavior: 'reshapeVertex' }),
        expect.objectContaining({ id: 'cloud.vertex.1', behavior: 'reshapeVertex' }),
        expect.objectContaining({ id: 'cloud.vertex.2', behavior: 'reshapeVertex' }),
      ]),
      controlPaths: [expect.objectContaining({ id: 'cloud.draftControlPath', closed: true })],
    });

    const markup = definition.interaction?.commitDraft?.(draft, {
      page: pageStub(),
      hasExceededDragThreshold: true,
      createMarkupId: (prefix) => `${prefix}-1`,
    });

    expect(markup).toMatchObject({
      id: 'cloud-1',
      kind: 'cloud',
      controlPath: [pdfPoint(10, 10), pdfPoint(40, 50), pdfPoint(90, 10)],
      borderEffectIntensity: 2,
    });
  });

  it('backs callout with composite text box and leader primitives', () => {
    const markup = createCalloutMarkup({
      id: 'callout-1',
      pageIndex: 0,
      textBox: rect(100, 60, 130, 42),
      leader: { points: [pdfPoint(100, 81), pdfPoint(70, 81), pdfPoint(40, 40)] },
      text: 'Need to check',
      color: '#ff0000',
    });
    const definition = getMarkupToolDefinition(markup);

    expect(definition?.geometry?.getGeometry(markup, { page: pageStub() }).components).toEqual(expect.arrayContaining([
      expect.objectContaining({ id: 'callout.textBox', role: 'textBox', bodyDrag: 'moveSelf' }),
      expect.objectContaining({ id: 'callout.leader', role: 'leader', bodyDrag: 'moveGroup' }),
    ]));
    expect(definition?.geometry?.hitTest(markup, pdfPoint(120, 80), { page: pageStub(), tolerance: 3 })).toMatchObject({
      componentId: 'callout.textBox',
      bodyDrag: 'moveSelf',
    });
    expect(definition?.geometry?.hitTest(markup, pdfPoint(69, 81), { page: pageStub(), tolerance: 3 })).toMatchObject({
      componentId: 'callout.leader',
      bodyDrag: 'moveGroup',
    });
    expect(definition?.render?.getContentPrimitives(markup, { page: pageStub(), phase: 'idle' })).toEqual(expect.arrayContaining([
      expect.objectContaining({ kind: 'textBox', text: 'Need to check' }),
      expect.objectContaining({ kind: 'polyline', points: markup.leader.points }),
    ]));
    expect(definition?.selection?.getSelectionChrome(markup, { page: pageStub(), phase: 'focused' })).toMatchObject({
      bounds: { kind: 'group', canResize: false, canRotate: false },
      handles: expect.arrayContaining([
        expect.objectContaining({ id: 'callout.textBox.resize.e', behavior: 'resizeSelf' }),
        expect.objectContaining({ id: 'callout.leader.connection', behavior: 'moveEndpoint' }),
        expect.objectContaining({ id: 'callout.leader.knee.1', behavior: 'moveKnee' }),
        expect.objectContaining({ id: 'callout.leader.tip', behavior: 'moveEndpoint' }),
      ]),
      controlPaths: [expect.objectContaining({ id: 'callout.leaderPath', closed: false })],
    });
  });

  it('moves callout text boxes independently while preserving the leader connection', () => {
    const markup = createCalloutMarkup({
      id: 'callout-1',
      pageIndex: 0,
      textBox: rect(100, 60, 130, 42),
      leader: { points: [pdfPoint(100, 81), pdfPoint(70, 81), pdfPoint(40, 40)] },
      text: 'Need to check',
    });
    const definition = getMarkupToolDefinition(markup);
    const movedTextBox = definition?.interaction?.dragMarkup?.(markup, {
      componentId: 'callout.textBox',
      bodyDrag: 'moveSelf',
      delta: pdfPoint(10, -5),
    });
    const movedGroup = definition?.interaction?.dragMarkup?.(markup, {
      componentId: 'callout.leader',
      bodyDrag: 'moveGroup',
      delta: pdfPoint(10, -5),
    });

    expect(movedTextBox?.kind).toBe('callout');
    expect(movedGroup?.kind).toBe('callout');
    if (movedTextBox?.kind === 'callout') {
      expect(movedTextBox.textBox).toEqual(rect(110, 55, 130, 42));
      expect(movedTextBox.leader.points).toEqual([pdfPoint(100, 81), pdfPoint(70, 81), pdfPoint(50, 35)]);
    }
    if (movedGroup?.kind === 'callout') {
      expect(movedGroup.textBox).toEqual(rect(110, 55, 130, 42));
      expect(movedGroup.leader.points).toEqual([pdfPoint(110, 76), pdfPoint(80, 76), pdfPoint(50, 35)]);
    }
  });

  it('imports FreeTextCallout annotations into editable callout primitives', () => {
    const definition = getToolDefinition('callout');
    const markup = definition.pdf?.import({
      subtype: 'FreeText',
      rect: [196.1939, 530.5244, 383.7504, 590.273],
      fields: {
        IT: 'FreeTextCallout',
        Contents: 'default callout',
        CL: [201.6939, 536.0244, 232.4504, 574.773, 252.2504, 574.773],
      },
    }, { pageIndex: 0, fallbackId: 'callout-imported' });

    expect(markup).toMatchObject({
      id: 'callout-imported',
      kind: 'callout',
      text: 'default callout',
      leader: {
        points: [pdfPoint(201.6939, 536.0244), pdfPoint(232.4504, 574.773), pdfPoint(252.2504, 574.773)],
      },
    });
    if (markup?.kind === 'callout') {
      expect(markup.textBox.x).toBeCloseTo(196.1939);
      expect(markup.textBox.y).toBeCloseTo(530.5244);
      expect(markup.textBox.width).toBeCloseTo(187.5565);
      expect(markup.textBox.height).toBeCloseTo(59.7486);
    }
  });

  it('returns the topmost hit markup', () => {
    const back = { id: 'rect-back', kind: 'rectangle', pageIndex: 0, rect: rect(10, 10, 100, 50) } as const;
    const front = { id: 'rect-front', kind: 'rectangle', pageIndex: 0, rect: rect(10, 10, 100, 50) } as const;

    expect(hitTestMarkups([back, front], pdfPoint(11, 20), { tolerance: 3 })?.markupId).toBe('rect-front');
  });

  it('hit-tests handles independently from annotation body geometry', () => {
    const handles = getMarkupToolDefinition({
      id: 'rect-1',
      kind: 'rectangle',
      pageIndex: 0,
      rect: rect(10, 10, 100, 50),
    } as const)?.geometry?.getGeometry({
      id: 'rect-1',
      kind: 'rectangle',
      pageIndex: 0,
      rect: rect(10, 10, 100, 50),
    } as const, { page: pageStub() }).handles ?? [];

    expect(hitTestHandles(handles, pdfPoint(110, 35), { tolerance: 4 })).toMatchObject({
      id: 'rectangle.resize.e',
      behavior: 'resizeSelf',
    });
    expect(hitTestHandles(handles, pdfPoint(110, 35), { tolerance: 4 })?.cursor).toContain('data:image/svg+xml');
    expect(hitTestHandles(handles, pdfPoint(60, 72), { tolerance: 4 })).toMatchObject({
      id: 'rectangle.rotate',
      behavior: 'rotateSelf',
    });
  });
});

describe('interaction chrome', () => {
  it('derives handles and chrome separately from annotation content style', () => {
    const handles = getResizeHandles(rect(10, 20, 100, 50));
    const selectedStyle = getChromeStyle('selected');
    const hoveredStyle = getChromeStyle('hovered');
    const draftStyle = getChromeStyle('draft');

    expect(handles).toHaveLength(8);
    expect(handles.map((handle) => handle.kind)).toEqual(['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w']);
    expect(handles.find((handle) => handle.kind === 'nw')).toMatchObject({ x: 10, y: 70 });
    expect(handles.find((handle) => handle.kind === 'sw')).toMatchObject({ x: 10, y: 20 });
    expect(handles.find((handle) => handle.kind === 'nw')?.cursor).toContain('rotate(45%2012%2012)');
    expect(handles.find((handle) => handle.kind === 'nw')?.cursor).toContain('scale(0.75)');
    expect(handles.find((handle) => handle.kind === 'sw')?.cursor).toContain('rotate(315%2012%2012)');
    expect(handles.find((handle) => handle.kind === 'sw')?.cursor).toContain('scale(0.75)');
    expect(getRotationHandle(rect(10, 20, 100, 50))).toMatchObject({ x: 60, y: 82 });
    expect(getRotationHandle(rect(10, 20, 100, 50)).cursor).toContain('data:image/svg+xml');
    expect(getRotateCursor(45)).toContain('width%3D%2224%22');
    expect(getRotateCursor(45)).toContain('stroke%3D%22%23fff%22');
    expect(getRotateCursor(45)).toContain('stroke-width%3D%227.333%22');
    expect(getRotateCursor(45)).toContain('stroke%3D%22%23111827%22');
    expect(getRotateCursor(45)).toContain('stroke-width%3D%222%22');
    expect(getRotateCursor(45)).toContain('12 12, none');
    expect(getRotateCursor(45)).toContain('rotate(45%2012%2012)');
    expect(getRotateCursor(45)).toContain('scale(0.75)');
    expect(getResizeCursor('e', 30)).toContain('rotate(30%2012%2012)');
    expect(getResizeCursor('n', 30)).toContain('rotate(120%2012%2012)');
    expect(getMoveCursor(30)).toContain('rotate(30%2012%2012)');
    expect(getMoveCursor()).toContain('rotate(0%2012%2012)');
    expect(getRotateCursor()).toContain('rotate(0%2012%2012)');
    expect(getMoveCursor()).toContain('m5%209-3%203%203%203');
    expect(selectedStyle.handleSize).toBeGreaterThan(0);
    expect(hoveredStyle.handleSize).toBeGreaterThan(0);
    expect(hoveredStyle.boundsStroke).toBe('#93c5fd');
    expect(hoveredStyle.handleFill).toBe('#fef08a');
    expect(hoveredStyle.handleStroke).toBe('#facc15');
    expect(hoveredStyle.strokeDasharray).toBe('4 3');
    expect(hoveredStyle.boundsOutsetPx).toBe(8);
    expect(selectedStyle.boundsOutsetPx).toBe(8);
    expect(selectedStyle.boundsStroke).toBe('#2563eb');
    expect(selectedStyle.strokeDasharray).toBe('5 4');
    expect(draftStyle.strokeDasharray).toBe('6 4');
  });
});

describe('generated line types', () => {
  it('models cloud as a generated line type with PDF border-effect compatibility metadata', () => {
    const rendered = CLOUD_LINE_TYPE_RENDERER.render({
      controlPath: [pdfPoint(0, 0), pdfPoint(20, 0), pdfPoint(20, 10)],
      closed: true,
      strokeWidth: 1,
      options: DEFAULT_CLOUD_LINE_OPTIONS,
    });

    expect(rendered.d).toContain('M ');
    expect(rendered.d).toContain(' C ');
    expect(rendered.d).not.toBe('M 0 0 L 20 0 L 20 10 Z');
    expect(rendered.points.length).toBeGreaterThan(3);
    expect(rendered.points.some((point) => point.y !== 0 && point.x > 0 && point.x < 20)).toBe(true);
    expect(rendered.pdfCompatibility).toEqual({
      borderEffect: {
        style: 'cloud',
        intensity: 2,
      },
    });
  });

  it('fits default cloud scallop bounds to the measured Bluebeam cloud sample', () => {
    const points = generateCloudScallopPoints([
      pdfPoint(45.76695, 589.2418),
      pdfPoint(45.76695, 532.2991),
      pdfPoint(160.1843, 532.2991),
      pdfPoint(160.1843, 589.2418),
    ], true, DEFAULT_CLOUD_LINE_OPTIONS);
    const xs = points.map((point) => point.x);
    const ys = points.map((point) => point.y);

    expect(Math.min(...xs)).toBeCloseTo(36.01125, 2);
    expect(Math.min(...ys)).toBeCloseTo(522.5434, 2);
    expect(Math.max(...xs)).toBeCloseTo(169.9401, 2);
    expect(Math.max(...ys)).toBeCloseTo(598.9975, 2);
  });

  it('fits the first Bluebeam cloud lobe cubic commands', () => {
    const d = CLOUD_LINE_TYPE_RENDERER.render({
      controlPath: [
        pdfPoint(45.76695, 589.2418),
        pdfPoint(45.76695, 532.2991),
        pdfPoint(160.1843, 532.2991),
        pdfPoint(160.1843, 589.2418),
      ],
      closed: true,
      strokeWidth: 1,
      options: DEFAULT_CLOUD_LINE_OPTIONS,
    }).d;
    const numbers = d.match(/-?\d+(?:\.\d+)?/g)?.map(Number) ?? [];

    expect(numbers.slice(0, 20)).toEqual([
      52.907, 593.977,
      55.523, 597.9198, 60.8395, 598.9965, 64.7836, 596.3804,
      66.3801, 595.3222, 68.0066, 592.946, 68.4065, 591.0739,
      68.2165, 591.9393, 67.6653, 593.2388, 67.187, 593.977,
    ]);
  });
});

function pageStub() {
  return {
    id: 'page-1',
    index: 0,
    size: { width: 612, height: 792 },
    rotation: 0,
  } as const;
}

function pageTransformStub() {
  return createPageTransform(pageStub(), 1);
}
