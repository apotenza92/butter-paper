import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import test from "node:test";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const tool = resolve(import.meta.dirname, "fixture-oracle.mjs");

async function generateFixtureBundle() {
  const output = await mkdtemp(resolve(tmpdir(), "bp-fixture-oracle-"));
  await execFileAsync(process.execPath, [tool, "generate", "--output", output]);
  return output;
}

test("generates the public rectangle fixture and its canonical oracle", async () => {
  const output = await generateFixtureBundle();
  try {
    const commands = JSON.parse(
      await readFile(resolve(output, "bp-rectangle-v1.commands.json"), "utf8"),
    );
    const expected = JSON.parse(
      await readFile(resolve(output, "bp-rectangle-v1.expected.json"), "utf8"),
    );
    const crops = JSON.parse(
      await readFile(resolve(output, "bp-rectangle-v1.crops.json"), "utf8"),
    );
    const pdf = await readFile(resolve(output, "bp-rectangle-v1.pdf"));

    assert.equal(pdf.subarray(0, 8).toString("ascii"), "%PDF-1.7");
    assert.equal(commands.fixture_id, "bp-rectangle-v1");
    assert.equal(commands.coordinate_space, "pdf-points-bottom-left");
    assert.deepEqual(
      commands.commands.map((command) => command.command_id),
      [
        "rectangle:create:001",
        "rectangle:select:001",
        "rectangle:move:001",
        "rectangle:resize-east:001",
        "rectangle:style:001",
        "rectangle:undo-style:001",
        "rectangle:redo-style:001",
        "rectangle:verify:001",
      ],
    );
    assert.deepEqual(expected.annotations[0].bounds, {
      x: 90,
      y: 132,
      width: 210,
      height: 96,
    });
    assert.deepEqual(expected.selected_annotation_ids, ["bp-rectangle-v1:rectangle:0001"]);
    assert.deepEqual(crops.crops[0].pdf_rect, {
      x: 78,
      y: 120,
      width: 234,
      height: 120,
    });
  } finally {
    await rm(output, { recursive: true, force: true });
  }
});

test("expands the annotation-density recipe into 1,000 stable PDF-space commands", async () => {
  const output = await generateFixtureBundle();
  try {
    const commands = JSON.parse(
      await readFile(resolve(output, "bp-annotation-density-v1.commands.json"), "utf8"),
    );
    const expected = JSON.parse(
      await readFile(resolve(output, "bp-annotation-density-v1.expected.json"), "utf8"),
    );
    const crops = JSON.parse(
      await readFile(resolve(output, "bp-annotation-density-v1.crops.json"), "utf8"),
    );
    const perPage = Object.groupBy(
      expected.annotations,
      (annotation) => annotation.page_id,
    );

    assert.equal(commands.coordinate_space, "pdf-points-bottom-left");
    assert.equal(commands.commands.length, 1_000);
    assert.equal(expected.annotations.length, 1_000);
    assert.equal(perPage["bp-annotation-density-v1:page:001"], undefined);
    assert.equal(perPage["bp-annotation-density-v1:page:002"].length, 100);
    assert.equal(perPage["bp-annotation-density-v1:page:003"].length, 10);
    assert.equal(perPage["bp-annotation-density-v1:page:021"].length, 9);
    assert.equal(perPage["bp-annotation-density-v1:page:100"].length, 9);
    assert.equal(commands.commands[0].annotation_id, "bp-annotation-density-v1:p002:rectangle:0001");
    assert.deepEqual(commands.commands[0].bounds_pdf, {
      x: 48,
      y: 96,
      width: 36,
      height: 24,
    });
    assert.equal(
      commands.commands.at(-1).annotation_id,
      "bp-annotation-density-v1:p100:rectangle:0009",
    );
    assert.deepEqual(
      crops.crops.map((crop) => [crop.crop_id, crop.expected_annotation_count]),
      [
        ["density-empty-interaction-page", 0],
        ["density-100-annotation-page", 100],
        ["density-typical-page", 9],
      ],
    );
  } finally {
    await rm(output, { recursive: true, force: true });
  }
});

