import { execFileSync } from 'node:child_process';
import { homedir } from 'node:os';
import { relative, resolve, sep } from 'node:path';

export const DEFAULT_REVU_PATH = String.raw`C:\Program Files\Bluebeam Software\Bluebeam Revu\21\Revu\Revu.exe`;

export function powershellQuote(value) {
  return `'${String(value).replaceAll("'", "''")}'`;
}

export function encodePowerShell(script) {
  return Buffer.from(script, 'utf16le').toString('base64');
}

export function hostPathToParallelsGuest(hostPath, { hostHome = homedir(), share = String.raw`\\Mac\Home` } = {}) {
  const absolute = resolve(hostPath);
  const relativeToHome = relative(resolve(hostHome), absolute);
  if (!relativeToHome || relativeToHome === '.') return share;
  if (relativeToHome === '..' || relativeToHome.startsWith(`..${sep}`) || relativeToHome.startsWith('../')) {
    throw new Error('The PDF is outside the host home directory; pass --guest-pdf with its Parallels shared-folder path.');
  }
  return `${share}\\${relativeToHome.split(sep).join('\\')}`;
}

export function parseJsonOutput(output) {
  const lines = String(output).split(/\r?\n/).map((line) => line.trim()).filter(Boolean);
  for (let index = lines.length - 1; index >= 0; index -= 1) {
    try {
      return JSON.parse(lines[index]);
    } catch {
      // prlctl can add status lines around guest stdout. The last JSON line wins.
    }
  }
  throw new Error(`Guest command did not return JSON: ${String(output).trim()}`);
}

export class ParallelsRevu {
  constructor({ vmName, revuPath = DEFAULT_REVU_PATH, execute = defaultExecute } = {}) {
    if (!vmName) throw new Error('vmName is required');
    this.vmName = vmName;
    this.revuPath = revuPath;
    this.execute = execute;
  }

  prlctl(args) {
    return this.execute('prlctl', args);
  }

  powershell(script) {
    return this.prlctl([
      'exec', this.vmName, '--current-user', 'powershell.exe',
      '-NoLogo', '-NoProfile', '-NonInteractive', '-OutputFormat', 'Text',
      '-EncodedCommand', encodePowerShell(`$ErrorActionPreference = 'Stop'\n[Console]::OutputEncoding = [System.Text.Encoding]::UTF8\n${script}`),
    ]);
  }

  assertRunning() {
    const output = this.prlctl(['list', '-a', '-o', 'name,status']);
    const line = output.split(/\r?\n/).find((candidate) => candidate.includes(this.vmName));
    if (!line || !/\brunning\b/i.test(line)) throw new Error(`Parallels VM is not running: ${this.vmName}`);
    return line.trim();
  }

  environment() {
    const script = String.raw`
$revuPath = ${powershellQuote(this.revuPath)}
if (-not (Test-Path -LiteralPath $revuPath)) { throw "Revu executable not found: $revuPath" }
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
$screen = [System.Windows.Forms.Screen]::PrimaryScreen
$graphics = [System.Drawing.Graphics]::FromHwnd([IntPtr]::Zero)
$dpiX = $graphics.DpiX
$graphics.Dispose()
$os = Get-CimInstance Win32_OperatingSystem
$computer = Get-CimInstance Win32_ComputerSystem
$themeValue = Get-ItemPropertyValue -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize' -Name AppsUseLightTheme -ErrorAction SilentlyContinue
$fontNames = (New-Object System.Drawing.Text.InstalledFontCollection).Families.Name | Sort-Object -Unique
$version = (Get-Item -LiteralPath $revuPath).VersionInfo.ProductVersion
$fileVersion = (Get-Item -LiteralPath $revuPath).VersionInfo.FileVersion
[ordered]@{
  os = $os.Caption
  windowsVersion = $os.Version
  windowsBuild = $os.BuildNumber
  osArchitecture = $os.OSArchitecture
  systemType = $computer.SystemType
  processArchitecture = $env:PROCESSOR_ARCHITECTURE
  app = 'Bluebeam Revu'
  appVersion = $version
  appFileVersion = $fileVersion
  edition = 'Revu 21'
  displayResolution = ('{0}x{1}' -f $screen.Bounds.Width, $screen.Bounds.Height)
  displayScale = [math]::Round($dpiX / 96, 4)
  dpi = $dpiX
  locale = (Get-Culture).Name
  uiLocale = (Get-UICulture).Name
  timezone = (Get-TimeZone).Id
  observedAt = (Get-Date).ToUniversalTime().ToString('o')
  theme = $(if ($themeValue -eq 0) { 'dark' } else { 'light' })
  captureMethod = 'prlctl capture'
  fonts = @($fontNames)
} | ConvertTo-Json -Compress -Depth 4
`;
    return parseJsonOutput(this.powershell(script));
  }

