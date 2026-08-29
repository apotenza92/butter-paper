# Production PDF rendering architecture

Status: proposed decision for issue 45
Scope: the GPUI desktop migration on macOS, Windows, and Linux, for ARM64 and x64

## Decision

Use PDFium behind a crash-isolated, application-owned worker process. Do not ship the current Poppler command-line spike as the production renderer.

The GPUI application should own a small, versioned `PdfEngine` protocol. A bundled `butter-paper-pdf-worker` process should implement that protocol with a pinned PDFium revision. The application should send document handles and requests to the worker over private interprocess communication (IPC). Rendered pixels should return in bounded shared-memory surfaces, not PNG files.

This design gives Butter Paper one rendering and extraction engine on all six release targets. It also puts native parser crashes and runaway work outside the GPUI process. PDFium has the public APIs needed for custom data loading, password handling, typed load failures, metadata, character-level text geometry, clipped rendering, external bitmap buffers, and cooperative progressive rendering. Its public headers are intended to be stable, and its license is compatible with a proprietary or MIT-licensed application when the required notices are preserved.

This is a conditional production decision. PDFium must pass the representative corpus and packaged-candidate gates below before the Electron PDF.js path is removed. The API review establishes feasibility; it does not establish equivalent visual fidelity.

## Why the current architecture needs replacement

The Electron application currently reads the complete file into main-process memory, inspects a second in-memory representation with `pdf-lib`, transfers the bytes to the renderer, copies them again, and then loads them into PDF.js. Rendering uses full-page HTML canvases. The renderer has useful queue, generation, cancellation, and cache-budget behavior, but those controls sit above multiple complete byte copies and a browser canvas allocation.

There is also a behavior split before rendering starts. `pdf-lib` does not decrypt encrypted PDFs. Its documented `ignoreEncryption` option only suppresses the load error and does not decrypt the content. Because the current inspection call does not use that option, an encrypted document fails before PDF.js can handle it. The migration therefore needs one explicit contract for password-required, bad-password, unsupported-security, malformed, and repaired-document outcomes.

The existing Poppler spike proves that GPUI can invoke a native renderer. It does not prove a production architecture. It invokes `pdfinfo` to parse English command output and launches a new `pdftoppm` process for each raster request. Each render reparses the file, writes a complete PNG to disk, then makes the application decode and upload that PNG. Cancellation is a fixed timeout and process kill. The spike has no structured text contract, no process memory budget, a 4096-pixel width cap, and a cache under the application bundle's `Contents/Resources` directory. The bundle script copies a local Homebrew Poppler tree and applies ad-hoc deep signing. These choices are suitable for a disposable benchmark only.

## Options considered

| Option | Technical fit | License and distribution | Decision |
| --- | --- | --- | --- |
| PDFium in an isolated worker | Strong rendering, text, metadata, password, custom loader, clipping, and progressive-render APIs. One engine can serve every supported operating system. | BSD-style terms plus third-party notices. Butter Paper must pin, build, audit, and package its exact native dependencies. | **Select, subject to corpus qualification.** |
| MuPDF in an isolated worker | Excellent low-level design. It has custom allocation and stores, cooperative abort cookies, reusable display lists, structured text, passwords, and documented multi-thread rules. | AGPL-3.0-or-later or a commercial license. Shipping it in this MIT project would require an incompatible relicensing decision or procurement. | Keep as the strongest commercial-license fallback. |
| Poppler command-line subprocesses | Good renderer coverage. Process isolation and hard termination are useful. | Poppler is GPL, not LGPL. Bundling requires GPL compliance and source obligations. Whether a private protocol remains a separate work is fact-specific; a richer protocol or shared memory makes that boundary less comfortable. Cross-platform binary packaging is also application-owned. | Keep only as a research oracle or short-lived spike. |
| Poppler library binding | Rich C++/GLib/Qt APIs, without command encoding and disk PNG overhead. | Direct library integration has a clear GPL conflict with the current product license. Internal Poppler APIs are unstable. | Reject. |
| Pure Rust renderer, currently Hayro | Attractive memory-safe implementation language and permissive license. | Hayro documents missing encryption, blend/isolation behavior, knockout groups, and color-key masks. It describes itself as experimental and says performance is not yet a focus. | Monitor; not production-ready for fidelity-sensitive documents. |
| Platform frameworks | Apple PDFKit and Windows.Data.Pdf can render and open password-protected documents on their native platforms. | No common Linux implementation, different behavior by platform, and Windows.Data.Pdf has a much narrower document/text surface. | Reject as the primary engine. |
| Commercial cross-platform SDK | Can transfer compatibility work and support risk to a vendor. Nutrient advertises cross-platform viewing, encrypted documents, text, and a PDFium-derived renderer. | Commercial terms, binary dependency, vendor roadmap, and integration constraints need procurement and legal review. | Procurement fallback if maintaining PDFium is too expensive. |

