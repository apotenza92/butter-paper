# Cross-project release and signing migration

Status: approved execution document; implementation in progress; no Butter Paper release has been published

Last repository, release, and GitHub control-plane inventory: 23 July 2026

This document persists the coordinated migration requested by the repository owner. It is an explicit, temporary exception to the normal rule that changing work state belongs in GitHub issues and pull requests. Do not create another tracked plan, handoff, worklog, roadmap, or duplicate migration document.

When every exit criterion in this document has passed, move any genuinely durable conventions into the relevant repository `AGENTS.md` files, retain user-visible history in changelogs, and delete this document. Git history remains the recovery mechanism.

## Outcome

Build, test, sign, notarize, update, and publish every maintained desktop application from GitHub-hosted native runners without depending on a local Mac. Keep recoverable credential masters in 1Password, operational release credentials in protected GitHub environments, beta publication automated after required checks, and stable publication human-approved.

The migration covers:

- Butter Paper
- Macsimize
- Dockmint
- Caul
- Facebook Messenger Desktop
- Fraia once its packaging boundary is ready

The inventory found no other current repository under the owner account that signs or notarizes a macOS application. Re-run the inventory before final cleanup because repositories can be added after this snapshot.

## Non-negotiable safety boundaries

- Inspect `git status` and the repository's complete `AGENTS.md` before working in any repository.
- Preserve unrelated changes. Do not combine release migration work with an existing dirty cleanup or UI change unless the overlap has been reviewed explicitly.
- Do not print, log, upload as an artifact, or place in source control any certificate private key, certificate password, App Store Connect private key, app-specific password, Sparkle private key, package-store credential, or service token.
- Subagents may inspect secret names and references but must not retrieve or handle secret values.
- Do not attempt to extract an existing secret from GitHub Actions. GitHub secret values are intentionally non-recoverable.
- Do not revoke a Developer ID certificate used by a published application.
- Do not rotate a Sparkle signing key without a separately reviewed key-transition design.
- Do not delete a released stable or beta bundle identifier.
- Do not tag, publish, promote, revoke, delete, or alter GitHub remote settings without the repository owner's explicit authorization required by that repository.
- Stable releases always require a final human confirmation. Preserve stricter repository-specific confirmation phrases.
- Facebook Messenger Desktop stable publication requires the repository's exact confirmation phrase: `yes do it`.
- Keep CI and release permissions minimal. Never give untrusted pull-request code access to release secrets or write-capable tokens.
- Keep local disposable output in ignored directories and temporary system directories.

## Agent execution and concurrency

Use subagents and concurrency whenever work can be divided into concrete, bounded, independent tasks. The lead agent remains responsible for integration, credentials, destructive decisions, and the final verification record.

### Default team shape

When four concurrency slots are available, prefer:

1. Lead agent: critical path, integration, credential-sensitive work, final diff and verification.
2. Subagent A: read-only workflow/configuration audit or a separate repository's deterministic tests.
3. Subagent B: updater/feed/package validation or another independent repository audit.
4. Subagent C: stale-reference, security, documentation, and cleanup audit.

Use fewer agents when the work is inherently serial. Do not spawn agents merely to satisfy a quota.

### Work that should run concurrently

- Read-only audits of different repositories.
- Static checks and deterministic tests in different repositories.
- ARM64 and x64 build/test jobs on separate runners.
- Windows, Linux, and macOS package validation after shared source checks pass.
- Updater metadata validation and package-content inspection when neither mutates shared output.
- Stale-reference searches and documentation checks after an implementation patch stabilizes.

### Work that must remain serialized

- Apple certificate or App Store Connect key creation.
- 1Password recovery/import operations.
- GitHub secret or environment mutation.
- Changes to the same file or overlapping subsystem.
- Tagging, release publication, package-store promotion, Homebrew updates, certificate revocation, key revocation, and credential deletion.
- Final integration of subagent changes into a dirty worktree.

### Repository ownership rule

- Assign at most one writing agent to a repository at a time.
- Other agents may run read-only audits or tests against that repository, but must not edit overlapping files.
- Prefer separate worktrees for simultaneous write tasks only when explicitly requested and when the repository state is clean enough to support them safely.
- Every subagent handoff must list files inspected or changed, commands run, results, assumptions, and unresolved risk.
- The lead agent must review all subagent diffs and re-run the integration-level checks. A subagent report is not final proof.

### Credential boundary

- Credential-sensitive GUI and command-line operations remain with the lead agent.
- A subagent may propose secret names, environment mappings, or migration order, but may not create, copy, rotate, revoke, or delete credentials.
- If a command would echo a secret or embed it in an argument visible to logs or process listings, stop and redesign the operation.

## Cleanup rule for every phase

Cleanup is part of each element, not a deferred optional task.

For every phase:

1. Search for all current definitions and references before changing anything.
2. Classify each item as current, replacement-in-progress, historical, generated/disposable, or obsolete.
3. Implement and verify the maintained replacement.
4. Remove obsolete scripts, commands, configuration, tests, documentation, generated artifacts, and duplicated instructions made unnecessary by that replacement.
5. Update package commands, workflows, documentation, imports, and links that referenced removed paths.
6. Search again for dangling paths, old secret names, old bundle identifiers where no compatibility obligation remains, obsolete environment variables, and duplicated configuration.
7. Run repository hygiene plus the complete deterministic test gate.
8. Preserve legitimate history in changelogs and Git history.

Do not remove compatibility assets that active installed applications still require. In particular, updater feeds, bundle identifiers, signing certificates, Sparkle public keys, and package aliases need explicit migration proof before cleanup.

## Current inventory

