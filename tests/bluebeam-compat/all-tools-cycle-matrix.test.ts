import { mkdtemp, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  createArcMarkup,
  createAreaMarkup,
  createArrowMarkup,
  createCalloutMarkup,
  createCloudMarkup,
  createCloudPlusMarkup,
  createCustomPageScale,
  createDimensionMarkup,
  createEllipseMarkup,
  createHighlightMarkup,
  createImageMarkup,
  createLengthMarkup,
  createLineMarkup,
  createPenMarkup,
  createPolygonMarkup,
  createPolylengthMarkup,
  createPolylineMarkup,
  createRectangleMarkup,
  createSnapshotMarkup,
  createTextBoxMarkup,
  pdfPoint,
  translateMarkup,
  type Markup,
  type PageScale,
} from '@butter-paper/core';
import { createBlankPdf, openPdfDocument } from '@butter-paper/pdf';
import { describe, expect, it } from 'vitest';
import { inspectAnnotations } from '../../scripts/bluebeam-compat/pdf-inspector.mjs';

const IMAGE_DATA_URL = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAEAAAAAoCAYAAABOzvzpAAAABHNCSVQICAgIfAhkiAAAAAFzUkdCAK7OHOkAAAIpSURBVGiB7Zi/TxNhHMafO0vvWiBBlIC2ooi1xkT5MZFAHEwMJCwMpotuDqZx4R9QEyWyOJkuYhxcTAcHEkYSEjc3fqXByo9AyJUCRh3s3XE1d06uvO8dtd8v6T3z8z7vk8/dffO+p6QeHXpoYKnUBagVAqAuQK0QAHUBaoUAqAtQKwRAXYBaSumXF54EG1khAOoC1IpQF5CR67n48nsFBWsT36wdFO1tKFCQjl3Gdf0KbsauYqjlNlTF//NkPwRLziFeGm+xaq4f67sVT+Fp4jEuRjt85QsBxO7f8xUokvVpXto79/Mz3pQ/wnJtKX9M1THZ9QDjZ+9I78H2E5j9sYDXex98rbFcG9Ol96h6fzDRfldqDcshaDgHyO3nA6/P7edhOAdSXnYAXHiYMmZgu0eBM2z3CFPGDFyIxxs7AGvWlnDgyWjVXMeatSX0sQNQMDfqmsUOwGLla82yCtam0MMOgExp6SzzFAKopRyvKvSwA5DSu+uaxQ7AtRoCkMliB6Dh34CR1gH06skT5/RoCYy0Dgp97ADoqoZniSyalODXlCYlgheXnkBXo0IvOwAA0Ksnke3MBF6f7cygR0tIedneBjPnRqGpUeTKeenrcMuZOCa7HmKsbVh6H/Y/RPaq3/HKeCc8IQ4038DzZBbnI22+8tkD+Kddp4ylShFLZhHLlSI8eOhvTqMvnkZ/PI1u7UKg3FMD4H+J5RCsp0IA1AWoFQKgLkCtv9cipgMsRDYAAAAAAElFTkSuQmCC';

const pageScale: PageScale = createCustomPageScale({
  pageIndex: 0,
  name: '1:100',
  pdfUnits: 'cm',
  realUnits: 'm',
  scaleX: 0.01,
  scaleY: 0.01,
  precision: { mode: 'decimal', value: 0.01 },
});

type BuiltInKind = Markup['kind'];

interface NativeComponentContract {
  readonly suffix?: ':cloud' | ':text';
  readonly subtype: string;
  readonly intent?: string;
  readonly measure?: boolean;
  readonly blendMode?: string;
}

interface ToolContract {
  readonly kind: BuiltInKind;
  readonly components: readonly NativeComponentContract[];
}

