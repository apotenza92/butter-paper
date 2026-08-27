// Keep the compatibility probe on the already-reviewed experiment worker.
// The included source is GPUI-free and is compiled against this crate's exact
// pinned dependency graph. It is not a production PDFium distribution seam.
include!("../../../gpui-gallery/src/bin/butter-paper-pdf-worker.rs");
