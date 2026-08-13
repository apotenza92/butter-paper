import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

describe('native release package boundaries', () => {
  it('keeps notarization credentials and public identifiers in their correct contexts', () => {
    const workflow = readWorkflow();

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

  it('proves release-source provenance and gates release plus feed publication together', () => {
    const workflow = readWorkflow();
    const publishJob = workflow.split('  publish:', 2)[1].split('  verify-publication:', 1)[0];

    expect(workflow).toContain('Prove tag commit belongs to the approved release source');
    expect(workflow).toContain('DEFAULT_BRANCH: ${{ github.event.repository.default_branch }}');
    expect(workflow).toContain('git merge-base --is-ancestor');
    expect(workflow).toContain('needs: [prepare, seal-tuf, attest]');
    expect(publishJob).toContain('Release prerelease classification is wrong');
    expect(publishJob).toContain('Publish authenticated updater feeds atomically');
    expect(publishJob).toContain('git commit -m "Publish Butter Paper');
    expect(publishJob).toContain('git push origin HEAD:updates');
  });

  it('installs, feature-tests, and uninstalls the exact Windows NSIS package', () => {
    const workflow = readWorkflow();
    const ciWorkflow = readFileSync(resolve('.github/workflows/ci.yml'), 'utf8');
    const verifier = readFileSync(resolve('scripts/test-windows-installer.ps1'), 'utf8');
    const installerInclude = readFileSync(resolve('apps/desktop/build/installer.nsh'), 'utf8');
    const desktopPackage = JSON.parse(
      readFileSync(resolve('apps/desktop/package.json'), 'utf8'),
    ) as { devDependencies: Record<string, string> };

    expect(workflow).toContain('./scripts/test-windows-installer.ps1');
    expect(workflow).toContain('scripts/verify-release-package-sizes.mjs');
    expect(workflow).toContain('--arm64-installer');
    expect(ciWorkflow).toContain('scripts/verify-release-package-sizes.mjs');
    expect(ciWorkflow).toContain('--arm64-installer');
    expect(workflow).not.toContain("-path '*unpacked*'");
    expect(verifier).toContain('Assert-PeMachine $application $expectedMachine');
    expect(verifier).toContain('Assert-PeMachine $runtimePath $expectedMachine');
    expect(verifier).toContain('$env:BP_RELEASE_CHANNEL = $Channel');
    expect(verifier).toContain('& pnpm test:package:desktop');
    expect(verifier).toContain('missing TUF-gated updater configuration');
    expect(verifier).toContain('missing the reviewed TUF root');
    expect(verifier).toContain('verify-packaged-runtime-dependencies.cjs');
    expect(verifier).toContain("Invoke-And-Wait $uninstaller.FullName @('/S')");
    expect(verifier).toContain('NSIS uninstall left the install directory behind');
    expect(desktopPackage.devDependencies['electron-builder']).toBe('26.15.7');
    expect(installerInclude).not.toContain('customFiles_arm64');
    expect(installerInclude).not.toContain('BP_NSIS_ARM64_UNPACKED_DIR');
  });

  it('tests the AppImage, installed DEB, and extracted RPM on native Linux', () => {
    const workflow = readWorkflow();
    const verifier = readFileSync(resolve('scripts/test-linux-packages.sh'), 'utf8');

    expect(workflow).toContain('./scripts/test-linux-packages.sh');
    expect(verifier).toContain('--appimage-extract');
    expect(verifier).toContain('sudo apt-get install -y "$deb"');
    expect(verifier).toContain('sudo apt-get purge -y "$installed_deb_package"');
    expect(verifier).toContain('rpm2cpio "$rpm"');
    expect(verifier.match(/pnpm test:package:desktop/g)).toHaveLength(2);
    expect(verifier.match(/BP_RELEASE_CHANNEL="\$channel"/g)).toHaveLength(2);
    expect(verifier).toContain('BP_TEST_STARTUP_ONLY=1');
    expect(verifier).toContain('Package verification requires native');
    expect(verifier).toContain('assert_update_contract');
    expect(verifier).toContain('verify-packaged-runtime-dependencies.cjs');
    expect(verifier).toContain('maximum_glibc=\'2.35\'');
    expect(verifier).toContain('assert_elf_contract');
    expect(verifier).toContain('ldd "$candidate"');
    expect(verifier).toContain('assert_desktop_integration');
    expect(verifier).toContain('desktop-file-validate');
    expect(verifier).toContain("desktop_executable='AppRun'");
  });

  it('publishes authenticated update metadata for all supported updater packages', () => {
    const workflow = readWorkflow();
    const contract = readFileSync(resolve('scripts/release-asset-contract.mjs'), 'utf8');

    expect(workflow).toContain('Reject unexpectedly oversized Windows ARM64 installers');
    expect(workflow).toContain('update-${{ matrix.variant }}-win32');
    expect(workflow).toContain('update-${{ matrix.variant }}-linux');
    expect(contract).toContain('update-${variant}-win32');
    expect(contract).toContain('update-${variant}-linux');
    expect(workflow).toContain('uses: ./.github/workflows/nonmac-updater-audit.yml');
  });

  it('promotes stable code to both isolated products while beta releases remain beta-only', () => {
    const workflow = readWorkflow();

    expect(workflow).toContain(
      'process.env.GITHUB_ACTOR === process.env.GITHUB_REPOSITORY_OWNER',
    );
    expect(workflow).toContain(
      "ownerRelease ? 'stable-release-self' : 'stable-release'",
    );
    expect(workflow).toContain(
      "ownerRelease ? 'beta-release-self' : 'beta-release'",
    );
    expect(workflow).toContain(
      "['channel=stable', `environment=${environment}`, 'prerelease=false', 'variants=[\"stable\",\"beta\"]']",
    );
    expect(workflow).toContain(
      "['channel=beta', `environment=${environment}`, 'prerelease=true', 'variants=[\"beta\"]']",
    );
    expect(workflow).toContain("const app = channel === 'beta' ? 'Butter Paper Beta.app' : 'Butter Paper.app'");
    expect(workflow).toContain("variant === 'beta' || !release.prerelease");
  });

  it('supplies Electron ICU data to the native Windows ARM canvas module', () => {
    const nativeDependencySetup = readFileSync(resolve('scripts/ensure-native-deps.mjs'), 'utf8');

    expect(nativeDependencySetup).toContain("process.platform !== 'win32' || process.arch !== 'arm64'");
    expect(nativeDependencySetup).toContain("packageRoot('@napi-rs/canvas-win32-arm64-msvc')");
    expect(nativeDependencySetup).toContain("join(electronRoot, 'dist', 'icudtl.dat')");
    expect(nativeDependencySetup).toContain('copyFileSync(source, destination)');
    expect(nativeDependencySetup).toContain("'powershell.exe'");
    expect(nativeDependencySetup).toContain('Expand-Archive -LiteralPath');
  });

  it('locates unpacked packages in the configured release directory', () => {
    const smokeHarness = readFileSync(resolve('scripts/smoke-packaged-desktop.mjs'), 'utf8');

    expect(smokeHarness).toContain('process.env.BP_RELEASE_OUTPUT_DIR?.trim()');
    expect(smokeHarness).toContain("resolve(repoRoot, 'apps/desktop', configuredReleaseDir)");
    expect(smokeHarness).toContain("getByTestId('document-tab-template-picker')");
    expect(smokeHarness).toContain("getByTestId('template-picker-item-built-in-blank')");
    expect(smokeHarness).toContain("process.env.BP_TEST_STARTUP_ONLY === '1'");
    expect(smokeHarness).not.toContain("getByTestId('document-tab-new-pdf-settings')");
  });
});

function readWorkflow(): string {
  return readFileSync(resolve('.github/workflows/release.yml'), 'utf8').replaceAll('\r\n', '\n');
}