| Project | Stack | Current release credential model | Current release evidence | Principal gap |
| --- | --- | --- | --- | --- |
| Butter Paper | Electron, React, pnpm | Electron Builder variables expected; GitHub secrets not configured | No package workflow run | Complete signing, notarization, updater, publication, and package testing |
| Macsimize | Swift, Xcode, Sparkle | Explicit base64 P12 import and App Store Connect API key | Successful `v0.3.8` workflow | Move required tests into CI, use native Intel runner, add Gatekeeper/stapler checks and environments |
| Dockmint | Swift, Xcode, Sparkle | Explicit base64 P12 import and App Store Connect API key | Successful `v0.4.1` workflow | Move required tests into CI, use native Intel runner, finish obsolete Docktor migration cleanup, add release hardening |
| Caul | Electron, React, Rust, Swift | Electron Builder `CSC_LINK` and Apple app-specific password | Successful `v0.1.21` workflow | Adopt canonical credentials, expand/test architectures where supported, verify nested helpers and updater |
| Facebook Messenger Desktop | Electron, TypeScript | Electron Builder `CSC_LINK` and Apple app-specific password | Successful `v1.3.1-beta.40` workflow | Migrate last without breaking stable/beta or package-store channels; split native Mac architectures |
| Fraia | Electron, React, Rust | No release workflow | Not release-ready | Stabilize packaging boundary and packaged persistence before adopting the common contract |

The native Swift projects currently demonstrate the strongest credential handling pattern. They import an encrypted P12 into a temporary Keychain and use App Store Connect API keys with `notarytool`. Use that as the canonical baseline while preserving framework-specific packaging behavior.

### Live GitHub control-plane baseline

This metadata-only inventory was refreshed through the GitHub API on 23 July 2026. GitHub secret values were not and cannot be retrieved.

| Repository | Existing environments | Existing repository variables | Existing repository secrets | Secret scanning / push protection | Immutable releases |
| --- | --- | --- | --- | --- | --- |
| Butter Paper | None | None | None | Disabled / disabled | Disabled |
| Macsimize | `github-pages` | None | Five Apple signing/notarization entries (including key and issuer IDs currently stored as secrets), `APPLE_SIGNING_IDENTITY`, `APPLE_TEAM_ID`, `HOMEBREW_TAP_TOKEN`, `SPARKLE_PRIVATE_ED_KEY` | Disabled / disabled | Disabled |
| Dockmint | `github-pages` | Four Dockmint transition flags | Five Apple signing/notarization entries (including key and issuer IDs currently stored as secrets), `APPLE_SIGNING_IDENTITY`, `APPLE_TEAM_ID`, `HOMEBREW_TAP_TOKEN`, `SPARKLE_PRIVATE_ED_KEY` | Disabled / disabled | Disabled |
| Caul | `github-pages` | None | `APPLE_ID`, `APPLE_APP_SPECIFIC_PASSWORD`, `APPLE_TEAM_ID`, `CSC_LINK`, `CSC_KEY_PASSWORD`, `HOMEBREW_TAP_TOKEN`, `PAGES_DEPLOY_KEY` | Disabled / disabled | Disabled |
| Facebook Messenger Desktop | `github-pages` | None | `APPLE_ID`, `APPLE_APP_SPECIFIC_PASSWORD`, `CSC_LINK`, `CSC_KEY_PASSWORD`, `FLATPAK_GPG_PRIVATE_KEY`, `HOMEBREW_TAP_TOKEN`, `SNAPCRAFT_STORE_CREDENTIALS`, `WINGET_TOKEN` | Enabled / enabled | Disabled |
| Fraia | None | None | None | Unavailable on the current private-repository plan | Disabled |

The authenticated owner is `apotenza92` (GitHub user ID `53612685`). Butter Paper and Fraia have no public releases. The latest published migration baselines remain Macsimize `v0.3.8`, Dockmint `v0.4.1`, Caul `v0.1.21`, Messenger stable `v1.3.0`, and Messenger beta `v1.3.1-beta.40`.

All six repositories currently give `GITHUB_TOKEN` read-only permissions by default and do not let Actions approve pull requests. Actions are enabled with `allowed_actions=all`, but repository-level full-SHA enforcement is disabled. Every non-local action in the five locally prepared release worktrees is nevertheless pinned to a full 40-character commit SHA. The five public releasing repositories currently have no rulesets.

A same-day local trust-boundary refresh confirmed that pull-request workflows cannot reach release secrets or protected release environments, stable/beta approval is consumed only by final publication, and public immutable release verification precedes updater-feed, Homebrew, and store advancement. Macsimize and Dockmint now verify immutable-release policy before creating any draft rather than only before making it public. Caul retains its trusted manual Pages recovery path while its pull-request workflow is mechanically required to remain secret-free. Messenger mechanically allowlists the trusted triggers of every secret-bearing release or Snap workflow, and Fraia now fails its pre-secret package gate when the required root `LICENSE` is absent. Focused contract tests, workflow lint, repository hygiene, changed-file secret/path scans, and diff checks pass. A fresh signing-reference inventory of the local code workspace found no additional current macOS-signing project beyond the six already listed.

Do not delete the existing repository-level secrets or Dockmint transition variables merely because their replacements are prepared locally. GitHub secret values are non-recoverable, and compatibility or rollback may still require the existing entries until a replacement beta passes.

### Verified Developer ID baseline

The locally recovered identity and the public Macsimize `v0.3.8`, Dockmint `v0.4.1`, Caul `v0.1.21`, Messenger `v1.3.0`, and Messenger `v1.3.1-beta.40` packages all use the same certificate:

- Identity: `Developer ID Application: Alexander Potenza (27JL2VERNC)`
- Team: `27JL2VERNC`
- Leaf-certificate SHA-256: `C20E3A100252224861FF8474DEBB21E5A120210E7CD61905EFDA0B6464E18594`
- Keychain/identity SHA-1: `85871BAFAF0ADEFAD7083A599B7DB375CA34303E`
- Serial: `55299A2BB4DBB5C6`
- Validity: 29 December 2025 through 1 February 2027

Do not confuse the 40-hex Keychain identity hash with the 64-hex certificate SHA-256 required by release verification. No Developer ID rotation is required for this migration. The prior-certificate variable should initially equal the current SHA-256 and remains a separate N-1 trust input so a future reviewed rotation does not weaken candidate verification.

The recovery identity was proven in an isolated temporary Keychain and then removed as intended. A metadata-only scan on 23 July 2026 found no project P12 or App Store Connect P8 backup in `Downloads`, `Documents`, or `code`; the only matches were third-party test fixtures. The default login Keychain currently exposes only the Apple Development identity, not the Developer ID private key. Therefore the maintained 1Password recovery source must be unlocked and re-proven before a fresh current-source Butter Paper build; do not mistake the absence from the default Keychain for a need to create or rotate the still-valid Developer ID certificate.

