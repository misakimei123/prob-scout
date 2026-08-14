[CmdletBinding()]
param(
    [string]$Version = "",
    [string]$ArtifactRoot = "",
    [string]$SeriesResultPath = "",
    [string]$FeatureSnapshotPath = "",
    [string]$CandidateAuditPath = "",
    [string]$TemporalSplitPath = "",
    [string]$ConfigPath = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-Sha256([string]$Path) {
    return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

function Get-RepositoryRelativePath([string]$Root, [string]$Path) {
    return ([System.IO.Path]::GetRelativePath($Root, $Path)).Replace('\', '/')
}

$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if ([string]::IsNullOrWhiteSpace($ArtifactRoot)) {
    $ArtifactRoot = Join-Path $repositoryRoot "artifacts"
}
if ([string]::IsNullOrWhiteSpace($SeriesResultPath)) {
    $SeriesResultPath = Join-Path $repositoryRoot "data/processed/lol-series-results/2026-08-13.f42324d.m3r003-series-v1/series-results.csv"
}
if ([string]::IsNullOrWhiteSpace($FeatureSnapshotPath)) {
    $FeatureSnapshotPath = Join-Path $repositoryRoot "data/processed/lol-prematch-features/2026-08-13.f42324d.m3r003-features-v1/prematch-feature-snapshots.json"
}
if ([string]::IsNullOrWhiteSpace($CandidateAuditPath)) {
    $CandidateAuditPath = Join-Path $repositoryRoot "data/processed/lol-historical-series-candidates/2026-08-13.8db1666.m3r002-h2-2025-v2/historical-candidate-audit.json"
}
if ([string]::IsNullOrWhiteSpace($TemporalSplitPath)) {
    $TemporalSplitPath = Join-Path $repositoryRoot "data/processed/lol-temporal-splits/2026-08-14.3d155d3.m3r004-split-v1/temporal-split-manifest.json"
}
if ([string]::IsNullOrWhiteSpace($ConfigPath)) {
    $ConfigPath = Join-Path $repositoryRoot "research/model008_recovery_config.json"
}
$ArtifactRoot = [System.IO.Path]::GetFullPath($ArtifactRoot)
$SeriesResultPath = [System.IO.Path]::GetFullPath($SeriesResultPath)
$FeatureSnapshotPath = [System.IO.Path]::GetFullPath($FeatureSnapshotPath)
$CandidateAuditPath = [System.IO.Path]::GetFullPath($CandidateAuditPath)
$TemporalSplitPath = [System.IO.Path]::GetFullPath($TemporalSplitPath)
$ConfigPath = [System.IO.Path]::GetFullPath($ConfigPath)

$datasetPaths = @($SeriesResultPath, $FeatureSnapshotPath, $CandidateAuditPath, $TemporalSplitPath)
foreach ($path in $datasetPaths + @($ConfigPath)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "缺少 M3R-005 输入：$path"
    }
}
foreach ($path in $datasetPaths) {
    $manifestPath = "$path.manifest.json"
    if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
        throw "缺少 M3R-005 Dataset Manifest：$manifestPath"
    }
    & cargo run --quiet --locked --bin validate_dataset_manifest -- $manifestPath
    if ($LASTEXITCODE -ne 0) {
        throw "M3R-005 上游 Dataset Manifest Rust 校验失败：$manifestPath"
    }
}

$gitCommit = (& git -C $repositoryRoot rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $gitCommit -notmatch '^[0-9a-f]{40,64}$') {
    throw "无法读取生成时 Git commit。"
}
if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = "{0}.{1}.m3r005-p0" -f (Get-Date).ToUniversalTime().ToString("yyyy-MM-dd"), $gitCommit.Substring(0, 7)
}
if ($Version -notmatch '^[A-Za-z0-9._-]+$') {
    throw "Version 只能包含 ASCII 字母、数字、点、下划线和连字符。"
}
$artifactDirectory = Join-Path $ArtifactRoot "models/recovery-model/$Version"
if (Test-Path -LiteralPath $artifactDirectory) {
    throw "artifact version 已存在，禁止覆盖：$artifactDirectory"
}

