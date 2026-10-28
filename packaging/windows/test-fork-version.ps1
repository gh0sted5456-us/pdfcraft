$ErrorActionPreference='Stop'
$Script=Join-Path $PSScriptRoot 'fork-version.ps1'
$TestRoot=[IO.Directory]::CreateTempSubdirectory('pdfcraft-fork-version-').FullName
$PreviousRepo=$env:GITHUB_REPOSITORY; $PreviousEnv=$env:GITHUB_ENV
Push-Location $TestRoot
try {
 $env:GITHUB_REPOSITORY='gh0sted5456-us/pdfcraft'; $env:GITHUB_ENV=Join-Path $TestRoot 'github-env.txt'
 function gh { if (-not ($args -join ' ').EndsWith('--paginate --jq .[] | .tag_name')) {throw 'All release tags, including draft reservations, must be read'}; $global:LASTEXITCODE=0; $global:PdfCraftVersionTestTag }
 function cargo { if (($args -join ' ') -ne 'update --workspace --offline') {throw 'Unexpected cargo command'}; $global:LASTEXITCODE=0 }
 foreach($Case in @(
  @{source='0.14.0';latest='null';expected='0.14.0'},
  @{source='0.14.1';latest='null';expected='0.14.1'},
  @{source='0.14.1';latest=@('v0.14.4','v0.14.7');expected='0.14.8'},
  @{source='0.14.1';latest='fork-v0.14.5';expected='0.14.6'},
  @{source='0.15.0';latest='v0.14.9';expected='0.15.0'},
  @{source='0.14.0';latest='build-inputs-v1';expected='0.14.0'},
  @{source='0.14.0';latest=@('build-inputs-v2','fork-v0.14.7','build-inputs-v1');expected='0.14.8'},
  @{source='1.0.0';latest='fork-v0.14.1';expected='1.0.0'},
  @{source='1.0.0';latest='null';expected='1.0.0'},
  @{source='1.0.0';latest='fork-v1.0.0';expected='1.0.1'},
  @{source='1.0.0';latest=@('fork-v1.0.2','fork-v1.0.1');expected='1.0.3'},
  @{source='1.0.3';latest='fork-v1.0.0';expected='1.0.3'},
  @{source='1.0.65535';latest='null';expected='1.0.65535'},
  @{source='255.255.65535';latest='null';expected='255.255.65535'})) {
  $global:PdfCraftVersionTestTag=$Case.latest
  [IO.File]::WriteAllText((Join-Path $TestRoot 'Cargo.toml'),("[workspace.package]"+[Environment]::NewLine+'version = "'+$Case.source+'"'+[Environment]::NewLine))
  & $Script
  $Actual=[regex]::Match((Get-Content Cargo.toml -Raw),'(?m)^version = "([^"]+)"\r?$').Groups[1].Value
  if($Actual -cne $Case.expected -or (Get-Content $env:GITHUB_ENV -Tail 1) -cne "FORK_VERSION=$($Case.expected)"){throw "Expected $($Case.expected), got $Actual"}
 }
 foreach($Case in @(
  @{source='0.14.0';latest='nightly'},
  @{source='0.14.0';latest='build-inputs-nightly'},
  @{source='0.13.0';latest='v0.14.5'},
  @{source='1.0.0';latest='fork-v2.0.0'},
  @{source='1.0.0';latest='fork-v1.1.0'},
  @{source='1.0.65535';latest='fork-v1.0.65535'},
  @{source='256.0.0';latest='null'},
  @{source='1.256.0';latest='null'},
  @{source='1.0.65536';latest='null'})) {
  $global:PdfCraftVersionTestTag=$Case.latest
  [IO.File]::WriteAllText((Join-Path $TestRoot 'Cargo.toml'),("[workspace.package]"+[Environment]::NewLine+'version = "'+$Case.source+'"'+[Environment]::NewLine))
  $Rejected=$false
  try { & $Script } catch { $Rejected=$true }
  if(-not $Rejected){throw 'Unsafe version was not rejected'}
 }
 Write-Output 'Release version self-check: source floor, first 1.0.0, reserved/draft increments, build inputs, downgrade and MSI bounds passed (23 cases)'
} finally {
 Pop-Location
 $env:GITHUB_REPOSITORY=$PreviousRepo; $env:GITHUB_ENV=$PreviousEnv
 Remove-Item Function:\gh -ErrorAction SilentlyContinue
 Remove-Item Function:\cargo -ErrorAction SilentlyContinue
 Remove-Variable -Name PdfCraftVersionTestTag -Scope Global -ErrorAction SilentlyContinue
 $TempParent=[IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd([char[]]'\/')
 $ActualParent=[IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($TestRoot))
 if($ActualParent -ne $TempParent -or -not ([IO.Path]::GetFileName($TestRoot)).StartsWith('pdfcraft-fork-version-')) {throw 'Refusing cleanup outside the version-test folder'}
 Remove-Item -LiteralPath $TestRoot -Recurse -Force
}
