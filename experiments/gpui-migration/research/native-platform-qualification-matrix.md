# Native platform qualification matrix for the GPUI migration

Issue: [Define the native platform qualification matrix](https://github.com/apotenza92/butter-paper/issues/52)

## Decision

Qualify Butter Paper as one shared product on every supported operating system,
then add gates owned by each native backend, architecture, package, and update
mechanism. Linux GPU evidence proves Linux only. macOS and Windows runs include
ordinary shared workflows, not only platform-specific behavior.

Keep these evidence levels distinct:

1. deterministic source and module checks;
2. a development runtime from an identified worktree;
3. an unpacked internal candidate;
4. the exact packaged beta candidate;
5. the exact signed/notarized release candidate;
6. an installed application and N-1 update result;
7. a public release.

Evidence cannot move upward or sideways between levels, operating systems,
architectures, window-system backends, package types, or product channels.

## Supported target matrix

Preserve the current `aarch64` and `x86_64` product promise for macOS, Windows,
and Linux unless a separate product decision removes a target.

| Target | Native runtime and renderer | Package proof | Required native sessions |
| --- | --- | --- | --- |
| macOS ARM64 | AppKit, CoreText, Metal | Signed, notarized, stapled DMG and ZIP; stable and beta identity | Current Apple Silicon reference Mac; minimum supported macOS ARM64 host or virtual machine |
| macOS x64 | AppKit, CoreText, Metal | Signed, notarized, stapled DMG and ZIP; stable and beta identity | Native Intel runner for every candidate; minimum supported Intel host before stable |
| Windows ARM64 | Win32, DirectWrite, Direct3D 11 | Signed NSIS install, launch, repair/replace, uninstall; stable and beta identity | Native Windows ARM64 runner for every candidate |
| Windows x64 | Win32, DirectWrite, Direct3D 11 | Signed NSIS install, launch, repair/replace, uninstall; stable and beta identity | `alex-pc` when currently authorized; Windows 10 floor host or VM; native current-version runner |
| Linux ARM64 X11 | X11/XCB, XIM, AccessKit Unix, WGPU/Vulkan | AppImage, DEB, and RPM architecture and runtime boundaries | Native ARM64 GNU/Linux package host and a real Vulkan X11 session before stable |
| Linux ARM64 Wayland | Wayland, text-input-v3, XDG Desktop Portal, AccessKit Unix, WGPU/Vulkan | Same three package types | Native ARM64 GNOME and KDE Wayland sessions before stable |
| Linux x64 X11 | X11/XCB, XIM, AccessKit Unix, WGPU/Vulkan | AppImage, DEB, and RPM | Paid or owned hardware-GPU X11 session plus package-floor hosts |
| Linux x64 Wayland | Wayland, text-input-v3, XDG Desktop Portal, AccessKit Unix, WGPU/Vulkan | AppImage, DEB, and RPM | Paid or owned hardware-GPU GNOME and KDE Wayland sessions |

An x64 Linux GPU result does not prove ARM64. X11 does not prove Wayland, and
XWayland does not prove a native Wayland run. A hosted architecture build is
not native runtime proof.

Do not advertise a framework-declared operating-system floor. The oldest
supported product version is the oldest version on which the exact package has
passed launch, open/edit/save/reopen, accessibility-tree, native-dialog, and
uninstall gates. Until floor hosts pass, keep the current Electron product floor
or raise it through an explicit product decision.

For Linux, preserve the current GLIBC 2.35 ceiling and prove:

- AppImage on the oldest supported GNU/Linux baseline and current Ubuntu;
- DEB installation, launch, upgrade ownership, and removal on Ubuntu 22.04 and
  24.04 or the reviewed replacement baselines;
- actual RPM installation, launch, upgrade ownership, and removal on a current
  Fedora host and one reviewed RHEL-compatible host. Extracting an RPM is not an
  installation result.

## Gate ladder

### G0 — deterministic source gate

Run on the current development host before native or paid lanes:

- format, lint, compile, unit, property, state-machine, serialization, and
  generated-fixture tests;
- target-specific dependency, source, license, notice, vulnerability, unsafe,
  feature, and symbol inventories for all six architecture triples;
- cross-target compile where supported, without calling it native proof;
- canonical annotation and PDF writer oracles;
- performance event schema and report-validator tests;
- package manifest, product identity, updater trust, and release-policy tests.

G0 must pass before source or artifacts leave the development host.

### G1 — smallest native development smoke

For a platform slice that changed, run its development runtime on the target
platform and prove:

- one native window and expected GPU backend;
- Butter Paper-owned Geist font, tokens, and icons;
- Open PDF, first settled page, zoom, scroll, and close;
- one menu, dialog, popover, split control, and text input;
- one rectangle create/select/move/resize/property/undo/save/reopen path;
- semantic selector and accessibility-tree availability;
- clean shutdown with no worker, cache, or temporary-file leak.

G1 is implementation feedback. It does not prove packaging or release.

### G2 — exact package smoke

Build and hash the exact candidate once. Install or extract the package through
its public entry point, then prove:

- native architecture and complete runtime dependency closure;
- stable/beta name, application ID, executable, icon, user-data and cache
  separation, file association, and update feed identity;
- launch from application entry, PDF association, and drag/drop;
- shared Open/Edit/Save/Reopen scenario with isolated disposable user data;
- package-owned dialogs, menus, accessibility adapter, clipboard, and crash
  report location;
- uninstall or package removal leaves user documents intact and follows the
  reviewed user-data policy.

The smoke must not modify an installed public Butter Paper application.

### G3 — packaged beta qualification

Run the complete shared matrix, platform additions, corpus, accessibility,
input, display, performance, and 24-hour soak gates against one immutable beta
artifact set. G3 requires the same artifact hashes on all subsequent beta
review steps.

### G4 — release and replacement qualification

Repeat release-owned gates on the exact signed/notarized public candidates,
including native N-1 replacement, bad-update rejection, offline and interrupted
update recovery, stable/beta coexistence, clean install, in-place replacement,
file association, and uninstall. Performance follows `bp-perf-v2`.

No stable cutover occurs until all required G4 cells pass or an explicit product
decision removes the unsupported target.

## Shared product scenario matrix

Run these scenarios on every supported operating system and architecture at the
evidence level required by the current gate:

| Area | Required shared scenarios |
| --- | --- |
| Lifecycle | First launch, normal launch, reopen, close clean/dirty tabs, cancel close, safe shutdown, crash and worker-crash recovery |
| Documents | Open by dialog, path, association, and drop; multi-tab isolation; new blank PDF; import; Save, Save As, export; reopen twice; external-change conflict |
| Rendering | Single Page and Continuous modes; page columns; fit page/width; zoom limits; pan; two-axis scroll; thumbnails; page rotation and mixed sizes; stale-job rejection |
| Annotations | Every shipped tool family; create/select/multi-select/move/resize/vertex/text/property/lock/delete; snap; calibration; undo/redo; save/reopen and untouched-import preservation |
| Shell | Tabs, separate tab close, split buttons and settings menus, toolbar, rails, sidebars, properties, transient panels, empty/error/loading states |
| Input | Pointer, trackpad/mouse wheel, keyboard-only traversal, shortcut arbitration, text editing, selection, clipboard, drag/drop, one composing IME |
| Layout | Minimum and normal windows, long names, menu/panel overflow, 100%, 125%, 150%, and 200% scale where supported, display transition, light/dark/system theme |
| Accessibility | Names, roles, states, values, bounds, actions, reading order, focus entry/return, announcements, annotation and handle projection, contrast, keyboard-only completion |
| Integrity | No lost page content; canonical markup model and native annotation oracles; corrupt/hostile input limits; safe save publication; no private data in logs |
| Resources | `bp-perf-v2` correctness, latency, native presentation, CPU, memory, GPU memory, cache, upload, recovery, and artifact-completeness gates |

Use identical scenario IDs and fixture identities on every platform. Allow only
explicitly reviewed native interaction differences, such as Command versus
Control shortcut labels.

## Display and input matrix

### Scale and display

- macOS: 1x and 2x backing scale; Retina scaled modes; window move between
  displays; full screen, Spaces, sleep/display reconnection when a dedicated
  test explicitly authorizes those state changes.
- Windows: 100%, 125%, 150%, and 200% DPI; Per-Monitor-V2 transition between
  two scales; current and minimum supported Windows versions.
- Linux X11: 100% and 200%, plus the supported fractional-scale policy.
- Linux Wayland: native 100%, 125%, 150%, and 200% scale on GNOME and KDE.

Run the shared visual/interaction matrix at 60 Hz. Run frame pacing and input
latency at 120 Hz or higher on at least one reference machine per operating
system. Record the measured refresh period; do not infer it from settings.

### Text input and IME

Every platform must pass plain Latin input, dead-key composition, multiline
selection, Unicode paste, emoji, and a Japanese composition/conversion scenario.
Add:

- macOS native input-source switch and Command-key shortcut arbitration;
- Windows Text Services Framework input, AltGr, and one Chinese or Korean IME;
- X11 XIM input and clipboard selection behavior;
- Wayland text-input-v3 composition and portal clipboard behavior.

An IME pass requires visible pre-edit text, candidate selection, commit,
cancel, caret bounds, and no application shortcut firing during composition.

## Platform-owned gates

### macOS

- Metal device and drawable lifecycle, Retina geometry, full screen, Spaces,
  display reconnect, and native window activation;
- application menu ownership, Command shortcuts, services-safe text input,
  native open/save panels, drag/drop, clipboard, recent items, and PDF
  association;
- Accessibility API inspection plus VoiceOver completion of Open, page-mode
  change, rectangle edit, properties, Save As, and dirty-close;
- entitlements, Developer ID signing, notarization, staple, quarantine launch,
  DMG/ZIP identity, stable/beta coexistence, and native N-1 replacement on both
  architectures.

Use `alexs-macbook-pro` only when the current task explicitly authorizes the
named session or artifact. Preserve its current interactive and power state.

### Windows

- Direct3D 11 adapter selection, device loss, DPI transitions, native window
  chrome, multiple displays, high-contrast/system theme, and file dialogs;
- application menus and accelerators, clipboard, drag/drop, association launch,
  long/non-ASCII paths, cloud placeholders, and controlled-folder failure;
- UI Automation tree inspection plus Narrator completion of the shared critical
  workflow;
- Authenticode, NSIS clean/custom-path install, in-place replacement, ARM64
  architecture, stable/beta coexistence, N-1 replacement, and uninstall.

Use `alex-pc` only when the current task explicitly authorizes the named session
or artifact. Preserve its current interactive and power state.

### Linux X11

- real Vulkan hardware path, selected adapter and driver, device loss, XIM,
  clipboard primary/clipboard selections, drag/drop, and window-manager focus;
- native or portal file dialogs according to the reviewed desktop policy;
- AT-SPI tree inspection plus Orca completion of the shared critical workflow;
- AppImage, DEB, and RPM entry points, icons, desktop files, MIME association,
  update/package-manager ownership, install/removal, and GLIBC contract.

### Linux Wayland

- native Wayland rather than XWayland, real Vulkan, fractional scale, reconnect,
  text-input-v3, clipboard, drag/drop, and XDG Desktop Portal dialogs;
- both GNOME and KDE compositors;
- AT-SPI/Orca critical workflow;
- AppImage, DEB, and RPM launch through the native Wayland backend and the same
  package/update ownership gates.

## GPU failure policy

Normal supported operation requires Metal on macOS, Direct3D 11 hardware on
Windows, and Vulkan hardware on Linux.

- Windows WARP and Linux software Vulkan are **recovery adapters**, not normal
  performance targets. They must launch, show a clear recovery status, open a
  PDF, allow Save/Save As, and exit without data loss. They cannot satisfy
  performance or supported-GPU claims.
- Do not silently change renderer adapter after startup. Record adapter, driver,
  and fallback reason in the local diagnostic report without document content.
- On GPU device loss, pause mutation-independent rendering, preserve
  AnnotationSession state, recreate the device once, reject stale render
  generations, and resume. If recreation fails, enter the recovery adapter when
  available or show an actionable failure. Save must remain available.
- A blank window, stale page, unrecoverable data loss, or undocumented software
  fallback is release-blocking.

## Accessibility evidence

Automated semantic-tree assertions run on every native candidate. Before beta
and stable, a human-assisted screen-reader pass runs the same critical workflow
with VoiceOver, Narrator, and Orca.

Capture a scrubbed tree dump containing stable semantic IDs, roles, names,
states, values, actions, bounds, focus order, and announcements. Do not capture
private PDF text. Dense annotation documents expose visible pages plus active
selection through the windowed AccessibilityProjection; virtualization must not
reuse stale node IDs.

Any critical control, selected annotation, mutation handle, error, dirty-close
decision, or save result that is absent or inoperable through the native
accessibility API blocks beta.

## Lane authorization and isolation

Before using an external machine or paid resource, record:

- current-task authorization and exact machine or provider lane;
- source revision and artifact hashes;
- allowed test scope and fixture storage classes;
- expected duration, task limit, cleanup grace, absolute TTL, and maximum cost
  for paid compute;
- disposable user-data, cache, install, output, and log locations;
- current interactive-session and power state that must be preserved;
- artifact collection and cleanup plan.

Do not wake, unlock, lock, sleep, restart, shut down, or sign out a device unless
the current task explicitly requests that state change. Do not replace or alter
an installed public application. Use disposable profiles and isolated install
roots. Package/update tests that must replace an application run on disposable
CI runners or dedicated disposable virtual machines.

The paid Linux GPU lane follows the TTL and cost policy in
[`performance/protocol.md`](../performance/protocol.md). Upload only reviewed
public fixtures and exact candidate artifacts. Destroy all billable compute and
storage immediately after evidence collection or a conclusive outcome, then
verify destruction.

## Retry and failure policy

- Product assertion failure, crash, timeout, blank/stale presentation,
  accessibility failure, corrupt save, package failure, and unexpected renderer
  fallback get no automatic retry. They fail that candidate.
- A predeclared external infrastructure failure may be marked **aborted** only
  with host evidence. Correct the external state and run one new attempt with a
  new attempt ID. Preserve both attempts.
- A flaky result is a failure until its cause is fixed. Do not rerun to green or
  discard outliers.
- Paid-lane expiration is **timed out**. Do not silently extend its TTL.
- After a code or package change, use a new artifact hash and rerun every gate
  affected by that change. Do not splice results from different candidates.

## Evidence artifacts and retention

Every native run produces a machine-readable manifest with:

- scenario and matrix-cell IDs, start/end time, outcome, attempt ID;
- source revision, dirty-state hash, artifact/package hashes and channel;
- operating system, architecture, backend, compositor, CPU, memory, GPU, driver,
  displays, refresh, scale, theme, power and thermal facts;
- fixture IDs and hashes, with private content excluded;
- raw semantic events, native presentation trace, resource samples, renderer
  facts, accessibility tree, package/install log, and scrubbed screenshots or
  video where required;
- cleanup result and paid cost/duration when applicable.

Store development output under ignored result directories. Default retention:

- passing development artifacts: 7 days;
- failed, blocked, aborted, or timed-out development artifacts: 30 days;
- beta qualification artifacts: 90 days after the beta is superseded;
- stable release manifests, summaries, signatures, and updater evidence: the
  supported life of that release plus one year;
- private-corpus raw evidence: owner-controlled local storage only; publish only
  a scrubbed aggregate with no page content.

Uploading issue, pull-request, beta, or release evidence still requires explicit
publication authorization.

## Release-blocking outcomes

Block beta or stable for any required cell with:

- failed, timed-out, missing, stale, wrong-artifact, or underpowered evidence;
- lost PDF content, corrupt save, annotation round-trip failure, or stale worker
  publication;
- renderer crash, blank window, device-loss data loss, or silent recovery
  adapter use;
- inaccessible critical workflow, broken keyboard/IME input, or focus escape;
- package identity, architecture, association, install, update, or uninstall
  failure;
- unsupported minimum operating-system or distribution claim;
- license/source inventory failure;
- performance failure under `bp-perf-v2`;
- incomplete cleanup or an unverified billable paid resource.

Report required cells as passed, failed, blocked, aborted, timed out, or not run.
Never collapse those states into one overall green result.

## Current evidence status

- **Passed:** the maintained Electron target/package matrix, manual CI gates,
  existing package smoke scripts, current GPUI platform adapters, and available
  local device roles were inspected to define this matrix.
- **Failed:** the current pinned GPUI dependency graph still fails the separate
  distribution license gate.
- **Blocked:** current macOS capture files and the private Hibbeler corpus were
  not transferred to this VPS. No current artifact was authorized for native
  device qualification in this decision task.
- **Not run:** all GPUI packaged-candidate, architecture, X11, Wayland, native
  accessibility, IME, display-scale, updater, and performance matrix cells.
