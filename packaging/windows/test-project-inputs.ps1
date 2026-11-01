$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'project-inputs.ps1')
$fixture = Join-Path ([IO.Path]::GetTempPath()) ('lion-project-inputs-' + [guid]::NewGuid().ToString('N'))
$before = @{}
foreach ($name in 'CRAFT_FONTS_DIR','CRAFT_FONTS_REQUIRED','CARGO_TARGET_DIR','CARGO_BUILD_BUILD_DIR','CARGO_HOME','TEMP','TMP','DIST','DOTNET_CLI_HOME','NUGET_PACKAGES','NUGET_HTTP_CACHE_PATH','NUGET_SCRATCH','XDG_CACHE_HOME','SCCACHE_DIR') { $before[$name] = [Environment]::GetEnvironmentVariable($name) }
try {
  $source = Join-Path $fixture 'pdfcraft-converter'
  New-Item -ItemType Directory -Path $source -Force | Out-Null
  $env:CRAFT_FONTS_DIR = 'unrelated-fonts'
  Set-ProjectBuildEnvironment $source
  if ($env:CRAFT_FONTS_DIR -ne 'unrelated-fonts') { throw 'An ordinary checkout must retain its configured inputs' }
  Set-Content -LiteralPath (Join-Path $fixture 'DependencyManifest.json') '{}'
  $rejected = $false
  try { Set-ProjectBuildEnvironment $source } catch { $rejected = $true }
  if (-not $rejected) { throw 'A portable project must reject missing owned fonts' }
  $fonts = Join-Path $fixture 'craft-fonts/fonts'
  New-Item -ItemType Directory -Path $fonts -Force | Out-Null
  Set-Content -LiteralPath (Join-Path $fonts 'manifest.txt') '# synthetic'
  Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
  Set-ProjectBuildEnvironment $source
  if ($env:CRAFT_FONTS_DIR -ne (Join-Path $fixture 'craft-fonts')) { throw 'The project must override unrelated font inputs' }
  if ($env:CRAFT_FONTS_REQUIRED -ne '1') { throw 'Portable builds must require their owned font input' }
  if ($env:CARGO_TARGET_DIR -ne (Join-Path $fixture '.build/target')) { throw 'Default build output must follow the moved project' }
  $env:CARGO_TARGET_DIR = Join-Path $fixture 'explicit-cache'
  Set-ProjectBuildEnvironment $source
  if ($env:CARGO_TARGET_DIR -ne (Join-Path $fixture 'explicit-cache')) { throw 'Explicit build-cache location must be preserved' }
  Remove-Item Env:CRAFT_FONTS_DIR -ErrorAction SilentlyContinue
  Set-ProjectBuildEnvironment $source -WithoutCraftFonts
  if ($env:CRAFT_FONTS_DIR) { throw 'The explicit font-free build must remain font-free' }

  $vars = 'CARGO_TARGET_DIR','CARGO_BUILD_BUILD_DIR','CARGO_HOME','TEMP','TMP','DIST','DOTNET_CLI_HOME','NUGET_PACKAGES','NUGET_HTTP_CACHE_PATH','NUGET_SCRATCH','XDG_CACHE_HOME','SCCACHE_DIR'
  $drive = [IO.Path]::GetPathRoot($fixture)
  @{ storageRouting = @{ activeDriveRoots = @($drive); activeCacheRoots = @((Join-Path $fixture 'explicit-cache')); minimumFreeBytes = 0 } } |
    ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $fixture 'DependencyManifest.json')
  foreach ($name in $vars) { [Environment]::SetEnvironmentVariable($name, (Join-Path $drive 'stale-unrelated-output')) }
  Set-ProjectBuildEnvironment $source
  foreach ($name in $vars) {
    $value = [Environment]::GetEnvironmentVariable($name)
    if (-not $value.StartsWith((Join-Path $fixture '.build') + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw "Inherited output escaped project: $name=$value" }
    if (-not (Test-Path -LiteralPath $value -PathType Container)) { throw "Output directory missing: $name" }
  }
  $env:CARGO_TARGET_DIR = Join-Path $fixture 'explicit-cache'
  Set-ProjectBuildEnvironment $source
  if ($env:CARGO_TARGET_DIR -ne (Join-Path $fixture 'explicit-cache')) { throw 'Owned explicit cache override was lost' }
  @{ storageRouting = @{ activeDriveRoots = @('Z:\'); activeCacheRoots = @(); minimumFreeBytes = 0 } } |
    ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $fixture 'DependencyManifest.json')
  $rejected = $false
  try { Set-ProjectBuildEnvironment $source } catch { $rejected = $true }
  if (-not $rejected) { throw 'Archive drive must be rejected before any build' }

  Write-Output 'Project input and storage routing regression checks passed'
} finally {
  foreach ($name in $before.Keys) { [Environment]::SetEnvironmentVariable($name,$before[$name]) }
  $resolved = [IO.Path]::GetFullPath($fixture)
  if (-not $resolved.StartsWith([IO.Path]::GetFullPath([IO.Path]::GetTempPath()),[StringComparison]::OrdinalIgnoreCase)) { throw 'Fixture escaped temporary directory' }
  if (Test-Path -LiteralPath $resolved) { Remove-Item -LiteralPath $resolved -Recurse -Force }
}
