# Butter Paper

<img src="assets/butter-paper-icon.png" alt="Butter Paper icon" width="144" />

Butter Paper is a free, open-source PDF markup app for macOS, Windows, and Linux.
It is a cross-platform alternative to Bluebeam Revu for everyday document
review, including architecture, engineering, and construction workflows.

## Install

Download the package for your platform from the
[latest release](https://github.com/apotenza92/butter-paper/releases/latest):

- macOS (13 or later): `Butter-Paper-macOS-arm64.zip` (Apple silicon) or
  `Butter-Paper-macOS-x64.zip` (Intel), or `brew install --cask
  apotenza92/tap/butter-paper`. Butter Paper Beta installs beside it
  (`butter-paper@beta`).
- Windows: `Butter-Paper-Windows-x64.zip` or `-arm64.zip`; unzip and run
  `install.ps1`.
- Linux: `Butter-Paper-Linux-x64.tar.xz` or `-arm64.tar.xz`; unpack and run
  `./install-user.sh`.

Butter Paper then updates itself. `SHA256SUMS.txt` on each release lists the
packages' checksums.

## Develop

Butter Paper is a Rust application built with [GPUI](https://www.gpui.rs/)
and [GPUI Component](https://github.com/longbridge/gpui-component). PDFs are
rendered and edited by PDFium in a separate worker process.

```sh
cargo xtask pdfium                 # fetch the approved PDFium; prints its path
BP_NATIVE_DEVELOPMENT=1 BP_PDFIUM_LIBRARY=<that path> cargo run
cargo test --workspace
cargo xtask check
```

See [AGENTS.md](AGENTS.md) for the repository layout, conventions and the
release process.

## Licence

MIT. Third-party notices are generated into each package as
`THIRD_PARTY_NOTICES.md` (`cargo xtask notices`).
