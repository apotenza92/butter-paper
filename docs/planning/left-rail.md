# Left toolbar rail

## Approved scope

User approved the blue rail in test-results/parallel-pilot/proposed-left-regions-labelled.png (main checkout). Reference bounds x=0,y=108,w=46,h=660; expanded thumbnail sidebar is comparison context, not part of this migration. Rail begins below tabs and reaches the window bottom. Its right separator has one owner.

Electron source uses a 32px toggle, 8px rail insets and a 48px nominal rail; screenshot dimensions reflect interface scale. Use stock medium GPUI Button with ghost/selected state, standard spacing and semantic border/background tokens. Following user feedback, the glyph now uses the exact Electron Lucide Files paths and matching stroke weight as an embedded application SVG; it replaces the approximate Copy icon without changing the prepared component library. Stock Button and Icon render it. No custom control or animation. Page Thumbnails is both tooltip and accessible name, with expanded state matching the existing panel.

## Behaviour and ownership

DocumentWorkspace owns panel visibility. Pointer and keyboard activation toggle the existing thumbnail panel once. The rail remains when the panel closes; the viewer occupies the remaining column. Empty/loading/error-without-document states retain a disabled rail. Switching documents preserves the shell's visibility choice. Thumbnail contents, styling, page actions and resizing redesign remain out of scope.

## Current assessment

User visually accepted the corrected Files icon and requested moving to the next area. This records visual acceptance only; open verification and integration checks remain below.

User visually accepted the rail layout, then requested exact Page Thumbnails icon matching. The icon correction is implemented and verified in the current Mac bundle: folded-corner Files glyph renders and pointer activation still exposes the panel with toggle state on. Asset regression 1/1, Mac build and pnpm check pass. Evidence in the main checkout: test-results/parallel-pilot/page-thumbnails-icon-comparison.png (3× detail), page-thumbnails-icon-after-full.png and bp-rail-icon-{tests,build,repo-check}.log. Remaining gates below are unchanged; the earlier Copy-icon difference is superseded.

Implemented in the actual loaded and empty workspace render paths. Standard medium Button, selected styling and explicit toggled accessibility state; no upstream patch. Shell rail stays below tabs and outside the viewer column. The existing resizable thumbnail panel starts hidden and is shown/hidden without replacing document state. No sidebar redesign.

## Verification

- Pass: native application suite 12/12, including new current-workspace tests for 48px rail / 32px target at default test scale, stable bounds across pointer toggles, existing thumbnail panel visibility and empty-state inertness. Mac build, cargo check and pnpm check pass.
- Pass, final isolated Mac build: pointer opens panel; normal Tab navigation then Return closes it; Space reopens it. Accessibility tree shows Page Thumbnails as a toggle and on/off follows the actual list. Tooltip observed. The separate expanded attribute is supplied but not independently exposed by this tool's tree text.
- Visual review: rail begins below tabs, extends to window bottom, has a single right separator and a stock stacked-pages icon. Captures taken with pointer moved to title bar. GPUI before/after use the same disposable PDF and light theme; Electron uses a different fixture with loading thumbnails, so comparison proves chrome only. Stock icon differs from the reference's page glyph detail.
- Open: narrow-window and interface-scale coverage, keyboard focus-ring clarity, loading/error transitions and document-switch retention checks. Empty-state disabled semantics need native accessibility verification; deterministic inertness alone does not prove them.
- Deferred: expanded sidebar header/content/actions/resize review and final viewer centring between expanded sidebars/rails. Existing thumbnail panel remains below the legacy tool row; this is explicitly not sidebar parity. Existing menu/tab gaps remain in their briefs. User acceptance of the implemented rail is pending.

## Evidence

Main checkout, test-results/parallel-pilot: left-rail-comparison.png, left-rail-before-full.png, left-rail-final-{closed,open}-full.png, bp-left-rail-final-tests.log, bp-left-rail-final-build.log and bp-left-rail-repo-check.log. Comparison annotations are not application UI. Reproduction uses an isolated bundle with separate app ID and disposable data/TMPDIR; no installed app or reference source modified.
