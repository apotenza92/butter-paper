# PDF persistence adapter

## Decision

The migration experiment uses `lopdf` 0.44.0 for a bounded annotation-writer
adapter. The dependency is pinned exactly in `Cargo.toml`, resolved with a
crates.io checksum in `Cargo.lock`, built without default features, and
licensed under MIT.

This does not replace the accepted renderer decision. PDFium remains the
candidate renderer behind the isolated PDF worker. The application-owned
`PdfPersistenceSession` seam keeps the writer independent of PDFium and GPUI.

`lopdf` is suitable for this slice because it can edit an existing PDF object
graph and write native PDF annotation dictionaries and appearance streams. It
is a pure Rust library and adds no separately installed runtime library. The
adapter does not expose `lopdf` types through its public interface.

## Proven slice

The deterministic public fixture contains one native `/Square` annotation and
one unknown `/Text` annotation with custom dictionary and appearance-stream
entries. A generated `bp-annotation-all-v1` equivalent adds the four comparison
representatives. The test performs this sequence:

1. Open the PDF and import the rectangle through the public persistence seam.
2. Record the unknown annotation without interpreting it.
3. Change rectangle geometry and appearance.
4. Append a newly created rectangle with a stable native annotation name.
5. Write and sync a same-directory temporary file, then publish a new path
   without overwriting an existing file.
6. Validate the result independently with `qpdf --check` and `pdfinfo`.
7. Reopen, save to a second new path, validate again, and reopen again.
8. Add, import, replace, and reimport these native contracts:
   `/Ink` highlight paths with Multiply blend; `/FreeText` contents, default
   appearance, and Form appearance; `/Line` with `/IT /LineDimension`, scale,
   caption, and Form appearance; and `/Square` with `/IT /SquareImage` plus a
   real DeviceRGB image XObject, alpha soft mask, and Form appearance.
9. Compare the complete canonical typed annotation collection after each
   reopen. The gate checks exact order and cardinality as well as every field;
   a matching subset cannot pass.
10. Discover every untouched annotation imported from the source. Compare each
    complete annotation dictionary and recursively resolved appearance graph,
    including stream dictionaries and bytes, after each save.
11. Compare original page content, media and crop boxes, and document metadata
    after each save.
12. Render the same fixed page-one crop at 72 DPI with Poppler's `pdftoppm`.
    The first saved crop must differ from the source, and the two saved-cycle
    crops must have identical SHA-256 identities. This is an independent visual
    oracle, not the application's PDF renderer.
13. Refuse an existing output path without changing its bytes or leaving a
    temporary file.

`PersistenceComparisonScenario::execute_with_evidence_directory()` retains
both safe-save PDFs and all three raster crops in an explicitly supplied
directory. It never replaces an existing PDF or crop. The default
`execute()` path records the same PDF, validator, and crop hashes, but removes
its temporary raster files after the report is built. The caller continues to
own the two default PDF output paths and their cleanup.

Editing a known rectangle replaces its annotation dictionary with an
allowlisted native representation and a generated Form XObject appearance.
Reviewed metadata fields are retained. Unknown annotations remain in the
existing object graph and are not rebuilt.

Run the focused gate with:

```sh
cargo test --locked --test pdf_persistence
```

The host must provide `qpdf`, Poppler's `pdfinfo`, and Poppler's `pdftoppm`.
These are independent test validators, not application runtime dependencies.

## Boundary and blocked work

This is compatibility evidence for bounded, unencrypted, indirect annotations.
The exact raster oracle proves repeatability in one pinned host's Poppler
renderer. It does not prove PDFium, PDF.js, Preview, or Acrobat parity and it
does not replace later fixed-crop Structural Similarity Index Measure (SSIM)
comparison between the two applications. This slice is not acceptance of a
production PDF writer. FreeText currently uses the
standard Helvetica Type 1 font. It does not embed or subset fonts, shape complex
text, provide fallback fonts, or prove rendering parity across PDF viewers.
Ink import supports one `/InkList` path. Exact scale, font, smoothing, and asset
semantics use reviewed Butter Paper metadata in addition to native dictionaries.
Image import currently accepts the adapter's raw DeviceRGB XObject plus soft
mask, not arbitrary compressed PNG/JPEG-derived vendor appearances.

The slice also does not cover encrypted or repaired PDFs, direct annotation
objects, incremental or signature-preserving updates, source-file replacement
races, resource limits for hostile inputs, replies and groups, callouts,
attachments, forms, or every vendor extension.

Before production use, Butter Paper must add the complete TypeScript
compatibility corpus, including real Hibbeler documents if transfer is
authorized, and define failure-safe behavior for each unsupported document.
It must also pass the six-target foundation gate, native packaging tests, and
the full dependency advisory and license policy. The current Linux test does
not prove macOS or Windows behavior.
