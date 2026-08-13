[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$CandidateManifest,
    [string]$Version = "",
    [string]$OutputRoot = "",
    [ValidateRange(100, 500)]
    [int]$PageSize = 500,
    [ValidateRange(1, 100)]
    [int]$MaxPagesPerQuery = 50,
    [switch]$Refresh
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-Sha256([string]$Path) {
    return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

function Get-StringSha256([string]$Value) {
    $sha256 = [System.Security.Cryptography.SHA256]::Create()
    try {
        return [Convert]::ToHexString(
            $sha256.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($Value))
        ).ToLowerInvariant()
    }
    finally {
        $sha256.Dispose()
    }
}

function Get-RepositoryRelativePath([string]$Root, [string]$Path) {
    return ([System.IO.Path]::GetRelativePath($Root, (Resolve-Path -LiteralPath $Path).Path)).Replace('\', '/')
}

function Format-Utc([datetimeoffset]$Value) {
    return $Value.ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ss'Z'")
}

function ConvertTo-QueryHash([System.Collections.IDictionary]$Parameters) {
    $identity = ($Parameters.GetEnumerator() | ForEach-Object {
            "{0}={1}" -f [string]$_.Key, [string]$_.Value
        }) -join "`n"
    return Get-StringSha256 $identity
}

function Save-CargoPage(
    [string]$QueryName,
    [System.Collections.IDictionary]$BaseParameters,
    [int]$Offset,
    [string]$QueryHash,
    [string]$Directory,
    [switch]$Force
) {
    $prefix = "{0}.{1}.offset-{2}" -f $QueryName, $QueryHash.Substring(0, 12), $Offset
    $cached = @(Get-ChildItem -LiteralPath $Directory -Filter "$prefix.*.json" -File -ErrorAction SilentlyContinue | Sort-Object Name)
    if (-not $Force -and $cached.Count -gt 0) {
        $candidate = $cached[-1]
        Get-Content -Raw -LiteralPath $candidate.FullName | ConvertFrom-Json | Out-Null
        if ($candidate.BaseName.Split('.')[-1] -ne (Get-Sha256 $candidate.FullName).Substring(0, 12)) {
            throw "Leaguepedia 缓存文件名 hash 与内容不一致：$($candidate.FullName)"
        }
        return $candidate.FullName
    }

    $parameters = [ordered]@{}
    foreach ($entry in $BaseParameters.GetEnumerator()) {
        $parameters[$entry.Key] = $entry.Value
    }
    $parameters["offset[0]"] = [string]$Offset
    $queryPairs = foreach ($entry in $parameters.GetEnumerator()) {
        "{0}={1}" -f [uri]::EscapeDataString([string]$entry.Key), [uri]::EscapeDataString([string]$entry.Value)
    }
    $uri = "https://lol.fandom.com/wiki/Special:CargoExport?{0}" -f ($queryPairs -join "&")
    $temporaryPath = Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-hist010-page-{0}.tmp" -f [guid]::NewGuid().ToString("N"))
    try {
        $response = Invoke-WebRequest `
            -UseBasicParsing `
            -Uri $uri `
            -Headers @{ "User-Agent" = "prob-scout-research/0.1 (HIST-010 identity evidence)" } `
            -TimeoutSec 60 `
            -OutFile $temporaryPath `
            -PassThru
        $contentType = [string]$response.Headers["Content-Type"]
        if ([int]$response.StatusCode -ne 200 -or $contentType -notmatch '^application/json') {
            throw "Leaguepedia CargoExport 未返回预期 JSON：HTTP $($response.StatusCode)，Content-Type=$contentType"
        }
        Get-Content -Raw -LiteralPath $temporaryPath | ConvertFrom-Json | Out-Null
        $hash = Get-Sha256 $temporaryPath
        $destination = Join-Path $Directory ("{0}.{1}.json" -f $prefix, $hash.Substring(0, 12))
        if (Test-Path -LiteralPath $destination) {
            Remove-Item -LiteralPath $temporaryPath
        }
        else {
            Move-Item -LiteralPath $temporaryPath -Destination $destination
        }
        return $destination
    }
    catch {
        throw "Leaguepedia CargoExport 请求失败；不允许回退 HTML scraper。query=$QueryName offset=$Offset。原始错误：$($_.Exception.Message)"
    }
    finally {
        if (Test-Path -LiteralPath $temporaryPath) {
            Remove-Item -LiteralPath $temporaryPath
        }
    }
}

function Get-PagedCargoRows(
    [string]$QueryName,
    [string]$SourceName,
    [System.Collections.IDictionary]$BaseParameters,
    [string]$Directory,
    [int]$Limit,
    [int]$MaxPages,
    [string]$RepositoryRoot,
    [switch]$Force
) {
    $queryHash = ConvertTo-QueryHash $BaseParameters
    $rawInputs = [System.Collections.Generic.List[object]]::new()
    $paths = [System.Collections.Generic.List[string]]::new()
    $responseHashes = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    $rowCount = 0
    $completed = $false
    for ($page = 0; $page -lt $MaxPages; $page++) {
        $offset = $page * $Limit
        $pagePath = Save-CargoPage `
            -QueryName $QueryName `
            -BaseParameters $BaseParameters `
            -Offset $offset `
            -QueryHash $queryHash `
            -Directory $Directory `
            -Force:$Force
        $pageRows = @(Get-Content -Raw -LiteralPath $pagePath | ConvertFrom-Json)
        if ($pageRows.Count -gt $Limit) {
            throw "Leaguepedia 单页超过请求 limit：query=$QueryName rows=$($pageRows.Count) limit=$Limit"
        }
        $pageHash = Get-Sha256 $pagePath
        if ($pageRows.Count -gt 0 -and -not $responseHashes.Add($pageHash)) {
            throw "Leaguepedia 不同 offset 返回相同非空页面：query=$QueryName offset=$offset"
        }
        $rowCount += $pageRows.Count
        $paths.Add($pagePath)
        $rawInputs.Add([pscustomobject][ordered]@{
                source = $SourceName
                relative_path = Get-RepositoryRelativePath $RepositoryRoot $pagePath
                sha256 = $pageHash
                captured_at_utc = (Get-Item -LiteralPath $pagePath).LastWriteTimeUtc.ToString("o")
            })
        if ($pageRows.Count -lt $Limit) {
            $completed = $true
            break
        }
    }
    if (-not $completed) {
        throw "Leaguepedia 分页达到 MaxPagesPerQuery=$MaxPages，无法证明查询完整结束：query=$QueryName"
    }
    return [pscustomobject]@{
        QueryHash = $queryHash
        Paths = @($paths)
        RawInputs = @($rawInputs)
        PageCount = $paths.Count
        RowCount = $rowCount
    }
}

function Invoke-HistoricalIdentityBuilder(
    [string]$CandidateAudit,
    [string[]]$TeamRedirectPaths,
    [string[]]$TournamentPaths,
    [string]$Output
) {
    $arguments = @(
        "run", "--quiet", "--locked", "--bin", "build_historical_identity_audit", "--",
        "--candidate-audit", $CandidateAudit
    )
    foreach ($path in $TeamRedirectPaths) {
        $arguments += @("--team-redirects", $path)
    }
    foreach ($path in $TournamentPaths) {
        $arguments += @("--tournaments", $path)
    }
    $arguments += @("--output", $Output)
    & cargo @arguments
    if ($LASTEXITCODE -ne 0) {
        throw "HIST-010 Rust historical identity audit 构建失败。"
    }
}

$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    $OutputRoot = Join-Path $repositoryRoot "data"
}
else {
    $OutputRoot = [System.IO.Path]::GetFullPath($OutputRoot)
}
$CandidateManifest = (Resolve-Path -LiteralPath $CandidateManifest).Path
& cargo run --quiet --locked --bin validate_dataset_manifest -- $CandidateManifest
if ($LASTEXITCODE -ne 0) {
    throw "HIST-008 upstream Dataset Manifest v1 校验失败。"
}
$candidateManifestDocument = Get-Content -Raw -LiteralPath $CandidateManifest | ConvertFrom-Json
if ([string]$candidateManifestDocument.dataset.name -ne "lol-historical-series-candidates") {
    throw "CandidateManifest 不是 HIST-008 historical candidate dataset。"
}
$candidateAudit = Join-Path $repositoryRoot ([string]$candidateManifestDocument.output.relative_path)
if (-not (Test-Path -LiteralPath $candidateAudit -PathType Leaf)) {
    throw "HIST-008 upstream output 不存在：$candidateAudit"
}
if ((Get-Sha256 $candidateAudit) -ne [string]$candidateManifestDocument.output.sha256) {
    throw "HIST-008 upstream output hash 与 manifest 不一致。"
}

$gitCommit = (& git -C $repositoryRoot rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $gitCommit -notmatch '^[0-9a-f]{40}$') {
    throw "无法读取生成时 Git commit。"
}
if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = "{0}.{1}.hist010" -f (Get-Date).ToUniversalTime().ToString("yyyy-MM-dd"), $gitCommit.Substring(0, 7)
}
if ($Version -notmatch '^[A-Za-z0-9._-]+$') {
    throw "Version 只能包含 ASCII 字母、数字、点、下划线和连字符。"
}

