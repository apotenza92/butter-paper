# Beta, rollback, and stable cutover plan for the GPUI replacement

Issue: [Choose the beta, rollback, and stable cutover plan](https://github.com/apotenza92/butter-paper/issues/54)

## Decision

Replace Electron through explicit development, internal-candidate, closed-beta,
public-beta, stable-candidate, and stable-observation stages. Promotion is a
manual product decision over one immutable candidate and a complete
qualification ledger. No stage promotes automatically because time elapsed or
no one reported a problem.

Electron stable remains the public rollback product throughout GPUI beta. GPUI
becomes the sole newly shipped stable application when the first qualified GPUI
stable release and its updater feeds become public. Keep the buildable Electron
rollback product for at least 60 days and two successful GPUI stable releases,
whichever is later. Repository retirement follows the separate Electron
retirement decision only after that window and every exit criterion pass.

## Canonical terms

- **Candidate:** one immutable cross-platform stable or beta artifact set built
  from one commit, version, dependency graph, and release manifest. Any binary,
  resource, package, identity, or updater change creates a new Candidate.
- **Cohort:** consenting people and identified test installations authorized to
  use one Candidate and submit only the agreed evidence.
- **Document Session:** open or create through close for one PDF tab. A crash or
  forced termination ends the session as not crash-free.
- **Save Attempt:** one explicit Save, Save As, or export request through its
  confirmed publication or classified error.
- **Observation Window:** consecutive calendar time during which the same
  Candidate meets the required exposure and outcome thresholds.
- **Qualification Ledger:** the manifest-backed set of deterministic, native,
  package, performance, cohort, incident, and cleanup outcomes for a Candidate.
- **Promotion:** an explicitly authorized move of one Candidate to the next
  stage. Promotion does not rebuild it.
- **Rollout Stop:** prevent additional installations from the affected feed or
  distribution entry while preserving already-published immutable assets.
- **Rollback Product:** maintained Electron source and release configuration
  capable of producing a corrected higher-version emergency package.

## Non-negotiable entry gates

No Candidate enters internal qualification until:

- the pinned GPUI/runtime source, license, notice, vulnerability, and target
  dependency gates pass;
- the product-contract inventory and every implementation issue required for
  the target stage are complete;
- document, annotation, save/reopen, imported-object preservation, hostile PDF,
  accessibility, keyboard/IME, visual/interaction parity, and constrained
  layout gates pass at the required evidence level;
- `bp-perf-v2` passes on the required current-host and native lanes;
- exact package, identity, stable/beta isolation, signing/notarization, updater
  trust, N-1 replacement, and rollback/fault-injection gates pass;
- every required native matrix cell is passed rather than blocked, not run,
  stale, or underpowered;
- the LegacyStateImporter passes migration and rollback-state isolation tests;
- the exact Candidate has a complete scrubbed evidence manifest and no open
  severity P0 or P1 incident.

The private Hibbeler lane must pass on an authorized local machine or be
explicitly waived as unavailable in the stable decision. A missing private file
is never reported as a pass.

## Stage 0 — development slices

### Identity and audience

- Non-public GPUI development identity and isolated runtime state.
- Developers and explicitly authorized reference-device sessions only.
- Updates and public PDF association disabled.

### Required result

Each migration slice passes deterministic gates, its smallest native smoke, the
matched parity manifest, and a real product consumer before the next dependent
slice starts. Placeholders never unlock shell, workflow, beta, or package claims.

This stage can use the current Linux VPS for CPU-only work and an authorized
short-lived Linux GPU lease only after artifacts, checks, TTL, cost, and cleanup
are declared. It cannot produce beta evidence.

## Stage 1 — internal packaged candidate

### Cohort

- Owner-controlled or dedicated disposable installations on macOS, Windows,
  X11, GNOME Wayland, and KDE Wayland.
- Native ARM64/x64 package cells from the qualification matrix.
- No public feed or public file association takeover.

### Minimum exposure

The same Candidate must complete:

- 7 consecutive days;
- at least 25 active document-hours across the three operating systems;
- at least 100 Document Sessions;
- at least 50 annotation Save Attempts followed by two successful reopens;
- at least 10 clean update/replacement cycles for every updater-capable
  platform/architecture cell, using disposable runners or virtual machines;
- one 24-hour idle/open-document soak per operating system;
- the full public corpus and required private-local workload outcome.

All scripted matrix and performance sample-count requirements remain separate;
these exposure numbers do not replace them.

### Exit

No P0/P1 incident, no unresolved required P2 product regression, complete
cleanup, and an explicitly approved immutable beta Candidate.

## Stage 2 — closed GPUI beta

### Product relationship

GPUI takes the existing **beta** identity only. Electron stable stays public and
unchanged. The maintained Electron beta rollback package uses a separate
emergency identity if testers need side-by-side comparison. It never writes the
GPUI beta runtime epoch.

### Cohort

Minimum five consenting human testers and eight identified beta installations:

- at least two macOS installations;
- at least two Windows installations;
- at least two Linux installations, including native X11 and Wayland;
- at least one daily-use installation on each operating system;
- architecture package qualification still covers ARM64 and x64 even when the
  human cohort does not own every architecture.

Testers receive the data-handling statement, known intentional differences,
rollback instructions, diagnostic-export instructions, and a direct way to
report a stop-ship issue. Participation never grants automatic device access.

### Minimum exposure

The same Candidate must complete:

- 21 consecutive days;
- at least 100 active document-hours;
- at least 250 Document Sessions, including 50 documents above 100 pages;
- at least 100 annotation Save Attempts with two-reopen verification;
- at least 25 real beta update cycles across all three operating systems;
- at least 20 dirty-close/cancel/reopen decisions;
- at least 10 legacy Electron-state imports, including one no-secure-storage
  result and one idempotent relaunch;
- the screen-reader critical workflow with VoiceOver, Narrator, and Orca.

### Exit

All entry gates remain green, the exposure manifest is complete, crash-free
Document Sessions are at least 99.5%, unexpected open failure is at most 0.5%,
unexpected Save Attempt failure is at most 0.1%, and no stop-ship trigger occurs.

## Stage 3 — public opt-in beta

### Publication

Publish the exact GPUI beta Candidate through the protected beta release and
feed. Beta remains explicitly opt-in and visually distinct. Keep Electron stable
as the default public product. The release notes state current limitations,
approved better/worse differences, rollback steps, private diagnostic policy,
and the one-time manual-install fact for legacy Windows/Linux users when it
still applies.

### Cohort and minimum exposure

The promotion ledger must contain at least 15 consenting active beta testers,
with at least three on each operating system, who submit versioned session
summaries or diagnostic bundles. The same Candidate must complete:

- 30 consecutive days;
- at least 300 active document-hours;
- at least 500 Document Sessions;
- at least 200 annotation Save Attempts with two-reopen verification;
- at least 50 successful beta update cycles, including every supported updater
  architecture through native automated qualification;
- at least 25 documents above 500 pages or the equivalent public/private heavy
  workload repetitions;
- at least one full workday on a 60 Hz display and one on a 120 Hz-or-higher
  display per operating system.

If participation does not reach the denominator, the result is blocked or
underpowered. Calendar time alone cannot approve stable.

### Exit

- Every required Qualification Ledger cell passes on the exact Candidate.
- No open P0/P1 incident.
- Rates remain within closed-beta limits.
- `bp-perf-v2` release sampling passes.
- No critical accessibility workflow fails.
- Every intentional difference has an approved better/worse user-experience
  statement.
- The emergency higher-version Electron package drill passes from current
  release source without publishing it.
- Product owner explicitly authorizes stable-candidate construction.

## Stage 4 — stable release candidate

Build the stable and beta variants once from the approved commit. The stable
variant has the public stable identity and a new version; it is a new Candidate
because identity and package resources differ from beta.

Before public release, the exact stable Candidate completes:

- every G4 native and package matrix cell;
- latest-public Electron stable to GPUI stable N-1 replacement on every
  updater-capable architecture, plus DEB/RPM package-manager upgrade;
- clean install, stable/beta coexistence, association, Homebrew, uninstall, and
  fault-injection recovery;
- 7 consecutive days on disposable stable-identity installations;
- at least 20 active document-hours per operating system and 100 total
  Document Sessions;
- `bp-perf-v2` against the latest public Electron stable on each platform;
- legacy-state import without modifying the Electron rollback state;
- a final emergency Electron higher-version build and exact package smoke.

The stable Candidate may be public as a draft release during protected
verification, but no public update feed points to it before authorization.

## Stage 5 — public GPUI stable and observation

### Cutover point

After explicit publication authority, publish and independently verify the exact
stable assets, then atomically publish the sealed feeds. At that moment GPUI is
the sole newly shipped stable Butter Paper application. Do not ship Electron and
GPUI as permanent parallel stable products.

Existing Electron installations update to GPUI where their bootstrap supports
it. Legacy Windows/Linux users without updater bootstrap receive the documented
manual-install path. DEB/RPM users upgrade through their package manager.

### Observation windows

Review the Qualification Ledger and incident log at:

- 24 hours: acute install, launch, migration, update, and save integrity;
- 72 hours: rollout-stop decision point;
- 7 days: first cross-platform field review;
- 30 days: first stable retention review;
- 60 days: minimum rollback-window review;
- after the second successful GPUI stable release: final rollback exit review.

The rollback window ends only after both 60 days and two GPUI stable releases,
and only when every retirement criterion passes. Until then:

- maintain the Electron rollback source, lockfile, signing/package scripts,
  updater compatibility, and current security fixes;
- keep signing and release environments able to produce a higher-version
  Electron emergency release;
- do not delete Electron production sources or release workflows;
- do not migrate or delete legacy Electron state;
- rehearse the emergency package at stable day 14 and before the second GPUI
  stable release.

## Diagnostics and privacy

Do not add automatic product analytics or remote crash upload for this cutover.
Use three evidence sources:

1. deterministic/native/package/performance manifests from controlled runs;
2. an explicit **Export Diagnostic Bundle** action for consenting testers;
3. user-reported incidents and support conversations.

The local DiagnosticBundle module may include:

- random cohort installation ID, Candidate version/hash, channel, runtime
  epoch, operating system, architecture, backend, GPU/driver, display facts;
- scrubbed crash stack/symbol IDs, scenario counters, performance aggregates,
  updater state category, and qualification event IDs;
- success/failure counts for Document Sessions, opens, Save Attempts, and
  update transactions.

It must exclude PDF bytes, page images, thumbnails, extracted text, markup
content, filenames, full paths, recent-document lists, templates, signatures,
clipboard data, account names, hostnames, network addresses, private keys, and
credentials. Show the exact bundle before export. Export does not send it.
Publication or delivery requires an explicit user action and destination.

Public incidents without a reliable denominator are counts, not rates. Never
invent adoption, crash-free, or save-success rates from downloads or silence.

## Severity and rollback triggers

### P0 — immediate rollout stop and rollback preparation

Any confirmed occurrence is sufficient:

- lost PDF content, corrupt save, annotation round-trip loss, wrong-file write,
  or stale save publication;
- signing, notarization, updater trust, channel isolation, package provenance,
  or update rollback/freeze bypass;
- leaked private PDF/signature/credential data or privilege expansion;
- GPUI migration destroys or makes Electron rollback state unreadable;
- unrecoverable launch failure or data-loss GPU/device-loss path on a required
  target;
- public asset/feed bytes differ from the qualified Candidate.

One credible untriaged P0 report freezes promotion and rollout until classified.
A confirmed P0 triggers the rollback procedure.

### P1 — promotion stop; fix within 72 hours or roll back

Any of these triggers:

- crash-free Document Sessions below 99.5% after at least 200 sessions;
- unexpected open failure above 0.5% after at least 200 attempts;
- unexpected Save Attempt failure above 0.1%, or any unexplained save-integrity
  failure regardless of rate;
- the same unhandled crash or hang on two independent installations;
- a critical workflow cannot be completed with keyboard or the native screen
  reader on any required platform;
- exact package install, N-1 replacement, association, or uninstall fails on a
  required target;
- any required `bp-perf-v2` absolute or non-inferiority gate fails;
- memory/cache growth exceeds its ceiling or causes one out-of-memory failure;
- five or more independent users report the same workflow-blocking regression.

### P2 — hold or explicitly accept before promotion

- the same non-critical regression on three independent installations;
- more than 2% of recorded sessions hit the same recoverable workflow defect;
- an unapproved visual/interaction/accessibility difference;
- a diagnostic or support burden that makes the beta misleading or unusable.

A P2 can remain only with a documented owner, workaround, scope, target release,
and explicit better/worse acceptance. It cannot conceal a P0/P1 classification.

## Rollout stop and rollback procedure

1. Freeze promotion and new feed publication immediately. Preserve all public
   assets, feeds, manifests, reports, and incident evidence.
2. Within one hour of a confirmed P0, decide whether an affected feed can safely
   restore the exact prior feed commit without contradicting installed/public
   state. Never replace public assets or enable downgrade.
3. Identify affected channel/platform/architecture and stop only narrower cells
   when evidence proves other cells safe. A document-integrity, trust, identity,
   or shared-state P0 stops every channel in scope.
4. Preserve GPUI runtime state and the immutable Electron legacy source. Give
   users explicit Save/Export and recovery instructions.
5. For already installed bad GPUI packages, publish either a fixed GPUI package
   or the maintained Electron rollback product at a **higher** version. Target
   confirmed P0 availability within 24 hours. P1 must fix forward within 72
   hours or use the higher-version rollback product.
6. Run the full affected package, N-1, migration, document-integrity, native,
   performance, and publication gates on the emergency Candidate.
7. Independently verify public assets before publishing its feeds.
8. Keep the incident open through recovery verification. Do not resume rollout
   because the emergency package merely built.

If a Candidate changes after a P0/P1 fix, its observation clock and exposure
counts restart. Evidence from the prior hash remains historical. A documentation
correction that does not alter artifacts may retain technical evidence, but its
user communication still needs review.

## Recovery behavior

- Update transaction failure restores the prior runnable local artifact without
  mutating user documents or shared legacy state.
- First-launch migration failure leaves Electron state untouched, marks GPUI
  import incomplete, and offers retry or start-with-defaults without deleting
  source state.
- Renderer failure preserves DocumentSession state and keeps Save/Save As
  available through the reviewed recovery adapter or actionable failure UI.
- A user who returns to Electron during the rollback window sees the unchanged
  Electron state as of migration time. GPUI changes remain in user PDFs after
  successful saves; no application-state merger attempts to reconcile runtime
  preferences automatically.
- Do not auto-open a private document during update health checks or recovery.

## Promotion authority and evidence states

Every stage transition requires an explicit product-owner decision linked to
the Candidate manifest. The decision lists:

- passed cells and exact artifact hashes;
- failed incidents and their fixed Candidate hashes;
- blocked evidence and any explicit waiver;
- aborted infrastructure attempts;
- timed-out scenarios or paid lanes;
- not-run cells;
- approved intentional better/worse differences;
- cohort denominators and observation dates;
- rollback readiness and cleanup verification.

No publication, user message, feed mutation, device access, or paid lease occurs
only because this plan exists. Those actions still require their task-specific
authority.

## Current evidence status

- **Passed:** current stable/beta channel behavior, package/update gates,
  diagnostic capabilities, migration decisions, native/performance matrices,
  privacy constraints, and release rollback rules were inspected to define the
  staged cutover.
- **Failed:** the current pinned GPUI dependency graph still fails the separate
  distribution license gate.
- **Blocked:** no qualified immutable GPUI beta Candidate or cohort exists.
  Current macOS captures and the private Hibbeler corpus were not transferred to
  this VPS; the private lane remains blocked here rather than passed.
- **Not run:** all GPUI internal, closed-beta, public-beta, stable-candidate,
  public-stable, cohort, diagnostics, observation, emergency Electron build,
  rollback, device, package, updater, and performance stages.