test("validates locked artifact hashes and rejects canonical-state drift", async () => {
  const output = await generateFixtureBundle();
  try {
    const validation = await execFileAsync(
      process.execPath,
      [tool, "validate", "--output", output],
    );
    assert.match(validation.stdout, /Validated 7 deterministic fixture oracles/);

    const expectedPath = resolve(output, "bp-rectangle-v1.expected.json");
    const expected = JSON.parse(await readFile(expectedPath, "utf8"));
    expected.annotations[0].bounds.width = 211;
    await writeFile(expectedPath, `${JSON.stringify(expected)}\n`, "utf8");

    await assert.rejects(
      execFileAsync(process.execPath, [tool, "validate", "--output", output]),
      /bp-rectangle-v1\.expected\.json: sha256 mismatch/,
    );
  } finally {
    await rm(output, { recursive: true, force: true });
  }
});

test("generates locked single-page and 100-page navigation controls", async () => {
  const output = await generateFixtureBundle();
  try {
    const index = JSON.parse(await readFile(resolve(output, "fixture-index.json"), "utf8"));
    const single = index.fixtures.find(({ fixture_id }) => fixture_id === "bp-single-page-v1");
    const multiple = index.fixtures.find(({ fixture_id }) => fixture_id === "bp-multi-page-v1");
    const singleExpected = JSON.parse(
      await readFile(resolve(output, "bp-single-page-v1.expected.json"), "utf8"),
    );
    const multipleExpected = JSON.parse(
      await readFile(resolve(output, "bp-multi-page-v1.expected.json"), "utf8"),
    );

    assert.equal(single.page_count, 1);
    assert.equal(multiple.page_count, 100);
    assert.deepEqual(singleExpected.document.page_ids, ["bp-single-page-v1:page:001"]);
    assert.equal(multipleExpected.document.page_ids.length, 100);
    assert.equal(multipleExpected.document.page_ids.at(-1), "bp-multi-page-v1:page:100");
    assert.equal(single.artifacts.pdf.sha256, "f31adeeb3f17ef180012fe707cb2f2650854305dab4b16bba34d73652b6d8fdc");
    assert.equal(multiple.artifacts.pdf.sha256, "517ebc78ee84071ce15040da05f2155ca0fe4b5d5871dc95cea1a95c97b1f57b");
  } finally {
    await rm(output, { recursive: true, force: true });
  }
});

test("generates a moderate engineering sheet that reaches tiled rendering at 1600 percent", async () => {
  const output = await generateFixtureBundle();
  try {
    const index = JSON.parse(await readFile(resolve(output, "fixture-index.json"), "utf8"));
    const fixture = index.fixtures.find(
      ({ fixture_id }) => fixture_id === "bp-engineering-sheet-v1",
    );
    const commands = JSON.parse(
      await readFile(resolve(output, "bp-engineering-sheet-v1.commands.json"), "utf8"),
    );
    const pdf = await readFile(resolve(output, "bp-engineering-sheet-v1.pdf"));

    assert.equal(fixture.page_count, 1);
    assert(pdf.length <= 16_384);
    assert.deepEqual(commands.commands[0].expected_page_size_points, {
      width: 1584,
      height: 1224,
    });
    assert.deepEqual(commands.commands[1], {
      command_id: "engineering:assert-tiling:001",
      display_scale_factor: 1,
      expected_tiled_rendering: true,
      operation: "assert-visible-tiled-rendering",
      page_id: "bp-engineering-sheet-v1:page:001",
      zoom_percent: 1600,
    });
    assert.equal(1584 * 16, 25_344);
    assert(25_344 > 4096);
  } finally {
    await rm(output, { recursive: true, force: true });
  }
});