const toolContracts: readonly ToolContract[] = [
  { kind: 'rectangle', components: [{ subtype: 'Square' }] },
  { kind: 'ellipse', components: [{ subtype: 'Circle' }] },
  { kind: 'arc', components: [{ subtype: 'Circle', intent: 'CircleArc' }] },
  { kind: 'line', components: [{ subtype: 'Line' }] },
  { kind: 'arrow', components: [{ subtype: 'Line', intent: 'LineArrow' }] },
  { kind: 'dimension', components: [{ subtype: 'Line', intent: 'LineDimension' }] },
  { kind: 'length', components: [{ subtype: 'Line', intent: 'LineDimension', measure: true }] },
  { kind: 'polylength', components: [{ subtype: 'PolyLine', intent: 'PolyLineDimension', measure: true }] },
  { kind: 'area', components: [{ subtype: 'Polygon', intent: 'PolygonDimension', measure: true }] },
  { kind: 'polyline', components: [{ subtype: 'PolyLine' }] },
  { kind: 'polygon', components: [{ subtype: 'Polygon' }] },
  { kind: 'pen', components: [{ subtype: 'Ink' }] },
  { kind: 'highlight', components: [{ subtype: 'Ink', blendMode: 'Multiply' }] },
  { kind: 'cloud', components: [{ subtype: 'Polygon', intent: 'PolygonCloud' }] },
  {
    kind: 'cloud-plus',
    components: [
      { suffix: ':cloud', subtype: 'Polygon', intent: 'PolygonCloud' },
      { suffix: ':text', subtype: 'FreeText', intent: 'FreeTextCallout' },
    ],
  },
  { kind: 'text-box', components: [{ subtype: 'FreeText' }] },
  { kind: 'callout', components: [{ subtype: 'FreeText', intent: 'FreeTextCallout' }] },
  { kind: 'image', components: [{ subtype: 'Square', intent: 'SquareImage' }] },
  { kind: 'snapshot', components: [{ subtype: 'Stamp', intent: 'StampSnapshot' }] },
] as const;

const commonAppearance = {
  stroke: { color: '#123456', widthPt: 3.25 },
  fill: { color: '#abcdef' },
  text: {
    color: '#345678',
    fontId: 'Helvetica',
    fontSizePt: 13,
    lineHeightPt: 17,
    align: 'center' as const,
    insetPt: 4,
  },
  opacity: 0.625,
  blendMode: 'normal' as const,
};

