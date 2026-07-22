# butter-paper-pdfium-render-core

Minimal Rust CLI proof package for desktop PDFium migration work.

## What it does

This CLI binds to PDFium via Rust crates and prints JSON to stdout for three commands:

- `document-info --file <path>`
- `page-info --file <path> --page-index <n>`
- `render-page --file <path> --page-index <n> --width <w> --height <h> [--rotation <deg>]`

`render-page` returns a base64-encoded PNG payload so the desktop TypeScript side can decode and display it without writing a temporary image file.

## Runtime behavior

This crate uses:

- [`pdfium-render`](https://crates.io/crates/pdfium-render) for the Rust PDFium API
- [`pdfium-auto`](https://crates.io/crates/pdfium-auto) to avoid requiring a manual local PDFium installation

On first run, `pdfium-auto` may download and cache the platform PDFium shared library if one is not already available. Useful environment variables:

- `PDFIUM_LIB_PATH`: use an existing PDFium dynamic library instead of auto-download
- `PDFIUM_AUTO_CACHE_DIR`: override the default cache location used by `pdfium-auto`

If PDFium cannot be bound or the requested PDF/page is invalid, the CLI exits non-zero and still prints a JSON error object to stdout:

```json
{"error":"..."}
```

## Example usage

From this crate directory:

```bash
cargo run -- document-info --file /absolute/path/to/file.pdf
cargo run -- page-info --file /absolute/path/to/file.pdf --page-index 0
cargo run -- render-page --file /absolute/path/to/file.pdf --page-index 0 --width 1200 --height 1600
cargo run -- render-page --file /absolute/path/to/file.pdf --page-index 0 --width 1200 --height 1600 --rotation 90
```

Example successful outputs:

```json
{"pageCount":12}
```

```json
{"width":612.0,"height":792.0,"rotation":0}
```

```json
{"width":1200,"height":1600,"pngBase64":"iVBORw0KGgoAAAANSUhEUgAA..."}
```

## Integration expectation for the desktop TS side

The desktop integration can spawn this binary as a child process, pass CLI args, read stdout JSON, and decode `pngBase64` into a `Buffer` / `Uint8Array` for image transport or display.
