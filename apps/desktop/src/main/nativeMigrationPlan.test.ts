import {
  MAC_DESIGNATED_REQUIREMENT,
  NATIVE_PACKAGES,
  linuxPackageDirectory,
  macBundlePath,
  macDestinationDirectory,
  macElectronBundleCandidates,
  nativePackageUrl,
  peMachine,
  requiredPeMachine,
  resolveMigrationEligibility,
  windowsHandoverScript,
  type MigrationHost,
} from './nativeMigrationPlan';

const macHost: MigrationHost = {
  platform: 'darwin',
  arch: 'arm64',
  runningUnderArm64Translation: false,
  isPackaged: true,
  isTestMode: false,
  disabledByEnvironment: false,
  macosVersion: '15.4.1',
  executablePath: '/Applications/Butter Paper Beta.app/Contents/MacOS/Butter Paper Beta',
};

function peBytes(machine: number): Uint8Array {
  const bytes = new Uint8Array(0x100);
  const view = new DataView(bytes.buffer);
  bytes[0] = 0x4d;
  bytes[1] = 0x5a;
  view.setUint32(0x3c, 0x80, true);
  view.setUint32(0x80, 0x00004550, true);
  view.setUint16(0x84, machine, true);
  return bytes;
}

describe('native app migration plan', () => {
  it('pins the exact published v0.1.0 package for each target', () => {
    expect(nativePackageUrl(NATIVE_PACKAGES['windows-x64'])).toBe(
      'https://github.com/apotenza92/butter-paper/releases/download/v0.1.0/Butter-Paper-Windows-x64.zip',
    );
    for (const nativePackage of Object.values(NATIVE_PACKAGES)) {
      expect(nativePackage.sha256).toMatch(/^[0-9a-f]{64}$/);
      expect(nativePackage.bytes).toBeGreaterThan(1_000_000);
    }
  });

  it('never migrates development, test or explicitly disabled runs', () => {
    expect(resolveMigrationEligibility({ ...macHost, isPackaged: false })).toEqual({ eligible: false, reason: 'development' });
    expect(resolveMigrationEligibility({ ...macHost, isTestMode: true })).toEqual({ eligible: false, reason: 'development' });
    expect(resolveMigrationEligibility({ ...macHost, disabledByEnvironment: true })).toEqual({ eligible: false, reason: 'development' });
  });

  it('selects the native architecture, including x64 builds under ARM64 translation', () => {
    expect(resolveMigrationEligibility(macHost)).toMatchObject({ eligible: true, nativePackage: { target: 'macos-arm64' } });
    expect(resolveMigrationEligibility({ ...macHost, arch: 'x64' })).toMatchObject({ nativePackage: { target: 'macos-x64' } });
    expect(resolveMigrationEligibility({ ...macHost, arch: 'x64', runningUnderArm64Translation: true })).toMatchObject({ nativePackage: { target: 'macos-arm64' } });
    expect(resolveMigrationEligibility({ ...macHost, platform: 'win32', arch: 'x64', runningUnderArm64Translation: true })).toMatchObject({ nativePackage: { target: 'windows-arm64' } });
    expect(resolveMigrationEligibility({ ...macHost, platform: 'win32', arch: 'x64' })).toMatchObject({ nativePackage: { target: 'windows-x64' } });
    expect(resolveMigrationEligibility({ ...macHost, platform: 'win32', arch: 'ia32' })).toEqual({ eligible: false, reason: 'unsupported-platform' });
  });

  it('keeps macOS 12 on Electron because the native app needs macOS 13', () => {
    expect(resolveMigrationEligibility({ ...macHost, macosVersion: '12.7.6' })).toEqual({
      eligible: false,
      reason: 'macos-too-old',
      minimumMacosMajorVersion: 13,
    });
    expect(resolveMigrationEligibility({ ...macHost, macosVersion: '13.0' })).toMatchObject({ eligible: true });
  });

  it('migrates Linux AppImage installs only', () => {
    expect(resolveMigrationEligibility({ ...macHost, platform: 'linux', arch: 'x64' })).toEqual({ eligible: false, reason: 'unsupported-install' });
    expect(resolveMigrationEligibility({ ...macHost, platform: 'linux', arch: 'x64', appImagePath: '/home-dir/Butter-Paper-Linux-x64.AppImage' }))
      .toMatchObject({ nativePackage: { target: 'linux-x64' } });
    expect(linuxPackageDirectory('linux-x64')).toBe('butter-paper-linux-x86_64-0.1.0');
    expect(linuxPackageDirectory('linux-arm64')).toBe('butter-paper-linux-arm64-0.1.0');
  });

  it('installs the Mac app beside the running app only inside an Applications folder', () => {
    expect(macBundlePath(macHost.executablePath)).toBe('/Applications/Butter Paper Beta.app');
    expect(macBundlePath('/usr/local/bin/electron')).toBeNull();
    expect(macDestinationDirectory('/Applications/Butter Paper Beta.app', '/home-dir')).toBe('/Applications');
    expect(macDestinationDirectory('/home-dir/Applications/Butter Paper.app', '/home-dir')).toBe('/home-dir/Applications');
    expect(macDestinationDirectory('/home-dir/Downloads/Butter Paper.app', '/home-dir')).toBe('/Applications');
    expect(macElectronBundleCandidates('/home-dir')).toEqual([
      '/Applications/Butter Paper.app',
      '/Applications/Butter Paper Beta.app',
      '/home-dir/Applications/Butter Paper.app',
      '/home-dir/Applications/Butter Paper Beta.app',
    ]);
    expect(MAC_DESIGNATED_REQUIREMENT).toBe('=identifier "com.butterpaper.desktop" and anchor apple generic and certificate leaf[subject.OU] = "27JL2VERNC"');
  });

  it('requires a Visual C++ runtime matching the native Windows architecture', () => {
    expect(peMachine(peBytes(0x8664))).toBe(requiredPeMachine('windows-x64'));
    expect(peMachine(peBytes(0xaa64))).toBe(requiredPeMachine('windows-arm64'));
    expect(peMachine(new Uint8Array(0))).toBeNull();
    expect(peMachine(new Uint8Array(0x100))).toBeNull();
  });

  it('uninstalls Electron before installing the native Windows app into the shared Programs folder', () => {
    const script = windowsHandoverScript({
      electronProcessId: 4242,
      packageDirectory: "C:\\Users\\O'Neil\\Temp\\bp\\package",
      target: 'windows-arm64',
      pdfPaths: ['C:\\Docs\\a b.pdf'],
      logPath: 'C:\\Temp\\log.txt',
    });
    expect(script).toContain('Wait-Process -Id 4242');
    expect(script).toContain("$packageDirectory = 'C:\\Users\\O''Neil\\Temp\\bp\\package'");
    expect(script).toContain("$architecture = 'arm64'");
    expect(script).toContain(`$pdfArguments = @('"C:\\Docs\\a b.pdf"')`);
    expect(script).toContain("'^Butter Paper( Beta)?( [0-9][0-9.a-z-]*)?$'");
    expect(script.indexOf('Start-Process -FilePath $uninstaller')).toBeLessThan(script.indexOf("install.ps1"));
    expect(script).toContain("Programs\\Butter Paper\\' + $version");
    expect(windowsHandoverScript({ electronProcessId: 1, packageDirectory: 'C:\\p', target: 'windows-x64', pdfPaths: [], logPath: 'C:\\l' }))
      .toContain("$architecture = 'x86_64'");
  });
});