$processedDirectory = Join-Path $OutputRoot "processed/lol-historical-identity-evidence/$Version"
if (Test-Path -LiteralPath $processedDirectory) {
    throw "processed version 已存在，禁止覆盖：$processedDirectory"
}
$rawDirectory = Join-Path $OutputRoot "raw/historical_identity/leaguepedia"
New-Item -ItemType Directory -Force -Path $rawDirectory | Out-Null

$teamRedirectParameters = [ordered]@{
    "tables[0]" = "TeamRedirects=R"
    "fields[0]" = "R._pageName=CanonicalPage,R.AllName"
    "order_by[0]" = "R.AllName ASC,R._pageName ASC"
    "limit[0]" = [string]$PageSize
    format = "json"
}
$tournamentParameters = [ordered]@{
    "tables[0]" = "Tournaments=T"
    "fields[0]" = "T.OverviewPage,T.Name,T.League,T.Year,T.Region"
    "order_by[0]" = "T.OverviewPage ASC,T.League ASC"
    "limit[0]" = [string]$PageSize
    format = "json"
}
$teamRedirectFetch = Get-PagedCargoRows `
    -QueryName "team-redirects" `
    -SourceName "leaguepedia_team_redirects" `
    -BaseParameters $teamRedirectParameters `
    -Directory $rawDirectory `
    -Limit $PageSize `
    -MaxPages $MaxPagesPerQuery `
    -RepositoryRoot $repositoryRoot `
    -Force:$Refresh
