# List corpus fixtures

Lists made for Butter Paper's list tests (`tests/list_corpus.rs` and the
worker's real-PDFium list test). We wrote the sources; the PDFs are their
producers' output. Regenerate a file only to add coverage: the tests assert
on its lists.

- `lo-nested.pdf`: `lo-nested.fodt` converted by LibreOffice 26.2 untagged
  (`soffice --headless --convert-to 'pdf:writer_pdf_Export:{"UseTaggedPDF":{"type":"boolean","value":"false"}}'`).
  Two three-level lists of 21 items: 1. / a. / i. with bullets mixed in,
  then 1. / a. / i. throughout, the second running onto page 2.
- `lo-mixed.pdf`: `lo-mixed.fodt`, converted the same way. A bulleted, a
  numbered and an en-dash list, between text that must stay plain: the
  headings "1. Scope" and "2. Programme", "- as noted above" at a line
  start, dates ("12. März 2024") and "1. The other" at line starts.
- `lo-docx-wordstyle.pdf`: `lo-docx-wordstyle.docx` (numbering written as
  Word writes it), converted the same way. Symbol and Wingdings bullets
  read as private-use characters (U+F0B7, U+F0A7, U+F0D8, U+F0FC) and the
  level-two "o" in Courier New's stand-in; a 21-item 1. / a. / i. list
  with right-aligned roman numerals; 1), I. and (1) lists; and plain
  "1. Introduction" headings.
- `firefox-numbering-styles.pdf`: `firefox-numbering-styles.html` printed
  by headless Firefox 154 through WebDriver (A4, 2 cm margins). One list
  per style: 1) (1) a. a) A. i. I., and `type="A"` and `type="I"` lists.
