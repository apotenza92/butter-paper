# GPUI foundation gate

Issue: [Reconcile the accepted GPUI Component foundation and reproducibility gates](https://github.com/apotenza92/butter-paper/issues/83)

## Current application candidate

The isolated Butter Paper migration uses Longbridge GPUI Component as its
default component system. The reviewed component revision is
`c27f5d5c8f70d534978c2f0739ad9e10d4e41eb4`. The compatibility application
uses the exact Zed GPUI revision required by that source:
`8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc`.

The deterministic source-preparation process produces tree digest
`630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3`.
The policy pins the component and Zed commits and trees, preparation patch,
license evidence, allowed Git sources, shared experiment source receipts, and
the prepared tree. The resolved compatibility graph has one GPUI crate
identity. It does not combine the Zed and GPUI-CE runtimes.

The preparation patch keeps Longbridge on its intended Zed API. It removes
unneeded forced profiler and runtime-shader features. It replaces the reachable
GPL-marked tracing dependency with a small, local Apache-2.0 compatibility
shim. It does not copy, modify, or relabel GPL source. The exact policy and
third-party notices are in `../gpui-component-compat/`.

This is an isolated development candidate. It is not a packaged application,
an installed application, a production promotion, or a public release.

## Historical GPUI-CE gallery

The `gpui-gallery` crate was the earlier direct-GPUI research application. It
uses GPUI-CE revision `c738623ffbcec2aeddc44a645cc6b74646d5cf97` and preserves
useful PDF, annotation, persistence, performance, and platform-domain evidence.
Its `foundation-policy.json` and `foundation-gate.mjs` qualify that historical
gallery graph only. They do not select or qualify the current application
candidate.

The current compatibility application reuses only checksum-receipted,
GPUI-independent gallery modules. UI-bound adapters are ported to the single
Zed GPUI graph. No runtime may load both GPUI crate identities.

## Exact candidate

| Item | Pin or policy |
| --- | --- |
| Component source | `https://github.com/longbridge/gpui-component` |
| GPUI Component | `c27f5d5c8f70d534978c2f0739ad9e10d4e41eb4` |
| GPUI source | `https://github.com/zed-industries/zed` |
| `gpui` and `gpui_platform` | `8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc` |
| Prepared source digest | `630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3` |
| Rust | `1.97.1`, minimal profile, `rustfmt`, and `clippy` |
| Linux development features | no defaults; required X11 and Wayland platform features only |
| Forbidden release features | `profiler`, `runtime_shaders`, `screen-capture`, `test-support` |
| Dependency policy tool | `cargo-deny 0.20.2` |

Supported production targets remain a later native qualification gate:

- `aarch64-apple-darwin`
- `x86_64-apple-darwin`
- `aarch64-pc-windows-msvc`
- `x86_64-pc-windows-msvc`
- `aarch64-unknown-linux-gnu`
- `x86_64-unknown-linux-gnu`

Rust target installation alone does not prove a native build. Apple targets
need Xcode and the Apple software development kit. Windows targets need the
Microsoft Visual C++ and Windows software development kits. Each platform also
needs fresh runtime, accessibility, package, update, and physical-device proof.

## PDF dependencies

The bounded PDF annotation writer uses a separate pinned `lopdf` adapter. See
[PDF-PERSISTENCE.md](PDF-PERSISTENCE.md) for its license rationale,
independent validation, and incomplete production boundary.

The isolated PDF worker pins `pdfium-render` 0.9.4 to immutable commit
`6cee8b9a3951832ac0ff62ce4c32800278001cb8`, with default features disabled
and only the `pdfium_7881` API feature enabled. This revision contains upstream
font and page-object double-free fixes that are not in published 0.9.3.

`pdfium-development-binaries.json` pins community development binaries by
release URL, byte count, and SHA-256. The fetch tool writes only below ignored
experiment storage. The manifest states `productionApproved: false`. Butter
Paper must not ship these binaries or fetch them at runtime. Production PDFium
needs a separate audited, reproducible, application-owned supply decision and
native qualification on every supported target.

## Reproduce the current gate

From `../gpui-component-compat/`, run the fast immutable-source gates:

```sh
node --test tests/foundation-truth.test.mjs tests/source-preparation.test.mjs
node scripts/foundation-truth.mjs
node scripts/prepare.mjs verify
node scripts/verify-cargo-graph.mjs
```

Run dependency policy with the exact configured `cargo-deny` command recorded
in the compatibility README. Run every Rust compile through both the
host-storage guard and `scripts/run-bounded-button-probe.sh`. The wrapper keeps
one Cargo job, disables incremental compilation, checks the 30 GiB preflight
and 20 GiB runtime floors, caps its owned target at 4 GiB, and retains a valid
target after ordinary test failures.

The historical gallery commands remain useful only when a change directly
affects that historical graph:

```sh
node --test scripts/foundation-gate.test.mjs
node scripts/foundation-gate.mjs --metadata-only
```

## Passing conditions

Accept an application-foundation change only when all of these conditions are
true:

1. Exact revisions, trees, patches, receipts, licenses, and the prepared digest
   reproduce from the reviewed policy.
2. The resolved graph contains one GPUI identity and no forbidden package,
   feature, source, vulnerability, yanked package, or hidden license exception.
3. GPUI Component supplies the application primitives. Butter Paper owns
   feature and document state outside component internals.
4. The selected GPUI revision keeps the framework APIs and accessibility
   integration required by the migration architecture.
5. The exact Linux development gate passes within the storage and process
   bounds.
6. Each claimed native platform passes its own compile, runtime, visual, input,
   accessibility, packaging, update, and physical-device gates.
7. Production PDFium supply and production promotion receive their separate
   human approvals.

Any upstream upgrade is a new exact-source decision. Regenerate the lockfile,
prepared digest, receipts, dependency report, and native qualification evidence
for the new immutable revisions.
