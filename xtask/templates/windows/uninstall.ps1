# Reversible per-user uninstall for Butter Paper @VERSION@ (@ARCH@).
$ErrorActionPreference = 'Stop'
$version = '@VERSION@'
$architecture = '@ARCH@'
$progId = 'ButterPaper.PDF.@VERSION@.@ARCH@'
$localAppData = [Environment]::GetFolderPath('LocalApplicationData')
$installRoot = Join-Path $localAppData ('Programs\Butter Paper\' + $version + '\' + $architecture)
$expectedRoot = [IO.Path]::GetFullPath($installRoot)
if (-not (Test-Path -LiteralPath $expectedRoot -PathType Container)) { throw ('Owned install directory is missing: ' + $expectedRoot) }
$markerPath = Join-Path $expectedRoot '.butter-paper-install.json'
if (-not (Test-Path -LiteralPath $markerPath -PathType Leaf)) { throw 'Ownership marker is missing; preserving the install directory.' }
try { $marker = Get-Content -LiteralPath $markerPath -Raw | ConvertFrom-Json } catch { throw 'Ownership marker is invalid; preserving the install directory.' }
if ($marker.product -ne 'Butter Paper' -or $marker.version -ne $version -or $marker.architecture -ne $architecture -or [IO.Path]::GetFullPath([string]$marker.installPath) -ne $expectedRoot) { throw 'Ownership marker does not match this exact package install; preserving the install directory.' }
$shortcutPath = Join-Path ([Environment]::GetFolderPath('Programs')) 'Butter Paper @VERSION@ (@ARCH@).lnk'
$shortcutMatches = $false
if (Test-Path -LiteralPath $shortcutPath) {
  $shell = New-Object -ComObject WScript.Shell
  $shortcut = $shell.CreateShortcut($shortcutPath)
  $shortcutMatches = ($shortcut.TargetPath -eq (Join-Path $expectedRoot 'butter-paper.exe'))
}
if ($shortcutMatches) { Remove-Item -LiteralPath $shortcutPath -Force }
$classes = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Software\Classes', $true)
if ($null -ne $classes) {
  try {
    $registration = $classes.OpenSubKey($progId)
    $owned = $false
    if ($null -ne $registration) { try { $command = $registration.OpenSubKey('shell\open\command'); if ($null -ne $command) { try { $owned = ($command.GetValue('') -eq ('"' + (Join-Path $expectedRoot 'butter-paper.exe') + '" "%1"')) } finally { $command.Dispose() } } } finally { $registration.Dispose() } }
    if ($owned) {
      $pdf = $classes.OpenSubKey('.pdf\OpenWithProgids', $true)
      if ($null -ne $pdf) { try { $pdf.DeleteValue($progId, $false) } finally { $pdf.Dispose() } }
      $classes.DeleteSubKeyTree($progId, $false)
    }
  } finally { $classes.Dispose() }
}
Remove-Item -LiteralPath $expectedRoot -Recurse -Force
Write-Output ('Removed this user installation at ' + $expectedRoot + '. Other PDF associations were preserved.')
