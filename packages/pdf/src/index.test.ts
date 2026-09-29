// @vitest-environment node

import { createHash } from "node:crypto";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createCanvas, loadImage } from "@napi-rs/canvas";
import { describe, expect, it } from "vitest";
import {
  beginMarkedContent,
  decodePDFRawStream,
  endMarkedContent,
  PDFArray,
  PDFBool,
  PDFDict,
  PDFDocument,
  PDFName,
  PDFNumber,
  PDFRawStream,
  PDFRef,
  PDFStream,
  PDFString,
  StandardFonts,
  rgb,
  type PDFObject,
} from "pdf-lib";
import {
  extractPdfPageGeometryIndex,
  inspectPdfDocumentBytes,
  openPdfDocument,
  PdfAnnotationWriter,
  PdfRenderCache,
} from "./index.js";
import type { PdfDocumentHandle } from "./index.js";
import {
  calibratePageScale,
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
  createRedactMarkup,
  createSnapshotMarkup,
  createTextBoxMarkup,
  pdfPoint,
} from "@butter-paper/core";

const testImageDataUrl =
  "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAEAAAAAoCAYAAABOzvzpAAAABHNCSVQICAgIfAhkiAAAAAFzUkdCAK7OHOkAAAIpSURBVGiB7Zi/TxNhHMafO0vvWiBBlIC2ooi1xkT5MZFAHEwMJCwMpotuDqZx4R9QEyWyOJkuYhxcTAcHEkYSEjc3fqXByo9AyJUCRh3s3XE1d06uvO8dtd8v6T3z8z7vk8/dffO+p6QeHXpoYKnUBagVAqAuQK0QAHUBaoUAqAtQKwRAXYBaSumXF54EG1khAOoC1IpQF5CR67n48nsFBWsT36wdFO1tKFCQjl3Gdf0KbsauYqjlNlTF//NkPwRLziFeGm+xaq4f67sVT+Fp4jEuRjt85QsBxO7f8xUokvVpXto79/Mz3pQ/wnJtKX9M1THZ9QDjZ+9I78H2E5j9sYDXex98rbFcG9Ol96h6fzDRfldqDcshaDgHyO3nA6/P7edhOAdSXnYAXHiYMmZgu0eBM2z3CFPGDFyIxxs7AGvWlnDgyWjVXMeatSX0sQNQMDfqmsUOwGLla82yCtam0MMOgExp6SzzFAKopRyvKvSwA5DSu+uaxQ7AtRoCkMliB6Dh34CR1gH06skT5/RoCYy0Dgp97ADoqoZniSyalODXlCYlgheXnkBXo0IvOwAA0Ksnke3MBF6f7cygR0tIedneBjPnRqGpUeTKeenrcMuZOCa7HmKsbVh6H/Y/RPaq3/HKeCc8IQ4038DzZBbnI22+8tkD+Kddp4ylShFLZhHLlSI8eOhvTqMvnkZ/PI1u7UKg3FMD4H+J5RCsp0IA1AWoFQKgLkCtv9cipgMsRDYAAAAAAElFTkSuQmCC";

const gpuiRichTextMatrix = [
  "Helvetica",
  "Arimo",
  "Roboto Mono",
  "Tinos",
].flatMap((fontId, familyIndex, families) => [
  { text: `${fontId} regular | `, fontId, color: "#2255aa", fontSizePt: 11 },
  { text: "bold | ", fontId, bold: true, color: "#aa1122", fontSizePt: 11 },
  { text: "italic | ", fontId, italic: true, color: "#2255aa", fontSizePt: 13 },
  {
    text: `bold italic${familyIndex < families.length - 1 ? "\n" : ""}`,
    fontId,
    bold: true,
    italic: true,
    color: "#aa1122",
    fontSizePt: 13,
  },
]);

async function createFixturePdf(): Promise<string> {
  const pdfDoc = await PDFDocument.create();
  const page = pdfDoc.addPage([240, 180]);
  const font = await pdfDoc.embedFont(StandardFonts.Helvetica);
  page.drawRectangle({
    x: 20,
    y: 20,
    width: 80,
    height: 40,
    borderColor: rgb(0.95, 0.3, 0.2),
    borderWidth: 2,
  });
  page.drawText("Butter Paper", {
    x: 20,
    y: 140,
    size: 12,
    font,
    color: rgb(0, 0, 0),
  });

  const dir = await mkdtemp(join(tmpdir(), "butter-paper-pdf-"));
  const file = join(dir, "fixture.pdf");
  const bytes = await pdfDoc.save();
  await writeFile(file, bytes);
  return file;
}

async function createCoordinateSpaceFixturePdf(): Promise<string> {
  const pdfDoc = await PDFDocument.create();
  const page = pdfDoc.addPage([600, 800]);
  const font = await pdfDoc.embedFont(StandardFonts.Helvetica);
  page.drawRectangle({
    x: 52,
    y: 102,
    width: 396,
    height: 596,
    borderColor: rgb(0.15, 0.15, 0.15),
    borderWidth: 1,
  });
  page.drawText("Inherited coordinate-space compatibility", {
    x: 72,
    y: 660,
    size: 12,
    font,
    color: rgb(0, 0, 0),
  });

  const vendor = pdfDoc.context.obj({
    Type: PDFName.of("Annot"),
    Subtype: PDFName.of("Stamp"),
    Rect: [350, 150, 390, 190],
    NM: PDFString.of("vendor-coordinate-probe"),
    Subj: PDFString.of("Independent vendor annotation"),
    Contents: PDFString.of("Preserve this opaque annotation"),
    F: PDFNumber.of(4),
    VendorProbe: PDFString.of("coordinate-space-sentinel"),
  });
  page.node.set(
    PDFName.of("Annots"),
    pdfDoc.context.obj([pdfDoc.context.register(vendor)]),
  );

  const parentRef = page.node.get(PDFName.of("Parent"));
  const parent = parentRef
    ? pdfDoc.context.lookup(parentRef, PDFDict)
    : undefined;
  if (!parent) throw new Error("Fixture page tree parent is missing.");
  page.node.delete(PDFName.of("MediaBox"));
  page.node.delete(PDFName.of("CropBox"));
  page.node.delete(PDFName.of("Rotate"));
  parent.set(PDFName.of("MediaBox"), pdfDoc.context.obj([10, 20, 610, 820]));
  parent.set(PDFName.of("CropBox"), pdfDoc.context.obj([50, 100, 450, 700]));
  parent.set(PDFName.of("Rotate"), PDFNumber.of(90));
  page.node.set(PDFName.of("UserUnit"), PDFNumber.of(2));

  const dir = await mkdtemp(join(tmpdir(), "butter-paper-coordinate-space-"));
  const file = join(dir, "coordinate-space.pdf");
  await writeFile(file, await pdfDoc.save());
  return file;
}

async function createGpuiRichTextFixturePdf(): Promise<string> {
  const pdfDoc = await PDFDocument.create();
  const page = pdfDoc.addPage([600, 300]);
  const pageFont = await pdfDoc.embedFont(StandardFonts.Helvetica);
  page.drawText("Butter Paper rich text compatibility page", {
    x: 24,
    y: 260,
    size: 12,
    font: pageFont,
    color: rgb(0, 0, 0),
  });
  const richContent = gpuiRichTextMatrix
    .map((run) => {
      const styles = [
        `font-family:${run.fontId}`,
        `font-size:${run.fontSizePt.toFixed(6)}pt`,
        `color:${run.color.toUpperCase()}`,
        ...(run.bold ? ["font-weight:bold"] : []),
        ...(run.italic ? ["font-style:italic"] : []),
      ];
      return `<span style="${styles.join(";")}">${run.text.replace("\n", "<br/>")}</span>`;
    })
    .join("");
  const annotation = pdfDoc.context.obj({
    Type: PDFName.of("Annot"),
    Subtype: PDFName.of("FreeText"),
    Rect: [24, 80, 576, 180],
    Contents: PDFString.of(gpuiRichTextMatrix.map((run) => run.text).join("")),
    Q: PDFNumber.of(0),
    DA: PDFString.of("/BPArimo 12 Tf 0.090196 0.168627 0.301961 rg"),
    DS: PDFString.of("font-family: Arimo; font-size: 12pt; color: #172B4D"),
    RC: PDFString.of(
      `<?xml version="1.0"?><body xmlns="http://www.w3.org/1999/xhtml"><p>${richContent}</p></body>`,
    ),
    BS: pdfDoc.context.obj({
      W: PDFNumber.of(1),
      S: PDFName.of("S"),
      Type: PDFName.of("Border"),
    }),
    NM: PDFString.of("bp:native-rich-text"),
    Subj: PDFString.of("Text Box"),
    F: PDFNumber.of(4),
    C: [0.09, 0.17, 0.3],
  });
  page.node.set(
    PDFName.of("Annots"),
    pdfDoc.context.obj([pdfDoc.context.register(annotation)]),
  );

  const dir = await mkdtemp(join(tmpdir(), "butter-paper-gpui-rich-text-"));
  const file = join(dir, "gpui-rich-text.pdf");
  await writeFile(file, await pdfDoc.save());
  return file;
}

async function createBluebeamNativeFixturePdf(): Promise<string> {
  const pdfDoc = await PDFDocument.create();
  const page = pdfDoc.addPage([300, 220]);
  const rectangle = pdfDoc.context.obj({
    Type: PDFName.of("Annot"),
    Subtype: PDFName.of("Square"),
    Rect: [20, 20, 100, 70],
    C: [1, 0, 0],
    NM: PDFString.of("BB-RECT"),
    T: PDFString.of("A. Reviewer"),
    Subj: PDFString.of("Structural review"),
    CreationDate: PDFString.of("D:20260803101500+10'00'"),
    M: PDFString.of("D:20260803103000+10'00'"),
    Contents: PDFString.of("Keep this independent comment"),
    F: 4,
    StateModel: PDFString.of("Review"),
    State: PDFString.of("Accepted"),
    Rotation: PDFNumber.of(15),
    BPProbe: PDFString.of("preserve-me"),
  });
  const cloud = pdfDoc.context.obj({
    Type: PDFName.of("Annot"),
    Subtype: PDFName.of("Polygon"),
    Rect: [120, 20, 225, 95],
    Vertices: [130, 30, 130, 80, 210, 80, 210, 30],
    C: [1, 0, 0],
    BE: pdfDoc.context.obj({ S: PDFName.of("C"), I: 2 }),
    IT: PDFName.of("PolygonCloud"),
    ITEx: PDFName.of("PolyText"),
    NM: PDFString.of("BB-CLOUD"),
    Subj: PDFString.of("Custom cloud subject"),
  });
  const text = pdfDoc.context.obj({
    Type: PDFName.of("Annot"),
    Subtype: PDFName.of("FreeText"),
    IT: PDFName.of("FreeTextCallout"),
    // Revu removes ITEx from this half after independently moving Cloud+ text.
    Rect: [204.5, 29.5, 295.5, 80.5],
    RD: [20.5, 5.5, 5.5, 5.5],
    Contents: PDFString.of("Native Cloud+"),
    CL: [210, 55, 220, 55, 225, 55],
    LE: [PDFName.of("None"), PDFName.of("None")],
    NM: PDFString.of("BB-TEXT"),
    Subj: PDFString.of("Custom cloud subject"),
    GroupNesting: [
      PDFString.of("Cloud+"),
      PDFName.of("BB-TEXT"),
      PDFName.of("BB-CLOUD"),
    ],
  });
  const rectangleRef = pdfDoc.context.register(rectangle);
  const cloudRef = pdfDoc.context.register(cloud);
  const textRef = pdfDoc.context.register(text);
  cloud.set(PDFName.of("IRT"), textRef);
  cloud.set(PDFName.of("RT"), PDFName.of("Group"));
  const reply = pdfDoc.context.obj({
    Type: PDFName.of("Annot"),
    Subtype: PDFName.of("Text"),
    Rect: [20, 20, 40, 40],
    NM: PDFString.of("BB-REPLY"),
    T: PDFString.of("B. Reviewer"),
    Contents: PDFString.of("Reply stays attached"),
    IRT: rectangleRef,
    RT: PDFName.of("Reply"),
    F: 4,
  });
  page.node.set(
    PDFName.of("Annots"),
    pdfDoc.context.obj([
      rectangleRef,
      cloudRef,
      textRef,
      pdfDoc.context.register(reply),
    ]),
  );

  const dir = await mkdtemp(join(tmpdir(), "butter-paper-bluebeam-native-"));
  const file = join(dir, "bluebeam-native.pdf");
  await writeFile(file, await pdfDoc.save());
  return file;
}

async function createOptionalContentAnnotationFixturePdf(
  optionalContentCloudPart: "cloud" | "text",
): Promise<string> {
  const pdfDoc = await PDFDocument.create();
  const page = pdfDoc.addPage([300, 220]);
  const layer = pdfDoc.context.obj({
    Type: PDFName.of("OCG"),
    Name: PDFString.of("Hidden review layer"),
  });
  const layerRef = pdfDoc.context.register(layer);
  pdfDoc.catalog.set(
    PDFName.of("OCProperties"),
    pdfDoc.context.obj({
      OCGs: [layerRef],
      D: pdfDoc.context.obj({
        OFF: [layerRef],
        Order: [layerRef],
      }),
    }),
  );

  const hiddenRectangle = pdfDoc.context.obj({
    Type: PDFName.of("Annot"),
    Subtype: PDFName.of("Square"),
    Rect: [20, 20, 100, 70],
    C: [1, 0, 0],
    NM: PDFString.of("bp:hidden-rectangle"),
    Subj: PDFString.of("Rectangle"),
    OC: layerRef,
    VendorProbe: PDFString.of("retain-hidden"),
  });
  const visibleRectangle = pdfDoc.context.obj({
    Type: PDFName.of("Annot"),
    Subtype: PDFName.of("Square"),
    Rect: [20, 100, 100, 150],
    C: [0, 0, 1],
    NM: PDFString.of("bp:visible-rectangle"),
    Subj: PDFString.of("Rectangle"),
  });
  const cloud = pdfDoc.context.obj({
    Type: PDFName.of("Annot"),
    Subtype: PDFName.of("Polygon"),
    Rect: [120, 20, 225, 95],
    Vertices: [130, 30, 130, 80, 210, 80, 210, 30],
    IT: PDFName.of("PolygonCloud"),
    ITEx: PDFName.of("PolyText"),
    NM: PDFString.of("OC-CLOUD"),
    Subj: PDFString.of("Cloud+"),
    ...(optionalContentCloudPart === "cloud" ? { OC: layerRef } : {}),
  });
  const text = pdfDoc.context.obj({
    Type: PDFName.of("Annot"),
    Subtype: PDFName.of("FreeText"),
    IT: PDFName.of("FreeTextCallout"),
    Rect: [210, 35, 290, 75],
    Contents: PDFString.of("Layer-controlled pair"),
    CL: [210, 55, 220, 55, 225, 55],
    NM: PDFString.of("OC-TEXT"),
    Subj: PDFString.of("Cloud+"),
    GroupNesting: [
      PDFString.of("Cloud+"),
      PDFName.of("OC-TEXT"),
      PDFName.of("OC-CLOUD"),
    ],
    ...(optionalContentCloudPart === "text" ? { OC: layerRef } : {}),
  });
  const hiddenRef = pdfDoc.context.register(hiddenRectangle);
  const visibleRef = pdfDoc.context.register(visibleRectangle);
  const cloudRef = pdfDoc.context.register(cloud);
  const textRef = pdfDoc.context.register(text);
  cloud.set(PDFName.of("IRT"), textRef);
  cloud.set(PDFName.of("RT"), PDFName.of("Group"));
  page.node.set(
    PDFName.of("Annots"),
    pdfDoc.context.obj([hiddenRef, visibleRef, cloudRef, textRef]),
  );

  const dir = await mkdtemp(join(tmpdir(), "butter-paper-optional-content-"));
  const file = join(dir, "optional-content.pdf");
  await writeFile(file, await pdfDoc.save());
  return file;
}

const optionalContentPageProgram = [
  "q /OC /HiddenLayer BDC 1 0 0 rg 20 180 40 40 re f EMC Q",
  "q /OC /VisibleExpression BDC 0 1 0 rg 80 180 40 40 re f EMC Q",
  "q 1 0 0 1 140 180 cm /HiddenForm Do Q",
  "q 1 0 0 1 200 180 cm /VisibleForm Do Q",
].join("\n");
const optionalContentFormProgram = "q 0 0 1 rg 0 0 40 40 re f Q";

async function createOptionalContentMembershipFixturePdf(): Promise<string> {
  const pdfDoc = await PDFDocument.create();
  const page = pdfDoc.addPage([320, 240]);
  const onLayerRef = pdfDoc.context.register(
    pdfDoc.context.obj({
      Type: PDFName.of("OCG"),
      Name: PDFString.of("Visible review layer"),
    }),
  );
  const offLayerRef = pdfDoc.context.register(
    pdfDoc.context.obj({
      Type: PDFName.of("OCG"),
      Name: PDFString.of("Hidden review layer"),
    }),
  );
  const allOnMembershipRef = pdfDoc.context.register(
    pdfDoc.context.obj({
      Type: PDFName.of("OCMD"),
      OCGs: [onLayerRef, offLayerRef],
      P: PDFName.of("AllOn"),
      VendorPolicyProbe: PDFString.of("retain-all-on"),
    }),
  );
  const anyOnMembershipRef = pdfDoc.context.register(
    pdfDoc.context.obj({
      Type: PDFName.of("OCMD"),
      OCGs: [onLayerRef, offLayerRef],
      P: PDFName.of("AnyOn"),
      VendorPolicyProbe: PDFString.of("retain-any-on"),
    }),
  );
  const expressionMembershipRef = pdfDoc.context.register(
    pdfDoc.context.obj({
      Type: PDFName.of("OCMD"),
      OCGs: [onLayerRef, offLayerRef],
      P: PDFName.of("AllOn"),
      VE: [
        PDFName.of("And"),
        [PDFName.of("Or"), offLayerRef, onLayerRef],
        [PDFName.of("Not"), offLayerRef],
      ],
      VendorPolicyProbe: PDFString.of("retain-ve-precedence"),
    }),
  );
  const hiddenFormRef = pdfDoc.context.register(
    pdfDoc.context.flateStream(optionalContentFormProgram, {
      Type: PDFName.of("XObject"),
      Subtype: PDFName.of("Form"),
      FormType: PDFNumber.of(1),
      BBox: [0, 0, 40, 40],
      Resources: pdfDoc.context.obj({}),
      OC: offLayerRef,
      VendorStreamProbe: PDFString.of("retain-hidden-form"),
    }),
  );
  const visibleFormRef = pdfDoc.context.register(
    pdfDoc.context.flateStream(optionalContentFormProgram, {
      Type: PDFName.of("XObject"),
      Subtype: PDFName.of("Form"),
      FormType: PDFNumber.of(1),
      BBox: [0, 0, 40, 40],
      Resources: pdfDoc.context.obj({}),
      OC: expressionMembershipRef,
      VendorStreamProbe: PDFString.of("retain-visible-form"),
    }),
  );
  const pageContentRef = pdfDoc.context.register(
    pdfDoc.context.flateStream(optionalContentPageProgram),
  );
  const alternateConfigurationRef = pdfDoc.context.register(
    pdfDoc.context.obj({
      Name: PDFString.of("Alternate review state"),
      BaseState: PDFName.of("OFF"),
      ON: [offLayerRef],
      OFF: [onLayerRef],
      RBGroups: [[onLayerRef, offLayerRef]],
      VendorConfigProbe: PDFString.of("retain-alternate-config"),
    }),
  );
  page.node.set(
    PDFName.of("Resources"),
    pdfDoc.context.obj({
      Properties: {
        HiddenLayer: offLayerRef,
        VisibleExpression: expressionMembershipRef,
      },
      XObject: {
        HiddenForm: hiddenFormRef,
        VisibleForm: visibleFormRef,
      },
    }),
  );
  page.node.set(PDFName.of("Contents"), pageContentRef);
  pdfDoc.catalog.set(
    PDFName.of("OCProperties"),
    pdfDoc.context.obj({
      OCGs: [onLayerRef, offLayerRef],
      D: pdfDoc.context.obj({
        BaseState: PDFName.of("ON"),
        ON: [onLayerRef],
        OFF: [offLayerRef],
        Order: [onLayerRef, offLayerRef],
        RBGroups: [[onLayerRef, offLayerRef]],
      }),
      Configs: [alternateConfigurationRef],
    }),
  );

  const setLayerStateActionRef = pdfDoc.context.register(
    pdfDoc.context.obj({
      S: PDFName.of("SetOCGState"),
      State: [
        PDFName.of("Toggle"),
        onLayerRef,
        PDFName.of("OFF"),
        offLayerRef,
      ],
      PreserveRB: false,
      VendorActionProbe: PDFString.of("retain-set-ocg-state"),
    }),
  );
  const layerStateLinkRef = pdfDoc.context.register(
    pdfDoc.context.obj({
      Type: PDFName.of("Annot"),
      Subtype: PDFName.of("Link"),
      Rect: [240, 20, 300, 70],
      Border: [0, 0, 0],
      NM: PDFString.of("bp:set-ocg-state-link"),
      A: setLayerStateActionRef,
    }),
  );

  const layeredAllOnRef = pdfDoc.context.register(
    pdfDoc.context.obj({
      Type: PDFName.of("Annot"),
      Subtype: PDFName.of("Square"),
      Rect: [20, 20, 100, 70],
      C: [1, 0, 0],
      NM: PDFString.of("bp:ocmd-all-on"),
      Subj: PDFString.of("Rectangle"),
      OC: allOnMembershipRef,
    }),
  );
  const layeredAnyOnRef = pdfDoc.context.register(
    pdfDoc.context.obj({
      Type: PDFName.of("Annot"),
      Subtype: PDFName.of("Square"),
      Rect: [120, 20, 200, 70],
      C: [0, 1, 0],
      NM: PDFString.of("bp:ocmd-any-on"),
      Subj: PDFString.of("Rectangle"),
      OC: anyOnMembershipRef,
    }),
  );
  const visibleRef = pdfDoc.context.register(
    pdfDoc.context.obj({
      Type: PDFName.of("Annot"),
      Subtype: PDFName.of("Square"),
      Rect: [20, 120, 100, 170],
      C: [0, 0, 1],
      NM: PDFString.of("bp:visible-ocmd-control"),
      Subj: PDFString.of("Rectangle"),
    }),
  );
  const layeredExpressionRef = pdfDoc.context.register(
    pdfDoc.context.obj({
      Type: PDFName.of("Annot"),
      Subtype: PDFName.of("Square"),
      Rect: [120, 120, 200, 170],
      C: [0.75, 0, 0.75],
      NM: PDFString.of("bp:ocmd-visible-expression"),
      Subj: PDFString.of("Rectangle"),
      OC: expressionMembershipRef,
    }),
  );
  page.node.set(
    PDFName.of("Annots"),
    pdfDoc.context.obj([
      layeredAllOnRef,
      layeredAnyOnRef,
      layeredExpressionRef,
      visibleRef,
      layerStateLinkRef,
    ]),
  );

  const dir = await mkdtemp(
    join(tmpdir(), "butter-paper-optional-content-membership-"),
  );
  const file = join(dir, "optional-content-membership.pdf");
  await writeFile(file, await pdfDoc.save());
  return file;
}

