# Edit Text corpus

Real producer output for `tests/page_text_corpus.rs` and the worker's
real-PDFium Edit Text test. All three are made from files in this folder:

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

Regenerate a file only to add coverage; the tests assert on its text.
