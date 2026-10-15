# Adobe Unicode CMaps

Range tables for the predefined CMaps `UniGB-UTF16-H`, `UniCNS-UTF16-H`,
`UniJIS-UTF16-H` and `UniKS-UTF16-H` (the UCS2 CMaps map the same code
points), which Edit PDF Content needs to find a glyph's CID and width in fonts that
use them (`src/pdf_cmaps.rs`).

Packed by `pack.py` from Adobe's cmap-resources
(https://github.com/adobe-type-tools/cmap-resources) at `f5cf3bca7fdfeaceb77aa82847e974f2306c20b4`:
Adobe-GB1-6, Adobe-CNS1-7, Adobe-Japan1-7 and Adobe-Korea1-2. BSD-3-Clause;
the licence text is in `NOTICE.md`.
