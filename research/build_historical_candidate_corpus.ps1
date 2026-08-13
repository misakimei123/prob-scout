[CmdletBinding()]
param(
    [ValidatePattern('^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$')]
    [string]$StartUtc = "2025-01-01 00:00:00",
    [ValidatePattern('^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$')]
    [string]$EndUtc = "2025-07-01 00:00:00",
    [string]$Version = "",
    [string]$OutputRoot = "",
    [ValidateRange(100, 500)]
    [int]$PageSize = 500,
    [ValidateRange(1, 200)]
    [int]$MaxPagesPerQuery = 100,
    [ValidateRange(1, [int]::MaxValue)]
    [int]$MinimumCandidateCount = 700,
    [ValidateRange(1, 366)]
    [int]$MinimumDistinctUtcDates = 60,
    [ValidateRange(1, 100)]
    [int]$MinimumDistinctPatches = 3,
    [string]$ReferenceSeriesManifest = "",
    [ValidatePattern('^$|^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$')]
    [string]$MinimumRecoveryStartUtc = "",
    [ValidateRange(1, 100)]
    [int]$MinimumDistinctRegions = 3,
    [ValidateRange(1, [int]::MaxValue)]
    [int]$MinimumBo3Count = 1,
    [ValidateRange(1, [int]::MaxValue)]
    [int]$MinimumBo5Count = 1,
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
        # 缓存只有在 JSON 可解析且文件名 hash 与内容一致时才可复用。
        $candidate = $cached[-1]
        Get-Content -Raw -LiteralPath $candidate.FullName | ConvertFrom-Json | Out-Null
        $segments = $candidate.BaseName.Split('.')
        if ($segments[-1] -ne (Get-Sha256 $candidate.FullName).Substring(0, 12)) {
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
    $temporaryPath = Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-hist008-page-{0}.tmp" -f [guid]::NewGuid().ToString("N"))
    try {
        $response = Invoke-WebRequest `
            -UseBasicParsing `
            -Uri $uri `
            -Headers @{ "User-Agent" = "prob-scout-research/0.1 (HIST-008 historical candidates)" } `
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
    $rows = [System.Collections.Generic.List[object]]::new()
    $rawInputs = [System.Collections.Generic.List[object]]::new()
    $responseHashes = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    $offset = 0
    $completed = $false

    for ($page = 0; $page -lt $MaxPages; $page++) {
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
            throw "Leaguepedia 不同 offset 返回相同非空页面，停止避免重复/死循环：query=$QueryName offset=$offset"
        }
        foreach ($row in $pageRows) {
            $rows.Add($row)
        }
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
        $offset += $Limit
    }
    if (-not $completed) {
        throw "Leaguepedia 分页达到 MaxPagesPerQuery=$MaxPages，无法证明查询已完整结束：query=$QueryName"
    }

    return [pscustomobject]@{
        QueryHash = $queryHash
        Rows = @($rows)
        RawInputs = @($rawInputs)
        PageCount = $rawInputs.Count
    }
}

$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$start = [datetimeoffset]::ParseExact(
    $StartUtc,
    "yyyy-MM-dd HH:mm:ss",
    [System.Globalization.CultureInfo]::InvariantCulture,
    [System.Globalization.DateTimeStyles]::AssumeUniversal
).ToUniversalTime()
$end = [datetimeoffset]::ParseExact(
    $EndUtc,
    "yyyy-MM-dd HH:mm:ss",
    [System.Globalization.CultureInfo]::InvariantCulture,
    [System.Globalization.DateTimeStyles]::AssumeUniversal
).ToUniversalTime()
if ($start -ge $end) {
    throw "StartUtc 必须早于 EndUtc。"
}
if (($end - $start).TotalDays -gt 366) {
    throw "单次 HIST-008 查询不得超过 366 天；请拆分为不可变版本。"
}

$recoveryMode = -not [string]::IsNullOrWhiteSpace($ReferenceSeriesManifest) -or
    -not [string]::IsNullOrWhiteSpace($MinimumRecoveryStartUtc)
if ($recoveryMode -and (
        [string]::IsNullOrWhiteSpace($ReferenceSeriesManifest) -or
        [string]::IsNullOrWhiteSpace($MinimumRecoveryStartUtc)
    )) {
    throw "M3R-002 必须同时提供 ReferenceSeriesManifest 和 MinimumRecoveryStartUtc。"
}
$minimumRecoveryStart = $null
$referenceManifestDocument = $null
$referenceSeriesRows = @()
if ($recoveryMode) {
    $minimumRecoveryStart = [datetimeoffset]::ParseExact(
        $MinimumRecoveryStartUtc,
        "yyyy-MM-dd HH:mm:ss",
        [System.Globalization.CultureInfo]::InvariantCulture,
        [System.Globalization.DateTimeStyles]::AssumeUniversal
    ).ToUniversalTime()
    if ($start -lt $minimumRecoveryStart) {
        throw "M3R-002 StartUtc 不得早于 MinimumRecoveryStartUtc。"
    }
}

if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    $OutputRoot = Join-Path $repositoryRoot "data"
}
else {
    $OutputRoot = [System.IO.Path]::GetFullPath($OutputRoot)
}
$gitCommit = (& git -C $repositoryRoot rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $gitCommit -notmatch '^[0-9a-f]{40}$') {
    throw "无法读取生成时 Git commit。"
}
if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = "{0}.{1}.hist008" -f (Get-Date).ToUniversalTime().ToString("yyyy-MM-dd"), $gitCommit.Substring(0, 7)
}
if ($Version -notmatch '^[A-Za-z0-9._-]+$') {
    throw "Version 只能包含 ASCII 字母、数字、点、下划线和连字符。"
}

