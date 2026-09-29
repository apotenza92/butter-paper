import { mkdir, mkdtemp, rm, symlink, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { describe, expect, it } from "vitest";
import {
  assessRichTextOutputAmplification,
  assertCoordinateState,
  assertEllipseState,
  assertInkState,
  assertMediaState,
  assertRedactState,
  assertRichTextState,
  assertSameManifest,
  captureManifest,
  parseArguments,
} from "../../scripts/electron-gpui-compat/rotated-media.mjs";

describe("Electron to GPUI rotated-media compatibility harness", () => {
  it("requires explicit reference, seed and output paths without accepting duplicates", () => {
    expect(
      parseArguments([
        "--electron-reference",
        "/reference",
        "--seed",
        "/seed.pdf",
        "--ellipse-seed",
        "/ellipse.pdf",
        "--ink-seed",
        "/ink.pdf",
        "--redact-seed",
        "/redact.pdf",
        "--rich-text-seed",
        "/rich-text.pdf",
        "--coordinate-seed",
        "/coordinate.pdf",
        "--output-dir",
        "/output",
        "--developer-dir",
        "/Xcode.app/Contents/Developer",
      ]),
    ).toEqual({
      electronReference: "/reference",
      seed: "/seed.pdf",
      ellipseSeed: "/ellipse.pdf",
      inkSeed: "/ink.pdf",
      redactSeed: "/redact.pdf",
      richTextSeed: "/rich-text.pdf",
      coordinateSeed: "/coordinate.pdf",
      outputDirectory: "/output",
      developerDirectory: "/Xcode.app/Contents/Developer",
    });
    expect(() => parseArguments(["--seed", "/seed.pdf"])).toThrow(
      "--electron-reference is required",
    );
    expect(() =>
      parseArguments([
        "--electron-reference",
        "/one",
        "--electron-reference",
        "/two",
        "--seed",
        "/seed.pdf",
        "--output-dir",
        "/output",
      ]),
    ).toThrow("--electron-reference may be provided only once");
  });

  it("binds sorted file bytes and rejects mutation", async () => {
    const root = await mkdtemp(join(tmpdir(), "bp-rotated-media-manifest-"));
    try {
      await mkdir(join(root, "nested"));
      await writeFile(join(root, "z.txt"), "z");
      await writeFile(join(root, "nested/a.txt"), "a");
      const before = await captureManifest(root, ["z.txt", "nested"]);
      expect(before.files.map((entry) => entry.file)).toEqual([
        "nested/a.txt",
        "z.txt",
      ]);
      assertSameManifest(
        before,
        await captureManifest(root, ["nested", "z.txt"]),
        "fixture",
      );
      await writeFile(join(root, "nested/a.txt"), "changed");
      const changed = await captureManifest(root, ["nested", "z.txt"]);
      expect(() => assertSameManifest(before, changed, "fixture")).toThrow(
        "fixture changed during the isolated compatibility run",
      );
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });

  it("rejects symlinked reference entries", async () => {
    const root = await mkdtemp(join(tmpdir(), "bp-rotated-media-symlink-"));
    try {
      await writeFile(join(root, "target.txt"), "target");
      await symlink(join(root, "target.txt"), join(root, "alias.txt"));
      await expect(captureManifest(root, ["alias.txt"])).rejects.toThrow(
        "Reference entry must not be a symlink",
      );
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });

  it("requires stable media identity, payload, presentation and aspect-lock semantics", () => {
    const state = [
      {
        id: "native-rotated-image",
        kind: "image",
        rect: { x: 10, y: 20, width: 96, height: 60 },
        rotation: 30,
        opacity: 0.65,
        locked: false,
        aspectRatioLocked: false,
        mimeType: "image/png",
        dataSha256: "a".repeat(64),
      },
      {
        id: "native-rotated-snapshot",
        kind: "snapshot",
        rect: { x: 30, y: 40, width: 96, height: 60 },
        rotation: 30,
        opacity: 0.65,
        locked: false,
        mimeType: "image/png",
        dataSha256: "a".repeat(64),
      },
    ];
    expect(() => assertMediaState(state, false)).not.toThrow();
    expect(() => assertMediaState([...state].reverse(), false)).toThrow(
      "rotated media order or identity changed",
    );
    expect(() =>
      assertMediaState(
        [state[0], { ...state[1], dataSha256: "b".repeat(64) }],
        false,
      ),
    ).toThrow("Image and Snapshot payloads diverged");
    expect(() => assertMediaState(state, true)).toThrow(
      "Image aspect-lock state changed",
    );
  });

  it("requires the rotated and legacy Ellipse semantics", () => {
    const state = [
      {
        id: "legacy-ellipse",
        kind: "ellipse",
        rect: { x: 40, y: 50, width: 100, height: 60 },
        rotation: 15,
        opacity: 1,
        locked: false,
        stroke: { color: "#ff0000", widthPt: 1, style: "solid" },
      },
      {
        id: "native-rotated-ellipse",
        kind: "ellipse",
        rect: { x: 324, y: 200, width: 200, height: 110 },
        rotation: 30,
        opacity: 0.7,
        locked: false,
        stroke: { color: "#3366cc", widthPt: 2 },
        fill: { color: "#cce6ff" },
      },
    ];
    expect(() => assertEllipseState(state)).not.toThrow();
    expect(() =>
      assertEllipseState([state[0], { ...state[1], rotation: 31 }]),
    ).toThrow("native Ellipse rotation changed");
  });

  it("requires ordered Highlight and Pen semantics plus raw downgrade state", () => {
    const state = [
      {
        id: "compat-highlight",
        kind: "highlight",
        paths: [
          [
            { x: 6, y: 40 },
            { x: 80, y: 40 },
            { x: 6, y: 40 },
          ],
          [
            { x: 42, y: 6 },
            { x: 42, y: 82 },
          ],
        ],
        color: "#ffcc00",
        widthPt: 12,
        opacity: 0.35,
        locked: false,
        blendMode: "multiply",
        hasAppearance: false,
        hasCanonicalPointBits: false,
      },
      {
        id: "compat-pen",
        kind: "pen",
        paths: [
          [
            { x: 110, y: 40 },
            { x: 145, y: 70 },
            { x: 180, y: 40 },
          ],
          [
            { x: 135, y: 25 },
            { x: 155, y: 85 },
          ],
        ],
        color: "#1f6feb",
        widthPt: 3.25,
        opacity: 0.8,
        locked: true,
        blendMode: "normal",
        smoothCurves: true,
        hasAppearance: false,
        hasCanonicalPointBits: false,
      },
    ];
    expect(() =>
      assertInkState(state, {
        hasAppearance: false,
        hasCanonicalPointBits: false,
      }),
    ).not.toThrow();
    expect(() =>
      assertInkState([state[0], { ...state[1], locked: false }], {
        hasAppearance: false,
        hasCanonicalPointBits: false,
      }),
    ).toThrow("Pen lock changed");
  });

  it("requires pending Redact semantics, raw geometry and unchanged page content", () => {
    const state = {
      markups: [
        {
          id: "compat-redact",
          kind: "redact",
          rect: { x: 20, y: 130, width: 90, height: 20 },
          redactionColor: "#102030",
          overlayText: "CONFIDENTIAL",
          locked: false,
        },
      ],
      contentSha256: "c".repeat(64),
      raw: {
        subtype: "/Redact",
        subject: "Redaction",
        contents: "Marked for redaction",
        rect: [20, 130, 110, 150],
        quadPoints: [20, 150, 110, 150, 20, 130, 110, 130],
        interiorColor: [0x10 / 255, 0x20 / 255, 0x30 / 255],
        overlayText: "CONFIDENTIAL",
        flags: 4,
        hasAppearance: false,
      },
    };
    expect(() => assertRedactState(state, false)).not.toThrow();
    expect(() =>
      assertRedactState(
        { ...state, raw: { ...state.raw, hasAppearance: true } },
        false,
      ),
    ).toThrow("pending Redact must not acquire an appearance");
    expect(() =>
      assertRedactState(
        {
          ...state,
          markups: [{ ...state.markups[0], locked: true }],
          raw: { ...state.raw, flags: 132 },
        },
        true,
      ),
    ).not.toThrow();
    expect(() =>
      assertRedactState(
        {
          ...state,
          raw: {
            ...state.raw,
            quadPoints: [20, 130, 110, 130, 20, 150, 110, 150],
          },
        },
        false,
      ),
    ).toThrow("pending Redact /QuadPoints changed");
  });

  it("requires inherited crop/rotation, UserUnit, calibration and opaque coordinate sentinels", () => {
    const state = {
      markups: [
        {
          id: "coordinate-rectangle",
          kind: "rectangle",
          rect: { x: 80, y: 140, width: 100, height: 60 },
          locked: true,
        },
        {
          id: "coordinate-length",
          kind: "length",
          start: { x: 100, y: 500 },
          end: { x: 300, y: 500 },
          locked: false,
        },
      ],
      page: {
        index: 0,
        width: 1_200,
        height: 800,
        rotation: 90,
        viewBox: { x: 50, y: 100, width: 400, height: 600 },
        userUnit: 2,
      },
      pageContentSha256: "a".repeat(64),
      raw: {
        mediaBox: [10, 20, 610, 820],
        cropBox: [50, 100, 450, 700],
        rotation: 90,
        userUnit: 2,
        pageOwnsMediaBox: false,
        pageOwnsCropBox: false,
        pageOwnsRotation: false,
        pageScale: {
          pageIndex: 0,
          source: "calibrated",
          name: "Coordinate 1 m",
          pdfUnits: "in",
          realUnits: "m",
          scaleX: 0.01,
          scaleY: 0.01,
          precision: { mode: "decimal", value: 0.01 },
        },
        lengthLine: [100, 500, 300, 500],
        vendor: {
          subtype: "/Stamp",
          subject: "Independent vendor annotation",
          contents: "Preserve this opaque annotation",
          rect: [350, 150, 390, 190],
          probe: "coordinate-space-sentinel",
        },
      },
    };
    expect(() => assertCoordinateState(state, true)).not.toThrow();
    expect(() =>
      assertCoordinateState(
        { ...state, page: { ...state.page, userUnit: 1 } },
        true,
      ),
    ).toThrow();
    expect(() =>
      assertCoordinateState(
        {
          ...state,
          raw: {
            ...state.raw,
            lengthLine: [200, 100, 500, 300],
          },
        },
        true,
      ),
    ).toThrow("coordinate Length");
  });

  it("requires editable rich Text Box runs plus raw resources, appearance and page content", () => {
    const families = ["Helvetica", "Arimo", "Roboto Mono", "Tinos"];
    const richTextRuns = families.flatMap((fontId, familyIndex) => [
      {
        text: `${fontId} regular | `,
        fontId,
        color: "#2255aa",
        fontSizePt: 11,
      },
      { text: "bold | ", fontId, bold: true, color: "#aa1122", fontSizePt: 11 },
      {
        text: "italic | ",
        fontId,
        italic: true,
        color: "#2255aa",
        fontSizePt: 13,
      },
      {
        text: `bold italic${familyIndex < families.length - 1 ? "\n" : ""}`,
        fontId,
        bold: true,
        italic: true,
        color: "#aa1122",
        fontSizePt: 13,
      },
    ]);
    const fonts = [
      "Helv",
      "HelvBold",
      "HelvBoldOblique",
      "HelvOblique",
      "BPArimo",
      "BPArimoBold",
      "BPArimoBoldItalic",
      "BPArimoItalic",
      "BPRobotoMono",
      "BPRobotoMonoBold",
      "BPRobotoMonoBoldItalic",
      "BPRobotoMonoItalic",
      "BPTinos",
      "BPTinosBold",
      "BPTinosBoldItalic",
      "BPTinosItalic",
    ].sort();
    const text = richTextRuns.map((run) => run.text).join("");
    const richContent =
      '<?xml version="1.0"?><body xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/" ' +
      'xfa:contentType="text/html" xfa:APIVersion="BluebeamPDFRevu:2018" xfa:spec="2.2.0" ' +
      'style="font: Arimo 12pt; text-align:left; margin:5pt; line-height:13.8pt; color:#172B4D" ' +
      'xmlns="http://www.w3.org/1999/xhtml"><p>' +
      richTextRuns
        .map((run) => {
          const styles = [
            `font-family:${run.fontId}`,
            `font-size:${run.fontSizePt}pt`,
            `color:${run.color.toUpperCase()}`,
            ...("bold" in run && run.bold ? ["font-weight:bold"] : []),
            ...("italic" in run && run.italic ? ["font-style:italic"] : []),
          ];
          return `<span style="${styles.join("; ")}">${run.text.replaceAll("\n", "<br/>")}</span>`;
        })
        .join("") +
      "</p></body>";
    const fontObjects = fonts.map((name, index) => ({
      name,
      object: `${index + 1} 0 R`,
      subtype: name.startsWith("Helv") ? "/Type1" : "/Type0",
      baseFont: name.startsWith("Helv")
        ? `/${name.replace("Helv", "Helvetica").replace("BoldOblique", "-BoldOblique").replace("Bold", "-Bold").replace("Oblique", "-Oblique")}`
        : `/${name
            .replace(/^BP/, "")
            .replace(/BoldItalic$/, "-BoldItalic")
            .replace(/Italic$/, "-Italic")
            .replace(/Bold$/, "-Bold")}-Regular`,
      embedded: !name.startsWith("Helv"),
      ...(!name.startsWith("Helv")
        ? { programSha256: `${index % 10}`.repeat(64) }
        : {}),
    }));
    const appearanceFontSelections = [
      "BPArimo@12",
      ...richTextRuns.map((run) => {
        const prefix =
          run.fontId === "Helvetica"
            ? "Helv"
            : `BP${run.fontId.replaceAll(" ", "")}`;
        const italicSuffix =
          "italic" in run && run.italic
            ? run.fontId === "Helvetica"
              ? "Oblique"
              : "Italic"
            : "";
        return `${prefix}${"bold" in run && run.bold ? "Bold" : ""}${italicSuffix}@${run.fontSizePt}`;
      }),
    ];
    const state = {
      markups: [
        {
          id: "native-rich-text",
          kind: "text-box",
          rect: { x: 32, y: 80, width: 552, height: 100 },
          text,
          fontFamily: "Arimo",
          fontSizePt: 12,
          opacity: 1,
          locked: false,
          richTextRuns,
        },
      ],
      contentSha256: "d".repeat(64),
      raw: {
        subtype: "/FreeText",
        subject: "Text Box",
        contents: text,
        rect: [32, 80, 584, 180],
        richContent,
        defaultAppearance: "0 0 0 rg /BPArimo 12 Tf",
        defaultStyle: "font: Arimo 12pt",
        flags: 4,
        opacity: 1,
        defaultFonts: fonts,
        appearanceFonts: fonts,
        defaultFontObjects: fontObjects,
        appearanceFontObjects: fontObjects,
        indirectObjectCount: 97,
        appearanceBounds: [32, 80, 584, 180],
        appearanceTextBoundary: {
          beginText: 1,
          endText: 1,
          textPaints: 16,
          fontSelections: appearanceFontSelections,
        },
        appearanceBytes: 512,
        appearanceSha256: "e".repeat(64),
      },
    };
    expect(() => assertRichTextState(state, false, "electron")).not.toThrow();
    const nativeFonts = fonts
      .map((name) => name.replace("Italic", "Oblique"))
      .sort();
    const nativeRichContent =
      '<?xml version="1.0"?><body xmlns="http://www.w3.org/1999/xhtml"><p>' +
      richTextRuns
        .map((run) => {
          const styles = [
            `font-family:${run.fontId}`,
            `font-size:${run.fontSizePt.toFixed(6)}pt`,
            `color:${run.color.toUpperCase()}`,
            ...("bold" in run && run.bold ? ["font-weight:bold"] : []),
            ...("italic" in run && run.italic ? ["font-style:italic"] : []),
          ];
          return `<span style="${styles.join(";")}">${run.text.replaceAll("\n", "<br/>")}</span>`;
        })
        .join("") +
      "</p></body>";
    const nativeFontObjects = fontObjects
      .map((font) => ({
        ...font,
        name: font.name.replace("Italic", "Oblique"),
      }))
      .sort((left, right) => left.name.localeCompare(right.name));
    expect(() =>
      assertRichTextState(
        {
          ...state,
          raw: {
            ...state.raw,
            richContent: nativeRichContent,
            defaultFonts: nativeFonts,
            appearanceFonts: nativeFonts,
            defaultFontObjects: nativeFontObjects,
            appearanceFontObjects: nativeFontObjects,
            appearanceTextBoundary: {
              ...state.raw.appearanceTextBoundary,
              fontSelections: appearanceFontSelections
                .slice(1)
                .map((selection) => selection.replace("Italic@", "Oblique@")),
            },
          },
        },
        false,
        "native",
      ),
    ).not.toThrow();
    expect(() =>
      assertRichTextState(
        {
          ...state,
          markups: [
            { ...state.markups[0], richTextRuns: richTextRuns.slice(1) },
          ],
        },
        false,
        "electron",
      ),
    ).toThrow("rich Text Box editable runs changed");
    expect(() =>
      assertRichTextState(
        {
          ...state,
          raw: { ...state.raw, appearanceFonts: fonts.slice(1) },
        },
        false,
        "electron",
      ),
    ).toThrow("rich Text Box /AP fonts changed");
    expect(() =>
      assertRichTextState(
        {
          ...state,
          raw: {
            ...state.raw,
            richContent: richContent.replace("<p>", "<div>"),
          },
        },
        false,
        "electron",
      ),
    ).toThrow("rich Text Box /RC wrapper or ordered runs changed");
    expect(() =>
      assertRichTextState(
        {
          ...state,
          raw: {
            ...state.raw,
            appearanceFontObjects: fontObjects.map((font, index) =>
              index === 4 ? { ...font, embedded: false } : font,
            ),
          },
        },
        false,
        "electron",
      ),
    ).toThrow("lost its embedded font program");
    expect(() =>
      assertRichTextState(
        {
          ...state,
          raw: {
            ...state.raw,
            appearanceTextBoundary: {
              ...state.raw.appearanceTextBoundary,
              fontSelections: [...appearanceFontSelections].reverse(),
            },
          },
        },
        false,
        "electron",
      ),
    ).toThrow("rich Text Box /AP ordered font operators changed");
  });

  it("records per-leg output amplification, native pruning and frozen Electron blockers", () => {
    const raw = {
      indirectObjectCount: 100,
      defaultFontObjects: Array.from({ length: 16 }, () => ({})),
      appearanceFontObjects: [
        ...Array.from({ length: 4 }, () => ({ embedded: false })),
        ...Array.from({ length: 12 }, () => ({ embedded: true })),
      ],
    };
    const assessment = assessRichTextOutputAmplification(
      {
        sourceSeed: 2_342_368,
        stableFirst: 4_683_380,
        stableFinal: 4_683_380,
        nativeRichText: 3_934_604,
        electronRichTextFirst: 6_275_616,
        electronRichTextFinal: 6_275_616,
      },
      [
        ["seed", { raw }],
        ["stable-final", { raw }],
        ["native-final", { raw }],
        ["electron-final", { raw }],
      ],
    );
    expect(assessment.gate.nativePrunePassed).toBe(true);
    expect(assessment.gate.passed).toBe(false);
    expect(assessment.status).toBe("blocked");
    expect(
      assessment.legs.map(({ input, output }) => `${input}->${output}`),
    ).toEqual([
      "seed->stable-first",
      "stable-first->stable-final",
      "stable-final->native-final",
      "native-final->electron-first",
      "electron-first->electron-final",
    ]);
    expect(assessment.unresolvedBlockers.map(({ leg }) => leg)).toEqual([
      "seed->stable-first",
      "native-final->electron-first",
    ]);
    expect(assessment.resourceCounts[0]).toEqual({
      stage: "seed",
      indirectObjects: 100,
      defaultFonts: 16,
      appearanceFonts: 16,
      embeddedAppearanceFonts: 12,
    });
  });
});
