# Exercise the real desktop packaging entry point with a synthetic compiler result; never install.
$ErrorActionPreference = 'Stop'
$workflow = Get-Content -LiteralPath (Join-Path $PSScriptRoot '../../.github/workflows/fork-release.yml') -Raw
$step = [regex]::Match($workflow, '(?s)- name: Package desktop.*?(?=- name: Publish verified fork assets)').Value
if (-not $step -or $step -match 'package-printer\.ps1|-PrinterPackage' -or $step -notmatch 'package-setup\.ps1') { throw 'Fork releases must package desktop-only setup without a virtual printer' }
$fixture = Join-Path ([IO.Path]::GetTempPath()) ('lion-desktop-setup-' + [guid]::NewGuid().ToString('N'))
$before = @{}
foreach ($name in 'CARGO_TARGET_DIR','DIST','PDFCRAFT_SETUP_MSI') { $before[$name] = [Environment]::GetEnvironmentVariable($name) }
try {
  New-Item -ItemType Directory -Path $fixture | Out-Null
  $msi = Join-Path $fixture 'desktop.msi'
  Set-Content -LiteralPath $msi -Value 'Synthetic input; never install'
  $env:CARGO_TARGET_DIR = Join-Path $fixture 'target'
  $env:DIST = Join-Path $fixture 'dist'
  $env:PDFCRAFT_SETUP_MSI = 'previous-value'
  $desktopBuild = @{ invocations = 0 }
  function cargo {
    if (($args -join ' ') -notmatch '--features desktop-only' -or $args -contains 'winrt-server') { throw 'Desktop packaging tried to build a virtual printer' }
    if ($env:PDFCRAFT_SETUP_MSI -ne $msi) { throw 'Wrong embedded desktop MSI' }
    $desktopBuild.invocations++
    $binary = Join-Path $env:CARGO_TARGET_DIR 'x86_64-pc-windows-msvc/debug/pdfcraft-printer-setup.exe'
    New-Item -ItemType Directory -Path (Split-Path $binary) -Force | Out-Null
    $pe = [byte[]]::new(256)
    [BitConverter]::GetBytes([int]128).CopyTo($pe, 0x3C)
    [BitConverter]::GetBytes([uint16]0x8664).CopyTo($pe, 132)
    [BitConverter]::GetBytes([uint16]2).CopyTo($pe, 220)
    [IO.File]::WriteAllBytes($binary, $pe)
    $global:LASTEXITCODE = 0
  }
  & (Join-Path $PSScriptRoot 'package-setup.ps1') -MainInstaller $msi -Profile debug | Out-Null
  $setups = @(Get-ChildItem -LiteralPath $env:DIST -File)
  if ($desktopBuild.invocations -ne 1 -or $setups.Count -ne 1 -or $setups[0].Name -notmatch '^pdfcraft-[0-9.]+-with-printer-x64\.exe$') { throw 'Missing updater-compatible desktop setup' }
  if ($env:PDFCRAFT_SETUP_MSI -ne 'previous-value') { throw 'Embedded input environment was not restored' }
  Write-Output 'Desktop-only release regression: no printer packaging, correct feature/MSI, GUI x64 validation and updater-compatible asset passed'
} finally {
  foreach ($name in $before.Keys) { [Environment]::SetEnvironmentVariable($name, $before[$name]) }
  $resolved = [IO.Path]::GetFullPath($fixture)
  if (-not $resolved.StartsWith([IO.Path]::GetFullPath([IO.Path]::GetTempPath()), [StringComparison]::OrdinalIgnoreCase)) { throw 'Fixture escaped temporary directory' }
  if (Test-Path -LiteralPath $resolved) { Remove-Item -LiteralPath $resolved -Recurse -Force }
}
