# butter-paper

The Butter Paper application: the `butter-paper` GUI and the
`butter-paper-pdf-worker` process that owns PDFium.

- `src/`: application shell, document workspace, annotation model and
  editors, PDF engine and worker, persistence and recovery, updater.
- `tests/`: integration tests, including GPUI window tests and the UI source
  policy (`ui_source_policy.rs`). Fixtures live in `tests/fixtures`.
- `assets/`: icons and the fonts PDF appearances embed (with their licences).
- `macos/SignatureCamera.swift`: the macOS camera signature helper, compiled
  into the app by `cargo xtask package`.
- `phone-helper/`: the phone signature helper (pinned qrcp plus an adapter
  patch and page), built by `cargo xtask phone-helper`.
  `BP_PHONE_HELPER=<built helper> python3 phone-helper/test_transfer.py`
  checks it end to end.
- `bundle/Info.plist`: the development app bundle template.
- `NOTICE.md`: the app's own third-party notes, the start of every
  package's generated THIRD_PARTY_NOTICES.md.

## Running

```sh
cargo xtask pdfium                       # prints the approved libpdfium path
BP_NATIVE_DEVELOPMENT=1 BP_PDFIUM_LIBRARY=<path> cargo run
```

`BP_GPUI_DATA_DIR=<disposable folder>` keeps settings, templates and recovery
data out of your real profile; use it for live review.

## Tests

```sh
cargo test --workspace
```

Tests marked `#[ignore]` drive the real PDFium worker:

```sh
BP_PDFIUM_LIBRARY=<path> cargo test -- --ignored
```

GPUI and GPUI Component come from Butter Paper's forks, pinned by commit in
the workspace `Cargo.toml`; change them there, not by patching copies here.