function specimenMarkups(): readonly Markup[] {
  const cloudPath = 'M 330 230 C 330 230 330 300 330 300 L 410 300 C 410 300 410 230 410 230 Z';
  const cloudPlusPath = 'M 430 230 C 430 230 430 300 430 300 L 510 300 C 510 300 510 230 510 230 Z';
  return [
    createRectangleMarkup({ id: 'matrix-rectangle', pageIndex: 0, rect: { x: 30, y: 30, width: 70, height: 45 }, rotation: 15, appearance: commonAppearance }),
    createEllipseMarkup({ id: 'matrix-ellipse', pageIndex: 0, rect: { x: 120, y: 30, width: 70, height: 45 }, rotation: 20, appearance: commonAppearance }),
    createArcMarkup({ id: 'matrix-arc', pageIndex: 0, rect: { x: 210, y: 30, width: 70, height: 45 }, angle1: 25, angle2: 210, appearance: commonAppearance }),
    createLineMarkup({ id: 'matrix-line', pageIndex: 0, start: pdfPoint(300, 40), end: pdfPoint(370, 80), appearance: commonAppearance }),
    createArrowMarkup({ id: 'matrix-arrow', pageIndex: 0, start: pdfPoint(30, 140), end: pdfPoint(100, 180), appearance: commonAppearance }),
    createDimensionMarkup({ id: 'matrix-dimension', pageIndex: 0, start: pdfPoint(120, 150), end: pdfPoint(200, 150), dimensionLineOffset: 28, text: '80 mm', appearance: commonAppearance }),
    createLengthMarkup({ id: 'matrix-length', pageIndex: 0, start: pdfPoint(230, 150), end: pdfPoint(310, 175), displayUnit: 'm', appearance: commonAppearance }),
    createPolylengthMarkup({ id: 'matrix-polylength', pageIndex: 0, points: [pdfPoint(340, 140), pdfPoint(380, 190), pdfPoint(420, 145)], displayUnit: 'cm', appearance: commonAppearance }),
    createAreaMarkup({ id: 'matrix-area', pageIndex: 0, points: [pdfPoint(450, 140), pdfPoint(450, 195), pdfPoint(520, 195), pdfPoint(520, 140)], displayUnit: 'm', appearance: commonAppearance }),
    createPolylineMarkup({ id: 'matrix-polyline', pageIndex: 0, points: [pdfPoint(550, 140), pdfPoint(585, 195), pdfPoint(630, 145)], appearance: commonAppearance }),
    createPolygonMarkup({ id: 'matrix-polygon', pageIndex: 0, points: [pdfPoint(30, 240), pdfPoint(55, 295), pdfPoint(105, 270), pdfPoint(95, 230)], appearance: commonAppearance }),
    createPenMarkup({ id: 'matrix-pen', pageIndex: 0, paths: [[pdfPoint(130, 240), pdfPoint(155, 290), pdfPoint(185, 245)]], strokeWidth: 5.5, appearance: commonAppearance }),
    createHighlightMarkup({
      id: 'matrix-highlight',
      pageIndex: 0,
      paths: [[pdfPoint(220, 260), pdfPoint(300, 260)]],
      strokeWidth: 12,
      appearance: { ...commonAppearance, stroke: { color: '#ffee00', widthPt: 12 }, fill: undefined, blendMode: 'multiply' },
    }),
    createCloudMarkup({ id: 'matrix-cloud', pageIndex: 0, controlPath: [pdfPoint(330, 230), pdfPoint(330, 300), pdfPoint(410, 300), pdfPoint(410, 230)], appearancePath: cloudPath, scallopRadius: 10, borderEffectIntensity: 1.5, appearance: commonAppearance }),
    createCloudPlusMarkup({
      id: 'matrix-cloud-plus',
      pageIndex: 0,
      cloud: { controlPath: [pdfPoint(430, 230), pdfPoint(430, 300), pdfPoint(510, 300), pdfPoint(510, 230)], appearancePath: cloudPlusPath, scallopRadius: 10, borderEffectIntensity: 1.5 },
      leader: { points: [pdfPoint(510, 265), pdfPoint(530, 265), pdfPoint(550, 265)] },
      textBox: { x: 550, y: 240, width: 80, height: 50 },
      text: 'Cloud+\nsecond line',
      appearance: commonAppearance,
    }),
    createTextBoxMarkup({
      id: 'matrix-text-box',
      pageIndex: 0,
      rect: { x: 30, y: 340, width: 120, height: 55 },
      text: 'Text box\nsecond line',
      rotation: 25,
      fontSizePt: 13,
      lineHeightPt: 17,
      textAlign: 'center',
      borderWidth: 3.25,
      appearance: commonAppearance,
    }),
    createCalloutMarkup({
      id: 'matrix-callout',
      pageIndex: 0,
      leader: { points: [pdfPoint(180, 350), pdfPoint(205, 370), pdfPoint(230, 370)] },
      textBox: { x: 230, y: 345, width: 100, height: 50 },
      text: 'Callout\nsecond line',
      appearance: commonAppearance,
    }),
    createImageMarkup({ id: 'matrix-image', pageIndex: 0, rect: { x: 360, y: 340, width: 96, height: 60 }, dataUrl: IMAGE_DATA_URL, mimeType: 'image/png', rotation: 18, appearance: { opacity: 0.625 } }),
    createSnapshotMarkup({ id: 'matrix-snapshot', pageIndex: 0, rect: { x: 500, y: 340, width: 96, height: 60 }, dataUrl: IMAGE_DATA_URL, mimeType: 'image/png', rotation: 22, appearance: { opacity: 0.625 } }),
  ];
}

type InspectedAnnotation = Awaited<ReturnType<typeof inspectAnnotations>>['pages'][number]['annotations'][number];

function expectedNativeName(kind: BuiltInKind, suffix = ''): string {
  return `bp:matrix-${kind}${suffix}`;
}

function annotationsByName(inspection: Awaited<ReturnType<typeof inspectAnnotations>>): ReadonlyMap<string, InspectedAnnotation> {
  return new Map(inspection.pages.flatMap((page) => page.annotations).map((annotation) => [annotation.name, annotation]));
}

function componentSnapshot(annotation: InspectedAnnotation) {
  return {
    canonicalHash: annotation.canonicalHash,
    appearances: annotation.appearances.map((appearance) => ({
      path: appearance.path.replace(/@\d+ \d+ R/g, '@ref'),
      sha256: appearance.sha256,
      dictionaryHash: appearance.dictionaryHash,
    })),
  };
}

