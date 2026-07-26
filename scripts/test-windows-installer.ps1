param(
  [Parameter(Mandatory = $true)][ValidateSet('x64', 'arm64')][string]$Arch,
  [Parameter(Mandatory = $true)][ValidateSet('stable', 'beta')][string]$Channel,
  [Parameter(Mandatory = $true)][string]$ReleaseDirectory
)

$ErrorActionPreference = 'Stop'

function Assert-PeMachine([string]$Path, [UInt16]$Expected) {
  $stream = [System.IO.File]::OpenRead($Path)
  try {
    $reader = [System.IO.BinaryReader]::new($stream)
    $stream.Position = 0x3c
    $peOffset = $reader.ReadInt32()
    $stream.Position = $peOffset + 4
    $actual = $reader.ReadUInt16()
  } finally {
    $stream.Dispose()
  }
  if ($actual -ne $Expected) {
    throw ('PE machine 0x{0:X4} does not match expected 0x{1:X4}: {2}' -f $actual, $Expected, $Path)
  }
}

function Invoke-And-Wait([string]$Executable, [string[]]$Arguments) {
  $process = Start-Process -FilePath $Executable -ArgumentList $Arguments -Wait -PassThru
  if ($process.ExitCode -ne 0) {
    throw "$Executable exited with code $($process.ExitCode)"
  }
}

$releaseRoot = (Resolve-Path -LiteralPath $ReleaseDirectory).Path
$prefix = if ($Channel -eq 'beta') { 'Butter-Paper-Beta' } else { 'Butter-Paper' }
$product = if ($Channel -eq 'beta') { 'Butter Paper Beta' } else { 'Butter Paper' }
$installer = Join-Path $releaseRoot "$prefix-Windows-$Arch-Setup.exe"
if (-not (Test-Path -LiteralPath $installer -PathType Leaf)) {
  throw "Missing exact NSIS installer: $installer"
}

$expectedMachine = if ($Arch -eq 'arm64') { [UInt16]0xAA64 } else { [UInt16]0x8664 }
$smokeRoot = Join-Path $env:RUNNER_TEMP "butter-paper-nsis-$Channel-$Arch"
$installRoot = Join-Path $smokeRoot 'installed'
$userDataRoot = Join-Path $smokeRoot 'user-data'
$uninstaller = $null

if (Test-Path -LiteralPath $smokeRoot) {
  Remove-Item -LiteralPath $smokeRoot -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $smokeRoot, $userDataRoot | Out-Null

try {
  # NSIS requires /D= to be the final argument. The controlled path has no spaces.
  Invoke-And-Wait $installer @('/S', "/D=$installRoot")
  $application = Join-Path $installRoot "$product.exe"
  $installDeadline = [DateTime]::UtcNow.AddSeconds(60)
  while ((-not (Test-Path -LiteralPath $application -PathType Leaf)) -and [DateTime]::UtcNow -lt $installDeadline) {
    Start-Sleep -Milliseconds 500
  }
  if (-not (Test-Path -LiteralPath $application -PathType Leaf)) {
    Get-ChildItem -LiteralPath $smokeRoot -Recurse -Force -ErrorAction SilentlyContinue |
      ForEach-Object { Write-Host $_.FullName }
    throw "NSIS did not install the expected application: $application"
  }
  Assert-PeMachine $application $expectedMachine
  $runtimeLibraries = @(
    'd3dcompiler_47.dll',
    'dxcompiler.dll',
    'dxil.dll',
    'ffmpeg.dll',
    'libEGL.dll',
    'libGLESv2.dll',
    'vk_swiftshader.dll',
    'vulkan-1.dll'
  )
  foreach ($runtimeLibrary in $runtimeLibraries) {
    $runtimePath = Join-Path $installRoot $runtimeLibrary
    if (-not (Test-Path -LiteralPath $runtimePath -PathType Leaf)) {
      throw "NSIS did not install required Electron runtime library: $runtimePath"
    }
    Assert-PeMachine $runtimePath $expectedMachine
  }
  $unexpectedUpdateConfig = Join-Path $installRoot 'resources/app-update.yml'
  if (Test-Path -LiteralPath $unexpectedUpdateConfig) {
    throw "Unsigned Windows package unexpectedly contains updater configuration: $unexpectedUpdateConfig"
  }

  $uninstaller = Get-ChildItem -LiteralPath $installRoot -File -Filter 'Uninstall*.exe' | Select-Object -First 1
  if (-not $uninstaller) {
    throw "NSIS did not install an uninstaller in $installRoot"
  }

  $previousExecutable = $env:BP_ELECTRON_EXECUTABLE_PATH
  $previousReleaseChannel = $env:BP_RELEASE_CHANNEL
  $previousUserData = $env:BP_TEST_USER_DATA_DIR
  try {
    $env:BP_ELECTRON_EXECUTABLE_PATH = $application
    $env:BP_RELEASE_CHANNEL = $Channel
    $env:BP_TEST_USER_DATA_DIR = $userDataRoot
    & pnpm test:package:desktop
    if ($LASTEXITCODE -ne 0) {
      throw "Installed $product package smoke failed with code $LASTEXITCODE"
    }
  } finally {
    $env:BP_ELECTRON_EXECUTABLE_PATH = $previousExecutable
    $env:BP_RELEASE_CHANNEL = $previousReleaseChannel
    $env:BP_TEST_USER_DATA_DIR = $previousUserData
  }

  Invoke-And-Wait $uninstaller.FullName @('/S')
  $uninstaller = $null
  $deadline = [DateTime]::UtcNow.AddSeconds(30)
  while ((Test-Path -LiteralPath $installRoot) -and [DateTime]::UtcNow -lt $deadline) {
    Start-Sleep -Milliseconds 500
  }
  if (Test-Path -LiteralPath $installRoot) {
    throw "NSIS uninstall left the install directory behind: $installRoot"
  }
  Write-Host "Windows $Channel/$Arch NSIS install, native launch, feature smoke, and uninstall passed."
} finally {
  if ($null -ne $uninstaller -and (Test-Path -LiteralPath $uninstaller.FullName)) {
    Start-Process -FilePath $uninstaller.FullName -ArgumentList '/S' -Wait | Out-Null
  }
  if (Test-Path -LiteralPath $smokeRoot) {
    Remove-Item -LiteralPath $smokeRoot -Recurse -Force
  }
}
