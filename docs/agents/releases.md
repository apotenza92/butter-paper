# Packaging and releases

Read this before packaging, changing release automation or publishing a release.
Authorization is governed by [AGENTS.md](../../AGENTS.md); these commands do not
provide permission to commit, push or publish.

## Package and notices

- Package one target: `cargo xtask package --target <triple> [--channel stable,beta] --out <dir>`.
- macOS signing uses `--sign`, as in the release workflow.
- Third-party notices: `cargo xtask notices --target <triple> --out <file>`.
- User-facing package names stay Butter Paper or Butter Paper Beta, without
  version or architecture suffixes.

## Publish an authorized release

Test while working: `cargo test --workspace` and `cargo xtask check`. Releasing
runs no tests; do not add a repeated test gate to the release command.

1. Run `cargo xtask version X.Y.Z` (or `X.Y.Z-beta.N`).
2. Edit the new `CHANGELOG.md` section.
3. Commit and push to `main` within the authorized release scope.
4. Run `cargo xtask release`.

The release command checks that `main` is pushed and clean, versions agree, the
changelog contains the section and the tag is new, then pushes the tag. Release
tags must resolve to commits reachable from `main`.

`.github/workflows/release.yml` packages every target in
`.github/release-targets.json` with `cargo xtask package`. A beta tag builds
Butter Paper Beta for macOS only. It publishes one complete release with
`SHA256SUMS.txt` and attestations, then asks `apotenza92/homebrew-tap` to update
`butter-paper` and `butter-paper@beta`; the bundle file name is recorded in the
tap registry. Verify publication and the affected downstream distributions
before calling the release complete.

A manual workflow run from `main` is an unsigned dry run that publishes
nothing; use it after changing packaging. macOS releases are Developer ID
signed and notarised; Windows and Linux packages are unsigned.

## Microsoft Store

Windows release jobs also build `Butter-Paper-Windows-x64.msix` and
`Butter-Paper-Windows-arm64.msix` (`--store-out`). They are run artifacts
named `microsoft-store-windows-*`, not release assets; the Store signs them.
Its identity in Partner Center is `xtask/templates/windows/store-identity.json`
and the listing text is [docs/store/listing.md](../store/listing.md).

After a stable release is published, the release workflow's `microsoft-store`
job runs `cargo xtask store-submit`. It creates a Store submission from the
live one, replaces its packages with both MSIX files and commits it; Microsoft
then certifies and publishes the update. Watch it in Partner Center (Apps and
games > Butter Paper). A draft left unsubmitted is replaced; a submission
already in certification stops the job, so wait for it or cancel it first.

The job authenticates as a Microsoft Entra application associated with the
Partner Center account (Account settings > User management > Microsoft Entra
applications, Manager role). Its credentials are the `microsoft-store`
environment secrets `STORE_TENANT_ID`, `STORE_CLIENT_ID` and
`STORE_CLIENT_SECRET`, with the Store ID in the repository variable
`STORE_PRODUCT_ID`. The 1Password vault "Butter Paper Release" holds them and
the Entra tenant's admin sign-in. The client secret expires: when Partner
Center shows it near expiry, add a new key there, update the 1Password item
and the environment secret, then remove the old key.

### Setting up or rotating the Store credentials

The account signs in to Partner Center with a personal Microsoft account, so
the application lives in a Microsoft Entra tenant created for it
(`AlexanderPotenza.onmicrosoft.com`). Its admin sign-in is the 1Password item
"Microsoft Entra admin (AlexanderPotenza.onmicrosoft.com)", including its
one-time password; the application's credentials are the item "Microsoft
Store submission API (Partner Center app)". Both are in the vault "Butter
Paper Release". Do these steps in a browser with 1Password autofill:

1. Once only: Partner Center > Account settings > Tenants > Associate
   Microsoft Entra ID, signing in as that admin. If Microsoft asks for MFA,
   choose a different authenticator app and save its QR code to the admin
   item as a one-time password.
2. Once only: Account settings > User management > Microsoft Entra
   applications > Create, named "Butter Paper Store publishing", with the
   Manager role. Copy its tenant ID and client ID into the credentials item.
3. To create or rotate the secret: open the application, Add new key, and
   paste the key (shown once) and its expiry into the credentials item.
4. Copy the three values to the environment, reading them from 1Password
   without printing them:

   ```sh
   item="op://Butter Paper Release/Microsoft Store submission API (Partner Center app)"
   for name in STORE_TENANT_ID STORE_CLIENT_ID STORE_CLIENT_SECRET; do
     op read "$item/$name" | gh secret set "$name" --env microsoft-store
   done
   ```

5. After rotating, remove the old key in Partner Center.

Until the credentials are set, the job notes that and skips. Then submit from
a Mac with `cargo xtask store-chrome`: it downloads both packages from the
latest successful release run (`--run ID` for another), drives Partner Center
in Google Chrome in a window of its own, replaces the packages and submits
them for certification. It stops if a submission is already in
certification or the Store already has the version. Chrome must be signed in
to Partner Center as the developer account that owns the app (not a work or
tenant account) and needs View > Developer > Allow JavaScript from Apple
Events. Partner Center can take minutes to render a fresh window.

The Entra tenant for the credentials (`AlexanderPotenza.onmicrosoft.com`) is
active, with its admin's MFA key in 1Password, but the Store developer
account is an individual one, and Partner Center offers user management and
Microsoft Entra applications to company accounts only (an individual account
cannot become a company one). So no application can hold the Manager role,
the credential item's values are empty and `store-submit` cannot run: submit
with `store-chrome` or by hand.

Without either (or if both fail), submit by hand: download both artifacts with
`gh run download <run> -p 'microsoft-store-*'`, start an update submission,
replace the packages with both MSIX files and submit it for certification.

The Store copy needs the Microsoft Visual C++ runtime framework package,
which the Store installs with it.

Releases are immutable: never edit one; ship a new version. Never log, copy into
artifacts or commit a signing key or certificate.