## Production boundary

### Application-owned interface

Keep the rest of the application independent of PDFium. The owned Rust interface should use stable Butter Paper concepts and typed results, not expose PDFium handles or constants.

A minimal versioned protocol is:

```text
Open(session_id, inherited_source_handle, optional_password)
  -> document_info(page_count, metadata, permissions, security,
                   xref_reconstructed)

PageInfo(session_id, page_index)
  -> media_box, crop_box, rotation, display_size

TextPage(session_id, page_index)
  -> normalized_utf8, characters[], ranges[], quads[]

Render(job_id, session_id, page_index, transform, clip,
       pixel_size, annotation_mode)
  -> shared_surface(handle, width, height, stride, pixel_format)

Cancel(job_id)
ReleaseSurface(surface_id)
Close(session_id)
```

The text response needs stable Butter Paper indices. PDFium exposes per-character Unicode values and geometry, while `FPDFText_GetText()` is UCS-2 and omits characters that have no representation. The adapter must therefore define normalization, replacement-character, ligature, combining-mark, and line-order behavior. Search and selection must use those same indices.

Errors should be explicit protocol values:

- `password_required`
- `bad_password`
- `unsupported_security`
- `malformed_document`
- `repaired_document`
- `page_error`
- `cancelled`
- `limit_exceeded`
- `worker_crashed`

PDFium distinguishes format, password, security, and page errors. It can also report whether it reconstructed an invalid cross-reference table. The adapter should preserve this information instead of reducing every failure to “could not open PDF.” A repaired document can be viewed, but the application should warn before any operation that could overwrite or normalize it.

### Process and file boundary

Run PDFium in `butter-paper-pdf-worker`, not in the GPUI process. PDFium's public header states that its APIs are not thread-safe. Each worker must serialize PDFium calls on one actor thread. Parallel rendering should come from a small, bounded number of worker processes, not concurrent calls into one PDFium library instance.

The parent should open the document and pass an inherited read-only file descriptor or Windows handle. Do not send an arbitrary path for the worker to reopen. Back that handle with `FPDF_LoadCustomDocument()` so PDFium can request byte ranges without a complete-file copy. The worker must have no network access and the narrowest practical filesystem access.

Send a password only in an IPC message. Never put a password in command-line arguments, environment variables, logs, crash metadata, or cache keys. Clear the temporary password buffer after the open attempt.

Treat malformed PDFs as untrusted input. If the worker exits, the parent should invalidate its sessions and shared surfaces, report `worker_crashed`, and start a clean worker for the next request. Repeated crashes for the same file should stop automatic retries.

### Pixels, zoom, and thumbnails

Render directly into a validated BGRA shared-memory buffer. The parent allocates or approves the allocation, and the worker validates width, height, stride, integer overflow, and total bytes before PDFium writes into it. The GPU upload should consume that surface directly. Do not encode, write, read, and decode PNGs in the hot path.

Use `FPDF_RenderPageBitmapWithMatrix()` with a device-space clip. At high zoom, render visible tiles, normally 1024–2048 pixels per side, plus a small prefetch margin. Never allocate a full-page surface at the maximum zoom. Use the same render request for thumbnails at a low scale and with a distinct cache class. Preserve the existing separation between visible-page, bitmap, and thumbnail budgets.

The initial renderer should make annotation rendering an explicit option. This prevents a hidden behavior change between flattened PDF annotations, Butter Paper annotations, and interactive form appearances.

### Cancellation and resource control

Use PDFium's progressive rendering API for long renders. Its pause callback allows the worker to yield, observe `Cancel(job_id)`, and stop calling the continuation function. The worker must call `FPDF_RenderPage_Close()` after completion or cancellation as required by the API.

Cooperative cancellation is not a hard safety boundary. The parent must also enforce a deadline and resident-memory ceiling. If a worker ignores cancellation, exceeds its budget, or stops responding, terminate that worker and invalidate its outstanding surfaces. Limit the number of workers, active documents, queued jobs, per-request pixels, and total shared-memory bytes.

