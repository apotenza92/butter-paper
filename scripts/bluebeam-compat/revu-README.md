# Trusted Revu capture lane

`revu-capture-run.mjs` controls an already licensed Bluebeam Revu installation through Parallels Desktop's `prlctl` and guest PowerShell. It is intentionally separate from deterministic CI: it needs a trusted local Windows VM and does not save, restyle, move, or delete annotations.

Example:

```sh
node scripts/bluebeam-compat/revu-capture-run.mjs \
  --vm "Windows 11" \
  --pdf "/host/path/to/specimen.pdf" \
  --output test-results/bluebeam-compat/revu-run \
  --specimen all-tools \
  --expected-tools text-box,cloud-plus,cloud-plus \
  --inspection test-results/bluebeam-compat/revu-run/source-inspection.json \
  --select arrow:760,228
```

The input PDF normally needs to be reachable through Parallels' `\\Mac\Home` share. The runner copies it to a unique guest temporary path and verifies the Windows-side SHA-256 before Revu opens it. `--guest-pdf` can point at an existing Windows-visible path when the host path is not shared; in that mode the caller is responsible for using a disposable copy.

Each run records:

- the exact source and guest-copy SHA-256;
- Git commit, dirty state, and dirty-worktree hash;
- Windows, Revu, display, DPI, locale, theme, and installed fonts;
- semantic action timing and failures;
- an open-document capture and a final operation capture;
- image dimensions, luminance range, and bright-pixel ratio so transient black Parallels frames are retried;
- a complete operation-result scaffold from `tool-contract.json`.

`--expected-tools` may repeat a tool ID when a fixture contains multiple logical specimens, such as external and inline Cloud+. When supplied, `--inspection` is mandatory. The native subtype/intent/component validator must match the complete inspection without missing or unexpected native annotations before the evidence manifest is written.

Operation results use `evidence-captured` until a reviewer confirms the screenshot proves the Revu behavior. The runner never upgrades a coordinate click to `passed` merely because Windows accepted the input event. Save, resave, movement, reshape, resize, restyle, text/measure edit, reimport, and deletion remain explicit later operations rather than being inferred from a successful open or selection.

Generated PDFs, screenshots, manifests, action logs, and results stay under ignored `test-results/bluebeam-compat/`. Do not commit them as baselines unless the source hash and complete compatibility identity have been reviewed.

## Headless native-mutation matrix

`revu-script-matrix.mjs` uses the installed Revu `ScriptEngine.exe` for native inventory, mutation, deletion, save/reopen, structural inspection, and Butter Paper reimport. Run it only against a disposable specimen:

```sh
node scripts/bluebeam-compat/revu-script-matrix.mjs \
  --vm "Windows 11" \
  --pdf packages/pdf/tmp/pdfs/all-tools-bluebeam-compat.pdf \
  --output test-results/bluebeam-compat/revu-script-matrix
```

The runner verifies the host/guest input hash, records the exact ScriptEngine binary and version, writes one PDF per mutation class, inspects the resulting native objects, and reimports every result through `@butter-paper/pdf`. A property is `passed` only when the post-save value exactly matches the request. Revu-normalized values are `partial`; properties absent from ScriptEngine are `unsupported`. GUI-only endpoint/vertex manipulation and pixel judgment therefore remain in the capture lane rather than being overstated as headless coverage.
