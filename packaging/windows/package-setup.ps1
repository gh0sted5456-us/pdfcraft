<# Wrap the validated desktop MSI in the existing update/restart bootstrapper, without a printer. #>
param(
  [Parameter(Mandatory)][string] $MainInstaller,
  [ValidateSet('debug','release')][string] $Profile = 'release',
  [string] $TargetTriple = 'x86_64-pc-windows-msvc',
  [switch] $Online
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'project-inputs.ps1')
. (Join-Path $PSScriptRoot 'pe.ps1')
$Root = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$Root = Join-Path (Get-ProjectRoot $Root) (Split-Path $Root -Leaf)
Set-ProjectBuildEnvironment $Root
$MainInstaller = (Resolve-Path -LiteralPath $MainInstaller).Path
$Dist = if ($env:DIST) { $env:DIST } else { Join-Path $Root 'dist' }
New-Item -ItemType Directory -Force -Path $Dist | Out-Null
$Version = (Select-String -LiteralPath (Join-Path $Root 'Cargo.toml') -Pattern '^version = "([0-9]+\.[0-9]+\.[0-9]+)"' | Select-Object -First 1).Matches.Groups[1].Value
if (-not $Version) { throw 'Could not read the Lion PDF version' }
$BuildArgs = @('build','--locked','-p','pdfcraft-printer','--bin','pdfcraft-printer-setup','--features','desktop-only','--target',$TargetTriple,'-j2')
if (-not $Online) { $BuildArgs += '--offline' }
if ($Profile -eq 'release') { $BuildArgs += '--release' }
$before = $env:PDFCRAFT_SETUP_MSI
Push-Location $Root
try {
  $env:PDFCRAFT_SETUP_MSI = $MainInstaller
  & cargo @BuildArgs
  if ($LASTEXITCODE -ne 0) { throw 'Desktop bootstrapper build failed' }
  $TargetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $Root 'target' }
  $Binary = Join-Path $TargetDir "$TargetTriple/$Profile/pdfcraft-printer-setup.exe"
  Assert-PeHeader $Binary 0x8664 2
  # Existing installed updaters require this legacy asset name; its payload is desktop-only.
  $Setup = Join-Path $Dist "pdfcraft-$Version-with-printer-x64.exe"
  Copy-Item -LiteralPath $Binary -Destination $Setup -Force
  Get-FileHash -LiteralPath $Setup -Algorithm SHA256
} finally {
  $env:PDFCRAFT_SETUP_MSI = $before
  Pop-Location
}