### Local Butter Paper notarization evidence

Apple accepted fresh current-source ARM64 DMG notarization job `294d4bf1-1844-4320-903c-059c2d648a1e` on 23 July 2026 with zero issues and status `Ready for distribution`. The ignored proof under `apps/desktop/release/current-source-proof/stable/arm64` was built with pnpm `10.33.0`, Xcode-beta, the recovered Developer ID P12, and the preferred `PQ2Z5SK4TR` P8. It passes:

- exact P12 fingerprint, team, hardened-runtime, timestamp, entitlement, 16 Mach-O, and 9 signed-bundle verification;
- app and DMG notarization, stapling, `stapler`, Gatekeeper, DMG/ZIP equivalence, ZIP integrity, and DMG checksum verification;
- post-staple blockmap, SHA-256, updater SHA-512/size, and public `latest-mac.yml` assembly checks;
- isolated packaged launch, PDF annotation round-trip, protected custom AEC/Fit Width/Fit Page/Butter Canvas icons, Fit controls, Butter Canvas save/reopen, and user-data persistence.

The final DMG is `135187200` bytes with SHA-256 `59275e809b573d4f6eb25ad8a6857818b1dea11be4175d5b43a3f0995afd3c56`. The final ZIP is `149771775` bytes with SHA-256 `c0f7cdad803e4cc8d7138769816a5b61edc4ff9c6c31daadd002926fb84823bd`. Both are local proof artifacts only; version `0.0.1` has not been tagged or published.

The older ignored `release/stable/arm64` evidence from job `412507d0-9651-42eb-9e6f-c384efd2ddef` predates the final updater source and is no longer the current proof. It remains untouched because ignored artifacts are not recoverable from Git history; remove it only under an explicit cleanup decision.

The evidence also exposed that stapling mutates the DMG after Electron Builder writes `latest-mac.yml`. The maintained build now refreshes the DMG size and SHA-512 after stapling and rebuilding its blockmap, while the package verifier validates every advertised DMG/ZIP byte count and digest. Do not treat a notarization acceptance alone as proof that updater metadata remains current.

### Verified recovery inventory

On 23 July 2026 an approved recovery drill used the authenticated 1Password CLI only inside the private tmux session. The recovered P12 imported into an isolated temporary Keychain, exposed the exact expected Developer ID identity and fingerprints above, and signed and verified a fresh temporary Mach-O. The temporary Keychain, decoded P12, password, item records, tokens, and all other retrieved files were then deleted and their absence verified.

Two distinct App Store Connect P8 keys remain valid for issuer `69a6de7d-6513-47e3-e053-5b8c7c11a4d1` and team `27JL2VERNC`:

- `PQ2Z5SK4TR`, held in the structured Macsimize notary recovery record. Its P8 parses and `notarytool history` returns accepted submissions. This is the preferred shared migration candidate because its maintained record already carries the key ID, issuer, and team metadata.
- `3KUJSSLW7Q`, held in the older Dock Actioner notary record. Its distinct P8 also parses and authenticates to the same issuer. Retain it as a legacy rollback credential until every replacement beta succeeds; do not revoke it merely because the preferred candidate works.

The recovered Macsimize Sparkle private key derives exactly the `SUPublicEDKey` bundled in the maintained app. The Homebrew token authenticates as `apotenza92` with write/admin access to `apotenza92/homebrew-tap`. The WinGet token authenticates as `apotenza92` with the classic `public_repo` scope and read access to `microsoft/winget-pkgs`, which is consistent with a fork/pull-request submission route. The Butter Paper app-specific password is present but remains a legacy credential; do not use it now that both P8 routes are proven.

No matching 1Password recovery record was found for the Dockmint Sparkle private key, Messenger Snap credential, Messenger Flatpak key, or Caul Pages deployment key. Existing GitHub secret values cannot be read back. Recover each missing master from an independently proven source before replacing or deleting its current GitHub secret; otherwise the corresponding publication route remains blocked.

## Target credential contract

### GitHub release secrets

- `APPLE_SIGNING_CERTIFICATE_P12_BASE64`
- `APPLE_SIGNING_CERTIFICATE_PASSWORD`
- `APPLE_NOTARYTOOL_KEY_P8_BASE64`
- `SPARKLE_PRIVATE_ED_KEY` only for applications using Sparkle
- `HOMEBREW_TAP_TOKEN` only for repositories publishing casks
- `IMMUTABLE_RELEASES_READ_TOKEN` only in the read-only release-policy preflight described below
- Store-specific credentials only in jobs that need the corresponding store

### GitHub variables

- `APPLE_SIGNING_IDENTITY`
- `APPLE_SIGNING_CERTIFICATE_SHA256`
- `APPLE_PRIOR_SIGNING_CERTIFICATE_SHA256`
- `APPLE_TEAM_ID`
- `APPLE_NOTARYTOOL_KEY_ID` and `APPLE_NOTARYTOOL_ISSUER_ID`, scoped to `release-signing`
- Public feed, repository, channel, and bundle metadata where not maintained in source

### 1Password

Keep the recoverable master copy and human-facing metadata in a restricted vault:

- Encrypted Developer ID P12
- P12 password
- App Store Connect P8 key and identifiers
- Certificate fingerprint, creation date, expiry date, team, and intended consumers
- Recovery/import procedure
- Sparkle keys in separate per-application items
- The fine-grained immutable-release policy token, including its repository scope, Administration-read-only permission, owner, and expiry
- Package-store recovery credentials where applicable

GitHub Actions should use GitHub-native environment secrets. Do not add a 1Password service token unless a future non-GitHub execution environment creates a concrete need.

## Release environments

Create equivalent environments in every releasing repository:

- `release-signing`: tag-restricted, no reviewer; contains only macOS signing/notarization credentials and is used only by native macOS package jobs.
- `release-policy`: tag-restricted, no reviewer; contains only the repository-scoped Administration-read token used to confirm immutable releases before publication.
- `beta-release`: prerelease tags only; automated after required checks.
- `stable-release`: stable tags only; final human approval required before publication.
- `homebrew-release`: tag-restricted, no reviewer; contains only `HOMEBREW_TAP_TOKEN` and runs only after the public release and update feeds have been verified.

