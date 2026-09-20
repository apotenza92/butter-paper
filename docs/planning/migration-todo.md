# Migration todo

Orchestrator for the GPUI migration. Status and next action only; detail, evidence paths and acceptance stay in the linked brief. Check off or delete subtasks as they close. Do not duplicate evidence here. Procedure for every review: [ux-review.md](ux-review.md).

## 1. Phase 1 shell — accepted, preserve

Checkpoint `95bfb62725cf1248bcdfb24c28796c686bdb58ae`. Detail: [gpui-migration.md](gpui-migration.md).

- [x] 1.1 Application menu — shell behaviour accepted; production actions deferred. Detail: [menu.md](menu.md).
- [ ] 1.2 Document tabs — composition accepted; latest shape/palette/close corrections await final visual acceptance. Detail: [tabs.md](tabs.md).
- [ ] 1.3 Top viewer controls — accepted; canvas-column adjacency provisional until sidebars/rails land. Detail: [viewer-controls.md](viewer-controls.md).
- [ ] 1.4 Left toolbar rail — Files icon visually accepted; narrow-window, scale, focus-ring and integration gates open. Detail: [left-rail.md](left-rail.md).
- [ ] 1.5 Left sidebar — functional gate verified; integration and constrained-window coverage open. Detail: [left-sidebar.md](left-sidebar.md).
- [ ] 1.6 Right tool rail — implementation and scoped checks complete; user acceptance open. Detail: [right-rail.md](right-rail.md).
- [ ] 1.7 Right properties — Highlight defaults visually approved; toolkit migration and remaining families open. Detail: [right-properties.md](right-properties.md).
- [ ] 1.8 Workspace integration — composition accepted; visual acceptance and physical-wheel confirmation open. Detail: [workspace-integration.md](workspace-integration.md).

## 2. Phase 2 tool matrix — Linux complete, Mac qualification open

Shared Rectangle/Ellipse, Line/Arrow, Pen/Highlight, Text Box, Polyline/Polygon, Length/Polylength/Area, Cloud, Callout, Cloud+, Dimension, Arc, snapping, Redact, Snapshot, Image, Signature, plus declared Length, measurement-text and image-opacity gaps. Detail: [right-properties.md](right-properties.md) and [gpui-migration.md](gpui-migration.md).

- [ ] 2.1 Qualify the completed matrix on Mac; Linux receipts are development-only.

## 3. Phase 3 secondary surfaces — complete with exceptions

Closed at the user's request with 166/166 Mac harness. Detail: [gpui-migration.md](gpui-migration.md). Deferred hardware: [backlog.md](backlog.md).

- [ ] 3.1 Camera capture — open: permission, actual capture, cancellation, device release on macOS.
- [x] 3.2 Physical-phone signing — waived for Phase 3; retain same-Mac transfer evidence only.

## 4. Phase 4 interaction parity — active

Reference contract and native assessment: [phase-4-interactions.md](phase-4-interactions.md). Matrix authority: [gpui-migration.md](gpui-migration.md). Work gaps in order.

- [x] 4.1 Gap 1 hover/focus/draft roles — implemented; stroke/dash/halo/handle-size geometry and visual acceptance open.
- [x] 4.2 Gap 2 hold-Space and double-tap toggle — implemented with live Mac pixel evidence; grab cursor deferred to cursor-policy slice.
- [ ] 4.3 Gap 3 one-shot reset divergence — native resets to Select where reference keeps tool armed.
- [ ] 4.4 Gap 4 Length/Dimension drag creation rejected — only two-click API works.
- [ ] 4.5 Gap 5 Escape draft coverage partial — measurement-path, arc, snapshot, length/dimension-pending.
- [ ] 4.6 Gap 6 no Enter-to-commit for Length/Dimension/Arc/Snapshot.
- [ ] 4.7 Gap 7 marquee never starts as box drag; X-only direction.
- [ ] 4.8 Gap 8 no time-based hold disambiguation; no touch/double-tap path beyond click counts.
- [ ] 4.9 Gap 9 `cancel()` drops non-domain gestures without compensating command.
- [ ] 4.10 Gap 10 audit-first items — focus-loss cancel, per-tab restore, double-click-to-properties, page-scoped Select All with locked retention, marquee-candidate hover, cursor policy.
- [ ] 4.11 Gap 11 file-drop error surfacing and busy indication unconfirmed.
- [ ] 4.12 Gap 12 fullscreen layout adaptation unobserved; traffic-light clearance and state subscription open.

Next: gaps 3–4, then 5–7, per [phase-4-interactions.md](phase-4-interactions.md). Each correction ships with a focused regression plus real Mac input/visual evidence; do not infer acceptance from green tests.

## 5. Deferred — not authorised

Production integration, native qualification, and product requests stay in [backlog.md](backlog.md). No installed-app mutation or public release is authorised by this list.
