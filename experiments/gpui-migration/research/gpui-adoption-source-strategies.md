# GPUI adoption and source strategies

Date: 2026-08-21

## Question

How do maintained applications consume GPUI, and can Butter Paper avoid
maintaining a public Zed fork while retaining current accessibility APIs,
cross-platform support, reproducible builds, and an MIT-compatible distributed
dependency graph?

This survey uses application manifests, lockfiles, release assets, and project
CI policy as primary sources. A repository using GPUI is not evidence that its
license, accessibility, packaging, or reproducibility policy is sufficient for
Butter Paper.

## Official baseline

Zed's current GPUI README recommends the released `gpui` and `gpui_platform`
packages, describes GPUI as pre-1.0, and warns that breaking changes remain
common. It also documents macOS, Windows, X11, and Wayland platform selection
([GPUI README](https://github.com/zed-industries/zed/blob/main/crates/gpui/README.md)).
Zed's own `create-gpui-app` template instead follows the Zed Git repository
without an explicit revision
([template manifest](https://github.com/zed-industries/create-gpui-app/blob/63fbe214da83c5409f6845147d793d595b12e2c5/templates/default/_Cargo.toml)).
That template is useful for starting an experiment, but its moving dependency
is not a production provenance policy.

The official crates.io `gpui 0.2.2` package remains the newest official release
([package](https://docs.rs/crate/gpui/0.2.2)). Its source is permissively
licensed and avoids the current Git graph's `ztracing` chain, but Butter Paper's
earlier audit found that it predates the selected AccessKit integration and the
current split `gpui_platform` boundary. It is therefore not a drop-in foundation
for Butter Paper's accepted accessibility contract.

## Application evidence

| Application | Shipped source strategy | What it proves | Butter Paper consequence |
| --- | --- | --- | --- |
| [OpenLogi](https://github.com/AprilNEA/OpenLogi) | Follows `zed-industries/zed` and pins the resolved commit only in `Cargo.lock` ([manifest](https://github.com/AprilNEA/OpenLogi/blob/57db38dd39ed11d7c050f4d0826c1eb900981496/Cargo.toml)). | A current released application can track Zed directly. | Its own `deny.toml` explicitly excludes the GUI graph because the Zed/GPUI tree trips advisory, license, and source checks ([policy](https://github.com/AprilNEA/OpenLogi/blob/57db38dd39ed11d7c050f4d0826c1eb900981496/deny.toml)). This does not resolve Butter Paper's gate. |
| [Okena](https://github.com/contember/okena/releases/tag/v0.28.0) | Follows Zed Git without a manifest revision ([manifest](https://github.com/contember/okena/blob/93eacbf802709b5ac732a72dff7722ef84d357e2/Cargo.toml)). | A maintained packaged terminal application accepts lockfile-only pinning. | Its locked GPUI graph contains `ztracing`, `ztracing_macro`, and `zlog`; copying this policy would waive Butter Paper's existing license rule. |
| [rgitui](https://github.com/noahbclarkson/rgitui/releases/tag/v0.4.0) | Pins an exact official Zed commit in the manifest ([manifest](https://github.com/noahbclarkson/rgitui/blob/4474a9822e0b8b899347859a3b78756135eee985/Cargo.toml)). | Exact upstream pins are practical for released apps. | Its pinned lock graph still contains the three GPL-marked packages, so exactness alone is insufficient. |
| [Script Kit](https://github.com/johnlindquist/script-kit-next/releases/tag/v0.1.17) | Vendors the required GPUI crates and patches Cargo to use local paths ([manifest](https://github.com/johnlindquist/script-kit-next/blob/018c75dc573a8e5294200fbfdb592cdd434e69ea/Cargo.toml)). Its vendored GPUI replaces Git `sum_tree` with Apache-2.0 `zed-sum-tree` specifically to sever the GPL chain ([vendored manifest](https://github.com/johnlindquist/script-kit-next/blob/018c75dc573a8e5294200fbfdb592cdd434e69ea/vendor/gpui/Cargo.toml), [license policy](https://github.com/johnlindquist/script-kit-next/blob/018c75dc573a8e5294200fbfdb592cdd434e69ea/deny.toml)). | A shipped app has independently encountered and removed the same dependency problem without a public Zed fork. | Vendoring avoids a remote fork but makes the application repository the owner of copied framework source, updates, notices, and local modifications. That is more source ownership than Butter Paper currently wants. |
| [Scope](https://github.com/scopeclient/scope) | Uses its own Zed fork on a moving feature branch ([manifest](https://github.com/scopeclient/scope/blob/a61a2d0c31685064fade18dc53da5cc52bb6211f/Cargo.toml)). | Downstream forks are a real ecosystem pattern. | A moving private feature branch has weaker provenance than Butter Paper permits. |
| [SuperHQ](https://github.com/superhq-ai/superhq/releases/tag/v0.4.4) | Uses crates.io `gpui = "0.2"` ([manifest](https://github.com/superhq-ai/superhq/blob/4d49f8284cbd073a8b1594ea2933c97deaab715c/Cargo.toml)). | A released app can stay on the official package and avoid Git source maintenance. | This is the simplest pattern, but the official package lacks Butter Paper's selected current accessibility boundary. |
| [Monocurl](https://github.com/monocurl/monocurl/releases/tag/v0.3.2) and [Psst](https://github.com/phisch/psst/releases/tag/v0.2.0) | Use published `gpui-ce 0.3.3` from crates.io ([Monocurl manifest](https://github.com/monocurl/monocurl/blob/60800178c56ab1dad4763af3d53e0669f79b6d15/Cargo.toml), [Psst manifest](https://github.com/phisch/psst/blob/851c599c599e1dbaba48c6336ff41971f210826e/crates/ui/Cargo.toml)). | A community GPUI distribution is used by packaged applications without app-owned forks. | Inspection of the published `0.3.3` source found no AccessKit integration. That exact release cannot satisfy Butter Paper. |
| [Hummingbird](https://github.com/hummingbird-player/hummingbird/releases/tag/0.3.0) | Uses current GPUI-CE Git packages; its lockfile pins commit `c738623ffbcec2aeddc44a645cc6b74646d5cf97` ([manifest](https://github.com/hummingbird-player/hummingbird/blob/536c689d2867e4510c793e2badf6f1e77c604ffa/Cargo.toml)). | Its release publishes macOS ARM64/x64, Windows ARM64/x64, and Linux AppImage ARM64/x64 artifacts. This is the closest downstream platform precedent for Butter Paper. | Current GPUI-CE Git is a serious fork-free candidate, but Butter Paper must run its own dependency, API, accessibility, and native qualification gates. |
| [Frame](https://github.com/66HEX/frame/releases/tag/0.33.0) | Vendors GPUI-CE and redirects all GPUI-CE packages to local paths ([manifest](https://github.com/66HEX/frame/blob/2ccdb5d4f4ec29f54d2b710d22e7e4934451680a/Cargo.toml)). | GPUI-CE supports another released application with macOS, Windows, Linux, ARM64, x64, source, vendor, and software-bill-of-materials artifacts. | It proves packaging breadth, but vendoring remains a framework-ownership choice rather than a zero-maintenance dependency. |
| [GitComet](https://github.com/Auto-Explore/GitComet/releases/tag/v0.2.1) | Pins a commit from its own GPUI-CE-derived fork ([manifest](https://github.com/Auto-Explore/GitComet/blob/a9edb6c0a99d934555449498e4683d5fa2f2ea07/Cargo.toml)). | It releases macOS, Windows, and Linux ARM64/x64 packages from a pinned derived source. | This is strong platform evidence, but it does not avoid downstream fork ownership. |

## Current GPUI-CE Git candidate

GPUI-CE describes itself as a general-purpose community fork and documents both
crates.io and Git consumption
([README](https://github.com/gpui-ce/gpui-ce/blob/c738623ffbcec2aeddc44a645cc6b74646d5cf97/README.md)).
At exact commit `c738623ffbcec2aeddc44a645cc6b74646d5cf97`:

- the workspace and GPUI package declare Apache-2.0;
- the current source includes AccessKit dependencies for macOS, Windows, and
  Unix and retains the split `gpui_platform` package
  ([workspace manifest](https://github.com/gpui-ce/gpui-ce/blob/c738623ffbcec2aeddc44a645cc6b74646d5cf97/Cargo.toml),
  [platform manifest](https://github.com/gpui-ce/gpui-ce/blob/c738623ffbcec2aeddc44a645cc6b74646d5cf97/crates/gpui_platform/Cargo.toml));
- its committed lockfile contains none of `ztracing`, `ztracing_macro`, or
  `zlog`;
- its CI builds and tests Linux, macOS, and Windows
  ([CI](https://github.com/gpui-ce/gpui-ce/blob/c738623ffbcec2aeddc44a645cc6b74646d5cf97/.github/workflows/ci.yml)); and
- Hummingbird provides real packaged six-architecture application evidence.

Important qualifications:

- GPUI-CE is not Zed's official GPUI release and can diverge from Zed APIs.
- The usable current source is a Git commit, not the older published `0.3.3`
  package.
- GPUI-CE's own audit jobs are `continue-on-error`, and its policy currently
  ignores two unavailable-fix `quick-xml` advisories
  ([audit workflow](https://github.com/gpui-ce/gpui-ce/blob/c738623ffbcec2aeddc44a645cc6b74646d5cf97/.github/workflows/audit.yml),
  [deny policy](https://github.com/gpui-ce/gpui-ce/blob/c738623ffbcec2aeddc44a645cc6b74646d5cf97/deny.toml)).
  Butter Paper must apply its stricter independent policy and must not inherit
  those exceptions automatically.
- A disposable Butter Paper gallery port compiled and passed its focused Rust
  tests against this exact commit. The port required only source selection,
  one explicit `palette` dependency/import, GPUI-CE's color-conversion trait,
  and removal of five redundant `.accessibility_id(...)` calls. The affected
  elements retain stable GPUI element IDs, roles, and accessible labels.
- Appearance, native accessibility output, and performance remain unproved.

## Recommendation

Do not create `apotenza92/zed` yet.

Use exact GPUI-CE commit
`c738623ffbcec2aeddc44a645cc6b74646d5cf97` as a bounded replacement candidate:

1. Replace the GPUI and `gpui_platform` source URL and revision in the isolated
   experiment. Keep Butter Paper's current feature selection and apply only the
   mechanical API changes proven by the disposable probe.
2. Preserve Butter Paper's target-union license, provenance, source, advisory,
   and checksum gate with no inherited GPUI-CE exceptions.
3. Run the six native compile lanes and separate
   X11/Wayland runtime checks before accepting the foundation.
4. Pin the exact commit and define an upgrade cadence. Never follow GPUI-CE
   `main` or a branch in the distributable candidate.

If GPUI-CE fails the bounded probe, the next fork-free choice is the already
verified source-preparation boundary: fetch the exact official Zed commit and
apply the checksum-bound patch during a reproducible preparation step. That
keeps the patch in Butter Paper rather than in a public GitHub fork, but Butter
Paper still owns the patch and prepared-source build process.

Do not adopt these observed patterns:

- a moving Zed or GPUI-CE branch;
- direct Zed Git with the GPL-marked runtime graph left unaudited;
- `gpui-unofficial`, whose current release family republishes renamed
  `ztracing`, `ztracing_macro`, and `zlog` crates;
- crates.io `gpui 0.2.2` or `gpui-ce 0.3.3` while programmatic accessibility is
  a maintained Butter Paper requirement; or
- an app-owned vendor tree or fork before the bounded GPUI-CE probe shows that
  a maintained public foundation cannot work.

## Evidence status

### Passed

- Inspected primary-source manifests and released artifacts for the application
  strategies above.
- Confirmed current GPUI-CE Git declares Apache-2.0, contains AccessKit and the
  split platform boundary, omits the three GPL-marked packages from its lockfile,
  and runs Linux/macOS/Windows CI.
- Confirmed released cross-platform applications use GPUI-CE-derived sources.
- Resolved the locked normal dependency graph for all six Butter Paper target
  triples: macOS ARM64/x64, Windows ARM64/x64, and Linux ARM64/x64.
- Passed Butter Paper's strict advisory, license, provenance, and source policy
  against the disposable lockfile. The result contained no inherited GPUI-CE
  advisory exceptions and none of `ztracing`, `ztracing_macro`, or `zlog`.
- Compiled the gallery for Linux x86_64 against exact GPUI-CE commit
  `c738623ffbcec2aeddc44a645cc6b74646d5cf97` after a small mechanical port.
- Passed all 26 focused Rust tests: 23 gallery tests and 3 component-state tests.

### Failed

- Current direct Zed application graphs inspected for OpenLogi, Okena, and
  rgitui still contain `ztracing`, `ztracing_macro`, and `zlog`.
- Published crates.io `gpui-ce 0.3.3` lacks the required AccessKit integration.

### Blocked

- No GPUI source is fully accepted until the native matrix and runtime gates
  pass.
- Current macOS captures and the private Hibbeler corpus were not transferred;
  neither is needed for this source-strategy research.

### Not run

- Native accessibility inspection, visual comparison, and performance
  measurements against GPUI-CE.
- Native GPUI-CE Butter Paper probes on macOS, Windows, Linux ARM64, X11, or
  Wayland. The six target dependency graphs were resolved, but cross-compilation
  and native execution were not run.