$rawDirectory = Join-Path $OutputRoot "raw/historical_candidates/leaguepedia"
$processedDirectory = Join-Path $OutputRoot "processed/lol-historical-series-candidates/$Version"
if (Test-Path -LiteralPath $processedDirectory) {
    throw "processed version 已存在，禁止覆盖：$processedDirectory"
}
New-Item -ItemType Directory -Force -Path $rawDirectory | Out-Null

if ($recoveryMode) {
    $ReferenceSeriesManifest = (Resolve-Path -LiteralPath $ReferenceSeriesManifest).Path
    & cargo run --quiet --locked --bin validate_dataset_manifest -- $ReferenceSeriesManifest
    if ($LASTEXITCODE -ne 0) {
        throw "M3R-002 reference Dataset Manifest v1 校验失败。"
    }
    $referenceManifestDocument = Get-Content -Raw -LiteralPath $ReferenceSeriesManifest | ConvertFrom-Json
    $referenceDatasetPath = Join-Path $repositoryRoot ([string]$referenceManifestDocument.output.relative_path)
    if (-not (Test-Path -LiteralPath $referenceDatasetPath -PathType Leaf)) {
        throw "M3R-002 reference dataset 不存在：$referenceDatasetPath"
    }
    if ((Get-Sha256 $referenceDatasetPath) -ne [string]$referenceManifestDocument.output.sha256) {
        throw "M3R-002 reference dataset hash 与 manifest 不一致。"
    }
    $referenceCsvRows = @(Import-Csv -LiteralPath $referenceDatasetPath)
    if ($referenceCsvRows.Count -ne [int]$referenceManifestDocument.output.row_count) {
        throw "M3R-002 reference dataset row count 与 manifest 不一致。"
    }
    $referenceSeriesRows = @($referenceCsvRows | ForEach-Object {
            [ordered]@{
                series_id = [string]$_.series_id
                scheduled_start_utc = [string]$_.scheduled_start_utc
            }
        })
}

