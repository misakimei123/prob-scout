[CmdletBinding()]
param(
    [string]$SeriesResultPath = "",
    [string]$SeriesResultManifestPath = "",
    [string]$Version = "",
    [string]$OutputRoot = "",
    [ValidateRange(1, 1440)]
    [int]$SnapshotLeadMinutes = 15,
    [ValidateRange(1, 730)]
    [int]$HistoryDays = 180,
    [ValidateRange(1, 100)]
    [int]$TeamBatchSize = 50,
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
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($Value)
        return [Convert]::ToHexString($sha256.ComputeHash($bytes)).ToLowerInvariant()
    }
    finally {
        $sha256.Dispose()
    }
}

function Get-RepositoryRelativePath([string]$Root, [string]$Path) {
    return ([System.IO.Path]::GetRelativePath($Root, $Path)).Replace('\', '/')
}

function Format-Utc([datetimeoffset]$Value) {
    return $Value.ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ss.fffffffZ")
}

function Parse-CargoUtc([string]$Value) {
    return [datetimeoffset]::ParseExact(
        $Value,
        "yyyy-MM-dd HH:mm:ss",
        [System.Globalization.CultureInfo]::InvariantCulture,
        [System.Globalization.DateTimeStyles]::AssumeUniversal
    ).ToUniversalTime()
}

function Escape-CargoString([string]$Value) {
    return $Value.Replace('\', '\\').Replace('"', '\"')
}

function Save-CargoPage(
    [System.Collections.IDictionary]$BaseParameters,
    [int]$Offset,
    [string]$QueryHash,
    [string]$Directory,
    [switch]$Force
) {
    $prefix = "team-history.{0}.offset-{1}" -f $QueryHash.Substring(0, 12), $Offset
    $cached = @(Get-ChildItem -LiteralPath $Directory -Filter "$prefix.*.json" -File -ErrorAction SilentlyContinue | Sort-Object Name)
    if (-not $Force -and $cached.Count -gt 0) {
        return $cached[-1].FullName
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
    $temporaryPath = Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-hist004-{0}.json" -f [guid]::NewGuid().ToString("N"))
    try {
        Invoke-WebRequest `
            -Uri $uri `
            -OutFile $temporaryPath `
            -UserAgent "prob-scout-research/0.1 (HIST-004 prematch features)" `
            -TimeoutSec 60
        $rows = @(Get-Content -Raw -LiteralPath $temporaryPath | ConvertFrom-Json)
        $contentHash = Get-Sha256 $temporaryPath
        $destination = Join-Path $Directory ("{0}.{1}.json" -f $prefix, $contentHash.Substring(0, 12))
        if (-not (Test-Path -LiteralPath $destination)) {
            Move-Item -LiteralPath $temporaryPath -Destination $destination
        }
        return $destination
    }
    finally {
        if (Test-Path -LiteralPath $temporaryPath) {
            Remove-Item -LiteralPath $temporaryPath
        }
    }
}

$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    $OutputRoot = Join-Path $repositoryRoot "data"
}
else {
    $OutputRoot = [System.IO.Path]::GetFullPath($OutputRoot)
}
if ([string]::IsNullOrWhiteSpace($SeriesResultPath)) {
    $candidate = Get-ChildItem -Path (Join-Path $OutputRoot "processed/lol-series-results") -Recurse -Filter "series-results.csv" -File -ErrorAction SilentlyContinue |
        Sort-Object FullName |
        Select-Object -Last 1
    if ($null -eq $candidate) {
        throw "未找到 HIST-003 series-results.csv。"
    }
    $SeriesResultPath = $candidate.FullName
}
else {
    $SeriesResultPath = [System.IO.Path]::GetFullPath($SeriesResultPath)
}
if ([string]::IsNullOrWhiteSpace($SeriesResultManifestPath)) {
    $SeriesResultManifestPath = "$SeriesResultPath.manifest.json"
}
else {
    $SeriesResultManifestPath = [System.IO.Path]::GetFullPath($SeriesResultManifestPath)
}
foreach ($path in @($SeriesResultPath, $SeriesResultManifestPath)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "缺少 HIST-003 输入：$path"
    }
}

& cargo run --quiet --locked --bin validate_dataset_manifest -- $SeriesResultManifestPath
if ($LASTEXITCODE -ne 0) {
    throw "HIST-003 manifest Rust 校验失败。"
}
$seriesManifest = Get-Content -Raw -LiteralPath $SeriesResultManifestPath | ConvertFrom-Json
$seriesRelativePath = Get-RepositoryRelativePath $repositoryRoot $SeriesResultPath
if ([string]$seriesManifest.output.relative_path -ne $seriesRelativePath) {
    throw "HIST-003 manifest output 路径与输入文件不一致。"
}
if ([string]$seriesManifest.output.sha256 -ne (Get-Sha256 $SeriesResultPath)) {
    throw "HIST-003 dataset hash 与 manifest 不一致。"
}

$gitCommit = (& git -C $repositoryRoot rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $gitCommit -notmatch '^[0-9a-f]{40,64}$') {
    throw "无法读取生成时 Git commit。"
}
if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = "{0}.{1}.hist004" -f (Get-Date).ToUniversalTime().ToString("yyyy-MM-dd"), $gitCommit.Substring(0, 7)
}
if ($Version -notmatch '^[A-Za-z0-9._-]+$') {
    throw "Version 只能包含 ASCII 字母、数字、点、下划线和连字符。"
}