  copyPdfToTemp(sourceGuestPath, { sha256, name = 'butter-paper-compatibility.pdf' } = {}) {
    if (!sourceGuestPath) throw new Error('copyPdfToTemp requires a Windows-visible source path');
    if (!/^[a-f0-9]{64}$/i.test(String(sha256))) throw new Error('copyPdfToTemp requires the source SHA-256');
    const safeName = String(name).replaceAll(/[^a-z0-9._-]+/gi, '-').replaceAll(/^-+|-+$/g, '') || 'specimen.pdf';
    const staged = parseJsonOutput(this.powershell(String.raw`
$sourcePath = ${powershellQuote(sourceGuestPath)}
if (-not (Test-Path -LiteralPath $sourcePath)) { throw "Shared source PDF not found: $sourcePath" }
$directory = Join-Path $env:TEMP 'ButterPaperCompat'
New-Item -ItemType Directory -Path $directory -Force | Out-Null
$pdfPath = Join-Path $directory ${powershellQuote(safeName)}
Copy-Item -LiteralPath $sourcePath -Destination $pdfPath -Force
$hash = (Get-FileHash -LiteralPath $pdfPath -Algorithm SHA256).Hash.ToLowerInvariant()
[ordered]@{ path = $pdfPath; sha256 = $hash; bytes = (Get-Item -LiteralPath $pdfPath).Length } | ConvertTo-Json -Compress
`));
    if (staged.sha256 !== String(sha256).toLowerCase()) throw new Error('Guest-staged PDF does not match the source bytes');
    return staged;
  }

  verifyPdf(guestPdfPath, expectedSha256) {
    if (!guestPdfPath) throw new Error('verifyPdf requires a Windows-visible PDF path');
    if (!/^[a-f0-9]{64}$/i.test(String(expectedSha256))) throw new Error('verifyPdf requires the source SHA-256');
    const verified = parseJsonOutput(this.powershell(String.raw`
$pdfPath = ${powershellQuote(guestPdfPath)}
if (-not (Test-Path -LiteralPath $pdfPath)) { throw "Guest PDF not found: $pdfPath" }
$hash = (Get-FileHash -LiteralPath $pdfPath -Algorithm SHA256).Hash.ToLowerInvariant()
[ordered]@{ path = $pdfPath; sha256 = $hash; bytes = (Get-Item -LiteralPath $pdfPath).Length } | ConvertTo-Json -Compress
`));
    if (verified.sha256 !== String(expectedSha256).toLowerCase()) throw new Error('Guest PDF does not match the source SHA-256');
    return verified;
  }

