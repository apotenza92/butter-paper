# Local planning

For every migration UI change or acceptance review, read [the evolving UX review checklist](ux-review.md). It is the canonical procedure used by the gpui-migration-ux-review skill; update lessons there rather than creating another review plan.

Local Markdown is the authoritative planning store for Butter Paper. Start with [the migration todo orchestrator](migration-todo.md), then [the migration roadmap](gpui-migration.md) (including the Phase 4 interaction appearance and behaviour inventory), region briefs such as [the active menu brief](menu.md) and [the markup file format brief](pdf-format.md), and [the deferred backlog](backlog.md).

Update these files in place after meaningful decisions or verification. Record the next concrete action, unresolved requirements, evidence paths and user acceptance. Do not create a transcript, daily log, separate spec/ticket hierarchy, or duplicate GitHub plan. Add a region file only when work on that region begins. Keep screenshots, recordings, logs and generated receipts in ignored test-results or target directories; link them with repository-relative paths and identify the checkout holding them.

Routine implementation inside an approved region proceeds without repeated approval. The user still approves each region crop before its brief or implementation and accepts each result. Approved regions may proceed in parallel using isolated candidates, with shared changes coordinated and an overall integration review afterwards. Skills supply guidance; their issue-publishing and duplicate state-file instructions are adapted to these local files. GitHub issues are historical context unless the user explicitly requests GitHub tracking.

The Electron application was removed from `main` on 2026-10-01; the native application is the product and these plans now track it on `main`. Electron parity notes below are historical references to the frozen `electron-final` tag. No commit or push is implied by keeping Markdown in the repository.
