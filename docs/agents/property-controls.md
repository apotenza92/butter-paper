# Property controls

Read this before adding or changing a property family. Use the gpui-component
skill and [the UI policy](ui-policy.md), and follow
[the markup contract](../planning/pdf-format.md) for stored appearance data.

Compose standard controls through the application toolkit in
`crates/butter-paper/src/property_controls.rs`. Reuse its panel, header,
number, size-stepper and slider/input layouts rather than creating a parallel
property UI system.

Keep applicability and ranges in `tool_properties.rs`, defaults in the session
adapter, and selected-object mutations in identity-checked workspace/domain
paths. Preserve the current domain value, precision and history behaviour when
formatting or laying out a control.

A new control needs a real rendering and persistence path plus per-family tests.
Adding a field or hiding a missing capability does not complete the property.
Verify the composed editor, including coupled controls, overflow and exposed
alternate settings, using [the UX checklist](../planning/ux-review.md).

## Optional settings

A properties panel never adds or removes a row when a setting changes. It
enables or disables the row. When a setting changes the kind of markup, the
rows stay the same. Show a row that the new kind cannot use as disabled.

An optional group of settings is a collapsible section. Build it with
`PropertyOptionalSection` in `property_controls.rs`. Its heading holds the
checkbox, then the title, then a chevron at the far end.

- Unchecked: the heading stays and the fields of the section show disabled.
- Checked: the fields are enabled and the section opens.
- The chevron opens or closes the section. It does not change the checkbox.
- The checkbox and the chevron are separate click targets. Each is a tab stop.
  Space and Enter press it.
- A locked markup or a save in progress disables the checkbox and the fields.
  The chevron stays active, because it changes no document.

A collapsible group with no checkbox, such as the Visual search's Options, uses
`PropertyDisclosureSection`: its heading is one control that opens and closes the
section, with a short muted summary of the values inside while it is closed.
The owner keeps the open state.

A setting that depends on another setting in the same section sits indented
under that setting. Mark it with `PropertyRow::dependent`. Disable it while
the setting that it depends on is off.

Every control family in a section needs a disabled state: colour, numeric
input, slider and input, text field, select, switch, button and read-only
value. Use the `disabled` option of the control. Dim a read-only value with
the muted foreground token.

A row that exists for one kind of markup only, such as a fill on a Polygon, is
not an optional setting. It follows the kind of markup and does not change
when the user toggles a setting.

Test each optional section. Check that checking and unchecking keeps the same
set of row ids, that only the enabled state changes, that checking opens the
section, and that Space and Enter press the checkbox and the chevron.

## Sliders act while they are dragged

A slider in a properties panel previews its value on the page every frame and
commits it once, on release. Build it with `subscribe_property_slider` in
`property_controls.rs` (or, for a slider with its own handler, the same
steps):

- On Change, set the number beside the slider, then emit
  `PropertyPreviewEvent::Show(event)` with the event that the release will
  commit. Emit `PropertyPreviewEvent::Clear` when the value is the committed
  one, as `patch_event` returns `None` then.
- On Release, emit `Clear`, then emit the usual event. The usual event makes
  the one undo step.
- Subscribe the workspace to `PropertyPreviewEvent<YourEvent>` and run the
  inspector's usual `apply_*_event` handler through `apply_property_preview`.
  The handler then edits a copy of the document for display only
  (`AnnotationAdapter::begin_property_preview`), so no history, revision,
  dirty state or recovery write happens before release.
- Add a test in `tests/live_property_sliders.rs`: the page shows the value
  before release, the number follows, one release is one undo step, and
  Escape restores the committed value.

A slider that acts on a tool default needs no preview. It sets the default on
every Change and has no undo step.

## Typed fields act as they are typed

A text or number field in a properties panel shows its value on the page
after each keystroke. It commits the value as one undo step when the field
loses focus or Enter is pressed.

- Read the event with `FieldPhase::of`. A `Change` is a `Preview`. A `Blur`
  or `PressEnter` is a `Commit`.
- Preview: when the text is a valid value, emit `PropertyPreviewEvent::Show`.
  When it is partial or out of range, such as an empty field or "."
  alone, emit `PropertyPreviewEvent::Pending`. The page keeps the last valid
  preview and no error shows.
