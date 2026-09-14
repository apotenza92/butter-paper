# Native template-manager contract

GitHub issue: [#84](https://github.com/apotenza92/butter-paper/issues/84)

This contract freezes the maintained Electron behavior before the native
implementation changes. The source inventory is the shipping template library,
manager, picker, blank-paper settings, main-process template store, application
command routing, and their tests. Missing behavior below is a recorded gap, not
an invitation to invent a new contract.

## Records, order, and authority

The visible order is the six built-ins, custom generated records in insertion
order, then imported PDFs in durable index order. Stable built-in IDs are
`built-in-blank`, `built-in-dots`, `built-in-grid`, `built-in-lined`,
`built-in-isometric`, and `built-in-triangle`.

One application-owned template library is the durable authority. It owns
custom settings, imported metadata and managed PDF bytes, and the last-used
stable ID. Presentation state owns only the transient selected ID, create
draft, validation result, pending operation generation, and focus owner.
Imported source paths never enter presentation persistence.

Opening the manager selects the last-used template, exits create mode, and
clears its current error. Selecting a row changes only the preview. A successful
create or import becomes last-used. Removing the last-used custom or imported
record falls back to Blank Paper. Built-ins cannot be removed.

## Generated templates

Names trim and collapse whitespace. Empty names fail with `Template name is
required.` Names over 80 UTF-16 code units fail with `Template name must be 80
characters or fewer.` Supported paper, orientation, pattern, spacing, colour,
and numeric bounds match `blankPdfSettings.ts`.

Create mode starts with A3 landscape Blank Paper defaults, an empty name, and
focus in the name input. It shows a live settings summary and preview. Cancel
returns to the list without closing the manager. Save validates synchronously,
persists one custom record, selects it, makes it last-used, and returns to the
list. Failure preserves the draft and focus.

## Imported templates and document creation

Import uses one native PDF selection request. Cancel changes nothing. Success
copies a parseable PDF with at least one page into managed storage, records its
page count and checksum, and makes it last-used. A stale completion must not
replace a newer request. Failure preserves the last good library and produces
deterministic visible status.

Creating from a generated or imported template always materializes independent
temporary bytes. The resulting `Untitled.pdf` session starts dirty. Save/reopen
must not mutate the template source. Close releases its worker, mapped surfaces,
and owned temporary source.

`Save Document as Template...` imports the authorized current source bytes and
filename. It does not include unsaved in-memory annotations or page-scale
changes. Success makes the imported record last-used. Failure leaves the
library unchanged. This compatibility limitation must remain visible until a
separate product decision changes it.

## Migration and command decision

When no native library exists, legacy `butter-paper.blank-pdf-settings.v1`
settings migrate once. A complete match selects the matching built-in.
Otherwise the native library creates `custom-migrated-blank-pdf-default` named
`Previous Blank PDF`. Malformed legacy data falls back to Blank Paper.

Electron currently routes the native File command to the manager but its
renderer menu to the older blank-PDF dialog. The native application resolves
this inconsistency in favor of one `New from Template...` command that opens
the template manager. This is the smallest behavior that exposes built-in,
custom, and imported records through one coherent path.

## Input, accessibility, and layout

Use real pinned GPUI Component Dialog, ListItem/list presentation, Input,
Field, Button, Alert/Notification, and scrollbar primitives. Stable product IDs
sit on every row and action. Product state suppresses disabled list activation
because the pinned outer list does not honor inner disabled state.

The dialog restores focus to its opener after Done, Escape, or outside-click
dismissal. Create mode focuses the name input. Input state remains native and
composition-ready; deterministic tests must not synthesize IME acceptance.
Feedback has a stable visible status node because the pinned Notification does
not expose a live-region contract.

At normal width the list and 280 px preview are side by side. At constrained
width the presentation stacks them and keeps one explicit vertical scroll
owner. No action may overlap, shrink below its hit target, clip, or cause
horizontal growth. Fresh live accessibility, IME, and visual acceptance remain
native-platform gates.

## Development acceptance

The experiment implements this contract through one persistent manager and one
DocumentWorkspace-owned template split control. Dynamic rows, all generated
settings, native import, create, manage, remove, authorized-source save,
restart, rollback, stale-result rejection, and resource release have
deterministic Linux development proof. The final focused gates are 16/16 for
the manager, 6/6 for native application commands, and 23/23 for the tab bar.
Both checksum-controlled real PDF journeys pass 1/1. The warm all-targets gate
passes 217 active tests with 23
explicit gated tests.

This is not packaged or shipping proof. Production PDFium redistribution is
blocked. Fresh visual, live accessibility, screen-reader, IME, native
Escape/outside-click input, macOS, Windows, Hibbeler, and matched performance
evidence are not run.
