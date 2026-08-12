[CmdletBinding()]
param(
    [string]$FeatureSnapshotPath = "",
    [string]$FeatureSnapshotManifestPath = "",
    [string]$Version = "",
    [string]$OutputRoot = "",
    [string]$TrainStartUtc = "",
    [string]$ValidationStartUtc = "",
    [string]$CalibrationStartUtc = "",
    [string]$FinalTestStartUtc = "",
    [string]$FinalTestEndUtc = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-Sha256([string]$Path) {
    return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

function Get-RepositoryRelativePath([string]$Root, [string]$Path) {
    return ([System.IO.Path]::GetRelativePath($Root, $Path)).Replace('\', '/')
}

function Format-Utc([datetimeoffset]$Value) {
    return $Value.ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ss.fffffffZ")
}

function Parse-Utc([string]$Value, [string]$Field) {
    try {
        $parsed = [datetimeoffset]::Parse(
            $Value,
            [System.Globalization.CultureInfo]::InvariantCulture,
            [System.Globalization.DateTimeStyles]::AssumeUniversal
        )
    }
    catch {
        throw "$Field 不是有效 UTC 时间：$Value"
    }
    if ($parsed.Offset -ne [timespan]::Zero) {
        throw "$Field 必须显式使用 UTC offset Z 或 +00:00：$Value"
    }
    return $parsed.ToUniversalTime()
}

$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    $OutputRoot = Join-Path $repositoryRoot "data"
}
else {
    $OutputRoot = [System.IO.Path]::GetFullPath($OutputRoot)
}
if ([string]::IsNullOrWhiteSpace($FeatureSnapshotPath)) {
    $candidate = Get-ChildItem -Path (Join-Path $OutputRoot "processed/lol-prematch-features") -Recurse -Filter "prematch-feature-snapshots.json" -File -ErrorAction SilentlyContinue |
        Sort-Object FullName |
        Select-Object -Last 1
    if ($null -eq $candidate) {
        throw "未找到 HIST-004 prematch-feature-snapshots.json。"
    }
    $FeatureSnapshotPath = $candidate.FullName
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
foreach ($path in @($FeatureSnapshotPath, $FeatureSnapshotManifestPath)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "缺少 HIST-004 输入：$path"
    }
}

& cargo run --quiet --locked --bin validate_dataset_manifest -- $FeatureSnapshotManifestPath
if ($LASTEXITCODE -ne 0) {
    throw "HIST-004 manifest Rust 校验失败。"
}
$featureManifest = Get-Content -Raw -LiteralPath $FeatureSnapshotManifestPath | ConvertFrom-Json
$featureRelativePath = Get-RepositoryRelativePath $repositoryRoot $FeatureSnapshotPath
$featureHash = Get-Sha256 $FeatureSnapshotPath
if ([string]$featureManifest.output.relative_path -ne $featureRelativePath -or
    [string]$featureManifest.output.sha256 -ne $featureHash) {
    throw "HIST-004 dataset 路径或 hash 与 manifest 不一致。"
}

$snapshots = @(Get-Content -Raw -LiteralPath $FeatureSnapshotPath | ConvertFrom-Json)
if ($snapshots.Count -lt 4) {
    throw "时间划分至少需要 4 个 series，当前只有 $($snapshots.Count)。"
}
$candidates = [System.Collections.Generic.List[object]]::new()
$seriesIds = [System.Collections.Generic.HashSet[string]]::new()
$eventTimes = [System.Collections.Generic.List[datetimeoffset]]::new()
foreach ($snapshot in $snapshots) {
    if ([string]::IsNullOrWhiteSpace([string]$snapshot.series_id) -or
        [string]::IsNullOrWhiteSpace([string]$snapshot.scheduled_start_utc)) {
        throw "HIST-004 snapshot 缺少 series_id 或 scheduled_start_utc。"
    }
    if (-not $seriesIds.Add([string]$snapshot.series_id)) {
        throw "HIST-004 snapshot 包含重复 series_id：$($snapshot.series_id)"
    }
    $scheduledStart = Parse-Utc ([string]$snapshot.scheduled_start_utc) "scheduled_start_utc"
    $eventTimes.Add($scheduledStart)
    # 划分输入只投影 ID 与 Event 时间，不读取 feature value、winner 或 market 字段。
    $candidates.Add([pscustomobject][ordered]@{
            series_id = [string]$snapshot.series_id
            scheduled_start_utc = Format-Utc $scheduledStart
        })
}

$boundaryValues = @($TrainStartUtc, $ValidationStartUtc, $CalibrationStartUtc, $FinalTestStartUtc, $FinalTestEndUtc)
$providedBoundaryCount = @($boundaryValues | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }).Count
if ($providedBoundaryCount -notin @(0, 5)) {
    throw "时间边界必须全部省略或同时提供 TrainStartUtc、ValidationStartUtc、CalibrationStartUtc、FinalTestStartUtc、FinalTestEndUtc。"
}
if ($providedBoundaryCount -eq 0) {
    # 默认只在 UTC 自然日边界切分；至少四个日期，每个集合获得一个或多个完整日期。
    $dates = @($eventTimes | ForEach-Object { $_.UtcDateTime.Date } | Sort-Object -Unique)
    if ($dates.Count -lt 4) {
        throw "自动划分至少需要 4 个不同 UTC 日期；请扩充数据，不按单场计数切开同一天。"
    }
    $validationIndex = [math]::Floor($dates.Count / 4)
    $calibrationIndex = [math]::Floor($dates.Count / 2)
    $finalTestIndex = [math]::Floor(3 * $dates.Count / 4)
    if ($validationIndex -lt 1 -or
        $calibrationIndex -le $validationIndex -or
        $finalTestIndex -le $calibrationIndex -or
        $finalTestIndex -ge $dates.Count) {
        throw "无法从 UTC 日期生成四个非空连续窗口。"
    }
    $TrainStartUtc = ([datetimeoffset]::new($dates[0], [timespan]::Zero)).ToString("o")
    $ValidationStartUtc = ([datetimeoffset]::new($dates[$validationIndex], [timespan]::Zero)).ToString("o")
    $CalibrationStartUtc = ([datetimeoffset]::new($dates[$calibrationIndex], [timespan]::Zero)).ToString("o")
    $FinalTestStartUtc = ([datetimeoffset]::new($dates[$finalTestIndex], [timespan]::Zero)).ToString("o")
    $FinalTestEndUtc = ([datetimeoffset]::new($dates[-1].AddDays(1), [timespan]::Zero)).ToString("o")
}

$trainStart = Parse-Utc $TrainStartUtc "TrainStartUtc"
$validationStart = Parse-Utc $ValidationStartUtc "ValidationStartUtc"
$calibrationStart = Parse-Utc $CalibrationStartUtc "CalibrationStartUtc"
$finalTestStart = Parse-Utc $FinalTestStartUtc "FinalTestStartUtc"
$finalTestEnd = Parse-Utc $FinalTestEndUtc "FinalTestEndUtc"

$gitCommit = (& git -C $repositoryRoot rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $gitCommit -notmatch '^[0-9a-f]{40,64}$') {
    throw "无法读取生成时 Git commit。"
}
if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = "{0}.{1}.hist005" -f (Get-Date).ToUniversalTime().ToString("yyyy-MM-dd"), $gitCommit.Substring(0, 7)
}
if ($Version -notmatch '^[A-Za-z0-9._-]+$') {
    throw "Version 只能包含 ASCII 字母、数字、点、下划线和连字符。"
}
$processedDirectory = Join-Path $OutputRoot "processed/lol-temporal-splits/$Version"
if (Test-Path -LiteralPath $processedDirectory) {
    throw "processed version 已存在，禁止覆盖：$processedDirectory"
}