  openPdf(guestPdfPath, { timeoutMilliseconds = 25_000 } = {}) {
    const script = String.raw`
$revuPath = ${powershellQuote(this.revuPath)}
$pdfPath = ${powershellQuote(guestPdfPath)}
if (-not (Test-Path -LiteralPath $pdfPath)) { throw "Guest PDF not found: $pdfPath" }
Start-Process -FilePath $revuPath -ArgumentList @($pdfPath)
$deadline = [DateTime]::UtcNow.AddMilliseconds(${Math.max(1, Number(timeoutMilliseconds))})
$fileName = [IO.Path]::GetFileNameWithoutExtension($pdfPath)
do {
  Start-Sleep -Milliseconds 250
  $process = Get-Process Revu -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 -and $_.MainWindowTitle -like "*$fileName*" } | Select-Object -First 1
} until ($process -or [DateTime]::UtcNow -ge $deadline)
if (-not $process) { throw "Revu did not open $fileName before the timeout" }
[ordered]@{ processId = $process.Id; title = $process.MainWindowTitle; pdf = $fileName } | ConvertTo-Json -Compress
`;
    return parseJsonOutput(this.powershell(script));
  }

  focus() {
    return parseJsonOutput(this.powershell(`${guestInputType()}\n${focusRevu()}\n[ordered]@{ processId = $process.Id; title = $process.MainWindowTitle; focused = $true } | ConvertTo-Json -Compress`));
  }

  sendKeys(keys) {
    const script = `${guestInputType()}\nAdd-Type -AssemblyName System.Windows.Forms\n${focusRevu()}\n[System.Windows.Forms.SendKeys]::SendWait(${powershellQuote(keys)})\nStart-Sleep -Milliseconds 250`;
    this.powershell(script);
    return { keysSent: keys };
  }

  click({ x, y, count = 1 }) {
    const values = [x, y, count].map(Number);
    if (!values.every(Number.isFinite) || values[2] < 1) throw new Error('click requires finite x, y, and count >= 1');
    const [clickX, clickY, clickCount] = values;
    const script = `${guestInputType()}\n${focusRevu()}\n[GuestInput]::SetCursorPos(${clickX}, ${clickY}) | Out-Null\nStart-Sleep -Milliseconds 120\nfor ($i = 0; $i -lt ${clickCount}; $i++) {\n  [GuestInput]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)\n  Start-Sleep -Milliseconds 60\n  [GuestInput]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)\n  Start-Sleep -Milliseconds 100\n}`;
    this.powershell(script);
    return { x: clickX, y: clickY, count: clickCount };
  }

  windowState() {
    const script = `${guestInputType({ includeWindowRect: true })}\n${focusRevu()}\n$rect = New-Object GuestInput+RECT\n[GuestInput]::GetWindowRect($process.MainWindowHandle, [ref]$rect) | Out-Null\n[ordered]@{ processId = $process.Id; title = $process.MainWindowTitle; window = [ordered]@{ x = $rect.Left; y = $rect.Top; width = $rect.Right - $rect.Left; height = $rect.Bottom - $rect.Top } } | ConvertTo-Json -Compress -Depth 3`;
    return parseJsonOutput(this.powershell(script));
  }

  capture(hostOutputPath) {
    return this.prlctl(['capture', this.vmName, '--file', resolve(hostOutputPath)]);
  }
}

function guestInputType({ includeWindowRect = false } = {}) {
  const rect = includeWindowRect ? '[StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }' : '';
  const getWindowRect = includeWindowRect ? '[DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, ref RECT rect);' : '';
  return String.raw`
$source = @'
using System;
using System.Runtime.InteropServices;
public static class GuestInput {
  ${rect}
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extraInfo);
  ${getWindowRect}
}
'@
Add-Type -TypeDefinition $source
`;
}

function focusRevu() {
  return String.raw`
$process = Get-Process Revu -ErrorAction Stop | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not [GuestInput]::SetForegroundWindow($process.MainWindowHandle)) { throw 'Could not focus the Revu window' }
Start-Sleep -Milliseconds 250
`;
}

function defaultExecute(command, args) {
  return execFileSync(command, args, { encoding: 'utf8', maxBuffer: 32 * 1024 * 1024, timeout: 45_000, killSignal: 'SIGTERM' });
}
