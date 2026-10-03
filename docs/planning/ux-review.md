# UX review

The review procedure for native UI changes, used with the GPUI Component design and coding guides. It does not authorise redesigns or new custom components.

## Review to disprove, not confirm

After implementing, set aside the intended result and inspect the actual output as a reviewer. Ask: what in this image or interaction contradicts its stated state or our contract? Name concrete observations before offering a cause. Review neighbouring controls and boundaries too. Do not stop at confirming that the requested change is present.

Repeat this pass on the exact images being handed to the user, after the last code change. An earlier successful capture does not validate a later build. Reopen generated crops and check their provenance, contents and captions. If a contradiction is visible, flag it before handoff; recapture or fix within scope rather than presenting it as verified.

## Evidence must represent its claimed state

- Identify the actual app/build, fixture, theme, scale, window dimensions, active document, focus and pointer location. Keep reference/target conditions matched where practical; disclose mismatches and limit claims accordingly.
- A resting-state capture must not contain unexplained hover, pressed, open or focus feedback. Moving the pointer alone may leave keyboard focus; deactivating a window may change appearance. Verify the resulting state instead of assuming the input established it.
- Distinguish rest, pointer hover, keyboard focus, selected and disabled. An active tab need not be hovered; keyboard focus is not evidence of pointer hover.
- If an input tool cannot perform or reliably deliver an action, mark that check not verified. Do not relabel a focus/click capture as hover evidence.
- If a workaround places the pointer via wheel input, record both effects and verify scrolling did not change the tested geometry. A pointer covering the close glyph or its feedback cannot establish close-only hover clarity; obtain unobscured evidence or leave that check open.
- Capture transitions where the requirement is transitional. A screenshot of one endpoint cannot prove animation, focus restoration or absence of movement between states.

## Coupled states must agree

For each compound control, specify which elements should respond together. Exercise entering, moving within and leaving it, and keyboard focus entering/leaving. Inspect active and inactive instances.

- Tab example: when its hover treatment is visible, the requested close affordance and label truncation must also be visible. When neither hover nor focus applies, restore the full label and hide close. Parent highlight with missing close/truncation is contradictory evidence requiring investigation, not a pass.
- The close button's own hover/pressed feedback must distinguish closing from selecting the tab. Verify the pointer target, full accessible label, single correct effect and successor focus.
- Menu/popover triggers must agree with open state and return to the correct state after dismissal. Disabled controls must not advertise an available action through hover feedback.
- Do not infer a root cause solely from a screenshot: stale hover, wrong capture state and mismatched event ownership are hypotheses to test.

## Geometry and content

- Inventory reference grouping and control type, not only field names: record paired columns, full-width spans and slider/input pairs. A plain numeric field replacing a working slider, or a stacked layout replacing paired columns, is a structural gap, not density polish waived by general visual approval.
- Inspect numeric text at the minimum paired-column width, including units. If display precision is reduced to fit, prove that unchanged focus/blur leaves the full stored value and undo history untouched; formatting is not a domain edit.
- Inspect full hit targets and hover/focus backgrounds, not just glyphs. Check clearance from rounded corners, text/icon baselines and centring, internal leading/trailing padding, label-to-icon space, repeated button/group gaps and app/region-edge insets.
- Test short, long and truncated labels plus relevant modified/loading/error states. A hidden action must follow the approved space policy: reserved space where intended, or a stable-width overlay with truncation where intended. Reveal must not unexpectedly move neighbouring tabs/actions.
- Measure intended invariant edges and gaps. Pixel equality proves only the compared region/property. Equal control pixels cannot prove tab behaviour elsewhere; unequal pixels may reflect a state change rather than geometry drift.
- Compare font size/weight to the reference using the closest stock typography tier. Review separator presence, ownership, thickness and continuity; check whole-group centring within the correct region and adjacency to sidebars/rails.
- For state-colour parity, trace the final composed variant (including ButtonGroup or DropdownButton propagation), not just Button::new at a child call site. Compare rendered normal, hovered and selected fills against the actual reference control; shared resolver names alone do not prove the same variant or appearance.
- Include contextual crops around boundaries as well as native-scale details. Do not crop away a visible defect or label provisional region adjacency as integrated parity.
- When a region is narrowly too wide, inspect intrinsic control widths, parent gaps and nested insets before changing controls. Correct the spacing owner with standard tokens; do not shrink stock hit targets or relax the minimum-width assertion to hide the failure.
- For resizable regions, test minimum/default/maximum sizes and interface zoom after resizing. Check retained pixel sizes against rem-scaled controls, single-column wrapping, pinned controls, group centring and neighbouring boundaries; a correct default screenshot does not establish these states.
- For nested resizable groups, measure the untouched sidebar while dragging, toggling or resetting its neighbour. Verify preferred widths after narrowing then expanding the window, and distinguish constrained widths from saved preferences. A successful drag on one panel does not prove resize isolation.
- Combine maximum interface zoom with a narrow window, not only separate extremes. Check pinned rail controls and explicit column counts: fractional-pixel flex wrapping can change a two-column rail into one column even when its nominal width is correct. For automatic sidebar collapse, prove restoration and explicit-close preference separately.

## Test the implementation users actually run

