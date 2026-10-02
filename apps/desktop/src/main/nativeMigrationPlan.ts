// Pure decisions for the final Electron release, which moves every Electron
// install onto the native Butter Paper release and removes the Electron app.

export const NATIVE_RELEASE = {
  version: '0.1.0',
  tag: 'v0.1.0',
  pageUrl: 'https://github.com/apotenza92/butter-paper/releases/tag/v0.1.0',
  macTeamIdentifier: '27JL2VERNC',
  macBundleIdentifier: 'com.butterpaper.desktop',
  macMinimumMajorVersion: 13,
} as const;

export type NativeTarget = 'macos-arm64' | 'macos-x64' | 'windows-arm64' | 'windows-x64' | 'linux-arm64' | 'linux-x64';

export interface NativePackage {
  readonly target: NativeTarget;
  readonly assetName: string;
  readonly bytes: number;
  readonly sha256: string;
}

// Exact published v0.1.0 assets; the download must match these byte-for-byte.
export const NATIVE_PACKAGES: Readonly<Record<NativeTarget, NativePackage>> = {
  'macos-arm64': { target: 'macos-arm64', assetName: 'Butter-Paper-macOS-arm64.zip', bytes: 34582189, sha256: '3946ab95874661f93cc99900d437931514074d48a691015f7d7d67da1f0cf637' },
  'macos-x64': { target: 'macos-x64', assetName: 'Butter-Paper-macOS-x64.zip', bytes: 37000187, sha256: 'cb04a28998b4595cbea412e723cf9e451f04e55adb13f41550bb20734e7ec075' },
  'windows-arm64': { target: 'windows-arm64', assetName: 'Butter-Paper-Windows-arm64.zip', bytes: 74200190, sha256: '147137eb3f792e7077587c7cfc85657dd50dc68a314d7f1a3e74045918579b27' },
  'windows-x64': { target: 'windows-x64', assetName: 'Butter-Paper-Windows-x64.zip', bytes: 83390247, sha256: '88bad356a3dcbf372ce7bb75be68ab0f3443b48703dd747cf0dcb2a015c5276a' },
  'linux-arm64': { target: 'linux-arm64', assetName: 'Butter-Paper-Linux-arm64.tar.xz', bytes: 26681928, sha256: '8e072163ab298f2584c746b4c802cb5df15c9285720e06f7599e72511892f856' },
  'linux-x64': { target: 'linux-x64', assetName: 'Butter-Paper-Linux-x64.tar.xz', bytes: 29658164, sha256: '77d4f0d08b3add68533ce953de8be5d8fd6ed05f11959b34bc0d787a9ff33601' },
};

export function nativePackageUrl(nativePackage: NativePackage): string {
  return `https://github.com/apotenza92/butter-paper/releases/download/${NATIVE_RELEASE.tag}/${nativePackage.assetName}`;
}

export interface MigrationHost {
  readonly platform: NodeJS.Platform;
  readonly arch: string;
  // True when an x64 build runs under ARM64 translation (Rosetta or Windows on Arm).
  readonly runningUnderArm64Translation: boolean;
  readonly isPackaged: boolean;
  readonly isTestMode: boolean;
  readonly disabledByEnvironment: boolean;
  readonly macosVersion?: string;
  readonly executablePath: string;
  readonly appImagePath?: string;
}

export type MigrationEligibility =
  | { readonly eligible: true; readonly nativePackage: NativePackage }
  | { readonly eligible: false; readonly reason: 'development' | 'unsupported-platform' | 'unsupported-install' | 'macos-too-old'; readonly minimumMacosMajorVersion?: number };

