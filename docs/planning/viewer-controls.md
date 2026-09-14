# Top viewer controls

## Approved contract

Reference crop x=46,y=108,w=1022,h=46: zoom, fit, page modes and existing navigation using stock controls and closest standard typography/spacing. Exclude tabs, side rails and canvas. Preserve domain rendering, actual command effects, shortcuts and per-document state; no custom controls/animations.

Apply ux-review.md to selected/disabled/open/focus states, icons, padding, borders, centring and constrained overflow. Final toolbar belongs within the canvas column between sidebars/rails. Current full-width allocation is provisional, not integrated parity.

## Current assessment

User-directed removal: the active workspace now constructs the zoom/page-mode toolbar without CAD View or its settings; its CAD control creation, subscription and event handler are removed. No document schema or lower-level layout compatibility data was deleted. Historical CAD component tests below are not an active UI requirement. The actual workspace regression verifies both CAD buttons/group absent and Continuous/Single Page present. Mac build, focused template/workspace checks, four source checks and `pnpm check` pass. Live Mac accessibility and final screenshot confirm removal and retained canvas-column centring. Comparison: main checkout `test-results/tab-split-review/comparison.png`; toolbar evidence only, not blanket regional acceptance.

Integrated candidate; acceptance open. Standard medium controls, token height/spacing, themed bottom border, centring within assigned region.

Recorded live passes: zoom/rendered Fit Page; per-document fit/zoom; five double-click mappings (percentage→100%, Fit Width→Continuous, Fit Page→Single, Continuous→Fit Width, Single→Fit Page); CAD disables fit and disabled Fit Width has no effect; CAD count A=11/B=10/return A=11 after state-sync fix; Escape. Five entity tests pass. After isolated test-window repair, 9/10 window tests pass, including CAD input regression.

## Open findings

- Narrow layout repaired using standard 4px horizontal content insets instead of 8px, preserving control sizes and the 480px assertion. Viewer window suite now 10/10, including representative narrow widths.
- Live keyboard selection now verified: open Continuous menu, Down, Return selects Mousewheel Zoom. Wheel over canvas changes 130% to 159% with CAD remaining off; Fit Width restores 130%. The earlier CAD activation is not used as passing evidence.
- Verify narrow-window reachability and coupled states after fixes.
- Final canvas-column adjacency/centring awaits sidebars and rails.

## Evidence

Main checkout: test-results/parallel-pilot/reference-review-viewer.png, viewer-after-review-viewer.png and window-harness-final.log. Reference/candidate differ in crop width and thumbnail visibility/fit zoom; not matched final-parity proof. Latest combined full-window captures also show the viewer row. Refresh deliberate matched region evidence before acceptance. Superseded platform-panic notes do not override newer test results.
Latest test receipt: cleanup-verified-tests.log (10/10). Current combined screenshots: cleanup-final-{rest,hover,close-hover}.png. Full canvas-column adjacency remains provisional until sidebars/rails exist.
