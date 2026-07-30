# Butter Paper

Butter Paper is a cross-platform desktop app for reviewing and marking up PDFs,
with a focus on architecture, engineering, and construction workflows.

The current release candidate is version 0.0.12. Stable and beta are
intentionally maintained as separate application identities, so both can be
installed at once without sharing settings or updater trust.

## Development

Requirements:

- Node.js 24.16.0
- pnpm 10.33.0

Install dependencies and run the local quality gate:

```sh
pnpm install --frozen-lockfile
pnpm check
```

Start the desktop app:

```sh
pnpm dev:desktop
```

GUI-driven Electron E2E, packaged-app smoke, and updater tests run only on
disposable GitHub Actions runners. Local macOS quality checks remain headless
and do not open application windows.

## Releases and automatic updates

The tagged release workflow targets native ARM64 and x64 hosts on macOS,
Windows, and Linux:

- macOS DMG and ZIP packages are Developer ID signed, hardened, notarized,
  stapled, and Gatekeeper verified. The app updates automatically.
- Windows NSIS installers are currently unsigned. The app authenticates update
  metadata with TUF before Electron Updater verifies and installs the declared
  package bytes.
- Linux AppImages are currently unsigned and self-update through the same
  TUF-authenticated path. DEB and RPM upgrades remain the responsibility of the
  system package manager.

Windows and Linux users on 0.0.11 need to install 0.0.12 manually once because
those older packages did not contain the TUF updater. Automatic updates apply
after that bootstrap install. A stable install remains stable and a beta
install remains beta.

Every release publishes SHA-256 checksums, GitHub build provenance, exact
updater metadata, and release notes extracted from [CHANGELOG.md](CHANGELOG.md).
Those notes are also shown in the in-app update dialog. The TUF root private
key is kept offline; separate protected keys sign targets, snapshot, and
timestamp metadata. Production update feeds require HTTPS and fail closed if
metadata or payloads are corrupt, expired, redirected, missing, or signed by
an untrusted key.

Electron supplies most of the installed footprint. The package configuration
still removes source maps, unused Electron locales, duplicate renderer
dependencies, and native canvas binaries for other operating systems and
architectures.

## License

[MIT](LICENSE)
