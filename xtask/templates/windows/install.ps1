# Per-user install for Butter Paper @VERSION@ (@ARCH@); no elevation is requested.
$ErrorActionPreference = 'Stop'
$version = '@VERSION@'
$architecture = '@ARCH@'
$progId = 'ButterPaper.PDF.@VERSION@.@ARCH@'
$source = $PSScriptRoot
$localAppData = [Environment]::GetFolderPath('LocalApplicationData')
if ([string]::IsNullOrWhiteSpace($localAppData)) { throw 'LocalAppData is unavailable.' }
$installRoot = Join-Path $localAppData ('Programs\Butter Paper\' + $version + '\' + $architecture)
$shortcutPath = Join-Path ([Environment]::GetFolderPath('Programs')) 'Butter Paper @VERSION@ (@ARCH@).lnk'
if (Test-Path -LiteralPath $installRoot) { throw ('Install destination already exists: ' + $installRoot) }
if (Test-Path -LiteralPath $shortcutPath) { throw ('Shortcut already exists: ' + $shortcutPath) }
$readClasses = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Software\Classes')
if ($null -ne $readClasses) {
  try {
    $existingProgId = $readClasses.OpenSubKey($progId)
    if ($null -ne $existingProgId) { $existingProgId.Dispose(); throw ('ProgID already exists: ' + $progId) }
    $existingOpenWith = $readClasses.OpenSubKey('.pdf\OpenWithProgids')
    if ($null -ne $existingOpenWith) {
      try { if ($existingOpenWith.GetValueNames() -contains $progId) { throw ('OpenWith value already exists: ' + $progId) } } finally { $existingOpenWith.Dispose() }
    }
  } finally { $readClasses.Dispose() }
}
$parent = Split-Path -Parent $installRoot
New-Item -ItemType Directory -Path $parent -Force | Out-Null
New-Item -ItemType Directory -Path $installRoot | Out-Null
$packageFiles = @(@PACKAGE_FILES@)
$expectedManifestFiles = @(@MANIFEST_FILES@)
$createdOpenWithValue = $false
$createdProgId = $false
$createdShortcut = $false
try {
  foreach ($name in $packageFiles) {
    $from = Join-Path $source $name
    if (-not (Test-Path -LiteralPath $from -PathType Leaf)) { throw ('Package file is missing: ' + $name) }
    Copy-Item -LiteralPath $from -Destination (Join-Path $installRoot $name)
  }
  $exe = Join-Path $installRoot 'butter-paper.exe'
  $manifestPath = Join-Path $installRoot 'MANIFEST.json'
  $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
  $expectedTarget = if ($architecture -eq 'arm64') { 'aarch64-pc-windows-msvc' } else { 'x86_64-pc-windows-msvc' }
  if ($manifest.product -ne 'Butter Paper' -or $manifest.version -ne $version -or $manifest.target -ne $expectedTarget) { throw 'Copied package manifest identity is invalid.' }
  $actualManifestFiles = @($manifest.files.PSObject.Properties.Name | Sort-Object)
  if (Compare-Object $expectedManifestFiles $actualManifestFiles) { throw 'Copied package manifest inventory is invalid.' }
  foreach ($name in $expectedManifestFiles) {
    $record = $manifest.files.$name
    $copiedFile = Join-Path $installRoot $name
    if (-not (Test-Path -LiteralPath $copiedFile -PathType Leaf)) { throw ('Manifest package file is missing: ' + $name) }
    $fileInfo = Get-Item -LiteralPath $copiedFile
    $fileHash = (Get-FileHash -LiteralPath $copiedFile -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($fileInfo.Length -ne $record.bytes -or $fileHash -ne $record.sha256) { throw ('Copied package file does not match manifest: ' + $name) }
  }
  $marker = [pscustomobject]@{ product = 'Butter Paper'; version = $version; architecture = $architecture; installPath = [IO.Path]::GetFullPath($installRoot) }
  $marker | ConvertTo-Json -Compress | Set-Content -LiteralPath (Join-Path $installRoot '.butter-paper-install.json') -Encoding UTF8
  $classes = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Software\Classes')
  try {
    $pdf = $classes.CreateSubKey('.pdf\OpenWithProgids')
    try { $createdOpenWithValue = $true; $pdf.SetValue($progId, [byte[]]@(), [Microsoft.Win32.RegistryValueKind]::None) } finally { $pdf.Dispose() }
    $registration = $classes.CreateSubKey($progId)
    if ($null -eq $registration) { throw 'Could not create the package ProgID.' }
    $createdProgId = $true
    try {
      $registration.SetValue('', 'Butter Paper PDF Document', [Microsoft.Win32.RegistryValueKind]::String)
      $icon = $registration.CreateSubKey('DefaultIcon')
      try { $icon.SetValue('', ('"' + (Join-Path $installRoot 'butter-paper.ico') + '"'), [Microsoft.Win32.RegistryValueKind]::String) } finally { $icon.Dispose() }
      $command = $registration.CreateSubKey('shell\open\command')
      try { $command.SetValue('', ('"' + $exe + '" "%1"'), [Microsoft.Win32.RegistryValueKind]::String) } finally { $command.Dispose() }
    } finally { $registration.Dispose() }
  } finally { $classes.Dispose() }
  $shell = New-Object -ComObject WScript.Shell
  $shortcut = $shell.CreateShortcut($shortcutPath)
  $shortcut.TargetPath = $exe
  $shortcut.WorkingDirectory = $installRoot
  $shortcut.IconLocation = (Join-Path $installRoot 'butter-paper.ico')
  $shortcut.Save()
  $createdShortcut = $true
} catch {
  if ($createdShortcut) { try { Remove-Item -LiteralPath $shortcutPath -Force -ErrorAction SilentlyContinue } catch {} }
  if ($createdOpenWithValue -or $createdProgId) {
    try {
      $rollbackClasses = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Software\Classes', $true)
      if ($null -ne $rollbackClasses) {
        try {
          if ($createdOpenWithValue) { $rollbackPdf = $rollbackClasses.OpenSubKey('.pdf\OpenWithProgids', $true); if ($null -ne $rollbackPdf) { try { $rollbackPdf.DeleteValue($progId, $false) } finally { $rollbackPdf.Dispose() } } }
          if ($createdProgId) { $rollbackClasses.DeleteSubKeyTree($progId, $false) }
        } finally { $rollbackClasses.Dispose() }
      }
    } catch {}
  }
  if (Test-Path -LiteralPath $installRoot) { Remove-Item -LiteralPath $installRoot -Recurse -Force -ErrorAction SilentlyContinue }
  throw
}
Write-Output ('Installed for this user at ' + $installRoot + '. PDF default choice was not changed.')
