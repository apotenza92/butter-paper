import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

describe('native release package boundaries', () => {
  it('keeps notarization credentials and public identifiers in their correct contexts', () => {
    const workflow = readFileSync(resolve('.github/workflows/release.yml'), 'utf8');

    expect(workflow).toContain(
      'APPLE_NOTARYTOOL_KEY_P8_BASE64: ${{ secrets.APPLE_NOTARYTOOL_KEY_P8_BASE64 }}',
    );
    expect(workflow).toContain(
      'APPLE_NOTARYTOOL_KEY_ID: ${{ vars.APPLE_NOTARYTOOL_KEY_ID }}',
    );
    expect(workflow).toContain(
      'APPLE_NOTARYTOOL_ISSUER_ID: ${{ vars.APPLE_NOTARYTOOL_ISSUER_ID }}',
    );
    expect(workflow).not.toContain('secrets.APPLE_NOTARYTOOL_KEY_ID');
    expect(workflow).not.toContain('secrets.APPLE_NOTARYTOOL_ISSUER_ID');
  });

  it('proves release-source provenance and requires immutable-release policy before publication', () => {
    const workflow = readFileSync(resolve('.github/workflows/release.yml'), 'utf8');
    const policyJob = workflow.split('  verify-release-policy:', 2)[1].split('  publish:', 1)[0];
    const publishJob = workflow.split('  publish:', 2)[1].split('  verify-publication:', 1)[0];

    expect(workflow).toContain('Prove tag commit belongs to the approved release source');
    expect(workflow).toContain('DEFAULT_BRANCH: ${{ github.event.repository.default_branch }}');
    expect(workflow).toContain('git merge-base --is-ancestor');
    expect(workflow).toContain('environment: release-policy');
    expect(workflow).toContain('repos/$GH_REPO/immutable-releases');
    expect(workflow).toContain('attest, verify-release-policy]');
    expect(workflow.match(/secrets\.IMMUTABLE_RELEASES_READ_TOKEN/g)).toHaveLength(1);
    expect(policyJob).toContain('permissions:\n      contents: read');
    expect(policyJob).toContain('secrets.IMMUTABLE_RELEASES_READ_TOKEN');
    expect(publishJob).not.toContain('IMMUTABLE_RELEASES_READ_TOKEN');
    expect(publishJob).toContain('Published release is not immutable');
    expect(publishJob.indexOf('Published release is not immutable')).toBeLessThan(
      publishJob.indexOf('- name: Advance static update feed'),
    );
  });

  it('installs, feature-tests, and uninstalls the exact Windows NSIS package', () => {
    const workflow = readFileSync(resolve('.github/workflows/release.yml'), 'utf8');
    const verifier = readFileSync(resolve('scripts/test-windows-installer.ps1'), 'utf8');

    expect(workflow).toContain('./scripts/test-windows-installer.ps1');
    expect(workflow).not.toContain("-path '*unpacked*'");
    expect(verifier).toContain('Assert-PeMachine $application $expectedMachine');
    expect(verifier).toContain('& pnpm test:package:desktop');
    expect(verifier).toContain('unexpectedly contains updater configuration');
    expect(verifier).toContain("Invoke-And-Wait $uninstaller.FullName @('/S')");
    expect(verifier).toContain('NSIS uninstall left the install directory behind');
  });

  it('tests the AppImage, installed DEB, and extracted RPM on native Linux', () => {
    const workflow = readFileSync(resolve('.github/workflows/release.yml'), 'utf8');
    const verifier = readFileSync(resolve('scripts/test-linux-packages.sh'), 'utf8');

    expect(workflow).toContain('./scripts/test-linux-packages.sh');
    expect(verifier).toContain('--appimage-extract');
    expect(verifier).toContain('sudo apt-get install -y "$deb"');
    expect(verifier).toContain('sudo apt-get purge -y "$installed_deb_package"');
    expect(verifier).toContain('rpm2cpio "$rpm"');
    expect(verifier.match(/pnpm test:package:desktop/g)).toHaveLength(2);
    expect(verifier).toContain('Package verification requires native');
    expect(verifier).toContain('assert_no_update_config');
  });

  it('publishes update feeds only for the supported macOS updater', () => {
    const workflow = readFileSync(resolve('.github/workflows/release.yml'), 'utf8');
    const contract = readFileSync(resolve('scripts/release-asset-contract.mjs'), 'utf8');

    expect(workflow).not.toContain('update-${{ matrix.variant }}-win32');
    expect(workflow).not.toContain('update-${{ matrix.variant }}-linux');
    expect(contract).not.toContain('update-${variant}-win32');
    expect(contract).not.toContain('update-${variant}-linux');
  });
});