$processedDirectory = Join-Path $OutputRoot "processed/lol-prematch-features/$Version"
if (Test-Path -LiteralPath $processedDirectory) {
    throw "processed version 已存在，禁止覆盖：$processedDirectory"
}
$historyDirectory = Join-Path $OutputRoot "raw/prematch_features/leaguepedia"
New-Item -ItemType Directory -Force -Path $historyDirectory | Out-Null

$seriesRows = @(Import-Csv -LiteralPath $SeriesResultPath)
if ($seriesRows.Count -eq 0) {
    throw "HIST-003 输入为空。"
}
$targets = [System.Collections.Generic.List[object]]::new()
$teamIdsByLeaguepediaName = @{}
$targetIds = [System.Collections.Generic.HashSet[string]]::new()
$targetTimes = [System.Collections.Generic.List[datetimeoffset]]::new()
foreach ($row in $seriesRows) {
    foreach ($field in @("series_id", "competition_id", "region", "patch", "scheduled_start_utc", "best_of", "team_1_id", "team_1_name", "team_2_id", "team_2_name")) {
        if ([string]::IsNullOrWhiteSpace([string]$row.$field)) {
            throw "HIST-003 赛前投影缺少字段：series_id=$($row.series_id), field=$field"
        }
    }
    if (-not $targetIds.Add([string]$row.series_id)) {
        throw "HIST-003 输入包含重复 series_id：$($row.series_id)"
    }
    $scheduledStart = [datetimeoffset]::Parse([string]$row.scheduled_start_utc).ToUniversalTime()
    $targetTimes.Add($scheduledStart)
    foreach ($index in 1..2) {
        $name = [string]$row.("team_{0}_name" -f $index)
        $teamId = [string]$row.("team_{0}_id" -f $index)
        if ($teamIdsByLeaguepediaName.ContainsKey($name) -and $teamIdsByLeaguepediaName[$name] -ne $teamId) {
            throw "同一 Leaguepedia 队名映射到多个 Canonical Team：$name"
        }
        $teamIdsByLeaguepediaName[$name] = $teamId
    }

    # 这里只构造显式赛前投影；比分、winner 和 market resolution 不进入 Rust builder 输入。
    $targets.Add([pscustomobject][ordered]@{
            series_id = [string]$row.series_id
            competition_id = [string]$row.competition_id
            region = [string]$row.region
            patch = [string]$row.patch
            scheduled_start_utc = Format-Utc $scheduledStart
            best_of = [int]$row.best_of
            team_ids = @([string]$row.team_1_id, [string]$row.team_2_id)
            source_team_keys = @([string]$row.team_1_name, [string]$row.team_2_name)
        })
}

