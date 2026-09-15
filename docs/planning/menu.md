# Application menu

## Approved contract

Approved region: 1108×32 band below title bar and above tabs; native macOS global menu, title bar and tabs excluded. Stock AppMenuBar/PopupMenu in h_8; separate native menu remains. Approved pl_1 leading inset is 4px at default scale; stock trigger padding unchanged.

Product: registration, updates/status/frequency, releases, safe Quit. File: templates, Open, Save, Save As, Save Document as Template. Edit: Undo/Redo/Cut/Copy/Paste/Select All to the pre-menu document or input exactly once. View: visibility, interface scale and fullscreen with persistence/checked state. Preserve keyboard navigation, hover switching, dismissal, focus, disabled inertness, accessible states, shortcuts, constrained containment and light/dark/zoom. Approved differences: product name, omit Electron Reload/Force Reload; production-only actions truthfully disabled.

## Current assessment

Phase 1 shell behaviour is accepted at checkpoint `95bfb62725cf1248bcdfb24c28796c686bdb58ae`. Development and production gates remain separate.

Recorded live passes: Open/Save As; saved-template six-page round trip; first/subsequent Save clearing dirty state after canonical-root repair; Copy preserving source and Paste producing one offset duplicate; document/input editing; releases URL; safe Quit/Cancel; user-confirmed hover switching; fullscreen; visibility/relaunch/zoom; light/dark/narrow popup; Escape/outside dismissal. Native application suite: 9/9 after isolated handle repair. Recorded results are not blanket proof of the latest build.

## Open development checks

- Enabled/disabled submenu regression passes (1/1) after correcting its borrowed MenuItem fixtures to the required OwnedMenuItem API. Component preparation was regenerated with matching patch/tree identities. The shared-source receipts were subsequently audited against accepted checkpoint `95bfb627`, and full preparation verification passes.
- Resolved for Phase 1: a separate checksum-locked component patch delegates the stock menu row unchanged, then projects disabled and checked-true state through AccessKit. Disabled/checked-false cannot be distinguished from an ordinary action in the pinned `OwnedMenuItem` boolean model, so no false toggled state is fabricated. Source preparation tests constrain the patch to the two menu files and reject visual/click-handler changes. The guarded native-application suite passes 25/25 after clean re-preparation.
- Deferred: production actions and native assistive-technology qualification remain separate from Phase 1 shell acceptance.

## Evidence

Main checkout: test-results/gpui-migration-archive/menu-baseline contains reference-full.png and reference-menu.png. Under the same archive, menu-development contains inset-after.png, final-after-product-menu.png and constrained-view-menu.png; menu-focus/descender-fix retain earlier evidence. Archived scripts/worker receipts are historical, not active planning sources.
Current functional receipts: test-results/parallel-pilot/menu-template-roundtrip.png, menu-copy-before-paste.png, menu-copy-paste-after.png and window-harness-final.log.
Latest Phase 1 checks: guarded native application 25/25; eight source-preparation tests; exact fresh component digest `35254d5f899bb03514766c834996cc9025f16e06ad19ffd4fcfb3e32c105dd69`; full source/shared-receipt preparation verification; and full `pnpm check`. Direct execution of the new component-local tests is blocked because the upstream component workspace lock does not match the reviewed dependency patch and `--locked` correctly refuses to rewrite it; application compilation plus checksum/scope tests are the deterministic evidence.

## Deferred production gate

Default-PDF registration, actual updates/scheduling, signed package verification and Electron handover are not implemented by disabled entries. See backlog.md. No releases, remote settings or installed apps are changed by local development work.
