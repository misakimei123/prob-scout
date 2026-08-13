[CmdletBinding()]
param(
    [string]$SeriesResultPath = "",
    [string]$SeriesResultManifestPath = "",
    [string]$FeatureSnapshotPath = "",
    [string]$FeatureSnapshotManifestPath = "",
    [string]$TemporalSplitPath = "",
    [string]$TemporalSplitManifestPath = "",
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
            throw "缺少 MODEL-004 上游输入：$path"
        }
    }
    & cargo run --quiet --locked --bin validate_dataset_manifest -- $ManifestPath
    if ($LASTEXITCODE -ne 0) {
        throw "MODEL-004 上游 Dataset Manifest v1 Rust 校验失败：$ManifestPath"
    }
    $manifest = Get-Content -Raw -LiteralPath $ManifestPath | ConvertFrom-Json
    $relativePath = Get-RepositoryRelativePath $RepositoryRoot $DatasetPath
    $datasetHash = Get-Sha256 $DatasetPath
    if ([string]$manifest.dataset.name -ne $ExpectedDatasetName -or
        [string]$manifest.output.relative_path -ne $relativePath -or
        [string]$manifest.output.sha256 -ne $datasetHash) {
        throw "MODEL-004 上游 dataset 名称、路径或 SHA-256 与 manifest 不一致。"
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

$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if ([string]::IsNullOrWhiteSpace($ArtifactRoot)) {
    $ArtifactRoot = Join-Path $repositoryRoot "artifacts"
}
else {
    $ArtifactRoot = [System.IO.Path]::GetFullPath($ArtifactRoot)
}

# MODEL-004 固定消费同一 HIST-010 重建批次，避免“latest”误配不同语料版本。
if ([string]::IsNullOrWhiteSpace($SeriesResultPath)) {
    $SeriesResultPath = Join-Path $repositoryRoot "data/processed/lol-series-results/2026-08-13.8db1666.hist010-series/series-results.csv"
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
    $FeatureSnapshotPath = Join-Path $repositoryRoot "data/processed/lol-prematch-features/2026-08-13.8db1666.hist010-features/prematch-feature-snapshots.json"
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
    $TemporalSplitPath = Join-Path $repositoryRoot "data/processed/lol-temporal-splits/2026-08-13.8db1666.hist010-split/temporal-split-manifest.json"
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

$seriesInput = Assert-UpstreamDataset $repositoryRoot $SeriesResultPath $SeriesResultManifestPath "lol-series-results"
$featureInput = Assert-UpstreamDataset $repositoryRoot $FeatureSnapshotPath $FeatureSnapshotManifestPath "lol-prematch-features"
$splitInput = Assert-UpstreamDataset $repositoryRoot $TemporalSplitPath $TemporalSplitManifestPath "lol-temporal-splits"

$gitCommit = (& git -C $repositoryRoot rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $gitCommit -notmatch '^[0-9a-f]{40,64}$') {
    throw "无法读取 MODEL-004 生成时 Git commit。"
}
if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = "{0}.{1}.model004" -f (Get-Date).ToUniversalTime().ToString("yyyy-MM-dd"), $gitCommit.Substring(0, 7)
}
if ($Version -notmatch '^[A-Za-z0-9._-]+$') {
    throw "Version 只能包含 ASCII 字母、数字、点、下划线和连字符。"
}

$artifactDirectory = Join-Path $ArtifactRoot "models/statistical-model/$Version"
if (Test-Path -LiteralPath $artifactDirectory) {
    throw "artifact version 已存在，禁止覆盖：$artifactDirectory"
}
$pythonEntrypoint = Join-Path $repositoryRoot "research/model004_statistical_model.py"
$temporaryArtifactOne = Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-model004-{0}.json" -f [guid]::NewGuid().ToString("N"))
$temporaryArtifactTwo = Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-model004-{0}.json" -f [guid]::NewGuid().ToString("N"))
try {
    foreach ($temporaryArtifact in @($temporaryArtifactOne, $temporaryArtifactTwo)) {
        & uv run --project $repositoryRoot --frozen python $pythonEntrypoint `
            --repository-root $repositoryRoot `
            --series-results $SeriesResultPath `
            --series-manifest $SeriesResultManifestPath `
            --feature-snapshots $FeatureSnapshotPath `
            --feature-manifest $FeatureSnapshotManifestPath `
            --temporal-split $TemporalSplitPath `
            --temporal-split-manifest $TemporalSplitManifestPath `
            --output $temporaryArtifact
        if ($LASTEXITCODE -ne 0) {
            throw "MODEL-004 Python artifact 构建失败。"
        }
    }
    $firstHash = Get-Sha256 $temporaryArtifactOne
    $secondHash = Get-Sha256 $temporaryArtifactTwo
    if ($firstHash -ne $secondHash) {
        throw "MODEL-004 相同输入重放得到不同 artifact：first=$firstHash, second=$secondHash"
    }
    New-Item -ItemType Directory -Path $artifactDirectory | Out-Null
    $artifactPath = Join-Path $artifactDirectory "statistical-model-artifact.json"
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
    [string]$artifact.model.family -ne "logistic_regression" -or
    [string]$artifact.model.training_split -ne "train" -or
    [string]$artifact.model.probability_status -ne "raw_uncalibrated" -or
    [string]$artifact.final_test_evaluation.status -ne "sealed_not_evaluated") {
    throw "MODEL-004 artifact 合同校验失败。"
}
if ($artifact.final_test_evaluation.PSObject.Properties.Name -contains "series_ids") {
    throw "MODEL-004 artifact 不得包含 final test IDs。"
}

$statusLines = @(& git -C $repositoryRoot status --porcelain=v1 --untracked-files=all)
$dirty = $statusLines.Count -gt 0
$diffHash = $null
if ($dirty) {
    $statePath = Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-model004-worktree-{0}.txt" -f [guid]::NewGuid().ToString("N"))
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
        kind = "probability-model"
        name = "statistical-model"
        version = $Version
    }
    generated_at_utc = (Get-Date).ToUniversalTime().ToString("o")
    code = [ordered]@{
        git_commit = $gitCommit
        dirty = $dirty
        diff_sha256 = $diffHash
    }
    generator = [ordered]@{
        entrypoint = "research/build_statistical_model.ps1"
        arguments = @("-Version", $Version)
    }
    inputs = @($seriesInput, $featureInput, $splitInput)
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
    RandomSeed = [int]$artifact.model.config.random_seed
    TrainSeries = [int]$artifact.development_evaluation.train.series_count
    ValidationSeries = [int]$artifact.development_evaluation.validation.series_count
    CalibrationSeries = [int]$artifact.development_evaluation.calibration.series_count
    TrainBrier = [double]$artifact.development_evaluation.train.brier_score
    ValidationBrier = [double]$artifact.development_evaluation.validation.brier_score
    CalibrationBrier = [double]$artifact.development_evaluation.calibration.brier_score
    FinalTestSeries = [int]$artifact.final_test_evaluation.series_count
    FinalTestStatus = [string]$artifact.final_test_evaluation.status
    DeterministicReplay = ($firstHash -eq $secondHash)
    ArtifactSha256 = $artifactHash
    Artifact = $artifactPath
    Manifest = $artifactManifestPath
}