function assertNativeContracts(inspection: Awaited<ReturnType<typeof inspectAnnotations>>, prefix = 'matrix-'): void {
  const annotations = annotationsByName(inspection);
  const expectedCount = toolContracts.reduce((sum, tool) => sum + tool.components.length, 0);
  expect(annotations.size).toBe(expectedCount);
  for (const tool of toolContracts) {
    tool.components.forEach((component) => {
      const name = `bp:${prefix}${tool.kind}${component.suffix ?? ''}`;
      expect(annotations.get(name), name).toMatchObject({
        name,
        subtype: component.subtype,
        intent: component.intent ?? null,
        measure: component.measure ?? false,
        blendMode: component.blendMode ?? null,
      });
    });
  }
}

async function importMarkups(path: string): Promise<readonly Markup[]> {
  const handle = await openPdfDocument(path);
  try {
    return await handle.annotations.readPageAnnotations(0);
  } finally {
    await handle.close();
  }
}

async function saveMarkups(sourcePath: string, targetPath: string, markups: readonly Markup[]): Promise<void> {
  const handle = await openPdfDocument(sourcePath);
  try {
    await handle.writer.save(handle, markups, 'saveAs', targetPath, [pageScale]);
  } finally {
    await handle.close();
  }
}

function translatedAndRenamed(markup: Markup): Markup {
  return {
    ...translateMarkup(markup, { x: 7, y: 11 }),
    id: `edited-${markup.kind}`,
  } as Markup;
}

