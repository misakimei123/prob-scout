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

function Assert-Artifact([string]$ArtifactPath, [string]$ManifestPath) {
    foreach ($path in @($ArtifactPath, $ManifestPath)) {
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "缺少 MODEL-007 冻结 artifact：$path"
        }
    }
    $manifest = Get-Content -Raw -LiteralPath $ManifestPath | ConvertFrom-Json
    $hash = Get-Sha256 $ArtifactPath
    if ([string]$manifest.output.sha256 -ne $hash) {
        throw "MODEL-007 artifact SHA-256 与 manifest 不一致：$ArtifactPath"
    }
    return $hash
}

$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if ([string]::IsNullOrWhiteSpace($ArtifactRoot)) {
    $ArtifactRoot = Join-Path $repositoryRoot "artifacts"
}
else {
    $ArtifactRoot = [System.IO.Path]::GetFullPath($ArtifactRoot)
}

$seriesPath = Join-Path $repositoryRoot "data/processed/lol-series-results/2026-08-13.8db1666.hist010-series/series-results.csv"
$featurePath = Join-Path $repositoryRoot "data/processed/lol-prematch-features/2026-08-13.8db1666.hist010-features/prematch-feature-snapshots.json"
$splitPath = Join-Path $repositoryRoot "data/processed/lol-temporal-splits/2026-08-13.8db1666.hist010-split/temporal-split-manifest.json"
$constantPath = Join-Path $ArtifactRoot "models/constant-baseline/2026-08-13.4d92b27.model001-v2/constant-baseline-artifact.json"
$eloPath = Join-Path $ArtifactRoot "models/elo-baseline/2026-08-13.68be155.model002-v2/elo-baseline-artifact.json"
$modelPath = Join-Path $ArtifactRoot "models/statistical-model/2026-08-13.7605cdd.model004-v2/statistical-model-artifact.json"
$calibrationPath = Join-Path $ArtifactRoot "models/probability-calibration/2026-08-13.e9ed531.model005-v2/probability-calibration-artifact.json"
$walkForwardPath = Join-Path $ArtifactRoot "models/walk-forward-evaluation/2026-08-13.e87d978.model006-v2/walk-forward-evaluation-artifact.json"
$configPath = Join-Path $repositoryRoot "research/model007_gate1_config.json"
$evaluationCodePath = Join-Path $repositoryRoot "research/model007_gate1.py"
$releaseCodePath = Join-Path $repositoryRoot "src/bin/release_final_test_manifest.rs"

foreach ($path in @($seriesPath, $featurePath, $splitPath, $configPath, $evaluationCodePath, $releaseCodePath)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "缺少 MODEL-007 输入：$path"
    }
}

$constantHash = Assert-Artifact $constantPath "$constantPath.manifest.json"
$eloHash = Assert-Artifact $eloPath "$eloPath.manifest.json"
$modelHash = Assert-Artifact $modelPath "$modelPath.manifest.json"
$calibrationHash = Assert-Artifact $calibrationPath "$calibrationPath.manifest.json"
$walkForwardHash = Assert-Artifact $walkForwardPath "$walkForwardPath.manifest.json"
$modelArtifact = Get-Content -Raw -LiteralPath $modelPath | ConvertFrom-Json
$modelConfigHash = [string]$modelArtifact.model.config_sha256
if ($modelConfigHash -notmatch '^[0-9a-f]{64}$') {
    throw "MODEL-004 model config SHA-256 无效。"
}

# code hash 固定所有可能影响 release、概率重放、指标和 Gate 裁决的入口。
$codeHashes = [ordered]@{
    gate_evaluation = Get-Sha256 $evaluationCodePath
    final_release = Get-Sha256 $releaseCodePath
    raw_feature_contract = Get-Sha256 (Join-Path $repositoryRoot "research/model004_statistical_model.py")
    elo_contract = Get-Sha256 (Join-Path $repositoryRoot "research/model002_elo_baseline.py")
}
$evaluationCodeHash = [System.BitConverter]::ToString(
    [System.Security.Cryptography.SHA256]::HashData(
        [System.Text.Encoding]::UTF8.GetBytes(($codeHashes | ConvertTo-Json -Compress))
    )
).Replace("-", "").ToLowerInvariant()