$tournamentFetch = Get-PagedCargoRows `
    -QueryName "tournaments" `
    -SourceName "leaguepedia_tournaments" `
    -BaseParameters $tournamentParameters `
    -Directory $rawDirectory `
    -Limit $PageSize `
    -MaxPages $MaxPagesPerQuery `
    -RepositoryRoot $repositoryRoot `
    -Force:$Refresh

$temporaryOutput = Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-hist010-output-{0}.json" -f [guid]::NewGuid().ToString("N"))
$replayOutput = Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-hist010-replay-{0}.json" -f [guid]::NewGuid().ToString("N"))
try {
    Invoke-HistoricalIdentityBuilder `
        -CandidateAudit $candidateAudit `
        -TeamRedirectPaths $teamRedirectFetch.Paths `
        -TournamentPaths $tournamentFetch.Paths `
        -Output $temporaryOutput
    Invoke-HistoricalIdentityBuilder `
        -CandidateAudit $candidateAudit `
        -TeamRedirectPaths $teamRedirectFetch.Paths `
        -TournamentPaths $tournamentFetch.Paths `
        -Output $replayOutput
    if ((Get-Sha256 $temporaryOutput) -ne (Get-Sha256 $replayOutput)) {
        throw "HIST-010 相同输入双重构建不一致。"
    }

    $audit = Get-Content -Raw -LiteralPath $temporaryOutput | ConvertFrom-Json
    $candidateCount = [int]$candidateManifestDocument.output.row_count
    if ([int]$audit.summary.candidate_count -ne $candidateCount) {
        throw "HIST-010 未覆盖全部 upstream candidates。"
    }
    if ([int]$audit.summary.fully_resolved_series + [int]$audit.summary.blocked_series -ne $candidateCount) {
        throw "HIST-010 resolved/blocked series 数量不守恒。"
    }
    if ([int]$audit.summary.series_result_count -ne [int]$audit.summary.fully_resolved_series) {
        throw "HIST-010 Series Result 数与 fully resolved series 不一致。"
    }
    if ([int]$audit.summary.resolved_team_key_count + [int]$audit.summary.unresolved_team_key_count -ne [int]$audit.summary.source_team_key_count) {
        throw "HIST-010 team source key 数量不守恒。"
    }
    if ([int]$audit.summary.resolved_competition_key_count + [int]$audit.summary.unresolved_competition_key_count -ne [int]$audit.summary.source_competition_key_count) {
        throw "HIST-010 competition source key 数量不守恒。"
    }

    New-Item -ItemType Directory -Path $processedDirectory | Out-Null
    $datasetPath = Join-Path $processedDirectory "historical-identity-audit.json"
    Move-Item -LiteralPath $temporaryOutput -Destination $datasetPath
}
finally {
    foreach ($temporaryPath in @($temporaryOutput, $replayOutput)) {
        if (Test-Path -LiteralPath $temporaryPath) {
            Remove-Item -LiteralPath $temporaryPath
        }
    }
}

