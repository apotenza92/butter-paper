# Property controls

Read this before adding or changing a property family. Use the gpui-component
skill and [the UI policy](ui-policy.md), and follow
[the markup contract](../planning/pdf-format.md) for stored appearance data.

Compose standard controls through the application toolkit in
`crates/butter-paper/src/property_controls.rs`. Reuse its panel, header,
stepper-free numeric and slider/input layouts rather than creating a parallel
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

