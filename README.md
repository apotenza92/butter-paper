# Butter Paper

Butter Paper is a cross-platform desktop app for reviewing and marking up PDFs,
with a focus on architecture, engineering, and construction workflows.

The current release is version 0.0.7. Stable and beta desktop variants are
published for macOS, Windows, and Linux.

## Development

Requirements:

- Node.js 24
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

Create an unpacked desktop package and smoke-test it:

```sh
pnpm --dir apps/desktop package
pnpm test:package:desktop
```

The tagged release workflow targets macOS, Windows, and Linux on both ARM64
and x64. In-app stable/beta updates are currently supported only for signed
and notarized macOS packages; Windows and Linux packages do not advertise
updater feeds.

## License

[MIT](LICENSE)
