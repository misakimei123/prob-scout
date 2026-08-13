[CmdletBinding()]
param(
    [string]$SeriesResultPath = "",
    [string]$SeriesResultManifestPath = "",
    [string]$MarketLinkPath = "",
    [string]$MarketLinkManifestPath = "",
    [string]$TemporalSplitPath = "",
    [string]$TemporalSplitManifestPath = "",
    [string]$MappingReviewPath = "",
    [string]$MarketGradePath = "",
    [string]$Version = "",
    [string]$ArtifactRoot = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-Sha256([string]$Path) {
    return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

function Get-RepositoryRelativePath([string]$Root, [string]$Path) {
    return ([System.IO.Path]::GetRelativePath($Root, $Path)).Replace('\', '/')
}

function Assert-UpstreamDataset(
    [string]$RepositoryRoot,
    [string]$DatasetPath,
    [string]$ManifestPath,
    [string]$ExpectedDatasetName
) {
    foreach ($path in @($DatasetPath, $ManifestPath)) {
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "缺少 MODEL-003 上游输入：$path"
        }
    }
    & cargo run --quiet --locked --bin validate_dataset_manifest -- $ManifestPath
    if ($LASTEXITCODE -ne 0) {
        throw "MODEL-003 上游 Dataset Manifest v1 Rust 校验失败：$ManifestPath"
    }
    $manifest = Get-Content -Raw -LiteralPath $ManifestPath | ConvertFrom-Json
    $relativePath = Get-RepositoryRelativePath $RepositoryRoot $DatasetPath
    $datasetHash = Get-Sha256 $DatasetPath
    if ([string]$manifest.dataset.name -ne $ExpectedDatasetName -or
        [string]$manifest.output.relative_path -ne $relativePath -or
        [string]$manifest.output.sha256 -ne $datasetHash) {
        throw "MODEL-003 上游 dataset 名称、路径或 SHA-256 与 manifest 不一致。"
    }
    return [ordered]@{
        dataset_name = [string]$manifest.dataset.name
        dataset_version = [string]$manifest.dataset.version
        dataset_relative_path = $relativePath
        dataset_sha256 = $datasetHash
        manifest_relative_path = Get-RepositoryRelativePath $RepositoryRoot $ManifestPath
        manifest_sha256 = Get-Sha256 $ManifestPath
    }
}

function Get-EvidenceReference(
    [string]$RepositoryRoot,
    [string]$EvidencePath
) {
    if (-not (Test-Path -LiteralPath $EvidencePath -PathType Leaf)) {
        throw "缺少 MODEL-003 审核证据：$EvidencePath"
    }
    return [ordered]@{
        relative_path = Get-RepositoryRelativePath $RepositoryRoot $EvidencePath
        sha256 = Get-Sha256 $EvidencePath
    }
}

$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if ([string]::IsNullOrWhiteSpace($ArtifactRoot)) {
    $ArtifactRoot = Join-Path $repositoryRoot "artifacts"
}
else {
    $ArtifactRoot = [System.IO.Path]::GetFullPath($ArtifactRoot)
}

# MODEL-003 必须使用相互兼容的固定 linked dataset 与原始时间切分，不能误选最新 marketless 语料。
if ([string]::IsNullOrWhiteSpace($SeriesResultPath)) {
    $SeriesResultPath = Join-Path $repositoryRoot "data/processed/lol-series-results/2026-08-13.8db1666.hist007-linked-v2/series-results.csv"
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

if ([string]::IsNullOrWhiteSpace($MarketLinkPath)) {
    $MarketLinkPath = Join-Path $repositoryRoot "data/processed/lol-market-resolution-links/2026-08-13.8db1666.hist007-linked-v2/market-resolution-links.csv"
}
else {
    $MarketLinkPath = [System.IO.Path]::GetFullPath($MarketLinkPath)
}
if ([string]::IsNullOrWhiteSpace($MarketLinkManifestPath)) {
    $MarketLinkManifestPath = "$MarketLinkPath.manifest.json"
}
else {
    $MarketLinkManifestPath = [System.IO.Path]::GetFullPath($MarketLinkManifestPath)
}

if ([string]::IsNullOrWhiteSpace($TemporalSplitPath)) {
    $TemporalSplitPath = Join-Path $repositoryRoot "data/processed/lol-temporal-splits/2026-08-12.e678afb.hist005/temporal-split-manifest.json"
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

if ([string]::IsNullOrWhiteSpace($MappingReviewPath)) {
    $MappingReviewPath = Join-Path $repositoryRoot "docs/DATA_008_MAPPING_REVIEW.csv"
}
else {
    $MappingReviewPath = [System.IO.Path]::GetFullPath($MappingReviewPath)
}
if ([string]::IsNullOrWhiteSpace($MarketGradePath)) {
    $MarketGradePath = Join-Path $repositoryRoot "docs/DATA_009_HISTORICAL_MARKET_GRADES.csv"
}
else {
    $MarketGradePath = [System.IO.Path]::GetFullPath($MarketGradePath)
}

$seriesInput = Assert-UpstreamDataset $repositoryRoot $SeriesResultPath $SeriesResultManifestPath "lol-series-results"
$linkInput = Assert-UpstreamDataset $repositoryRoot $MarketLinkPath $MarketLinkManifestPath "lol-market-resolution-links"
$splitInput = Assert-UpstreamDataset $repositoryRoot $TemporalSplitPath $TemporalSplitManifestPath "lol-temporal-splits"
$reviewInput = Get-EvidenceReference $repositoryRoot $MappingReviewPath
$gradeInput = Get-EvidenceReference $repositoryRoot $MarketGradePath

$gitCommit = (& git -C $repositoryRoot rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $gitCommit -notmatch '^[0-9a-f]{40,64}$') {
    throw "无法读取 MODEL-003 生成时 Git commit。"
}
if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = "{0}.{1}.model003" -f (Get-Date).ToUniversalTime().ToString("yyyy-MM-dd"), $gitCommit.Substring(0, 7)
}
if ($Version -notmatch '^[A-Za-z0-9._-]+$') {
    throw "Version 只能包含 ASCII 字母、数字、点、下划线和连字符。"
}

$artifactDirectory = Join-Path $ArtifactRoot "models/market-baseline/$Version"
if (Test-Path -LiteralPath $artifactDirectory) {
    throw "artifact version 已存在，禁止覆盖：$artifactDirectory"
}
$pythonEntrypoint = Join-Path $repositoryRoot "research/model003_market_baseline.py"
$temporaryArtifactOne = Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-model003-{0}.json" -f [guid]::NewGuid().ToString("N"))
$temporaryArtifactTwo = Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-model003-{0}.json" -f [guid]::NewGuid().ToString("N"))
try {
    foreach ($temporaryArtifact in @($temporaryArtifactOne, $temporaryArtifactTwo)) {
        & uv run --project $repositoryRoot --frozen python $pythonEntrypoint `
            --repository-root $repositoryRoot `
            --series-results $SeriesResultPath `
            --series-manifest $SeriesResultManifestPath `
            --market-links $MarketLinkPath `
            --market-links-manifest $MarketLinkManifestPath `
            --temporal-split $TemporalSplitPath `
            --temporal-split-manifest $TemporalSplitManifestPath `
            --mapping-review $MappingReviewPath `
            --market-grades $MarketGradePath `
            --output $temporaryArtifact
        if ($LASTEXITCODE -ne 0) {
            throw "MODEL-003 Python artifact 构建失败。"
        }
    }
    $firstHash = Get-Sha256 $temporaryArtifactOne
    $secondHash = Get-Sha256 $temporaryArtifactTwo
    if ($firstHash -ne $secondHash) {
        throw "MODEL-003 相同输入重放得到不同 artifact：first=$firstHash, second=$secondHash"
    }
    New-Item -ItemType Directory -Path $artifactDirectory | Out-Null
    $artifactPath = Join-Path $artifactDirectory "market-baseline-artifact.json"
    Move-Item -LiteralPath $temporaryArtifactOne -Destination $artifactPath
}
finally {
    foreach ($temporaryPath in @($temporaryArtifactOne, $temporaryArtifactTwo)) {
        if (Test-Path -LiteralPath $temporaryPath) {
            Remove-Item -LiteralPath $temporaryPath
        }
    }
}

$artifact = Get-Content -Raw -LiteralPath $artifactPath | ConvertFrom-Json
if ([int]$artifact.artifact_schema_version -ne 1 -or
    [string]$artifact.model.family -ne "market_baseline" -or
    [string]$artifact.probability_contract.execution_price_status -ne "unavailable" -or
    [string]$artifact.final_test_evaluation.status -ne "sealed_not_evaluated") {
    throw "MODEL-003 artifact 合同校验失败。"
}
if ($artifact.final_test_evaluation.PSObject.Properties.Name -contains "series_ids") {
    throw "MODEL-003 artifact 不得包含 final test IDs。"
}

$statusLines = @(& git -C $repositoryRoot status --porcelain=v1 --untracked-files=all)
$dirty = $statusLines.Count -gt 0
$diffHash = $null
if ($dirty) {
    $statePath = Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-model003-worktree-{0}.txt" -f [guid]::NewGuid().ToString("N"))
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

$artifactHash = Get-Sha256 $artifactPath
$artifactManifest = [ordered]@{
    artifact_manifest_version = 1
    artifact = [ordered]@{
        kind = "probability-baseline"
        name = "market-baseline"
        version = $Version
    }
    generated_at_utc = (Get-Date).ToUniversalTime().ToString("o")
    code = [ordered]@{
        git_commit = $gitCommit
        dirty = $dirty
        diff_sha256 = $diffHash
    }
    generator = [ordered]@{
        entrypoint = "research/build_market_baseline.ps1"
        arguments = @("-Version", $Version)
    }
    inputs = @($seriesInput, $linkInput, $splitInput, $reviewInput, $gradeInput)
    output = [ordered]@{
        relative_path = Get-RepositoryRelativePath $repositoryRoot $artifactPath
        sha256 = $artifactHash
    }
}
$artifactManifestPath = "$artifactPath.manifest.json"
$artifactManifest | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 -LiteralPath $artifactManifestPath

[pscustomobject]@{
    Version = $Version
    Strategy = [string]$artifact.model.strategy
    DevelopmentLinkedSeries = [int]$artifact.source_scope.development_linked_series_count
    TrainBrier = [double]$artifact.development_evaluation.train.brier_score
    ValidationBrier = [double]$artifact.development_evaluation.validation.brier_score
    CalibrationBrier = [double]$artifact.development_evaluation.calibration.brier_score
    FinalTestSeries = [int]$artifact.final_test_evaluation.series_count
    FinalTestStatus = [string]$artifact.final_test_evaluation.status
    ExecutionPriceStatus = [string]$artifact.probability_contract.execution_price_status
    DeterministicReplay = ($firstHash -eq $secondHash)
    ArtifactSha256 = $artifactHash
    Artifact = $artifactPath
    Manifest = $artifactManifestPath
}
