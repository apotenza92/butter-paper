import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import {
  buildMacUpdateMetadata,
  compareReleaseVersions,
  expectedMacIdentity,
  parseMacUpdateArguments,
  parseExecutableProcessIds,
} from '../scripts/test-macos-update.mjs';
import { resolvePriorSigningFingerprints } from '../scripts/verify-macos-package.mjs';

describe('macOS updater integration harness', () => {
  it('keeps ordinary CI deterministic and reserves GUI/package matrices for manual runs', () => {
    const workflow = readFileSync(resolve('.github/workflows/ci.yml'), 'utf8');
    expect(workflow).toContain('deterministic:');
    expect(workflow).toContain('name: Deterministic checks');
    expect(workflow).toContain('run: pnpm check');
    expect(workflow).toContain("if: github.event_name == 'workflow_dispatch'");
    expect(workflow).toContain('name: Manual Electron E2E');
    expect(workflow).toContain('name: Manual package smoke (${{ matrix.platform }} ${{ matrix.arch }})');
    const ciTriggers = workflow.slice(workflow.indexOf('on:'), workflow.indexOf('permissions:'));
    expect(ciTriggers).toContain('workflow_call:');
    expect(ciTriggers).toContain('workflow_dispatch:');
    expect(ciTriggers).not.toContain('push:');
    expect(ciTriggers).not.toContain('pull_request:');
    for (const runner of [
      'macos-15',
      'macos-15-intel',
      'windows-2025',
      'windows-11-arm',
      'ubuntu-24.04',
      'ubuntu-24.04-arm',
    ]) {
      expect(workflow).toContain(`runner: ${runner}`);
    }
    expect(workflow).toContain("test \"$(node -p 'process.arch')\" = \"${{ matrix.arch }}\"");
    const deterministicJob = workflow.slice(
      workflow.indexOf('  deterministic:'),
      workflow.indexOf('\n  manual-gui-e2e:'),
    );
    expect(deterministicJob).not.toContain('playwright');
    expect(deterministicJob).not.toContain('test:package:desktop');

    const releaseWorkflow = readFileSync(resolve('.github/workflows/release.yml'), 'utf8');
    expect(releaseWorkflow).toContain('uses: ./.github/workflows/ci.yml');
    for (const job of ['package-macos', 'package-windows', 'package-linux']) {
      const jobStart = releaseWorkflow.indexOf(`  ${job}:`);
      expect(jobStart).toBeGreaterThan(-1);
      expect(releaseWorkflow.slice(jobStart, jobStart + 300))
        .toContain('needs: [prepare, validate]');
    }
  });

  it('parses an explicit scenario without accepting unknown options', () => {
    const parsed = parseMacUpdateArguments([
      '--prior-zip', 'prior.zip',
      '--candidate-zip', 'candidate.zip',
      '--channel', 'beta',
      '--expected-version', '0.0.2-beta.2',
      '--scenario', 'valid',
      '--evidence', 'evidence.json',
    ]);

    expect(parsed).toMatchObject({
      channel: 'beta',
      expectedVersion: '0.0.2-beta.2',
      scenario: 'valid',
    });
    expect(() => parseMacUpdateArguments([
      '--prior-zip', 'prior.zip',
      '--candidate-zip', 'candidate.zip',
      '--channel', 'beta',
      '--expected-version', '0.0.2-beta.2',
      '--scenario', 'valid',
      '--token', 'secret',
    ])).toThrow('Unknown --token argument');
  });

  it('creates standard latest-mac metadata for one immutable zip', async () => {
    const sha512 = Buffer.alloc(64, 7).toString('base64');
    const metadata = buildMacUpdateMetadata({
      artifactName: 'Butter-Paper-Beta-macOS-arm64.zip',
      sha512,
      size: 1234,
      version: '0.0.2-beta.2',
      channel: 'beta',
    });

    expect(metadata).toContain('version: 0.0.2-beta.2');
    expect(metadata).toContain('butterPaperChannel: beta');
    expect(metadata).toContain('url: Butter-Paper-Beta-macOS-arm64.zip');
    expect(metadata).toContain(`sha512: ${sha512}`);
    expect(() => buildMacUpdateMetadata({
      artifactName: '../candidate.zip',
      sha512,
      size: 1234,
      version: '0.0.2',
      channel: 'stable',
    })).toThrow('Unsafe update artifact name');
  });

  it('keeps stable and beta identities isolated', () => {
    expect(expectedMacIdentity('stable')).toEqual({
      productName: 'Butter Paper',
      bundleId: 'com.butterpaper.desktop',
    });
    expect(expectedMacIdentity('beta')).toEqual({
      productName: 'Butter Paper Beta',
      bundleId: 'com.butterpaper.desktop.beta',
    });
  });

  it('trusts only the current and explicitly reviewed prior signer during certificate rotation', () => {
    const current = 'ab'.repeat(32);
    const prior = 'cd'.repeat(32);
    expect(resolvePriorSigningFingerprints(current, prior)).toEqual([
      current.toUpperCase(),
      prior.toUpperCase(),
    ]);
    expect(resolvePriorSigningFingerprints(current, current)).toEqual([current.toUpperCase()]);
    expect(resolvePriorSigningFingerprints(current, '')).toEqual([current.toUpperCase()]);
    expect(() => resolvePriorSigningFingerprints(current, 'invalid')).toThrow(/SHA-256/);
  });

  it('requires a SemVer-newer candidate, including prerelease ordering', () => {
    expect(compareReleaseVersions('0.0.2-beta.2', '0.0.2-beta.1')).toBeGreaterThan(0);
    expect(compareReleaseVersions('0.0.2', '0.0.2-beta.9')).toBeGreaterThan(0);
    expect(compareReleaseVersions('0.0.2-beta.1', '0.0.2')).toBeLessThan(0);
    expect(compareReleaseVersions('1.0.0', '0.99.99')).toBeGreaterThan(0);
    expect(() => compareReleaseVersions('latest', '0.0.1')).toThrow('Invalid release version');
  });

  it('identifies only the updater-started replacement executable', () => {
    const executable = '/Applications/Butter Paper.app/Contents/MacOS/Butter Paper';
    expect(parseExecutableProcessIds([
      `101 ${executable}`,
      `102 ${executable} --relaunch`,
      '103 /Applications/Other.app/Contents/MacOS/Other',
      `104 ${executable} Helper`,
    ].join('\n'), executable)).toEqual([101, 102, 104]);

    const harness = readFileSync(resolve('scripts/test-macos-update.mjs'), 'utf8');
    expect(harness.indexOf('await automaticRelaunch')).toBeLessThan(
      harness.indexOf('app = await electron.launch({', harness.indexOf('await automaticRelaunch')),
    );
  });

  it('makes native updater rejection and replacement tests a publication prerequisite', () => {
    const workflow = readFileSync(resolve('.github/workflows/release.yml'), 'utf8');
    expect(workflow).toContain('test-macos-updater:');
    expect(workflow).toContain('runs-on: ${{ matrix.arch == \'arm64\' && \'macos-15\' || \'macos-15-intel\' }}');
    expect(workflow).toContain('scenario: [channel, corrupt, signature, valid]');
    expect(workflow).toContain('--scenario "$RELEASE_SCENARIO"');
    expect(workflow).toContain('needs: [prepare, package-macos, package-windows, package-linux, test-macos-updater]');

    const updaterJob = workflow.slice(
      workflow.indexOf('  test-macos-updater:'),
      workflow.indexOf('\n  assemble:'),
    );
    expect(updaterJob).toContain('name: ${{ matrix.variant }}-updater-verification');
    expect(updaterJob).not.toContain('name: ${{ needs.prepare.outputs.environment }}');
    expect(updaterJob).not.toContain('environment: release-signing');
    expect(updaterJob).not.toContain('APPLE_SIGNING_CERTIFICATE_P12');
    expect(updaterJob).not.toContain('APPLE_NOTARYTOOL');
    expect(updaterJob).toContain('MACOS_UPDATER_BOOTSTRAP_TAG');
    expect(updaterJob).toContain('APPLE_PRIOR_SIGNING_CERTIFICATE_SHA256');
    expect(updaterJob).toContain('APPLE_SIGNING_CERTIFICATE_SHA256');
    expect(updaterJob).toContain('"$UPDATER_BOOTSTRAP_TAG" != "$GITHUB_REF_NAME"');
    expect(updaterJob).toContain(".filter(release => variant === 'beta' || !release.prerelease)");
    expect(updaterJob).toContain('--pattern SHA256SUMS');
    expect(updaterJob).toContain('gh attestation verify');
    expect(updaterJob).toContain('if: always()');

    const harness = readFileSync(resolve('scripts/test-macos-update.mjs'), 'utf8');
    expect(harness).toContain('trustExpectations({ prior: true })');
    expect(harness).toContain('...trustExpectations()');
  });

  it('makes the verified release public before sealing its update-feed bundle', () => {
    const workflow = readFileSync(resolve('.github/workflows/release.yml'), 'utf8');
    const publishJob = workflow.slice(
      workflow.indexOf('  publish:'),
      workflow.indexOf('\n  verify-publication:'),
    );
    expect(publishJob.indexOf('- name: Seal static update feed publication bundle')).toBeGreaterThan(-1);
    expect(publishJob.indexOf('- name: Seal static update feed publication bundle')).toBeGreaterThan(
      publishJob.indexOf('- name: Publish verified GitHub release'),
    );
    expect(publishJob.indexOf('- name: Seal static update feed publication bundle')).toBeGreaterThan(
      publishJob.indexOf('- name: Verify public assets before sealing the update feed'),
    );
    expect(publishJob).toContain('Release is still a draft');
    expect(publishJob).toContain('Release prerelease classification is wrong');
    expect(publishJob).toContain('Published release is not immutable');
    expect(publishJob).toContain('release.immutable === true');
    expect(publishJob).toContain('Release is already public with the expected classification.');
    expect(publishJob).toContain('Published release has the wrong stable/beta classification.');
    expect(publishJob).toContain('sha256sum --check --strict');
    expect(publishJob).toContain('Apply these exact reviewed bytes manually');
    expect(publishJob).not.toContain('git commit');
    expect(publishJob).not.toContain('git push');
  });
});
