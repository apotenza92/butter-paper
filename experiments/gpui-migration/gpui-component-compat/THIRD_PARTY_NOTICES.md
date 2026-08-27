# Third-party provenance for the compatibility probe

This notice covers the isolated dependency-preparation experiment. A packaged
candidate must generate and verify its own complete notice bundle from its
exact final lockfile.

## Longbridge GPUI Component

- Revision: `c27f5d5c8f70d534978c2f0739ad9e10d4e41eb4`
- Git tree: `027dd3ea35614ddd365ac352987047c190ae051f`
- License: Apache-2.0
- License file SHA-256:
  `d1b0449e5478c574ba4f686c2656df7fe77d66821a61f8b6ed3378a58ed9a811`
- Local change: the tracked preparation patch pins the Zed dependencies and
  the dormant `psm` patch, and removes `profiler` and `runtime_shaders` from
  the component workspace dependency requests.

## Zed GPUI

- Revision: `8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc`
- Git tree: `85eccaf309692769ec7458482ec7b39c6faf430f`
- Relevant first-party crate license: Apache-2.0
- Root `LICENSE-APACHE` SHA-256:
  `752daf2fb234ca4a1fa372c073fe127f44b7b90fd2529ae44273a64f9d53da7a`
- GPL-marked `ztracing`, `ztracing_macro`, and `zlog` source is not copied.
  The local Apache-2.0 `ztracing` package re-exports only
  `tracing::instrument`, the sole reachable API used by the reviewed GPUI and
  `sum_tree` source.

## Lucide icons

- Name: Lucide
- Version evidence: `0.546.0` in the reviewed Longbridge
  `website/bun.lock`
- Integrity evidence:
  `sha512-Z94u6fKT43lKeYHiVyvyR8fT7pwCzDu7RyMPpTvh054+xahSgj4HFQ+NmflvzdXsoAjYGdCguGaFKYuvq0ThCQ==`
- License: ISC

Longbridge attributes the embedded component icon set to Lucide. The exact
version evidence above closes the earlier unversioned notice gap for this
experiment. A future packaged candidate must include the corresponding Lucide
license text and verify the final embedded asset inventory.

## Butter Paper GPUI gallery library seam

- Source: `../gpui-gallery` in this isolated experiment
- Package: `butter-paper-gpui-gallery` 0.1.0
- License: MIT
- Feature policy: `default-features = false`; the GPUI-CE gallery and its
  development-only visual application are not in this compatibility graph.

The compatibility probe reuses the gallery's GPUI-free worker protocol and
document-domain modules. Its local worker entry includes the already-reviewed
experiment worker source. This does not fork or patch either GPUI upstream.

## pdfium-render and development PDFium

- Wrapper: `pdfium-render` 0.9.4
- Wrapper revision: `6cee8b9a3951832ac0ff62ce4c32800278001cb8`
- Wrapper feature: `pdfium_7881`
- Development binary supplier record:
  `../gpui-gallery/pdfium-development-binaries.json`
- Development binary release: `chromium/7881`
- Linux x86_64 archive SHA-256:
  `1470e21b8b4a3b4ad7f85684e2da11d94f3b69a86d81dee11b9b6709d927ac1d`

The binary manifest marks these community builds `productionApproved: false`.
This slice does not download, embed, redistribute, package, or approve a
PDFium binary. A shipping build, notices, signing, and redistribution review
remain separate blocked gates.