export function resolveMigrationEligibility(host: MigrationHost): MigrationEligibility {
  if (!host.isPackaged || host.isTestMode || host.disabledByEnvironment) {
    return { eligible: false, reason: 'development' };
  }
  const arch = host.arch === 'arm64' || host.runningUnderArm64Translation ? 'arm64' : host.arch === 'x64' ? 'x64' : null;
  if (arch == null) {
    return { eligible: false, reason: 'unsupported-platform' };
  }
  if (host.platform === 'darwin') {
    if (macBundlePath(host.executablePath) == null) {
      return { eligible: false, reason: 'unsupported-install' };
    }
    const major = Number.parseInt(host.macosVersion ?? '', 10);
    if (!Number.isFinite(major) || major < NATIVE_RELEASE.macMinimumMajorVersion) {
      return { eligible: false, reason: 'macos-too-old', minimumMacosMajorVersion: NATIVE_RELEASE.macMinimumMajorVersion };
    }
    return { eligible: true, nativePackage: NATIVE_PACKAGES[`macos-${arch}`] };
  }
  if (host.platform === 'win32') {
    return { eligible: true, nativePackage: NATIVE_PACKAGES[`windows-${arch}`] };
  }
  if (host.platform === 'linux') {
    // Only AppImage installs receive Electron updates; DEB and RPM stay package-manager controlled.
    if (!host.appImagePath) {
      return { eligible: false, reason: 'unsupported-install' };
    }
    return { eligible: true, nativePackage: NATIVE_PACKAGES[`linux-${arch}`] };
  }
  return { eligible: false, reason: 'unsupported-platform' };
}

// The running .app bundle, or null when the executable is not inside one.
export function macBundlePath(executablePath: string): string | null {
  const match = /^(.*?\.app)\/Contents\/MacOS\/[^/]+$/.exec(executablePath);
  return match?.[1] ?? null;
}

// Install next to the running app when it lives in an Applications folder,
// otherwise into /Applications.
export function macDestinationDirectory(runningBundlePath: string, homeDirectory: string): string {
  const parent = runningBundlePath.slice(0, runningBundlePath.lastIndexOf('/'));
  return parent === '/Applications' || parent === `${homeDirectory}/Applications` ? parent : '/Applications';
}

export function macElectronBundleCandidates(homeDirectory: string): string[] {
  return ['/Applications', `${homeDirectory}/Applications`].flatMap((directory) => [
    `${directory}/Butter Paper.app`,
    `${directory}/Butter Paper Beta.app`,
  ]);
}

export const MAC_DESIGNATED_REQUIREMENT = `=identifier "${NATIVE_RELEASE.macBundleIdentifier}" and anchor apple generic and certificate leaf[subject.OU] = "${NATIVE_RELEASE.macTeamIdentifier}"`;

// Electron bundles are replaced; a bundle already at the native version is kept.
export function isNativeBundleVersion(shortVersion: string | null): boolean {
  return shortVersion === NATIVE_RELEASE.version;
}

export function windowsNativeArchitecture(target: NativeTarget): 'arm64' | 'x86_64' {
  return target === 'windows-arm64' ? 'arm64' : 'x86_64';
}

export function linuxPackageDirectory(target: NativeTarget): string {
  return `butter-paper-linux-${target === 'linux-arm64' ? 'arm64' : 'x86_64'}-${NATIVE_RELEASE.version}`;
}

// PE machine field of a DLL: 0x8664 is x64, 0xAA64 is ARM64 (including ARM64X).
export function peMachine(bytes: Uint8Array): number | null {
  if (bytes.length < 0x40 || bytes[0] !== 0x4d || bytes[1] !== 0x5a) {
    return null;
  }
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const peOffset = view.getUint32(0x3c, true);
  if (peOffset + 6 > bytes.length || view.getUint32(peOffset, true) !== 0x00004550) {
    return null;
  }
  return view.getUint16(peOffset + 4, true);
}

export function requiredPeMachine(target: NativeTarget): number {
  return target === 'windows-arm64' ? 0xaa64 : 0x8664;
}

function powershellString(value: string): string {
  return `'${value.replaceAll("'", "''")}'`;
}

export interface WindowsHandoverOptions {
  readonly electronProcessId: number;
  readonly packageDirectory: string;
  readonly target: NativeTarget;
  readonly pdfPaths: readonly string[];
  readonly logPath: string;
}