Preserve the current application-level generation identifiers. Late results from a closed document, changed viewport, or superseded zoom must be discarded even if native cancellation loses a race. Preserve interactive queue priority so visible tiles and direct user actions stay ahead of thumbnail prefetch.

### Writing and annotations

Do not silently make the renderer responsible for saves. Keep the current annotation and document-write path behind a separate interface during the first GPUI renderer migration. Port or replace that writer as a separate decision with import/export and round-trip tests. PDFium editing APIs can be evaluated later, but renderer selection alone does not authorize a save-format change.

## Packaging and supply chain

Build and ship the exact same worker contract for:

- macOS ARM64 and x64
- Windows ARM64 and x64
- Linux ARM64 and x64

Pin a PDFium source revision and record its Chromium toolchain inputs. Prefer a reproducible, application-owned build with V8 and XFA disabled unless the corpus demonstrates a product requirement for them. Generate a software bill of materials, retain the PDFium license and all applicable third-party notices, and publish the corresponding binary hashes and build provenance with each package.

Community PDFium binaries cover the six target combinations and can accelerate a prototype. They are explicitly not affiliated with Google or Foxit. Production may use them only after the project audits their provenance, pins immutable artifacts, verifies attestations and checksums, and accepts that supplier. A self-built pinned revision gives Butter Paper the clearer long-term supply-chain boundary. Never download a renderer at application runtime and never fall back to a system PDF library.

Package and verify the complete loader closure:

- On macOS, embed and sign the helper and all dynamic libraries as nested code before signing and notarizing the application. Verify both architectures and hardened-runtime launch behavior.
- On Windows, install the matching helper and DLLs next to the application in the Electron Builder artifact. Verify the ARM64 and x64 dependency closure on clean systems.
- On Linux, use controlled `$ORIGIN` loader paths and include the required libraries in AppImage, DEB, and RPM outputs. Do not depend on a distribution Poppler or PDFium package.

The packaged-candidate smoke test must confirm the helper architecture, signature where applicable, dependency hashes, notices, startup, open, render, text extraction, password behavior, malformed-file recovery, worker restart, and cleanup. A successful development build is not packaging proof.

## Poppler spike disposition

The Poppler subprocess spike is useful for three limited purposes:

1. Prove that the GPUI viewport can display native-rendered pixels.
2. Provide a second rasterization result for selected corpus comparisons.
3. Measure the cost of process startup and disk image transfer against the shared-memory design.

It is not a production candidate in its present form. Even if legal review accepts an executable boundary as separate-program aggregation, Butter Paper would still need to ship Poppler's license, source obligations, notices, and exact transitive binaries. The current text-command protocol also gives up the advantages that could justify that cost.

Do not expand the spike into a custom long-lived Poppler server. That would add engineering investment while making the GPL boundary more integrated and still leave Butter Paper responsible for six native distributions. If PDFium fails the corpus, evaluate commercially licensed MuPDF or a commercial SDK before revisiting Poppler.

## Qualification gate

Issue 46's public corpus and private Hibbeler sample must drive the renderer decision. Establish a fixed current-PDF.js baseline before changing the default. Test both development runtimes and packaged candidates.

The gate must cover:

- page geometry, crop boxes, rotation, page count, document metadata, and permissions;
- raster differences for embedded and substituted fonts, transparency, blend modes, patterns, gradients, image masks, ICC/CMYK color, JPX/JBIG2 images, annotations, and forms;
- Unicode text, ligatures, combining marks, right-to-left and vertical text, reading order, search ranges, and selection quads;
- unencrypted files, user and owner passwords, a missing password, a wrong password, unsupported security handlers, and permission flags;
- truncated files, damaged cross-reference tables, repaired files, malformed object streams, and deterministic error classification;
- thumbnails, resize, rotation, continuous scroll, rapid page changes, maximum zoom with tiled rendering, cancellation, and stale completion rejection;
- adversarial dimensions, integer overflow, image decompression bombs, worker memory ceilings, forced worker crashes, restart, and repeated-crash suppression;
- macOS, Windows, and Linux on ARM64 and x64, using the actual packaged helper and libraries.

