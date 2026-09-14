# Right tool rail

Status: implementation and scoped functional checks complete; final comparison awaits user acceptance. Expanded properties is a distinct approved region; its first Highlight-defaults slice is in [right-properties.md](right-properties.md). Neither gate certifies every annotation-family workflow.

## Approved scope

User-approved Electron rail: x=1068,y=108,w=84,h=660 at 1152×768. Main-checkout evidence `test-results/parallel-pilot/proposed-right-rail-region.png` retains padded context x=1008,y=92,w=144,h=676. The user explicitly rejected the former horizontal strip.

The rail begins below tabs, beside the canvas toolbar. Scope includes pinned controls, General/Review/Draw/Measure groups, headings, separators, icons, selected/hover states, spacing, scrolling and resize boundary. Expanded sidebar contents and annotation-engine redesign are separate. Preserve stock focus and animations.

## Implemented behaviour

- Stock ghost Buttons and Resizable divider. Default: two columns, 32-pixel buttons, 8-pixel gaps, centred headings and semantic separators. Geometry follows interface scale.
- Release snaps to 1–8 columns. Top controls and General use at most two centred columns. One column hides headings and stacks the pinned controls in an 88-pixel band above the scrolling tools.
- Ordinary single click selects/arms; second pointer click toggles properties without re-arming. Keyboard activation remains single. Highlight uses this Button path instead of competing popover mouse-down ownership. Signature and Page Scale retain dedicated stock triggers.
- Select/Hand exclusivity and pan-without-annotation behaviour remain. Saving disables mutation and properties entry points.
- Highlight opens its migrated defaults panel. Other tools retain interim actions until their property family is reviewed; that drawer is not accepted expanded-properties parity.
- Callout and Cloud+ overlays preserve reference Type size and absolute stroke. Cloud+ uses a symmetric 36-unit SVG overscan viewport and the stock Large icon-button option: button stays 32 pixels; its 24-pixel icon canvas preserves the original 16-pixel cloud without clipping Type. Regression checks button size and row alignment.

## Current evidence

Main-checkout ignored directory: `test-results/parallel-pilot/`.

- `right-rail-before.jpg`: original horizontal strip.
- `right-rail-accepted-comparison.png`: actual Electron reference / GPUI before this pass / final GPUI rail. Same Rail-review.pdf fixture, Select, default interface size, two columns, closed properties. Crops retain tab/canvas boundaries.
- `right-rail-accepted-after.jpg`: final rebuilt resting rail.
- `right-rail-accepted-eight-columns.jpg`, `right-rail-accepted-one-column.jpg`, `right-rail-accepted-one-column-bottom.jpg`: integrated resize/overflow proof before final icon overscan correction. These verify interaction, not corrected icon pixels. Scrolling reaches Measure with top controls pinned.
- `right-rail-functional-properties.jpg`: earlier Rectangle double-click/interim-drawer proof. Live Highlight double-click and defaults controls also verified; see right-properties.md.
- Earlier `right-rail-snap.jpg`, `right-rail-signature.jpg`, `right-rail-scale.jpg`, `right-rail-pan.jpg` remain earlier-build checks, not claims that every workflow was rerun.

## Checks and next gate

- 111 library, 20 native application, 5 viewer state and 1 real Highlight control/history/scene regression pass. Includes actual divider drags, 1/8-column bounds, restoration, group geometry, short-window scrolling, pinning, double-click, reset and saving guards. Not the entire native integration suite.
- `pnpm check` and `git diff --check` pass. No fixture saves, commits, pushes, remote tracking changes or Linux/Windows execution.
- Mac lock and live resize blockers cleared. Some initial Computer Use drags did not change the window; those attempts are not proof and their cause remains unassigned.
- Orchestrate supplied icon review and initial panel implementation. Controller corrected clipping, coupled values, palette AX duplication, resize measurements and toolbar synchronisation. Cross-model adversarial review timed out without a verdict and is not counted as approval.

Next: user reviews final rail and Highlight comparisons. Then compare the next property family within the approved expanded-sidebar region. Other families and overall integration remain explicit open gates.
