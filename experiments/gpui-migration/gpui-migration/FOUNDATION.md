# GPUI Migration foundation

This is the active native Butter Paper migration app. Its ordinary controls
are supplied by the prepared Longbridge GPUI Component source, and its
document, PDF, annotation, persistence, and viewer code is compiled directly
into this crate. The production Electron app remains unchanged.

## Exact source pins

- Longbridge GPUI Component: `c27f5d5c8f70d534978c2f0739ad9e10d4e41eb4`
- Zed GPUI: `8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc`
- Prepared source digest: `911330d721c582c4c9ef0b409b0d3d5b17cf59f89deae75b0df21301a02d2a73`
- Reviewed local exception: `patches/gpui-component-tab-button-states.patch` adds opt-in Button colours to Outline tabs without changing default tab styles or geometry. It is separate from upstream backports.
- Rust: `1.97.1`

The preparation policy and third-party notice pin the source trees, patch,
licenses, allowed Git inputs, and prepared digest. The resolved application
graph contains one `gpui` identity and one GPUI Component identity. No legacy
gallery crate or alternate native UI is part of this app.

## PDF boundary

The app uses the pinned `pdfium-render` 0.9.4 adapter and the checksum-bound
PDFium development manifest for local development only. The manifest is
explicitly `productionApproved: false`; production redistribution requires a
separate supply, licensing, signing, and platform qualification decision.

## Reproduce the foundation checks

From this directory:

```sh
node --test tests/foundation-truth.test.mjs tests/source-preparation.test.mjs
node scripts/foundation-truth.mjs
node scripts/prepare.mjs verify
node scripts/verify-cargo-graph.mjs
DEVELOPER_DIR=/Applications/Xcode-beta.app/Contents/Developer cargo check --locked --bin gpui-migration
```