$gitCommit = (& git -C $repositoryRoot rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $gitCommit -notmatch '^[0-9a-f]{40,64}$') {
    throw "无法读取 MODEL-007 Git commit。"
}
if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = "{0}.{1}.model007" -f (Get-Date).ToUniversalTime().ToString("yyyy-MM-dd"), $gitCommit.Substring(0, 7)
}
if ($Version -notmatch '^[A-Za-z0-9._-]+$') {
    throw "Version 只能包含 ASCII 字母、数字、点、下划线和连字符。"
}
$artifactDirectory = Join-Path $ArtifactRoot "models/gate1-decision/$Version"
if (Test-Path -LiteralPath $artifactDirectory) {
    throw "MODEL-007 version 已存在；Final Test 禁止重复执行：$artifactDirectory"
}

$frozenAt = (Get-Date).ToUniversalTime().ToString("o")
$releaseAuthorization = [ordered]@{
    frozen_at_utc = $frozenAt
    model_artifact_sha256 = $modelHash
    model_config_sha256 = $modelConfigHash
    evaluation_code_sha256 = $evaluationCodeHash
}
$freeze = [ordered]@{
    freeze_manifest_version = 1
    frozen_at_utc = $frozenAt
    candidate_selected_before_release = "raw_statistical"
    calibration_decision_before_release = "rollback_sigmoid"
    gate_config_sha256 = Get-Sha256 $configPath
    evaluation_code_files = $codeHashes
    evaluation_code_sha256 = $evaluationCodeHash
    model_config_sha256 = $modelConfigHash
    frozen_artifacts = [ordered]@{
        constant_artifact_sha256 = $constantHash
        elo_artifact_sha256 = $eloHash
        model_artifact_sha256 = $modelHash
        calibration_artifact_sha256 = $calibrationHash
        walk_forward_artifact_sha256 = $walkForwardHash
    }
    release_authorization = $releaseAuthorization
}

