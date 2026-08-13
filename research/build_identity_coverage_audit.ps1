[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$CandidateManifest,
    [string]$Version = "",
    [string]$OutputRoot = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-Sha256([string]$Path) {
    return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

function Get-RepositoryRelativePath([string]$Root, [string]$Path) {
    return ([System.IO.Path]::GetRelativePath($Root, (Resolve-Path -LiteralPath $Path).Path)).Replace('\', '/')
}

function Format-Utc([datetimeoffset]$Value) {
    return $Value.ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ss'Z'")
}

function Save-ImmutableEvidenceSnapshot(
    [string]$SourcePath,
    [string]$DestinationDirectory
) {
    $sourceHash = Get-Sha256 $SourcePath
    $stem = [System.IO.Path]::GetFileNameWithoutExtension($SourcePath).ToLowerInvariant().Replace('_', '-')
    $destination = Join-Path $DestinationDirectory ("{0}.{1}.csv" -f $stem, $sourceHash.Substring(0, 12))
    if (Test-Path -LiteralPath $destination) {
        if ((Get-Sha256 $destination) -ne $sourceHash) {
            throw "HIST-002 raw snapshot 内容与文件名 hash 不一致：$destination"
        }
    }
    else {
        Copy-Item -LiteralPath $SourcePath -Destination $destination
    }
    return $destination
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
    $Version = "{0}.{1}.hist009" -f (Get-Date).ToUniversalTime().ToString("yyyy-MM-dd"), $gitCommit.Substring(0, 7)
}
if ($Version -notmatch '^[A-Za-z0-9._-]+$') {
    throw "Version 只能包含 ASCII 字母、数字、点、下划线和连字符。"
}

$processedDirectory = Join-Path $OutputRoot "processed/lol-identity-coverage-audits/$Version"
if (Test-Path -LiteralPath $processedDirectory) {
    throw "processed version 已存在，禁止覆盖：$processedDirectory"
}
$rawDirectory = Join-Path $OutputRoot "raw/identity_coverage/hist002"
New-Item -ItemType Directory -Force -Path $rawDirectory | Out-Null
$mappingReview = Save-ImmutableEvidenceSnapshot `
    -SourcePath (Join-Path $repositoryRoot "docs/DATA_008_MAPPING_REVIEW.csv") `
    -DestinationDirectory $rawDirectory
$teamReview = Save-ImmutableEvidenceSnapshot `
    -SourcePath (Join-Path $repositoryRoot "docs/HIST_002_TEAM_ALIAS_REVIEW.csv") `
    -DestinationDirectory $rawDirectory
$competitionReview = Save-ImmutableEvidenceSnapshot `
    -SourcePath (Join-Path $repositoryRoot "docs/HIST_002_COMPETITION_MAPPING.csv") `
    -DestinationDirectory $rawDirectory

$temporaryOutput = Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-hist009-output-{0}.json" -f [guid]::NewGuid().ToString("N"))
$replayOutput = Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-hist009-replay-{0}.json" -f [guid]::NewGuid().ToString("N"))
try {
    foreach ($output in @($temporaryOutput, $replayOutput)) {
        & cargo run --quiet --locked --bin build_identity_coverage_audit -- `
            --candidate-audit $candidateAudit `
            --mapping-review $mappingReview `
            --team-review $teamReview `
            --competition-review $competitionReview `
            --output $output
        if ($LASTEXITCODE -ne 0) {
            throw "HIST-009 Rust identity coverage audit 构建失败。"
        }
    }
    if ((Get-Sha256 $temporaryOutput) -ne (Get-Sha256 $replayOutput)) {
        throw "HIST-009 相同输入双重构建不一致。"
    }

    $audit = Get-Content -Raw -LiteralPath $temporaryOutput | ConvertFrom-Json
    $candidateCount = [int]$candidateManifestDocument.output.row_count
    if ([int]$audit.summary.candidate_count -ne $candidateCount) {
        throw "HIST-009 未覆盖全部 upstream candidates：actual=$($audit.summary.candidate_count), expected=$candidateCount"
    }
    if ([int]$audit.summary.fully_resolved_series + [int]$audit.summary.blocked_series -ne $candidateCount) {
        throw "HIST-009 resolved/blocked series 数量不守恒。"
    }
    if ([int]$audit.summary.team_occurrences.resolved + [int]$audit.summary.team_occurrences.missing + [int]$audit.summary.team_occurrences.ambiguous -ne 2 * $candidateCount) {
        throw "HIST-009 team occurrence 数量不守恒。"
    }
    if ([int]$audit.summary.competition_occurrences.resolved + [int]$audit.summary.competition_occurrences.missing + [int]$audit.summary.competition_occurrences.ambiguous -ne $candidateCount) {
        throw "HIST-009 competition occurrence 数量不守恒。"
    }
    if (@($audit.series_resolutions).Count -ne $candidateCount) {
        throw "HIST-009 series_resolutions 行数不等于 candidate 数。"
    }
    if (@($audit.review_queue).Count -ne [int]$audit.summary.review_queue_items) {
        throw "HIST-009 review_queue 行数与 summary 不一致。"
    }

    New-Item -ItemType Directory -Path $processedDirectory | Out-Null
    $datasetPath = Join-Path $processedDirectory "identity-coverage-audit.json"
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

$rawInputs = foreach ($rawPath in @($mappingReview, $teamReview, $competitionReview)) {
    [ordered]@{
        source = "reviewed_explicit_identity_evidence"
        relative_path = Get-RepositoryRelativePath $repositoryRoot $rawPath
        sha256 = Get-Sha256 $rawPath
        captured_at_utc = (Get-Item -LiteralPath $rawPath).LastWriteTimeUtc.ToString("o")
    }
}
$seriesTimes = @($audit.series_resolutions | ForEach-Object { [datetimeoffset]$_.scheduled_start_utc } | Sort-Object)
$manifest = [ordered]@{
    manifest_version = 1
    dataset = [ordered]@{ name = "lol-identity-coverage-audits"; version = $Version }
    generated_at_utc = (Get-Date).ToUniversalTime().ToString("o")
    code = [ordered]@{ git_commit = $gitCommit; dirty = $dirty; diff_sha256 = $diffHash }
    generator = [ordered]@{
        entrypoint = "research/build_identity_coverage_audit.ps1"
        arguments = @("-CandidateManifest", (Get-RepositoryRelativePath $repositoryRoot $CandidateManifest), "-Version", $Version)
    }
    upstream_datasets = @([ordered]@{
            manifest_relative_path = Get-RepositoryRelativePath $repositoryRoot $CandidateManifest
            manifest_sha256 = Get-Sha256 $CandidateManifest
            output_relative_path = [string]$candidateManifestDocument.output.relative_path
            output_sha256 = [string]$candidateManifestDocument.output.sha256
        })
    raw_inputs = @($rawInputs | Sort-Object relative_path)
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
    throw "HIST-009 Dataset Manifest v1 Rust 校验失败。"
}

[pscustomobject]@{
    Version = $Version
    CandidateRows = [int]$audit.summary.candidate_count
    FullyResolvedSeries = [int]$audit.summary.fully_resolved_series
    BlockedSeries = [int]$audit.summary.blocked_series
    TeamResolved = [int]$audit.summary.team_occurrences.resolved
    TeamMissing = [int]$audit.summary.team_occurrences.missing
    TeamAmbiguous = [int]$audit.summary.team_occurrences.ambiguous
    CompetitionResolved = [int]$audit.summary.competition_occurrences.resolved
    CompetitionMissing = [int]$audit.summary.competition_occurrences.missing
    CompetitionAmbiguous = [int]$audit.summary.competition_occurrences.ambiguous
    ReviewQueueItems = [int]$audit.summary.review_queue_items
    DeterministicReplay = $true
    DatasetSha256 = $datasetHash
    Dataset = $datasetPath
    Manifest = $manifestPath
}
