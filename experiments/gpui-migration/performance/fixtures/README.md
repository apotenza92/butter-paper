# Deterministic public fixture oracles

These tracked specifications define public, synthetic inputs for identical
Electron and GPUI annotation journeys. They contain no Hibbeler material or
other private document content.

Generate the disposable PDFs and expanded oracle files under the ignored
performance results directory:

```sh
node experiments/gpui-migration/performance/fixture-oracle.mjs generate \
  --output experiments/gpui-migration/performance/results/public-fixtures-v1
node experiments/gpui-migration/performance/fixture-oracle.mjs validate \
  --output experiments/gpui-migration/performance/results/public-fixtures-v1
```

Each fixture produces four base artifacts:

- `<fixture-id>.pdf`: deterministic base document;
- `<fixture-id>.commands.json`: implementation-neutral commands in PDF points
  with a bottom-left origin;
- `<fixture-id>.expected.json`: canonical final annotation state; and
- `<fixture-id>.crops.json`: fixed PDF-space visual crop oracles.

`bp-annotation-all-v1` and `bp-coordinate-space-v1` also produce native-PDF
annotation oracles. The bundle contains the locked `bp-image-checker-v1.png`
512 by 384 pixel asset used by the all-annotation image appearance stream.

`fixture-index.json` records the byte size and SHA-256 digest of every output.
The same digests are locked in each tracked fixture specification, so
`validate` rejects generator drift and modified artifacts.

## Current fixtures

`bp-rectangle-v1` is a one-page rectangle vertical slice. Its fixed commands
create, select, move, east-resize, restyle, undo, redo, and verify one stable
annotation ID. Its appearance crop and empty control crop give both runners
the same visual regions.

`bp-annotation-density-v1` expands to 1,000 stable rectangle-create commands
over 100 pages. Page 1 is empty, page 2 contains 100 annotations, pages 3–20
contain 10 each, and pages 21–100 contain 9 each. This supplies empty, dense,
and typical page crops without checking a large generated JSON or PDF into
Git.

`bp-single-page-v1` is the one-page shell and cold-open control.
`bp-multi-page-v1` is the 100-page navigation and cache control.

`bp-engineering-sheet-v1` is a deterministic 22 by 17 inch vector sheet. Its
small PDF isolates the high-zoom rendering path from pathological file parsing.
At 1600% and scale factor 1, its 1584 by 1224 point page expands to 25,344 by
19,584 pixels, which reaches the gallery's 4096-pixel visible-tiling boundary.
The fixture oracle also caps the generated PDF at 16 KiB.

`bp-annotation-all-v1` contains native `Square`, `Ink`, `FreeText`, `Line`, and
image-backed `Square` annotations. It also contains an untouched `Text`
annotation with locked custom dictionary and appearance-stream probes. Page 2
has a different media box, crop box, and 90-degree rotation. The semantic,
native-PDF, and visual-crop oracles all have locked SHA-256 values.

`bp-coordinate-space-v1` is the one-page coordinate-space control. Its page
combines media box `[0 0 360 240]`, crop box `[18 24 342 216]`, 90-degree
rotation, and `/UserUnit 2`. Its deterministic PDF SHA-256 is
`dc450b09b502f23518ed361986d9a939ed6b9c2dc1fdb6890af30fae4b253a7d`.

## Evidence boundary

All inputs are deterministic public data. No Hibbeler material is downloaded,
generated, or transferred. Validation checks every locked artifact, then uses
`qpdf` and `pdfinfo` as independent PDF readers. Validation reports an explicit
`BLOCKED` result if either reader is unavailable. These fixtures qualify the
input and oracle boundary; application save/reopen behavior still needs to pass
the separate two-cycle persistence journey.