async function createPopupLinkedAnnotationFixturePdf(
  popupLinkedCloudPart: "cloud" | "text",
): Promise<string> {
  const pdfDoc = await PDFDocument.create();
  const page = pdfDoc.addPage([320, 240]);
  const parent = pdfDoc.context.obj({
    Type: PDFName.of("Annot"),
    Subtype: PDFName.of("Square"),
    Rect: [20, 20, 100, 70],
    C: [1, 0, 0],
    NM: PDFString.of("bp:popup-parent"),
    VendorProbe: PDFString.of("retain-parent"),
  });
  const visible = pdfDoc.context.obj({
    Type: PDFName.of("Annot"),
    Subtype: PDFName.of("Square"),
    Rect: [20, 120, 100, 170],
    C: [0, 0, 1],
    NM: PDFString.of("bp:visible-popup-control"),
  });
  const cloud = pdfDoc.context.obj({
    Type: PDFName.of("Annot"),
    Subtype: PDFName.of("Polygon"),
    Rect: [120, 20, 225, 95],
    Vertices: [130, 30, 130, 80, 210, 80, 210, 30],
    IT: PDFName.of("PolygonCloud"),
    ITEx: PDFName.of("PolyText"),
    NM: PDFString.of("POPUP-CLOUD"),
    Subj: PDFString.of("Cloud+"),
  });
  const text = pdfDoc.context.obj({
    Type: PDFName.of("Annot"),
    Subtype: PDFName.of("FreeText"),
    IT: PDFName.of("FreeTextCallout"),
    Rect: [220, 35, 310, 75],
    Contents: PDFString.of("Popup-linked pair"),
    CL: [210, 55, 220, 55, 225, 55],
    NM: PDFString.of("POPUP-TEXT"),
    Subj: PDFString.of("Cloud+"),
    GroupNesting: [
      PDFString.of("Cloud+"),
      PDFName.of("POPUP-TEXT"),
      PDFName.of("POPUP-CLOUD"),
    ],
  });
  const parentRef = pdfDoc.context.register(parent);
  const parentPopup = pdfDoc.context.obj({
    Type: PDFName.of("Annot"),
    Subtype: PDFName.of("Popup"),
    Rect: [105, 20, 220, 100],
    Parent: parentRef,
    VendorProbe: PDFString.of("retain-parent-popup"),
  });
  const parentPopupRef = pdfDoc.context.register(parentPopup);
  parent.set(PDFName.of("Popup"), parentPopupRef);

  const visibleRef = pdfDoc.context.register(visible);
  const cloudRef = pdfDoc.context.register(cloud);
  const textRef = pdfDoc.context.register(text);
  const popupLinkedPart = popupLinkedCloudPart === "cloud" ? cloud : text;
  const popupLinkedRef = popupLinkedCloudPart === "cloud" ? cloudRef : textRef;
  const pairPopup = pdfDoc.context.obj({
    Type: PDFName.of("Annot"),
    Subtype: PDFName.of("Popup"),
    Rect: [130, 100, 300, 210],
    Parent: popupLinkedRef,
    VendorProbe: PDFString.of("retain-pair-popup"),
  });
  const pairPopupRef = pdfDoc.context.register(pairPopup);
  popupLinkedPart.set(PDFName.of("Popup"), pairPopupRef);
  cloud.set(PDFName.of("IRT"), textRef);
  cloud.set(PDFName.of("RT"), PDFName.of("Group"));
  page.node.set(
    PDFName.of("Annots"),
    pdfDoc.context.obj([
      parentRef,
      parentPopupRef,
      visibleRef,
      cloudRef,
      textRef,
      pairPopupRef,
    ]),
  );

  const dir = await mkdtemp(join(tmpdir(), "butter-paper-popup-linked-"));
  const file = join(dir, "popup-linked.pdf");
  await writeFile(file, await pdfDoc.save());
  return file;
}

async function createGpuiRotatedRectangleFixturePdf(): Promise<string> {
  const pdfDoc = await PDFDocument.create();
  const page = pdfDoc.addPage([640, 480]);
  const nativeRectangle = pdfDoc.context.obj({
    Type: PDFName.of("Annot"),
    Subtype: PDFName.of("Square"),
    Rect: [285.89746, 157.36861, 514.10254, 352.6314],
    BPRect: [300, 200, 500, 310],
    BPRotation: PDFNumber.of(30),
    Rotation: PDFNumber.of(30),
    C: [0.2, 0.4, 0.8],
    BS: pdfDoc.context.obj({ W: 2, S: PDFName.of("S") }),
    NM: PDFString.of("bp:native-rotated-rectangle"),
    Subj: PDFString.of("Rectangle"),
    F: 4,
  });
  const legacyRectangle = pdfDoc.context.obj({
    Type: PDFName.of("Annot"),
    Subtype: PDFName.of("Square"),
    Rect: [40, 50, 140, 110],
    Rotation: PDFNumber.of(15),
    C: [1, 0, 0],
    NM: PDFString.of("legacy-rectangle"),
    F: 4,
  });
  page.node.set(
    PDFName.of("Annots"),
    pdfDoc.context.obj([
      pdfDoc.context.register(nativeRectangle),
      pdfDoc.context.register(legacyRectangle),
    ]),
  );

  const dir = await mkdtemp(
    join(tmpdir(), "butter-paper-gpui-rotated-rectangle-"),
  );
  const file = join(dir, "gpui-rotated-rectangle.pdf");
  await writeFile(file, await pdfDoc.save());
  return file;
}

async function createGpuiRotatedEllipseFixturePdf(): Promise<string> {
  const pdfDoc = await PDFDocument.create();
  const page = pdfDoc.addPage([640, 480]);
  const nativeEllipse = pdfDoc.context.obj({
    Type: PDFName.of("Annot"),
    Subtype: PDFName.of("Circle"),
    Rect: [285.89746, 157.36861, 514.10254, 352.6314],
    BPRect: [300, 200, 500, 310],
    BPRotation: PDFNumber.of(30),
    Rotation: PDFNumber.of(30),
    C: [0.2, 0.4, 0.8],
    IC: [0.8, 0.9, 1],
    CA: PDFNumber.of(0.7),
    BS: pdfDoc.context.obj({ W: 2, S: PDFName.of("D"), D: [8, 4] }),
    NM: PDFString.of("bp:native-rotated-ellipse"),
    Subj: PDFString.of("Ellipse"),
    F: 4,
  });
  const legacyEllipse = pdfDoc.context.obj({
    Type: PDFName.of("Annot"),
    Subtype: PDFName.of("Circle"),
    Rect: [40, 50, 140, 110],
    Rotation: PDFNumber.of(15),
    C: [1, 0, 0],
    NM: PDFString.of("legacy-ellipse"),
    F: 4,
  });
  page.node.set(
    PDFName.of("Annots"),
    pdfDoc.context.obj([
      pdfDoc.context.register(nativeEllipse),
      pdfDoc.context.register(legacyEllipse),
    ]),
  );

  const dir = await mkdtemp(
    join(tmpdir(), "butter-paper-gpui-rotated-ellipse-"),
  );
  const file = join(dir, "gpui-rotated-ellipse.pdf");
  await writeFile(file, await pdfDoc.save());
  return file;
}

async function createGpuiRotatedImageFixturePdf(): Promise<string> {
  const pdfDoc = await PDFDocument.create();
  const page = pdfDoc.addPage([640, 480]);
  const imageBytes = Uint8Array.from(
    Buffer.from(testImageDataUrl.split(",").at(-1) ?? "", "base64"),
  );
  const image = await pdfDoc.embedPng(imageBytes);
  const appearance = pdfDoc.context.flateStream(
    "q 1 0 0 1 0 0 cm /GS0 gs 83.138439 48 -30 51.961524 30 0 cm /Im0 Do Q",
    {
      Type: PDFName.of("XObject"),
      Subtype: PDFName.of("Form"),
      FormType: PDFNumber.of(1),
      BBox: [0, 0, 113.138439, 99.961524],
      Resources: pdfDoc.context.obj({
        XObject: { Im0: image.ref },
        ExtGState: {
          GS0: pdfDoc.context.obj({
            Type: PDFName.of("ExtGState"),
            CA: 0.65,
            ca: 0.65,
          }),
        },
      }),
    },
  );
  const appearanceRef = pdfDoc.context.register(appearance);
  const annotation = pdfDoc.context.obj({
    Type: PDFName.of("Annot"),
    Subtype: PDFName.of("Square"),
    Rect: [291.430781, 180.019238, 404.569219, 279.980762],
    Rotation: PDFNumber.of(30),
    NM: PDFString.of("bp:native-rotated-image"),
    Subj: PDFString.of("Image"),
    IT: PDFName.of("SquareImage"),
    BPImageData: PDFString.of(testImageDataUrl),
    BPImageMimeType: PDFString.of("image/png"),
    BPAspectLocked: PDFBool.True,
    CA: PDFNumber.of(0.65),
    F: 4,
    AP: pdfDoc.context.obj({ N: appearanceRef }),
  });
  const snapshot = pdfDoc.context.obj({
    Type: PDFName.of("Annot"),
    Subtype: PDFName.of("Stamp"),
    Rect: [91.430781, 80.019238, 204.569219, 179.980762],
    Rotation: PDFNumber.of(30),
    NM: PDFString.of("bp:native-rotated-snapshot"),
    Subj: PDFString.of("Snapshot"),
    IT: PDFName.of("StampSnapshot"),
    BPSnapshotData: PDFString.of(testImageDataUrl),
    BPSnapshotMimeType: PDFString.of("image/png"),
    CA: PDFNumber.of(0.65),
    ca: PDFNumber.of(0.65),
    F: 4,
    AP: pdfDoc.context.obj({ N: appearanceRef }),
  });
  page.node.set(
    PDFName.of("Annots"),
    pdfDoc.context.obj([
      pdfDoc.context.register(annotation),
      pdfDoc.context.register(snapshot),
    ]),
  );

  const dir = await mkdtemp(join(tmpdir(), "butter-paper-gpui-rotated-image-"));
  const file = join(dir, "gpui-rotated-image.pdf");
  await writeFile(file, await pdfDoc.save());
  return file;
}

async function createProprietaryFontFixturePdf(): Promise<string> {
  const pdfDoc = await PDFDocument.create();
  const page = pdfDoc.addPage([240, 180]);
  const arial = pdfDoc.context.obj({
    Type: PDFName.of("Font"),
    Subtype: PDFName.of("Type1"),
    BaseFont: PDFName.of("Arial"),
    Encoding: PDFName.of("WinAnsiEncoding"),
  });
  const arialRef = pdfDoc.context.register(arial);
  const appearance = pdfDoc.context.flateStream(
    "q BT /Arial 12 Tf 1 0 0 rg 4 14 Td (Imported Arial) Tj ET Q",
    {
      Type: PDFName.of("XObject"),
      Subtype: PDFName.of("Form"),
      FormType: PDFNumber.of(1),
      BBox: [0, 0, 160, 30],
      Resources: pdfDoc.context.obj({ Font: { Arial: arialRef } }),
    },
  );
  const annotation = pdfDoc.context.obj({
    Type: PDFName.of("Annot"),
    Subtype: PDFName.of("FreeText"),
    Rect: [20, 80, 180, 110],
    Contents: PDFString.of("Imported Arial"),
    NM: PDFString.of("PROPRIETARY-ARIAL"),
    DA: PDFString.of("/Arial 12 Tf 1 0 0 rg"),
    DS: PDFString.of(
      "font: Arial 12pt; text-align:left; margin:4pt; line-height:14pt; color:#FF0000",
    ),
    DR: pdfDoc.context.obj({ Font: { Arial: arialRef } }),
    AP: pdfDoc.context.obj({ N: pdfDoc.context.register(appearance) }),
    C: [1, 0, 0],
    Border: [0, 0, 1],
  });
  page.node.set(
    PDFName.of("Annots"),
    pdfDoc.context.obj([pdfDoc.context.register(annotation)]),
  );

  const dir = await mkdtemp(join(tmpdir(), "butter-paper-proprietary-font-"));
  const file = join(dir, "proprietary-font.pdf");
  await writeFile(file, await pdfDoc.save());
  return file;
}

async function createRevuNativeMediaFixturePdf(): Promise<string> {
  const pdfDoc = await PDFDocument.create();
  const page = pdfDoc.addPage([320, 220]);
  const canvas = createCanvas(13, 9);
  const context = canvas.getContext("2d");
  context.fillStyle = "#1234c8";
  context.fillRect(0, 0, 13, 9);
  context.fillStyle = "#f2b51d";
  context.fillRect(1, 1, 5, 4);
  context.fillStyle = "#28a861";
  context.fillRect(8, 3, 4, 5);
  const pngBytes = canvas.toBuffer("image/png");
  const jpegBytes = canvas.toBuffer("image/jpeg", 92);
  const pngImage = await pdfDoc.embedPng(pngBytes);
  const jpegImage = await pdfDoc.embedJpg(jpegBytes);

  const createAppearance = (imageRef: PDFRef, nested: boolean) => {
    const imageForm = pdfDoc.context.flateStream(
      "q 13 0 0 9 0 0 cm /Payload Do Q",
      {
        Type: PDFName.of("XObject"),
        Subtype: PDFName.of("Form"),
        FormType: PDFNumber.of(1),
        BBox: [0, 0, 13, 9],
        Resources: pdfDoc.context.obj({ XObject: { Payload: imageRef } }),
      },
    );
    const imageFormRef = pdfDoc.context.register(imageForm);
    if (!nested) {
      return imageFormRef;
    }
    const wrapper = pdfDoc.context.flateStream("q /MediaLayer Do Q", {
      Type: PDFName.of("XObject"),
      Subtype: PDFName.of("Form"),
      FormType: PDFNumber.of(1),
      BBox: [0, 0, 13, 9],
      Resources: pdfDoc.context.obj({ XObject: { MediaLayer: imageFormRef } }),
    });
    return pdfDoc.context.register(wrapper);
  };

  const imageAppearance = createAppearance(pngImage.ref, false);
  const snapshotAppearance = createAppearance(jpegImage.ref, true);
  const image = pdfDoc.context.obj({
    Type: PDFName.of("Annot"),
    Subtype: PDFName.of("Square"),
    Rect: [30, 110, 160, 200],
    IT: PDFName.of("SquareImage"),
    Subj: PDFString.of("Imported site photograph"),
    NM: PDFString.of("REVU-IMAGE-1"),
    F: PDFNumber.of(4),
    AP: pdfDoc.context.obj({ N: imageAppearance }),
  });
  const snapshot = pdfDoc.context.obj({
    Type: PDFName.of("Annot"),
    Subtype: PDFName.of("Stamp"),
    Rect: [175, 110, 305, 200],
    IT: PDFName.of("StampSnapshot"),
    Subj: PDFString.of("Imported plan snapshot"),
    NM: PDFString.of("REVU-SNAPSHOT-1"),
    F: PDFNumber.of(4),
    AS: PDFName.of("Visible"),
    AP: pdfDoc.context.obj({
      N: pdfDoc.context.obj({ Visible: snapshotAppearance }),
    }),
  });
  page.node.set(
    PDFName.of("Annots"),
    pdfDoc.context.obj([
      pdfDoc.context.register(image),
      pdfDoc.context.register(snapshot),
    ]),
  );

  const dir = await mkdtemp(join(tmpdir(), "butter-paper-revu-media-"));
  const file = join(dir, "revu-native-media.pdf");
  await writeFile(file, await pdfDoc.save());
  return file;
}

