# Form filling corpus

Real producer output for the form filling tests in `src/pdf_forms.rs` and
the worker's real-PDFium form test:

- `libreoffice.pdf`: `source.html` converted by LibreOffice
  (`soffice --headless --infilter="HTML (StarWriter)" --convert-to
  'pdf:writer_pdf_Export:{"ExportFormFields":{"type":"boolean","value":"true"}}'`).
  Text fields (one multiline, one with `/MaxLen`), a radio group with an
  inherited value, a check box, a combo box and a multiple-selection list
  box. Its text fields have no appearances and the form sets
  `/NeedAppearances`, and each widget keeps its fonts in its own `/DR`.

Regenerate the file only to add coverage; the tests assert on its fields.