$buildInput = [ordered]@{
    source_dataset_sha256 = $featureHash
    plan = [ordered]@{
        train = [ordered]@{ start_utc = Format-Utc $trainStart; end_utc = Format-Utc $validationStart }
        validation = [ordered]@{ start_utc = Format-Utc $validationStart; end_utc = Format-Utc $calibrationStart }
        calibration = [ordered]@{ start_utc = Format-Utc $calibrationStart; end_utc = Format-Utc $finalTestStart }
        final_test = [ordered]@{ start_utc = Format-Utc $finalTestStart; end_utc = Format-Utc $finalTestEnd }
    }
    candidates = @($candidates)
}
$temporaryInput = Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-hist005-input-{0}.json" -f [guid]::NewGuid().ToString("N"))
try {
    $buildInput | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 -LiteralPath $temporaryInput
    New-Item -ItemType Directory -Path $processedDirectory | Out-Null
    $datasetPath = Join-Path $processedDirectory "temporal-split-manifest.json"
    & cargo run --quiet --locked --bin build_temporal_split_manifest -- $temporaryInput $datasetPath
    if ($LASTEXITCODE -ne 0) {
        throw "Rust 时间划分构建失败。"
    }
}
finally {
    if (Test-Path -LiteralPath $temporaryInput) {
        Remove-Item -LiteralPath $temporaryInput
    }
}