// Runs after Electron exits: uninstall every Electron Butter Paper install first
// (the stable NSIS uninstaller deletes its whole folder, which contains the
// native install root), then install and open the native app.
export function windowsHandoverScript(options: WindowsHandoverOptions): string {
  const architecture = windowsNativeArchitecture(options.target);
  const pdfArguments = options.pdfPaths.map((path) => powershellString(`"${path}"`)).join(', ');
  return `$ErrorActionPreference = 'Stop'
$log = ${powershellString(options.logPath)}
$packageDirectory = ${powershellString(options.packageDirectory)}
$architecture = ${powershellString(architecture)}
$version = ${powershellString(NATIVE_RELEASE.version)}
$releasePage = ${powershellString(NATIVE_RELEASE.pageUrl)}
$pdfArguments = @(${pdfArguments})
function Write-Log($message) { Add-Content -LiteralPath $log -Value ('[' + (Get-Date).ToString('o') + '] ' + $message) }
function Stop-Handover($message) {
  Write-Log ('failed: ' + $message)
  Add-Type -AssemblyName System.Windows.Forms
  [void][System.Windows.Forms.MessageBox]::Show("Butter Paper could not finish installing the new app.\`n\`n$message\`n\`nThe download page will open so you can install it manually.", 'Butter Paper')
  Start-Process $releasePage
  exit 1
}
try {
  Write-Log 'waiting for Electron to exit'
  Wait-Process -Id ${options.electronProcessId} -Timeout 60 -ErrorAction SilentlyContinue
  $uninstallRoot = 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall'
  $entries = @()
  if (Test-Path -LiteralPath $uninstallRoot) {
    $entries = @(Get-ChildItem -LiteralPath $uninstallRoot | ForEach-Object { Get-ItemProperty -LiteralPath $_.PSPath } | Where-Object {
      $_.DisplayName -match '^Butter Paper( Beta)?( [0-9][0-9.a-z-]*)?$' -and [string]$_.UninstallString -match 'Uninstall Butter Paper( Beta)?\\.exe'
    })
  }
  foreach ($entry in $entries) {
    if ([string]$entry.UninstallString -notmatch '^"([^"]+)"\\s*(.*)$') { continue }
    $uninstaller = $Matches[1]
    $arguments = $Matches[2]
    $installLocation = if ($entry.InstallLocation) { [string]$entry.InstallLocation } else { Split-Path -Parent $uninstaller }
    Write-Log ('removing ' + $entry.DisplayName + ' from ' + $installLocation)
    Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.Path -and $_.Path.StartsWith($installLocation, [StringComparison]::OrdinalIgnoreCase) } | Stop-Process -Force -ErrorAction SilentlyContinue
    if (Test-Path -LiteralPath $uninstaller) {
      Start-Process -FilePath $uninstaller -ArgumentList (($arguments + ' /S').Trim()) -Wait
      $deadline = (Get-Date).AddSeconds(120)
      while ((Test-Path -LiteralPath $uninstaller) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 500 }
      if (Test-Path -LiteralPath $uninstaller) { Stop-Handover ('The old ' + $entry.DisplayName + ' could not be removed.') }
    }
  }
  $localAppData = [Environment]::GetFolderPath('LocalApplicationData')
  $installRoot = Join-Path $localAppData ('Programs\\Butter Paper\\' + $version + '\\' + $architecture)
  $executable = Join-Path $installRoot 'butter-paper.exe'
  if (-not (Test-Path -LiteralPath $executable)) {
    # Clear registrations left by a native install whose folder an Electron uninstaller removed.
    $progId = 'ButterPaper.PDF.' + $version + '.' + $architecture
    $shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) ('Butter Paper ' + $version + ' (' + $architecture + ').lnk')
    Remove-Item -LiteralPath $shortcut -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath ('HKCU:\\Software\\Classes\\' + $progId) -Recurse -Force -ErrorAction SilentlyContinue
    Remove-ItemProperty -LiteralPath 'HKCU:\\Software\\Classes\\.pdf\\OpenWithProgids' -Name $progId -ErrorAction SilentlyContinue
    if (Test-Path -LiteralPath $installRoot) { Remove-Item -LiteralPath $installRoot -Recurse -Force }
    Write-Log 'installing native app'
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $packageDirectory 'install.ps1') *>> $log
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $executable)) { Stop-Handover 'The new app could not be installed.' }
  }
  Write-Log 'opening native app'
  if ($pdfArguments.Count -gt 0) {
    Start-Process -FilePath $executable -WorkingDirectory $installRoot -ArgumentList $pdfArguments
  } else {
    Start-Process -FilePath $executable -WorkingDirectory $installRoot
  }
  Remove-Item -LiteralPath (Split-Path -Parent $packageDirectory) -Recurse -Force -ErrorAction SilentlyContinue
  Write-Log 'done'
} catch {
  Stop-Handover $_.Exception.Message
}
`;
}