- When edited annotation text outgrows its original box, inspect every line on the canvas and after save/reopen. Retained text and a correct editor value do not prove visible glyphs. Compare canvas clipping, stored geometry and exported appearance separately; the reference may itself differ between screen and PDF.
- For caption-only marquee evidence, keep both pointer endpoints outside direct-hit targets and exclude body paths and extension overhangs from the region. A selected final state can otherwise come from a direct click or non-text intersection. Verify imported typography before comparing caption bounds, and distinguish moving candidate feedback from the committed selection.
- For raster pipelines, verify asymmetric red/blue pixels at each byte-format boundary, including PDF rendering, highlight composition, image upload and Snapshot export. Greyscale or green-only fixtures cannot detect red/blue swaps; two incorrect conversions can cancel on screen while exported bytes remain wrong. Keep stored asset formats distinct from GPU upload formats.
- Trace the live app's render path before choosing regression tests. A legacy preview or standalone component suite does not prove the current workspace composition. Add a focused regression on the actual path for demonstrated state/geometry failures.
- Keyboard-input regressions must register the same workspace/application key bindings as the live app. A text field can pass a component-only typing test while tool shortcuts consume letters in production. Include ordinary text containing shortcut letters, assert the exact resulting text, and verify no picker, tool switch or document action occurred.
- For popovers over the canvas, arm a drawing tool before clicking an input or control. Place the control over the actual contained PDF page, not merely the annotation layer including its letterbox margins. Assert that the click reaches the intended control, preserves the underlying document and creates no canvas draft. A Select-only test can hide interception by global pointer handlers.
- Modal interaction tests must paint the same root dialog layer as the real application. Render the first animation frame before advancing animation time and targeting controls; owning dialog state alone does not paint its hit targets.
- For bounded scrolling overlays, exercise enough content to overflow and verify the final action becomes reachable by scrolling. A viewport rectangle inside the window is insufficient: a height limit inherited by the inner content can erase the scroll range while children still overflow.
- For nested stock components, check callback ownership before attaching handlers: tooltips or managed behaviour may already register the same callback. Exercise the composed control, not just the handler in isolation.
- Audit every control exposed inside stock overlays, including alternate tabs and alpha/transparency sliders. A working colour swatch does not prove that the picker's transparency is wired; do not silently discard exposed values. Preserve an existing domain capability or record the unsupported control explicitly.
- When a compound control changes colour and opacity together, verify one atomic domain/history edit, not two competing events. Reapply an unchanged value and check that numeric precision conversion creates no undo entry. Preserve unrelated imported fields when changing one property, including fields the current editor does not expose.
- When old tests disagree with an approved component size or composition, separate obsolete expectations from still-valid behaviour failures. Port or repair useful contracts with evidence; neither delete a failing suite nor change expected numbers just to make it green.
- Inspect accessible role, full name, selected/checked and disabled states separately from painted appearance and click inertness. A missing foundation capability remains a tracked gap, not an invitation to fake semantics with styling.
- Keep dependency preparation and runtime verification separate. A passing corrected component test does not prove all source receipts match; audit drift before refreshing identities, and preserve unrelated dirty changes. When swapping source for a red/green probe, refresh copied source modification times or otherwise force the relevant rebuild; restoring timestamps can make Cargo reuse the wrong test binary even when current bytes match. Verify the intended test names and nonzero test counts, not just exit status.

- For composed property editors, verify section bounds do not overlap and that one intended container owns scrolling. Accessibility-tree presence alone does not prove a control is visible: inspect appearance and measurement sections together, then scroll to and operate the final control.

## Handoff gate

For relevant checks record pass, fail, blocked or not run in the handoff. Separate source/build checks, runtime interaction checks, visual checks and user acceptance. Passing one category does not imply the others.

Check coupled values, not just the edited control: a stepper must update its slider and document defaults immediately; a resized fitted canvas must agree with the toolbar percentage in both resize directions. Test popup opening with native accessibility active, since headless rendering may omit that code path. Inspect complete glyph strokes against the SVG viewport, including composite overlays that deliberately extend beyond the base icon; correct source coordinates alone do not prove unclipped pixels.

For slider/input pairs, check the number during pointer-down and intermediate drag movement, not only after release. At the slider maximum, measure clearance from the painted thumb and its hover ring to the input, not merely the gap between layout boxes.

Show reference/before/after for implementation handoffs, with honest state/theme/build labels. Planning-only sessions do not invent an after. Report visible defects and unverified interactions before declaring completion; fix known in-scope defects when authorised, without redundant approval pauses.

For blended annotations, compare the live reference separately from exported PDF appearances: their caps and blend results can differ. Include reversed drawing order, separate overlapping paths and self-overlap within one path. Use raw renderer readback for exact colour assertions; desktop captures can carry a monitor colour profile and lossy encoding even when saved with a `.png` filename.

## Maintaining the checklist

When a demonstrated miss recurs or teaches a general review failure, update the narrowest rule here with a concrete observable check. Track open defects in backlog.md. Consolidate overlapping rules; do not append transcripts, dated worklogs or universal restrictions inferred from one example. Do not modify the upstream GPUI guides to encode application-specific preferences.

### Native macOS launch and Signature evidence

- Launch the isolated review `.app` through LaunchServices, with explicit disposable data/temporary directories and fixture arguments. A raw executable launch can differ in native input/repaint behaviour; compare launch paths before attributing delayed state to application code.
- Verify the launched process command, window title and created data root before interacting. Shell-prefixed environment values are not evidence that LaunchServices delivered them; when isolation depends on environment, use a uniquely identified copied review bundle with an explicit disposable launch environment or a purpose-built harness, then confirm the owned root was populated and the ordinary development root was not selected.
- Exercise Tab from the popover's first control through both Recent use/removal controls. Verify that focus remains inside the popover and the removal control appears while focused, even when the pointer is elsewhere.
- Label QR, expiry, hover and keyboard screenshots with their actual state. Cancel disposable QR sessions after capturing evidence. Reopen the saved files for contradiction review; a QR picture alone cannot establish a successful physical-phone transfer.
