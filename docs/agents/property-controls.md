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
