# Planning

Local Markdown is Butter Paper's planning store. Keep it short and current.

- [backlog.md](backlog.md): open work, in rough priority order.
- [decisions.md](decisions.md): product and process decisions still in force.
- [interactions.md](interactions.md): how the canvas behaves (tools, selection,
  cursors, keys, viewing). Change it when the behaviour changes.
- [pdf-format.md](pdf-format.md): how markups are stored (Bluebeam parity).
- [ux-review.md](ux-review.md): the checklist for reviewing UI changes before
  handoff.

Rules:

- Update a file in place when a decision is made or work finishes; remove
  items that are done rather than marking them done. History lives in git and
  `CHANGELOG.md`.
- No transcripts, dated worklogs or evidence trails. Screenshots and logs go
  in ignored `target/` or `test-results/` directories and are not linked from
  here.
- GitHub issues are not used for tracking unless the owner asks.
- The Electron app is frozen at the `electron-final` tag; consult it as a
  reference, never revive it.
