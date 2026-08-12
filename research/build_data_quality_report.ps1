[CmdletBinding()]
param(
    [string]$SeriesResultPath = "",
    [string]$SeriesResultManifestPath = "",
    [string]$FeatureSnapshotPath = "",
    [string]$FeatureSnapshotManifestPath = "",
    [string]$TemporalSplitPath = "",
    [string]$TemporalSplitManifestPath = "",
    [string]$MarketGradePath = "",
    [string]$Version = "",
    [string]$OutputRoot = "",
    [ValidateRange(1, [int]::MaxValue)]
    [int]$MinimumEligibleSeries = 500
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-Sha256([string]$Path) {
    return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

function Get-RepositoryRelativePath([string]$Root, [string]$Path) {
    return ([System.IO.Path]::GetRelativePath($Root, $Path)).Replace('\', '/')
}

function Get-LatestDatasetFile([string]$Root, [string]$Dataset, [string]$FileName) {
    $candidate = Get-ChildItem -Path (Join-Path $Root "processed/$Dataset") -Recurse -Filter $FileName -File -ErrorAction SilentlyContinue |
        Sort-Object FullName |
        Select-Object -Last 1
    if ($null -eq $candidate) {
        throw "未找到上游数据集文件：dataset=$Dataset, file=$FileName"
    }
    return $candidate.FullName
}

function Assert-UpstreamDataset(
    [string]$RepositoryRoot,
    [string]$DatasetPath,
    [string]$ManifestPath,
    [string]$Label
) {
    foreach ($path in @($DatasetPath, $ManifestPath)) {
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "缺少 $Label 输入：$path"
        }
    }
    & cargo run --quiet --locked --bin validate_dataset_manifest -- $ManifestPath
    if ($LASTEXITCODE -ne 0) {
        throw "$Label Dataset Manifest v1 Rust 校验失败。"
    }
    $manifest = Get-Content -Raw -LiteralPath $ManifestPath | ConvertFrom-Json
    $relativePath = Get-RepositoryRelativePath $RepositoryRoot $DatasetPath
    $datasetHash = Get-Sha256 $DatasetPath
    if ([string]$manifest.output.relative_path -ne $relativePath -or
        [string]$manifest.output.sha256 -ne $datasetHash) {
        throw "$Label dataset 路径或 SHA-256 与 manifest 不一致。"
    }
    return [pscustomobject]@{
        Manifest = $manifest
        RelativePath = $relativePath
        DatasetHash = $datasetHash
        ManifestHash = Get-Sha256 $ManifestPath
        ManifestRelativePath = Get-RepositoryRelativePath $RepositoryRoot $ManifestPath
    }
}

function New-UpstreamReference([object]$Validated) {
    return [ordered]@{
        manifest_relative_path = $Validated.ManifestRelativePath
        manifest_sha256 = $Validated.ManifestHash
        output_relative_path = $Validated.RelativePath
        output_sha256 = $Validated.DatasetHash
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
    $SeriesResultPath = Get-LatestDatasetFile $OutputRoot "lol-series-results" "series-results.csv"
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

if ([string]::IsNullOrWhiteSpace($FeatureSnapshotPath)) {
    $FeatureSnapshotPath = Get-LatestDatasetFile $OutputRoot "lol-prematch-features" "prematch-feature-snapshots.json"
}
else {
    $FeatureSnapshotPath = [System.IO.Path]::GetFullPath($FeatureSnapshotPath)
}
if ([string]::IsNullOrWhiteSpace($FeatureSnapshotManifestPath)) {
    $FeatureSnapshotManifestPath = "$FeatureSnapshotPath.manifest.json"
}
else {
    $FeatureSnapshotManifestPath = [System.IO.Path]::GetFullPath($FeatureSnapshotManifestPath)
}

if ([string]::IsNullOrWhiteSpace($TemporalSplitPath)) {
    $TemporalSplitPath = Get-LatestDatasetFile $OutputRoot "lol-temporal-splits" "temporal-split-manifest.json"
}
else {
    $TemporalSplitPath = [System.IO.Path]::GetFullPath($TemporalSplitPath)
}
if ([string]::IsNullOrWhiteSpace($TemporalSplitManifestPath)) {
    $TemporalSplitManifestPath = "$TemporalSplitPath.manifest.json"
}
else {
    $TemporalSplitManifestPath = [System.IO.Path]::GetFullPath($TemporalSplitManifestPath)
}

if ([string]::IsNullOrWhiteSpace($MarketGradePath)) {
    $MarketGradePath = Join-Path $repositoryRoot "docs/DATA_009_HISTORICAL_MARKET_GRADES.csv"
}
else {
    $MarketGradePath = [System.IO.Path]::GetFullPath($MarketGradePath)
}
if (-not (Test-Path -LiteralPath $MarketGradePath -PathType Leaf)) {
    throw "缺少 DATA-009 市场等级输入：$MarketGradePath"
}

$seriesUpstream = Assert-UpstreamDataset $repositoryRoot $SeriesResultPath $SeriesResultManifestPath "HIST-003"
$featureUpstream = Assert-UpstreamDataset $repositoryRoot $FeatureSnapshotPath $FeatureSnapshotManifestPath "HIST-004"
$splitUpstream = Assert-UpstreamDataset $repositoryRoot $TemporalSplitPath $TemporalSplitManifestPath "HIST-005"

$seriesRows = @(Import-Csv -LiteralPath $SeriesResultPath)
if ($seriesRows.Count -eq 0) {
    throw "HIST-003 series result 输入为空。"
}
$seriesResults = [System.Collections.Generic.List[object]]::new()
$eventTimes = [System.Collections.Generic.List[datetimeoffset]]::new()
foreach ($row in $seriesRows) {
    foreach ($field in @(
            "series_id", "competition_id", "region", "patch", "scheduled_start_utc", "best_of",
            "team_1_id", "team_1_name", "team_2_id", "team_2_name", "team_1_score", "team_2_score",
            "winner_team_id", "mapping_evidence_id", "result_evidence_id", "market_id",
            "market_winner_outcome_index", "market_resolution_evidence_id", "duplicate_candidate_count"
        )) {
        if ([string]::IsNullOrWhiteSpace([string]$row.$field)) {
            throw "HIST-003 缺少质量审查必需字段：series_id=$($row.series_id), field=$field"
        }
    }
    $scheduledStart = [datetimeoffset]::Parse([string]$row.scheduled_start_utc).ToUniversalTime()
    $eventTimes.Add($scheduledStart)
    # Rust 质量门禁需要完整 SeriesResult，而不是只核对 CSV 表头或行数。
    $seriesResults.Add([pscustomobject][ordered]@{
            series_id = [string]$row.series_id
            competition_id = [string]$row.competition_id
            region = [string]$row.region
            patch = [string]$row.patch
            scheduled_start_utc = $scheduledStart.ToString("o")
            best_of = [int]$row.best_of
            team_ids = @([string]$row.team_1_id, [string]$row.team_2_id)
            team_names = @([string]$row.team_1_name, [string]$row.team_2_name)
            scores = @([int]$row.team_1_score, [int]$row.team_2_score)
            winner_team_id = [string]$row.winner_team_id
            mapping_evidence_id = [string]$row.mapping_evidence_id
            result_evidence_id = [string]$row.result_evidence_id
            market_id = [string]$row.market_id
            market_winner_outcome_index = [int]$row.market_winner_outcome_index
            market_resolution_evidence_id = [string]$row.market_resolution_evidence_id
            duplicate_candidate_count = [int]$row.duplicate_candidate_count
        })
}

$featureSnapshots = @(Get-Content -Raw -LiteralPath $FeatureSnapshotPath | ConvertFrom-Json)
$temporalSplit = Get-Content -Raw -LiteralPath $TemporalSplitPath | ConvertFrom-Json
$marketRows = @(Import-Csv -LiteralPath $MarketGradePath)
if ($marketRows.Count -eq 0) {
    throw "DATA-009 市场等级输入为空。"
}
$gradeCounts = [ordered]@{ A = 0; B = 0; C = 0; Unavailable = 0 }
foreach ($row in $marketRows) {
    $grade = [string]$row.grade
    if ([string]::IsNullOrWhiteSpace($grade)) {
        $grade = "Unavailable"
    }
    if (-not $gradeCounts.Contains($grade)) {
        throw "DATA-009 包含未知 grade：review_id=$($row.review_id), grade=$grade"
    }
    $gradeCounts[$grade]++
}

$marketGradeHash = Get-Sha256 $MarketGradePath
$marketGradeSnapshotDirectory = Join-Path $OutputRoot "raw/data_quality/review"
New-Item -ItemType Directory -Force -Path $marketGradeSnapshotDirectory | Out-Null
$marketGradeSnapshotPath = Join-Path $marketGradeSnapshotDirectory ("data-009.{0}.csv" -f $marketGradeHash.Substring(0, 12))
if (-not (Test-Path -LiteralPath $marketGradeSnapshotPath)) {
    # DATA-009 文档 CSV 先固定为 content-addressed raw snapshot，避免后续编辑改变本次报告证据。
    Copy-Item -LiteralPath $MarketGradePath -Destination $marketGradeSnapshotPath
}
if ((Get-Sha256 $marketGradeSnapshotPath) -ne $marketGradeHash) {
    throw "DATA-009 immutable snapshot hash 校验失败。"
}

$buildInput = [ordered]@{
    minimum_eligible_series = $MinimumEligibleSeries
    series_results = @($seriesResults)
    feature_snapshots = @($featureSnapshots)
    temporal_split_manifest = $temporalSplit
    market_grade_summary = [ordered]@{
        total_markets = $marketRows.Count
        grade_a = $gradeCounts.A
        grade_b = $gradeCounts.B
        grade_c = $gradeCounts.C
        unavailable = $gradeCounts.Unavailable
    }
}

$gitCommit = (& git -C $repositoryRoot rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $gitCommit -notmatch '^[0-9a-f]{40,64}$') {
    throw "无法读取生成时 Git commit。"
}
if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = "{0}.{1}.hist006" -f (Get-Date).ToUniversalTime().ToString("yyyy-MM-dd"), $gitCommit.Substring(0, 7)
}
if ($Version -notmatch '^[A-Za-z0-9._-]+$') {
    throw "Version 只能包含 ASCII 字母、数字、点、下划线和连字符。"
}
$processedDirectory = Join-Path $OutputRoot "processed/lol-data-quality-reports/$Version"
if (Test-Path -LiteralPath $processedDirectory) {
    throw "processed version 已存在，禁止覆盖：$processedDirectory"
}

$temporaryInput = Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-hist006-input-{0}.json" -f [guid]::NewGuid().ToString("N"))
$temporaryReportOne = Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-hist006-report-{0}.md" -f [guid]::NewGuid().ToString("N"))
$temporaryReportTwo = Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-hist006-report-{0}.md" -f [guid]::NewGuid().ToString("N"))
try {
    $buildInput | ConvertTo-Json -Depth 12 | Set-Content -Encoding utf8 -LiteralPath $temporaryInput
    # 同一输入连续构建两次并比较 bytes；只有重放一致时才发布 processed artifact。
    foreach ($reportPath in @($temporaryReportOne, $temporaryReportTwo)) {
        & cargo run --quiet --locked --bin build_data_quality_report -- $temporaryInput $reportPath
        if ($LASTEXITCODE -ne 0) {
            throw "Rust 数据质量报告构建失败。"
        }
    }
    $firstHash = Get-Sha256 $temporaryReportOne
    $secondHash = Get-Sha256 $temporaryReportTwo
    if ($firstHash -ne $secondHash) {
        throw "相同输入重放得到不同报告：first=$firstHash, second=$secondHash"
    }
    New-Item -ItemType Directory -Path $processedDirectory | Out-Null
    $datasetPath = Join-Path $processedDirectory "data-quality-report.md"
    Move-Item -LiteralPath $temporaryReportOne -Destination $datasetPath
}
finally {
    foreach ($temporaryPath in @($temporaryInput, $temporaryReportOne, $temporaryReportTwo)) {
        if (Test-Path -LiteralPath $temporaryPath) {
            Remove-Item -LiteralPath $temporaryPath
        }
    }
}

$reportText = Get-Content -Raw -LiteralPath $datasetPath
foreach ($requiredText in @("M2 Gate", "缺失率（Missingness）", "缺失与降级规则", "异常发现")) {
    if (-not $reportText.Contains($requiredText)) {
        throw "质量报告缺少必需内容：$requiredText"
    }
}
$gateMatch = [regex]::Match($reportText, 'M2 Gate：`(?<decision>ReadyForM3|NotReadyForM3)`')
if (-not $gateMatch.Success) {
    throw "质量报告缺少可解析的 M2 Gate decision。"
}
$gateDecision = $gateMatch.Groups["decision"].Value
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

$orderedEventTimes = @($eventTimes | Sort-Object)
$manifest = [ordered]@{
    manifest_version = 1
    dataset = [ordered]@{ name = "lol-data-quality-reports"; version = $Version }
    generated_at_utc = (Get-Date).ToUniversalTime().ToString("o")
    code = [ordered]@{ git_commit = $gitCommit; dirty = $dirty; diff_sha256 = $diffHash }
    generator = [ordered]@{
        entrypoint = "research/build_data_quality_report.ps1"
        arguments = @("-Version", $Version, "-MinimumEligibleSeries", [string]$MinimumEligibleSeries)
    }
    upstream_datasets = @(
        (New-UpstreamReference $seriesUpstream),
        (New-UpstreamReference $featureUpstream),
        (New-UpstreamReference $splitUpstream)
    )
    raw_inputs = @([ordered]@{
            source = "data_009_market_grade_review"
            relative_path = Get-RepositoryRelativePath $repositoryRoot $marketGradeSnapshotPath
            sha256 = $marketGradeHash
            captured_at_utc = (Get-Item -LiteralPath $marketGradeSnapshotPath).LastWriteTimeUtc.ToString("o")
        })
    output = [ordered]@{
        relative_path = Get-RepositoryRelativePath $repositoryRoot $datasetPath
        sha256 = $datasetHash
        row_count = $seriesRows.Count
        event_time_range_utc = [ordered]@{
            start = $orderedEventTimes[0].ToString("o")
            end = $orderedEventTimes[-1].ToString("o")
        }
    }
}
$manifestPath = "$datasetPath.manifest.json"
$manifest | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 -LiteralPath $manifestPath
& cargo run --quiet --locked --bin validate_dataset_manifest -- $manifestPath
if ($LASTEXITCODE -ne 0) {
    throw "HIST-006 Dataset Manifest v1 Rust 校验失败。"
}

[pscustomobject]@{
    Version = $Version
    EligibleSeries = $seriesRows.Count
    MinimumEligibleSeries = $MinimumEligibleSeries
    MarketGradeA = $gradeCounts.A
    MarketGradeB = $gradeCounts.B
    MarketGradeC = $gradeCounts.C
    MarketGradeUnavailable = $gradeCounts.Unavailable
    GateDecision = $gateDecision
    DeterministicReplay = ($firstHash -eq $secondHash)
    DatasetSha256 = $datasetHash
    Dataset = $datasetPath
    Manifest = $manifestPath
}
