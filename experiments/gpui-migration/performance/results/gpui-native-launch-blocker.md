# GPUI native-launch blocker

Captured 19 August 2026 after `IOConsoleLocked=No`.

Some launches from the root Codex shell abort before `window-created` or any
GPUI render event. A separate approved GUI-capable agent context ran the same
signed bundle successfully, so the failure is session-specific rather than a
GPUI launch defect.
The macOS crash report identifies:

- signal: `SIGABRT`;
- main-thread stack: `HIServices.__RegisterApplication_block_invoke` →
  `_RegisterApplication` → `NSApplication sharedApplication` → GPUI
  `MacPlatform::run`;
- stderr: `Connection Invalid error for service
  com.apple.hiservices-xpcservice`;
- direct launch and the detached benchmark runner reproduce the same result;
- the smallest plain AppKit command-line probe succeeds, while a minimal
  AppKit `.app` bundle reproduces the same registration abort;
- the disposable bundle now rebuilds from a clean directory, contains one
  conventional no-space executable (`ButterPaperGPUI`), and passes
  `codesign --verify --deep --strict`;
- the runner passes `-ApplePersistenceIgnoreState YES` and the gallery filters
  those two launch arguments before opening the PDF, so stale crash dialogs do
  not become workload measurements;
- `/usr/bin/open` still reports Launch Services error `-10827`
  (`kLSNoExecutableErr`) in this Codex session, even after the clean signed
  rebuild;
- Computer Use cannot inspect or operate Finder in this session because native
  Computer Use access is not approved.

The decisive unified-log evidence for the failed root-shell attempts is a launchd denial for
`com.apple.hiservices-xpcservice` with `error = 159: Sandbox restriction`.
Electron and SDL/xemu children launched from the same Codex coalition show the
same HIServices registration failure. This is an inherited execution-sandbox
blocker, not a GPUI, Poppler, Metal, or PDF problem. It occurs before PDF
metadata, GPUI layout, Metal rendering, or the comparison state machine. The
raw failed reports are `gpui-*-unlocked-1.json` and `gpui-*-unlocked-2.json` in
this directory. Successful native evidence is in
`gpui-open-pdf-agent-3.json`, `gpui-page-navigation-agent-3.json`, and
`gpui-zoom-agent-3.json`.

Retry the same runners from a normal user Terminal or another approved
unsandboxed GUI executor when a fresh native capture is required. Do not treat
the failed root-shell process samples as workload measurements because those
attempts never created a window. The successful agent reports are valid local
development performance evidence, not packaged-release or Metal-trace proof.