$orderedTimes = @($targetTimes | Sort-Object)
$historyStart = $orderedTimes[0].AddDays(-$HistoryDays)
$historyEnd = $orderedTimes[-1]
$teamNames = @($teamIdsByLeaguepediaName.Keys | Sort-Object)
$historyRowsByKey = @{}
$historySignatures = @{}
$rawInputs = [System.Collections.Generic.List[object]]::new()
$historyQueryCount = 0
for ($batchStart = 0; $batchStart -lt $teamNames.Count; $batchStart += $TeamBatchSize) {
    $batchEnd = [Math]::Min($batchStart + $TeamBatchSize - 1, $teamNames.Count - 1)
    $quotedNames = @($teamNames[$batchStart..$batchEnd] | ForEach-Object { '"{0}"' -f (Escape-CargoString $_) })
    $namesClause = $quotedNames -join ','
    $where = 'MS.DateTime_UTC >= "{0}" AND MS.DateTime_UTC < "{1}" AND MS.BestOf IN (3,5) AND MS.Winner IN (1,2) AND (MS.Team1 IN ({2}) OR MS.Team2 IN ({2}))' -f `
        $historyStart.ToString("yyyy-MM-dd HH:mm:ss"),
        $historyEnd.ToString("yyyy-MM-dd HH:mm:ss"),
        $namesClause
    $baseParameters = [ordered]@{
        "tables[0]" = "MatchSchedule=MS,ScoreboardGames=SG"
        "fields[0]" = "MS.MatchId,MS.DateTime_UTC=MatchStartUtc,MS.Team1,MS.Team2,MS.Team1Score,MS.Team2Score,MS.Winner,MS.BestOf,SG.Patch,SG.N_GameInMatch,SG.DateTime_UTC=GameStartUtc,SG.Gamelength_Number"
        "where[0]" = $where
        "join_on[0]" = "MS.MatchId=SG.MatchId"
        "order_by[0]" = "MS.DateTime_UTC ASC,MS.MatchId ASC,SG.N_GameInMatch ASC"
        "limit[0]" = "500"
        format = "json"
    }
    $queryIdentity = ($baseParameters.GetEnumerator() | ForEach-Object { "{0}={1}" -f $_.Key, $_.Value }) -join "`n"
    $queryHash = Get-StringSha256 $queryIdentity
    $historyQueryCount++
    $pageHashes = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    $offset = 0
    do {
        $pagePath = Save-CargoPage -BaseParameters $baseParameters -Offset $offset -QueryHash $queryHash -Directory $historyDirectory -Force:$Refresh
        $pageRows = @(Get-Content -Raw -LiteralPath $pagePath | ConvertFrom-Json)
        $pageHash = Get-Sha256 $pagePath
        if ($pageRows.Count -gt 0 -and -not $pageHashes.Add($pageHash)) {
            throw "HIST-004 同一 batch 的不同 offset 返回相同页面：batchStart=$batchStart, offset=$offset"
        }
        foreach ($pageRow in $pageRows) {
            $gameNumber = [string]$pageRow.('N GameInMatch')
            $rowKey = "{0}|{1}" -f [string]$pageRow.MatchId, $gameNumber
            $signature = @(
                [string]$pageRow.MatchStartUtc,
                [string]$pageRow.Team1,
                [string]$pageRow.Team2,
                [string]$pageRow.Team1Score,
                [string]$pageRow.Team2Score,
                [string]$pageRow.Winner,
                [string]$pageRow.BestOf,
                [string]$pageRow.Patch,
                [string]$pageRow.GameStartUtc,
                [string]$pageRow.('Gamelength Number')
            ) -join "`n"
            if ($historySignatures.ContainsKey($rowKey) -and $historySignatures[$rowKey] -ne $signature) {
                throw "HIST-004 跨 batch 的同一 game row 存在冲突：$rowKey"
            }
            $historySignatures[$rowKey] = $signature
            $historyRowsByKey[$rowKey] = $pageRow
        }
        $rawInputs.Add([pscustomobject][ordered]@{
                source = "leaguepedia"
                relative_path = Get-RepositoryRelativePath $repositoryRoot $pagePath
                sha256 = $pageHash
                captured_at_utc = (Get-Item -LiteralPath $pagePath).LastWriteTimeUtc.ToString("o")
            })
        $offset += 500
    } while ($pageRows.Count -eq 500)
}
$historyRows = @($historyRowsByKey.Values | Sort-Object MatchId, { [int]$_.('N GameInMatch') })
if ($historyRows.Count -eq 0) {
    throw "Leaguepedia 历史查询没有返回任何目标队伍记录。"
}

