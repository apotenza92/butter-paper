# Application menu

## Approved contract

Approved region: 1108×32 band below title bar and above tabs; native macOS global menu, title bar and tabs excluded. Stock AppMenuBar/PopupMenu in h_8; separate native menu remains. Approved pl_1 leading inset is 4px at default scale; stock trigger padding unchanged.

Product: registration, updates/status/frequency, releases, safe Quit. File: templates, Open, Save, Save As, Save Document as Template. Edit: Undo/Redo/Cut/Copy/Paste/Select All to the pre-menu document or input exactly once. View: visibility, interface scale and fullscreen with persistence/checked state. Preserve keyboard navigation, hover switching, dismissal, focus, disabled inertness, accessible states, shortcuts, constrained containment and light/dark/zoom. Approved differences: product name, omit Electron Reload/Force Reload; production-only actions truthfully disabled.

## Current assessment

Implemented, not fully accepted. Apply ux-review.md. Development and production gates are separate.

Recorded live passes: Open/Save As; saved-template six-page round trip; first/subsequent Save clearing dirty state after canonical-root repair; Copy preserving source and Paste producing one offset duplicate; document/input editing; releases URL; safe Quit/Cancel; user-confirmed hover switching; fullscreen; visibility/relaunch/zoom; light/dark/narrow popup; Escape/outside dismissal. Native application suite: 9/9 after isolated handle repair. Recorded results are not blanket proof of the latest build.

## Open development checks

- Enabled/disabled submenu regression now passes (1/1) after correcting its borrowed MenuItem fixtures to the required OwnedMenuItem API. Component preparation was regenerated with matching patch/tree identities. Full preparation verification still stops on pre-existing shared src/lib.rs receipt drift; do not refresh that receipt without auditing the existing changes.
- Resolve accessible disabled/checked state exposure; live accessibility inspection confirms these states are absent. The pinned component does not project them, and pinned GPUI has no disabled-state helper. This needs a reviewed foundation/component change, not a visual workaround. Verify keyboard opening, focus restoration and exactly-once dispatch.
- Refresh matched region screenshots and review spacing/coupled states before final user acceptance.

## Evidence

Main checkout: test-results/gpui-migration-archive/menu-baseline contains reference-full.png and reference-menu.png. Under the same archive, menu-development contains inset-after.png, final-after-product-menu.png and constrained-view-menu.png; menu-focus/descender-fix retain earlier evidence. Archived scripts/worker receipts are historical, not active planning sources.
Current functional receipts: test-results/parallel-pilot/menu-template-roundtrip.png, menu-copy-before-paste.png, menu-copy-paste-after.png and window-harness-final.log.
Latest checks: cleanup-verified-tests.log (native application 10/10, including actual workspace hover); menu-submenu-corrected.log (component regression 1/1); cleanup-preparation-verify.log (remaining shared-source receipt drift). Latest combined captures: cleanup-final-{rest,hover,close-hover}.png. These do not replace open-popup state evidence.

## Deferred production gate

Default-PDF registration, actual updates/scheduling, signed package verification and Electron handover are not implemented by disabled entries. See backlog.md. No releases, remote settings or installed apps are changed by local development work.
