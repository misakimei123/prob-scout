[CmdletBinding()]
param(
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

$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if ([string]::IsNullOrWhiteSpace($ArtifactRoot)) {
    $ArtifactRoot = Join-Path $repositoryRoot "artifacts"
}
else {
    $ArtifactRoot = [System.IO.Path]::GetFullPath($ArtifactRoot)
}

$modelPath = Join-Path $ArtifactRoot "models/statistical-model/2026-08-13.7605cdd.model004-v2/statistical-model-artifact.json"
$walkForwardPath = Join-Path $ArtifactRoot "models/walk-forward-evaluation/2026-08-13.e87d978.model006-v2/walk-forward-evaluation-artifact.json"
$gatePath = Join-Path $ArtifactRoot "models/gate1-decision/2026-08-13.e87d978.model007-v1/gate1-decision-artifact.json"
foreach ($path in @($modelPath, "$modelPath.manifest.json", $walkForwardPath, "$walkForwardPath.manifest.json", $gatePath, "$gatePath.manifest.json")) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "缺少 M3R-001 不可变输入：$path"
    }
}

$gitCommit = (& git -C $repositoryRoot rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $gitCommit -notmatch '^[0-9a-f]{40,64}$') {
    throw "无法读取 M3R-001 Git commit。"
}
if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = "{0}.{1}.m3r001" -f (Get-Date).ToUniversalTime().ToString("yyyy-MM-dd"), $gitCommit.Substring(0, 7)
}
if ($Version -notmatch '^[A-Za-z0-9._-]+$') {
    throw "Version 只能包含 ASCII 字母、数字、点、下划线和连字符。"
}
$artifactDirectory = Join-Path $ArtifactRoot "models/gate1-failure-attribution/$Version"
if (Test-Path -LiteralPath $artifactDirectory) {
    throw "M3R-001 artifact version 已存在，禁止覆盖：$artifactDirectory"
}

$temporaryOne = Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-m3r001-{0}.json" -f [guid]::NewGuid().ToString("N"))
$temporaryTwo = Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-m3r001-{0}.json" -f [guid]::NewGuid().ToString("N"))
try {
    foreach ($output in @($temporaryOne, $temporaryTwo)) {
        & uv run --project $repositoryRoot --frozen python -m research.m3r001_gate1_failure_attribution `
            --model-artifact $modelPath `
            --model-manifest "$modelPath.manifest.json" `
            --walk-forward-artifact $walkForwardPath `
            --walk-forward-manifest "$walkForwardPath.manifest.json" `
            --gate-artifact $gatePath `
            --gate-manifest "$gatePath.manifest.json" `
            --output $output
        if ($LASTEXITCODE -ne 0) {
            throw "M3R-001 Python 归因构建失败。"
        }
    }
    $firstHash = Get-Sha256 $temporaryOne
    $secondHash = Get-Sha256 $temporaryTwo
    if ($firstHash -ne $secondHash) {
        throw "M3R-001 相同输入重放结果不一致。"
    }
    New-Item -ItemType Directory -Path $artifactDirectory | Out-Null
    $artifactPath = Join-Path $artifactDirectory "gate1-failure-attribution-artifact.json"
    Move-Item -LiteralPath $temporaryOne -Destination $artifactPath
}
finally {
    foreach ($path in @($temporaryOne, $temporaryTwo)) {
        if (Test-Path -LiteralPath $path) {
            Remove-Item -LiteralPath $path
        }
    }
}

$artifact = Get-Content -Raw -LiteralPath $artifactPath | ConvertFrom-Json
if ([string]$artifact.artifact_kind -ne "gate1_failure_attribution" -or
    [string]$artifact.analysis_status -ne "diagnostic_complete_not_a_new_gate" -or
    [string]$artifact.cohort_governance.retired_final_status -ne "retired_diagnostic_evidence_never_independent_again" -or
    [string]$artifact.next_task.task_id -ne "M3R-002" -or
    [bool]$artifact.next_task.model_development_authorized -or
    [bool]$artifact.next_task.m4_authorized) {
    throw "M3R-001 artifact 治理合同校验失败。"
}

$statusLines = @(& git -C $repositoryRoot status --porcelain=v1 --untracked-files=all)
$artifactHash = Get-Sha256 $artifactPath
$manifest = [ordered]@{
    artifact_manifest_version = 1
    artifact = [ordered]@{ kind = "model-diagnostic"; name = "gate1-failure-attribution"; version = $Version }
    generated_at_utc = (Get-Date).ToUniversalTime().ToString("o")
    code = [ordered]@{
        git_commit = $gitCommit
        dirty = $statusLines.Count -gt 0
        generator_sha256 = Get-Sha256 (Join-Path $repositoryRoot "research/m3r001_gate1_failure_attribution.py")
    }
    generator = [ordered]@{ entrypoint = "research/build_gate1_failure_attribution.ps1"; arguments = @("-Version", $Version) }
    inputs = [ordered]@{
        model004_sha256 = Get-Sha256 $modelPath
        model006_sha256 = Get-Sha256 $walkForwardPath
        model007_sha256 = Get-Sha256 $gatePath
    }
    output = [ordered]@{ relative_path = Get-RepositoryRelativePath $repositoryRoot $artifactPath; sha256 = $artifactHash }
}
$manifestPath = "$artifactPath.manifest.json"
$manifest | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 -LiteralPath $manifestPath

[pscustomobject]@{
    Version = $Version
    PublicSeries = [int]$artifact.cohort_governance.public_walk_forward_series_count
    RetiredFinalSeries = [int]$artifact.cohort_governance.retired_final_series_count
    FixedPublicBrierDelta = [double]$artifact.cohort_metrics.public_fixed_candidate.raw_minus_elo.brier_score
    FinalBrierDelta = [double]$artifact.cohort_metrics.retired_final.raw_minus_elo.brier_score
    CompositionBrierEffect = [double]$artifact.composition_decomposition.composition_effect.brier_score
    WithinCellBrierResidual = [double]$artifact.composition_decomposition.within_cell_time_shift_residual.brier_score
    NextTask = [string]$artifact.next_task.task_id
    ArtifactSha256 = $artifactHash
    Artifact = $artifactPath
    Manifest = $manifestPath
}