Butter Paper and Caul also use separate `beta-updater-verification` and `stable-updater-verification` environments. These have no reviewer and contain only optional one-time updater bootstrap state. This keeps native N-1 updater tests ahead of the final stable publication approval and avoids consuming signing credentials in the test jobs.

For Butter Paper's first release in each channel, set `MACOS_UPDATER_BOOTSTRAP_TAG` in that channel's updater-verification environment to the one exact bootstrap tag. Remove it after the successful bootstrap release. Never advance it to excuse a missing N-1 package on a later release.

Ordinary CI receives no release secrets. Fork pull requests and untrusted issue material must never enter a secret-bearing job.

## Prepared action-time GitHub migration manifest

This is the exact non-secret mutation manifest prepared from the local workflows and live GitHub metadata. It has not been executed. Before any 1Password recovery item is created or changed, any secret material is uploaded, or any environment, reviewer, security feature, or immutable-release setting is changed, obtain action-time confirmation that lists the data classes and destinations below. Never include secret values in that confirmation or in logs.

### Common repository variables

Create these repository variables in Butter Paper, Macsimize, Dockmint, Caul, and Facebook Messenger Desktop:

| Variable | Value |
| --- | --- |
| `APPLE_SIGNING_IDENTITY` | `Developer ID Application: Alexander Potenza (27JL2VERNC)` |
| `APPLE_TEAM_ID` | `27JL2VERNC` |
| `APPLE_SIGNING_CERTIFICATE_SHA256` | `C20E3A100252224861FF8474DEBB21E5A120210E7CD61905EFDA0B6464E18594` |
| `APPLE_PRIOR_SIGNING_CERTIFICATE_SHA256` | `C20E3A100252224861FF8474DEBB21E5A120210E7CD61905EFDA0B6464E18594` initially; change only during a separately reviewed certificate rotation |

Create `APPLE_NOTARYTOOL_KEY_ID` and `APPLE_NOTARYTOOL_ISSUER_ID` as non-secret variables in each repository's `release-signing` environment after recovering and proving their exact values. They identify the App Store Connect key but do not contain the private key. Do not store either identifier in GitHub Secrets or duplicate them at repository scope.

### Common environments

Create the following environments in all five releasing repositories. Each custom deployment policy is the tag pattern `v*`; the checked-in workflow's strict tag grammar selects stable versus beta.

| Environment | Protection | Contents |
| --- | --- | --- |
| `release-signing` | Tag `v*`; no reviewer | Three Apple signing/notarization secrets plus the two non-secret App Store Connect identifier variables |
| `release-policy` | Tag `v*`; no reviewer | `IMMUTABLE_RELEASES_READ_TOKEN` only; the token is limited to Administration read access and is consumed only by the pre-publication immutable-release check |
| `beta-release` | Tag `v*`; no reviewer | Per-channel publication state only; no signing private keys |
| `stable-release` | Tag `v*`; required reviewer `apotenza92` (`53612685`); `prevent_self_review=false` because this is a single-owner account | Per-channel publication state only; no signing private keys |
| `homebrew-release` | Tag `v*`; no reviewer | `HOMEBREW_TAP_TOKEN` only |

The three confidential Apple values placed only in each repository's `release-signing` environment are:

- `APPLE_SIGNING_CERTIFICATE_P12_BASE64`
- `APPLE_SIGNING_CERTIFICATE_PASSWORD`
- `APPLE_NOTARYTOOL_KEY_P8_BASE64`

### Repository-specific environments and secret destinations

- Butter Paper: create `stable-updater-verification` and `beta-updater-verification` with tag policy `v*` and no reviewer. Set `MACOS_UPDATER_BOOTSTRAP_TAG=v0.0.1-beta.1` in `beta-updater-verification` for the first beta only. The first stable tag will need the same one-time variable in `stable-updater-verification` because no prior stable package exists. Remove each variable immediately after its successful release.
- Macsimize: create a `sparkle-signing` environment with tag policy `v*`, no reviewer, and only `SPARKLE_PRIVATE_ED_KEY`. Only the updater E2E and signed-appcast jobs may use it; draft staging and stable/beta publication remain secret-free. Leave `SPARKLE_UPDATE_BOOTSTRAP_TAG` unset. All four public stable/beta ARM64/x64 appcasts resolve to accessible `v0.3.8` channel-specific ZIPs; the beta packages can execute a genuine external Sparkle CLI install/relaunch. Beta tags must run only beta-channel rows, while stable tags run stable and beta rows because a newer stable release can advance both feeds. The isolated harness must disable the immutable N-1 app's own scheduled update check before launch. Re-resolve and verify all four public baselines immediately before tagging; use the protected bootstrap variable only if a separately reviewed future first-channel release genuinely has no prior package.
- Dockmint: create a `sparkle-signing` environment with tag policy `v*`, no reviewer, and only `SPARKLE_PRIVATE_ED_KEY`. Only signed-appcast generation may use it; draft staging and stable/beta publication remain secret-free. Configure no environment-variable bootstrap override. Its four public stable/beta ARM64/x64 appcasts and ZIPs resolve to `v0.4.1`, which predates the E2E hook and is therefore covered by the exact source-pinned `sparkle-update-bootstrap.json` exception. `v0.4.2-beta.1` may validate that one-time beta-channel bootstrap boundary, but `v0.4.2-beta.2` must then update and relaunch from beta.1 before Phase 5 can pass. Beta tags must not run irrelevant stable-channel updater rows.
- Caul: create `stable-updater-verification` and `beta-updater-verification` with tag policy `v*` and no reviewer, and leave `MACOS_UPDATER_BOOTSTRAP_TAG` unset in both. Public `v0.1.21` contains both `Caul-macos-arm64.zip` and `Caul-Beta-macos-arm64.zip`; their downloaded bytes match the source-pinned legacy SHA-256 baselines. Revalidate both immediately before tagging. Move `PAGES_DEPLOY_KEY` from repository scope to the existing `github-pages` environment only after that recovery credential has been identified.
- Facebook Messenger Desktop: create `stable-updater-verification` and `beta-updater-verification` with tag policy `v*`. Set `MESSENGER_MAC_UPDATER_BOOTSTRAP_TAG=v1.3.1-beta.41` only in `beta-updater-verification` because the trusted `v1.3.1-beta.40` packages predate the E2E update hook; remove it immediately after success. Set the equivalent exact variable in `stable-updater-verification` only when the first hardened stable tag is selected.
- Facebook Messenger Desktop: create `winget-release` and `flatpak-release` with tag policy `v*`; place `WINGET_CREATE_GITHUB_TOKEN` and `FLATPAK_GPG_PRIVATE_KEY` only in their matching environments.
- Facebook Messenger Desktop: create `snap-edge-release`, `snap-security-rebuild`, and `snap-beta-promotion` with a `main` branch policy and no reviewer. Create `snap-stable-promotion` with a `main` branch policy and required reviewer `apotenza92` (`53612685`), with `prevent_self_review=false`. Place `SNAPCRAFT_STORE_CREDENTIALS` separately in each Snap environment that consumes it.
- Fraia: provision no release environments, variables, or secrets until its package boundary passes Phase 8. Its inactive workflow already names the future `release-signing`, `release-policy`, `stable-updater-verification`, `beta-updater-verification`, `stable-release`, and `beta-release` boundaries so the contract can be tested without enabling it.

