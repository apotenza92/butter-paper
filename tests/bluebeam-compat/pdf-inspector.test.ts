import { mkdtemp, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { PDFDocument, PDFName, PDFString } from 'pdf-lib';
import { describe, expect, it } from 'vitest';
import { canonicalObject, hashCanonical, inspectAnnotations } from '../../scripts/bluebeam-compat/pdf-inspector.mjs';

describe('canonical PDF annotation inspection', () => {
  it('sorts dictionary keys and produces stable hashes', async () => {
    const pdf = await PDFDocument.create();
    const first = pdf.context.obj({ Z: 2, A: 1 });
    const second = pdf.context.obj({ A: 1, Z: 2 });
    const canonical = canonicalObject(first, pdf.context);
    expect(canonical).toEqual({ A: 1, Z: 2 });
    expect(hashCanonical(canonical)).toBe(hashCanonical(canonicalObject(second, pdf.context)));
  });

  it('canonicalizes indirect object numbers while retaining reference relationships', async () => {
    const firstPdf = await PDFDocument.create();
    const firstTarget = firstPdf.context.register(firstPdf.context.obj({ Value: 7 }));
    const first = canonicalObject(firstPdf.context.obj({ Target: firstTarget, Again: firstTarget }), firstPdf.context);
    const secondPdf = await PDFDocument.create();
    secondPdf.context.register(secondPdf.context.obj({ Padding: true }));
    const secondTarget = secondPdf.context.register(secondPdf.context.obj({ Value: 7 }));
    const second = canonicalObject(secondPdf.context.obj({ Target: secondTarget, Again: secondTarget }), secondPdf.context);
    expect(first).toEqual(second);
    expect(hashCanonical(first)).toBe(hashCanonical(second));
  });

  it('inspects native subtype, intent dictionary, and appearance streams', async () => {
    const directory = await mkdtemp(join(tmpdir(), 'bluebeam-pdf-inspector-'));
    const file = join(directory, 'annotation.pdf');
    const pdf = await PDFDocument.create();
    const page = pdf.addPage([200, 200]);
    const appearance = pdf.context.flateStream('q 1 0 0 RG 0 0 10 10 re S Q', { Type: 'XObject', Subtype: 'Form', BBox: [0, 0, 10, 10] });
    const appearanceRef = pdf.context.register(appearance);
    const target = pdf.context.obj({
      Type: 'Annot', Subtype: 'FreeText', NM: PDFString.of('cloud-text-1'),
      Rect: [100, 10, 180, 50], GroupNesting: [PDFString.of('Cloud+'), PDFName.of('cloud-text-1'), PDFName.of('cloud-1')],
    });
    target.set(PDFName.of('Subtype'), PDFName.of('FreeText'));
    const targetRef = pdf.context.register(target);
    const annotation = pdf.context.obj({
      Type: 'Annot', Subtype: 'Polygon', IT: 'PolygonCloud', NM: PDFString.of('cloud-1'),
      Rect: [10, 10, 100, 100], AP: { N: appearanceRef }, IRT: targetRef,
    });
    annotation.set(PDFName.of('Subtype'), PDFName.of('Polygon'));
    annotation.set(PDFName.of('IT'), PDFName.of('PolygonCloud'));
    annotation.set(PDFName.of('ITEx'), PDFName.of('PolyText'));
    annotation.set(PDFName.of('RT'), PDFName.of('Group'));
    const annotationRef = pdf.context.register(annotation);
    page.node.set(PDFName.of('Annots'), pdf.context.obj([annotationRef, targetRef]));
    await writeFile(file, await pdf.save({ useObjectStreams: false }));

    const inspected = await inspectAnnotations(file);
    const result = inspected.pages[0].annotations[0];
    expect(result).toMatchObject({
      ref: expect.any(String), subtype: 'Polygon', intent: 'PolygonCloud', intentEx: 'PolyText',
      measure: false, name: 'cloud-1', inReplyTo: 'cloud-text-1', replyType: 'Group', groupNesting: [],
    });
    expect(result.canonical).toMatchObject({ IT: { $name: 'PolygonCloud' }, Subtype: { $name: 'Polygon' } });
    expect(result.appearances).toHaveLength(1);
    expect(result.appearances[0]).toMatchObject({ path: expect.stringContaining('/N'), sha256: expect.stringMatching(/^[a-f0-9]{64}$/), storedSha256: expect.stringMatching(/^[a-f0-9]{64}$/) });
    expect(inspected.pages[0].annotations[1]).toMatchObject({
      name: 'cloud-text-1', groupNesting: ['Cloud+', 'cloud-text-1', 'cloud-1'],
    });
  });
});
