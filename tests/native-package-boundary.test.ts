import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

describe('native release package boundaries', () => {
  it('resolves the lazy blank-PDF subpath before generated package output exists', () => {
    const desktopTsconfig = JSON.parse(
      readFileSync(resolve('apps/desktop/tsconfig.json'), 'utf8'),
    ) as { compilerOptions: { paths: Record<string, string[]> } };
    const pdfPackage = JSON.parse(
      readFileSync(resolve('packages/pdf/package.json'), 'utf8'),
    ) as { exports: Record<string, { types: string; import: string }> };

    expect(desktopTsconfig.compilerOptions.paths['@butter-paper/pdf/blank'])
      .toEqual(['../../packages/pdf/src/blankPdf.ts']);
    expect(pdfPackage.exports['./blank']).toEqual({
      types: './dist/blankPdf.d.ts',
      import: './dist/blankPdf.js',
    });
  });

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
    expect(workflow).toContain('needs: [prepare, seal-tuf]');
    expect(publishJob).toContain('Release prerelease classification is wrong');
    expect(publishJob).toContain('Publish authenticated updater feeds atomically');
    expect(publishJob).toContain('git commit -m "Publish Butter Paper');
    expect(publishJob).toContain('git push origin HEAD:updates');
  });

  it('publishes authenticated update metadata for all supported updater packages', () => {
    const workflow = readWorkflow();
    const contract = readFileSync(resolve('scripts/release-asset-contract.mjs'), 'utf8');

    expect(workflow).toContain('Reject unexpectedly oversized Windows ARM64 installers');
    expect(workflow).toContain('update-${{ matrix.variant }}-win32');
    expect(workflow).toContain('update-${{ matrix.variant }}-linux');
    expect(contract).toContain('update-${variant}-win32');
    expect(contract).toContain('update-${variant}-linux');
  });

  it('promotes stable code to both isolated products while beta releases remain beta-only', () => {
    const workflow = readWorkflow();

    expect(workflow).not.toContain('process.env.GITHUB_ACTOR');
    expect(workflow).not.toContain('stable-release-self');
    expect(workflow).not.toContain('beta-release-self');
    expect(workflow).toContain(
      "const environment = stable\n            ? 'stable-release'\n            : 'beta-release';",
    );
    expect(workflow).toContain(
      "['channel=stable', `environment=${environment}`, 'prerelease=false', 'variants=[\"stable\",\"beta\"]']",
    );
    expect(workflow).toContain(
      "['channel=beta', `environment=${environment}`, 'prerelease=true', 'variants=[\"beta\"]']",
    );
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
});

function readWorkflow(): string {
  return readFileSync(resolve('.github/workflows/release.yml'), 'utf8').replaceAll('\r\n', '\n');
}