$temporaryDirectory = Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-model007-{0}" -f [guid]::NewGuid().ToString("N"))
$completed = $false
try {
    New-Item -ItemType Directory -Path $temporaryDirectory | Out-Null
    $freezePath = Join-Path $temporaryDirectory "gate1-freeze-manifest.json"
    $freeze | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 -LiteralPath $freezePath

    # Rust release 从完整 Series Result 重新计算 final membership；此步骤成功后才产生显式 IDs。
    $split = Get-Content -Raw -LiteralPath $splitPath | ConvertFrom-Json
    $candidates = Import-Csv -LiteralPath $seriesPath | ForEach-Object {
        [ordered]@{
            series_id = [string]$_.series_id
            scheduled_start_utc = [string]$_.scheduled_start_utc
        }
    }
    $releaseInput = [ordered]@{
        sealed_manifest = $split
        candidates = @($candidates)
        authorization = $releaseAuthorization
    }
    $releaseInputPath = Join-Path $temporaryDirectory "release-input.json"
    $releasedPath = Join-Path $temporaryDirectory "released-final-test-manifest.json"
    $releaseInput | ConvertTo-Json -Depth 10 | Set-Content -Encoding utf8 -LiteralPath $releaseInputPath
    & cargo run --quiet --locked --bin release_final_test_manifest -- $releaseInputPath $releasedPath
    if ($LASTEXITCODE -ne 0) {
        throw "MODEL-007 Rust Final Test release 失败。"
    }

    $temporaryArtifact = Join-Path $temporaryDirectory "gate1-decision-artifact.json"
    & uv run --project $repositoryRoot --frozen python -m research.model007_gate1 `
        --series-results $seriesPath `
        --feature-snapshots $featurePath `
        --temporal-split $splitPath `
        --released-manifest $releasedPath `
        --constant-artifact $constantPath `
        --constant-manifest "$constantPath.manifest.json" `
        --elo-artifact $eloPath `
        --elo-manifest "$eloPath.manifest.json" `
        --model-artifact $modelPath `
        --model-manifest "$modelPath.manifest.json" `
        --calibration-artifact $calibrationPath `
        --calibration-manifest "$calibrationPath.manifest.json" `
        --walk-forward-artifact $walkForwardPath `
        --walk-forward-manifest "$walkForwardPath.manifest.json" `
        --config $configPath `
        --freeze $freezePath `
        --output $temporaryArtifact
    if ($LASTEXITCODE -ne 0) {
        throw "MODEL-007 Final Test 主评估失败。"
    }

    $artifact = Get-Content -Raw -LiteralPath $temporaryArtifact | ConvertFrom-Json
    if ([string]$artifact.release.status -ne "released_and_evaluated_once" -or
        [int]$artifact.release.series_count -ne 356 -or
        [bool]$artifact.candidate_selection.calibrated_probability_evaluated_on_final_test) {
        throw "MODEL-007 主评估 artifact 合同校验失败。"
    }

    New-Item -ItemType Directory -Path $artifactDirectory | Out-Null
    Move-Item -LiteralPath $freezePath -Destination (Join-Path $artifactDirectory "gate1-freeze-manifest.json")
    Move-Item -LiteralPath $releasedPath -Destination (Join-Path $artifactDirectory "released-final-test-manifest.json")
    $artifactPath = Join-Path $artifactDirectory "gate1-decision-artifact.json"
    Move-Item -LiteralPath $temporaryArtifact -Destination $artifactPath

    $statusLines = @(& git -C $repositoryRoot status --porcelain=v1 --untracked-files=all)
    $dirty = $statusLines.Count -gt 0
    $artifactHash = Get-Sha256 $artifactPath
    $artifactManifest = [ordered]@{
        artifact_manifest_version = 1
        artifact = [ordered]@{ kind = "model-gate-decision"; name = "gate1-decision"; version = $Version }
        generated_at_utc = (Get-Date).ToUniversalTime().ToString("o")
        code = [ordered]@{ git_commit = $gitCommit; dirty = $dirty; evaluation_code_sha256 = $evaluationCodeHash }
        generator = [ordered]@{ entrypoint = "research/build_gate1_decision.ps1"; arguments = @("-Version", $Version) }
        inputs = [ordered]@{
            series_result_sha256 = Get-Sha256 $seriesPath
            feature_snapshot_sha256 = Get-Sha256 $featurePath
            temporal_split_sha256 = Get-Sha256 $splitPath
            freeze_manifest_sha256 = Get-Sha256 (Join-Path $artifactDirectory "gate1-freeze-manifest.json")
            released_manifest_sha256 = Get-Sha256 (Join-Path $artifactDirectory "released-final-test-manifest.json")
        }
        output = [ordered]@{ relative_path = Get-RepositoryRelativePath $repositoryRoot $artifactPath; sha256 = $artifactHash }
    }
    $manifestPath = "$artifactPath.manifest.json"
    $artifactManifest | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 -LiteralPath $manifestPath
    $completed = $true
}
finally {
    if (Test-Path -LiteralPath $temporaryDirectory) {
        Remove-Item -LiteralPath $temporaryDirectory -Recurse -Force
    }
    if (-not $completed -and (Test-Path -LiteralPath $artifactDirectory)) {
        Remove-Item -LiteralPath $artifactDirectory -Recurse -Force
    }
}

$artifact = Get-Content -Raw -LiteralPath $artifactPath | ConvertFrom-Json
$models = $artifact.final_test.models
[pscustomobject]@{
    Version = $Version
    FinalSeries = [int]$artifact.release.series_count
    RawBrier = [double]$models.raw_statistical.brier_score
    EloBrier = [double]$models.elo_baseline.brier_score
    RawLogLoss = [double]$models.raw_statistical.log_loss
    EloLogLoss = [double]$models.elo_baseline.log_loss
    Gate1 = [string]$artifact.gate1_decision.status
    NextTask = [string]$artifact.gate1_decision.next_task_authorized
    ArtifactSha256 = Get-Sha256 $artifactPath
    Artifact = $artifactPath
    Manifest = $manifestPath
}
