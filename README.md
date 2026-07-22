# Butter Paper

Butter Paper is a cross-platform desktop app for reviewing and marking up PDFs,
with a focus on architecture, engineering, and construction workflows.

The project is pre-release. Version 0.0.1 is being prepared; no public release
or updater channel is available yet.

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

Packaging workflows target macOS, Windows, and Linux on both ARM64 and x64.
Release publication and automatic updates are not enabled yet.

## License

[MIT](LICENSE)