$splitManifest = Get-Content -Raw -LiteralPath $datasetPath | ConvertFrom-Json
$developmentIds = @($splitManifest.train.series_ids) + @($splitManifest.validation.series_ids) + @($splitManifest.calibration.series_ids)
if (@($developmentIds | Sort-Object -Unique).Count -ne $developmentIds.Count) {
    throw "development splits 存在重复 series ID。"
}
if ($developmentIds.Count + [int]$splitManifest.final_test.series_count -ne $snapshots.Count) {
    throw "四个 split 的总数与 HIST-004 输入不一致。"
}
if ($splitManifest.final_test.PSObject.Properties.Name -contains "series_ids") {
    throw "调参 manifest 不得暴露 final test series IDs。"
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

$orderedEventTimes = @($eventTimes | Sort-Object)
$manifest = [ordered]@{
    manifest_version = 1
    dataset = [ordered]@{ name = "lol-temporal-splits"; version = $Version }
    generated_at_utc = (Get-Date).ToUniversalTime().ToString("o")
    code = [ordered]@{ git_commit = $gitCommit; dirty = $dirty; diff_sha256 = $diffHash }
    generator = [ordered]@{
        entrypoint = "research/build_temporal_split_dataset.ps1"
        arguments = @(
            "-Version", $Version,
            "-TrainStartUtc", (Format-Utc $trainStart),
            "-ValidationStartUtc", (Format-Utc $validationStart),
            "-CalibrationStartUtc", (Format-Utc $calibrationStart),
            "-FinalTestStartUtc", (Format-Utc $finalTestStart),
            "-FinalTestEndUtc", (Format-Utc $finalTestEnd)
        )
    }
    upstream_datasets = @([ordered]@{
            manifest_relative_path = Get-RepositoryRelativePath $repositoryRoot $FeatureSnapshotManifestPath
            manifest_sha256 = Get-Sha256 $FeatureSnapshotManifestPath
            output_relative_path = $featureRelativePath
            output_sha256 = $featureHash
        })
    raw_inputs = @()
    output = [ordered]@{
        relative_path = Get-RepositoryRelativePath $repositoryRoot $datasetPath
        sha256 = $datasetHash
        row_count = $snapshots.Count
        event_time_range_utc = [ordered]@{
            start = Format-Utc $orderedEventTimes[0]
            end = Format-Utc $orderedEventTimes[-1]
        }
    }
}
$datasetManifestPath = "$datasetPath.manifest.json"
$manifest | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 -LiteralPath $datasetManifestPath
& cargo run --quiet --locked --bin validate_dataset_manifest -- $datasetManifestPath
if ($LASTEXITCODE -ne 0) {
    throw "HIST-005 Dataset Manifest v1 Rust 校验失败。"
}

[pscustomobject]@{
    Version = $Version
    SourceRows = $snapshots.Count
    TrainRows = @($splitManifest.train.series_ids).Count
    ValidationRows = @($splitManifest.validation.series_ids).Count
    CalibrationRows = @($splitManifest.calibration.series_ids).Count
    FinalTestRows = [int]$splitManifest.final_test.series_count
    FinalTestAccessPolicy = [string]$splitManifest.final_test.access_policy
    FinalTestMembershipSha256 = [string]$splitManifest.final_test.membership_sha256
    DatasetSha256 = $datasetHash
    Dataset = $datasetPath
    Manifest = $datasetManifestPath
}