$datasetHash = Get-Sha256 $datasetPath
$audit = Get-Content -Raw -LiteralPath $datasetPath | ConvertFrom-Json
$statusLines = @(& git -C $repositoryRoot status --porcelain=v1 --untracked-files=all)
$dirty = $statusLines.Count -gt 0
$diffHash = $null
if ($dirty) {
    $statePath = Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-worktree-{0}.txt" -f [guid]::NewGuid().ToString("N"))
    try {
        @(& git -C $repositoryRoot diff --binary HEAD) | Set-Content -Encoding utf8 -LiteralPath $statePath
        $untrackedPaths = @(& git -C $repositoryRoot ls-files --others --exclude-standard) | Where-Object { $_ -notlike '.idea/*' }
        foreach ($path in ($untrackedPaths | Sort-Object)) {
            $absolutePath = Join-Path $repositoryRoot $path
            if (Test-Path -LiteralPath $absolutePath -PathType Leaf) {
                Add-Content -Encoding utf8 -LiteralPath $statePath -Value ("UNTRACKED {0} {1}" -f $path, (Get-Sha256 $absolutePath))
            }
        }
        $diffHash = Get-Sha256 $statePath
    }
    finally {
        if (Test-Path -LiteralPath $statePath) {
            Remove-Item -LiteralPath $statePath
        }
    }
}

$rawInputs = @($teamRedirectFetch.RawInputs) + @($tournamentFetch.RawInputs)
$seriesTimes = @($audit.coverage.series_resolutions | ForEach-Object { [datetimeoffset]$_.scheduled_start_utc } | Sort-Object)
$generatorArguments = @(
    "-CandidateManifest", (Get-RepositoryRelativePath $repositoryRoot $CandidateManifest),
    "-Version", $Version,
    "-PageSize", [string]$PageSize,
    "-MaxPagesPerQuery", [string]$MaxPagesPerQuery
)
if ($Refresh) {
    $generatorArguments += "-Refresh"
}
$manifest = [ordered]@{
    manifest_version = 1
    dataset = [ordered]@{ name = "lol-historical-identity-evidence"; version = $Version }
    generated_at_utc = (Get-Date).ToUniversalTime().ToString("o")
    code = [ordered]@{ git_commit = $gitCommit; dirty = $dirty; diff_sha256 = $diffHash }
    generator = [ordered]@{
        entrypoint = "research/build_historical_identity_evidence.ps1"
        arguments = $generatorArguments
    }
    upstream_datasets = @([ordered]@{
            manifest_relative_path = Get-RepositoryRelativePath $repositoryRoot $CandidateManifest
            manifest_sha256 = Get-Sha256 $CandidateManifest
            output_relative_path = [string]$candidateManifestDocument.output.relative_path
            output_sha256 = [string]$candidateManifestDocument.output.sha256
        })
    raw_inputs = @($rawInputs | Sort-Object relative_path -Unique)
    output = [ordered]@{
        relative_path = Get-RepositoryRelativePath $repositoryRoot $datasetPath
        sha256 = $datasetHash
        row_count = [int]$audit.summary.candidate_count
        event_time_range_utc = [ordered]@{
            start = Format-Utc $seriesTimes[0]
            end = Format-Utc $seriesTimes[-1]
        }
    }
}
$manifestPath = "$datasetPath.manifest.json"
$manifest | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 -LiteralPath $manifestPath
& cargo run --quiet --locked --bin validate_dataset_manifest -- $manifestPath
if ($LASTEXITCODE -ne 0) {
    throw "HIST-010 Dataset Manifest v1 Rust 校验失败。"
}

[pscustomobject]@{
    Version = $Version
    TeamRedirectPages = $teamRedirectFetch.PageCount
    TeamRedirectRows = $teamRedirectFetch.RowCount
    TournamentPages = $tournamentFetch.PageCount
    TournamentRows = $tournamentFetch.RowCount
    CandidateRows = [int]$audit.summary.candidate_count
    ResolvedTeamKeys = [int]$audit.summary.resolved_team_key_count
    UnresolvedTeamKeys = [int]$audit.summary.unresolved_team_key_count
    AmbiguousTeamKeys = [int]$audit.summary.ambiguous_team_key_count
    ResolvedCompetitionKeys = [int]$audit.summary.resolved_competition_key_count
    UnresolvedCompetitionKeys = [int]$audit.summary.unresolved_competition_key_count
    AmbiguousCompetitionKeys = [int]$audit.summary.ambiguous_competition_key_count
    FullyResolvedSeries = [int]$audit.summary.fully_resolved_series
    BlockedSeries = [int]$audit.summary.blocked_series
    ReviewQueueItems = [int]$audit.coverage.summary.review_queue_items
    DeterministicReplay = $true
    DatasetSha256 = $datasetHash
    Dataset = $datasetPath
    Manifest = $manifestPath
}
