function Get-ProjectRoot([string] $SourceRoot) {
  $source = Get-Item -LiteralPath $SourceRoot
  $resolved = $source.ResolveLinkTarget($true)
  if ($resolved) { $source = $resolved }
  $project = $source.Parent
  $resolved = $project.ResolveLinkTarget($true)
  if ($resolved) { $project = $resolved }
  return $project.FullName
}

# The movable local project owns sources, fonts, Cargo inputs and WiX; CI may supply its own tools.
function Set-ProjectBuildEnvironment([string] $SourceRoot, [switch] $WithoutCraftFonts) {
  $project = Get-ProjectRoot $SourceRoot
  if (-not (Test-Path -LiteralPath (Join-Path $project 'DependencyManifest.json'))) { return }
  $fonts = Join-Path $project 'craft-fonts'
  if (-not $WithoutCraftFonts) {
    if (-not (Test-Path -LiteralPath (Join-Path $fonts 'fonts/manifest.txt'))) { throw 'The project-owned craft-fonts input is missing.' }
    $env:CRAFT_FONTS_DIR = $fonts
    $env:CRAFT_FONTS_REQUIRED = '1'
  }
  if (-not $env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR = Join-Path $project '.build/target' }
}

function Invoke-ProjectWix {
  $project = Get-ProjectRoot (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
  $dll = Join-Path $project 'pdfcraft-build-tools/.store/wix/5.0.2/wix/5.0.2/tools/net6.0/any/wix.dll'
  if (Test-Path -LiteralPath $dll) {
    & dotnet $dll @args
  } elseif (Test-Path -LiteralPath (Join-Path $project 'DependencyManifest.json')) {
    throw 'The project-owned WiX 5.0.2 tool and native runtime files are missing.'
  } else {
    & wix @args
  }
}
