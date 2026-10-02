# PDFium

Butter Paper ships PDFium chromium/7881 (source
`91b9d569b34be4f38eed7b3c49b227356c3aadad`), built without V8 or XFA with
the two patches in this folder:

- `pdfium-production-shared-library.patch`: builds PDFium as one shared
  library.
- `pdfium-production-local-deps.patch`: the dependency policy for the build.

## How releases get it

The human-approved build (workflow run 36525856484) is published once as the
immutable `pdfium-7881` GitHub release: one library and one notices file per
target, plus `approval-evidence.tar.gz` (approval manifest, SBOMs, provenance,
GN arguments). `xtask/pdfium.json` pins every file by SHA-256, and
`cargo xtask pdfium` / `cargo xtask package` download and verify them.
Development uses the same library (`cargo xtask pdfium` prints its path).

## Upgrading PDFium

The build and approval workflows were retired with the clean rebuild (0.1.0).
They are in git history at commit `da1562a`:
`.github/workflows/build-gpui-pdfium-production.yml`,
`.github/workflows/approve-gpui-pdfium-production.yml` and their scripts
under `crates/butter-paper/scripts/` (`prepare-pdfium-production-candidate`,
`approve-pdfium-production-candidate`, `aggregate-pdfium-approvals`,
`pdfium-review-kit`). To upgrade:

1. Restore those files on a branch, update the source revision, API build
   and `pdfium-render` feature, and run the build workflow, then the
   approval workflow.
2. Publish the approved files as a new `pdfium-<build>` release (draft, check
   every size, then publish), as `pdfium-7881` was.
3. Point `xtask/pdfium.json` at it with the new digests, bump `pdfium-render`
   if its feature changes, and update `crates/butter-paper/NOTICE.md`.
4. Run the release workflow's dry run (Actions → Release → Run workflow).