### Repository security settings

- Enable secret scanning and push protection for Butter Paper, Macsimize, Dockmint, and Caul. Messenger already has both enabled. Reassess Fraia only when the private-repository plan exposes those features.
- Preserve the read-only default `GITHUB_TOKEN` permission and `can_approve_pull_request_reviews=false` in all five releasing repositories. After the maintained workflows are on the default branch, enable repository-level full-SHA action pinning; the prepared workflows already satisfy it.
- Enable immutable releases for Butter Paper, Macsimize, Dockmint, Caul, and Messenger only after each maintained workflow is on the default branch and before its first migration beta. GitHub reports all five settings disabled as of the current inventory. Do not enable it for Fraia yet.
- Provision each repository's `release-policy` environment with a fine-grained `IMMUTABLE_RELEASES_READ_TOKEN` limited to Administration read access for the intended release repository. `GITHUB_TOKEN` cannot be granted that repository-administration permission. The workflow must fail before draft publication when the live immutable-release setting is disabled, and the policy token must never reach build, signing, asset, or publication steps.
- Create no release-tag ruleset in this migration. Immutable releases already lock each published release's tag and assets; add a broader pre-publication tag ruleset only if future collaborators or a concrete threat model justify the extra release-authoring constraint.
- Leave the existing `github-pages` environments and their current deployment policies unchanged except for moving Caul's `PAGES_DEPLOY_KEY` into its already-referenced environment.

### Proposed first migration betas

These are proposed verification tags, not authorization to edit versions, commit, push, tag, or publish:

| Repository | Current source version | Proposed beta | Required preparation |
| --- | --- | --- | --- |
| Butter Paper | `0.0.1` | `v0.0.1-beta.1` | Set both root and desktop package versions exactly; add release notes; authorize the beta bootstrap variable |
| Macsimize | `0.3.8` | `v0.3.9-beta.1` | Set `MARKETING_VERSION=0.3.9`; add changelog entry; revalidate all four public N-1 appcasts and ZIPs |
| Dockmint | `0.4.1` | `v0.4.2-beta.1`, then `v0.4.2-beta.2` | Set `MARKETING_VERSION=0.4.2`; add changelog entries; revalidate all four public appcasts/ZIPs; require beta.2 to prove the real beta N-1 install/relaunch |
| Caul | `0.1.21` | `v0.1.22-beta.1` | Set package version exactly; add release notes; revalidate the two source-pinned public N-1 ZIPs |
| Facebook Messenger Desktop | `1.3.1-beta.40` | `v1.3.1-beta.41` | Set package version exactly; add release notes; authorize the one-time updater-hook bootstrap variable |
| Fraia | `0.1.0` | None | Complete and prove Phase 8 first |

### Safe execution order

1. Unlock 1Password and complete a metadata-first inventory of the shared P12/P8 recovery material and each per-app/store credential without retrieving values.
2. Prove the P12 fingerprint and notarization key from temporary, protected files; clean those files and the temporary Keychain.
3. Present the exact recovered item names, data classes, repositories, environments, and destination names for action-time approval.
4. After approval, create environments and variables, then copy secrets directly from protected temporary files or standard input without echoing them. Do not delete any old secret.
5. Verify names, environment policies, reviewer configuration, and secret timestamps through read-only GitHub API calls.
6. After separate source-publication approval, commit and push the maintained workflows and version/release-note changes.
7. Enable the approved security and immutable-release settings, then obtain separate explicit approval before creating and pushing each beta tag.
8. Verify hosted native jobs, published artifacts, N-1 updates, feeds, Homebrew/store results, checksums, and provenance. Remove one-time bootstrap variables immediately after their successful release.
9. Only after replacement beta proof, prepare an exact legacy-secret, credential, transition-variable, and certificate cleanup list. Obtain a separate destructive approval before removing, archiving, revoking, or deleting any target.

## Shared macOS release gate

Every macOS architecture must pass all applicable checks:

1. Required static analysis and deterministic tests.
2. Build on its native runner.
3. Confirm the expected Developer ID fingerprint.
4. Confirm hardened runtime and secure timestamp.
5. Inspect entitlements and reject `get-task-allow` in release artifacts.
6. Verify every nested executable, framework, helper, XPC service, and native module.
7. Run `codesign --verify --deep --strict`.
8. Submit with `notarytool` and require `Accepted`.
9. Retrieve and inspect the notary log on failure and review warnings on success.
10. Staple the ticket.
11. Run `xcrun stapler validate`.
12. Run Gatekeeper assessment with `spctl --assess`.
13. Validate archive integrity and expected contents.
14. Launch the packaged application in an isolated profile where supported.
15. Verify stable/beta bundle identity, icon, feed URL, and update public key.
16. Test an update from the previous released version.
17. Generate checksums and provenance for public artifacts.

Native runner mapping:

- ARM64: `macos-15` or a later explicitly pinned ARM64 image.
- x64: `macos-15-intel` or a later explicitly pinned Intel image.

Do not treat cross-compiling x64 on an ARM runner as native x64 runtime verification.

## Shared cross-platform gate

