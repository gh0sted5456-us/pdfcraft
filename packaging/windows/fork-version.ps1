# CI-only version override: monotonically increase this fork's patch version.
$ErrorActionPreference='Stop'
if ($env:GITHUB_REPOSITORY -ne 'gh0sted5456-us/pdfcraft') { throw 'This release script belongs to the personal fork' }
$Manifest=Get-Content Cargo.toml -Raw
$Match=[regex]::Match($Manifest,'(?s)(\[workspace\.package\].*?version\s*=\s*")([0-9]+)\.([0-9]+)\.([0-9]+)(")')
if (-not $Match.Success) { throw 'Missing numeric workspace version' }
$Major=[int]$Match.Groups[2].Value; $Minor=[int]$Match.Groups[3].Value; $Patch=[int]$Match.Groups[4].Value
# Drafts reserve versions too, so failed uploads never collide with the next run.
$Tags=gh api "repos/$env:GITHUB_REPOSITORY/releases?per_page=100" --paginate --jq '.[] | .tag_name'
if ($LASTEXITCODE -ne 0) { throw 'Could not read fork releases' }
foreach ($Latest in @($Tags)) {
  if (-not $Latest -or $Latest -eq 'null') { continue }
  # Versioned OCR inputs are build dependencies, not installable application releases.
  if ($Latest -cmatch '^build-inputs-v[1-9][0-9]*$') { continue }
  if ($Latest -notmatch '^(?:fork-)?v([0-9]+)\.([0-9]+)\.([0-9]+)$') { throw 'A fork release has a nonnumeric version tag' }
  if ([int]$Matches[1] -gt $Major -or ([int]$Matches[1] -eq $Major -and [int]$Matches[2] -gt $Minor)) { throw 'Source version is older than the latest release series' }
  if ([int]$Matches[1] -eq $Major -and [int]$Matches[2] -eq $Minor) { $Patch=[Math]::Max($Patch,[int]$Matches[3]) }
}
$Patch++
if ($Major -gt 255 -or $Minor -gt 255 -or $Patch -gt 65535) { throw 'Version exceeds Windows Installer limits' }
$Version="$Major.$Minor.$Patch"
$Prefix=$Match.Groups[1].Value; $Suffix=$Match.Groups[5].Value
$Manifest=$Manifest.Remove($Match.Index,$Match.Length).Insert($Match.Index,"$Prefix$Version$Suffix")
[IO.File]::WriteAllText((Join-Path (Get-Location) 'Cargo.toml'),$Manifest)
cargo update --workspace --offline
if ($LASTEXITCODE -ne 0) { throw 'Could not update workspace versions in Cargo.lock' }
"FORK_VERSION=$Version" | Add-Content -LiteralPath $env:GITHUB_ENV
Write-Output "Building fork release $Version"
