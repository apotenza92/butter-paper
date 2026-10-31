# Edit PDF Content corpus

Real producer output for `tests/page_text_corpus.rs`,
`tests/page_text_fallback_fonts.rs` and the worker's
real-PDFium Edit PDF Content test. All three are made from files in this folder:

- `chrome.pdf`: `source.html` printed by headless Google Chrome
  (`--headless=new --no-pdf-header-footer --print-to-pdf`). Subset TrueType
  fonts with ToUnicode; a line can switch fonts mid-line.
- `libreoffice.pdf`: `source.html` converted by LibreOffice
  (`soffice --headless --convert-to pdf:writer_pdf_Export`). Each word is
  positioned separately, justified lines widen the gaps around spaces, and
  ligatures carry `/ActualText` while ToUnicode maps each ligature glyph to
  the text of its first use.
- `quartz.pdf`: `quartz.txt` printed by macOS `cupsfilter`. Monospaced lines
  padded to 80 columns, and blank lines of spaces.
- `quartz-no-modify.pdf` and `quartz-owner-password.pdf`: `quartz.pdf`
  encrypted by `qpdf --encrypt "" owner 256` (AES-256, no user password),
  the first with `--modify=none`.

- `chrome-table.pdf`: `table.html` printed by headless Chrome. Table
  borders are thin filled rectangles per cell edge, under a y-down matrix.
- `libreoffice-table.pdf`: `table.fodt` converted by LibreOffice. Borders
  are stroked full-length lines; the header row has a filled background.
- `word-table.pdf`: `word-table.fodt`, a made-up home-visit report laid out
  like a Word report (a heading block, then a ruled two-column table with
  wrapped paragraphs in its right-hand cells, two in its last), converted
  by LibreOffice 26.2 untagged:
  `soffice --headless --convert-to 'pdf:writer_pdf_Export:{"UseTaggedPDF":{"type":"boolean","value":"false"}}' word-table.fodt`.
  For `tests/page_text_untagged.rs`, `tests/table_editing.rs` and the Word
  document converter.

Regenerate a file only to add coverage; the tests assert on its text.