Define numerical thresholds before evaluation: pixel-difference tolerance and allowed affected area, first-visible-page latency, cached-page latency, text/geometry mismatches, cancellation latency, peak worker resident memory, and maximum shared-memory use. Issues 47 and 51 can set the product performance and regression budgets. Any known divergence needs a reviewed corpus fixture and an explicit product decision.

Keep the Electron PDF.js implementation as a reversible migration fallback until the PDFium worker passes this gate and the annotation/save path passes its existing round-trip tests.

## Primary sources

- PDFium project, build model, supported platform instructions, public-header stability, and test suites: [PDFium README](https://pdfium.googlesource.com/pdfium/+/refs/heads/main/README.md) and [getting started](https://pdfium.googlesource.com/pdfium/+/refs/heads/main/docs/getting-started.md)
- PDFium loading, typed errors, thread-safety warning, custom file access, cross-reference repair status, bitmap buffers, and clipped rendering: [`fpdfview.h`](https://pdfium.googlesource.com/pdfium/+/refs/heads/main/public/fpdfview.h)
- PDFium character text and geometry: [`fpdf_text.h`](https://pdfium.googlesource.com/pdfium/+/refs/heads/main/public/fpdf_text.h)
- PDFium metadata: [`fpdf_doc.h`](https://pdfium.googlesource.com/pdfium/+/refs/heads/main/public/fpdf_doc.h)
- PDFium cooperative rendering: [`fpdf_progressive.h`](https://pdfium.googlesource.com/pdfium/+/refs/heads/main/public/fpdf_progressive.h)
- PDFium redistribution terms: [PDFium license](https://pdfium.googlesource.com/pdfium/+/refs/heads/main/LICENSE)
- Rust wrapper deployment and serialization behavior: [`pdfium-render` README](https://github.com/ajrcarey/pdfium-render/blob/master/README.md), [license](https://github.com/ajrcarey/pdfium-render/blob/master/LICENSE.md), and [open thread-safety concern](https://github.com/ajrcarey/pdfium-render/issues/262)
- Community binary targets and provenance: [`pdfium-binaries`](https://github.com/bblanchon/pdfium-binaries)
- MuPDF threading, stores, and document ownership: [MuPDF C overview](https://mupdf.readthedocs.io/en/latest/reference/c/overview.html)
- MuPDF cancellation and reusable display lists: [`fz_cookie`](https://mupdf.readthedocs.io/en/latest/_static/generated/c/html/structfz__cookie.html) and [display-list API](https://mupdf.readthedocs.io/en/1.28.0/_static/generated/c/html/display-list_8h.html)
- MuPDF password, metadata, and structured text: [`pdf/document.h`](https://mupdf.readthedocs.io/en/latest/_static/generated/c/html/pdf_2document_8h_source.html), [document API](https://mupdf.readthedocs.io/en/1.28.0/_static/generated/c/html/fitz_2document_8h.html), and [structured-text API](https://mupdf.readthedocs.io/en/1.28.0/_static/generated/c/html/structured-text_8h.html)
- MuPDF license and commercial option: [MuPDF repository](https://github.com/ArtifexSoftware/mupdf) and [official releases and licensing notice](https://mupdf.com/releases)
- Poppler API and GPL statement: [Poppler maintainer mirror README](https://github.com/tsdgeos/poppler_mirror/blob/master/README.md) and [Poppler project](https://poppler.freedesktop.org/)
- Poppler command behavior: [`pdftoppm` manual](https://github.com/tsdgeos/poppler_mirror/blob/master/utils/pdftoppm.1)
- GPL executable-boundary considerations: [GNU GPL FAQ on aggregation](https://www.gnu.org/licenses/gpl-faq.en.html#MereAggregation)
- Hayro limitations and maturity: [Hayro crate documentation](https://docs.rs/hayro/latest/hayro/) and [Hayro repository](https://github.com/LaurenzV/hayro)
- Platform-specific alternatives: [Apple PDFKit](https://developer.apple.com/documentation/pdfkit), [`PDFDocument`](https://developer.apple.com/documentation/pdfkit/pdfdocument), and [Windows.Data.Pdf `PdfDocument`](https://learn.microsoft.com/en-us/uwp/api/windows.data.pdf.pdfdocument)
- Commercial fallback capabilities: [Nutrient viewing SDK](https://www.nutrient.io/sdk/solutions/viewing/)
- Current encrypted-file limitation: [`pdf-lib` encryption handling](https://github.com/Hopding/pdf-lib#encryption-handling)