describe('all-tools Bluebeam compatibility cycle matrix', () => {
  it('keeps all 19 logical tools native, stable, editable, and independently deletable', async () => {
    expect(toolContracts.map((tool) => tool.kind)).toHaveLength(19);
    expect(new Set(toolContracts.map((tool) => tool.kind)).size).toBe(19);

    const directory = await mkdtemp(join(tmpdir(), 'butter-all-tools-cycle-'));
    const blankPath = join(directory, 'blank.pdf');
    const initialPath = join(directory, 'initial.pdf');
    const untouchedPath = join(directory, 'untouched-second-save.pdf');
    const editedPath = join(directory, 'edited.pdf');
    const editedSecondPath = join(directory, 'edited-second-save.pdf');
    await writeFile(blankPath, await createBlankPdf({ widthMm: 240, heightMm: 180 }));

    await saveMarkups(blankPath, initialPath, specimenMarkups());
    const initialInspection = await inspectAnnotations(initialPath);
    assertNativeContracts(initialInspection);

    const imported = await importMarkups(initialPath);
    expect(imported.map((markup) => markup.kind).sort()).toEqual(toolContracts.map((tool) => tool.kind).sort());
    expect(imported).toHaveLength(19);

    await saveMarkups(initialPath, untouchedPath, imported);
    const untouchedInspection = await inspectAnnotations(untouchedPath);
    assertNativeContracts(untouchedInspection);
    const initialByName = annotationsByName(initialInspection);
    const untouchedByName = annotationsByName(untouchedInspection);
    for (const [name, initial] of initialByName) {
      expect(componentSnapshot(untouchedByName.get(name)!), `${name} untouched dictionary/AP`).toEqual(componentSnapshot(initial));
    }

    const edited = imported.map(translatedAndRenamed);
    await saveMarkups(initialPath, editedPath, edited);
    const editedInspection = await inspectAnnotations(editedPath);
    assertNativeContracts(editedInspection, 'edited-');
    const editedByName = annotationsByName(editedInspection);
    for (const sourceName of initialByName.keys()) {
      expect(editedByName.has(sourceName), `${sourceName} source component removed`).toBe(false);
    }

    const reopenedEdited = await importMarkups(editedPath);
    expect(reopenedEdited.map((markup) => markup.kind).sort()).toEqual(toolContracts.map((tool) => tool.kind).sort());
    await saveMarkups(editedPath, editedSecondPath, reopenedEdited);
    const editedSecondInspection = await inspectAnnotations(editedSecondPath);
    assertNativeContracts(editedSecondInspection, 'edited-');
    const editedSecondByName = annotationsByName(editedSecondInspection);
    for (const [name, firstEdited] of editedByName) {
      expect(componentSnapshot(editedSecondByName.get(name)!), `${name} edited dictionary/AP`).toEqual(componentSnapshot(firstEdited));
    }

    for (const contract of toolContracts) {
      const deletionPath = join(directory, `deleted-${contract.kind}.pdf`);
      await saveMarkups(initialPath, deletionPath, imported.filter((markup) => markup.kind !== contract.kind));
      const deletionInspection = await inspectAnnotations(deletionPath);
      const remaining = annotationsByName(deletionInspection);
      for (const component of contract.components) {
        expect(remaining.has(expectedNativeName(contract.kind, component.suffix)), `${contract.kind} ${component.suffix ?? 'primary'} deleted`).toBe(false);
      }
      expect(remaining.size).toBe(toolContracts.reduce((sum, tool) => sum + tool.components.length, 0) - contract.components.length);
    }
  }, 60_000);

  it('serializes representative visual/style compatibility fields with standard PDF keys', async () => {
    const directory = await mkdtemp(join(tmpdir(), 'butter-all-tools-style-'));
    const blankPath = join(directory, 'blank.pdf');
    const outputPath = join(directory, 'styled.pdf');
    await writeFile(blankPath, await createBlankPdf({ widthMm: 240, heightMm: 180 }));
    await saveMarkups(blankPath, outputPath, specimenMarkups());
    const annotations = annotationsByName(await inspectAnnotations(outputPath));

    const rectangle = annotations.get(expectedNativeName('rectangle'))!.canonical as Record<string, any>;
    expect(rectangle).toMatchObject({ Rotation: 15, CA: 0.625, ca: 0.625, BS: { W: 3.25 }, IC: expect.any(Array) });

    const arrow = annotations.get(expectedNativeName('arrow'))!.canonical as Record<string, any>;
    expect(arrow.LE).toEqual([{ $name: 'None' }, { $name: 'ClosedArrow' }]);

    for (const kind of ['length', 'polylength', 'area'] as const) {
      const measurement = annotations.get(expectedNativeName(kind))!.canonical as Record<string, any>;
      expect(measurement.Measure).toMatchObject({ Type: { $name: 'Measure' }, Subtype: { $name: 'RL' }, R: { $string: '1:100' } });
    }

    const highlight = annotations.get(expectedNativeName('highlight'))!;
    expect(highlight).toMatchObject({ blendMode: 'Multiply' });
    expect((highlight.canonical as Record<string, any>).BS).toMatchObject({ W: 12 });

    const textBoxAnnotation = annotations.get(expectedNativeName('text-box'))!;
    const textBox = textBoxAnnotation.canonical as Record<string, any>;
    expect(textBox).toMatchObject({ Rotation: 25, Contents: { $string: 'Text box\nsecond line' }, Q: 1 });
    expect(textBoxAnnotation.appearances).not.toHaveLength(0);

    const callout = annotations.get(expectedNativeName('callout'))!.canonical as Record<string, any>;
    expect(callout).toMatchObject({ Contents: { $string: 'Callout\nsecond line' }, CL: expect.any(Array) });
    expect(callout.LE).toEqual([{ $name: 'None' }, { $name: 'OpenArrow' }]);

    const cloudPlusText = annotations.get(expectedNativeName('cloud-plus', ':text'))!.canonical as Record<string, any>;
    expect(cloudPlusText).toMatchObject({ Contents: { $string: 'Cloud+\nsecond line' }, CL: expect.any(Array) });
    expect(cloudPlusText.LE).toEqual([{ $name: 'None' }, { $name: 'None' }]);

    expect(annotations.get(expectedNativeName('cloud'))!.appearances).not.toHaveLength(0);
    expect(annotations.get(expectedNativeName('cloud-plus', ':cloud'))!.appearances).not.toHaveLength(0);

    expect(annotations.get(expectedNativeName('image'))!.canonical).toMatchObject({ Rotation: 18 });
    expect(annotations.get(expectedNativeName('snapshot'))!.canonical).toMatchObject({ Rotation: 22 });
  });
});