$observations = [System.Collections.Generic.List[object]]::new()
$excludedIncompleteSeries = 0
foreach ($group in ($historyRows | Group-Object MatchId | Sort-Object Name)) {
    # Cargo 会把带下划线的原字段名渲染为空格，按实际 JSON key 读取。
    $rows = @($group.Group | Sort-Object { [int]$_.('N GameInMatch') })
    $first = $rows[0]
    $required = @("MatchStartUtc", "Team1", "Team2", "Team1Score", "Team2Score", "Winner", "BestOf")
    if ($required | Where-Object { [string]::IsNullOrWhiteSpace([string]$first.$_) }) {
        $excludedIncompleteSeries++
        continue
    }
    $bestOf = [int]$first.BestOf
    $scores = @([int]$first.Team1Score, [int]$first.Team2Score)
    $winnerIndex = [int]$first.Winner - 1
    $winsNeeded = [math]::Floor($bestOf / 2) + 1
    $patches = @($rows | ForEach-Object { [string]$_.Patch } | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Sort-Object -Unique)
    $hasMissingGameTime = @($rows | Where-Object {
            [string]::IsNullOrWhiteSpace([string]$_.GameStartUtc) -or
            [string]::IsNullOrWhiteSpace([string]$_.('Gamelength Number')) -or
            [double]$_.('Gamelength Number') -le 0
        }).Count -gt 0
    if ($bestOf -notin @(3, 5) -or
        $winnerIndex -notin @(0, 1) -or
        $scores[$winnerIndex] -ne $winsNeeded -or
        $scores[1 - $winnerIndex] -ge $winsNeeded -or
        $rows.Count -ne ($scores[0] + $scores[1]) -or
        $patches.Count -ne 1 -or
        $hasMissingGameTime) {
        $excludedIncompleteSeries++
        continue
    }

    $scheduledStart = Parse-CargoUtc ([string]$first.MatchStartUtc)
    $gameEnds = @($rows | ForEach-Object {
            (Parse-CargoUtc ([string]$_.GameStartUtc)).AddMinutes([double]$_.('Gamelength Number'))
        } | Sort-Object)
    $completedAt = $gameEnds[-1]
    if ($completedAt -le $scheduledStart) {
        $excludedIncompleteSeries++
        continue
    }

    foreach ($index in 0..1) {
        $teamName = [string]$first.("Team{0}" -f ($index + 1))
        if (-not $teamIdsByLeaguepediaName.ContainsKey($teamName)) {
            continue
        }
        # 历史记录按 Leaguepedia 精确 source key 回连；不把当前名称外推为跨名称 Canonical identity。
        $observations.Add([pscustomobject][ordered]@{
                series_id = "leaguepedia:$($group.Name)"
                source_team_key = $teamName
                patch = [string]$patches[0]
                scheduled_start_utc = Format-Utc $scheduledStart
                completed_at_utc = Format-Utc $completedAt
                best_of = $bestOf
                games_won = $scores[$index]
                games_lost = $scores[1 - $index]
                series_won = ($winnerIndex -eq $index)
            })
    }
}
if ($observations.Count -eq 0) {
    throw "没有可映射且带完成时间的历史队伍记录。"
}

$buildInput = [ordered]@{
    snapshot_lead_minutes = $SnapshotLeadMinutes
    targets = @($targets)
    team_series_observations = @($observations)
}
$temporaryInput = Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-hist004-input-{0}.json" -f [guid]::NewGuid().ToString("N"))
try {
    $buildInput | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 -LiteralPath $temporaryInput
    New-Item -ItemType Directory -Path $processedDirectory | Out-Null
    $datasetPath = Join-Path $processedDirectory "prematch-feature-snapshots.json"
    & cargo run --quiet --locked --bin build_prematch_feature_snapshots -- $temporaryInput $datasetPath
    if ($LASTEXITCODE -ne 0) {
        throw "Rust 赛前特征构建失败。"
    }
}
finally {
    if (Test-Path -LiteralPath $temporaryInput) {
        Remove-Item -LiteralPath $temporaryInput
    }
}