Where an application supports the platform:

| Platform | Architecture | Minimum package proof |
| --- | --- | --- |
| macOS | ARM64 | Sign, notarize, staple, Gatekeeper, launch, updater |
| macOS | x64 | Sign, notarize, staple, Gatekeeper, native Intel launch, updater |
| Windows | ARM64 | Build, install/uninstall, launch, signature and updater where explicitly supported; otherwise prove updater metadata is absent and the UI is honest |
| Windows | x64 | Build, install/uninstall, launch, signature and updater where explicitly supported; otherwise prove updater metadata is absent and the UI is honest |
| Linux | ARM64 | Package-content check, install or extract, launch smoke, updater where explicitly supported; otherwise prove updater metadata is absent and the UI is honest |
| Linux | x64 | Package-content check, install or extract, launch smoke, updater where explicitly supported; otherwise prove updater metadata is absent and the UI is honest |

Do not claim an architecture is supported until its package runs on that architecture. Windows public-trust signing remains a separate decision because the recommended Microsoft service has regional eligibility constraints; do not put an ordinary exportable Windows signing key into GitHub as a shortcut.

## Phase 0: Stabilize worktrees and capture the baseline

### Work

- Review current dirty changes in all scoped repositories.
- Finish, preserve, or explicitly separate existing cleanup and UI migrations before touching release files.
- Capture current workflow files, secret names, environment names, latest successful runs, release assets, updater feeds, Homebrew casks, and store channels.
- Download current public artifacts into ignored temporary directories and validate them as the pre-migration baseline.
- Record which release behavior is contractual and which machinery is obsolete.

### Parallel work

- One read-only subagent per repository group.
- A separate subagent may inspect published artifacts and updater feeds.
- A cleanup subagent may search for stale signing and release references.

### Cleanup within the phase

- Remove no compatibility asset yet.
- Delete only confirmed disposable local artifacts already ignored by the repository.
- Identify, but do not yet delete, duplicated workflows, obsolete commands, and stale documents.

### Exit criteria

- Every dirty file is understood.
- Current release behavior and rollback points are known.
- No credential value has been exposed.

## Phase 1: Recover signing and notarization masters

### Work

- Use the 1Password CLI (`op`) for a metadata-first inventory of relevant vault items and attachments. Retrieve secret material only into a protected temporary location for an immediate verification step; never print it or pass it to a subagent. Use the 1Password app when an item is ambiguous, attachment access needs human review, or a destructive change needs confirmation.
- Inspect 1Password item names and attachments for the P12 already used by Macsimize and Dockmint.
- Inspect 1Password for the matching App Store Connect P8 keys.
- Search approved local backup locations for P12/P8 files without printing their contents.
- Compare any recovered certificate fingerprint with the active Developer ID certificate.
- Import recovered material into a temporary Keychain and prove the signing identity.
- Verify notarization authentication with a harmless history or validation request.
- Inspect local signing identities through Keychain Access and Xcode's Manage Certificates interface.
- Treat the Apple Developer certificates portal as authoritative for certificate status and revocation, App Store Connect's Users and Access/Integrations area as authoritative for API keys, and the Apple Account security interface as authoritative for app-specific passwords.
- If the private key cannot be recovered, generate a CSR in Keychain Access and create a new Developer ID Application certificate through the Apple Developer portal or Xcode's supported certificate workflow without revoking the existing one.
- Export the working identity to a new encrypted P12 and test recovery from the stored copy.

### Serialized work

All credential handling, Apple portal interaction, 1Password access, Keychain import/export, and key creation remain with the lead agent.

### Cleanup within the phase

- Remove temporary Keychains and temporary decoded key files after verification.
- Do not revoke or delete old certificates, API keys, or app-specific passwords.
- Consolidate duplicate recovery notes into the maintained 1Password item only after the recovered identity is proven.

### Exit criteria

- A recoverable Developer ID private key exists outside GitHub.
- A recovery drill succeeds from the encrypted master.
- A working notarization credential is recoverable.

## Phase 2: Butter Paper local release proof

### Work

- Complete Electron Builder signing/notarization configuration using the canonical credential contract.
- Build the host-native ARM64 app and DMG/ZIP.
- Verify nested Electron helpers and `@napi-rs/canvas` native modules.
- Run the complete shared macOS gate.
- Launch the packaged application and verify PDF opening, document persistence, annotations, custom AEC icons, Fit Width/Fit Page controls, and Butter Canvas.
- Validate updater configuration without publishing a release.

### Parallel work

- Subagent A: package-content and native-module audit.
- Subagent B: packaged E2E and accessibility checks.
- Subagent C: signing configuration, stale-reference, and cleanup review.
- Lead: credential-sensitive build, notarization, Gatekeeper, and integration.

### Cleanup within the phase

- Remove superseded package commands and duplicate notarization branches after the maintained route passes.
- Remove stale release documentation and references to unsupported packaging systems.
- Keep Electron Forge only for development and Electron Builder only for packaging.

### Exit criteria

- A local ARM64 artifact is signed, notarized, stapled, Gatekeeper-accepted, and smoke-tested.
- The P12 recovery copy can reproduce the signing identity in a clean temporary Keychain.

The fresh current-source ARM64 proof recorded above satisfies this phase's local artifact and clean-Keychain recovery criteria. The older pre-updater proof is retained only pending an explicit ignored-artifact cleanup decision and must not be published.

## Phase 3: Butter Paper CI, updater, and first release

### Work

- Separate unprivileged CI from privileged package/release workflows.
- Configure the six-target native matrix.
- Configure pnpm supported architectures so target-specific optional native modules are present.
- Import the P12 into a temporary Keychain explicitly.
- Use the App Store Connect API key for CI notarization.
- Add per-architecture package inspection and launch tests.
- Implement stable and beta updater feeds, version/tag validation, publish ordering, checksums, provenance, and retention.
- Test previous-version-to-current-version updates, channel isolation, corrupted downloads, and signature rejection.
- Create and test stable/beta Homebrew casks after the first accepted public artifact exists.
- Publish beta only after all required jobs pass.
- Publish stable only after explicit approval.

### Parallel work

- OS build jobs run concurrently after shared checks.
- Updater metadata tests may run concurrently with package inspection after artifacts exist.
- Homebrew generation may be prepared concurrently but must publish only after the final release assets are immutable.

