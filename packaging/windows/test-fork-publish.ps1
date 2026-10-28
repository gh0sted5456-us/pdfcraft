# Exercise the actual publication step entirely in memory: no network, files or installation.
$ErrorActionPreference = 'Stop'
$workflow = Join-Path $PSScriptRoot '../../.github/workflows/fork-release.yml'
$inPublish = $false; $capture = $false; $code = @()
foreach ($line in Get-Content -LiteralPath $workflow) {
  if ($line -eq '      - name: Publish verified fork assets') { $inPublish = $true; continue }
  if ($inPublish -and $line -eq '        run: |') { $capture = $true; continue }
  if ($capture) {
    if ($line.StartsWith('          ')) { $code += $line.Substring(10) }
    elseif ($line.Trim()) { break }
  }
}
if ($code.Count -lt 10) { throw 'Publication step was not found' }
$publish = [scriptblock]::Create($code -join [Environment]::NewLine)
$previousRepo = $env:GITHUB_REPOSITORY; $previousVersion = $env:FORK_VERSION
try {
  $env:FORK_VERSION = '0.14.1'
  function Get-FileHash {
    param([string] $LiteralPath, [string] $Algorithm)
    if ($LiteralPath -ne 'dist/pdfcraft-0.14.1-with-printer-x64.exe' -or $Algorithm -ne 'SHA256') { throw 'Unexpected hash request' }
    [pscustomobject]@{ Hash = $script:publicationTest.hash }
  }
  function Get-Item {
    param([string] $LiteralPath)
    if ($LiteralPath -ne 'dist/pdfcraft-0.14.1-with-printer-x64.exe') { throw 'Unexpected file request' }
    [pscustomobject]@{ Length = $script:publicationTest.size }
  }
  function Set-Content {
    param([Parameter(Position=0)][string] $Path, [string] $Encoding, [Parameter(ValueFromPipeline=$true)] $Value)
    process {
      if ($Path -ne 'dist/SHA256SUMS.txt' -or $Encoding -ne 'utf8') { throw 'Unexpected output path' }
      $script:publicationTest.checksum = $Value
    }
  }
  function git {
    $global:LASTEXITCODE = 0
    if (($args -join ' ') -eq 'rev-parse HEAD') { return $script:publicationTest.head }
    if (($args -join ' ') -ne 'ls-remote origin refs/heads/main') { throw 'Unexpected git command' }
    $script:publicationTest.mainChecks++
    $head = $script:publicationTest.head
    if ($script:publicationTest.scenario -eq 'stale-before' -or ($script:publicationTest.scenario -eq 'stale-after' -and $script:publicationTest.mainChecks -gt 1)) { $head = 'b' * 40 }
    return "$head refs/heads/main"
  }
  function gh {
    $global:LASTEXITCODE = 0
    $state = $script:publicationTest
    if ($args[0] -eq 'release' -and $args[1] -eq 'create') {
      $state.created++
      if ($state.scenario -eq 'upload-failed') { $global:LASTEXITCODE = 1; return }
      return 'https://github.com/gh0sted5456-us/pdfcraft/releases/tag/untagged-test'
    }
    if ($args[0] -ne 'api') { throw 'Unexpected gh command' }
    $endpoint = @($args | Where-Object { $_ -like 'repos/*' })[0]
    if ($endpoint -like '*/releases/tags/*') { $global:LASTEXITCODE = 1; return } # The real draft lookup returned 404.
    if ($endpoint -eq 'repos/gh0sted5456-us/pdfcraft/releases?per_page=100') {
      if (($args -join ' ') -notmatch '--paginate --slurp') { throw 'All draft release pages must be read' }
      if ($state.scenario -eq 'list-failed') { $global:LASTEXITCODE = 1; return }
      $draft = @{ id = 123; tag_name = 'fork-v0.14.1'; draft = $true; target_commitish = $state.head }
      if ($state.scenario -eq 'wrong-head') { $draft.target_commitish = 'b' * 40 }
      if ($state.scenario -eq 'bad-id') { $draft.id = 'not/an/id' }
      $draftEntries = @($draft)
      if ($state.scenario -eq 'ambiguous') { $draftEntries += $draft.Clone() }
      return (ConvertTo-Json -Depth 8 -InputObject @(@(@{ id = 1; tag_name = 'fork-v0.14.0'; draft = $false }), $draftEntries))
    }
    if ($endpoint -ne 'repos/gh0sted5456-us/pdfcraft/releases/123') { throw "Unexpected API endpoint: $endpoint" }
    if ($args -contains 'PATCH') {
      if (($args -join ' ') -notmatch '-F draft=false -f make_latest=true') { throw 'Publishing must use the verified release ID' }
      $state.published++
      return
    }
    $digest = $state.hash; $bytes = $state.size; $uploaded = 'uploaded'; $target = $state.head
    if ($state.scenario -eq 'bad-digest') { $digest = '0' * 64 }
    if ($state.scenario -eq 'bad-size') { $bytes++ }
    if ($state.scenario -eq 'not-uploaded') { $uploaded = 'new' }
    if ($state.scenario -eq 'metadata-changed') { $target = 'b' * 40 }
    return (@{ id = 123; tag_name = 'fork-v0.14.1'; draft = $true; target_commitish = $target; assets = @(@{ name = 'pdfcraft-0.14.1-with-printer-x64.exe'; digest = "sha256:$digest"; size = $bytes; state = $uploaded }) } | ConvertTo-Json -Depth 6)
  }
  foreach ($case in @(
      @('good', ''), @('stale-before', 'newer main'), @('stale-after', 'advanced'),
      @('upload-failed', 'upload failed'), @('list-failed', 'read draft'),
      @('wrong-head', 'matching draft'), @('ambiguous', 'matching draft'),
      @('bad-id', 'release ID'), @('bad-digest', 'does not match'),
      @('bad-size', 'does not match'), @('not-uploaded', 'does not match'),
      @('metadata-changed', 'draft changed'), @('wrong-repo', 'personal fork'))) {
    $env:GITHUB_REPOSITORY = if ($case[0] -eq 'wrong-repo') { 'someone/else' } else { 'gh0sted5456-us/pdfcraft' }
    $script:publicationTest = @{ scenario = $case[0]; head = 'a' * 40; hash = 'c' * 64; size = 100; checksum = ''; created = 0; published = 0; mainChecks = 0 }
    $failure = $null
    try { & $publish | Out-Null } catch { $failure = $_.Exception.Message }
    if ($case[0] -in @('stale-before','wrong-repo') -and $script:publicationTest.created -ne 0) { throw "Case $($case[0]) uploaded before checking its source" }
    if (-not $case[1]) {
      if ($failure -or $script:publicationTest.published -ne 1 -or $script:publicationTest.checksum -ne (('c' * 64) + '  pdfcraft-0.14.1-with-printer-x64.exe')) { throw "Good draft must publish by ID: $failure" }
    } elseif (-not $failure -or $failure -notmatch [regex]::Escape($case[1]) -or $script:publicationTest.published -ne 0) {
      throw "Case $($case[0]) did not fail safely: $failure"
    }
  }
  Write-Output 'Release publication self-check: draft IDs, pagination, digest/size/state, fork/source guards and interrupted uploads passed (13 cases)'
} finally {
  $env:GITHUB_REPOSITORY = $previousRepo; $env:FORK_VERSION = $previousVersion
}
