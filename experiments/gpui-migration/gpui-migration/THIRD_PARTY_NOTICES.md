# Third-party provenance for GPUI Migration

This notice covers the isolated dependency-preparation experiment. A packaged
candidate must generate and verify its own complete notice bundle from its
exact final lockfile.

## Page thumbnails icon

`assets/icons/page-thumbnails.svg` uses Lucide Files from the Electron
reference's lucide-react 1.8.0. Paths are unchanged; stroke width matches
PagesRailIcon's 1.5px absolute stroke at 16px. See assets/icons/LICENSE-lucide.txt.
This application asset supplements the stock GPUI Component icons without
changing the prepared dependency tree.

The sidebar action assets `thumbnail-scale.svg`, `thumbnail-rotate-left.svg`
and `thumbnail-rotate-right.svg` use the unchanged ScanLine, RotateCcw and
RotateCw paths from the same lucide-react version, with the standard 2-unit
stroke. The same ISC notice applies.

## Longbridge GPUI Component

- Revision: `c27f5d5c8f70d534978c2f0739ad9e10d4e41eb4`
- Git tree: `027dd3ea35614ddd365ac352987047c190ae051f`
- License: Apache-2.0
- License file SHA-256:
  `d1b0449e5478c574ba4f686c2656df7fe77d66821a61f8b6ed3378a58ed9a811`
- Local change: the tracked preparation patch pins the Zed dependencies and
  the dormant `psm` patch, and removes `profiler` and `runtime_shaders` from
  the component workspace dependency requests.
- Upstream bug-fix backport: [button/tab: Fix descenders being hidden
  (#2921)](https://github.com/longbridge/gpui-kit/pull/2921), commit
  `20f8a4502b001fca85a9d7e6718b37cb78053238`. The exact Button and Tab
  label changes retain single-line ellipsis without clipping glyph descenders;
  they do not change font size, line-height, or control dimensions.
- Approved local bug fix: `PopupMenu::with_menu_items` preserves the source
  submenu's disabled flag on the stock submenu item. This uses the existing
  disabled rendering without changing styling, and prevents disabled submenus
  from opening through pointer selection or keyboard navigation.
  This correction is local, not an upstream backport.

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

## Electron product icon geometry

- Name: `lucide-react`
- Version: `1.8.0`
- Repository lock integrity:
  `sha512-WuvlsjngSk7TnTBJ1hsCy3ql9V9VOdcPkd3PKcSmM34vJD8KG6molxz7m7zbYFgICwsanQWmJ13JlYs4Zp7Arw==`
- License: ISC
- Installed reviewed license SHA-256:
  `b495047bd93a9b06913511076f504daba17d5bbeb3e0650f3bb53a4220329c57`

The experiment-owned canvas paths for MoveHorizontal, Expand,
RectangleVertical, ZoomIn, and ZoomOut reproduce the 24 px geometry used by the
shipping Electron toolbar. This product dependency is separate from the
Longbridge Lucide 0.546.0 component-asset evidence above.

## PDF annotation fonts

The native PDF appearance writer embeds application-owned copies of the same
annotation-font files pinned by the Electron workspace. The package wrapper
code is MIT licensed and the font files are licensed under OFL-1.1. The macOS
bundle carries the family-specific OFL notices and the Expo Google Fonts MIT
notice from `assets/fonts`.

- `@expo-google-fonts/arimo` 0.4.3: Regular
  `88cc899855d30f9c779b73c3319d0e077b098f927187aafce11ca029add707a6`,
  Bold `ec96ac9fdd94766f66f6140c1375bf011577558f88d43f76b3b59307d2a6bca3`,
  Italic `6bd2c6a6fa87f4566cf0a92c5834884597acbd7be9719e550837865ac4167a42`,
  Bold Italic `b42b19c10bf5d526f00068303a643282c3df55a79c6fa2eea80b4a078a4a705a`.
- `@expo-google-fonts/roboto-mono` 0.4.2: Regular
  `e4bf45837847d4abfa5a502b807a83393aa138727bdb9ea9c93fb17d53208d95`,
  Bold `92649d819cc4c20480b23a47aaa111769924eb31c66b6ecdcf6f573662558c83`,
  Italic `82629b0b8a721ce1a79942f68b39438512b406dad2a4a7476c94b72e7794b2f4`,
  Bold Italic `5bc2918abfd7399aff52e0a7ee22c97d544601f7ae8ff3b49ad176497acff1ed`.
- `@expo-google-fonts/tinos` 0.4.2: Regular
  `924ef269e73da94c1803ff68877f5d998dd16605c5ffad82ee181034f8a1fffe`,
  Bold `576a19b5dc026cafe6ae8fde4c849588dae6475cf5d912cebdf2827388c43ad9`,
  Italic `4a52de5bcf70e37bd71949f8a85302b75795a1943aedffbbee4f2db7744e1c81`,
  Bold Italic `10c90ef7896d758923c06a1485d7824cdb457466da0bf7d42f81c5228931281b`.

The exact source paths, byte sizes, hashes, package versions and licence
receipts are enforced by `source-preparation-policy.json` and its deterministic
source-preparation tests.

## GPUI Migration application

- Package: `butter-paper-gpui-migration` 0.0.27
- License: MIT
- Feature policy: default features are disabled; the app uses the prepared
  Longbridge GPUI Component source and the pinned Zed GPUI graph directly.
- The PDF worker and document-domain modules are local application modules.
  This does not fork or patch either GPUI upstream.

## pdfium-render and development PDFium

- Wrapper: `pdfium-render` 0.9.4
- Wrapper revision: `6cee8b9a3951832ac0ff62ce4c32800278001cb8`
- Wrapper feature: `pdfium_7881`
- Development binary supplier record:
  `../gpui-migration/pdfium-development-binaries.json`
- Development binary release: `chromium/7881`
- Linux x86_64 archive SHA-256:
  `1470e21b8b4a3b4ad7f85684e2da11d94f3b69a86d81dee11b9b6709d927ac1d`

The binary manifest marks these community builds `productionApproved: false`.
This slice does not download, embed, redistribute, package, or approve a
PDFium binary. A shipping build, notices, signing, and redistribution review
remain separate blocked gates.