### Cleanup within the phase

- Remove obsolete unsigned fallback paths from tagged releases.
- Remove duplicate workflow configuration and unused secrets after the replacement beta succeeds.
- Remove stale package commands and dangling artifact globs.
- Keep manual dispatch only where it has a concrete recovery purpose.

### Exit criteria

- All claimed targets build and run natively.
- Both Mac architectures pass the full signing gate.
- Stable and beta update tests pass.
- A beta is publicly downloadable and updateable.
- Stable release remains human-gated.

## Phase 4: Macsimize hardening and migration

### Baseline to preserve

- Stable and beta bundle identities.
- ARM64 and x64 artifacts.
- Sparkle feed/public-key compatibility.
- Homebrew stable and beta casks.

### Work

- Move `xcodebuild test` from the local-only release script into required CI.
- Run CI for stable and beta variants.
- Move x64 packaging to an Intel runner.
- Add Gatekeeper and stapler validation.
- Verify every Sparkle framework helper and XPC signature.
- Verify generated appcast signatures against the bundled public key.
- Test N-1 stable and beta updates on both architectures.
- Move required deterministic settings/titlebar behavior tests into CI test mode.
- Keep any TCC-dependent Accessibility GUI check as an explicit final Mac confirmation until it can be deterministic on a hosted runner.
- Add release environments and narrow permissions per job.

### Cleanup within the phase

- Remove local release-script checks that become exact duplicates of required CI, while retaining fast pre-tag validation where useful.
- Remove unused fallback Apple-ID notarization only after API-key recovery and rotation are proven.
- Remove duplicate signing variables and stale documentation.

### Exit criteria

- A beta release passes native ARM64 and Intel tests, Sparkle update tests, notarization, Gatekeeper, Homebrew, and provenance.

## Phase 5: Dockmint hardening and migration

### Baseline to preserve

- Stable and beta bundle identities.
- Existing installed-user update path.
- Sparkle public key and appcasts.
- Homebrew casks.

### Work

- Add required Xcode unit tests to CI.
- Move x64 packaging to an Intel runner.
- Add Gatekeeper and stapler validation.
- Run deterministic settings and Dock-action logic in test mode.
- Verify Sparkle bundle signatures and N-1 updates.
- Keep real event-tap and Accessibility behavior as an explicit final Mac check where TCC blocks hosted automation.
- Add release environments and per-job permissions.
- Audit the completed Docktor-to-Dockmint transition before removing legacy machinery.

### Cleanup within the phase

- Remove transition flags, legacy feed mirroring, obsolete Docktor aliases, dead bundle-ID branches, stale scripts, and migration documentation only after active users no longer rely on them and update continuity is proven.
- Preserve historical release notes in the changelog.

### Exit criteria

- A beta release passes native ARM64 and Intel tests, Sparkle updates, notarization, Gatekeeper, Homebrew, and provenance.
- Any retained legacy Docktor path has a documented current consumer.

## Phase 6: Caul migration and testing

### Work

- Adopt the canonical P12 and App Store Connect API-key contract.
- Move team and signing identity values to variables.
- Verify the Electron app, Rust backend, Swift audio helper, frameworks, and nested binaries are signed.
- Run renderer checks, Rust tests, packaged onboarding, helper/backend launch, and permissions-state tests.
- Avoid global TCC resets.
- Audit Intel support for every native component before adding macOS x64.
- Test Windows and Linux native packages where supported.
- Test stable/beta updater behavior and package-store/Homebrew publication.
- Release a beta before migrating stable.

### Parallel work

- Renderer tests, Rust tests, native-helper audit, and release-reference cleanup may run concurrently.
- Packaged macOS verification remains serialized after artifact assembly.

### Cleanup within the phase

- Remove old `CSC_LINK` mappings, unused `APPLE_TEAM_ID` secret copies, obsolete VM scripts, stale packaging plans, and duplicated configuration only after their replacements pass.
- Remove no diagnostic audio backend that still has a documented current purpose.

### Exit criteria

- A beta passes all supported native package checks, nested signature verification, updater tests, and notarization.

## Phase 7: Facebook Messenger Desktop migration and testing

### Work

- Migrate last because the current release and update channels serve users.
- Preserve feed filenames, release asset names, stable/beta behavior, and package-store channels.
- Adopt the canonical credential contract and native Mac runners.
- Run the full deterministic repository test suite.
- Test packaged tray, close/quit, notification policy, updater, and launch behavior without a live Facebook account in the required gate.
- Test previous beta to new beta and previous stable to new stable.
- Validate Linux AppImage, Windows installers, Snap, Flatpak, Winget, and Homebrew publication boundaries.
- Publish a beta using the replacement identity before stable migration.

### Parallel work

- Platform package jobs and store-specific dry validation may run concurrently after shared tests.
- Public issue/live-account checks remain supplemental and isolated from release secrets.

### Cleanup within the phase

- Remove old signing secret references, duplicated update scripts, obsolete VM/test machinery, and stale package-store configuration only after current deterministic replacements and real release evidence exist.
- Preserve published beta changelog history and compatibility aliases still consumed by users.

### Exit criteria

- A downloadable beta passes every platform's required gate and updates from the prior beta.
- Stable remains untouched until explicit approval.

## Phase 8: Fraia release foundation

Do not add signing merely to create activity. First stabilize the package boundary.

### Work

- Define the Electron/Rust packaged boundary and native sidecars.
- Make packaged CalculiX fail closed to the reviewed bundled binary. Packaged tests must reject `FRAIA_CCX_PATH`, user-data runtimes, Homebrew locations, and `PATH` fallbacks rather than passing with an unbundled host installation.
- Add and verify the five supported CalculiX payloads and their five matching reviewed provenance manifests, build recipes, and `THIRD_PARTY_NOTICES.txt` files from an approved redistributable source.
- Add maintained `icon.icns`, `icon.ico`, and `icons/512x512.png` assets and reject Electron's default icon.
- Add the repository-level MIT `LICENSE` file already declared by the package metadata.
- Prove project save/reopen, the exact bundled CalculiX/backend launch, and packaged persistence.
- Add deterministic packaged E2E tests.
- Add the common platform matrix only for supported targets.
- Adopt the canonical credentials, environments, updater channels, and release gates from the start.

