<# Build the personal x64 PdfCraft + optional PdfCraft Printer combined installer.
   MainInstaller is a previously validated PdfCraft MSI. The printer is a separate Windows 11 24H2+ MSIX.
   No certificate store, Developer Mode setting, default printer or PDF association is changed. #>
param(
  [Parameter(Mandatory=$true)][string]$MainInstaller,
  [ValidateRange(1,65535)][int]$Revision = 1,
  [ValidateSet('debug','release')][string]$Profile = 'debug',
  [string]$TargetTriple = '',
  [switch]$Online
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'pe.ps1')
$Root = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$MainInstaller = (Resolve-Path -LiteralPath $MainInstaller).Path
$Version = (Select-String -LiteralPath (Join-Path $Root 'Cargo.toml') -Pattern '^version = "([0-9]+\.[0-9]+\.[0-9]+)"' | Select-Object -First 1).Matches.Groups[1].Value
if (-not $Version) { throw 'Could not read the PdfCraft version' }
$SdkRoot = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10/bin'
$Sdk = Get-ChildItem -LiteralPath $SdkRoot -Directory | Where-Object { $_.Name -match '^10\.0\.[0-9]+\.0$' -and [version]$_.Name -ge [version]'10.0.26100.0' } | Sort-Object { [version]$_.Name } -Descending | ForEach-Object { Join-Path $_.FullName 'x64' } | Where-Object { (Test-Path (Join-Path $_ 'MakeAppx.exe')) -and (Test-Path (Join-Path $_ 'MakePri.exe')) } | Select-Object -First 1
if (-not $Sdk) { throw 'A Windows 11 SDK with MakeAppx and MakePri is required' }
$MakeAppx = Join-Path $Sdk 'MakeAppx.exe'
$MakePri = Join-Path $Sdk 'MakePri.exe'
if (-not (Test-Path -LiteralPath $MakeAppx)) { throw 'Windows SDK 26100 x64 MakeAppx.exe is required' }
$TargetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $Root 'target' }
$Target = if ($TargetTriple) { Join-Path $TargetDir "$TargetTriple/$Profile" } else { Join-Path $TargetDir $Profile }
$BuildArgs = @('build','--locked','-p','pdfcraft-printer','-j2')
if (-not $Online) { $BuildArgs += '--offline' }
if ($Profile -eq 'release') { $BuildArgs += '--release' }
if ($TargetTriple) { $BuildArgs += @('--target',$TargetTriple) }
$Dist = Join-Path $Root 'dist'
New-Item -ItemType Directory -Force -Path $Dist | Out-Null
$Stage = Join-Path $TargetDir ('printer-package-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $Stage | Out-Null
Push-Location $Root
try {
  & cargo @BuildArgs --features winrt-server
  if ($LASTEXITCODE -ne 0) { throw 'Printer build failed' }
  Copy-Item -LiteralPath (Join-Path $Target 'pdfcraft_printer.dll'),(Join-Path $Target 'pdfcraft-printer-setup.exe') -Destination $Stage
  Assert-PeHeader (Join-Path $Stage 'pdfcraft-printer-setup.exe') 0x8664 2
  Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'printer/Printer.pdc') -Destination $Stage
  [xml]$Manifest = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'printer/AppxManifest.xml') -Raw
  $Manifest.Package.Identity.SetAttribute('Version', "$Version.$Revision")
  $Manifest.Save((Join-Path $Stage 'AppxManifest.xml'))
  New-Item -ItemType Directory -Path (Join-Path $Stage 'Assets') | Out-Null
  Copy-Item -LiteralPath (Join-Path $Root 'assets/app-icon/hicolor/256x256/apps/ai.storyteller.pdfcraft.png') -Destination (Join-Path $Stage 'Assets/Logo.png')
  $Strings = Join-Path $Stage 'Strings/en-US'
  New-Item -ItemType Directory -Path $Strings -Force | Out-Null
  Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'printer/Resources.resw') -Destination (Join-Path $Strings 'Resources.resw')
  # The virtual-printer deployment handler requires a PRI-backed display name.
  $PriConfig = Join-Path $TargetDir ('printer-priconfig-' + [guid]::NewGuid().ToString('N') + '.xml')
  & $MakePri createconfig /cf $PriConfig /dq en-US /o
  if ($LASTEXITCODE -ne 0) { throw 'Printer resource configuration failed' }
  & $MakePri new /pr $Stage /cf $PriConfig /of (Join-Path $Stage 'resources.pri') /o
  if ($LASTEXITCODE -ne 0) { throw 'Printer resource indexing failed' }
  foreach ($License in @('LICENSE-MIT','LICENSE-APACHE','NOTICE','assets/fonts/OFL-Inter.txt','assets/fonts/OFL-JetBrainsMono.txt','assets/fonts/OFL-DancingScript.txt','assets/app-icon/LICENSE.txt')) {
    $Name = if ($License -eq 'assets/app-icon/LICENSE.txt') { 'LICENSE-lion.txt' } else { Split-Path -Leaf $License }
    Copy-Item -LiteralPath (Join-Path $Root $License) -Destination (Join-Path $Stage $Name)
  }
  $Msix = Join-Path $Dist "PdfCraft.Printer-$Version.$Revision-x64.msix"
  & $MakeAppx pack /d $Stage /p $Msix /o
  if ($LASTEXITCODE -ne 0) { throw 'MSIX package validation failed' }
  $env:PDFCRAFT_SETUP_MSI = $MainInstaller
  $env:PDFCRAFT_SETUP_MSIX = $Msix
  & cargo @BuildArgs --bin pdfcraft-printer-setup --features bundle
  if ($LASTEXITCODE -ne 0) { throw 'Combined installer build failed' }
  $Setup = Join-Path $Dist "pdfcraft-$Version-with-printer-x64.exe"
  Assert-PeHeader (Join-Path $Target 'pdfcraft-printer-setup.exe') 0x8664 2
  Copy-Item -LiteralPath (Join-Path $Target 'pdfcraft-printer-setup.exe') -Destination $Setup -Force
  Get-FileHash -LiteralPath $Setup,$Msix,$MainInstaller -Algorithm SHA256 | Format-Table Path,Hash -AutoSize
} finally {
  Remove-Item Env:PDFCRAFT_SETUP_MSI -ErrorAction SilentlyContinue
  Remove-Item Env:PDFCRAFT_SETUP_MSIX -ErrorAction SilentlyContinue
  Pop-Location
}
