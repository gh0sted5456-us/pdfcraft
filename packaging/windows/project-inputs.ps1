function Resolve-ProjectPhysicalPath([string] $Path) {
  $full = [IO.Path]::GetFullPath($Path)
  $current = [IO.Path]::GetPathRoot($full)
  foreach ($part in $full.Substring($current.Length).Split([char[]]@('\','/'), [StringSplitOptions]::RemoveEmptyEntries)) {
    $current = Join-Path $current $part
    if (Test-Path -LiteralPath $current) {
      $resolved = (Get-Item -LiteralPath $current -Force).ResolveLinkTarget($true)
      if ($resolved) { $current = $resolved.FullName }
    }
  }
  return $current
}

function Get-ProjectRoot([string] $SourceRoot) {
  return Split-Path (Resolve-ProjectPhysicalPath $SourceRoot) -Parent
}

# Local storage rules live in the owned manifest; ordinary CI checkouts retain their configuration.
function Set-ProjectBuildEnvironment([string] $SourceRoot, [switch] $WithoutCraftFonts) {
  $project = Get-ProjectRoot $SourceRoot
  $manifest = Join-Path $project 'DependencyManifest.json'
  if (-not (Test-Path -LiteralPath $manifest)) { return }
  $routing = (Get-Content -LiteralPath $manifest -Raw | ConvertFrom-Json).storageRouting
  if ($routing) {
    if ([IO.Path]::GetPathRoot($project) -notin $routing.activeDriveRoots) {
      throw 'This is an archive or donor. Restore the owned working copy to F: (or D:) before building.'
    }
    if ([IO.DriveInfo]::new([IO.Path]::GetPathRoot($project)).AvailableFreeSpace -lt $routing.minimumFreeBytes) { throw 'Insufficient active-drive headroom; keep at least 40 GiB free.' }
    $allowed = @($project) + @($routing.activeCacheRoots)
    $defaults = [ordered]@{
      CARGO_TARGET_DIR = '.build/target'
      CARGO_BUILD_BUILD_DIR = '.build/intermediate'
      CARGO_HOME = '.build/cargo-home'
      TEMP = '.build/tmp'
      TMP = '.build/tmp'
      DIST = '.build/dist'
      DOTNET_CLI_HOME = '.build/dotnet'
      NUGET_PACKAGES = '.build/nuget/packages'
      NUGET_HTTP_CACHE_PATH = '.build/nuget/http'
      NUGET_SCRATCH = '.build/nuget/tmp'
      XDG_CACHE_HOME = '.build/cache'
      SCCACHE_DIR = '.build/sccache'
    }
    foreach ($name in $defaults.Keys) {
      $candidate = [Environment]::GetEnvironmentVariable($name)
      $accepted = $false
      if ($candidate -and [IO.Path]::IsPathFullyQualified($candidate)) {
        $candidate = Resolve-ProjectPhysicalPath $candidate
        foreach ($root in $allowed) {
          $physicalRoot = Resolve-ProjectPhysicalPath $root
          if (($candidate -eq $physicalRoot -or $candidate.StartsWith($physicalRoot.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) -and
              [IO.Path]::GetPathRoot($candidate) -in $routing.activeDriveRoots) { $accepted = $true; break }
        }
      }
      if (-not $accepted) {
        $candidate = Resolve-ProjectPhysicalPath (Join-Path $project $defaults[$name])
        if (-not $candidate.StartsWith($project.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw "Default output escapes the owned project: $name" }
      }
      if ([IO.Path]::GetPathRoot($candidate) -notin $routing.activeDriveRoots) { throw "Output path resolves outside the active drives: $name" }
      New-Item -ItemType Directory -Path $candidate -Force | Out-Null
      [Environment]::SetEnvironmentVariable($name, $candidate)
    }
  }
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