$snapshots = @(Get-Content -Raw -LiteralPath $datasetPath | ConvertFrom-Json)
if ($snapshots.Count -ne $targets.Count) {
    throw "特征快照数量与目标赛事数量不一致。"
}
$sourceTimeViolations = 0
$snapshotsWithAnyHistory = 0
foreach ($snapshot in $snapshots) {
    $snapshotAt = [datetimeoffset]$snapshot.snapshot_at_utc
    $hasHistory = $false
    foreach ($teamFeatures in @($snapshot.team_features)) {
        foreach ($featureName in @("prior_series_count", "prior_series_win_rate", "prior_game_count", "prior_game_win_rate", "same_patch_series_count", "same_patch_series_win_rate", "rest_minutes")) {
            $sourceTime = $teamFeatures.$featureName.source_latest_at_utc
            if ($null -ne $sourceTime) {
                $hasHistory = $true
                if ([datetimeoffset]$sourceTime -gt $snapshotAt) {
                    $sourceTimeViolations++
                }
            }
        }
    }
    if ($hasHistory) {
        $snapshotsWithAnyHistory++
    }
}
if ($sourceTimeViolations -ne 0) {
    throw "发现晚于赛前 cutoff 的特征来源时间：$sourceTimeViolations"
}
$datasetText = Get-Content -Raw -LiteralPath $datasetPath
if ($datasetText -match 'winner_team_id|team_[12]_score|market_resolution|result_evidence') {
    throw "特征输出泄漏了 HIST-003 的赛后 label 或市场结算字段。"
}

$datasetHash = Get-Sha256 $datasetPath
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

$generatedAtUtc = (Get-Date).ToUniversalTime().ToString("o")
$manifest = [ordered]@{
    manifest_version = 1
    dataset = [ordered]@{ name = "lol-prematch-features"; version = $Version }
    generated_at_utc = $generatedAtUtc
    code = [ordered]@{ git_commit = $gitCommit; dirty = $dirty; diff_sha256 = $diffHash }
    generator = [ordered]@{
        entrypoint = "research/build_prematch_feature_dataset.ps1"
        arguments = @("-Version", $Version, "-SnapshotLeadMinutes", [string]$SnapshotLeadMinutes, "-HistoryDays", [string]$HistoryDays, "-TeamBatchSize", [string]$TeamBatchSize)
    }
    upstream_datasets = @([ordered]@{
            manifest_relative_path = Get-RepositoryRelativePath $repositoryRoot $SeriesResultManifestPath
            manifest_sha256 = Get-Sha256 $SeriesResultManifestPath
            output_relative_path = $seriesRelativePath
            output_sha256 = Get-Sha256 $SeriesResultPath
        })
    raw_inputs = @($rawInputs | Sort-Object relative_path -Unique)
    output = [ordered]@{
        relative_path = Get-RepositoryRelativePath $repositoryRoot $datasetPath
        sha256 = $datasetHash
        row_count = $snapshots.Count
        event_time_range_utc = [ordered]@{
            start = Format-Utc $orderedTimes[0]
            end = Format-Utc $orderedTimes[-1]
        }
    }
}
$manifestPath = "$datasetPath.manifest.json"
$manifest | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 -LiteralPath $manifestPath
& cargo run --quiet --locked --bin validate_dataset_manifest -- $manifestPath
if ($LASTEXITCODE -ne 0) {
    throw "HIST-004 Dataset Manifest v1 Rust 校验失败。"
}

[pscustomobject]@{
    Version = $Version
    TargetSeries = $targets.Count
    HistoryDays = $HistoryDays
    HistoryRows = $historyRows.Count
    HistoryQueries = $historyQueryCount
    HistoryPages = $rawInputs.Count
    TeamObservations = $observations.Count
    ExcludedIncompleteSeries = $excludedIncompleteSeries
    Snapshots = $snapshots.Count
    SnapshotsWithAnyHistory = $snapshotsWithAnyHistory
    SourceTimeViolations = $sourceTimeViolations
    DatasetSha256 = $datasetHash
    Dataset = $datasetPath
    Manifest = $manifestPath
}