### Cleanup within the phase

- Remove obsolete renderer experiments, generated bundles, stale package commands, and duplicated architecture documentation as the maintained packaging boundary becomes authoritative.

### Exit criteria

- Fraia has a tested package boundary and can adopt signing without hiding unresolved persistence or backend failures.

## Phase 9: Common supply-chain hardening

Apply consistently after project-specific migrations prove their behavior:

- Minimize permissions per job.
- Pin third-party actions to full commit SHAs and mechanically enforce that policy. While the owner's pause on scheduled maintenance remains active, review and update those pins manually; do not add Dependabot, Renovate, or equivalent scheduled update automation.
- Add concurrency guards to release, updater-feed, and Homebrew publication jobs.
- Generate GitHub artifact provenance for public binaries.
- Produce checksums and, where useful, SBOMs.
- Make release publication depend on every required architecture.
- Validate that release assets are immutable before updater feeds or package manifests reference them.
- Ensure logs and artifacts contain no credentials, private messages, account data, cookies, or tokens.
- Add mechanical repository checks that reject removed secret names, obsolete workflow files, tracked generated output, and dangling paths.

## Phase 10: Apple, GitHub, 1Password, Sparkle, and store cleanup

Cleanup only after every affected application has released successfully with the maintained replacement.

Use the 1Password CLI (`op`) for repeatable inventory and reference searches, with the 1Password app as the human confirmation surface for ambiguous or destructive changes. Use Xcode and Keychain Access for local identity inspection and removal, the Apple Developer portal for certificate and provisioning-profile status or revocation, App Store Connect for API-key management, and the Apple Account security interface for app-specific passwords. Reconcile fingerprints and key IDs across all surfaces before deleting or revoking anything.

All credential deletion, certificate revocation, key revocation, and 1Password item removal is serialized lead-agent work and requires a final explicit confirmation listing the exact targets and their proven replacements.

### Apple certificates

- Keep the locally usable Apple Development certificate dated 13 June 2026.
- Treat the Apple Development certificates marked `Not in Keychain` as revocation candidates only after confirming no other Mac uses them.
- Keep the existing Developer ID certificate used by published apps. Let it expire naturally rather than revoking it.
- Keep the new or recovered Developer ID identity and its tested recovery copy.

### Apple keys and passwords

- Map each App Store Connect key ID to its repositories and purpose.
- Revoke only unmapped or superseded keys after all consumers have migrated.
- Retire app-specific passwords only after API-key notarization succeeds everywhere that used them.
- Keep released bundle identifiers and required provisioning profiles.
- Remove expired or orphaned profiles only after entitlement and app mapping.

### GitHub

- Remove repository secrets no workflow references.
- Convert non-secret identifiers to variables.
- Remove obsolete environments after no workflow references them.
- Remove superseded release workflows and commands after the replacement has a successful real run.

### 1Password

- Keep one maintained recovery item for the shared Developer ID identity.
- Keep separate per-app Sparkle key items.
- Remove duplicate notes and obsolete credentials only after fingerprint/key-ID comparison.
- Use `op` to search every scoped repository reference before proposing an item deletion, without exposing the item's secret fields.
- Prefer archiving an obsolete 1Password item before permanent deletion when the vault and retention policy support recovery.
- Perform a final recovery drill before deleting any duplicate that might be the only usable master.

### Sparkle and package stores

- Never delete a Sparkle private key still trusted by installed applications.
- Remove legacy appcast credentials only after update continuity is verified.
- Remove store tokens only after the corresponding publication route is retired or replaced.

## Phase 11: Final release and recovery drills

For every maintained releasing application:

1. Import the maintained P12 into a clean temporary Keychain.
2. Reconstruct notarization authentication from the maintained recovery source.
3. Run the full deterministic test gate.
4. Build on every supported native architecture.
5. Release a beta through the maintained GitHub environments.
6. Install the prior beta and update to the new beta.
7. Verify signatures, notarization, Gatekeeper, updater feeds, Homebrew, and relevant stores.
8. Review workflow permissions, logs, artifacts, and attestations.
9. Obtain explicit approval before any stable release.
10. Search all scoped repositories for obsolete secret names, removed paths, stale bundle identifiers, and duplicate instructions.

## Completion criteria

- Butter Paper, Macsimize, Dockmint, Caul, and Messenger release without a local Mac.
- Fraia uses the same contract when its package boundary is ready.
- Every claimed architecture has native runtime proof.
- Every Mac artifact is Developer ID signed, notarized, stapled, Gatekeeper-accepted, and updater-tested.
- Required tests run in CI rather than relying on an operator's local release script.
- Stable publication has a human gate.
- Pull-request jobs cannot access release credentials.
- The Developer ID identity and notarization credential can be recovered from the maintained 1Password source.
- Public binaries have checksums and provenance.
- Every obsolete script, command, workflow, configuration, document, generated artifact, secret reference, and compatibility path identified during a phase has either been removed or has a verified current consumer.
- No active Developer ID certificate, bundle identifier, updater key, feed, or package alias was removed prematurely.
- Repository hygiene and complete deterministic tests pass in every changed repository.
- This execution document has been deleted after durable instructions are consolidated.

## Maintained references

- Apple Developer ID certificates: https://developer.apple.com/help/account/certificates/create-developer-id-certificates/
- Apple notarization: https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution
- Apple custom notarization workflow: https://developer.apple.com/documentation/security/customizing-the-notarization-workflow
- GitHub environments: https://docs.github.com/en/actions/reference/workflows-and-actions/deployments-and-environments
- GitHub artifact attestations: https://docs.github.com/en/actions/concepts/security/artifact-attestations
- GitHub immutable releases: https://docs.github.com/en/code-security/concepts/supply-chain-security/immutable-releases
- GitHub Actions full-SHA policy: https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/enabling-features-for-your-repository/managing-github-actions-settings-for-a-repository
- GitHub macOS runner inventory: https://docs.github.com/en/actions/reference/runners/github-hosted-runners
- Electron Builder notarization: https://www.electron.build/docs/notarization/
- pnpm supported architectures: https://pnpm.io/settings#supportedarchitectures