- Commit: emit `PropertyPreviewEvent::Clear`, then the usual event. Text that
  is not valid at commit ends the preview and keeps the old rules (the field
  shows the markup's value again).
- Escape in a field, or in a slider drag, restores the markup and the fields
  to the committed values
  (`DocumentWorkspace::cancel_property_slider_drag`). The blur that follows
  commits nothing.
- A markup's Text field uses `PropertyPreviewEvent<PropertyTextEditEvent>`.
  The workspace runs the same `try_property_text_edit` for the preview and
  the commit.
- A tool-defaults field changes the default at each valid keystroke, with no
  undo step. A panel's `sync` must not rewrite a number field that already
  reads the same value (`set_number_text`), or "1." becomes "1" under the
  caret.
- A preview copies the document for each keystroke. It must not save, write
  recovery data or touch the history.

## Colours and sizes have rows of their own

A colour and a size never share a row. Name the rows after the pair:
"Line colour" and "Line width", "Font colour" and "Font size". A tool's
defaults panel uses "Colour" and "Line width" where one colour serves the
line and the text (Callout, Cloud+, Dimension). The rows stay in place when a
setting changes, as in [Optional settings](#optional-settings).

Every numeric size (line width, font size, offset, landing) and the cloud
Bump size use `PropertyStepInput`. (Where Bump size has a slider, in the Cloud+
properties and the tool defaults, the slider's number has the same − and +
buttons: `PropertySliderInput::stepper`.) It is the stock gpui-component number input with − and +
buttons, in the full width of the value column. Wire it so:

- Build its `InputState` for the field's `min` and `max`. Do not call
  `.step(..)`.
- Call `subscribe_number_steps(&input, step, min, max, window, cx)`. It turns
  off the input's own stepping, so each press of − or +, and each press of
  Up or Down in the focused field, reaches the field's usual handler as the
  `Change` and the `PressEnter` of a typed value. The value then previews
  and commits as for a typed one, and each press is one undo step. The
  presses do not coalesce.
- A press at the smallest or largest value changes nothing.
- Steps: line width 0.5 pt, font size 1 pt, offset 1 pt, landing 1 pt, Bump size
  0.5 (0 to 4).
  Ranges: the field's own (line width 0.25 to 24 pt, font size 6 to 72 pt,
  landing 0 to the Cloud+ limit). Ink and rectangle line width keep their
  slider's range (0.25 to 20 pt, 0 to 20 pt).
- Opacity, X, Y, Width, Height and Rotation keep `PropertySliderInput` and
  `PropertyNumericInput`: a slider steps them, or two share one row.

## The wheel steps numbers and sliders

A mouse wheel or trackpad scroll over a number or a slider in a properties
or tool-defaults panel steps its value. Scroll up adds, scroll down subtracts.
The code is in `property_wheel.rs`.

- A slider (`PropertySliderInput`, the slider and the number beside it) steps
  by the slider's own step. A stepper (`PropertyStepInput`) steps as its − and
  + buttons do. `subscribe_number_steps` gives it the wheel. Call
  `register_number_wheel(&input, step, min, max, cx)` to give the wheel to a
  plain `PropertyNumericInput` (X, Y, Width, Height, Rotation, Line
  spacing). Both clamp to the min and max.
- One wheel notch is one step. A trackpad's pixels add up: 24 px is one step.
  Shift makes one step ten steps, on every platform.
- The value shows on the markup as it changes (as a typed value does). A
  burst of steps is one undo step. It is committed when the wheel has rested
  for 300 ms (`WHEEL_IDLE`) or when the pointer leaves the control. The field
  gets the `Change` of each step, then the `PressEnter` (a slider gets
  `SliderEvent::Change` and `Release`) that its usual handler already reads.
- The wheel works with or without the keyboard focus.
- A scroll that starts over the panel's empty space or its labels keeps
  scrolling the panel until it rests, even when a field passes under the
  pointer. A scroll that starts over a field belongs to the field, and the
  panel does not move. `PropertyInspectorPanel` marks the first kind
  (`wheel_over_panel`). A new panel layout must use it, or fields take the
  wheel in the middle of a panel scroll.