test("generates native representative annotations, unknown probes, and the checker image", async () => {
  const output = await generateFixtureBundle();
  try {
    const index = JSON.parse(await readFile(resolve(output, "fixture-index.json"), "utf8"));
    const fixture = index.fixtures.find(({ fixture_id }) => fixture_id === "bp-annotation-all-v1");
    const native = JSON.parse(
      await readFile(resolve(output, "bp-annotation-all-v1.native.json"), "utf8"),
    );
    const expected = JSON.parse(
      await readFile(resolve(output, "bp-annotation-all-v1.expected.json"), "utf8"),
    );
    const crops = JSON.parse(
      await readFile(resolve(output, "bp-annotation-all-v1.crops.json"), "utf8"),
    );
    const checker = await readFile(resolve(output, "bp-image-checker-v1.png"));

    assert.deepEqual(native.annotations.map(({ native_name, subtype }) => [native_name, subtype]), [
      ["rectangle-1", "Square"],
      ["highlight-1", "Ink"],
      ["text-1", "FreeText"],
      ["length-1", "Line"],
      ["image-1", "Square"],
      ["unknown-1", "Text"],
    ]);
    assert.deepEqual(native.annotations.at(-1).probes, {
      dictionary: "unknown-dictionary-preserve-me",
      appearance_dictionary: "unknown-stream-preserve-me",
      appearance_content_ascii: "q 0.7 0.2 0.8 rg 0 0 24 24 re f Q\n",
    });
    assert.equal(expected.annotations.length, 6);
    assert.equal(expected.document.pages[1].rotation, 90);
    assert.deepEqual(crops.crops.map(({ crop_id }) => crop_id), [
      "all-rectangle",
      "all-highlight",
      "all-text",
      "all-length",
      "all-image",
      "all-unknown",
      "all-empty-control",
    ]);
    assert.equal(checker.subarray(1, 4).toString("ascii"), "PNG");
    assert.equal(checker.readUInt32BE(16), 512);
    assert.equal(checker.readUInt32BE(20), 384);
    assert.equal(index.assets[0].id, "bp-image-checker-v1");
    assert.equal(index.assets[0].sha256, "fcc714d1ac60ed4b88abf7297830479c7557cb9d219033e7a5a5ad4d6ec18dda");
    assert.equal(fixture.artifacts.native.sha256, "5ff7d499c7418cb541d06f96439172765c0eeeee46d83851bd78b40b678e06b0");
  } finally {
    await rm(output, { recursive: true, force: true });
  }
});

test("generates a locked CropBox, rotation, and UserUnit coordinate-space fixture", async () => {
  const output = await generateFixtureBundle();
  try {
    const index = JSON.parse(await readFile(resolve(output, "fixture-index.json"), "utf8"));
    const fixture = index.fixtures.find(({ fixture_id }) => fixture_id === "bp-coordinate-space-v1");
    const expected = JSON.parse(
      await readFile(resolve(output, "bp-coordinate-space-v1.expected.json"), "utf8"),
    );
    const pdf = await readFile(resolve(output, "bp-coordinate-space-v1.pdf"), "latin1");

    assert.equal(fixture.page_count, 1);
    assert.deepEqual(expected.document.pages[0], {
      crop_box: [18, 24, 342, 216],
      height: 240,
      page_id: "bp-coordinate-space-v1:page:001",
      rotation: 90,
      user_unit: 2,
      width: 360,
    });
    assert.match(pdf, /\/CropBox \[18 24 342 216\]/);
    assert.match(pdf, /\/Rotate 90/);
    assert.match(pdf, /\/UserUnit 2/);
  } finally {
    await rm(output, { recursive: true, force: true });
  }
});

test("independent qpdf and pdfinfo validation passes or reports an explicit blocker", async () => {
  const output = await generateFixtureBundle();
  try {
    const validation = await execFileAsync(
      process.execPath,
      [tool, "validate", "--output", output],
    );
    assert.match(validation.stdout, /qpdf and pdfinfo independently accepted 7 PDFs/);

    await assert.rejects(
      execFileAsync(process.execPath, [tool, "validate", "--output", output], {
        env: { ...process.env, PATH: "/definitely-no-pdf-validators" },
      }),
      /BLOCKED: required independent PDF validators unavailable: qpdf, pdfinfo/,
    );
  } finally {
    await rm(output, { recursive: true, force: true });
  }
});