$where = 'MS.DateTime_UTC >= "{0}" AND MS.DateTime_UTC < "{1}"' -f $StartUtc, $EndUtc
$seriesParameters = [ordered]@{
    "tables[0]" = "MatchSchedule=MS"
    "fields[0]" = "MS.MatchId,MS.DateTime_UTC=MatchStartUtc,MS.Team1,MS.Team2,MS.Team1Score,MS.Team2Score,MS.Winner,MS.BestOf,MS.OverviewPage"
    "where[0]" = $where
    "order_by[0]" = "MS.DateTime_UTC ASC,MS.MatchId ASC"
    "limit[0]" = [string]$PageSize
    format = "json"
}
$gameParameters = [ordered]@{
    "tables[0]" = "MatchSchedule=MS,ScoreboardGames=SG"
    "fields[0]" = "MS.MatchId,SG.Patch,SG.N_GameInMatch,SG.DateTime_UTC=GameStartUtc,SG.Gamelength_Number"
    "where[0]" = $where
    "join_on[0]" = "MS.MatchId=SG.MatchId"
    "order_by[0]" = "MS.DateTime_UTC ASC,MS.MatchId ASC,SG.N_GameInMatch ASC"
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

$seriesFetch = Get-PagedCargoRows `
    -QueryName "match-schedule" `
    -SourceName "leaguepedia_match_schedule" `
    -BaseParameters $seriesParameters `
    -Directory $rawDirectory `
    -Limit $PageSize `
    -MaxPages $MaxPagesPerQuery `
    -RepositoryRoot $repositoryRoot `
    -Force:$Refresh
$gameFetch = Get-PagedCargoRows `
    -QueryName "scoreboard-games" `
    -SourceName "leaguepedia_scoreboard_games" `
    -BaseParameters $gameParameters `
    -Directory $rawDirectory `
    -Limit $PageSize `
    -MaxPages $MaxPagesPerQuery `
    -RepositoryRoot $repositoryRoot `
    -Force:$Refresh
$tournamentFetch = $null
if ($recoveryMode) {
    # Region 在候选阶段只作为 OverviewPage 的精确 source coverage，不生成 canonical competition identity。
    # 复用 HIST-010 已完整分页保存的同一 Tournaments 查询；raw hash 仍逐页进入本次 manifest。
    $tournamentRawDirectory = Join-Path $OutputRoot "raw/historical_identity/leaguepedia"
    New-Item -ItemType Directory -Force -Path $tournamentRawDirectory | Out-Null
    $tournamentFetch = Get-PagedCargoRows `
        -QueryName "tournaments" `
        -SourceName "leaguepedia_tournaments" `
        -BaseParameters $tournamentParameters `
        -Directory $tournamentRawDirectory `
        -Limit $PageSize `
        -MaxPages $MaxPagesPerQuery `
        -RepositoryRoot $repositoryRoot `
        -Force:$Refresh
}

$temporaryInput = Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-hist008-input-{0}.json" -f [guid]::NewGuid().ToString("N"))
$temporaryOutput = Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-hist008-output-{0}.json" -f [guid]::NewGuid().ToString("N"))
$replayOutput = Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-hist008-replay-{0}.json" -f [guid]::NewGuid().ToString("N"))
try {
    [ordered]@{
        series_rows = @($seriesFetch.Rows)
        game_rows = @($gameFetch.Rows)
        tournament_rows = if ($null -eq $tournamentFetch) { @() } else { @($tournamentFetch.Rows) }
        reference_series_rows = @($referenceSeriesRows)
        minimum_recovery_start_utc = if ($null -eq $minimumRecoveryStart) { $null } else { Format-Utc $minimumRecoveryStart }
    } | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 -LiteralPath $temporaryInput

    $startRfc3339 = Format-Utc $start
    $endRfc3339 = Format-Utc $end
    & cargo run --quiet --locked --bin build_historical_candidate_audit -- `
        --input $temporaryInput `
        --output $temporaryOutput `
        --start-utc $startRfc3339 `
        --end-utc $endRfc3339
    if ($LASTEXITCODE -ne 0) {
        throw "HIST-008 Rust candidate audit 构建失败。"
    }
    & cargo run --quiet --locked --bin build_historical_candidate_audit -- `
        --input $temporaryInput `
        --output $replayOutput `
        --start-utc $startRfc3339 `
        --end-utc $endRfc3339
    if ($LASTEXITCODE -ne 0 -or (Get-Sha256 $temporaryOutput) -ne (Get-Sha256 $replayOutput)) {
        throw "HIST-008 相同输入双重构建不一致。"
    }

    $audit = Get-Content -Raw -LiteralPath $temporaryOutput | ConvertFrom-Json
    if ([int]$audit.coverage.candidate_count -lt $MinimumCandidateCount) {
        throw "HIST-008 candidate 数不足：actual=$($audit.coverage.candidate_count), minimum=$MinimumCandidateCount"
    }
    if ([int]$audit.coverage.distinct_utc_dates -lt $MinimumDistinctUtcDates) {
        throw "HIST-008 UTC 日期覆盖不足：actual=$($audit.coverage.distinct_utc_dates), minimum=$MinimumDistinctUtcDates"
    }
    $patchCount = @($audit.coverage.patches.PSObject.Properties).Count
    if ($patchCount -lt $MinimumDistinctPatches) {
        throw "HIST-008 Patch 覆盖不足：actual=$patchCount, minimum=$MinimumDistinctPatches"
    }
    if ([int]$audit.coverage.candidate_count + [int]$audit.coverage.rejected_count -ne [int]$audit.coverage.distinct_match_ids) {
        throw "HIST-008 candidate/rejection 未覆盖全部 MatchId。"
    }
    if ($recoveryMode) {
        if ($null -eq $audit.recovery_disjointness -or
            [int]$audit.recovery_disjointness.member_overlap_count -ne 0 -or
            [int]$audit.recovery_disjointness.temporal_overlap_count -ne 0) {
            throw "M3R-002 未形成成员和时间双重零重叠证明。"
        }
        $regionCoverage = $audit.source_region_coverage
        if ($null -eq $regionCoverage) {
            throw "M3R-002 缺少 source Region coverage。"
        }
        if ([int]$regionCoverage.resolved_candidate_count +
            [int]$regionCoverage.missing_candidate_count +
            [int]$regionCoverage.ambiguous_candidate_count -ne [int]$audit.coverage.candidate_count) {
            throw "M3R-002 source Region coverage 数量不守恒。"
        }
        $regionCount = @($regionCoverage.regions.PSObject.Properties).Count
        if ($regionCount -lt $MinimumDistinctRegions) {
            throw "M3R-002 Region 覆盖不足：actual=$regionCount, minimum=$MinimumDistinctRegions"
        }
        $bo3Count = [int]$audit.coverage.best_of.BO3
        $bo5Count = [int]$audit.coverage.best_of.BO5
        if ($bo3Count -lt $MinimumBo3Count -or $bo5Count -lt $MinimumBo5Count) {
            throw "M3R-002 BO 覆盖不足：BO3=$bo3Count/$MinimumBo3Count, BO5=$bo5Count/$MinimumBo5Count"
        }
    }

    New-Item -ItemType Directory -Path $processedDirectory | Out-Null
    $datasetPath = Join-Path $processedDirectory "historical-candidate-audit.json"
    Move-Item -LiteralPath $temporaryOutput -Destination $datasetPath
}
finally {
    foreach ($temporaryPath in @($temporaryInput, $temporaryOutput, $replayOutput)) {
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

$generatorArguments = @(
    "-StartUtc", $StartUtc,
    "-EndUtc", $EndUtc,
    "-Version", $Version,
    "-PageSize", [string]$PageSize,
    "-MaxPagesPerQuery", [string]$MaxPagesPerQuery,
    "-MinimumCandidateCount", [string]$MinimumCandidateCount,
    "-MinimumDistinctUtcDates", [string]$MinimumDistinctUtcDates,
    "-MinimumDistinctPatches", [string]$MinimumDistinctPatches
)
if ($recoveryMode) {
    $generatorArguments += @(
        "-ReferenceSeriesManifest", (Get-RepositoryRelativePath $repositoryRoot $ReferenceSeriesManifest),
        "-MinimumRecoveryStartUtc", $MinimumRecoveryStartUtc,
        "-MinimumDistinctRegions", [string]$MinimumDistinctRegions,
        "-MinimumBo3Count", [string]$MinimumBo3Count,
        "-MinimumBo5Count", [string]$MinimumBo5Count
    )
}
if ($Refresh) {
    $generatorArguments += "-Refresh"
}
$rawInputs = @($seriesFetch.RawInputs) + @($gameFetch.RawInputs)
if ($null -ne $tournamentFetch) {
    $rawInputs += @($tournamentFetch.RawInputs)
}
$upstreamDatasets = @()
if ($recoveryMode) {
    # 先赋给数组变量，避免 PowerShell 的 if pipeline 将单元素数组展开为 JSON object。
    $upstreamDatasets = @([ordered]@{
            manifest_relative_path = Get-RepositoryRelativePath $repositoryRoot $ReferenceSeriesManifest
            manifest_sha256 = Get-Sha256 $ReferenceSeriesManifest
            output_relative_path = [string]$referenceManifestDocument.output.relative_path
            output_sha256 = [string]$referenceManifestDocument.output.sha256
        })
}
$candidateTimes = @($audit.candidates | ForEach-Object { [datetimeoffset]$_.scheduled_start_utc } | Sort-Object)
$manifest = [ordered]@{
    manifest_version = 1
    dataset = [ordered]@{ name = "lol-historical-series-candidates"; version = $Version }
    generated_at_utc = (Get-Date).ToUniversalTime().ToString("o")
    code = [ordered]@{ git_commit = $gitCommit; dirty = $dirty; diff_sha256 = $diffHash }
    generator = [ordered]@{
        entrypoint = "research/build_historical_candidate_corpus.ps1"
        arguments = $generatorArguments
    }
    upstream_datasets = $upstreamDatasets
    raw_inputs = @($rawInputs | Sort-Object relative_path -Unique)
    output = [ordered]@{
        relative_path = Get-RepositoryRelativePath $repositoryRoot $datasetPath
        sha256 = $datasetHash
        row_count = [int]$audit.coverage.candidate_count
        event_time_range_utc = [ordered]@{
            start = Format-Utc $candidateTimes[0]
            end = Format-Utc $candidateTimes[-1]
        }
    }
}
$manifestPath = "$datasetPath.manifest.json"
$manifest | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 -LiteralPath $manifestPath
& cargo run --quiet --locked --bin validate_dataset_manifest -- $manifestPath
if ($LASTEXITCODE -ne 0) {
    throw "HIST-008 Dataset Manifest v1 Rust 校验失败。"
}

[pscustomobject]@{
    Version = $Version
    ScopeStartUtc = Format-Utc $start
    ScopeEndUtcExclusive = Format-Utc $end
    SeriesPages = $seriesFetch.PageCount
    GamePages = $gameFetch.PageCount
    RawSeriesRows = [int]$audit.coverage.raw_series_rows
    RawGameRows = [int]$audit.coverage.raw_game_rows
    DistinctMatchIds = [int]$audit.coverage.distinct_match_ids
    CandidateRows = [int]$audit.coverage.candidate_count
    RejectedRows = [int]$audit.coverage.rejected_count
    DistinctUtcDates = [int]$audit.coverage.distinct_utc_dates
    DistinctPatches = @($audit.coverage.patches.PSObject.Properties).Count
    DistinctRegions = if ($null -eq $audit.source_region_coverage) { 0 } else { @($audit.source_region_coverage.regions.PSObject.Properties).Count }
    RegionResolvedCandidates = if ($null -eq $audit.source_region_coverage) { 0 } else { [int]$audit.source_region_coverage.resolved_candidate_count }
    RegionMissingCandidates = if ($null -eq $audit.source_region_coverage) { 0 } else { [int]$audit.source_region_coverage.missing_candidate_count }
    RegionAmbiguousCandidates = if ($null -eq $audit.source_region_coverage) { 0 } else { [int]$audit.source_region_coverage.ambiguous_candidate_count }
    ReferenceSeriesRows = if ($null -eq $audit.recovery_disjointness) { 0 } else { [int]$audit.recovery_disjointness.reference_series_count }
    MemberOverlapRows = if ($null -eq $audit.recovery_disjointness) { 0 } else { [int]$audit.recovery_disjointness.member_overlap_count }
    TemporalOverlapRows = if ($null -eq $audit.recovery_disjointness) { 0 } else { [int]$audit.recovery_disjointness.temporal_overlap_count }
    Years = @($audit.coverage.years) -join ","
    DeterministicReplay = $true
    DatasetSha256 = $datasetHash
    Dataset = $datasetPath
    Manifest = $manifestPath
}
