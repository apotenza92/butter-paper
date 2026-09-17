# Deferred work

This is a queue of retained requirements, not authorisation to execute every item. Current migration work and region approvals are in gpui-migration.md.

## Native application qualification (former #85–89)

- Reader: open multiple real fixtures; pages/thumbnails; navigation, fit, zoom, rotation and scrolling; failed-open isolation; bounded worker/resource cleanup.
- Core editing: Rectangle/Ellipse, Line/Arrow, Pen/Highlight, Text Box, Image; create/select/move/resize/properties/clipboard/history; save-close-independent-reopen; retain unknown PDF content.
- Engineering: Polyline/Polygon/Arc/Cloud/Cloud+/Callout/Dimension, Length/Polylength/Area, calibration/units/precision, snapping, Snapshot and pending Redaction; verify rotated/cropped/UserUnit pages. Pending redaction does not imply secure removal.
- Documents: multiple tabs, dirty close/quit, validated ingress, session restore, built-in/custom/imported/saved templates, clipboard, save collision/external change/worker failure/recovery and safe publication.
- Integrated acceptance: representative end-to-end journeys with public fixtures, semantic and independent raster checks, keyboard/accessibility/IME where applicable, minimum windows and resource cleanup. Existing functionality needs an evidence audit rather than a second implementation.
- Later performance qualification: choose one maintained matched Electron/GPUI protocol with correctness preflight; CPU/GPU/memory/startup/latency/leaks. Versioned v4–v7 protocols are historical inputs, not simultaneous gates.
- Linux/Windows execution, paid compute and cross-platform package qualification are deferred until separately requested. Current scope is Mac-only.

## Production integration and promotion (former #90 and #41)

Production PDFium supply/licensing/redistribution; signature security and workflows; default-PDF registration; stable/beta identity, signing/notarisation and packaging; native updater checks/scheduling and trusted N-1 handover; source promotion, phased rollout, rollback and eventual Electron retirement. Preserve current product security and format requirements in AGENTS.md. Historical rollout schedules or investment plans must be reviewed when this work starts, not silently treated as current authorisation. No installed-app mutation or public release is authorised by this backlog.

## Properties capabilities absent from the Electron implementation

The universal properties migration omits these inert reference controls instead of presenting fake wiring: line style and endpoint configuration where not already implemented natively; separate fill opacity; hatch pattern/scale/colour; typography emphasis/script, vertical alignment and automatic sizing; selected measurement configuration; image border painting; imported-annotation appearance editing. Future work needs model, rendering, history and save/reopen coverage together. Preserve equivalent capabilities that already work in GPUI. This backlog is not authorisation to expand the current migration into those backend features.

## Other product requests retained from GitHub

- #36 — PDF search: text search with result navigation/page jumps, followed eventually by scanned/image-content search.
- #37 — Agentic CLI: reliable scriptable document/PDF/annotation/edit/export capabilities shared with the app; consider an MCP interface and skill.
- #38 — Pi-backed desktop agent chat: provider sign-in and available models; optional local models via LM Studio/Ollama/llama.cpp; automatic detection and accessible setup; hardware-aware model recommendations verified with a local benchmark; consistent app tools for local/cloud models. Separate from current region migration.

All 15 formerly open issues were moved to local planning at the user's request. Their historical discussions remain available on GitHub. Closing them as not planned means the GitHub tracking objects were retired; retained product requirements above remain pending.

## Deferred Phase 3 hardware verification

- **Camera signature capture — open:** on native macOS, exercise permission handling, an actual capture, cancellation and device release after closing. Helper compilation and deterministic pipe/lifecycle checks passed; these do not prove physical-camera behaviour. Carried forward when the user requested Phase 3 closure.
- **Physical-phone signing — waived for Phase 3 by the user:** retain same-Mac transfer evidence; do not describe physical QR scanning/touch as tested.