async function readNativeMediaContracts(file: string) {
  const pdfDoc = await PDFDocument.load(await readFile(file));
  const annots = pdfDoc.context.lookup(pdfDoc.getPage(0).node.Annots());
  expect(annots).toBeInstanceOf(PDFArray);
  return (annots as PDFArray)
    .asArray()
    .map((ref) => pdfDoc.context.lookup(ref))
    .filter(
      (annotation): annotation is PDFDict => annotation instanceof PDFDict,
    )
    .filter((annotation) =>
      ["SquareImage", "StampSnapshot"].includes(
        String(annotation.get(PDFName.of("IT"))).replace(/^\//, ""),
      ),
    )
    .map((annotation) => {
      const normal = readTestNormalAppearance(pdfDoc, annotation);
      const streams = normal
        ? collectTestAppearanceStreams(pdfDoc, normal)
        : [];
      const appearanceHash = createHash("sha256");
      for (const stream of streams) {
        appearanceHash.update(String(stream.dict.get(PDFName.of("Subtype"))));
        appearanceHash.update(stream.contents);
      }
      const payloadHashes = streams
        .filter(
          (stream) =>
            String(stream.dict.get(PDFName.of("Subtype"))) === "/Image",
        )
        .map((stream) =>
          createHash("sha256").update(stream.contents).digest("hex"),
        )
        .sort();
      return {
        name: readPdfText(annotation.get(PDFName.of("NM"))),
        subtype: String(annotation.get(PDFName.of("Subtype"))),
        intent: String(annotation.get(PDFName.of("IT"))),
        rect: readPdfNumberArray(annotation.get(PDFName.of("Rect"))),
        appearanceState: String(annotation.get(PDFName.of("AS")) ?? ""),
        appearanceHash: appearanceHash.digest("hex"),
        payloadHashes,
        hasPrivateData:
          annotation.has(PDFName.of("BPImageData")) ||
          annotation.has(PDFName.of("BPSnapshotData")),
      };
    });
}

function readTestNormalAppearance(
  pdfDoc: PDFDocument,
  annotation: PDFDict,
): PDFRawStream | undefined {
  const ap = pdfDoc.context.lookup(annotation.get(PDFName.of("AP")));
  if (!(ap instanceof PDFDict)) return undefined;
  const normal = pdfDoc.context.lookup(ap.get(PDFName.of("N")));
  if (normal instanceof PDFRawStream) return normal;
  if (!(normal instanceof PDFDict)) return undefined;
  const state = pdfDoc.context.lookup(annotation.get(PDFName.of("AS")));
  const selected =
    state instanceof PDFName
      ? pdfDoc.context.lookup(normal.get(state))
      : undefined;
  if (selected instanceof PDFRawStream) return selected;
  for (const key of normal.keys()) {
    const candidate = pdfDoc.context.lookup(normal.get(key));
    if (candidate instanceof PDFRawStream) return candidate;
  }
  return undefined;
}

function collectTestAppearanceStreams(
  pdfDoc: PDFDocument,
  root: PDFRawStream,
): readonly PDFRawStream[] {
  const streams: PDFRawStream[] = [];
  const visited = new Set<PDFRawStream>();
  const visit = (stream: PDFRawStream) => {
    if (visited.has(stream)) return;
    visited.add(stream);
    streams.push(stream);
    const resources = pdfDoc.context.lookup(
      stream.dict.get(PDFName.of("Resources")),
    );
    const xObjects =
      resources instanceof PDFDict
        ? pdfDoc.context.lookup(resources.get(PDFName.of("XObject")))
        : undefined;
    if (!(xObjects instanceof PDFDict)) return;
    for (const key of [...xObjects.keys()].sort((left, right) =>
      String(left).localeCompare(String(right)),
    )) {
      const child = pdfDoc.context.lookup(xObjects.get(key));
      if (child instanceof PDFRawStream) visit(child);
    }
  };
  visit(root);
  return streams;
}

function unreachablePdfObjects(
  document: PDFDocument,
): readonly [PDFRef, PDFObject][] {
  const reachableRefs = new Set<string>();
  const visitedDirectObjects = new Set<PDFObject>();
  const visit = (object: PDFObject | undefined): void => {
    if (!object) return;
    if (object instanceof PDFRef) {
      const identity = object.toString();
      if (reachableRefs.has(identity)) return;
      reachableRefs.add(identity);
      visit(document.context.lookup(object));
      return;
    }
    if (visitedDirectObjects.has(object)) return;
    visitedDirectObjects.add(object);
    if (object instanceof PDFStream) {
      visit(object.dict);
    } else if (object instanceof PDFDict) {
      for (const value of object.values()) visit(value);
    } else if (object instanceof PDFArray) {
      for (const value of object.asArray()) visit(value);
    }
  };
  for (const root of Object.values(document.context.trailerInfo)) visit(root);
  return document.context
    .enumerateIndirectObjects()
    .filter(([ref]) => !reachableRefs.has(ref.toString()));
}

async function readRawCalloutAnnotation(file: string) {
  const pdfDoc = await PDFDocument.load(await readFile(file));
  const page = pdfDoc.getPage(0);
  const annots = pdfDoc.context.lookup(page.node.Annots());
  expect(annots).toBeInstanceOf(PDFArray);
  const annotations = (annots as PDFArray)
    .asArray()
    .map((ref) => pdfDoc.context.lookup(ref))
    .filter(
      (annotation): annotation is PDFDict => annotation instanceof PDFDict,
    );
  const callout = annotations.find(
    (annotation) =>
      String(annotation.get(PDFName.of("IT"))) === "/FreeTextCallout" &&
      readPdfText(annotation.get(PDFName.of("Subj"))) === "Callout",
  );
  expect(callout).toBeTruthy();
  if (!callout) {
    throw new Error("Expected raw FreeTextCallout annotation");
  }

  const borderStyle = pdfDoc.context.lookup(callout.get(PDFName.of("BS")));
  const appearance = readAppearanceGeometry(pdfDoc, callout);
  return {
    subtype: String(callout.get(PDFName.of("Subtype"))),
    intent: String(callout.get(PDFName.of("IT"))),
    subject: readPdfText(callout.get(PDFName.of("Subj"))),
    defaultAppearance: readPdfText(callout.get(PDFName.of("DA"))),
    defaultStyle: readPdfText(callout.get(PDFName.of("DS"))),
    color: readPdfNumberArray(callout.get(PDFName.of("C"))),
    border: readPdfNumberArray(callout.get(PDFName.of("Border"))),
    borderStyleWidth:
      borderStyle instanceof PDFDict
        ? Number(borderStyle.get(PDFName.of("W")))
        : undefined,
    lineEnding: readPdfNameArray(callout.get(PDFName.of("LE"))),
    calloutLine: readPdfNumberArray(callout.get(PDFName.of("CL"))),
    rect: readPdfNumberArray(callout.get(PDFName.of("Rect"))),
    rectangleDifferences: readPdfNumberArray(callout.get(PDFName.of("RD"))),
    richContent: readPdfText(callout.get(PDFName.of("RC"))),
    ...appearance,
  };
}

async function readRawCloudPlusAnnotations(file: string) {
  const pdfDoc = await PDFDocument.load(await readFile(file));
  const page = pdfDoc.getPage(0);
  const annots = pdfDoc.context.lookup(page.node.Annots());
  expect(annots).toBeInstanceOf(PDFArray);
  const annotations = (annots as PDFArray)
    .asArray()
    .map((ref) => pdfDoc.context.lookup(ref))
    .filter(
      (annotation): annotation is PDFDict => annotation instanceof PDFDict,
    )
    .filter(
      (annotation) =>
        readPdfText(annotation.get(PDFName.of("Subj"))) === "Cloud+",
    );
  return annotations.map((annotation) => ({
    subtype: String(annotation.get(PDFName.of("Subtype"))),
    intent: String(annotation.get(PDFName.of("IT"))),
    intentEx: String(annotation.get(PDFName.of("ITEx"))),
    subject: readPdfText(annotation.get(PDFName.of("Subj"))),
    vertices: readPdfNumberArray(annotation.get(PDFName.of("Vertices"))),
    calloutLine: readPdfNumberArray(annotation.get(PDFName.of("CL"))),
    lineEnding: readPdfNameArray(annotation.get(PDFName.of("LE"))),
    borderEffect: annotation.get(PDFName.of("BE")) ? "present" : undefined,
    appearance: annotation.get(PDFName.of("AP")) ? "present" : undefined,
    name: readPdfText(annotation.get(PDFName.of("NM"))),
    groupNesting: readPdfMixedArray(annotation.get(PDFName.of("GroupNesting"))),
    color: readPdfNumberArray(annotation.get(PDFName.of("C"))),
    rect: readPdfNumberArray(annotation.get(PDFName.of("Rect"))),
    rectangleDifferences: readPdfNumberArray(annotation.get(PDFName.of("RD"))),
    richContent: readPdfText(annotation.get(PDFName.of("RC"))),
    ...readAppearanceGeometry(pdfDoc, annotation),
  }));
}

function readAppearanceGeometry(pdfDoc: PDFDocument, annotation: PDFDict) {
  const appearanceDictionary = pdfDoc.context.lookup(
    annotation.get(PDFName.of("AP")),
  );
  const normalAppearance =
    appearanceDictionary instanceof PDFDict
      ? pdfDoc.context.lookup(appearanceDictionary.get(PDFName.of("N")))
      : undefined;
  return normalAppearance instanceof PDFRawStream
    ? {
        appearanceBounds: readPdfNumberArray(
          normalAppearance.dict.get(PDFName.of("BBox")),
        ),
        appearanceMatrix: readPdfNumberArray(
          normalAppearance.dict.get(PDFName.of("Matrix")),
        ),
        appearanceContent: Buffer.from(
          decodePDFRawStream(normalAppearance).decode(),
        ).toString("latin1"),
      }
    : {};
}

async function readRawImageAnnotation(file: string) {
  const pdfDoc = await PDFDocument.load(await readFile(file));
  const page = pdfDoc.getPage(0);
  const annots = pdfDoc.context.lookup(page.node.Annots());
  expect(annots).toBeInstanceOf(PDFArray);
  const annotations = (annots as PDFArray)
    .asArray()
    .map((ref) => pdfDoc.context.lookup(ref))
    .filter(
      (annotation): annotation is PDFDict => annotation instanceof PDFDict,
    );
  const image = annotations.find(
    (annotation) =>
      readPdfText(annotation.get(PDFName.of("Subj"))) === "Image" ||
      String(annotation.get(PDFName.of("IT"))) === "/SquareImage",
  );
  expect(image).toBeTruthy();
  if (!image) {
    throw new Error("Expected raw image annotation");
  }
  return {
    subtype: String(image.get(PDFName.of("Subtype"))),
    intent: String(image.get(PDFName.of("IT"))),
    subject: readPdfText(image.get(PDFName.of("Subj"))),
    rect: readPdfNumberArray(image.get(PDFName.of("Rect"))),
    imageMimeType: readPdfText(image.get(PDFName.of("BPImageMimeType"))),
    aspectRatioLocked:
      image.get(PDFName.of("BPAspectRatioLocked")) instanceof PDFBool
        ? (image.get(PDFName.of("BPAspectRatioLocked")) as PDFBool).asBoolean()
        : undefined,
    hasAppearance: Boolean(image.get(PDFName.of("AP"))),
  };
}

async function readRawArcAnnotation(file: string) {
  const pdfDoc = await PDFDocument.load(await readFile(file));
  const page = pdfDoc.getPage(0);
  const annots = pdfDoc.context.lookup(page.node.Annots());
  expect(annots).toBeInstanceOf(PDFArray);
  const annotations = (annots as PDFArray)
    .asArray()
    .map((ref) => pdfDoc.context.lookup(ref))
    .filter(
      (annotation): annotation is PDFDict => annotation instanceof PDFDict,
    );
  const arc = annotations.find(
    (annotation) => readPdfText(annotation.get(PDFName.of("Subj"))) === "Arc",
  );
  expect(arc).toBeTruthy();
  if (!arc) {
    throw new Error("Expected raw arc annotation");
  }
  return {
    subtype: String(arc.get(PDFName.of("Subtype"))),
    intent: String(arc.get(PDFName.of("IT"))),
    subject: readPdfText(arc.get(PDFName.of("Subj"))),
    rect: readPdfNumberArray(arc.get(PDFName.of("Rect"))),
    angle1: Number(arc.get(PDFName.of("Angle1"))),
    angle2: Number(arc.get(PDFName.of("Angle2"))),
    rd: readPdfNumberArray(arc.get(PDFName.of("RD"))),
    hasAppearance: Boolean(arc.get(PDFName.of("AP"))),
  };
}

async function readRawDimensionAnnotation(file: string) {
  const pdfDoc = await PDFDocument.load(await readFile(file));
  const page = pdfDoc.getPage(0);
  const annots = pdfDoc.context.lookup(page.node.Annots());
  expect(annots).toBeInstanceOf(PDFArray);
  const annotations = (annots as PDFArray)
    .asArray()
    .map((ref) => pdfDoc.context.lookup(ref))
    .filter(
      (annotation): annotation is PDFDict => annotation instanceof PDFDict,
    );
  const dimension = annotations.find(
    (annotation) =>
      readPdfText(annotation.get(PDFName.of("Subj"))) === "Dimension",
  );
  expect(dimension).toBeTruthy();
  if (!dimension) {
    throw new Error("Expected raw Dimension annotation");
  }
  return {
    subtype: String(dimension.get(PDFName.of("Subtype"))),
    intent: String(dimension.get(PDFName.of("IT"))),
    subject: readPdfText(dimension.get(PDFName.of("Subj"))),
    line: readPdfNumberArray(dimension.get(PDFName.of("L"))),
    lineEnding: readPdfNameArray(dimension.get(PDFName.of("LE"))),
    lineLeader: Number(dimension.get(PDFName.of("LL"))),
    lineLeaderExtension: Number(dimension.get(PDFName.of("LLE"))),
    caption: readPdfText(dimension.get(PDFName.of("Cap"))),
    hasAppearance: Boolean(dimension.get(PDFName.of("AP"))),
  };
}

async function readRawMeasurementAnnotations(file: string) {
  const pdfDoc = await PDFDocument.load(await readFile(file));
  const page = pdfDoc.getPage(0);
  const annots = pdfDoc.context.lookup(page.node.Annots());
  expect(annots).toBeInstanceOf(PDFArray);
  const annotations = (annots as PDFArray)
    .asArray()
    .map((ref) => pdfDoc.context.lookup(ref))
    .filter(
      (annotation): annotation is PDFDict => annotation instanceof PDFDict,
    )
    .filter((annotation) =>
      [
        "Length Measurement",
        "Polylength Measurement",
        "Area Measurement",
      ].includes(readPdfText(annotation.get(PDFName.of("Subj"))) ?? ""),
    );
  return annotations.map((annotation) => ({
    subtype: String(annotation.get(PDFName.of("Subtype"))),
    intent: String(annotation.get(PDFName.of("IT"))),
    subject: readPdfText(annotation.get(PDFName.of("Subj"))),
    line: readPdfNumberArray(annotation.get(PDFName.of("L"))),
    vertices: readPdfNumberArray(annotation.get(PDFName.of("Vertices"))),
    contents: readPdfText(annotation.get(PDFName.of("Contents"))),
    caption: readPdfText(annotation.get(PDFName.of("Cap"))),
    hasAppearance: Boolean(annotation.get(PDFName.of("AP"))),
  }));
}

async function readRawSnapshotAnnotation(file: string) {
  const pdfDoc = await PDFDocument.load(await readFile(file));
  const page = pdfDoc.getPage(0);
  const annots = pdfDoc.context.lookup(page.node.Annots());
  expect(annots).toBeInstanceOf(PDFArray);
  const annotations = (annots as PDFArray)
    .asArray()
    .map((ref) => pdfDoc.context.lookup(ref))
    .filter(
      (annotation): annotation is PDFDict => annotation instanceof PDFDict,
    );
  const snapshot = annotations.find(
    (annotation) =>
      readPdfText(annotation.get(PDFName.of("Subj"))) === "Snapshot",
  );
  expect(snapshot).toBeTruthy();
  if (!snapshot) {
    throw new Error("Expected raw snapshot annotation");
  }
  return {
    subtype: String(snapshot.get(PDFName.of("Subtype"))),
    intent: String(snapshot.get(PDFName.of("IT"))),
    subject: readPdfText(snapshot.get(PDFName.of("Subj"))),
    rect: readPdfNumberArray(snapshot.get(PDFName.of("Rect"))),
    snapshotMimeType: readPdfText(
      snapshot.get(PDFName.of("BPSnapshotMimeType")),
    ),
    hasAppearance: Boolean(snapshot.get(PDFName.of("AP"))),
  };
}

async function readRawAnnotationContracts(file: string) {
  const pdfDoc = await PDFDocument.load(await readFile(file));
  const annots = pdfDoc.context.lookup(pdfDoc.getPage(0).node.Annots());
  expect(annots).toBeInstanceOf(PDFArray);
  return (annots as PDFArray)
    .asArray()
    .map((ref) => pdfDoc.context.lookup(ref))
    .filter(
      (annotation): annotation is PDFDict => annotation instanceof PDFDict,
    )
    .map((annotation) => ({
      name: readPdfText(annotation.get(PDFName.of("NM"))),
      subtype: String(annotation.get(PDFName.of("Subtype"))),
      subject: readPdfText(annotation.get(PDFName.of("Subj"))),
      intent: String(annotation.get(PDFName.of("IT")) ?? ""),
      intentEx: String(annotation.get(PDFName.of("ITEx")) ?? ""),
      hasAppearance: Boolean(annotation.get(PDFName.of("AP"))),
    }));
}

async function readRawAnnotationMetadata(file: string) {
  const pdfDoc = await PDFDocument.load(await readFile(file));
  const annots = pdfDoc.context.lookup(pdfDoc.getPage(0).node.Annots());
  expect(annots).toBeInstanceOf(PDFArray);
  return (annots as PDFArray)
    .asArray()
    .map((ref) => pdfDoc.context.lookup(ref))
    .filter(
      (annotation): annotation is PDFDict => annotation instanceof PDFDict,
    )
    .map((annotation) => {
      const inReplyTo = pdfDoc.context.lookup(
        annotation.get(PDFName.of("IRT")),
      );
      return {
        name: readPdfText(annotation.get(PDFName.of("NM"))),
        author: readPdfText(annotation.get(PDFName.of("T"))),
        subject: readPdfText(annotation.get(PDFName.of("Subj"))),
        creationDate: readPdfText(annotation.get(PDFName.of("CreationDate"))),
        modificationDate: readPdfText(annotation.get(PDFName.of("M"))),
        contents: readPdfText(annotation.get(PDFName.of("Contents"))),
        flags: Number(String(annotation.get(PDFName.of("F")) ?? "NaN")),
        stateModel: readPdfText(annotation.get(PDFName.of("StateModel"))),
        state: readPdfText(annotation.get(PDFName.of("State"))),
        replyType: String(annotation.get(PDFName.of("RT")) ?? ""),
        inReplyTo:
          inReplyTo instanceof PDFDict
            ? readPdfText(inReplyTo.get(PDFName.of("NM")))
            : undefined,
        unsafeProbe: readPdfText(annotation.get(PDFName.of("BPProbe"))),
      };
    });
}

function readPdfText(value: unknown): string | undefined {
  if (!value || typeof value !== "object") {
    return undefined;
  }
  if ("decodeText" in value && typeof value.decodeText === "function") {
    return value.decodeText();
  }
  return String(value);
}

function readPdfNumberArray(value: unknown): number[] {
  if (!(value instanceof PDFArray)) {
    return [];
  }
  return value.asArray().map(Number);
}

function readPdfNameArray(value: unknown): string[] {
  if (!(value instanceof PDFArray)) {
    return [];
  }
  return value.asArray().map(String);
}

function readPdfMixedArray(value: unknown): string[] {
  if (!(value instanceof PDFArray)) {
    return [];
  }
  return value.asArray().map((item) => readPdfText(item) ?? String(item));
}

function imageBytesForTest(dataUrl: string): Uint8Array {
  return Uint8Array.from(
    Buffer.from(dataUrl.split(",").at(-1) ?? "", "base64"),
  );
}

describe("pdf package", () => {
  it("extracts and losslessly reuses Revu-native Image and Snapshot appearances without Butter private keys", async () => {
    const file = await createRevuNativeMediaFixturePdf();
    const sourceContracts = await readNativeMediaContracts(file);
    expect(sourceContracts).toHaveLength(2);
    expect(sourceContracts).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          subtype: "/Square",
          intent: "/SquareImage",
          rect: [30, 110, 160, 200],
          hasPrivateData: false,
        }),
        expect.objectContaining({
          subtype: "/Stamp",
          intent: "/StampSnapshot",
          rect: [175, 110, 305, 200],
          appearanceState: "/Visible",
          hasPrivateData: false,
        }),
      ]),
    );

    const handle = await openPdfDocument(file);
    const imported = await handle.annotations.readPageAnnotations(0);
    const image = imported.find((markup) => markup.kind === "image");
    const snapshot = imported.find((markup) => markup.kind === "snapshot");
    expect(imported.map((markup) => markup.kind)).toEqual([
      "image",
      "snapshot",
    ]);
    if (
      !image ||
      image.kind !== "image" ||
      !snapshot ||
      snapshot.kind !== "snapshot"
    ) {
      throw new Error("Expected native Image and Snapshot markups");
    }
    expect(image.mimeType).toBe("image/png");
    expect(snapshot.mimeType).toBe("image/jpeg");
    expect(image.dataUrl).not.toBe(testImageDataUrl);
    expect(imageBytesForTest(image.dataUrl).slice(0, 8)).toEqual(
      Uint8Array.of(137, 80, 78, 71, 13, 10, 26, 10),
    );
    expect(imageBytesForTest(snapshot.dataUrl).slice(0, 2)).toEqual(
      Uint8Array.of(0xff, 0xd8),
    );
    const decodedImage = await loadImage(image.dataUrl);
    const decodedSnapshot = await loadImage(snapshot.dataUrl);
    expect([decodedImage.width, decodedImage.height]).toEqual([13, 9]);
    expect([decodedSnapshot.width, decodedSnapshot.height]).toEqual([13, 9]);

    const firstOutput = file.replace(/\.pdf$/i, ".first-edit.pdf");
    const firstEdited = [
      { ...image, rect: { x: 45, y: 85, width: 156, height: 108 } },
      { ...snapshot, rect: { x: 168, y: 75, width: 143, height: 99 } },
    ];
    await handle.writer.save(handle, firstEdited, "saveAs", firstOutput);
    const firstContracts = await readNativeMediaContracts(firstOutput);
    expect(firstContracts.map((contract) => contract.appearanceHash)).toEqual(
      sourceContracts.map((contract) => contract.appearanceHash),
    );
    expect(firstContracts.map((contract) => contract.payloadHashes)).toEqual(
      sourceContracts.map((contract) => contract.payloadHashes),
    );
    expect(firstContracts).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          subtype: "/Square",
          intent: "/SquareImage",
          rect: [45, 85, 201, 193],
          hasPrivateData: true,
        }),
        expect.objectContaining({
          subtype: "/Stamp",
          intent: "/StampSnapshot",
          rect: [168, 75, 311, 174],
          appearanceState: "/Visible",
          hasPrivateData: true,
        }),
      ]),
    );

    const firstReopened = await openPdfDocument(firstOutput);
    const firstImported =
      await firstReopened.annotations.readPageAnnotations(0);
    const firstImage = firstImported.find((markup) => markup.kind === "image");
    const firstSnapshot = firstImported.find(
      (markup) => markup.kind === "snapshot",
    );
    if (
      !firstImage ||
      firstImage.kind !== "image" ||
      !firstSnapshot ||
      firstSnapshot.kind !== "snapshot"
    ) {
      throw new Error("Expected native media after first edit");
    }
    expect(firstImage.dataUrl).toBe(image.dataUrl);
    expect(firstSnapshot.dataUrl).toBe(snapshot.dataUrl);

    const secondOutput = file.replace(/\.pdf$/i, ".second-edit.pdf");
    await firstReopened.writer.save(
      firstReopened,
      [
        {
          ...firstImage,
          rect: { ...firstImage.rect, x: firstImage.rect.x + 9 },
        },
        {
          ...firstSnapshot,
          rect: { ...firstSnapshot.rect, y: firstSnapshot.rect.y - 7 },
        },
      ],
      "saveAs",
      secondOutput,
    );
    const secondContracts = await readNativeMediaContracts(secondOutput);
    expect(secondContracts.map((contract) => contract.appearanceHash)).toEqual(
      sourceContracts.map((contract) => contract.appearanceHash),
    );
    expect(secondContracts.map((contract) => contract.payloadHashes)).toEqual(
      sourceContracts.map((contract) => contract.payloadHashes),
    );

    const secondReopened = await openPdfDocument(secondOutput);
    const deletedOutput = file.replace(/\.pdf$/i, ".deleted.pdf");
    await secondReopened.writer.save(
      secondReopened,
      [],
      "saveAs",
      deletedOutput,
    );
    expect(await readNativeMediaContracts(deletedOutput)).toEqual([]);

    await secondReopened.close();
    await firstReopened.close();
    await handle.close();
  });

  it("accepts the native Image aspect-lock key and preserves it through an Electron edit", async () => {
    const source = await createRevuNativeMediaFixturePdf();
    const writeVariant = async (suffix: string, value: boolean | undefined) => {
      const pdfDoc = await PDFDocument.load(await readFile(source));
      const annots = pdfDoc.context.lookup(
        pdfDoc.getPage(0).node.Annots(),
      ) as PDFArray;
      const image = annots
        .asArray()
        .map((ref) => pdfDoc.context.lookup(ref))
        .filter(
          (annotation): annotation is PDFDict => annotation instanceof PDFDict,
        )
        .find(
          (annotation) =>
            String(annotation.get(PDFName.of("IT"))) === "/SquareImage",
        );
      if (!image) throw new Error("Expected native Image fixture");
      image.delete(PDFName.of("BPAspectRatioLocked"));
      if (value === undefined) {
        image.delete(PDFName.of("BPAspectLocked"));
      } else {
        image.set(
          PDFName.of("BPAspectLocked"),
          value ? PDFBool.True : PDFBool.False,
        );
      }
      const path = source.replace(/\.pdf$/i, `.${suffix}.pdf`);
      await writeFile(path, await pdfDoc.save());
      return path;
    };

    const nativeTrue = await writeVariant("native-aspect-true", true);
    const nativeFalse = await writeVariant("native-aspect-false", false);
    const nativeMissing = await writeVariant(
      "native-aspect-missing",
      undefined,
    );
    const trueHandle = await openPdfDocument(nativeTrue);
    const trueImage = (
      await trueHandle.annotations.readPageAnnotations(0)
    ).find((markup) => markup.kind === "image");
    expect(trueImage).toMatchObject({ kind: "image", aspectRatioLocked: true });
    if (!trueImage || trueImage.kind !== "image")
      throw new Error("Expected native Image markup");

    const editedOutput = source.replace(/\.pdf$/i, ".native-aspect-edited.pdf");
    await trueHandle.writer.save(
      trueHandle,
      [{ ...trueImage, rect: { ...trueImage.rect, x: trueImage.rect.x + 12 } }],
      "saveAs",
      editedOutput,
    );
    expect(await readRawImageAnnotation(editedOutput)).toMatchObject({
      aspectRatioLocked: true,
    });
    const reopened = await openPdfDocument(editedOutput);
    expect(
      (await reopened.annotations.readPageAnnotations(0)).find(
        (markup) => markup.kind === "image",
      ),
    ).toMatchObject({
      kind: "image",
      aspectRatioLocked: true,
    });

    for (const path of [nativeFalse, nativeMissing]) {
      const handle = await openPdfDocument(path);
      const image = (await handle.annotations.readPageAnnotations(0)).find(
        (markup) => markup.kind === "image",
      );
      expect(image?.kind).toBe("image");
      expect(
        image?.kind === "image"
          ? (image.aspectRatioLocked ?? false)
          : undefined,
      ).toBe(false);
      await handle.close();
    }
    await reopened.close();
    await trueHandle.close();
  });

  it("opens metadata and page information", async () => {
    const file = await createFixturePdf();
    const handle = await openPdfDocument(file);

    const metadata = await handle.getMetadata();
    expect(metadata.pageCount).toBe(1);

    const pageInfo = await handle.getPageInfo(0);
    expect(pageInfo.width).toBeGreaterThan(0);
    expect(pageInfo.height).toBeGreaterThan(0);

    await handle.close();
  }, 15_000);

  it("inspects metadata and page information without starting the render backend", async () => {
    const file = await createFixturePdf();
    const inspection = await inspectPdfDocumentBytes(
      new Uint8Array(await readFile(file)),
    );

    expect(inspection.metadata.pageCount).toBe(1);
    expect(inspection.pages).toHaveLength(1);
    expect(inspection.pages[0]).toMatchObject({
      index: 0,
      rotation: 0,
      userUnit: 1,
    });
    expect(inspection.pages[0].width).toBeGreaterThan(0);
    expect(inspection.pages[0].height).toBeGreaterThan(0);
    expect(inspection.annotationsByPage).toHaveLength(1);
  });

  it("persists page rotation when saving", async () => {
    const file = await createFixturePdf();
    const handle = await openPdfDocument(file);
    const output = file.replace(/\.pdf$/i, ".rotated.pdf");

    await handle.writer.save(
      handle,
      [],
      "saveAs",
      output,
      [],
      [{ pageIndex: 0, rotation: 90 }],
    );

    const reopened = await openPdfDocument(output);
    const pageInfo = await reopened.getPageInfo(0);
    expect(pageInfo.rotation).toBe(90);
    expect(pageInfo.width).toBeCloseTo(180);
    expect(pageInfo.height).toBeCloseTo(240);

    await handle.close();
    await reopened.close();
  }, 15_000);

  it("extracts simple page-content snap geometry", async () => {
    const file = await createFixturePdf();
    const index = await extractPdfPageGeometryIndex(file, 0);

    expect(index.pageIndex).toBe(0);
    expect(index.buildMs).toBeGreaterThanOrEqual(0);
    expect(index.primitives).toEqual(
      expect.arrayContaining([
        {
          kind: "rect",
          rect: { x: 20, y: 20, width: 80, height: 40 },
        },
      ]),
    );
  });

  it("excludes artifact content from snap geometry", async () => {
    const document = await PDFDocument.create();
    const page = document.addPage([200, 200]);
    page.pushOperators(beginMarkedContent("Artifact"));
    page.drawLine({ start: { x: 10, y: 10 }, end: { x: 190, y: 10 } });
    page.pushOperators(endMarkedContent());
    page.drawLine({ start: { x: 20, y: 20 }, end: { x: 180, y: 20 } });
    const directory = await mkdtemp(
      join(tmpdir(), "butter-paper-artifact-geometry-"),
    );
    const file = join(directory, "artifact.pdf");
    await writeFile(file, await document.save());

    const index = await extractPdfPageGeometryIndex(file, 0);
    expect(index.primitives).toEqual([
      { kind: "line", start: { x: 20, y: 20 }, end: { x: 180, y: 20 } },
    ]);
  });

  it("renders a page and caches the surface", async () => {
    const file = await createFixturePdf();
    const handle = await openPdfDocument(file);
    const rendered = await handle.renderPage({ pageIndex: 0, scale: 1 });

    expect(rendered.width).toBeGreaterThan(0);
    expect(rendered.height).toBeGreaterThan(0);
    expect(handle.cache.stats().entries).toBe(1);

    const cached = handle.cache.get(
      `0:1:${(await handle.getPageInfo(0)).rotation}`,
    );
    expect(cached).toBeDefined();

    await handle.close();
  });

  it("keeps the cache within limits", async () => {
    const cache = new PdfRenderCache({ maxEntries: 2, maxBytes: 1000 });
    const canvas = { width: 10, height: 10, getContext: () => null } as const;
    cache.set("a", {
      pageIndex: 0,
      width: 10,
      height: 10,
      canvas: canvas as never,
    });
    cache.set("b", {
      pageIndex: 0,
      width: 10,
      height: 10,
      canvas: canvas as never,
    });
    cache.set("c", {
      pageIndex: 0,
      width: 10,
      height: 10,
      canvas: canvas as never,
    });
    expect(cache.stats().entries).toBeLessThanOrEqual(2);
  });

  it("writes, reopens and optionally emits a pending-redaction compatibility fixture", async () => {
    const file = await createFixturePdf();
    const output = file.replace(/\.pdf$/i, ".redact-first.pdf");
    const secondOutput = file.replace(/\.pdf$/i, ".redact-second.pdf");
    const handle = await openPdfDocument(file);
    const redact = createRedactMarkup({
      id: "compat-redact",
      pageIndex: 0,
      rect: { x: 20, y: 130, width: 90, height: 20 },
      redactionColor: "#102030",
      overlayText: "CONFIDENTIAL",
    });

    await handle.writer.save(handle, [redact], "saveAs", output);
    await handle.close();

    const rawDocument = await PDFDocument.load(await readFile(output));
    const annotations = rawDocument.getPage(0).node.Annots();
    const rawRedact = annotations?.lookup(0, PDFDict);
    expect(String(rawRedact?.get(PDFName.of("Subtype")))).toBe("/Redact");
    expect(readPdfNumberArray(rawRedact?.get(PDFName.of("Rect")))).toEqual([
      20, 130, 110, 150,
    ]);
    expect(
      readPdfNumberArray(rawRedact?.get(PDFName.of("QuadPoints"))),
    ).toEqual([20, 150, 110, 150, 20, 130, 110, 130]);
    expect(readPdfNumberArray(rawRedact?.get(PDFName.of("IC")))).toEqual([
      0x10 / 255,
      0x20 / 255,
      0x30 / 255,
    ]);
    expect(readPdfText(rawRedact?.get(PDFName.of("OverlayText")))).toBe(
      "CONFIDENTIAL",
    );
    expect(rawRedact?.has(PDFName.of("AP"))).toBe(false);

    const reopened = await openPdfDocument(output);
    const reopenedMarkups = await reopened.annotations.readPageAnnotations(0);
    expect(reopenedMarkups).toEqual([
      expect.objectContaining({
        id: "compat-redact",
        kind: "redact",
        rect: { x: 20, y: 130, width: 90, height: 20 },
        redactionColor: "#102030",
        overlayText: "CONFIDENTIAL",
        locked: false,
      }),
    ]);
    await reopened.writer.save(
      reopened,
      reopenedMarkups,
      "saveAs",
      secondOutput,
    );
    const reopenedTwice = await openPdfDocument(secondOutput);
    expect(await reopenedTwice.annotations.readPageAnnotations(0)).toEqual(
      reopenedMarkups,
    );

    const requestedOutput = process.env.BP_ELECTRON_REDACT_FIXTURE_OUTPUT;
    if (requestedOutput) {
      await writeFile(requestedOutput, await readFile(secondOutput), {
        flag: "wx",
      });
    }
    await reopened.close();
    await reopenedTwice.close();
  });

  it("round-trips rectangle, text box and callout annotations", async () => {
    const file = await createFixturePdf();
    const handle = await openPdfDocument(file);
    const output = file.replace(/\.pdf$/i, ".annotated.pdf");
    const rectangle = createRectangleMarkup({
      id: "rect-1",
      pageIndex: 0,
      rect: { x: 20, y: 20, width: 80, height: 40 },
    });
    const callout = createCalloutMarkup({
      id: "callout-1",
      pageIndex: 0,
      leader: {
        points: [pdfPoint(30, 30), pdfPoint(60, 60), pdfPoint(100, 90)],
      },
      textBox: { x: 110, y: 60, width: 90, height: 40 },
      text: "Need to check",
    });
    const textBox = createTextBoxMarkup({
      id: "text-1",
      pageIndex: 0,
      rect: { x: 20, y: 110, width: 90, height: 24 },
      text: "Default text",
    });
    const ellipse = createEllipseMarkup({
      id: "ellipse-1",
      pageIndex: 0,
      rect: { x: 120, y: 20, width: 70, height: 35 },
    });
    const arc = createArcMarkup({
      id: "arc-1",
      pageIndex: 0,
      rect: { x: 120, y: 110, width: 70, height: 55 },
      angle1: 90,
      angle2: 180,
    });
    const line = createLineMarkup({
      id: "line-1",
      pageIndex: 0,
      start: pdfPoint(20, 70),
      end: pdfPoint(100, 95),
    });
    const arrow = createArrowMarkup({
      id: "arrow-1",
      pageIndex: 0,
      start: pdfPoint(120, 70),
      end: pdfPoint(200, 95),
    });
    const dimension = createDimensionMarkup({
      id: "dimension-1",
      pageIndex: 0,
      start: pdfPoint(20, 50),
      end: pdfPoint(120, 50),
      dimensionLineOffset: 24,
      text: "100 ft",
    });
    const polyline = createPolylineMarkup({
      id: "polyline-1",
      pageIndex: 0,
      points: [pdfPoint(20, 130), pdfPoint(80, 160), pdfPoint(140, 120)],
    });
    const polygon = createPolygonMarkup({
      id: "polygon-1",
      pageIndex: 0,
      points: [
        pdfPoint(150, 130),
        pdfPoint(200, 160),
        pdfPoint(220, 120),
        pdfPoint(170, 110),
      ],
    });
    const pen = createPenMarkup({
      id: "pen-1",
      pageIndex: 0,
      paths: [[pdfPoint(20, 150), pdfPoint(60, 170), pdfPoint(100, 150)]],
      smoothCurves: true,
    });
    const highlight = createHighlightMarkup({
      id: "highlight-1",
      pageIndex: 0,
      paths: [[pdfPoint(120, 150), pdfPoint(220, 150)]],
    });
    const cloud = createCloudMarkup({
      id: "cloud-1",
      pageIndex: 0,
      controlPath: [
        pdfPoint(35, 35),
        pdfPoint(35, 70),
        pdfPoint(100, 70),
        pdfPoint(100, 35),
      ],
      appearancePath: "M 35 35 L 35 70 L 100 70 L 100 35 Z",
    });
    const cloudPlus = createCloudPlusMarkup({
      id: "cloud-plus-1",
      pageIndex: 0,
      cloud: {
        controlPath: [
          pdfPoint(30, 85),
          pdfPoint(30, 120),
          pdfPoint(95, 120),
          pdfPoint(95, 85),
        ],
        borderEffectIntensity: 2,
        appearancePath: "M 30 85 L 30 120 L 95 120 L 95 85 Z",
      },
      leader: {
        points: [pdfPoint(95, 102), pdfPoint(115, 102), pdfPoint(135, 102)],
      },
      textBox: { x: 135, y: 82, width: 90, height: 40 },
      text: "Cloud plus",
    });
    const image = createImageMarkup({
      id: "image-1",
      pageIndex: 0,
      rect: { x: 20, y: 75, width: 64, height: 40 },
      dataUrl: testImageDataUrl,
      mimeType: "image/png",
      aspectRatioLocked: true,
    });
    const snapshot = createSnapshotMarkup({
      id: "snapshot-1",
      pageIndex: 0,
      rect: { x: 90, y: 75, width: 64, height: 40 },
      dataUrl: testImageDataUrl,
      mimeType: "image/png",
    });
    const length = createLengthMarkup({
      id: "length-1",
      pageIndex: 0,
      start: pdfPoint(20, 130),
      end: pdfPoint(120, 130),
      displayUnit: "cm",
    });
    const polylength = createPolylengthMarkup({
      id: "polylength-1",
      pageIndex: 0,
      points: [pdfPoint(20, 140), pdfPoint(70, 140), pdfPoint(70, 170)],
    });
    const area = createAreaMarkup({
      id: "area-1",
      pageIndex: 0,
      points: [
        pdfPoint(130, 125),
        pdfPoint(190, 125),
        pdfPoint(190, 165),
        pdfPoint(130, 165),
      ],
    });
    const measurementScale = createCustomPageScale({
      pageIndex: 0,
      name: "1:100",
      pdfUnits: "cm",
      realUnits: "m",
      scaleX: 0.01,
      scaleY: 0.01,
      precision: { mode: "decimal", value: 0.01 },
    });

    await handle.writer.save(
      handle,
      [
        rectangle,
        ellipse,
        arc,
        line,
        arrow,
        dimension,
        length,
        polylength,
        area,
        polyline,
        polygon,
        pen,
        highlight,
        cloud,
        cloudPlus,
        image,
        snapshot,
        textBox,
        callout,
      ],
      "saveAs",
      output,
      [measurementScale],
    );
    const rawCallout = await readRawCalloutAnnotation(output);
    const rawCloudPlus = await readRawCloudPlusAnnotations(output);
    const rawArc = await readRawArcAnnotation(output);
    const rawDimension = await readRawDimensionAnnotation(output);
    const rawMeasurements = await readRawMeasurementAnnotations(output);
    const rawImage = await readRawImageAnnotation(output);
    const rawSnapshot = await readRawSnapshotAnnotation(output);
    const rawContracts = await readRawAnnotationContracts(output);
    expect(rawCallout).toMatchObject({
      subtype: "/FreeText",
      intent: "/FreeTextCallout",
      subject: "Callout",
      defaultAppearance: "1 0 0 rg /Helv 12 Tf",
      defaultStyle:
        "font: Helvetica 12pt; text-align:left; margin:3pt; line-height:13.8pt; color:#FF0000",
      color: [],
      border: [0, 0, 0],
      borderStyleWidth: 0,
      lineEnding: ["/None", "/OpenArrow"],
      calloutLine: [30, 30, 60, 60, 100, 90],
      rect: [24.5, 24.5, 205.5, 105.5],
      rectangleDifferences: [85.5, 35.5, 5.5, 5.5],
      appearanceBounds: [24.5, 24.5, 205.5, 105.5],
      appearanceMatrix: [1, 0, 0, 1, -24.5, -24.5],
    });
    expect(rawCallout.richContent).toContain("<p>Need to check</p>");
    expect(rawCloudPlus).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          subtype: "/Polygon",
          intent: "/PolygonCloud",
          intentEx: "/PolyText",
          subject: "Cloud+",
          borderEffect: "present",
          vertices: [30, 85, 30, 120, 95, 120, 95, 85],
          name: "bp:cloud-plus-1:cloud",
          groupNesting: [],
        }),
        expect.objectContaining({
          subtype: "/FreeText",
          intent: "/FreeTextCallout",
          intentEx: "/PolyText",
          subject: "Cloud+",
          calloutLine: [95, 102, 115, 102, 135, 102],
          lineEnding: ["/None", "/None"],
          appearance: "present",
          name: "bp:cloud-plus-1:text",
          groupNesting: [
            "Cloud+",
            "bp:cloud-plus-1:text",
            "bp:cloud-plus-1:cloud",
          ],
          color: [],
          rect: [89.5, 76.5, 230.5, 127.5],
          rectangleDifferences: [45.5, 5.5, 5.5, 5.5],
          appearanceBounds: [89.5, 76.5, 230.5, 127.5],
          appearanceMatrix: [1, 0, 0, 1, -89.5, -76.5],
        }),
      ]),
    );
    expect(
      rawCloudPlus.find((annotation) => annotation.subtype === "/FreeText")
        ?.richContent,
    ).toContain("<p>Cloud plus</p>");
    expect(rawImage).toMatchObject({
      subtype: "/Square",
      intent: "/SquareImage",
      subject: "Image",
      rect: [20, 75, 84, 115],
      imageMimeType: "image/png",
      aspectRatioLocked: true,
      hasAppearance: true,
    });
    expect(rawArc).toMatchObject({
      subtype: "/Circle",
      intent: "/CircleArc",
      subject: "Arc",
      rect: [120, 110, 190, 165],
      angle1: 90,
      angle2: 180,
      rd: [0.5, 0.5, 0.5, 0.5],
      hasAppearance: true,
    });
    expect(rawDimension).toMatchObject({
      subtype: "/Line",
      intent: "/LineDimension",
      subject: "Dimension",
      line: [20, 50, 120, 50],
      lineEnding: ["/ClosedArrow", "/ClosedArrow"],
      lineLeader: 24,
      lineLeaderExtension: 4,
      caption: "100 ft",
      hasAppearance: true,
    });
    expect(rawMeasurements).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          subtype: "/Line",
          intent: "/LineDimension",
          subject: "Length Measurement",
          line: [20, 130, 120, 130],
          contents: "100.00 cm",
          caption: "true",
          hasAppearance: true,
        }),
        expect.objectContaining({
          subtype: "/PolyLine",
          intent: "/PolyLineDimension",
          subject: "Polylength Measurement",
          vertices: [20, 140, 70, 140, 70, 170],
          contents: "0.80 m",
          hasAppearance: true,
        }),
        expect.objectContaining({
          subtype: "/Polygon",
          intent: "/PolygonDimension",
          subject: "Area Measurement",
          vertices: [130, 125, 190, 125, 190, 165, 130, 165],
          contents: "0.24 m^2",
          hasAppearance: true,
        }),
      ]),
    );
    expect(rawSnapshot).toMatchObject({
      subtype: "/Stamp",
      intent: "/StampSnapshot",
      subject: "Snapshot",
      rect: [90, 75, 154, 115],
      snapshotMimeType: "image/png",
      hasAppearance: true,
    });
    expect(rawContracts).toEqual(
      expect.arrayContaining([
        {
          name: "bp:rect-1",
          subtype: "/Square",
          subject: "Rectangle",
          intent: "",
          intentEx: "",
          hasAppearance: true,
        },
        {
          name: "bp:ellipse-1",
          subtype: "/Circle",
          subject: "Ellipse",
          intent: "",
          intentEx: "",
          hasAppearance: true,
        },
        {
          name: "bp:arc-1",
          subtype: "/Circle",
          subject: "Arc",
          intent: "/CircleArc",
          intentEx: "",
          hasAppearance: true,
        },
        {
          name: "bp:line-1",
          subtype: "/Line",
          subject: "Line",
          intent: "",
          intentEx: "",
          hasAppearance: false,
        },
        {
          name: "bp:arrow-1",
          subtype: "/Line",
          subject: "Arrow",
          intent: "/LineArrow",
          intentEx: "",
          hasAppearance: false,
        },
        {
          name: "bp:dimension-1",
          subtype: "/Line",
          subject: "Dimension",
          intent: "/LineDimension",
          intentEx: "",
          hasAppearance: true,
        },
        {
          name: "bp:length-1",
          subtype: "/Line",
          subject: "Length Measurement",
          intent: "/LineDimension",
          intentEx: "",
          hasAppearance: true,
        },
        {
          name: "bp:polylength-1",
          subtype: "/PolyLine",
          subject: "Polylength Measurement",
          intent: "/PolyLineDimension",
          intentEx: "",
          hasAppearance: true,
        },
        {
          name: "bp:area-1",
          subtype: "/Polygon",
          subject: "Area Measurement",
          intent: "/PolygonDimension",
          intentEx: "",
          hasAppearance: true,
        },
        {
          name: "bp:polyline-1",
          subtype: "/PolyLine",
          subject: "PolyLine",
          intent: "",
          intentEx: "",
          hasAppearance: true,
        },
        {
          name: "bp:polygon-1",
          subtype: "/Polygon",
          subject: "Polygon",
          intent: "",
          intentEx: "",
          hasAppearance: true,
        },
        {
          name: "bp:pen-1",
          subtype: "/Ink",
          subject: "Pen",
          intent: "",
          intentEx: "",
          hasAppearance: true,
        },
        {
          name: "bp:highlight-1",
          subtype: "/Ink",
          subject: "Highlight",
          intent: "",
          intentEx: "",
          hasAppearance: true,
        },
        {
          name: "bp:cloud-1",
          subtype: "/Polygon",
          subject: "Cloud",
          intent: "/PolygonCloud",
          intentEx: "",
          hasAppearance: true,
        },
        {
          name: "bp:cloud-plus-1:cloud",
          subtype: "/Polygon",
          subject: "Cloud+",
          intent: "/PolygonCloud",
          intentEx: "/PolyText",
          hasAppearance: true,
        },
        {
          name: "bp:cloud-plus-1:text",
          subtype: "/FreeText",
          subject: "Cloud+",
          intent: "/FreeTextCallout",
          intentEx: "/PolyText",
          hasAppearance: true,
        },
        {
          name: "bp:image-1",
          subtype: "/Square",
          subject: "Image",
          intent: "/SquareImage",
          intentEx: "",
          hasAppearance: true,
        },
        {
          name: "bp:snapshot-1",
          subtype: "/Stamp",
          subject: "Snapshot",
          intent: "/StampSnapshot",
          intentEx: "",
          hasAppearance: true,
        },
        {
          name: "bp:text-1",
          subtype: "/FreeText",
          subject: "Text Box",
          intent: "",
          intentEx: "",
          hasAppearance: true,
        },
        {
          name: "bp:callout-1",
          subtype: "/FreeText",
          subject: "Callout",
          intent: "/FreeTextCallout",
          intentEx: "",
          hasAppearance: true,
        },
      ]),
    );

    const reopened = await openPdfDocument(output);
    const annotations = await reopened.annotations.readPageAnnotations(0);
    const allAnnotations = await reopened.annotations.readAllPageAnnotations();

    expect(annotations).toHaveLength(19);
    expect(annotations.some((markup) => markup.kind === "rectangle")).toBe(
      true,
    );
    expect(annotations.some((markup) => markup.kind === "ellipse")).toBe(true);
    expect(annotations.some((markup) => markup.kind === "arc")).toBe(true);
    expect(annotations.some((markup) => markup.kind === "line")).toBe(true);
    expect(annotations.some((markup) => markup.kind === "arrow")).toBe(true);
    expect(
      annotations.some(
        (markup) => markup.kind === "dimension" && markup.text === "100 ft",
      ),
    ).toBe(true);
    expect(annotations.some((markup) => markup.kind === "length")).toBe(true);
    expect(annotations.some((markup) => markup.kind === "polylength")).toBe(
      true,
    );
    expect(annotations.some((markup) => markup.kind === "area")).toBe(true);
    expect(annotations.some((markup) => markup.kind === "polyline")).toBe(true);
    expect(annotations.some((markup) => markup.kind === "polygon")).toBe(true);
    expect(annotations.some((markup) => markup.kind === "pen")).toBe(true);
    expect(
      annotations.some(
        (markup) => markup.kind === "pen" && markup.smoothCurves === true,
      ),
    ).toBe(true);
    expect(annotations.some((markup) => markup.kind === "highlight")).toBe(
      true,
    );
    expect(annotations.some((markup) => markup.kind === "cloud")).toBe(true);
    expect(
      annotations.some(
        (markup) =>
          markup.kind === "cloud-plus" && markup.text === "Cloud plus",
      ),
    ).toBe(true);
    expect(
      annotations.some(
        (markup) =>
          markup.kind === "image" && markup.aspectRatioLocked === true,
      ),
    ).toBe(true);
    expect(annotations.some((markup) => markup.kind === "snapshot")).toBe(true);
    expect(
      annotations.some(
        (markup) =>
          markup.kind === "text-box" && markup.text === "Default text",
      ),
    ).toBe(true);
    expect(
      annotations.some(
        (markup) =>
          markup.kind === "callout" && markup.text === "Need to check",
      ),
    ).toBe(true);
    for (const original of [
      rectangle,
      ellipse,
      arc,
      line,
      arrow,
      dimension,
      length,
      polylength,
      area,
      polyline,
      polygon,
      pen,
      highlight,
      cloud,
      cloudPlus,
      image,
      snapshot,
      textBox,
      callout,
    ]) {
      expect(
        annotations.find((markup) => markup.id === original.id)?.appearance,
        original.id,
      ).toEqual(original.appearance);
    }
    expect(allAnnotations).toHaveLength(1);
    expect(allAnnotations[0]).toHaveLength(19);
    expect(allAnnotations[0]).toEqual(annotations);

    const secondOutput = file.replace(
      /\.pdf$/i,
      ".second-roundtrip.annotated.pdf",
    );
    await reopened.writer.save(reopened, annotations, "saveAs", secondOutput, [
      measurementScale,
    ]);
    const secondRawPdf = await PDFDocument.load(await readFile(secondOutput));
    const secondRawAnnots = secondRawPdf.context.lookup(
      secondRawPdf.getPage(0).node.Annots(),
    );
    expect(secondRawAnnots).toBeInstanceOf(PDFArray);
    expect((secondRawAnnots as PDFArray).size()).toBe(20);
    const secondReopened = await openPdfDocument(secondOutput);
    expect(
      await secondReopened.annotations.readPageAnnotations(0),
    ).toHaveLength(19);

    await handle.close();
    await reopened.close();
    await secondReopened.close();
  });

  it("emits a twice-saved two-Ink compatibility fixture on explicit request", async () => {
    const file = await createFixturePdf();
    const firstOutput = file.replace(/\.pdf$/i, ".ink-first.pdf");
    const secondOutput = file.replace(/\.pdf$/i, ".ink-second.pdf");
    const highlightPaths = [
      [pdfPoint(6, 40), pdfPoint(80, 40), pdfPoint(6, 40)],
      [pdfPoint(42, 6), pdfPoint(42, 82)],
    ];
    const penPaths = [
      [pdfPoint(110, 40), pdfPoint(145, 70), pdfPoint(180, 40)],
      [pdfPoint(135, 25), pdfPoint(155, 85)],
    ];
    const highlight = createHighlightMarkup({
      id: "compat-highlight",
      pageIndex: 0,
      paths: highlightPaths,
      appearance: {
        stroke: { color: "#ffcc00", widthPt: 12 },
        opacity: 0.35,
        blendMode: "multiply",
      },
    });
    const pen = createPenMarkup({
      id: "compat-pen",
      pageIndex: 0,
      paths: penPaths,
      smoothCurves: true,
      locked: true,
      appearance: {
        stroke: { color: "#1f6feb", widthPt: 3.25 },
        opacity: 0.8,
        blendMode: "normal",
      },
    });

    const assertInkState = (
      markups: Awaited<
        ReturnType<PdfDocumentHandle["annotations"]["readPageAnnotations"]>
      >,
    ) => {
      expect(markups.map(({ id, kind }) => ({ id, kind }))).toEqual([
        { id: "compat-highlight", kind: "highlight" },
        { id: "compat-pen", kind: "pen" },
      ]);
      const reopenedHighlight = markups[0];
      const reopenedPen = markups[1];
      expect(reopenedHighlight?.kind).toBe("highlight");
      expect(reopenedPen?.kind).toBe("pen");
      if (
        reopenedHighlight?.kind !== "highlight" ||
        reopenedPen?.kind !== "pen"
      ) {
        throw new Error(
          "Expected the Highlight and Pen compatibility annotations",
        );
      }
      expect(reopenedHighlight.paths).toEqual(highlightPaths);
      expect(reopenedHighlight.appearance).toEqual({
        stroke: { color: "#ffcc00", widthPt: 12 },
        opacity: 0.35,
        blendMode: "multiply",
      });
      expect(reopenedHighlight.locked).toBe(false);
      expect(reopenedHighlight.blendMode).toBeUndefined();
      expect(reopenedPen.paths).toEqual(penPaths);
      expect(reopenedPen.appearance).toEqual({
        stroke: { color: "#1f6feb", widthPt: 3.25 },
        opacity: 0.8,
        blendMode: "normal",
      });
      expect(reopenedPen.locked).toBe(true);
      expect(reopenedPen.smoothCurves).toBe(true);
    };

    const readRawInkState = async (path: string) => {
      const pdfDoc = await PDFDocument.load(await readFile(path));
      const annots = pdfDoc.context.lookup(pdfDoc.getPage(0).node.Annots());
      expect(annots).toBeInstanceOf(PDFArray);
      return (annots as PDFArray)
        .asArray()
        .map((ref) => pdfDoc.context.lookup(ref))
        .filter(
          (annotation): annotation is PDFDict => annotation instanceof PDFDict,
        )
        .filter(
          (annotation) =>
            String(annotation.get(PDFName.of("Subtype"))) === "/Ink",
        )
        .map((annotation) => {
          const inkList = pdfDoc.context.lookup(
            annotation.get(PDFName.of("InkList")),
          );
          const borderStyle = pdfDoc.context.lookup(
            annotation.get(PDFName.of("BS")),
          );
          const appearance = readTestNormalAppearance(pdfDoc, annotation);
          const resources = appearance
            ? pdfDoc.context.lookup(
                appearance.dict.get(PDFName.of("Resources")),
              )
            : undefined;
          const extGState =
            resources instanceof PDFDict
              ? pdfDoc.context.lookup(resources.get(PDFName.of("ExtGState")))
              : undefined;
          const graphicsState =
            extGState instanceof PDFDict
              ? pdfDoc.context.lookup(extGState.get(PDFName.of("GS0")))
              : undefined;
          return {
            name: readPdfText(annotation.get(PDFName.of("NM"))),
            subtype: String(annotation.get(PDFName.of("Subtype"))),
            subject: readPdfText(annotation.get(PDFName.of("Subj"))),
            inkList:
              inkList instanceof PDFArray
                ? inkList
                    .asArray()
                    .map((path) =>
                      readPdfNumberArray(pdfDoc.context.lookup(path)),
                    )
                : [],
            rect: readPdfNumberArray(annotation.get(PDFName.of("Rect"))),
            color: readPdfNumberArray(annotation.get(PDFName.of("C"))),
            opacity: Number(annotation.get(PDFName.of("CA"))),
            blendMode: String(annotation.get(PDFName.of("BM")) ?? ""),
            borderWidth:
              borderStyle instanceof PDFDict
                ? Number(borderStyle.get(PDFName.of("W")))
                : undefined,
            flags: Number(annotation.get(PDFName.of("F"))),
            smoothCurves:
              annotation.get(PDFName.of("BPSmoothCurves")) instanceof PDFBool
                ? (
                    annotation.get(PDFName.of("BPSmoothCurves")) as PDFBool
                  ).asBoolean()
                : undefined,
            hasAppearance: Boolean(appearance),
            appearanceBounds: appearance
              ? readPdfNumberArray(appearance.dict.get(PDFName.of("BBox")))
              : [],
            appearanceContent: appearance
              ? Buffer.from(decodePDFRawStream(appearance).decode()).toString(
                  "latin1",
                )
              : "",
            appearanceBlendMode:
              graphicsState instanceof PDFDict
                ? String(graphicsState.get(PDFName.of("BM")) ?? "")
                : "",
            appearanceOpacity:
              graphicsState instanceof PDFDict
                ? Number(graphicsState.get(PDFName.of("CA")))
                : undefined,
          };
        });
    };

    const handle = await openPdfDocument(file);
    await handle.writer.save(handle, [highlight, pen], "saveAs", firstOutput);
    const firstReopened = await openPdfDocument(firstOutput);
    const firstState = await firstReopened.annotations.readPageAnnotations(0);
    assertInkState(firstState);
    await firstReopened.writer.save(
      firstReopened,
      firstState,
      "saveAs",
      secondOutput,
    );
    const secondReopened = await openPdfDocument(secondOutput);
    const secondState = await secondReopened.annotations.readPageAnnotations(0);
    assertInkState(secondState);

    for (const output of [firstOutput, secondOutput]) {
      const raw = await readRawInkState(output);
      expect(
        raw.map(({ name, subtype, subject }) => ({ name, subtype, subject })),
      ).toEqual([
        { name: "bp:compat-highlight", subtype: "/Ink", subject: "Highlight" },
        { name: "bp:compat-pen", subtype: "/Ink", subject: "Pen" },
      ]);
      expect(raw[0]).toMatchObject({
        inkList: [
          [6, 40, 80, 40, 6, 40],
          [42, 6, 42, 82],
        ],
        rect: [0, 0, 86, 88],
        color: [1, 0.8, 0],
        opacity: 0.35,
        blendMode: "/Multiply",
        borderWidth: 12,
        flags: 4,
        smoothCurves: undefined,
        hasAppearance: true,
        appearanceBounds: [0, 0, 86, 88],
        appearanceBlendMode: "/Multiply",
        appearanceOpacity: 0.35,
      });
      expect(raw[0]?.appearanceContent.match(/\bS\b/g)).toHaveLength(2);
      expect(raw[0]?.appearanceContent.match(/\bm\b/g)).toHaveLength(2);
      expect(raw[1]).toMatchObject({
        inkList: [
          [110, 40, 145, 70, 180, 40],
          [135, 25, 155, 85],
        ],
        rect: [108.375, 23.375, 181.625, 86.625],
        color: [0x1f / 255, 0x6f / 255, 0xeb / 255],
        opacity: 0.8,
        blendMode: "",
        borderWidth: 3.25,
        flags: 128,
        smoothCurves: true,
        hasAppearance: true,
        appearanceBounds: [108.375, 23.375, 181.625, 86.625],
        appearanceBlendMode: "",
        appearanceOpacity: 0.8,
      });
      expect(raw[1]?.appearanceContent.match(/\bS\b/g)).toHaveLength(2);
      expect(raw[1]?.appearanceContent.match(/\bm\b/g)).toHaveLength(2);
      expect(raw[1]?.appearanceContent).toContain("3.25 w 1 J 1 j");
    }

    const requestedOutput = process.env.BP_ELECTRON_INK_FIXTURE_OUTPUT;
    if (requestedOutput) {
      await writeFile(requestedOutput, await readFile(secondOutput), {
        flag: "wx",
      });
    }

    await handle.close();
    await firstReopened.close();
    await secondReopened.close();
  });

  it.skipIf(!process.env.BP_NATIVE_INK_BRIDGE_FIXTURE)(
    "edits native-produced Ink through the current Electron bridge and regenerates standard appearances",
    async () => {
      const input = process.env.BP_NATIVE_INK_BRIDGE_FIXTURE!;
      const requestedOutput = process.env.BP_ELECTRON_INK_BRIDGE_OUTPUT;
      const temporaryDirectory = await mkdtemp(
        join(tmpdir(), "butter-paper-ink-bridge-"),
      );
      const finalOutput =
        requestedOutput ?? join(temporaryDirectory, "electron-bridge-final.pdf");
      const firstOutput = finalOutput.replace(/\.pdf$/i, ".first.pdf");
      expect(firstOutput).not.toBe(finalOutput);

      const inspectRawInk = async (path: string) => {
        const pdfDoc = await PDFDocument.load(await readFile(path));
        const annots = pdfDoc.context.lookup(pdfDoc.getPage(0).node.Annots());
        expect(annots).toBeInstanceOf(PDFArray);
        return (annots as PDFArray)
          .asArray()
          .map((ref) => pdfDoc.context.lookup(ref))
          .filter(
            (annotation): annotation is PDFDict =>
              annotation instanceof PDFDict &&
              String(annotation.get(PDFName.of("Subtype"))) === "/Ink",
          )
          .map((annotation) => {
            const appearance = readTestNormalAppearance(pdfDoc, annotation);
            const resources = appearance
              ? pdfDoc.context.lookup(
                  appearance.dict.get(PDFName.of("Resources")),
                )
              : undefined;
            const extGState =
              resources instanceof PDFDict
                ? pdfDoc.context.lookup(resources.get(PDFName.of("ExtGState")))
                : undefined;
            const graphicsState =
              extGState instanceof PDFDict
                ? pdfDoc.context.lookup(extGState.get(PDFName.of("GS0")))
                : undefined;
            return {
              id: readPdfText(annotation.get(PDFName.of("NM")))?.replace(
                /^bp:/,
                "",
              ),
              subject: readPdfText(annotation.get(PDFName.of("Subj"))),
              hasCanonicalPointBits: annotation.has(
                PDFName.of("BPCanonicalPointBits"),
              ),
              hasAppearance: Boolean(appearance),
              appearanceSubtype: appearance
                ? String(appearance.dict.get(PDFName.of("Subtype")))
                : "",
              appearanceContent: appearance
                ? Buffer.from(decodePDFRawStream(appearance).decode()).toString(
                    "latin1",
                  )
                : "",
              appearanceBlendMode:
                graphicsState instanceof PDFDict
                  ? String(graphicsState.get(PDFName.of("BM")) ?? "")
                  : "",
              appearanceOpacity:
                graphicsState instanceof PDFDict
                  ? Number(graphicsState.get(PDFName.of("CA")))
                  : undefined,
            };
          });
      };

      const assertSemanticState = (
        markups: Awaited<
          ReturnType<PdfDocumentHandle["annotations"]["readPageAnnotations"]>
        >,
        expectedPaths: ReadonlyMap<string, readonly (readonly { x: number; y: number }[])[]>,
      ) => {
        expect(markups.map(({ id, kind }) => ({ id, kind }))).toEqual([
          { id: "compat-highlight", kind: "highlight" },
          { id: "compat-pen", kind: "pen" },
        ]);
        const [highlight, pen] = markups;
        expect(highlight).toMatchObject({
          id: "compat-highlight",
          kind: "highlight",
          appearance: {
            stroke: { color: "#ffcc00", widthPt: 12 },
            opacity: 0.35,
            blendMode: "multiply",
          },
          locked: false,
        });
        expect(pen).toMatchObject({
          id: "compat-pen",
          kind: "pen",
          appearance: {
            stroke: { color: "#1f6feb", widthPt: 3.25 },
            opacity: 0.8,
            blendMode: "normal",
          },
          locked: true,
          smoothCurves: true,
        });
        if (highlight?.kind !== "highlight" || pen?.kind !== "pen") {
          throw new Error("Expected the native Highlight and Pen bridge fixture");
        }
        expect(highlight.paths).toEqual(expectedPaths.get(highlight.id));
        expect(pen.paths).toEqual(expectedPaths.get(pen.id));
      };

      const assertBridgeAppearance = (
        state: Awaited<ReturnType<typeof inspectRawInk>>,
      ) => {
        expect(
          state.map(({ id, subject }) => ({ id, subject })),
        ).toEqual([
          { id: "compat-highlight", subject: "Highlight" },
          { id: "compat-pen", subject: "Pen" },
        ]);
        for (const annotation of state) {
          expect(annotation.hasAppearance).toBe(true);
          expect(annotation.appearanceSubtype).toBe("/Form");
          expect(annotation.appearanceContent).toContain("1 J 1 j");
          expect(annotation.appearanceContent.match(/\bm\b/g)).toHaveLength(2);
          expect(annotation.appearanceContent.match(/\bS\b/g)).toHaveLength(2);
        }
        expect(state[0]).toMatchObject({
          appearanceBlendMode: "/Multiply",
          appearanceOpacity: 0.35,
        });
        expect(state[0]?.appearanceContent).toContain("12 w 1 J 1 j");
        expect(state[1]).toMatchObject({
          appearanceBlendMode: "",
          appearanceOpacity: 0.8,
        });
        expect(state[1]?.appearanceContent).toContain("3.25 w 1 J 1 j");
      };

      const nativeRaw = await inspectRawInk(input);
      expect(
        nativeRaw.map(({ id, hasCanonicalPointBits, hasAppearance }) => ({
          id,
          hasCanonicalPointBits,
          hasAppearance,
        })),
      ).toEqual([
        {
          id: "compat-highlight",
          hasCanonicalPointBits: true,
          hasAppearance: true,
        },
        {
          id: "compat-pen",
          hasCanonicalPointBits: true,
          hasAppearance: true,
        },
      ]);

      const source = await openPdfDocument(input);
      const sourceMarkups = await source.annotations.readPageAnnotations(0);
      const editedMarkups = sourceMarkups.map((markup) => {
        if (markup.id === "compat-highlight" && markup.kind === "highlight") {
          return {
            ...markup,
            paths: markup.paths.map((path, pathIndex) =>
              pathIndex === 0
                ? path
                : path.map((point) => pdfPoint(point.x + 11, point.y + 2)),
            ),
          };
        }
        if (markup.id === "compat-pen" && markup.kind === "pen") {
          return {
            ...markup,
            paths: markup.paths.map((path) =>
              path.map((point) => pdfPoint(point.x - 4, point.y + 6)),
            ),
          };
        }
        return markup;
      });
      const expectedPaths = new Map<
        string,
        readonly (readonly { x: number; y: number }[])[]
      >();
      for (const markup of editedMarkups) {
        if (markup.kind === "highlight" || markup.kind === "pen") {
          if (
            markup.id === "compat-highlight" ||
            markup.id === "compat-pen"
          ) {
            expectedPaths.set(markup.id, markup.paths);
          }
        }
      }
      assertSemanticState(
        editedMarkups.filter(
          (markup) =>
            markup.id === "compat-highlight" || markup.id === "compat-pen",
        ),
        expectedPaths,
      );
      await source.writer.save(source, editedMarkups, "saveAs", firstOutput);
      await source.close();

      const first = await openPdfDocument(firstOutput);
      const firstMarkups = await first.annotations.readPageAnnotations(0);
      assertSemanticState(
        firstMarkups.filter(
          (markup) =>
            markup.id === "compat-highlight" || markup.id === "compat-pen",
        ),
        expectedPaths,
      );
      assertBridgeAppearance(await inspectRawInk(firstOutput));
      await first.writer.save(first, firstMarkups, "saveAs", finalOutput);
      await first.close();

      const final = await openPdfDocument(finalOutput);
      const finalMarkups = await final.annotations.readPageAnnotations(0);
      assertSemanticState(
        finalMarkups.filter(
          (markup) =>
            markup.id === "compat-highlight" || markup.id === "compat-pen",
        ),
        expectedPaths,
      );
      assertBridgeAppearance(await inspectRawInk(finalOutput));
      await final.close();
    },
  );

  it("round-trips an exact calibrated page scale through two PDF inspections", async () => {
    const file = await createFixturePdf();
    const firstOutput = file.replace(/\.pdf$/i, ".calibrated-scale-1.pdf");
    const secondOutput = file.replace(/\.pdf$/i, ".calibrated-scale-2.pdf");
    const scale = calibratePageScale({
      pageIndex: 0,
      start: pdfPoint(0, 0),
      end: pdfPoint(72, 0),
      realLength: 1,
      realUnits: "m",
      pdfUnits: "in",
      name: "Calibrated 1 m",
      precision: { mode: "decimal", value: 0.01 },
    });
    const length = createLengthMarkup({
      id: "calibrated-length",
      pageIndex: 0,
      start: pdfPoint(90, 510),
      end: pdfPoint(342, 510),
    });
    const source = await openPdfDocument(file);
    await source.writer.save(source, [length], "saveAs", firstOutput, [scale]);

    const firstInspection = await inspectPdfDocumentBytes(
      await readFile(firstOutput),
    );
    expect(firstInspection.pageScales).toEqual([scale]);
    const reopened = await openPdfDocument(firstOutput);
    const reopenedMarkups = await reopened.annotations.readPageAnnotations(0);
    await reopened.writer.save(
      reopened,
      reopenedMarkups,
      "saveAs",
      secondOutput,
      firstInspection.pageScales,
    );

    const secondInspection = await inspectPdfDocumentBytes(
      await readFile(secondOutput),
    );
    expect(secondInspection.pageScales).toEqual([scale]);

    await source.close();
    await reopened.close();
  });

  it("round-trips inherited crop, rotation and UserUnit with calibrated markups and optionally emits a compatibility fixture", async () => {
    const file = await createCoordinateSpaceFixturePdf();
    const firstOutput = file.replace(/\.pdf$/i, ".annotated-1.pdf");
    const secondOutput = file.replace(/\.pdf$/i, ".annotated-2.pdf");
    const scale = calibratePageScale({
      pageIndex: 0,
      start: pdfPoint(100, 500),
      end: pdfPoint(200, 500),
      realLength: 1,
      realUnits: "m",
      pdfUnits: "in",
      name: "Coordinate 1 m",
      precision: { mode: "decimal", value: 0.01 },
    });
    const rectangle = {
      ...createRectangleMarkup({
        id: "coordinate-rectangle",
        pageIndex: 0,
        rect: { x: 80, y: 140, width: 100, height: 60 },
      }),
      locked: true,
    };
    const length = createLengthMarkup({
      id: "coordinate-length",
      pageIndex: 0,
      start: pdfPoint(100, 500),
      end: pdfPoint(300, 500),
    });
    const source = await openPdfDocument(file);
    await source.writer.save(
      source,
      [rectangle, length],
      "saveAs",
      firstOutput,
      [scale],
    );
    await source.close();

    const first = await inspectPdfDocumentBytes(await readFile(firstOutput));
    expect(first.pages).toEqual([
      {
        index: 0,
        width: 1_200,
        height: 800,
        rotation: 90,
        viewBox: { x: 50, y: 100, width: 400, height: 600 },
        userUnit: 2,
      },
    ]);
    expect(first.pageScales).toEqual([scale]);
    expect(first.annotationsByPage[0]).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          id: "coordinate-rectangle",
          kind: "rectangle",
          locked: true,
        }),
        expect.objectContaining({ id: "coordinate-length", kind: "length" }),
        expect.objectContaining({
          id: "vendor-coordinate-probe",
          kind: "imported-annotation",
        }),
      ]),
    );

    const reopened = await openPdfDocument(firstOutput);
    const reopenedMarkups = await reopened.annotations.readPageAnnotations(0);
    await reopened.writer.save(
      reopened,
      reopenedMarkups,
      "saveAs",
      secondOutput,
      [scale],
    );
    await reopened.close();
    const second = await inspectPdfDocumentBytes(await readFile(secondOutput));
    expect(second.pages).toEqual(first.pages);
    expect(second.pageScales).toEqual([scale]);
    expect(second.annotationsByPage).toEqual(first.annotationsByPage);

    const raw = await PDFDocument.load(await readFile(secondOutput));
    const page = raw.getPage(0);
    for (const key of ["MediaBox", "CropBox", "Rotate"]) {
      expect(page.node.has(PDFName.of(key))).toBe(false);
    }
    expect(
      raw.context
        .lookup(page.node.get(PDFName.of("UserUnit")), PDFNumber)
        ?.asNumber(),
    ).toBe(2);
    const parentRef = page.node.get(PDFName.of("Parent"));
    const parent = parentRef
      ? raw.context.lookup(parentRef, PDFDict)
      : undefined;
    expect(readPdfNumberArray(parent?.get(PDFName.of("MediaBox")))).toEqual([
      10, 20, 610, 820,
    ]);
    expect(readPdfNumberArray(parent?.get(PDFName.of("CropBox")))).toEqual([
      50, 100, 450, 700,
    ]);
    expect(
      raw.context
        .lookup(parent?.get(PDFName.of("Rotate")), PDFNumber)
        ?.asNumber(),
    ).toBe(90);

    const requestedOutput =
      process.env.BP_ELECTRON_COORDINATE_SPACE_FIXTURE_OUTPUT;
    if (requestedOutput) {
      await writeFile(requestedOutput, await readFile(secondOutput), {
        flag: "wx",
      });
    }
  });

  it("preserves untouched Bluebeam annotations and replaces complete logical objects on edit or delete", async () => {
    const file = await createBluebeamNativeFixturePdf();
    const handle = await openPdfDocument(file);
    const imported = await handle.annotations.readPageAnnotations(0);
    expect(imported).toHaveLength(3);
    expect(imported.map((markup) => markup.kind)).toEqual([
      "cloud-plus",
      "rectangle",
      "imported-annotation",
    ]);
    const cloudPlus = imported.find((markup) => markup.kind === "cloud-plus");
    const rectangle = imported.find((markup) => markup.kind === "rectangle");
    const reply = imported.find(
      (markup) => markup.kind === "imported-annotation",
    );
    expect(cloudPlus?.source?.annotationIds).toEqual([
      "nm:BB-CLOUD",
      "nm:BB-TEXT",
    ]);
    expect(cloudPlus).toMatchObject({
      textBox: { x: 225, y: 35, width: 65, height: 40 },
    });
    expect(
      cloudPlus?.source?.annotationMetadata?.map((metadata) => metadata.role),
    ).toEqual(["cloud", "text"]);
    expect(rectangle?.source?.annotationMetadata).toEqual([
      expect.objectContaining({
        annotationId: "nm:BB-RECT",
        role: "primary",
        author: "A. Reviewer",
        subject: "Structural review",
        creationDate: "D:20260803101500+10'00'",
        modificationDate: "D:20260803103000+10'00'",
        contents: "Keep this independent comment",
        flags: 4,
        statusModel: "Review",
        status: "Accepted",
      }),
    ]);
    expect(rectangle).toMatchObject({ kind: "rectangle", rotation: 15 });

    const untouchedOutput = file.replace(/\.pdf$/i, ".untouched.pdf");
    await handle.writer.save(handle, imported, "saveAs", untouchedOutput);
    const untouchedPdf = await PDFDocument.load(
      await readFile(untouchedOutput),
    );
    const untouchedAnnots = untouchedPdf.context.lookup(
      untouchedPdf.getPage(0).node.Annots(),
    ) as PDFArray;
    expect(untouchedAnnots.size()).toBe(4);
    const untouchedDicts = untouchedAnnots
      .asArray()
      .map((ref) => untouchedPdf.context.lookup(ref)) as PDFDict[];
    expect(
      untouchedDicts.map((annot) => readPdfText(annot.get(PDFName.of("NM")))),
    ).toEqual(["BB-RECT", "BB-CLOUD", "BB-TEXT", "BB-REPLY"]);
    expect(readPdfText(untouchedDicts[0]?.get(PDFName.of("BPProbe")))).toBe(
      "preserve-me",
    );

    if (!cloudPlus || cloudPlus.kind !== "cloud-plus") {
      throw new Error("Expected imported Cloud+");
    }
    const editedCloudPlus = { ...cloudPlus, text: "Edited in Butter Paper" };
    const editedOutput = file.replace(/\.pdf$/i, ".edited.pdf");
    await handle.writer.save(
      handle,
      [rectangle!, reply!, editedCloudPlus],
      "saveAs",
      editedOutput,
    );
    const editedPdf = await PDFDocument.load(await readFile(editedOutput));
    const editedAnnots = editedPdf.context.lookup(
      editedPdf.getPage(0).node.Annots(),
    ) as PDFArray;
    expect(editedAnnots.size()).toBe(4);
    const editedNames = editedAnnots.asArray().map((ref) => {
      const annot = editedPdf.context.lookup(ref) as PDFDict;
      return readPdfText(annot.get(PDFName.of("NM")));
    });
    expect(editedNames).toContain("BB-RECT");
    expect(editedNames).not.toContain("BB-CLOUD");
    expect(editedNames).not.toContain("BB-TEXT");
    expect(editedNames).toEqual(
      expect.arrayContaining([
        `bp:${cloudPlus.id}:cloud`,
        `bp:${cloudPlus.id}:text`,
      ]),
    );
    const editedCloudMetadata = await readRawAnnotationMetadata(editedOutput);
    expect(editedCloudMetadata).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          name: `bp:${cloudPlus.id}:cloud`,
          subject: "Custom cloud subject",
          replyType: "/Group",
          inReplyTo: `bp:${cloudPlus.id}:text`,
        }),
        expect.objectContaining({
          name: `bp:${cloudPlus.id}:text`,
          subject: "Custom cloud subject",
          replyType: "",
          inReplyTo: undefined,
        }),
      ]),
    );

    if (!rectangle || rectangle.kind !== "rectangle" || !reply) {
      throw new Error("Expected imported rectangle and reply");
    }
    const editedRectangle = {
      ...rectangle,
      rect: { ...rectangle.rect, x: rectangle.rect.x + 10 },
    };
    const metadataOutput = file.replace(/\.pdf$/i, ".metadata-edited.pdf");
    const fixedClockWriter = new PdfAnnotationWriter(
      file,
      () => new Date("2026-08-04T01:02:03.000Z"),
    );
    await fixedClockWriter.save(
      handle,
      [cloudPlus, reply, editedRectangle],
      "saveAs",
      metadataOutput,
    );
    const editedMetadata = await readRawAnnotationMetadata(metadataOutput);
    expect(editedMetadata).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          name: `bp:${rectangle.id}`,
          author: "A. Reviewer",
          subject: "Structural review",
          creationDate: "D:20260803101500+10'00'",
          modificationDate: "D:20260804010203Z",
          contents: "Keep this independent comment",
          flags: 4,
          stateModel: "Review",
          state: "Accepted",
          unsafeProbe: undefined,
        }),
        expect.objectContaining({
          name: "BB-REPLY",
          replyType: "/Reply",
          inReplyTo: `bp:${rectangle.id}`,
        }),
      ]),
    );

    const deletedOutput = file.replace(/\.pdf$/i, ".deleted.pdf");
    await handle.writer.save(handle, [], "saveAs", deletedOutput);
    const deletedPdf = await PDFDocument.load(await readFile(deletedOutput));
    const deletedAnnots = deletedPdf.context.lookup(
      deletedPdf.getPage(0).node.Annots(),
    ) as PDFArray;
    expect(deletedAnnots.size()).toBe(0);

    await handle.close();
  });

  it.each(["cloud", "text"] as const)(
    "keeps optional-content annotations and a Cloud+ pair with /OC on its %s member opaque across an unrelated edit",
    async (optionalContentCloudPart) => {
      const file = await createOptionalContentAnnotationFixturePdf(
        optionalContentCloudPart,
      );
      const handle = await openPdfDocument(file);
      const imported = await handle.annotations.readPageAnnotations(0);
      expect(imported.map((markup) => markup.kind)).toEqual([
        "imported-annotation",
        "imported-annotation",
        "imported-annotation",
        "rectangle",
      ]);
      expect(
        imported.filter((markup) => markup.kind === "cloud-plus"),
      ).toHaveLength(0);
      const visible = imported.find(
        (markup) => markup.id === "visible-rectangle",
      );
      if (!visible || visible.kind !== "rectangle") {
        throw new Error(
          "Expected the non-layered Rectangle to remain editable",
        );
      }
      const editedVisible = {
        ...visible,
        rect: { ...visible.rect, x: visible.rect.x + 8 },
      };
      const output = file.replace(/\.pdf$/i, ".edited.pdf");
      await handle.writer.save(
        handle,
        imported.map((markup) => (markup === visible ? editedVisible : markup)),
        "saveAs",
        output,
      );

      const saved = await PDFDocument.load(await readFile(output));
      const optionalContent = saved.context.lookup(
        saved.catalog.get(PDFName.of("OCProperties")),
      ) as PDFDict;
      const groups = saved.context.lookup(
        optionalContent.get(PDFName.of("OCGs")),
      ) as PDFArray;
      const defaults = saved.context.lookup(
        optionalContent.get(PDFName.of("D")),
      ) as PDFDict;
      const disabled = saved.context.lookup(
        defaults.get(PDFName.of("OFF")),
      ) as PDFArray;
      expect(groups.size()).toBe(1);
      expect(disabled.size()).toBe(1);
      expect(String(disabled.get(0))).toBe(String(groups.get(0)));

      const annotations = saved.context.lookup(
        saved.getPage(0).node.Annots(),
      ) as PDFArray;
      const dictionaries = annotations
        .asArray()
        .map((ref) => saved.context.lookup(ref)) as PDFDict[];
      const byName = new Map(
        dictionaries.map((annotation) => [
          readPdfText(annotation.get(PDFName.of("NM"))),
          annotation,
        ]),
      );
      expect([...byName.keys()]).toEqual([
        "bp:hidden-rectangle",
        "OC-CLOUD",
        "OC-TEXT",
        "bp:visible-rectangle",
      ]);
      expect(
        readPdfText(
          byName.get("bp:hidden-rectangle")?.get(PDFName.of("VendorProbe")),
        ),
      ).toBe("retain-hidden");
      expect(
        String(byName.get("bp:hidden-rectangle")?.get(PDFName.of("OC"))),
      ).toBe(String(groups.get(0)));
      const optionalMemberName =
        optionalContentCloudPart === "cloud" ? "OC-CLOUD" : "OC-TEXT";
      const plainMemberName =
        optionalContentCloudPart === "cloud" ? "OC-TEXT" : "OC-CLOUD";
      expect(
        String(byName.get(optionalMemberName)?.get(PDFName.of("OC"))),
      ).toBe(String(groups.get(0)));
      expect(
        byName.get(plainMemberName)?.get(PDFName.of("OC")),
      ).toBeUndefined();
      expect(
        readPdfNumberArray(
          byName.get("bp:visible-rectangle")?.get(PDFName.of("Rect")),
        ),
      ).toEqual([28, 100, 108, 150]);

      await handle.close();
    },
  );

  it("keeps OCMD membership policies and their OCG graph opaque across an unrelated edit", async () => {
    const file = await createOptionalContentMembershipFixturePdf();
    const handle = await openPdfDocument(file);
    const expectPageOptionalContent = async (document: PdfDocumentHandle) => {
      const rendered = await document.renderPage({
        pageIndex: 0,
        scale: 1,
        renderAnnotations: false,
      });
      const context = rendered.canvas.getContext("2d");
      expect(context).not.toBeNull();
      const pixelAtPdfPoint = (x: number, y: number) => {
        const pixel = context!.getImageData(x, rendered.height - y, 1, 1).data;
        return [pixel[0]!, pixel[1]!, pixel[2]!, pixel[3]!] as const;
      };
      const isBackground = (
        [red, green, blue, alpha]: readonly [number, number, number, number],
      ) => alpha === 0 || (red > 240 && green > 240 && blue > 240);
      expect(isBackground(pixelAtPdfPoint(40, 200))).toBe(true);
      const visibleExpression = pixelAtPdfPoint(100, 200);
      expect(visibleExpression[1]).toBeGreaterThan(200);
      expect(visibleExpression[0]).toBeLessThan(50);
      expect(visibleExpression[2]).toBeLessThan(50);
      expect(isBackground(pixelAtPdfPoint(160, 200))).toBe(true);
      const visibleForm = pixelAtPdfPoint(220, 200);
      expect(visibleForm[2]).toBeGreaterThan(200);
      expect(visibleForm[0]).toBeLessThan(50);
      expect(visibleForm[1]).toBeLessThan(50);
    };
    await expectPageOptionalContent(handle);
    const imported = await handle.annotations.readPageAnnotations(0);
    expect(imported.map((markup) => markup.kind)).toEqual([
      "imported-annotation",
      "imported-annotation",
      "imported-annotation",
      "rectangle",
    ]);
    const visible = imported.find(
      (markup) => markup.id === "visible-ocmd-control",
    );
    if (!visible || visible.kind !== "rectangle") {
      throw new Error(
        "Expected the non-layered OCMD control to remain editable",
      );
    }
    const output = file.replace(/\.pdf$/i, ".edited.pdf");
    await handle.writer.save(
      handle,
      imported.map((markup) =>
        markup === visible
          ? { ...visible, rect: { ...visible.rect, x: visible.rect.x + 8 } }
          : markup,
      ),
      "saveAs",
      output,
    );

    const saved = await PDFDocument.load(await readFile(output));
    const optionalContent = saved.context.lookup(
      saved.catalog.get(PDFName.of("OCProperties")),
    ) as PDFDict;
    const groups = saved.context.lookup(
      optionalContent.get(PDFName.of("OCGs")),
    ) as PDFArray;
    const defaults = saved.context.lookup(
      optionalContent.get(PDFName.of("D")),
    ) as PDFDict;
    const enabled = saved.context.lookup(
      defaults.get(PDFName.of("ON")),
    ) as PDFArray;
    const disabled = saved.context.lookup(
      defaults.get(PDFName.of("OFF")),
    ) as PDFArray;
    expect(groups.size()).toBe(2);
    expect(String(defaults.get(PDFName.of("BaseState")))).toBe("/ON");
    expect(enabled.asArray().map(String)).toEqual([String(groups.get(0))]);
    expect(disabled.asArray().map(String)).toEqual([String(groups.get(1))]);
    const radioButtonGroups = saved.context.lookup(
      defaults.get(PDFName.of("RBGroups")),
    ) as PDFArray;
    expect(radioButtonGroups.size()).toBe(1);
    const radioButtonGroup = saved.context.lookup(
      radioButtonGroups.get(0),
    ) as PDFArray;
    expect(radioButtonGroup.asArray().map(String)).toEqual(
      groups.asArray().map(String),
    );
    const configurations = saved.context.lookup(
      optionalContent.get(PDFName.of("Configs")),
    ) as PDFArray;
    expect(configurations.size()).toBe(1);
    const alternateConfiguration = saved.context.lookup(
      configurations.get(0),
    ) as PDFDict;
    expect(readPdfText(alternateConfiguration.get(PDFName.of("Name")))).toBe(
      "Alternate review state",
    );
    expect(String(alternateConfiguration.get(PDFName.of("BaseState")))).toBe(
      "/OFF",
    );
    const alternateEnabled = saved.context.lookup(
      alternateConfiguration.get(PDFName.of("ON")),
    ) as PDFArray;
    const alternateDisabled = saved.context.lookup(
      alternateConfiguration.get(PDFName.of("OFF")),
    ) as PDFArray;
    expect(alternateEnabled.asArray().map(String)).toEqual([
      String(groups.get(1)),
    ]);
    expect(alternateDisabled.asArray().map(String)).toEqual([
      String(groups.get(0)),
    ]);
    const alternateRadioButtonGroups = saved.context.lookup(
      alternateConfiguration.get(PDFName.of("RBGroups")),
    ) as PDFArray;
    const alternateRadioButtonGroup = saved.context.lookup(
      alternateRadioButtonGroups.get(0),
    ) as PDFArray;
    expect(alternateRadioButtonGroup.asArray().map(String)).toEqual(
      groups.asArray().map(String),
    );
    expect(
      readPdfText(alternateConfiguration.get(PDFName.of("VendorConfigProbe"))),
    ).toBe("retain-alternate-config");

    const pageResources = saved.context.lookup(
      saved.getPage(0).node.Resources(),
    ) as PDFDict;
    const properties = saved.context.lookup(
      pageResources.get(PDFName.of("Properties")),
    ) as PDFDict;
    expect(String(properties.get(PDFName.of("HiddenLayer")))).toBe(
      String(groups.get(1)),
    );
    const visibleExpressionRef = properties.get(
      PDFName.of("VisibleExpression"),
    );
    const visibleExpression = saved.context.lookup(
      visibleExpressionRef,
    ) as PDFDict;
    expect(String(visibleExpression.get(PDFName.of("Type")))).toBe("/OCMD");
    expect(readPdfText(visibleExpression.get(PDFName.of("VendorPolicyProbe")))).toBe(
      "retain-ve-precedence",
    );
    const pageContent = saved.context.lookup(
      saved.getPage(0).node.Contents(),
    );
    expect(pageContent).toBeInstanceOf(PDFRawStream);
    expect(
      Buffer.from(decodePDFRawStream(pageContent as PDFRawStream).decode()).toString(
        "latin1",
      ),
    ).toBe(optionalContentPageProgram);
    const xObjects = saved.context.lookup(
      pageResources.get(PDFName.of("XObject")),
    ) as PDFDict;
    for (const [name, expectedOptionalContent, expectedProbe] of [
      ["HiddenForm", groups.get(1), "retain-hidden-form"],
      ["VisibleForm", visibleExpressionRef, "retain-visible-form"],
    ] as const) {
      const form = saved.context.lookup(xObjects.get(PDFName.of(name)));
      expect(form).toBeInstanceOf(PDFRawStream);
      expect(String((form as PDFRawStream).dict.get(PDFName.of("OC")))).toBe(
        String(expectedOptionalContent),
      );
      expect(
        readPdfText(
          (form as PDFRawStream).dict.get(PDFName.of("VendorStreamProbe")),
        ),
      ).toBe(expectedProbe);
      expect(
        Buffer.from(decodePDFRawStream(form as PDFRawStream).decode()).toString(
          "latin1",
        ),
      ).toBe(optionalContentFormProgram);
    }

    const annotations = saved.context.lookup(
      saved.getPage(0).node.Annots(),
    ) as PDFArray;
    const dictionaries = annotations
      .asArray()
      .map((ref) => saved.context.lookup(ref)) as PDFDict[];
    const byName = new Map(
      dictionaries.map((annotation) => [
        readPdfText(annotation.get(PDFName.of("NM"))),
        annotation,
      ]),
    );
    expect([...byName.keys()]).toEqual([
      "bp:ocmd-all-on",
      "bp:ocmd-any-on",
      "bp:ocmd-visible-expression",
      "bp:set-ocg-state-link",
      "bp:visible-ocmd-control",
    ]);
    const layerStateAction = saved.context.lookup(
      byName.get("bp:set-ocg-state-link")?.get(PDFName.of("A")),
    ) as PDFDict;
    expect(String(layerStateAction.get(PDFName.of("S")))).toBe(
      "/SetOCGState",
    );
    expect(layerStateAction.get(PDFName.of("PreserveRB"))).toEqual(
      PDFBool.False,
    );
    expect(
      readPdfText(layerStateAction.get(PDFName.of("VendorActionProbe"))),
    ).toBe("retain-set-ocg-state");
    const layerState = saved.context.lookup(
      layerStateAction.get(PDFName.of("State")),
    ) as PDFArray;
    expect(layerState.asArray().map(String)).toEqual([
      "/Toggle",
      String(groups.get(0)),
      "/OFF",
      String(groups.get(1)),
    ]);
    for (const [annotationName, policy, probe] of [
      ["bp:ocmd-all-on", "AllOn", "retain-all-on"],
      ["bp:ocmd-any-on", "AnyOn", "retain-any-on"],
      ["bp:ocmd-visible-expression", "AllOn", "retain-ve-precedence"],
    ] as const) {
      const membership = saved.context.lookup(
        byName.get(annotationName)?.get(PDFName.of("OC")),
      ) as PDFDict;
      expect(String(membership.get(PDFName.of("Type")))).toBe("/OCMD");
      expect(String(membership.get(PDFName.of("P")))).toBe(`/${policy}`);
      expect(readPdfText(membership.get(PDFName.of("VendorPolicyProbe")))).toBe(
        probe,
      );
      const members = saved.context.lookup(
        membership.get(PDFName.of("OCGs")),
      ) as PDFArray;
      expect(members.asArray().map(String)).toEqual(
        groups.asArray().map(String),
      );
    }
    const expressionMembership = saved.context.lookup(
      byName.get("bp:ocmd-visible-expression")?.get(PDFName.of("OC")),
    ) as PDFDict;
    const expression = saved.context.lookup(
      expressionMembership.get(PDFName.of("VE")),
    ) as PDFArray;
    const expressionOr = saved.context.lookup(expression.get(1)) as PDFArray;
    const expressionNot = saved.context.lookup(expression.get(2)) as PDFArray;
    expect(String(expression.get(0))).toBe("/And");
    expect(expressionOr.asArray().map(String)).toEqual([
      "/Or",
      String(groups.get(1)),
      String(groups.get(0)),
    ]);
    expect(expressionNot.asArray().map(String)).toEqual([
      "/Not",
      String(groups.get(1)),
    ]);
    expect(
      readPdfNumberArray(
        byName.get("bp:visible-ocmd-control")?.get(PDFName.of("Rect")),
      ),
    ).toEqual([28, 120, 108, 170]);

    const reopened = await openPdfDocument(output);
    await expectPageOptionalContent(reopened);
    await reopened.close();

    await handle.close();
  });

  it.each(["cloud", "text"] as const)(
    "keeps reciprocal Popup annotations and a Cloud+ pair with /Popup on its %s member opaque across an unrelated edit",
    async (popupLinkedCloudPart) => {
      const file =
        await createPopupLinkedAnnotationFixturePdf(popupLinkedCloudPart);
      const handle = await openPdfDocument(file);
      const imported = await handle.annotations.readPageAnnotations(0);
      expect(
        imported.filter((markup) => markup.kind === "cloud-plus"),
      ).toHaveLength(0);
      expect(
        imported.filter((markup) => markup.kind === "imported-annotation"),
      ).toHaveLength(3);
      const visible = imported.find(
        (markup) => markup.id === "visible-popup-control",
      );
      if (!visible || visible.kind !== "rectangle") {
        throw new Error("Expected the unrelated Rectangle to remain editable");
      }
      const output = file.replace(/\.pdf$/i, ".edited.pdf");
      await handle.writer.save(
        handle,
        imported.map((markup) =>
          markup === visible
            ? { ...visible, rect: { ...visible.rect, x: visible.rect.x + 8 } }
            : markup,
        ),
        "saveAs",
        output,
      );

      const saved = await PDFDocument.load(await readFile(output));
      const annotations = saved.context.lookup(
        saved.getPage(0).node.Annots(),
      ) as PDFArray;
      const refs = annotations.asArray();
      const dictionaries = refs.map((ref) =>
        saved.context.lookup(ref),
      ) as PDFDict[];
      const nameByRef = new Map(
        refs.map((ref, index) => [
          String(ref),
          readPdfText(dictionaries[index]?.get(PDFName.of("NM"))),
        ]),
      );
      const parent = dictionaries.find(
        (annotation) =>
          readPdfText(annotation.get(PDFName.of("NM"))) === "bp:popup-parent",
      );
      const parentPopup = dictionaries.find(
        (annotation) =>
          readPdfText(annotation.get(PDFName.of("VendorProbe"))) ===
          "retain-parent-popup",
      );
      const pairMemberName =
        popupLinkedCloudPart === "cloud" ? "POPUP-CLOUD" : "POPUP-TEXT";
      const pairMember = dictionaries.find(
        (annotation) =>
          readPdfText(annotation.get(PDFName.of("NM"))) === pairMemberName,
      );
      const pairPopup = dictionaries.find(
        (annotation) =>
          readPdfText(annotation.get(PDFName.of("VendorProbe"))) ===
          "retain-pair-popup",
      );
      expect(readPdfText(parent?.get(PDFName.of("VendorProbe")))).toBe(
        "retain-parent",
      );
      expect(
        nameByRef.get(String(parentPopup?.get(PDFName.of("Parent")))),
      ).toBe("bp:popup-parent");
      expect(String(parent?.get(PDFName.of("Popup")))).toBe(
        String(refs[dictionaries.indexOf(parentPopup!)]),
      );
      expect(nameByRef.get(String(pairPopup?.get(PDFName.of("Parent"))))).toBe(
        pairMemberName,
      );
      expect(String(pairMember?.get(PDFName.of("Popup")))).toBe(
        String(refs[dictionaries.indexOf(pairPopup!)]),
      );
      expect(
        dictionaries
          .map((annotation) => readPdfText(annotation.get(PDFName.of("NM"))))
          .filter(Boolean),
      ).toEqual([
        "bp:popup-parent",
        "POPUP-CLOUD",
        "POPUP-TEXT",
        "bp:visible-popup-control",
      ]);
      expect(
        readPdfNumberArray(
          dictionaries
            .find(
              (annotation) =>
                readPdfText(annotation.get(PDFName.of("NM"))) ===
                "bp:visible-popup-control",
            )
            ?.get(PDFName.of("Rect")),
        ),
      ).toEqual([28, 120, 108, 170]);

      await handle.close();
    },
  );

  it("preserves GPUI logical Rectangle geometry through an Electron edit and rotated appearance rebuild", async () => {
    const file = await createGpuiRotatedRectangleFixturePdf();
    const handle = await openPdfDocument(file);
    const imported = await handle.annotations.readPageAnnotations(0);
    const native = imported.find(
      (markup) => markup.id === "native-rotated-rectangle",
    );
    const legacy = imported.find((markup) => markup.id === "legacy-rectangle");
    expect(native).toMatchObject({
      kind: "rectangle",
      rect: { x: 300, y: 200, width: 200, height: 110 },
      rotation: 30,
    });
    expect(legacy).toMatchObject({
      kind: "rectangle",
      rect: { x: 40, y: 50, width: 100, height: 60 },
      rotation: 15,
    });
    if (!native || native.kind !== "rectangle") {
      throw new Error(
        "Expected the GPUI Rectangle to import as editable geometry",
      );
    }

    const edited = {
      ...native,
      rect: { ...native.rect, x: native.rect.x + 12 },
    };
    const output = file.replace(/\.pdf$/i, ".edited.pdf");
    await handle.writer.save(handle, [edited, legacy!], "saveAs", output);

    const rawPdf = await PDFDocument.load(await readFile(output));
    const rawAnnots = rawPdf.context.lookup(
      rawPdf.getPage(0).node.Annots(),
    ) as PDFArray;
    const rawRectangle = rawAnnots
      .asArray()
      .map((ref) => rawPdf.context.lookup(ref))
      .filter((value): value is PDFDict => value instanceof PDFDict)
      .find(
        (annotation) =>
          readPdfText(annotation.get(PDFName.of("NM"))) ===
          "bp:native-rotated-rectangle",
      );
    expect(rawRectangle).toBeDefined();
    expect(readPdfNumberArray(rawRectangle?.get(PDFName.of("BPRect")))).toEqual(
      [312, 200, 512, 310],
    );
    expect(Number(rawRectangle?.get(PDFName.of("BPRotation")))).toBe(30);
    expect(Number(rawRectangle?.get(PDFName.of("Rotation")))).toBe(30);
    const worldRect = readPdfNumberArray(rawRectangle?.get(PDFName.of("Rect")));
    expect(worldRect[0]).toBeLessThan(312);
    expect(worldRect[1]).toBeLessThan(200);
    expect(worldRect[2]).toBeGreaterThan(512);
    expect(worldRect[3]).toBeGreaterThan(310);
    const normal = rawRectangle
      ? readTestNormalAppearance(rawPdf, rawRectangle)
      : undefined;
    expect(normal).toBeDefined();
    const appearanceBounds = readPdfNumberArray(
      normal?.dict.get(PDFName.of("BBox")),
    );
    expect(appearanceBounds[0]).toBe(0);
    expect(appearanceBounds[1]).toBe(0);
    expect(appearanceBounds[2]).toBeCloseTo(worldRect[2]! - worldRect[0]!, 4);
    expect(appearanceBounds[3]).toBeCloseTo(worldRect[3]! - worldRect[1]!, 4);
    const appearance = new TextDecoder().decode(
      decodePDFRawStream(normal!).decode(),
    );
    expect(appearance).toContain(" m");
    expect(appearance.match(/ l/g)).toHaveLength(3);
    expect(appearance).not.toContain(" re");

    const reopened = await openPdfDocument(output);
    const reopenedRectangle = (
      await reopened.annotations.readPageAnnotations(0)
    ).find((markup) => markup.id === native.id);
    expect(reopenedRectangle).toMatchObject({
      kind: "rectangle",
      rect: edited.rect,
      rotation: 30,
    });
    await handle.close();
    await reopened.close();
  });

  it("preserves GPUI logical Ellipse geometry through Electron edits and rotated appearance rebuilds", async () => {
    const file = await createGpuiRotatedEllipseFixturePdf();
    const handle = await openPdfDocument(file);
    const imported = await handle.annotations.readPageAnnotations(0);
    const native = imported.find(
      (markup) => markup.id === "native-rotated-ellipse",
    );
    const legacy = imported.find((markup) => markup.id === "legacy-ellipse");
    expect(native).toMatchObject({
      kind: "ellipse",
      rect: { x: 300, y: 200, width: 200, height: 110 },
      rotation: 30,
    });
    expect(legacy).toMatchObject({
      kind: "ellipse",
      rect: { x: 40, y: 50, width: 100, height: 60 },
      rotation: 15,
    });
    if (!native || native.kind !== "ellipse") {
      throw new Error(
        "Expected the GPUI Ellipse to import as editable geometry",
      );
    }

    const edited = {
      ...native,
      rect: { ...native.rect, x: native.rect.x + 12 },
    };
    const output = file.replace(/\.pdf$/i, ".edited.pdf");
    await handle.writer.save(handle, [edited, legacy!], "saveAs", output);

    const inspectNativeEllipse = async (path: string) => {
      const rawPdf = await PDFDocument.load(await readFile(path));
      const rawAnnots = rawPdf.context.lookup(
        rawPdf.getPage(0).node.Annots(),
      ) as PDFArray;
      const rawEllipse = rawAnnots
        .asArray()
        .map((ref) => rawPdf.context.lookup(ref))
        .filter((value): value is PDFDict => value instanceof PDFDict)
        .find(
          (annotation) =>
            readPdfText(annotation.get(PDFName.of("NM"))) ===
            "bp:native-rotated-ellipse",
        );
      expect(rawEllipse).toBeDefined();
      expect(readPdfNumberArray(rawEllipse?.get(PDFName.of("BPRect")))).toEqual(
        [312, 200, 512, 310],
      );
      expect(Number(rawEllipse?.get(PDFName.of("BPRotation")))).toBe(30);
      expect(Number(rawEllipse?.get(PDFName.of("Rotation")))).toBe(30);
      const worldRect = readPdfNumberArray(rawEllipse?.get(PDFName.of("Rect")));
      expect(worldRect[0]).toBeLessThan(312);
      expect(worldRect[1]).toBeLessThan(200);
      expect(worldRect[2]).toBeGreaterThan(512);
      expect(worldRect[3]).toBeGreaterThan(310);
      const normal = rawEllipse
        ? readTestNormalAppearance(rawPdf, rawEllipse)
        : undefined;
      expect(normal).toBeDefined();
      const appearanceBounds = readPdfNumberArray(
        normal?.dict.get(PDFName.of("BBox")),
      );
      expect(appearanceBounds[0]).toBe(0);
      expect(appearanceBounds[1]).toBe(0);
      expect(appearanceBounds[2]).toBeCloseTo(worldRect[2]! - worldRect[0]!, 4);
      expect(appearanceBounds[3]).toBeCloseTo(worldRect[3]! - worldRect[1]!, 4);
      const appearance = new TextDecoder().decode(
        decodePDFRawStream(normal!).decode(),
      );
      expect(appearance).toContain(" m");
      expect(appearance.match(/ c/g)).toHaveLength(16);
      expect(appearance).toContain(" h");
      return worldRect;
    };

    const firstWorldRect = await inspectNativeEllipse(output);
    const reopened = await openPdfDocument(output);
    const reopenedMarkups = await reopened.annotations.readPageAnnotations(0);
    const reopenedEllipse = reopenedMarkups.find(
      (markup) => markup.id === native.id,
    );
    expect(reopenedEllipse).toMatchObject({
      kind: "ellipse",
      rect: edited.rect,
      rotation: 30,
    });
    const secondOutput = file.replace(/\.pdf$/i, ".edited-twice.pdf");
    await reopened.writer.save(
      reopened,
      reopenedMarkups,
      "saveAs",
      secondOutput,
    );
    expect(await inspectNativeEllipse(secondOutput)).toEqual(firstWorldRect);

    const reopenedTwice = await openPdfDocument(secondOutput);
    expect(
      (await reopenedTwice.annotations.readPageAnnotations(0)).find(
        (markup) => markup.id === native.id,
      ),
    ).toMatchObject({
      kind: "ellipse",
      rect: edited.rect,
      rotation: 30,
    });
    const handoffOutput =
      process.env.BP_ELECTRON_ROTATED_ELLIPSE_FIXTURE_OUTPUT;
    if (handoffOutput) {
      await writeFile(handoffOutput, await readFile(secondOutput), {
        flag: "wx",
      });
    }
    await handle.close();
    await reopened.close();
    await reopenedTwice.close();
  });

  it("recovers native rotated Image and Snapshot geometry without repeated world-bounds expansion", async () => {
    const file = await createGpuiRotatedImageFixturePdf();
    const handle = await openPdfDocument(file);
    const imported = await handle.annotations.readPageAnnotations(0);
    const native = imported.find(
      (markup) => markup.id === "native-rotated-image",
    );
    const nativeSnapshot = imported.find(
      (markup) => markup.id === "native-rotated-snapshot",
    );
    expect(native).toMatchObject({
      kind: "image",
      rotation: 30,
      aspectRatioLocked: true,
    });
    expect(nativeSnapshot).toMatchObject({ kind: "snapshot", rotation: 30 });
    if (!native || native.kind !== "image")
      throw new Error("Expected the native rotated Image");
    if (!nativeSnapshot || nativeSnapshot.kind !== "snapshot")
      throw new Error("Expected the native rotated Snapshot");
    expect(native.rect.x).toBeCloseTo(300, 4);
    expect(native.rect.y).toBeCloseTo(200, 4);
    expect(native.rect.width).toBeCloseTo(96, 4);
    expect(native.rect.height).toBeCloseTo(60, 4);
    expect(nativeSnapshot.rect.x).toBeCloseTo(100, 4);
    expect(nativeSnapshot.rect.y).toBeCloseTo(100, 4);
    expect(nativeSnapshot.rect.width).toBeCloseTo(96, 4);
    expect(nativeSnapshot.rect.height).toBeCloseTo(60, 4);

    const edited = {
      ...native,
      rect: { ...native.rect, x: native.rect.x + 12 },
    };
    const editedSnapshot = {
      ...nativeSnapshot,
      rect: { ...nativeSnapshot.rect, x: nativeSnapshot.rect.x + 12 },
    };
    const output = file.replace(/\.pdf$/i, ".edited.pdf");
    await handle.writer.save(
      handle,
      [edited, editedSnapshot],
      "saveAs",
      output,
    );

    const inspectNativeMedia = async (path: string, id: string) => {
      const rawPdf = await PDFDocument.load(await readFile(path));
      const rawAnnots = rawPdf.context.lookup(
        rawPdf.getPage(0).node.Annots(),
      ) as PDFArray;
      const rawMedia = rawAnnots
        .asArray()
        .map((ref) => rawPdf.context.lookup(ref))
        .filter((value): value is PDFDict => value instanceof PDFDict)
        .find(
          (annotation) =>
            readPdfText(annotation.get(PDFName.of("NM"))) === `bp:${id}`,
        );
      expect(rawMedia).toBeDefined();
      expect(Number(rawMedia?.get(PDFName.of("Rotation")))).toBe(30);
      if (id === "native-rotated-image")
        expect(rawMedia?.get(PDFName.of("BPAspectRatioLocked"))).toBe(
          PDFBool.True,
        );
      const normal = rawMedia
        ? readTestNormalAppearance(rawPdf, rawMedia)
        : undefined;
      expect(normal).toBeDefined();
      expect(
        normal
          ? collectTestAppearanceStreams(rawPdf, normal).filter(
              (stream) =>
                String(stream.dict.get(PDFName.of("Subtype"))) === "/Image",
            )
          : [],
      ).toHaveLength(1);
      return readPdfNumberArray(rawMedia?.get(PDFName.of("Rect")));
    };

    const firstWorldRect = await inspectNativeMedia(output, native.id);
    const firstSnapshotWorldRect = await inspectNativeMedia(
      output,
      nativeSnapshot.id,
    );
    const reopened = await openPdfDocument(output);
    const reopenedMarkups = await reopened.annotations.readPageAnnotations(0);
    const reopenedImage = reopenedMarkups.find(
      (markup) => markup.id === native.id,
    );
    const reopenedSnapshot = reopenedMarkups.find(
      (markup) => markup.id === nativeSnapshot.id,
    );
    expect(reopenedImage).toMatchObject({
      kind: "image",
      rotation: 30,
      aspectRatioLocked: true,
    });
    expect(reopenedSnapshot).toMatchObject({ kind: "snapshot", rotation: 30 });
    if (!reopenedImage || reopenedImage.kind !== "image")
      throw new Error("Expected the edited rotated Image");
    if (!reopenedSnapshot || reopenedSnapshot.kind !== "snapshot")
      throw new Error("Expected the edited rotated Snapshot");
    expect(reopenedImage.rect.x).toBeCloseTo(312, 4);
    expect(reopenedImage.rect.y).toBeCloseTo(200, 4);
    expect(reopenedImage.rect.width).toBeCloseTo(96, 4);
    expect(reopenedImage.rect.height).toBeCloseTo(60, 4);
    expect(reopenedSnapshot.rect.x).toBeCloseTo(112, 4);
    expect(reopenedSnapshot.rect.y).toBeCloseTo(100, 4);
    expect(reopenedSnapshot.rect.width).toBeCloseTo(96, 4);
    expect(reopenedSnapshot.rect.height).toBeCloseTo(60, 4);

    const secondOutput = file.replace(/\.pdf$/i, ".edited-twice.pdf");
    await reopened.writer.save(
      reopened,
      reopenedMarkups,
      "saveAs",
      secondOutput,
    );
    const secondWorldRect = await inspectNativeMedia(secondOutput, native.id);
    const secondSnapshotWorldRect = await inspectNativeMedia(
      secondOutput,
      nativeSnapshot.id,
    );
    expect(secondWorldRect).toHaveLength(4);
    secondWorldRect.forEach((value, index) =>
      expect(value).toBeCloseTo(firstWorldRect[index]!, 4),
    );
    secondSnapshotWorldRect.forEach((value, index) =>
      expect(value).toBeCloseTo(firstSnapshotWorldRect[index]!, 4),
    );
    const reopenedTwice = await openPdfDocument(secondOutput);
    const reopenedTwiceMarkups =
      await reopenedTwice.annotations.readPageAnnotations(0);
    const reopenedTwiceImage = reopenedTwiceMarkups.find(
      (markup) => markup.id === native.id,
    );
    const reopenedTwiceSnapshot = reopenedTwiceMarkups.find(
      (markup) => markup.id === nativeSnapshot.id,
    );
    expect(reopenedTwiceImage?.kind).toBe("image");
    if (!reopenedTwiceImage || reopenedTwiceImage.kind !== "image")
      throw new Error("Expected the twice-edited rotated Image");
    expect(reopenedTwiceImage.rect.width).toBeCloseTo(96, 4);
    expect(reopenedTwiceImage.rect.height).toBeCloseTo(60, 4);
    expect(reopenedTwiceSnapshot?.kind).toBe("snapshot");
    if (!reopenedTwiceSnapshot || reopenedTwiceSnapshot.kind !== "snapshot")
      throw new Error("Expected the twice-edited rotated Snapshot");
    expect(reopenedTwiceSnapshot.rect.width).toBeCloseTo(96, 4);
    expect(reopenedTwiceSnapshot.rect.height).toBeCloseTo(60, 4);

    const handoffOutput = process.env.BP_ELECTRON_ROTATED_MEDIA_FIXTURE_OUTPUT;
    if (handoffOutput) {
      await writeFile(handoffOutput, await readFile(secondOutput), {
        flag: "wx",
      });
    }

    await handle.close();
    await reopened.close();
    await reopenedTwice.close();
  });

  it("classifies native measurement tools from intent and Measure dictionaries when subjects are customized", async () => {
    const file = await createFixturePdf();
    const handle = await openPdfDocument(file);
    const output = file.replace(
      /\.pdf$/i,
      ".subject-independent-measurements.pdf",
    );
    const scale = createCustomPageScale({
      pageIndex: 0,
      name: "1:1",
      pdfUnits: "cm",
      realUnits: "m",
      scaleX: 1,
      scaleY: 1,
    });
    await handle.writer.save(
      handle,
      [
        createLengthMarkup({
          id: "custom-subject-length",
          pageIndex: 0,
          start: pdfPoint(20, 20),
          end: pdfPoint(100, 20),
        }),
        createPolylengthMarkup({
          id: "custom-subject-polylength",
          pageIndex: 0,
          points: [pdfPoint(20, 50), pdfPoint(60, 80), pdfPoint(100, 50)],
        }),
        createAreaMarkup({
          id: "custom-subject-area",
          pageIndex: 0,
          points: [
            pdfPoint(140, 20),
            pdfPoint(220, 20),
            pdfPoint(220, 80),
            pdfPoint(140, 80),
          ],
        }),
      ],
      "saveAs",
      output,
      [scale],
    );

    const raw = await PDFDocument.load(await readFile(output));
    const annots = raw.context.lookup(raw.getPage(0).node.Annots()) as PDFArray;
    for (const ref of annots.asArray()) {
      const annotation = raw.context.lookup(ref);
      if (annotation instanceof PDFDict) {
        annotation.set(
          PDFName.of("Subj"),
          PDFString.of("Alex custom review subject"),
        );
      }
    }
    await writeFile(output, await raw.save());

    const reopened = await openPdfDocument(output);
    expect(
      (await reopened.annotations.readPageAnnotations(0)).map(
        (markup) => markup.kind,
      ),
    ).toEqual(["length", "polylength", "area"]);
    await reopened.close();
    await handle.close();
  });

  it("round-trips a native Cloud+ inline label without a visible leader", async () => {
    const file = await createFixturePdf();
    const handle = await openPdfDocument(file);
    const output = file.replace(/\.pdf$/i, ".inline-cloud-plus.pdf");
    const inline = createCloudPlusMarkup({
      id: "inline-cloud-plus",
      pageIndex: 0,
      cloud: {
        controlPath: [
          pdfPoint(30, 30),
          pdfPoint(30, 120),
          pdfPoint(210, 120),
          pdfPoint(210, 30),
        ],
      },
      leader: { points: [] },
      textBox: { x: 70, y: 58, width: 100, height: 34 },
      text: "Inside cloud",
    });

    await handle.writer.save(handle, [inline], "saveAs", output);
    const raw = await readRawCloudPlusAnnotations(output);
    expect(
      raw.find((annotation) => annotation.subtype === "/FreeText"),
    ).toMatchObject({
      calloutLine: [120, 75, 120, 75, 120, 75],
      color: [],
      rect: [64.5, 52.5, 175.5, 97.5],
      rectangleDifferences: [5.5, 5.5, 5.5, 5.5],
      appearanceBounds: [64.5, 52.5, 175.5, 97.5],
      appearanceMatrix: [1, 0, 0, 1, -64.5, -52.5],
    });
    const reopened = await openPdfDocument(output);
    const imported = await reopened.annotations.readPageAnnotations(0);
    expect(imported).toHaveLength(1);
    expect(imported[0]).toMatchObject({
      kind: "cloud-plus",
      leader: { points: [] },
      text: "Inside cloud",
    });

    await handle.close();
    await reopened.close();
  });

  it.each([
    {
      name: "two-point",
      points: [pdfPoint(90, 50), pdfPoint(130, 50)],
      expected: [90, 50, 110, 50, 130, 50],
    },
    {
      name: "four-point",
      points: [
        pdfPoint(90, 50),
        pdfPoint(100, 60),
        pdfPoint(115, 55),
        pdfPoint(130, 50),
      ],
      expected: [90, 50, 100, 60, 130, 50],
    },
  ])(
    "normalizes $name edited Cloud+ leaders to three native CL points",
    async ({ points, expected }) => {
      const file = await createFixturePdf();
      const handle = await openPdfDocument(file);
      const output = file.replace(
        /\.pdf$/i,
        `.cloud-plus-${points.length}.pdf`,
      );
      const cloudPlus = createCloudPlusMarkup({
        id: `cloud-plus-${points.length}`,
        pageIndex: 0,
        cloud: {
          controlPath: [
            pdfPoint(10, 10),
            pdfPoint(10, 90),
            pdfPoint(90, 90),
            pdfPoint(90, 10),
          ],
        },
        leader: { points },
        textBox: { x: 130, y: 28, width: 100, height: 44 },
        text: "Canonical leader",
      });

      await handle.writer.save(handle, [cloudPlus], "saveAs", output);
      const raw = await readRawCloudPlusAnnotations(output);
      expect(
        raw.find((annotation) => annotation.subtype === "/FreeText")
          ?.calloutLine,
      ).toEqual(expected);
      await handle.close();
    },
  );

  it.each([
    {
      name: "two-point",
      points: [pdfPoint(90, 50), pdfPoint(130, 50)],
      expected: [90, 50, 130, 50],
      appearanceSegment: "130 50 m 90 50 l",
    },
    {
      name: "four-point",
      points: [
        pdfPoint(90, 50),
        pdfPoint(100, 60),
        pdfPoint(115, 55),
        pdfPoint(130, 50),
      ],
      expected: [90, 50, 100, 60, 130, 50],
      appearanceSegment: "130 50 m 100 60 l 90 50 l",
    },
  ])(
    "normalizes $name edited Callout leaders to native CL geometry and matching AP",
    async ({ points, expected, appearanceSegment }) => {
      const file = await createFixturePdf();
      const handle = await openPdfDocument(file);
      const output = file.replace(/\.pdf$/i, `.callout-${points.length}.pdf`);
      const callout = createCalloutMarkup({
        id: `callout-${points.length}`,
        pageIndex: 0,
        leader: { points },
        textBox: { x: 130, y: 28, width: 100, height: 44 },
        text: "Canonical callout",
      });

      await handle.writer.save(handle, [callout], "saveAs", output);
      const raw = await readRawCalloutAnnotation(output);
      expect(raw.calloutLine).toEqual(expected);
      expect(raw.appearanceContent).toContain(appearanceSegment);
      if (points.length === 4)
        expect(raw.appearanceContent).not.toContain("115 55 l");
      await handle.close();
    },
  );

  it("round-trips text box inline rich text runs", async () => {
    const file = await createFixturePdf();
    const handle = await openPdfDocument(file);
    const output = file.replace(/\.pdf$/i, ".rich-text.annotated.pdf");
    const textBox = createTextBoxMarkup({
      id: "rich-text-1",
      pageIndex: 0,
      rect: { x: 20, y: 80, width: 180, height: 48 },
      text: "Normal bold italic red",
      richTextRuns: [
        { text: "Normal " },
        { text: "bold ", bold: true },
        { text: "italic ", italic: true },
        { text: "red", color: "#0080ff", fontSizePt: 14 },
      ],
      color: "#ff0000",
      borderColor: "#ff0000",
      borderWidth: 1,
      fontSizePt: 12,
    });

    await handle.writer.save(handle, [textBox], "saveAs", output);
    const reopened = await openPdfDocument(output);
    const annotations = await reopened.annotations.readPageAnnotations(0);
    const richText = annotations.find((markup) => markup.kind === "text-box");

    expect(richText).toMatchObject({
      kind: "text-box",
      text: "Normal bold italic red",
      richTextRuns: [
        { text: "Normal ", color: "#ff0000", fontSizePt: 12 },
        { text: "bold ", bold: true, color: "#ff0000", fontSizePt: 12 },
        { text: "italic ", italic: true, color: "#ff0000", fontSizePt: 12 },
        { text: "red", color: "#0080ff", fontSizePt: 14 },
      ],
    });

    await handle.close();
    await reopened.close();
  });

  it("prunes only unreachable PDF objects before save", async () => {
    const directory = await mkdtemp(join(tmpdir(), "butter-paper-pdf-prune-"));
    const source = join(directory, "source.pdf");
    const output = join(directory, "output.pdf");
    const fixture = await PDFDocument.create();
    fixture.addPage([200, 200]);
    const vendorBytes = new Uint8Array(Buffer.from("reachable-vendor-stream"));
    const vendorRef = fixture.context.register(
      fixture.context.stream(vendorBytes, {
        VendorProbe: PDFString.of("retain-byte-exact"),
      }),
    );
    fixture.catalog.set(PDFName.of("VendorReachable"), vendorRef);
    fixture.context.register(
      fixture.context.stream(new Uint8Array(1024 * 1024).fill(0x5a), {
        VendorProbe: PDFString.of("unreachable-sentinel"),
      }),
    );
    await writeFile(source, await fixture.save({ useObjectStreams: false }));
    const loadedSource = await PDFDocument.load(await readFile(source));
    expect(unreachablePdfObjects(loadedSource).length).toBeGreaterThan(0);

    const handle = await openPdfDocument(source);
    await handle.writer.save(handle, [], "saveAs", output);
    const saved = await PDFDocument.load(await readFile(output));
    expect(unreachablePdfObjects(saved)).toEqual([]);
    const savedVendor = saved.context.lookup(
      saved.catalog.get(PDFName.of("VendorReachable")),
    );
    expect(savedVendor).toBeInstanceOf(PDFRawStream);
    expect(Buffer.from((savedVendor as PDFRawStream).getContents())).toEqual(
      Buffer.from(vendorBytes),
    );
    expect(
      saved.context.enumerateIndirectObjects().some(([, object]) =>
        object instanceof PDFRawStream &&
        readPdfText(object.dict.get(PDFName.of("VendorProbe"))) ===
          "unreachable-sentinel",
      ),
    ).toBe(false);
    await handle.close();
  });

  it("imports GPUI rich text and rewrites it in the Electron format without losing editable runs", async () => {
    const file = await createGpuiRichTextFixturePdf();
    const handle = await openPdfDocument(file);
    const imported = await handle.annotations.readPageAnnotations(0);
    const native = imported.find((markup) => markup.id === "native-rich-text");
    expect(native).toMatchObject({
      kind: "text-box",
      text: gpuiRichTextMatrix.map((run) => run.text).join(""),
      fontFamily: "Arimo",
      richTextRuns: gpuiRichTextMatrix,
    });
    if (!native || native.kind !== "text-box") {
      throw new Error("Expected the GPUI rich Text Box to remain editable");
    }

    const output = file.replace(/\.pdf$/i, ".electron-edited.pdf");
    const edited = {
      ...native,
      rect: { ...native.rect, x: native.rect.x + 8 },
    };
    await handle.writer.save(handle, [edited], "saveAs", output);

    const outputBytes = await readFile(output);
    const rawPdf = await PDFDocument.load(outputBytes);
    expect(unreachablePdfObjects(rawPdf)).toEqual([]);
    const rawAnnots = rawPdf.context.lookup(
      rawPdf.getPage(0).node.Annots(),
    ) as PDFArray;
    const annotation = rawPdf.context.lookup(rawAnnots.get(0));
    expect(annotation).toBeInstanceOf(PDFDict);
    const richContent = readPdfText(
      (annotation as PDFDict).get(PDFName.of("RC")),
    );
    expect(richContent).toContain(
      'xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/"',
    );
    for (const family of ["Helvetica", "Arimo", "Roboto Mono", "Tinos"]) {
      expect(richContent).toContain(`font-family:${family}`);
    }
    expect(richContent).toContain("font-weight:bold");
    expect(richContent).toContain("font-style:italic");
    expect(richContent).toContain("font-size:13pt; color:#AA1122");
    const defaultResources = rawPdf.context.lookup(
      (annotation as PDFDict).get(PDFName.of("DR")),
    );
    const defaultFonts =
      defaultResources instanceof PDFDict
        ? rawPdf.context.lookup(defaultResources.get(PDFName.of("Font")))
        : undefined;
    const normal = readTestNormalAppearance(rawPdf, annotation as PDFDict);
    const appearanceResources = normal
      ? rawPdf.context.lookup(normal.dict.get(PDFName.of("Resources")))
      : undefined;
    const appearanceFonts =
      appearanceResources instanceof PDFDict
        ? rawPdf.context.lookup(appearanceResources.get(PDFName.of("Font")))
        : undefined;
    for (const resourceName of [
      "Helv",
      "HelvBold",
      "HelvOblique",
      "HelvBoldOblique",
      "BPArimo",
      "BPArimoBold",
      "BPArimoItalic",
      "BPArimoBoldItalic",
      "BPRobotoMono",
      "BPRobotoMonoBold",
      "BPRobotoMonoItalic",
      "BPRobotoMonoBoldItalic",
      "BPTinos",
      "BPTinosBold",
      "BPTinosItalic",
      "BPTinosBoldItalic",
    ]) {
      expect(defaultFonts).toBeInstanceOf(PDFDict);
      expect((defaultFonts as PDFDict).has(PDFName.of(resourceName))).toBe(
        true,
      );
      expect(appearanceFonts).toBeInstanceOf(PDFDict);
      expect((appearanceFonts as PDFDict).has(PDFName.of(resourceName))).toBe(
        true,
      );
    }

    const reopened = await openPdfDocument(output);
    const rewritten = (await reopened.annotations.readPageAnnotations(0)).find(
      (markup) => markup.id === native.id,
    );
    expect(rewritten).toMatchObject({
      kind: "text-box",
      text: native.text,
      rect: edited.rect,
      richTextRuns: gpuiRichTextMatrix,
    });
    const rendered = await reopened.renderPage({
      pageIndex: 0,
      scale: 2,
      renderAnnotations: true,
    });
    const context = rendered.canvas.getContext("2d");
    expect(context).not.toBeNull();
    const pixels = context!.getImageData(64, 240, 1104, 200).data;
    let inkPixels = 0;
    let blueRunPixels = 0;
    let redRunPixels = 0;
    for (let index = 0; index < pixels.length; index += 4) {
      const red = pixels[index]!;
      const green = pixels[index + 1]!;
      const blue = pixels[index + 2]!;
      const alpha = pixels[index + 3]!;
      if (alpha > 0 && (red < 235 || green < 235 || blue < 235)) inkPixels += 1;
      if (alpha > 0 && blue > red + 35 && blue > green + 35) blueRunPixels += 1;
      if (alpha > 0 && red > green + 35 && red > blue + 35) redRunPixels += 1;
    }
    expect(inkPixels).toBeGreaterThan(500);
    expect(blueRunPixels).toBeGreaterThan(50);
    expect(redRunPixels).toBeGreaterThan(50);
    if (process.env.BP_GPUI_RICH_TEXT_ELECTRON_RENDER) {
      const nodeCanvas = rendered.canvas as unknown as {
        toBuffer(mimeType: "image/png"): Buffer;
      };
      await writeFile(
        process.env.BP_GPUI_RICH_TEXT_ELECTRON_RENDER,
        nodeCanvas.toBuffer("image/png"),
      );
    }
    const secondOutput = file.replace(/\.pdf$/i, ".electron-edited-twice.pdf");
    const reopenedMarkups = await reopened.annotations.readPageAnnotations(0);
    await reopened.writer.save(
      reopened,
      reopenedMarkups,
      "saveAs",
      secondOutput,
    );
    const secondOutputBytes = await readFile(secondOutput);
    const secondRawPdf = await PDFDocument.load(secondOutputBytes);
    expect(unreachablePdfObjects(secondRawPdf)).toEqual([]);
    expect(secondOutputBytes.length).toBeLessThanOrEqual(
      outputBytes.length + 512 * 1024,
    );
    const reopenedTwice = await openPdfDocument(secondOutput);
    expect(
      (await reopenedTwice.annotations.readPageAnnotations(0)).find(
        (markup) => markup.id === native.id,
      ),
    ).toMatchObject({
      kind: "text-box",
      text: native.text,
      rect: edited.rect,
      richTextRuns: gpuiRichTextMatrix,
    });
    const requestedOutput = process.env.BP_ELECTRON_RICH_TEXT_FIXTURE_OUTPUT;
    if (requestedOutput) {
      await writeFile(requestedOutput, await readFile(secondOutput), {
        flag: "wx",
      });
    }
    await handle.close();
    await reopened.close();
    await reopenedTwice.close();
  });

  it.skipIf(!process.env.BP_NATIVE_RICH_TEXT_BRIDGE_FIXTURE)(
    "edits native-produced rich text through the current Electron writer without retaining orphan object graphs",
    async () => {
      const input = process.env.BP_NATIVE_RICH_TEXT_BRIDGE_FIXTURE!;
      const temporaryDirectory = await mkdtemp(
        join(tmpdir(), "butter-paper-rich-text-bridge-"),
      );
      const finalOutput =
        process.env.BP_ELECTRON_RICH_TEXT_BRIDGE_OUTPUT ??
        join(temporaryDirectory, "electron-rich-text-bridge-final.pdf");
      const firstOutput = finalOutput.replace(/\.pdf$/i, ".first.pdf");
      const inputBytes = await readFile(input);
      const handle = await openPdfDocument(input);
      const imported = await handle.annotations.readPageAnnotations(0);
      const richText = imported.find(
        (markup) => markup.kind === "text-box" && markup.id === "native-rich-text",
      );
      if (!richText || richText.kind !== "text-box") {
        throw new Error("Expected the native rich Text Box bridge fixture");
      }
      await handle.writer.save(
        handle,
        imported.map((markup) =>
          markup.id === richText.id
            ? { ...richText, rect: { ...richText.rect, x: richText.rect.x + 3 } }
            : markup,
        ),
        "saveAs",
        firstOutput,
      );

      const firstBytes = await readFile(firstOutput);
      const firstPdf = await PDFDocument.load(firstBytes);
      expect(unreachablePdfObjects(firstPdf)).toEqual([]);
      expect(firstBytes.length).toBeLessThanOrEqual(
        inputBytes.length + 512 * 1024,
      );
      const firstReopened = await openPdfDocument(firstOutput);
      const firstMarkups = await firstReopened.annotations.readPageAnnotations(0);
      expect(firstMarkups.find((markup) => markup.id === richText.id)).toMatchObject({
        kind: "text-box",
        rect: { ...richText.rect, x: richText.rect.x + 3 },
        richTextRuns: gpuiRichTextMatrix,
      });
      await firstReopened.writer.save(
        firstReopened,
        firstMarkups,
        "saveAs",
        finalOutput,
      );

      const finalBytes = await readFile(finalOutput);
      const finalPdf = await PDFDocument.load(finalBytes);
      expect(unreachablePdfObjects(finalPdf)).toEqual([]);
      expect(finalBytes.length).toBeLessThanOrEqual(
        firstBytes.length + 512 * 1024,
      );
      const finalReopened = await openPdfDocument(finalOutput);
      expect(
        (await finalReopened.annotations.readPageAnnotations(0)).find(
          (markup) => markup.id === richText.id,
        ),
      ).toMatchObject({
        kind: "text-box",
        rect: { ...richText.rect, x: richText.rect.x + 3 },
        richTextRuns: gpuiRichTextMatrix,
      });

      await handle.close();
      await firstReopened.close();
      await finalReopened.close();
    },
  );

  it("embeds and round-trips the compatible annotation fonts under their real family names", async () => {
    const file = await createFixturePdf();
    const handle = await openPdfDocument(file);
    const output = file.replace(/\.pdf$/i, ".compatible-fonts.annotated.pdf");
    const fonts = [
      ["Arimo", "BPArimo"],
      ["Roboto Mono", "BPRobotoMono"],
      ["Noto Sans", "BPNotoSans"],
      ["Tinos", "BPTinos"],
    ] as const;
    const markups = fonts.map(([fontId], index) => {
      const text = `${fontId} compatible text`;
      return createTextBoxMarkup({
        id: `font-${index}`,
        pageIndex: 0,
        rect: { x: 15, y: 20 + index * 32, width: 190, height: 28 },
        text,
        ...(fontId === "Arimo"
          ? {
              richTextRuns: [
                { text: "Arimo ", fontId, bold: true },
                { text: "compatible ", fontId, italic: true },
                { text: "text", fontId, bold: true, italic: true },
              ],
            }
          : {}),
        appearance: { text: { fontId } },
      });
    });

    await handle.writer.save(handle, markups, "saveAs", output);
    const rawPdf = await PDFDocument.load(await readFile(output));
    const rawAnnots = rawPdf.context.lookup(
      rawPdf.getPage(0).node.Annots(),
    ) as PDFArray;
    const annotations = rawAnnots
      .asArray()
      .map((ref) => rawPdf.context.lookup(ref))
      .filter((value): value is PDFDict => value instanceof PDFDict);
    for (const [fontId, resourceName] of fonts) {
      const annotation = annotations.find((item) =>
        readPdfText(item.get(PDFName.of("Contents")))?.startsWith(fontId),
      );
      expect(readPdfText(annotation?.get(PDFName.of("DA")))).toContain(
        `/${resourceName} 12 Tf`,
      );
      expect(readPdfText(annotation?.get(PDFName.of("DS")))).toContain(
        `font: ${fontId} 12pt`,
      );
      const defaultResources = rawPdf.context.lookup(
        annotation?.get(PDFName.of("DR")),
      );
      const defaultResourceFonts =
        defaultResources instanceof PDFDict
          ? rawPdf.context.lookup(defaultResources.get(PDFName.of("Font")))
          : undefined;
      expect(defaultResourceFonts).toBeInstanceOf(PDFDict);
      expect(
        (defaultResourceFonts as PDFDict).has(PDFName.of(resourceName)),
      ).toBe(true);
      const appearance = rawPdf.context.lookup(
        annotation?.get(PDFName.of("AP")),
      );
      const normal =
        appearance instanceof PDFDict
          ? rawPdf.context.lookup(appearance.get(PDFName.of("N")))
          : undefined;
      expect(normal).toBeInstanceOf(PDFRawStream);
      const appearanceSource = new TextDecoder().decode(
        decodePDFRawStream(normal as PDFRawStream).decode(),
      );
      const encodedText =
        appearanceSource.match(/<([0-9A-F]+)>\s+Tj/i)?.[1] ?? "";
      const glyphIds = encodedText.match(/.{4}/g) ?? [];
      expect(new Set(glyphIds).size).toBeGreaterThan(5);
      const resources =
        normal instanceof PDFRawStream
          ? rawPdf.context.lookup(normal.dict.get(PDFName.of("Resources")))
          : undefined;
      const resourceFonts =
        resources instanceof PDFDict
          ? rawPdf.context.lookup(resources.get(PDFName.of("Font")))
          : undefined;
      expect(resourceFonts).toBeInstanceOf(PDFDict);
      expect((resourceFonts as PDFDict).has(PDFName.of(resourceName))).toBe(
        true,
      );
      if (fontId === "Arimo") {
        expect(
          (resourceFonts as PDFDict).has(PDFName.of(`${resourceName}Bold`)),
        ).toBe(true);
        expect(
          (resourceFonts as PDFDict).has(PDFName.of(`${resourceName}Italic`)),
        ).toBe(true);
        expect(
          (resourceFonts as PDFDict).has(
            PDFName.of(`${resourceName}BoldItalic`),
          ),
        ).toBe(true);
      } else {
        expect(
          (resourceFonts as PDFDict).has(PDFName.of(`${resourceName}Bold`)),
        ).toBe(false);
      }
    }

    const reopened = await openPdfDocument(output);
    const imported = await reopened.annotations.readPageAnnotations(0);
    expect(
      imported
        .filter((markup) => markup.kind === "text-box")
        .map((markup) => (markup.kind === "text-box" ? markup.fontFamily : "")),
    ).toEqual(fonts.map(([fontId]) => fontId));
    await handle.close();
    await reopened.close();
  });

  it("writes Bluebeam-compatible Helvetica annotations without embedding a font program", async () => {
    const file = await createFixturePdf();
    const handle = await openPdfDocument(file);
    const output = file.replace(/\.pdf$/i, ".helvetica.annotated.pdf");
    const textBox = createTextBoxMarkup({
      id: "helvetica-default",
      pageIndex: 0,
      rect: { x: 20, y: 80, width: 180, height: 40 },
      text: "Helvetica default",
      appearance: { text: { fontId: "Helvetica" } },
    });

    await handle.writer.save(handle, [textBox], "saveAs", output);
    const rawPdf = await PDFDocument.load(await readFile(output));
    const annots = rawPdf.context.lookup(
      rawPdf.getPage(0).node.Annots(),
    ) as PDFArray;
    const annotation = rawPdf.context.lookup(annots.get(0));
    expect(annotation).toBeInstanceOf(PDFDict);
    expect(
      readPdfText((annotation as PDFDict).get(PDFName.of("DA"))),
    ).toContain("/Helv 12 Tf");
    expect(
      readPdfText((annotation as PDFDict).get(PDFName.of("DS"))),
    ).toContain("font: Helvetica 12pt");
    const defaultResources = rawPdf.context.lookup(
      (annotation as PDFDict).get(PDFName.of("DR")),
    );
    const defaultFonts =
      defaultResources instanceof PDFDict
        ? rawPdf.context.lookup(defaultResources.get(PDFName.of("Font")))
        : undefined;
    const helvetica =
      defaultFonts instanceof PDFDict
        ? rawPdf.context.lookup(defaultFonts.get(PDFName.of("Helv")))
        : undefined;
    expect(helvetica).toBeInstanceOf(PDFDict);
    expect((helvetica as PDFDict).get(PDFName.of("BaseFont"))).toEqual(
      PDFName.of("Helvetica"),
    );
    expect((helvetica as PDFDict).has(PDFName.of("FontDescriptor"))).toBe(
      false,
    );

    const reopened = await openPdfDocument(output);
    const imported = await reopened.annotations.readPageAnnotations(0);
    expect(imported).toEqual([
      expect.objectContaining({
        kind: "text-box",
        fontFamily: "Helvetica",
        appearance: expect.objectContaining({
          text: expect.objectContaining({ fontId: "Helvetica" }),
        }),
      }),
    ]);
    await handle.close();
    await reopened.close();
  });

  it("silently embeds Noto Sans when Helvetica cannot encode the text", async () => {
    const file = await createFixturePdf();
    const handle = await openPdfDocument(file);
    const output = file.replace(/\.pdf$/i, ".unicode-fallback.annotated.pdf");
    const textBox = createTextBoxMarkup({
      id: "unicode-fallback",
      pageIndex: 0,
      rect: { x: 20, y: 80, width: 180, height: 40 },
      text: "Price €10",
      appearance: { text: { fontId: "Helvetica" } },
    });

    await handle.writer.save(handle, [textBox], "saveAs", output);
    const rawPdf = await PDFDocument.load(await readFile(output));
    const annots = rawPdf.context.lookup(
      rawPdf.getPage(0).node.Annots(),
    ) as PDFArray;
    const annotation = rawPdf.context.lookup(annots.get(0));
    expect(annotation).toBeInstanceOf(PDFDict);
    expect(
      readPdfText((annotation as PDFDict).get(PDFName.of("DA"))),
    ).toContain("/BPNotoSans 12 Tf");
    expect(
      readPdfText((annotation as PDFDict).get(PDFName.of("DS"))),
    ).toContain("font: Noto Sans 12pt");
    await handle.close();
  });

  it("uses compatible fonts for callouts, dimensions, and measurement labels", async () => {
    const file = await createFixturePdf();
    const handle = await openPdfDocument(file);
    const output = file.replace(/\.pdf$/i, ".compatible-text-tools.pdf");
    const callout = createCalloutMarkup({
      id: "font-callout",
      pageIndex: 0,
      leader: {
        points: [pdfPoint(20, 20), pdfPoint(40, 40), pdfPoint(70, 70)],
      },
      textBox: { x: 80, y: 55, width: 130, height: 40 },
      text: "Tinos callout",
      appearance: { text: { fontId: "Tinos" } },
    });
    const dimension = createDimensionMarkup({
      id: "font-dimension",
      pageIndex: 0,
      start: pdfPoint(20, 120),
      end: pdfPoint(140, 120),
      dimensionLineOffset: 20,
      text: "Arimo dimension",
      appearance: { text: { fontId: "Arimo" } },
    });
    const length = createLengthMarkup({
      id: "font-length",
      pageIndex: 0,
      start: pdfPoint(20, 150),
      end: pdfPoint(140, 150),
      appearance: { text: { fontId: "Roboto Mono" } },
    });

    await handle.writer.save(
      handle,
      [callout, dimension, length],
      "saveAs",
      output,
    );
    const rawPdf = await PDFDocument.load(await readFile(output));
    const annots = rawPdf.context.lookup(
      rawPdf.getPage(0).node.Annots(),
    ) as PDFArray;
    const annotations = annots
      .asArray()
      .map((ref) => rawPdf.context.lookup(ref))
      .filter((value): value is PDFDict => value instanceof PDFDict);
    for (const [contents, resourceName, family] of [
      ["Tinos callout", "BPTinos", "Tinos"],
      ["Arimo dimension", "BPArimo", "Arimo"],
      ["Scale not set", "BPRobotoMono", "Roboto Mono"],
    ] as const) {
      const annotation = annotations.find(
        (item) => readPdfText(item.get(PDFName.of("Contents"))) === contents,
      );
      expect(annotation).toBeDefined();
      expect(readPdfText(annotation?.get(PDFName.of("DA")))).toContain(
        `/${resourceName} 12 Tf`,
      );
      expect(readPdfText(annotation?.get(PDFName.of("DS")))).toContain(
        `font: ${family} 12pt`,
      );
      const appearance = rawPdf.context.lookup(
        annotation?.get(PDFName.of("AP")),
      );
      const normal =
        appearance instanceof PDFDict
          ? rawPdf.context.lookup(appearance.get(PDFName.of("N")))
          : undefined;
      expect(normal).toBeInstanceOf(PDFRawStream);
      expect(
        new TextDecoder().decode(
          decodePDFRawStream(normal as PDFRawStream).decode(),
        ),
      ).toContain(`/${resourceName} 12 Tf`);
    }
    await handle.close();
  });

  it("preserves an untouched proprietary font and converts it only when an edit rebuilds the annotation", async () => {
    const file = await createProprietaryFontFixturePdf();
    const handle = await openPdfDocument(file);
    const imported = await handle.annotations.readPageAnnotations(0);
    const textBox = imported[0];
    expect(textBox).toMatchObject({
      kind: "text-box",
      fontFamily: "Arial",
      appearance: { text: { fontId: "Arial" } },
    });

    const untouchedOutput = file.replace(/\.pdf$/i, ".untouched.pdf");
    await handle.writer.save(handle, imported, "saveAs", untouchedOutput);
    const untouchedPdf = await PDFDocument.load(
      await readFile(untouchedOutput),
    );
    const untouchedAnnots = untouchedPdf.context.lookup(
      untouchedPdf.getPage(0).node.Annots(),
    ) as PDFArray;
    const untouched = untouchedPdf.context.lookup(untouchedAnnots.get(0));
    expect(untouched).toBeInstanceOf(PDFDict);
    expect(readPdfText((untouched as PDFDict).get(PDFName.of("DA")))).toContain(
      "/Arial 12 Tf",
    );
    expect(readPdfText((untouched as PDFDict).get(PDFName.of("DS")))).toContain(
      "font: Arial 12pt",
    );

    if (!textBox || textBox.kind !== "text-box") {
      throw new Error("Expected imported text box");
    }
    const editedOutput = file.replace(/\.pdf$/i, ".edited.pdf");
    const edited = {
      ...textBox,
      appearance: {
        ...textBox.appearance,
        opacity: 0.5,
      },
    };
    await handle.writer.save(handle, [edited], "saveAs", editedOutput);
    const editedPdf = await PDFDocument.load(await readFile(editedOutput));
    const editedAnnots = editedPdf.context.lookup(
      editedPdf.getPage(0).node.Annots(),
    ) as PDFArray;
    const rebuilt = editedPdf.context.lookup(editedAnnots.get(0));
    expect(rebuilt).toBeInstanceOf(PDFDict);
    expect(readPdfText((rebuilt as PDFDict).get(PDFName.of("DA")))).toContain(
      "/BPArimo 12 Tf",
    );
    expect(readPdfText((rebuilt as PDFDict).get(PDFName.of("DS")))).toContain(
      "font: Arimo 12pt",
    );

    const reopened = await openPdfDocument(editedOutput);
    expect(await reopened.annotations.readPageAnnotations(0)).toEqual([
      expect.objectContaining({ kind: "text-box", fontFamily: "Arimo" }),
    ]);
    await reopened.close();
    await handle.close();
  });

  it("writes and reopens non-default appearance supplied directly in markup data", async () => {
    const file = await createFixturePdf();
    const handle = await openPdfDocument(file);
    const output = file.replace(/\.pdf$/i, ".custom-appearance.annotated.pdf");
    const rectangle = createRectangleMarkup({
      id: "custom-rect",
      pageIndex: 0,
      rect: { x: 20, y: 20, width: 80, height: 40 },
      appearance: {
        stroke: { color: "#123456", widthPt: 3.25, style: "dashed" },
        fill: { color: "#abcdef1f" },
        opacity: 0.35,
      },
    });
    const textBox = createTextBoxMarkup({
      id: "custom-text",
      pageIndex: 0,
      rect: { x: 20, y: 80, width: 160, height: 60 },
      text: "One explicit line\nSecond line",
      appearance: {
        stroke: { color: "#102030", widthPt: 2 },
        text: {
          color: "#654321",
          fontId: "Helvetica",
          fontSizePt: 20,
          lineHeightPt: 25,
          align: "right",
          insetPt: 9,
        },
        opacity: 0.6,
      },
    });

    await handle.writer.save(handle, [rectangle, textBox], "saveAs", output);
    const rawPdf = await PDFDocument.load(await readFile(output));
    const rawAnnots = rawPdf.context.lookup(
      rawPdf.getPage(0).node.Annots(),
    ) as PDFArray;
    const annotations = rawAnnots
      .asArray()
      .map((ref) => rawPdf.context.lookup(ref))
      .filter((value): value is PDFDict => value instanceof PDFDict);
    const rawRectangle = annotations.find(
      (annotation) =>
        readPdfText(annotation.get(PDFName.of("NM"))) === "bp:custom-rect",
    );
    const rawTextBox = annotations.find(
      (annotation) =>
        readPdfText(annotation.get(PDFName.of("NM"))) === "bp:custom-text",
    );

    expect(readPdfNumberArray(rawRectangle?.get(PDFName.of("C")))).toEqual([
      0x12 / 255,
      0x34 / 255,
      0x56 / 255,
    ]);
    expect(readPdfNumberArray(rawRectangle?.get(PDFName.of("IC")))).toEqual([
      0xab / 255,
      0xcd / 255,
      0xef / 255,
    ]);
    expect(Number(rawRectangle?.get(PDFName.of("CA")))).toBe(0.35);
    expect(Number(rawRectangle?.get(PDFName.of("ca")))).toBeCloseTo(
      (0.35 * 31) / 255,
    );
    const rawBorderStyle = rawRectangle?.context.lookup(
      rawRectangle.get(PDFName.of("BS")),
    );
    expect(rawBorderStyle).toBeInstanceOf(PDFDict);
    expect(String((rawBorderStyle as PDFDict).get(PDFName.of("S")))).toBe("/D");
    expect(
      readPdfNumberArray((rawBorderStyle as PDFDict).get(PDFName.of("D"))),
    ).toEqual([13, 6.5]);
    expect(readPdfText(rawTextBox?.get(PDFName.of("DA")))).toBe(
      "0.3961 0.2627 0.1294 rg /Helv 20 Tf",
    );
    expect(readPdfText(rawTextBox?.get(PDFName.of("DS")))).toContain(
      "text-align:right; margin:9pt; line-height:25pt; color:#654321",
    );

    const reopened = await openPdfDocument(output);
    const markups = await reopened.annotations.readPageAnnotations(0);
    expect(
      markups.find((markup) => markup.id === rectangle.id)?.appearance,
    ).toEqual(rectangle.appearance);
    expect(
      markups.find((markup) => markup.id === textBox.id)?.appearance,
    ).toEqual(textBox.appearance);

    await handle.close();
    await reopened.close();
  });
});
