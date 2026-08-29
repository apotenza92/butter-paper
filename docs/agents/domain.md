# Domain docs

Butter Paper is a multi-context monorepo. Read `CONTEXT-MAP.md` when it exists,
then read the `CONTEXT.md` files relevant to the current package or application.
Also read relevant system ADRs under `docs/adr/` and context ADRs beside the
affected context.

Domain documents are created lazily when a term or durable decision needs a
single source of truth. Their expected layout is:

```text
CONTEXT-MAP.md
docs/adr/
apps/<application>/CONTEXT.md
apps/<application>/docs/adr/
packages/<package>/CONTEXT.md
packages/<package>/docs/adr/
experiments/gpui-migration/CONTEXT.md
experiments/gpui-migration/docs/adr/
```

Use the glossary's exact terms in specifications, ticket titles, tests, and
code. Surface conflicts with an accepted ADR instead of silently overriding
it. Missing context documents are not a blocker; continue with repository
sources and create domain documentation only when the work resolves a real
terminology or architecture decision.