$pythonEntrypoint = Join-Path $repositoryRoot "research/model008_recovery_model.py"
$temporaryArtifacts = @(
    (Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-m3r005-{0}.json" -f [guid]::NewGuid().ToString("N"))),
    (Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-m3r005-{0}.json" -f [guid]::NewGuid().ToString("N")))
)
try {
    foreach ($outputPath in $temporaryArtifacts) {
        & uv run --project $repositoryRoot --frozen python $pythonEntrypoint `
            --repository-root $repositoryRoot `
            --series-results $SeriesResultPath `
            --series-manifest "$SeriesResultPath.manifest.json" `
            --feature-snapshots $FeatureSnapshotPath `
            --feature-manifest "$FeatureSnapshotPath.manifest.json" `
            --candidate-audit $CandidateAuditPath `
            --candidate-manifest "$CandidateAuditPath.manifest.json" `
            --temporal-split $TemporalSplitPath `
            --temporal-split-manifest "$TemporalSplitPath.manifest.json" `
            --config $ConfigPath `
            --output $outputPath
        if ($LASTEXITCODE -ne 0) {
            throw "M3R-005 Python artifact 构建失败。"
        }
    }
    $hashes = @($temporaryArtifacts | ForEach-Object { Get-Sha256 $_ })
    if ($hashes[0] -ne $hashes[1]) {
        throw "M3R-005 相同输入重放得到不同 artifact。"
    }
    New-Item -ItemType Directory -Path $artifactDirectory | Out-Null
    $artifactPath = Join-Path $artifactDirectory "recovery-model-artifact.json"
    Move-Item -LiteralPath $temporaryArtifacts[0] -Destination $artifactPath
}
finally {
    foreach ($path in $temporaryArtifacts) {
        if (Test-Path -LiteralPath $path) {
            Remove-Item -LiteralPath $path
        }
    }
}

$artifact = Get-Content -Raw -LiteralPath $artifactPath | ConvertFrom-Json
if ([string]$artifact.artifact_kind -ne "recovery_probability_model_development" -or
    [string]$artifact.model.status -ne "development_walk_forward_not_frozen_for_final" -or
    [int]$artifact.feature_lab.source_time_violation_count -ne 0 -or
    [string]$artifact.final_test_evaluation.status -ne "sealed_not_evaluated" -or
    [bool]$artifact.final_test_evaluation.series_ids_exposed) {
    throw "M3R-005 artifact 合同校验失败。"
}

$statusLines = @(& git -C $repositoryRoot status --porcelain=v1 --untracked-files=all)
$dirty = $statusLines.Count -gt 0
$diffHash = $null
if ($dirty) {
    $statePath = Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-m3r005-worktree-{0}.txt" -f [guid]::NewGuid().ToString("N"))
    try {
        @(& git -C $repositoryRoot diff --binary HEAD) | Set-Content -Encoding utf8 -LiteralPath $statePath
        foreach ($path in (@(& git -C $repositoryRoot ls-files --others --exclude-standard) | Sort-Object)) {
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
    artifact = [ordered]@{ kind = "probability-model"; name = "recovery-model-p0"; version = $Version }
    generated_at_utc = (Get-Date).ToUniversalTime().ToString("o")
    code = [ordered]@{ git_commit = $gitCommit; dirty = $dirty; diff_sha256 = $diffHash }
    generator = [ordered]@{ entrypoint = "research/build_recovery_model.ps1"; arguments = @("-Version", $Version) }
    inputs = $artifact.inputs
    output = [ordered]@{
        relative_path = Get-RepositoryRelativePath $repositoryRoot $artifactPath
        sha256 = $artifactHash
    }
}
$artifactManifestPath = "$artifactPath.manifest.json"
$artifactManifest | ConvertTo-Json -Depth 10 | Set-Content -Encoding utf8 -LiteralPath $artifactManifestPath

$overall = $artifact.walk_forward.overall_natural_composition
[pscustomobject]@{
    Version = $Version
    DevelopmentRows = [int]$artifact.feature_lab.model_matrix.Count
    AuditRows = [int]$artifact.feature_lab.audit_rows.Count
    EvaluationRows = [int]$overall.series_count
    EloBrier = [double]$overall.elo.brier_score
    ModelBrier = [double]$overall.offset_residual.brier_score
    BrierDelta = [double]$overall.delta_model_minus_elo.brier_score
    EloLogLoss = [double]$overall.elo.log_loss
    ModelLogLoss = [double]$overall.offset_residual.log_loss
    LogLossDelta = [double]$overall.delta_model_minus_elo.log_loss
    FallbackRows = [int]$artifact.walk_forward.fallback_count
    FinalTestStatus = [string]$artifact.final_test_evaluation.status
    DeterministicReplay = ($hashes[0] -eq $hashes[1])
    ArtifactSha256 = $artifactHash
    Artifact = $artifactPath
    Manifest = $artifactManifestPath
}
