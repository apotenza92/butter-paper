# Issue tracker: GitHub

Issues and specifications for this repository live in GitHub Issues at
`apotenza92/butter-paper`. Use the `gh` CLI from this clone so it resolves the
repository from `origin`.

## Operations

- Create, read, edit, label, comment on, and close work with `gh issue`.
- Publish one implementation ticket per issue.
- Apply `ready-for-agent` to fully specified tickets.
- Express blocking edges with GitHub issue dependencies. If the dependency API
  is unavailable, add a `Blocked by: #<issue>` line to the ticket body.
- A ticket is on the execution frontier when all its blockers are closed.
- Pull requests are not an incoming request or triage surface.

## Migration execution

The Electron-to-GPUI migration uses one parent specification and independent
tracer-bullet tickets. An implementation agent claims an unblocked ticket,
works it through deterministic tests and review, records evidence, and closes
it only when every acceptance criterion passes. Routine reversible choices
inside an approved ticket do not require another user decision.

Stop for user direction only when work needs new authority or an irreversible
licensing, security, product-scope, destructive-data, paid-infrastructure, or
production-promotion decision.
